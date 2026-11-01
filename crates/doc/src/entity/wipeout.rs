//! WIPEOUT is always a closed masking polygon, unlike a polyline with an open/closed flag.
//! One exact terminal copy of the first point is a closure marker, not an editable corner.
//! Preserve logical corners when editing, adding a separate marker if the endpoints meet.
//! Grips, edits and explicit outlines share this rule; other duplicates are not simplified.

use std::borrow::Cow;

use cadcraft_geom::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wipeout {
    /// Ordered boundary, implicitly closed or ending in a repeated first point.
    pub boundary: Vec<Vec2>,
    /// The image display settings of the record (groups 70, 280, 281, 282, 283), as read.
    #[serde(default)]
    pub display: WipeoutDisplay,
    /// The source image frame and clip of a canonical wipeout (see [`WipeoutFrame`]). When it is set the wipeout is
    /// `CanonicalV2`, `boundary` is empty and [`Wipeout::world_boundary`] gives the outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<WipeoutFrame>,
    /// A legacy wipeout's source image frame (origin, pixel vectors, size, clip type and clip) as its record held it. It
    /// describes the wipeout only while `boundary` is still the outline of its clip (see [`Wipeout::source_image`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<WipeoutFrame>,
}

impl Wipeout {
    /// The source image frame while it still describes the boundary: the boundary is the clip's outline in the frame's place.
    pub fn source_image(&self) -> Option<&WipeoutFrame> {
        let image = self.image.as_ref()?;
        let outline = image.legacy_outline();
        let same = outline.len() == self.boundary.len()
            && outline.iter().zip(&self.boundary).all(|(a, b)| a.dist(*b) <= 1e-9 * (1.0 + a.len().max(b.len())));
        same.then_some(image)
    }
}

/// The display settings a WIPEOUT record keeps like an image: the display flags (70: 1 show image, 2 show when not aligned with the
/// screen, 4 use the clipping boundary, 8 transparency on), whether clipping is on (280), and the brightness, contrast and fade
/// (281, 282, 283). The defaults are what a wipeout made in CADCraft has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WipeoutDisplay {
    pub flags: i64,
    pub clipping: i64,
    pub brightness: i64,
    pub contrast: i64,
    pub fade: i64,
}

impl Default for WipeoutDisplay {
    fn default() -> Self {
        WipeoutDisplay { flags: 7, clipping: 1, brightness: 50, contrast: 50, fade: 0 }
    }
}

/// The image frame of a canonical WIPEOUT as its record holds it: the origin and the per-pixel U and V vectors in WCS
/// (groups 10, 11, 12), the image size in pixels (13), and the clip (71, 14) in pixel space, whose origin is the image's
/// top-left corner with y pointing down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WipeoutFrame {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub size: Vec2,
    /// 1 rectangular (two corners), 2 polygonal.
    pub clip_type: u8,
    pub clip: Vec<Vec2>,
}

impl Wipeout {
    /// Editable vertices, excluding one exactly repeated closing point, without changing storage.
    pub fn vertices(&self) -> &[Vec2] {
        match self.boundary.split_last() {
            Some((last, vertices)) if vertices.first() == Some(last) => vertices,
            _ => &self.boundary,
        }
    }

