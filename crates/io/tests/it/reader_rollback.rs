//! Stage0 public-reader round trips across Safe/All and Off. No Canonical reader is enabled.
use cadcraft_doc::{Drawing, EntityKind, GeometryRevision, Handle};
use cadcraft_geom::{Mat4, Vec3};
use cadcraft_io::activation::{ACTIVE_FAMILIES, ReaderMode, ReaderPolicy};

const DIRECTIONS: [(ReaderMode, ReaderMode); 4] = [
    (ReaderMode::Safe, ReaderMode::Off),
    (ReaderMode::All, ReaderMode::Off),
    (ReaderMode::Off, ReaderMode::Safe),
    (ReaderMode::Off, ReaderMode::All),
];

fn read_raw_policy(bytes: &[u8], name: &str, mode: ReaderMode) -> Drawing {
    let dxf = if cadcraft_dwg::is_dwg(bytes) {
        std::borrow::Cow::Owned(cadcraft_dwg::dwg_to_dxf(bytes).unwrap())
    } else {
        std::borrow::Cow::Borrowed(bytes)
    };
    let policy = ReaderPolicy::new(mode, ACTIVE_FAMILIES).with_raw_path();
    cadcraft_io::read_with_need(bytes, name, policy, &crate::activation_fixtures::source_predicate(&dxf)).unwrap()
}

fn assert_raw(drawing: &Drawing) {
    let entity = drawing.entity(Handle(0x200)).unwrap();
    assert_eq!(entity.geometry_revision, GeometryRevision::RawSource, "the rollback proof must reach the Raw safety path");
    let EntityKind::Raw(raw) = &entity.kind else { panic!("Raw INSERT") };
    assert_eq!(
        raw.records().map(|record| (record.kind(), record.handle())).collect::<Vec<_>>(),
        [("INSERT", Handle(0x200)), ("ATTRIB", Handle(0x201)), ("ATTRIB", Handle(0x202)), ("SEQEND", Handle(0x203)),]
    );
    for (handle, expected) in [(Handle(0x201), "First value"), (Handle(0x202), "Second value")] {
        let record = raw.records().find(|record| record.handle() == handle).unwrap();
        assert_eq!(record.tags().iter().find(|tag| tag.code == 1).unwrap().value, expected);
    }
}

#[test]
fn stage0_explicit_raw_policy_survives_on_off_and_off_on_reopens_in_both_formats() {
    let source = include_bytes!("../data/activation/r5_raw_insert_with_full_tables.dxf");
    // The synthetic old-header source includes R2010 attribute fields. Use a target that retains them.
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    for (from, to) in DIRECTIONS {
        for name in ["rollback.dxf", "rollback.dwg"] {
            let mut drawing = read_raw_policy(source, "source.dxf", from);
            assert_raw(&drawing);
            let mut move_line = Mat4::IDENTITY;
            move_line.m[0][3] = 4.0;
            move_line.m[1][3] = 5.0;
            drawing.transform_entity(Handle(0x90), &move_line).unwrap();
            let line = drawing.entity(Handle(0x90)).unwrap();
            let snapshot = line.common.geometry.as_ref().unwrap();
            assert_ne!(cadcraft_doc::geometry_key(&line.kind, &line.common).unwrap(), snapshot.model, "the real edit must invalidate replay");
            // A successful DWG write includes the independent complete RawSaveCheck precondition.
            let saved = cadcraft_io::write_with(&drawing, name, &options).unwrap();
            let mut reopened = read_raw_policy(&saved, name, to);
            let control = read_raw_policy(&saved, name, from);
            assert_raw(&reopened);
            assert_eq!(reopened, control, "{from:?}->{to:?} {name}: only the reader mode changed");
            let before = reopened.clone();
            let error = reopened.transform_entity(Handle(0x200), &move_line).unwrap_err();
            assert!(matches!(error, cadcraft_doc::DocError::RawReadOnly { handle: Handle(0x200), .. }));
            assert_eq!(reopened, before);
            let raw = reopened.entity(Handle(0x200)).unwrap().as_ref().clone();
            assert!(matches!(
                reopened.duplicate_entities(vec![vec![raw]], cadcraft_doc::DuplicationSource::SameDrawing),
                Err(cadcraft_doc::DocError::RawReadOnly { .. })
            ));
            assert_eq!(reopened, before);
            let second_save = cadcraft_io::write_with(&reopened, name, &options).expect("a checked Raw unit must remain saveable after reopening");
            assert_eq!(second_save, cadcraft_io::write_with(&control, name, &options).unwrap(), "{name}: the mode cannot change save bytes");
            let second_reopen = read_raw_policy(&second_save, name, from);
            assert_raw(&second_reopen);
            assert_eq!(second_reopen, read_raw_policy(&second_save, name, to));
            reopened.transform_entity(Handle(0x90), &move_line).unwrap();
            let saved_again = cadcraft_io::write_with(&reopened, name, &options).unwrap();
            assert_raw(&read_raw_policy(&saved_again, name, from));
        }
    }
}

#[test]
fn stage0_normal_production_policy_preserves_plain_data_in_both_mode_directions() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n200\n10\n-0.0\n20\n1\n30\n0\n11\n3\n21\n4\n31\n0\n0\nENDSEC\n0\nEOF\n";
    for (from, to) in DIRECTIONS {
        for name in ["plain.dxf", "plain.dwg"] {
            let mut drawing = cadcraft_io::read_with_policy(source, "source.dxf", ReaderPolicy::new(from, ACTIVE_FAMILIES)).unwrap();
            let mut matrix = Mat4::IDENTITY;
            matrix.m[0][3] = 7.0;
            drawing.transform_entity(Handle(0x200), &matrix).unwrap();
            let bytes = cadcraft_io::write(&drawing, name).unwrap();
            let reopened = cadcraft_io::read_with_policy(&bytes, name, ReaderPolicy::new(to, ACTIVE_FAMILIES)).unwrap();
            let control = cadcraft_io::read_with_policy(&bytes, name, ReaderPolicy::new(from, ACTIVE_FAMILIES)).unwrap();
            assert_eq!(reopened, control);
            let EntityKind::Line(line) = &reopened.entity(Handle(0x200)).unwrap().kind else { panic!("line") };
            assert_eq!((line.a, line.b), (Vec3::new(7.0, 1.0, 0.0), Vec3::new(10.0, 4.0, 0.0)));
            assert_eq!(cadcraft_io::write(&reopened, name).unwrap(), cadcraft_io::write(&control, name).unwrap());
        }
    }
}
