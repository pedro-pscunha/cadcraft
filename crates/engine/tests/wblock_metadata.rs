use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

fn source() -> &'static [u8] {
    b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n0\nAPPID\n2\nVENDOR\n70\n0\n0\nENDTAB\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n100\n102\n{ACAD_XDICTIONARY\n360\n200\n102\n}\n100\nAcDbEntity\n8\n0\n347\nD1\n284\n2\n390\nF1\n100\nAcDbLine\n10\n0\n20\n0\n11\n5\n21\n5\n1001\nVENDOR\n1000\nline-data\n1005\n220\n\
0\nLINE\n5\n101\n102\n{ACAD_XDICTIONARY\n360\n210\n102\n}\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n10\n20\n10\n11\n15\n21\n15\n0\nENDSEC\n\
0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nSharedData\n350\n220\n3\nUnrelated\n350\n300\n3\nACAD_MATERIAL\n350\nD0\n3\nACAD_PLOTSTYLENAME\n350\nF0\n\
0\nDICTIONARY\n5\n200\n330\n100\n100\nAcDbDictionary\n281\n1\n3\nData\n350\n201\n0\nXRECORD\n5\n201\n330\n200\n100\nAcDbXrecord\n280\n1\n1\nselected-owned\n340\n220\n\
0\nDICTIONARY\n5\n210\n330\n101\n100\nAcDbDictionary\n281\n1\n3\nData\n350\n211\n0\nXRECORD\n5\n211\n330\n210\n100\nAcDbXrecord\n280\n1\n1\nunselected-owned\n\
0\nXRECORD\n5\n220\n330\nC\n100\nAcDbXrecord\n280\n1\n1\nselected-shared\n0\nXRECORD\n5\n300\n330\nC\n100\nAcDbXrecord\n280\n1\n1\nunrelated-root\n\
0\nDICTIONARY\n5\nD0\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nKeptMaterial\n350\nD1\n3\nUnusedMaterial\n350\nD3\n0\nMATERIAL\n5\nD1\n330\nD0\n100\nAcDbMaterial\n1\nKeptMaterial\n2\nselected material\n0\nMATERIAL\n5\nD3\n330\nD0\n100\nAcDbMaterial\n1\nUnusedMaterial\n2\nunselected material\n\
0\nACDBDICTIONARYWDFLT\n5\nF0\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nSelectedPen\n350\nF1\n100\nAcDbDictionaryWithDefault\n340\nF1\n0\nACDBPLACEHOLDER\n5\nF1\n330\nF0\n0\nENDSEC\n0\nEOF\n"
}

fn common(tags: &[cadcraft_dxf::Tag]) -> &[cadcraft_dxf::Tag] {
    let start = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbEntity").unwrap() + 1;
    let rest = &tags[start..];
    let end = rest.iter().position(|t| t.code == 100).unwrap_or(rest.len());
    &rest[..end]
}

fn hooks() {
    set_io(IoHooks {
        read: |bytes, path| cadcraft_io::read(bytes, path).map_err(|e| e.to_string()),
        write: |drawing, path| cadcraft_io::write(drawing, path).map_err(|e| e.to_string()),
        plot: None,
    });
    set_write_noted(|drawing, path, opts| {
        cadcraft_io::write_with_noted(drawing, path, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary })
            .map_err(|e| e.to_string())
    });
}

