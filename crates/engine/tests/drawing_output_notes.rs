use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

#[test]
fn exports_and_layout_templates_report_real_dwg_omissions() {
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
        session.log.clear();
        let result = session.execute(command, &params).unwrap();
        assert!(result["warnings"].as_array().unwrap().iter().any(|note| note.as_str().unwrap().contains("DWG: not saved: TESTCLASS")), "{command}");
        assert!(session.log.iter().any(|line| line.contains("DWG: not saved: TESTCLASS")), "{command}: session log");
    }
    std::fs::remove_file(path).unwrap();
}
