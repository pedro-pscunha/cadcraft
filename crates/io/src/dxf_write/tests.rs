use super::*;
use cadcraft_doc::Space;
use std::cell::Cell;
std::thread_local! { pub(super) static SCANNED: Cell<usize> = const { Cell::new(0) }; }

#[test]
fn inactive_gradient_replay_uses_the_dwg_target_not_the_bridge_declaration() {
    let drawing = crate::read_dxf(b"0\nSECTION\n2\nENTITIES\n0\nHATCH\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n2\nSOLID\n70\n1\n71\n0\n91\n0\n75\n0\n76\n1\n98\n0\n450\n0\n453\n2\n421\n66051\n421\n263430\n470\nSPHERICAL\n0\nENDSEC\n0\nEOF\n").unwrap();
    for target in [Version::R2000, Version::R2018] {
        let tags = cadcraft_dxf::parse(&dwg_bridge(&drawing, target)).unwrap();
        let record = cadcraft_dxf::sections(&tags)
            .into_iter()
            .filter(|s| s.name == "ENTITIES")
            .flat_map(|s| cadcraft_dxf::records(&s.tags))
            .find(|(k, _)| k == "HATCH")
            .unwrap()
            .1;
        let colors: Vec<_> = record.iter().filter(|t| t.code == 421).map(Tag::i64).collect();
        if target == Version::R2000 {
            assert!(colors.is_empty());
        } else {
            assert_eq!(colors, vec![66051, 263430]);
            assert_eq!(record.iter().find(|t| t.code == 450).unwrap().i64(), 0);
        }
    }
}

#[test]
fn named_shape_style_stays_named_in_the_bridge_reader() {
    let drawing = crate::read_dxf(include_bytes!("../../tests/data/named_shape_style.dxf")).unwrap();
    let bytes = dwg_bridge(&drawing, Version::R2018);
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let styles: Vec<_> = cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|s| s.name == "TABLES")
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .filter(|(k, _)| k == "STYLE")
        .collect();
    let source = &styles.iter().find(|(_, t)| t.iter().any(|t| t.code == 2 && t.str() == "SourceShapes")).unwrap().1;
    assert_eq!(source.iter().find(|t| t.code == 70).unwrap().i64() & 5, 5);
    assert_eq!(source.iter().find(|t| t.code == 42).unwrap().f64(), 12.75);
    let mut doc = acadrust::DxfReader::from_reader(std::io::Cursor::new(bytes)).unwrap().read().unwrap();
    assert!(
        doc.text_styles.iter().any(|s| s.name == "SourceShapes"),
        "reader styles: {:?}",
        doc.text_styles.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    doc.version = acadrust::DxfVersion::AC1032;
    let dwg = acadrust::DwgWriter::write_to_vec(&doc).unwrap();
    let doc = acadrust::DwgReader::from_stream(std::io::Cursor::new(dwg)).read().unwrap();
    assert!(
        doc.text_styles.iter().any(|s| s.name == "SourceShapes"),
        "DWG reader styles: {:?}",
        doc.text_styles.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

#[test]
fn dimstyle_fidelity_bridge_private_variables_are_plain_groups_only_for_supported_dwg_targets() {
    let mut d = Drawing::new_imperial();
    let fields = serde_json::json!({"DIMMZF": 12.5, "DIMALTMZF": 25.0, "DIMMZS": "m", "DIMALTMZS": "cm"});
    assert!(d.dim_styles[0].apply_fields(fields.as_object().unwrap()).is_empty());
    for target in [Version::R2000, Version::R2007, Version::R2010, Version::R2018] {
        let tags = tags(&d, target);
        // The public tag writer is not the bridge, even when its version is 2018.
        let bytes = dwg_bridge(&d, target);
        let bridge = cadcraft_dxf::parse(&bytes).unwrap();
        let has_private = |tags: &[Tag]| tags.iter().any(|t| t.code == 51 && t.f64() == 12.5);
        assert!(!has_private(&tags), "public DXF {target:?}");
        assert_eq!(has_private(&bridge), target >= Version::R2010, "DWG {target:?}");
        if target >= Version::R2010 {
            assert!(bridge.iter().any(|t| t.code == 52 && t.f64() == 25.0));
            assert!(bridge.iter().any(|t| t.code == 300 && t.str() == "m"));
            assert!(bridge.iter().any(|t| t.code == 301 && t.str() == "cm"));
        }
    }
}

#[test]
fn dimstyle_fidelity_bridge_true_colors_keep_all_24_bits() {
    let mut d = Drawing::new_imperial();
    let rgb = Color::True(cadcraft_color::Rgb(187, 130, 157));
    let st = &mut d.dim_styles[0];
    st.dim_line_color = rgb;
    st.ext_line_color = rgb;
    st.text_color = rgb;
    st.text_fill = 2;
    st.text_fill_color = rgb;
    for version in [Version::R2000, Version::R2004, Version::R2007, Version::R2018] {
        let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, version)).unwrap();
        let public = tags(&d, version);
        for code in 421..=424 {
            assert!(!public.iter().any(|t| t.code == code && t.i64() == 0xbb829d), "public DXF {version:?}/{code}");
            let supported = version >= if code == 424 { Version::R2007 } else { Version::R2004 };
            assert_eq!(bridge.iter().any(|t| t.code == code && t.i64() == 0xbb829d), supported, "{version:?}/{code}");
        }
    }
}

