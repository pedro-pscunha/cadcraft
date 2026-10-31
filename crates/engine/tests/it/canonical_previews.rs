//! Existing-object previews retain their revision, properties, and spatial geometry.
use cadcraft_doc::{Circle, Common, Entity, EntityKind, GeometryRevision, Handle};
use cadcraft_engine::Session;
use cadcraft_geom::{OcsFrame, Vec2, Vec3};

fn fixture(canonical: bool) -> (Session, Handle) {
    let mut s = Session::new();
    let h = s.doc_mut().unwrap().new_handle();
    let mut entity = Entity::new(h, EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }));
    if canonical {
        entity.geometry_revision = GeometryRevision::CanonicalV2;
    }
    entity.common = Common { extrusion: Vec3::new(0.0, 3.0, 4.0), thickness: 2.0, ..Default::default() };
    s.doc_mut().unwrap().model.push(entity);
    (s, h)
}
#[test]
fn spatial_line_preview_keeps_both_endpoint_elevations() {
    for canonical in [false, true] {
        let mut s = Session::new();
        let h = s.doc_mut().unwrap().new_handle();
        let mut entity = Entity::new(h, EntityKind::Line(cadcraft_doc::Line { a: Vec3::ZERO, b: Vec3::new(3.0, 0.0, 4.0) }));
        if canonical {
            entity.geometry_revision = GeometryRevision::CanonicalV2;
        }
        let revision = entity.geometry_revision;
        s.doc_mut().unwrap().model.push(entity);
        s.cmdline("MOVE ALL  0,0").unwrap();
        let preview = s.preview(Vec2::new(3.0, 2.0));
        let entity = preview.iter().find(|e| e.handle == h).expect("source preview");
        assert_eq!(entity.geometry_revision, revision);
        let EntityKind::Line(line) = &entity.kind else { panic!("line") };
        assert_eq!(line.a, Vec3::new(3.0, 2.0, 0.0));
        assert_eq!(line.b, Vec3::new(6.0, 2.0, 4.0));
    }
}
fn check(s: &Session, source: &Entity, cursor: Vec2, want: Vec3) {
    let preview = s.preview(cursor);
    let entity = preview.iter().find(|e| matches!(e.kind, EntityKind::Circle(_))).expect("circle preview");
    assert_eq!(entity.geometry_revision, source.geometry_revision);
    assert_eq!(entity.common, source.common);
    let EntityKind::Circle(circle) = &entity.kind else { panic!("circle") };
    let center = if entity.geometry_revision == GeometryRevision::CanonicalV2 {
        OcsFrame::new(entity.common.extrusion).unwrap().to_world(circle.center)
    } else {
        circle.center
    };
    assert!((center - want).len() < 1e-9, "{center:?} vs {want:?}");
    if entity.geometry_revision == GeometryRevision::CanonicalV2 {
        let world = cadcraft_render::entity_world_geometry(s.doc().unwrap(), entity, &cadcraft_geom::Mat4::IDENTITY).unwrap();
        assert!(
            world
                .primitives()
                .iter()
                .any(|primitive| matches!(primitive,cadcraft_doc::WorldPrimitive::Conic{center,..} if (*center-want).len()<1e-9))
        );
    }
}
#[test]
fn mirror_drag_keeps_the_text_entity_properties() {
    let mut s = Session::new();
    let h = s.doc_mut().unwrap().new_handle();
    let mut text = Entity::new(
        h,
        EntityKind::Text(cadcraft_doc::Text {
            insert: Vec3::new(1.0, 2.0, 5.0),
            align_pt: None,
            height: 1.0,
            value: "preview".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
            backward: false,
            upside_down: false,
        }),
    );
    text.common.thickness = 2.0;
    let common = text.common.clone();
    s.doc_mut().unwrap().model.push(text);
    s.cmdline("MIRROR ALL  0,0").unwrap();
    let (_, extra) = s.drag(Vec2::new(0.0, 10.0)).unwrap();
    let text = extra.iter().find(|e| matches!(e.kind, EntityKind::Text(_))).expect("mirrored text");
    assert_eq!(text.geometry_revision, GeometryRevision::LegacyProjectedV1);
    assert_eq!(text.common, common);
}
#[test]
fn canonical_mirror_text_preview_and_drag_equal_the_committed_candidate() {
    for mode in [0, 1] {
        let mut session = Session::new();
        let handle = session.doc_mut().unwrap().new_handle();
        let mut entity = Entity::new(
            handle,
            EntityKind::Text(cadcraft_doc::Text {
                insert: Vec3::new(4.0, 5.0, 6.0),
                align_pt: None,
                height: 2.0,
                value: "A".into(),
                rotation: 30_f64.to_radians(),
                width_factor: 0.8,
                oblique: 15_f64.to_radians(),
                style: "Standard".into(),
                halign: Default::default(),
                valign: Default::default(),
                backward: false,
                upside_down: false,
            }),
        );
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common = Common { extrusion: Vec3::new(0.0, 0.6, 0.8), thickness: 2.0, ..Default::default() };
        session.doc_mut().unwrap().header.set_i64("MIRRTEXT", mode);
        session.doc_mut().unwrap().model.push(entity);
        let mut committed = Session::new();
        committed.open_drawing(session.doc().unwrap().clone(), "mirror control", None);
        committed.execute("mirror", &serde_json::json!({"handles": [handle.hex()], "p1": [0, 0], "p2": [0, 10], "erase": true})).unwrap();
        let expected = committed.doc().unwrap().entity(handle).unwrap();
        session.cmdline("MIRROR ALL  0,0").unwrap();
        let preview = session.preview(Vec2::new(0.0, 10.0));
        assert_eq!(preview.iter().find(|entity| entity.handle == handle).unwrap(), expected.as_ref());
        let (_, extra) = session.drag(Vec2::new(0.0, 10.0)).unwrap();
        assert_eq!(extra.iter().find(|entity| entity.handle == handle).unwrap(), expected.as_ref());
    }
}

