//! Multileaders in DXF (DXF Reference: MULTILEADER entity). The entity's own groups (style,
//! leader type, landing and dogleg switches, content type, block attribute values) come after
//! the `AcDbMLeader` marker; the geometry lives in nested sections that reuse the same group
//! codes: `300 CONTEXT_DATA{` … `301 }` holds the content (MText or block) and the plane, each
//! `302 LEADER{` … `303 }` inside it a landing point with its dogleg, and each
//! `304 LEADER_LINE{` … `305 }` inside that the vertices of one leader line from the arrowhead.
//!
//! The reader walks those sections with a level counter over the record's tags, so an
//! unterminated or unbalanced section ends with the record; counts of leaders, lines,
//! vertices and attribute values are capped.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cadcraft_doc::{
    Attrib, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, MLeader, MLeaderBranch, MLeaderCodes, MLeaderContext,
    MLeaderContextGroups, MLeaderEntityGroups, MLeaderGroups, MLeaderRoot, MLeaderRootGroups, MLeaderStyle, MText,
};
use cadcraft_dxf::{Tag, Version};
use cadcraft_geom::{OcsFrame, Vec3};

/// Most leader roots (landings) read per multileader.
const MAX_ROOTS: usize = 256;
/// Most leader lines read per multileader, over all its roots.
const MAX_LINES: usize = 4096;
/// Most leader line vertices read per multileader.
const MAX_VERTICES: usize = 100_000;
/// Most block attribute values read per multileader.
const MAX_ATTRIBS: usize = 4096;

/// ByBlock as a 32-bit colour value (0xC1000000).
const BYBLOCK: i64 = -1_056_964_608;
/// No background colour (0xC8000000).
const NO_COLOR: i64 = -939_524_096;

/// Data of a multileader that needs the file's objects: its MLEADERSTYLE handle and the values
/// of its block attributes by ATTDEF handle (all upper case).
#[derive(Debug, Default)]
pub(crate) struct Pending {
    style: Option<String>,
    attribs: Vec<(String, String)>,
}

/// A point read group by group (x, then y and z).
#[derive(Clone, Copy, Default)]
struct P {
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
}

impl P {
    /// Take group `code` when it is coordinate `base`, `base + 10` or `base + 20`.
    fn take(&mut self, base: i32, code: i32, v: f64) -> bool {
        let slot = match code - base {
            0 => &mut self.x,
            10 => &mut self.y,
            20 => &mut self.z,
            _ => return false,
        };
        *slot = Some(v);
        true
    }
    fn get(&self) -> Option<Vec3> {
        self.x.map(|x| Vec3::new(x, self.y.unwrap_or(0.0), self.z.unwrap_or(0.0)))
    }
}

#[derive(Default)]
struct Root {
    /// The root number (90).
    id: Option<i64>,
    landing: P,
    /// The dogleg direction (11) and length (40).
    dir: P,
    dogleg: Option<f64>,
    /// Each leader line: its number (91) and vertices.
    lines: Vec<(Option<i64>, Vec<Vec3>)>,
}

#[derive(Default)]
struct Parsed {
    // Entity level.
    style: Option<String>,
    leader_type: Option<i64>,
    dogleg_on: Option<bool>,
    dogleg: Option<f64>,
    arrow: Option<f64>,
    /// Arrowhead block record (342).
    arrow_block: Option<String>,
    /// Leader line type record (341 of the entity).
    line_type: Option<String>,
    content: Option<i64>,
    text_style: Option<String>,
    block: Option<String>,
    block_scale: P,
    block_rotation: Option<f64>,
    attribs: Vec<(String, String)>,
    // CONTEXT_DATA.
    base: P,
    text_height: Option<f64>,
    arrow_ctx: Option<f64>,
    has_text: Option<bool>,
    text: Option<String>,
    text_style_ctx: Option<String>,
    text_location: P,
    text_direction: P,
    text_rotation: Option<f64>,
    text_width: Option<f64>,
    line_spacing: Option<f64>,
    line_spacing_style: Option<i64>,
    /// Text side (171: 1 left, 2 centre, 3 right).
    side: Option<i64>,
    /// How the text's left / right side attaches to the landing (174 / 175, or 173 / 95 on
    /// the entity): which line of the text the landing points at.
    left_attach: Option<i64>,
    right_attach: Option<i64>,
    has_block: Option<bool>,
    block_ctx: Option<String>,
    block_location: P,
    block_scale_ctx: P,
    block_rotation_ctx: Option<f64>,
    /// The context's frame: text plane (11), block plane (14) and the basis (110, 111, 112).
    text_normal: P,
    block_normal: P,
    origin: P,
    x_axis: P,
    y_axis: P,
    roots: Vec<Root>,
    /// The attachment and connection codes as the file has them: the context's 174, 175, 176, 177, and the entity's 173, 95,
    /// 174, 175, 176.
    ctx_codes: [Option<i64>; 4],
    entity_codes: [Option<i64>; 5],
    /// The other numeric groups of the context, the entity and the first root, as the file has them.
    ctx_groups: MLeaderContextGroups,
    entity_groups: MLeaderEntityGroups,
    root_groups: MLeaderRootGroups,
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Entity,
    Context,
    Root,
    Line,
}

