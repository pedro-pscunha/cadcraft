//! Copies preserve occurrence-local references; joins remap every consumed identity.

use std::sync::Arc;

use cadcraft_engine::doc::{Block, Drawing, Entity, EntityKind, Handle, RawTag};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::{Value, json};

fn handle(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

fn probe(s: &mut Session, h: Handle, role: &str, target: Handle) {
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            e.common.xdata = vec![
                RawTag { code: 1001, value: "COPY_PROBE".into() },
                RawTag { code: 1000, value: role.into() },
                RawTag { code: 1005, value: target.hex() },
            ];
        })
        .unwrap();
}

fn line(s: &mut Session, points: [[f64; 2]; 2]) -> Handle {
    handle(&s.execute("line", &json!({"points":points})).unwrap()["handles"][0])
}

/// Relocate, rather than duplicate, model objects into a definition so setup cannot fix the
/// references that NCOPY itself must remap. All original handles and owned metadata stay intact.
fn enclose(s: &mut Session, name: &str) -> Handle {
    let d = s.doc_mut().unwrap();
    let mut block = Block::new(name);
    block.entities = std::mem::take(&mut d.model);
    d.blocks.insert(name.into(), Arc::new(block));
    handle(&s.execute("insert", &json!({"name":name, "at":[0,0]})).unwrap()["handle"])
}

fn repeated_occurrences() -> (Session, Handle) {
    let mut s = Session::new();
    let a = line(&mut s, [[0.0, 0.0], [1.0, 0.0]]);
    let b = line(&mut s, [[1.0, 0.0], [2.0, 0.0]]);
    probe(&mut s, a, "A", b);
    probe(&mut s, b, "B", b);
    enclose(&mut s, "SOURCE");
    // The SOURCE INSERT itself has the same handle in both WRAPPER occurrences: grouping by
    // only its definition or immediate parent handle is not enough; the entire path matters.
    enclose(&mut s, "WRAPPER");
    s.execute("insert", &json!({"name":"WRAPPER", "at":[20,0]})).unwrap();
    let root = enclose(&mut s, "ROOT");
    (s, root)
}

fn role(e: &Entity, name: &str) -> bool {
    e.common.xdata.iter().any(|t| t.code == 1000 && t.value == name)
}

fn assert_occurrence_references(d: &Drawing) {
    let copies: Vec<_> = d.model.iter().filter(|e| role(e, "A") || role(e, "B")).collect();
    assert_eq!(copies.len(), 4, "both leaves of both occurrences must be copied");
    for a in copies.iter().filter(|e| role(e, "A")) {
        let EntityKind::Line(a_line) = &a.kind else { panic!("line A") };
        let b = copies.iter().find(|e| role(e, "B") && matches!(&e.kind, EntityKind::Line(b_line) if b_line.a == a_line.b)).unwrap();
        assert!(
            a.common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(b.handle)),
            "NCOPY sibling reference must follow its own occurrence's B copy"
        );
        assert!(
            b.common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(b.handle)),
            "NCOPY B self-reference must follow its own copy"
        );
    }
}

#[test]
fn ncopy_remaps_sibling_references_separately_for_repeated_occurrences() {
    let (mut s, root) = repeated_occurrences();
    let r = s.execute("ncopy", &json!({"handle":root.hex(), "delta":[0,10]})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 4);
    assert_occurrence_references(s.doc().unwrap());
    for extension in ["dxf", "dwg"] {
        let name = format!("ncopy.{extension}");
        let bytes = cadcraft_io::write(s.doc().unwrap(), &name).unwrap();
        assert_occurrence_references(&cadcraft_io::read(&bytes, &name).unwrap());
    }
}

#[test]
fn interactive_ncopy_groups_picked_siblings_by_occurrence() {
    let (mut s, _) = repeated_occurrences();
    s.start("ncopy").unwrap();
    for x in [0.5, 1.5, 20.5, 21.5] {
        s.input(Input::Point(Vec2::new(x, 0.0))).unwrap();
    }
    s.input(Input::Enter).unwrap();
    s.input(Input::Point(Vec2::ZERO)).unwrap();
    s.input(Input::Point(Vec2::new(0.0, 10.0))).unwrap();
    assert!(s.running.is_none());
    assert_occurrence_references(s.doc().unwrap());
}

