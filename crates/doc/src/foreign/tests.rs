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
