//! The canonical reading of the fills (revision `CanonicalV2`): the groups' own coordinates, with no projection.
//!
//! Not part of `read_dxf` yet: the reader still reads legacy fills (`entities.rs`) until the Drawing boundary accepts
//! canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of what
//! `canonical_fill_from_groups` returns.

use cadcraft_doc::{Common, EntityKind, Wipeout, WipeoutFrame};
use cadcraft_dxf::Tag;

use super::common::{T, common};

/// A WIPEOUT from the groups of a DXF record (without its `0` group), read canonically. `None` for another kind, or when
/// the image frame is degenerate (its U and V vectors do not span a plane).
pub fn canonical_fill_from_groups(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
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
            EntityKind::Wipeout(Wipeout { boundary: Vec::new(), frame: Some(frame) })
        }
        _ => return None,
    };
    Some((c, kind))
}
