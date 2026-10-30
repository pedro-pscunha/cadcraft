//! A block-owned native SURFACE must survive the complete CADCraft DWG bridge.
use acadrust::entities::acis::{SabWriter, primitives::build_box};
use acadrust::entities::solid3d::{AcisData, Wire};
use acadrust::entities::{EntityType, Surface, SurfaceKind};
use acadrust::tables::BlockRecord;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter};
use std::io::Cursor;

#[test]
fn a_block_surface_keeps_its_sab_and_twelve_wires_through_a_cadcraft_dwg_save() {
    let mut native = CadDocument::with_version(DxfVersion::AC1032);
    let mut block = BlockRecord::new("SURFACE_BLOCK");
    block.handle = native.allocate_handle();
    block.block_entity_handle = native.allocate_handle();
    block.block_end_handle = native.allocate_handle();
    let owner = block.handle;
    native.block_records.add(block).unwrap();
    let mut source = Surface::new(SurfaceKind::Generic);
    source.common.owner_handle = owner;
    source.acis_data = AcisData::from_sab(SabWriter::write(&build_box([0.0, 0.0, 0.0], 2.0, 3.0, 4.0)));
    source.acis_data.wireframe_data_present = true;
    source.acis_data.wireframe_point_present = true;
    source.acis_data.wireframe_isoline_present = true;
    source.acis_data.wireframe_isolines = 6;
    source.acis_data.revision.has_guid = true;
    source.acis_data.revision.major = 12345;
    source.point_of_reference = Vector3::new(3.0, 4.0, 5.0);
    source.wires =
        (0..12).map(|index| Wire::from_points(vec![Vector3::new(f64::from(index), 1.0, 2.0), Vector3::new(f64::from(index), 3.0, 4.0)])).collect();
    let handle = native.add_entity(EntityType::Surface(Box::new(source.clone()))).unwrap();
    let input = DwgWriter::write_to_vec(&native).unwrap();
    let drawing = cadcraft_io::read(&input, "surface.dwg").unwrap();
    let (saved, notes) = cadcraft_io::write_with_noted(&drawing, "saved.dwg", &Default::default()).unwrap();
    assert!(!notes.iter().any(|note| note.contains("not saved") && note.contains("SURFACE")), "{notes:?}");
    let restored = DwgReader::from_stream(Cursor::new(saved)).read().unwrap();
    let Some(EntityType::Surface(actual)) = restored.get_entity(handle) else {
        panic!("block SURFACE was omitted");
    };
    assert_eq!(actual.common.owner_handle, owner);
    assert_eq!(actual.acis_data.sab_data, source.acis_data.sab_data);
    assert_eq!(actual.acis_data.revision, source.acis_data.revision);
    assert_eq!(actual.wires, source.wires);
    assert_eq!(actual.point_of_reference, source.point_of_reference);
    assert_eq!(actual.acis_data.wireframe_isolines, 6);
}
