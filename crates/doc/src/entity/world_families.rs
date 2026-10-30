//! World-geometry builders for the canonical families whose geometry is straight: the WCS segments of a wipeout
//! outline, of a solid or trace outline and of a polyline without arcs, after the complete parent affine.
//!
//! A builder takes the canonical payload, the normal of its plane and the parent transform, and returns the primitives of
//! `WorldGeometry`'s vocabulary. It refuses (an error, no fallback) a zero or non-finite normal, a payload that is not
//! canonical, and a shape that vocabulary cannot hold exactly: a polyline arc needs an arc primitive. Projection to the
//! view happens only after this, in `WorldGeometry::project_xy`.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::curves::{LwPolyline, Solid};
use super::fill::Wipeout;
use super::world::WorldPrimitive;
use crate::{DocError, Result};

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical curve needs a plane with a finite, non-zero normal.".into()))
}

fn affine(parent: &Mat4) -> Result<()> {
    if parent.m[3] != [0.0, 0.0, 0.0, 1.0] || !parent.m.iter().flatten().all(|v| v.is_finite()) {
        return Err(DocError::Invalid("A world geometry parent must be a finite affine transform.".into()));
    }
    Ok(())
}

/// The edges of a closed loop of points.
fn closed(points: &[Vec3]) -> Vec<WorldPrimitive> {
    points.iter().enumerate().map(|(i, a)| WorldPrimitive::Line { a: *a, b: points[(i + 1) % points.len()] }).collect()
}

/// The outline of a canonical wipeout: its clip polygon (or the rectangle of a rectangular clip), closed.
pub fn wipeout_primitives(wipeout: &Wipeout, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let mut points = wipeout.world_boundary()?;
    // A polygonal clip repeats its first vertex at the end.
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let points: Vec<Vec3> = points.into_iter().map(|p| parent.apply(p)).collect();
    if points.len() < 2 {
        return Err(DocError::Invalid("A wipeout outline needs at least two vertices.".into()));
    }
    Ok(closed(&points))
}

/// The outline of a solid or trace in the DXF drawing order of its corners (1, 2, 4, 3); a triangle (the fourth corner
/// repeats the third) has three edges.
pub fn solid_primitives(solid: &Solid, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let [c0, c1, c2, c3] = solid.world_corners(normal)?.map(|c| parent.apply(c));
    Ok(if c3 == c2 { closed(&[c0, c1, c2]) } else { closed(&[c0, c1, c3, c2]) })
}

/// The segments of a canonical 2D polyline without arcs (all bulges zero), closed when the polyline is. A polyline with
/// an arc segment needs an arc primitive and is refused.
pub fn lwpolyline_primitives(polyline: &LwPolyline, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    frame(normal)?;
    let n = polyline.vertices.len();
    let segments = if polyline.closed { n } else { n.saturating_sub(1) };
    if polyline.vertices.iter().take(segments).any(|v| v.bulge != 0.0) {
        return Err(DocError::Invalid("A polyline arc segment needs an arc primitive in the world geometry.".into()));
    }
    let points: Vec<Vec3> = polyline.world_vertices(normal)?.into_iter().map(|p| parent.apply(p)).collect();
    Ok((0..segments).map(|i| WorldPrimitive::Line { a: points[i], b: points[(i + 1) % n] }).collect())
}

#[cfg(test)]
mod tests {
    use cadcraft_geom::{PolyVertex, Vec2};

    use super::super::fill::WipeoutFrame;
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn lines(p: &[WorldPrimitive]) -> Vec<(Vec3, Vec3)> {
        p.iter()
            .filter_map(|p| match p {
                WorldPrimitive::Line { a, b } => Some((*a, *b)),
                WorldPrimitive::Point(_) => None,
            })
            .collect()
    }

