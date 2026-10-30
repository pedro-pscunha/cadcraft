//! Canonical text (revision CanonicalV2) is written as it is: OCS points and baseline angle for TEXT, WCS insertion and
//! full direction for MTEXT, the plane's normal with its source magnitude, the generation flags and the thickness.
//! Each field changed alone reaches the user DXF and the DWG. The readers of the file still read legacy text: no
//! canonical import is active.

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, GeometryRevision, HAlign, MText, Text, VAlign};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

use acadrust::DwgReader;
use acadrust::entities::EntityType;

fn text() -> Text {
    Text {
        insert: Vec3::new(1.0, 2.0, 3.0),
        align_pt: None,
        height: 2.5,
        value: "Hello".into(),
        rotation: 0.3,
        width_factor: 0.9,
        oblique: 0.1,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        backward: false,
        upside_down: false,
    }
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(1.0, 2.0, 3.0),
        height: 0.5,
        width: 4.0,
        attach: 5,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Body".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(0.0, 0.6, 0.8)),
        drawing_direction: None,
    }
}

const NORMAL: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 2.0 };

fn drawing(kind: EntityKind, normal: Vec3, thickness: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    let handle = d.new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, thickness, ..Common::default() };
    d.model.push(e);
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(
        cadcraft_io::write_with(d, "x.dxf", &cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false }).unwrap(),
    )
    .unwrap()
}

/// The groups of the first record of `kind` in the ENTITIES section.
fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let record: Vec<Tag> = records(&section.tags).into_iter().find(|(k, _)| k == kind).unwrap_or_else(|| panic!("no {kind}")).1;
    record.iter().map(|t| (t.code, t.str())).collect()
}

fn num(g: &[(i32, String)], code: i32) -> Option<f64> {
    g.iter().find(|t| t.0 == code).map(|t| t.1.trim().parse().unwrap())
}

fn point(g: &[(i32, String)], code: i32) -> Option<Vec3> {
    Some(Vec3::new(num(g, code)?, num(g, code + 10)?, num(g, code + 20)?))
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn dwg_text(d: &Drawing) -> acadrust::entities::Text {
    let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap();
    stored.entities().find_map(|e| if let EntityType::Text(t) = e { Some(t.clone()) } else { None }).expect("a TEXT in the DWG")
}

fn dwg_mtext(d: &Drawing) -> acadrust::entities::MText {
    let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap();
    stored.entities().find_map(|e| if let EntityType::MText(t) = e { Some((**t).clone()) } else { None }).expect("an MTEXT in the DWG")
}

#[test]
fn a_canonical_text_is_written_in_its_own_coordinates_with_its_normal_and_flags() {
    let mut t = text();
    t.backward = true;
    t.upside_down = true;
    let g = groups(&dxf(&drawing(EntityKind::Text(t), Vec3::new(0.0, 3.0, 4.0), -0.5)), "TEXT");
    assert!(near(point(&g, 10).unwrap(), Vec3::new(1.0, 2.0, 3.0)), "OCS point, not projected: {g:?}");
    assert!(near(point(&g, 210).unwrap(), Vec3::new(0.0, 3.0, 4.0)), "the normal keeps its magnitude");
    assert_eq!((num(&g, 39), num(&g, 71)), (Some(-0.5), Some(6.0)));
    assert!((num(&g, 50).unwrap() - 0.3f64.to_degrees()).abs() < 1e-9);
}

#[test]
fn each_text_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Text, &mut Vec3, &mut f64);
    let cases: Vec<(&str, Mutation)> = vec![
        ("insert x", |t, _, _| t.insert.x = 1.5),
        ("insert y", |t, _, _| t.insert.y = 2.5),
        ("insert z (elevation)", |t, _, _| t.insert.z = 3.5),
        ("height", |t, _, _| t.height = 2.75),
        ("baseline angle", |t, _, _| t.rotation = 0.5),
        ("width factor", |t, _, _| t.width_factor = 0.8),
        ("oblique", |t, _, _| t.oblique = 0.2),
        ("backward", |t, _, _| t.backward = true),
        ("upside down", |t, _, _| t.upside_down = true),
        ("normal x", |_, n, _| n.x = 0.25),
        ("normal y", |_, n, _| n.y = 0.5),
        ("normal z", |_, n, _| n.z = 2.5),
        ("thickness", |_, _, k| *k = 1.25),
    ];
    let make = |mutate: Mutation| {
        let (mut t, mut n, mut k) = (text(), NORMAL, 0.0);
        mutate(&mut t, &mut n, &mut k);
        (drawing(EntityKind::Text(t.clone()), n, k), t, n, k)
    };
    let base_dxf = groups(&dxf(&make(|_, _, _| {}).0), "TEXT");
    for (name, mutate) in cases {
        let (d, t, n, k) = make(mutate);
        let g = groups(&dxf(&d), "TEXT");
        assert_ne!(g, base_dxf, "{name}: the DXF changes");
        // The DXF carries the changed value exactly.
        assert!(near(point(&g, 10).unwrap(), t.insert), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n), "{name}");
        assert_eq!(num(&g, 39).unwrap_or(0.0), k, "{name}");
        assert!((num(&g, 40).unwrap() - t.height).abs() < 1e-12, "{name}");
        assert!((num(&g, 50).unwrap_or(0.0) - t.rotation.to_degrees()).abs() < 1e-9, "{name}");
        assert!((num(&g, 41).unwrap_or(1.0) - t.width_factor).abs() < 1e-12, "{name}");
        assert!((num(&g, 51).unwrap_or(0.0) - t.oblique.to_degrees()).abs() < 1e-9, "{name}");
        assert_eq!(num(&g, 71).unwrap_or(0.0), f64::from(u8::from(t.backward) * 2 + u8::from(t.upside_down) * 4), "{name}");
        // The DWG gets the same.
        let s = dwg_text(&d);
        assert!(
            near(Vec3::new(s.insertion_point.x, s.insertion_point.y, s.insertion_point.z), t.insert),
            "{name}: DWG insertion {:?}",
            s.insertion_point
        );
        assert!(near(Vec3::new(s.normal.x, s.normal.y, s.normal.z), n), "{name}: DWG normal {:?}", s.normal);
        assert_eq!(s.thickness, k, "{name}");
        assert!((s.height - t.height).abs() < 1e-12 && (s.rotation - t.rotation).abs() < 1e-9, "{name}");
        assert_eq!(s.generation_flags, i16::from(t.backward) * 2 + i16::from(t.upside_down) * 4, "{name}");
    }
}

