//! Private DWG records survive the bridge, but never enter a user DXF.
use std::io::Cursor;

use acadrust::classes::DxfClass;
use acadrust::classes::arch::ArchObject;
use acadrust::io::dwg::dwg_stream_writers::DwgMergedWriter;
use acadrust::io::dwg::{DwgReferenceType, DwgVersion};
use acadrust::objects::{Dictionary, ObjectType};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfVersion, DxfWriter, Handle};

const CLASS: &str = "AECS_DISP_REP_MEMBER_MODEL_DETAIL";

fn source() -> Vec<u8> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.classes.add_or_update(DxfClass::new(CLASS, "AecsDbDispRepMemberModelDetail"));
    let owner = doc.header.named_objects_dict_handle;
    let handle = doc.allocate_handle();
    let target = doc.allocate_handle();
    let mut dictionary = Dictionary::new();
    dictionary.handle = target;
    dictionary.owner = owner;
    doc.objects.insert(target, ObjectType::Dictionary(dictionary));
    let number = doc.classes.get_by_name(CLASS).unwrap().class_number;
    let mut writer = DwgMergedWriter::new(DwgVersion::from_dxf_version(doc.version).unwrap(), doc.version);
    writer.write_object_type(number);
    writer.main_mut().write_handle_undefined(handle.value());
    writer.write_bit_short(0);
    writer.write_handle(DwgReferenceType::SoftPointer, owner.value());
    writer.write_bit_long(0);
    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_raw_long(0x1234_5678);
    writer.write_handle(DwgReferenceType::HardPointer, target.value());
    let raw = writer.merge();
    let mut object = ArchObject::new(CLASS, handle, owner);
    object.raw_dwg_handle_bits = raw.len() as i64 * 8 - writer.handle_start_bits();
    object.raw_dwg_data = Some(raw);
    object.raw_dwg_version = Some(doc.version);
    doc.objects.insert(handle, ObjectType::Arch(object));
    let Some(ObjectType::Dictionary(root)) = doc.objects.get_mut(&owner) else { panic!("root") };
    root.add_entry("raw class", handle);
    root.add_entry("raw target", target);
    DwgWriter::write_to_vec(&doc).unwrap()
}

fn raw_object(doc: &CadDocument) -> &ArchObject {
    doc.objects
        .values()
        .find_map(|object| match object {
            ObjectType::Arch(object) if object.dxf_name == CLASS => Some(object),
            _ => None,
        })
        .expect("raw class survives")
}

#[test]
fn raw_records_and_their_references_survive_two_dwg_saves() {
    let mut bytes = source();
    for _ in 0..2 {
        let drawing = cadcraft_io::read(&bytes, "source.dwg").unwrap();
        assert!(drawing.foreign.objects.iter().any(|o| o.kind == CLASS));
        bytes = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
        let doc = DwgReader::from_stream(Cursor::new(&bytes)).read().unwrap();
        let object = raw_object(&doc);
        assert_eq!(object.owner, doc.header.named_objects_dict_handle);
        let text = String::from_utf8(DxfWriter::new(&doc).for_dwg_bridge().write_to_vec().unwrap()).unwrap();
        let restored = acadrust::DxfReader::from_reader(Cursor::new(text.into_bytes())).unwrap().read().unwrap();
        let raw = raw_object(&restored);
        let Some(ObjectType::Dictionary(root)) = restored.objects.get(&restored.header.named_objects_dict_handle) else { panic!("root") };
        let target = root.entries.iter().find(|(name, _)| name == "raw target").unwrap().1;
        let references = raw.raw_dwg_references.as_ref().unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].handle, target);
        assert!(restored.objects.contains_key(&target));
        assert!(restored.classes.get_by_name(CLASS).is_some());
        assert_ne!(object.handle, Handle::NULL);
    }
}

#[test]
fn another_dwg_version_reports_the_raw_record_omission() {
    let drawing = cadcraft_io::read(&source(), "source.dwg").unwrap();
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: false };
    let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, "saved.dwg", &opts).unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains(CLASS), "{notes:?}");
    assert!(notes[0].contains("AC1032, not AC1024"), "{notes:?}");
    let doc = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    assert!(!doc.objects.values().any(|o| matches!(o, ObjectType::Arch(a) if a.dxf_name == CLASS)));
}

#[test]
fn a_user_dxf_has_no_private_records_or_private_classes() {
    let bridge = cadcraft_dwg::dwg_to_dxf(&source()).unwrap();
    assert!(String::from_utf8_lossy(&bridge).contains("AcDbAcadrustRawDwgObject"));
    let drawing = cadcraft_io::read_dxf(&bridge).unwrap();
    let saved = String::from_utf8(cadcraft_io::write(&drawing, "saved.dxf").unwrap()).unwrap();
    assert!(!saved.contains("AcDbAcadrustRawDwgObject"));
    assert!(!saved.contains(CLASS));
}