fn dimension_fixture(canonical: bool) -> (Session, Entity) {
    let mut s = Session::new();
    s.script("dimlinear 0,0 10,0 5,3\n").unwrap();
    let handle = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    let mut entity = (**s.doc().unwrap().entity(handle).unwrap()).clone();
    s.doc_mut().unwrap().model = Default::default();
    entity.common.thickness = 2.0;
    if canonical {
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
        let EntityKind::Dimension(dimension) = &mut entity.kind else { panic!("dimension") };
        dimension.p13 = Vec3::new(0.0, 3.0, 4.0);
        dimension.p14 = Vec3::new(-10.0, 3.0, 4.0);
        dimension.defpt = Vec3::new(-5.0, 0.6, 5.8);
        dimension.text_mid = Vec3::new(1.0, 2.0, 5.0);
        dimension.block = None;
    }
    s.doc_mut().unwrap().model.push(entity.clone());
    (s, entity)
}

#[test]
fn dimension_text_preview_keeps_legacy_geometry_and_common() {
    let (mut s, source) = dimension_fixture(false);
    s.set_selection(vec![source.handle]);
    s.start("dimtedit").unwrap();
    let preview = s.preview(Vec2::new(7.0, 6.0));
    let entity = preview.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).expect("dimension preview");
    assert_eq!(entity.geometry_revision, source.geometry_revision);
    assert_eq!(entity.common, source.common);
    let EntityKind::Dimension(dimension) = &entity.kind else { panic!("dimension") };
    assert_eq!(dimension.text_mid, Vec3::new(7.0, 6.0, 0.0));
}

