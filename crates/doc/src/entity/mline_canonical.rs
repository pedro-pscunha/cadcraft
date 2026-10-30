//! Canonical MLINE geometry.
//!
//! A canonical multiline keeps its vertices, and at each vertex the direction, the miter and the element parameters
//! its record declares, as WCS vectors and distances (`MLineVertex::frame`); the plane is the normal of the entity.
//! The elements' offsets and the caps stay with the style (`def`).
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact: the vertices move, the
//! directions turn, and every distance (the parameters and the offset scale) grows with the scale. A reflection swaps
//! the sides of the elements and has no measured convention for a tilted plane, so it refuses, as do a shear and a
//! non-uniform scale.

use cadcraft_geom::{Line as GLine, Mat4, OcsFrame, Segment, Vec2, Vec3};

use super::mline::{MLine, MLineGeometry, MLineVertexFrame, half_circle};
use super::text_canonical::Similarity;
use super::world::{WorldFill, WorldFillKind, WorldPrimitive};
use crate::{DocError, Result};

/// Role-bearing geometry in the multiline's plane and its complete placement. Both paint and world queries derive from it.
#[derive(Clone, Debug, PartialEq)]
pub struct MLineLayout {
    geometry: MLineGeometry,
    local_to_world: Mat4,
}

impl MLineLayout {
    /// Element-indexed strokes, caps, joints and fill in the source plane.
    pub fn geometry(&self) -> &MLineGeometry {
        &self.geometry
    }

    /// Source-plane coordinates to WCS, including the complete affine parent.
    pub fn local_to_world(&self) -> &Mat4 {
        &self.local_to_world
    }

    pub(super) fn into_primitives(self) -> Result<Vec<WorldPrimitive>> {
        let mut primitives: Vec<_> = self.geometry.lines.into_iter().map(|(_, a, b)| WorldPrimitive::Line { a: a.to3(0.0), b: b.to3(0.0) }).collect();
        for cap in self.geometry.caps {
            primitives.push(match cap {
                Segment::Line(line) => WorldPrimitive::Line { a: line.a.to3(0.0), b: line.b.to3(0.0) },
                Segment::Arc { arc, .. } => {
                    let radial = Vec2::new(arc.start.cos(), arc.start.sin()) * arc.radius;
                    WorldPrimitive::Conic {
                        center: arc.center.to3(0.0),
                        axis_u: radial.to3(0.0),
                        axis_v: radial.perp().to3(0.0),
                        start: 0.0,
                        end: arc.sweep(),
                    }
                }
            });
        }
        if !self.geometry.fill.is_empty() {
            let loops = self
                .geometry
                .fill
                .into_iter()
                .map(|points| {
                    points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .take(points.len())
                        .map(|(a, b)| WorldPrimitive::Line { a: a.to3(0.0), b: b.to3(0.0) })
                        .collect()
                })
                .collect();
            primitives.push(WorldPrimitive::Fill(WorldFill { loops, kind: WorldFillKind::Solid, style: 0 }));
        }
        primitives.iter().map(|primitive| primitive.transformed(&self.local_to_world)).collect()
    }
}

/// The record's element intervals after its start distance and break pairs.
fn element_pieces(list: &[f64], length: f64) -> Vec<(f64, f64)> {
    let Some(&start) = list.get(1) else { return Vec::new() };
    let (mut pieces, mut from) = (Vec::new(), start);
    let mut push = |a: f64, b: f64| {
        let (a, b) = (a.max(0.0), b.min(length));
        if b > a + 1e-12 {
            pieces.push((a, b));
        }
    };
    for pair in list.get(2..).unwrap_or(&[]).chunks(2) {
        match pair {
            [stop, restart] => {
                push(from, start + stop);
                from = start + restart;
            }
            [stop] => {
                push(from, start + stop);
                return pieces;
            }
            _ => {}
        }
    }
    push(from, length);
    pieces
}

