//! A save gives every object of the file it kept (an extension dictionary and its XRECORD, a table style, a scale, ...) the handle the
//! file gave it, and the owners and entries that name it follow. An object CADCraft adds gets a handle above them all.

use super::foreign_passthrough::{saved_twice, section, styled_objects, val};

/// The handle, owner and (for a dictionary) entry targets of the object of `kind` whose group `code` is `value`.
fn object(text: &str, kind: &str, code: i32, value: &str) -> (String, String, Vec<String>) {
    let (_, tags) = section(text, "OBJECTS")
        .into_iter()
        .find(|(k, t)| k == kind && val(t, code) == value)
        .unwrap_or_else(|| panic!("no {kind} with group {code} {value}: {text}"));
    let owner = tags.iter().filter(|t| t.code == 330).last().map(|t| t.str()).unwrap_or_default();
    (val(&tags, 5), owner, tags.iter().filter(|t| t.code == 350).map(|t| t.str()).collect())
}

#[test]
fn kept_objects_keep_their_handles_and_their_links() {
    let d = cadcraft_io::read_dxf(styled_objects().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        // The XRECORD of a layout's extension dictionary: handle 33, owned by the dictionary 32, which the layout (31) names.
        let (xrecord, owner, _) = object(&text, "XRECORD", 1, "thumbnail");
        assert_eq!((xrecord.as_str(), owner.as_str()), ("33", "32"), "{what}: XRECORD");
        let (dictionary, owner, entries) = object(&text, "DICTIONARY", 3, "ADSK_XREC_LAYOUTTHUMBNAIL");
        assert_eq!((dictionary.as_str(), owner.as_str(), entries.as_slice()), ("32", "31", ["33".to_string()].as_slice()), "{what}: dictionary");
        // The same for the style's extension dictionaries and the scale.
        for (text_of, handle, owner) in [("roundtrip", "43", "42"), ("cellstylemap", "53", "52")] {
            let (h, o, _) = object(&text, "XRECORD", 1, text_of);
            assert_eq!((h.as_str(), o.as_str()), (handle, owner), "{what}: {text_of}");
        }
        let (scale, _, _) = object(&text, "SCALE", 300, "A0");
        assert_eq!(scale, "61", "{what}: scale");
    }
}
