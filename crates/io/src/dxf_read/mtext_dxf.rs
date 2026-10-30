//! MTEXT specifics of DXF (DXF Reference, MTEXT): control characters in the contents written
//! as a caret and a letter (`^I` a tab, `^J` a line feed, `^ ` a caret), and the column groups
//! 75 (type), 76 (count), 78 (flow reversed), 79 (auto height), 48 (width), 49 (gutter) and
//! 50 (column heights, after the column type; before it, 50 is the rotation).

use cadcraft_doc::{MText, MTextColumnType, MTextColumns};
use cadcraft_dxf::Tag;

/// Upper bound on column heights read from a file.
const MAX_COLUMNS: usize = 1_000;

/// Decode the contents' `^I` (tab), `^J` (line feed) and `^ ` (caret). Other carets stay as
/// they are (older files write stacked tolerances `\S+0.1^-0.1;` with a bare caret).
pub(crate) fn decode_contents(s: &str) -> String {
    if !s.contains('^') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some(' ') => out.push('^'),
            Some('I') => out.push('\t'),
            Some('J') => out.push('\n'),
            _ => {
                out.push('^');
                continue;
            }
        }
        it.next();
    }
    out
}

/// Encode the contents for a DXF string: a tab as `^I`, a line feed as `^J`, a caret as `^ `;
/// other control characters (which draw nothing) are dropped.
pub(crate) fn encode_contents(s: &str) -> String {
    if !s.chars().any(|c| c == '^' || c.is_control()) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '^' => out.push_str("^ "),
            '\t' => out.push_str("^I"),
            '\n' => out.push_str("^J"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The MTEXT's own tags: up to an embedded object (group 101), whose groups reuse MTEXT's codes.
fn own(tags: &[Tag]) -> &[Tag] {
    let end = tags.iter().position(|t| t.code == 101).unwrap_or(tags.len());
    tags.get(..end).unwrap_or(tags)
}

/// The tags before the column groups (where group 50 is the rotation angle).
pub(crate) fn before_columns(tags: &[Tag]) -> &[Tag] {
    let tags = own(tags);
    let end = tags.iter().position(|t| t.code == 75).unwrap_or(tags.len());
    tags.get(..end).unwrap_or(tags)
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// The columns of an MTEXT, and the column width (group 48) when there are columns.
pub(crate) fn read_columns(tags: &[Tag]) -> Option<(MTextColumns, f64)> {
    let tags = own(tags);
    let start = tags.iter().position(|t| t.code == 75)?;
    let kind = match tags.get(start).map(Tag::i64) {
        Some(1) => MTextColumnType::Static,
        Some(2) => MTextColumnType::Dynamic,
        _ => return None,
    };
    let cols = tags.get(start..).unwrap_or_default();
    let first = |code: i32| cols.iter().find(|t| t.code == code);
    let heights: Vec<f64> = cols.iter().filter(|t| t.code == 50).take(MAX_COLUMNS).map(|t| finite(t.f64()).max(0.0)).collect();
    let count = first(76).map(Tag::i64).unwrap_or(heights.len() as i64).clamp(1, MAX_COLUMNS as i64);
    let auto_height = first(79).map(Tag::i64).unwrap_or(0) != 0;
    let defined = tags.iter().find(|t| t.code == 46).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0);
    let manual = kind == MTextColumnType::Dynamic && !auto_height;
    let height = if manual { heights.iter().copied().fold(0.0, f64::max) } else { heights.first().copied().filter(|h| *h > 0.0).unwrap_or(defined) };
    let c = MTextColumns {
        kind,
        count: u16::try_from(count).unwrap_or(1),
        gutter: first(49).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0),
        height,
        auto_height,
        heights: if manual { heights } else { Vec::new() },
        flow_reversed: first(78).map(Tag::i64).unwrap_or(0) != 0,
        extents: None,
        negated_heights: Vec::new(),
    };
    Some((c, first(48).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0)))
}

