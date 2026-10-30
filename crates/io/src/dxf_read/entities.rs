use super::common::{T, common, ocs, ocs_n};
use super::hatch::hatch;
use super::text::{attdef, attrib_modes, attrib_props, mtext_from, multiline_attrib, split_embedded, text_from};
use super::{
    Arc, Attrib, AttribPlane, Circle, Common, DimKind, Dimension, Ellipse, EntityKind, Face3d, Handle, HashMap, Helix, HelixConstrain, Insert,
    Leader, Line, LwPolyline, MAX_FRAME_CHARS, Point, PolyVertex, Polyline3d, RawTag, RayLine, Rx, Solid, Spline, Table, TableCell, Tag, Tolerance,
    Unknown, Vec2, Vec3, Viewport, Wipeout, records,
};

/// Parse one entity record. POLYLINE/INSERT followers are handled by the caller.
pub(super) fn entity(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
    entity_with_table_budget(kind, tags, &mut 4_000_000)
}

/// Parse one entity while charging materialized table cells to the caller's shared import budget.
/// An ARC, with its angles run the other way in a mirrored OCS.
fn arc(c: &Common, t: &T) -> Arc {
    let mut a = Arc { center: ocs(c, t.p(10)), radius: t.fd(40, 1.0).abs(), start: t.fd(50, 0.0).to_radians(), end: t.fd(51, 360.0).to_radians() };
    if c.extrusion.z < 0.0 {
        // Mirrored OCS: angles run the other way.
        let (s, e) = (std::f64::consts::PI - a.end, std::f64::consts::PI - a.start);
        a.start = cadcraft_geom::norm_angle(s);
        a.end = cadcraft_geom::norm_angle(e);
    }
    a
}

pub(super) fn ellipse(c: &Common, t: &T) -> Ellipse {
    Ellipse { center: t.p(10), major: t.p(11), normal: c.extrusion, ratio: t.fd(40, 1.0), start: t.fd(41, 0.0), end: t.fd(42, std::f64::consts::TAU) }
}

/// A LWPOLYLINE, or the region CADCraft wrote as one.
pub(super) fn lwpolyline(tags: &[Tag], c: &Common, t: &T) -> Option<EntityKind> {
    let mut verts = Vec::new();
    let mut cur: Option<PolyVertex> = None;
    for tg in tags {
        match tg.code {
            10 => {
                if let Some(v) = cur.take() {
                    verts.push(v);
                }
                cur = Some(PolyVertex { p: Vec2::new(tg.f64(), 0.0), ..Default::default() });
            }
            20 => {
                if let Some(v) = cur.as_mut() {
                    v.p.y = tg.f64();
                }
            }
            40 => {
                if let Some(v) = cur.as_mut() {
                    v.start_width = tg.f64();
                }
            }
            41 => {
                if let Some(v) = cur.as_mut() {
                    v.end_width = tg.f64();
                }
            }
            42 => {
                if let Some(v) = cur.as_mut() {
                    v.bulge = tg.f64();
                }
            }
            _ => {}
        }
    }
    verts.extend(cur);
    let flags = t.i(70).unwrap_or(0);
    if c.extrusion.z < 0.0 {
        for v in &mut verts {
            v.p.x = -v.p.x;
            v.bulge = -v.bulge;
        }
    }
    let pl = LwPolyline {
        records: Default::default(),
        vertices: verts,
        closed: flags & 1 != 0,
        const_width: t.fd(43, 0.0),
        elevation: t.fd(38, 0.0),
        plinegen: flags & 128 != 0,
        heavy: false,
        fit: None,
    };
    // A region CADCraft wrote as polylines (`dxf_region`).
    crate::dxf_region::from_polyline(tags, pl)
}

/// The tolerances (42, 43) and the linear flag (bit 16 of 70) of a SPLINE record; what a group lacks takes the value of a spline made here.
pub(super) fn spline_source(t: &T) -> Option<Box<cadcraft_geom::SplineSource>> {
    let d = cadcraft_geom::SplineSource::default();
    let source = cadcraft_geom::SplineSource {
        knot_tolerance: t.f(42).filter(|v| v.is_finite()).unwrap_or(d.knot_tolerance),
        control_tolerance: t.f(43).filter(|v| v.is_finite()).unwrap_or(d.control_tolerance),
        linear: t.i(70).unwrap_or(0) & 16 != 0,
    };
    (source != d).then(|| Box::new(source))
}

fn spline(t: &T) -> Spline {
    let degree = t.i(71).unwrap_or(3).clamp(1, 10) as usize;
    let knots = t.all_f(40);
    let control = t.pts(10);
    let weights = t.all_f(41);
    let fit = t.pts(11);
    let tangent = |c: i32| t.f(c).map(|_| t.p(c).xy());
    let fit_opts =
        cadcraft_geom::FitOptions { knots: Default::default(), start_tangent: tangent(12), end_tangent: tangent(13), tolerance: t.fd(44, 0.0) }
            .sanitized();
    let closed = t.i(70).unwrap_or(0) & 1 != 0;
    let mut sp = Spline { degree, knots, control, weights, fit: fit.clone(), closed, fit_opts, space: None, source: spline_source(t) };
    if !sp.is_valid() && fit.len() >= 2 {
        sp = Spline::fit_with(&fit, sp.closed, fit_opts);
    } else if !sp.is_valid() && sp.control.len() >= 2 {
        sp = Spline::from_control(sp.control.clone(), degree);
    } else if fit.len() >= 3 {
        // DXF keeps the knots, not how they were spaced: recover it for later refits.
        sp.fit_opts.knots = sp.infer_knot_param();
    }
    // A spline rebuilt from its fit or control points keeps the record's tolerances.
    sp.source = spline_source(t);
    sp
}

