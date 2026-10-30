//! The multileader (DXF MULTILEADER): its leader roots with their landings and doglegs, the leader lines
//! (branches) of each root, and the context that places the content.
//!
//! A root (`LEADER{` in the file) keeps its own landing point, dogleg direction and length, so a
//! multileader with two roots keeps two landings through an edit of one of its lines. Roots and
//! branches (`LEADER_LINE{`) keep the numbers the file gave them (groups 90 and 91); the ones a
//! command makes get the next free number.

use super::block::Insert;
use super::text::MText;
use super::{Deserialize, GSpline, Serialize, Vec2, Vec3, one, standard};

/// A leader line of a multileader: its vertices from the arrowhead, ending where the root's landing begins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeaderBranch {
    /// The branch number (group 91).
    pub id: i32,
    pub vertices: Vec<Vec3>,
}

/// A leader root of a multileader: a landing with its dogleg and the leader lines that end at it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeaderRoot {
    /// The root number (group 90).
    pub id: i32,
    /// The landing point (group 10 of the root).
    pub landing: Vec3,
    /// The direction of the dogleg from the landing (group 11 of the root).
    pub dogleg_dir: Vec3,
    /// The length of the dogleg (group 40 of the root); 0 for none.
    pub dogleg: f64,
    pub branches: Vec<MLeaderBranch>,
}

impl MLeaderRoot {
    /// A root at `landing` whose dogleg runs along +x (`dir` -1 for -x) with the lines `branches`.
    pub fn new(landing: Vec3, dogleg: f64, left: bool, lines: Vec<Vec<Vec3>>) -> Self {
        let branches =
            lines.into_iter().enumerate().map(|(n, vertices)| MLeaderBranch { id: i32::try_from(n).unwrap_or(i32::MAX), vertices }).collect();
        MLeaderRoot { id: 0, landing, dogleg_dir: Vec3::new(if left { -1.0 } else { 1.0 }, 0.0, 0.0), dogleg, branches }
    }

    /// The point the dogleg ends at, where the content's text or block attaches.
    pub fn dogleg_end(&self) -> Vec3 {
        self.landing + self.dogleg_dir * self.dogleg
    }
}

/// A set of numeric groups of a MULTILEADER record that CADCraft does not interpret: each is the value the file gave
/// (`None`: the writer emits the default of a multileader made in CADCraft). The DXF group code is the one in the
/// field's name.
macro_rules! numeric_groups {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $field:ident = $code:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $($(#[$fmeta])* #[serde(default)] pub $field: Option<f64>,)*
        }

        impl $name {
            /// Keep the value `v` of group `code` if it is one of this set's; whether it is.
            pub fn set(&mut self, code: i32, v: f64) -> bool {
                match code {
                    $($code => self.$field = Some(v),)*
                    _ => return false,
                }
                true
            }
        }
    };
}

numeric_groups! {
    /// The groups of the content context (`CONTEXT_DATA{`) besides the content, frame and attachment ones the model has.
    MLeaderContextGroups {
        /// Content scale (40).
        scale = 40,
        /// Text height (41), the landing gap (145), the text boundary height (44).
        text_height = 41,
        landing_gap = 145,
        column_height = 44,
        /// Text flow direction (172), text colour (90), background fill colour (91), scale factor (141), transparency (92),
        /// fill switch (291) and mask fill (292).
        flow_direction = 172,
        text_color = 90,
        background_color = 91,
        background_scale = 141,
        background_transparency = 92,
        background_fill = 291,
        mask_fill = 292,
        /// Column type (173), automatic text height (293), column width (142) and gutter (143), flow reversed (294), word break (295).
        column_type = 173,
        height_automatic = 293,
        column_width = 142,
        column_gutter = 143,
        column_flow_reversed = 294,
        word_break = 295,
        /// Block content colour (93), normal reversed (297), top and bottom attachment (272, 273).
        block_color = 93,
        normal_reversed = 297,
        attach_top = 272,
        attach_bottom = 273,
    }
}

