//! Transient WCS geometry. Projection never becomes persisted drawing truth.

use cadcraft_geom::{Ellipse as GEllipse, Line as GLine, Mat4, OcsFrame, Segment, TAU, Vec3};

use super::{Entity, EntityKind, GeometryRevision, Handle, Prim};
use crate::{DocError, Result, has_finite_numbers};

/// A spatial primitive after its complete parent transform.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldPrimitive {
    Line { a: Vec3, b: Vec3 },
    Point(Vec3),
    Conic { center: Vec3, axis_u: Vec3, axis_v: Vec3, start: f64, end: f64 },
}

impl WorldPrimitive {
    fn is_finite(&self) -> bool {
        match self {
            Self::Line { a, b } => a.is_finite() && b.is_finite(),
            Self::Point(point) => point.is_finite(),
            Self::Conic { center, axis_u, axis_v, start, end } => {
                center.is_finite() && axis_u.is_finite() && axis_v.is_finite() && start.is_finite() && end.is_finite()
            }
        }
    }

    fn project_xy(&self, owner: Handle) -> Result<Prim> {
        Ok(match self {
            Self::Line { a, b } => Prim::Seg(Segment::Line(GLine::new(a.xy(), b.xy()))),
            Self::Point(point) => Prim::Point(point.xy()),
            Self::Conic { center, axis_u, axis_v, start, end } => {
                if axis_u.xy() == cadcraft_geom::Vec2::ZERO && axis_v.xy() == cadcraft_geom::Vec2::ZERO {
                    return Ok(Prim::Point(center.xy()));
                }
                let ellipse = GEllipse::from_conjugate(center.xy(), axis_u.xy(), axis_v.xy(), *start, *end);
                if !has_finite_numbers(&ellipse) {
                    return Err(DocError::NonFinite(owner));
                }
                Prim::Ellipse(ellipse)
            }
        })
    }
}

/// Finite spatial geometry, kept separate from its view projection and the source payload.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometry {
    owner: Handle,
    primitives: Vec<WorldPrimitive>,
}

impl WorldGeometry {
    pub fn primitives(&self) -> &[WorldPrimitive] {
        &self.primitives
    }

    /// Project the completed WCS result into XY, or refuse an unrepresentable projection.
    pub fn project_xy(&self) -> Result<Vec<Prim>> {
        self.primitives.iter().map(|primitive| primitive.project_xy(self.owner)).collect()
    }
}

fn transformed_conic(parent: &Mat4, center: Vec3, axis_u: Vec3, axis_v: Vec3, start: f64, end: f64) -> WorldPrimitive {
    WorldPrimitive::Conic { center: parent.apply(center), axis_u: parent.apply_vec(axis_u), axis_v: parent.apply_vec(axis_v), start, end }
}

impl Entity {
    fn world_ocs_frame(&self) -> Result<OcsFrame> {
        OcsFrame::new(self.common.extrusion).ok_or_else(|| DocError::Invalid(format!("Entity {} has no valid source plane.", self.handle.hex())))
    }

    fn ocs_conic(&self, parent: &Mat4, center: Vec3, radius: f64, start: f64, end: f64) -> Result<WorldPrimitive> {
        let frame = self.world_ocs_frame()?;
        Ok(transformed_conic(
            parent,
            frame.to_world(center),
            frame.to_world(Vec3::new(radius, 0.0, 0.0)),
            frame.to_world(Vec3::new(0.0, radius, 0.0)),
            start,
            end,
        ))
    }

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
            EntityKind::Circle(circle) => self.ocs_conic(parent, circle.center, circle.radius, 0.0, TAU)?,
            EntityKind::Arc(arc) => self.ocs_conic(parent, arc.center, arc.radius, arc.start, arc.end)?,
            EntityKind::Ellipse(ellipse) => {
                let normal = self.world_ocs_frame()?.to_world(Vec3::Z);
                let major_direction = ellipse.major.try_normalized().ok_or_else(|| DocError::Invalid("An ellipse has no major axis.".into()))?;
                let minor_direction = normal
                    .cross(major_direction)
                    .try_normalized()
                    .ok_or_else(|| DocError::Invalid("An ellipse axis is parallel to its source normal.".into()))?;
                let scale = ellipse.major.x.abs().max(ellipse.major.y.abs()).max(ellipse.major.z.abs());
                let scaled_major = Vec3::new(ellipse.major.x / scale, ellipse.major.y / scale, ellipse.major.z / scale);
                let minor = minor_direction * (scaled_major.len() * ellipse.ratio) * scale;
                transformed_conic(parent, ellipse.center, ellipse.major, minor, ellipse.start, ellipse.end)
            }
            EntityKind::LwPolyline(_)
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
        if !primitive.is_finite() {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok(WorldGeometry { owner: self.handle, primitives: vec![primitive] })
    }
}
