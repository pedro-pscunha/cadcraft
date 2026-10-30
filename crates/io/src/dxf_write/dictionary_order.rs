//! AutoCAD lists the entries of every dictionary by name, ignoring case. The save puts the entries
//! it writes from the model first and the kept ones after them, so each dictionary is put in
//! AutoCAD's order last, whichever way its entries arrived.

use super::Tag;

/// Sort the entries (`3` name, then a `350` or `360` handle) of every dictionary record in place;
/// every other group stays where it is.
pub(super) fn sort(tags: &mut [Tag]) {
    let mut start = 0;
    while start < tags.len() {
        let end = tags[start + 1..].iter().position(|t| t.code == 0).map_or(tags.len(), |n| start + 1 + n);
        let head = tags[start].str();
        if cadcraft_doc::foreign::is_dictionary(&head) {
            sort_record(&mut tags[start..end]);
        }
        start = end;
    }
}

fn sort_record(record: &mut [Tag]) {
    let slots: Vec<usize> = (0..record.len().saturating_sub(1)).filter(|&i| record[i].code == 3 && matches!(record[i + 1].code, 350 | 360)).collect();
    let mut entries: Vec<(Tag, Tag)> = slots.iter().map(|&i| (record[i].clone(), record[i + 1].clone())).collect();
    entries.sort_by_cached_key(|(name, _)| name.str().to_lowercase());
    for (&slot, (name, target)) in slots.iter().zip(entries) {
        record[slot] = name;
        record[slot + 1] = target;
    }
}
