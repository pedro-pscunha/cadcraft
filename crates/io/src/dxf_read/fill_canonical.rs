//! The canonical reading of the fills (revision `CanonicalV2`): the groups' own coordinates, with no projection.
//!
//! Not part of `read_dxf` yet: the reader still reads legacy fills (`entities.rs`) until the Drawing boundary accepts
//! canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of what
//! `canonical_fill_from_groups` returns.

use cadcraft_doc::{Common, EntityKind, HelixFrame, Wipeout, WipeoutFrame};
use cadcraft_dxf::Tag;
use cadcraft_geom::{OcsFrame, Vec3};

use super::common::{T, canonical_common, common};
use super::{Drawing, Rx};

/// A WIPEOUT, HELIX, MLINE or HATCH from the groups of a DXF record (without its `0` group), read canonically. `None` for another
/// kind, or when the image frame of a wipeout is degenerate (its U and V vectors do not span a plane) or the axis of a
/// helix has no direction.
pub fn canonical_fill_from_groups(kind: &str, tags: &[Tag], drawing: &mut Drawing, rx: &Rx) -> Option<(Common, EntityKind)> {
    let t = T(tags);
    let c = common(&t);
    let kind = match kind.to_ascii_uppercase().as_str() {
        "WIPEOUT" => {
            let frame = WipeoutFrame {
                origin: t.p(10),
                u: t.p(11),
                v: t.p(12),
                size: cadcraft_geom::Vec2::new(t.fd(13, 1.0), t.fd(23, 1.0)),
                clip_type: u8::try_from(t.i(71).unwrap_or(2)).ok()?,
                clip: t.pts(14),
            };
            frame.world_boundary().ok()?;
            EntityKind::Wipeout(Wipeout { boundary: Vec::new(), display: super::entities::wipeout_display(&t), frame: Some(frame), image: None })
        }
        "MLINE" => {
            let normal = Vec3::new(t.fd(210, 0.0), t.fd(220, 0.0), t.fd(230, 1.0));
            OcsFrame::new(normal)?;
            let mline = crate::dxf_mline::read_canonical(tags, normal);
            mline.canonical_frames().ok()?;
            EntityKind::MLine(mline)
        }
        "HATCH" => {
            let hatch = super::hatch::exact_hatch(tags, &t)?;
            OcsFrame::new(c.extrusion)?;
            hatch
        }
        "HELIX" => {
            let hs = tags.iter().position(|g| g.code == 100 && g.str() == "AcDbHelix").unwrap_or(tags.len());
            let h = T(tags.get(hs..).unwrap_or_default());
            let frame = HelixFrame { axis: h.p(12), start: h.p(11) };
            let mut helix = super::entities::helix(tags);
            helix.frame = Some(frame);
            helix.start_angle = 0.0;
            // Without a turn height the height is how far the spline's end lies along the axis.
            if !h.f(42).is_some_and(f64::is_finite)
                && let Some(end) = super::entities::helix_spline_end(tags)
            {
                helix.height = (end - helix.center).dot(frame.axis.normalized());
            }
            helix.world_point_at(0.0).ok()?;
            EntityKind::Helix(helix)
        }
        _ => return None,
    };
    Some((canonical_common(c, tags, drawing, rx)?, kind))
}