#[test]
fn a_canonical_mtext_writes_its_plane_and_full_direction() {
    let g = groups(&dxf(&drawing(EntityKind::MText(mtext()), Vec3::new(0.0, 3.0, 4.0), 0.0)), "MTEXT");
    assert!(near(point(&g, 10).unwrap(), Vec3::new(1.0, 2.0, 3.0)));
    assert!(near(point(&g, 11).unwrap(), Vec3::new(0.0, 0.6, 0.8)), "the full direction with its Z: {g:?}");
    assert!(near(point(&g, 210).unwrap(), Vec3::new(0.0, 3.0, 4.0)));
    assert!(num(&g, 50).is_none(), "no rotation angle beside the direction");
}

#[test]
fn each_mtext_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut MText, &mut Vec3);
    let cases: Vec<(&str, Mutation)> = vec![
        ("insert x", |m, _| m.insert.x = 1.5),
        ("insert z", |m, _| m.insert.z = 3.5),
        ("direction x", |m, _| m.direction = Some(Vec3::new(0.2, 0.6, 0.8))),
        ("direction y", |m, _| m.direction = Some(Vec3::new(0.0, 0.8, 0.6))),
        ("direction z", |m, _| m.direction = Some(Vec3::new(0.0, 0.6, -0.8))),
        ("height", |m, _| m.height = 0.75),
        ("width", |m, _| m.width = 5.0),
        ("normal x", |_, n| n.x = 0.25),
        ("normal y", |_, n| n.y = 0.5),
        ("normal z", |_, n| n.z = 2.5),
    ];
    let make = |mutate: Mutation| {
        let (mut m, mut n) = (mtext(), NORMAL);
        mutate(&mut m, &mut n);
        (drawing(EntityKind::MText(m.clone()), n, 0.0), m, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "MTEXT");
    for (name, mutate) in cases {
        let (d, m, n) = make(mutate);
        let g = groups(&dxf(&d), "MTEXT");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), m.insert), "{name}");
        assert!(near(point(&g, 11).unwrap(), m.direction.unwrap()), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n), "{name}");
        assert!((num(&g, 40).unwrap() - m.height).abs() < 1e-12 && (num(&g, 41).unwrap() - m.width).abs() < 1e-12, "{name}");
        let s = dwg_mtext(&d);
        assert!(near(Vec3::new(s.insertion_point.x, s.insertion_point.y, s.insertion_point.z), m.insert), "{name}");
        assert!(near(Vec3::new(s.normal.x, s.normal.y, s.normal.z), n), "{name}: DWG normal {:?}", s.normal);
        let dir = s.dwg_x_direction.unwrap_or_default();
        assert!(near(Vec3::new(dir.x, dir.y, dir.z), m.direction.unwrap()), "{name}: DWG direction {dir:?}");
        assert!((s.height - m.height).abs() < 1e-12, "{name}");
    }
}

