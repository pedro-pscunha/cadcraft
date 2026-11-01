//! Revision-aware extent queries. A refusal retains every available bound and the refused source handles.

use std::fmt;

use cadcraft_doc::{DocError, Drawing, Entity, EntityKind, GeometryRevision, Space};
use cadcraft_geom::{Bounds2, Mat4};

use crate::{GeometryRefusal, entity_world_bounds};

#[derive(Clone, Debug, PartialEq)]
pub struct ExtentsRefusal {
    pub bounds: Bounds2,
    pub refusals: Vec<GeometryRefusal>,
}

impl fmt::Display for ExtentsRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Extent query refused")?;
        for refusal in &self.refusals {
            write!(formatter, " object {}: {};", refusal.handle.hex(), refusal.reason)?;
        }
        Ok(())
    }
}

impl std::error::Error for ExtentsRefusal {}

impl From<ExtentsRefusal> for DocError {
    fn from(refusal: ExtentsRefusal) -> Self {
        DocError::Invalid(refusal.to_string())
    }
}

/// Extents of one entity in its source space. The legacy depth remains part of its existing bounds contract.
pub fn entity_extents(drawing: &Drawing, entity: &Entity, depth: usize) -> Result<Bounds2, ExtentsRefusal> {
    if entity.unresolved_display().is_some() {
        return Ok(Bounds2::EMPTY);
    }
    let result = if entity.geometry_revision == GeometryRevision::CanonicalV2 {
        if matches!(entity.kind, EntityKind::Ray(_) | EntityKind::XLine(_)) {
            Ok(Bounds2::EMPTY)
        } else {
            entity_world_bounds(drawing, entity, &Mat4::IDENTITY)
        }
    } else {
        cadcraft_doc::legacy_entity_bounds(drawing, entity, depth)
    };
    result.map_err(|error| ExtentsRefusal {
        bounds: Bounds2::EMPTY,
        refusals: vec![GeometryRefusal { handle: entity.handle, reason: error.to_string() }],
    })
}

/// Extents of an entity set, including all available bounds when some source objects refuse.
pub fn entities_extents<'a>(drawing: &Drawing, entities: impl IntoIterator<Item = &'a Entity>, depth: usize) -> Result<Bounds2, ExtentsRefusal> {
    let mut bounds = Bounds2::EMPTY;
    let mut refusals = Vec::new();
    for entity in entities {
        match entity_extents(drawing, entity, depth) {
            Ok(entity_bounds) => bounds = bounds.union(&entity_bounds),
            Err(refused) => {
                bounds = bounds.union(&refused.bounds);
                refusals.extend(refused.refusals);
            }
        }
    }
    if refusals.is_empty() { Ok(bounds) } else { Err(ExtentsRefusal { bounds, refusals }) }
}

/// Extents of the entities in a space that count in its extents ([`Drawing::in_extents`]: a layer that is off counts, a frozen one
/// does not, as AutoCAD 2024 counts them). Refused objects never receive a projected fallback box.
pub fn space_extents(drawing: &Drawing, space: &Space) -> Result<Bounds2, ExtentsRefusal> {
    let mut bounds = Bounds2::EMPTY;
    let mut refusals = Vec::new();
    for entity in drawing.space(space).into_iter().flat_map(|store| store.iter()).filter(|entity| drawing.in_extents(entity)) {
        let result = if entity.unresolved_display().is_some() {
            Ok(Bounds2::EMPTY)
        } else if entity.geometry_revision == GeometryRevision::CanonicalV2 {
            if matches!(entity.kind, EntityKind::Ray(_) | EntityKind::XLine(_)) {
                Ok(Bounds2::EMPTY)
            } else {
                crate::world_bounds::entity_extent_bounds(drawing, entity, &Mat4::IDENTITY)
            }
        } else {
            cadcraft_doc::legacy_extent_bounds(drawing, entity)
        };
        match result {
            Ok(entity_bounds) => bounds = bounds.union(&entity_bounds),
            Err(error) => refusals.push(GeometryRefusal { handle: entity.handle, reason: error.to_string() }),
        }
    }
    if refusals.is_empty() { Ok(bounds) } else { Err(ExtentsRefusal { bounds, refusals }) }
}
