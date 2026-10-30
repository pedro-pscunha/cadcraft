use cadcraft_doc::{DimKind, Drawing, EntityKind, EntityStore, GeometryRevision, Handle, Space};
use cadcraft_geom::Vec3;

fn world(p: Vec3) -> Vec3 {
    // Independent OCS basis for normal (0,3,4): X=(-1,0,0), Y=(0,-0.8,0.6).
    Vec3::new(-p.x, -0.8 * p.y + 0.6 * p.z, 0.6 * p.y + 0.8 * p.z)
}

fn edited(kind: DimKind, revision: GeometryRevision) -> Drawing {
    let mut d = cadcraft_io::read_dxf(b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nDIMENSION\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n10\n0\n20\n-0.8\n30\n0.6\n11\n0\n21\n0\n31\n0\n70\n32\n210\n0\n220\n3\n230\n4\n100\nAcDbAlignedDimension\n13\n0\n23\n0\n33\n0\n14\n-1\n24\n0\n34\n0\n50\n0\n100\nAcDbRotatedDimension\n0\nENDSEC\n0\nEOF\n").unwrap();
    let style = d.dim_styles.first_mut().unwrap();
    style.arrow_size = 0.0;
    style.ext_offset = 0.0;
    style.ext_extend = 0.0;
    style.text_gap = 0.0;
    d.modify_entity(Handle(0x200), |e| {
        let EntityKind::Dimension(dm) = &mut e.kind else { panic!("dimension") };
        dm.kind = kind;
        dm.text = " ".into();
        dm.block = None;
        dm.p13 = Vec3::ZERO;
        match kind {
            DimKind::Linear { .. } => {
                dm.defpt = world(Vec3::new(0.0, 5.0, 0.0));
                dm.p14 = world(Vec3::new(6.0, 8.0, 0.0));
            }
            DimKind::Aligned => {
                dm.defpt = world(Vec3::new(0.0, 5.0, 0.0));
                dm.p14 = world(Vec3::new(3.0, 4.0, 0.0));
            }
            _ => {
                dm.defpt = Vec3::ZERO;
                dm.p14 = world(Vec3::new(10.0, 0.0, 0.0));
                dm.p15 = world(Vec3::new(5.0, 5.0, 0.0));
                dm.p16 = Vec3::new(3.0, 1.0, 0.0);
            }
        }
    })
    .unwrap();
    // Canonical reader activation stays off. Build that control by hand after the legacy edit.
    let mut entity = d.entity(Handle(0x200)).unwrap().as_ref().clone();
    entity.geometry_revision = revision;
    d.model.replace(entity);
    d
}

fn expected(kind: DimKind) -> [Vec3; 2] {
    match kind {
        DimKind::Linear { .. } => [Vec3::new(0.0, 5.0, 0.0), Vec3::new(6.0, 5.0, 0.0)],
        DimKind::Aligned => [Vec3::new(-2.4, 1.8, 0.0), Vec3::new(0.6, 5.8, 0.0)],
        _ => [Vec3::new(10.0_f64.sqrt(), 0.0, 0.0), Vec3::new(5.0_f64.sqrt(), 5.0_f64.sqrt(), 0.0)],
    }
}

fn has(points: impl IntoIterator<Item = Vec3>, expected: Vec3) -> bool {
    points.into_iter().any(|p| (p - expected).len() < 1e-8)
}

fn canvas_contains(d: &Drawing, points: &[Vec3]) {
    let list = cadcraft_render::build(d, &Space::Model, &cadcraft_render::Options { text: false, ..Default::default() });
    assert!(list.geometry_refusals.is_empty(), "{:?}", list.geometry_refusals);
    for point in points {
        assert!(list.verts.iter().any(|v| (*v - point.xy()).len() < 1e-8), "canvas lacks {point:?}: {:?}", list.verts);
    }
}

#[test]
fn edited_tilted_dimension_blocks_use_local_ocs_in_dxf_and_dwg() {
    for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
        for kind in [DimKind::Linear { rotation: 0.0 }, DimKind::Aligned, DimKind::Angular] {
            for elevation in [0.0, 2.0] {
                let mut d = edited(kind, revision);
                let mut entity = d.entity(Handle(0x200)).unwrap().as_ref().clone();
                let EntityKind::Dimension(dm) = &mut entity.kind else { panic!("dimension") };
                let lift = world(Vec3::new(0.0, 0.0, elevation));
                dm.defpt = dm.defpt + lift;
                dm.p13 = dm.p13 + lift;
                dm.p14 = dm.p14 + lift;
                dm.p15 = dm.p15 + lift;
                dm.p16.z = elevation;
                dm.text_mid.z = elevation;
                d.model.replace(entity);
                let local_points = expected(kind).map(|p| Vec3::new(p.x, p.y, elevation));
                for name in ["plane.dxf", "plane.dwg"] {
                    let bytes = cadcraft_io::write(&d, name).unwrap();
                    let reopened = cadcraft_io::read(&bytes, name).unwrap();
                    let EntityKind::Dimension(dm) = &reopened.entity(Handle(0x200)).unwrap().kind else { panic!("dimension") };
                    let block = reopened.block(dm.block.as_ref().unwrap()).unwrap();
                    let points: Vec<_> = block
                        .entities
                        .iter()
                        .filter_map(|e| match &e.kind {
                            EntityKind::Line(line) => Some([line.a, line.b]),
                            _ => None,
                        })
                        .flatten()
                        .collect();
                    for point in local_points {
                        assert!(has(points.iter().copied(), point), "{name} {revision:?} {kind:?}: block lacks local {point:?}: {points:?}");
                    }
                    assert!(points.iter().all(|p| (p.z - elevation).abs() < 1e-10));
                    assert_eq!(reopened.entity(Handle(0x200)).unwrap().common.extrusion, Vec3::new(0.0, 3.0, 4.0));
                    canvas_contains(&reopened, &local_points.map(world));
                }
                canvas_contains(&d, &local_points.map(world));
            }
        }
    }
}

