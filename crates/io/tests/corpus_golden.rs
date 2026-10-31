//! G1, the frozen baseline of the reader activation (plan v3, section 7). `corpus.toml` lists the drawings
//! the activation is proved on (path, owner, the activation families they hold); `golden.sha256` holds the
//! hash of the DXF and the DWG that a read and a save of each makes at the pre-activation sha, and the
//! outputs themselves are kept, compressed, in `CADCRAFT_GOLDEN_DIR` (default `/tmp/cad/activation/golden`)
//! so that a drift can be named by record. With the families of a stage OFF, every record of the save
//! outside them is byte-equal to its golden, and so is every record of a file with all families OFF.
//!
//! The check reads and saves drawings of up to 35 MB (minutes in all), so it is ignored by default:
//!
//! ```text
//! cargo test -p cadcraft-io --test corpus_golden -- --ignored match --nocapture
//! ```
//!
//! A save-fidelity change that moves the output on purpose refreshes the baseline with
//! `write_golden_set` (`CADCRAFT_G1_WRITE=1`, and `-- --ignored write_golden_set`), in the commit that makes it. The filter `match` keeps the check
//! from running `write_golden_set`, which rewrites the baseline.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_io::activation::records::family_of_kind;
use cadcraft_io::activation::{Family, FamilySet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The families the stage under test turns ON: their records may differ from the golden. Before the
/// activation nothing is on, and nothing may differ.
const ON: FamilySet = FamilySet::NONE;

#[derive(Serialize, Deserialize)]
struct Corpus {
    file: Vec<Entry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    /// Absolute, or relative to the repository root.
    path: String,
    owner: String,
    /// The activation families the file holds (the entities of its ENTITIES and BLOCKS sections).
    families: Vec<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus.toml")
}

fn hashes_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden.sha256")
}

fn golden_dir() -> PathBuf {
    std::env::var_os("CADCRAFT_GOLDEN_DIR").map_or_else(|| PathBuf::from("/tmp/cad/activation/golden"), PathBuf::from)
}

fn corpus() -> Vec<Entry> {
    toml::from_str::<Corpus>(&std::fs::read_to_string(corpus_path()).expect("corpus.toml")).expect("corpus.toml parses").file
}

fn resolve(entry: &Entry) -> PathBuf {
    let path = Path::new(&entry.path);
    if path.is_absolute() { path.to_path_buf() } else { root().join(path) }
}

/// The name of an entry's golden files.
fn slug(entry: &Entry) -> String {
    entry.path.trim_start_matches('/').chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Format {
    Dxf,
    Dwg,
}

impl Format {
    const ALL: [Format; 2] = [Format::Dxf, Format::Dwg];

    fn name(self) -> &'static str {
        match self {
            Format::Dxf => "dxf",
            Format::Dwg => "dwg",
        }
    }
}

/// What a read and a save of the file makes, `None` when the file is not on this machine.
fn saves(entry: &Entry) -> Option<BTreeMap<Format, Vec<u8>>> {
    let path = resolve(entry);
    let bytes = std::fs::read(&path).ok()?;
    let drawing = cadcraft_io::read(&bytes, &path).unwrap_or_else(|e| panic!("{}: {e}", entry.path));
    let dxf = cadcraft_io::write_dxf(&drawing).into_bytes();
    let dwg = cadcraft_io::write(&drawing, "golden.dwg").unwrap_or_else(|e| panic!("{}: DWG save: {e}", entry.path));
    Some(BTreeMap::from([(Format::Dxf, dxf), (Format::Dwg, dwg)]))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The hash list: `(path, format)` → (sha256, bytes).
fn read_hashes() -> BTreeMap<(String, String), (String, usize)> {
    let text = std::fs::read_to_string(hashes_path()).unwrap_or_default();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.splitn(4, ' ');
            let (hash, format, bytes, path) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
            ((path.to_string(), format.to_string()), (hash.to_string(), bytes.parse().unwrap()))
        })
        .collect()
}

type Record = (String, Vec<Tag>);

