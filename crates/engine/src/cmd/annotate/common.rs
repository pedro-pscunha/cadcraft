use super::{Accept, AssocSnap, DimAssoc, DimKind, Dimension, EntityKind, Handle, Input, Prompt, Result, Segment, Session, Value, Vec2, Vec3, v3};

pub(super) fn style(s: &Session) -> String {
    s.doc().map(|d| d.header.str("DIMSTYLE", "Standard")).unwrap_or_else(|_| "Standard".into())
}

pub(super) fn dim(s: &Session, kind: DimKind, defpt: Vec2, p13: Vec2, p14: Vec2, p15: Vec2, text: &str) -> EntityKind {
    EntityKind::Dimension(Dimension {
        kind,
        defpt: v3(defpt),
        text_mid: Vec3::ZERO,
        p13: v3(p13),
        p14: v3(p14),
        p15: v3(p15),
        p16: v3(Vec2::ZERO),
        text: text.into(),
        style: style(s),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: s.doc().map(|d| d.dim_overrides()).unwrap_or_default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    })
}

/// Horizontal (0) or vertical (90°) from where the dimension line is placed (AutoCAD's rule).
fn auto_rotation(p1: Vec2, p2: Vec2, at: Vec2) -> f64 {
    let (x0, x1) = (p1.x.min(p2.x), p1.x.max(p2.x));
    let (y0, y1) = (p1.y.min(p2.y), p1.y.max(p2.y));
    let outside_y = at.y < y0 || at.y > y1;
    let outside_x = at.x < x0 || at.x > x1;
    if outside_x && !outside_y { std::f64::consts::FRAC_PI_2 } else { 0.0 }
}

pub(super) fn linear_kind(p1: Vec2, p2: Vec2, at: Vec2, rotation: Option<f64>, aligned: bool) -> DimKind {
    if aligned { DimKind::Aligned } else { DimKind::Linear { rotation: rotation.unwrap_or_else(|| auto_rotation(p1, p2, at)) } }
}

/// Add a dimension. With DIMASSOC on, definition points that sit on object endpoints,
/// vertices or centres are attached to them (associative dimensions).
pub(super) fn add_dim(s: &mut Session, k: EntityKind) -> Result<Handle> {
    let mut k = k;
    if let EntityKind::Dimension(dm) = &mut k
        && dm.assoc.is_empty()
        && assoc_enabled(s)
    {
        dm.assoc = crate::assoc::reassociate(s.doc()?, &s.space(), dm);
    }
    if let EntityKind::Dimension(dm) = &mut k {
        take_object_z(s, dm);
    }
    let h = s.draw(k)?;
    s.last_dim = Some(h);
    Ok(h)
}

/// The Z of the point of `object` that an associative definition point sits on.
fn object_z(s: &Session, object: Handle, snap: &AssocSnap) -> f64 {
    let Some(entity) = s.doc().ok().and_then(|d| d.entity(object)) else { return 0.0 };
    match &entity.kind {
        EntityKind::Circle(c) => c.center.z,
        EntityKind::Arc(a) => a.center.z,
        EntityKind::Line(l) if *snap == AssocSnap::End => l.b.z,
        EntityKind::Line(l) => l.a.z,
        EntityKind::LwPolyline(p) => p.elevation,
        _ => 0.0,
    }
}

/// A dimension attached to an object takes the object's Z for the points that come from it, and the current elevation for the points
/// that were picked (the location, the text), as AutoCAD does with an object at Z 7 and ELEVATION 3 (measured in LT 2025). `Session::draw`
/// adds the elevation to every point after this, so the shift here is the difference. An arc length dimension also measures from the
/// arc's centre and end (15, 16), and a jogged radius dimension puts its override centre and its jog on the circle's plane.
fn take_object_z(s: &Session, dm: &mut Dimension) {
    let elevation = s.elevation();
    let mut z: Vec<(&str, f64)> = dm.assoc.iter().map(|a| (a.point.as_str(), object_z(s, a.handle, &a.snap) - elevation)).collect();
    let first = z.first().map(|(_, v)| *v);
    if let (DimKind::ArcLength, Some(v)) = (dm.kind, first) {
        z.extend([("p15", v), ("p16", v)]);
    }
    for (name, shift) in z {
        match name {
            "defpt" => dm.defpt.z += shift,
            "p13" => dm.p13.z += shift,
            "p14" => dm.p14.z += shift,
            "p15" => dm.p15.z += shift,
            "p16" => dm.p16.z += shift,
            _ => {}
        }
    }
    if let (Some(jogged), Some(attached)) = (&mut dm.marks.jogged, dm.assoc.iter().find(|a| a.point == "defpt")) {
        let shift = object_z(s, attached.handle, &attached.snap) - elevation;
        jogged.center.z += shift;
        jogged.jog.z += shift;
    }
}

