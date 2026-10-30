//! Recovery of old serialized SPLINE and LEADER payloads.
//!
//! A payload serialized before the spatial model has no geometry revision and no Z values or leader frame. When its plane
//! is +Z (the normal has only a positive Z component, as when the record had no normal), the payload keeps its 2D meaning
//! exactly: its Z values are 0 and its leader fields have their defaults, so it is promoted to the canonical revision
//! without changing a number in it. A payload on any other plane lost its Z values and its projection when it was
//! imported: it stays `LegacyProjectedV1`, opens, displays and saves as before, and its edits follow the legacy rules.
//! Recovery of pre-revision payloads does not fail. New raw units must keep their validated identity and raw revision.

use cadcraft_geom::{SplineSpace, Vec3};

use super::dim::LeaderFrame;
use super::{Common, Entity, EntityKind, GeometryRevision, Handle};
use crate::{Family, FamilySet};

#[derive(serde::Deserialize)]
pub(super) struct SerializedEntity {
    #[serde(default, rename = "geometryRevision")]
    geometry_revision: GeometryRevision,
    handle: Handle,
    #[serde(default)]
    common: Common,
    kind: EntityKind,
}

// Deserialization does not promote: until display, snaps, pick and grips read canonical payloads, a promoted
// spline or leader would be refused by every edit that still takes the legacy path. A load promotes with
// `apply_load_policy`, after it, with the families of its caller.
impl TryFrom<SerializedEntity> for Entity {
    type Error = crate::DocError;
    fn try_from(value: SerializedEntity) -> crate::Result<Self> {
        let geometry_revision = if let EntityKind::Raw(raw) = &value.kind {
            if value.geometry_revision != GeometryRevision::RawSource || value.handle != raw.parent().handle() {
                return Err(crate::DocError::Invalid("A raw entity must keep its source identity and raw revision.".into()));
            }
            raw.validate_common(&value.common)?;
            GeometryRevision::RawSource
        } else {
            if value.geometry_revision == GeometryRevision::RawSource {
                return Err(crate::DocError::Invalid("Only an opaque raw unit can use the raw revision.".into()));
            }
            value.geometry_revision
        };
        Ok(Entity { geometry_revision, handle: value.handle, common: value.common, kind: value.kind })
    }
}

/// What [`promote_legacy_spline_leader`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Promotion {
    /// The payload is now canonical, its numbers unchanged.
    Promoted,
    /// A legacy spline or leader on a plane that is not +Z: left exactly as it was.
    KeptLegacy,
    /// Not a legacy spline or leader.
    NotApplicable,
}

fn is_plus_z(normal: Vec3) -> bool {
    normal.x == 0.0 && normal.y == 0.0 && normal.z > 0.0 && normal.z.is_finite()
}

/// What a load does to the entities it deserializes: it promotes the old payloads of the families in its mask. The mask is
/// the caller's, so concurrent loads with different masks share nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadPolicy {
    pub families: FamilySet,
}

impl LoadPolicy {
    pub fn new(families: FamilySet) -> Self {
        LoadPolicy { families }
    }
}

/// Apply a load policy to an entity just deserialized: [`promote_legacy_spline_leader`] when the policy has the entity's family,
/// nothing otherwise. An explicit revision is never overridden, and a raw unit is neither promoted nor demoted.
pub fn apply_load_policy(entity: &mut Entity, policy: &LoadPolicy) -> Promotion {
    match Family::of(&entity.kind) {
        Some(family) if policy.families.contains(family) => promote_legacy_spline_leader(entity),
        _ => Promotion::NotApplicable,
    }
}

/// Promote an old SPLINE or LEADER on a +Z plane to the canonical revision, or leave it as it is.
pub fn promote_legacy_spline_leader(entity: &mut Entity) -> Promotion {
    if entity.geometry_revision != GeometryRevision::LegacyProjectedV1 {
        return Promotion::NotApplicable;
    }
    if !matches!(entity.kind, EntityKind::Spline(_) | EntityKind::Leader(_)) {
        return Promotion::NotApplicable;
    }
    if !is_plus_z(entity.common.extrusion) {
        return Promotion::KeptLegacy;
    }
    match &mut entity.kind {
        EntityKind::Spline(s) => {
            if s.space.is_some() {
                return Promotion::KeptLegacy;
            }
            s.space = Some(SplineSpace { control: vec![0.0; s.control.len()], fit: vec![0.0; s.fit.len()], start_tangent: 0.0, end_tangent: 0.0 });
        }
        EntityKind::Leader(l) => {
            if l.frame.is_some() {
                return Promotion::KeptLegacy;
            }
            l.frame = Some(LeaderFrame {
                direction: Vec3::new(1.0, 0.0, 0.0),
                block_offset: Vec3::ZERO,
                annotation_offset: Vec3::ZERO,
                text_height: 0.0,
                text_width: 0.0,
            });
        }
        _ => return Promotion::NotApplicable,
    }
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    Promotion::Promoted
}
