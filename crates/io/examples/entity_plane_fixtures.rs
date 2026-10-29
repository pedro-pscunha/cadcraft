//! Export authored AutoCAD reference inputs. Native results remain outside git.

#[path = "../tests/it/entity_plane_manifest.rs"]
mod fixtures;

use std::io::Write;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let output = args.next().ok_or("expected output directory and remote directory")?;
    let remote = args.next().and_then(|value| value.into_string().ok()).ok_or("expected a UTF-8 remote directory")?;
    if args.next().is_some() {
        return Err("expected exactly two arguments".into());
    }
    let exported = fixtures::export_all(std::path::Path::new(&output), &remote)?;
    let mut output = std::io::stdout().lock();
    for path in exported.scripts {
        writeln!(output, "EXPORTED {}", path.display()).map_err(|error| error.to_string())?;
    }
    for id in exported.requires_ui {
        writeln!(output, "REQUIRES_NATIVE_UI {id}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
