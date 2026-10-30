//! What a public DXF holds depends on its version: AutoCAD refuses (or misreads) groups of a newer one.
//! Each case is one AutoCAD LT 2025 refused or read wrongly when opening CADCraft's DXF of a blank drawing
//! and of a real one (Planeta Toulouse) at each version; the groups below are the ones its own DXFOUT
//! writes at that version.

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_io::{SaveOptions, read_dxf, write_with};

fn save(d: &Drawing, version: Version) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap()).unwrap()
}

fn dxf(entities: &str) -> String {
    let mut text = String::from("0\nSECTION\n2\nENTITIES\n");
    for line in entities.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        text.push_str(&format!("{code}\n{value}\n"));
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

fn section(text: &str, name: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags).into_iter().find(|s| s.name == name).map(|s| records(&s.tags)).unwrap_or_default()
}

/// Group 91 of a CLASS record (the instance count) is AutoCAD 2004's: AutoCAD 2000 expects group 280 after 90.
#[test]
fn class_instance_counts_exist_from_2004() {
    for version in Version::ALL {
        let text = save(&Drawing::new_metric(), version);
        let classes = section(&text, "CLASSES");
        assert!(!classes.is_empty());
        for (kind, tags) in classes.iter().filter(|(k, _)| k == "CLASS") {
            let has = tags.iter().any(|t| t.code == 91);
            assert_eq!(has, version >= Version::R2004, "{version:?}: {kind} {tags:?}");
        }
    }
    // Classes read from a file with counts do not bring them to a 2000 DXF either.
    let source = "0\nSECTION\n2\nCLASSES\n0\nCLASS\n1\nACME_WIDGET\n2\nAcmeWidget\n3\nAcme\n90\n0\n91\n7\n280\n0\n281\n0\n0\nENDSEC\n\
        0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nW\n350\n40\n0\nACME_WIDGET\n5\n40\n330\nC\n100\nAcmeWidget\n0\nENDSEC\n0\nEOF\n";
    let d = read_dxf(source.as_bytes()).unwrap();
    for version in [Version::R2000, Version::R2004] {
        let classes = section(&save(&d, version), "CLASSES");
        let widget = classes.iter().find(|(_, t)| t.iter().any(|t| t.code == 1 && t.str() == "ACME_WIDGET")).expect("the kept class");
        assert_eq!(widget.1.iter().any(|t| t.code == 91), version >= Version::R2004, "{version:?}");
    }
}

/// Gradient fills (450 to 470) are AutoCAD 2004's: a 2000 hatch ends at its seed points. The fill stays in CADCraft's xdata.
#[test]
fn gradient_groups_exist_from_2004() {
    let source = dxf(
        "0 HATCH\n5 50\n100 AcDbEntity\n8 0\n100 AcDbHatch\n10 0\n20 0\n30 0\n210 0\n220 0\n230 1\n2 SOLID\n70 1\n71 0\n91 1\n92 1\n93 1\n72 1\n10 0\n20 0\n11 5\n21 0\n97 0\n75 0\n76 1\n98 0\n450 1\n451 0\n460 0\n461 1\n452 0\n462 0\n453 2\n463 0\n63 5\n421 255\n463 1\n63 7\n421 16777215\n470 CYLINDER",
    );
    let d = read_dxf(source.as_bytes()).unwrap();
    for version in Version::ALL {
        let text = save(&d, version);
        let hatch = &section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "HATCH").unwrap().1;
        assert_eq!(hatch.iter().any(|t| t.code == 450), version >= Version::R2004, "{version:?}");
        let back = read_dxf(text.as_bytes()).unwrap();
        let name = |d: &Drawing| {
            d.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { h.gradient.as_ref().map(|g| g.name.clone()) } else { None })
        };
        assert_eq!(name(&back), Some("CYLINDER".into()), "{version:?}: the gradient is kept");
    }
}

