//! A table cell that holds a block (DXF cell type 171 = 2, with the block record 340, the scale 144 and the attribute values 179,
//! 331, 300) keeps it through a DXF and a DWG save, the block is drawn in the cell and in the table's anonymous block, and a block
//! cell from a DWG of AutoCAD's layout is not turned into a text cell.

use std::io::Cursor;

use acadrust::entities::table::{CellContent, CellType, Table as RTable, TableAttribute, TableCellContentType};
use acadrust::tables::BlockRecord;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, EntityType, Handle as RHandle};
use cadcraft_doc::{
    Attrib, Block, Circle, Common, Drawing, Entity, EntityKind, HAlign, Handle, Space, Table, TableCell, TableCellBlock, Text, VAlign,
};
use cadcraft_dxf::{Version, records, sections};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, read, read_dxf, write_with};

const ATTDEF: Handle = Handle(0x4202);

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut logo = Block::new("Logo");
    logo.entities.push(Entity::new(Handle(0x4201), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 })));
    let text = Text {
        insert: Vec3::new(-0.5, -0.1, 0.0),
        align_pt: None,
        height: 0.3,
        value: "?".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::default(),
        valign: VAlign::default(),
        backward: false,
        upside_down: false,
    };
    logo.entities.push(Entity::new(ATTDEF, EntityKind::AttDef(Attrib { prompt: "Name".into(), ..Attrib::new("NAME", text) })));
    d.blocks.insert("Logo".into(), std::sync::Arc::new(logo));
    let cell = |t: &str| TableCell { text: t.into(), ..Default::default() };
    let block = TableCell {
        block: Some(TableCellBlock { block: "Logo".into(), scale: 2.0, attribs: vec![(ATTDEF, "Acme".into())] }),
        rotation: 0.5,
        ..Default::default()
    };
    let t = Table {
        insert: Vec3::new(1.0, 20.0, 0.0),
        col_widths: vec![6.0, 6.0],
        row_heights: vec![5.0, 5.0],
        cells: vec![vec![cell("a"), block], vec![cell("c"), cell("d")]],
        style: "Standard".into(),
        text_height: 2.5,
        ..Table::default()
    };
    d.add(&Space::Model, Common::default(), EntityKind::Table(t)).unwrap();
    d
}

fn first_table(d: &Drawing) -> Table {
    d.model.iter().find_map(|e| if let EntityKind::Table(t) = &e.kind { Some(t.clone()) } else { None }).expect("a table")
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap()
}

fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| records(&s.tags).into_iter().filter(|(k, _)| k == kind).map(|(_, t)| t).collect::<Vec<_>>())
        .next()
        .map(|t| t.into_iter().map(|t| (t.code, t.str().trim().to_string())).collect())
        .unwrap_or_default()
}

#[test]
fn a_block_cell_keeps_its_block_scale_and_attributes_through_a_dxf_save() {
    let d = drawing();
    let text = dxf(&d);
    let g = groups(&text, "ACAD_TABLE");
    let at = |code: i32| g.iter().filter(|(c, _)| *c == code).map(|(_, v)| v.clone()).collect::<Vec<_>>();
    assert_eq!(at(171), ["1", "2", "1", "1"], "one block cell");
    assert_eq!(
        (at(144), at(179), at(331), at(300)),
        (["2.0".to_string()].to_vec(), ["1".into()].to_vec(), ["4202".into()].to_vec(), ["Acme".into()].to_vec())
    );
    assert_eq!(at(340).len(), 1);
    let back = read_dxf(text.as_bytes()).unwrap();
    assert_eq!(first_table(&back).cells[0][1].block, first_table(&d).cells[0][1].block);
    assert_eq!(first_table(&back).cells[0][1].rotation, 0.5);
    // The block record named by 340 is Logo's.
    let record = at(340)[0].clone();
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let tables = sections(&tags).into_iter().find(|s| s.name == "TABLES").unwrap();
    let logo = records(&tables.tags).into_iter().find(|(k, t)| k == "BLOCK_RECORD" && t.iter().any(|t| t.code == 2 && t.str() == "Logo")).unwrap();
    assert_eq!(logo.1.iter().find(|t| t.code == 5).map(|t| t.str()), Some(record));
    // The second save writes the same cell.
    let again = groups(&dxf(&back), "ACAD_TABLE");
    assert_eq!(
        again.iter().filter(|(c, _)| matches!(*c, 171 | 144 | 179 | 331 | 300)).collect::<Vec<_>>(),
        g.iter().filter(|(c, _)| matches!(*c, 171 | 144 | 179 | 331 | 300)).collect::<Vec<_>>()
    );
}