#[test]
fn an_angular_picture_uses_the_arc_elevation_not_the_definition_line_elevation() {
    for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
        let mut d = edited(DimKind::Angular, revision);
        let mut entity = d.entity(Handle(0x200)).unwrap().as_ref().clone();
        let EntityKind::Dimension(dm) = &mut entity.kind else { panic!("dimension") };
        let lift = world(Vec3::new(0.0, 0.0, 2.0));
        dm.defpt = dm.defpt + lift;
        dm.p13 = dm.p13 + lift;
        dm.p14 = dm.p14 + lift;
        dm.p15 = dm.p15 + lift;
        assert_eq!(dm.p16.z, 0.0);
        d.model.replace(entity);
        canvas_contains(&d, &expected(DimKind::Angular).map(world));
        for name in ["arc-plane.dxf", "arc-plane.dwg"] {
            let bytes = cadcraft_io::write(&d, name).unwrap();
            let reopened = cadcraft_io::read(&bytes, name).unwrap();
            let EntityKind::Dimension(dm) = &reopened.entity(Handle(0x200)).unwrap().kind else { panic!("dimension") };
            let block = reopened.block(dm.block.as_ref().unwrap()).unwrap();
            let points: Vec<_> = block
                .entities
                .iter()
                .filter_map(|e| if let EntityKind::Line(line) = &e.kind { Some([line.a, line.b]) } else { None })
                .flatten()
                .collect();
            for point in expected(DimKind::Angular) {
                assert!(has(points.iter().copied(), point), "{name}: no local arc endpoint {point:?}");
            }
            assert!(points.iter().all(|p| p.z == 0.0));
            canvas_contains(&reopened, &expected(DimKind::Angular).map(world));
        }
    }
}

#[test]
fn regenerated_text_and_arrowheads_use_the_same_local_axes_as_the_block_lines() {
    for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
        let mut d = edited(DimKind::Linear { rotation: 0.0 }, revision);
        d.dim_styles.first_mut().unwrap().arrow_size = 1.0;
        let mut entity = d.entity(Handle(0x200)).unwrap().as_ref().clone();
        let EntityKind::Dimension(dm) = &mut entity.kind else { panic!("dimension") };
        dm.text.clear();
        dm.user_text_pos = true;
        dm.text_mid = Vec3::new(2.0, 7.0, 0.0);
        d.model.replace(entity);
        for name in ["labels.dxf", "labels.dwg"] {
            let bytes = cadcraft_io::write(&d, name).unwrap();
            let reopened = cadcraft_io::read(&bytes, name).unwrap();
            let EntityKind::Dimension(dm) = &reopened.entity(Handle(0x200)).unwrap().kind else { panic!("dimension") };
            let block = reopened.block(dm.block.as_ref().unwrap()).unwrap();
            assert!(block.entities.iter().all(|e| e.common.extrusion == Vec3::Z));
            let text = block.entities.iter().find_map(|e| if let EntityKind::MText(text) = &e.kind { Some(text) } else { None }).unwrap();
            assert!((text.insert - Vec3::new(2.0, 7.0, 0.0)).len() < 1e-9, "{name}: {:?}", text.insert);
            let corners: Vec<_> = block
                .entities
                .iter()
                .filter_map(|e| if let EntityKind::Solid(solid) = &e.kind { Some(solid.corners) } else { None })
                .flatten()
                .collect();
            for tip in [Vec3::new(0.0, 5.0, 0.0), Vec3::new(6.0, 5.0, 0.0)] {
                assert!(has(corners.iter().copied(), tip), "{name}: no local arrow tip {tip:?}");
            }
        }
    }
}

