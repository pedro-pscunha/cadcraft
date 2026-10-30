//! Canonical MULTILEADER geometry (the entity and its context both `CanonicalV2`).
//!
//! A MULTILEADER record has no extrusion: every point it holds is a WCS point and every direction a WCS vector, and
//! a canonical multileader keeps them as the record gives them. That is the landing, the dogleg direction and the
//! vertices of every root; the origin and the X and Y axes of the context; the text location, its normal and its
//! direction; the block location, its normal, scale and rotation. The text rotation is the angle of the direction in
//! the plane of the text normal, and the block rotation is measured in the plane of the block normal from its OCS X
//! axis (the arbitrary axis rule, as for a block reference). Nothing is projected or mirrored on import.
//!
//! The block's attributes are in the coordinates of the block definition they come from, like their definitions:
//! they ride the block matrix, so no edit of the multileader touches them.
//!
//! Edits are exact for what a similarity does to a plane: a translation, a rotation about any axis and a uniform
//! positive scale. A reflection, a shear and a non-uniform scale have no measured convention for the text or the block
//! of a multileader, so they refuse atomically.

use cadcraft_geom::{Mat4, OcsFrame, Vec2, Vec3};

use super::{GeometryRevision, Insert, Leader, LeaderFrame, MLeader, MText, Similarity};
use crate::{DocError, Result};

fn frame_of(normal: Vec3) -> Result<OcsFrame> {
    frame(normal, "block")
}

fn frame(normal: Vec3, what: &str) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid(format!("A canonical multileader needs a finite, non-zero {what} normal.")))
}

impl MLeader {
    /// Refuse a payload whose context, block attributes or the multiline texts embedded in them are not canonical.
    pub fn require_canonical(&self) -> Result<()> {
        let canonical_attrib = |a: &super::Attrib| {
            a.geometry_revision == GeometryRevision::CanonicalV2
                && a.multiline.as_ref().is_none_or(|m| m.geometry_revision == GeometryRevision::CanonicalV2)
        };
        let canonical =
            self.context.geometry_revision == GeometryRevision::CanonicalV2 && self.block.iter().flat_map(|b| &b.attribs).all(canonical_attrib);
        if canonical { Ok(()) } else { Err(DocError::Invalid("This multileader payload is not canonical.".into())) }
    }

    /// The block of the content in WCS: the matrix from the block definition's coordinates (with the definition's base
    /// point `base`) to WCS. `None` for text content or none.
    pub fn block_world_transform(&self, base: Vec3) -> Result<Option<Mat4>> {
        let Some(block) = &self.block else { return Ok(None) };
        let to = frame(self.context.block_normal, "block")?;
        let placed = Insert { insert: to.to_ocs(block.insert), ..block.clone() };
        placed.world_transform(self.context.block_normal, base, 0, 0).map(Some)
    }

    /// The unit normal of the plane of the leader lines: the context's X axis crossed into its Y axis.
    pub fn leader_plane_normal(&self) -> Result<Vec3> {
        self.context
            .x_axis
            .cross(self.context.y_axis)
            .try_normalized()
            .ok_or_else(|| DocError::Invalid("The X and Y axes of a multileader context span no plane.".into()))
    }

    /// This multileader after the similarity `transform`: every point, direction, length and size of the roots, the
    /// context, the text and the block follow together, or the call fails and nothing changes.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<MLeader> {
        self.require_canonical()?;
        let sim = Similarity::from_mat4(transform)?;
        let mut moved = self.clone();
        for root in &mut moved.roots {
            root.landing = sim.point(root.landing);
            root.dogleg_dir = sim.direction(root.dogleg_dir);
            root.dogleg *= sim.scale;
            for vertex in root.branches.iter_mut().flat_map(|b| b.vertices.iter_mut()) {
                *vertex = sim.point(*vertex);
            }
        }
        moved.arrow_size *= sim.scale;
        let c = &mut moved.context;
        c.origin = sim.point(c.origin);
        c.x_axis = sim.direction(c.x_axis);
        c.y_axis = sim.direction(c.y_axis);
        // A record holds the text's frame only with a text and the block's normal only with a block, so the others stay.
        if let Some(text) = &self.text {
            let (text, normal) = text.transformed_canonical(self.context.text_normal, transform)?;
            moved.context.text_normal = normal;
            moved.context.text_dir = text.direction.unwrap_or(moved.context.text_dir);
            moved.text = Some(text);
        }
        if let Some(block) = &self.block {
            let from = frame(self.context.block_normal, "block")?;
            moved.context.block_normal = sim.direction(self.context.block_normal);
            let to = frame(moved.context.block_normal, "block")?;
            let (s, c) = block.rotation.sin_cos();
            let x_axis = to.to_ocs(sim.direction(from.to_world(Vec3::new(c, s, 0.0))));
            moved.block = Some(Insert {
                insert: sim.point(block.insert),
                scale: block.scale * sim.scale,
                rotation: Vec2::new(x_axis.x, x_axis.y).angle(),
                ..block.clone()
            });
        }
        Ok(moved)
    }
}

