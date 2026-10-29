use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

#[test]
fn a_save_reports_each_writer_omission() {
    set_io(IoHooks { read: |_, _| Err("unused".into()), write: |_, _| Ok(vec![1]), plot: None });
    set_write_noted(|_, _, _| Ok((vec![1], vec!["DWG: not saved: TESTCLASS (A1): source version differs".into()])));
    let mut session = Session::new();
    let result = session.execute("document.bytes", &json!({"format": "dwg"})).unwrap();
    assert_eq!(result["warnings"][0], "DWG: not saved: TESTCLASS (A1): source version differs");
    assert!(session.log.iter().any(|line| line.contains("TESTCLASS (A1)")));
    let path = std::env::temp_dir().join(format!("cadcraft-omission-{}.dwg", std::process::id()));
    let result = session.execute("saveas", &json!({"path": path})).unwrap();
    assert_eq!(result["warnings"][0], "DWG: not saved: TESTCLASS (A1): source version differs");
    let result = session.execute("qsave", &json!({})).unwrap();
    assert_eq!(result["warnings"][0], "DWG: not saved: TESTCLASS (A1): source version differs");
    std::fs::remove_file(path).unwrap();
}
