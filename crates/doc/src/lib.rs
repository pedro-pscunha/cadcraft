//! The CADCraft drawing database.
//!
//! A [`Drawing`] holds the header variables, symbol tables (layers, linetypes, styles), block
//! definitions, model space and paper-space layouts. Entity collections are copy-on-write
//! ([`EntityStore`]), so a whole-drawing clone is cheap and serves as an undo snapshot.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod array;
mod association;
mod constraint;
mod entity;
mod extents;
mod finite;
mod format;
mod gdt;
mod header;
mod identity;
pub use identity::HandleRepair;
pub mod library;
mod plotstyle;
mod plotter;
mod refedit;
mod store;
mod tables;

use std::collections::{BTreeMap, BTreeSet};

pub use array::*;
pub use association::{association_point, association_supported};
pub use cadcraft_color as color;
pub use cadcraft_geom as geom;
pub use constraint::*;
pub use entity::*;
pub use extents::{MAX_BLOCK_DEPTH, legacy_entity_bounds};
pub use finite::has_finite_numbers;
pub use format::Format;
pub use gdt::*;
pub use header::{HVal, Header};
pub use plotstyle::*;
pub use plotter::*;
pub use refedit::{RefEdit, WorkItem};
pub use store::EntityStore;
pub use tables::*;

/// File data CADCraft doesn't model, kept for saving (#528).
pub mod dynblock;
pub mod foreign;
pub use foreign::DuplicationSource;

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("no such layer `{0}`")]
    NoLayer(String),
    #[error("no such block `{0}`")]
    NoBlock(String),
    #[error("no such entity {0:?}")]
    NoEntity(Handle),
    #[error("{0}")]
    Invalid(String),
    #[error("Canonical geometry of object {0:?} requires a revision-aware extent query.")]
    CanonicalExtent(Handle),
    /// An edit would give the entity (its handle; 0 for a copy not yet in a drawing) infinite or undefined coordinates.
    #[error("The geometry result of {0:?} has infinite or undefined coordinates.")]
    NonFinite(Handle),
}

pub type Result<T> = std::result::Result<T, DocError>;

/// Which entity collection: model space or a named layout's paper space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Space {
    #[default]
    Model,
    Paper(String),
}

impl Space {
    /// Whether this is a layout's paper space.
    pub fn is_paper(&self) -> bool {
        matches!(self, Space::Paper(_))
    }

    /// The layout whose paper space this is; `None` for model space.
    pub fn layout(&self) -> Option<&str> {
        match self {
            Space::Paper(n) => Some(n),
            Space::Model => None,
        }
    }

    /// The tab name (CTAB): `Model`, or the layout's name.
    pub fn tab_name(&self) -> &str {
        self.layout().unwrap_or("Model")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub header: Header,
    /// File data CADCraft does not model, written back on save.
    pub foreign: std::sync::Arc<foreign::Foreign>,
    pub layers: Vec<Layer>,
    pub linetypes: Vec<Linetype>,
    pub text_styles: Vec<TextStyle>,
    /// Registered application names, including those with no remaining xdata.
    pub registered_apps: Vec<String>,
    /// Named plot style dictionary entries, including unused names.
    pub plot_style_names: Vec<String>,
    pub dim_styles: Vec<DimStyle>,
    pub mleader_styles: Vec<MLeaderStyle>,
    pub table_styles: Vec<TableStyle>,
    /// Multiline styles (MLSTYLE); STANDARD is always there.
    pub mline_styles: Vec<MLineStyle>,
    /// Raster definitions, including unused entries of ACAD_IMAGE_DICT.
    pub image_definitions: Vec<ImageDefinition>,
    pub blocks: BTreeMap<String, std::sync::Arc<Block>>,
    pub model: EntityStore,
    pub layouts: Vec<Layout>,
    /// The layout a file marks active (its block is `*Paper_Space`, the others `*Paper_SpaceN`),
    /// written active again. `None`, or a layout that no longer exists: the first by tab order.
    pub active_layout: Option<String>,
    pub views: Vec<NamedView>,
    /// Model space viewport configurations (VPORTS): the tiles shown and named ones.
    pub vports: Vec<VportConfig>,
    pub ucss: Vec<Ucs>,
    pub layer_states: Vec<LayerState>,
    pub groups: Vec<Group>,
    /// Plot style tables kept with the drawing (STYLESMANAGER); they shadow built-in tables of
    /// the same name.
    pub plot_style_tables: Vec<PlotStyleTable>,
    /// Parametric constraints (GEOMCONSTRAINT / DIMCONSTRAINT).
    pub constraints: Vec<Constraint>,
    /// User parameters and parametric settings.
    pub parametric: Parametric,
    /// The annotation scale list (SCALELISTEDIT).
    pub scales: Vec<AnnoScale>,
    /// Next free handle.
    pub handseed: u64,
    /// Objects hidden for this session by ISOLATEOBJECTS / HIDEOBJECTS. Never written to a file:
    /// a drawing opens with everything shown again.
    pub hidden_objects: BTreeSet<Handle>,
    /// What reading the file repaired or left out (such as undecodable DWG object records), told
    /// to the user when the drawing opens. Never written to a file.
    pub read_notes: Vec<String>,
    /// The in-place block reference edit in progress (REFEDIT). Never written to a file; files are
    /// written from [`Drawing::committed`].
    pub refedit: Option<RefEdit>,
    /// Records of the file this drawing was read from that the reader could not place (a damaged
    /// file), counted by the section they came from. Never written to a file.
    pub skipped_records: BTreeMap<String, usize>,
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing::new_imperial()
    }
}

