//! Revision admission shared by solver entry points and direct model queries.
use crate::{SolveError, model::MAX_RIGID_WORK};
use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, MAX_BLOCK_DEPTH};
use std::collections::HashSet;

pub fn require_legacy_inputs(drawing: &Drawing, handles: impl IntoIterator<Item = Handle>) -> Result<(), SolveError> {
    Admission::new(drawing).check(handles)
}

pub(crate) struct Admission<'a> {
    drawing: &'a Drawing,
    visited: HashSet<&'a str>,
    remaining: usize,
}
impl<'a> Admission<'a> {
    pub(crate) fn new(drawing: &'a Drawing) -> Self {
        Self { drawing, visited: HashSet::new(), remaining: MAX_RIGID_WORK }
    }
    pub(crate) fn drawing(&self) -> &'a Drawing {
        self.drawing
    }
    pub(crate) fn check(&mut self, handles: impl IntoIterator<Item = Handle>) -> Result<(), SolveError> {
        for handle in handles {
            if let Some(entity) = self.drawing.entity(handle) {
                self.entity(entity, 0)?;
            }
        }
        Ok(())
    }
    pub(crate) fn entity(&mut self, entity: &Entity, depth: usize) -> Result<(), SolveError> {
        self.remaining = self.remaining.checked_sub(1).ok_or(SolveError::TooLarge)?;
        entity.require_legacy_geometry().map_err(|_| SolveError::CanonicalGeometry { handle: entity.handle })?;
        if depth > MAX_BLOCK_DEPTH {
            return Err(SolveError::TooLarge);
        }
        if let EntityKind::Insert(insert) = &entity.kind
            && let Some(block) = self.drawing.block(&insert.block)
            && self.visited.insert(block.name.as_str())
        {
            let result = block.entities.iter().try_for_each(|child| self.entity(child, depth + 1));
            if result.is_err() {
                self.visited.remove(block.name.as_str());
            }
            result?;
        }
        Ok(())
    }
}