#[test]
fn the_readers_still_read_legacy_text_whatever_the_file_holds() {
    // A negative normal still gets the legacy mirrored reading: the canonical reader is not wired in.
    let source = "0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n5\n20\n0\n30\n0\n40\n1\n1\nA\n50\n0\n210\n0\n220\n0\n230\n-1\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n";
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let e = d.model.iter().next().unwrap();
    assert_eq!(e.geometry_revision, GeometryRevision::LegacyProjectedV1);
    let EntityKind::Text(t) = &e.kind else { panic!("not text") };
    assert!(t.halign == HAlign::Right, "the legacy reader mirrors text under (0, 0, -1)");
}

fn attrib(plane: Vec3, thickness: f64) -> cadcraft_doc::Attrib {
    let mut a = cadcraft_doc::Attrib::new("TAG", text());
    a.geometry_revision = GeometryRevision::CanonicalV2;
    a.plane = cadcraft_doc::AttribPlane { normal: plane, thickness };
    a
}

fn insert_with(a: cadcraft_doc::Attrib, normal: Vec3) -> Drawing {
    let ins = cadcraft_doc::Insert {
        block: "TAG".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![a],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let mut d = Drawing::new_metric();
    let mut b = cadcraft_doc::Block::new("TAG");
    b.entities.push(Entity::new(d.new_handle(), EntityKind::AttDef(cadcraft_doc::Attrib::new("TAG", text()))));
    d.blocks.insert("TAG".into(), std::sync::Arc::new(b));
    let handle = d.new_handle();
    let mut e = Entity::new(handle, EntityKind::Insert(ins));
    e.common.extrusion = normal;
    d.model.push(e);
    d
}

fn dwg_insert_attrib(d: &Drawing) -> acadrust::entities::AttributeEntity {
    let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap();
    stored
        .entities()
        .find_map(|e| if let EntityType::Insert(i) = e { i.attributes.first().cloned() } else { None })
        .expect("an INSERT with an attribute in the DWG")
}

#[test]
fn each_attrib_plane_field_alone_reaches_the_dxf_and_the_dwg_apart_from_the_inserts_normal() {
    type Mutation = fn(&mut cadcraft_doc::Attrib);
    let cases: Vec<(&str, Mutation)> = vec![
        ("plane x", |a| a.plane.normal.x = 0.25),
        ("plane y", |a| a.plane.normal.y = 0.5),
        ("plane z", |a| a.plane.normal.z = 2.5),
        ("thickness", |a| a.plane.thickness = 1.25),
        ("insert z", |a| a.text.insert.z = 3.5),
        ("rotation", |a| a.text.rotation = 0.5),
        ("backward", |a| a.text.backward = true),
        ("upside down", |a| a.text.upside_down = true),
    ];
    let insert_normal = Vec3::new(1.0, 0.0, 0.0);
    let make = |mutate: Mutation| {
        let mut a = attrib(Vec3::new(0.0, 3.0, 4.0), 0.5);
        mutate(&mut a);
        (insert_with(a.clone(), insert_normal), a)
    };
    let base = groups(&dxf(&make(|_| {}).0), "ATTRIB");
    for (name, mutate) in cases {
        let (d, a) = make(mutate);
        let g = groups(&dxf(&d), "ATTRIB");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), a.text.insert), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), a.plane.normal), "{name}: the attribute keeps its own normal, not the insert's");
        assert_eq!(num(&g, 39).unwrap_or(0.0), a.plane.thickness, "{name}");
        assert!((num(&g, 50).unwrap_or(0.0) - a.text.rotation.to_degrees()).abs() < 1e-9, "{name}");
        assert_eq!(num(&g, 71).unwrap_or(0.0), f64::from(u8::from(a.text.backward) * 2 + u8::from(a.text.upside_down) * 4), "{name}");
        let s = dwg_insert_attrib(&d);
        assert!(near(Vec3::new(s.normal.x, s.normal.y, s.normal.z), a.plane.normal), "{name}: DWG normal {:?}", s.normal);
        assert_eq!(s.thickness, a.plane.thickness, "{name}");
        assert!((s.rotation - a.text.rotation).abs() < 1e-9, "{name}");
    }
}

