//! Converts a DWG to the DXF rendition that CADCraft reads it as (`cadcraft_dwg::dwg_to_dxf_noted`), for comparing a saved DWG with
//! its source group by group (the save-equality compare tool):
//!
//! ```sh
//! cargo run -p cadcraft-dwg --example dwg2dxf -- <in.dwg> <out.dxf>
//! ```
//!
//! The notes about what the read left out go to standard error, one per line. The size limits are lifted: the files compared are
//! the ones the tool is asked about.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(input), Some(output), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: dwg2dxf <in.dwg> <out.dxf>");
        return ExitCode::from(2);
    };
    match convert(Path::new(&input), Path::new(&output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn convert(input: &Path, output: &Path) -> Result<(), String> {
    let bytes = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let limits = cadcraft_dwg::Limits { max_dwg_bytes: usize::MAX, max_dxf_bytes: usize::MAX };
    let (dxf, notes) = cadcraft_dwg::dwg_to_dxf_noted(&bytes, limits)?;
    for note in notes {
        eprintln!("note: {note}");
    }
    std::fs::write(output, dxf).map_err(|e| format!("{}: {e}", output.display()))
}
