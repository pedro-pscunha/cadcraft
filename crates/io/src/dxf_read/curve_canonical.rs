//! The canonical reading of CIRCLE, ARC and the 2D polylines (revision `CanonicalV2`): the groups' own OCS coordinates,
//! with no conversion to WCS and no mirroring under a negative normal.
//!
//! Not part of `read_dxf` yet: the reader still reads legacy curves (`entities.rs`) until the Drawing boundary accepts
//! canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of what they return.

use cadcraft_doc::{Arc, Circle, Common, Drawing, EntityKind};
use cadcraft_dxf::Tag;
use cadcraft_geom::OcsFrame;

use super::common::{T, common};

/// A CIRCLE, ARC or LWPOLYLINE from the groups of a DXF record (without its `0` group), read canonically. `None` for
/// another kind, a normal without direction, a radius that is not a positive finite number, or an LWPOLYLINE that is a
/// CADCraft region.
pub fn canonical_curve_from_groups(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
    let t = T(tags);
    let c = common(&t);
    OcsFrame::new(c.extrusion)?;
    let radius = || Some(t.fd(40, 1.0)).filter(|r| r.is_finite() && *r > 0.0);
    let kind = match kind.to_ascii_uppercase().as_str() {
        "CIRCLE" => EntityKind::Circle(Circle { center: t.p(10), radius: radius()? }),
        "ARC" => EntityKind::Arc(Arc { center: t.p(10), radius: radius()?, start: t.fd(50, 0.0).to_radians(), end: t.fd(51, 360.0).to_radians() }),
        "LWPOLYLINE" => {
            // The legacy reader mirrors under a negative Z: read the vertices as if the plane were +Z.
            let k = super::entities::lwpolyline(tags, &Common { extrusion: cadcraft_geom::Vec3::Z, ..c.clone() }, &t)?;
            if !matches!(k, EntityKind::LwPolyline(_)) {
                return None;
            }
            k
        }
        _ => return None,
    };
    Some((c, kind))
}

/// A 2D POLYLINE from its parent record and the child records that follow it (VERTEX, then SEQEND), each with its groups
/// without the `0` group, read canonically: the vertices stay in the OCS, with the fit data and record identities as
/// the legacy reader keeps them. `None` for a 3D polyline or a mesh, or a normal without direction.
pub fn canonical_polyline_from_groups(parent: &[Tag], children: &[(String, Vec<Tag>)]) -> Option<(Common, EntityKind)> {
    let t = T(parent);
    let flags = t.i(70).unwrap_or(0);
    if flags & (8 | 16 | 64) != 0 || OcsFrame::new(common(&t).extrusion).is_none() {
        return None;
    }
    let (mut drawing, rx) = (Drawing::default(), Default::default());
    let (c, kind) = super::entities::polyline_entity_in(children, &mut 0, &t, &mut drawing, &rx, true);
    matches!(kind, EntityKind::LwPolyline(_)).then_some((c, kind))
}