impl Drawing {
    /// An empty drawing with the standard tables (inch-based, like acad.dwt-style defaults).
    pub fn new_imperial() -> Self {
        let mut d = Drawing::bare();
        d.header = Header::imperial();
        d
    }
    /// An empty metric drawing (millimetres, ISO-25 dimensions, multileaders and tables to match).
    pub fn new_metric() -> Self {
        let mut d = Drawing::bare();
        d.header = Header::metric();
        d.scales = metric_scales();
        d.dim_styles.push(DimStyle::iso25());
        d.mleader_styles = vec![MLeaderStyle::metric()];
        d.table_styles = vec![TableStyle::metric()];
        d.header.set_str("DIMSTYLE", "ISO-25");
        d
    }
    fn bare() -> Self {
        Drawing {
            header: Header::default(),
            foreign: std::sync::Arc::default(),
            layers: vec![Layer::default(), Layer { name: "Defpoints".into(), plot: false, ..Layer::default() }],
            linetypes: vec![
                Linetype { name: "ByBlock".into(), description: String::new(), pattern: Vec::new() },
                Linetype { name: "ByLayer".into(), description: String::new(), pattern: Vec::new() },
                Linetype::continuous(),
            ],
            text_styles: vec![TextStyle::default()],
            registered_apps: Vec::new(),
            plot_style_names: Vec::new(),
            dim_styles: vec![DimStyle::default()],
            mleader_styles: vec![MLeaderStyle::default()],
            table_styles: vec![TableStyle::default()],
            mline_styles: vec![MLineStyle::default()],
            image_definitions: Vec::new(),
            blocks: BTreeMap::new(),
            model: EntityStore::new(),
            layouts: vec![Layout::new("Layout1", 1), Layout::new("Layout2", 2)],
            active_layout: None,
            views: Vec::new(),
            vports: Vec::new(),
            ucss: Vec::new(),
            layer_states: Vec::new(),
            groups: Vec::new(),
            plot_style_tables: Vec::new(),
            constraints: Vec::new(),
            parametric: Parametric::default(),
            scales: imperial_scales(),
            handseed: 0x100,
            hidden_objects: BTreeSet::new(),
            read_notes: Vec::new(),
            refedit: None,
            skipped_records: BTreeMap::new(),
        }
    }

