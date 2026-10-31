//! The CADCraft engine.
//!
//! Every user-visible action is a command with a stable id (AutoCAD's command names in lower
//! case: `line`, `circle`, `zoom`, `layer`…, plus dotted ids for UI operations). Commands run two
//! ways: programmatically with JSON parameters ([`Session::execute`], no dialogs, used by the
//! control channel, MCP, CLI and scripts) and interactively through prompts ([`Session::start`],
//! [`Session::input`], [`Session::cmdline`]) exactly like typing at AutoCAD's command line.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod assoc;
#[cfg(not(target_arch = "wasm32"))]
pub mod atomic_write;
pub mod audit;
pub mod autosave;
pub mod blockedit;
mod canonical;
pub use canonical::canonical_geometry_rows;
mod canonical_grips;
#[cfg(not(target_arch = "wasm32"))]
pub mod capped_read;
pub mod cmd;
pub mod doc_path;
mod finite;
pub mod grip_dims;
pub mod grips;
mod guard;
pub mod lisp;
pub mod mtext_edit;
pub mod pointmod;
pub mod prompt;
mod recent;
pub mod sample;
pub mod select;
pub mod settings_dir;
pub mod snap;
pub mod spatial;
pub mod sysvars;
pub mod undoctl;
pub mod units;

use std::sync::Arc;

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, Space};
use cadcraft_geom::{Bounds2, Vec2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use cadcraft_doc as doc;
pub use cadcraft_fonts as fonts;
pub use cadcraft_geom as geom;
pub use cadcraft_render as render;
pub use cmd::hatch::{HatchPicks, PatternChoice, PickMode, pattern_choices};
pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use prompt::{Accept, Drag, Input, Interactive, Picker, Prompt, Step};

mod doc_state;
mod error;
mod last_used;
mod running;
mod session;
mod settings;
mod view;

pub use doc_state::{DocState, Snapshot};
pub use error::{EngineError, Result};
pub use last_used::{LastUsed, LeaderOrder, LeaderType, MLeaderOptions};
pub use running::{PendingWindow, Running};
pub use settings::Settings;
pub use view::{ModelTile, ModelTiles, View};

/// `<version> (<commit>)`, what `--version` prints after the program name in the app and the CLI.
/// The commit comes from `CADCRAFT_BUILD_SHA` at build time, `dev` without it.
pub fn build_version() -> &'static str {
    static VERSION: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("{} ({})", env!("CARGO_PKG_VERSION"), option_env!("CADCRAFT_BUILD_SHA").unwrap_or("dev")));
    &VERSION
}