    /// An explicitly closed outline for geometry, rendering and export. Already closed or
    /// empty/single-point boundaries are borrowed; otherwise one closing point is appended.
    pub fn closed_boundary(&self) -> Cow<'_, [Vec2]> {
        if self.vertices().len() < self.boundary.len() || self.boundary.len() < 2 {
            Cow::Borrowed(&self.boundary)
        } else {
            let mut points = self.boundary.clone();
            points.extend(self.boundary.first());
            Cow::Owned(points)
        }
    }

    /// Move an editable vertex, carrying an existing closing marker with vertex zero. If an
    /// implicit boundary's endpoints meet, add a marker so both corners remain addressable.
    /// Invalid indices/non-finite points leave the boundary unchanged.
    pub fn set_vertex(&mut self, index: usize, to: Vec2) -> Option<()> {
        let count = self.vertices().len();
        if index >= count || !to.is_finite() {
            return None;
        }
        let explicit = count < self.boundary.len();
        *self.boundary.get_mut(index)? = to;
        if explicit
            && index == 0
            && let Some(last) = self.boundary.last_mut()
        {
            *last = to;
        }
        if !explicit && self.vertices().len() < count {
            // The moved endpoint is a logical corner, not the newly implied closing marker.
            if let Some(first) = self.boundary.first().copied() {
                self.boundary.push(first);
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityKind;

    #[test]
    fn vertices_and_grips_exclude_only_the_exact_closing_point() {
        let (a, b, c) = (Vec2::ZERO, Vec2::new(7.0, 1.0), Vec2::new(3.0, 7.0));
        let near_a = Vec2::new(1e-13, 0.0);
        for (boundary, expected) in [
            (vec![], vec![]),
            (vec![a], vec![a]),
            (vec![a, a], vec![a]),
            (vec![a, b, c], vec![a, b, c]),
            (vec![a, b, c, a], vec![a, b, c]),
            (vec![a, b, a, c, a], vec![a, b, a, c]),
            (vec![a, b, c, a, a], vec![a, b, c, a]),
            (vec![a, b, c, near_a], vec![a, b, c, near_a]),
        ] {
            let wipeout = Wipeout { boundary: boundary.clone(), ..Default::default() };
            assert_eq!(wipeout.vertices(), expected);
            assert_eq!(EntityKind::Wipeout(wipeout.clone()).grips(), expected);
            assert_eq!(wipeout.boundary, boundary);
        }
    }

    #[test]
    fn explicit_outline_is_stable_and_does_not_add_zero_length_closing_edges() {
        let points = vec![Vec2::ZERO, Vec2::new(7.0, 1.0), Vec2::new(3.0, 7.0)];
        let implicit = Wipeout { boundary: points.clone(), ..Default::default() };
        let mut expected = points.clone();
        expected.push(points[0]);
        assert!(matches!(implicit.closed_boundary(), Cow::Owned(_)));
        assert_eq!(implicit.closed_boundary().as_ref(), expected);
        assert_eq!(implicit.boundary, points);
        let explicit = Wipeout { boundary: expected.clone(), ..Default::default() };
        assert!(matches!(explicit.closed_boundary(), Cow::Borrowed(_)));
        assert_eq!(explicit.closed_boundary().as_ref(), expected);
        let a = EntityKind::Wipeout(implicit).prims();
        let b = EntityKind::Wipeout(explicit).prims();
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        for boundary in [vec![], vec![Vec2::ZERO]] {
            let wipeout = Wipeout { boundary: boundary.clone(), ..Default::default() };
            assert!(matches!(wipeout.closed_boundary(), Cow::Borrowed(_)));
            assert_eq!(wipeout.closed_boundary().as_ref(), boundary);
            assert!(EntityKind::Wipeout(wipeout).prims().is_empty());
        }
    }

    #[test]
    fn moving_endpoints_together_keeps_both_corners_editable() {
        let points = [Vec2::ZERO, Vec2::new(8.0, 0.0), Vec2::new(8.0, 6.0), Vec2::new(0.0, 6.0)];
        for explicit in [false, true] {
            for index in [0, 3] {
                let other = 3 - index;
                let mut boundary = points.to_vec();
                if explicit {
                    boundary.push(points[0]);
                }
                let mut wipeout = Wipeout { boundary, ..Default::default() };
                let mut expected = points;
                expected[index] = points[other];
                for _ in 0..2 {
                    assert_eq!(wipeout.set_vertex(index, points[other]), Some(()));
                    assert_eq!(wipeout.vertices(), expected);
                    assert_eq!(wipeout.boundary.len(), 5, "one separate closing marker, no growth");
                }
                expected[other] = Vec2::new(-2.0, 4.0);
                assert_eq!(wipeout.set_vertex(other, expected[other]), Some(()));
                assert_eq!(wipeout.vertices(), expected);
                assert_eq!(wipeout.boundary.len(), 5);
                assert_eq!(wipeout.boundary.first(), wipeout.boundary.last());
            }
        }
    }

    #[test]
    fn editing_keeps_the_closure_but_not_other_coincident_vertices_attached() {
        let (a, b, c, to) = (Vec2::ZERO, Vec2::new(7.0, 1.0), Vec2::new(3.0, 7.0), Vec2::new(2.0, 3.0));
        for explicit in [false, true] {
            let mut boundary = vec![a, b, a, c];
            if explicit {
                boundary.push(a);
            }
            let mut wipeout = Wipeout { boundary, ..Default::default() };
            assert_eq!(wipeout.set_vertex(0, to), Some(()));
            let mut expected = vec![to, b, a, c];
            if explicit {
                expected.push(to);
            }
            assert_eq!(wipeout.boundary, expected);
            for (index, point) in [(4, a), (usize::MAX, a), (0, Vec2::new(f64::NAN, 0.0)), (0, Vec2::new(0.0, f64::INFINITY))] {
                assert_eq!(wipeout.set_vertex(index, point), None);
                assert_eq!(wipeout.boundary, expected);
            }
        }
    }
}