#[test]
fn a_canonical_attdef_writes_its_plane_flags_and_thickness() {
    let mut a = cadcraft_doc::Attrib::new("TAG", text());
    a.text.backward = true;
    let g = groups(&dxf(&drawing(EntityKind::AttDef(a), Vec3::new(0.0, 3.0, 4.0), -0.5)), "ATTDEF");
    assert!(near(point(&g, 10).unwrap(), Vec3::new(1.0, 2.0, 3.0)), "{g:?}");
    assert!(near(point(&g, 210).unwrap(), Vec3::new(0.0, 3.0, 4.0)));
    assert_eq!((num(&g, 39), num(&g, 71)), (Some(-0.5), Some(2.0)));
}

#[test]
fn each_embedded_mtext_field_alone_reaches_the_attributes_dxf() {
    type Mutation = fn(&mut cadcraft_doc::MultilineAttrib);
    let cases: Vec<(&str, Mutation)> = vec![
        ("direction x", |m| m.mtext.direction = Some(Vec3::new(0.2, 0.6, 0.8))),
        ("direction z", |m| m.mtext.direction = Some(Vec3::new(0.0, 0.6, -0.8))),
        ("insert z", |m| m.mtext.insert.z = 4.5),
        ("normal x", |m| m.normal.x = 0.25),
        ("normal z", |m| m.normal.z = 2.5),
    ];
    let make = |mutate: Mutation| {
        let mut a = attrib(Vec3::Z, 0.0);
        let mut ml = cadcraft_doc::MultilineAttrib {
            geometry_revision: GeometryRevision::CanonicalV2,
            normal: Vec3::new(0.0, 3.0, 4.0),
            mtext: mtext(),
            read_as: "Body".into(),
            embedded_46: None,
            type_72: None,
        };
        mutate(&mut ml);
        a.text.value = "Body".into();
        a.multiline = Some(ml.clone());
        (insert_with(a, Vec3::Z), ml)
    };
    let embedded = |d: &Drawing| -> Vec<(i32, String)> {
        let g = groups(&dxf(d), "ATTRIB");
        let at = g.iter().position(|t| t.0 == 101).expect("an embedded object");
        g[at..].to_vec()
    };
    let base = embedded(&make(|_| {}).0);
    for (name, mutate) in cases {
        let (d, ml) = make(mutate);
        let g = embedded(&d);
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), ml.mtext.insert), "{name}");
        assert!(near(point(&g, 11).unwrap(), ml.mtext.direction.unwrap()), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), ml.normal), "{name}");
    }
}

fn record(text: &str, kind: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags).into_iter().find(|(k, _)| k == kind).unwrap().1
}

#[test]
fn the_canonical_reader_returns_what_the_canonical_writer_wrote() {
    let normal = Vec3::new(0.0, 3.0, 4.0);
    let mut t = text();
    t.backward = true;
    t.upside_down = true;
    let out = dxf(&drawing(EntityKind::Text(t.clone()), normal, -0.5));
    let (c, kind) = cadcraft_io::canonical_text_from_groups("TEXT", &record(&out, "TEXT")).unwrap();
    assert_eq!((c.extrusion, c.thickness), (normal, -0.5));
    let EntityKind::Text(mut read) = kind else { panic!("not TEXT") };
    // Degrees in the file: the angles come back to the last bit of rounding.
    assert!((read.oblique - t.oblique).abs() < 1e-12 && (read.rotation - t.rotation).abs() < 1e-12);
    (read.oblique, read.rotation) = (t.oblique, t.rotation);
    assert_eq!(read, t, "no mirroring, no projection");

    let m = mtext();
    let out = dxf(&drawing(EntityKind::MText(m.clone()), normal, 0.0));
    let (c, kind) = cadcraft_io::canonical_text_from_groups("MTEXT", &record(&out, "MTEXT")).unwrap();
    assert_eq!(c.extrusion, normal);
    let EntityKind::MText(read) = kind else { panic!("not MTEXT") };
    assert_eq!(read.insert, m.insert);
    assert!(near(read.direction.unwrap(), m.direction.unwrap()));
    assert_eq!((read.height, read.width, read.contents.as_str()), (m.height, m.width, "Body"));

    let a = cadcraft_doc::Attrib::new("TAG", text());
    let out = dxf(&drawing(EntityKind::AttDef(a.clone()), Vec3::new(0.0, -1.0, 0.0), 0.0));
    let (c, kind) = cadcraft_io::canonical_text_from_groups("ATTDEF", &record(&out, "ATTDEF")).unwrap();
    assert_eq!(c.extrusion, Vec3::new(0.0, -1.0, 0.0));
    let EntityKind::AttDef(read) = kind else { panic!("not ATTDEF") };
    let mut text = read.text;
    (text.oblique, text.rotation) = (a.text.oblique, a.text.rotation);
    assert_eq!((text, read.tag), (a.text, a.tag));
}