/// The editor session: open drawings, settings, the running command and the command log.
pub struct Session {
    pub docs: Vec<DocState>,
    pub active: usize,
    /// Monotonic count of changes to the open drawings and the active one. A screenshot queued
    /// across a switch is rejected even when a later switch returns to the same drawing.
    pub doc_epoch: u64,
    pub settings: Settings,
    pub running: Option<Running>,
    /// A script is running ([`Session::script`]); a SCRIPT command inside it is refused.
    in_script: bool,
    /// The command suspended while a transparent command (`'ZOOM`, `'PAN`…) prompts inside it;
    /// it resumes when that command ends. One level, as in AutoCAD.
    pub suspended: Option<Running>,
    pub log: Vec<String>,
    pub last_command: Option<String>,
    /// LASTPOINT.
    pub last_point: Vec2,
    /// The Z of the typed point being given to the running command (see [`prompt::typed_z`]); `None` for any other point.
    pub typed_z: Option<(f64, bool)>,
    /// The last segment given at a point prompt (its base point and the point): polar angles
    /// relative to the last segment (POLARMODE bit 1) are measured from it.
    pub last_segment: Option<(Vec2, Vec2)>,
    /// The last typed `@distance<angle` input, as `(distance, angle in radians)`.
    /// Set by [`Session::typed`] and consumed by commands that resolve a deferred
    /// tangent first point (e.g. [`cmd::draw::LineM`]) when the next point is typed
    /// rather than clicked. Cleared at [`Session::start`].
    pub last_polar: Option<(f64, f64)>,
    /// Point modifiers pending at the current point prompt (`FROM`, `M2P`, `.x`, `<a`…).
    pub point_mods: Vec<pointmod::Frame>,
    /// Cursor position in world coordinates (from the UI; used for direct distance entry).
    pub cursor: Vec2,
    /// The deferred tangent/perpendicular snap under the cursor, if any (from the UI; lets the
    /// rubber band show the line it resolves to).
    pub cursor_deferred: Option<snap::Deferred>,
    /// Whether Ctrl is held right now (from the UI, overwritten every frame). Commands that
    /// change behavior on Ctrl — currently only ARC's "hold Ctrl to switch direction" — read
    /// this instead of receiving modifiers through `Input`, which stays free of UI state.
    pub ctrl_down: bool,
    /// Origin of the point currently being fed by `input_snapped`; never reused by typed points.
    picked_viewport: Option<snap::ViewportPoint>,
    /// The last drawing comparison (COMPARE), drawn over the drawing it was made for.
    pub compare: Option<cmd::file::compare::Comparison>,
    /// Object snap tracking: acquired points and the path under the cursor (from the UI's
    /// [`Session::snap_cursor`] calls; direct distance entry follows the path).
    pub tracking: snap::tracking::Tracker,
    /// Viewport size in pixels (from the UI; used for zoom and pick apertures).
    pub viewport_px: (f64, f64),
    pub clipboard: Vec<Entity>,
    pub clipboard_base: Vec2,
    pub pending_window: Option<PendingWindow>,
    /// A selection mode typed at "Select objects" (W, C, F, WP, CP) collecting its points.
    pub select_mode: Option<select::SelectMode>,
    /// SIngle typed at "Select objects": the next selection ends the prompt.
    pub select_single: bool,
    pub untitled_counter: u32,
    /// The last dimension created (DIMCONTINUE / DIMBASELINE).
    pub last_dim: Option<Handle>,
    /// Options remembered between invocations of interactive commands.
    pub last_used: LastUsed,
    /// Where `clipboard` came from and the layers, styles and blocks its objects need.
    pub clipboard_source: Option<cmd::clipboard::ClipSource>,
    /// The window last picked with PLOTWINDOW (the Plot dialog's "Window <").
    pub plot_window: Option<cadcraft_geom::Bounds2>,
    /// AutoLISP: each drawing's symbols and the routine running ([`lisp`]).
    lisp: lisp::LispState,
    /// How the last LISP run ended (its value or error message).
    lisp_last: Option<std::result::Result<lisp::value::Value, String>>,
    /// What the last HATCHPICK kept (the Hatch and Gradient dialog's picks).
    pub hatch_picks: Option<cmd::hatch::HatchPicks>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

/// The interactive commands whose preview shows the object they will draw with `Session::draw`.
pub(crate) const DRAWN_AT_ELEVATION: &[&str] = &[
    "ARC",
    "ATTDEF",
    "CIRCLE",
    "DIMALIGNED",
    "DIMANGULAR",
    "DIMARC",
    "DIMDIAMETER",
    "DIMLINEAR",
    "DIMORDINATE",
    "DIMRADIUS",
    "DONUT",
    "ELLIPSE",
    "HELIX",
    "INSERT",
    "MLEADER",
    "MTEXT",
    "POLYGON",
    "RAY",
    "REVCLOUD",
    "SPLINE",
    "TABLE",
    "TEXT",
    "WIPEOUT",
    "XLINE",
];

/// Move a flat object drawn at Z 0 to Z `z`. An object of a kind that has no Z of its own to move is left as it is.
pub(crate) fn raise_to(kind: &mut EntityKind, z: f64) {
    if z == 0.0 {
        return;
    }
    match kind {
        EntityKind::Line(l) => {
            l.a.z += z;
            l.b.z += z;
        }
        EntityKind::Circle(c) => c.center.z += z,
        EntityKind::Arc(a) => a.center.z += z,
        EntityKind::Ellipse(e) => e.center.z += z,
        EntityKind::Ray(r) | EntityKind::XLine(r) => r.base.z += z,
        EntityKind::Text(t) => {
            t.insert.z += z;
            if let Some(align) = &mut t.align_pt {
                align.z += z;
            }
        }
        EntityKind::AttDef(a) => {
            a.text.insert.z += z;
            if let Some(align) = &mut a.text.align_pt {
                align.z += z;
            }
        }
        EntityKind::MText(t) => t.insert.z += z,
        EntityKind::Insert(i) => {
            i.insert.z += z;
            for attrib in &mut i.attribs {
                attrib.text.insert.z += z;
                if let Some(align) = &mut attrib.text.align_pt {
                    align.z += z;
                }
            }
        }
        EntityKind::Helix(h) => h.center.z += z,
        EntityKind::Table(t) => t.insert.z += z,
        EntityKind::LwPolyline(p) => p.elevation += z,
        EntityKind::Leader(l) => l.vertices.iter_mut().for_each(|v| v.z += z),
        EntityKind::Dimension(dm) => raise_dimension(dm, z),
        EntityKind::MLeader(m) => {
            for root in &mut m.roots {
                root.landing.z += z;
                root.branches.iter_mut().flat_map(|b| b.vertices.iter_mut()).for_each(|v| v.z += z);
            }
            if let Some(text) = &mut m.text {
                text.insert.z += z;
            }
            if let Some(block) = &mut m.block {
                block.insert.z += z;
            }
            m.context.origin.z += z;
        }
        EntityKind::Spline(sp) => {
            let space = sp.space.get_or_insert_with(|| cadcraft_geom::SplineSpace {
                control: vec![0.0; sp.control.len()],
                fit: vec![0.0; sp.fit.len()],
                start_tangent: 0.0,
                end_tangent: 0.0,
            });
            space.control.iter_mut().chain(space.fit.iter_mut()).for_each(|v| *v += z);
        }
        EntityKind::Wipeout(w) => raise_wipeout(w, z),
        _ => {}
    }
}

/// Raise the definition points that AutoCAD gives the elevation: the points a kind of dimension uses, and for an ordinate dimension
/// not group 10, which is the origin of the UCS. Measured in AutoCAD LT 2025 (see the consumer matrix).
fn raise_dimension(dm: &mut cadcraft_doc::Dimension, z: f64) {
    use cadcraft_doc::DimKind::{Aligned, Angular, Angular3P, ArcLength, Diameter, Linear, Ordinate, Radius};
    let (defpt, p13, p14, p15, p16) = match dm.kind {
        Linear { .. } | Aligned => (true, true, true, false, false),
        Radius | Diameter => (true, false, false, true, false),
        Angular | ArcLength => (true, true, true, true, true),
        Angular3P => (true, true, true, true, false),
        Ordinate { .. } => (false, true, true, false, false),
    };
    dm.text_mid.z += z;
    for (used, point) in [(defpt, &mut dm.defpt), (p13, &mut dm.p13), (p14, &mut dm.p14), (p15, &mut dm.p15), (p16, &mut dm.p16)] {
        if used {
            point.z += z;
        }
    }
    if let Some(jogged) = &mut dm.marks.jogged {
        jogged.center.z += z;
        jogged.jog.z += z;
    }
}

/// A wipeout at Z: a legacy wipeout has no Z, so it gets the image frame that the writer would make for it, with the Z in the origin.
fn raise_wipeout(w: &mut cadcraft_doc::Wipeout, z: f64) {
    use cadcraft_geom::{Bounds2, Vec2};
    let bounds = Bounds2::from_points(w.boundary.iter().copied());
    let origin = if bounds.is_empty() { Vec2::ZERO } else { bounds.min };
    let (sx, sy) = (if bounds.width() > 0.0 { bounds.width() } else { 1.0 }, if bounds.height() > 0.0 { bounds.height() } else { 1.0 });
    let mut outline = w.boundary.clone();
    if let (Some(first), Some(last)) = (outline.first().copied(), outline.last().copied())
        && first != last
    {
        outline.push(first);
    }
    let clip = outline.iter().map(|q| Vec2::new((q.x - origin.x) / sx - 0.5, 0.5 - (q.y - origin.y) / sy)).collect();
    w.image = Some(cadcraft_doc::WipeoutFrame {
        origin: cadcraft_geom::Vec3::new(origin.x, origin.y, z),
        u: cadcraft_geom::Vec3::new(sx, 0.0, 0.0),
        v: cadcraft_geom::Vec3::new(0.0, sy, 0.0),
        size: Vec2::new(1.0, 1.0),
        clip_type: 2,
        clip,
    });
    w.boundary = outline;
}

impl Session {
    pub fn new() -> Self {
        let mut s = Session::empty();
        s.new_drawing(false);
        s
    }
    /// A session with no open drawing (the Start tab).
    pub fn empty() -> Self {
        Session {
            docs: Vec::new(),
            active: 0,
            doc_epoch: 0,
            settings: Settings::default(),
            running: None,
            in_script: false,
            suspended: None,
            log: Vec::new(),
            last_command: None,
            last_point: Vec2::ZERO,
            typed_z: None,
            last_segment: None,
            last_polar: None,
            point_mods: Vec::new(),
            cursor: Vec2::ZERO,
            cursor_deferred: None,
            ctrl_down: false,
            picked_viewport: None,
            compare: None,
            tracking: snap::tracking::Tracker::default(),
            viewport_px: (1200.0, 800.0),
            clipboard: Vec::new(),
            clipboard_base: Vec2::ZERO,
            pending_window: None,
            select_mode: None,
            select_single: false,
            untitled_counter: 0,
            last_dim: None,
            last_used: LastUsed::default(),
            clipboard_source: None,
            plot_window: None,
            lisp: lisp::LispState::default(),
            lisp_last: None,
            hatch_picks: None,
        }
    }

