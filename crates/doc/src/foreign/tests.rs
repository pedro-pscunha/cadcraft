use super::*;
use crate::geom::Vec3;
use crate::{Common, Drawing, Line, Space};
use std::cell::Cell;
std::thread_local! { pub(super) static OWNERS: Cell<usize> = const {Cell::new(0)}; }

#[test]
fn many_dictionary_roots_need_only_linear_owner_inspection() {
    let mut d = Drawing::new_metric();
    let mut originals = Vec::new();
    for _ in 0..200 {
        let h = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
        originals.push((**d.entity(h).unwrap()).clone());
        let dictionary = d.new_handle();
        let foreign = std::sync::Arc::make_mut(&mut d.foreign);
        foreign.entities.entry(h).or_default().xdict = Some(dictionary);
        foreign.objects.push(ForeignObject {
            handle: dictionary,
            kind: "DICTIONARY".into(),
            tags: vec![
                RawTag { code: 5, value: dictionary.hex() },
                RawTag { code: 330, value: h.hex() },
                RawTag { code: 100, value: "AcDbDictionary".into() },
            ],
        });
    }
    OWNERS.with(|count| count.set(0));
    let copies = d.duplicate_entities(vec![originals], DuplicationSource::SameDrawing).unwrap();
    assert_eq!(copies.len(), 200);
    assert!(copies.iter().all(|e| d.foreign.entities.get(&e.handle).is_none_or(|extra| extra.xdict.is_none())));
    OWNERS.with(|count| assert!(count.get() <= 1000, "dictionary roots repeatedly scanned their object set: {} owner inspections", count.get()));
}

/// A line whose extension object holds a 320 (an arbitrary handle, DXF Reference: taken as is)
/// and a 331 (a translated pointer) that both name the line.
fn line_with_references() -> (Drawing, crate::Entity, Handle) {
    let mut d = Drawing::new_metric();
    let h = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    let dictionary = d.new_handle();
    let foreign = std::sync::Arc::make_mut(&mut d.foreign);
    foreign.entities.entry(h).or_default().xdict = Some(dictionary);
    foreign.objects.push(ForeignObject {
        handle: dictionary,
        kind: "XRECORD".into(),
        tags: vec![
            RawTag { code: 5, value: dictionary.hex() },
            RawTag { code: 330, value: h.hex() },
            RawTag { code: 100, value: "AcDbXrecord".into() },
            RawTag { code: 320, value: h.hex() },
            RawTag { code: 331, value: h.hex() },
        ],
    });
    let line = (**d.entity(h).unwrap()).clone();
    (d, line, dictionary)
}

fn group(tags: &[RawTag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(|t| t.value.clone()).unwrap()
}

#[test]
fn copying_translates_a_331_pointer_and_keeps_a_320_handle() {
    let (mut d, line, _) = line_with_references();
    let copies = d.duplicate_entities(vec![vec![line.clone()]], DuplicationSource::SameDrawing).unwrap();
    let copy = copies[0].handle;
    assert_ne!(copy, line.handle);
    let dictionary = d.foreign.entities.get(&copy).and_then(|x| x.xdict).expect("the copy has an extension object");
    let object = d.foreign.objects.iter().find(|o| o.handle == dictionary).unwrap();
    assert_eq!(group(&object.tags, 331), copy.hex(), "331 names the copy");
    assert_eq!(group(&object.tags, 320), line.handle.hex(), "320 is taken as is");
}

#[test]
fn merging_translates_a_331_pointer_and_keeps_a_320_handle() {
    let (mut d, line, dictionary) = line_with_references();
    let result = d.new_handle();
    // The dictionary survives the merge, owned by the result.
    std::sync::Arc::make_mut(&mut d.foreign).objects.iter_mut().find(|o| o.handle == dictionary).unwrap().tags[1].value = result.hex();
    std::sync::Arc::make_mut(&mut d.foreign).entities.remove(&line.handle);
    d.remap_merged_references(&[line.handle], result);
    let object = d.foreign.objects.iter().find(|o| o.handle == dictionary).unwrap();
    assert_eq!(group(&object.tags, 331), result.hex(), "331 names the result");
    assert_eq!(group(&object.tags, 320), line.handle.hex(), "320 is taken as is");
}

#[test]
fn merging_translates_the_data_store_link_of_a_solid() {
    let (mut d, line, _) = line_with_references();
    let result = d.new_handle();
    std::sync::Arc::make_mut(&mut d.foreign).data_store = vec![RawTag { code: 320, value: line.handle.hex() }];
    d.remap_merged_references(&[line.handle], result);
    assert_eq!(d.foreign.data_store[0].value, result.hex());
}
