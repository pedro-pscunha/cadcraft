//! Canonical fill geometry: WIPEOUT and HATCH.
//!
//! A canonical wipeout keeps the image frame its record declares: the origin and the per-pixel U and V vectors in WCS,
//! the image size and the clip in pixel space. The outline is derived from them, `origin + u * (x + 0.5) + v * (0.5 - y)`
//! for each clip vertex. The frame is an affine frame of its plane, so every finite affine map that does not collapse
//! the plane (any rotation, scale, mirror or shear, in 3D) is exactly representable; a perspective map or a collapse
//! refuses atomically.

use cadcraft_geom::{Mat4, OcsFrame, Vec2, Vec3};

use super::fill::{Hatch, HatchEdge, HatchLoop, HatchSpline, Wipeout, WipeoutFrame};
use super::text_canonical::Similarity;
use crate::{DocError, Result};

fn invalid(what: &str) -> DocError {
    DocError::Invalid(format!("A canonical wipeout needs {what}."))
}

impl WipeoutFrame {
    fn check(&self) -> Result<()> {
        let finite =
            self.origin.is_finite() && self.u.is_finite() && self.v.is_finite() && self.size.is_finite() && self.clip.iter().all(|p| p.is_finite());
        if !finite {
            return Err(invalid("finite coordinates"));
        }
        let area = self.u.cross(self.v).len();
        if area <= 1e-12 * self.u.len() * self.v.len() || area <= 0.0 {
            return Err(invalid("independent U and V vectors"));
        }
        Ok(())
    }

    /// The outline in WCS: the polygon's vertices, or the four corners of a rectangular clip.
    pub fn world_boundary(&self) -> Result<Vec<Vec3>> {
        self.check()?;
        let place = |x: f64, y: f64| self.origin + self.u * (x + 0.5) + self.v * (0.5 - y);
        if self.clip_type == 1 && self.clip.len() == 2 {
            let (a, b) = (self.clip[0], self.clip[1]);
            return Ok(vec![place(a.x, a.y), place(b.x, a.y), place(b.x, b.y), place(a.x, b.y)]);
        }
        Ok(self.clip.iter().map(|p| place(p.x, p.y)).collect())
    }
}

impl Wipeout {
    /// The outline of a canonical wipeout in WCS; an error for a legacy one (its `boundary` is already XY).
    pub fn world_boundary(&self) -> Result<Vec<Vec3>> {
        self.frame.as_ref().ok_or_else(|| invalid("an image frame"))?.world_boundary()
    }

    /// This wipeout after any affine `transform` that keeps its plane a plane.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<Wipeout> {
        let frame = self.frame.as_ref().ok_or_else(|| invalid("an image frame"))?;
        frame.check()?;
        let m = &transform.m;
        if m[3] != [0.0, 0.0, 0.0, 1.0] || !m.iter().flatten().all(|v| v.is_finite()) {
            return Err(DocError::Invalid(
                "A wipeout can be moved, rotated, scaled, mirrored and sheared; this transform has another effect.".into(),
            ));
        }
        let moved =
            WipeoutFrame { origin: transform.apply(frame.origin), u: transform.apply_vec(frame.u), v: transform.apply_vec(frame.v), ..frame.clone() };
        moved.check()?;
        Ok(Wipeout { frame: Some(moved), ..self.clone() })
    }
}