pub(super) fn entity_with_table_budget(kind: &str, tags: &[Tag], table_cells_left: &mut usize) -> Option<(Common, EntityKind)> {
    // The paper-space viewport our writer adds (dxf_write::paper_view) stays implicit.
    if kind == "VIEWPORT" && crate::dxf_ext::is_paper_view(tags) {
        return None;
    }
    let t = T(tags);
    let c = common(&t);
    let k = match kind {
        "LINE" => EntityKind::Line(Line { a: t.p(10), b: t.p(11) }),
        "POINT" => EntityKind::Point(Point { p: t.p(10), angle: t.fd(50, 0.0).to_radians() }),
        "CIRCLE" => EntityKind::Circle(Circle { center: ocs(&c, t.p(10)), radius: t.fd(40, 1.0).abs() }),
        "ARC" => EntityKind::Arc(arc(&c, &t)),
        "ELLIPSE" => EntityKind::Ellipse(ellipse(&c, &t)),
        "LWPOLYLINE" => lwpolyline(tags, &c, &t)?,
        "SPLINE" => EntityKind::Spline(spline(&t)),
        "HELIX" => EntityKind::Helix(helix(tags)),
        "RAY" => EntityKind::Ray(RayLine { base: t.p(10), dir: t.p(11) }),
        "XLINE" => EntityKind::XLine(RayLine { base: t.p(10), dir: t.p(11) }),
        "TEXT" => EntityKind::Text(text_from(&t)),
        "MTEXT" => EntityKind::MText(mtext_from(tags, &c)),
        "ATTDEF" => EntityKind::AttDef(attdef(tags, &c)),
        "INSERT" => {
            // A mirrored block reference comes with extrusion (0,0,-1): its insertion point, scale,
            // rotation and column spacing are in that OCS, where x runs the other way. In the WCS that
            // is the mirrored point, a negative x scale, the opposite rotation and a negative column
            // spacing. Noise of up to 1e-3 in the extrusion's x/y moves the projection by ~1e-7 of the
            // size, so it still counts as mirrored. A truly tilted extrusion (block plane not parallel
            // to XY) needs the full 3D block transform, which Insert cannot hold; it is left as read.
            let n = c.extrusion;
            let mirrored = n.z < 0.0 && n.x.hypot(n.y) <= 1e-3 * n.z.abs();
            let mut p = t.p(10);
            let mut scale = Vec3::new(t.fd(41, 1.0), t.fd(42, 1.0), t.fd(43, 1.0));
            let mut rotation = t.fd(50, 0.0).to_radians();
            let mut col_spacing = t.fd(44, 0.0);
            if mirrored {
                p = ocs(&c, p);
                scale.x = -scale.x;
                rotation = -rotation;
                col_spacing = -col_spacing;
            }
            EntityKind::Insert(Insert {
                block: t.s(2).unwrap_or_default(),
                insert: p,
                scale,
                rotation,
                attribs: Vec::new(),
                cols: t.i(70).unwrap_or(1).clamp(1, 10_000) as u32,
                rows: t.i(71).unwrap_or(1).clamp(1, 10_000) as u32,
                col_spacing,
                row_spacing: t.fd(45, 0.0),
            })
        }
        "DIMENSION" => EntityKind::Dimension(dimension_from(&t, if c.extrusion.z < 0.0 { Some(c.extrusion) } else { None })),
        "LEADER" => EntityKind::Leader(Leader {
            vertices: t.pts(10).into_iter().map(|p| p.to3(0.0)).collect(),
            arrow: t.i(71).unwrap_or(1) != 0,
            spline: t.i(72).unwrap_or(0) != 0,
            style: t.s(3).unwrap_or_else(|| "Standard".into()),
            frame: None,
        }),
        // Names behind handles follow in `entity_extras` and `Objects::apply`.
        "MULTILEADER" | "MLEADER" => EntityKind::MLeader(crate::dxf_mleader::read(tags)),
        "SOLID" | "TRACE" => {
            let corners = [t.p(10), t.p(11), t.p(12), if t.f(13).is_some() { t.p(13) } else { t.p(12) }];
            let corners = corners.map(|p| ocs(&c, p));
            if kind == "SOLID" { EntityKind::Solid(Solid { corners }) } else { EntityKind::Trace(Solid { corners }) }
        }
        "MLINE" => EntityKind::MLine(crate::dxf_mline::read_entity(tags)),
        "3DFACE" => EntityKind::Face3d(Face3d { corners: [t.p(10), t.p(11), t.p(12), t.p(13)], hidden_edges: t.i(70).unwrap_or(0) as u8 }),
        "HATCH" => {
            let mut k = hatch(tags, &t)?;
            if c.extrusion.z < 0.0
                && let EntityKind::Hatch(h) = &mut k
            {
                // Boundary in the mirrored OCS, like LWPOLYLINE; the pattern direction mirrors too.
                for v in h.loops.iter_mut().flat_map(|l| l.vertices.iter_mut()) {
                    v.p.x = -v.p.x;
                    v.bulge = -v.bulge;
                }
                h.angle = cadcraft_geom::norm_angle(std::f64::consts::PI - h.angle);
                h.seed_points.iter_mut().for_each(|p| p.x = -p.x);
            }
            k
        }
        "ACAD_TABLE" => EntityKind::Table(acad_table(tags, table_cells_left)),
        "VIEWPORT" => {
            let flags = t.i(90).unwrap_or(32768);
            let stacking = t.i(68).unwrap_or_else(|| if flags & 131072 == 0 { t.i(69).unwrap_or(0).clamp(1, 32767) } else { 0 });
            let source_status = cadcraft_doc::ViewportStatus { stacking, flags };
            EntityKind::Viewport(Viewport {
                center: t.p(10),
                width: t.fd(40, 1.0),
                height: t.fd(41, 1.0),
                view_center: Vec2::new(t.fd(12, 0.0), t.fd(22, 0.0)),
                view_height: t.fd(45, 1.0),
                id: t.i(69).unwrap_or(0) as u32,
                locked: t.i(90).unwrap_or(0) & 16384 != 0,
                frozen_layers: crate::dxf_ext::read_frozen(tags),
                layer_colors: crate::dxf_ext::read_layer_colors(tags),
                // Status flags: 65536 = non-rectangular clipping (by the 340 entity), 131072 = off.
                clip: t.s(340).and_then(|h| Handle::parse_hex(&h)).filter(|h| h.0 != 0 && t.i(90).unwrap_or(0) & 65536 != 0),
                on: source_status.is_on(),
                source_status: Some(source_status),
            })
        }
        "WIPEOUT" => {
            let o = t.p(10);
            let u = t.p(11);
            let v = t.p(12);
            // Clip vertices are in pixel space: origin at the image's top-left corner, y pointing down.
            let pts: Vec<Vec2> = t.pts(14).into_iter().map(|q| o.xy() + u.xy() * (q.x + 0.5) + v.xy() * (0.5 - q.y)).collect();
            EntityKind::Wipeout(Wipeout { boundary: pts, display: wipeout_display(&t), frame: None })
        }
        // The file path comes from the IMAGEDEF object (`dxf_image::resolve`).
        "IMAGE" => EntityKind::Image(crate::dxf_image::read_entity(tags)),
        "TOLERANCE" => {
            let dir = t.p(11);
            let dir = if dir.xy().is_finite() && dir.xy().len() > 1e-12 { dir } else { Vec3::new(1.0, 0.0, 0.0) };
            EntityKind::Tolerance(tolerance_from(&t, dir))
        }
        other => EntityKind::Unknown(Unknown {
            dxf_type: other.to_string(),
            tags: tags.iter().map(|tg| RawTag { code: tg.code, value: tg.str() }).collect(),
        }),
    };
    Some((c, k))
}

