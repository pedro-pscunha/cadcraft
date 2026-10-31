use cadcraft_doc::EntityKind;
use cadcraft_io::activation::records::RecordView;
use cadcraft_io::activation::{FamilySet, Need, ReaderMode, ReaderPolicy};

#[test]
fn many_unreadable_records_in_one_block_keep_bounded_raw_reasons() {
    let count = 2_000;
    let mut source = String::from("0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\nB0\n2\nSHARED\n70\n0\n10\n0\n20\n0\n30\n0\n");
    for index in 0..count {
        source.push_str(&format!("0\nCIRCLE\n5\n{:X}\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n", 0x10000 + index));
    }
    source.push_str("0\nENDBLK\n5\nB1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
    let policy = ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE).with_raw_path();
    let need = |view: &RecordView<'_>| if view.kind == "CIRCLE" { Need::Needs { readable: false } } else { Need::Plain };
    let drawing = cadcraft_io::read_with_need(source.as_bytes(), "shared.dxf", policy, &need).unwrap();
    let block = drawing.blocks.get("SHARED").unwrap();
    assert_eq!(block.entities.len(), count);
    let reasons = block
        .entities
        .iter()
        .map(|entity| {
            let EntityKind::Raw(raw) = &entity.kind else { panic!("every unreadable record must remain Raw") };
            raw.reason()
        })
        .collect::<Vec<_>>();
    assert!(reasons.iter().all(|reason| *reason == reasons[0]));
    assert!(reasons[0].len() < 512, "the reader copied a component-sized reason into each Raw record");
    assert!(reasons[0].contains("its canonical reading fails"));
}