#[test]
fn the_tables_anonymous_block_draws_the_block_in_its_cell() {
    let text = dxf(&drawing());
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let blocks = sections(&tags).into_iter().find(|s| s.name == "BLOCKS").unwrap();
    let recs = records(&blocks.tags);
    let start = recs.iter().position(|(k, t)| k == "BLOCK" && t.iter().any(|t| t.code == 2 && t.str().starts_with("*T"))).expect("the *T block");
    let end = recs[start..].iter().position(|(k, _)| k == "ENDBLK").map_or(recs.len(), |p| start + p);
    let inserts: Vec<_> = recs[start..end].iter().filter(|(k, _)| k == "INSERT").collect();
    assert_eq!(inserts.len(), 1, "one reference to the block");
    let value = |code: i32| inserts[0].1.iter().find(|t| t.code == code).map(|t| t.f64());
    assert_eq!(inserts[0].1.iter().find(|t| t.code == 2).map(|t| t.str()).as_deref(), Some("Logo"));
    assert_eq!(value(41), Some(2.0));
    assert!((value(50).unwrap() - 0.5_f64.to_degrees()).abs() < 1e-9, "{:?}", value(50));
    // Inside the cell: column 2 is x 6 to 12 right of the table's corner, row 1 y 0 to -5 below it.
    let (x, y) = (value(10).unwrap(), value(20).unwrap());
    assert!(x > 6.0 && x < 12.0 && y < 0.0 && y > -5.0, "({x}, {y})");
    let attribs: Vec<_> = recs[start..end].iter().filter(|(k, _)| k == "ATTRIB").collect();
    assert_eq!(attribs.len(), 1);
    assert_eq!(attribs[0].1.iter().find(|t| t.code == 1).map(|t| t.str()).as_deref(), Some("Acme"));
}

fn read_dwg(bytes: &[u8]) -> CadDocument {
    DwgReader::from_stream(Cursor::new(bytes.to_vec())).read().expect("acadrust reads the DWG")
}

