//! The direct CLI save refuses a DWG that would leave out an object the DWG writer cannot write.
use std::process::Command;

#[test]
fn run_save_refuses_a_dwg_that_would_omit_an_object() {
    let dir = std::env::temp_dir().join(format!("cc-cli-omissions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.dxf");
    let saved = dir.join("saved.dwg");
    std::fs::write(
        &source,
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nTest\n350\nAA\n\
0\nTESTCLASS\n5\nAA\n330\nC\n100\nAcDbTestClass\n1\nopaque\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    let result =
        Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(["run", source.to_str().unwrap(), "--save", saved.to_str().unwrap()]).output().unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success(), "{stderr}");
    assert!(stderr.contains("DWG save refused") && stderr.contains("TESTCLASS (AA)"), "{stderr}");
    assert!(!saved.exists(), "a refused save writes no file");
    std::fs::remove_dir_all(dir).unwrap();
}
