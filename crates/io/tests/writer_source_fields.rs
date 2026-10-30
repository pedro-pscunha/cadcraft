use cadcraft_dxf::Tag;

fn entity_record(bytes: &[u8], kind: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .find(|(k, _)| k == kind)
        .unwrap()
        .1
}

fn saved_dxf(drawing: &cadcraft_doc::Drawing, name: &str) -> Vec<u8> {
    let bytes = cadcraft_io::write(drawing, name).unwrap();
    if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes }
}

#[test]
fn dimension_presentation_fields_survive_geometry_edits_and_native_saves() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nDIMENSION\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n280\n1\n10\n0\n20\n3\n30\n0\n11\n5\n21\n3\n31\n0\n70\n32\n71\n9\n72\n2\n41\n1.375\n73\n1\n74\n1\n75\n1\n100\nAcDbAlignedDimension\n13\n0\n23\n0\n33\n0\n14\n10\n24\n0\n34\n0\n50\n0\n100\nAcDbRotatedDimension\n0\nENDSEC\n0\nEOF\n";
    for name in ["presentation.dxf", "presentation.dwg"] {
        for edited in [false, true] {
            let mut drawing = cadcraft_io::read(source, "source.dxf").unwrap();
            if edited {
                drawing
                    .modify_entity(cadcraft_doc::Handle(0x200), |e| {
                        let cadcraft_doc::EntityKind::Dimension(dim) = &mut e.kind else { panic!("dimension") };
                        dim.p14.x += 3.0;
                    })
                    .unwrap();
            }
            for _ in 0..2 {
                let dxf = saved_dxf(&drawing, name);
                let rec = entity_record(&dxf, "DIMENSION");
                let start = rec.iter().position(|t| t.code == 100 && t.str() == "AcDbDimension").unwrap();
                let own: Vec<_> = rec.iter().skip(start + 1).take_while(|t| t.code != 100).collect();
                let expected = [(71, 9.0), (72, 2.0), (41, 1.375), (73, 1.0), (74, 1.0), (75, 1.0), (280, 1.0)];
                let actual: Vec<_> =
                    expected.iter().map(|(code, _)| (*code, own.iter().filter(|t| t.code == *code).map(|t| t.f64()).collect::<Vec<_>>())).collect();
                let expected: Vec<_> = expected.into_iter().map(|(code, value)| (code, vec![value])).collect();
                assert_eq!(actual, expected, "{name} edited={edited}");
                drawing = cadcraft_io::read(&dxf, "saved.dxf").unwrap();
            }
        }
    }
}

#[test]
fn single_column_mtext_keeps_its_defined_annotation_height_after_an_edit() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nMTEXT\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbMText\n10\n1\n20\n2\n30\n0\n40\n2.5\n41\n20\n46\n12.75\n71\n1\n72\n1\n1\nSource\n0\nENDSEC\n0\nEOF\n";
    for name in ["height.dxf", "height.dwg"] {
        let mut drawing = cadcraft_io::read(source, "source.dxf").unwrap();
        drawing
            .modify_entity(cadcraft_doc::Handle(0x200), |e| {
                let cadcraft_doc::EntityKind::MText(text) = &mut e.kind else { panic!("mtext") };
                text.insert.x += 10.0;
                text.contents = "Edited".into();
            })
            .unwrap();
        for _ in 0..2 {
            let dxf = saved_dxf(&drawing, name);
            let rec = entity_record(&dxf, "MTEXT");
            let height: Vec<_> = rec.iter().take_while(|t| t.code != 101).filter(|t| t.code == 46).map(Tag::f64).collect();
            assert_eq!(height, vec![12.75], "{name}");
            drawing = cadcraft_io::read(&dxf, "saved.dxf").unwrap();
        }
    }
}
