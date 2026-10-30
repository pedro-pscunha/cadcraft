use super::common::Handle;
use super::{Deserialize, Serialize, Vec3, standard, yes};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DimKind {
    /// Rotated linear dimension (horizontal = 0, vertical = 90°).
    Linear {
        rotation: f64,
    },
    Aligned,
    Angular,
    Angular3P,
    Diameter,
    Radius,
    Ordinate {
        x_type: bool,
    },
    ArcLength,
}

impl DimKind {
    /// Public association slots in DIMASSOC flag order. Radial dimensions serialize their two
    /// model points through one circle reference; ArcLength is an aligned public dimension.
    pub fn association_points(self) -> &'static [&'static str] {
        match self {
            Self::Linear { .. } | Self::Aligned | Self::ArcLength => &["p13", "p14"],
            Self::Angular => &["p13", "p14", "defpt", "p15"],
            Self::Angular3P => &["p13", "p14", "p15"],
            Self::Radius | Self::Diameter => &["defpt", "p15"],
            Self::Ordinate { .. } => &["p13"],
        }
    }
}

/// A dimension. Definition points follow DXF: `defpt` (10: dimension line point),
/// `p13`/`p14` (extension origins), `p15`/`p16` (arc/angle points), `text_mid` (11).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub kind: DimKind,
    pub defpt: Vec3,
    pub text_mid: Vec3,
    #[serde(default)]
    pub p13: Vec3,
    #[serde(default)]
    pub p14: Vec3,
    #[serde(default)]
    pub p15: Vec3,
    #[serde(default)]
    pub p16: Vec3,
    /// "" = measured value, "<>" inside = measurement placeholder, " " = suppressed.
    #[serde(default)]
    pub text: String,
    #[serde(default = "standard")]
    pub style: String,
    #[serde(default)]
    pub measurement: f64,
    #[serde(default)]
    pub text_rotation: f64,
    /// User moved the text (text_mid is authoritative).
    #[serde(default)]
    pub user_text_pos: bool,
    /// The leader length of a radius or diameter dimension (DXF 40) as a file gave it; 0 for one made here.
    #[serde(default)]
    pub leader_length: f64,
    /// Anonymous block with the rendered geometry (from files); regenerated when edited.
    #[serde(default)]
    pub block: Option<String>,
    /// Per-dimension style overrides (DIMOVERRIDE): DimStyle field names (camelCase) → values,
    /// merged over the named style.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub overrides: serde_json::Map<String, serde_json::Value>,
    /// Associativity: which definition points follow which objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assoc: Vec<DimAssoc>,
    /// Oblique extension lines, jogs, breaks and inspection frames.
    #[serde(default, skip_serializing_if = "DimMarks::is_empty")]
    pub marks: DimMarks,
}

impl Dimension {
    /// Definition points by their DXF/model association slot name.
    pub fn definition_point(&self, name: &str) -> Option<Vec3> {
        Some(match name {
            "defpt" => self.defpt,
            "p13" => self.p13,
            "p14" => self.p14,
            "p15" => self.p15,
            "p16" => self.p16,
            "text" => self.text_mid,
            _ => return None,
        })
    }
}

/// What dimension edits add to a dimension's drawing: DIMEDIT Oblique, DIMJOGLINE, DIMJOGGED,
/// DIMBREAK and DIMINSPECT.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DimMarks {
    /// Angle (radians) of the extension lines of a linear or aligned dimension; None =
    /// perpendicular to the dimension line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oblique: Option<f64>,
    /// A jog on the dimension line of a linear or aligned dimension, at this point (DIMJOGLINE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jog_line: Option<Vec3>,
    /// A jogged radius dimension (DIMJOGGED).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jogged: Option<JoggedRadius>,
    /// Gaps in the dimension and extension lines (DIMBREAK).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<DimBreak>,
    /// Inspection frame, label and rate (DIMINSPECT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspect: Option<Inspection>,
}

impl DimMarks {
    pub fn is_empty(&self) -> bool {
        *self == DimMarks::default()
    }

