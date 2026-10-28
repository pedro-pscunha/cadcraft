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
    let ents = section(dxf, if in_block { "BLOCKS" } else { "ENTITIES" });
    let dim = &ents.iter().find(|(k, _)| k == "DIMENSION").unwrap().1;
    let line = &ents.iter().find(|(k, _)| k == "LINE").unwrap().1;
    assert!(dim.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "DIMENSION lost its extension dictionary");
    let objs = section(dxf, "OBJECTS");
    let assocs: Vec<_> = objs.iter().filter(|(k, _)| k == "DIMASSOC").collect();
    assert_eq!(assocs.len(), 1, "exactly one DIMASSOC, never raw plus modelled");
    let assoc = &assocs[0].1;
    let dict = &objs.iter().find(|(k, t)| k == "DICTIONARY" && val(t, 5) == val(dim, 360)).unwrap().1;
    assert_eq!(val(dict, 330), val(dim, 5));
    assert_eq!(val(dict, 3), "ACAD_DIMASSOC");
    assert_eq!(val(dict, 360), val(assoc, 5));
    assert_eq!(val(assoc, 330), val(dict, 5));
    assert!(assoc.iter().any(|t| t.code == 330 && t.str() == val(dim, 5)));
    assert_eq!(val(assoc, 90), "3");
    let snaps: Vec<_> = assoc.iter().filter(|t| t.code == 72).map(|t| t.i64()).collect();
    assert_eq!(snaps, osnaps);
    let refs: Vec<_> = assoc.iter().filter(|t| t.code == 331).map(Tag::str).collect();
    assert_eq!(refs, vec![val(line, 5); 2]);
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
    let source = association_dxf(false, [13, 1])
        .replace("100\nAcDbDimension\n", "67\n1\n100\nAcDbDimension\n")
        .replace("90\n3\n70\n0\n71\n0\n", "90\n3\n70\n1\n71\n0\n");
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