/// Follow the boundary groups of a hatch as the DXF Reference lays them out and return how many groups they take, or `None`
/// at the first group out of place. Spline edges have a fit data count (97, then the fit points) from AutoCAD 2010.
fn boundary_len(groups: &[Tag], fit_counts: bool) -> Option<usize> {
    let code = |i: usize| groups.get(i).map(|t| t.code);
    let int = |i: usize| groups.get(i).map(|t| t.i64().max(0) as usize);
    let expect = |i: &mut usize, c: i32| (code(*i) == Some(c)).then(|| *i += 1);
    let mut i = 0;
    expect(&mut i, 91)?;
    for _ in 0..int(0)? {
        let flags = groups.get(i).filter(|t| t.code == 92)?.i64();
        i += 1;
        if flags & 2 != 0 {
            let has_bulge = groups.get(i).filter(|t| t.code == 72)?.i64() != 0;
            expect(&mut i, 72)?;
            expect(&mut i, 73)?;
            let n = int(i).filter(|_| code(i) == Some(93))?;
            i += 1;
            for _ in 0..n {
                expect(&mut i, 10)?;
                expect(&mut i, 20)?;
                if has_bulge {
                    expect(&mut i, 42)?;
                }
            }
        } else {
            let n = int(i).filter(|_| code(i) == Some(93))?;
            i += 1;
            for _ in 0..n {
                let kind = groups.get(i).filter(|t| t.code == 72)?.i64();
                i += 1;
                match kind {
                    1 => [10, 20, 11, 21].iter().try_for_each(|c| expect(&mut i, *c))?,
                    2 => [10, 20, 40, 50, 51, 73].iter().try_for_each(|c| expect(&mut i, *c))?,
                    3 => [10, 20, 11, 21, 40, 50, 51, 73].iter().try_for_each(|c| expect(&mut i, *c))?,
                    4 => {
                        let rational = groups.get(i + 1).filter(|t| t.code == 73)?.i64() != 0;
                        let (knots, controls) = (int(i + 3).filter(|_| code(i + 3) == Some(95))?, int(i + 4).filter(|_| code(i + 4) == Some(96))?);
                        [94, 73, 74, 95, 96].iter().try_for_each(|c| expect(&mut i, *c))?;
                        for _ in 0..knots {
                            expect(&mut i, 40)?;
                        }
                        for _ in 0..controls {
                            expect(&mut i, 10)?;
                            expect(&mut i, 20)?;
                            if rational {
                                expect(&mut i, 42)?;
                            }
                        }
                        if fit_counts {
                            let fit = int(i).filter(|_| code(i) == Some(97))?;
                            i += 1;
                            for _ in 0..fit {
                                expect(&mut i, 11)?;
                                expect(&mut i, 21)?;
                            }
                            if fit > 0 {
                                [12, 22, 13, 23].iter().try_for_each(|c| expect(&mut i, *c))?;
                            }
                        }
                    }
                    _ => return None,
                }
            }
        }
        let sources = int(i).filter(|_| code(i) == Some(97))?;
        i += 1;
        for _ in 0..sources {
            expect(&mut i, 330)?;
        }
    }
    Some(i)
}

/// A hatch whose loop has a spline edge between two lines, written as acadrust writes it for a file before 2010
/// (`fit_count: false`: no fit data count) or after it.
fn spline_hatch(fit_count: bool) -> String {
    let fit = if fit_count { "97 0\n" } else { "" };
    let spline = format!(
        "72 4\n94 3\n73 0\n74 0\n95 8\n96 4\n40 0\n40 0\n40 0\n40 0\n40 1\n40 1\n40 1\n40 1\n10 5\n20 0\n10 6\n20 2\n10 7\n20 2\n10 8\n20 0\n{fit}"
    );
    dxf(&format!(
        "0 HATCH\n5 51\n100 AcDbEntity\n8 0\n100 AcDbHatch\n10 0\n20 0\n30 2.5\n210 0\n220 0\n230 -1\n2 ANSI31\n70 0\n71 0\n91 1\n92 1\n93 3\n\
         72 1\n10 0\n20 0\n11 5\n21 0\n{spline}72 1\n10 8\n20 0\n11 0\n21 0\n97 0\n75 0\n76 1\n52 0\n41 1\n77 0\n78 0\n98 0"
    ))
}

#[test]
fn a_hatch_spline_edge_has_its_fit_count_from_2010_only() {
    for source in [spline_hatch(false), spline_hatch(true)] {
        let d = read_dxf(source.as_bytes()).unwrap();
        for version in Version::ALL {
            let text = save(&d, version);
            let hatch = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "HATCH").unwrap().1;
            let start = hatch.iter().position(|t| t.code == 91).unwrap();
            let end = hatch.iter().position(|t| t.code == 75).unwrap();
            let loops = boundary_len(&hatch[start..], version >= Version::R2010);
            assert_eq!(loops.map(|n| start + n), Some(end), "{version:?}: {:?}", &hatch[start..end]);
            // The spline edge itself is there (a replay of the source, not a polygon).
            assert!(hatch.iter().any(|t| t.code == 72 && t.i64() == 4), "{version:?}");
        }
    }
}

