use cadcraft_dxf::Tag;
use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

fn raw_session() -> (Session, String, String) {
    let mut s = Session::new();
    let line = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"object":line,"at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut own = false;
    tags.retain(|t| {
        if t.code == 0 {
            own = false;
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        !own
    });
    let mut inside = false;
    for t in &mut tags {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
        } else if inside && t.code == 72 {
            *t = Tag::i(72, 8);
        }
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(Handle::parse_hex(&dim).unwrap()));
    s.open_drawing(d, "host.dxf", None);
    (s, dim, line)
}
fn add_block(s: &mut Session, name: &str) -> String {
    let line = s.execute("line", &json!({"points":[[100,100],[110,100]]})).unwrap()["handles"][0].clone();
    s.execute("block", &json!({"name":name,"handles":[line],"base":[0,0]})).unwrap()["insert"].as_str().unwrap().to_string()
}
fn count(s: &Session) -> usize {
    s.doc().unwrap().foreign.objects.iter().filter(|o| o.kind == "DIMASSOC").count()
}

#[test]
fn both_editors_update_definition_owned_associations_on_commit() {
    for refedit in [false, true] {
        let mut s = Session::new();
        let line = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let dim = s.execute("dimlinear", &json!({"object":line,"at":[5,5]})).unwrap()["handle"].clone();
        s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
        let insert = s.execute("block", &json!({"name":"MEASURES","handles":[line,dim],"base":[0,0]})).unwrap()["insert"].clone();
        let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
        let mut own = false;
        tags.retain(|t| {
            if t.code == 0 {
                own = false;
            }
            if t.code == 1001 {
                own = t.str() == "CADCRAFT";
            }
            !own
        });
        let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
        let block = d.block("MEASURES").unwrap();
        let dimension = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
        let target = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
        assert!(d.has_raw_dimassoc(dimension));
        s.open_drawing(d, "block-raw.dxf", None);
        if refedit {
            s.execute("refedit", &json!({"handle":insert})).unwrap();
        } else {
            s.execute("bedit", &json!({"name":"MEASURES"})).unwrap();
        }
        let working = if refedit {
            s.doc().unwrap().refedit.as_ref().unwrap().items.iter().find(|item| item.source == Some(target)).unwrap().copy
        } else {
            target
        };
        s.execute("move", &json!({"handles":[working.hex()],"delta":[2,0]})).unwrap();
        s.execute(if refedit { "refclose" } else { "bclose" }, &json!({"save":true})).unwrap();
        let block = s.doc().unwrap().block("MEASURES").unwrap();
        let EntityKind::Dimension(dm) = &block.entities.get(dimension).unwrap().kind else { panic!("dimension") };
        assert!(
            (dm.p13.x - 2.0).abs() < 1e-9 && (dm.p14.x - 12.0).abs() < 1e-9,
            "editor={refedit}: definition association was not validated on commit: {dm:?}"
        );
        assert!(s.doc().unwrap().has_raw_dimassoc(dimension));
    }
}