impl MLine {
    /// The frames of all vertices, when each has one, with one parameter list per element and finite numbers.
    pub fn canonical_frames(&self) -> Result<Vec<&MLineVertexFrame>> {
        let invalid = |what: &str| DocError::Invalid(format!("A canonical multiline needs {what}."));
        let frames: Vec<&MLineVertexFrame> = self.vertices.iter().filter_map(|v| v.frame.as_ref()).collect();
        if frames.len() != self.vertices.len() || frames.is_empty() {
            return Err(invalid("a frame at every vertex"));
        }
        let elements = frames[0].params.len();
        let finite = self.vertices.iter().all(|v| v.p.is_finite())
            && frames.iter().all(|f| f.dir.is_finite() && f.miter.is_finite() && f.params.iter().flatten().all(|x| x.is_finite()));
        if !finite || frames.iter().any(|f| f.params.len() != elements) {
            return Err(invalid("finite numbers and the same elements at every vertex"));
        }
        Ok(frames)
    }

    /// Build the canonical paint roles from the record's own miters and parameters. Off-plane source data refuses.
    pub fn layout_canonical(&self, normal: Vec3, parent: &Mat4) -> Result<MLineLayout> {
        if !crate::has_finite_numbers(self) || !crate::has_finite_numbers(parent) || parent.m[3] != [0.0, 0.0, 0.0, 1.0] {
            return Err(DocError::Invalid("A canonical multiline needs finite data and an affine parent.".into()));
        }
        let frames = self.canonical_frames()?;
        let plane = OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical multiline needs a finite source plane.".into()))?;
        let first = self.vertices.first().ok_or_else(|| DocError::Invalid("A canonical multiline needs vertices.".into()))?;
        let unit_normal = plane.to_world(Vec3::Z);
        let in_plane = self.vertices.iter().all(|v| {
            let delta = v.p - first.p;
            unit_normal.dot(delta).abs() <= 1e-9 * (1.0 + delta.len())
        }) && frames.iter().all(|f| [f.dir, f.miter].iter().all(|v| unit_normal.dot(*v).abs() <= 1e-9 * (1.0 + v.len())));
        if !in_plane {
            return Err(DocError::Invalid("A canonical multiline has vertices or vertex vectors outside its declared plane.".into()));
        }
        let elements = frames[0].params.len();
        if elements != self.def.elements.len() {
            return Err(DocError::Invalid("A canonical multiline needs one parameter list per style element.".into()));
        }
        let height = plane.to_ocs(first.p).z;
        let lift = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, height], [0.0, 0.0, 0.0, 1.0]] };
        let local_to_world = parent.mul(&plane.matrix()).mul(&lift);
        if !crate::has_finite_numbers(&local_to_world) {
            return Err(DocError::Invalid("The canonical multiline placement is not finite.".into()));
        }
        let n = self.vertices.len();
        let at = |k: usize, element: usize| {
            let (vertex, frame) = (&self.vertices[k % n], frames[k % n]);
            let miter = plane.to_ocs(frame.miter).xy().normalized();
            plane.to_ocs(vertex.p).xy() + miter * frame.params[element].first().copied().unwrap_or(0.0)
        };
        let mut geometry = MLineGeometry::default();
        for (k, frame) in frames.iter().enumerate().take(self.segment_count()) {
            for element in 0..elements {
                let (a, b) = (at(k, element), at(k + 1, element));
                let length = a.dist(b);
                if length <= 1e-12 {
                    continue;
                }
                let direction = (b - a) / length;
                geometry.lines.extend(
                    element_pieces(&frame.params[element], length)
                        .into_iter()
                        .map(|(start, end)| (element, a + direction * start, a + direction * end)),
                );
            }
        }
        let offsets = self.offsets();
        let mut order: Vec<usize> = (0..elements).collect();
        order.sort_by(|a, b| offsets[*b].total_cmp(&offsets[*a]));
        if let (Some(&top), Some(&bottom)) = (order.first(), order.last()) {
            let across = |k| Segment::Line(GLine::new(at(k, top), at(k, bottom)));
            if !self.closed {
                for (k, caps, skip, start) in
                    [(0, self.def.start_caps, self.no_start_caps, true), (n - 1, self.def.end_caps, self.no_end_caps, false)]
                {
                    if skip {
                        continue;
                    }
                    if caps.line && order.len() > 1 {
                        geometry.caps.push(across(k));
                    }
                    for i in 0..order.len() / 2 {
                        if (i == 0 && !caps.outer_arc) || (i > 0 && !caps.inner_arcs) {
                            continue;
                        }
                        if let Some(cap) = half_circle(at(k, order[i]), at(k, order[order.len() - 1 - i]), start) {
                            geometry.caps.push(cap);
                        }
                    }
                }
            }
            if self.def.joints && order.len() > 1 {
                let inner = if self.closed { 0..n } else { 1..n.saturating_sub(1) };
                geometry.caps.extend(inner.map(across));
            }
            if self.def.fill.is_some() {
                let side = |element| (0..n).map(|k| at(k, element)).collect::<Vec<_>>();
                geometry.fill = if self.closed {
                    vec![side(top), side(bottom)]
                } else {
                    let mut polygon = side(top);
                    polygon.extend(side(bottom).into_iter().rev());
                    vec![polygon]
                };
            }
        }
        Ok(MLineLayout { geometry, local_to_world })
    }

    /// This multiline, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(MLine, Vec3)> {
        self.canonical_frames()?;
        OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical multiline needs a plane with a finite, non-zero normal.".into()))?;
        let sim = Similarity::from_mat4(transform)?;
        let k = sim.scale;
        let mut moved = self.clone();
        for v in &mut moved.vertices {
            v.p = sim.point(v.p);
            if let Some(f) = &mut v.frame {
                f.dir = sim.direction(f.dir);
                f.miter = sim.direction(f.miter);
                f.params.iter_mut().flatten().for_each(|x| *x *= k);
            }
        }
        moved.scale *= k;
        Ok((moved, sim.direction(normal)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MLineJust, MLineStyle, MLineVertex};

    fn mline() -> MLine {
        let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 2.0);
        for (p, dir, miter) in [
            (Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            (Vec3::new(5.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        ] {
            let mut v = MLineVertex::new(p);
            v.frame = Some(MLineVertexFrame { dir, miter, params: vec![vec![1.0, 5.0], vec![-1.0, 5.0]] });
            m.vertices.push(v);
        }
        m
    }

    fn scale(k: f64) -> Mat4 {
        Mat4 { m: [[k, 0.0, 0.0, 1.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn a_scale_grows_every_distance_and_keeps_the_directions() {
        let (m, n) = mline().transformed_canonical(Vec3::Z, &scale(3.0)).unwrap();
        assert_eq!(m.scale, 6.0);
        assert!(near(m.vertices[1].p, Vec3::new(16.0, 0.0, 3.0)) && near(n, Vec3::Z));
        let f = m.vertices[1].frame.as_ref().unwrap();
        assert!(near(f.dir, Vec3::new(1.0, 0.0, 0.0)) && near(f.miter, Vec3::new(0.0, 1.0, 0.0)));
        assert_eq!(f.params, vec![vec![3.0, 15.0], vec![-3.0, 15.0]]);
    }

    #[test]
    fn a_rotation_about_x_turns_the_vectors_and_the_normal() {
        let (s, c) = std::f64::consts::FRAC_PI_2.sin_cos();
        let rx = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (m, n) = mline().transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &rx).unwrap();
        assert!(near(n, Vec3::new(0.0, -2.0, 0.0)));
        assert!(near(m.vertices[0].frame.as_ref().unwrap().miter, Vec3::new(0.0, 0.0, 1.0)));
        assert!(near(m.vertices[0].p, Vec3::new(0.0, -1.0, 0.0)));
    }

    #[test]
    fn a_missing_frame_a_reflection_or_a_bad_normal_refuses() {
        let mut m = mline();
        m.vertices[1].frame = None;
        assert!(m.transformed_canonical(Vec3::Z, &scale(1.0)).is_err());
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(mline().transformed_canonical(Vec3::Z, &mirror).is_err());
        assert!(mline().transformed_canonical(Vec3::ZERO, &scale(1.0)).is_err());
        let mut ragged = mline();
        ragged.vertices[1].frame.as_mut().unwrap().params.pop();
        assert!(ragged.transformed_canonical(Vec3::Z, &scale(1.0)).is_err());
    }
}
