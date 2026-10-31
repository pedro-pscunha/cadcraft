use super::{Entity, EntityKind, Handle, PolyVertex, Polyline, Result, Segment, Session, Value, Vec2, arc, json, line, lwpoly, targets, v3};

// ---------------- explode / array / order / break / join ----------------

fn explode_block_ref(d: &cadcraft_doc::Drawing, e: &Entity, ins: &cadcraft_doc::Insert) -> Result<Option<Vec<Entity>>> {
    // AutoCAD refuses to explode a MINSERT (a block reference with rows or columns): "the object is minserted".
    if ins.cols > 1 || ins.rows > 1 {
        return Err(crate::EngineError::BadParams {
            cmd: "explode".into(),
            msg: format!("object {} is a MINSERT ({} columns by {} rows): a block array cannot be exploded", e.handle.hex(), ins.cols, ins.rows),
        });
    }
    if let Some(items) = crate::cmd::array::explode_array(d, e)? {
        return Ok(Some(items));
    }
    let Some(blk) = d.block(&ins.block) else { return Ok(None) };
    let m = ins.transform(blk.base.xy());
    Ok(Some(
        blk.entities
            .iter()
            .map(|be| {
                let mut ne = be.transformed(&cadcraft_geom::Mat4::from_mat3(&m))?;
                if ne.common.layer == "0" {
                    ne.common.layer = e.common.layer.clone();
                }
                if ne.common.color == cadcraft_color::Color::ByBlock {
                    ne.common.color = e.common.color;
                }
                Ok(ne)
            })
            .collect::<Result<Vec<_>>>()?,
    ))
}

fn explode_mtext(e: &Entity, m: &cadcraft_doc::MText) -> Vec<Entity> {
    cadcraft_fonts::plain_mtext(&m.contents)
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let at = m.insert.xy() + Vec2::from_angle(m.rotation - std::f64::consts::FRAC_PI_2) * (m.height * (1.0 + 5.0 / 3.0 * i as f64));
            Entity {
                geometry_revision: Default::default(),
                handle: Handle(0),
                common: e.common.clone(),
                kind: EntityKind::Text(cadcraft_doc::Text {
                    insert: v3(at),
                    align_pt: None,
                    height: m.height,
                    value: l.into(),
                    rotation: m.rotation,
                    width_factor: 1.0,
                    oblique: 0.0,
                    style: m.style.clone(),
                    halign: Default::default(),
                    valign: Default::default(),
                    backward: false,
                    upside_down: false,
                }),
            }
        })
        .collect()
}

/// The entities of a boundary's lines and arcs, on the object's layer and style.
fn segment_entities(e: &Entity, segs: impl IntoIterator<Item = Segment>) -> Vec<Entity> {
    segs.into_iter()
        .map(|s| {
            let k = match s {
                Segment::Line(l) => line(l.a, l.b),
                Segment::Arc { arc: a, .. } => arc(&a),
            };
            Entity { geometry_revision: Default::default(), handle: Handle(0), common: e.common.clone(), kind: k }
        })
        .collect()
}

/// The ARC that a run of points lies on from its first to its last point through the middle one, when every point is on that circle.
fn arc_through(points: &[Vec2]) -> Option<cadcraft_geom::Arc> {
    let (first, last) = (*points.first()?, *points.last()?);
    let arc = cadcraft_geom::Arc::from_3_points(first, *points.get(points.len() / 2)?, last)?;
    let tolerance = 1e-6 * (1.0 + arc.radius);
    points.iter().all(|p| (p.dist(arc.center) - arc.radius).abs() <= tolerance).then_some(arc)
}

/// The pieces of the dimension `dm` as an EXPLODE leaves them (and as a dimension made with DIMASSOC 0 is): LINEs, ARCs, SOLID arrowheads and
/// the MTEXT, on the properties of `e`. A run of straight segments is one LINE per segment, a run on a circle one ARC; a closed run (the
/// inspection frame) stays a polyline.
pub(crate) fn explode_dimension(d: &cadcraft_doc::Drawing, e: &Entity, dm: &cadcraft_doc::Dimension) -> Vec<Entity> {
    // The dimension as drawn: its overrides, DIMSCALE (0 = the drawing's) and text font.
    let style = d.dim_style(&dm.style).cloned().unwrap_or_default().with_overrides(&dm.overrides);
    let g = cadcraft_render::dimension_in(d, dm);
    let piece = |kind: EntityKind| Entity { geometry_revision: Default::default(), handle: Handle(0), common: e.common.clone(), kind };
    let line_entity = |l: &Vec<Vec2>| -> Vec<Entity> {
        if l.len() == 2 {
            l.windows(2).map(|w| piece(line(w[0], w[1]))).collect()
        } else if l.first() == l.last() {
            vec![piece(lwpoly(l.iter().map(|p| PolyVertex::new(*p)).collect(), false))]
        } else if let Some(a) = arc_through(l) {
            vec![piece(arc(&a))]
        } else {
            l.windows(2).map(|w| piece(line(w[0], w[1]))).collect()
        }
    };
    // The text's marks (arc length symbol, DIMINSPECT frame) go after the text background.
    let text_marks = &g.text_marks;
    let marks = |want: bool| g.lines.iter().enumerate().filter(move |(i, _)| text_marks.contains(i) == want).map(|(_, l)| l);
    let mut v: Vec<Entity> = marks(false).flat_map(&line_entity).collect();
    for t in &g.fills {
        if let [a, b, c] = t.as_slice() {
            v.push(Entity {
                geometry_revision: Default::default(),
                handle: Handle(0),
                common: e.common.clone(),
                kind: EntityKind::Solid(cadcraft_doc::Solid { corners: [v3(*a), v3(*b), v3(*c), v3(*c)] }),
            });
        }
    }
    // DIMTFILL 2: the text background as a SOLID in DIMTFILLCLR, over the lines and
    // arrowheads, under the text and its marks (SOLID corners go 1, 2, 4, 3).
    if style.text_fill == 2
        && let [p0, p1, p2, p3] = g.text_fill.as_slice()
    {
        let fill = style.text_fill_color;
        let color = if fill == cadcraft_doc::color::Color::ByBlock { e.common.color } else { fill };
        v.push(Entity {
            geometry_revision: Default::default(),
            handle: Handle(0),
            common: cadcraft_doc::Common { color, ..e.common.clone() },
            kind: EntityKind::Solid(cadcraft_doc::Solid { corners: [v3(*p0), v3(*p1), v3(*p3), v3(*p2)] }),
        });
    }
    v.extend(marks(true).flat_map(&line_entity));
    if !g.value.is_empty() {
        v.push(Entity {
            geometry_revision: Default::default(),
            handle: Handle(0),
            common: e.common.clone(),
            kind: EntityKind::MText(cadcraft_doc::MText {
                insert: v3(g.text_pos),
                height: g.text_height,
                width: 0.0,
                attach: 5,
                rotation: g.text_angle,
                style: style.text_style.clone(),
                contents: g.mtext_contents(),
                line_spacing: 1.0,
                line_spacing_exact: false,
                columns: None,
                direction: None,
                drawing_direction: None,
            }),
        });
    }
    v
}

