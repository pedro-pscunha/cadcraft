//! Test-only differential definition. Neither branch uses the static classification decision.
use super::{LegacyDisposition, classify_legacy_unit, family_of_kind, policy};
use cadcraft_doc::{Drawing, EntityStore, Handle};
use cadcraft_dxf::{Tag, Version};
use cadcraft_geom::Mat4;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(super) enum Operation {
    Identity,
    Move,
    Rotate,
    Scale,
    Mirror,
}
impl Operation {
    pub const ALL: [Self; 5] = [Self::Identity, Self::Move, Self::Rotate, Self::Scale, Self::Mirror];
    fn matrix(self) -> Mat4 {
        let mut matrix = Mat4::IDENTITY;
        match self {
            Self::Identity => {}
            Self::Move => {
                matrix.m[0][3] = 17.0;
                matrix.m[1][3] = -23.0;
            }
            Self::Rotate => {
                matrix.m[0][0] = 0.0;
                matrix.m[0][1] = -1.0;
                matrix.m[1][0] = 1.0;
                matrix.m[1][1] = 0.0;
            }
            Self::Scale => {
                matrix.m[0][0] = 2.0;
                matrix.m[1][1] = 2.0;
            }
            Self::Mirror => {
                matrix.m[0][0] = -1.0;
            }
        }
        matrix
    }
    fn native(self) -> acadrust::types::Transform {
        use acadrust::types::{Transform, Vector3};
        match self {
            Self::Identity => Transform::identity(),
            Self::Move => Transform::from_translation(Vector3::new(17.0, -23.0, 0.0)),
            Self::Rotate => Transform::from_rotation(Vector3::UNIT_Z, std::f64::consts::FRAC_PI_2),
            Self::Scale => Transform::from_scaling(Vector3::new(2.0, 2.0, 1.0)),
            Self::Mirror => Transform::from_mirror_x(),
        }
    }
}

fn transform(drawing: &mut Drawing, operation: Operation) -> BTreeSet<Handle> {
    let mut refused = BTreeSet::new();
    let mut store = |store: &mut EntityStore| {
        let entities: Vec<_> = store.iter().map(|entity| entity.as_ref().clone()).collect();
        for mut entity in entities {
            entity.common.geometry = None;
            for attribute in entity.kind.attributes_mut() {
                attribute.geometry = None;
            }
            if entity.try_transform(&operation.matrix()).is_err() {
                refused.insert(entity.handle);
            }
            store.replace(entity);
        }
    };
    store(&mut drawing.model);
    for layout in &mut drawing.layouts {
        store(&mut layout.entities);
    }
    for block in drawing.blocks.values_mut() {
        store(&mut Arc::make_mut(block).entities);
    }
    assert!(crate::dxf_geometry::eligible(drawing).is_empty(), "the oracle must not use geometry replay");
    refused
}

fn records(bytes: &[u8]) -> BTreeMap<Handle, (String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|section| matches!(section.name.as_str(), "ENTITIES" | "BLOCKS"))
        .flat_map(|section| cadcraft_dxf::records(&section.tags))
        .filter(|(kind, _)| family_of_kind(kind).is_some())
        .filter_map(|record| {
            let handle = record.1.iter().find(|tag| tag.code == 5).and_then(|tag| Handle::parse_hex(&tag.str()))?;
            Some((handle, record))
        })
        .fold(BTreeMap::new(), |mut indexed, (handle, record)| {
            assert!(indexed.insert(handle, record).is_none(), "ambiguous family record handle {handle:?}");
            indexed
        })
}

fn fields(kind: &str, tags: &[Tag], version: Version) -> BTreeMap<(i32, usize), String> {
    let Some(family) = family_of_kind(kind) else { return BTreeMap::new() };
    let policy = policy::policy(family, version);
    let mut owned = None;
    let mut ordinal = BTreeMap::new();
    let mut vertex = 0usize;
    let mut vertex_seen = false;
    let mut result = BTreeMap::new();
    for tag in tags.iter().take_while(|tag| tag.code < 1000) {
        if tag.code == 100 {
            let marker = tag.str();
            owned = Some(policy.subclasses.contains(&marker.as_str()));
            continue;
        }
        if owned.unwrap_or_else(|| policy.headerless_groups.contains(&tag.code)) && !matches!(tag.code, 5 | 330 | 100) {
            if family == crate::activation::Family::LwPolyline && matches!(tag.code, 10 | 20 | 40 | 41 | 42 | 91) {
                if tag.code == 10 {
                    if vertex_seen {
                        vertex += 1;
                    }
                    vertex_seen = true;
                }
                result.insert((tag.code, vertex), tag.str());
            } else {
                let n = ordinal.entry(tag.code).or_insert(0);
                result.insert((tag.code, *n), tag.str());
                *n += 1;
            }
        }
    }
    result
}

