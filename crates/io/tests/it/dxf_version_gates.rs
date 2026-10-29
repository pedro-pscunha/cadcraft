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