/// A hatch of AutoCAD 2010 whose spline edge has fit points, and the pixel size (47) AutoCAD wrote after the pattern.
fn fit_data_hatch() -> String {
    let spline = "72 4\n94 3\n73 0\n74 0\n95 8\n96 4\n40 0\n40 0\n40 0\n40 0\n40 1\n40 1\n40 1\n40 1\n10 5\n20 0\n10 6\n20 2\n10 7\n20 2\n10 8\n20 0\n\
                  97 2\n11 5\n21 0\n11 8\n21 0\n12 0\n22 1\n13 0\n23 -1\n";
    dxf(&format!(
        "0 HATCH\n5 51\n100 AcDbEntity\n8 0\n100 AcDbHatch\n10 0\n20 0\n30 2.5\n210 0\n220 0\n230 -1\n2 ANSI31\n70 0\n71 0\n91 1\n92 1\n93 3\n\
         72 1\n10 0\n20 0\n11 5\n21 0\n{spline}72 1\n10 8\n20 0\n11 0\n21 0\n97 0\n75 0\n76 1\n52 0\n41 1\n77 0\n78 1\n53 45\n43 0\n44 0\n45 1\n46 1\n79 0\n47 0.5\n98 0"
    ))
}

/// AutoCAD refuses a pattern hatch of a DXF before 2010 that lost its pixel size (47) because its spline edge had fit points:
/// the hatch is kept as the file had it, without the fit data a file of that version cannot hold.
#[test]
fn a_hatch_with_spline_fit_points_keeps_its_pixel_size_before_2010() {
    let d = read_dxf(fit_data_hatch().as_bytes()).unwrap();
    for version in Version::ALL {
        let text = save(&d, version);
        let hatch = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "HATCH").unwrap().1;
        let start = hatch.iter().position(|t| t.code == 91).unwrap();
        let end = hatch.iter().position(|t| t.code == 75).unwrap();
        let loops = boundary_len(&hatch[start..], version >= Version::R2010);
        assert_eq!(loops.map(|n| start + n), Some(end), "{version:?}: {:?}", &hatch[start..end]);
        let pixel = hatch.iter().position(|t| t.code == 47).unwrap_or_else(|| panic!("{version:?}: no pixel size {hatch:?}"));
        assert_eq!(hatch.get(pixel + 1).map(|t| t.code), Some(98), "{version:?}: the pixel size comes before the seed points");
        assert_eq!(hatch.iter().any(|t| t.code == 12), version >= Version::R2010, "{version:?}: the fit data stays from 2010 on");
    }
}

/// AutoCAD writes no oblique angle (52) for extension lines perpendicular to the dimension line and refuses a
/// DXF that has one, also when the source file had it (a DWG's -270 degrees).
#[test]
fn a_perpendicular_oblique_of_a_source_dimension_is_not_replayed() {
    let source = dxf(
        "0 DIMENSION\n5 52\n100 AcDbEntity\n8 0\n100 AcDbDimension\n2 *D1\n10 5.5\n20 6.5\n30 0\n11 3.5\n21 4.5\n31 0\n70 32\n53 0\n3 Standard\n\
         100 AcDbAlignedDimension\n13 0.5\n23 1.5\n33 0\n14 8.5\n24 1.5\n34 0\n50 180\n52 -270\n100 AcDbRotatedDimension",
    );
    let d = read_dxf(source.as_bytes()).unwrap();
    for version in [Version::R2000, Version::R2018] {
        let dim = section(&save(&d, version), "ENTITIES").into_iter().find(|(k, _)| k == "DIMENSION").unwrap().1;
        assert!(!dim.iter().any(|t| t.code == 52), "{version:?}: {dim:?}");
    }
}

/// A DXF of AutoCAD 2010 (Citycon) with what AutoCAD refused in a CADCraft DXF of another version: a maintenance
/// version in the format's own group code, and a dynamic block node with an empty marker at its end.
fn objects_source() -> String {
    let lines = [
        "0 SECTION\n2 HEADER\n9 $ACADVER\n1 AC1024\n9 $ACADMAINTVER\n70 0\n0 ENDSEC",
        "0 SECTION\n2 OBJECTS",
        "0 DICTIONARY\n5 C\n330 0\n100 AcDbDictionary\n281 1\n3 ACME_NODE\n350 45",
        "0 ACDB_DYNAMICBLOCKPROXYNODE\n5 45\n330 C\n100 AcDbEvalExpr\n90 1\n98 33\n99 328\n100 AcDbDynamicBlockProxyNode",
        "0 ENDSEC\n0 EOF\n",
    ];
    lines.iter().flat_map(|l| l.lines()).map(|l| l.split_once(' ').map_or(format!("{l}\n"), |(c, v)| format!("{c}\n{v}\n"))).collect()
}

