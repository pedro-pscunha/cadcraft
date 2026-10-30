use super::{Deserialize, Serialize, Vec2, Vec3, one, standard};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
    Aligned,
    Middle,
    Fit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    pub insert: Vec3,
    #[serde(default)]
    pub align_pt: Option<Vec3>,
    pub height: f64,
    pub value: String,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "one")]
    pub width_factor: f64,
    #[serde(default)]
    pub oblique: f64,
    #[serde(default = "standard")]
    pub style: String,
    #[serde(default)]
    pub halign: HAlign,
    #[serde(default)]
    pub valign: VAlign,
    /// Backward text (DXF generation flag 2 of group 71): the glyphs run right to left. Only canonical text keeps the
    /// source's reflection; legacy text reads it as unreflected.
    #[serde(default)]
    pub backward: bool,
    /// Upside-down text (generation flag 4 of group 71).
    #[serde(default)]
    pub upside_down: bool,
}

impl Text {
    /// A key of what fixes the baseline start of justified text relative to its alignment point: the value, style, size, slant,
    /// width factor, justification and reflection, and not the place or the rotation.
    pub fn layout_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.value.hash(&mut h);
        self.style.hash(&mut h);
        for v in [self.height, self.width_factor, self.oblique] {
            v.to_bits().hash(&mut h);
        }
        (self.halign, self.valign, self.backward, self.upside_down).hash(&mut h);
        h.finish()
    }

    /// The point the text is placed by, as the Insertion snap and the grip show it: the alignment
    /// point (DXF group 11) of justified text, the start point (group 10) of left-baseline,
    /// aligned and fit text.
    pub fn justify_point(&self) -> Vec2 {
        match (self.halign, self.valign) {
            (HAlign::Left, VAlign::Baseline) | (HAlign::Aligned | HAlign::Fit, _) => self.insert.xy(),
            _ => self.align_pt.unwrap_or(self.insert).xy(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MText {
    pub insert: Vec3,
    pub height: f64,
    /// Reference rectangle width (0 = no wrapping).
    #[serde(default)]
    pub width: f64,
    /// 1..=9: TL TC TR ML MC MR BL BC BR.
    #[serde(default = "attach_tl")]
    pub attach: u8,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "standard")]
    pub style: String,
    /// Raw MTEXT contents with inline codes (\P, {\fArial|b1;...}, \S1/2; …).
    pub contents: String,
    #[serde(default = "one")]
    pub line_spacing: f64,
    /// Line spacing style (DXF 73): Exactly (`true`) keeps `line_spacing` whatever the character
    /// heights; At least (`false`, the default) lets taller characters push lines apart.
    #[serde(default)]
    pub line_spacing_exact: bool,
    /// Columns the text flows through, each `width` wide (DXF 75/76/78/79/48/49/50); `None`:
    /// one column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<MTextColumns>,
    /// The full direction of the text's X axis in WCS (DXF groups 11, 21, 31), kept by canonical text: `rotation` is
    /// then only its angle in the plane. `None` for legacy text, whose direction is `rotation` alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Vec3>,
    /// The drawing direction of the text (DXF 72) as a file gave it: 1 left to right, 3 top to bottom, 5 by style. `None` for
    /// text made here, which is written left to right (an attribute's embedded text: by style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing_direction: Option<u8>,
}

impl MText {
    /// A key of what the laid-out extents depend on: the contents, style, height, width, line spacing and
    /// the columns (not their extents), and not the place or the rotation.
    pub fn layout_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.contents.hash(&mut h);
        self.style.hash(&mut h);
        for v in [self.height, self.width, self.line_spacing] {
            v.to_bits().hash(&mut h);
        }
        (self.attach, self.line_spacing_exact).hash(&mut h);
        if let Some(c) = &self.columns {
            (c.kind as u8, c.count, c.auto_height, c.flow_reversed).hash(&mut h);
            for v in std::iter::once(&c.gutter).chain(std::iter::once(&c.height)).chain(&c.heights) {
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }

    /// Keep `width` and `height` as the laid-out extents of this text as it is now.
    pub fn set_column_extents(&mut self, width: f64, height: f64) {
        let key = self.layout_key();
        if let Some(c) = &mut self.columns {
            c.extents = Some(ColumnExtents { width, height, key });
        }
    }

    /// The laid-out extents a file gave, while the text is as it was then; else zeros (unknown).
    pub fn column_extents(&self) -> (f64, f64) {
        self.columns.as_ref().and_then(|c| c.extents).filter(|e| e.key == self.layout_key()).map_or((0.0, 0.0), |e| (e.width, e.height))
    }
}

fn attach_tl() -> u8 {
    1
}

/// Static columns: a fixed number of columns of one height. Dynamic columns: as many columns
/// as the text needs, of one height (auto height) or each its own (manual heights).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MTextColumnType {
    #[default]
    Static,
    Dynamic,
}

/// MTEXT columns (MTEXT Columns option, DXF Reference MTEXT groups 75–79, 48–50).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MTextColumns {
    #[serde(rename = "type")]
    pub kind: MTextColumnType,
    /// Number of columns (static columns: the columns there are).
    pub count: u16,
    /// Space between two columns.
    pub gutter: f64,
    /// Height of every column (static, and dynamic with auto height).
    pub height: f64,
    /// Dynamic columns: one height for all columns (`height`) instead of `heights`.
    pub auto_height: bool,
    /// Dynamic columns with manual heights: the height of each column.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub heights: Vec<f64>,
    /// Columns run right to left.
    pub flow_reversed: bool,
    /// The extents of the laid-out text (DXF embedded object groups 42 and 43) as a file gave them, with
    /// the key of the text they were measured for. CADCraft does not lay text out the way AutoCAD
    /// does: the values are written back only while the text, its size and its columns are as read.
    #[serde(skip)]
    pub extents: Option<ColumnExtents>,
    /// Dynamic columns with manual heights: which `heights` the file gave as the negative of the text's extents height (AutoCAD's way of
    /// saying the column has no height of its own; the height here is 0). Empty when none was. Written back negated, as AutoCAD's DWG has it.
    #[serde(skip)]
    pub negated_heights: Vec<bool>,
}

/// The laid-out extents of a column text and the [`MText::layout_key`] they belong to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColumnExtents {
    pub width: f64,
    pub height: f64,
    pub key: u64,
}

impl MTextColumns {
    /// The width of `count` columns of `column_width` and their gutters.
    pub fn total_width(&self, column_width: f64) -> f64 {
        let n = f64::from(self.count.max(1));
        column_width * n + self.gutter.max(0.0) * (n - 1.0)
    }

    /// The tallest column.
    pub fn max_height(&self) -> f64 {
        self.heights.iter().copied().fold(self.height, f64::max).max(0.0)
    }

    /// Scale every length (SCALE, block insertion).
    pub fn scale(&mut self, s: f64) {
        self.gutter *= s;
        self.height *= s;
        self.heights.iter_mut().for_each(|h| *h *= s);
    }
}