/// The display settings of a WIPEOUT record (groups 70, 280, 281, 282, 283), the defaults of a new wipeout for a group it lacks.
pub(super) fn wipeout_display(t: &T) -> cadcraft_doc::WipeoutDisplay {
    let d = cadcraft_doc::WipeoutDisplay::default();
    cadcraft_doc::WipeoutDisplay {
        flags: t.i(70).unwrap_or(d.flags),
        clipping: t.i(280).unwrap_or(d.clipping),
        brightness: t.i(281).unwrap_or(d.brightness),
        contrast: t.i(282).unwrap_or(d.contrast),
        fade: t.i(283).unwrap_or(d.fade),
    }
}

/// A DIMENSION from its groups. The text midpoint is the one OCS point of the record the legacy model holds in WCS:
/// `mirrored` is the normal of a mirrored plane (-Z), whose text midpoint is converted. A canonical read passes `None`.
pub(super) fn dimension_from(t: &T, mirrored: Option<Vec3>) -> Dimension {
    let ty = t.i(70).unwrap_or(0) & 0x0f;
    let kind = match ty {
        0 => DimKind::Linear { rotation: t.fd(50, 0.0).to_radians() },
        1 => DimKind::Aligned,
        2 => DimKind::Angular,
        3 => DimKind::Diameter,
        4 => DimKind::Radius,
        5 => DimKind::Angular3P,
        6 => DimKind::Ordinate { x_type: t.i(70).unwrap_or(0) & 64 != 0 },
        8 => DimKind::ArcLength,
        _ => DimKind::Aligned,
    };
    Dimension {
        kind,
        defpt: t.p(10),
        text_mid: mirrored.map_or(t.p(11), |n| ocs_n(n, t.p(11))),
        p13: t.p(13),
        p14: t.p(14),
        p15: t.p(15),
        p16: t.p(16),
        text: t.s(1).unwrap_or_default(),
        style: t.s(3).unwrap_or_else(|| "Standard".into()),
        measurement: t.fd(42, 0.0),
        text_rotation: t.fd(53, 0.0).to_radians(),
        user_text_pos: t.i(70).unwrap_or(0) & 128 != 0,
        leader_length: if matches!(ty, 3 | 4) { t.fd(40, 0.0) } else { 0.0 },
        block: t.s(2).filter(|s| !s.is_empty()),
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    }
}