numeric_groups! {
    /// The groups of the entity itself besides its geometry and attachment codes: the property-override flags (90), colours (91,
    /// 92, 93), lineweight (171), landing and dogleg switches (290, 291), text frame (292), annotation scale switch (293), text
    /// direction negative (294), alignment in the in-place editor (178), attachment point (179), scale factor (45) and the
    /// attachment direction and top and bottom attachments (271, 272, 273); the block rotation of the entity (43), which AutoCAD
    /// leaves as it was when the multileader is rotated (the rotation lives in the context's 46), and the group 295 that ends an
    /// AutoCAD 2018 entity.
    MLeaderEntityGroups {
        override_flags = 90,
        line_color = 91,
        line_weight = 171,
        landing_enabled = 290,
        dogleg_enabled = 291,
        text_color = 92,
        text_frame = 292,
        block_color = 93,
        annotation_scale = 293,
        text_direction_negative = 294,
        text_align_in_editor = 178,
        attachment_point = 179,
        scale = 45,
        attach_direction = 271,
        attach_top = 272,
        attach_bottom = 273,
        block_rotation = 43,
        trailer = 295,
    }
}

numeric_groups! {
    /// The groups of a leader root (`LEADER{`) besides its landing, direction, number and dogleg length: the last-point switch
    /// (290), the dogleg switch (291) and the attachment direction (271). The first root's values stand for every root.
    MLeaderRootGroups {
        last_point_set = 290,
        dogleg_set = 291,
        attach_direction = 271,
    }
}

/// The attachment and connection codes of a multileader's content as its file gave them (DXF groups 174 to 177 of the content
/// context, and 173, 95, 174, 175 and 176 of the entity). They say how AutoCAD attached the text to the landing and connected the
/// block; the writer derives what a code is `None` for from the text's attachment point. The codes of the context and of the entity
/// that depend on the text's attachment apply while the text still has the attachment point they were read with
/// (`derived_attach`); the block connection codes always apply.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeaderCodes {
    /// The text's attachment point (1..=9) when the codes were read.
    pub derived_attach: Option<u8>,
    /// Text left attachment, right attachment and alignment of the context (174, 175, 176).
    pub ctx_left: Option<i64>,
    pub ctx_right: Option<i64>,
    pub ctx_alignment: Option<i64>,
    /// Block content connection type of the context (177): 0 centre extents, 1 insertion point.
    pub ctx_connection: Option<i64>,
    /// Text left and right attachment (173, 95), text angle type (174) and alignment (175) of the entity.
    pub left: Option<i64>,
    pub right: Option<i64>,
    pub angle_type: Option<i64>,
    pub alignment: Option<i64>,
    /// Block attachment type of the entity (176): 0 centre extents, 1 insertion point.
    pub block_attachment: Option<i64>,
    /// The other numeric groups of the context, the entity and its first root (boxed: the multileader is one of the larger
    /// entity kinds).
    #[serde(default)]
    pub groups: Box<MLeaderGroups>,
}

/// The numeric groups of the content context, of the entity and of its first root that the model keeps as read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeaderGroups {
    pub ctx: MLeaderContextGroups,
    pub entity: MLeaderEntityGroups,
    pub root: MLeaderRootGroups,
}

/// The frame of a multileader's content (the `CONTEXT_DATA{` groups): the basis of the context and the
/// directions of its text and block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeaderContext {
    /// Origin of the context's basis (group 110).
    pub origin: Vec3,
    /// Its X and Y axes (groups 111 and 112).
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    /// The normal of the text's plane (group 11 of the context).
    pub text_normal: Vec3,
    /// The direction of the text (group 13); its angle in the plane is the text's rotation.
    pub text_dir: Vec3,
    /// The normal of the block content's plane (group 14).
    pub block_normal: Vec3,
    /// What the content's placement (text and block) means: the projected 2D one of earlier drawings, or the plane's.
    #[serde(default)]
    pub geometry_revision: super::GeometryRevision,
    /// How the content attaches and connects, as the file gave it.
    #[serde(default)]
    pub codes: MLeaderCodes,
}