#[test]
fn wblock_keeps_selected_metadata_and_only_its_referenced_foreign_objects() {
    hooks();
    for format in ["dxf", "dwg"] {
        let mut session = Session::new();
        session.open_drawing(cadcraft_io::read_dxf(source()).unwrap(), "source.dxf", None);
        let path = std::env::temp_dir().join(format!("cadcraft-wblock-metadata-{}.{format}", std::process::id()));
        session.execute("wblock", &json!({"handles": ["100"], "path": path})).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let dxf = if format == "dwg" { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes.clone() };
        let tags = cadcraft_dxf::parse(&dxf).unwrap();
        let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
        let lines: Vec<_> = records.iter().filter(|(kind, _)| kind == "LINE").collect();
        assert_eq!(lines.len(), 1);
        let line = &lines[0].1;
        assert!(line.iter().any(|t| t.code == 1000 && t.str() == "line-data"), "{format}: entity xdata");
        let owner = line.iter().find(|t| t.code == 5).unwrap().str();
        let dictionary = line.iter().find(|t| t.code == 360).expect("selected extension dictionary").str();
        let dict = &records.iter().find(|(kind, tags)| kind == "DICTIONARY" && tags.iter().any(|t| t.code == 5 && t.str() == dictionary)).unwrap().1;
        assert!(dict.iter().any(|t| t.code == 330 && t.str() == owner), "{format}: dictionary owner");
        let data = dict.iter().find(|t| t.code == 350).unwrap().str();
        let record = &records.iter().find(|(kind, tags)| kind == "XRECORD" && tags.iter().any(|t| t.code == 5 && t.str() == data)).unwrap().1;
        assert!(record.iter().any(|t| t.code == 1 && t.str() == "selected-owned"), "{format}: owned data");
        let shared = record.iter().find(|t| t.code == 340).unwrap().str();
        assert!(line.iter().any(|t| t.code == 1005 && t.str() == shared), "{format}: xdata handle follows the referenced object");
        assert!(
            records.iter().any(|(kind, tags)| kind == "XRECORD"
                && tags.iter().any(|t| t.code == 5 && t.str() == shared)
                && tags.iter().any(|t| t.code == 1 && t.str() == "selected-shared")),
            "{format}: referenced foreign object"
        );
        let material =
            &records.iter().find(|(kind, tags)| kind == "MATERIAL" && tags.iter().any(|t| t.code == 1 && t.str() == "KeptMaterial")).unwrap().1;
        let material_handle = material.iter().find(|t| t.code == 5).unwrap().str();
        assert!(common(line).iter().any(|t| t.code == 347 && t.str() == material_handle), "{format}: material pointer in AcDbEntity");
        assert!(common(line).iter().any(|t| t.code == 284 && t.i64() == 2), "{format}: shadow mode in AcDbEntity");
        for unwanted in ["unselected-owned", "unrelated-root", "UnusedMaterial"] {
            assert!(!tags.iter().any(|t| t.str() == unwanted), "{format}: {unwanted}");
        }
        let back = cadcraft_io::read(&bytes, &format!("block.{format}")).unwrap();
        assert_eq!(back.model.iter().next().unwrap().common.plot_style, "SelectedPen");
        assert_eq!(session.doc().unwrap().entity_count(), 2, "WBLOCK does not mutate the source");
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn line_material_and_shadow_mode_stay_in_acdbentity_through_dwg() {
    let drawing = cadcraft_io::read_dxf(source()).unwrap();
    for version in [cadcraft_io::Version::R2007, cadcraft_io::Version::R2018] {
        let bytes = cadcraft_io::write_with(&drawing, "material.dwg", &cadcraft_io::SaveOptions { version: Some(version), binary: false }).unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&bytes).unwrap();
        let tags = cadcraft_dxf::parse(&dxf).unwrap();
        let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
        let material =
            &records.iter().find(|(kind, tags)| kind == "MATERIAL" && tags.iter().any(|t| t.code == 1 && t.str() == "KeptMaterial")).unwrap().1;
        let handle = material.iter().find(|t| t.code == 5).unwrap().str();
        let line = &records.iter().find(|(kind, tags)| kind == "LINE" && tags.iter().any(|t| t.code == 5 && t.str() == "100")).unwrap().1;
        assert!(common(line).iter().any(|t| t.code == 347 && t.str() == handle), "{version:?}: material pointer");
        assert!(common(line).iter().any(|t| t.code == 284 && t.i64() == 2), "{version:?}: shadow mode");
    }
}

#[test]
fn named_wblock_moves_block_metadata_to_its_model_space_record() {
    use cadcraft_engine::doc::foreign::{ForeignExtra, ForeignObject};
    use cadcraft_engine::doc::{Block, Handle, RawTag};
    use std::sync::Arc;
    hooks();
    let mut drawing = cadcraft_io::read_dxf(source()).unwrap();
    let line = drawing.remove_entity(Handle(0x100)).unwrap();
    let mut block = Block::new("SOURCE");
    block.base = cadcraft_engine::geom::Vec3::new(2.0, 3.0, 0.0);
    block.entities.push((*line).clone());
    drawing.blocks.insert(block.name.clone(), Arc::new(block));
    drawing.bump_handseed(Handle(0x500));
    let tags = |values: &[(i32, &str)]| values.iter().map(|(code, value)| RawTag { code: *code, value: (*value).into() }).collect();
    let foreign = Arc::make_mut(&mut drawing.foreign);
    foreign.targets.insert(Handle(0x500), ("BLOCK_RECORD".into(), "SOURCE".into()));
    foreign.records.insert(("BLOCK_RECORD".into(), "SOURCE".into()), ForeignExtra { xdict: Some(Handle(0x400)), ..Default::default() });
    foreign.objects.extend([
        ForeignObject {
            handle: Handle(0x400),
            kind: "DICTIONARY".into(),
            tags: tags(&[(5, "400"), (330, "500"), (100, "AcDbDictionary"), (3, "BlockData"), (350, "401")]),
        },
        ForeignObject {
            handle: Handle(0x401),
            kind: "XRECORD".into(),
            tags: tags(&[(5, "401"), (330, "400"), (100, "AcDbXrecord"), (280, "1"), (1, "block-owned")]),
        },
    ]);
    for format in ["dxf", "dwg"] {
        let mut session = Session::new();
        session.open_drawing(drawing.clone(), "source.dxf", None);
        let path = std::env::temp_dir().join(format!("cadcraft-wblock-named-{}.{format}", std::process::id()));
        session.execute("wblock", &json!({"name": "SOURCE", "path": path})).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let dxf = if format == "dwg" { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes };
        let tags = cadcraft_dxf::parse(&dxf).unwrap();
        let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
        let model = &records
            .iter()
            .find(|(kind, tags)| kind == "BLOCK_RECORD" && tags.iter().any(|t| t.code == 2 && t.str().eq_ignore_ascii_case("*Model_Space")))
            .unwrap()
            .1;
        let handle = model.iter().find(|t| t.code == 5).unwrap().str();
        let dictionary = model.iter().find(|t| t.code == 360).expect("block metadata migrated to model space").str();
        assert!(records.iter().any(|(kind, tags)| kind == "DICTIONARY"
            && tags.iter().any(|t| t.code == 5 && t.str() == dictionary)
            && tags.iter().any(|t| t.code == 330 && t.str() == handle)));
        assert!(tags.iter().any(|t| t.code == 1 && t.str() == "block-owned"));
        let line = &records.iter().find(|(kind, _)| kind == "LINE").unwrap().1;
        assert_eq!(line.iter().find(|t| t.code == 10).unwrap().f64(), -2.0);
        assert_eq!(line.iter().find(|t| t.code == 20).unwrap().f64(), -3.0);
        std::fs::remove_file(path).unwrap();
    }
}