/// AutoCAD ends a maintenance version of another group code, and a node whose subclass marker is empty, early.
#[test]
fn the_maintenance_version_and_an_empty_node_marker_are_not_replayed() {
    let d = read_dxf(objects_source().as_bytes()).unwrap();
    for version in Version::ALL {
        let text = save(&d, version);
        let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
        let header = sections(&tags).into_iter().find(|s| s.name == "HEADER").unwrap();
        assert!(!header.tags.iter().any(|t| t.code == 9 && t.str() == "$ACADMAINTVER"), "{version:?}");
        let node = section(&text, "OBJECTS").into_iter().find(|(k, _)| k == "ACDB_DYNAMICBLOCKPROXYNODE").expect("the node is kept").1;
        assert!(!node.iter().any(|t| t.code == 100 && t.str() == "AcDbDynamicBlockProxyNode"), "{version:?}: {node:?}");
        assert!(node.iter().any(|t| t.code == 99 && t.i64() == 328), "{version:?}: its data stays");
    }
}

/// The block record's insertion units (70), explodability (280) and scalability (281) are AutoCAD 2007's, and
/// AutoCAD writes all three on every record, also on the model space. A save keeps the scalability.
#[test]
fn a_block_record_has_units_explodability_and_scalability_from_2007() {
    let mut d = Drawing::new_metric();
    let mut b = cadcraft_doc::Block::new("Door");
    (b.units, b.explodable, b.scale_uniformly) = (4, false, true);
    d.blocks.insert("Door".into(), std::sync::Arc::new(b));
    let groups = |text: &str| -> Vec<(String, Vec<i32>)> {
        let tables = sections(&cadcraft_dxf::parse(text.as_bytes()).unwrap()).into_iter().find(|s| s.name == "TABLES").unwrap();
        records(&tables.tags)
            .into_iter()
            .filter(|(k, _)| k == "BLOCK_RECORD")
            .map(|(_, tags)| {
                let name = tags.iter().find(|t| t.code == 2).map(|t| t.str().to_string()).unwrap_or_default();
                (name, tags.iter().map(|t| t.code).filter(|c| [70, 280, 281].contains(c)).collect())
            })
            .collect()
    };
    for version in Version::ALL {
        let records = groups(&save(&d, version));
        assert!(records.len() >= 3, "{version:?}: {records:?}");
        for (name, codes) in records {
            let expected: &[i32] = if version >= Version::R2007 { &[70, 280, 281] } else { &[] };
            assert_eq!(codes, expected, "{version:?}: {name}");
        }
    }
    let door = |d: &Drawing| d.block("Door").map(|b| (b.units, b.explodable, b.scale_uniformly));
    assert_eq!(door(&read_dxf(save(&d, Version::R2018).as_bytes()).unwrap()), Some((4, false, true)), "the user DXF");
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    assert_eq!(door(&cadcraft_io::read(&dwg, "x.dwg").unwrap()), Some((4, false, true)), "the DWG bridge");
}

