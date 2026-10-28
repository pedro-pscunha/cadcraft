//! A DIMASSOC CADCraft cannot reproduce completely stays raw rather than losing references.

use cadcraft_doc::{EntityKind, Handle};
use cadcraft_dxf::{Tag, records, sections};

fn association_dxf(in_block: bool, osnaps: [i64; 2]) -> String {
    let mut groups: Vec<(i32, String)> = Vec::new();
    let mut add = |pairs: &[(i32, &str)]| groups.extend(pairs.iter().map(|(c, v)| (*c, (*v).to_string())));
    add(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1032"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "CLASSES"),
        (0, "CLASS"),
        (1, "DIMASSOC"),
        (2, "AcDbDimAssoc"),
        (3, "ObjectDBX Classes"),
        (90, "0"),
        (91, "1"),
        (280, "0"),
        (281, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
    ]);
    if in_block {
        add(&[
            (0, "BLOCK"),
            (5, "20"),
            (100, "AcDbEntity"),
            (8, "0"),
            (100, "AcDbBlockBegin"),
            (2, "LABEL"),
            (70, "0"),
            (10, "0"),
            (20, "0"),
            (3, "LABEL"),
        ]);
    } else {
        add(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    }
    add(&[
        (0, "LINE"),
        (5, "30"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "0"),
        (0, "DIMENSION"),
        (5, "40"),
        (102, "{ACAD_XDICTIONARY"),
        (360, "50"),
        (102, "}"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbDimension"),
        (10, "0"),
        (20, "5"),
        (11, "5"),
        (21, "5"),
        (70, "0"),
        (3, "Standard"),
        (100, "AcDbAlignedDimension"),
        (13, "0"),
        (23, "0"),
        (14, "10"),
        (24, "0"),
        (100, "AcDbRotatedDimension"),
        (50, "0"),
    ]);
    if in_block {
        add(&[
            (0, "ENDBLK"),
            (5, "21"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "INSERT"),
            (5, "41"),
            (100, "AcDbEntity"),
            (8, "0"),
            (100, "AcDbBlockReference"),
            (2, "LABEL"),
            (10, "0"),
            (20, "0"),
        ]);
    }
    add(&[
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (0, "DICTIONARY"),
        (5, "50"),
        (330, "40"),
        (100, "AcDbDictionary"),
        (280, "1"),
        (3, "ACAD_DIMASSOC"),
        (360, "51"),
        (0, "DIMASSOC"),
        (5, "51"),
        (102, "{ACAD_REACTORS"),
        (330, "50"),
        (102, "}"),
        (330, "50"),
        (100, "AcDbDimAssoc"),
        (330, "40"),
        (90, "3"),
        (70, "0"),
        (71, "0"),
    ]);
    for (slot, osnap) in osnaps.into_iter().enumerate() {
        groups.extend([
            (1, "AcDbOsnapPointRef".into()),
            (72, osnap.to_string()),
            (331, "30".into()),
            (73, "0".into()),
            (91, "0".into()),
            (301, "".into()),
            (40, if osnap == 1 { "1" } else { "0" }.into()),
            (10, if slot == 1 { "10" } else { "0" }.into()),
            (20, "0".into()),
            (30, "0".into()),
            (75, "0".into()),
        ]);
    }
    groups.extend([(0, "ENDSEC".into()), (0, "EOF".into())]);
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

fn section(dxf: &str, name: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let s = sections(&tags).into_iter().find(|s| s.name == name).unwrap();
    records(&s.tags)
}

fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

fn assert_association(dxf: &str, in_block: bool, osnaps: [i64; 2]) {
    let all = section(dxf, if in_block { "BLOCKS" } else { "ENTITIES" });
    let ents = if in_block {
        let start = all.iter().position(|(k, t)| k == "BLOCK" && val(t, 2) == "LABEL").expect("fixture LABEL block");
        let end = all.iter().enumerate().skip(start + 1).find(|(_, (k, _))| k == "ENDBLK").map(|(i, _)| i).expect("LABEL end");
        &all[start + 1..end]
    } else {
        all.as_slice()
    };
    let dim = &ents.iter().find(|(k, _)| k == "DIMENSION").unwrap().1;
    let line = &ents.iter().find(|(k, _)| k == "LINE").unwrap().1;
    assert!(dim.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "DIMENSION lost its extension dictionary");
    let objs = section(dxf, "OBJECTS");
    let assocs: Vec<_> = objs.iter().filter(|(k, _)| k == "DIMASSOC").collect();
    assert_eq!(assocs.len(), 1, "exactly one DIMASSOC, never raw plus modelled");
    let assoc = &assocs[0].1;
    let dict = &objs.iter().find(|(k, t)| k == "DICTIONARY" && val(t, 5) == val(dim, 360)).unwrap().1;
    assert_eq!(val(dict, 330), val(dim, 5), "xdict owner must be the written dimension");
    assert_eq!(val(dict, 3), "ACAD_DIMASSOC", "association dictionary key changed");
    assert_eq!(val(dict, 360), val(assoc, 5), "dictionary must point to its DIMASSOC object");
    assert_eq!(val(assoc, 330), val(dict, 5), "DIMASSOC owner must be its dictionary");
    assert!(assoc.iter().any(|t| t.code == 330 && t.str() == val(dim, 5)), "DIMASSOC lost its dimension reference");
    assert_eq!(val(assoc, 90), "3", "association flags changed");
    let snaps: Vec<_> = assoc.iter().filter(|t| t.code == 72).map(|t| t.i64()).collect();
    assert_eq!(snaps, osnaps, "osnap types changed");
    let refs: Vec<_> = assoc.iter().filter(|t| t.code == 331).map(Tag::str).collect();
    assert_eq!(
        refs,
        vec![val(line, 5); 2],
        "point references must target the fixture's LINE in the same space/block, not generated dimension geometry"
    );
}

fn roundtrip(in_block: bool, osnaps: [i64; 2], dwg: bool) {
    let d = cadcraft_io::read_dxf(association_dxf(in_block, osnaps).as_bytes()).unwrap();
    let saved = if dwg {
        let bytes = cadcraft_io::write(&d, "x.dwg").unwrap();
        String::from_utf8(cadcraft_dwg::dwg_to_dxf(&bytes).unwrap()).unwrap()
    } else {
        cadcraft_io::write_dxf(&d)
    };
    assert_association(&saved, in_block, osnaps);
    // A second DXF save must not start treating a partial set of links as complete.
    let back = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_association(&cadcraft_io::write_dxf(&back), in_block, osnaps);
}

#[test]
fn block_dimassoc_survives_dxf_save() {
    roundtrip(true, [13, 1], false);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn block_dimassoc_survives_dwg_save() {
    roundtrip(true, [13, 1], true);
}

#[test]
fn unsupported_dimassoc_survives_dxf_save() {
    for osnaps in [[8, 8], [13, 8]] {
        roundtrip(false, osnaps, false);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn unsupported_dimassoc_survives_dwg_save() {
    for osnaps in [[8, 8], [13, 8]] {
        roundtrip(false, osnaps, true);
    }
}

fn payload(dxf: &str) -> Vec<(i32, String)> {
    let objs = section(dxf, "OBJECTS");
    let assoc = &objs.iter().find(|(k, _)| k == "DIMASSOC").unwrap().1;
    assoc.iter().skip_while(|t| t.code != 100 || t.str() != "AcDbDimAssoc").map(|t| (t.code, t.str())).collect()
}

fn assert_raw_semantics(source: &str, dwg: bool) {
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(d.foreign.objects.iter().any(|o| o.kind == "DIMASSOC"), "equal flags/count do not prove the association is reproducible");
    let saved = cadcraft_io::write_dxf(&d);
    assert_eq!(payload(&saved), payload(source));
    if dwg {
        let bytes = cadcraft_io::write(&d, "x.dwg").unwrap();
        let back = cadcraft_dwg::dwg_to_dxf(&bytes).unwrap();
        assert_eq!(payload(&String::from_utf8_lossy(&back)), payload(source));
    }
}

#[test]
fn trans_space_dimassoc_keeps_its_original_semantics() {
    let source = association_dxf(false, [13, 1]);
    assert_eq!(source.matches("100\nAcDbDimension\n").count(), 1, "fixture must move exactly one dimension into paper space");
    assert_eq!(source.matches("90\n3\n70\n0\n71\n0\n").count(), 1, "fixture must set exactly one trans-space flag");
    let source = source.replace("100\nAcDbDimension\n", "67\n1\n100\nAcDbDimension\n").replace("90\n3\n70\n0\n71\n0\n", "90\n3\n70\n1\n71\n0\n");
    assert_raw_semantics(&source, true);
}

#[test]
fn rotated_dimassoc_keeps_its_original_semantics() {
    let source = association_dxf(false, [13, 1]).replace("90\n3\n70\n0\n71\n0\n", "90\n3\n70\n0\n71\n1\n");
    assert_raw_semantics(&source, true);
}

#[test]
fn quadrant_dimassoc_is_not_rewritten_as_nearest() {
    let source = association_dxf(false, [5, 5]).replace(
        "0\nLINE\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n11\n10\n21\n0\n",
        "0\nCIRCLE\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbCircle\n10\n0\n20\n0\n40\n10\n",
    );
    assert_raw_semantics(&source, true);
}

#[test]
fn dimassoc_keeps_parameters_the_inferred_snap_does_not_reproduce() {
    let source = association_dxf(false, [13, 1]).replace("40\n0\n10\n0\n", "40\n0.25\n10\n0\n");
    assert_raw_semantics(&source, true);
}

#[test]
fn dimassoc_keeps_handle_paths_and_subentity_markers() {
    let source = association_dxf(false, [13, 1]).replace("331\n30\n73\n0\n91\n0\n301\n\n", "331\n30\n331\n30\n73\n1\n91\n1\n301\nnested-path\n");
    assert_raw_semantics(&source, false);
}

#[test]
fn unevaluable_inferred_snap_stays_raw_even_when_serialization_matches() {
    let source = association_dxf(false, [2, 2]);
    let line = "0\nLINE\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n11\n10\n21\n0\n";
    assert_eq!(source.matches(line).count(), 1);
    let source = source.replace(line,"0\nSPLINE\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbSpline\n70\n0\n71\n1\n72\n4\n73\n2\n74\n0\n40\n0\n40\n0\n40\n1\n40\n1\n10\n-10\n20\n0\n30\n0\n10\n10\n20\n0\n30\n0\n")
        .replace("14\n10\n24\n0\n","14\n0\n24\n0\n").replace("40\n0\n10\n10\n20\n0\n","40\n0\n10\n0\n20\n0\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(
        d.foreign.objects.iter().any(|o| o.kind == "DIMASSOC"),
        "a serialized Mid/Spline pair the engine cannot evaluate was treated as fully modelled"
    );
}

#[test]
fn supported_dimassoc_handle_spelling_does_not_make_it_opaque() {
    let source = association_dxf(false, [13, 1]);
    assert_eq!(source.matches("5\n30\n").count(), 1, "fixture LINE handle must be unique");
    assert_eq!(source.matches("331\n30\n").count(), 2, "fixture must name the LINE in both point references");
    let source = source.replace("5\n30\n", "5\n2A\n").replace("331\n30\n", "331\n002a\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(d.foreign.objects.iter().all(|o| o.kind != "DIMASSOC"), "equivalent hexadecimal pointers were treated as changed semantics");
}

#[test]
fn normalized_subclass_parsing_finds_the_dimension_not_the_dictionary() {
    let source = association_dxf(false, [13, 1]);
    assert_eq!(source.matches("100\nAcDbDimAssoc\n").count(), 1);
    let d = cadcraft_io::read_dxf(source.replace("100\nAcDbDimAssoc\n", "100\n AcDbDimAssoc \n").as_bytes()).unwrap();
    let EntityKind::Dimension(dimension) = &d.entity(Handle(0x40)).unwrap().kind else { panic!("dimension") };
    assert_eq!(dimension.assoc.len(), 2, "normalized subclass owner parsing used the header's dictionary 330");
}

#[test]
fn unsupported_header_multiplicity_is_preserved_instead_of_regenerated() {
    let source = association_dxf(false, [13, 1]);
    assert_eq!(source.matches("102\n{ACAD_REACTORS\n330\n50\n").count(), 1);
    let source = source.replace("102\n{ACAD_REACTORS\n330\n50\n", "102\n{ACAD_REACTORS\n330\n50\n330\n50\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(d.foreign.objects.iter().any(|o| o.kind == "DIMASSOC"), "header was not validated against the emitted header grammar");
}

#[test]
fn complete_dimassoc_is_written_once_from_model() {
    let d = cadcraft_io::read_dxf(association_dxf(false, [13, 1]).as_bytes()).unwrap();
    assert!(d.foreign.objects.iter().all(|o| o.kind != "DIMASSOC"), "complete associations belong to the model, not raw data");
    roundtrip(false, [13, 1], false);
}

#[test]
fn only_declared_owner_can_attach_a_raw_extension_dictionary() {
    let mut source = association_dxf(false, [8, 8]);
    let start = source.find("0\nDIMENSION\n").unwrap();
    let end = source.find("0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n").unwrap();
    let duplicate = source[start..end].replace("5\n40\n", "5\n41\n");
    assert_eq!(duplicate.matches("5\n41\n").count(), 1, "fixture must make a distinct second owner");
    source.insert_str(end, &duplicate);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let entities = section(&text, "ENTITIES");
    let owner = &entities.iter().find(|(k, t)| k == "DIMENSION" && val(t, 5) == "40").unwrap().1;
    let other = &entities.iter().find(|(k, t)| k == "DIMENSION" && val(t, 5) == "41").unwrap().1;
    assert!(!val(owner, 360).is_empty(), "valid declared owner lost its dictionary");
    assert!(val(other, 360).is_empty(), "a second entity claimed another entity's extension dictionary");
}

#[test]
fn modelled_dimension_dictionary_merges_foreign_entries_reactors_and_xdata() {
    let source = association_dxf(false, [13, 1]);
    assert_eq!(source.matches("0\nDICTIONARY\n5\n50\n330\n40\n").count(), 1);
    assert_eq!(source.matches("3\nACAD_DIMASSOC\n360\n51\n0\nDIMASSOC\n").count(), 1);
    let source = source
        .replace("0\nDICTIONARY\n5\n50\n330\n40\n", "0\nDICTIONARY\n5\n50\n102\n{ACAD_REACTORS\n330\n52\n102\n}\n330\n40\n")
        .replace(
            "3\nACAD_DIMASSOC\n360\n51\n0\nDIMASSOC\n",
            "3\nACAD_DIMASSOC\n360\n51\n3\nFOREIGN_KEEP\n350\n52\n1001\nFOREIGN_DICT\n1000\ndictionary data\n1005\n52\n0\nDIMASSOC\n",
        )
        .replace("0\nENDSEC\n0\nEOF\n", "0\nXRECORD\n5\n52\n330\n50\n100\nAcDbXrecord\n280\n1\n1\npayload\n0\nENDSEC\n0\nEOF\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(d.foreign.objects.iter().all(|o| o.kind != "DIMASSOC"), "fixture must exercise a regenerated modelled association");
    let check = |text: &str| {
        let entities = section(text, "ENTITIES");
        let dimension = &entities.iter().find(|(k, _)| k == "DIMENSION").unwrap().1;
        let objects = section(text, "OBJECTS");
        let children: Vec<_> = objects.iter().filter(|(k, t)| k == "XRECORD" && val(t, 1) == "payload").collect();
        assert_eq!(children.len(), 1, "modelled xdict discarded or duplicated the foreign entry's object");
        let child = &children[0].1;
        let dictionaries: Vec<_> = objects.iter().filter(|(k, t)| k == "DICTIONARY" && val(t, 5) == val(dimension, 360)).collect();
        assert_eq!(dictionaries.len(), 1, "modelled and raw extension dictionaries were written twice");
        let dictionary = &dictionaries[0].1;
        assert_eq!(
            cadcraft_doc::foreign::object_owner(dictionary.iter().map(|t| (t.code, t.str()))),
            Handle::parse_hex(&val(dimension, 5)),
            "merged dictionary owner is not the written dimension"
        );
        assert_eq!(val(child, 330), val(dictionary, 5), "foreign child owner did not follow the modelled dictionary");
        for key in ["ACAD_DIMASSOC", "FOREIGN_KEEP"] {
            assert_eq!(dictionary.iter().filter(|t| t.code == 3 && t.str() == key).count(), 1, "merged dictionary key {key} is absent or duplicated");
        }
        assert_eq!(objects.iter().filter(|(k, _)| k == "DIMASSOC").count(), 1, "merge replaced the modelled association or wrote it twice");
        assert!(dictionary.iter().any(|t| t.code == 330 && t.str() == val(child, 5)), "foreign dictionary reactor disappeared");
        assert_eq!(val(dictionary, 1001), "FOREIGN_DICT", "foreign dictionary xdata disappeared");
        assert_eq!(val(dictionary, 1005), val(child, 5), "dictionary xdata reference was not remapped");
        assert!(
            dictionary.iter().rposition(|t| t.code == 3).unwrap() < dictionary.iter().position(|t| t.code == 1001).unwrap(),
            "entries were appended after xdata"
        );
    };
    let text = cadcraft_io::write_dxf(&d);
    check(&text);
    check(&cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()));
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    check(&String::from_utf8_lossy(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()));
}

#[test]
fn raw_dimassoc_follows_owner_lifetime_like_other_foreign_data() {
    let mut d = cadcraft_io::read_dxf(association_dxf(false, [8, 8]).as_bytes()).unwrap();
    let dim = Handle(0x40);
    d.modify_entity(dim, |e| {
        if let EntityKind::Dimension(dm) = &mut e.kind {
            dm.text = "edited label".into();
        }
    })
    .unwrap();
    assert_association(&cadcraft_io::write_dxf(&d), false, [8, 8]);
    d.remove_entity(dim);
    assert!(section(&cadcraft_io::write_dxf(&d), "OBJECTS").iter().all(|(k, _)| k != "DIMASSOC"));
}
