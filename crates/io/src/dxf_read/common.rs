use super::{Color, Common, Lineweight, Rgb, Tag, Transparency, Vec2, Vec3};

/// Value lookup helpers over an entity's tags.
pub(super) struct T<'a>(pub(super) &'a [Tag]);

impl T<'_> {
    pub(super) fn f(&self, code: i32) -> Option<f64> {
        self.0.iter().find(|t| t.code == code).map(Tag::f64)
    }
    pub(super) fn fd(&self, code: i32, d: f64) -> f64 {
        self.f(code).unwrap_or(d)
    }
    pub(super) fn i(&self, code: i32) -> Option<i64> {
        self.0.iter().find(|t| t.code == code).map(Tag::i64)
    }
    pub(super) fn s(&self, code: i32) -> Option<String> {
        self.0.iter().find(|t| t.code == code).map(Tag::str)
    }
    pub(super) fn p(&self, code: i32) -> Vec3 {
        Vec3::new(self.fd(code, 0.0), self.fd(code + 10, 0.0), self.fd(code + 20, 0.0))
    }
    pub(super) fn all_f(&self, code: i32) -> Vec<f64> {
        self.0.iter().filter(|t| t.code == code).map(Tag::f64).collect()
    }
    pub(super) fn pts(&self, code: i32) -> Vec<Vec2> {
        let xs = self.all_f(code);
        let ys = self.all_f(code + 10);
        xs.into_iter().zip(ys).map(|(x, y)| Vec2::new(x, y)).collect()
    }
}

/// An entity's group 440: `0x01` in the top byte is ByBlock, `0x02` a fixed alpha (shown as a
/// percentage as for layers, the alpha itself kept beside it), anything else ByLayer.
pub(super) fn transparency_440(v: i64) -> (Transparency, Option<u8>) {
    match (v >> 24) & 0xff {
        1 => (Transparency::ByBlock, None),
        2 => {
            let alpha = u8::try_from(v & 0xff).unwrap_or(255);
            (Transparency::from_alpha(alpha), Some(alpha))
        }
        _ => (Transparency::ByLayer, None),
    }
}

/// The tags outside application-defined groups (`102 {` to `102 }`, whose codes are the
/// application's), with their positions in `tags`.
pub(super) fn outside_app(tags: &[Tag]) -> impl Iterator<Item = (usize, &Tag)> {
    let mut in_app = false;
    tags.iter().enumerate().filter(move |(_, t)| {
        if t.code == 102 {
            in_app = t.str().starts_with('{');
        }
        !in_app && t.code != 102
    })
}

/// The groups of the AcDbEntity subclass: the record up to the entity's own subclass marker (all
/// of it in files without markers, and in lists with no AcDbEntity marker, whose layer and colour
/// may follow the entity's marker), or up to an embedded object (101). Later groups reuse the
/// codes: an MTEXT's column width is 48. Markers inside an application-defined group (`102 {`
/// to `102 }`, whose codes are the application's) do not count, and neither do their values.
fn entity_groups(tags: &[Tag]) -> Vec<Tag> {
    let outside = || outside_app(tags);
    let marked = outside().any(|(_, t)| t.code == 100 && t.str() == "AcDbEntity");
    let end = outside().find(|(_, t)| (marked && t.code == 100 && t.str() != "AcDbEntity") || t.code == 101).map_or(tags.len(), |(i, _)| i);
    outside_app(tags.get(..end).unwrap_or(tags)).map(|(_, t)| t.clone()).collect()
}

pub(super) fn common(t: &T) -> Common {
    let groups = entity_groups(t.0);
    let e = T(&groups);
    let mut c = Common { layer: e.s(8).unwrap_or_else(|| "0".into()), linetype: e.s(6).unwrap_or_else(|| "ByLayer".into()), ..Common::default() };
    if let Some(v) = e.i(62) {
        c.color = Color::from_aci(v as i16);
        if v < 0 {
            c.color = Color::ByLayer;
            c.visible = true;
        }
    }
    if let Some(tc) = e.i(420) {
        c.color = Color::True(Rgb::from_u32(tc as u32));
    }
    if let Some(lw) = e.i(370) {
        c.lineweight = Lineweight::from_dxf(lw as i16);
    }
    if let Some(s) = e.f(48) {
        c.ltscale = if s.is_finite() && s >= 0.0 { s } else { 1.0 };
    }
    if e.i(60) == Some(1) {
        c.visible = false;
    }
    if let Some(v) = e.i(440) {
        (c.transparency, c.transparency_alpha) = transparency_440(v);
    }
    c.thickness = t.fd(39, 0.0);
    c.extrusion = Vec3::new(t.fd(210, 0.0), t.fd(220, 0.0), t.fd(230, 1.0));
    c
}

/// Resolve Common properties and reserve the source identity only after a canonical payload succeeds.
pub(super) fn canonical_common(mut common: Common, tags: &[Tag], drawing: &mut super::Drawing, rx: &super::Rx) -> Option<Common> {
    let style = super::entities::entity_plot_style(tags, rx);
    let mut in_entity = false;
    let requires_style = tags.iter().take_while(|tag| tag.code < 1000).any(|tag| {
        if tag.code == 100 {
            in_entity = tag.str() == "AcDbEntity";
        }
        in_entity && tag.code == 390 && super::Handle::parse_hex(&tag.str()).is_none_or(|handle| handle.0 != 0)
    });
    if requires_style && style.is_none() {
        return None;
    }
    if let Some(style) = style {
        common.plot_style = style;
    }
    if let Some(handle) = T(tags).s(5).and_then(|value| super::Handle::parse_hex(&value)) {
        drawing.bump_handseed(handle);
    }
    Some(common)
}

/// Apply the OCS → WCS transform for 2D entities with a non-default extrusion.
pub(super) fn ocs(c: &Common, p: Vec3) -> Vec3 {
    ocs_n(c.extrusion, p)
}

pub(super) fn ocs_n(n: Vec3, p: Vec3) -> Vec3 {
    if (n.z - 1.0).abs() < 1e-12 && n.x.abs() < 1e-12 && n.y.abs() < 1e-12 { p } else { cadcraft_geom::Mat4::ocs(n).apply(p) }
}