/// A canonical hatch keeps its boundary edges exactly (`HatchLoop::edges`, in the OCS of its plane, 2D, with the
/// elevation in `Hatch::elevation`), and the pattern as the model keeps it: scale, angle, origin and the pattern lines
/// relative to them. A similarity (translation, rotation about any axis, uniform positive scale) is exact: the OCS of
/// the new plane is rebuilt, every edge is placed in it, arc angles turn by the rotation of the OCS X axis, lengths
/// grow with the scale, and the pattern scale, angle, origin and gradient angle follow. Anything else refuses.
impl Hatch {
    /// This hatch, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Hatch, Vec3)> {
        let invalid = |what: &str| DocError::Invalid(format!("A canonical hatch needs {what}."));
        if self.loops.is_empty() || self.loops.iter().any(|l| l.edges.is_empty() || !l.vertices.is_empty()) {
            return Err(invalid("exact boundary edges in every loop"));
        }
        let from = OcsFrame::new(normal).ok_or_else(|| invalid("a plane with a finite, non-zero normal"))?;
        let sim = Similarity::from_mat4(transform)?;
        let new_normal = sim.direction(normal);
        let to = OcsFrame::new(new_normal).ok_or_else(|| invalid("a plane with a finite, non-zero normal"))?;
        let k = sim.scale;
        let elevation = self.elevation;
        let place3 = |p: Vec2| to.to_ocs(sim.point(from.to_world(p.to3(elevation))));
        let place = |p: Vec2| place3(p).xy();
        let turn = |v: Vec2| to.to_ocs(sim.direction(from.to_world(v.to3(0.0)))).xy() * k;
        let delta = turn(Vec2::X).angle();
        let edge = |e: &HatchEdge| -> HatchEdge {
            match e {
                HatchEdge::Line { a, b } => HatchEdge::Line { a: place(*a), b: place(*b) },
                HatchEdge::Arc { center, radius, start, end, ccw } => {
                    HatchEdge::Arc { center: place(*center), radius: radius * k, start: start + delta, end: end + delta, ccw: *ccw }
                }
                HatchEdge::Ellipse { center, major, ratio, start, end, ccw } => {
                    HatchEdge::Ellipse { center: place(*center), major: turn(*major), ratio: *ratio, start: *start, end: *end, ccw: *ccw }
                }
                HatchEdge::Spline(sp) => HatchEdge::Spline(HatchSpline {
                    control: sp.control.iter().map(|p| place(*p)).collect(),
                    fit: sp.fit.iter().map(|p| place(*p)).collect(),
                    start_tangent: turn(sp.start_tangent),
                    end_tangent: turn(sp.end_tangent),
                    ..sp.clone()
                }),
                HatchEdge::Polyline { vertices, closed } => HatchEdge::Polyline {
                    vertices: vertices
                        .iter()
                        .map(|v| cadcraft_geom::PolyVertex { p: place(v.p), start_width: v.start_width * k, end_width: v.end_width * k, ..*v })
                        .collect(),
                    closed: *closed,
                },
            }
        };
        let mut moved = self.clone();
        moved.loops = self.loops.iter().map(|l| HatchLoop { edges: l.edges.iter().map(edge).collect(), ..l.clone() }).collect();
        moved.elevation = place3(Vec2::ZERO).z;
        moved.origin = place(self.origin);
        moved.scale *= k;
        moved.angle += delta;
        if let Some(g) = &mut moved.gradient {
            g.angle += delta;
        }
        let finite = moved.loops.iter().flat_map(|l| &l.edges).all(crate::has_finite_numbers);
        if !finite || !moved.elevation.is_finite() || !moved.origin.is_finite() {
            return Err(invalid("finite coordinates"));
        }
        Ok((moved, new_normal))
    }
}

#[cfg(test)]
mod tests {
    use cadcraft_geom::Vec2;

    use super::*;

    fn wipeout() -> Wipeout {
        Wipeout {
            boundary: Vec::new(),
            display: Default::default(),
            frame: Some(WipeoutFrame {
                origin: Vec3::new(10.0, 0.0, 0.0),
                u: Vec3::new(4.0, 0.0, 0.0),
                v: Vec3::new(0.0, 0.0, 2.0),
                size: Vec2::new(1.0, 1.0),
                clip_type: 2,
                clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
            }),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn the_outline_lies_in_the_plane_of_u_and_v() {
        let b = wipeout().world_boundary().unwrap();
        assert!(near(b[0], Vec3::new(10.0, 0.0, 2.0)) && near(b[2], Vec3::new(14.0, 0.0, 0.0)), "{b:?}");
        assert!(b.iter().all(|p| p.y == 0.0), "a vertical wipeout keeps its Z");
    }

    #[test]
    fn a_rectangular_clip_gives_four_corners() {
        let mut w = wipeout();
        let f = w.frame.as_mut().unwrap();
        (f.clip_type, f.clip) = (1, vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, 0.5)]);
        assert_eq!(w.world_boundary().unwrap().len(), 4);
    }