/// A TOLERANCE from its groups, with the frame direction `dir`.
pub(super) fn tolerance_from(t: &T, dir: Vec3) -> Tolerance {
    Tolerance {
        insert: t.p(10),
        dir,
        style: t.s(3).unwrap_or_else(|| "Standard".into()),
        text: t.s(1).unwrap_or_default().chars().take(MAX_FRAME_CHARS).collect(),
    }
}

/// The groups of a cell's own style (see `TableCell::style_groups`).
const CELL_STYLE_GROUPS: [i32; 17] = [170, 283, 63, 64, 65, 66, 68, 69, 7, 275, 276, 278, 279, 285, 286, 288, 289];

/// An ACAD_TABLE entity: sizes, cell texts and merges (DXF Reference), plus CADCraft's
/// title/header flags, text height and style name from xdata when present.
pub(super) fn acad_table(tags: &[Tag], table_cells_left: &mut usize) -> Table {
    const MAX_CELLS: usize = 1_000_000;
    let start = tags.iter().position(|x| x.code == 100 && x.str() == "AcDbTable").unwrap_or(0);
    let tail = tags.get(start..).unwrap_or(&[]);
    let first_cell = tail.iter().position(|x| x.code == 171).unwrap_or(tail.len());
    let head = T(tail.get(..first_cell).unwrap_or(&[]));
    let mut rows = usize::try_from(head.i(91).unwrap_or(0)).unwrap_or(0).min(10_000);
    let mut cols = usize::try_from(head.i(92).unwrap_or(0)).unwrap_or(0).min(10_000);
    // Large declared sizes must be backed by cell records (no allocation from a bare count).
    let n_cells = tail.iter().filter(|x| x.code == 171).count();
    let declared = rows.saturating_mul(cols);
    if declared > MAX_CELLS || (declared > 10_000 && n_cells < declared) {
        rows = 0;
        cols = 0;
    }
    // Charge before any cell vector is materialized, including discarded tables.
    let count = rows.saturating_mul(cols);
    match table_cells_left.checked_sub(count) {
        Some(left) => *table_cells_left = left,
        None => return Table { insert: T(tags).p(10), ..Table::default() },
    }
    let size = |v: f64| if v.is_finite() && v > 0.0 { v } else { 0.27 };
    let mut row_heights: Vec<f64> = head.all_f(141).into_iter().take(rows).map(size).collect();
    row_heights.resize(rows, 0.27);
    let mut col_widths: Vec<f64> = head.all_f(142).into_iter().take(cols).map(size).collect();
    col_widths.resize(cols, 2.5);
    // Cells, row-major, each starting at a 171 group; xdata ends the last one.
    let body = tail.get(first_cell..).unwrap_or(&[]);
    let body = body.get(..body.iter().position(|x| x.code >= 1000).unwrap_or(body.len())).unwrap_or(&[]);
    let mut flat: Vec<TableCell> = Vec::new();
    let mut i = 0;
    while let Some(t0) = body.get(i) {
        if flat.len() >= rows * cols {
            break;
        }
        i += 1;
        if t0.code != 171 {
            continue;
        }
        let end = body.get(i..).and_then(|r| r.iter().position(|x| x.code == 171)).map(|p| i + p).unwrap_or(body.len());
        let cell = body.get(i..end).unwrap_or(&[]);
        i = end;
        let c = T(cell);
        let mut text: String = cell.iter().filter(|x| x.code == 2 || x.code == 3).map(Tag::str).collect();
        text.push_str(&c.s(1).unwrap_or_default());
        let span = |code: i32| c.i(code).and_then(|v| u32::try_from(v).ok());
        // A merge starts at the cell whose span (rows 176, columns 175) is more than one cell.
        // Its own merged flag (173) varies by writer: CADCraft sets it, while AutoCAD-style
        // writers (acadrust 0.6.4) set it only on the cells the merge covers, whose span is 0.
        let merged = match (span(176), span(175)) {
            (Some(rs), Some(cs)) if rs >= 1 && cs >= 1 && (rs, cs) != (1, 1) => Some((rs, cs)),
            _ => None,
        };
        // The cell's own text height: 140 outside the value block (301 … 304), where it is a number. Its flag (91), rotation (145),
        // flag value (172), auto-fit (174), virtual edge (178) and type (171, the record's first group) are read outside the
        // value block too, where the codes mean other things.
        let mut in_value = false;
        let mut text_height = None;
        let (mut flag, mut rotation, mut flag_value, mut auto_fit, mut virtual_edge) = (None, None, None, None, None);
        let mut style_groups = Vec::new();
        for x in cell {
            if !in_value && CELL_STYLE_GROUPS.contains(&x.code) {
                style_groups.push(cadcraft_doc::RawTag { code: x.code, value: x.str() });
            }
            match x.code {
                301 => in_value = true,
                304 => in_value = false,
                140 if !in_value => text_height = Some(x.f64()).filter(|h| h.is_finite() && *h > 0.0 && *h < 1e9),
                91 if !in_value => flag = flag.or(i32::try_from(x.i64()).ok()),
                145 if !in_value => rotation = rotation.or(Some(x.f64()).filter(|v| v.is_finite())),
                172 => flag_value = flag_value.or(i32::try_from(x.i64()).ok()),
                174 => auto_fit = auto_fit.or(Some(x.i64() != 0)),
                178 => virtual_edge = virtual_edge.or(i32::try_from(x.i64()).ok()),
                _ => {}
            }
        }
        // A block cell (171 = 2): its block record handle (resolved to the name once the records are known), scale (144) and the
        // attribute values by their ATTDEF handle (331, 300).
        let block = (t0.i64() == 2).then(|| {
            let mut attribs: Vec<(cadcraft_doc::Handle, String)> = Vec::new();
            let mut open = false;
            for x in cell.iter().filter(|x| matches!(x.code, 331 | 300)) {
                match x.code {
                    331 => {
                        open = true;
                        if let Some(h) = cadcraft_doc::Handle::parse_hex(x.str().trim()) {
                            attribs.push((h, String::new()));
                        } else {
                            open = false;
                        }
                    }
                    _ if open => {
                        if let Some(a) = attribs.last_mut() {
                            a.1 = x.str();
                        }
                        open = false;
                    }
                    _ => {}
                }
            }
            cadcraft_doc::TableCellBlock {
                block: c.s(340).unwrap_or_default().trim().to_string(),
                scale: c.f(144).filter(|v| v.is_finite()).unwrap_or(1.0),
                attribs,
            }
        });
        flat.push(TableCell {
            text,
            block,
            merged,
            text_height,
            flag_value: flag_value.unwrap_or(0),
            override_flag: flag.unwrap_or(0),
            auto_fit: auto_fit.unwrap_or(false),
            virtual_edge: virtual_edge.unwrap_or(0),
            rotation: rotation.unwrap_or(0.0),
            style_groups,
            ..TableCell::default()
        });
    }
    flat.resize(rows * cols, TableCell::default());
    let cells: Vec<Vec<TableCell>> = if cols == 0 { vec![Vec::new(); rows] } else { flat.chunks(cols).map(<[TableCell]>::to_vec).collect() };
    let x = crate::dxf_ext::xdata(tags, crate::dxf_ext::APP);
    let ours = x.iter().position(|t| t.code == 1000 && t.str() == "TABLE").map(|p| x.get(p + 1..).unwrap_or(&[]));
    let (title, header, text_height, style) = match ours {
        Some(o) => {
            let flags: Vec<i64> = o.iter().filter(|t| t.code == 1070).map(Tag::i64).collect();
            let h = o.iter().find(|t| t.code == 1040).map(Tag::f64).filter(|h| h.is_finite() && *h > 0.0).unwrap_or(0.18);
            let style = o.iter().find(|t| t.code == 1000).map(Tag::str).unwrap_or_else(|| "Standard".into());
            (flags.first().is_some_and(|v| *v != 0), flags.get(1).is_some_and(|v| *v != 0), h, style)
        }
        // Other writers: a first row merged across every column reads as a title.
        // Their text height is resolved to the table style's once the styles are read (`apply`).
        None => (cols > 1 && cells.first().and_then(|r| r.first()).and_then(|c| c.merged) == Some((1, cols as u32)), false, 0.0, "Standard".into()),
    };
    let overrides = table_overrides(tail.get(..first_cell).unwrap_or(&[]));
    let mut t = Table { insert: T(tags).p(10), col_widths, row_heights, cells, style, text_height, title, header, overrides, ..Table::default() };
    crate::dxf_table::read_table_xdata(tags, &mut t);
    t
}

