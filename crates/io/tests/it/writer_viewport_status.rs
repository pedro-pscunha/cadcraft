//! VIEWPORT stacking status is independent of its ID and of its display-mode flags.
use cadcraft_doc::{Drawing, EntityKind};

const FLAGS: i64 = 32768 | 16384 | 1 | 2 | 4 | 8 | 32 | 64 | 128 | 256 | 512 | 1024 | 2048 | 262144 | 524288;

fn fixture(status: i64) -> Drawing {
    let text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$MAXACTVP\n70\n64\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nVIEWPORT\n5\n100\n100\nAcDbEntity\n67\n1\n8\n0\n100\nAcDbViewport\n10\n20\n20\n30\n30\n0\n40\n50\n41\n40\n68\n{status}\n69\n2\n12\n5\n22\n6\n45\n20\n90\n{FLAGS}\n0\nENDSEC\n0\nEOF\n"
    );
    let mut drawing = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    // The layout is taken as made here: its paper-space viewport is generated (a file's layout without one gets none).
    std::sync::Arc::make_mut(&mut drawing.source_symbols).layouts_without_sheet.clear();
    drawing
}

fn status(bytes: &[u8]) -> (i64, i64) {
    status_for(bytes, 2)
}

fn status_for(bytes: &[u8], id: i64) -> (i64, i64) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let record = cadcraft_dxf::sections(&tags)
        .into_iter()
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .find(|(k, t)| k == "VIEWPORT" && t.iter().any(|t| t.code == 69 && t.i64() == id))
        .unwrap()
        .1;
    let value = |code| record.iter().find(|t| t.code == code).unwrap().i64();
    (value(68), value(90))
}

#[test]
fn dxf_keeps_source_stacking_and_dwg_derives_it_from_native_state() {
    for stacking in [-1, 7] {
        for format in ["saved.dxf", "saved.dwg"] {
            let mut drawing = fixture(stacking);
            for _ in 0..2 {
                let saved = cadcraft_io::write(&drawing, format).unwrap();
                let text = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
                // ODA 20.4.38 has no DWG field for 68. An on-screen viewport within
                // MAXACTVP gets its viewport number on DXF output, not the source DXF status.
                let expected = if format.ends_with("dwg") { 2 } else { stacking };
                assert_eq!(status(&text), (expected, FLAGS), "{format}");
                drawing = cadcraft_io::read(&saved, format).unwrap();
            }
        }
    }
}

#[test]
fn dwg_derives_off_and_maxactvp_status_without_changing_other_mode_bits() {
    for on in [false, true] {
        let mut drawing = fixture(7);
        // The generated base paper viewport occupies one active slot.
        drawing.header.set_i64("MAXACTVP", 2);
        let handle = drawing.layouts.iter().flat_map(|l| l.entities.iter()).find(|e| matches!(e.kind, EntityKind::Viewport(_))).unwrap().handle;
        let EntityKind::Viewport(mut second) = drawing.entity(handle).unwrap().kind.clone() else { panic!("viewport") };
        second.id = 3;
        let space = drawing.space_of(handle).unwrap();
        drawing.add(&space, cadcraft_doc::Common::default(), EntityKind::Viewport(second)).unwrap();
        drawing
            .modify_entity(handle, |e| {
                let EntityKind::Viewport(viewport) = &mut e.kind else { panic!("viewport") };
                viewport.on = on;
            })
            .unwrap();
        let expected = if on { (2, FLAGS) } else { (0, FLAGS | 131072) };
        let second_status = if on { -1 } else { 3 };
        for _ in 0..2 {
            let bytes = cadcraft_io::write(&drawing, "derived.dwg").unwrap();
            let dxf = cadcraft_dwg::dwg_to_dxf(&bytes).unwrap();
            assert_eq!(status(&dxf), expected);
            assert_eq!(status_for(&dxf, 3), (second_status, FLAGS));
            drawing = cadcraft_io::read(&bytes, "derived.dwg").unwrap();
        }
    }
}

#[test]
fn an_off_stacking_status_keeps_independent_flags_until_an_on_edit() {
    let mut drawing = fixture(0);
    let handle = drawing.layouts.iter().flat_map(|l| l.entities.iter()).find(|e| matches!(e.kind, EntityKind::Viewport(_))).unwrap().handle;
    let EntityKind::Viewport(viewport) = &drawing.entity(handle).unwrap().kind else { panic!("viewport") };
    assert!(!viewport.on);
    assert_eq!(status(&cadcraft_io::write(&drawing, "unchanged.dxf").unwrap()), (0, FLAGS));
    drawing
        .modify_entity(handle, |e| {
            let EntityKind::Viewport(viewport) = &mut e.kind else { panic!("viewport") };
            viewport.on = true;
        })
        .unwrap();
    assert_eq!(status(&cadcraft_io::write(&drawing, "edited.dxf").unwrap()), (2, FLAGS));
}

#[test]
fn clipping_edits_change_only_the_clip_mode_bit() {
    let mut drawing = fixture(7);
    let handle = drawing.layouts.iter().flat_map(|l| l.entities.iter()).find(|e| matches!(e.kind, EntityKind::Viewport(_))).unwrap().handle;
    let space = drawing.space_of(handle).unwrap();
    let clip = drawing
        .add(
            &space,
            cadcraft_doc::Common::default(),
            EntityKind::Circle(cadcraft_doc::Circle { center: cadcraft_doc::geom::Vec3::new(20.0, 30.0, 0.0), radius: 10.0 }),
        )
        .unwrap();
    for clipped in [true, false] {
        drawing
            .modify_entity(handle, |e| {
                let EntityKind::Viewport(viewport) = &mut e.kind else { panic!("viewport") };
                viewport.clip = clipped.then_some(clip);
            })
            .unwrap();
        for format in ["edited.dxf", "edited.dwg"] {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved };
            let expected_status = if format.ends_with("dwg") { 2 } else { 7 };
            assert_eq!(status(&dxf), (expected_status, FLAGS | if clipped { 65536 } else { 0 }));
        }
    }
}

#[test]
fn lock_and_display_edits_preserve_independent_status_bits() {
    let mut drawing = fixture(-1);
    let handle = drawing.layouts.iter().flat_map(|l| l.entities.iter()).find(|e| matches!(e.kind, EntityKind::Viewport(_))).unwrap().handle;
    drawing
        .modify_entity(handle, |e| {
            let EntityKind::Viewport(viewport) = &mut e.kind else { panic!("viewport") };
            viewport.locked = false;
            viewport.on = false;
        })
        .unwrap();
    for format in ["edited.dxf", "edited.dwg"] {
        let saved = cadcraft_io::write(&drawing, format).unwrap();
        let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved };
        assert_eq!(status(&dxf), (0, (FLAGS & !16384) | 131072));
    }
}