/// The block cell of the first table: its type, block name, scale and attribute values.
fn block_cell(doc: &CadDocument) -> (CellType, Option<String>, f64, Vec<String>) {
    let table = doc
        .entities()
        .find_map(|e| match e {
            EntityType::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("a table");
    let cell = &table.rows[0].cells[1];
    let content = cell.contents.first();
    let name = content.and_then(|c| c.block_handle).and_then(|h| doc.block_records.iter().find(|r| r.handle == h)).map(|r| r.name.to_string());
    let scale = content.map_or(0.0, |c| if c.scale != 1.0 { c.scale } else { cell.block_scale });
    (cell.cell_type, name, scale, content.map(|c| c.attributes.iter().map(|a| a.value.clone()).collect()).unwrap_or_default())
}

#[test]
fn a_block_cell_survives_a_dwg_save() {
    for version in [Version::R2007, Version::R2018] {
        let d = drawing();
        let dwg = write_with(&d, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let held = block_cell(&read_dwg(&dwg));
        assert_eq!(held, (CellType::Block, Some("Logo".to_string()), 2.0, vec!["Acme".to_string()]), "{version:?}");
        // And back through CADCraft: the cell is still a block cell.
        let back = read(&dwg, "x.dwg").unwrap();
        assert_eq!(
            first_table(&back).cells[0][1].block.as_ref().map(|b| (b.block.as_str(), b.scale, b.attribs.len())),
            Some(("Logo", 2.0, 1)),
            "{version:?}"
        );
        let again = write_with(&back, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        assert_eq!(block_cell(&read_dwg(&again)), held, "{version:?}: second save");
    }
}

/// A table as AutoCAD lays out a block cell in a DWG, built natively.
fn native(version: DxfVersion) -> CadDocument {
    let mut doc = CadDocument::with_version(version);
    let mut record = BlockRecord::new("Logo");
    record.handle = doc.allocate_handle();
    let block = record.handle;
    doc.block_records.add(record).expect("add block record");
    let mut table = RTable::new(Vector3::new(1.0, 20.0, 0.0), 1, 2);
    table.set_cell_text(0, 0, "a");
    let cell = &mut table.rows[0].cells[1];
    cell.cell_type = CellType::Block;
    cell.block_scale = 2.0;
    let mut content = CellContent::new();
    content.content_type = TableCellContentType::Block;
    content.block_handle = Some(block);
    content.scale = 2.0;
    content.attributes.push(TableAttribute { definition_handle: RHandle::new(0x4202), value: "Acme".into(), index: 1 });
    cell.contents = vec![content];
    doc.add_entity(EntityType::Table(Box::new(table))).unwrap();
    doc
}

#[test]
fn a_native_block_cell_is_not_turned_into_a_text_cell_by_a_save() {
    for (version, source, expected) in [(Version::R2007, DxfVersion::AC1021, 2.0), (Version::R2018, DxfVersion::AC1032, 2.0)] {
        let doc = native(source);
        let dwg = DwgWriter::write_to_vec(&doc).unwrap();
        let held = block_cell(&read_dwg(&dwg));
        assert_eq!(held.0, CellType::Block, "{version:?}: the source");
        let back = read(&dwg, "x.dwg").unwrap();
        let cell = first_table(&back).cells[0][1].clone();
        assert!(cell.block.is_some(), "{version:?}: a block cell in the drawing: {cell:?}");
        let again = write_with(&back, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let saved = block_cell(&read_dwg(&again));
        assert_eq!((saved.0, saved.1.as_deref(), saved.2), (CellType::Block, Some("Logo"), expected), "{version:?}: after a save");
    }
}

#[test]
fn a_block_cell_whose_extents_are_refused_is_still_saved_as_it_is() {
    use cadcraft_doc::GeometryRevision;
    let mut d = drawing();
    // A canonical entity in the block makes a legacy measurement refuse: the table cannot place the block in its graphics.
    let mut logo = d.block("Logo").unwrap().as_ref().clone();
    logo.entities.push(Entity {
        geometry_revision: GeometryRevision::CanonicalV2,
        ..Entity::new(Handle(0x4203), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }))
    });
    d.blocks.insert("Logo".into(), std::sync::Arc::new(logo));
    let text = dxf(&d);
    let g = groups(&text, "ACAD_TABLE");
    let at = |code: i32| g.iter().filter(|(c, _)| *c == code).map(|(_, v)| v.clone()).collect::<Vec<_>>();
    assert_eq!(
        (at(171), at(144), at(179), at(331), at(300)),
        (
            ["1", "2", "1", "1"].map(String::from).to_vec(),
            ["2.0".to_string()].to_vec(),
            ["1".to_string()].to_vec(),
            ["4202".to_string()].to_vec(),
            ["Acme".to_string()].to_vec()
        )
    );
    assert_eq!(at(340).len(), 1);
    assert_eq!(first_table(&read_dxf(text.as_bytes()).unwrap()).cells[0][1].block, first_table(&d).cells[0][1].block);
    // The anonymous block holds no reference for it.
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let blocks = sections(&tags).into_iter().find(|s| s.name == "BLOCKS").unwrap();
    let recs = records(&blocks.tags);
    let start = recs.iter().position(|(k, t)| k == "BLOCK" && t.iter().any(|t| t.code == 2 && t.str().starts_with("*T"))).unwrap();
    let end = recs[start..].iter().position(|(k, _)| k == "ENDBLK").map_or(recs.len(), |p| start + p);
    assert!(recs[start..end].iter().all(|(k, _)| k != "INSERT"));
}