#[test]
fn private_header_variables_stay_in_the_dwg_bridge() {
    let mut d = Drawing::new_imperial();
    for (name, code, value) in [
        ("ISOLINES", 70, HVal::Int(4)),
        ("FACETRES", 40, HVal::Real(0.5)),
        ("TEXTQLTY", 70, HVal::Int(50)),
        ("PELLIPSE", 70, HVal::Int(0)),
        ("DIMMZF", 40, HVal::Real(1.0)),
        ("DIMMZS", 1, HVal::Str("m".into())),
        ("DIMALTMZF", 40, HVal::Real(1.0)),
        ("DIMALTMZS", 1, HVal::Str("cm".into())),
        ("TITLE", 1, HVal::Str("Plan".into())),
        ("SUBJECT", 1, HVal::Str("Garden".into())),
        ("AUTHOR", 1, HVal::Str("Arterra".into())),
        ("KEYWORDS", 1, HVal::Str("landscape".into())),
        ("COMMENTS", 1, HVal::Str("none".into())),
        ("LASTSAVEDBY", 1, HVal::Str("User".into())),
        ("REVISIONNUMBER", 1, HVal::Str("3".into())),
    ] {
        d.header.set(name, value);
        d.header.dxf_codes.insert(name.into(), vec![code]);
    }
    let public = tags(&d, Version::R2018);
    let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, Version::R2018)).unwrap();
    // The drawing properties are not DXF header variables: AutoCAD reports `$LASTSAVEDBY` as an
    // unknown variable, and its own DXFOUT writes none of them.
    for name in [
        "ISOLINES",
        "FACETRES",
        "TEXTQLTY",
        "PELLIPSE",
        "DIMMZF",
        "DIMMZS",
        "DIMALTMZF",
        "DIMALTMZS",
        "TITLE",
        "SUBJECT",
        "AUTHOR",
        "KEYWORDS",
        "COMMENTS",
        "LASTSAVEDBY",
        "REVISIONNUMBER",
    ] {
        let variable = format!("${name}");
        assert!(!public.iter().any(|t| t.code == 9 && t.str() == variable), "public {variable}");
        assert!(bridge.iter().any(|t| t.code == 9 && t.str() == variable), "bridge {variable}");
    }
}

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