    /// Allocate a fresh handle.
    pub fn new_handle(&mut self) -> Handle {
        identity::allocate_handle(&mut self.handseed)
    }
    /// Make sure future handles don't collide with `h` (after loading a file).
    pub fn bump_handseed(&mut self, h: Handle) {
        if h.0 >= self.handseed {
            self.handseed = h.0.saturating_add(1);
        }
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn layer_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn linetype(&self, name: &str) -> Option<&Linetype> {
        self.linetypes.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn text_style(&self, name: &str) -> Option<&TextStyle> {
        self.text_styles.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn dim_style(&self, name: &str) -> Option<&DimStyle> {
        self.dim_styles.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    /// The DIM* variables in the header that differ from the current dimension style, as
    /// [`DimStyle`] fields: AutoCAD's style overrides (set with SETVAR), which new dimensions
    /// take on.
    pub fn dim_overrides(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut out = serde_json::Map::new();
        let Some(st) = self.dim_style(&self.header.str("DIMSTYLE", "Standard")) else { return out };
        let base = serde_json::to_value(st).unwrap_or_default();
        for (var, field) in DIMVARS {
            let Some(v) = self.header.get(var).and_then(|h| serde_json::to_value(h).ok()) else { continue };
            let mut cand = st.clone();
            if !cand.apply_fields(&serde_json::Map::from_iter([((*field).to_string(), v)])).is_empty() {
                continue;
            }
            let Some(cv) = serde_json::to_value(&cand).ok().and_then(|c| c.get(*field).cloned()) else { continue };
            let same = match (base.get(*field), &cv) {
                // Text style and arrow block names compare without case.
                (Some(serde_json::Value::String(a)), serde_json::Value::String(b))
                    if matches!(
                        *field,
                        "textStyle"
                            | "arrowBlock"
                            | "arrowBlock1"
                            | "arrowBlock2"
                            | "leaderArrowBlock"
                            | "dimLinetype"
                            | "extLinetype1"
                            | "extLinetype2"
                    ) =>
                {
                    a.eq_ignore_ascii_case(b)
                }
                (b, cv) => b == Some(cv),
            };
            if !same {
                out.insert((*field).to_string(), cv);
            }
        }
        out
    }

    /// Set the header's DIM* variables to the current dimension style's values, clearing the
    /// overrides (as making a style current or editing it does). Variables the header doesn't
    /// carry stay absent; ones that can't hold the style's value are removed.
    pub fn sync_dim_vars(&mut self) {
        let Some(st) = self.dim_style(&self.header.str("DIMSTYLE", "Standard")) else { return };
        let v = serde_json::to_value(st).unwrap_or_default();
        for (var, field) in DIMVARS {
            let Some(old) = self.header.get(var) else { continue };
            let new = match (old, v.get(*field)) {
                (HVal::Int(_), Some(serde_json::Value::Bool(b))) => Some(HVal::Int(i64::from(*b))),
                (HVal::Int(_), Some(serde_json::Value::Number(n))) => {
                    n.as_i64().or_else(|| n.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 1e15).map(|f| f as i64)).map(HVal::Int)
                }
                (HVal::Real(_), Some(serde_json::Value::Number(n))) => n.as_f64().map(HVal::Real),
                (HVal::Str(_), Some(serde_json::Value::String(s))) => Some(HVal::Str(s.clone())),
                _ => None,
            };
            match new {
                Some(h) => self.header.set(var, h),
                None => {
                    self.header.vars.remove(*var);
                }
            }
        }
    }
    pub fn block(&self, name: &str) -> Option<&std::sync::Arc<Block>> {
        self.blocks.get(name).or_else(|| self.blocks.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v))
    }
    pub fn layout(&self, name: &str) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    /// Add a layer if missing; returns whether it was created.
    pub fn ensure_layer(&mut self, name: &str) -> bool {
        if self.layer(name).is_some() {
            return false;
        }
        self.layers.push(Layer::new(name));
        true
    }

    /// The layouts in the order their paper space blocks are named in a file: the active one
    /// (`*Paper_Space`) first, then the others by tab order (`*Paper_Space0`, `*Paper_Space1`, …).
    pub fn paper_space_layouts(&self) -> Vec<&Layout> {
        let mut ls: Vec<&Layout> = self.layouts.iter().collect();
        ls.sort_by_key(|l| l.tab_order);
        if let Some(i) = self.active_layout.as_ref().and_then(|a| ls.iter().position(|l| l.name.eq_ignore_ascii_case(a))) {
            let active = ls.remove(i);
            ls.insert(0, active);
        }
        ls
    }

    pub fn space(&self, space: &Space) -> Option<&EntityStore> {
        match space {
            Space::Model => Some(&self.model),
            Space::Paper(n) => self.layouts.iter().find(|l| &l.name == n).map(|l| &l.entities),
        }
    }
    pub fn space_mut(&mut self, space: &Space) -> Option<&mut EntityStore> {
        match space {
            Space::Model => Some(&mut self.model),
            Space::Paper(n) => self.layouts.iter_mut().find(|l| &l.name == n).map(|l| &mut l.entities),
        }
    }

    /// Find an entity in model space or any layout.
    pub fn entity(&self, h: Handle) -> Option<&std::sync::Arc<Entity>> {
        self.model.get(h).or_else(|| self.layouts.iter().find_map(|l| l.entities.get(h)))
    }
    /// Find a database entity by record identity in model space, layouts or block definitions.
    pub fn entity_including_blocks(&self, h: Handle) -> Option<&std::sync::Arc<Entity>> {
        self.entity(h).or_else(|| self.blocks.values().find_map(|b| b.entities.get(h)))
    }

    /// Resolve a reference in its owner's coordinate database, never by falling back from a
    /// block-local reference to an unrelated model/layout record.
    pub fn association_entity(&self, owner: Handle, target: Handle) -> Option<&std::sync::Arc<Entity>> {
        if self.model.contains(owner) {
            return self.model.get(target);
        }
        if let Some(layout) = self.layouts.iter().find(|layout| layout.entities.contains(owner)) {
            return layout.entities.get(target);
        }
        self.blocks.values().find(|block| block.entities.contains(owner)).and_then(|block| block.entities.get(target))
    }

    pub fn space_of(&self, h: Handle) -> Option<Space> {
        if self.model.contains(h) {
            return Some(Space::Model);
        }
        self.layouts.iter().find(|l| l.entities.contains(h)).map(|l| Space::Paper(l.name.clone()))
    }
    /// Modify legacy geometry only, with revision and candidate checks before commit.
    pub fn modify_entity<F: FnOnce(&mut Entity)>(&mut self, h: Handle, f: F) -> Result<()> {
        self.try_modify_entity(h, |entity| {
            f(entity);
            Ok(())
        })
    }
    /// Commit one legacy candidate edit only after its revision check and operation succeed.
    pub fn try_modify_entity<F, T>(&mut self, h: Handle, edit: F) -> Result<T>
    where
        F: FnOnce(&mut Entity) -> Result<T>,
    {
        let (candidate, result) = self.entity(h).ok_or(DocError::NoEntity(h))?.legacy_edited(edit)?;
        self.commit_entity_candidate(candidate)?;
        Ok(result)
    }

    /// Edit appearance metadata without exposing the payload, revision, or identity to the callback.
    pub fn try_modify_metadata<F, T>(&mut self, h: Handle, edit: F) -> Result<T>
    where
        F: FnOnce(&mut Common) -> Result<T>,
    {
        let (candidate, result) = self.entity(h).ok_or(DocError::NoEntity(h))?.metadata_edited(edit)?;
        self.commit_entity_candidate(candidate)?;
        Ok(result)
    }

    /// Apply the entity's typed affine operation, independently from legacy edit callbacks.
    pub fn transform_entity(&mut self, h: Handle, transform: &cadcraft_geom::Mat4) -> Result<()> {
        let candidate = self.entity(h).ok_or(DocError::NoEntity(h))?.transformed(transform)?;
        self.commit_entity_candidate(candidate)
    }

    /// Commit a canonical `candidate` for the existing entity with its handle: the entity and the candidate are both canonical
    /// and of the same kind, the candidate's nested payloads (a multileader's context and block attributes, a block
    /// reference's attributes) are canonical too, and its numbers are finite. Anything else refuses and leaves the drawing as
    /// it was. It replaces the entity in its space the way [`Drawing::transform_entity`] does.
    pub fn commit_canonical_entity(&mut self, candidate: Entity) -> Result<()> {
        let h = candidate.handle;
        let current = self.entity(h).ok_or(DocError::NoEntity(h))?;
        if current.geometry_revision != entity::GeometryRevision::CanonicalV2 {
            return Err(DocError::Invalid("A canonical edit needs an entity with a canonical payload.".into()));
        }
        if std::mem::discriminant(&current.kind) != std::mem::discriminant(&candidate.kind) {
            return Err(DocError::Invalid("A canonical edit cannot change the kind of an entity.".into()));
        }
        candidate.require_canonical_geometry()?;
        if !has_finite_numbers(&candidate) {
            return Err(DocError::NonFinite(h));
        }
        self.commit_entity_candidate(candidate)
    }

    fn commit_entity_candidate(&mut self, candidate: Entity) -> Result<()> {
        let h = candidate.handle;
        let space = self.space_of(h).ok_or(DocError::NoEntity(h))?;
        let store = self.space_mut(&space).ok_or(DocError::NoEntity(h))?;
        if store.replace(candidate) { Ok(()) } else { Err(DocError::NoEntity(h)) }
    }

    /// Modify a globally unique record in model/layout space or a block definition.
    pub fn modify_entity_including_blocks<F: FnOnce(&mut Entity)>(&mut self, h: Handle, f: F) -> Result<()> {
        if self.space_of(h).is_some() {
            return self.modify_entity(h, f);
        }
        let block = self.blocks.values_mut().find(|block| block.entities.contains(h)).ok_or(DocError::NoEntity(h))?;
        let (candidate, ()) = block.entities.get(h).ok_or(DocError::NoEntity(h))?.legacy_edited(|entity| {
            f(entity);
            Ok(())
        })?;
        if std::sync::Arc::make_mut(block).entities.replace(candidate) { Ok(()) } else { Err(DocError::NoEntity(h)) }
    }

    pub fn remove_entity(&mut self, h: Handle) -> Option<std::sync::Arc<Entity>> {
        let sp = self.space_of(h)?;
        self.space_mut(&sp)?.remove(h)
    }

    /// Add an entity to a space with a new handle, using the given common properties.
    pub fn add(&mut self, space: &Space, common: Common, mut kind: EntityKind) -> Result<Handle> {
        let h = self.new_handle();
        self.allocate_attribute_handles(&mut kind);
        self.ensure_layer(&common.layer);
        let store = self.space_mut(space).ok_or_else(|| DocError::Invalid(format!("no such space {space:?}")))?;
        store.push(Entity { geometry_revision: Default::default(), handle: h, common, kind });
        Ok(h)
    }

    /// Effective visibility of an entity (its own flag, its layer on/thawed, not hidden by object
    /// isolation).
    pub fn is_visible(&self, e: &Entity) -> bool {
        e.common.visible && self.layer(&e.common.layer).is_none_or(Layer::visible) && !self.is_hidden_object(e.handle)
    }

    /// Whether ISOLATEOBJECTS / HIDEOBJECTS hid `h` for this session.
    pub fn is_hidden_object(&self, h: Handle) -> bool {
        !self.hidden_objects.is_empty() && self.hidden_objects.contains(&h)
    }

    /// Bounds of all visible entities in a space.
    pub fn legacy_extents(&self, space: &Space) -> Result<cadcraft_geom::Bounds2> {
        let mut b = cadcraft_geom::Bounds2::EMPTY;
        if let Some(store) = self.space(space) {
            for e in store.iter() {
                if self.is_visible(e) {
                    b = b.union(&legacy_entity_bounds(self, e, 0)?);
                }
            }
        }
        Ok(b)
    }

    /// Total entity count across model and paper space.
    pub fn entity_count(&self) -> usize {
        self.model.len() + self.layouts.iter().map(|l| l.entities.len()).sum::<usize>()
    }

    /// Next unused anonymous block name with the given prefix (e.g. `*D`).
    pub fn anonymous_block_name(&self, prefix: &str) -> String {
        let mut i = 1usize;
        loop {
            let n = format!("{prefix}{i}");
            if self.block(&n).is_none() || i > 1_000_000 {
                return n;
            }
            i += 1;
        }
    }
}

mod anno;
pub use anno::*;

#[cfg(test)]
mod tests;
