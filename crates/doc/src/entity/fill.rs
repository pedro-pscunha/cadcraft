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
    /// A legacy loop that came from edges: the `vertices` when `edges` were read. `edges` still describe the loop only while
    /// `vertices` are these (an edit of the vertices makes the exact edges stale, and the loop is written from its vertices).
    /// Empty for a canonical loop, whose `edges` are the loop, and for a loop with no exact edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_vertices: Vec<PolyVertex>,
}

impl HatchLoop {
    /// The exact edges that describe this loop: those of a canonical loop (no vertices), or of a legacy loop whose vertices are
    /// still the ones its edges were read with. `None` for a loop written from its vertices.
    pub fn exact_edges(&self) -> Option<&[HatchEdge]> {
        (!self.edges.is_empty() && (self.vertices.is_empty() || self.vertices == self.edge_vertices)).then_some(self.edges.as_slice())
    }
}

impl HatchEdge {
    /// This edge after the 2D map `m`, or `None` when the edge cannot keep its type under it: lines and splines follow any
    /// affine map; arcs, ellipses and polyline edges need a similarity (a rotation, uniform scale and reflection). A reflection
    /// reverses the direction of an arc or ellipse, so its angles mirror about the image of the X axis and `ccw` flips.
    pub fn transformed(&self, m: &cadcraft_geom::Mat3) -> Option<HatchEdge> {
        let k = m.scale_factor();
        let (a, b, c, d) = (m.a, m.b, m.c, m.d);
        let similar = ((a * c + b * d).abs() <= 1e-9 * k * k) && ((a * a + b * b - c * c - d * d).abs() <= 1e-9 * k * k);
        let mirror = m.is_mirroring();
        let delta = m.rotation();
        let turn = |angle: f64| if mirror { delta - angle } else { angle + delta };
        Some(match self {
            HatchEdge::Line { a, b } => HatchEdge::Line { a: m.apply(*a), b: m.apply(*b) },
            HatchEdge::Arc { center, radius, start, end, ccw } if similar => {
                HatchEdge::Arc { center: m.apply(*center), radius: radius * k, start: turn(*start), end: turn(*end), ccw: ccw ^ mirror }
            }
            HatchEdge::Ellipse { center, major, ratio, start, end, ccw } if similar => {
                let (start, end) = if mirror { (-start, -end) } else { (*start, *end) };
                HatchEdge::Ellipse { center: m.apply(*center), major: m.apply_vec(*major), ratio: *ratio, start, end, ccw: ccw ^ mirror }
            }
            HatchEdge::Spline(sp) => HatchEdge::Spline(HatchSpline {
                control: sp.control.iter().map(|p| m.apply(*p)).collect(),
                fit: sp.fit.iter().map(|p| m.apply(*p)).collect(),
                start_tangent: m.apply_vec(sp.start_tangent),
                end_tangent: m.apply_vec(sp.end_tangent),
                ..sp.clone()
            }),
            HatchEdge::Polyline { vertices, closed } if similar => HatchEdge::Polyline {
                vertices: vertices
                    .iter()
                    .map(|v| PolyVertex {
                        p: m.apply(v.p),
                        start_width: v.start_width * k,
                        end_width: v.end_width * k,
                        bulge: if mirror { -v.bulge } else { v.bulge },
                    })
                    .collect(),
                closed: *closed,
            },
            _ => return None,
        })
    }
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
    /// The pattern type (DXF 76): 0 user-defined, 1 predefined, 2 custom. A hatch made here is predefined.
    #[serde(default = "predefined")]
    pub pattern_type: u8,
    /// The pattern is double (DXF 77): drawn a second time at right angles.
    #[serde(default)]
    pub pattern_double: bool,
    /// The seed points (DXF 98): points of the hatched area that the boundary was found from, in the hatch's OCS.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed_points: Vec<Vec2>,
}

fn predefined() -> u8 {
    1
}

impl Hatch {
    /// The seed points that still lie in the hatched area, which is where a seed point must be: after an edit that reshaped the
    /// boundary, a seed point it left outside is dropped. The loops of a canonical hatch (exact edges) only move, turn and
    /// scale with their seed points, so all of them stay.
    pub fn kept_seed_points(&self) -> Vec<Vec2> {
        if self.loops.iter().any(|l| l.vertices.is_empty() && !l.edges.is_empty()) {
            return self.seed_points.clone();
        }
        let loops: Vec<cadcraft_geom::Polyline> =
            self.loops.iter().map(|l| cadcraft_geom::Polyline { vertices: l.vertices.clone(), closed: true }).collect();
        self.seed_points.iter().copied().filter(|p| loops.iter().filter(|l| l.contains(*p)).count() % 2 == 1).collect()
    }
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
    /// The gradient groups the colours and `centered` do not hold, as a file gave them.
    #[serde(default)]
    pub source: GradientSource,
}

/// The groups of a gradient that [`Gradient`]'s own fields only partly hold: the shift (DXF 461, a real number between 0
/// centred and 1 shifted), the colour mode (452: 0 two colours, 1 one colour), the tint (462) of a one-colour gradient, and
/// the indexed colour (63) that goes with each true colour (421), with that true colour. All empty for a gradient made here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientSource {
    pub shift: Option<f64>,
    pub color_mode: i64,
    pub tint: f64,
    pub aci: [Option<(i16, cadcraft_color::Rgb)>; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wipeout {
    pub boundary: Vec<Vec2>,
    /// The image display settings of the record (groups 70, 280, 281, 282, 283), as read.
    #[serde(default)]
    pub display: WipeoutDisplay,
    /// The source image frame and clip of a canonical wipeout (see [`WipeoutFrame`]). When it is set the wipeout is
    /// `CanonicalV2`, `boundary` is empty and [`Wipeout::world_boundary`] gives the outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<WipeoutFrame>,
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
