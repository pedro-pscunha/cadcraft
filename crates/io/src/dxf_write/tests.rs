use super::*;
use std::cell::Cell;
std::thread_local! { pub(super) static SCANNED: Cell<usize> = const { Cell::new(0) }; }

#[test]
fn entity_groups_does_not_scan_the_document_namespace() {
    let mut d = Drawing::new_metric();
    let h = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    for _ in 0..1000 {
        d.add(&Space::Model, Common::default(), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 })).unwrap();
    }
    SCANNED.with(|count| count.set(0));
    let (kind, tags) = entity_groups(&d, d.entity(h).unwrap());
    assert_eq!(kind, "LINE");
    assert!(tags.iter().any(|t| t.code == 5 && t.str() == h.hex()));
    SCANNED.with(|count| assert_eq!(count.get(), 0, "entget must not reserve every drawing handle"));
}
