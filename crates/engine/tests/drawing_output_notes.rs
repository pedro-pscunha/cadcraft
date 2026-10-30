use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

#[test]
fn exports_and_layout_templates_refuse_a_dwg_that_would_omit_an_object() {
    set_io(IoHooks {
        read: |bytes, path| cadcraft_io::read(bytes, path).map_err(|e| e.to_string()),
        write: |d, path| cadcraft_io::write(d, path).map_err(|e| e.to_string()),
        plot: None,
    });
    set_write_noted(|d, path, opts| {
        cadcraft_io::write_with_noted(d, path, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary }).map_err(|e| e.to_string())
    });
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nTest\n350\nAA\n0\nTESTCLASS\n5\nAA\n330\nC\n100\nAcDbTestClass\n1\nopaque\n0\nENDSEC\n0\nEOF\n";
    let mut session = Session::new();
    session.open_drawing(cadcraft_io::read_dxf(source).unwrap(), "source.dxf", None);
    let layout = session.doc().unwrap().layouts[0].name.clone();
    let path = std::env::temp_dir().join(format!("cadcraft-output-notes-{}.dwg", std::process::id()));
    for (command, params) in [
        ("export", json!({"format": "dwg"})),
        ("export", json!({"format": "dwg", "path": path})),
        ("layout.saveas", json!({"name": layout, "path": path})),
    ] {
        let error = session.execute(command, &params).unwrap_err().to_string();
        assert!(error.contains("DWG save refused") && error.contains("TESTCLASS (AA)"), "{command}: {error}");
        assert!(!path.exists(), "{command}: a refused save writes no file");
    }
}
