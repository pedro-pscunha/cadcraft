//! Saving is not PURGE: unused definitions remain available over DXF and DWG saves.

use std::sync::Arc;

use cadcraft_doc::{Block, Drawing, Entity, EntityKind, Line, MLineStyle};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

fn objects(text: &[u8]) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text).unwrap();
    records(&sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap().tags)
}

fn value(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

fn source_images() -> String {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n0\nENDSEC\n\
     0\nSECTION\n2\nOBJECTS\n\
     0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n3\nACAD_IMAGE_DICT\n350\n40\n3\nACAD_IMAGE_VARS\n350\n43\n\
     0\nDICTIONARY\n5\n40\n330\nC\n100\nAcDbDictionary\n3\nUnusedLogo\n350\n41\n\
     0\nIMAGEDEF\n5\n41\n330\n40\n100\nAcDbRasterImageDef\n90\n0\n1\nimages/logo.jpg\n10\n640\n20\n480\n11\n0.25\n21\n0.5\n280\n0\n281\n2\n\
     0\nRASTERVARIABLES\n5\n43\n330\nC\n100\nAcDbRasterVariables\n90\n0\n70\n0\n71\n0\n72\n2\n\
     0\nENDSEC\n0\nEOF\n"
        .into()
}

fn check_images(text: &[u8], units: f64) {
    let objs = objects(text);
    let defs: Vec<_> = objs.iter().filter(|(k, _)| k == "IMAGEDEF").collect();
    assert_eq!(defs.len(), 1, "an unused IMAGEDEF is not purged");
    let def = &defs[0].1;
    assert_eq!(value(def, 1), "images/logo.jpg");
    for (c, want) in [(10, 640.0), (20, 480.0), (11, 0.25), (21, 0.5), (280, 0.0), (281, units)] {
        assert_eq!(def.iter().find(|t| t.code == c).unwrap().f64(), want, "IMAGEDEF group {c}");
    }
    assert_eq!(objs.iter().filter(|(k, _)| k == "IMAGEDEF_REACTOR").count(), 0);
    assert!(
        objs.iter()
            .any(|(k, t)| k == "DICTIONARY" && t.windows(2).any(|p| p[0].code == 3 && p[0].str() == "UnusedLogo" && p[1].str() == value(def, 5)))
    );
    let vars: Vec<_> = objs.iter().filter(|(k, _)| k == "RASTERVARIABLES").collect();
    assert_eq!(vars.len(), 1);
    for (c, want) in [(70, "0"), (71, "0"), (72, "2")] {
        assert_eq!(value(&vars[0].1, c), want);
    }
}

#[test]
fn unused_image_definitions_survive_dxf_saves() {
    let mut d = cadcraft_io::read_dxf(source_images().as_bytes()).unwrap();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        check_images(text.as_bytes(), 2.0);
        d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    }
}

#[test]
fn unused_image_definitions_survive_dwg_saves() {
    let mut d = cadcraft_io::read_dxf(source_images().as_bytes()).unwrap();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        let back = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
        check_images(&back, 2.0);
        d = cadcraft_io::read_dxf(&back).unwrap();
    }
}

#[test]
fn unused_dimension_blocks_survive_dxf_and_dwg_saves() {
    let mut d = Drawing::new_metric();
    let mut b = Block::new("*D7");
    b.entities.push(Entity::new(d.new_handle(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) })));
    d.blocks.insert(b.name.clone(), Arc::new(b));
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        let blocks: Vec<_> = back.blocks.values().filter(|b| b.name.starts_with("*D")).collect();
        assert_eq!(blocks.len(), 1, "unused dimension block survives DXF");
        assert_eq!(blocks[0].entities.len(), 1);
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        d = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
        let blocks: Vec<_> = d.blocks.values().filter(|b| b.name.starts_with("*D")).collect();
        assert_eq!(blocks.len(), 1, "unused dimension block survives DWG: {:?}", d.blocks.keys().collect::<Vec<_>>());
        assert_eq!(blocks[0].entities.len(), 1);
    }
}

#[test]
fn unused_mline_styles_survive_dxf_and_dwg_saves() {
    let mut d = Drawing::new_metric();
    d.mline_styles.push(MLineStyle { name: "UnusedDouble".into(), description: "spare style".into(), ..MLineStyle::default() });
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert!(back.mline_styles.iter().any(|s| s.name == "UnusedDouble"));
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        d = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
        assert!(d.mline_styles.iter().any(|s| s.name == "UnusedDouble"));
        assert_eq!(d.mline_styles.len(), 2);
    }
}