    pub fn state(&self) -> Result<&DocState> {
        self.docs.get(self.active).ok_or(EngineError::NoDocument)
    }
    pub fn state_mut(&mut self) -> Result<&mut DocState> {
        self.docs.get_mut(self.active).ok_or(EngineError::NoDocument)
    }
    pub fn doc(&self) -> Result<&Drawing> {
        Ok(&self.state()?.doc)
    }
    /// Copy-on-write access to the drawing.
    pub fn doc_mut(&mut self) -> Result<&mut Drawing> {
        Ok(Arc::make_mut(&mut self.state_mut()?.doc))
    }
    /// Settings for the active drawing, without caching drawing variables in the profile.
    pub fn drafting_settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        if let Ok(d) = self.doc() {
            settings.snapunit = d.header.snap_unit();
            settings.gridunit = d.header.grid_unit();
        }
        settings
    }
    /// The space commands act on (model space while working inside a layout viewport).
    pub fn space(&self) -> Space {
        self.state().map(DocState::edit_space).unwrap_or_default()
    }
    /// The tab being displayed: Model or a layout, regardless of MSPACE.
    pub fn layout_space(&self) -> Space {
        self.state().map(|s| s.space.clone()).unwrap_or_default()
    }
    pub fn selection(&self) -> Vec<Handle> {
        self.state().map(|s| s.selection.clone()).unwrap_or_default()
    }
    pub fn set_selection(&mut self, sel: Vec<Handle>) {
        if let Ok(st) = self.state_mut() {
            let mut seen = std::collections::HashSet::new();
            st.selection = sel.into_iter().filter(|h| seen.insert(*h)).collect();
        }
    }

