//! The read fixtures of the activation plan (`crates/io/tests/data/activation/*.dxf`) as the io and engine tests read them: the folder, the
//! static section 3 predicate that says which records need Canonical, and a read with the raw path on.
//!
//! Readability is independent of Legacy losslessness. Some fixtures inject a failure (an `UNREADABLE` xdata mark) or an inconsistent
//! SPLINE count, without calling a canonical reader in the predicate.

use std::path::{Path, PathBuf};

use cadcraft_doc::Drawing;
use cadcraft_dxf::{Section, Tag, Version, parse, sections};
use cadcraft_io::activation::records::RecordView;
use cadcraft_io::activation::{FamilySet, Need, ReaderMode, ReaderPolicy};

pub(crate) fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../io/tests/data/activation")
}

fn group(groups: &[Tag], code: i32) -> Option<f64> {
    groups.iter().find(|t| t.code == code).map(Tag::f64)
}

pub(crate) fn predicate(sections: &[Section]) -> impl Fn(&RecordView<'_>) -> Need + use<> {
    use cadcraft_io::{LegacyClassifier, LegacyDisposition, LegacyRecordClassification};
    let classifier = LegacyClassifier::new(sections, Version::R2013);
    move |view| {
        match classifier.record(view) {
            LegacyRecordClassification::OwnerDerived(_) => return Need::Plain,
            LegacyRecordClassification::Authored(classification) if classification.disposition == LegacyDisposition::Plain => {
                return Need::Plain;
            }
            LegacyRecordClassification::Authored(_) => {}
        }
        let forced = view.groups.iter().any(|t| t.code == 1000 && t.str() == "UNREADABLE");
        let readable = !forced
            && (view.kind != "SPLINE" || group(view.groups, 73).is_some_and(|n| n as usize == view.groups.iter().filter(|t| t.code == 10).count()));
        Need::Needs { readable }
    }
}

pub(crate) fn source_predicate(bytes: &[u8]) -> impl Fn(&RecordView<'_>) -> Need + use<> {
    predicate(&sections(&parse(bytes).unwrap()))
}

/// The policy of a read that keeps what Legacy cannot hold as Raw units.
pub(crate) fn raw_policy() -> ReaderPolicy {
    ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE).with_raw_path()
}

/// The fixture `name` read with the raw path on, and its text.
pub(crate) fn read_fixture(name: &str) -> (Drawing, String) {
    let text = std::fs::read_to_string(dir().join(format!("{name}.dxf"))).unwrap_or_else(|e| panic!("{name}: {e}"));
    (cadcraft_io::read_with_need(text.as_bytes(), "x.dxf", raw_policy(), &source_predicate(text.as_bytes())).unwrap(), text)
}