/// AutoCAD keeps a preview image (BLOCK_RECORD group 310) with each block and makes a new one after an edit.
/// A save writes the preview it read while the block is as it was read, in a DXF and in a DWG.
#[test]
fn a_block_keeps_its_preview_until_it_is_edited() {
    let preview = "424D360000000000000036000000280000000100000001000000010018000000000004000000000000000000000000000000000000";
    let mut b = cadcraft_doc::Block::new("Door");
    let line = |x: f64| {
        cadcraft_doc::Entity::new(
            cadcraft_doc::Handle(0x500),
            EntityKind::Line(cadcraft_doc::Line { a: cadcraft_geom::Vec3::new(0.0, 0.0, 0.0), b: cadcraft_geom::Vec3::new(x, 1.0, 0.0) }),
        )
    };
    b.entities.push(line(5.0));
    b.preview = Some(cadcraft_doc::BlockPreview { groups: vec![preview.into()], key: b.content_key() });
    let mut d = Drawing::new_metric();
    d.bump_handseed(cadcraft_doc::Handle(0x510));
    d.blocks.insert("Door".into(), std::sync::Arc::new(b));
    let previews = |d: &Drawing| d.block("Door").and_then(|b| b.preview.as_ref().map(|p| p.groups.concat().to_ascii_uppercase()));
    let has_310 = |text: &str| text.lines().any(|l| l.trim().eq_ignore_ascii_case(preview));
    for version in Version::ALL {
        assert!(has_310(&save(&d, version)), "{version:?}: the preview is written");
    }
    let back = read_dxf(save(&d, Version::R2018).as_bytes()).unwrap();
    assert_eq!(previews(&back), Some(preview.to_ascii_uppercase()), "read again");
    let dwg = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(previews(&dwg), Some(preview.to_ascii_uppercase()), "the DWG bridge");
    // An edit of the block's contents, a property edit included, drops the preview.
    let mut edited = d.clone();
    let mut b = (*edited.blocks["Door"]).clone();
    let mut moved = (**b.entities.iter().next().unwrap()).clone();
    moved.common.layer = "Other".into();
    b.entities = cadcraft_doc::EntityStore::new();
    b.entities.push(moved);
    edited.blocks.insert("Door".into(), std::sync::Arc::new(b));
    assert!(!has_310(&save(&edited, Version::R2018)), "an edited block has no old preview");
}

/// A VISUALSTYLE of AutoCAD 2010 (the style "2dWireframe" as its DXFOUT writes it): the form depends on the
/// release (below 2010, 2010 to 2012, 2013 and later), and AutoCAD LT 2025 ends a short form early in a 2013 or
/// later file. A save writes the record as it was while the release has the same form, and in the form of the
/// release otherwise.
#[test]
fn a_visual_style_is_written_in_the_form_of_the_release() {
    let style = "0 VISUALSTYLE\n5 42\n330 41\n100 AcDbVisualStyle\n2 2dWireframe\n70 4\n177 2\n291 0\n71 0\n176 1\n72 2\n176 1\n73 0\n176 1\n90 0\n176 1\n\
        40 0.6\n176 1\n41 30.0\n176 1\n63 7\n421 16777215\n176 1\n74 1\n176 1\n91 4\n176 1\n64 7\n176 1\n65 257\n176 1\n75 1\n176 1\n175 1\n176 1\n\
        42 1.0\n176 1\n92 0\n176 1\n66 257\n176 1\n43 1.0\n176 1\n76 1\n176 1\n77 6\n176 1\n78 2\n176 1\n67 7\n176 1\n79 5\n176 1\n\
        170 0\n176 1\n171 0\n176 1\n290 0\n176 1\n93 1\n176 1\n44 0.0\n176 1\n173 0\n176 1";
    let source = [
        "0 SECTION\n2 HEADER\n9 $ACADVER\n1 AC1024\n0 ENDSEC",
        "0 SECTION\n2 OBJECTS",
        "0 DICTIONARY\n5 C\n330 0\n100 AcDbDictionary\n281 1\n3 ACAD_VISUALSTYLE\n350 41",
        "0 DICTIONARY\n5 41\n330 C\n100 AcDbDictionary\n281 1\n3 2dWireframe\n350 42",
        style,
        "0 ENDSEC\n0 EOF\n",
    ]
    .iter()
    .flat_map(|l| l.lines())
    .map(|l| l.split_once(' ').map_or(format!("{l}\n"), |(c, v)| format!("{c}\n{v}\n")))
    .collect::<String>();
    let d = read_dxf(source.as_bytes()).unwrap();
    let style_of =
        |version: Version| section(&save(&d, version), "OBJECTS").into_iter().find(|(k, _)| k == "VISUALSTYLE").expect("the style is kept").1;
    let short = style_of(Version::R2010);
    assert!(short.iter().any(|t| t.code == 71) && short.iter().any(|t| t.code == 421), "unchanged for 2010: {short:?}");
    let count = |t: &[Tag], code: i32| t.iter().filter(|t| t.code == code).count();
    assert_eq!(count(&short, 176), 28);
    for version in [Version::R2013, Version::R2018] {
        let long = style_of(version);
        assert!(!long.iter().any(|t| t.code == 71), "{version:?}: no short form property codes");
        assert!(long.len() > short.len() * 3 / 2, "{version:?}: {} groups against {}", long.len(), short.len());
        assert_eq!(long.iter().find(|t| t.code == 2).map(|t| t.str()).as_deref(), Some("2dWireframe"));
    }
    for version in [Version::R2000, Version::R2004] {
        let old = style_of(version);
        assert!(old.iter().any(|t| t.code == 71), "{version:?}: the older form keeps its property codes");
        assert!(!old.iter().any(|t| t.code == 177), "{version:?}: and has no lighting model");
    }
}