/// The value flags (90) and the override masks (93 to 96) of an ACAD_TABLE's own groups `head`, with the override values that follow
/// them up to the row heights.
fn table_overrides(head: &[Tag]) -> cadcraft_doc::TableOverrides {
    let int = |code: i32| head.iter().find(|t| t.code == code).and_then(|t| i32::try_from(t.i64()).ok()).unwrap_or(0);
    let values = head
        .iter()
        .skip_while(|t| t.code != 96)
        .skip(1)
        .take_while(|t| !matches!(t.code, 141 | 142))
        .map(|t| cadcraft_doc::RawTag { code: t.code, value: t.str() })
        .collect();
    cadcraft_doc::TableOverrides {
        value_flags: int(90),
        style: int(93),
        border_color: int(94),
        border_lineweight: int(95),
        border_visibility: int(96),
        values,
    }
}

/// The plot style names of a file: the entries of the `ACAD_PLOTSTYLENAME` dictionary of the
/// named object dictionary (the first object), by placeholder handle.
pub(super) fn plot_style_names(secs: &[cadcraft_dxf::Section]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(objects) = secs.iter().find(|s| s.name == "OBJECTS") else { return out };
    let recs = records(&objects.tags);
    let entries = |tags: &[Tag]| {
        let mut v: Vec<(String, String)> = Vec::new();
        let mut name: Option<String> = None;
        for t in tags {
            match t.code {
                3 => name = Some(t.str()),
                350 | 360 => {
                    if let Some(n) = name.take() {
                        v.push((n, t.str().trim().to_ascii_uppercase()));
                    }
                }
                _ => {}
            }
        }
        v
    };
    let Some((_, root)) = recs.iter().find(|(k, _)| k == "DICTIONARY") else { return out };
    let Some((_, dict)) = entries(root).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(crate::dxf_ext::PLOT_STYLE_NAMES_KEY)) else {
        return out;
    };
    let is_dict = |k: &str| k == "DICTIONARY" || k == "ACDBDICTIONARYWDFLT";
    let Some((_, tags)) = recs.iter().find(|(k, tg)| is_dict(k) && T(tg).s(5).is_some_and(|h| h.trim().eq_ignore_ascii_case(&dict))) else {
        return out;
    };
    for (n, h) in entries(tags).into_iter().take(crate::dxf_ext::MAX_PLOT_STYLE_NAMES) {
        if !n.trim().is_empty() {
            out.insert(h, n.chars().take(255).collect());
        }
    }
    out
}

