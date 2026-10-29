//! A canonical TOLERANCE or TABLE (revision CanonicalV2) is written with the direction and the normal it holds; each
//! field changed alone reaches the user DXF and the DWG. The canonical reader returns what the writer wrote.

use acadrust::DwgReader;
use acadrust::entities::EntityType;
use cadcraft_doc::{Common, Drawing, Entity, EntityKind, GeometryRevision, Table, TableCell, Tolerance};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

const NORMAL: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 2.0 };

fn tolerance() -> Tolerance {
    Tolerance { insert: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.6, 0.8, 0.0), style: "Standard".into(), text: "{\\Fgdt;j}%%v0.1".into() }
}

fn table() -> Table {
    let cell = |text: &str| TableCell { text: text.into(), ..TableCell::default() };
    Table {
        insert: Vec3::new(1.0, 2.0, 3.0),
        dir: Vec3::new(0.6, 0.8, 0.0),
        col_widths: vec![2.0, 3.0],
        row_heights: vec![1.0, 1.5],
        cells: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
        ..Table::default()
    }
}

fn drawing(kind: EntityKind, normal: Vec3) -> Drawing {
    let mut d = Drawing::new_metric();
    let handle = d.new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, ..Common::default() };
    d.model.push(e);
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap()
}

fn record(text: &str, kind: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags).into_iter().find(|(k, _)| k == kind).unwrap_or_else(|| panic!("no {kind}")).1
}

fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    record(text, kind).iter().map(|t| (t.code, t.str())).collect()
}

fn point(g: &[(i32, String)], code: i32) -> Option<Vec3> {
    let n = |c: i32| g.iter().find(|t| t.0 == c).map(|t| t.1.trim().parse::<f64>().unwrap());
    Some(Vec3::new(n(code)?, n(code + 10)?, n(code + 20)?))
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn dwg_entities(d: &Drawing) -> Vec<EntityType> {
    let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap();
    stored.entities().cloned().collect()
}

#[test]
fn each_tolerance_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Tolerance, &mut Vec3);
    let cases: Vec<(&str, Mutation)> = vec![
        ("insert x", |t, _| t.insert.x = 1.5),
        ("insert y", |t, _| t.insert.y = 2.5),
        ("insert z", |t, _| t.insert.z = 3.5),
        ("direction x", |t, _| t.dir.x = 0.8),
        ("direction y", |t, _| t.dir.y = 0.6),
        ("direction z", |t, _| t.dir.z = 0.25),
        ("normal x", |_, n| n.x = 0.25),
        ("normal y", |_, n| n.y = 0.5),
        ("normal z", |_, n| n.z = 2.5),
    ];
    let make = |mutate: Mutation| {
        let (mut t, mut n) = (tolerance(), NORMAL);
        mutate(&mut t, &mut n);
        (drawing(EntityKind::Tolerance(t.clone()), n), t, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "TOLERANCE");
    for (name, mutate) in cases {
        let (d, t, n) = make(mutate);
        let g = groups(&dxf(&d), "TOLERANCE");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), t.insert) && near(point(&g, 11).unwrap(), t.dir), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n), "{name}");
        let s =
            dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Tolerance(s) = e { Some(s) } else { None }).expect("a TOLERANCE in the DWG");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.insertion_point), t.insert), "{name}: DWG insertion {:?}", s.insertion_point);
        assert!(near(v(s.direction), t.dir), "{name}: DWG direction {:?}", s.direction);
        assert!(near(v(s.normal), n), "{name}: DWG normal {:?}", s.normal);
    }
}

#[test]
fn each_table_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Table, &mut Vec3);
    let cases: Vec<(&str, Mutation)> = vec![
        ("insert x", |t, _| t.insert.x = 1.5),
        ("insert z", |t, _| t.insert.z = 3.5),
        ("direction x", |t, _| t.dir.x = 0.8),
        ("direction y", |t, _| t.dir.y = 0.6),
        ("direction z", |t, _| t.dir.z = 0.25),
        ("normal x", |_, n| n.x = 0.25),
        ("normal y", |_, n| n.y = 0.5),
        ("normal z", |_, n| n.z = 2.5),
    ];
    let make = |mutate: Mutation| {
        let (mut t, mut n) = (table(), NORMAL);
        mutate(&mut t, &mut n);
        (drawing(EntityKind::Table(t.clone()), n), t, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "ACAD_TABLE");
    for (name, mutate) in cases {
        let (d, t, n) = make(mutate);
        let g = groups(&dxf(&d), "ACAD_TABLE");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), t.insert) && near(point(&g, 11).unwrap(), t.dir), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n), "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Table(s) = e { Some(s) } else { None }).expect("a TABLE in the DWG");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.insertion_point), t.insert), "{name}: DWG insertion {:?}", s.insertion_point);
        assert!(near(v(s.horizontal_direction), t.dir), "{name}: DWG direction {:?}", s.horizontal_direction);
        assert!(near(v(s.normal), n), "{name}: DWG normal {:?}", s.normal);
    }
}

#[test]
fn the_canonical_reader_returns_what_the_canonical_writer_wrote() {
    let normal = Vec3::new(0.0, 3.0, 4.0);
    let mut t = tolerance();
    t.dir = Vec3::new(0.0, 0.8, -0.6);
    let out = dxf(&drawing(EntityKind::Tolerance(t.clone()), normal));
    let (c, kind) = cadcraft_io::canonical_layout_from_groups("TOLERANCE", &record(&out, "TOLERANCE")).unwrap();
    assert_eq!(c.extrusion, normal);
    assert_eq!(kind, EntityKind::Tolerance(t));

    let mut table = table();
    table.dir = Vec3::new(1.0, 0.0, 0.0);
    let out = dxf(&drawing(EntityKind::Table(table.clone()), normal));
    let (c, kind) = cadcraft_io::canonical_layout_from_groups("ACAD_TABLE", &record(&out, "ACAD_TABLE")).unwrap();
    assert_eq!(c.extrusion, normal);
    let EntityKind::Table(read) = kind else { panic!("not a table") };
    assert_eq!((read.insert, read.dir, read.col_widths, read.row_heights), (table.insert, table.dir, table.col_widths, table.row_heights));
}

#[test]
fn the_canonical_reader_refuses_a_direction_outside_the_plane_and_a_zero_normal() {
    // The direction (1, 0, 0) is not perpendicular to the normal (1, 0, 1).
    let out = dxf(&drawing(EntityKind::Tolerance(Tolerance { dir: Vec3::new(1.0, 0.0, 0.0), ..tolerance() }), Vec3::new(1.0, 0.0, 1.0)));
    assert!(cadcraft_io::canonical_layout_from_groups("TOLERANCE", &record(&out, "TOLERANCE")).is_none());
    let tags = vec![Tag::s(1, "x"), Tag::f(210, 0.0), Tag::f(220, 0.0), Tag::f(230, 0.0)];
    assert!(cadcraft_io::canonical_layout_from_groups("TOLERANCE", &tags).is_none());
}

#[test]
fn the_readers_still_read_a_legacy_table_as_horizontal() {
    let d = drawing(EntityKind::Table(table()), NORMAL);
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    let e = back.model.iter().find(|e| matches!(e.kind, EntityKind::Table(_))).unwrap();
    assert_eq!(e.geometry_revision, GeometryRevision::LegacyProjectedV1);
    let EntityKind::Table(t) = &e.kind else { unreachable!() };
    assert_eq!(t.dir, Vec3::new(1.0, 0.0, 0.0), "no canonical import is active");
}