#[test]
fn ncopy_preserves_raw_association_when_geometry_is_another_leaf() {
    let mut s = Session::new();
    let geometry = line(&mut s, [[0.0, 0.0], [10.0, 0.0]]);
    let dimension = handle(&s.execute("dimlinear", &json!({"object":geometry.hex(), "at":[5,5]})).unwrap()["handle"]);
    s.execute("dimreassociate", &json!({"handles":[dimension.hex()]})).unwrap();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut own = false;
    let mut association = false;
    tags.retain_mut(|t| {
        if t.code == 0 {
            own = false;
            association = t.str() == "DIMASSOC";
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        if association && t.code == 72 {
            *t = cadcraft_dxf::Tag::i(72, 8);
        }
        !own
    });
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(dimension), "fixture must have an opaque raw association");
    s.open_drawing(d, "raw.dxf", None);
    let root = enclose(&mut s, "ASSOCIATED");
    let r = s.execute("ncopy", &json!({"handle":root.hex()})).unwrap();
    let d = s.doc().unwrap();
    let copies: Vec<_> = r["handles"].as_array().unwrap().iter().map(handle).collect();
    let copied_dimension = copies.iter().copied().find(|h| matches!(d.entity(*h).unwrap().kind, EntityKind::Dimension(_))).unwrap();
    let copied_geometry = copies.iter().copied().find(|h| matches!(d.entity(*h).unwrap().kind, EntityKind::Line(_))).unwrap();
    assert!(d.has_raw_dimassoc(copied_dimension), "NCOPY must retain raw association when all of its geometry is copied");
    let raw = d
        .foreign
        .objects
        .iter()
        .find(|o| {
            o.kind == "DIMASSOC"
                && cadcraft_engine::doc::foreign::dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str())))
                    .is_some_and(|(_, h)| h == copied_dimension)
        })
        .unwrap();
    assert!(
        raw.tags.iter().filter(|t| t.code == 331).all(|t| Handle::parse_hex(&t.value) == Some(copied_geometry)),
        "raw references must name the copied geometry"
    );
    for extension in ["dxf", "dwg"] {
        let name = format!("ncopy-raw.{extension}");
        let bytes = cadcraft_io::write(d, &name).unwrap();
        let saved = cadcraft_io::read(&bytes, &name).unwrap();
        assert!(saved.has_raw_dimassoc(copied_dimension), "{extension}: copied raw association must survive a save");
        assert!(
            saved
                .foreign
                .objects
                .iter()
                .filter(|o| o.kind == "DIMASSOC")
                .flat_map(|o| &o.tags)
                .any(|t| t.code == 331 && Handle::parse_hex(&t.value) == Some(copied_geometry)),
            "{extension}: copied raw geometry reference was lost"
        );
    }
}

fn owned_probe(s: &mut Session, owner: Handle, target: Handle, marker: &str) {
    use cadcraft_engine::doc::foreign::ForeignObject;
    let d = s.doc_mut().unwrap();
    let dictionary = d.new_handle();
    let record = d.new_handle();
    let tag = |code, value: String| RawTag { code, value };
    let foreign = Arc::make_mut(&mut d.foreign);
    foreign.entities.entry(owner).or_default().xdict = Some(dictionary);
    foreign.objects.extend([
        ForeignObject {
            handle: dictionary,
            kind: "DICTIONARY".into(),
            tags: vec![
                tag(5, dictionary.hex()),
                tag(330, owner.hex()),
                tag(100, "AcDbDictionary".into()),
                tag(3, "MERGE_PROBE".into()),
                tag(350, record.hex()),
            ],
        },
        ForeignObject {
            handle: record,
            kind: "XRECORD".into(),
            tags: vec![
                tag(5, record.hex()),
                tag(330, dictionary.hex()),
                tag(100, "AcDbXrecord".into()),
                tag(1, marker.into()),
                tag(340, target.hex()),
                tag(341, owner.hex()),
            ],
        },
    ]);
}