/// The plot style an entity record names among its AcDbEntity groups: ByBlock (group 380 = 1), the
/// dictionary default (380 = 2) or the named style its placeholder (group 390) stands for.
pub(super) fn entity_plot_style(tags: &[Tag], rx: &Rx) -> Option<String> {
    let mut own = false;
    for (i, tag) in tags.iter().enumerate() {
        if tag.code == 1001 {
            own = tag.str() == crate::dxf_ext::APP;
        } else if own
            && tag.code == 1000
            && tag.str() == "PLOTSTYLE"
            && let Some(value) = tags.get(i + 1).filter(|t| t.code == 1070 && matches!(t.i64(), 1 | 2))
        {
            return Some(if value.i64() == 1 { "ByBlock" } else { "ByDictionaryDefault" }.into());
        }
    }
    let mut in_entity = false;
    for t in tags {
        if t.code == 100 {
            if in_entity {
                return None;
            }
            in_entity = t.str() == "AcDbEntity";
        } else if t.code == 380 && in_entity && matches!(t.i64(), 1 | 2) {
            return Some(if t.i64() == 1 { "ByBlock" } else { "ByDictionaryDefault" }.into());
        } else if t.code == 390 && in_entity {
            return rx.pstyles.get(&t.str().trim().to_ascii_uppercase()).cloned();
        }
    }
    None
}

/// Dimension overrides and associativity, table references: data of an entity record that
/// needs the file's handle maps.
pub(super) fn entity_extras(kind: &str, tags: &[Tag], h: Handle, k: &mut EntityKind, rx: &mut Rx) {
    match (kind, k) {
        ("ATTDEF", EntityKind::AttDef(a)) if rx.from_dwg => forget_dwg_type_72(a),
        ("DIMENSION", EntityKind::Dimension(dm)) => {
            dm.overrides = crate::dxf_ext::read_dstyle(tags, &rx.styles, &rx.brs, &rx.ltypes);
            dm.assoc = crate::dxf_ext::read_assoc(tags);
            if let Some(center) = crate::dxf_ext::read_arclen(tags) {
                dm.kind = DimKind::ArcLength;
                dm.p15 = center;
            }
            dm.marks = crate::dxf_ext::read_marks(tags);
            // Group 12 (in OCS) shifts the `*D` block: a dimension copied inside a block keeps
            // its old graphics and records the shift here.
            let t = T(tags);
            let shift = super::common::ocs_n(Vec3::new(t.fd(210, 0.0), t.fd(220, 0.0), t.fd(230, 1.0)), t.p(12)).xy();
            if let Some(name) = dm.block.as_ref().filter(|_| shift.is_finite() && (shift.x != 0.0 || shift.y != 0.0)) {
                rx.dim_block_shifts.push((name.to_ascii_uppercase(), shift));
            }
            // Group 52: the extension lines' angle relative to the dimension line (DIMEDIT Oblique).
            let base = match dm.kind {
                DimKind::Linear { rotation } => Some(rotation),
                DimKind::Aligned => Some((dm.p14.xy() - dm.p13.xy()).angle()),
                _ => None,
            };
            if let (Some(base), Some(o)) = (base, T(tags).f(52).filter(|o| o.is_finite() && o.rem_euclid(180.0) != 0.0)) {
                dm.marks.oblique = Some(cadcraft_geom::norm_angle(base + o.to_radians()));
            }
        }
        ("ACAD_TABLE", EntityKind::Table(table)) => {
            // The blocks the cells hold, by their BLOCK_RECORD handles; a cell whose block is unknown is a text cell.
            for cell in table.cells.iter_mut().flatten() {
                if let Some(content) = &mut cell.block {
                    match rx.brs.get(&content.block.to_ascii_uppercase()).filter(|n| !n.is_empty()) {
                        Some(name) => {
                            content.block = name.clone();
                            rx.inserted.insert(name.to_ascii_uppercase());
                        }
                        None => cell.block = None,
                    }
                }
            }
            let t = T(tags);
            if let Some(b) = t.s(2) {
                rx.table_blocks.insert(b.to_ascii_uppercase());
            }
            let ours = crate::dxf_ext::xdata(tags, crate::dxf_ext::APP).iter().any(|t| t.code == 1000 && t.str() == "TABLE");
            if !ours {
                rx.table_style_fix.push((h, t.s(342).map(|sh| sh.trim().to_ascii_uppercase()).unwrap_or_default()));
            }
        }
        ("INSERT", EntityKind::Insert(ins)) => {
            rx.inserted.insert(ins.block.to_ascii_uppercase());
        }
        ("MULTILEADER" | "MLEADER", EntityKind::MLeader(m)) => {
            let pending = crate::dxf_mleader::resolve(tags, m, &rx.styles, &rx.brs);
            if let Some(b) = &m.block {
                rx.inserted.insert(b.block.to_ascii_uppercase());
            }
            rx.mleader_fix.push((h, pending));
        }
        // CADCraft's VPFROZEN xdata wins; otherwise AutoCAD's group 331 layer handles.
        ("VIEWPORT", EntityKind::Viewport(v)) if v.frozen_layers.is_empty() => {
            v.frozen_layers = tags
                .iter()
                .take_while(|t| t.code < 1000)
                .filter(|t| t.code == 331)
                .filter_map(|t| rx.layers.get(&t.str().trim().to_ascii_uppercase()).cloned())
                .collect();
        }
        _ => {}
    }
}

