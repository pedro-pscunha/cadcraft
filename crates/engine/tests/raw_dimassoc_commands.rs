//! Explicit dimension association commands replace raw associations; unrelated edits do not.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use cadcraft_engine::doc::{Handle, RawTag};
use serde_json::json;

fn session() -> (Session, String, String, Handle) {
    session_with_snaps([8, 8])
}

fn session_with_snaps(snaps: [i64; 2]) -> (Session, String, String, Handle) {
    session_with_shape(snaps, false)
}

fn session_with_shape(snaps: [i64; 2], autocad: bool) -> (Session, String, String, Handle) {
    let mut s = Session::new();
    let line = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"object":line, "at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    let source = cadcraft_io::write_dxf(s.doc().unwrap());
    let mut tags = cadcraft_dxf::parse(source.as_bytes()).unwrap();
    // Remove CADCraft's own association xdata so it cannot override the synthetic raw snaps.
    let mut own = false;
    tags.retain(|t| {
        if t.code == 0 {
            own = false;
        }
        if t.code == 1001 {
            own = t.str().eq_ignore_ascii_case("CADCRAFT");
        }
        !own
    });
    let mut in_assoc = false;
    let mut point = 0;
    for tag in &mut tags {
        if tag.code == 0 {
            in_assoc = tag.str() == "DIMASSOC";
        } else if in_assoc && tag.code == 72 {
            *tag = Tag::i(72, snaps[point]);
            point += 1;
        } else if in_assoc && autocad {
            match tag.code {
                10 | 20 => *tag = Tag::f(tag.code, 0.0),
                30 => *tag = Tag::f(30, 2e50),
                73 => *tag = Tag::i(73, 2),
                91 => *tag = Tag::i(91, 1),
                _ => {}
            }
        }
    }
    if autocad {
        let mut in_assoc = false;
        tags.retain(|t| {
            if t.code == 0 {
                in_assoc = t.str() == "DIMASSOC";
            }
            !(in_assoc && t.code == 301)
        });
    }
    assert_eq!(point, 2, "fixture must replace exactly two DIMASSOC point references");
    let original = objects(&source);
    let raw =
        original.iter().find(|(k, _)| k == "DIMASSOC").unwrap().1.iter().find(|t| t.code == 5).map(|t| Handle::parse_hex(&t.str()).unwrap()).unwrap();
    let handle = tags.iter().position(|t| t.code == 5 && t.str() == line).unwrap();
    assert_eq!(
        tags[..handle].iter().rev().find(|t| t.code == 0).map(Tag::str).as_deref(),
        Some("LINE"),
        "fixture handle must identify the model-space LINE, not a line in the dimension block"
    );
    tags.splice(handle + 1..handle + 1, [Tag::s(102, "{ACAD_REACTORS"), Tag::s(330, raw.hex()), Tag::s(102, "}")]);
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(
        d.foreign.entities.get(&Handle::parse_hex(&line).unwrap()).is_some_and(|x| x.reactors.contains(&raw)),
        "fixture reactor must be read from the source DXF: line={line}, raw={raw:?}, extras={:?}",
        d.foreign.entities
    );
    let cadcraft_engine::doc::EntityKind::Dimension(model) = &d.entity(Handle::parse_hex(&dim).unwrap()).unwrap().kind else { panic!("dimension") };
    assert_eq!(
        model.assoc.len(),
        snaps.iter().filter(|s| **s == 13 || **s == 1).count(),
        "fixture inferred attachments must come from raw snaps, not hidden own xdata"
    );
    let mut s = Session::new();
    s.open_drawing(d, "raw.dxf", None);
    (s, dim, line, raw)
}

fn objects(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let secs = sections(&tags);
    records(&secs.iter().find(|s| s.name == "OBJECTS").unwrap().tags)
}

fn saved(s: &Session) -> Vec<String> {
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let back = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let mut out = vec![dxf, cadcraft_io::write_dxf(&back)];
    let dwg = cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    out.push(cadcraft_io::write_dxf(&back));
    out
}

fn assert_retired(s: &Session, raw: Handle) {
    let foreign = &s.doc().unwrap().foreign;
    assert!(foreign.objects.iter().all(|o| o.handle != raw), "raw DIMASSOC was not retired");
    assert!(foreign.entities.values().chain(foreign.records.values()).all(|x| !x.reactors.contains(&raw)), "retired reactor edge remains");
}

