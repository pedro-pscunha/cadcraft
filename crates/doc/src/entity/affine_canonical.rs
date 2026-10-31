//! Canonical ELLIPSE, LINE, POINT, 3D POLYLINE, polygon and polyface MESH and 3DFACE geometry.
//!
//! These keep WCS coordinates already, so a canonical edit is an affine map of their points, exact whenever the family allows it:
//!
//! - An ELLIPSE is the image of a circle: any invertible affine map gives an ellipse, so the new center, major axis, ratio,
//!   normal and parameter range are found from the images of two conjugate radii. The major axis is the longer principal
//!   axis, the normal is `major x minor` (keeping the source magnitude), and the parameters are shifted by the angle between
//!   the image radii and the principal axes; a full ellipse stays `0` to `TAU`. A map that collapses the plane refuses.
//! - A LINE moves with its endpoints. Its extrusion direction is a vector, so it follows the linear part of the map, and
//!   the thickness grows with the length of that image.
//! - A POINT moves; its angle (the direction of the OCS X axis turned by it) follows the map and is read in the OCS of the
//!   new extrusion. A map that takes that direction out of the new plane refuses.
//! - A RAY or XLINE moves with its base point and its direction follows the linear part (kept as long as it was). An IMAGE
//!   frame (insertion point, pixel vectors U and V) is an affine frame, so the clip and the size stay in pixels and the frame
//!   maps exactly; a map that makes U and V parallel refuses.
//! - The vertices of a 3D polyline, the position records of a mesh and the corners of a 3DFACE move; a polyface face
//!   record holds vertex numbers, not positions, and stays as it is.

use std::f64::consts::{FRAC_PI_2, TAU};

use cadcraft_geom::{Mat4, OcsFrame, Vec3, norm_angle};

use super::curves::{Ellipse, Face3d, Line, Point, Polyline3d, RayLine};
use super::layout::Image;
use super::mesh::PolylineMesh;
use crate::{DocError, Result};

fn invalid(what: &str) -> DocError {
    DocError::Invalid(what.into())
}

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| invalid("A canonical entity needs a plane with a finite, non-zero normal."))
}

/// `m` as a finite invertible affine map, or a refusal.
fn invertible(m: &Mat4) -> Result<()> {
    m.inverse().map(|_| ()).ok_or_else(|| invalid("This map collapses the geometry or is not affine."))
}

impl Ellipse {
    /// This ellipse, in the plane of `normal`, after the invertible affine map `m`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, m: &Mat4) -> Result<(Ellipse, Vec3)> {
        invertible(m)?;
        let unit = frame(normal)?.to_world(Vec3::Z);
        let valid = self.center.is_finite()
            && self.major.is_finite()
            && self.ratio.is_finite()
            && self.ratio > 0.0
            && self.ratio <= 1.0
            && self.start.is_finite()
            && self.end.is_finite();
        let length = self.major.len();
        if !valid || length <= 0.0 || self.major.dot(unit).abs() > 1e-9 * length {
            return Err(invalid("A canonical ellipse needs a major axis in its plane and a ratio in (0, 1]."));
        }
        // AutoCAD keeps the parameter range and ratio under a reflected similarity.
        if let Ok(similarity) = super::text_canonical::Similarity::including_reflection(m)
            && similarity.reflected()
        {
            let normal = -similarity.direction(normal);
            let moved = Ellipse { center: similarity.point(self.center), major: m.apply_vec(self.major), normal, ..self.clone() };
            if !crate::has_finite_numbers(&moved) {
                return Err(invalid("An ellipse cannot be placed with finite coordinates."));
            }
            return Ok((moved, normal));
        }
        // Two conjugate radii (here perpendicular): the major axis and the minor axis, `normal x major` scaled by the ratio.
        let minor = unit.cross(self.major) * self.ratio;
        let (u, v) = (m.apply_vec(self.major), m.apply_vec(minor));
        let t0 = 0.5 * (2.0 * u.dot(v)).atan2(u.dot(u) - v.dot(v));
        let (s, c) = t0.sin_cos();
        let (mut major, mut minor, mut shift) = (u * c + v * s, v * c - u * s, t0);
        if minor.len() > major.len() {
            (major, minor, shift) = (minor, -major, t0 + FRAC_PI_2);
        }
        let ratio = minor.len() / major.len();
        let new_unit = major.cross(minor).try_normalized().ok_or_else(|| invalid("The image of this ellipse has no plane."))?;
        let full = self.end - self.start >= TAU - 1e-12;
        let (start, end) = if full { (self.start, self.end) } else { (self.start - shift, self.end - shift) };
        let moved = Ellipse { center: m.apply(self.center), major, normal: new_unit * normal.len(), ratio, start, end };
        if !(crate::has_finite_numbers(&moved) && ratio > 0.0 && ratio <= 1.0 + 1e-12) {
            return Err(invalid("An ellipse cannot be placed with finite coordinates."));
        }
        let n = moved.normal;
        Ok((moved, n))
    }
}