/// A POLYLINE and its child records, advancing `i` past the VERTEX and SEQEND records.
pub(super) fn polyline_entity(recs: &[(String, Vec<Tag>)], i: &mut usize, t: &T, d: &mut super::Drawing, rx: &Rx) -> (Common, EntityKind) {
    polyline_entity_in(recs, i, t, d, rx, false)
}

/// [`polyline_entity`], and with `canonical` a 2D polyline keeps its OCS vertices as the records hold them instead of
/// mirroring them under a negative Z.
pub(super) fn polyline_entity_in(
    recs: &[(String, Vec<Tag>)],
    i: &mut usize,
    t: &T,
    d: &mut super::Drawing,
    rx: &Rx,
    canonical: bool,
) -> (Common, EntityKind) {
    let flags = t.i(70).unwrap_or(0);
    let mut pts: Vec<(Vec3, f64, f64, f64)> = Vec::new();
    let mut fit_vertices = Vec::new();
    let mut seqend = None;
    let mut record = |tags: &[Tag]| {
        let tt = T(tags);
        let mut c = common(&tt);
        if let Some(style) = entity_plot_style(tags, rx) {
            c.plot_style = style;
        }
        let handle = tt.s(5).and_then(|h| Handle::parse_hex(&h)).unwrap_or_else(|| d.new_handle());
        cadcraft_doc::PolylineRecord { handle: Some(handle), common: c }
    };
    let mesh = flags & (16 | 64) != 0;
    let mut mesh_vertices = Vec::new();
    while let Some((k2, t2)) = recs.get(*i) {
        if k2 == "VERTEX" {
            let tt = T(t2);
            let vflags = tt.i(70).unwrap_or(0);
            let vertex = cadcraft_doc::PolylineFitVertex {
                record: record(t2),
                position: tt.p(10),
                flags: vflags,
                start_width: tt.fd(40, t.fd(40, 0.0)),
                end_width: tt.fd(41, t.fd(41, 0.0)),
                bulge: tt.fd(42, 0.0),
                tangent: tt.fd(50, 0.0).to_radians(),
            };
            if mesh {
                mesh_vertices.push(cadcraft_doc::MeshVertex { vertex, indices: [71, 72, 73, 74].map(|c| tt.i(c).unwrap_or(0)) });
            } else {
                if vflags & 16 == 0 {
                    pts.push((vertex.position, vertex.bulge, vertex.start_width, vertex.end_width));
                }
                fit_vertices.push(vertex);
            }
            *i += 1;
        } else {
            if k2 == "SEQEND" {
                seqend = Some(record(t2));
                *i += 1;
            }
            break;
        }
    }
    let records =
        cadcraft_doc::PolylineRecords { vertices: fit_vertices.iter().filter(|v| v.flags & 16 == 0).map(|v| v.record.clone()).collect(), seqend };
    let mut c = common(t);
    if let Some(style) = entity_plot_style(t.0, rx) {
        c.plot_style = style;
    }
    let k = if mesh {
        EntityKind::PolylineMesh(cadcraft_doc::PolylineMesh {
            kind: if flags & 64 != 0 { cadcraft_doc::MeshKind::Polyface } else { cadcraft_doc::MeshKind::Polygon },
            flags: flags & !(16 | 64),
            m_count: t.i(71).unwrap_or(0),
            n_count: t.i(72).unwrap_or(0),
            m_density: t.i(73).unwrap_or(0),
            n_density: t.i(74).unwrap_or(0),
            smooth_type: t.i(75).unwrap_or(0),
            vertices: mesh_vertices,
            seqend: records.seqend,
        })
    } else if flags & 8 != 0 {
        let points: Vec<Vec3> = pts.iter().map(|p| p.0).collect();
        // Curve-fit and spline-fit data (header flags 2 and 4, curve type 75, vertices that are not plain 3D polyline vertices:
        // the frame control points and the generated curve) is kept whole, as read.
        let fitted = flags & 6 != 0 || t.i(75).unwrap_or(0) != 0 || fit_vertices.iter().any(|v| v.flags & !32 != 0);
        let fit = fitted.then(|| cadcraft_doc::Polyline3dFit {
            flags,
            smooth_type: t.i(75).unwrap_or(0),
            vertices: fit_vertices,
            display_points: points.clone(),
        });
        EntityKind::Polyline3d(Polyline3d { records, points, closed: flags & 1 != 0, fit })
    } else {
        // A 2D polyline's vertices are in its OCS: mirrored like LWPOLYLINE.
        let m = if c.extrusion.z < 0.0 && !canonical { -1.0 } else { 1.0 };
        let vertices: Vec<_> =
            pts.iter().map(|p| PolyVertex { p: Vec2::new(m * p.0.x, p.0.y), bulge: m * p.1, start_width: p.2, end_width: p.3 }).collect();
        let fit = (flags & 6 != 0 || t.i(75).unwrap_or(0) != 0 || fit_vertices.iter().any(|v| v.flags != 0 || v.tangent != 0.0)).then(|| {
            cadcraft_doc::PolylineFit {
                flags,
                smooth_type: t.i(75).unwrap_or(0),
                m_count: t.i(71).unwrap_or(0),
                n_count: t.i(72).unwrap_or(0),
                m_density: t.i(73).unwrap_or(0),
                n_density: t.i(74).unwrap_or(0),
                start_width: t.fd(40, 0.0),
                end_width: t.fd(41, 0.0),
                elevation: t.p(10).z,
                extrusion: c.extrusion,
                vertices: fit_vertices,
                display_vertices: vertices.clone(),
            }
        });
        EntityKind::LwPolyline(LwPolyline {
            records,
            vertices,
            closed: flags & 1 != 0,
            // The effective widths live on each vertex, including inherited defaults.
            const_width: 0.0,
            elevation: t.p(10).z,
            plinegen: flags & 128 != 0,
            heavy: true,
            fit,
        })
    };
    (c, k)
}

