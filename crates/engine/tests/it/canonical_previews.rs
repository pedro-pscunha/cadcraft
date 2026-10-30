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
fn dimension_text_preview_keeps_its_revision_and_common() {
    for canonical in [false, true] {
        let mut s = Session::new();
        s.script("dimlinear 0,0 10,0 5,3\n").unwrap();
        let old = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
        let mut entity = (**s.doc().unwrap().entity(old).unwrap()).clone();
        s.doc_mut().unwrap().model = Default::default();
        entity.common.thickness = 2.0;
        if canonical {
            entity.geometry_revision = GeometryRevision::CanonicalV2;
        }
        let source = entity.clone();
        s.doc_mut().unwrap().model.push(entity);
        s.set_selection(vec![old]);
        s.cmdline("DIMTEDIT").unwrap();
        let preview = s.preview(Vec2::new(7.0, 6.0));
        let entity = preview.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).expect("dimension preview");
        assert_eq!(entity.geometry_revision, source.geometry_revision);
        assert_eq!(entity.common, source.common);
    }
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
