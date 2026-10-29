//! Edits whose result has coordinates too large to represent (NaN or infinite) are refused.
//!
//! Finite inputs can still overflow: moving by `[1e308, 1e308]` twice, scaling by a huge factor or
//! offsetting by a huge distance. Infinite coordinates break extents, inspection and rendering, and
//! are silently changed by a DXF save and reopen, so the command fails and the drawing stays as it
//! was. Only objects the command changed are checked: every number in their serialized fields, and
//! their extents.

use std::collections::HashSet;
use std::sync::Arc;

use cadcraft_doc::{Drawing, Entity, EntityStore, Handle, entity_bounds, has_finite_numbers};

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
        let Some(h) = first_non_finite(before, &st.doc) else { return Ok(()) };
        st.doc = before.clone();
        st.selection = selection.to_vec();
        Err(EngineError::BadParams {
            cmd: cmd.into(),
            msg: format!("the result is too large: object {} would get infinite or undefined coordinates. Nothing was changed", h.hex()),
        })
    }
}

/// The first object changed between `before` and `after` that has a NaN or infinite number, or
/// whose extents overflow.
pub(crate) fn first_non_finite(before: &Drawing, after: &Drawing) -> Option<Handle> {
    let layouts = after.layouts.iter().map(|l| (&l.entities, before.layout(&l.name).map(|b| &b.entities)));
    let blocks = after
        .blocks
        .iter()
        .filter(|(name, b)| before.blocks.get(*name).is_none_or(|old| !Arc::ptr_eq(old, b)))
        .map(|(name, b)| (&b.entities, before.blocks.get(name).map(|old| &old.entities)));
    std::iter::once((&after.model, Some(&before.model))).chain(layouts).chain(blocks).find_map(|(a, b)| changed_non_finite(after, b, a))
}

fn changed_non_finite(d: &Drawing, before: Option<&EntityStore>, after: &EntityStore) -> Option<Handle> {
    if before.is_some_and(|b| b.same_as(after)) {
        return None;
    }
    // Chunks shared with `before` hold only unchanged objects.
    let shared: HashSet<usize> = before.map(|b| b.chunk_slices().map(|(key, _)| key).collect()).unwrap_or_default();
    let finite = |e: &Entity| {
        let b = entity_bounds(d, e, 0);
        has_finite_numbers(e) && (b.is_empty() || (b.min.is_finite() && b.max.is_finite()))
    };
    after
        .chunk_slices()
        .filter(|(key, _)| !shared.contains(key))
        .flat_map(|(_, items)| items.iter())
        .filter(|e| before.and_then(|b| b.get(e.handle)).is_none_or(|old| !Arc::ptr_eq(old, e)))
        .find(|e| !finite(e))
        .map(|e| e.handle)
}
