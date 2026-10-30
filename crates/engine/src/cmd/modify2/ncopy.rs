use super::{
    Accept, Entity, EntityKind, Handle, Input, Interactive, MAX_BLOCK_DEPTH, MAX_GEN, Mat3, Prompt, Result, Session, Step, Value, Vec2, bad, curves,
    json, other, point_param, targets,
};

// =====================================================================================
// NCOPY
// =====================================================================================

struct NestedLeaf {
    entity: Entity,
    /// The INSERT path distinguishes repeated occurrences of the same definition's handles.
    occurrence: Vec<Handle>,
}

/// The leaf objects of a block reference in world coordinates (nested references expanded).
fn nested_leaves(d: &cadcraft_doc::Drawing, ins: &cadcraft_doc::Insert, m: &Mat3, path: &mut Vec<Handle>, out: &mut Vec<NestedLeaf>) -> Result<()> {
    if path.len() > MAX_BLOCK_DEPTH || out.len() >= MAX_GEN {
        return Ok(());
    }
    let Some(b) = d.block(&ins.block) else { return Ok(()) };
    let mm = m.then_before(ins.transform(b.base.xy()));
    for e in b.entities.iter() {
        match &e.kind {
            EntityKind::Insert(inner) => {
                path.push(e.handle);
                nested_leaves(d, inner, &mm, path, out)?;
                path.pop();
            }
            EntityKind::AttDef(_) => {}
            _ => {
                let copy = e.transformed(&cadcraft_geom::Mat4::from_mat3(&mm))?;
                out.push(NestedLeaf { entity: copy, occurrence: path.clone() });
            }
        }
    }
    Ok(())
}

fn leaves_of(s: &Session, h: Handle) -> Result<Vec<NestedLeaf>> {
    let e = curves::entity(s, h)?;
    let EntityKind::Insert(ins) = &e.kind else { return Err(other("Select a block reference.")) };
    crate::cmd::modify::check_block_expansion(s.doc()?, &e, &Mat3::IDENTITY, 0)?;
    let mut out = Vec::new();
    nested_leaves(s.doc()?, ins, &Mat3::IDENTITY, &mut vec![h], &mut out)?;
    Ok(out)
}

fn nearest_leaf(leaves: Vec<NestedLeaf>, p: Vec2) -> Option<NestedLeaf> {
    leaves.into_iter().min_by(|a, b| curves::kind_distance(&a.entity.kind, p).total_cmp(&curves::kind_distance(&b.entity.kind, p)))
}

fn place_copies(s: &mut Session, items: &[NestedLeaf], delta: Vec2) -> Result<Vec<Handle>> {
    let space = s.space();
    let m = Mat3::translate(delta);
    let mut sets: std::collections::BTreeMap<&[Handle], Vec<Entity>> = Default::default();
    let mut selected = std::collections::HashSet::new();
    for source in items {
        if !selected.insert((source.occurrence.as_slice(), source.entity.handle)) {
            continue;
        }
        let mut copy = source.entity.clone();
        copy.try_transform(&cadcraft_geom::Mat4::from_mat3(&m))?;
        sets.entry(&source.occurrence).or_default().push(copy);
    }
    let d = s.doc_mut()?;
    let copies = d.duplicate_entities(sets.into_values().collect(), cadcraft_doc::DuplicationSource::SameDrawing)?;
    let store = d.space_mut(&space).ok_or_else(|| other("no such space"))?;
    let mut out = Vec::with_capacity(copies.len());
    for copy in copies {
        out.push(copy.handle);
        store.push(copy);
    }
    Ok(out)
}

