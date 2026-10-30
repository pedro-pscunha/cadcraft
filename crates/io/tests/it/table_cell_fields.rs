//! A table cell that shows a FIELD (DXF 344, cached text in the cell) keeps its link: the FIELD object, owned by the table, is kept
//! and the cell points at it after a DXF and a DWG save.

use std::io::Cursor;

use acadrust::classes::DxfClass;
use acadrust::entities::table::{CellContent, Table as RTable, TableCellContentType};
use acadrust::objects::{Field, ObjectType};
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, EntityType};
use cadcraft_dxf::{Version, records, sections};
use cadcraft_io::{SaveOptions, read, read_dxf, write_with};

const CODE: &str = "%<\\AcVar Date \\f \"yyyy\">%";

/// A table whose second cell shows a field, as AutoCAD lays it out: the field object owned by the table.
fn native(version: DxfVersion) -> CadDocument {
    let mut doc = CadDocument::with_version(version);
    for (name, class) in [("FIELD", "AcDbField"), ("FIELDLIST", "AcDbFieldList")] {
        let mut c = DxfClass::new(name, class);
        c.application_name = "ObjectDBX Classes".into();
        doc.classes.add_or_update(c);
    }
    let mut table = RTable::new(Vector3::ZERO, 1, 2);
    table.set_cell_text(0, 0, "a");
    let field = doc.allocate_handle();
    doc.objects.insert(
        field,
        ObjectType::Field(Field {
            handle: field,
            evaluator_id: "AcDbText".into(),
            code: CODE.into(),
            value_string: "2026".into(),
            value_string_length: 4,
            ..Field::default()
        }),
    );
    let mut content = CellContent::new();
    content.content_type = TableCellContentType::Field;
    content.field_handle = Some(field);
    table.rows[0].cells[1].contents = vec![content];
    table.field_handles = vec![field];
    let host = doc.add_entity(EntityType::Table(Box::new(table))).unwrap();
    if let Some(ObjectType::Field(f)) = doc.objects.get_mut(&field) {
        f.owner = host;
    }
    doc
}

fn read_dwg(bytes: &[u8]) -> CadDocument {
    DwgReader::from_stream(Cursor::new(bytes.to_vec())).read().expect("acadrust reads the DWG")
}

/// The field of the table's second cell: its content type, and the code of the FIELD object it names and its owner.
fn field_cell(doc: &CadDocument) -> (TableCellContentType, Option<String>, bool) {
    let table = doc
        .entities()
        .find_map(|e| match e {
            EntityType::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("a table");
    let content = &table.rows[0].cells[1].contents[0];
    let field = content.field_handle.and_then(|h| match doc.objects.get(&h) {
        Some(ObjectType::Field(f)) => Some(f.clone()),
        _ => None,
    });
    let owned = field.as_ref().is_some_and(|f| doc.entities().any(|e| e.common().handle == f.owner));
    (content.content_type, field.map(|f| f.code), owned)
}

#[test]
fn a_field_cell_keeps_its_field_through_a_dwg_save() {
    // From R2010 a cell holds its contents, which can be a field. acadrust's R2007 DWG has no field in a cell.
    for (version, source) in [(Version::R2010, DxfVersion::AC1024), (Version::R2018, DxfVersion::AC1032)] {
        let dwg = DwgWriter::write_to_vec(&native(source)).unwrap();
        let held = field_cell(&read_dwg(&dwg));
        assert_eq!(held, (TableCellContentType::Field, Some(CODE.to_string()), true), "{version:?}: the source");
        let back = read(&dwg, "x.dwg").unwrap();
        let opts = SaveOptions { version: Some(version), binary: false };
        let first = write_with(&back, "x.dwg", &opts).unwrap();
        assert_eq!(field_cell(&read_dwg(&first)), held, "{version:?}: first save");
        let second = write_with(&read(&first, "x.dwg").unwrap(), "x.dwg", &opts).unwrap();
        assert_eq!(field_cell(&read_dwg(&second)), held, "{version:?}: second save");
    }
}

#[test]
fn a_field_cell_keeps_its_field_through_a_dxf_save() {
    let dwg = DwgWriter::write_to_vec(&native(DxfVersion::AC1032)).unwrap();
    let drawing = read(&dwg, "x.dwg").unwrap();
    let text = String::from_utf8(write_with(&drawing, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap();
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let handle_of = |kind: &str, section: &str| {
        sections(&tags)
            .into_iter()
            .filter(|s| s.name == section)
            .flat_map(|s| records(&s.tags))
            .filter(|(k, _)| k == kind)
            .map(|(_, t)| t)
            .next()
            .unwrap_or_default()
    };
    let table = handle_of("ACAD_TABLE", "ENTITIES");
    let field = handle_of("FIELD", "OBJECTS");
    let value = |t: &[cadcraft_dxf::Tag], code: i32| t.iter().find(|t| t.code == code).map(|t| t.str().trim().to_string());
    assert!(value(&field, 2).is_some_and(|c| c == CODE), "the FIELD object is kept: {field:?}");
    assert_eq!(value(&table, 344), value(&field, 5), "the cell names the FIELD");
    assert_eq!(value(&field, 330), value(&table, 5), "the table owns the FIELD");
    // A second save keeps it too.
    let again = write_with(&read_dxf(text.as_bytes()).unwrap(), "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    assert!(String::from_utf8(again).unwrap().contains("AcVar Date"));
}