/// The image of the extrusion direction of a line or a point under the linear part of `m`: the new extrusion (with the source
/// magnitude) and how much a thickness along it grows.
fn extrusion_image(normal: Vec3, m: &Mat4) -> Result<(Vec3, f64)> {
    let unit = frame(normal)?.to_world(Vec3::Z);
    let image = m.apply_vec(unit);
    let factor = image.len();
    if !(factor.is_finite() && factor > 0.0) {
        return Err(invalid("This map collapses the extrusion direction."));
    }
    Ok((image * (normal.len() / factor), factor))
}

impl Line {
    /// This line after the invertible affine map `m`; the new extrusion and the factor the thickness grows by.
    pub fn transformed_canonical(&self, normal: Vec3, m: &Mat4) -> Result<(Line, Vec3, f64)> {
        invertible(m)?;
        let (extrusion, factor) = extrusion_image(normal, m)?;
        Ok((Line { a: m.apply(self.a), b: m.apply(self.b) }, extrusion, factor))
    }
}

impl Point {
    /// This point after the invertible affine map `m`; the new extrusion and the factor the thickness grows by. The angle
    /// is the direction of the OCS X axis turned by it, mapped and read in the OCS of the new extrusion.
    pub fn transformed_canonical(&self, normal: Vec3, m: &Mat4) -> Result<(Point, Vec3, f64)> {
        invertible(m)?;
        let from = frame(normal)?;
        let (extrusion, factor) = extrusion_image(normal, m)?;
        let to = frame(extrusion)?;
        let direction = m.apply_vec(from.to_world(Vec3::new(self.angle.cos(), self.angle.sin(), 0.0)));
        let local = to.to_ocs(direction);
        if !local.is_finite() || local.z.abs() > 1e-9 * local.len() || local.xy().len() <= 0.0 {
            return Err(invalid("This map takes the direction of the point out of its plane."));
        }
        let angle = norm_angle(local.y.atan2(local.x));
        // An angle of exactly 0 stays 0 when nothing turned it.
        Ok((Point { p: m.apply(self.p), angle: if (angle - self.angle).abs() < 1e-14 { self.angle } else { angle } }, extrusion, factor))
    }
}

impl RayLine {
    /// This ray or construction line after the invertible affine map `m`.
    pub fn transformed_canonical(&self, m: &Mat4) -> Result<RayLine> {
        invertible(m)?;
        let source = self.dir.len();
        let image = m.apply_vec(self.dir);
        if !(source.is_finite() && source > 0.0 && image.len() > 0.0) {
            return Err(invalid("A ray needs a direction that the map keeps."));
        }
        Ok(RayLine { base: m.apply(self.base), dir: image * (source / image.len()) })
    }
}

impl Image {
    /// This image after the invertible affine map `m`: the insertion point and the pixel vectors map, the clip and the size
    /// stay in pixels.
    pub fn transformed_canonical(&self, m: &Mat4) -> Result<Image> {
        invertible(m)?;
        let moved = Image { insert: m.apply(self.insert), u: m.apply_vec(self.u), v: m.apply_vec(self.v), ..self.clone() };
        let area = moved.u.cross(moved.v).len();
        if !crate::has_finite_numbers(&moved) || area <= 1e-12 * moved.u.len() * moved.v.len() || area <= 0.0 {
            return Err(invalid("The pixel vectors of an image must span a plane after the map."));
        }
        Ok(moved)
    }
}

impl Polyline3d {
    /// This 3D polyline after the invertible affine map `m`.
    pub fn transformed_canonical(&self, m: &Mat4) -> Result<Polyline3d> {
        invertible(m)?;
        let mut moved = self.clone();
        moved.map_points(|p| *p = m.apply(*p));
        Ok(moved)
    }
}