/// The columns of an R2018+ MTEXT, from its embedded object (group 101 and after): the column type
/// (71), count (72), width (44), gutter (45), auto height (73), flow (74), and a height (46) per column
/// of manual columns; the rectangle width (40) and height (41). Returns them with the column width.
pub(crate) fn read_embedded_columns(tags: &[Tag]) -> Option<(MTextColumns, f64)> {
    let at = tags.iter().position(|t| t.code == 101)?;
    let block: Vec<&Tag> = tags.get(at + 1..)?.iter().take_while(|t| t.code < 1000 && t.code != 0 && t.code != 100).collect();
    let first = |code: i32| block.iter().find(|t| t.code == code).copied();
    let kind = match first(71).map(|t| t.i64()) {
        Some(1) => MTextColumnType::Static,
        Some(2) => MTextColumnType::Dynamic,
        _ => return None,
    };
    let auto_height = first(73).map(|t| t.i64()).unwrap_or(0) != 0;
    let manual = kind == MTextColumnType::Dynamic && !auto_height;
    let heights: Vec<f64> = block.iter().filter(|t| t.code == 46).take(MAX_COLUMNS).map(|t| finite(t.f64()).max(0.0)).collect();
    let negated: Vec<bool> = block.iter().filter(|t| t.code == 46).take(MAX_COLUMNS).map(|t| finite(t.f64()) < 0.0).collect();
    let count = first(72).map(|t| t.i64()).unwrap_or(heights.len() as i64).clamp(1, MAX_COLUMNS as i64);
    let rect = first(41).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0);
    let height = if manual { heights.iter().copied().fold(0.0, f64::max) } else { rect };
    let c = MTextColumns {
        kind,
        count: u16::try_from(count).unwrap_or(1),
        gutter: first(45).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0),
        height,
        auto_height,
        heights: if manual { heights } else { Vec::new() },
        flow_reversed: first(74).map(|t| t.i64()).unwrap_or(0) != 0,
        extents: None,
        negated_heights: if manual && negated.contains(&true) { negated } else { Vec::new() },
    };
    Some((c, first(44).map(|t| finite(t.f64()).max(0.0)).unwrap_or(0.0)))
}

/// The laid-out extents (42, 43) of the embedded object of an R2018+ MTEXT.
pub(crate) fn read_embedded_extents(tags: &[Tag]) -> Option<(f64, f64)> {
    let at = tags.iter().position(|t| t.code == 101)?;
    let block: Vec<&Tag> = tags.get(at + 1..)?.iter().take_while(|t| t.code < 1000 && t.code != 0 && t.code != 100).collect();
    let get = |code: i32| block.iter().find(|t| t.code == code).map(|t| finite(t.f64()).max(0.0));
    Some((get(42)?, get(43)?))
}

/// The ACAD application's extended data of an entity record (from its `1001 ACAD` to the next application).
pub(crate) fn acad_xdata(tags: &[Tag]) -> Option<&[Tag]> {
    let start = tags.iter().position(|t| t.code == 1001 && t.str().trim().eq_ignore_ascii_case("ACAD"))?;
    let rest = tags.get(start..)?;
    let end = rest.iter().skip(1).position(|t| t.code == 1001).map_or(rest.len(), |p| p + 1);
    rest.get(..end)
}

/// The ACAD extended data AutoCAD 2007 to 2013 keeps the columns in (`ACAD_MTEXT_COLUMN_INFO_BEGIN`
/// ... `_END`): 75 type, 79 auto height, 76 count, 78 flow reversed, 48 column width, 49 gutter,
/// 50 and the number of column heights, then the heights. The record must be the application's only
/// content; returns the columns and the column width.
pub(crate) fn read_xdata_columns(xdata: &[Tag]) -> Option<(MTextColumns, f64)> {
    let values = xdata_column_values(xdata)?;
    let mut it = values.iter();
    let (mut kind, mut auto_height, mut count, mut flow, mut width, mut gutter) = (0i64, false, 1i64, false, 0.0, 0.0);
    let mut heights = Vec::new();
    while let Some(code) = it.next() {
        let value = it.next()?;
        match code.i64() {
            75 => kind = value.i64(),
            79 => auto_height = value.i64() != 0,
            76 => count = value.i64(),
            78 => flow = value.i64() != 0,
            48 => width = finite(value.f64()).max(0.0),
            49 => gutter = finite(value.f64()).max(0.0),
            50 => {
                let n = usize::try_from(value.i64()).ok()?.min(MAX_COLUMNS);
                for _ in 0..n {
                    heights.push(finite(it.next()?.f64()).max(0.0));
                }
            }
            _ => return None,
        }
    }
    let kind = match kind {
        1 => MTextColumnType::Static,
        2 => MTextColumnType::Dynamic,
        _ => return None,
    };
    let manual = kind == MTextColumnType::Dynamic && !auto_height;
    let height = if manual { heights.iter().copied().fold(0.0, f64::max) } else { heights.first().copied().unwrap_or(0.0) };
    let count = count.clamp(1, MAX_COLUMNS as i64);
    let c = MTextColumns {
        kind,
        count: u16::try_from(count).unwrap_or(1),
        gutter,
        height,
        auto_height,
        heights: if manual { heights } else { Vec::new() },
        flow_reversed: flow,
        extents: None,
        negated_heights: Vec::new(),
    };
    Some((c, width))
}

