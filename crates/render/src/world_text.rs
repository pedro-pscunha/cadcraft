//! The layout of a [`WorldText`]: the text is shaped once in its local glyph coordinates (the insertion or attachment point
//! at the origin, x along the baseline, heights and widths in the payload's local units) and every point and glyph basis
//! is then mapped once through the complete affine frame, `origin + x_axis * x + y_axis * y`. The frame carries the whole
//! parent (rotation, scale, shear), so no length of the layout is scaled a second time, and a shear skews the glyphs the
//! way the parent skews the plane.
//!
//! A single-line text keeps its alignment (justification and the alignment point), its width factor and slant, and the
//! backward and upside-down generation flags of the entity beside those of its text style. A multiline text keeps its
//! colour runs and its columns: the pieces come out as the font layout gives them.

use cadcraft_doc::{DocError, Drawing, Result, Text, TextFrame, WorldText, WorldTextContent};
use cadcraft_fonts::{Shaped, TextFont, TextGlyph, TextParams};
use cadcraft_geom::{Vec2, Vec3};

use crate::text_entity::{layout_mtext_entity, text_align, text_style};

/// Shaped text with its points in WCS: strokes and glyph outlines as 3D polylines, and the font glyphs with their 3D origin
/// and em vectors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldShaped {
    pub strokes: Vec<Vec<Vec3>>,
    pub glyphs: Vec<Vec<Vec<Vec3>>>,
    pub text: Vec<WorldGlyph>,
    /// The local width of the shaped text (in the payload's local units, before the frame).
    pub width: f64,
}

/// An outline font glyph with its basis in WCS: the glyph point `(u, v)` (in em units, 1000 per em) lands at
/// `origin + x * u / 1000 + y * v / 1000`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGlyph {
    pub glyph: TextGlyph,
    pub origin: Vec3,
    pub x: Vec3,
    pub y: Vec3,
}

impl WorldShaped {
    /// The text as the 2D renderer draws it, seen along Z.
    pub fn project_xy(&self) -> Shaped {
        let flat = |p: &Vec3| p.xy();
        Shaped {
            strokes: self.strokes.iter().map(|s| s.iter().map(flat).collect()).collect(),
            glyphs: self.glyphs.iter().map(|g| g.iter().map(|c| c.iter().map(flat).collect()).collect()).collect(),
            width: self.width,
            text: self.text.iter().map(|g| TextGlyph { origin: g.origin.xy(), x: g.x.xy(), y: g.y.xy(), ..g.glyph.clone() }).collect(),
        }
    }
}

/// One colour run of a multiline text.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldMTextPiece {
    pub color: Option<cadcraft_fonts::MTextColor>,
    pub shaped: WorldShaped,
}

/// The layout of a world text.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldTextLayout {
    Text(WorldShaped),
    MText(Vec<WorldMTextPiece>),
}

/// The frame's coefficients `(a, b)` of the vector `d = x_axis * a + y_axis * b`, or `None` when the axes do not span a plane.
fn local_coordinates(frame: &TextFrame, d: Vec3) -> Option<Vec2> {
    let (uu, uv, vv) = (frame.x_axis.dot(frame.x_axis), frame.x_axis.dot(frame.y_axis), frame.y_axis.dot(frame.y_axis));
    let det = uu * vv - uv * uv;
    if !(det.is_finite() && det > 1e-18 * uu * vv && det > 0.0) {
        return None;
    }
    let (du, dv) = (d.dot(frame.x_axis), d.dot(frame.y_axis));
    Some(Vec2::new((du * vv - dv * uv) / det, (dv * uu - du * uv) / det))
}

fn map(frame: &TextFrame, p: Vec2) -> Vec3 {
    frame.origin + frame.x_axis * p.x + frame.y_axis * p.y
}

fn map_vector(frame: &TextFrame, v: Vec2) -> Vec3 {
    frame.x_axis * v.x + frame.y_axis * v.y
}

/// A local shaped piece mapped through the frame once.
fn world_shaped(frame: &TextFrame, shaped: &Shaped) -> WorldShaped {
    WorldShaped {
        strokes: shaped.strokes.iter().map(|s| s.iter().map(|p| map(frame, *p)).collect()).collect(),
        glyphs: shaped.glyphs.iter().map(|g| g.iter().map(|c| c.iter().map(|p| map(frame, *p)).collect()).collect()).collect(),
        text: shaped
            .text
            .iter()
            .map(|g| WorldGlyph { glyph: g.clone(), origin: map(frame, g.origin), x: map_vector(frame, g.x), y: map_vector(frame, g.y) })
            .collect(),
        width: shaped.width,
    }
}

/// A single-line text laid out in local coordinates: insertion point at the origin, the baseline along x, and the alignment
/// point (when there is one) in the frame's coordinates. The text style gives the font, and the entity's generation flags
/// join the style's.
pub fn local_text_layout(d: &Drawing, t: &Text, local_align: Option<Vec2>, value: &str) -> Shaped {
    let ts = text_style(d, &t.style);
    let font = TextFont::resolve(&ts.font);
    let (h, v) = text_align(t.halign, t.valign);
    let params = TextParams {
        insert: Vec2::ZERO,
        align_pt: local_align,
        height: t.height,
        rotation: 0.0,
        width_factor: t.width_factor,
        oblique: t.oblique,
        h,
        v,
        backwards: ts.backwards || t.backward,
        upside_down: ts.upside_down || t.upside_down,
    };
    cadcraft_fonts::place(&font, value, &params).0
}

/// Lay out `text` (TEXT, ATTRIB, ATTDEF or MTEXT content) in local coordinates and map it through its affine frame.
/// `value` is the displayed string of a single-line text (the attribute value, say); a multiline text uses its contents.
/// A frame whose axes do not span a plane, or a non-finite frame, is refused.
pub fn layout_world_text(d: &Drawing, text: &WorldText, value: &str) -> Result<WorldTextLayout> {
    let frame = &text.frame;
    let finite = [frame.origin, frame.x_axis, frame.y_axis].iter().all(|v| v.is_finite());
    if !finite || local_coordinates(frame, Vec3::new(1.0, 0.0, 0.0)).is_none() {
        return Err(DocError::Invalid("A text frame whose axes do not span a plane cannot be laid out.".into()));
    }
    Ok(match &text.content {
        WorldTextContent::Text(t) => {
            let local_align = text
                .align
                .map(|a| local_coordinates(frame, a - frame.origin))
                .map(|c| c.ok_or_else(|| DocError::Invalid("The alignment point is outside the text frame.".into())))
                .transpose()?;
            let shaped = local_text_layout(d, t, local_align, value);
            WorldTextLayout::Text(world_shaped(frame, &shaped))
        }
        WorldTextContent::MText(t) => {
            // The insertion at the origin and the baseline along x: the frame carries the position and the direction.
            let local = cadcraft_doc::MText { insert: Vec3::ZERO, rotation: 0.0, direction: None, ..t.clone() };
            let layout = layout_mtext_entity(d, &local);
            WorldTextLayout::MText(layout.pieces.iter().map(|p| WorldMTextPiece { color: p.color, shaped: world_shaped(frame, &p.shaped) }).collect())
        }
    })
}
