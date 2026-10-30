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
fn solid_hatches_keep_inactive_gradient_data_without_becoming_gradients() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nHATCH\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n4\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n10\n0\n20\n10\n97\n0\n75\n0\n76\n1\n98\n0\n450\n0\n451\n0\n460\n0.25\n461\n0.75\n452\n1\n462\n0.6\n453\n2\n463\n0\n63\n3\n421\n66051\n463\n1\n63\n5\n421\n263430\n470\nSPHERICAL\n0\nENDSEC\n0\nEOF\n";
    let gradient_codes = [450, 451, 460, 461, 452, 462, 453, 463, 63, 421, 470];
    let fields = |record: &[Tag]| record.iter().filter(|t| gradient_codes.contains(&t.code)).map(|t| (t.code, t.str())).collect::<Vec<_>>();
    let expected = fields(&entity_record(source, "HATCH"));
    for name in ["solid.dxf", "solid.dwg"] {
        let mut drawing = cadcraft_io::read(source, "source.dxf").unwrap();
        drawing
            .modify_entity(cadcraft_doc::Handle(0x200), |e| {
                let cadcraft_doc::EntityKind::Hatch(hatch) = &mut e.kind else { panic!("hatch") };
                assert!(hatch.solid && hatch.gradient.is_none());
                for vertex in hatch.loops.iter_mut().flat_map(|l| &mut l.vertices) {
                    vertex.p.x += 1.0;
                }
            })
            .unwrap();
        for _ in 0..2 {
            let dxf = saved_dxf(&drawing, name);
            assert_eq!(fields(&entity_record(&dxf, "HATCH")), expected, "{name}");
            drawing = cadcraft_io::read(&dxf, "saved.dxf").unwrap();
            let cadcraft_doc::EntityKind::Hatch(hatch) = &drawing.entity(cadcraft_doc::Handle(0x200)).unwrap().kind else { panic!("hatch") };
            assert!(hatch.solid && hatch.gradient.is_none());
        }
        let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_dxf::Version::R2000), ..Default::default() };
        let (old, _) = cadcraft_io::write_with_noted(&drawing, "old.dxf", &opts).unwrap();
        assert!(fields(&entity_record(&old, "HATCH")).is_empty());
        drawing
            .modify_entity(cadcraft_doc::Handle(0x200), |e| {
                let cadcraft_doc::EntityKind::Hatch(hatch) = &mut e.kind else { panic!("hatch") };
                hatch.gradient = Some(cadcraft_doc::Gradient {
                    name: "LINEAR".into(),
                    color1: cadcraft_color::Color::Index(1),
                    color2: cadcraft_color::Color::Index(2),
                    angle: 0.0,
                    centered: true,
                    source: Default::default(),
                });
            })
            .unwrap();
        let dxf = saved_dxf(&drawing, name);
        let rec = entity_record(&dxf, "HATCH");
        assert_eq!(rec.iter().filter(|t| t.code == 450).map(Tag::i64).collect::<Vec<_>>(), vec![1]);
        assert_eq!(rec.iter().filter(|t| t.code == 470).map(Tag::str).collect::<Vec<_>>(), vec!["LINEAR"]);
    }
}

#[test]
fn block_boundary_entities_keep_independent_colors_and_linetype_scales() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n70\n1\n0\nBLOCK_RECORD\n5\n50\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\nSourceBlock\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n51\n330\n50\n100\nAcDbEntity\n8\n0\n48\n2.5\n62\n3\n100\nAcDbBlockBegin\n2\nSourceBlock\n70\n0\n10\n0\n20\n0\n30\n0\n3\nSourceBlock\n0\nLINE\n5\n52\n330\n50\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n10\n21\n0\n31\n0\n0\nENDBLK\n5\n53\n330\n50\n100\nAcDbEntity\n8\n0\n48\n3.75\n62\n5\n100\nAcDbBlockEnd\n0\nENDSEC\n0\nEOF\n";
    for name in ["boundary.dxf", "boundary.dwg"] {
        let mut drawing = cadcraft_io::read(source, "source.dxf").unwrap();
        for _ in 0..2 {
            let dxf = saved_dxf(&drawing, name);
            let tags = cadcraft_dxf::parse(&dxf).unwrap();
            let records: Vec<_> =
                cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name == "BLOCKS").flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
            let start = records.iter().position(|(k, r)| k == "BLOCK" && r.iter().any(|t| t.code == 2 && t.str() == "SourceBlock")).unwrap();
            let begin = &records[start].1;
            let end = &records.iter().skip(start + 1).find(|(k, _)| k == "ENDBLK").unwrap().1;
            for (rec, scale, color) in [(begin, 2.5, 3.0), (end, 3.75, 5.0)] {
                let entity =
                    rec.iter().skip_while(|t| t.code != 100 || t.str() != "AcDbEntity").skip(1).take_while(|t| t.code != 100).collect::<Vec<_>>();
                let actual: Vec<_> =
                    [48, 62].into_iter().map(|code| (code, entity.iter().filter(|t| t.code == code).map(|t| t.f64()).collect::<Vec<_>>())).collect();
                assert_eq!(actual, vec![(48, vec![scale]), (62, vec![color])], "{name}");
            }
            drawing = cadcraft_io::read(&dxf, "saved.dxf").unwrap();
        }
    }
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
