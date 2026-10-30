use super::common::Handle;
use super::{Color, Deserialize, Serialize, Vec2, Vec3, standard, yes};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub center: Vec3,
    pub width: f64,
    pub height: f64,
    pub view_center: Vec2,
    pub view_height: f64,
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub frozen_layers: Vec<String>,
    /// Per-viewport layer colour overrides (VP Color in the Layer Properties Manager).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_colors: Vec<(String, Color)>,
    /// The paper-space entity whose outline clips the viewport (a closed polyline, circle,
    /// ellipse or spline; DXF group 340). `None`: the rectangle `center`/`width`/`height`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Handle>,
    /// Display on (MVIEW ON/OFF). An off viewport shows its border but no model space.
    #[serde(default = "yes")]
    pub on: bool,
    /// Source stacking status and display flags, before MVIEW edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_status: Option<ViewportStatus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportStatus {
    pub stacking: i64,
    pub flags: i64,
}

impl ViewportStatus {
    pub fn is_on(self) -> bool {
        self.stacking != 0 && self.flags & 131072 == 0
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            center: Vec3::ZERO,
            width: 1.0,
            height: 1.0,
            view_center: Vec2::ZERO,
            view_height: 1.0,
            id: 0,
            locked: false,
            frozen_layers: Vec::new(),
            layer_colors: Vec::new(),
            clip: None,
            on: true,
            source_status: None,
        }
    }
}

/// A named raster definition, retained even when no IMAGE entity uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageDefinition {
    pub name: String,
    pub path: String,
    pub size: Vec2,
    pub pixel_size: Vec2,
    pub units: u8,
    pub loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub insert: Vec3,
    /// One pixel's width (U) and height (V) vectors in drawing units.
    pub u: Vec3,
    pub v: Vec3,
    /// Image size in pixels.
    pub size: Vec2,
    /// The image file (its IMAGEDEF object's path).
    pub path: String,
    /// The image definition's name (its key in the drawing's image dictionary).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Clip boundary in pixel coordinates (origin at the top-left corner of the image, y down):
    /// two opposite corners of a rectangle, or a polygon's vertices. Empty: the whole image.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clip: Vec<Vec2>,
    /// The clip boundary is applied.
    #[serde(default)]
    pub clipping: bool,
    /// Display flags: 1 show the image, 2 show it when not aligned with the screen, 4 use the
    /// clip boundary, 8 transparency on.
    #[serde(default = "image_display")]
    pub display: u16,
    /// Brightness, contrast and fade, 0..=100.
    #[serde(default = "image_fifty")]
    pub brightness: u8,
    #[serde(default = "image_fifty")]
    pub contrast: u8,
    #[serde(default)]
    pub fade: u8,
    /// The definition's default size of one pixel and its resolution unit (0 none, 2 cm, 5 inch).
    #[serde(default = "image_pixel")]
    pub pixel_size: Vec2,
    #[serde(default)]
    pub resolution_units: u8,
}

fn image_display() -> u16 {
    7
}

fn image_fifty() -> u8 {
    50
}

fn image_pixel() -> Vec2 {
    Vec2::new(1.0, 1.0)
}