fn handle(t: &Tag) -> Option<String> {
    let h = t.str().trim().to_ascii_uppercase();
    (!h.is_empty() && h != "0").then_some(h)
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

fn parse(tags: &[Tag]) -> Parsed {
    // The entity's own groups follow the AcDbMLeader marker (or, without one, the last marker).
    let start =
        tags.iter().position(|t| t.code == 100 && t.str() == "AcDbMLeader").or_else(|| tags.iter().rposition(|t| t.code == 100)).map_or(0, |i| i + 1);
    let mut p = Parsed::default();
    let mut lvl = Level::Entity;
    // Whether the current root / line is kept (under the caps) and the last vertex was.
    let (mut root_kept, mut line_kept, mut vertex_kept) = (false, false, false);
    let (mut lines, mut vertices) = (0usize, 0usize);
    let mut attrib: Option<String> = None;
    for t in tags.get(start..).unwrap_or(&[]) {
        let (c, f) = (t.code, t.f64());
        // The numeric groups the model keeps as read, whatever else reads them.
        match lvl {
            Level::Entity => drop(p.entity_groups.set(c, f)),
            Level::Context => drop(p.ctx_groups.set(c, f)),
            Level::Root if root_kept && p.roots.len() == 1 => drop(p.root_groups.set(c, f)),
            _ => {}
        }
        match lvl {
            Level::Entity => match c {
                300 if t.str().trim_start().starts_with("CONTEXT_DATA") => lvl = Level::Context,
                340 => p.style = handle(t),
                170 => p.leader_type = Some(t.i64()),
                291 => p.dogleg_on = Some(t.i64() != 0),
                41 => p.dogleg = finite(f),
                42 => p.arrow = finite(f),
                342 => p.arrow_block = handle(t),
                341 => p.line_type = handle(t),
                172 => p.content = Some(t.i64()),
                343 => p.text_style = handle(t),
                344 => p.block = handle(t),
                10 | 20 | 30 => {
                    p.block_scale.take(10, c, f);
                }
                43 => p.block_rotation = finite(f),
                173 => {
                    p.left_attach = p.left_attach.or(Some(t.i64()));
                    p.entity_codes[0] = Some(t.i64());
                }
                95 => {
                    p.right_attach = p.right_attach.or(Some(t.i64()));
                    p.entity_codes[1] = Some(t.i64());
                }
                174 => p.entity_codes[2] = Some(t.i64()),
                175 => p.entity_codes[3] = Some(t.i64()),
                176 => p.entity_codes[4] = Some(t.i64()),
                // Block attribute values: ATTDEF handle (330), index, width, then the text (302).
                330 => attrib = handle(t),
                302 => {
                    if let Some(h) = attrib.take()
                        && p.attribs.len() < MAX_ATTRIBS
                    {
                        p.attribs.push((h, t.str()));
                    }
                }
                _ => {}
            },
            Level::Context => match c {
                301 => lvl = Level::Entity,
                302 if t.str().trim_start().starts_with("LEADER") => {
                    root_kept = p.roots.len() < MAX_ROOTS;
                    if root_kept {
                        p.roots.push(Root::default());
                    }
                    lvl = Level::Root;
                }
                10 | 20 | 30 => {
                    p.base.take(10, c, f);
                }
                41 => p.text_height = finite(f),
                140 => p.arrow_ctx = finite(f),
                290 => p.has_text = Some(t.i64() != 0),
                304 => p.text = Some(t.str()),
                340 => p.text_style_ctx = handle(t),
                11 | 21 | 31 => {
                    p.text_normal.take(11, c, f);
                }
                14 | 24 | 34 => {
                    p.block_normal.take(14, c, f);
                }
                110 | 120 | 130 => {
                    p.origin.take(110, c, f);
                }
                111 | 121 | 131 => {
                    p.x_axis.take(111, c, f);
                }
                112 | 122 | 132 => {
                    p.y_axis.take(112, c, f);
                }
                12 | 22 | 32 => {
                    p.text_location.take(12, c, f);
                }
                13 | 23 | 33 => {
                    p.text_direction.take(13, c, f);
                }
                42 => p.text_rotation = finite(f),
                43 => p.text_width = finite(f),
                45 => p.line_spacing = finite(f),
                170 => p.line_spacing_style = Some(t.i64()),
                171 => p.side = Some(t.i64()),
                174 => {
                    p.left_attach = Some(t.i64());
                    p.ctx_codes[0] = Some(t.i64());
                }
                175 => {
                    p.right_attach = Some(t.i64());
                    p.ctx_codes[1] = Some(t.i64());
                }
                176 => p.ctx_codes[2] = Some(t.i64()),
                177 => p.ctx_codes[3] = Some(t.i64()),
                296 => p.has_block = Some(t.i64() != 0),
                341 => p.block_ctx = handle(t),
                15 | 25 | 35 => {
                    p.block_location.take(15, c, f);
                }
                16 | 26 | 36 => {
                    p.block_scale_ctx.take(16, c, f);
                }
                46 => p.block_rotation_ctx = finite(f),
                _ => {}
            },
            Level::Root => match c {
                303 => lvl = Level::Context,
                301 => lvl = Level::Entity,
                304 if t.str().trim_start().starts_with("LEADER_LINE") => {
                    line_kept = root_kept && lines < MAX_LINES;
                    if line_kept && let Some(r) = p.roots.last_mut() {
                        r.lines.push((None, Vec::new()));
                        lines += 1;
                    }
                    vertex_kept = false;
                    lvl = Level::Line;
                }
                10 | 20 | 30 if root_kept => {
                    if let Some(r) = p.roots.last_mut() {
                        r.landing.take(10, c, f);
                    }
                }
                11 | 21 | 31 if root_kept => {
                    if let Some(r) = p.roots.last_mut() {
                        r.dir.take(11, c, f);
                    }
                }
                90 if root_kept => {
                    if let Some(r) = p.roots.last_mut() {
                        r.id = Some(t.i64());
                    }
                }
                40 if root_kept => {
                    if let Some(r) = p.roots.last_mut() {
                        r.dogleg = finite(f);
                    }
                }
                _ => {}
            },
            Level::Line => match c {
                305 => lvl = Level::Root,
                303 => lvl = Level::Context,
                301 => lvl = Level::Entity,
                10 => {
                    vertex_kept = line_kept && vertices < MAX_VERTICES;
                    if vertex_kept && let Some((_, l)) = p.roots.last_mut().and_then(|r| r.lines.last_mut()) {
                        l.push(Vec3::new(f, 0.0, 0.0));
                        vertices += 1;
                    }
                }
                91 if line_kept => {
                    if let Some(line) = p.roots.last_mut().and_then(|r| r.lines.last_mut()) {
                        line.0 = Some(t.i64());
                    }
                }
                20 | 30 if vertex_kept => {
                    if let Some(v) = p.roots.last_mut().and_then(|r| r.lines.last_mut()).and_then(|(_, l)| l.last_mut()) {
                        if c == 20 {
                            v.y = f;
                        } else {
                            v.z = f;
                        }
                    }
                }
                _ => {}
            },
        }
    }
    p
}

/// A MULTILEADER record as a multileader. Names that come from handles (style, text style,
/// block) are filled in by [`resolve`] and [`apply`].
pub(crate) fn read(tags: &[Tag]) -> MLeader {
    read_as(tags, false).0
}

/// A MULTILEADER record as a canonical multileader: every point and direction as the record gives it (WCS), the text
/// direction in full, the block scale as it is, with a context of the canonical revision. `None` when the text or the
/// block has no plane (a normal without direction) or the text direction has no length.
pub(crate) fn read_canonical(tags: &[Tag]) -> Option<MLeader> {
    let (leader, valid) = read_as(tags, true);
    valid.then_some(leader)
}

/// The multileader of the record, read as legacy (projected, as always) or as canonical; and whether a canonical reading
/// found the planes it needs.
fn read_as(tags: &[Tag], canonical: bool) -> (MLeader, bool) {
    let p = parse(tags);
    let def = MLeaderStyle::default();
    let ctx = MLeaderContext::default();
    // The planes of the content: the text direction and rotation of a canonical multileader are read in its text plane.
    let text_normal = p.text_normal.get().unwrap_or(ctx.text_normal);
    let block_normal = p.block_normal.get().unwrap_or(ctx.block_normal);
    let text_frame = OcsFrame::new(text_normal);
    let text_dir = text_frame.and_then(|f| {
        let d = p.text_direction.get().unwrap_or_else(|| {
            let a = p.text_rotation.unwrap_or(0.0);
            f.to_world(Vec3::new(a.cos(), a.sin(), 0.0))
        });
        d.try_normalized().map(|_| d)
    });
    let valid = text_dir.is_some() && OcsFrame::new(block_normal).is_some();
    let pos = |v: Option<f64>| v.filter(|x| *x > 0.0);
    let none = p.leader_type == Some(0);
    let text_location = p.text_location.get();
    let block_location = p.block_location.get();
    let landing = p.roots.iter().find_map(|r| r.landing.get()).or(p.base.get()).or(text_location).or(block_location).unwrap_or(Vec3::ZERO);
    let content_left = text_location.or(block_location).is_some_and(|c| c.x < landing.x);
    // Every root keeps its landing, dogleg and lines. Without a leader line (leader type None) there is no root
    // in the file: one without lines holds the landing the content hangs on.
    let dogleg_of = |r: &Root| {
        if p.dogleg_on == Some(false) { 0.0 } else { r.dogleg.or(p.dogleg).filter(|d| *d >= 0.0).unwrap_or(0.0) }
    };
    let mut next_branch = 0i32;
    let mut roots: Vec<MLeaderRoot> = p
        .roots
        .iter()
        .enumerate()
        .map(|(n, r)| {
            let land = r.landing.get().unwrap_or(landing);
            let branches = r
                .lines
                .iter()
                .filter(|(_, l)| !l.is_empty() && !none)
                .map(|(id, vertices)| {
                    let id = id.and_then(|v| i32::try_from(v).ok()).unwrap_or(next_branch);
                    next_branch = next_branch.max(id).saturating_add(1);
                    MLeaderBranch { id, vertices: vertices.clone() }
                })
                .collect::<Vec<_>>();
            let dir = r.dir.get().unwrap_or(Vec3::new(if content_left { -1.0 } else { 1.0 }, 0.0, 0.0));
            MLeaderRoot {
                id: r.id.and_then(|v| i32::try_from(v).ok()).unwrap_or(i32::try_from(n).unwrap_or(0)),
                landing: land,
                dogleg_dir: dir,
                dogleg: if branches.is_empty() { 0.0 } else { dogleg_of(r) },
                branches,
            }
        })
        .collect();
    if roots.is_empty() {
        roots.push(MLeaderRoot::new(landing, 0.0, content_left, Vec::new()));
    }
    let landing = roots.first().map_or(landing, |r| r.landing);
    let has_block = p.has_block.unwrap_or(p.content == Some(1));
    let has_text = !has_block && p.has_text.unwrap_or(p.content == Some(2));
    let text = has_text.then(|| {
        let rotation = match (canonical, text_frame.zip(text_dir)) {
            (true, Some((f, d))) => {
                let local = f.to_ocs(d);
                local.y.atan2(local.x)
            }
            _ => match p.text_direction.get() {
                Some(d) if d.x != 0.0 || d.y != 0.0 => d.y.atan2(d.x),
                _ => p.text_rotation.unwrap_or(0.0),
            },
        };
        MText {
            insert: text_location.unwrap_or(landing),
            height: pos(p.text_height).unwrap_or(def.text_height),
            width: p.text_width.unwrap_or(0.0).max(0.0),
            attach: attach_point(&p, text_location.unwrap_or(landing).x < landing.x),
            rotation,
            style: def.text_style.clone(),
            contents: p.text.clone().unwrap_or_default(),
            line_spacing: pos(p.line_spacing).unwrap_or(1.0),
            line_spacing_exact: p.line_spacing_style == Some(2),
            columns: None,
            direction: if canonical { text_dir } else { None },
            drawing_direction: None,
        }
    });
    let block = has_block.then(|| {
        let scale = p.block_scale_ctx.get().or(p.block_scale.get()).unwrap_or(Vec3::new(1.0, 1.0, 1.0));
        // A legacy block cannot be scaled to nothing; a canonical one keeps the scale it has.
        let nz = |v: f64| if v != 0.0 || canonical { v } else { 1.0 };
        Insert {
            block: String::new(),
            insert: block_location.unwrap_or(landing),
            scale: Vec3::new(nz(scale.x), nz(scale.y), nz(scale.z)),
            rotation: p.block_rotation_ctx.or(p.block_rotation).unwrap_or(0.0),
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }
    });
    let context = MLeaderContext {
        origin: p.origin.get().unwrap_or(Vec3::new(0.0, 0.0, landing.z)),
        x_axis: p.x_axis.get().unwrap_or(ctx.x_axis),
        y_axis: p.y_axis.get().unwrap_or(ctx.y_axis),
        text_normal,
        text_dir: text_dir.filter(|_| canonical).or(p.text_direction.get()).unwrap_or_else(|| {
            let a = p.text_rotation.unwrap_or(0.0);
            Vec3::new(a.cos(), a.sin(), 0.0)
        }),
        block_normal,
        geometry_revision: if canonical { GeometryRevision::CanonicalV2 } else { ctx.geometry_revision },
        codes: MLeaderCodes {
            derived_attach: text.as_ref().map(|t| t.attach),
            ctx_left: p.ctx_codes[0],
            ctx_right: p.ctx_codes[1],
            ctx_alignment: p.ctx_codes[2],
            ctx_connection: p.ctx_codes[3],
            left: p.entity_codes[0],
            right: p.entity_codes[1],
            angle_type: p.entity_codes[2],
            alignment: p.entity_codes[3],
            block_attachment: p.entity_codes[4],
            line_type: None,
            groups: Box::new(MLeaderGroups { ctx: p.ctx_groups, entity: p.entity_groups, root: p.root_groups }),
        },
    };
    let leader = MLeader {
        roots,
        text,
        style: def.name,
        arrow_size: pos(p.arrow_ctx).or(pos(p.arrow)).unwrap_or(def.arrow_size),
        spline: p.leader_type == Some(2),
        block,
        arrow_block: String::new(),
        context,
    };
    (leader, valid)
}

/// The MText attachment point (1..=9) of the text: its column from the text side (171), its
/// row from how the side facing the landing attaches (top line, middle or bottom line).
fn attach_point(p: &Parsed, text_left_of_landing: bool) -> u8 {
    let col = p.side.unwrap_or(1).clamp(1, 3);
    let side = if text_left_of_landing { p.right_attach.or(p.left_attach) } else { p.left_attach.or(p.right_attach) };
    let row = match side.unwrap_or(0) {
        2 | 9 => 1,
        3..=5 => 2,
        _ => 0,
    };
    u8::try_from(row * 3 + col).unwrap_or(1)
}

/// Text style, block and arrowhead names from the file's STYLE and BLOCK_RECORD handles (upper case →
/// name); block content whose block is unknown is dropped. Returns what needs the objects.
pub(crate) fn resolve(
    tags: &[Tag],
    m: &mut MLeader,
    styles: &HashMap<String, String>,
    brs: &HashMap<String, String>,
    ltypes: &HashMap<String, String>,
) -> Pending {
    let p = parse(tags);
    if let Some(t) = &mut m.text
        && let Some(n) = p.text_style_ctx.iter().chain(&p.text_style).find_map(|h| styles.get(h)).filter(|n| !n.is_empty())
    {
        t.style = n.clone();
    }
    if m.block.is_some() {
        match p.block_ctx.iter().chain(&p.block).find_map(|h| brs.get(h)).filter(|n| !n.is_empty()) {
            Some(n) => {
                if let Some(b) = &mut m.block {
                    b.block = n.clone();
                }
            }
            None => m.block = None,
        }
    }
    if let Some(n) = p.arrow_block.as_ref().and_then(|h| brs.get(h)).filter(|n| !n.is_empty()) {
        m.arrow_block = n.clone();
    }
    m.context.codes.line_type = p.line_type.as_ref().and_then(|h| ltypes.get(h)).cloned();
    Pending { style: p.style, attribs: p.attribs }
}

/// Multileader style names (by MLEADERSTYLE handle, from the ACAD_MLEADERSTYLE dictionary) and
/// block attribute values (by the ATTDEF of the content block), for multileaders in model
/// space, the layouts and block definitions.
pub(crate) fn apply(d: &mut Drawing, fixes: &[(Handle, Pending)], names: &HashMap<String, String>) -> crate::Result<()> {
    let fixes: Vec<&(Handle, Pending)> = fixes.iter().filter(|(_, p)| p.style.is_some() || !p.attribs.is_empty()).collect();
    if fixes.is_empty() {
        return Ok(());
    }
    // Block definitions holding the multileaders that are in no space.
    let wanted: HashSet<Handle> = fixes.iter().map(|(h, _)| *h).filter(|h| d.entity(*h).is_none()).collect();
    let mut owner: HashMap<Handle, String> = HashMap::new();
    if !wanted.is_empty() {
        for (name, b) in &d.blocks {
            for e in b.entities.iter().filter(|e| wanted.contains(&e.handle)) {
                owner.entry(e.handle).or_insert_with(|| name.clone());
            }
        }
    }
    for (h, pend) in fixes {
        let block_of = owner.get(h);
        let current = match block_of {
            Some(b) => d.block(b).and_then(|b| b.entities.get(*h)),
            None => d.entity(*h),
        };
        let Some(current) = current else { continue };
        let mut entity = current.as_ref().clone();
        let EntityKind::MLeader(m) = &mut entity.kind else { continue };
        if let Some(s) = pend.style.as_ref().and_then(|s| names.get(s)).filter(|n| !n.is_empty()) {
            m.style = s.clone();
        }
        if !pend.attribs.is_empty()
            && let Some(ins) = &m.block
        {
            let a = attribs(d, ins, &pend.attribs, m.context.geometry_revision == GeometryRevision::CanonicalV2)?;
            if let Some(ins) = &mut m.block {
                ins.attribs = a;
            }
        }
        match block_of {
            Some(b) => {
                if let Some(b) = d.blocks.get_mut(b) {
                    Arc::make_mut(b).entities.replace(entity);
                }
            }
            None => {
                if let Some(space) = d.space_of(*h)
                    && let Some(store) = d.space_mut(&space)
                {
                    store.replace(entity);
                }
            }
        }
    }
    Ok(())
}

/// The attributes of a multileader's block content: each value placed like its ATTDEF. A canonical multileader keeps
/// each one in the coordinates of the block definition, exactly as its ATTDEF is (the block matrix places it).
fn attribs(d: &Drawing, ins: &Insert, values: &[(String, String)], canonical: bool) -> crate::Result<Vec<Attrib>> {
    let Some(blk) = d.block(&ins.block) else { return Ok(Vec::new()) };
    let m = ins.transform(blk.base.xy());
    let mut out = Vec::new();
    for (h, value) in values {
        let def = Handle::parse_hex(h).and_then(|h| blk.entities.get(h));
        let Some(EntityKind::AttDef(ad)) = def.map(|e| &e.kind) else { continue };
        if canonical {
            let text = cadcraft_doc::Text { value: value.clone(), ..ad.text.clone() };
            out.push(Attrib {
                invisible: ad.invisible,
                plane: ad.plane,
                geometry_revision: GeometryRevision::CanonicalV2,
                ..Attrib::new(ad.tag.clone(), text)
            });
            continue;
        }
        let mut entity = Entity::new(Handle(0), EntityKind::Text(ad.text.clone()));
        entity.try_transform(&cadcraft_geom::Mat4::from_mat3(&m)).map_err(|error| crate::IoError::Format(error.to_string()))?;
        let EntityKind::Text(mut text) = entity.kind else { continue };
        text.value = value.clone();
        out.push(Attrib { invisible: ad.invisible, ..Attrib::new(ad.tag.clone(), text) });
    }
    Ok(out)
}

/// What a MULTILEADER record points at in the file being written.
pub(crate) struct Refs {
    /// MLEADERSTYLE handle.
    pub style: String,
    /// STYLE record handle of the text.
    pub text_style: Option<String>,
    /// BLOCK_RECORD handle of the content block.
    pub block: Option<String>,
    /// BLOCK_RECORD handle of the entity's own arrowhead (none: the style's).
    pub arrow: Option<String>,
    /// LTYPE record handle of the leader line type the file gave.
    pub line_type: Option<String>,
}

/// The groups of a MULTILEADER record after its `AcDbMLeader` marker.
///
/// A `canonical` multileader (its entity and context canonical, with a block matrix and text frame that exist) writes the text
/// direction and rotation it holds and the block's complete matrix in WCS; the legacy one writes the projected 2D values.
pub(crate) fn entity_tags(d: &Drawing, m: &MLeader, r: &Refs, canonical: bool, target: Version) -> Vec<Tag> {
    let mut t: Vec<Tag> = Vec::new();
    let s = |t: &mut Vec<Tag>, c: i32, v: &str| t.push(Tag::s(c, v));
    let f = |t: &mut Vec<Tag>, c: i32, v: f64| t.push(Tag::f(c, v));
    let i = |t: &mut Vec<Tag>, c: i32, v: i64| t.push(Tag::i(c, v));
    let p = |t: &mut Vec<Tag>, c: i32, v: Vec3| {
        t.push(Tag::f(c, v.x));
        t.push(Tag::f(c + 10, v.y));
        t.push(Tag::f(c + 20, v.z));
    };
    let style = d.mleader_styles.iter().find(|x| x.name.eq_ignore_ascii_case(&m.style)).cloned().unwrap_or_default();
    // Block content wins over text when an entity has both (DXF has one content type).
    let block = m.block.as_ref().zip(r.block.as_ref());
    let text = if block.is_some() { None } else { m.text.as_ref() };
    // The first root is the one the content hangs on; with no leader line there is no root in the file.
    let first = m.roots.first().cloned().unwrap_or_else(|| MLeaderRoot::new(Vec3::ZERO, 0.0, false, Vec::new()));
    let lined = m.line_count() > 0;
    let (ids, branch_ids) = ids(m);
    let attach = text.map_or(1, |x| x.attach.clamp(1, 9));
    // The text's row as its attachment to the landing (top of top line, middle of text,
    // bottom of bottom line) and its column as the text side and alignment.
    let vert = match attach {
        1..=3 => 0,
        4..=6 => 2,
        _ => 4,
    };
    let align = i64::from((attach - 1) % 3);
    // The codes the file gave apply while the text keeps the attachment point they were read with; the block connection always.
    let codes = &m.context.codes;
    let (cg, eg, rg) = (codes.groups.ctx, codes.groups.entity, codes.groups.root);
    // The numeric groups the file gave (`None`: the default of a multileader made in CADCraft).
    let fnum = |t: &mut Vec<Tag>, c: i32, v: Option<f64>, d: f64| t.push(Tag::f(c, v.unwrap_or(d)));
    let inum = |t: &mut Vec<Tag>, c: i32, v: Option<f64>, d: i64| t.push(Tag::i(c, v.map_or(d, |x| x as i64)));
    let same = codes.derived_attach == text.map(|x| x.attach);
    let pick = |code: Option<i64>, derived: i64| if same { code.unwrap_or(derived) } else { derived };
    i(&mut t, 270, 2);
    s(&mut t, 300, "CONTEXT_DATA{");
    fnum(&mut t, 40, cg.scale, 1.0);
    p(&mut t, 10, first.dogleg_end());
    f(&mut t, 41, text.map_or(cg.text_height.unwrap_or(style.text_height), |x| x.height));
    f(&mut t, 140, m.arrow_size);
    fnum(&mut t, 145, cg.landing_gap, style.landing_gap);
    i(&mut t, 174, pick(codes.ctx_left, vert));
    i(&mut t, 175, pick(codes.ctx_right, vert));
    i(&mut t, 176, pick(codes.ctx_alignment, align));
    i(&mut t, 177, codes.ctx_connection.unwrap_or(1));
    i(&mut t, 290, i64::from(text.is_some()));
    if let Some(x) = text {
        s(&mut t, 304, &x.contents);
        p(&mut t, 11, m.context.text_normal);
        if let Some(h) = &r.text_style {
            s(&mut t, 340, h);
        }
        p(&mut t, 12, x.insert);
        // The direction as read, while the text's rotation is still the one it gives.
        let dir = m.context.text_dir;
        let turned = (dir.y.atan2(dir.x) - x.rotation).abs() > 1e-12;
        let written = if canonical {
            x.direction.unwrap_or(dir)
        } else if turned {
            Vec3::new(x.rotation.cos(), x.rotation.sin(), 0.0)
        } else {
            dir
        };
        p(&mut t, 13, written);
        // A canonical text's rotation is its direction's angle in the text plane, whatever the field holds.
        let rotation = OcsFrame::new(m.context.text_normal).filter(|_| canonical).map_or(x.rotation, |frame| {
            let local = frame.to_ocs(written);
            local.y.atan2(local.x)
        });
        f(&mut t, 42, rotation);
        f(&mut t, 43, x.width);
        fnum(&mut t, 44, cg.column_height, 0.0);
        f(&mut t, 45, x.line_spacing);
        i(&mut t, 170, if x.line_spacing_exact { 2 } else { 1 });
        inum(&mut t, 90, cg.text_color, BYBLOCK);
        i(&mut t, 171, align + 1);
        inum(&mut t, 172, cg.flow_direction, 1);
        inum(&mut t, 91, cg.background_color, NO_COLOR);
        fnum(&mut t, 141, cg.background_scale, 1.5);
        inum(&mut t, 92, cg.background_transparency, 0);
        inum(&mut t, 291, cg.background_fill, 0);
        inum(&mut t, 292, cg.mask_fill, 0);
        inum(&mut t, 173, cg.column_type, 0);
        inum(&mut t, 293, cg.height_automatic, 0);
        fnum(&mut t, 142, cg.column_width, 0.0);
        fnum(&mut t, 143, cg.column_gutter, 0.0);
        inum(&mut t, 294, cg.column_flow_reversed, 0);
        inum(&mut t, 295, cg.word_break, 0);
    }
    i(&mut t, 296, i64::from(block.is_some()));
    if let Some((b, h)) = block {
        s(&mut t, 341, h);
        p(&mut t, 14, m.context.block_normal);
        p(&mut t, 15, b.insert);
        p(&mut t, 16, b.scale);
        f(&mut t, 46, b.rotation);
        inum(&mut t, 93, cg.block_color, BYBLOCK);
        // Block → world, row by row.
        let base = d.block(&b.block).map_or(Vec3::ZERO, |x| x.base);
        let exact = if canonical { m.block_world_transform(base).ok().flatten() } else { None };
        let rows = match exact {
            Some(w) => [w.m[0], w.m[1], w.m[2], w.m[3]].concat(),
            None => {
                let mx = b.transform(base.xy());
                let z = b.insert.z - b.scale.z * base.z;
                vec![mx.a, mx.c, 0.0, mx.e, mx.b, mx.d, 0.0, mx.f, 0.0, 0.0, b.scale.z, z, 0.0, 0.0, 0.0, 1.0]
            }
        };
        for v in rows {
            f(&mut t, 47, v);
        }
    }
    p(&mut t, 110, m.context.origin);
    p(&mut t, 111, m.context.x_axis);
    p(&mut t, 112, m.context.y_axis);
    inum(&mut t, 297, cg.normal_reversed, 0);
    if lined {
        let mut line = 0;
        for (n, root) in m.roots.iter().enumerate() {
            s(&mut t, 302, "LEADER{");
            inum(&mut t, 290, rg.last_point_set, 1);
            inum(&mut t, 291, rg.dogleg_set, 1);
            p(&mut t, 10, root.landing);
            p(&mut t, 11, root.dogleg_dir);
            i(&mut t, 90, ids.get(n).copied().unwrap_or(0));
            f(&mut t, 40, root.dogleg);
            for b in &root.branches {
                s(&mut t, 304, "LEADER_LINE{");
                for v in &b.vertices {
                    p(&mut t, 10, *v);
                }
                i(&mut t, 91, branch_ids.get(line).copied().unwrap_or(0));
                line += 1;
                s(&mut t, 305, "}");
            }
            inum(&mut t, 271, rg.attach_direction, 0);
            s(&mut t, 303, "}");
        }
    }
    inum(&mut t, 272, cg.attach_top, 9);
    inum(&mut t, 273, cg.attach_bottom, 9);
    s(&mut t, 301, "}");
    s(&mut t, 340, &r.style);
    // Overridden properties: leader type, landing and dogleg, arrowhead size, content type,
    // text style and height, block content (the entity holds its own values).
    // An own arrowhead (342) different from the style's is the 0x100 override; one the file gave without the bit (AutoCAD writes the
    // style's arrowhead there too) stays without it, so a change of the style still reaches the multileader.
    let source_bit = eg.override_flags.is_some_and(|x| (x as i64) & 0x100 != 0);
    let differs = !m.arrow_block.eq_ignore_ascii_case(&style.arrow_block);
    let arrow_flag = if r.arrow.is_some() && (eg.override_flags.is_none() || source_bit || differs) { 0x100 } else { 0 };
    // The override flags as read, with the own-arrowhead bit following the multileader.
    let overrides = eg.override_flags.map_or(0x1 | 0x10 | 0x40 | 0x80 | 0x200 | 0x400 | 0x800 | 0x1_0000 | 0x8_0000, |x| (x as i64) & !0x100);
    i(&mut t, 90, overrides | arrow_flag);
    i(
        &mut t,
        170,
        if !lined {
            0
        } else if m.spline {
            2
        } else {
            1
        },
    );
    inum(&mut t, 91, eg.line_color, BYBLOCK);
    if let Some(h) = &r.line_type {
        s(&mut t, 341, h);
    }
    inum(&mut t, 171, eg.line_weight, -2);
    inum(&mut t, 290, eg.landing_enabled, 1);
    inum(&mut t, 291, eg.dogleg_enabled, i64::from(first.dogleg > 0.0));
    f(&mut t, 41, first.dogleg);
    if let Some(h) = &r.arrow {
        s(&mut t, 342, h);
    }
    f(&mut t, 42, m.arrow_size);
    i(
        &mut t,
        172,
        if block.is_some() {
            1
        } else if text.is_some() {
            2
        } else {
            0
        },
    );
    if let Some(h) = &r.text_style {
        s(&mut t, 343, h);
    }
    i(&mut t, 173, pick(codes.left, vert));
    i(&mut t, 95, pick(codes.right, vert));
    i(&mut t, 174, pick(codes.angle_type, 1));
    i(&mut t, 175, pick(codes.alignment, align));
    inum(&mut t, 92, eg.text_color, BYBLOCK);
    inum(&mut t, 292, eg.text_frame, 0);
    // The block groups end the entity's content part for every content type (AutoCAD writes them for text too, with their defaults).
    if let Some((_, h)) = block {
        s(&mut t, 344, h);
    }
    inum(&mut t, 93, eg.block_color, BYBLOCK);
    p(&mut t, 10, block.map_or(Vec3::new(1.0, 1.0, 1.0), |(b, _)| b.scale));
    // The entity's own rotation is not the context's (46): a rotated multileader keeps the first.
    f(&mut t, 43, eg.block_rotation.unwrap_or_else(|| block.map_or(0.0, |(b, _)| b.rotation)));
    i(&mut t, 176, codes.block_attachment.unwrap_or(i64::from(block.is_some())));
    if let Some((b, _)) = block {
        // Attribute values by their ATTDEF in the block.
        let defs: Vec<(Handle, &str)> = d
            .block(&b.block)
            .map(|blk| {
                blk.entities
                    .iter()
                    .filter_map(|e| if let EntityKind::AttDef(ad) = &e.kind { Some((e.handle, ad.tag.as_str())) } else { None })
                    .collect()
            })
            .unwrap_or_default();
        let mut n = 0i64;
        for a in &b.attribs {
            let Some((h, _)) = defs.iter().find(|(_, tag)| tag.eq_ignore_ascii_case(&a.tag)) else { continue };
            s(&mut t, 330, &h.hex());
            i(&mut t, 177, n);
            f(&mut t, 44, 0.0);
            s(&mut t, 302, &a.text.value);
            n += 1;
        }
    }
    inum(&mut t, 293, eg.annotation_scale, 0);
    inum(&mut t, 294, eg.text_direction_negative, 0);
    inum(&mut t, 178, eg.text_align_in_editor, 0);
    inum(&mut t, 179, eg.attachment_point, 1);
    fnum(&mut t, 45, eg.scale, 1.0);
    inum(&mut t, 271, eg.attach_direction, 0);
    inum(&mut t, 272, eg.attach_top, 9);
    inum(&mut t, 273, eg.attach_bottom, 9);
    // Measured (AutoCAD LT 2025): the entity ends with 295 in an AC1032 file and not in an AC1024 one.
    if target >= Version::R2018 {
        inum(&mut t, 295, eg.trailer, 0);
    }
    t
}

/// The numbers written for the roots (group 90) and the leader lines (group 91): the ones the multileader has when
/// they are all different, else the positions (a command that adds a root or a line gives it a number the file
/// may already use).
fn ids(m: &MLeader) -> (Vec<i64>, Vec<i64>) {
    let unique = |v: &[i64]| v.iter().collect::<HashSet<_>>().len() == v.len();
    let roots: Vec<i64> = m.roots.iter().map(|r| i64::from(r.id)).collect();
    let branches: Vec<i64> = m.lines().map(|(_, b)| i64::from(b.id)).collect();
    let positions = |n: usize| (0..n).map(|i| i64::try_from(i).unwrap_or(i64::MAX)).collect::<Vec<_>>();
    (if unique(&roots) { roots } else { positions(m.roots.len()) }, if unique(&branches) { branches } else { positions(m.line_count()) })
}