    /// Transform the points and angles (part of [`super::EntityKind::transform`]).
    pub(super) fn transform(&mut self, t3: &dyn Fn(&mut Vec3), rot: f64, s: f64, mirror: bool) {
        if let Some(a) = &mut self.oblique {
            *a = cadcraft_geom::norm_angle(if mirror { rot - (*a - rot) } else { *a + rot });
        }
        if let Some(j) = &mut self.jog_line {
            t3(j);
        }
        if let Some(j) = &mut self.jogged {
            t3(&mut j.center);
            t3(&mut j.jog);
        }
        for b in &mut self.breaks {
            t3(&mut b.at);
            b.size *= s;
        }
    }
}

/// The definition of a jogged radius dimension: the dimension line starts at an overriding
/// centre and jogs on its way to the arc.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JoggedRadius {
    /// Where the dimension line starts instead of the true centre.
    pub center: Vec3,
    /// The middle of the jog.
    pub jog: Vec3,
}

/// A gap in a dimension's lines: everything within `size / 2` of `at` is left out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DimBreak {
    pub at: Vec3,
    pub size: f64,
    /// Made by DIMBREAK Auto (recomputed by the next Auto) rather than picked by hand.
    pub auto: bool,
}

/// An inspection dimension (DIMINSPECT).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Inspection {
    /// Frame shape: 0 round ends, 1 angular ends, 2 none.
    pub shape: u8,
    /// Text left of the value ("" = none).
    pub label: String,
    /// Inspection rate right of the value ("" = none).
    pub rate: String,
}

/// A dimension definition point that follows an object (associative dimensions).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DimAssoc {
    /// Definition point: "defpt", "p13", "p14", "p15" or "p16".
    pub point: String,
    /// The object it is attached to.
    pub handle: Handle,
    pub snap: AssocSnap,
}

/// Where on the object an associative definition point sits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssocSnap {
    /// Start point of a line/arc (or polyline vertex 0).
    Start,
    /// End point of a line/arc.
    End,
    Mid,
    /// Centre of a circle/arc.
    Center,
    /// On a circle/arc at a fixed angle (radians) from its centre.
    OnCircle {
        angle: f64,
    },
    /// Intersection of this (line) object with another line.
    Intersection {
        other: Handle,
    },
    /// Vertex `index` of a polyline.
    Vertex {
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leader {
    pub vertices: Vec<Vec3>,
    #[serde(default = "yes")]
    pub arrow: bool,
    #[serde(default)]
    pub spline: bool,
    #[serde(default = "standard")]
    pub style: String,
    /// The direction, offsets and sizes of a canonical leader as its record holds them (see [`LeaderFrame`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<LeaderFrame>,
    /// The creation type, hookline settings and annotation link of the record, as a file gave them.
    #[serde(default, skip_serializing_if = "LeaderLinks::is_empty")]
    pub links: LeaderLinks,
}

/// Groups of a LEADER that no other field holds: the annotation type (73: 0 text, 1 tolerance, 2 block reference, 3 none), the
/// hookline direction (74: 0 opposite to the horizontal direction, 1 same) and flag (75), and the annotation handle (340). All
/// `None` for a leader made here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderLinks {
    pub creation: Option<i64>,
    pub hookline_direction: Option<i64>,
    pub hookline: Option<i64>,
    pub annotation: Option<super::Handle>,
}

impl LeaderLinks {
    pub fn is_empty(&self) -> bool {
        *self == LeaderLinks::default()
    }
}

/// What a canonical LEADER holds beside its WCS vertices and the normal of its plane (`Common.extrusion`): the
/// horizontal direction (DXF group 211), the offsets of the last vertex from the block reference (212) and from the
/// annotation placement (213), and the annotation text height (40) and width (41).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderFrame {
    pub direction: Vec3,
    pub block_offset: Vec3,
    pub annotation_offset: Vec3,
    pub text_height: f64,
    pub text_width: f64,
}

impl Default for Leader {
    fn default() -> Self {
        Leader { vertices: Vec::new(), arrow: true, spline: false, style: "Standard".into(), frame: None, links: Default::default() }
    }
}
