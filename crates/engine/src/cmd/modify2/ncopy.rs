use super::{
    Accept, Entity, EntityKind, Handle, Input, Interactive, MAX_BLOCK_DEPTH, MAX_GEN, Mat3, Prompt, Result, Session, Step, Value, Vec2, bad, curves,
    json, other, point_param, targets,
};

// =====================================================================================
// NCOPY
// =====================================================================================

/// The leaf objects of a block reference in world coordinates (nested references expanded).
fn nested_leaves(d: &cadcraft_doc::Drawing, ins: &cadcraft_doc::Insert, m: &Mat3, depth: usize, out: &mut Vec<Entity>) {
    if depth >= MAX_BLOCK_DEPTH || out.len() >= MAX_GEN {
        return;
    }
    let Some(b) = d.block(&ins.block) else { return };
    let mm = m.then_before(ins.transform(b.base.xy()));
    for e in b.entities.iter() {
        match &e.kind {
            EntityKind::Insert(inner) => nested_leaves(d, inner, &mm, depth + 1, out),
            EntityKind::AttDef(_) => {}
            k => {
                let mut copy = (**e).clone();
                copy.kind = k.clone();
                copy.kind.transform(&mm);
                out.push(copy);
            }
        }
    }
}

fn leaves_of(s: &Session, h: Handle) -> Result<Vec<Entity>> {
    let e = curves::entity(s, h)?;
    let EntityKind::Insert(ins) = &e.kind else { return Err(other("Select a block reference.")) };
    let mut out = Vec::new();
    nested_leaves(s.doc()?, ins, &Mat3::IDENTITY, 0, &mut out);
    Ok(out)
}

fn nearest_leaf(leaves: Vec<Entity>, p: Vec2) -> Option<Entity> {
    leaves.into_iter().min_by(|a, b| curves::kind_distance(&a.kind, p).total_cmp(&curves::kind_distance(&b.kind, p)))
}

fn place_copies(s: &mut Session, items: &[Entity], delta: Vec2) -> Result<Vec<Handle>> {
    let space = s.space();
    let m = Mat3::translate(delta);
    let sets = items
        .iter()
        .map(|source| {
            let mut copy = source.clone();
            copy.kind.transform(&m);
            vec![copy]
        })
        .collect();
    let d = s.doc_mut()?;
    let copies = d.duplicate_entities(sets, cadcraft_doc::DuplicationSource::SameDrawing);
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
    let items: Vec<Entity> = match point_param(p, "pick") {
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
    items: Vec<Entity>,
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
                    s.echo(format!("Nested object: {}", leaf.kind.type_name()));
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
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        let Some(b) = self.base else { return Vec::new() };
        let m = Mat3::translate(c - b);
        self.items
            .iter()
            .take(500)
            .map(|entity| {
                let mut k = entity.kind.clone();
                k.transform(&m);
                k
            })
            .collect()
    }
}
