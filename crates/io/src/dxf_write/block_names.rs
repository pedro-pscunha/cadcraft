//! Block names that AutoCAD's DXF reader refuses. A `|` belongs to an xref-dependent block (BLOCK flag 16) only, but a bound
//! dynamic block's representation can keep one in a DWG (Eztec Firenze: `…$0$XR-19354-APTO-A-RES|Janela enrolar`, flag 0).
//! AutoCAD LT 2025 discards a DXF with such a BLOCK_RECORD name ("Nome de registro de tabela de símbolos inválido"). Its own
//! DXFOUT writes the block as an anonymous `*U` block (flag 1) and keeps the true name in the AcDbDynamicBlockTrueName xdata,
//! which the record already carries. A DWG keeps the name, so the DWG bridge does not rename.

use super::{HashMap, HashSet, Tag};

/// The record starts (index of each group 0) and ends of `tags`.
fn records(tags: &[Tag]) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    starts.iter().enumerate().map(|(n, &s)| (s, starts.get(n + 1).copied().unwrap_or(tags.len()))).collect()
}

/// The first group `code` of a record, outside its application groups (102) and before its xdata.
fn first(tags: &[Tag], (start, end): (usize, usize), code: i32) -> Option<usize> {
    let mut grouped = false;
    (start + 1..end).take_while(|&i| tags[i].code < 1000).find(|&i| {
        if tags[i].code == 102 {
            grouped = tags[i].str().trim().starts_with('{');
            return false;
        }
        !grouped && tags[i].code == code
    })
}

/// Write every block whose name AutoCAD's DXF reader refuses as an anonymous `*U<n>` block with the lowest free numbers, and
/// rename the records that use it. The renames, by upper-case old name (the Raw save check compares a Raw record through them). A source can number its anonymous blocks above 2^32 (Firenze: `*U7260417301`).
pub(super) fn anonymize_refused(tags: &mut [Tag]) -> HashMap<String, String> {
    let records = records(tags);
    let kind = |tags: &[Tag], r: (usize, usize)| tags[r.0].str();
    let mut refused = Vec::new();
    let mut taken = HashSet::new();
    for &r in records.iter().filter(|&&r| kind(tags, r) == "BLOCK") {
        let Some(name) = first(tags, r, 2).map(|i| tags[i].str()) else { continue };
        let flags = first(tags, r, 70).map_or(0, |i| tags[i].i64());
        if name.contains('|') && flags & 16 == 0 {
            refused.push(name.to_ascii_uppercase());
        } else if let Some(n) = name.get(..2).filter(|p| p.eq_ignore_ascii_case("*U")).and_then(|_| name[2..].parse::<u64>().ok()) {
            taken.insert(n);
        }
    }
    if refused.is_empty() {
        return HashMap::new();
    }
    let free = (0u64..).filter(|n| !taken.contains(n));
    let renamed: HashMap<String, String> = refused.into_iter().zip(free).map(|(old, n)| (old, format!("*U{n}"))).collect();
    let rename = |tags: &mut [Tag], at: Option<usize>| {
        if let Some(i) = at
            && let Some(new) = renamed.get(&tags[i].str().to_ascii_uppercase())
        {
            tags[i] = Tag::s(tags[i].code, new.clone());
        }
    };
    for &r in &records {
        match kind(tags, r).as_str() {
            // The records that name a block by group 2: an insert, a table's graphics block, and every dimension's.
            "BLOCK_RECORD" | "INSERT" | "ACAD_TABLE" | "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => rename(tags, first(tags, r, 2)),
            "BLOCK" => {
                let at = first(tags, r, 2);
                if at.is_some_and(|i| renamed.contains_key(&tags[i].str().to_ascii_uppercase())) {
                    if let Some(i) = first(tags, r, 70) {
                        tags[i] = Tag::i(70, tags[i].i64() | 1);
                    }
                    rename(tags, first(tags, r, 3));
                    rename(tags, at);
                }
            }
            _ => {}
        }
    }
    renamed
}