    /// The Z of a point a command receives: the typed Z when it has one (relative to `previous`, the Z of the point before it, for an
    /// `@` form), else the current elevation (ELEVATION), which is `previous` for a relative point without Z.
    pub fn point_z(&self, previous: Option<f64>) -> f64 {
        match self.typed_z {
            Some((z, false)) => z,
            Some((dz, true)) => previous.unwrap_or_else(|| self.elevation()) + dz,
            None => self.elevation(),
        }
    }

    /// The current elevation (ELEVATION) of new objects.
    pub fn elevation(&self) -> f64 {
        self.doc().ok().map(|d| d.header.f64("ELEVATION", 0.0)).filter(|z| z.is_finite()).unwrap_or(0.0)
    }

    /// Message on the command line history.
    pub fn echo(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        log::debug!("{m}");
        self.log.push(m);
        if self.log.len() > 5000 {
            self.log.drain(..1000);
        }
    }

    /// Common properties for new objects (CLAYER, CECOLOR, CELTYPE, CELWEIGHT, CELTSCALE).
    pub fn current_common(&self) -> Common {
        let Ok(d) = self.doc() else { return Common::default() };
        let h = &d.header;
        Common {
            layer: h.str("CLAYER", "0"),
            color: cadcraft_color::Color::from_aci(h.i64("CECOLOR", 256) as i16),
            linetype: h.str("CELTYPE", "ByLayer"),
            lineweight: cadcraft_doc::Lineweight::from_dxf(h.i64("CELWEIGHT", -1) as i16),
            ltscale: h.f64("CELTSCALE", 1.0),
            plot_style: h.str("CPLOTSTYLE", "ByLayer"),
            ..Common::default()
        }
    }