fn reference_fields(kind: &str, source: &[Tag], expected: &[Tag], version: Version, operation: Operation) -> BTreeMap<(i32, usize), String> {
    if matches!(operation, Operation::Identity) {
        return fields(kind, source, version);
    }
    let mut result = fields(kind, expected, version);
    if kind == "ARC" {
        // These source angles use the +Z OCS admitted by the static rule. A native
        // endpoint reconstruction loses precision for short arcs far from the origin.
        let angles = fields(kind, source, version);
        for code in [50, 51] {
            let source_code = if matches!(operation, Operation::Mirror) { 101 - code } else { code };
            if let Some(angle) = angles.get(&(source_code, 0)).and_then(|value| value.parse::<f64>().ok()) {
                let angle = match operation {
                    Operation::Rotate => angle + 90.0,
                    Operation::Mirror => 180.0 - angle,
                    Operation::Identity | Operation::Move | Operation::Scale => angle,
                };
                result.insert((code, 0), angle.to_string());
            }
        }
    }
    if kind == "LWPOLYLINE" && matches!(operation, Operation::Scale) {
        // Constant and vertex widths are drawing lengths. Their scale comes from
        // source fields, independently of the native transform's width omissions.
        for (key, width) in fields(kind, source, version) {
            if matches!(key.0, 40 | 41 | 43) {
                result.insert(key, (width.parse::<f64>().unwrap() * 2.0).to_string());
            }
        }
    }
    if kind == "HATCH" && matches!(operation, Operation::Mirror) {
        // Reflection reverses an ellipse's parameter because its minor vector is reconstructed
        // perpendicular to the mirrored major vector. This comes from source groups, not either writer.
        let mut edge = 0;
        let mut ordinals = BTreeMap::new();
        for tag in source.iter().take_while(|tag| tag.code < 1000) {
            if matches!(tag.code, 92 | 97) {
                edge = 0;
            }
            if tag.code == 72 {
                edge = tag.i64();
            }
            if matches!(tag.code, 50 | 51) {
                let ordinal = ordinals.entry(tag.code).or_insert(0);
                if edge == 3 {
                    result.insert((tag.code, *ordinal), (-tag.f64()).to_string());
                }
                *ordinal += 1;
            }
        }
    }
    result
}

fn equivalent(kind: &str, expected: &BTreeMap<(i32, usize), String>, actual: &BTreeMap<(i32, usize), String>, input_scale: f64) -> bool {
    fields_contained(kind, expected, actual, input_scale) && fields_contained(kind, actual, expected, input_scale)
}

