//! The VX table (the viewport entity headers of a drawing of 2004 or later) has no public DXF form: AutoCAD's DXFOUT
//! does not write it. The DWG bridge carries it as two private records, `ACADRUST_VXCONTROL` and `ACADRUST_VXRECORD`:
//! a CADCraft save to DWG keeps the table with its viewports, and a DXF holds none of it.

use std::io::Cursor;

use acadrust::entities::{EntityType, Viewport};
use acadrust::tables::VxTableRecord;
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, Handle};
use cadcraft_io::{SaveOptions, Version, write_with};

/// A drawing of 2018 with two viewports and three VX records chained by previous entry (as Planeta Toulouse has
/// them): the first has no viewport.
fn dwg() -> (Vec<u8>, [Handle; 2]) {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut viewports = [Handle::NULL; 2];
    for (n, handle) in viewports.iter_mut().enumerate() {
        let mut viewport = Viewport::new();
        viewport.id = n as i16 + 2;
        *handle = doc.add_paper_space_entity(EntityType::Viewport(Box::new(viewport))).expect("add viewport");
    }
    let [first, second, third] = [doc.allocate_handle(), doc.allocate_handle(), doc.allocate_handle()];
    let mut a = VxTableRecord::new("");
    a.handle = first;
    a.is_on = true;
    let mut b = VxTableRecord::new("1");
    (b.handle, b.is_on, b.viewport, b.previous_entry) = (second, true, viewports[0], third);
    let mut c = VxTableRecord::new("");
    (c.handle, c.is_on, c.viewport) = (third, false, viewports[1]);
    for record in [a, b, c] {
        doc.vx_table.add_allow_duplicate(record);
    }
    doc.vx_control_entries = vec![first, second, third];
    (DwgWriter::write_to_vec(&doc).expect("write dwg"), viewports)
}

/// (name, on, viewport, the table position of the previous entry) of each record, by handle.
fn records(dwg: &[u8]) -> Vec<(String, bool, u64, Option<usize>)> {
    let doc = DwgReader::from_stream_with_options(Cursor::new(dwg), DwgReadOptions::failsafe()).read().expect("read dwg");
    let mut all: Vec<_> = doc.vx_table.iter().collect();
    all.sort_by_key(|r| r.handle.value());
    all.iter().map(|r| (r.name.clone(), r.is_on, r.viewport.value(), all.iter().position(|p| p.handle == r.previous_entry))).collect()
}

#[test]
fn a_dwg_save_keeps_the_vx_table_and_a_dxf_has_none() {
    let (source, viewports) = dwg();
    let before = records(&source);
    assert_eq!(before.len(), 3, "the source has its table");
    assert!(before.iter().any(|r| r.2 == viewports[0].value()));
    let d = cadcraft_io::read(&source, "x.dwg").unwrap();
    let saved = cadcraft_io::write(&d, "x.dwg").unwrap();
    assert_eq!(records(&saved), before, "the DWG keeps every record, its viewport and its chain");
    // A second save keeps them too.
    let again = cadcraft_io::write(&cadcraft_io::read(&saved, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(records(&again), before);
    for version in Version::ALL {
        let text = String::from_utf8(write_with(&d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap()).unwrap();
        assert!(!text.contains("ACADRUST_VX"), "{version:?}: a user DXF has no VX record");
        assert!(!text.contains("AcDbVx"), "{version:?}");
    }
}
