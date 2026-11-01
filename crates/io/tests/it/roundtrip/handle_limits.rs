//! Exhausting generated-handle space may be refused, but must not silently lose authored
//! entities. This is separate from the preservation matrix's large handles with free capacity.
//! Neither a particular allocator strategy nor a successful save is required at exhaustion.

use cadcraft_io::{SaveOptions, Version, read, write_with};

use super::{cases::properties, compare};

fn preserves_or_refuses(path: &str, binary: bool) {
    let original = properties::exhausted_handles().drawing;
    for version in Version::ALL {
        let options = SaveOptions { version: Some(version), binary };
        let mut current = original.clone();
        for pass in 1..=2 {
            let context = format!("exhausted handles · {path} {version} binary={binary} pass={pass}");
            let Ok(bytes) = write_with(&current, path, &options) else {
                break; // An explicit save error is safe; a nominal success must preserve the data.
            };
            let back = read(&bytes, path).unwrap_or_else(|e| panic!("{context}: successful save is unreadable: {e}"));
            assert!(back.read_notes.is_empty(), "{context}: {:?}", back.read_notes);
            assert!(back.skipped_records.is_empty(), "{context}: {:?}", back.skipped_records);
            compare::model(&original, &back, &context);
            current = back;
        }
    }
}

#[test]
fn dxf_exhausted_handles_are_preserved_or_refused() {
    for binary in [false, true] {
        preserves_or_refuses("memory.dxf", binary);
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn dwg_exhausted_handles_are_preserved_or_refused() {
    preserves_or_refuses("memory.dwg", false);
}

// The fork reserves HANDLE_ROOM above the largest handle. AutoCAD never reuses a handle
// below $HANDSEED, so insufficient space is an explicit refusal, not wraparound allocation.
pub(super) fn reserved_handles_refuse(path: &str, binary: bool) {
    let cases: Vec<_> = properties::handles_near_integer_limits().into_iter().filter(|case| !case.drawing.has_handle_room()).collect();
    assert!(!cases.is_empty());
    for case in cases {
        for version in Version::ALL {
            let error = write_with(&case.drawing, path, &SaveOptions { version: Some(version), binary }).unwrap_err();
            assert!(
                error.to_string().contains("the drawing's handles reach the largest handle: there is no room above them for a save"),
                "{} {path} {version}: {error}",
                case.name
            );
        }
    }
}