#[test]
fn a_dimension_with_a_source_block_refuses_an_invalid_plane_without_a_flat_fallback() {
    let d = edited(DimKind::Linear { rotation: 0.0 }, GeometryRevision::LegacyProjectedV1);
    let bytes = cadcraft_io::write(&d, "source.dxf").unwrap();
    for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
        let mut reopened = cadcraft_io::read(&bytes, "source.dxf").unwrap();
        let mut entity = reopened.entity(Handle(0x200)).unwrap().as_ref().clone();
        entity.geometry_revision = revision;
        entity.common.extrusion = Vec3::ZERO;
        reopened.model.replace(entity);
        let list = cadcraft_render::build(&reopened, &Space::Model, &Default::default());
        assert!(list.verts.is_empty() && list.tris.is_empty());
        assert_eq!(list.geometry_refusals.len(), 1);
        assert_eq!(list.geometry_refusals[0].handle, Handle(0x200));
    }
}

#[test]
fn flat_legacy_dimension_canvas_vertices_keep_their_exact_bits() {
    let mut d = edited(DimKind::Linear { rotation: 0.0 }, GeometryRevision::LegacyProjectedV1);
    let mut entity = d.entity(Handle(0x200)).unwrap().as_ref().clone();
    entity.common.extrusion = Vec3::Z;
    let EntityKind::Dimension(dm) = &mut entity.kind else { panic!("dimension") };
    dm.defpt = Vec3::new(0.0, 5.0, 0.0);
    dm.p13 = Vec3::ZERO;
    dm.p14 = Vec3::new(6.0, 8.0, 0.0);
    d.model.replace(entity);
    let list = cadcraft_render::build(&d, &Space::Model, &cadcraft_render::Options { text: false, ..Default::default() });
    let bits: Vec<_> = list.verts.iter().map(|p| (p.x.to_bits(), p.y.to_bits())).collect();
    let expected = [(0.0_f64, 0.0_f64), (0.0, 5.0), (6.0, 8.0), (6.0, 5.0), (0.0, 5.0), (6.0, 5.0)].map(|(x, y)| (x.to_bits(), y.to_bits())).to_vec();
    assert_eq!(bits, expected);
    assert!(list.geometry_refusals.is_empty());
}

#[test]
#[ignore = "requires the external AutoCAD DIM1 fixture via CADCRAFT_AUTOCAD_DIM_DXF"]
fn measured_autocad_dimension_blocks_place_definition_points_and_visible_lines_in_wcs() {
    let path = std::env::var("CADCRAFT_AUTOCAD_DIM_DXF").unwrap();
    let bytes = std::fs::read(path).unwrap();
    let drawing = cadcraft_io::read(&bytes, "measured.dxf").unwrap();
    // Independent OCS basis for the measured normal (-sqrt(1/2),0,sqrt(1/2)).
    let place = |p: Vec3| Vec3::new((p.y - p.z) * std::f64::consts::FRAC_1_SQRT_2, -p.x, (p.y + p.z) * std::f64::consts::FRAC_1_SQRT_2);
    let mut placements = Vec::new();
    for (handle, name) in [(0x1182, "*D1"), (0x119A, "*D2"), (0x11B2, "*D3")] {
        let entity = drawing.entity(Handle(handle)).unwrap();
        assert_eq!(entity.common.extrusion, Vec3::new(-std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2));
        let EntityKind::Dimension(dm) = &entity.kind else { panic!("dimension") };
        assert_eq!(dm.block.as_deref(), Some(name));
        let block = drawing.block(name).unwrap();
        let points: Vec<_> = block
            .entities
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Point(point) => Some(place(point.p)),
                _ => None,
            })
            .collect();
        for point in [dm.defpt, dm.p13, dm.p14] {
            assert!(has(points.iter().copied(), point), "{handle:X}: no placed definition POINT at {point:?}");
        }
        if matches!(dm.kind, DimKind::Angular) {
            assert!(has(points.iter().copied(), dm.p15));
            assert!(has(points.iter().copied(), place(dm.p16)));
        }
        let lines: Vec<_> = block
            .entities
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Line(line) => Some([place(line.a), place(line.b)]),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(!lines.is_empty());
        placements.push((handle, lines.into_iter().chain(points).collect::<Vec<_>>()));
    }
    for (handle, points) in placements {
        for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
            let mut isolated = drawing.clone();
            isolated.model = EntityStore::default();
            let mut entity = drawing.entity(Handle(handle)).unwrap().as_ref().clone();
            entity.geometry_revision = revision;
            isolated.model.push(entity);
            canvas_contains(&isolated, &points);
        }
    }
}