impl PolylineMesh {
    /// This mesh after the invertible affine map `m`: every vertex position; the face records of a polyface stay.
    pub fn transformed_canonical(&self, m: &Mat4) -> Result<PolylineMesh> {
        invertible(m)?;
        let mut moved = self.clone();
        for v in moved.vertices.iter_mut().filter(|v| !v.is_face()) {
            v.vertex.position = m.apply(v.vertex.position);
        }
        Ok(moved)
    }
}

impl Face3d {
    /// This 3D face after the invertible affine map `m`.
    pub fn transformed_canonical(&self, m: &Mat4) -> Result<Face3d> {
        invertible(m)?;
        Ok(Face3d { corners: self.corners.map(|c| m.apply(c)), ..self.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn rotation_x(angle: f64, shift: Vec3) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4 { m: [[1.0, 0.0, 0.0, shift.x], [0.0, c, -s, shift.y], [0.0, s, c, shift.z], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn affine() -> Mat4 {
        Mat4 { m: [[2.0, 0.5, 0.0, 1.0], [0.0, 1.0, 0.25, -2.0], [0.3, 0.0, 3.0, 4.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn ellipse() -> Ellipse {
        Ellipse {
            center: Vec3::new(1.0, 2.0, 3.0),
            major: Vec3::new(4.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 0.0, 2.0),
            ratio: 0.5,
            start: 0.3,
            end: 2.0,
        }
    }

    /// The point of the ellipse at the parameter `t`, from its normal and major axis.
    fn at(e: &Ellipse, t: f64) -> Vec3 {
        let minor = e.normal.normalized().cross(e.major) * e.ratio;
        e.center + e.major * t.cos() + minor * t.sin()
    }

    #[test]
    fn an_ellipse_stays_the_image_of_itself_under_any_invertible_affine_map() {
        for m in [rotation_x(0.8, Vec3::new(1.0, 2.0, 3.0)), affine()] {
            let (moved, n) = ellipse().transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &m).unwrap();
            assert!(moved.ratio > 0.0 && moved.ratio <= 1.0 && near(n, moved.normal));
            assert!((moved.normal.len() - 2.0).abs() < 1e-9, "the normal keeps its magnitude");
            assert!(moved.major.dot(moved.normal).abs() < 1e-9, "the major axis lies in the new plane");
            // The same arc: the old parameter t is the new one t - shift; compare through the end points and the middle.
            let old = ellipse();
            let mid = |e: &Ellipse| at(e, 0.5 * (e.start + e.end));
            assert!(near(at(&moved, moved.start), m.apply(at(&old, old.start))), "start");
            assert!(near(at(&moved, moved.end), m.apply(at(&old, old.end))), "end");
            assert!(near(mid(&moved), m.apply(mid(&old))), "middle");
        }
    }

    #[test]
    fn a_full_ellipse_stays_full_and_a_longer_image_swaps_the_axes() {
        let full = Ellipse { start: 0.0, end: TAU, ..ellipse() };
        let (moved, _) = full.transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &affine()).unwrap();
        assert_eq!((moved.start, moved.end), (0.0, TAU));
        // Stretch the minor direction (y) so that the image of the minor axis is the longer one.
        let stretch = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 10.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (moved, _) = ellipse().transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &stretch).unwrap();
        assert!(
            moved.ratio <= 1.0 && (moved.major.len() - 2.0 * 10.0).abs() < 1e-9,
            "the major axis is the image of the old minor one: {:?}",
            moved.major
        );
        assert!(near(at(&moved, moved.start), stretch.apply(at(&ellipse(), 0.3))));
    }

    #[test]
    fn an_ellipse_refuses_a_collapse_and_a_bad_payload() {
        let flat = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(ellipse().transformed_canonical(Vec3::Z, &flat).is_err());
        assert!(ellipse().transformed_canonical(Vec3::ZERO, &Mat4::IDENTITY).is_err());
        let tilted_major = Ellipse { major: Vec3::new(4.0, 0.0, 1.0), ..ellipse() };
        assert!(tilted_major.transformed_canonical(Vec3::Z, &Mat4::IDENTITY).is_err(), "the major axis must lie in the plane");
        assert!(Ellipse { ratio: 0.0, ..ellipse() }.transformed_canonical(Vec3::Z, &Mat4::IDENTITY).is_err());
    }

    #[test]
    fn a_line_moves_with_its_endpoints_and_its_extrusion_follows_the_linear_part() {
        let line = Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(4.0, 1.0, 2.0) };
        let m = rotation_x(0.8, Vec3::new(1.0, 2.0, 3.0));
        let (moved, extrusion, factor) = line.transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &m).unwrap();
        assert!(near(moved.a, m.apply(line.a)) && near(moved.b, m.apply(line.b)));
        assert!(near(extrusion, m.apply_vec(Vec3::new(0.0, 0.0, 2.0))) && (factor - 1.0).abs() < 1e-12);
        // A stretch grows the thickness by the length of the image of the extrusion direction.
        let (_, _, factor) = line.transformed_canonical(Vec3::Z, &affine()).unwrap();
        assert!((factor - affine().apply_vec(Vec3::Z).len()).abs() < 1e-12);
        assert!(
            line.transformed_canonical(
                Vec3::Z,
                &Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] }
            )
            .is_err()
        );
    }

    #[test]
    fn a_point_turns_its_angle_with_the_map_and_refuses_one_that_leaves_the_plane() {
        let point = Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.5 };
        let m = rotation_x(0.8, Vec3::ZERO);
        let normal = Vec3::new(0.0, 0.0, 1.0);
        let (moved, extrusion, _) = point.transformed_canonical(normal, &m).unwrap();
        let (old, new) = (OcsFrame::new(normal).unwrap(), OcsFrame::new(extrusion).unwrap());
        let direction = |f: &OcsFrame, a: f64| f.to_world(Vec3::new(a.cos(), a.sin(), 0.0));
        assert!(near(direction(&new, moved.angle), m.apply_vec(direction(&old, 0.5))), "the symbol direction in the world is the mapped one");
        assert!(near(moved.p, m.apply(point.p)));
        // A mirror is exact as well; a shear that pushes the direction out of the plane is not.
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(point.transformed_canonical(normal, &mirror).is_ok());
        let lift = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [1.0, 1.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(point.transformed_canonical(normal, &lift).is_err());
    }

    #[test]
    fn a_polyline_a_mesh_and_a_face_move_their_positions_only() {
        let m = rotation_x(0.8, Vec3::new(1.0, 2.0, 3.0));
        let poly =
            Polyline3d { records: Default::default(), points: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 4.0)], closed: false, fit: None };
        let moved = poly.transformed_canonical(&m).unwrap();
        assert!(near(moved.points[1], m.apply(poly.points[1])) && !moved.closed);
        let face = Face3d { corners: [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.0, 1.0, 0.0)], hidden_edges: 3 };
        let moved = face.transformed_canonical(&affine()).unwrap();
        assert!(near(moved.corners[2], affine().apply(face.corners[2])) && moved.hidden_edges == 3);
        assert!(face.transformed_canonical(&Mat4 { m: [[0.0; 4], [0.0; 4], [0.0; 4], [0.0, 0.0, 0.0, 1.0]] }).is_err());
    }

    #[test]
    fn a_ray_keeps_the_length_of_its_direction_and_an_image_maps_its_frame() {
        let ray = RayLine { base: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.0, 1.0) };
        let moved = ray.transformed_canonical(&affine()).unwrap();
        assert!(near(moved.base, affine().apply(ray.base)) && (moved.dir.len() - 1.0).abs() < 1e-12);
        assert!(near(moved.dir.normalized(), affine().apply_vec(ray.dir).normalized()));
        assert!(RayLine { dir: Vec3::ZERO, ..ray }.transformed_canonical(&Mat4::IDENTITY).is_err());
        let image = Image {
            insert: Vec3::new(1.0, 2.0, 3.0),
            u: Vec3::new(0.5, 0.0, 0.0),
            v: Vec3::new(0.0, 0.5, 0.25),
            size: cadcraft_geom::Vec2::new(100.0, 50.0),
            clip: vec![cadcraft_geom::Vec2::new(0.0, 0.0), cadcraft_geom::Vec2::new(10.0, 10.0)],
            clipping: true,
            ..Image::default()
        };
        let m = rotation_x(0.8, Vec3::new(1.0, 2.0, 3.0));
        let moved = image.transformed_canonical(&m).unwrap();
        assert!(near(moved.insert, m.apply(image.insert)) && near(moved.u, m.apply_vec(image.u)) && near(moved.v, m.apply_vec(image.v)));
        assert_eq!((moved.size, moved.clip.clone(), moved.clipping), (image.size, image.clip.clone(), true));
        let fold = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(image.transformed_canonical(&fold).is_err());
    }
}