    /// Add an object that a draw command made on the plan view: it sits at the current elevation (ELEVATION), as AutoCAD draws it.
    /// A command with a Z of its own (POINT, LINE, PLINE, an object it derives from another) uses `add_entity`.
    pub fn draw(&mut self, mut kind: EntityKind) -> Result<Handle> {
        raise_to(&mut kind, self.elevation());
        self.add_entity(kind)
    }

    /// Add an entity to the current space with current properties.
    pub fn add_entity(&mut self, kind: EntityKind) -> Result<Handle> {
        let common = self.current_common();
        let space = self.space();
        let true_color = self.doc()?.header.get("CECOLOR_RGB").and_then(|v| v.as_i64());
        let mut common = common;
        if let Some(rgb) = true_color {
            common.color = cadcraft_color::Color::True(cadcraft_color::Rgb::from_u32(rgb as u32));
        }
        // Text, dimensions, multileaders, hatches and block references of annotative styles are
        // annotative, supporting the current annotation scale.
        let d = self.doc()?;
        if d.annotative_by_style(&kind) {
            common.anno_scales = vec![d.current_anno_scale()];
        }
        // Lines, arcs, circles, points, polylines and 2D solids take the current 3D thickness
        // (THICKNESS). RECTANG sets its own, and text never takes it.
        if matches!(
            kind,
            EntityKind::Line(_)
                | EntityKind::Arc(_)
                | EntityKind::Circle(_)
                | EntityKind::Point(_)
                | EntityKind::LwPolyline(_)
                | EntityKind::Solid(_)
        ) {
            common.thickness = Some(d.header.f64("THICKNESS", 0.0)).filter(|t| t.is_finite()).unwrap_or(0.0);
        }
        let h = self.doc_mut()?.add(&space, common, kind)?;
        Ok(h)
    }

    /// World units per pixel at the current view.
    pub fn pixel_size(&self) -> f64 {
        let v = self.state().map(DocState::view).unwrap_or_default();
        v.height / self.viewport_px.1.max(1.0)
    }

    /// Serializable summary for inspect/automation.
    pub fn summary(&self) -> Value {
        let docs: Vec<Value> = self
            .docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                serde_json::json!({
                    "index": i, "title": d.title, "path": d.path, "dirty": d.is_dirty(),
                    "entities": d.doc.entity_count(), "active": i == self.active, "revision": d.revision,
                    "blockEditor": d.block_editor(),
                })
            })
            .collect();
        serde_json::json!({
            "documents": docs,
            "prompt": self.prompt_text(),
            "running": self.running.as_ref().map(|r| r.id.clone()),
            "selection": self.selection().iter().map(|h| h.hex()).collect::<Vec<_>>(),
            "settings": self.drafting_settings(),
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../tests/support/temp_dir.rs"]
mod temp_dir;
