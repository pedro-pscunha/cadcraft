use super::common::Handle;
use super::{Color, Deserialize, PolyVertex, Serialize, Vec2, Vec3, one};

/// A hatch boundary loop: closed polyline with bulges.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HatchLoop {
    pub vertices: Vec<PolyVertex>,
    #[serde(default)]
    pub outer: bool,
    /// The objects the loop was made from (DXF group 97 count, 330 handles): an associative
    /// hatch re-fills the loop from them when they change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Handle>,
    /// The loop type flags of a canonical loop as its record holds them (group 92: 1 external, 2 polyline, 4 derived,
    /// 8 text box, 16 outermost, and so on).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub flags: u32,
    /// The exact boundary edges of a canonical loop, in the record's order. When they are set `vertices` is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<HatchEdge>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// One boundary edge of a canonical hatch loop, in the hatch's OCS (2D) as its record holds it. Angles are in radians,
/// as the model keeps them; `ccw` is the record's direction flag (a clockwise edge keeps the angles it was given).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HatchEdge {
    Line {
        a: Vec2,
        b: Vec2,
    },
    Arc {
        center: Vec2,
        radius: f64,
        start: f64,
        end: f64,
        ccw: bool,
    },
    /// `major` is the major axis endpoint relative to the center; `start` and `end` are parameters.
    Ellipse {
        center: Vec2,
        major: Vec2,
        ratio: f64,
        start: f64,
        end: f64,
        ccw: bool,
    },
    Spline(HatchSpline),
    /// A polyline boundary (loop flag 2): one edge holding all its vertices.
    Polyline {
        vertices: Vec<PolyVertex>,
        closed: bool,
    },
}

/// A spline edge as its record holds it. `weights` is empty unless the spline is rational; the tangents are zero
/// without fit points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HatchSpline {
    pub degree: u32,
    pub rational: bool,
    pub periodic: bool,
    pub knots: Vec<f64>,
    pub control: Vec<Vec2>,
    pub weights: Vec<f64>,
    pub fit: Vec<Vec2>,
    pub start_tangent: Vec2,
    pub end_tangent: Vec2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hatch {
    pub pattern: String,
    pub solid: bool,
    pub loops: Vec<HatchLoop>,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub angle: f64,
    #[serde(default)]
    pub associative: bool,
    /// 0 normal (odd parity), 1 outer, 2 ignore.
    #[serde(default)]
    pub style: u8,
    #[serde(default)]
    pub elevation: f64,
    /// Gradient fill: name + second colour.
    #[serde(default)]
    pub gradient: Option<Gradient>,
    #[serde(default)]
    pub origin: Vec2,
    #[serde(default)]
    pub background: Option<Color>,
    /// The pattern's line families as a file defined them, for a pattern that isn't in the
    /// standard library (unit scale and angle, like the library's; empty otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pattern_lines: Vec<crate::library::PatternLine>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    pub name: String,
    pub color1: Color,
    pub color2: Color,
    #[serde(default)]
    pub angle: f64,
    #[serde(default)]
    pub centered: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wipeout {
    pub boundary: Vec<Vec2>,
    /// The source image frame and clip of a canonical wipeout (see [`WipeoutFrame`]). When it is set the wipeout is
    /// `CanonicalV2`, `boundary` is empty and [`Wipeout::world_boundary`] gives the outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<WipeoutFrame>,
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