fn fields_contained(kind: &str, expected: &BTreeMap<(i32, usize), String>, actual: &BTreeMap<(i32, usize), String>, input_scale: f64) -> bool {
    expected.iter().all(|(key, expected)| {
        let default = family_of_kind(kind).and_then(|family| policy::omitted_default(family, key.0));
        actual.get(key).map(String::as_str).or_else(|| default.map(|(value, _)| value)).is_some_and(|actual| {
            if expected == actual {
                return true;
            }
            if cadcraft_dxf::kind_of(key.0) != cadcraft_dxf::Kind::Real {
                return false;
            }
            match (expected.parse::<f64>(), actual.parse::<f64>()) {
                (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() => {
                    let delta = family_of_kind(kind).and_then(|family| policy::angular_period(family, key.0)).map_or((a - b).abs(), |period| {
                        let distance = (a - b).rem_euclid(period);
                        distance.min(period - distance)
                    });
                    delta <= 16.0 * f64::EPSILON * a.abs().max(b.abs()).max(input_scale)
                }
                _ => false,
            }
        })
    })
}

pub(super) fn measure(source: &[u8], operation: Operation) -> BTreeMap<Handle, bool> {
    measure_with_origin(source, operation, false)
}

fn measure_with_origin(source: &[u8], operation: Operation, from_dwg: bool) -> BTreeMap<Handle, bool> {
    let policy = crate::activation::ReaderPolicy::resolve(Some("off")).unwrap();
    let mut legacy = if from_dwg {
        crate::dxf_read::read_from_dwg(source, policy, &|_| crate::activation::Need::Plain)
    } else {
        crate::dxf_read::read_with_policy(source, policy)
    }
    .unwrap();
    let version = crate::save_version(&legacy);
    let refused = transform(&mut legacy, operation);
    let actual = records(crate::write_dxf(&legacy).as_bytes());
    drop(legacy);
    let original = records(source);
    let mut native = acadrust::DxfReader::from_reader(Cursor::new(source.to_vec())).unwrap().read().unwrap();
    for entity in native.entities_mut() {
        entity.apply_transform(&operation.native());
    }
    let mut bytes = Vec::new();
    acadrust::DxfWriter::new(&native).keep_heavy_polylines().write_to_writer(&mut bytes).unwrap();
    drop(native);
    let expected = records(&bytes);
    drop(bytes);
    let mut reported = BTreeSet::new();
    original
        .into_iter()
        .filter(|(_, (kind, _))| family_of_kind(kind).is_some())
        .map(|(handle, (kind, source_tags))| {
            let source_fields = fields(&kind, &source_tags, version);
            // Bound roundoff by input magnitude too: rotation can cancel a large coordinate to zero.
            let input_scale = source_fields
                .iter()
                .filter(|((code, _), _)| (10..=49).contains(code))
                .filter_map(|(_, value)| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .map(f64::abs)
                .fold(32.0, f64::max)
                * 2.0;
            let good = !refused.contains(&handle)
                && actual.get(&handle).zip(expected.get(&handle)).is_some_and(|((actual_kind, actual_tags), (expected_kind, expected_tags))| {
                    actual_kind == &kind
                        && expected_kind == &kind
                        && equivalent(
                            &kind,
                            &reference_fields(&kind, &source_tags, expected_tags, version, operation),
                            &fields(&kind, actual_tags, version),
                            input_scale,
                        )
                        && (!matches!(operation, Operation::Identity)
                            || equivalent(&kind, &source_fields, &fields(&kind, actual_tags, version), input_scale))
                });
            if !good
                && classify_legacy_unit(&[(kind.clone(), source_tags.clone())], version)
                    .is_some_and(|classification| classification.disposition == LegacyDisposition::Plain)
                && reported.insert(kind.clone())
            {
                let actual_fields = actual.get(&handle).map(|(_, tags)| fields(&kind, tags, version)).unwrap_or_default();
                let expected_fields =
                    expected.get(&handle).map(|(_, tags)| reference_fields(&kind, &source_tags, tags, version, operation)).unwrap_or_default();
                let extras: Vec<_> = actual_fields.iter().filter(|(key, _)| !expected_fields.contains_key(key)).take(8).collect();
                eprintln!("LEGACY_ADDITIONS kind={kind} handle={handle:?} operation={operation:?} fields={extras:?}");
                for (label, target) in [("reference", &expected_fields), ("source", &source_fields)] {
                    let differences: Vec<_> = target
                        .iter()
                        .filter(|(key, value)| !fields_contained(&kind, &BTreeMap::from([(**key, (*value).clone())]), &actual_fields, input_scale))
                        .take(8)
                        .map(|(key, value)| (key, value, actual_fields.get(key)))
                        .collect();
                    eprintln!(
                        "LEGACY_DIFFERENCE kind={kind} handle={handle:?} operation={operation:?} refused={} branch={label} fields={differences:?}",
                        refused.contains(&handle)
                    );
                }
            }
            (handle, good)
        })
        .collect()
}

fn containers(source: &[u8]) -> BTreeMap<Handle, crate::activation::records::Container> {
    use crate::activation::records::Container;
    let tags = cadcraft_dxf::parse(source).unwrap();
    let mut result = BTreeMap::new();
    for section in cadcraft_dxf::sections(&tags).into_iter().filter(|section| matches!(section.name.as_str(), "ENTITIES" | "BLOCKS")) {
        let mut container = Container::Entities;
        for (kind, groups) in cadcraft_dxf::records(&section.tags) {
            if kind == "BLOCK"
                && let Some(name) = groups.iter().find(|tag| tag.code == 2)
            {
                container = Container::block(&name.str());
            }
            if let Some(handle) = groups.iter().find(|tag| tag.code == 5).and_then(|tag| Handle::parse_hex(&tag.str())) {
                result.insert(handle, container.clone());
            }
            if kind == "ENDBLK" {
                container = Container::Entities;
            }
        }
    }
    result
}

fn isolate_line(source: &[u8], record: &(String, Vec<Tag>)) -> Vec<u8> {
    let parsed = cadcraft_dxf::parse(source).unwrap();
    let mut tags = Vec::new();
    for section in cadcraft_dxf::sections(&parsed).into_iter().filter(|section| !matches!(section.name.as_str(), "ENTITIES" | "BLOCKS")) {
        tags.extend([Tag::s(0, "SECTION"), Tag::s(2, &section.name)]);
        tags.extend(section.tags);
        tags.push(Tag::s(0, "ENDSEC"));
    }
    tags.extend([Tag::s(0, "SECTION"), Tag::s(2, "ENTITIES"), Tag::s(0, &record.0)]);
    tags.extend(record.1.iter().map(|tag| if tag.code == 330 { Tag::s(330, "0") } else { tag.clone() }));
    tags.extend([Tag::s(0, "ENDSEC"), Tag::s(0, "EOF")]);
    cadcraft_dxf::write_ascii(&tags).into_bytes()
}

#[derive(serde::Deserialize)]
struct Corpus {
    file: Vec<CorpusEntry>,
}
#[derive(serde::Deserialize)]
struct CorpusEntry {
    path: String,
    owner: String,
    families: Vec<String>,
}

fn run_corpus(owner: Option<&str>) {
    let manifest: Corpus = toml::from_str(include_str!("../../../tests/corpus.toml")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for entry in manifest.file.into_iter().filter(|entry| owner.is_none_or(|owner| entry.owner == owner)) {
        let path = std::path::Path::new(&entry.path);
        let path = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
        let input = std::fs::read(&path).unwrap_or_else(|error| panic!("corpus input {}: {error}", entry.path));
        let from_dwg = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("dwg"));
        let source = if from_dwg { cadcraft_dwg::dwg_to_dxf(&input).unwrap() } else { input };
        let original = records(&source);
        let drawing = crate::dxf_read::read_with_policy(&source, crate::activation::ReaderPolicy::resolve(Some("off")).unwrap()).unwrap();
        let version = crate::save_version(&drawing);
        drop(drawing);
        let parsed = cadcraft_dxf::parse(&source).unwrap();
        let classifier = super::LegacyClassifier::new(&cadcraft_dxf::sections(&parsed), version);
        drop(parsed);
        let places = containers(&source);
        for operation in Operation::ALL {
            let measured = measure_with_origin(&source, operation, from_dwg);
            let mut counts: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
            let mut derived: BTreeMap<String, usize> = BTreeMap::new();
            for (handle, (kind, tags)) in &original {
                if let Some(owner) = places.get(handle).and_then(|place| classifier.derived_owner(place)) {
                    *derived.entry(format!("{:?}", owner.family)).or_default() += 1;
                    continue;
                }
                let Some(classification) = classify_legacy_unit(&[(kind.clone(), tags.clone())], version) else { continue };
                let count = counts.entry(format!("{:?}", classification.family)).or_default();
                let lossless = measured.get(handle) == Some(&true);
                match (classification.disposition, lossless) {
                    (LegacyDisposition::Plain, true) => count.0 += 1,
                    (LegacyDisposition::Plain, false) => {
                        count.1 += 1;
                        if failures.len() < 50 {
                            failures.push(format!("{} {version:?} {kind} {handle:?} {operation:?}: false plain", entry.path));
                        }
                    }
                    (LegacyDisposition::Needs, true) => count.2 += 1,
                    (LegacyDisposition::Needs, false) => count.3 += 1,
                }
            }
            eprintln!(
                "LEGACY_ORACLE file={} version={version:?} operation={operation:?} declared={:?} counts(plain,false_plain,extra_needs,needs)={counts:?} owner_derived={derived:?}",
                entry.path, entry.families
            );
        }
        checked += 1;
    }
    eprintln!("LEGACY_ORACLE drawings={checked} false_plain_examples={failures:?}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn table_graphics_lines_are_measured_as_isolated_units_with_original_context() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/apartment.dxf");
    let source = std::fs::read(path).unwrap();
    let parsed = cadcraft_dxf::parse(&source).unwrap();
    let classifier = super::LegacyClassifier::new(&cadcraft_dxf::sections(&parsed), Version::R2000);
    let places = containers(&source);
    let mut checked = 0;
    for (handle, record) in records(&source) {
        if record.0 != "LINE"
            || places
                .get(&handle)
                .and_then(|place| classifier.derived_owner(place))
                .is_none_or(|owner| owner.family != crate::activation::Family::Table)
        {
            continue;
        }
        let isolated = isolate_line(&source, &record);
        for operation in Operation::ALL {
            assert_eq!(measure(&isolated, operation).get(&handle), Some(&true), "isolated derived LINE {handle:?} {operation:?}");
        }
        checked += 1;
    }
    assert_eq!(checked, 29);
}

#[test]
fn structural_metadata_cannot_replace_a_family_record_in_the_oracle_index() {
    let source = b"0\nSECTION\n2\nBLOCKS\n0\nLINE\n5\nB1\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDBLK\n5\nB1\n0\nENDSEC\n0\nEOF\n";
    assert_eq!(records(source).get(&Handle(0xB1)).unwrap().0, "LINE");
}

#[test]
#[should_panic(expected = "ambiguous family record handle")]
fn duplicate_family_handles_stop_the_oracle_instead_of_dropping_a_record() {
    let source = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\nB1\n0\nCIRCLE\n5\nB1\n0\nENDSEC\n0\nEOF\n";
    records(source);
}

#[test]
fn comparison_rejects_unexpected_nondefault_fields() {
    let source = BTreeMap::from([((10, 0), "1".to_string())]);
    let mut actual = source.clone();
    actual.insert((39, 0), "3".to_string());
    assert!(!equivalent("LINE", &source, &actual, 32.0));
}

#[test]
fn reference_uses_source_angles_and_scales_polyline_constant_width() {
    let arc = cadcraft_dxf::parse(b"50\n90.14528841801683\n51\n115.93705821811227\n").unwrap();
    let changed = cadcraft_dxf::parse(b"50\n90.14529003267106\n51\n115.93705948035071\n").unwrap();
    assert_eq!(reference_fields("ARC", &arc, &changed, Version::R2018, Operation::Identity), fields("ARC", &arc, Version::R2018));
    let polyline = cadcraft_dxf::parse(b"43\n0.25\n").unwrap();
    let expected = reference_fields("LWPOLYLINE", &polyline, &polyline, Version::R2018, Operation::Scale);
    assert_eq!(expected.get(&(43, 0)).unwrap().parse::<f64>().unwrap(), 0.5);
    let vertices = cadcraft_dxf::parse(b"10\n0\n20\n0\n40\n0.25\n41\n0.5\n10\n1\n20\n1\n40\n0.75\n41\n1\n").unwrap();
    let expected = reference_fields("LWPOLYLINE", &vertices, &vertices, Version::R2018, Operation::Scale);
    for (key, value) in [((40, 0), 0.5), ((41, 0), 1.0), ((40, 1), 1.5), ((41, 1), 2.0)] {
        assert_eq!(expected.get(&key).unwrap().parse::<f64>().unwrap(), value);
    }
}

#[test]
fn repository_corpus_has_no_false_plain() {
    run_corpus(Some("repo"));
}

#[test]
fn activation_corpus_has_no_false_plain() {
    run_corpus(Some("activation fixtures"));
}

#[test]
#[ignore = "external manifest corpus: run explicitly and fail if any listed input is missing"]
fn external_corpus_has_no_false_plain() {
    run_corpus(None);
}

#[test]
fn every_declared_plain_family_has_a_replay_disabled_edit_witness() {
    let cases = [
        ("LINE", "10\n1\n20\n2\n30\n0\n11\n3\n21\n4\n31\n0\n"),
        ("CIRCLE", "10\n1\n20\n2\n30\n0\n40\n3\n"),
        ("POINT", "10\n1\n20\n2\n30\n0\n50\n0\n"),
        ("ARC", "10\n1\n20\n2\n30\n0\n40\n3\n50\n20\n51\n120\n"),
        ("ELLIPSE", "10\n1\n20\n2\n30\n0\n11\n3\n21\n0\n31\n0\n40\n0.5\n41\n0\n42\n3\n"),
        ("RAY", "10\n1\n20\n2\n30\n0\n11\n1\n21\n0\n31\n0\n"),
        ("XLINE", "10\n1\n20\n2\n30\n0\n11\n1\n21\n0\n31\n0\n"),
        ("SOLID", "10\n0\n20\n0\n11\n2\n21\n0\n12\n0\n22\n1\n13\n2\n23\n1\n"),
        ("TRACE", "10\n0\n20\n0\n11\n2\n21\n0\n12\n0\n22\n1\n13\n2\n23\n1\n"),
        ("3DFACE", "10\n0\n20\n0\n11\n2\n21\n0\n12\n2\n22\n1\n13\n0\n23\n1\n70\n0\n"),
        ("LWPOLYLINE", "90\n2\n70\n0\n10\n1\n20\n2\n10\n3\n20\n4\n42\n0.25\n"),
    ];
    for version in Version::ALL {
        for (kind, fields) in cases {
            let code = version.code();
            let source = format!(
                "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\n{code}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\n{kind}\n5\n100\n{fields}0\nENDSEC\n0\nEOF\n"
            );
            let records = records(source.as_bytes());
            let unit = records.get(&Handle(0x100)).unwrap().clone();
            if classify_legacy_unit(&[unit], version).unwrap().disposition != LegacyDisposition::Plain {
                continue;
            }
            for operation in Operation::ALL {
                assert_eq!(measure(source.as_bytes(), operation).get(&Handle(0x100)), Some(&true), "{version:?} {kind} {operation:?}");
            }
        }
    }
}

#[test]
fn ellipse_hatch_is_plain_only_when_its_actual_edits_are_lossless() {
    let record = super::tests::ELLIPSE_HATCH.replacen("HATCH\n", "HATCH\n5\n100\n", 1);
    for version in Version::ALL {
        let code = version.code();
        let source = format!("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\n{code}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{record}0\nENDSEC\n0\nEOF\n");
        for operation in Operation::ALL {
            assert_eq!(measure(source.as_bytes(), operation).get(&Handle(0x100)), Some(&true), "{version:?} HATCH {operation:?}");
        }
    }
}

#[test]
fn point_display_angle_fix_matches_the_independent_rotation_reference() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOINT\n5\n100\n10\n1\n20\n2\n30\n0\n50\n15\n0\nENDSEC\n0\nEOF\n";
    let record = records(source).remove(&Handle(0x100)).unwrap();
    assert_eq!(classify_legacy_unit(&[record], Version::R2018).unwrap().disposition, LegacyDisposition::Needs);
    assert_eq!(measure(source, Operation::Rotate).get(&Handle(0x100)), Some(&true));
}

#[test]
fn primitive_plain_candidates_satisfy_the_differential_definition() {
    let source = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n100\n10\n1\n20\n2\n30\n0\n11\n3\n21\n4\n31\n0\n0\nCIRCLE\n5\n101\n10\n1\n20\n2\n30\n0\n40\n3\n0\nENDSEC\n0\nEOF\n";
    for operation in Operation::ALL {
        let measured = measure(source, operation);
        for (handle, (kind, tags)) in records(source) {
            let decision = classify_legacy_unit(&[(kind, tags)], Version::R2018).unwrap();
            if decision.disposition == LegacyDisposition::Plain {
                assert_eq!(measured.get(&handle), Some(&true), "{operation:?} {handle:?}");
            }
        }
    }
}
