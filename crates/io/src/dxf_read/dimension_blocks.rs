//! Dimension clone offsets are interpreted in the payload's own frame before source capture.

use std::collections::HashMap;

use cadcraft_doc::{Drawing, EntityKind, Handle};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

use super::Rx;
use crate::{IoError, Result};

#[derive(Clone, Copy)]
pub(super) enum BlockOffset {
    Legacy(Vec2),
    Canonical(Vec3),
}

impl BlockOffset {
    fn agrees(self, other: Self) -> bool {
        match (self, other) {
            (Self::Legacy(a), Self::Legacy(b)) => a.near(b, 1e-9),
            (Self::Canonical(a), Self::Canonical(b)) => (a - b).len() <= 1e-9,
            _ => false,
        }
    }

    fn canonical(self) -> bool {
        matches!(self, Self::Canonical(_))
    }

    fn matrix(self) -> Mat4 {
        match self {
            Self::Legacy(offset) => Mat4::from_mat3(&Mat3::translate(offset)),
            Self::Canonical(offset) => {
                let mut matrix = Mat4::IDENTITY;
                matrix.m[0][3] = offset.x;
                matrix.m[1][3] = offset.y;
                matrix.m[2][3] = offset.z;
                matrix
            }
        }
    }
}

pub(super) struct DimensionBlockShift {
    pub block: String,
    pub handle: Handle,
    pub offset: BlockOffset,
}

/// Normalize a shared picture only when all users agree. Canonical conflicts refuse before any block changes.
pub(super) fn shift_dimension_blocks(drawing: &mut Drawing, rx: &Rx) -> Result<()> {
    let mut shifts: HashMap<&str, (&DimensionBlockShift, bool)> = HashMap::new();
    for shift in &rx.dim_block_shifts {
        if let Some((first, compatible)) = shifts.get_mut(shift.block.as_str()) {
            if !first.offset.agrees(shift.offset) {
                if first.offset.canonical() || shift.offset.canonical() {
                    return Err(IoError::DimensionBlockDisplacement { block: shift.block.clone(), first: first.handle, other: shift.handle });
                }
                *compatible = false;
            }
        } else {
            shifts.insert(shift.block.as_str(), (shift, true));
        }
    }
    let mut notes = Vec::new();
    for (upper, (shift, compatible)) in shifts {
        if !compatible {
            continue;
        }
        let matrix = shift.offset.matrix();
        if matrix == Mat4::IDENTITY {
            continue;
        }
        let Some(name) = drawing.blocks.keys().find(|name| name.eq_ignore_ascii_case(upper)).cloned() else { continue };
        if let Some(block) = drawing.blocks.get_mut(&name) {
            let store = &mut std::sync::Arc::make_mut(block).entities;
            for handle in store.handles() {
                let mut entity = store.get(handle).ok_or_else(|| IoError::Format("missing dimension block entity".into()))?.as_ref().clone();
                if !shift.offset.canonical() && matches!(entity.kind, EntityKind::Unknown(_)) {
                    notes.push(format!("Dimension block {name}: entity {} has unsupported geometry. Its source geometry stays unchanged during display normalization.", handle.hex()));
                    continue;
                }
                if shift.offset.canonical() {
                    entity.require_canonical_geometry().map_err(|error| IoError::Format(error.to_string()))?;
                }
                entity.try_transform(&matrix).map_err(|error| IoError::Format(error.to_string()))?;
                store.replace(entity);
            }
        }
    }
    drawing.read_notes.extend(notes);
    Ok(())
}