/// Before 2007 a block's insertion units are not group 70 but the ACAD xdata `DesignCenter Data { 1, units }` of its
/// block record, which AutoCAD writes for a block that has units. A save writes it once, and a file with it opens
/// with the units, also when it is saved again.
#[test]
fn block_units_before_2007_are_design_center_xdata() {
    let mut d = Drawing::new_metric();
    for (name, units) in [("Door", 4), ("Plain", 0)] {
        let mut b = cadcraft_doc::Block::new(name);
        b.units = units;
        d.blocks.insert(name.into(), std::sync::Arc::new(b));
    }
    let xdata_of = |text: &str, name: &str| -> Vec<(i32, String)> {
        let tables = sections(&cadcraft_dxf::parse(text.as_bytes()).unwrap()).into_iter().find(|s| s.name == "TABLES").unwrap();
        let record =
            records(&tables.tags).into_iter().find(|(k, t)| k == "BLOCK_RECORD" && t.iter().any(|t| t.code == 2 && t.str() == name)).unwrap().1;
        let at = record.iter().position(|t| t.code == 1001).unwrap_or(record.len());
        record[at..].iter().map(|t| (t.code, t.str())).collect()
    };
    let expected: Vec<(i32, String)> = [(1001, "ACAD"), (1000, "DesignCenter Data"), (1002, "{"), (1070, "1"), (1070, "4"), (1002, "}")]
        .iter()
        .map(|(c, v)| (*c, v.to_string()))
        .collect();
    for version in [Version::R2000, Version::R2004] {
        let text = save(&d, version);
        assert_eq!(xdata_of(&text, "Door"), expected, "{version:?}");
        assert!(xdata_of(&text, "Plain").is_empty(), "{version:?}: no units, no xdata");
        let back = read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.block("Door").map(|b| b.units), Some(4), "{version:?}: read back");
        // Saved again, as the same version or a later one, the units are written once.
        assert_eq!(xdata_of(&save(&back, version), "Door"), expected, "{version:?}: second save");
        let later = save(&back, Version::R2018);
        assert!(xdata_of(&later, "Door").is_empty(), "{version:?}: 2018 has group 70 instead");
        assert_eq!(read_dxf(later.as_bytes()).unwrap().block("Door").map(|b| b.units), Some(4));
    }
}

/// MULTILEADER remains native at every supported DXF version.
#[test]
fn a_multileader_remains_one_native_record_at_every_target() {
    let mleader = cadcraft_doc::MLeader {
        roots: vec![cadcraft_doc::MLeaderRoot::new(
            cadcraft_geom::Vec3::new(5.0, 4.0, 0.0),
            0.75,
            false,
            vec![
                vec![cadcraft_geom::Vec3::new(0.0, 0.0, 0.0), cadcraft_geom::Vec3::new(2.0, 3.0, 0.0)],
                vec![cadcraft_geom::Vec3::new(9.0, 0.0, 0.0), cadcraft_geom::Vec3::new(7.0, 3.0, 0.0)],
            ],
        )],
        text: Some(cadcraft_doc::MText {
            insert: cadcraft_geom::Vec3::new(5.9, 4.0, 0.0),
            height: 0.4,
            width: 0.0,
            attach: 4,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "NOTE".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: None,
            drawing_direction: None,
        }),
        style: "Standard".into(),
        arrow_size: 0.3,
        spline: false,
        block: None,
        arrow_block: String::new(),
        context: cadcraft_doc::MLeaderContext::default(),
    };
    let mut d = Drawing::new_metric();
    d.model.push(cadcraft_doc::Entity::new(cadcraft_doc::Handle(0x80), EntityKind::MLeader(mleader)));
    d.bump_handseed(cadcraft_doc::Handle(0x90));
    for version in Version::ALL {
        let kinds: Vec<String> = section(&save(&d, version), "ENTITIES").into_iter().map(|(k, _)| k).filter(|k| k != "ENDSEC").collect();
        let count = |k: &str| kinds.iter().filter(|x| *x == k).count();
        assert_eq!((count("MULTILEADER"), count("LEADER"), count("MTEXT")), (1, 0, 0), "{version:?}: {kinds:?}");
    }
}
