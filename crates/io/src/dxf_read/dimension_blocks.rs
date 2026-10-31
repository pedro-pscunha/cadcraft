//! Legacy display normalization. Canonical dimensions retain their own source-block placement.

use std::collections::HashMap;

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_geom::{Mat3, Mat4, Vec2};

use super::Rx;
use crate::{IoError, Result};

pub(super) struct DimensionBlockShift {
    pub block: String,
    pub offset: Vec2,
}

pub(super) fn shift_dimension_blocks(drawing: &mut Drawing, rx: &Rx) -> Result<()> {
    let mut shifts: HashMap<&str, (&DimensionBlockShift, bool)> = HashMap::new();
    for shift in &rx.dim_block_shifts {
        if let Some((first, compatible)) = shifts.get_mut(shift.block.as_str()) {
            if !first.offset.near(shift.offset, 1e-9) {
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
        let matrix = Mat4::from_mat3(&Mat3::translate(shift.offset));
        if matrix == Mat4::IDENTITY {
            continue;
        }
        let Some(name) = drawing.blocks.keys().find(|name| name.eq_ignore_ascii_case(upper)).cloned() else { continue };
        if let Some(block) = drawing.blocks.get_mut(&name) {
            let store = &mut std::sync::Arc::make_mut(block).entities;
            for handle in store.handles() {
                let mut entity = store.get(handle).ok_or_else(|| IoError::Format("missing dimension block entity".into()))?.as_ref().clone();
                if matches!(entity.kind, EntityKind::Unknown(_)) {
                    notes.push(format!("Dimension block {name}: entity {} has unsupported geometry. Its source geometry stays unchanged during display normalization.", handle.hex()));
                    continue;
                }
                entity.try_transform(&matrix).map_err(|error| IoError::Format(error.to_string()))?;
                store.replace(entity);
            }
        }
    }
    drawing.read_notes.extend(notes);
    Ok(())
}