    #[test]
    fn a_mirror_or_a_shear_is_exact_and_a_collapse_or_perspective_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let moved = wipeout().transformed_canonical(&mirror).unwrap();
        let (a, b) = (wipeout().world_boundary().unwrap(), moved.world_boundary().unwrap());
        assert!(a.iter().zip(&b).all(|(p, q)| near(Vec3::new(-p.x, p.y, p.z), *q)));
        let flat = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(wipeout().transformed_canonical(&flat).is_err());
        let perspective = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.1, 0.0, 0.0, 1.0]] };
        assert!(wipeout().transformed_canonical(&perspective).is_err());
    }
    fn hatch() -> Hatch {
        let arc = HatchEdge::Arc { center: Vec2::new(1.0, 0.0), radius: 2.0, start: 0.0, end: std::f64::consts::FRAC_PI_2, ccw: true };
        let spline = HatchEdge::Spline(HatchSpline {
            degree: 3,
            rational: false,
            periodic: false,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(2.0, 2.0), Vec2::new(3.0, 0.0)],
            weights: Vec::new(),
            fit: Vec::new(),
            start_tangent: Vec2::ZERO,
            end_tangent: Vec2::ZERO,
        });
        let ellipse = HatchEdge::Ellipse { center: Vec2::new(5.0, 5.0), major: Vec2::new(2.0, 0.0), ratio: 0.5, start: 0.0, end: 3.0, ccw: true };
        let line = HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) };
        Hatch {
            pattern: "ANSI31".into(),
            solid: false,
            loops: vec![HatchLoop { flags: 1, edges: vec![line, arc, ellipse, spline], outer: true, ..HatchLoop::default() }],
            scale: 2.0,
            angle: 0.25,
            associative: false,
            style: 0,
            elevation: 3.0,
            gradient: None,
            origin: Vec2::new(1.0, 1.0),
            background: None,
            pattern_lines: Vec::new(),
        }
    }

    /// The WCS point of the OCS 2D point `p` of a hatch on `normal` at elevation `z`.
    fn world(normal: Vec3, z: f64, p: Vec2) -> Vec3 {
        OcsFrame::new(normal).unwrap().to_world(p.to3(z))
    }

    fn arc_end(e: &HatchEdge) -> (Vec2, f64, f64, f64) {
        match e {
            HatchEdge::Arc { center, radius, start, .. } => (*center, *radius, *start, 0.0),
            _ => panic!("not an arc"),
        }
    }

    #[test]
    fn a_rotation_about_a_horizontal_axis_places_every_edge_where_the_matrix_puts_it() {
        let (s, c) = 0.7f64.sin_cos();
        let rx = Mat4 { m: [[1.0, 0.0, 0.0, 4.0], [0.0, c, -s, -2.0], [0.0, s, c, 1.0], [0.0, 0.0, 0.0, 1.0]] };
        let normal = Vec3::new(0.0, 0.0, 2.0);
        let (moved, n) = hatch().transformed_canonical(normal, &rx).unwrap();
        // The normal turns and keeps its magnitude.
        assert!(near(n, Vec3::new(0.0, -2.0 * s, 2.0 * c)), "{n:?}");
        let old = hatch().loops[0].edges.clone();
        let new = moved.loops[0].edges.clone();
        let (z0, z1) = (hatch().elevation, moved.elevation);
        // The line's ends.
        let (HatchEdge::Line { a: a0, b: b0 }, HatchEdge::Line { a: a1, b: b1 }) = (&old[0], &new[0]) else { panic!("line") };
        assert!(near(rx.apply(world(normal, z0, *a0)), world(n, z1, *a1)) && near(rx.apply(world(normal, z0, *b0)), world(n, z1, *b1)));
        // The arc's start point, from its center, radius and start angle.
        let (c0, r0, s0, _) = arc_end(&old[1]);
        let (c1, r1, s1, _) = arc_end(&new[1]);
        let start0 = world(normal, z0, Vec2::polar(c0, r0, s0));
        let start1 = world(n, z1, Vec2::polar(c1, r1, s1));
        assert!(near(rx.apply(start0), start1), "{start0:?} {start1:?}");
        // The ellipse's major axis end, and a spline control point.
        let (HatchEdge::Ellipse { center: e0, major: m0, .. }, HatchEdge::Ellipse { center: e1, major: m1, .. }) = (&old[2], &new[2]) else {
            panic!("ellipse")
        };
        assert!(near(rx.apply(world(normal, z0, *e0 + *m0)), world(n, z1, *e1 + *m1)));
        let (HatchEdge::Spline(p0), HatchEdge::Spline(p1)) = (&old[3], &new[3]) else { panic!("spline") };
        assert!(near(rx.apply(world(normal, z0, p0.control[1])), world(n, z1, p1.control[1])));
        assert_eq!((p1.knots.clone(), p1.degree), (p0.knots.clone(), 3));
        // The pattern follows the OCS: its scale is unchanged by a rotation, its angle turns, its origin is placed.
        assert_eq!(moved.scale, 2.0);
        assert!(near(rx.apply(world(normal, z0, hatch().origin)), world(n, z1, moved.origin)));
    }

    #[test]
    fn a_scale_grows_the_lengths_and_the_pattern() {
        let k = Mat4 { m: [[3.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 3.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (moved, n) = hatch().transformed_canonical(Vec3::Z, &k).unwrap();
        assert!(near(n, Vec3::Z) && moved.elevation == 9.0 && moved.scale == 6.0 && moved.angle == 0.25);
        let HatchEdge::Arc { center, radius, start, end, .. } = &moved.loops[0].edges[1] else { panic!("arc") };
        assert_eq!((*center, *radius, *start, *end), (Vec2::new(3.0, 0.0), 6.0, 0.0, std::f64::consts::FRAC_PI_2));
        let HatchEdge::Ellipse { major, ratio, end, .. } = &moved.loops[0].edges[2] else { panic!("ellipse") };
        assert_eq!((*major, *ratio, *end), (Vec2::new(6.0, 0.0), 0.5, 3.0), "the parameters stay");
    }

    #[test]
    fn a_full_circle_keeps_its_sweep_after_a_quarter_turn() {
        let mut h = hatch();
        h.loops[0].edges = vec![HatchEdge::Arc { center: Vec2::ZERO, radius: 1.0, start: 0.0, end: std::f64::consts::TAU, ccw: true }];
        let (s, c) = std::f64::consts::FRAC_PI_2.sin_cos();
        let rz = Mat4 { m: [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (moved, _) = h.transformed_canonical(Vec3::Z, &rz).unwrap();
        let HatchEdge::Arc { start, end, .. } = &moved.loops[0].edges[0] else { panic!("arc") };
        assert!((end - start - std::f64::consts::TAU).abs() < 1e-12, "{start} {end}");
    }

    #[test]
    fn a_legacy_loop_a_mirror_or_a_bad_normal_refuses() {
        let mut legacy = hatch();
        legacy.loops[0].edges.clear();
        assert!(legacy.transformed_canonical(Vec3::Z, &Mat4::IDENTITY).is_err());
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(hatch().transformed_canonical(Vec3::Z, &mirror).is_err());
        assert!(hatch().transformed_canonical(Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }
}
