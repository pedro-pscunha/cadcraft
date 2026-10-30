//! The source geometry of an entity (its plane, Z values and angles) goes with a copy to another
//! drawing, by IMPORT, by the insertion of a file, and by the clipboard (also as DXF text), and a
//! save of the copy writes it while the copy's geometry is unchanged.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::clipboard::{load_system_text, system_text};
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

const SOURCE: &str = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n5\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n1.5\n20\n2.5\n30\n-3.36738523e-06\n11\n4.0\n21\n5.0\n31\n-3.02954696e-06\n210\n6.3141e-10\n220\n2.5497228e-07\n230\n0.99999999999997\n\
0\nARC\n5\n101\n100\nAcDbEntity\n8\n0\n100\nAcDbCircle\n10\n3.25\n20\n4.5\n30\n7.75\n40\n2.125\n210\n0\n220\n0\n230\n-1\n100\nAcDbArc\n50\n30.000000000000004\n51\n120.5\n\
0\nENDSEC\n0\nEOF\n";

fn hooks() {
    set_io(IoHooks {
        read: |bytes, path| cadcraft_io::read(bytes, path).map_err(|e| e.to_string()),
        write: |drawing, path| cadcraft_io::write(drawing, path).map_err(|e| e.to_string()),
        plot: None,
    });
    set_write_noted(|drawing, path, opts| {
        cadcraft_io::write_with_noted(drawing, path, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary })
            .map_err(|e| e.to_string())
    });
}

/// A session with the source open, and a new empty drawing active after it.
fn session() -> Session {
    hooks();
    let mut s = Session::new();
    s.open_drawing(cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap(), "source.dxf", None);
    s
}

/// The bits of every number with group `code` in the first record of `kind` of the saved active drawing.
fn bits(s: &Session, kind: &str, code: i32) -> Vec<u64> {
    let text = cadcraft_io::write_dxf(s.doc().unwrap());
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let record: Vec<Tag> = sections(&tags)
        .into_iter()
        .filter(|sec| sec.name == "ENTITIES" || sec.name == "BLOCKS")
        .flat_map(|sec| records(&sec.tags))
        .find(|(k, _)| k == kind)
        .unwrap_or_else(|| panic!("no {kind}: {text}"))
        .1;
    record.iter().filter(|t| t.code == code).map(|t| t.str().trim().parse::<f64>().unwrap().to_bits()).collect()
}

fn assert_planes(s: &Session, what: &str) {
    assert_eq!(bits(s, "LINE", 210), vec![6.3141e-10f64.to_bits()], "{what}: LINE plane");
    assert_eq!(bits(s, "LINE", 230), vec![0.99999999999997f64.to_bits()], "{what}: LINE plane");
    assert_eq!(bits(s, "ARC", 230), vec![(-1.0f64).to_bits()], "{what}: ARC plane");
    assert_eq!(bits(s, "ARC", 50), vec![30.000000000000004f64.to_bits()], "{what}: ARC angle");
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() { out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char) } else { out.push('=') }
        }
    }
    out
}

#[test]
fn import_keeps_the_source_geometry() {
    let mut s = session();
    s.open_drawing(cadcraft_doc::Drawing::new_imperial(), "target.dxf", None);
    s.execute("import", &json!({"data": base64(SOURCE.as_bytes()), "name": "source.dxf"})).unwrap();
    assert_planes(&s, "IMPORT");
    // Imported at another place, the geometry is changed and the plane is not replayed.
    let mut moved = session();
    moved.open_drawing(cadcraft_doc::Drawing::new_imperial(), "target.dxf", None);
    moved.execute("import", &json!({"data": base64(SOURCE.as_bytes()), "name": "source.dxf", "at": [10, 0]})).unwrap();
    assert!(bits(&moved, "LINE", 210).is_empty());
}

#[test]
fn the_insertion_of_a_file_keeps_the_source_geometry() {
    let mut s = session();
    let path = std::env::temp_dir().join(format!("cadcraft-snapshot-insert-{}.dxf", std::process::id()));
    std::fs::write(&path, SOURCE).unwrap();
    s.open_drawing(cadcraft_doc::Drawing::new_imperial(), "target.dxf", None);
    s.execute("insert", &json!({"file": path.to_string_lossy(), "at": [0, 0]})).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_planes(&s, "file INSERT");
}

#[test]
fn the_clipboard_keeps_the_source_geometry() {
    let mut s = session();
    let handles: Vec<_> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("copyclip", &json!({"handles": handles})).unwrap();
    s.open_drawing(cadcraft_doc::Drawing::new_imperial(), "target.dxf", None);
    s.execute("pasteorig", &json!({})).unwrap();
    assert_planes(&s, "PASTEORIG");
    // Through the system clipboard: DXF text, loaded by another session.
    let text = system_text(&s).unwrap().expect("clipboard text");
    let mut other = Session::new();
    assert!(load_system_text(&mut other, &text));
    other.open_drawing(cadcraft_doc::Drawing::new_imperial(), "target.dxf", None);
    other.execute("pasteorig", &json!({})).unwrap();
    assert_planes(&other, "system clipboard");
}

/// How many LINE records of the saved active drawing have the source's plane.
fn lines_with_the_plane(s: &Session) -> usize {
    let text = cadcraft_io::write_dxf(s.doc().unwrap());
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let entities = sections(&tags).into_iter().find(|sec| sec.name == "ENTITIES").map(|sec| records(&sec.tags)).unwrap_or_default();
    entities
        .iter()
        .filter(|(k, t)| {
            k == "LINE" && t.iter().any(|t| t.code == 210 && t.str().trim().parse::<f64>().unwrap().to_bits() == 6.3141e-10f64.to_bits())
        })
        .count()
}

#[test]
fn a_move_is_a_geometry_edit_and_a_copy_in_place_is_not() {
    let mut s = session();
    let line = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
    s.execute("copy", &json!({"handles": [line.clone()], "delta": [0, 0]})).unwrap();
    assert_eq!(lines_with_the_plane(&s), 2, "a copy in place keeps the plane, as the source does");
    s.execute("move", &json!({"handles": [line], "delta": [3, 0]})).unwrap();
    assert_eq!(lines_with_the_plane(&s), 1, "the moved line is written from the model");
}