impl Default for Image {
    fn default() -> Self {
        Image {
            insert: Vec3::ZERO,
            u: Vec3::new(1.0, 0.0, 0.0),
            v: Vec3::new(0.0, 1.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            path: String::new(),
            name: String::new(),
            clip: Vec::new(),
            clipping: false,
            display: image_display(),
            brightness: image_fifty(),
            contrast: image_fifty(),
            fade: 0,
            pixel_size: image_pixel(),
            resolution_units: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub text: String,
    #[serde(default)]
    pub merged: Option<(u32, u32)>,
    /// The cell's own cell style; empty: its row's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style: String,
    /// A formula (`=Sum(A1:B3)`); `text` holds its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// The cell's own text height (DXF 140 in the cell record), before the table's scale;
    /// `None`: its cell style's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_height: Option<f64>,
    /// The cell flag value (group 172), as the file gave it.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub flag_value: i32,
    /// The cell flag (group 91 outside the cell's value block), as the file gave it.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub override_flag: i32,
    /// The cell fits its content automatically (group 174).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_fit: bool,
    /// The cell has a virtual edge (group 178).
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub virtual_edge: i32,
    /// The FIELD object the cell shows (DXF 344), by its handle in the file it came from: kept with the table. `text` is its cached
    /// value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Handle>,
    /// The block the cell holds instead of text (DXF cell type 171 = 2); `text` is then not drawn or written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<TableCellBlock>,
    /// The cell's own style groups (alignment 170, fill 283 and 63, text colour 64, text style 7, border colours, lineweights and
    /// visibilities 65 to 69, 275 to 279 and 285 to 289), as the file gave them: the values the cell flag (91) says are overridden.
    /// The text height (140) is `text_height`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_groups: Vec<super::RawTag>,
    /// The rotation of the cell's content (group 145), as the file gave it.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub rotation: f64,
}

/// The block a table cell holds: its name, scale (DXF 144) and attribute values, each by the handle of its ATTDEF in the block
/// definition (331) and the value (300).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCellBlock {
    pub block: String,
    pub scale: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribs: Vec<(Handle, String)>,
}

impl TableCellBlock {
    /// The reference that draws the block at `at`, turned by `rotation`, at the cell's block scale. Each attribute value is
    /// placed like its definition in the block (an attribute whose definition is gone is dropped).
    pub fn insert(&self, d: &crate::Drawing, at: Vec3, rotation: f64) -> super::Insert {
        let mut insert = super::Insert {
            block: self.block.clone(),
            insert: at,
            scale: Vec3::new(self.scale, self.scale, self.scale),
            rotation,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        };
        let Some(block) = d.block(&self.block) else { return insert };
        let matrix = super::Mat4::from_mat3(&insert.transform(block.base.xy()));
        for (handle, value) in &self.attribs {
            let Some(super::EntityKind::AttDef(def)) = block.entities.get(*handle).map(|e| &e.kind) else { continue };
            let mut placed = super::Entity::new(Handle(0), super::EntityKind::Text(def.text.clone()));
            if placed.try_transform(&matrix).is_err() {
                continue;
            }
            let super::EntityKind::Text(mut text) = placed.kind else { continue };
            text.value = value.clone();
            insert.attribs.push(super::Attrib { invisible: def.invisible, ..super::Attrib::new(def.tag.clone(), text) });
        }
        insert
    }
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// The table-wide groups of an ACAD_TABLE that the table model has no field for, as the file gave them: the value flags (90), the
/// style override masks (93), the border colour, lineweight and visibility override masks (94 to 96) and the override values that
/// follow them (280, 281, 70, 40, 41, 64 and so on up to the row heights).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableOverrides {
    /// Group 90 as the file gave it; `None` for a table made here, which gets the flags of its own state when it is written.
    #[serde(default)]
    pub value_flags: Option<i32>,
    pub style: i32,
    pub border_color: i32,
    pub border_lineweight: i32,
    pub border_visibility: i32,
    /// The override value groups after 96, with the text of each value.
    pub values: Vec<super::RawTag>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub insert: Vec3,
    /// The table's horizontal direction (DXF group 11, WCS). A canonical table keeps it whole; a legacy one is +X.
    #[serde(default = "table_x_axis", skip_serializing_if = "is_table_x_axis")]
    pub dir: Vec3,
    pub col_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    pub cells: Vec<Vec<TableCell>>,
    #[serde(default = "standard")]
    pub style: String,
    #[serde(default = "default_text_h")]
    pub text_height: f64,
    /// First row is a title (merged across, larger text).
    #[serde(default)]
    pub title: bool,
    /// The row after the title holds column headers.
    #[serde(default)]
    pub header: bool,
    /// The cell style of each row; missing or empty: by position (Title, Header, Data).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_styles: Vec<String>,
    /// Table breaking (TABLEBREAK): the table drawn in several parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaks: Option<crate::TableBreak>,
    /// The CSV file the cells come from (DATALINK).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_link: Option<crate::DataLink>,
    /// The table's override masks and values, as the file gave them.
    #[serde(default, skip_serializing_if = "TableOverrides::is_default")]
    pub overrides: TableOverrides,
}

impl TableOverrides {
    /// The value flags (group 90) AutoCAD writes for a table: 0x06 (it has a block), 0x10 (rows run down), and 0x20 when the title row is
    /// suppressed. A table with a title has 0x16 = 22, as AutoCAD LT 2025 writes for a -TABLE.
    pub fn default_value_flags(title: bool) -> i32 {
        0x06 | 0x10 | if title { 0 } else { 0x20 }
    }

    fn is_default(&self) -> bool {
        *self == TableOverrides::default()
    }
}

fn table_x_axis() -> Vec3 {
    Vec3::new(1.0, 0.0, 0.0)
}

fn is_table_x_axis(v: &Vec3) -> bool {
    *v == table_x_axis()
}

fn default_text_h() -> f64 {
    0.18
}
