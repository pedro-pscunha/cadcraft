//! Canonical CIRCLE, ARC and 2D polyline geometry.
//!
//! A canonical circle, arc or 2D polyline keeps the coordinates its record declares: the circle and arc centers are
//! OCS points (their Z is the elevation), the arc angles are OCS angles counter-clockwise from the OCS X axis, and a
//! polyline's vertices are OCS 2D points with the elevation beside them. The plane is the normal of the entity
//! (`Common.extrusion`, kept with its source magnitude), and the signed thickness is `Common.thickness`. Nothing is
//! mirrored or projected on import.
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact: the OCS of the rotated plane is
//! rebuilt and every point is placed in it, the arc angles turn with the OCS X axis (a full circle keeps its sweep), and
//! lengths grow with the scale. The caller grows `Common.thickness` by `Similarity::scale` and stores the returned
//! normal. A non-uniform scale would make a circle an ellipse and a reflection reverses an arc, so they refuse.

use cadcraft_geom::{Mat4, OcsFrame, PolyVertex, Vec2, Vec3};

use super::curves::{Arc, Circle, LwPolyline, PolylineFit};
use super::text_canonical::Similarity;
use crate::{DocError, Result};

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical curve needs a plane with a finite, non-zero normal.".into()))
}

/// How a similarity moves the OCS of a plane: both frames, the map of OCS points and the turn of the OCS X axis.
struct Plane {
    from: OcsFrame,
    to: OcsFrame,
    sim: Similarity,
    normal: Vec3,
}

impl Plane {
    fn new(normal: Vec3, transform: &Mat4) -> Result<Plane> {
        let sim = Similarity::from_mat4(transform)?;
        let new_normal = sim.direction(normal);
        Ok(Plane { from: frame(normal)?, to: frame(new_normal)?, sim, normal: new_normal })
    }

    fn point(&self, p: Vec3) -> Vec3 {
        self.to.to_ocs(self.sim.point(self.from.to_world(p)))
    }

    fn point2(&self, p: Vec2, elevation: f64) -> Vec2 {
        self.point(p.to3(elevation)).xy()
    }

    /// The turn of the OCS X axis, in radians.
    fn turn(&self) -> f64 {
        self.to.to_ocs(self.sim.direction(self.from.to_world(Vec3::new(1.0, 0.0, 0.0)))).xy().angle()
    }
}

impl Circle {
    /// The center in WCS, from the normal of the plane.
    pub fn world_center(&self, normal: Vec3) -> Result<Vec3> {
        Ok(frame(normal)?.to_world(self.center))
    }

    /// This circle, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Circle, Vec3)> {
        let plane = Plane::new(normal, transform)?;
        Ok((Circle { center: plane.point(self.center), radius: self.radius * plane.sim.scale }, plane.normal))
    }
}

impl Arc {
    /// The center in WCS, from the normal of the plane.
    pub fn world_center(&self, normal: Vec3) -> Result<Vec3> {
        Ok(frame(normal)?.to_world(self.center))
    }

    /// The start and end points in WCS.
    pub fn world_endpoints(&self, normal: Vec3) -> Result<(Vec3, Vec3)> {
        let f = frame(normal)?;
        let at = |angle: f64| f.to_world(self.center + Vec3::new(angle.cos(), angle.sin(), 0.0) * self.radius);
        Ok((at(self.start), at(self.end)))
    }

    /// This arc, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Arc, Vec3)> {
        let plane = Plane::new(normal, transform)?;
        let turn = plane.turn();
        let moved = Arc { center: plane.point(self.center), radius: self.radius * plane.sim.scale, start: self.start + turn, end: self.end + turn };
        Ok((moved, plane.normal))
    }
}

impl LwPolyline {
    /// The vertices in WCS, from the normal of the plane.
    pub fn world_vertices(&self, normal: Vec3) -> Result<Vec<Vec3>> {
        let f = frame(normal)?;
        Ok(self.vertices.iter().map(|v| f.to_world(v.p.to3(self.elevation))).collect())
    }