fn join_sources() -> (Session, Handle, Handle, Handle) {
    let mut s = Session::new();
    let a = line(&mut s, [[0.0, 0.0], [1.0, 0.0]]);
    let b = line(&mut s, [[1.0, 0.0], [1.0, 1.0]]);
    let external = line(&mut s, [[100.0, 0.0], [101.0, 0.0]]);
    probe(&mut s, a, "A", b);
    s.doc_mut()
        .unwrap()
        .modify_entity(a, |e| e.common.xdata.extend([RawTag { code: 1005, value: a.hex() }, RawTag { code: 1005, value: external.hex() }]))
        .unwrap();
    (s, a, b, external)
}

fn assert_merged_references(d: &Drawing, joined: Handle, external: Handle) {
    let references: Vec<_> =
        d.entity(joined).unwrap().common.xdata.iter().filter(|t| t.code == 1005).map(|t| Handle::parse_hex(&t.value).unwrap()).collect();
    assert_eq!(references, [joined, joined, external], "JOIN must map every consumed identity to the result, without remapping unrelated geometry");
}

#[test]
fn join_remaps_surviving_fields_and_xdata_document_wide_for_all_curve_routes() {
    let mut failures = Vec::new();
    for route in ["line", "spline", "ellipse"] {
        let d = cadcraft_io::read_dxf(include_bytes!("../../io/tests/data/attrib_fields.dxf")).unwrap();
        let mut s = Session::new();
        s.open_drawing(d, "fields.dxf", None);
        let b = Handle(0x30);
        let a = if route == "ellipse" {
            s.doc_mut()
                .unwrap()
                .modify_entity(b, |e| {
                    e.kind = EntityKind::Ellipse(cadcraft_engine::doc::Ellipse {
                        normal: cadcraft_geom::Vec3::Z,
                        center: cadcraft_engine::geom::Vec3::ZERO,
                        major: cadcraft_engine::geom::Vec3::new(10.0, 0.0, 0.0),
                        ratio: 0.5,
                        start: std::f64::consts::PI,
                        end: std::f64::consts::TAU,
                    })
                })
                .unwrap();
            s.doc_mut()
                .unwrap()
                .add(
                    &cadcraft_engine::doc::Space::Model,
                    Default::default(),
                    EntityKind::Ellipse(cadcraft_engine::doc::Ellipse {
                        normal: cadcraft_geom::Vec3::Z,
                        center: cadcraft_engine::geom::Vec3::ZERO,
                        major: cadcraft_engine::geom::Vec3::new(10.0, 0.0, 0.0),
                        ratio: 0.5,
                        start: 0.0,
                        end: std::f64::consts::PI,
                    }),
                )
                .unwrap()
        } else {
            if route == "spline" {
                s.doc_mut()
                    .unwrap()
                    .modify_entity(b, |e| e.kind = EntityKind::Spline(cadcraft_engine::geom::Spline::from_line(Vec2::ZERO, Vec2::new(10.0, 0.0), 1)))
                    .unwrap();
            }
            line(&mut s, [[-10.0, 0.0], [0.0, 0.0]])
        };
        let external = line(&mut s, [[100.0, 0.0], [101.0, 0.0]]);
        probe(&mut s, external, "survivor", b);
        owned_probe(&mut s, external, b, "outside");
        let f = Arc::make_mut(&mut s.doc_mut().unwrap().foreign);
        f.entities
            .entry(Handle(0x33))
            .or_default()
            .xdata
            .extend([RawTag { code: 1001, value: "SURVIVOR_ATTR".into() }, RawTag { code: 1005, value: b.hex() }]);
        f.records
            .entry(("LAYER".into(), "0".into()))
            .or_default()
            .xdata
            .extend([RawTag { code: 1001, value: "SURVIVOR_TABLE".into() }, RawTag { code: 1005, value: b.hex() }]);
        let joined = handle(&s.execute("join", &json!({"handles":[a.hex(),b.hex()]})).unwrap()["handle"]);
        for extension in ["dxf", "dwg"] {
            let name = format!("survivors.{extension}");
            let bytes = cadcraft_io::write(s.doc().unwrap(), &name).unwrap();
            let back = cadcraft_io::read(&bytes, &name).unwrap();
            let field_refs: Vec<_> = back
                .foreign
                .objects
                .iter()
                .filter(|o| o.kind == "FIELD")
                .flat_map(|o| &o.tags)
                .filter(|t| t.code == 331)
                .filter_map(|t| Handle::parse_hex(&t.value))
                .collect();
            assert!(!field_refs.is_empty(), "fixture FIELD must survive");
            if field_refs.iter().any(|h| *h != joined) {
                failures.push(format!("{route}/{extension}: FIELD={field_refs:?}, result={joined:?}"));
            }
            if !back.entity(external).unwrap().common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(joined)) {
                failures.push(format!("{route}/{extension}: surviving entity xdata"));
            }
            if !back
                .foreign
                .objects
                .iter()
                .filter(|o| o.kind == "XRECORD" && o.tags.iter().any(|t| t.code == 1 && t.value == "outside"))
                .flat_map(|o| &o.tags)
                .any(|t| t.code == 340 && Handle::parse_hex(&t.value) == Some(joined))
            {
                failures.push(format!("{route}/{extension}: surviving owned graph"));
            }
            if !back.foreign.entities.get(&Handle(0x33)).unwrap().xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(joined))
            {
                failures.push(format!("{route}/{extension}: surviving ATTRIB xdata"));
            }
            if !back
                .foreign
                .records
                .get(&("LAYER".into(), "0".into()))
                .unwrap()
                .xdata
                .iter()
                .any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(joined))
            {
                failures.push(format!("{route}/{extension}: table xdata"));
            }
        }
    }
    assert!(failures.is_empty(), "consumed identities were not remapped in surviving data: {failures:?}");
}