/// Where the column record of an ACAD xdata chunk (`1001 ACAD` and its groups) is: the indexes of its
/// `ACAD_MTEXT_COLUMN_INFO_BEGIN` and of the `_END` that follows, when only numbers are between them.
pub(crate) fn column_record<S: AsRef<[Tag]>>(chunk: S) -> Option<(usize, usize)> {
    let chunk = chunk.as_ref();
    chunk.first().filter(|t| t.code == 1001 && t.str().trim().eq_ignore_ascii_case("ACAD"))?;
    let marker = |t: &Tag, name: &str| t.code == 1000 && t.str().trim() == name;
    let begin = chunk.iter().position(|t| marker(t, "ACAD_MTEXT_COLUMN_INFO_BEGIN"))?;
    let end = begin + 1 + chunk.get(begin + 1..)?.iter().position(|t| marker(t, "ACAD_MTEXT_COLUMN_INFO_END"))?;
    chunk.get(begin + 1..end)?.iter().all(|t| matches!(t.code, 1070 | 1040)).then_some((begin, end))
}

/// The (code, value) groups of the column record of the entity's ACAD xdata, between its markers.
fn xdata_column_values(xdata: &[Tag]) -> Option<Vec<&Tag>> {
    let (begin, end) = column_record(xdata)?;
    Some(xdata.get(begin + 1..end)?.iter().collect())
}

/// The ACAD xdata record of an MTEXT's columns for a file of 2007 to 2017, as AutoCAD writes it.
pub(crate) fn xdata_column_tags(t: &MText) -> Vec<Tag> {
    let Some(c) = &t.columns else { return Vec::new() };
    let manual = c.kind == MTextColumnType::Dynamic && !c.auto_height;
    let count = if manual { c.heights.len().max(1) } else { usize::from(c.count.max(1)) };
    let mut v = vec![
        Tag::s(1001, "ACAD"),
        Tag::s(1000, "ACAD_MTEXT_COLUMN_INFO_BEGIN"),
        Tag::i(1070, 75),
        Tag::i(1070, if c.kind == MTextColumnType::Dynamic { 2 } else { 1 }),
        Tag::i(1070, 79),
        Tag::i(1070, i64::from(c.auto_height)),
        Tag::i(1070, 76),
        Tag::i(1070, count as i64),
        Tag::i(1070, 78),
        Tag::i(1070, i64::from(c.flow_reversed)),
        Tag::i(1070, 48),
        Tag::f(1040, t.width),
        Tag::i(1070, 49),
        Tag::f(1040, c.gutter),
        Tag::i(1070, 50),
        Tag::i(1070, if manual { c.heights.len() as i64 } else { 0 }),
    ];
    if manual {
        v.extend(c.heights.iter().map(|h| Tag::f(1040, *h)));
    }
    v.push(Tag::s(1000, "ACAD_MTEXT_COLUMN_INFO_END"));
    v
}