pub(crate) fn explode_kind(d: &cadcraft_doc::Drawing, e: &Entity) -> Result<Option<Vec<Entity>>> {
    // An annotative object explodes into plain objects as it draws at the current annotation scale.
    if cadcraft_doc::is_annotative(e) {
        let mut se = match d.anno_draw(e) {
            cadcraft_doc::AnnoDraw::Scaled(f) => cadcraft_doc::anno_scaled(e, f),
            _ => e.clone(),
        };
        se.common.anno_scales.clear();
        return explode_kind(d, &se);
    }
    let pieces = match &e.kind {
        EntityKind::LwPolyline(p) => {
            let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
            Some(segment_entities(e, pl.segments()))
        }
        EntityKind::Insert(ins) => return explode_block_ref(d, e, ins),
        // A helix explodes into its spline (seen from above).
        EntityKind::Helix(h) => Some(vec![Entity {
            geometry_revision: Default::default(),
            handle: Handle(0),
            common: e.common.clone(),
            kind: EntityKind::Spline(h.spline()),
        }]),
        EntityKind::Polyline3d(p) => Some(
            p.points
                .windows(2)
                .filter_map(|w| {
                    let kind = EntityKind::Line(cadcraft_doc::Line { a: *w.first()?, b: *w.get(1)? });
                    Some(Entity { geometry_revision: Default::default(), handle: Handle(0), common: e.common.clone(), kind })
                })
                .collect(),
        ),
        EntityKind::MLine(m) => Some(crate::cmd::draw2::mline::explode(e, m)),
        EntityKind::MText(m) => Some(explode_mtext(e, m)),
        // A region explodes into its boundary's lines and arcs.
        EntityKind::Region(r) => Some(segment_entities(e, r.segments())),
        EntityKind::Dimension(dm) => Some(explode_dimension(d, e, dm)),
        EntityKind::Tolerance(t) => Some(crate::cmd::tolerance::explode(d, e, t)),
        _ => None,
    };
    Ok(pieces)
}

pub(super) fn explode(s: &mut Session, hs: &[Handle]) -> Result<Vec<Handle>> {
    explode_with(s, hs, explode_kind)
}

/// Replace each of `hs` (off locked layers) by the pieces `pieces` gives for it; objects it gives
/// none for are left alone. EXPLODE, and BURST with its own pieces for block references.
pub(crate) fn explode_with(
    s: &mut Session,
    hs: &[Handle],
    pieces: impl Fn(&cadcraft_doc::Drawing, &Entity) -> Result<Option<Vec<Entity>>>,
) -> Result<Vec<Handle>> {
    let space = s.space();
    let mut sets = Vec::new();
    let mut removed = Vec::new();
    for h in hs {
        if crate::cmd::curves::is_locked(s, *h) {
            continue;
        }
        let Some(e) = s.doc()?.entity(*h).map(|e| (**e).clone()) else { continue };
        super::check_block_expansion(s.doc()?, &e, &cadcraft_geom::Mat3::IDENTITY, 0)?;
        let Some(mut parts) = pieces(s.doc()?, &e)? else { continue };
        if !matches!(e.kind, EntityKind::Insert(_)) {
            for p in &mut parts {
                p.common.xdata.clear();
            }
        }
        sets.push(parts);
        removed.push(*h);
    }
    let d = s.doc_mut()?;
    let copies = d.duplicate_entities(sets, cadcraft_doc::DuplicationSource::SameDrawing)?;
    for h in removed {
        d.remove_entity(h);
    }
    let mut out = Vec::new();
    for p in copies {
        out.push(p.handle);
        if let Some(store) = d.space_mut(&space) {
            store.push(p);
        }
    }
    s.set_selection(Vec::new());
    Ok(out)
}

pub(super) fn run_explode(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let r = explode(s, &hs)?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}