/// A multiline attribute's group 72 after its type is not in a DWG: the DWG reader writes a fixed value there.
fn forget_dwg_type_72(a: &mut Attrib) {
    if let Some(m) = &mut a.multiline {
        m.type_72 = None;
    }
}

/// The ATTRIB records after an INSERT, advancing `i` past them and their SEQEND.
pub(super) fn insert_attribs(recs: &[(String, Vec<Tag>)], i: &mut usize, rx: &Rx) -> Vec<Attrib> {
    let mut attribs = Vec::new();
    while let Some((k2, t2)) = recs.get(*i) {
        if k2 == "ATTRIB" {
            let tt = T(t2);
            // A multiline attribute has its text as embedded MTEXT after its own groups.
            let (own, embedded) = split_embedded(t2);
            let mut multiline = multiline_attrib(own, embedded, &common(&tt));
            if rx.from_dwg {
                multiline.iter_mut().for_each(|m| m.type_72 = None);
            }
            let own = T(own);
            let mut text = text_from(&own);
            if let Some(m) = &multiline {
                text.value.clone_from(&m.read_as);
            }
            attribs.push(Attrib {
                handle: tt.s(5).and_then(|h| Handle::parse_hex(&h)).filter(|h| h.0 != 0),
                invisible: own.i(70).unwrap_or(0) & 1 != 0,
                props: attrib_props(&tt, entity_plot_style(t2, rx)),
                multiline,
                modes: attrib_modes(own.0),
                plane: AttribPlane { thickness: own.fd(39, 0.0), ..AttribPlane::default() },
                ..Attrib::new(own.s(2).unwrap_or_default(), text)
            });
            *i += 1;
        } else {
            if k2 == "SEQEND" {
                *i += 1;
            }
            break;
        }
    }
    attribs
}

/// The last point of a HELIX's spline (AcDbSpline: the last control point, else fit point), from the record's groups.
pub(super) fn helix_spline_end(tags: &[Tag]) -> Option<Vec3> {
    let hs = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbHelix").unwrap_or(tags.len());
    let ss = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbSpline").unwrap_or(0).min(hs);
    let spline = tags.get(ss..hs).unwrap_or(&[]);
    // The last point of the spline (3D) with group `code`.
    let last = |code: i32| {
        let i = spline.iter().rposition(|g| g.code == code)?;
        let f = |c: i32| spline.get(i..).and_then(|r| r.iter().find(|g| g.code == c)).map(Tag::f64).unwrap_or(0.0);
        Some(Vec3::new(f(code), f(code + 10), f(code + 20)))
    };
    last(10).or_else(|| last(11))
}

/// A HELIX (DXF Reference): the axis base point, start point, axis, radius, turns, turn height,
/// handedness and constraint after the AcDbHelix marker; the top radius is how far the end of its
/// spline (AcDbSpline: the last control point, else fit point) lies from the axis.
pub(super) fn helix(tags: &[Tag]) -> Helix {
    let hs = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbHelix").unwrap_or(tags.len());
    let t = T(tags.get(hs..).unwrap_or(&[]));
    let end = helix_spline_end(tags);
    let center = t.p(10);
    let start = t.p(11);
    let axis = t.p(12);
    let axis = if axis.is_finite() && axis.len() > 1e-12 { axis.normalized() } else { Vec3::Z };
    // Distance from the axis.
    let radius = |p: Vec3| {
        let v = Vec3::new(p.x - center.x, p.y - center.y, p.z - center.z);
        let along = v.dot(axis);
        Vec3::new(v.x - axis.x * along, v.y - axis.y * along, v.z - axis.z * along).len()
    };
    let base_radius = t.f(40).filter(|r| r.is_finite() && *r >= 0.0).unwrap_or_else(|| radius(start));
    let turns = t.fd(41, 1.0);
    let height = match (t.f(42), end) {
        (Some(th), _) if th.is_finite() => th * turns,
        (_, Some(e)) => e.z - center.z,
        _ => 0.0,
    };
    Helix {
        center,
        base_radius,
        top_radius: end.map(radius).unwrap_or(base_radius),
        height,
        turns,
        start_angle: center.xy().angle_to(start.xy()),
        ccw: t.i(290).unwrap_or(1) != 0,
        constrain: HelixConstrain::from_dxf(t.i(280).unwrap_or(2)),
        frame: None,
    }
    .sanitized()
}