pub(super) fn run_ncopy(s: &mut Session, p: &Value) -> Result<Value> {
    let h = targets(s, p)?.first().copied().ok_or_else(|| bad("ncopy", "`handle` (block reference) is required"))?;
    let leaves = leaves_of(s, h)?;
    let items: Vec<NestedLeaf> = match point_param(p, "pick") {
        Some(q) => nearest_leaf(leaves, q).into_iter().collect(),
        None => leaves,
    };
    if items.is_empty() {
        return Err(other("The block reference has no nested objects."));
    }
    let delta = match (point_param(p, "delta"), point_param(p, "from"), point_param(p, "to")) {
        (Some(d), _, _) => d,
        (None, Some(a), Some(b)) => b - a,
        _ => Vec2::ZERO,
    };
    let hs = place_copies(s, &items, delta)?;
    Ok(json!({ "handles": hs.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

#[derive(Default)]
pub(super) struct NcopyM {
    items: Vec<NestedLeaf>,
    selecting_done: bool,
    base: Option<Vec2>,
}

impl Interactive for NcopyM {
    fn name(&self) -> &'static str {
        "NCOPY"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        s.set_selection(Vec::new());
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.selecting_done, self.base) {
            (false, _) => Prompt::new("Select nested objects to copy or", Accept::POINT).kw(&["Settings"]),
            (true, None) => Prompt::new("Specify base point or", Accept::POINT).kw(&["Displacement", "Multiple"]).default("Displacement"),
            (true, Some(b)) => Prompt::new("Specify second point or <use first point as displacement>", Accept::POINT).base(b),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if !self.selecting_done {
            return match i {
                Input::Point(p) => {
                    let h = curves::pick_at(s, p).ok_or_else(|| other("*Invalid selection*"))?;
                    let leaf = nearest_leaf(leaves_of(s, h)?, p).ok_or_else(|| other("Nothing nested there."))?;
                    s.echo(format!("Nested object: {}", leaf.entity.kind.type_name()));
                    if self.items.len() < MAX_GEN {
                        self.items.push(leaf);
                    }
                    Ok(Step::Continue)
                }
                Input::Keyword(_) => {
                    s.echo("Insert mode: Copy (nested objects are copied as independent objects).");
                    Ok(Step::Continue)
                }
                Input::Enter => {
                    if self.items.is_empty() {
                        return Ok(Step::Cancel);
                    }
                    self.selecting_done = true;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            };
        }
        match (self.base, i) {
            (None, Input::Point(p)) => {
                self.base = Some(p);
                Ok(Step::Continue)
            }
            (None, Input::Enter | Input::Keyword(_)) => {
                place_copies(s, &self.items, Vec2::ZERO)?;
                Ok(Step::Done)
            }
            (Some(b), Input::Point(p)) => {
                place_copies(s, &self.items, p - b)?;
                Ok(Step::Done)
            }
            (Some(b), Input::Enter) => {
                place_copies(s, &self.items, b)?;
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }
    fn preview_objects(&self, _s: &Session, c: Vec2) -> Vec<Entity> {
        let Some(b) = self.base else { return Vec::new() };
        let m = Mat3::translate(c - b);
        self.items.iter().take(500).filter_map(|entity| entity.entity.transformed(&cadcraft_geom::Mat4::from_mat3(&m)).ok()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Circle, GeometryRevision};
    use cadcraft_geom::{OcsFrame, Vec3};

    #[test]
    fn nested_copy_preview_keeps_the_leaf_revision_and_plane() {
        for canonical in [false, true] {
            let mut entity = Entity::new(Handle(0x100), EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }));
            if canonical {
                entity.geometry_revision = GeometryRevision::CanonicalV2;
            }
            entity.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
            let common = entity.common.clone();
            let machine =
                NcopyM { items: vec![NestedLeaf { entity, occurrence: vec![Handle(0x200)] }], selecting_done: true, base: Some(Vec2::ZERO) };
            let preview = machine.preview_objects(&Session::new(), Vec2::new(3.0, 2.0));
            let entity = &preview[0];
            assert_eq!(entity.geometry_revision, if canonical { GeometryRevision::CanonicalV2 } else { GeometryRevision::LegacyProjectedV1 });
            assert_eq!(entity.common, common);
            let center = match &entity.kind {
                EntityKind::Circle(circle) if canonical => OcsFrame::new(common.extrusion).map(|frame| frame.to_world(circle.center)),
                EntityKind::Circle(circle) => Some(circle.center),
                _ => None,
            };
            let want = if canonical { Vec3::new(2.0, 3.4, 5.2) } else { Vec3::new(4.0, 4.0, 5.0) };
            assert!(center.is_some_and(|center| (center - want).len() < 1e-9));
        }
    }
}
