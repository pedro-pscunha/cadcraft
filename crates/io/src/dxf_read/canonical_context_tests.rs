use super::*;

fn tags(source: &[u8]) -> Vec<Tag> {
    cadcraft_dxf::parse(source).unwrap()
}

#[test]
fn canonical_polyline_uses_document_plot_styles_and_the_shared_child_identity_pool() {
    let mut drawing = Drawing::new_metric();
    drawing.bump_handseed(Handle(0x900));
    let mut rx = Rx::default();
    rx.pstyles.insert("F00".into(), "NamedPlotStyle".into());
    let parent = tags(b"5\n200\n100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDb2dPolyline\n70\n0\n10\n0\n20\n0\n30\n7\n");
    let children = vec![
        ("VERTEX".into(), tags(b"100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n1\n20\n2\n30\n0\n")),
        ("VERTEX".into(), tags(b"5\n500\n100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n3\n20\n4\n30\n0\n")),
        ("SEQEND".into(), tags(b"100\nAcDbEntity\n8\n0\n390\nF00\n")),
    ];
    let (common, kind) = canonical_polyline_from_groups(&parent, &children, &mut drawing, &rx).unwrap();
    assert_eq!(common.plot_style, "NamedPlotStyle");
    let EntityKind::LwPolyline(polyline) = kind else { panic!("polyline") };
    assert_eq!(polyline.elevation, 7.0);
    assert_eq!(polyline.records.vertices.iter().map(|record| record.handle).collect::<Vec<_>>(), [Some(Handle(0x901)), Some(Handle(0x500))]);
    assert_eq!(polyline.records.seqend.as_ref().unwrap().handle, Some(Handle(0x902)));
    assert!(polyline.records.vertices.iter().chain(polyline.records.seqend.iter()).all(|record| record.common.plot_style == "NamedPlotStyle"));
    assert_eq!(drawing.handseed, 0x903);
}

#[test]
fn canonical_attrib_uses_the_document_namespace_plot_style_and_dwg_origin() {
    let record = tags(b"100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDbText\n10\n1\n20\n2\n30\n3\n40\n1\n1\nBody\n100\nAcDbAttribute\n2\nTAG\n70\n0\n71\n2\n72\n19\n101\nEmbedded Object\n10\n4\n20\n5\n30\n6\n40\n1\n41\n10\n1\nBody\n11\n1\n21\n0\n31\n0\n210\n0\n220\n3\n230\n4\n");
    for from_dwg in [false, true] {
        let mut drawing = Drawing::new_metric();
        drawing.bump_handseed(Handle(0x900));
        let mut rx = Rx { from_dwg, ..Default::default() };
        rx.pstyles.insert("F00".into(), "NamedPlotStyle".into());
        let attribute = canonical_attrib_from_groups(&record, &mut drawing, &rx).unwrap();
        assert_eq!(attribute.handle, Some(Handle(0x901)));
        assert_eq!(attribute.props.plot_style.as_deref(), Some("NamedPlotStyle"));
        assert_eq!(drawing.handseed, 0x902);
        let multiline = attribute.multiline.unwrap();
        assert_eq!(multiline.normal, Vec3::new(0.0, 3.0, 4.0));
        assert_eq!(multiline.type_72, if from_dwg { None } else { Some(19) });
    }
}

#[test]
fn a_canonical_read_with_an_unresolved_style_refuses_without_allocating_an_identity() {
    let mut drawing = Drawing::new_metric();
    let before = drawing.handseed;
    let record = tags(b"5\nA00\n100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDbCircle\n10\n1\n20\n2\n30\n3\n40\n4\n");
    assert!(canonical_curve_from_groups("CIRCLE", &record, &mut drawing, &Rx::default()).is_none());
    assert_eq!(drawing.handseed, before);
}

#[test]
fn canonical_polyline_reserves_explicit_children_before_allocating_missing_identities() {
    let mut drawing = Drawing::new_metric();
    let parent = tags(b"5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDb2dPolyline\n70\n0\n");
    let children = vec![
        ("VERTEX".into(), tags(b"100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n1\n20\n2\n30\n0\n")),
        ("VERTEX".into(), tags(b"5\nA00\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n3\n20\n4\n30\n0\n")),
        ("SEQEND".into(), tags(b"100\nAcDbEntity\n8\n0\n")),
    ];
    let (_, kind) = canonical_polyline_from_groups(&parent, &children, &mut drawing, &Rx::default()).unwrap();
    let EntityKind::LwPolyline(polyline) = kind else { panic!("polyline") };
    assert_eq!(polyline.records.vertices[0].handle, Some(Handle(0xA01)));
    assert_eq!(polyline.records.vertices[1].handle, Some(Handle(0xA00)));
    assert_eq!(polyline.records.seqend.unwrap().handle, Some(Handle(0xA02)));
}

#[test]
fn a_null_plot_style_reference_is_not_an_unresolved_required_style() {
    let mut drawing = Drawing::new_metric();
    let record = tags(b"5\nA00\n100\nAcDbEntity\n8\n0\n390\n0000\n100\nAcDbCircle\n10\n1\n20\n2\n30\n3\n40\n4\n");
    let (common, _) = canonical_curve_from_groups("CIRCLE", &record, &mut drawing, &Rx::default()).unwrap();
    assert_eq!(common.plot_style, "ByLayer");
}

#[test]
fn canonical_single_records_resolve_common_properties_from_the_real_reference_maps() {
    let mut drawing = Drawing::new_metric();
    let mut rx = Rx::default();
    rx.pstyles.insert("F00".into(), "NamedPlotStyle".into());
    let record = tags(b"5\nA00\n100\nAcDbEntity\n8\n0\n390\nF00\n100\nAcDbCircle\n10\n1\n20\n2\n30\n3\n40\n4\n");
    let (common, _) = canonical_curve_from_groups("CIRCLE", &record, &mut drawing, &rx).unwrap();
    assert_eq!(common.plot_style, "NamedPlotStyle");
    assert_eq!(drawing.handseed, 0xA01);
}