#[test]
fn the_canonical_reader_keeps_an_attribs_own_plane_and_its_embedded_planes() {
    let mut a = attrib(Vec3::new(0.0, 3.0, 4.0), 0.5);
    a.text.value = "Body".into();
    a.multiline = Some(cadcraft_doc::MultilineAttrib {
        geometry_revision: GeometryRevision::CanonicalV2,
        normal: Vec3::new(1.0, 0.0, 1.0),
        mtext: mtext(),
        read_as: "Body".into(),
        embedded_46: None,
        type_72: None,
    });
    let out = dxf(&insert_with(a.clone(), Vec3::new(1.0, 0.0, 0.0)));
    let read = cadcraft_io::canonical_attrib_from_groups(&record(&out, "ATTRIB"), None).unwrap();
    assert_eq!(read.geometry_revision, GeometryRevision::CanonicalV2);
    assert_eq!(read.plane, a.plane);
    let mut text = read.text.clone();
    assert!((text.oblique - a.text.oblique).abs() < 1e-12 && (text.rotation - a.text.rotation).abs() < 1e-12);
    (text.oblique, text.rotation) = (a.text.oblique, a.text.rotation);
    assert_eq!(text, a.text);
    let (want, got) = (a.multiline.unwrap(), read.multiline.unwrap());
    assert_eq!(got.normal, want.normal);
    assert!(near(got.mtext.direction.unwrap(), want.mtext.direction.unwrap()));
    assert_eq!(got.mtext.insert, want.mtext.insert);
}

#[test]
fn the_canonical_reader_refuses_a_normal_without_a_direction() {
    let tags = vec![Tag::s(1, "A"), Tag::f(40, 1.0), Tag::f(210, 0.0), Tag::f(220, 0.0), Tag::f(230, 0.0)];
    assert!(cadcraft_io::canonical_text_from_groups("TEXT", &tags).is_none());
}

/// The block `TAG` with a definition of thickness `def` and, in an insert, an attribute of thickness `inst` (legacy text).
fn legacy_attribs(def: f64, inst: f64) -> Drawing {
    let mut a = cadcraft_doc::Attrib::new("TAG", text());
    a.plane.thickness = inst;
    let mut d = insert_with(a, Vec3::Z);
    let mut b = cadcraft_doc::Block::new("TAG");
    let mut e = Entity::new(d.new_handle(), EntityKind::AttDef(cadcraft_doc::Attrib::new("TAG", text())));
    e.common.thickness = def;
    b.entities.push(e);
    d.blocks.insert("TAG".into(), std::sync::Arc::new(b));
    d
}

fn thicknesses(d: &Drawing) -> (f64, f64) {
    let def = d.block("TAG").unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::AttDef(_))).unwrap().common.thickness;
    let inst = d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i.attribs[0].plane.thickness) } else { None }).unwrap();
    (def, inst)
}

#[test]
fn attribute_thickness_survives_a_dxf_and_a_dwg_save() {
    let d = legacy_attribs(0.75, 1.25);
    assert_eq!(thicknesses(&d), (0.75, 1.25));
    let dxf = cadcraft_io::write(&d, "x.dxf").unwrap();
    assert_eq!(thicknesses(&cadcraft_io::read(&dxf, "x.dxf").unwrap()), (0.75, 1.25), "dxf");
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    assert_eq!(thicknesses(&cadcraft_io::read(&dwg, "x.dwg").unwrap()), (0.75, 1.25), "dwg");
}
