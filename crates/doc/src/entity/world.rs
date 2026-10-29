//! Transient WCS geometry. Projection never becomes persisted drawing truth.

use cadcraft_geom::{Line as GLine, Mat4, Segment, Vec3};

use super::{Entity, EntityKind, GeometryRevision, Prim};
use crate::{DocError, Result, has_finite_numbers};

/// A spatial primitive after its complete parent transform.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldPrimitive {
    Line { a: Vec3, b: Vec3 },
    Point(Vec3),
}

/// Finite spatial geometry, kept separate from its view projection and the source payload.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometry {
    primitives: Vec<WorldPrimitive>,
}

impl WorldGeometry {
    pub fn primitives(&self) -> &[WorldPrimitive] {
        &self.primitives
    }

    /// Project the completed WCS result into the current XY view.
    pub fn project_xy(&self) -> Vec<Prim> {
        self.primitives
            .iter()
            .map(|primitive| match primitive {
                WorldPrimitive::Line { a, b } => Prim::Seg(Segment::Line(GLine::new(a.xy(), b.xy()))),
                WorldPrimitive::Point(point) => Prim::Point(point.xy()),
            })
            .collect()
    }
}

impl Entity {
    /// Resolve canonical geometry in WCS before any display, pick, or spatial-query projection.
    pub fn world_geometry(&self, parent: &Mat4) -> Result<WorldGeometry> {
        if self.geometry_revision == GeometryRevision::LegacyProjectedV1 {
            return Err(DocError::Invalid("The legacy geometry payload needs recovery before world adaptation. Reopen the original DXF/DWG.".into()));
        }
        if !has_finite_numbers(self) || !has_finite_numbers(parent) {
            return Err(DocError::NonFinite(self.handle));
        }
        if parent.m[3] != [0.0, 0.0, 0.0, 1.0] {
            return Err(DocError::Invalid("A world geometry parent must be an affine transform.".into()));
        }
        // Each family adds its complete field contract here, not a projected legacy fallback.
        let primitive = match &self.kind {
            EntityKind::Line(line) => WorldPrimitive::Line { a: parent.apply(line.a), b: parent.apply(line.b) },
            EntityKind::Point(point) => WorldPrimitive::Point(parent.apply(point.p)),
            EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::PolylineMesh(_)
            | EntityKind::Spline(_)
            | EntityKind::Helix(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::AttDef(_)
            | EntityKind::Insert(_)
            | EntityKind::Dimension(_)
            | EntityKind::Leader(_)
            | EntityKind::MLeader(_)
            | EntityKind::MLine(_)
            | EntityKind::Hatch(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::Region(_)
            | EntityKind::Viewport(_)
            | EntityKind::Image(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Unknown(_) => {
                return Err(DocError::Invalid(format!("The canonical {} geometry adapter is not available.", self.kind.type_name())));
            }
        };
        let finite = match &primitive {
            WorldPrimitive::Line { a, b } => a.is_finite() && b.is_finite(),
            WorldPrimitive::Point(point) => point.is_finite(),
        };
        if !finite {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok(WorldGeometry { primitives: vec![primitive] })
    }
}
