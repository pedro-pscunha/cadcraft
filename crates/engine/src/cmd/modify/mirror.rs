//! MIRROR's measured text policy and atomic candidate preparation.

use cadcraft_doc::{DocError, Drawing, Entity, EntityKind, GeometryRevision, Handle, TextMirror};
use cadcraft_geom::{Mat3, Mat4};

use crate::{EngineError, Result, Session};

pub(super) fn candidate(drawing: &Drawing, entity: &Entity, transform: &Mat4) -> Result<Entity> {
    if entity.geometry_revision == GeometryRevision::CanonicalV2
        && let EntityKind::Text(text) = &entity.kind
    {
        let style = drawing
            .text_style(&text.style)
            .ok_or_else(|| DocError::Invalid(format!("Canonical TEXT MIRROR cannot resolve style {}.", text.style)))?;
        if style.backwards || style.upside_down {
            return Err(DocError::Invalid("Canonical TEXT MIRROR lacks a measurement for pre-mirrored style flags.".into()).into());
        }
        let candidate = entity.mirrored(transform, TextMirror::Reflected)?;
        return match drawing.header.i64("MIRRTEXT", 0) {
            0 => Err(EngineError::TextMirrorMetricsUnavailable { font: style.dxf_font_name().into(), renderer_font: style.font.clone() }),
            1 => Ok(candidate),
            _ => Err(DocError::Invalid("MIRRTEXT must be 0 or 1.".into()).into()),
        };
    }
    Ok(entity.mirrored(transform, TextMirror::Reflected)?)
}

pub(super) fn apply(session: &mut Session, handles: &[Handle], matrix: &Mat3, copy: bool) -> Result<Vec<Handle>> {
    let drawing = session.doc_mut()?;
    let mut seen = std::collections::HashSet::new();
    let originals: Vec<_> = handles
        .iter()
        .filter(|handle| seen.insert(**handle))
        .filter_map(|handle| Some((drawing.space_of(*handle)?, drawing.entity(*handle)?.clone())))
        .filter(|(_, entity)| copy || !drawing.layer(&entity.common.layer).is_some_and(|layer| layer.locked))
        .collect();
    let transform = Mat4::from_mat3(matrix);
    let mut candidates = Vec::with_capacity(originals.len());
    for (_, entity) in &originals {
        if entity.geometry_revision == GeometryRevision::LegacyProjectedV1 {
            super::transform::check_ellipse_transform(entity, matrix)?;
        }
        candidates.push(candidate(drawing, entity, &transform)?);
    }
    let mut out = Vec::with_capacity(candidates.len());
    if copy {
        let copies = drawing.duplicate_entities(vec![candidates], cadcraft_doc::DuplicationSource::SameDrawing)?;
        for ((space, _), entity) in originals.into_iter().zip(copies) {
            out.push(entity.handle);
            drawing.space_mut(&space).ok_or_else(|| DocError::Invalid("The MIRROR target space no longer exists.".into()))?.push(entity);
        }
    } else {
        for candidate in candidates {
            let handle = candidate.handle;
            if candidate.geometry_revision == GeometryRevision::CanonicalV2 {
                drawing.commit_canonical_entity(candidate)?;
            } else {
                drawing.try_modify_entity(handle, |entity| {
                    *entity = candidate;
                    Ok(())
                })?;
            }
            out.push(handle);
        }
    }
    Ok(out)
}