/// A canonical multileader as the entities a file older than R2007 holds (no MULTILEADER), each in its own plane.
#[derive(Clone, Debug, PartialEq)]
pub struct MLeaderParts {
    /// A LEADER for each leader line, through its vertices to its root's landing, then a LEADER without arrowhead for
    /// each dogleg; all in the plane of `leader_normal`.
    pub leaders: Vec<Leader>,
    pub leader_normal: Vec3,
    /// The text, with the normal of its plane.
    pub text: Option<(MText, Vec3)>,
    /// The block reference, its insertion point an OCS point of its normal, and that normal. The attributes are placed in
    /// WCS by the block matrix; one the matrix cannot place exactly (a non-uniform scale) is left out.
    pub block: Option<(Insert, Vec3)>,
}

impl MLeader {
    /// The entities of a file older than R2007 for this multileader: `style` is the dimension style of its leaders and
    /// `block_base` the base point of its block definition.
    pub fn split_canonical(&self, style: &str, block_base: Vec3) -> Result<MLeaderParts> {
        self.require_canonical()?;
        let normal = self.leader_plane_normal()?;
        let direction = self.context.x_axis.normalized();
        let (text_height, text_width) = self.text.as_ref().map_or((0.0, 0.0), |t| (t.height, t.width));
        let frame = LeaderFrame { direction, block_offset: Vec3::ZERO, annotation_offset: Vec3::ZERO, text_height, text_width };
        let leader = |vertices: Vec<Vec3>, arrow: bool, spline: bool| Leader {
            vertices,
            arrow,
            spline,
            style: style.to_string(),
            frame: Some(frame.clone()),
            links: Default::default(),
        };
        let mut leaders: Vec<Leader> = self
            .lines()
            .map(|(root, branch)| leader(branch.vertices.iter().copied().chain(std::iter::once(root.landing)).collect(), true, self.spline))
            .collect();
        leaders.extend(
            self.roots.iter().filter(|r| r.dogleg > 0.0 && !r.branches.is_empty()).map(|r| leader(vec![r.landing, r.dogleg_end()], false, false)),
        );
        let block = match (&self.block, self.block_world_transform(block_base)?) {
            (Some(block), Some(matrix)) => {
                let to = frame_of(self.context.block_normal)?;
                let attribs = block.attribs.iter().filter_map(|a| a.transformed_canonical(&matrix).ok()).collect();
                Some((Insert { insert: to.to_ocs(block.insert), attribs, ..block.clone() }, self.context.block_normal))
            }
            _ => None,
        };
        let text = self.text.clone().map(|t| (t, self.context.text_normal));
        Ok(MLeaderParts { leaders, leader_normal: normal, text, block })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MLeaderContext, MText};

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn text() -> MText {
        MText {
            insert: Vec3::new(12.0, 20.0, 3.0),
            height: 0.4,
            width: 10.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "T".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: Some(Vec3::new(1.0, 0.0, 0.0)),
            drawing_direction: None,
        }
    }

    fn text_for_attrib() -> crate::Text {
        crate::Text {
            insert: Vec3::ZERO,
            align_pt: None,
            height: 1.0,
            value: "v".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: crate::HAlign::Left,
            valign: crate::VAlign::Baseline,
            backward: false,
            upside_down: false,
        }
    }

    fn two_roots() -> MLeader {
        let mut m =
            MLeader::single(Vec3::new(10.0, 20.0, 3.0), 2.0, false, vec![vec![Vec3::new(0.0, 0.0, 3.0), Vec3::new(4.0, 6.0, 3.0)]], Some(text()));
        let mut second = crate::MLeaderRoot::new(Vec3::new(-10.0, 18.0, 3.0), 3.0, true, vec![vec![Vec3::new(-2.0, 0.0, 3.0)]]);
        second.id = 1;
        m.roots.push(second);
        m.arrow_size = 0.18;
        m.context =
            MLeaderContext { origin: Vec3::new(0.0, 0.0, 3.0), geometry_revision: GeometryRevision::CanonicalV2, ..MLeaderContext::default() };
        m
    }