/// A dynamic block's purge preventer as the DWG bridge reads it: with the block in group 340.
fn purge_preventer_dxf() -> String {
    let groups: &[(i32, &str)] = &[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1032"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "CLASSES"),
        (0, "CLASS"),
        (1, "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION"),
        (2, "AcDbDynamicBlockPurgePreventer"),
        (3, "ObjectDBX Classes"),
        (90, "1153"),
        (91, "1"),
        (280, "0"),
        (281, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "BLOCK_RECORD"),
        (5, "1"),
        (330, "0"),
        (100, "AcDbSymbolTable"),
        (70, "1"),
        (0, "BLOCK_RECORD"),
        (5, "80"),
        (102, "{ACAD_XDICTIONARY"),
        (360, "81"),
        (102, "}"),
        (330, "1"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbBlockTableRecord"),
        (2, "Door"),
        (70, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (5, "82"),
        (330, "80"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockBegin"),
        (2, "Door"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (3, "Door"),
        (1, ""),
        (0, "LINE"),
        (5, "84"),
        (330, "80"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "1"),
        (21, "0"),
        (31, "0"),
        (0, "ENDBLK"),
        (5, "83"),
        (330, "80"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockEnd"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (281, "1"),
        (0, "DICTIONARY"),
        (5, "81"),
        (330, "80"),
        (100, "AcDbDictionary"),
        (280, "1"),
        (281, "1"),
        (3, "AcDbDynamicBlockRoundTripPurgePreventer"),
        (360, "85"),
        (0, "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION"),
        (5, "85"),
        (102, "{ACAD_REACTORS"),
        (330, "81"),
        (102, "}"),
        (330, "81"),
        (100, "AcDbDynamicBlockPurgePreventer"),
        (70, "1"),
        (340, "80"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// The groups of the purge preventer record in `tags`.
fn purge_preventer(tags: &[Tag]) -> Vec<Tag> {
    let at = tags.iter().position(|t| t.code == 0 && t.str() == "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION").expect("purge preventer");
    tags[at + 1..].iter().take_while(|t| t.code != 0).cloned().collect()
}

#[test]
fn purge_preventer_block_pointer_stays_in_the_dwg_bridge() {
    // AutoCAD's DXFOUT writes the purge preventer without group 340, and AutoCAD LT 2025 refuses a
    // DXF whose preventer has it ("Fim prematuro do objeto"). The DWG bridge keeps it.
    let d = crate::read_dxf(purge_preventer_dxf().as_bytes()).unwrap();
    let public = purge_preventer(&tags(&d, Version::R2018));
    assert!(public.iter().any(|t| t.code == 70 && t.i64() == 1), "public preventer keeps its flags: {public:?}");
    assert!(!public.iter().any(|t| t.code == 340), "public preventer has no block pointer: {public:?}");
    let bridge = purge_preventer(&cadcraft_dxf::parse(&dwg_bridge(&d, Version::R2018)).unwrap());
    assert!(bridge.iter().any(|t| t.code == 340 && t.str() != "0"), "bridge preventer keeps its block: {bridge:?}");
}

#[test]
fn the_surface_modeler_cache_stays_in_the_private_bridge_without_dropping_xdata() {
    let text = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nSURFACE\n5\n60\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbSurface\n71\n6\n72\n6\n100\nAcDbAcadrustSurfaceCache\n310\nAABBCC\n340\n0\n1001\nACME\n1000\nsurface-user-data\n0\nENDSEC\n0\nEOF\n";
    let d = crate::read_dxf(text.as_bytes()).unwrap();
    let public = tags(&d, Version::R2018);
    assert!(!public.iter().any(|tag| tag.str() == "AcDbAcadrustSurfaceCache"));
    assert!(!public.iter().any(|tag| tag.code == 310 && tag.str() == "AABBCC"));
    assert!(public.iter().any(|tag| tag.code == 1000 && tag.str() == "surface-user-data"));
    let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, Version::R2018)).unwrap();
    assert!(bridge.iter().any(|tag| tag.str() == "AcDbAcadrustSurfaceCache"));
    assert!(bridge.iter().any(|tag| tag.code == 310 && tag.str() == "AABBCC"));
    assert!(bridge.iter().any(|tag| tag.code == 1000 && tag.str() == "surface-user-data"));
}

/// The bridge records the DWG layout of the bytes of an embedded entity in group 455, in SOLID history steps and
/// surfaces. A user DXF holds only the groups of the DXF Reference; the DWG bridge keeps the group.
#[test]
fn the_embedded_layout_group_stays_in_the_dwg_bridge_only() {
    let text: String = [
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC",
        "0\nSECTION\n2\nENTITIES",
        "0\nEXTRUDEDSURFACE\n5\n60\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbSurface\n310\nAABBCC\n455\n1032\n90\n4",
        "0\nENDSEC\n0\nSECTION\n2\nOBJECTS",
        "0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nACME_STEP\n350\n61",
        "0\nACSH_SWEEP_CLASS\n5\n61\n330\nC\n100\nAcDbEvalExpr\n90\n1\n310\nDDEE\n455\n1032\n90\n9",
        "0\nENDSEC\n0\nEOF\n",
    ]
    .iter()
    .flat_map(|chunk| chunk.lines())
    .collect::<Vec<_>>()
    .chunks(2)
    .map(|p| format!("{}\n{}\n", p[0], p.get(1).copied().unwrap_or("")))
    .collect();
    let d = crate::read_dxf(text.as_bytes()).unwrap();
    let kept = |tags: &[Tag]| tags.iter().filter(|t| t.code == 455 && t.i64() == 1032).count();
    for target in [Version::R2000, Version::R2010, Version::R2018] {
        let public = tags(&d, target);
        assert_eq!(kept(&public), 0, "public DXF {target:?}");
        // The bytes are in the layout of 2018 (the 455 value): another release gets them encoded again.
        if target == Version::R2018 {
            assert!(public.iter().any(|t| t.code == 310 && t.str() == "AABBCC"), "the surface bytes stay: {target:?}");
            assert!(public.iter().any(|t| t.code == 310 && t.str() == "DDEE"), "the history bytes stay: {target:?}");
        }
        let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, target)).unwrap();
        assert_eq!(kept(&bridge), 2, "DWG bridge {target:?}");
    }
}
