//! Document reference resolution is shared. Geometry interpretation follows the payload revision.

use cadcraft_doc::{DimKind, Dimension, EntityKind, GeometryRevision, Handle};
use cadcraft_geom::{OcsFrame, Vec3};

use super::common::T;
use super::{Rx, Tag};

pub(super) fn entity_extras(kind: &str, tags: &[Tag], h: Handle, k: &mut EntityKind, rx: &mut Rx, revision: GeometryRevision) {
    if revision == GeometryRevision::RawSource {
        return;
    }
    resolve_references(kind, tags, h, k, rx);
    apply_geometry(kind, tags, k, rx, revision);
}

/// Resolve document names, properties and associations without reinterpreting coordinate frames.
pub(super) fn resolve_references(kind: &str, tags: &[Tag], h: Handle, k: &mut EntityKind, rx: &mut Rx) {
    match (kind, k) {
        ("DIMENSION", EntityKind::Dimension(dm)) => {
            dm.overrides = crate::dxf_ext::read_dstyle(tags, &rx.styles, &rx.brs, &rx.ltypes);
            dm.assoc = crate::dxf_ext::read_assoc(tags);
        }
        ("ACAD_TABLE", EntityKind::Table(table)) => {
            for cell in table.cells.iter_mut().flatten() {
                if let Some(content) = &mut cell.block {
                    match rx.brs.get(&content.block.to_ascii_uppercase()).filter(|n| !n.is_empty()) {
                        Some(name) => {
                            content.block = name.clone();
                            rx.inserted.insert(name.to_ascii_uppercase());
                        }
                        None => cell.block = None,
                    }
                }
            }
            let t = T(tags);
            if let Some(b) = t.s(2) {
                rx.table_blocks.insert(b.to_ascii_uppercase());
            }
            let ours = crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str() == "TABLE");
            if !ours {
                rx.table_style_fix.push((h, t.s(342).map(|sh| sh.trim().to_ascii_uppercase()).unwrap_or_default()));
            }
        }
        ("INSERT", EntityKind::Insert(ins)) => {
            rx.inserted.insert(ins.block.to_ascii_uppercase());
        }
        ("MULTILEADER" | "MLEADER", EntityKind::MLeader(m)) => {
            let pending = crate::dxf_mleader::resolve(tags, m, &rx.styles, &rx.brs, &rx.ltypes);
            if let Some(b) = &m.block {
                rx.inserted.insert(b.block.to_ascii_uppercase());
            }
            rx.mleader_fix.push((h, pending));
        }
        ("VIEWPORT", EntityKind::Viewport(v)) if v.frozen_layers.is_empty() => {
            v.frozen_layers = tags
                .iter()
                .take_while(|t| t.code < 1000)
                .filter(|t| t.code == 331)
                .filter_map(|t| rx.layers.get(&t.str().trim().to_ascii_uppercase()).cloned())
                .collect();
        }
        _ => {}
    }
}

fn apply_geometry(kind: &str, tags: &[Tag], k: &mut EntityKind, rx: &mut Rx, revision: GeometryRevision) {
    match (kind, k) {
        ("ATTDEF", EntityKind::AttDef(a)) if rx.from_dwg => {
            // This group is not stored in DWG; its reader supplies a fixed value.
            if let Some(m) = &mut a.multiline {
                m.type_72 = None;
            }
        }
        ("DIMENSION", EntityKind::Dimension(dm)) => {
            if let Some(center) = crate::dxf_ext::read_arclen(tags) {
                dm.kind = DimKind::ArcLength;
                dm.p15 = center;
            }
            dm.marks = crate::dxf_ext::read_marks(tags);
            let t = T(tags);
            let normal = Vec3::new(t.fd(210, 0.0), t.fd(220, 0.0), t.fd(230, 1.0));
            if revision == GeometryRevision::LegacyProjectedV1 {
                // Only the Legacy path normalizes graphics blocks through the existing XY shift.
                let shift = super::common::ocs_n(normal, t.p(12)).xy();
                if let Some(name) = dm.block.as_ref().filter(|_| shift.is_finite() && (shift.x != 0.0 || shift.y != 0.0)) {
                    rx.dim_block_shifts.push((name.to_ascii_uppercase(), shift));
                }
            }
            dimension_oblique(dm, &t, normal, revision);
        }
        _ => {}
    }
}

/// Group 52 is relative to the dimension line in the revision's own coordinate frame.
pub(super) fn dimension_oblique(dm: &mut Dimension, t: &T<'_>, normal: Vec3, revision: GeometryRevision) {
    let base = match dm.kind {
        DimKind::Linear { rotation } => Some(rotation),
        DimKind::Aligned => match revision {
            GeometryRevision::LegacyProjectedV1 => Some((dm.p14.xy() - dm.p13.xy()).angle()),
            GeometryRevision::CanonicalV2 => OcsFrame::new(normal).map(|frame| (frame.to_ocs(dm.p14) - frame.to_ocs(dm.p13)).xy().angle()),
            GeometryRevision::RawSource => None,
        },
        _ => None,
    };
    if let (Some(base), Some(oblique)) = (base, t.f(52).filter(|angle| angle.is_finite() && angle.rem_euclid(180.0) != 0.0)) {
        dm.marks.oblique = Some(cadcraft_geom::norm_angle(base + oblique.to_radians()));
    }
}