    fn rotation_x(angle: f64) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4 { m: [[1.0, 0.0, 0.0, 5.0], [0.0, c, -s, -2.0], [0.0, s, c, 1.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    #[test]
    fn a_legacy_context_refuses() {
        let mut m = two_roots();
        m.context.geometry_revision = GeometryRevision::LegacyProjectedV1;
        assert!(m.transformed_canonical(&rotation_x(0.5)).is_err());
    }

    #[test]
    fn every_point_direction_and_length_follows_a_tilted_rotation_with_a_scale() {
        let t = rotation_x(std::f64::consts::FRAC_PI_2)
            .mul(&Mat4 { m: [[2.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]] });
        let m = two_roots();
        let moved = m.transformed_canonical(&t).unwrap();
        for (a, b) in m.roots.iter().zip(&moved.roots) {
            assert!(near(b.landing, t.apply(a.landing)));
            assert!(near(b.dogleg_end(), t.apply(a.dogleg_end())), "the dogleg end is the image of the dogleg end");
            assert!((b.dogleg - 2.0 * a.dogleg).abs() < 1e-12);
            assert_eq!((a.id, a.branches.iter().map(|x| x.id).collect::<Vec<_>>()), (b.id, b.branches.iter().map(|x| x.id).collect::<Vec<_>>()));
            for (u, v) in a.branches.iter().flat_map(|x| &x.vertices).zip(b.branches.iter().flat_map(|x| &x.vertices)) {
                assert!(near(*v, t.apply(*u)));
            }
        }
        assert!((moved.arrow_size - 0.36).abs() < 1e-12);
        let (c, d) = (&moved.context, &m.context);
        assert!(near(c.origin, t.apply(d.origin)));
        for (after, before) in [(c.x_axis, d.x_axis), (c.y_axis, d.y_axis), (c.text_normal, d.text_normal), (c.text_dir, d.text_dir)] {
            assert!(near(after, t.apply_vec(before) * 0.5), "{after:?}");
        }
        let text = moved.text.unwrap();
        assert!(near(text.insert, t.apply(Vec3::new(12.0, 20.0, 3.0))));
        assert!((text.height, text.width) == (0.8, 20.0));
        assert!(near(text.direction.unwrap(), c.text_dir), "the context direction is the text's");
        // The text rotation is the angle of the direction in the plane of the new text normal.
        let in_plane = OcsFrame::new(c.text_normal).unwrap().to_ocs(text.direction.unwrap());
        assert!((text.rotation - in_plane.y.atan2(in_plane.x)).abs() < 1e-12);
    }

    #[test]
    fn the_block_matrix_of_the_moved_multileader_is_the_transform_times_the_old_one() {
        let mut m = two_roots();
        m.text = None;
        m.context.block_normal = Vec3::new(0.0, 3.0, 4.0);
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::new(7.0, -3.0, 2.0),
            scale: Vec3::new(1.5, -2.0, 0.5),
            rotation: 0.4,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
        let base = Vec3::new(1.0, 2.0, 3.0);
        let (s, c) = 0.9f64.sin_cos();
        let t = Mat4 { m: [[3.0 * c, -3.0 * s, 0.0, 4.0], [3.0 * s, 3.0 * c, 0.0, 5.0], [0.0, 0.0, 3.0, -6.0], [0.0, 0.0, 0.0, 1.0]] }
            .mul(&rotation_x(0.6));
        let moved = m.transformed_canonical(&t).unwrap();
        let before = m.block_world_transform(base).unwrap().unwrap();
        let after = moved.block_world_transform(base).unwrap().unwrap();
        let want = t.mul(&before);
        for (row_a, row_b) in after.m.iter().zip(&want.m) {
            for (a, b) in row_a.iter().zip(row_b) {
                assert!((a - b).abs() < 1e-9, "{after:?} vs {want:?}");
            }
        }
        // The scale signs stay and the scale grows by the factor.
        let scale = moved.block.unwrap().scale;
        assert!(near(scale, Vec3::new(1.5 * 3.0, -2.0 * 3.0, 0.5 * 3.0)), "{scale:?}");
    }

    #[test]
    fn a_reflection_refuses_and_the_attributes_must_be_canonical() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(two_roots().transformed_canonical(&mirror).is_err());
        let mut m = two_roots();
        m.text = None;
        let legacy = crate::Attrib::new(
            "T",
            crate::Text {
                insert: Vec3::ZERO,
                align_pt: None,
                height: 1.0,
                value: "v".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: crate::HAlign::Left,
                valign: crate::VAlign::Baseline,
                backward: false,
                upside_down: false,
            },
        );
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![legacy],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
        assert!(m.require_canonical().is_err());
    }

    #[test]
    fn an_embedded_multiline_text_must_be_canonical_too() {
        let mut m = two_roots();
        m.text = None;
        let mut attrib = crate::Attrib::new("T", text_for_attrib());
        attrib.geometry_revision = GeometryRevision::CanonicalV2;
        attrib.multiline = Some(crate::MultilineAttrib {
            geometry_revision: GeometryRevision::LegacyProjectedV1,
            normal: Vec3::Z,
            mtext: text(),
            read_as: String::new(),
            embedded_46: None,
            type_72: None,
        });
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![attrib],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
        assert!(m.require_canonical().is_err(), "a legacy embedded text refuses");
        assert!(m.transformed_canonical(&rotation_x(0.5)).is_err());
        m.block.as_mut().unwrap().attribs[0].multiline.as_mut().unwrap().geometry_revision = GeometryRevision::CanonicalV2;
        assert!(m.require_canonical().is_ok());
    }

    #[test]
    fn the_leader_plane_normal_is_the_axes_cross_product() {
        let mut m = two_roots();
        m.context.x_axis = Vec3::new(0.0, 1.0, 0.0);
        m.context.y_axis = Vec3::new(0.0, 0.0, 1.0);
        assert!(near(m.leader_plane_normal().unwrap(), Vec3::new(1.0, 0.0, 0.0)));
        m.context.y_axis = Vec3::new(0.0, 2.0, 0.0);
        assert!(m.leader_plane_normal().is_err());
    }
}