impl Default for MLeaderContext {
    fn default() -> Self {
        MLeaderContext {
            origin: Vec3::ZERO,
            x_axis: Vec3::new(1.0, 0.0, 0.0),
            y_axis: Vec3::new(0.0, 1.0, 0.0),
            text_normal: Vec3::Z,
            text_dir: Vec3::new(1.0, 0.0, 0.0),
            block_normal: Vec3::Z,
            geometry_revision: super::GeometryRevision::LegacyProjectedV1,
            codes: MLeaderCodes::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MLeader {
    pub roots: Vec<MLeaderRoot>,
    pub text: Option<MText>,
    pub style: String,
    pub arrow_size: f64,
    /// Leader lines are smooth curves through their points (leader type Spline). A multileader whose roots have
    /// no branches has no leader line (leader type None).
    pub spline: bool,
    /// Block content in place of the text: a block reference placed at the landing.
    pub block: Option<Insert>,
    /// The arrowhead (DXF group 342): a standard name such as `_None` or a block name; empty =
    /// the style's.
    pub arrow_block: String,
    pub context: MLeaderContext,
}

/// The fields of a multileader as saved, including those of the layout before roots: `leaders`, `landing`
/// and `dogleg` become one root.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MLeaderRepr {
    #[serde(default)]
    roots: Vec<MLeaderRoot>,
    #[serde(default)]
    leaders: Vec<Vec<Vec3>>,
    #[serde(default)]
    landing: Vec3,
    #[serde(default)]
    dogleg: f64,
    text: Option<MText>,
    #[serde(default = "standard")]
    style: String,
    #[serde(default = "one")]
    arrow_size: f64,
    #[serde(default)]
    spline: bool,
    #[serde(default)]
    block: Option<Insert>,
    #[serde(default)]
    arrow_block: String,
    #[serde(default)]
    context: MLeaderContext,
}

impl<'de> Deserialize<'de> for MLeader {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let r = MLeaderRepr::deserialize(deserializer)?;
        let roots = if r.roots.is_empty() {
            let left = r.text.as_ref().map(|t| t.insert).or(r.block.as_ref().map(|b| b.insert)).is_some_and(|c| c.x < r.landing.x);
            vec![MLeaderRoot::new(r.landing, r.dogleg, left, r.leaders)]
        } else {
            r.roots
        };
        Ok(MLeader {
            roots,
            text: r.text,
            style: r.style,
            arrow_size: r.arrow_size,
            spline: r.spline,
            block: r.block,
            arrow_block: r.arrow_block,
            context: r.context,
        })
    }
}

impl MLeader {
    /// A multileader with one root at `landing`.
    pub fn single(landing: Vec3, dogleg: f64, left: bool, lines: Vec<Vec<Vec3>>, text: Option<MText>) -> Self {
        MLeader {
            roots: vec![MLeaderRoot::new(landing, dogleg, left, lines)],
            text,
            style: standard(),
            arrow_size: one(),
            spline: false,
            block: None,
            arrow_block: String::new(),
            context: MLeaderContext { origin: Vec3::new(0.0, 0.0, landing.z), ..MLeaderContext::default() },
        }
    }

    /// The landing of the first root: where the content attaches when there is one root.
    pub fn landing(&self) -> Vec3 {
        self.roots.first().map_or(Vec3::ZERO, |r| r.landing)
    }

    /// The dogleg length of the first root.
    pub fn dogleg(&self) -> f64 {
        self.roots.first().map_or(0.0, |r| r.dogleg)
    }

    /// Every leader line with its root, in file order.
    pub fn lines(&self) -> impl Iterator<Item = (&MLeaderRoot, &MLeaderBranch)> {
        self.roots.iter().flat_map(|r| r.branches.iter().map(move |b| (r, b)))
    }

    /// The number of leader lines over all roots.
    pub fn line_count(&self) -> usize {
        self.roots.iter().map(|r| r.branches.len()).sum()
    }

    /// Each leader line as drawn: through its points to its root's landing, smoothed for spline leaders.
    pub fn leader_paths(&self) -> Vec<Vec<Vec2>> {
        self.lines()
            .map(|(r, b)| {
                let pts: Vec<Vec2> = b.vertices.iter().map(|v| v.xy()).chain(std::iter::once(r.landing.xy())).collect();
                if self.spline && pts.len() > 2 { GSpline::from_fit_points(&pts).tessellate(1e-3) } else { pts }
            })
            .collect()
    }

    /// The branch number a new leader line gets.
    pub fn next_branch_id(&self) -> i32 {
        self.lines().map(|(_, b)| b.id).max().map_or(0, |m| m.saturating_add(1))
    }

    /// Every point the multileader's lines and landings are made of, to be moved with it.
    pub fn points_mut(&mut self) -> impl Iterator<Item = &mut Vec3> {
        self.roots.iter_mut().flat_map(|r| r.branches.iter_mut().flat_map(|b| b.vertices.iter_mut()).chain(std::iter::once(&mut r.landing)))
    }
}