#[test]
fn join_remaps_all_consumed_xdata_identities_without_foreign_storage() {
    let (mut s, a, b, external) = join_sources();
    let joined = handle(&s.execute("join", &json!({"handles":[a.hex(), b.hex()]})).unwrap()["handle"]);
    let d = s.doc().unwrap();
    assert!(d.entity(a).is_none() && d.entity(b).is_none());
    assert_merged_references(d, joined, external);
}

fn assert_merged_graph(d: &Drawing, joined: Handle) {
    let dictionary = d.foreign.entities.get(&joined).unwrap().xdict.unwrap();
    let dict = d.foreign.objects.iter().find(|o| o.handle == dictionary).unwrap();
    assert!(dict.tags.iter().any(|t| t.code == 330 && Handle::parse_hex(&t.value) == Some(joined)), "merged dictionary must belong to the result");
    let record = dict.tags.iter().find(|t| t.code == 350).map(|t| Handle::parse_hex(&t.value).unwrap()).unwrap();
    let record = d.foreign.objects.iter().find(|o| o.handle == record).unwrap();
    assert!(
        record.tags.iter().any(|t| t.code == 1 && t.value == "A"),
        "JOIN must clone only the chosen source's graph, not another consumed object's graph"
    );
    for code in [340, 341] {
        assert!(
            record.tags.iter().any(|t| t.code == code && Handle::parse_hex(&t.value) == Some(joined)),
            "JOIN's owned graph must remap consumed references {code}"
        );
    }
}

#[test]
fn join_remaps_owned_graph_without_adopting_other_consumed_metadata() {
    let (mut s, a, b, external) = join_sources();
    owned_probe(&mut s, a, b, "A");
    owned_probe(&mut s, b, a, "B");
    let joined = handle(&s.execute("join", &json!({"handles":[a.hex(), b.hex()]})).unwrap()["handle"]);
    let d = s.doc().unwrap();
    assert_merged_references(d, joined, external);
    assert_merged_graph(d, joined);
    assert_eq!(
        d.foreign.objects.iter().filter(|o| o.tags.iter().any(|t| t.code == 1 && t.value == "B")).count(),
        0,
        "consumed metadata is retired, not reparented to the result by its aliases"
    );
    for extension in ["dxf", "dwg"] {
        let name = format!("join.{extension}");
        let bytes = cadcraft_io::write(d, &name).unwrap();
        let saved = cadcraft_io::read(&bytes, &name).unwrap();
        assert_merged_references(&saved, joined, external);
        assert_merged_graph(&saved, joined);
    }
}