#[test]
fn refedit_attribute_projections_do_not_retire_or_prematurely_update_host_links() {
    let d = cadcraft_io::read_dxf(include_bytes!("../../io/tests/data/attrib_fields.dxf")).unwrap();
    let mut s = Session::new();
    s.open_drawing(d, "attributes.dxf", None);
    let insert = s.execute("block", &json!({"name":"OUTER","handles":["31"],"base":[0,0]})).unwrap()["insert"].clone();
    let block = s.doc().unwrap().block("OUTER").unwrap();
    let nested = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
    let attribute = block.entities.get(nested).unwrap().kind.attributes().iter().find(|a| a.text.value == "live").unwrap().handle.unwrap();
    let dim = s.execute("dimlinear", &json!({"object":"30","at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut own = false;
    tags.retain(|t| {
        if t.code == 0 {
            own = false;
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        !own
    });
    let mut inside = false;
    for t in &mut tags {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
        } else if inside {
            if t.code == 72 {
                *t = Tag::i(72, 8);
            }
            if t.code == 331 {
                *t = Tag::s(331, attribute.hex());
            }
        }
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    s.open_drawing(d, "attribute-target.dxf", None);
    s.execute("refedit", &json!({"handle":insert})).unwrap();
    assert_eq!(count(&s), 1, "projection aliases must not replace the persistent attribute owner");
    let working = s.doc().unwrap().refedit.as_ref().unwrap().items.iter().find(|item| item.source == Some(nested)).unwrap().copy;
    let result = s.execute("attedit", &json!({"handle":working.hex(),"values":{"LENGTH":"changed"}})).unwrap();
    assert!(result.get("warnings").is_none(), "an uncommitted projection changed host reference state");
    let result = s.execute("refclose", &json!({"save":true})).unwrap();
    assert!(result["warnings"].to_string().contains(&dim), "committed attribute changes did not validate host dependencies: {result}");
}

#[test]
fn both_editors_preserve_out_of_editor_raw_graphs() {
    for refedit in [false, true] {
        let (mut s, dim, _) = raw_session();
        let insert = add_block(&mut s, "EDIT");
        if refedit {
            s.execute("refedit", &json!({"handle":insert})).unwrap();
        } else {
            s.execute("bedit", &json!({"name":"EDIT"})).unwrap();
        }
        s.execute("line", &json!({"points":[[1,1],[2,1]]})).unwrap();
        assert_eq!(count(&s), 1, "editor={refedit}: host raw objects retired inside a projection database");
        s.execute(if refedit { "refclose" } else { "bclose" }, &json!({"save":true})).unwrap();
        assert!(s.doc().unwrap().has_raw_dimassoc(Handle::parse_hex(&dim).unwrap()));
        assert_eq!(count(&s), 1);
        for dwg in [false, true] {
            let bytes =
                if dwg { cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap() } else { cadcraft_io::write_dxf(s.doc().unwrap()).into_bytes() };
            let back = cadcraft_io::read(&bytes, if dwg { "x.dwg" } else { "x.dxf" }).unwrap();
            assert_eq!(back.foreign.objects.iter().filter(|o| o.kind == "DIMASSOC").count(), 1, "editor={refedit}, DWG={dwg}");
        }
    }
}

fn nested_target_session() -> (Session, String, Handle) {
    let (mut s, dim, _) = raw_session();
    let insert = add_block(&mut s, "TARGET");
    let leaf = s.doc().unwrap().block("TARGET").unwrap().entities.iter().next().unwrap().handle;
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut inside = false;
    let mut output = Vec::new();
    for t in tags.drain(..) {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
        }
        if inside && t.code == 331 {
            output.push(Tag::s(331, &insert));
            output.push(Tag::s(331, leaf.hex()));
        } else {
            output.push(t);
        }
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&output).as_bytes()).unwrap();
    s.open_drawing(d, "nested.dxf", None);
    (s, dim, leaf)
}

#[test]
fn block_commit_validates_host_dependencies_and_keeps_pending_state_across_saves() {
    let (mut s, dim, leaf) = nested_target_session();
    let h = Handle::parse_hex(&dim).unwrap();
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    s.execute("bedit", &json!({"name":"TARGET"})).unwrap();
    s.execute("move", &json!({"handles":[leaf.hex()],"delta":[2,0]})).unwrap();
    let saved = s.execute("bsave", &json!({})).unwrap();
    assert!(saved["warnings"].to_string().contains(&dim), "host dependency changed without commit warning: {saved}");
    s.execute("line", &json!({"points":[[20,20],[30,20]]})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before);
    assert!(s.doc().unwrap().raw_dimassoc_unupdated(h), "another save discarded host pending-reference state");
    assert_eq!(count(&s), 1);
}

#[test]
fn removing_a_definition_target_retires_host_raw_data_without_resurrection() {
    let (mut s, dim, leaf) = nested_target_session();
    s.execute("bedit", &json!({"name":"TARGET"})).unwrap();
    s.execute("erase", &json!({"handles":[leaf.hex()]})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("line", &json!({"points":[[20,20],[30,20]]})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert!(!s.doc().unwrap().has_raw_dimassoc(Handle::parse_hex(&dim).unwrap()));
    assert_eq!(count(&s), 0, "later save resurrected out-of-editor raw data");
}
