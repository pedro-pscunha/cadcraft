use std::path::{Path, PathBuf};

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Common, Drawing, Entity, RawCompound, RawRecord, RawTag};
use cadcraft_mcp::{Backend, Headless};
use serde_json::json;

fn session(paper: bool) -> Session {
    super::install_io();
    let tags = [(5, "200"), (330, "0"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbCircle"), (10, "0"), (20, "0"), (40, "1"), (340, "90")]
        .into_iter()
        .map(|(code, value)| RawTag { code, value: value.into() })
        .collect();
    let raw =
        RawCompound::new(RawRecord::new("CIRCLE".into(), tags).unwrap(), vec![], Common::default(), "Synthetic required-reference fixture".into())
            .unwrap();
    let mut drawing = Drawing::new_metric();
    if paper {
        drawing.layouts[0].entities.push(Entity::from_raw(raw));
    } else {
        drawing.model.push(Entity::from_raw(raw));
    }
    let mut session = Session::empty();
    session.open_drawing(drawing, "Raw integrity", None);
    session
}

fn destination(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cadcraft-raw-save-routes-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("protected.dxf");
    std::fs::write(&path, b"original destination").unwrap();
    path
}

fn unchanged(path: &Path) {
    assert_eq!(std::fs::read(path).unwrap(), b"original destination");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn production_cli_hooks_refuse_raw_loss_before_save_export_or_byte_delivery() {
    for command in ["qsave", "saveas", "export", "document.bytes", "layout.saveas", "layout"] {
        let mut session = session(matches!(command, "layout.saveas" | "layout"));
        let path = destination(command);
        let error =
            session.execute(command, &json!({ "path": path, "format": "dxf", "name": "Layout1", "option": "saveas" })).unwrap_err().to_string();
        assert!(error.contains("Save refused for raw unit") && error.contains("340"), "{command}: {error}");
        unchanged(&path);
    }
}

#[test]
fn the_save_command_alias_uses_the_checked_writer() {
    let mut session = session(false);
    let path = destination("save-alias");
    session.state_mut().unwrap().path = Some(path.to_string_lossy().into_owned());
    let error = session.cmdline("save").unwrap_err().to_string();
    assert!(error.contains("Save refused for raw unit") && error.contains("340"), "{error}");
    unchanged(&path);
}

#[test]
fn cli_convert_and_run_export_use_the_checked_file_writer() {
    let session = session(false);
    for framed in [false, true] {
        let path = destination(if framed { "cli-framed" } else { "cli-export" });
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut streams = super::output::Streams::new(&mut out, &mut err);
        let result = if framed {
            super::output::export_framed(&session, &path, None, &mut streams)
        } else {
            super::output::export(&session, &path, &mut streams)
        };
        assert!(result.unwrap_err().to_string().contains("Save refused for raw unit"));
        unchanged(&path);
    }
}

#[test]
fn mcp_save_and_execute_use_the_checked_engine_writer() {
    for method in ["app.save", "engine.execute", "command"] {
        let mut backend = Headless { session: session(false) };
        let path = destination(method);
        let params =
            if method == "app.save" { json!({ "path": path }) } else { json!({ "command": "export", "params": { "path": path, "format": "dxf" } }) };
        let error = backend.call(method, params).unwrap_err();
        assert!(error.contains("Save refused for raw unit") && error.contains("340"), "{method}: {error}");
        unchanged(&path);
    }
}

#[test]
fn each_public_dxf_byte_entry_point_checks_raw_integrity() {
    let session = session(false);
    let drawing = session.doc().unwrap();
    assert!(cadcraft_io::write(drawing, "raw.dxf").is_err());
    assert!(cadcraft_io::write_with(drawing, "raw.dxf", &cadcraft_io::SaveOptions::default()).is_err());
    assert!(cadcraft_io::write_with_noted(drawing, "raw.dxf", &cadcraft_io::SaveOptions::default()).is_err());
    assert!(cadcraft_io::write_framed(drawing, "raw.dxf", None).is_err());
}

#[test]
fn desktop_cli_and_wasm_hosts_register_only_checked_byte_writers() {
    for source in [include_str!("lib.rs"), include_str!("../../cadcraft/src/main.rs"), include_str!("../../cadcraft-web/src/main.rs")] {
        let source: String = source.split_whitespace().collect();
        for call in ["write:|d,name|cadcraft_io::write(d,name)", "cadcraft_io::write_with_noted(d,name,", "cadcraft_io::write_with(d,name,"] {
            assert!(source.contains(call), "a production host no longer registers {call}");
        }
    }
    let save = include_str!("../../../crates/engine/src/cmd/file.rs");
    assert!(save.contains("let (bytes, notes) = write_bytes(s, cmd, path, opts)?;"));
    assert!(save.contains("let n = commit_save(s, path, &bytes, store)?;"));
    assert!(save.contains("deliver(&name, \"application/octet-stream\", bytes)"));
    let web = include_str!("../../cadcraft-web/src/main.rs");
    assert!(web.contains("set_deliver(crate::download::download)"));
    assert!(web.contains("set_deliver(crate::host::deliver)"));
    let download = include_str!("../../cadcraft-web/src/download.rs");
    assert!(download.contains("pub fn download(name: &str, mime: &str, bytes: &[u8])"));
    assert!(!download.contains("cadcraft_io"), "the browser sink must only receive completed bytes");
    assert!(include_str!("../../../crates/engine/src/cmd/blocks.rs").contains("super::file::write_document(&d, \"wblock\""));
    assert!(include_str!("../../../crates/engine/src/cmd/clipboard.rs").contains("(hooks.write)(&d, \"clipboard.dxf\")"));
    assert!(include_str!("../../../crates/engine/src/autosave.rs").contains("crate::cmd::file::io().map(|h| h.write)"));
    for source in [include_str!("sample.rs"), include_str!("perf.rs")] {
        assert!(source.contains("cadcraft_io::write("));
    }
    for source in [include_str!("convert.rs"), include_str!("run.rs")] {
        assert!(source.contains("export_framed("));
    }
    assert!(include_str!("../../../crates/mcp/src/tools.rs").contains("\"save\" => b.call(\"app.save\", a)"));
}

fn unchecked_callers(dir: &Path, root: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let path = entry.path();
        if kind.is_dir() {
            if !["tests", "target", "examples", "benches"].contains(&entry.file_name().to_string_lossy().as_ref()) {
                unchecked_callers(&path, root, found);
            }
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path.strip_prefix(root).unwrap();
            let name = relative.to_string_lossy();
            let test_only = name.ends_with("/tests.rs")
                || name.ends_with("_tests.rs")
                || name.ends_with("/raw_save_routes.rs")
                || name.starts_with("crates/io/src/dxf_read/legacy_equivalence/oracle")
                || name == "crates/io/src/dxf_read/legacy_equivalence/generated.rs";
            if !test_only && name != "crates/io/src/lib.rs" && std::fs::read_to_string(&path).unwrap().contains("write_dxf") {
                found.push(relative.to_path_buf());
            }
        }
    }
}

#[test]
fn no_production_module_reaches_the_unchecked_test_serializer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for tree in ["apps", "crates"] {
        unchecked_callers(&root.join(tree), &root, &mut found);
    }
    assert!(found.is_empty(), "unchecked production DXF callers: {found:?}");
}
