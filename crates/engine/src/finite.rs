//! Edits whose result has coordinates too large to represent (NaN or infinite) are refused.
//!
//! Finite inputs can still overflow: moving by `[1e308, 1e308]` twice, scaling by a huge factor or
//! offsetting by a huge distance. Infinite coordinates break extents, inspection and rendering, and
//! are silently changed by a DXF save and reopen, so the command fails and the drawing stays as it
//! was. Only objects the command changed are checked: every number in their serialized fields, and
//! their extents.

use std::collections::HashSet;
use std::sync::Arc;

use cadcraft_doc::{Drawing, EntityStore, Handle, has_finite_numbers};
use cadcraft_render::entity_extents;

use crate::{EngineError, Session};

impl Session {
    /// After command `cmd` changed the drawing from `before` (the document `uid`, or the active
    /// one): when an object it changed has NaN or infinite numbers, put the drawing and selection
    /// back and return the error to report.
    pub(crate) fn refuse_non_finite(&mut self, uid: Option<u64>, before: &Arc<Drawing>, selection: &[Handle], cmd: &str) -> crate::Result<()> {
        let st = match uid {
            Some(uid) => self.docs.iter_mut().find(|d| d.uid == uid),
            None => self.state_mut().ok(),
        };
        let Some(st) = st.filter(|st| !Arc::ptr_eq(before, &st.doc)) else { return Ok(()) };
        let error = match first_non_finite(before, &st.doc) {
            Ok(None) => return Ok(()),
            Ok(Some(handle)) => EngineError::BadParams { cmd: cmd.into(), msg: too_large(handle) },
            Err(refused) => EngineError::from(refused),
        };
        st.doc = before.clone();
        st.selection = selection.to_vec();
        Err(error)
    }
}

/// The message for an object whose edit result has infinite or undefined coordinates.
pub(crate) fn too_large(h: Handle) -> String {
    format!("the result is too large: object {} would get infinite or undefined coordinates. Nothing was changed", h.hex())
}

/// The first object changed between `before` and `after` that has a NaN or infinite number, or
/// whose extents overflow.
pub(crate) fn first_non_finite(before: &Drawing, after: &Drawing) -> Result<Option<Handle>, cadcraft_render::ExtentsRefusal> {
    let layouts = after.layouts.iter().map(|l| (&l.entities, before.layout(&l.name).map(|b| &b.entities)));
    let blocks = after
        .blocks
        .iter()
        .filter(|(name, b)| before.blocks.get(*name).is_none_or(|old| !Arc::ptr_eq(old, b)))
        .map(|(name, b)| (&b.entities, before.blocks.get(name).map(|old| &old.entities)));
    for (current, previous) in std::iter::once((&after.model, Some(&before.model))).chain(layouts).chain(blocks) {
        if let Some(handle) = changed_non_finite(after, previous, current)? {
            return Ok(Some(handle));
        }
    }
    Ok(None)
}

fn changed_non_finite(d: &Drawing, before: Option<&EntityStore>, after: &EntityStore) -> Result<Option<Handle>, cadcraft_render::ExtentsRefusal> {
    if before.is_some_and(|b| b.same_as(after)) {
        return Ok(None);
    }
    // Chunks shared with `before` hold only unchanged objects.
    let shared: HashSet<usize> = before.map(|b| b.chunk_slices().map(|(key, _)| key).collect()).unwrap_or_default();
    for entity in after.chunk_slices().filter(|(key, _)| !shared.contains(key)).flat_map(|(_, items)| items.iter()) {
        let previous = before.and_then(|store| store.get(entity.handle));
        if previous.is_some_and(|old| Arc::ptr_eq(old, entity)) {
            continue;
        }
        if !has_finite_numbers(entity.as_ref()) {
            return Ok(Some(entity.handle));
        }
        if previous.is_some_and(|old| {
            old.kind == entity.kind && old.common.extrusion == entity.common.extrusion && old.common.thickness == entity.common.thickness
        }) {
            continue;
        }
        let bounds = entity_extents(d, entity, 0)?;
        if !bounds.is_empty() && (!bounds.min.is_finite() || !bounds.max.is_finite()) {
            return Ok(Some(entity.handle));
        }
    }
    Ok(None)
}