#[test]
fn dimtedit_refuses_a_tilted_canonical_dimension_before_preselection_or_picking() {
    for preselected in [false, true] {
        let (mut s, source) = dimension_fixture(true);
        let before = s.doc().unwrap().model.clone();
        if preselected {
            s.set_selection(vec![source.handle]);
            s.start("dimtedit").unwrap();
        } else {
            s.start("dimtedit").unwrap();
            s.input(cadcraft_engine::Input::Pick(vec![source.handle])).unwrap();
        }
        // Interactive input reports a refusal in the command log and keeps the selection prompt.
        assert!(s.log.iter().any(|line| line.contains("canonical")), "{:?}", s.log);
        assert!(s.log.iter().any(|line| line.contains("Nothing was changed")), "{:?}", s.log);
        assert!(s.prompt_text().contains("Select dimension"), "{}", s.prompt_text());
        assert!(s.preview(Vec2::new(7.0, 6.0)).is_empty());
        assert!(s.doc().unwrap().model.same_as(&before));
        let entity = s.doc().unwrap().entity(source.handle).unwrap();
        let EntityKind::Dimension(dimension) = &entity.kind else { panic!("dimension") };
        // OCS (1,2,5) in normal (0,3,4) is WCS (-1,1.4,5.2), not the cursor or its OCS reinterpretation.
        assert!((dimension.world_text_mid(entity.common.extrusion).unwrap() - Vec3::new(-1.0, 1.4, 5.2)).len() < 1e-9);
    }
}
#[test]
fn a_dimension_activated_mid_prompt_never_produces_a_projected_preview() {
    let (mut s, legacy) = dimension_fixture(false);
    s.set_selection(vec![legacy.handle]);
    s.start("dimtedit").unwrap();
    let (_, mut canonical) = dimension_fixture(true);
    canonical.handle = legacy.handle;
    assert!(s.doc_mut().unwrap().model.replace(canonical));
    let before = s.doc().unwrap().model.clone();
    assert!(s.preview(Vec2::new(7.0, 6.0)).is_empty());
    assert!(s.doc().unwrap().model.same_as(&before));
    let entity = s.doc().unwrap().entity(legacy.handle).unwrap();
    let EntityKind::Dimension(dimension) = &entity.kind else { panic!("dimension") };
    assert!((dimension.world_text_mid(entity.common.extrusion).unwrap() - Vec3::new(-1.0, 1.4, 5.2)).len() < 1e-9);
}

#[test]
fn hot_grip_preview_keeps_canonical_plane_and_world_center() {
    let (mut s, h) = fixture(true);
    let source = s.doc().unwrap().entity(h).unwrap().clone();
    s.start_grip_edit(&[(h, 0)]).unwrap();
    check(&s, &source, Vec2::new(2.0, 3.4), Vec3::new(2.0, 3.4, 5.2));
}
#[test]
fn array_intermediate_prompts_keep_full_entities() {
    for command in ["ARRAYRECT", "ARRAYPOLAR", "ARRAY"] {
        for canonical in [false, true] {
            let (mut s, h) = fixture(canonical);
            let mut source = (**s.doc().unwrap().entity(h).unwrap()).clone();
            source.common.thickness = 0.0;
            s.doc_mut().unwrap().model = Default::default();
            s.doc_mut().unwrap().model.push(source.clone());
            for input in [command, "all", ""] {
                s.cmdline(input).unwrap();
            }
            if command == "ARRAY" {
                s.cmdline("po").unwrap();
            }
            if command != "ARRAYRECT" {
                for input in ["0,0", "i", "2", "rot", "n"] {
                    s.cmdline(input).unwrap();
                }
            }
            let want = if command == "ARRAYRECT" {
                if canonical { Vec3::new(11.0, 1.4, 5.2) } else { Vec3::new(13.0, 2.0, 5.0) }
            } else if canonical {
                Vec3::new(1.0, -1.4, 5.2)
            } else {
                Vec3::new(-1.0, -2.0, 5.0)
            };
            check(&s, &source, Vec2::new(1.0, 0.0), want);
        }
    }
}
#[test]
fn move_and_copy_intermediate_prompts_keep_full_entities() {
    for command in ["MOVE ALL  0,0", "COPY ALL  0,0", "ROTATE ALL  0,0 R 0", "MOCORO ALL  0,0 M", "STRETCH ALL  0,0"] {
        for canonical in [false, true] {
            let (mut s, h) = fixture(canonical);
            let source = s.doc().unwrap().entity(h).unwrap().clone();
            s.cmdline(command).unwrap();
            let (want, cursor) = if command.starts_with("ROTATE") {
                if canonical { (Vec3::new(-1.0, 1.4, 5.2), Vec2::new(1.0, 0.0)) } else { (Vec3::new(1.0, 2.0, 5.0), Vec2::new(1.0, 0.0)) }
            } else if canonical {
                (Vec3::new(2.0, 3.4, 5.2), Vec2::new(3.0, 2.0))
            } else {
                (Vec3::new(4.0, 4.0, 5.0), Vec2::new(3.0, 2.0))
            };
            check(&s, &source, cursor, want);
        }
    }
}