#[test]
fn erasing_geometry_retires_raw_association_even_without_inferred_links() {
    let (mut s, _, line, raw) = session();
    s.execute("erase", &json!({"handles":[line]})).unwrap();
    assert_retired(&s, raw);
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(k, _)| k != "DIMASSOC"), "erased target left a raw association with a null 331");
    }
}

#[test]
fn changed_inferred_attachments_invalidate_raw_association_at_save_boundary() {
    let (mut s, dim, _, _) = session_with_snaps([13, 8]);
    s.doc_mut()
        .unwrap()
        .modify_entity(Handle::parse_hex(&dim).unwrap(), |e| {
            if let cadcraft_engine::doc::EntityKind::Dimension(d) = &mut e.kind {
                d.assoc.clear();
            }
        })
        .unwrap();
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(k, _)| k != "DIMASSOC"), "save resurrected raw data after model attachments changed");
    }
}

#[test]
fn arc_length_reassociation_uses_only_the_public_endpoint_slots() {
    let mut s = Session::new();
    s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap();
    s.execute("line", &json!({"points":[[5,5],[6,5]]})).unwrap();
    let h = s.execute("dimlinear", &json!({"p1":[0,0],"p2":[10,0],"at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.doc_mut()
        .unwrap()
        .modify_entity(Handle::parse_hex(&h).unwrap(), |e| {
            if let cadcraft_engine::doc::EntityKind::Dimension(dm) = &mut e.kind {
                dm.kind = cadcraft_engine::doc::DimKind::ArcLength;
                dm.p15 = cadcraft_engine::geom::Vec3::new(5.0, 5.0, 0.0);
            }
        })
        .unwrap();
    let result = s.execute("dimreassociate", &json!({"handles":[h]})).unwrap();
    assert_eq!(result["points"], 2, "ArcLength must use the same slots in reassociation and standard output: {result}");
}

#[test]
fn raw_association_with_missing_dimension_is_tracked_and_retired_at_save() {
    let (s, _, _, _) = session();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut payload = false;
    let mut in_assoc = false;
    for t in &mut tags {
        if t.code == 0 {
            in_assoc = t.str() == "DIMASSOC";
            payload = false;
        }
        if in_assoc && t.code == 100 {
            payload = true;
        }
        if in_assoc && payload && t.code == 330 {
            *t = Tag::s(330, "2BEEF");
            payload = false;
        }
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(Handle(0x2BEEF)), "raw classification must not depend on whether its owner resolves");
    assert!(
        objects(&cadcraft_io::write_dxf(&d)).iter().all(|(kind, _)| kind != "DIMASSOC"),
        "missing owner must retire, not resurrect, the orphan association"
    );
}

#[test]
fn model_only_dimensions_keep_partial_reassociation() {
    let mut s = Session::new();
    s.execute("line", &json!({"points":[[1000,1000],[1010,1000]]})).unwrap();
    let dim = s.execute("dimlinear", &json!({"p1":[1000,1000],"p2":[1020,1000],"at":[1005,1005]})).unwrap()["handle"].as_str().unwrap().to_string();
    let result = s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    assert_eq!(result["associated"], 1, "{result}");
    assert_eq!(result["points"], 1, "the supported partial attachment must be restored");
}

#[test]
fn audit_attachment_drop_retires_the_raw_association() {
    let (mut s, _, line, raw) = session_with_snaps([13, 8]);
    s.doc_mut().unwrap().remove_entity(Handle::parse_hex(&line).unwrap());
    s.execute("audit", &json!({"fix":true})).unwrap();
    assert_retired(&s, raw);
}

#[test]
fn autocad_endpoint_references_follow_geometry_without_rewriting_raw_bytes() {
    let (mut s, dim, line, raw) = session_with_shape([13, 1], true);
    let h = Handle::parse_hex(&dim).unwrap();
    let original = s.doc().unwrap().foreign.objects.iter().find(|o| o.handle == raw).unwrap().tags.clone();
    s.execute("move", &json!({"handles":[line],"delta":[2,0]})).unwrap();
    let cadcraft_engine::doc::EntityKind::Dimension(model) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("dimension") };
    assert_eq!((model.p13.x, model.p14.x, model.measurement), (2.0, 12.0, 10.0));
    s.execute("lengthen", &json!({"handle":line,"pick":[12,0],"delta":3})).unwrap();
    let cadcraft_engine::doc::EntityKind::Dimension(model) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("dimension") };
    assert_eq!((model.p13.x, model.p14.x, model.measurement), (2.0, 15.0, 13.0));
    assert_eq!(s.doc().unwrap().foreign.objects.iter().find(|o| o.handle == raw).unwrap().tags, original);
    for text in saved(&s) {
        let objects = objects(&text);
        let assoc = &objects.iter().find(|(kind, _)| kind == "DIMASSOC").unwrap().1;
        assert_eq!(assoc.iter().filter(|t| t.code == 331).map(Tag::str).collect::<Vec<_>>(), [line.clone(), line.clone()]);
    }
}

#[test]
fn unsupported_raw_associations_report_command_and_save_notes() {
    let (mut s, _, line, _) = session_with_shape([13, 8], true);
    let result = s.execute("move", &json!({"handles":[line],"delta":[2,0]})).unwrap();
    let note = "1 associative dimensions keep their AutoCAD link but were not updated";
    assert!(result["warnings"].to_string().contains(note), "{result}");
    assert!(s.log.iter().any(|line| line.contains(note)), "{:?}", s.log);
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |drawing, name| cadcraft_io::write(drawing, name).map_err(|e| e.to_string()),
        plot: None,
    });
    for format in ["dxf", "dwg"] {
        let result = s.execute("document.bytes", &json!({"format":format})).unwrap();
        assert!(result["warnings"].to_string().contains(note), "save did not report unsupported association");
    }
}