    /// This polyline, in the plane of `normal`, after the similarity `transform`; and the new normal. The vertices, the
    /// widths, the elevation, the fit data (with the plane it keeps) and its display copy move together.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(LwPolyline, Vec3)> {
        let plane = Plane::new(normal, transform)?;
        let (k, turn) = (plane.sim.scale, plane.turn());
        let place =
            |v: &PolyVertex| PolyVertex { p: plane.point2(v.p, self.elevation), start_width: v.start_width * k, end_width: v.end_width * k, ..*v };
        let mut moved = self.clone();
        moved.vertices = self.vertices.iter().map(place).collect();
        moved.const_width *= k;
        moved.elevation = plane.point(Vec3::new(0.0, 0.0, self.elevation)).z;
        if let Some(fit) = &self.fit {
            moved.fit = Some(PolylineFit {
                start_width: fit.start_width * k,
                end_width: fit.end_width * k,
                elevation: moved.elevation,
                extrusion: plane.normal,
                vertices: fit
                    .vertices
                    .iter()
                    .map(|v| {
                        let p = plane.point(Vec3::new(v.position.x, v.position.y, fit.elevation + v.position.z));
                        let position = Vec3::new(p.x, p.y, p.z - moved.elevation);
                        let tangent = if v.flags & 2 != 0 { v.tangent + turn } else { v.tangent };
                        super::curves::PolylineFitVertex {
                            position,
                            start_width: v.start_width * k,
                            end_width: v.end_width * k,
                            tangent,
                            ..v.clone()
                        }
                    })
                    .collect(),
                display_vertices: moved.vertices.clone(),
                ..fit.clone()
            });
        }
        if !crate::has_finite_numbers(&moved) {
            return Err(DocError::Invalid("A canonical curve cannot be placed with finite coordinates.".into()));
        }
        Ok((moved, plane.normal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn rotation_x(angle: f64) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4 { m: [[1.0, 0.0, 0.0, 4.0], [0.0, c, -s, -2.0], [0.0, s, c, 1.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn scale(k: f64) -> Mat4 {
        Mat4 { m: [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    #[test]
    fn a_circle_keeps_its_center_where_the_matrix_puts_it() {
        let rx = rotation_x(0.7);
        let normal = Vec3::new(0.0, 0.0, 2.0);
        let circle = Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 };
        let (moved, n) = circle.transformed_canonical(normal, &rx).unwrap();
        assert!(near(rx.apply(circle.world_center(normal).unwrap()), moved.world_center(n).unwrap()));
        assert_eq!(moved.radius, 2.0);
        assert!(near(n, Vec3::new(0.0, -2.0 * 0.7f64.sin(), 2.0 * 0.7f64.cos())));
    }

    #[test]
    fn an_arc_keeps_its_end_points_and_a_full_circle_its_sweep() {
        let rx = rotation_x(-1.1);
        let normal = Vec3::new(1.0, 1.0, 1.0);
        let arc = Arc { center: Vec3::new(1.0, 0.5, 0.25), radius: 2.0, start: 0.3, end: 2.0 };
        let (moved, n) = arc.transformed_canonical(normal, &rx).unwrap();
        let (a, b) = arc.world_endpoints(normal).unwrap();
        let (c, d) = moved.world_endpoints(n).unwrap();
        assert!(near(rx.apply(a), c) && near(rx.apply(b), d), "{a:?} {c:?}");
        let full = Arc { start: 0.0, end: std::f64::consts::TAU, ..arc };
        let (moved, _) = full.transformed_canonical(normal, &rx).unwrap();
        assert!((moved.end - moved.start - std::f64::consts::TAU).abs() < 1e-12);
    }

    #[test]
    fn a_scale_grows_the_radius_and_a_mirror_or_shear_refuses() {
        let arc = Arc { center: Vec3::new(1.0, 0.0, 2.0), radius: 2.0, start: 0.0, end: 1.0 };
        let (moved, _) = arc.transformed_canonical(Vec3::Z, &scale(3.0)).unwrap();
        assert_eq!((moved.center, moved.radius, moved.start, moved.end), (Vec3::new(3.0, 0.0, 6.0), 6.0, 0.0, 1.0));
        let squash = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(Circle { center: Vec3::ZERO, radius: 1.0 }.transformed_canonical(Vec3::Z, &squash).is_err());
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(arc.transformed_canonical(Vec3::Z, &mirror).is_err());
        assert!(arc.transformed_canonical(Vec3::ZERO, &scale(1.0)).is_err());
    }

    fn polyline() -> LwPolyline {
        LwPolyline {
            records: Default::default(),
            vertices: vec![PolyVertex::with_bulge(Vec2::new(0.0, 0.0), 0.5), PolyVertex::new(Vec2::new(3.0, 1.0))],
            closed: false,
            const_width: 0.5,
            elevation: 2.0,
            plinegen: false,
            heavy: false,
            fit: None,
        }
    }

    #[test]
    fn a_polyline_keeps_every_vertex_where_the_matrix_puts_it() {
        let rx = rotation_x(0.9);
        let normal = Vec3::new(0.0, 1.0, 1.0);
        let pl = polyline();
        let (moved, n) = pl.transformed_canonical(normal, &rx).unwrap();
        let (old, new) = (pl.world_vertices(normal).unwrap(), moved.world_vertices(n).unwrap());
        assert!(old.iter().zip(&new).all(|(a, b)| near(rx.apply(*a), *b)));
        assert_eq!((moved.vertices[0].bulge, moved.const_width), (0.5, 0.5), "a rotation keeps bulges and widths");
        let (big, _) = pl.transformed_canonical(Vec3::Z, &scale(2.0)).unwrap();
        assert_eq!((big.const_width, big.elevation, big.vertices[1].p), (1.0, 4.0, Vec2::new(6.0, 2.0)));
    }

    #[test]
    fn a_fitted_polyline_moves_its_fit_data_with_it() {
        let mut pl = polyline();
        pl.heavy = true;
        pl.const_width = 0.0;
        pl.fit = Some(PolylineFit {
            flags: 4,
            smooth_type: 5,
            m_count: 0,
            n_count: 0,
            m_density: 0,
            n_density: 0,
            start_width: 0.5,
            end_width: 0.5,
            elevation: 2.0,
            extrusion: Vec3::Z,
            vertices: vec![super::super::curves::PolylineFitVertex {
                record: Default::default(),
                position: Vec3::new(1.0, 1.0, 0.0),
                flags: 2,
                start_width: 0.5,
                end_width: 0.5,
                bulge: 0.0,
                tangent: 0.25,
            }],
            display_vertices: pl.vertices.clone(),
        });
        let (moved, n) = pl.transformed_canonical(Vec3::Z, &scale(2.0)).unwrap();
        let fit = moved.fit.as_ref().unwrap();
        assert_eq!((fit.extrusion, fit.elevation, fit.vertices[0].position, fit.vertices[0].tangent), (n, 4.0, Vec3::new(2.0, 2.0, 0.0), 0.25));
        assert_eq!(fit.display_vertices, moved.vertices);
        assert!(moved.saved_fit().is_some(), "the fit data still describes the polyline");
    }
}
