use cadcraft_dxf::{Tag, ascii_to_code_page, decode_code_page, single_byte_code_page, write_ascii};

#[test]
fn windows_single_byte_pages_encode_representable_characters_as_bytes() {
    for (page, value) in [
        ("ANSI_1250", "Ł"),
        ("ANSI_1251", "Ж"),
        ("ANSI_1252", "ã"),
        ("ANSI_1253", "Ω"),
        ("ANSI_1254", "ğ"),
        ("ANSI_1255", "א"),
        ("ANSI_1256", "ع"),
        ("ANSI_1257", "ė"),
        ("ANSI_1258", "ơ"),
        ("ANSI_874", "ก"),
    ] {
        assert_eq!(single_byte_code_page(page), page);
        let ascii = write_ascii(&[Tag::s(1, value)]);
        let encoded = ascii_to_code_page(&ascii, page);
        assert!(!encoded.windows(3).any(|w| w == b"\\U+"), "{page} must encode {value} as a byte");
        assert_eq!(decode_code_page(&encoded, page).unwrap(), format!("  1\r\n{value}\r\n"));
    }
}

#[test]
fn escaped_backslashes_protect_literal_unicode_text_in_bridge_encoding() {
    for n in 1..=4 {
        let value = format!("{}U+00E9", "\\".repeat(n));
        let expected = if n % 2 == 0 { value.clone() } else { format!("{}é", "\\".repeat(n - 1)) };
        let encoded = ascii_to_code_page(&value, "ANSI_1252");
        assert_eq!(decode_code_page(&encoded, "ANSI_1252").unwrap(), expected, "backslash run {n}");
    }
}

#[test]
fn escaped_backslashes_protect_literal_unicode_text_when_reading() {
    for n in 1..=4 {
        let value = format!("{}U+00E9", "\\".repeat(n));
        let expected = if n % 2 == 0 { value.clone() } else { format!("{}é", "\\".repeat(n - 1)) };
        let tags = cadcraft_dxf::parse_ascii(&format!("0\nMTEXT\n1\n{value}\n0\nEOF\n")).unwrap();
        assert_eq!(tags.iter().find(|t| t.code == 1).unwrap().str(), expected, "backslash run {n}");
    }
}

#[test]
fn unsupported_pages_fall_back_to_1252_and_leave_unmappable_characters_escaped() {
    for page in ["UTF-8", "ANSI_932", "unknown"] {
        assert_eq!(single_byte_code_page(page), "ANSI_1252");
        let ascii = write_ascii(&[Tag::s(1, "ãЖ😀")]);
        let encoded = ascii_to_code_page(&ascii, page);
        let decoded = decode_code_page(&encoded, "ANSI_1252").unwrap();
        assert_eq!(decoded, "  1\r\nã\\U+0416\\U+D83D\\U+DE00\r\n");
    }
    assert_eq!(single_byte_code_page(" ansi-1251 "), "ANSI_1251");
}
