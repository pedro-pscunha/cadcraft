//! BLOCK nonconstant-attribute flags and begin/end layers are independent source fields.
use cadcraft_dxf::Tag;

const SOURCE: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n0\nLAYER\n5\n40\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\nBeginLayer\n70\n0\n62\n7\n6\nContinuous\n0\nLAYER\n5\n41\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\nEndLayer\n70\n0\n62\n7\n6\nContinuous\n0\nENDTAB\n0\nTABLE\n2\nBLOCK_RECORD\n70\n1\n0\nBLOCK_RECORD\n5\n50\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\nWithAttrs\n70\n0\n0\nENDTAB\n0\nENDSEC\n\
0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n51\n330\n50\n100\nAcDbEntity\n8\nBeginLayer\n100\nAcDbBlockBegin\n2\nWithAttrs\n70\n2\n10\n0\n20\n0\n30\n0\n3\nWithAttrs\n1\n\n0\nATTDEF\n5\n52\n330\n50\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\nValue\n100\nAcDbAttributeDefinition\n3\nPrompt\n2\nTAG\n70\n0\n0\nENDBLK\n5\n53\n330\n50\n100\nAcDbEntity\n8\nEndLayer\n100\nAcDbBlockEnd\n0\nENDSEC\n0\nEOF\n";

fn fields(bytes: &[u8]) -> (i64, String, String) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let records: Vec<_> =
        cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name == "BLOCKS").flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
    let start = records.iter().position(|(k, t)| k == "BLOCK" && t.iter().any(|t| t.code == 2 && t.str() == "WithAttrs")).unwrap();
    let begin = &records[start].1;
    let end = &records.iter().skip(start + 1).find(|(k, _)| k == "ENDBLK").unwrap().1;
    (
        begin.iter().find(|t| t.code == 70).unwrap().i64() & 2,
        begin.iter().find(|t| t.code == 8).map(Tag::str).unwrap(),
        end.iter().find(|t| t.code == 8).map(Tag::str).unwrap(),
    )
}
#[test]
fn source_block_fields_survive_two_dxf_saves() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dxf").unwrap();
        assert_eq!(fields(&saved), (2, "BeginLayer".into(), "EndLayer".into()));
        drawing = cadcraft_io::read(&saved, "saved.dxf").unwrap();
    }
}

#[test]
fn native_block_attribute_flag_and_begin_layer_survive_two_dwg_saves() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&saved).unwrap();
        let (flags, begin, _) = fields(&dxf);
        assert_eq!((flags, begin), (2, "BeginLayer".into()));
        drawing = cadcraft_io::read(&saved, "saved.dwg").unwrap();
    }
}

#[test]
#[ignore = "pending acadrust: ENDBLK layer in DWG"]
fn native_endblock_layer_survives_two_dwg_saves() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&saved).unwrap();
        assert_eq!(fields(&dxf).2, "EndLayer");
        drawing = cadcraft_io::read(&saved, "saved.dwg").unwrap();
    }
}
#[test]
fn changing_an_attribute_to_constant_updates_only_the_block_attribute_bit() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let block = std::sync::Arc::make_mut(drawing.blocks.get_mut("WithAttrs").unwrap());
    let handle = block.entities.iter().next().unwrap().handle;
    block
        .entities
        .modify(handle, |e| {
            let cadcraft_doc::EntityKind::AttDef(a) = &mut e.kind else { panic!("attribute definition") };
            a.constant = true;
        })
        .unwrap();
    assert_eq!(fields(&cadcraft_io::write(&drawing, "edited.dxf").unwrap()), (0, "BeginLayer".into(), "EndLayer".into()));
}