#[test]
fn breaking_one_source_still_gives_each_piece_its_own_identity() {
    let mut s = Session::new();
    let a = line(&mut s, [[0.0, 0.0], [10.0, 0.0]]);
    let external = line(&mut s, [[100.0, 0.0], [110.0, 0.0]]);
    probe(&mut s, a, "A", a);
    s.doc_mut().unwrap().modify_entity(a, |e| e.common.xdata.push(RawTag { code: 1005, value: external.hex() })).unwrap();
    let r = s.execute("break", &json!({"handle":a.hex(), "p1":[3,0], "p2":[7,0]})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 2);
    for h in r["handles"].as_array().unwrap().iter().map(handle) {
        let references: Vec<_> = s
            .doc()
            .unwrap()
            .entity(h)
            .unwrap()
            .common
            .xdata
            .iter()
            .filter(|t| t.code == 1005)
            .map(|t| Handle::parse_hex(&t.value).unwrap())
            .collect();
        assert_eq!(references, [h, external], "independent derived pieces must not use JOIN's many-to-one mapping");
    }
}

#[test]
fn join_chooses_metadata_and_common_properties_only_from_used_geometry() {
    use cadcraft_engine::doc::color::Color;
    let mut s = Session::new();
    let circle = handle(&s.execute("circle", &json!({"center":[100,100], "radius":5})).unwrap()["handle"]);
    let a = line(&mut s, [[0.0, 0.0], [1.0, 0.0]]);
    let b = line(&mut s, [[1.0, 0.0], [1.0, 1.0]]);
    probe(&mut s, circle, "CIRCLE", circle);
    probe(&mut s, a, "A", b);
    owned_probe(&mut s, circle, circle, "CIRCLE");
    owned_probe(&mut s, a, b, "A");
    let d = s.doc_mut().unwrap();
    d.ensure_layer("CIRCLES");
    d.ensure_layer("LINES");
    d.modify_entity(circle, |e| {
        e.common.layer = "CIRCLES".into();
        e.common.color = Color::Index(1);
    })
    .unwrap();
    d.modify_entity(a, |e| {
        e.common.layer = "LINES".into();
        e.common.color = Color::Index(5);
    })
    .unwrap();
    let circle_before = d.entity(circle).unwrap().clone();
    let circle_metadata = d.foreign.entities.get(&circle).unwrap().clone();
    let mut expected = d.entity(a).unwrap().common.clone();
    let joined = handle(&s.execute("join", &json!({"handles":[circle.hex(), a.hex(), b.hex()]})).unwrap()["handle"]);
    for t in expected.xdata.iter_mut().filter(|t| t.code == 1005) {
        t.value = joined.hex();
    }
    let d = s.doc().unwrap();
    assert_eq!(d.entity(circle).unwrap(), &circle_before, "the non-contributing circle must remain unchanged");
    assert_eq!(d.foreign.entities.get(&circle), Some(&circle_metadata), "the circle must retain its own metadata");
    assert_eq!(
        d.entity(joined).unwrap().common,
        expected,
        "JOIN must inherit common properties and xdata from the first contributing geometry, not the leading circle"
    );
    assert_merged_graph(d, joined);
    for extension in ["dxf", "dwg"] {
        let name = format!("join-source.{extension}");
        let bytes = cadcraft_io::write(d, &name).unwrap();
        let saved = cadcraft_io::read(&bytes, &name).unwrap();
        assert_eq!(saved.entity(joined).unwrap().common.layer, "LINES");
        assert_eq!(saved.entity(joined).unwrap().common.color, Color::Index(5));
        assert_merged_graph(&saved, joined);
    }
}

#[test]
fn join_keeps_chain_seed_metadata_when_a_later_segment_is_prepended() {
    let mut s = Session::new();
    let a = line(&mut s, [[1.0, 0.0], [2.0, 0.0]]);
    let b = line(&mut s, [[0.0, 0.0], [1.0, 0.0]]);
    probe(&mut s, a, "A", b);
    probe(&mut s, b, "B", a);
    owned_probe(&mut s, a, b, "A");
    owned_probe(&mut s, b, a, "B");
    let joined = handle(&s.execute("join", &json!({"handles":[a.hex(), b.hex()]})).unwrap()["handle"]);
    let d = s.doc().unwrap();
    assert!(role(d.entity(joined).unwrap(), "A"), "prepending geometry must not switch metadata away from the first selected contributing source");
    assert_merged_graph(d, joined);
}
