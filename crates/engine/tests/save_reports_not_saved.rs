//! Saving a drawing with entities of types CADCraft doesn't model (read from another program's
//! file) as DWG says which ones the file doesn't keep, instead of dropping them silently. DXF
//! writes them back as read: nothing to report.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::doc::{Block, Common, Drawing, EntityKind, RawTag, Space, Unknown};
use serde_json::json;

fn unknown(kind: &str) -> EntityKind {
    EntityKind::Unknown(Unknown { dxf_type: kind.into(), tags: vec![RawTag { code: 8, value: "0".into() }] })
}

#[test]
fn save_lists_the_entities_the_file_does_not_keep() {
    set_io(IoHooks { read: |_, _| Err("no reader in this test".into()), write: |_, _| Ok(b"0\nEOF\n".to_vec()), plot: None });
    let mut d = Drawing::new_imperial();
    // ARC_DIMENSION and OLE2FRAME are kept as read, and the DWG writer keeps them too: not listed.
    for k in ["MLINE", "3DSOLID", "MLINE", "ARC_DIMENSION", "OLE2FRAME"] {
        d.add(&Space::Model, Common::default(), unknown(k)).unwrap();
    }
    let mut b = Block::new("Part");
    let h = d.new_handle();
    b.entities.push(cadcraft_engine::doc::Entity {
        geometry_revision: Default::default(),
        handle: h,
        common: Common::default(),
        kind: unknown("REGION"),
    });
    d.blocks.insert("Part".into(), std::sync::Arc::new(b));
    let mut s = Session::empty();
    s.open_drawing(d, "foreign.dxf", None);

    let path = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.dwg", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(r["notSaved"], json!({ "3DSOLID": 1, "MLINE": 2, "REGION": 1 }));
    let msg = "Not saved in the DWG file (its writer can't write these object types yet; DXF keeps them): 1 3DSOLID, 2 MLINE, 1 REGION";
    assert!(s.log.iter().any(|l| l == msg), "{:?}", s.log);

    // DXF keeps them.
    let dxf = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.dxf", std::process::id()));
    let n = s.log.len();
    let r = s.execute("saveas", &json!({ "path": dxf.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&dxf);
    assert!(r.get("notSaved").is_none() && s.log.len() == n);

    // Exporting an image isn't a save of the drawing: no report.
    let png = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.png", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": png.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&png);
    assert!(r.get("notSaved").is_none());

    // A drawing the file keeps completely: nothing to report.
    s.open_drawing(Drawing::new_imperial(), "plain.dxf", None);
    let n = s.log.len();
    let r = s.execute("document.bytes", &json!({})).unwrap();
    assert!(r.get("notSaved").is_none() && s.log.len() == n);
}

#[test]
fn plot_preview_refuses_unresolved_raw_geometry_with_its_handle() {
    use cadcraft_engine::doc::{Entity, RawCompound, RawRecord};
    let mut session = Session::new();
    session.doc_mut().unwrap().model.push(Entity::from_raw(
        RawCompound::new(
            RawRecord::new("WIDGETX".into(), vec![RawTag { code: 5, value: "A0".into() }, RawTag { code: 330, value: "1F".into() }]).unwrap(),
            Vec::new(),
            Default::default(),
            "No exact WIDGETX adapter.".into(),
        )
        .unwrap(),
    ));
    let result = session.execute("preview", &json!({"width":64}));
    assert!(result.is_err(), "preview silently omitted raw geometry");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("A0") && message.contains("unresolved"), "{message}");
}

#[test]
fn templates_report_what_dwg_does_not_keep() {
    use cadcraft_engine::cmd::file::drops_unknown;
    assert!(drops_unknown("a.dwg") && drops_unknown("B.DWT"));
    assert!(!drops_unknown("a.dxf"));
}

#[test]
fn surface_family_save_warnings_follow_the_native_data_store() {
    use cadcraft_engine::cmd::file::{Version, not_saved_as};
    for kind in ["SURFACE", "PLANESURFACE", "EXTRUDEDSURFACE", "LOFTEDSURFACE", "REVOLVEDSURFACE", "SWEPTSURFACE", "NURBSURFACE"] {
        let mut drawing = Drawing::new_metric();
        let handle = drawing.add(&Space::Model, Common::default(), unknown(kind)).unwrap();
        std::sync::Arc::make_mut(&mut drawing.foreign).data_store = vec![
            RawTag { code: 0, value: "ACDSRECORD".into() },
            RawTag { code: 320, value: handle.hex() },
            RawTag { code: 2, value: "ASM_Data".into() },
            RawTag { code: 310, value: "00".into() },
        ];
        for version in [Version::R2013, Version::R2018] {
            assert!(not_saved_as(&drawing, version).is_empty(), "{kind} {version:?}: stored geometry is not omitted");
        }
        assert_eq!(not_saved_as(&drawing, Version::R2010).get(kind), Some(&1));
        std::sync::Arc::make_mut(&mut drawing.foreign).data_store.clear();
        assert_eq!(not_saved_as(&drawing, Version::R2018).get(kind), Some(&1));
    }
}

#[test]
fn solids_whose_geometry_the_data_store_keeps_are_not_reported_from_2013() {
    use cadcraft_engine::cmd::file::{Version, not_saved_as};
    use cadcraft_engine::doc::Handle;
    use cadcraft_engine::doc::foreign::Foreign;
    let mut d = Drawing::new_imperial();
    let kept = d.add(&Space::Model, Common::default(), unknown("3DSOLID")).unwrap();
    d.add(&Space::Model, Common::default(), unknown("3DSOLID")).unwrap();
    d.add(&Space::Model, Common::default(), unknown("REGION")).unwrap();
    let record = |h: Handle| {
        vec![
            RawTag { code: 0, value: "ACDSRECORD".into() },
            RawTag { code: 320, value: h.hex() },
            RawTag { code: 2, value: "ASM_Data".into() },
            RawTag { code: 310, value: "00".into() },
        ]
    };
    let mut foreign = Foreign::default();
    foreign.data_store = record(kept);
    d.foreign = std::sync::Arc::new(foreign);
    // The solid with a data store record keeps its geometry from 2013 on; the others don't.
    assert_eq!(not_saved_as(&d, Version::R2018), [("3DSOLID".to_string(), 1), ("REGION".to_string(), 1)].into_iter().collect());
    // Before 2013 a file has no data store: every solid loses its geometry.
    assert_eq!(not_saved_as(&d, Version::R2010), [("3DSOLID".to_string(), 2), ("REGION".to_string(), 1)].into_iter().collect());
}