/// The whole ACAD xdata chunk of an MTEXT with columns for a file of 2007 to 2017: the model's column record
/// in the place the entity's other ACAD data left for it (`kept`, the entity's foreign xdata: an empty record
/// marks the place), else before the rest of that data.
pub(crate) fn xdata_column_chunk(t: &MText, kept: &[cadcraft_doc::RawTag]) -> Vec<Tag> {
    let record = xdata_column_tags(t);
    let Some(start) = kept.iter().position(|r| r.code == 1001 && r.value.trim().eq_ignore_ascii_case("ACAD")) else { return record };
    let chunk =
        kept.get(start..).map(|k| k.iter().skip(1).position(|r| r.code == 1001).map_or(k.len(), |p| p + 1)).and_then(|n| kept.get(start..start + n));
    let rest: Vec<Tag> = chunk.unwrap_or_default().iter().skip(1).map(crate::dxf_raw::tag).collect();
    // The record's own groups (between the application name and the end of the record).
    let inner = record.get(1..).unwrap_or_default();
    let at = rest.windows(2).position(|w| {
        w[0].code == 1000
            && w[0].str().trim() == "ACAD_MTEXT_COLUMN_INFO_BEGIN"
            && w[1].code == 1000
            && w[1].str().trim() == "ACAD_MTEXT_COLUMN_INFO_END"
    });
    let mut out = vec![Tag::s(1001, "ACAD")];
    match at {
        Some(i) => {
            out.extend(rest.get(..i).unwrap_or_default().iter().cloned());
            out.extend(inner.iter().cloned());
            out.extend(rest.get(i + 2..).unwrap_or_default().iter().cloned());
        }
        None => {
            out.extend(inner.iter().cloned());
            out.extend(rest);
        }
    }
    out
}

/// The embedded object (group 101) of an R2018+ MTEXT with columns, as AutoCAD writes it: the
/// attachment, X-axis direction, insertion point, rectangle size, laid-out extents (kept from the file while the text is as read) and the columns.
/// `negated`: the heights the file gave as the negative of the extents height are written so (the DWG bridge; AutoCAD's DXF has 0 there).
pub(crate) fn embedded_tags(t: &MText, negated: bool) -> Vec<Tag> {
    let Some(c) = &t.columns else { return Vec::new() };
    let manual = c.kind == MTextColumnType::Dynamic && !c.auto_height;
    let count = if manual { c.heights.len().max(1) } else { usize::from(c.count.max(1)) };
    let mut v = vec![
        Tag::s(101, "Embedded Object"),
        Tag::i(70, i64::from(t.attach)),
        // The X axis: a canonical text's full direction, else its rotation in the XY plane.
        Tag::f(10, t.direction.map_or(t.rotation.cos(), |d| d.x)),
        Tag::f(20, t.direction.map_or(t.rotation.sin(), |d| d.y)),
        Tag::f(30, t.direction.map_or(0.0, |d| d.z)),
        Tag::f(11, t.insert.x),
        Tag::f(21, t.insert.y),
        Tag::f(31, t.insert.z),
        Tag::f(40, c.total_width(t.width)),
        Tag::f(41, if manual { 0.0 } else { c.height }),
        Tag::f(42, t.column_extents().0),
        Tag::f(43, t.column_extents().1),
        Tag::i(71, if c.kind == MTextColumnType::Dynamic { 2 } else { 1 }),
        Tag::i(72, count as i64),
        Tag::f(44, t.width),
        Tag::f(45, c.gutter),
        Tag::i(73, i64::from(c.auto_height)),
        Tag::i(74, i64::from(c.flow_reversed)),
    ];
    if manual {
        let extents_height = t.column_extents().1;
        v.extend(c.heights.iter().enumerate().map(|(i, h)| {
            let automatic = negated && *h == 0.0 && extents_height > 0.0 && c.negated_heights.get(i).copied().unwrap_or(false);
            Tag::f(46, if automatic { -extents_height } else { *h })
        }));
    }
    v
}

/// The column groups of an MTEXT with columns.
pub(crate) fn column_tags(t: &MText) -> Vec<Tag> {
    let Some(c) = &t.columns else { return Vec::new() };
    let manual = c.kind == MTextColumnType::Dynamic && !c.auto_height;
    let heights: Vec<f64> = if manual { c.heights.clone() } else { vec![c.height; usize::from(c.count.max(1))] };
    let count = if manual { heights.len().max(1) } else { usize::from(c.count.max(1)) };
    let mut v = vec![
        Tag::i(75, if c.kind == MTextColumnType::Dynamic { 2 } else { 1 }),
        Tag::i(76, count as i64),
        Tag::i(78, i64::from(c.flow_reversed)),
        Tag::i(79, i64::from(c.auto_height)),
        Tag::f(48, t.width),
        Tag::f(49, c.gutter),
    ];
    v.extend(heights.into_iter().map(|h| Tag::f(50, h)));
    v
}
