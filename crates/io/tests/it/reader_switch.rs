//! Plan v3, section 9: the rollback switch. One setting, `ACTIVATION_DEFAULT`, overridden by `CADCRAFT_READER` (`off`, `safe` or
//! `all`), becomes a `ReaderPolicy` that the reader takes. At stage 0 no family is active, so every mode reads as the reader did
//! before.

use cadcraft_io::activation::{ACTIVATION_DEFAULT, ACTIVE_FAMILIES, Family, FamilySet, READER_ENV, ReaderMode, ReaderPolicy, WIRED_FAMILIES};

const MODES: [ReaderMode; 3] = [ReaderMode::Off, ReaderMode::Safe, ReaderMode::All];

#[test]
fn the_setting_names_three_modes_in_any_case() {
    for (text, mode) in [("off", ReaderMode::Off), (" Safe ", ReaderMode::Safe), ("ALL", ReaderMode::All)] {
        assert_eq!(ReaderMode::parse(text), Ok(mode), "{text:?}");
    }
}

#[test]
fn any_other_setting_is_an_error_that_names_the_variable_and_the_modes() {
    for text in ["", "none", "on", "safe,all", "1"] {
        let error = ReaderMode::parse(text).unwrap_err().to_string();
        assert!(error.contains(READER_ENV) && error.contains("off") && error.contains("safe") && error.contains("all"), "{error}");
    }
}

#[test]
fn the_environment_overrides_the_build_default() {
    assert_eq!(ReaderPolicy::resolve(None).unwrap().mode(), ACTIVATION_DEFAULT);
    for mode in MODES {
        let text = match mode {
            ReaderMode::Off => "off",
            ReaderMode::Safe => "safe",
            ReaderMode::All => "all",
        };
        assert_eq!(ReaderPolicy::resolve(Some(text)).unwrap().mode(), mode);
    }
    assert!(ReaderPolicy::resolve(Some("sometimes")).is_err());
}

#[test]
fn a_mode_decides_which_of_the_active_families_the_reader_turns_on() {
    let active = FamilySet::of(&[Family::Line, Family::Circle]);
    assert_eq!(ReaderPolicy::new(ReaderMode::Off, active).mask(), FamilySet::NONE);
    assert_eq!(ReaderPolicy::new(ReaderMode::Safe, active).mask(), active);
    assert_eq!(ReaderPolicy::new(ReaderMode::All, active).mask(), active);
    // Only `all` reads a record that Legacy loses nothing from as Canonical too.
    assert!(!ReaderPolicy::new(ReaderMode::Off, active).reads_plain_canonical());
    assert!(!ReaderPolicy::new(ReaderMode::Safe, active).reads_plain_canonical());
    assert!(ReaderPolicy::new(ReaderMode::All, active).reads_plain_canonical());
}

#[test]
fn stage_0_activates_nothing_and_every_active_family_has_a_reader() {
    assert_eq!(ACTIVE_FAMILIES, FamilySet::NONE);
    assert!(ACTIVE_FAMILIES.is_subset_of(WIRED_FAMILIES));
    for mode in MODES {
        assert_eq!(ReaderPolicy::new(mode, ACTIVE_FAMILIES).mask(), FamilySet::NONE);
    }
}

#[test]
fn production_reads_the_same_in_every_mode() {
    for (name, bytes) in [
        ("apartment.dxf", include_bytes!("../../../../examples/apartment.dxf").as_slice()),
        ("attrib_fields.dxf", include_bytes!("../data/attrib_fields.dxf").as_slice()),
        ("clockwise_arc_edge.dwg", include_bytes!("../data/clockwise_arc_edge.dwg").as_slice()),
    ] {
        let before = cadcraft_io::read(bytes, name).unwrap();
        for mode in MODES {
            let policy = ReaderPolicy::new(mode, ACTIVE_FAMILIES);
            let drawing = cadcraft_io::read_with_policy(bytes, name, policy).unwrap();
            assert_eq!(drawing, before, "{name} in {mode:?}");
        }
    }
}

#[test]
fn a_policy_that_asks_for_a_family_without_a_reader_is_refused() {
    let policy = ReaderPolicy::new(ReaderMode::Safe, FamilySet::of(&[Family::Line]));
    let error = cadcraft_io::read_with_policy(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n", "x.dxf", policy).unwrap_err().to_string();
    assert!(error.contains("no canonical reader") && error.contains("Line"), "{error}");
    // The same policy with the mode off turns nothing on.
    let off = ReaderPolicy::new(ReaderMode::Off, FamilySet::of(&[Family::Line]));
    assert!(cadcraft_io::read_with_policy(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n", "x.dxf", off).is_ok());
}