pub(super) fn assoc_enabled(s: &Session) -> bool {
    s.doc().map(|d| d.header.i64("DIMASSOC", 2) != 0).unwrap_or(false)
}

pub(super) fn with_assoc(k: EntityKind, assoc: Vec<DimAssoc>) -> EntityKind {
    match k {
        EntityKind::Dimension(mut d) => {
            d.assoc = assoc;
            EntityKind::Dimension(d)
        }
        other => other,
    }
}

pub(in crate::cmd) fn require_legacy_source(s: &Session, handle: Handle) -> Result<()> {
    if let Some(entity) = s.doc()?.entity(handle) {
        entity.require_legacy_geometry()?;
    }
    Ok(())
}

/// Centre and radius of a circle/arc entity.
pub(super) fn circle_of(s: &Session, h: Handle) -> Option<(Vec2, f64)> {
    match &s.doc().ok()?.entity(h)?.kind {
        EntityKind::Circle(c) => Some((c.center.xy(), c.radius)),
        EntityKind::Arc(a) => Some((a.center.xy(), a.radius)),
        _ => None,
    }
}

/// The dimension line location of a radius/diameter dimension puts the text there, inside or
/// outside the circle, as a user-positioned text (DXF flag 70 bit 128), like AutoCAD.
pub(super) fn text_at(mut k: EntityKind, at: Vec2) -> EntityKind {
    if let EntityKind::Dimension(d) = &mut k {
        d.text_mid = v3(at);
        d.user_text_pos = true;
    }
    k
}

pub(super) fn hex_param(p: &Value, key: &str) -> Option<Handle> {
    p.get(key).and_then(|v| v.as_str().and_then(Handle::parse_hex).or_else(|| v.as_u64().map(Handle)))
}

// ---------------- interactive ----------------

pub(super) fn pick_at(s: &Session, p: Vec2) -> Option<Handle> {
    let ap = s.pixel_size() * s.settings.pickbox.max(1.0) * 1.5;
    crate::select::pick(s.doc().ok()?, &s.space(), p, ap)
}

/// The straight or curved piece of `h` picked at `p`: a line, an arc, or the polyline segment nearest `p`.
pub(in crate::cmd) fn segment_at(s: &Session, h: Handle, p: Vec2) -> Option<Segment> {
    match &s.doc().ok()?.entity(h)?.kind {
        EntityKind::Line(l) => Some(Segment::Line(cadcraft_geom::Line::new(l.a.xy(), l.b.xy()))),
        EntityKind::Arc(a) => Some(Segment::Arc { arc: cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end), ccw: true }),
        EntityKind::LwPolyline(pl) => cadcraft_geom::Polyline { vertices: pl.vertices.clone(), closed: pl.closed }
            .segments()
            .into_iter()
            .min_by(|x, y| x.closest(p).dist(p).total_cmp(&y.closest(p).dist(p))),
        _ => None,
    }
}

/// What a dimension placement prompt asks after Mtext, Text or Angle.
#[derive(Clone, Copy, PartialEq)]
enum DimAsk {
    Text,
    Angle,
}

/// The Mtext / Text / Angle options of the dimension placement prompts: replace the measured text
/// (`<>` stands for the measurement) and rotate the text.
#[derive(Default)]
pub(super) struct DimText {
    text: String,
    angle: f64,
    asking: Option<DimAsk>,
}

impl DimText {
    /// Opens the sub-prompt for Mtext, Text or Angle; `false` for any other keyword.
    pub(super) fn keyword(&mut self, k: &str) -> bool {
        self.asking = match k {
            "Mtext" | "Text" => Some(DimAsk::Text),
            "Angle" => Some(DimAsk::Angle),
            _ => return false,
        };
        true
    }
    pub(super) fn prompt(&self) -> Option<Prompt> {
        Some(match self.asking? {
            DimAsk::Text => Prompt::new("Enter dimension text (<> = measurement)", Accept::TEXT),
            DimAsk::Angle => Prompt::new("Specify angle of dimension text", Accept::NUMBER),
        })
    }
    /// Takes the answer to the open sub-prompt (Enter keeps the current value); `false` when none is open.
    pub(super) fn answer(&mut self, s: &mut Session, i: &Input) -> bool {
        let Some(a) = self.asking else { return false };
        match (a, i) {
            (DimAsk::Text, Input::Text(t)) => self.text = t.clone(),
            (DimAsk::Angle, Input::Text(t)) => match s.angle_settings().direction(t) {
                Some(r) => self.angle = r,
                None => {
                    s.echo("Requires a valid angle.");
                    return true;
                }
            },
            _ => {}
        }
        self.asking = None;
        true
    }
    /// `k` with this text and text angle.
    pub(super) fn apply(&self, mut k: EntityKind) -> EntityKind {
        if let EntityKind::Dimension(d) = &mut k {
            d.text = self.text.clone();
            d.text_rotation = self.angle;
        }
        k
    }
}
