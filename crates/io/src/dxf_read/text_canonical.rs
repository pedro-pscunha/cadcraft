//! The canonical reading of the text family (revision `CanonicalV2`): the groups' own coordinates, with no mirroring.
//! TEXT, ATTDEF and ATTRIB keep OCS points and the baseline angle, the normal with its source magnitude, the signed
//! thickness and the generation flags. MTEXT keeps its WCS insertion and its full WCS direction. An ATTRIB has its own
//! plane, and an embedded MTEXT has its own too.
//!
//! These functions are not part of `read_dxf` yet: the reader still reads legacy text (`text.rs`) until the shared
//! Drawing boundary accepts canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of
//! what `canonical_text_from_groups` returns.

use cadcraft_doc::{Attrib, AttribModes, AttribPlane, Common, EntityKind, GeometryRevision, Handle, MText, MultilineAttrib, Text};
use cadcraft_dxf::Tag;
use cadcraft_geom::{OcsFrame, Vec2, Vec3};

use super::common::{T, common};
use super::text::{attdef, attrib_modes, attrib_props, mtext_from, split_embedded, text_fields};

/// The groups of the text itself: those before its attribute subclass, where the attribute type (71) comes after the flags.
fn text_part(tags: &[Tag]) -> &[Tag] {
    let end = tags.iter().position(|t| t.code == 100 && t.str().starts_with("AcDbAttribute")).unwrap_or(tags.len());
    tags.get(..end).unwrap_or(tags)
}

fn normal_of(t: &T) -> Option<Vec3> {
    let n = Vec3::new(t.fd(210, 0.0), t.fd(220, 0.0), t.fd(230, 1.0));
    OcsFrame::new(n).map(|_| n)
}

/// A TEXT or an attribute with its generation flags (group 71).
fn canonical_text(own: &[Tag]) -> Text {
    let mut text = text_fields(&T(text_part(own)));
    let flags = T(text_part(own)).i(71).unwrap_or(0);
    text.backward = flags & 2 != 0;
    text.upside_down = flags & 4 != 0;
    text
}

/// Multiline text with its own plane from the MTEXT groups of an MTEXT entity (`entity_normal`) or an embedded object.
/// The direction is the full WCS vector when there is one, else the baseline angle (50) turned into the plane.
fn canonical_mtext(tags: &[Tag], c: &Common, normal: Vec3) -> Option<MText> {
    let frame = OcsFrame::new(normal)?;
    // `mtext_from` mirrors under a negative Z: read it with a +Z plane and keep the normal apart.
    let mut mtext = mtext_from(tags, &Common { extrusion: Vec3::Z, ..c.clone() });
    let own = T(super::mtext_dxf::before_columns(tags));
    let direction = match (own.f(11), own.f(21)) {
        (Some(x), Some(y)) if x != 0.0 || y != 0.0 || own.fd(31, 0.0) != 0.0 => Vec3::new(x, y, own.fd(31, 0.0)),
        _ => {
            let (s, c) = own.fd(50, 0.0).to_radians().sin_cos();
            frame.to_world(Vec3::new(c, s, 0.0))
        }
    };
    let local = frame.to_ocs(direction);
    mtext.rotation = Vec2::new(local.x, local.y).angle();
    mtext.direction = Some(direction);
    Some(mtext)
}

fn canonical_multiline(embedded: Option<&[Tag]>, c: &Common) -> Option<MultilineAttrib> {
    let embedded = embedded.filter(|e| !e.is_empty())?;
    let normal = normal_of(&T(embedded))?;
    let mtext = canonical_mtext(embedded, c, normal)?;
    let read_as = cadcraft_fonts::plain_mtext(&mtext.contents);
    Some(MultilineAttrib { geometry_revision: GeometryRevision::CanonicalV2, normal, mtext, read_as })
}

/// A TEXT, MTEXT or ATTDEF from the groups of a DXF record (without its `0` group), read canonically. `None` for another
/// kind, or when the normal has no direction. The caller sets the entity's `geometry_revision`.
pub fn canonical_text_from_groups(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
    let t = T(tags);
    let c = common(&t);
    match kind.to_ascii_uppercase().as_str() {
        "TEXT" => {
            normal_of(&t)?;
            Some((c, EntityKind::Text(canonical_text(tags))))
        }
        "MTEXT" => {
            let normal = normal_of(&t)?;
            let mtext = canonical_mtext(tags, &c, normal)?;
            Some((c, EntityKind::MText(mtext)))
        }
        "ATTDEF" => {
            normal_of(&t)?;
            let (own, embedded) = split_embedded(tags);
            let mut a = attdef(tags, &c);
            a.text = canonical_text(own);
            a.multiline = canonical_multiline(embedded, &c);
            if let Some(m) = &a.multiline {
                a.text.value.clone_from(&m.read_as);
            }
            Some((c, EntityKind::AttDef(a)))
        }
        _ => None,
    }
}

/// An ATTRIB instance from its groups, read canonically with its own plane. `None` when its normal (or its embedded
/// text's) has no direction.
pub fn canonical_attrib_from_groups(tags: &[Tag], insert_layer: &str, plot_style: Option<String>) -> Option<Attrib> {
    let tt = T(tags);
    let c = common(&tt);
    let (own, embedded) = split_embedded(tags);
    let multiline = canonical_multiline(embedded, &c);
    if embedded.is_some_and(|e| !e.is_empty()) && multiline.is_none() {
        return None;
    }
    let own_t = T(own);
    let normal = normal_of(&own_t)?;
    let mut text = canonical_text(own);
    if let Some(m) = &multiline {
        text.value.clone_from(&m.read_as);
    }
    let modes: AttribModes = attrib_modes(own);
    Some(Attrib {
        geometry_revision: GeometryRevision::CanonicalV2,
        handle: tt.s(5).and_then(|h| Handle::parse_hex(&h)).filter(|h| h.0 != 0),
        invisible: own_t.i(70).unwrap_or(0) & 1 != 0,
        props: attrib_props(&tt, insert_layer, plot_style),
        multiline,
        modes,
        plane: AttribPlane { normal, thickness: own_t.fd(39, 0.0) },
        ..Attrib::new(own_t.s(2).unwrap_or_default(), text)
    })
}