/// Every record of a DXF text with a key that names it: the section, the type and the handle, else the place among its kind.
fn keyed_records(text: &str) -> BTreeMap<String, Record> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).expect("a DXF text");
    let mut out = BTreeMap::new();
    for section in sections(&tags) {
        let mut ordinal: BTreeMap<String, usize> = BTreeMap::new();
        for (kind, body) in records(&section.tags) {
            let handle = body.iter().find(|t| matches!(t.code, 5 | 105)).map(Tag::str);
            let key = match handle {
                Some(h) if section.name != "HEADER" => format!("{}/{kind}/{h}", section.name),
                _ => {
                    let n = ordinal.entry(kind.clone()).or_default();
                    *n += 1;
                    format!("{}/{kind}/#{n}", section.name)
                }
            };
            out.insert(key, (kind, body));
        }
    }
    out
}

fn dxf_text(format: Format, bytes: &[u8]) -> String {
    match format {
        Format::Dxf => String::from_utf8_lossy(bytes).into_owned(),
        Format::Dwg => String::from_utf8_lossy(&cadcraft_dwg::dwg_to_dxf(bytes).expect("a DWG that reads")).into_owned(),
    }
}

fn same(a: &Record, b: &Record) -> bool {
    a.0 == b.0 && a.1.len() == b.1.len() && a.1.iter().zip(&b.1).all(|(x, y)| x.code == y.code && x.str() == y.str())
}

/// The records of `new` that differ from `golden` (or are not in it, or are missing), by type, that a stage with
/// the families of [`ON`] may not change.
fn forbidden_drift(format: Format, golden: &[u8], new: &[u8]) -> Vec<String> {
    let (old, new) = (keyed_records(&dxf_text(format, golden)), keyed_records(&dxf_text(format, new)));
    let may_differ =
        |kind: &str, key: &str| (key.starts_with("ENTITIES/") || key.starts_with("BLOCKS/")) && family_of_kind(kind).is_some_and(|f| ON.contains(f));
    let mut drift = Vec::new();
    for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
        let differs = match (old.get(key), new.get(key)) {
            (Some(a), Some(b)) => !same(a, b),
            _ => true,
        };
        let kind = old.get(key).or_else(|| new.get(key)).map_or("", |r| r.0.as_str());
        if differs && !may_differ(kind, key) {
            drift.push(key.clone());
        }
    }
    drift
}

fn golden_file(entry: &Entry, format: Format) -> PathBuf {
    golden_dir().join(format!("{}.{}.z", slug(entry), format.name()))
}

fn read_golden(entry: &Entry, format: Format) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib(&std::fs::read(golden_file(entry, format)).ok()?).ok()
}

/// The families an entry's save holds, by their names.
fn families_of(dxf: &[u8]) -> Vec<String> {
    let mut found: BTreeSet<Family> = BTreeSet::new();
    let tags = cadcraft_dxf::parse(dxf).expect("a DXF text");
    for section in sections(&tags).into_iter().filter(|s| matches!(s.name.as_str(), "ENTITIES" | "BLOCKS")) {
        found.extend(records(&section.tags).into_iter().filter_map(|(kind, _)| family_of_kind(&kind)));
    }
    let mut names: Vec<String> = found.into_iter().map(|f| format!("{f:?}")).collect();
    names.sort();
    names
}

