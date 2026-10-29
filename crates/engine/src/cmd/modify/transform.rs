use super::{Handle, Mat3, Result, Session, Value, Vec2, bad, bool_or, f64_req, json, point_param, point_req, targets};

// ---------------- core operations ----------------

pub(crate) fn transform_entities(s: &mut Session, hs: &[Handle], m: &Mat3, copy: bool) -> Result<Vec<Handle>> {
    if copy {
        return copy_transformed(s, hs, std::iter::once(*m));
    }
    let d = s.doc_mut()?;
    for h in hs {
        if let Some(e) = d.entity(*h)
            && !d.layer(&e.common.layer).is_some_and(|l| l.locked)
        {
            check_ellipse_transform(e, m)?;
        }
    }
    let mut out = Vec::new();
    for h in hs {
        let locked = d.entity(*h).and_then(|e| d.layer(&e.common.layer)).is_some_and(|l| l.locked);
        if locked {
            continue;
        }
        if d.space_of(*h).is_some() {
            d.try_modify_entity(*h, |e| e.try_transform(&cadcraft_geom::Mat4::from_mat3(m)))?;
            out.push(*h);
        }
    }
    Ok(out)
}

fn copy_transformed(s: &mut Session, hs: &[Handle], transforms: impl IntoIterator<Item = Mat3>) -> Result<Vec<Handle>> {
    let d = s.doc_mut()?;
    let mut seen = std::collections::HashSet::new();
    let originals: Vec<_> = hs.iter().filter(|h| seen.insert(**h)).filter_map(|h| Some((d.space_of(*h)?, (**d.entity(*h)?).clone()))).collect();
    let transforms: Vec<_> = transforms.into_iter().collect();
    for m in &transforms {
        for (_, entity) in &originals {
            check_ellipse_transform(entity, m)?;
        }
    }
    let sets = transforms
        .into_iter()
        .map(|m| originals.iter().map(|(_, e)| e.transformed(&cadcraft_geom::Mat4::from_mat3(&m))).collect::<cadcraft_doc::Result<Vec<_>>>())
        .collect::<cadcraft_doc::Result<Vec<_>>>()?;
    let copies = d.duplicate_entities(sets, cadcraft_doc::DuplicationSource::SameDrawing);
    let mut out = Vec::new();
    for (entity, (space, _)) in copies.into_iter().zip(originals.iter().cycle()) {
        out.push(entity.handle);
        if let Some(store) = d.space_mut(space) {
            store.push(entity);
        }
    }
    Ok(out)
}

pub(crate) fn check_ellipse_transform(entity: &cadcraft_doc::Entity, m: &Mat3) -> Result<()> {
    if !(m.a == 1.0 && m.b == 0.0 && m.c == 0.0 && m.d == 1.0)
        && let cadcraft_doc::EntityKind::Ellipse(ellipse) = &entity.kind
        && !ellipse.clone().flatten()
    {
        return Err(bad("transform", "An edge-on or invalid ellipse cannot be edited in the 2D plane."));
    }
    Ok(())
}

pub(crate) fn check_block_expansion(d: &cadcraft_doc::Drawing, entity: &cadcraft_doc::Entity, m: &Mat3, depth: usize) -> Result<()> {
    if depth > cadcraft_doc::MAX_BLOCK_DEPTH {
        return Err(bad("transform", "The block references nest too deeply to expand."));
    }
    check_ellipse_transform(entity, m)?;
    if let cadcraft_doc::EntityKind::Insert(insert) = &entity.kind
        && let Some(block) = d.block(&insert.block)
    {
        let m = m.then_before(insert.transform(block.base.xy()));
        for child in block.entities.iter() {
            check_block_expansion(d, child, &m, depth + 1)?;
        }
    }
    Ok(())
}

pub(super) fn erase(s: &mut Session, hs: &[Handle]) -> Result<usize> {
    let d = s.doc_mut()?;
    let mut n = 0;
    for h in hs {
        let locked = d.entity(*h).and_then(|e| d.layer(&e.common.layer)).is_some_and(|l| l.locked);
        if !locked && d.remove_entity(*h).is_some() {
            n += 1;
        }
    }
    s.set_selection(Vec::new());
    Ok(n)
}

pub(super) fn run_erase(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let n = erase(s, &hs)?;
    Ok(json!({ "erased": n }))
}

pub(super) fn delta_of(cmd: &str, p: &Value) -> Result<Vec2> {
    if let Some(d) = point_param(p, "delta") {
        return Ok(d);
    }
    let a = point_req(cmd, p, "from")?;
    let b = point_req(cmd, p, "to")?;
    Ok(b - a)
}

pub(super) fn run_move(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let d = delta_of("move", p)?;
    let moved = transform_entities(s, &hs, &Mat3::translate(d), false)?;
    Ok(json!({ "moved": moved.len() }))
}

pub(super) fn run_copy(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let d = delta_of("copy", p)?;
    let n = p.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, 10_000);
    let out = copy_transformed(s, &hs, (1..=n).map(|k| Mat3::translate(d * k as f64)))?;
    Ok(json!({ "handles": out.into_iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

pub(super) fn run_rotate(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = point_req("rotate", p, "base")?;
    let a = f64_req("rotate", p, "angle")?.to_radians();
    let r = transform_entities(s, &hs, &Mat3::rotate_about(base, a), bool_or(p, "copy", false))?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

pub(super) fn run_scale(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = point_req("scale", p, "base")?;
    let f = f64_req("scale", p, "factor")?;
    if f <= 0.0 {
        return Err(bad("scale", "factor must be positive"));
    }
    let r = transform_entities(s, &hs, &Mat3::scale_about(base, f), bool_or(p, "copy", false))?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

pub(super) fn run_mirror(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let a = point_req("mirror", p, "p1")?;
    let b = point_req("mirror", p, "p2")?;
    if a.near(b, 1e-12) {
        return Err(bad("mirror", "mirror line points coincide"));
    }
    let erase_src = bool_or(p, "erase", false);
    let r = transform_entities(s, &hs, &Mat3::mirror(a, b), !erase_src)?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}
