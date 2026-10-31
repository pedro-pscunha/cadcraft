//! Opaque source compounds have one emitter; followers are never standalone Unknown entities.

use cadcraft_doc::{Entity, Handle, RawCompound, RawRecord};
use cadcraft_dxf::Tag;

use super::W;

pub(crate) fn changed_properties(raw: &RawCompound, entity: &Entity) -> Vec<i32> {
    let before = raw.source_common();
    let after = &entity.common;
    let mut codes = Vec::new();
    if before.layer != after.layer {
        codes.push(8);
    }
    if before.color != after.color {
        codes.extend([62, 420]);
    }
    if before.linetype != after.linetype {
        codes.push(6);
    }
    if before.lineweight != after.lineweight {
        codes.push(370);
    }
    if before.ltscale.to_bits() != after.ltscale.to_bits() {
        codes.push(48);
    }
    if before.visible != after.visible {
        codes.push(60);
    }
    if before.transparency != after.transparency {
        codes.push(440);
    }
    if before.plot_style != after.plot_style {
        codes.extend([380, 390]);
    }
    codes
}

fn source_record(record: &RawRecord) -> Vec<Tag> {
    std::iter::once(Tag::s(0, record.kind())).chain(record.tags().iter().map(|tag| Tag::s(tag.code, &tag.value))).collect()
}

/// DXF 320–329 hold arbitrary handles, not translated object relationships (DXF Reference).
pub(crate) fn raw_reference(code: i32) -> bool {
    matches!(code, 330..=369 | 390..=399 | 480..=481 | 1005)
}

/// Remap only references whose target identity changed. Keep all other source text, including numeric spelling.
pub(crate) fn remap_raw_references(tags: &[Tag], mut remap: impl FnMut(&str) -> Option<String>, out: &mut Vec<Tag>) {
    let mut depth = 0usize;
    let mut owner_seen = false;
    for tag in tags {
        if tag.code == 102 {
            if tag.str().trim().starts_with('{') {
                depth += 1;
            } else if tag.str().trim() == "}" {
                depth -= 1;
            }
        }
        let owner = tag.code == 330 && depth == 0 && !owner_seen;
        owner_seen |= owner;
        if raw_reference(tag.code)
            && !owner
            && let Some(mapped) = remap(&tag.str())
            && Handle::parse_hex(&mapped) != Handle::parse_hex(&tag.str())
        {
            out.push(Tag::s(tag.code, mapped));
        } else {
            out.push(tag.clone());
        }
    }
}

pub(super) fn emit(w: &mut W, entity: &Entity, raw: &RawCompound, owner: &str, paper: bool) {
    let changed = changed_properties(raw, entity);
    let replacement = if changed.is_empty() {
        Vec::new()
    } else {
        let mut props = W { pstyles: w.pstyles.clone(), ..Default::default() };
        super::common::common(&mut props, entity, owner, paper, "");
        props.t.into_iter().filter(|t| changed.contains(&t.code)).collect()
    };
    let mut parent = source_record(raw.parent());
    let mut depth = 0usize;
    let mut owner_written = false;
    for tag in &mut parent {
        if tag.code == 102 {
            if tag.str().trim().starts_with('{') {
                depth += 1;
            } else if tag.str().trim() == "}" {
                depth -= 1;
            }
        } else if tag.code == 330 && depth == 0 && !owner_written {
            *tag = Tag::s(330, owner);
            owner_written = true;
        }
    }
    if !owner_written {
        let at = parent.iter().position(|tag| tag.code == 100).unwrap_or(parent.len());
        parent.insert(at, Tag::s(330, owner));
    }
    if !changed.is_empty() {
        let mut depth = 0usize;
        let start = parent
            .iter()
            .position(|tag| {
                if tag.code == 102 {
                    if tag.str().trim().starts_with('{') {
                        depth += 1;
                    } else if tag.str().trim() == "}" {
                        depth -= 1;
                    }
                }
                depth == 0 && tag.code == 100 && tag.str() == "AcDbEntity"
            })
            .map_or(1, |at| at + 1);
        let mut end = start;
        // Put the actual properties before opaque lookalikes, without changing any application group.
        let mut properties = replacement;
        for tag in parent.iter().skip(start) {
            if depth == 0 && (tag.code == 100 || tag.code >= 1000) {
                break;
            }
            if tag.code == 102 {
                if tag.str().trim().starts_with('{') {
                    depth += 1;
                } else if tag.str().trim() == "}" {
                    depth -= 1;
                }
            }
            if depth != 0 || tag.code == 102 || !changed.contains(&tag.code) {
                properties.push(tag.clone());
            }
            end += 1;
        }
        parent.splice(start..end, properties);
    }
    w.t.extend(parent);
    for follower in raw.followers() {
        w.t.extend(source_record(follower));
    }
}