fn mixed_session(last_point: bool, unsupported: i64) -> (Session, String, String, String) {
    let (mut s, dim, line, _) = session_with_shape([13, unsupported], true);
    let other = s.execute("line", &json!({"points":[[5,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut in_assoc = false;
    let mut point = 0;
    let mut insertion = None;
    for (i, t) in tags.iter_mut().enumerate() {
        if t.code == 0 {
            in_assoc = t.str() == "DIMASSOC";
        } else if in_assoc {
            if t.code == 72 {
                point += 1;
            }
            if point == 2 && t.code == 331 {
                *t = Tag::s(331, &other);
            }
            if point == 2 && t.code == 75 && last_point {
                *t = Tag::i(75, 1);
                insertion = Some(i + 1);
            }
        }
    }
    if let Some(i) = insertion {
        tags.splice(
            i..i,
            [
                Tag::s(1, "AcDbOsnapPointRef"),
                Tag::i(72, 13),
                Tag::s(331, &line),
                Tag::i(73, 2),
                Tag::i(91, 0),
                Tag::f(40, 0.0),
                Tag::f(10, 0.0),
                Tag::f(20, 0.0),
                Tag::f(30, 2e50),
                Tag::i(75, 0),
            ],
        );
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    s.open_drawing(d, "mixed.dxf", None);
    (s, dim, line, other)
}

#[test]
fn only_changed_references_need_evaluation_and_last_point_dependencies_count() {
    for unsupported in [8, 10] {
        for (move_first, move_other, last, follows) in
            [(true, false, false, true), (false, true, false, false), (true, true, false, false), (true, false, true, false)]
        {
            let (mut s, dim, line, other) = mixed_session(last, unsupported);
            let mut handles = Vec::new();
            if move_first {
                handles.push(line);
            }
            if move_other {
                handles.push(other);
            }
            let result = s.execute("move", &json!({"handles":handles,"delta":[2,0]})).unwrap();
            let cadcraft_engine::doc::EntityKind::Dimension(model) = &s.doc().unwrap().entity(Handle::parse_hex(&dim).unwrap()).unwrap().kind else {
                panic!("dimension")
            };
            if follows {
                assert_eq!(
                    (model.p13.x, model.p14.x, model.measurement),
                    (2.0, 10.0, 8.0),
                    "unchanged unsupported point must stay where AutoCAD put it"
                );
                assert!(result.get("warnings").is_none(), "{result}");
            } else {
                assert_eq!((model.p13.x, model.p14.x, model.measurement), (0.0, 10.0, 10.0));
                assert!(result["warnings"].to_string().contains(&dim), "warning must identify dimension {dim}: {result}");
            }
        }
    }
}

#[test]
fn unsupported_changed_reference_freezes_the_whole_dimension() {
    let (mut s, dim, line, raw) = session_with_snaps([13, 8]);
    let dim = Handle::parse_hex(&dim).unwrap();
    let before = s.doc().unwrap().entity(dim).unwrap().kind.clone();
    s.execute("move", &json!({"handles":[line],"delta":[2,0]})).unwrap();
    assert_eq!(s.doc().unwrap().entity(dim).unwrap().kind, before, "opaque raw association must not partially move the cached dimension");
    assert!(
        s.doc().unwrap().foreign.objects.iter().any(|o| o.handle == raw),
        "moving geometry should keep opaque raw semantics while targets remain valid"
    );
}

#[test]
fn incomplete_reassociation_leaves_raw_and_model_unchanged() {
    for partial in [false, true] {
        let (mut s, dim, _, raw) = session_with_snaps([13, 8]);
        let h = Handle::parse_hex(&dim).unwrap();
        s.doc_mut()
            .unwrap()
            .modify_entity(h, |e| {
                if let cadcraft_engine::doc::EntityKind::Dimension(d) = &mut e.kind {
                    d.p14 = cadcraft_engine::geom::Vec3::new(50.0, 50.0, 0.0);
                    if !partial {
                        d.p13 = cadcraft_engine::geom::Vec3::new(40.0, 40.0, 0.0);
                    }
                }
            })
            .unwrap();
        let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
        s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
        assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before, "empty or partial reassociation changed the model");
        assert!(s.doc().unwrap().foreign.objects.iter().any(|o| o.handle == raw), "empty or partial plan discarded the raw association");
    }
}

#[test]
fn undo_and_redo_restore_raw_warning_and_document_snapshots_exactly() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
    for (undo, redo) in [("u", "redo"), ("undo", "mredo")] {
        let (mut s, dim, line, _) = session();
        let h = Handle::parse_hex(&dim).unwrap();
        let original = s.state().unwrap().doc.clone();
        assert!(!s.state().unwrap().is_dirty());
        s.execute("move", &json!({"handles":[line],"delta":[2,0]})).unwrap();
        let edited = s.state().unwrap().doc.clone();
        assert!(edited.raw_dimassoc_unupdated(h));
        let notes = s.log.iter().filter(|line| line.contains("associative dimensions keep")).count();
        assert_eq!(notes, 1);
        let result = s.execute(undo, &json!({})).unwrap();
        assert!(std::sync::Arc::ptr_eq(&s.state().unwrap().doc, &original), "{undo} reprocessed a restored snapshot");
        assert!(!s.state().unwrap().is_dirty());
        assert!(!s.doc().unwrap().raw_dimassoc_unupdated(h));
        assert!(result.get("warnings").is_none(), "undo emitted a new edit warning");
        let bytes = s.execute("document.bytes", &json!({"format":"dxf"})).unwrap();
        assert!(bytes.get("warnings").is_none(), "undo must restore the clean save-warning state");
        let result = s.execute(redo, &json!({})).unwrap();
        assert!(std::sync::Arc::ptr_eq(&s.state().unwrap().doc, &edited), "{redo} reprocessed a restored snapshot");
        assert!(s.doc().unwrap().raw_dimassoc_unupdated(h));
        assert!(result.get("warnings").is_none(), "redo emitted a new edit warning");
        assert_eq!(s.log.iter().filter(|line| line.contains("associative dimensions keep")).count(), notes);
        let bytes = s.execute("document.bytes", &json!({"format":"dxf"})).unwrap();
        assert!(bytes["warnings"].to_string().contains(&dim), "redo must restore the pending save-warning state");
    }
}

#[test]
fn an_over_budget_object_path_keeps_all_dependencies_for_change_detection() {
    let (mut s, dim, _, _) = session_with_shape([13, 1], true);
    let mut extra = Vec::new();
    for i in 0..9 {
        extra.push(s.execute("line", &json!({"points":[[100+i,0],[101+i,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string());
    }
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut in_assoc = false;
    let at = tags
        .iter()
        .position(|t| {
            if t.code == 0 {
                in_assoc = t.str() == "DIMASSOC";
            }
            in_assoc && t.code == 331
        })
        .unwrap();
    tags.splice(at + 1..at + 1, extra.iter().map(|h| Tag::s(331, h)));
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    s.open_drawing(d, "long-path.dxf", None);
    let before = s.doc().unwrap().entity(Handle::parse_hex(&dim).unwrap()).unwrap().kind.clone();
    let result = s.execute("move", &json!({"handles":[extra.last().unwrap()],"delta":[2,0]})).unwrap();
    assert_eq!(s.doc().unwrap().entity(Handle::parse_hex(&dim).unwrap()).unwrap().kind, before);
    assert!(result["warnings"].to_string().contains(&dim), "the tenth path object changed without a warning: {result}");
}

#[test]
fn dictionary_variants_are_pruned_after_retirement_and_dimension_only_copy() {
    for kind in ["DICTIONARY", "dictionary", "ACDBDICTIONARYWDFLT", " acdbdictionarywdflt "] {
        for copy in [false, true] {
            let (mut s, dim, _, _) = session();
            let h = Handle::parse_hex(&dim).unwrap();
            let dict = s.doc().unwrap().foreign.entities.get(&h).unwrap().xdict.unwrap();
            std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().foreign).objects.iter_mut().find(|o| o.handle == dict).unwrap().kind = kind.into();
            if copy {
                let result = s.execute("copy", &json!({"handles":[dim],"delta":[20,0]})).unwrap();
                let cloned = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
                assert!(
                    s.doc().unwrap().foreign.entities.get(&cloned).is_none_or(|e| e.xdict.is_none()),
                    "{kind}: empty copied dictionary remains attached"
                );
            } else {
                s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
                assert!(s.doc().unwrap().foreign.objects.iter().all(|o| o.handle != dict), "{kind}: retired association left an empty dictionary");
            }
        }
    }
}

#[test]
fn copying_a_raw_dimension_without_its_geometry_drops_the_cloned_association() {
    let (mut s, dim, _, _) = session();
    let copy = s.execute("copy", &json!({"handles":[dim], "delta":[20,0]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let h = Handle::parse_hex(&copy).unwrap();
    assert!(
        s.doc().unwrap().foreign.entities.get(&h).is_none_or(|x| x.xdict.is_none() && x.reactors.is_empty()),
        "copied dimension retained a dictionary or reactor outside its cloned graph"
    );
    for text in saved(&s) {
        let objects = objects(&text);
        let associations: Vec<_> = objects.iter().filter(|(k, _)| k == "DIMASSOC").collect();
        assert_eq!(associations.len(), 1, "only the original dimension keeps its association when geometry was not copied");
        assert!(!associations[0].1.iter().any(|t| t.code == 330 && t.str() == copy), "copy still owns a raw association to original geometry");
    }
}

#[test]
fn copying_raw_dimension_with_geometry_remaps_the_whole_copy_set() {
    let (mut s, dim, line, _) = session();
    let result = s.execute("copy", &json!({"handles":[line, dim], "delta":[20,0]})).unwrap();
    let line = result["handles"][0].as_str().unwrap();
    let dim = result["handles"][1].as_str().unwrap();
    for text in saved(&s) {
        let objects = objects(&text);
        let assoc = &objects
            .iter()
            .find(|(k, t)| k == "DIMASSOC" && t.iter().any(|t| t.code == 330 && t.str() == dim))
            .expect("copied dimension keeps the association to its copied geometry")
            .1;
        assert_eq!(
            assoc.iter().filter(|t| t.code == 331).map(Tag::str).collect::<Vec<_>>(),
            vec![line.to_string(); 2],
            "all raw references must follow the copied LINE"
        );
    }
}

#[test]
fn dimdisassociate_retires_raw_object_dictionary_and_reactors() {
    let (mut s, dim, _, raw) = session();
    let result = s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    assert_eq!(result["disassociated"], 1);
    let dim = Handle::parse_hex(&dim).unwrap();
    assert!(s.doc().unwrap().foreign.entities.get(&dim).is_none_or(|x| x.xdict.is_none()), "empty association dictionary remains attached");
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(k, _)| k != "DIMASSOC"));
    }
}

#[test]
fn dimdisassociate_matches_normalized_raw_subclass_names() {
    let (mut s, dim, _, raw) = session();
    let f = std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().foreign);
    let object = f.objects.iter_mut().find(|o| o.handle == raw).unwrap();
    object.tags.iter_mut().find(|t| t.code == 100).unwrap().value = " AcDbDimAssoc ".into();
    s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
}

#[test]
fn dimreassociate_replaces_raw_association_with_the_models_links() {
    let (mut s, dim, _, raw) = session();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    for text in saved(&s) {
        let objects = objects(&text);
        let assocs: Vec<_> = objects.iter().filter(|(k, _)| k == "DIMASSOC").collect();
        assert_eq!(assocs.len(), 1);
        let snaps: Vec<_> = assocs[0].1.iter().filter(|t| t.code == 72).map(Tag::i64).collect();
        assert_eq!(snaps, [13, 1], "raw unsupported snaps overrode the new model links");
    }
}

#[test]
fn unrelated_dimension_edits_keep_the_raw_association() {
    let (mut s, dim, _, raw) = session();
    s.execute("dimoverride", &json!({"handles":[dim], "text":"label"})).unwrap();
    assert!(s.doc().unwrap().foreign.objects.iter().any(|o| o.handle == raw));
    for text in saved(&s) {
        let objects = objects(&text);
        let assoc = &objects.iter().find(|(k, _)| k == "DIMASSOC").unwrap().1;
        assert_eq!(assoc.iter().filter(|t| t.code == 72).map(Tag::i64).collect::<Vec<_>>(), [8, 8]);
    }
}

#[test]
fn one_command_retires_all_selected_raw_associations() {
    let (mut s, dim, line, _) = session();
    let copy = s.execute("copy", &json!({"handles":[line, dim],"delta":[20,0]})).unwrap()["handles"][1].as_str().unwrap().to_string();
    let result = s.execute("dimdisassociate", &json!({"handles":[dim,copy]})).unwrap();
    assert_eq!(result["disassociated"], 2, "both raw-only associations must be counted once");
    assert!(s.doc().unwrap().foreign.objects.iter().all(|o| o.kind != "DIMASSOC"), "one batch left an association behind");
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(kind, _)| kind != "DIMASSOC"), "retired batch reappeared after save/reopen");
    }
}

#[test]
fn retiring_associations_does_not_rewrite_unrelated_object_storage() {
    let (mut s, dim, _, _) = session();
    let d = s.doc_mut().unwrap();
    let dimension = Handle::parse_hex(&dim).unwrap();
    let dictionary = d.foreign.entities.get(&dimension).unwrap().xdict.unwrap();
    let keep = d.new_handle();
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    f.objects
        .iter_mut()
        .find(|o| o.handle == dictionary)
        .unwrap()
        .tags
        .extend([RawTag { code: 3, value: "KEEP".into() }, RawTag { code: 350, value: keep.hex() }]);
    let mut tags = Vec::with_capacity(2048);
    tags.extend([
        RawTag { code: 5, value: keep.hex() },
        RawTag { code: 330, value: dictionary.hex() },
        RawTag { code: 100, value: "AcDbXrecord".into() },
        RawTag { code: 1, value: "unrelated".into() },
    ]);
    f.objects.push(cadcraft_engine::doc::foreign::ForeignObject { handle: keep, kind: "XRECORD".into(), tags });
    assert_eq!(std::sync::Arc::strong_count(&d.foreign), 1, "capacity check must not be confounded by legitimate undo copy-on-write");
    d.retire_raw_dimassocs(&std::collections::BTreeSet::from([dimension]));
    let kept = d.foreign.objects.iter().find(|o| o.handle == keep).unwrap();
    assert!(kept.tags.capacity() >= 2048, "unrelated object tags were reallocated by association retirement");
}

#[test]
fn retiring_raw_dimassoc_keeps_unrelated_dictionary_entries() {
    let (mut s, dim, _, raw) = session();
    let d = s.doc_mut().unwrap();
    let dim_handle = Handle::parse_hex(&dim).unwrap();
    let dictionary = d.foreign.entities.get(&dim_handle).unwrap().xdict.unwrap();
    let keep = d.new_handle();
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    let dict = f.objects.iter_mut().find(|o| o.handle == dictionary).unwrap();
    dict.tags.extend([RawTag { code: 3, value: "KEEP".into() }, RawTag { code: 350, value: keep.hex() }]);
    f.objects.push(cadcraft_engine::doc::foreign::ForeignObject {
        handle: keep,
        kind: "XRECORD".into(),
        tags: vec![
            RawTag { code: 5, value: keep.hex() },
            RawTag { code: 330, value: dictionary.hex() },
            RawTag { code: 100, value: "AcDbXrecord".into() },
            RawTag { code: 280, value: "1".into() },
            RawTag { code: 1, value: "unrelated".into() },
        ],
    });
    s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    assert_eq!(s.doc().unwrap().foreign.entities.get(&dim_handle).unwrap().xdict, Some(dictionary));
    for text in saved(&s) {
        let objects = objects(&text);
        assert!(objects.iter().all(|(k, _)| k != "DIMASSOC"));
        assert!(objects.iter().any(|(k, t)| k == "XRECORD" && t.iter().any(|t| t.code == 1 && t.str() == "unrelated")));
    }
}