    /// A parent that moves and turns about Z by a quarter turn: (x, y, z) -> (-y + 10, x - 5, z + 1).
    fn parent() -> Mat4 {
        Mat4 { m: [[0.0, -1.0, 0.0, 10.0], [1.0, 0.0, 0.0, -5.0], [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    /// The arbitrary axis algorithm written out by hand for the three planes: +Z, -Z and (1, 0, 0) (tilted).
    fn expected_world(plane: usize, p: Vec3) -> Vec3 {
        match plane {
            0 => p,
            1 => Vec3::new(-p.x, p.y, -p.z),
            _ => Vec3::new(p.z, p.x, p.y),
        }
    }

    const NORMALS: [Vec3; 3] = [Vec3 { x: 0.0, y: 0.0, z: 1.0 }, Vec3 { x: 0.0, y: 0.0, z: -2.0 }, Vec3 { x: 3.0, y: 0.0, z: 0.0 }];

    #[test]
    fn a_solid_outline_follows_the_dxf_corner_order_in_every_plane() {
        let corners = [Vec3::new(0.0, 0.0, 2.0), Vec3::new(4.0, 0.0, 2.0), Vec3::new(0.0, 3.0, 2.0), Vec3::new(4.0, 3.0, 2.0)];
        for (plane, normal) in NORMALS.iter().enumerate() {
            let got = lines(&solid_primitives(&Solid { corners }, *normal, &parent()).unwrap());
            let want: Vec<Vec3> = [0, 1, 3, 2].iter().map(|i| parent().apply(expected_world(plane, corners[*i]))).collect();
            assert_eq!(got.len(), 4);
            for (i, (a, b)) in got.iter().enumerate() {
                assert!(near(*a, want[i]) && near(*b, want[(i + 1) % 4]), "plane {plane} edge {i}: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_triangle_has_three_edges() {
        let t = Solid { corners: [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)] };
        assert_eq!(solid_primitives(&t, Vec3::Z, &Mat4::IDENTITY).unwrap().len(), 3);
    }

    fn polyline(closed: bool, bulge: f64) -> LwPolyline {
        LwPolyline {
            records: Default::default(),
            vertices: vec![
                PolyVertex::with_bulge(Vec2::new(0.0, 0.0), bulge),
                PolyVertex::new(Vec2::new(3.0, 0.0)),
                PolyVertex::new(Vec2::new(3.0, 2.0)),
            ],
            closed,
            const_width: 0.0,
            elevation: 1.5,
            plinegen: false,
            heavy: false,
            fit: None,
        }
    }

    #[test]
    fn a_polyline_has_its_segments_in_every_plane_with_the_elevation() {
        for (plane, normal) in NORMALS.iter().enumerate() {
            for (closed, count) in [(false, 2), (true, 3)] {
                let got = lines(&lwpolyline_primitives(&polyline(closed, 0.0), *normal, &parent()).unwrap());
                assert_eq!(got.len(), count);
                let pts = [Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, 2.0)];
                let at = |i: usize| parent().apply(expected_world(plane, pts[i % 3].to3(1.5)));
                for (i, (a, b)) in got.iter().enumerate() {
                    assert!(near(*a, at(i)) && near(*b, at(i + 1)), "plane {plane} closed {closed} segment {i}");
                }
            }
        }
    }

    #[test]
    fn a_polyline_arc_a_bad_normal_or_a_perspective_parent_refuses() {
        assert!(lwpolyline_primitives(&polyline(false, 0.5), Vec3::Z, &Mat4::IDENTITY).is_err());
        // The bulge of the closing segment counts only when it is drawn.
        assert!(lwpolyline_primitives(&polyline(false, 0.0), Vec3::ZERO, &Mat4::IDENTITY).is_err());
        let mut perspective = Mat4::IDENTITY;
        perspective.m[3][0] = 0.1;
        assert!(lwpolyline_primitives(&polyline(false, 0.0), Vec3::Z, &perspective).is_err());
    }

    #[test]
    fn a_wipeout_outline_is_its_closed_clip_in_the_parent_frame() {
        let w = Wipeout {
            boundary: Vec::new(),
            frame: Some(WipeoutFrame {
                origin: Vec3::new(10.0, 0.0, 0.0),
                u: Vec3::new(4.0, 0.0, 0.0),
                v: Vec3::new(0.0, 0.0, 2.0),
                size: Vec2::new(1.0, 1.0),
                clip_type: 2,
                clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
            }),
        };
        let got = lines(&wipeout_primitives(&w, &parent()).unwrap());
        // Pixel (x, y) is at origin + u (x + 0.5) + v (0.5 - y): the four corners of a 4 by 2 vertical rectangle.
        let corner = |x: f64, z: f64| parent().apply(Vec3::new(10.0 + x, 0.0, z));
        let want = [corner(0.0, 2.0), corner(4.0, 2.0), corner(4.0, 0.0), corner(0.0, 0.0)];
        assert_eq!(got.len(), 4, "the repeated closing vertex adds no edge");
        for (i, (a, b)) in got.iter().enumerate() {
            assert!(near(*a, want[i]) && near(*b, want[(i + 1) % 4]), "edge {i}");
        }
        assert!(wipeout_primitives(&Wipeout { boundary: Vec::new(), frame: None }, &parent()).is_err());
    }
}