/// `CADCRAFT_GOLDEN_OWNER` restricts a run to the files of one owner.
fn check(owner_is: impl Fn(&str) -> bool) {
    let only = std::env::var("CADCRAFT_GOLDEN_OWNER").ok();
    let owner_is = |owner: &str| owner_is(owner) && only.as_deref().is_none_or(|o| o == owner);
    let hashes = read_hashes();
    let (mut checked, mut missing) = (0, Vec::new());
    let mut failures = Vec::new();
    for entry in corpus().into_iter().filter(|e| owner_is(&e.owner)) {
        let Some(outputs) = saves(&entry) else {
            missing.push(entry.path.clone());
            continue;
        };
        checked += 1;
        if let Some(dxf) = outputs.get(&Format::Dxf) {
            let mut declared = entry.families.clone();
            declared.sort();
            if declared != families_of(dxf) {
                failures.push(format!("{}: families in corpus.toml are {declared:?}, the save holds {:?}", entry.path, families_of(dxf)));
            }
        }
        for format in Format::ALL {
            let bytes = &outputs[&format];
            let Some((hash, _)) = hashes.get(&(entry.path.clone(), format.name().to_string())) else {
                failures.push(format!("{} {}: not in golden.sha256 (refresh with write_golden_set)", entry.path, format.name()));
                continue;
            };
            if &sha256(bytes) == hash {
                continue;
            }
            match read_golden(&entry, format) {
                None => {
                    failures.push(format!("{} {}: the output changed and there is no golden file to name the records", entry.path, format.name()))
                }
                Some(golden) => {
                    let drift = forbidden_drift(format, &golden, bytes);
                    if !drift.is_empty() {
                        failures.push(format!(
                            "{} {}: {} records drifted, first: {:?}",
                            entry.path,
                            format.name(),
                            drift.len(),
                            &drift[..drift.len().min(5)]
                        ));
                    }
                }
            }
        }
    }
    eprintln!("G1: {checked} drawings checked, {} not on this machine: {missing:?}", missing.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "reads and saves the corpus (minutes): run it at every activation stage"]
fn the_repository_drawings_match_their_golden() {
    check(|owner| owner == "repo");
}

#[test]
#[ignore = "reads and saves the corpus (minutes): run it at every activation stage"]
fn every_corpus_drawing_on_this_machine_matches_its_golden() {
    check(|_| true);
}

/// Makes the baseline: the golden outputs, the hash list and the families column of `corpus.toml`. `CADCRAFT_GOLDEN_SHA`
/// names the sha it is made at; `CADCRAFT_GOLDEN_OWNER` restricts the run to the files of one owner.
#[test]
#[ignore = "rewrites tests/golden.sha256, tests/corpus.toml and the golden directory (needs CADCRAFT_G1_WRITE=1)"]
fn write_golden_set() {
    assert!(
        std::env::var_os("CADCRAFT_G1_WRITE").is_some(),
        "write_golden_set rewrites the baseline: set CADCRAFT_G1_WRITE=1 to run it, and filter the check with `-- --ignored match`"
    );
    let sha = std::env::var("CADCRAFT_GOLDEN_SHA").unwrap_or_else(|_| "unknown".into());
    std::fs::create_dir_all(golden_dir()).unwrap();
    let mut hashes = read_hashes();
    let mut entries = corpus();
    let only = std::env::var("CADCRAFT_GOLDEN_OWNER").ok();
    for entry in &mut entries {
        if only.as_deref().is_some_and(|owner| owner != entry.owner) {
            continue;
        }
        let Some(outputs) = saves(entry) else {
            eprintln!("G1: {} is not on this machine; its golden stays as it is", entry.path);
            continue;
        };
        entry.families = families_of(&outputs[&Format::Dxf]);
        for format in Format::ALL {
            let bytes = &outputs[&format];
            std::fs::write(golden_file(entry, format), miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6)).unwrap();
            hashes.insert((entry.path.clone(), format.name().to_string()), (sha256(bytes), bytes.len()));
        }
        eprintln!("G1: {} written", entry.path);
    }
    let mut list = format!(
        "# G1 frozen baseline: sha256 of the DXF and the DWG that a read and a save makes of each drawing of corpus.toml.\n\
         # Made at {sha}. Columns: sha256, format, bytes, path.\n"
    );
    for ((path, format), (hash, bytes)) in &hashes {
        let _ = writeln!(list, "{hash} {format} {bytes} {path}");
    }
    std::fs::write(hashes_path(), list).unwrap();
    let body = toml::to_string(&Corpus { file: entries }).unwrap();
    let header = "# The drawings the reader activation is proved on (plan v3, section 7): a file not listed here is not covered.\n\
                  # `families` is the activation families the save holds; write_golden_set refreshes it.\n";
    std::fs::write(corpus_path(), format!("{header}{body}")).unwrap();
}

#[test]
fn a_changed_record_is_named_and_an_unchanged_save_is_clean() {
    let mut drawing = cadcraft_io::read(&std::fs::read(root().join("examples/apartment.dxf")).unwrap(), "apartment.dxf").unwrap();
    let golden = cadcraft_io::write_dxf(&drawing);
    assert_eq!(forbidden_drift(Format::Dxf, golden.as_bytes(), cadcraft_io::write_dxf(&drawing).as_bytes()), Vec::<String>::new());
    let line = drawing.model.iter().find(|e| matches!(e.kind, cadcraft_doc::EntityKind::Line(_))).map(|e| e.handle).expect("a line");
    drawing.ensure_layer("G1-DRIFT");
    drawing.modify_entity(line, |e| e.common.layer = "G1-DRIFT".into()).unwrap();
    let drift = forbidden_drift(Format::Dxf, golden.as_bytes(), cadcraft_io::write_dxf(&drawing).as_bytes());
    assert!(drift.contains(&format!("ENTITIES/LINE/{}", line.hex())), "{drift:?}");
}
