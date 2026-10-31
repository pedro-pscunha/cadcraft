use super::*;
use cadcraft_doc::GeometryRevision;
use cadcraft_geom::Mat4;
use sha2::{Digest, Sha256};

fn source_records() -> (Vec<Tag>, Vec<Tag>) {
    let dimension = cadcraft_dxf::parse(b"5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n2\n*D1\n10\n0\n20\n-8\n30\n6\n11\n5\n21\n11\n31\n0\n12\n4\n22\n5\n32\n6\n70\n33\n1\n \n3\nStandard\n42\n10\n210\n0\n220\n3\n230\n4\n100\nAcDbAlignedDimension\n13\n0\n23\n0\n33\n0\n14\n-10\n24\n0\n34\n0\n").unwrap();
    let line = cadcraft_dxf::parse(b"5\n301\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n1\n20\n2\n30\n3\n11\n2\n21\n4\n31\n3\n").unwrap();
    (dimension, line)
}

fn drawing(revision: GeometryRevision) -> Drawing {
    let (dimension, line) = source_records();
    drawing_from_source(revision, dimension, line)
}

fn drawing_from_source(revision: GeometryRevision, dimension: Vec<Tag>, line: Vec<Tag>) -> Drawing {
    let mut drawing = Drawing::new_metric();
    let mut rx = Rx::default();
    let (common, mut kind) = if revision == GeometryRevision::CanonicalV2 {
        canonical_curve_from_groups("DIMENSION", &dimension, &mut drawing, &rx)
    } else {
        entities::entity("DIMENSION", &dimension)
    }
    .unwrap();
    extras::entity_extras("DIMENSION", &dimension, Handle(0x200), &mut kind, &mut rx, revision);
    drawing.model.push(Entity { geometry_revision: revision, handle: Handle(0x200), common, kind });
    let (common, kind) = if revision == GeometryRevision::CanonicalV2 {
        canonical_curve_from_groups("LINE", &line, &mut drawing, &rx)
    } else {
        entities::entity("LINE", &line)
    }
    .unwrap();
    let mut block = Block::new("*D1");
    block.entities.push(Entity { geometry_revision: revision, handle: Handle(0x301), common, kind });
    drawing.blocks.insert("*D1".into(), std::sync::Arc::new(block));
    shift_dimension_blocks(&mut drawing, &rx).unwrap();
    let mut block_tags = vec![Tag::s(0, "BLOCK"), Tag::s(5, "300"), Tag::s(2, "*D1"), Tag::i(70, 1), Tag::s(0, "LINE")];
    block_tags.extend(line);
    block_tags.extend([Tag::s(0, "ENDBLK"), Tag::s(5, "302")]);
    let mut entity_tags = vec![Tag::s(0, "DIMENSION")];
    entity_tags.extend(dimension);
    crate::dxf_raw::read(
        &[Section { name: "BLOCKS".into(), tags: block_tags }, Section { name: "ENTITIES".into(), tags: entity_tags }],
        &mut drawing,
    );
    drawing
}

fn decoded(bytes: &[u8], name: &str) -> Vec<Section> {
    let dxf = if name.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(bytes).unwrap() } else { bytes.to_vec() };
    sections(&cadcraft_dxf::parse(&dxf).unwrap())
}

fn point(tags: &[Tag], code: i32) -> Vec3 {
    Vec3::new(
        tags.iter().find(|tag| tag.code == code).map_or(0.0, Tag::f64),
        tags.iter().find(|tag| tag.code == code + 10).map_or(0.0, Tag::f64),
        tags.iter().find(|tag| tag.code == code + 20).map_or(0.0, Tag::f64),
    )
}

#[test]
fn legacy_native_dimension_at_z3_displays_local_zero_children_without_changing_save_bytes() {
    // Numeric control from AutoCAD LT2025, tilted UCS (0,.6,.8), ELEVATION3, DIMLINEAR1180.
    // This synthetic excerpt is not an AutoCAD-produced file.
    let dimension = cadcraft_dxf::parse(b"5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n2\n*D1\n10\n-10\n20\n-8.600000000000001\n30\n10.2\n11\n5\n21\n14.875\n31\n3\n70\n32\n3\nStandard\n210\n0\n220\n0.6\n230\n0.8\n100\nAcDbAlignedDimension\n13\n0\n23\n-6.2\n33\n8.4\n14\n-10\n24\n-6.2\n34\n8.4\n50\n0\n100\nAcDbRotatedDimension\n").unwrap();
    let line = cadcraft_dxf::parse(b"5\n301\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n10.625\n30\n0\n11\n0\n21\n14.25\n31\n0\n").unwrap();
    let drawing = drawing_from_source(GeometryRevision::LegacyProjectedV1, dimension, line);
    let before_dxf = crate::write(&drawing, "native-plane.dxf").unwrap();
    let before_dwg = crate::write(&drawing, "native-plane.dwg").unwrap();
    let list = cadcraft_render::build(&drawing, &cadcraft_doc::Space::Model, &cadcraft_render::Options { text: false, ..Default::default() });
    assert!(list.geometry_refusals.is_empty(), "{:?}", list.geometry_refusals);
    // The native plane projects OCS y to -.8*y and Z to +.6*Z, so Z3 shifts both endpoints by +1.8.
    for y in [-6.7, -9.6] {
        assert!(
            list.verts.iter().any(|point| (*point - cadcraft_geom::Vec2::new(0.0, y)).len() < 1e-10),
            "missing native elevated endpoint {y}: {:?}",
            list.verts
        );
    }
    assert_eq!(crate::write(&drawing, "native-plane.dxf").unwrap(), before_dxf);
    assert_eq!(crate::write(&drawing, "native-plane.dwg").unwrap(), before_dwg);
}

#[test]
fn canonical_dimension_picture_uses_the_plane_elevation_when_clone_z_is_not_serialized() {
    let (mut dimension, mut line) = source_records();
    dimension.retain(|tag| !matches!(tag.code, 12 | 22 | 32));
    for (code, value) in [(30, 3.0), (31, 3.0), (33, 3.0), (34, 3.0), (210, 0.0), (220, 0.0), (230, 1.0)] {
        *dimension.iter_mut().find(|tag| tag.code == code).unwrap() = Tag::f(code, value);
    }
    for tag in line.iter_mut().filter(|tag| matches!(tag.code, 30 | 31)) {
        *tag = Tag::f(tag.code, 0.0);
    }
    let drawing = drawing_from_source(GeometryRevision::CanonicalV2, dimension, line);
    let entity = drawing.entity(Handle(0x200)).unwrap();
    let geometry = cadcraft_render::entity_world_geometry(&drawing, entity, &Mat4::IDENTITY).unwrap();
    assert!(geometry.primitives().iter().any(|primitive| matches!(primitive, cadcraft_doc::WorldPrimitive::Line { a, b }
        if (*a - Vec3::new(1.0, 2.0, 3.0)).len() < 1e-10 && (*b - Vec3::new(2.0, 4.0, 3.0)).len() < 1e-10)));
    for name in ["source-elevated.dxf", "source-elevated.dwg"] {
        let bytes = crate::write(&drawing, name).unwrap();
        let sections = decoded(&bytes, name);
        let entities = sections.iter().find(|section| section.name == "ENTITIES").unwrap();
        let dimension = records(&entities.tags).into_iter().find(|(kind, _)| kind == "DIMENSION").unwrap().1;
        assert_eq!(point(&dimension, 11).z, 3.0, "{name}");
        if name.ends_with("dxf") {
            assert!(!dimension.iter().any(|tag| matches!(tag.code, 12 | 22 | 32)), "derived plane placement must not invent serialized clone groups");
        } else {
            assert_eq!(point(&dimension, 12), Vec3::ZERO, "the DWG clone point stays default; the common elevation supplies its plane");
        }
    }
}

#[test]
fn canonical_clone_displacement_keeps_the_full_ocs_z_in_display() {
    let drawing = drawing(GeometryRevision::CanonicalV2);
    let list = cadcraft_render::build(&drawing, &cadcraft_doc::Space::Model, &cadcraft_render::Options { text: false, ..Default::default() });
    assert!(list.geometry_refusals.is_empty(), "{:?}", list.geometry_refusals);
    // Source line (1,2,3)-(2,4,3), plus OCS clone point (4,5,6), under normal (0,3,4).
    // The independent basis is X=(-1,0,0), Y=(0,-0.8,0.6), Z=(0,0.6,0.8).
    for expected in [cadcraft_geom::Vec2::new(-5.0, -0.2), cadcraft_geom::Vec2::new(-6.0, -1.8)] {
        assert!(list.verts.iter().any(|point| (*point - expected).len() < 1e-10), "missing {expected:?}: {:?}", list.verts);
    }
}

#[test]
fn canonical_clone_displacement_replays_once_then_clears_when_the_block_is_regenerated() {
    let mut drawing = drawing(GeometryRevision::CanonicalV2);
    let bytes = crate::write(&drawing, "clone.dxf").unwrap();
    let sections = decoded(&bytes, "clone.dxf");
    let dimension = sections.iter().find(|section| section.name == "ENTITIES").unwrap();
    let dimension = records(&dimension.tags).into_iter().find(|(kind, _)| kind == "DIMENSION").unwrap().1;
    assert_eq!(point(&dimension, 12), Vec3::new(4.0, 5.0, 6.0));
    let blocks = sections.iter().find(|section| section.name == "BLOCKS").unwrap();
    let line = records(&blocks.tags)
        .into_iter()
        .find(|(kind, tags)| kind == "LINE" && tags.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()) == Some(Handle(0x301))))
        .unwrap()
        .1;
    assert_eq!(point(&line, 10), Vec3::new(1.0, 2.0, 3.0), "source block, not a second displacement");
    let mut translation = Mat4::IDENTITY;
    translation.m[0][3] = 10.0;
    translation.m[1][3] = 20.0;
    translation.m[2][3] = 30.0;
    drawing.transform_entity(Handle(0x200), &translation).unwrap();
    assert!(!crate::dxf_geometry::eligible(&drawing).contains_key(&Handle(0x200)));
    for name in ["edited-clone.dxf", "edited-clone.dwg"] {
        let bytes = crate::write(&drawing, name).unwrap();
        let sections = decoded(&bytes, name);
        let dimension = sections.iter().find(|section| section.name == "ENTITIES").unwrap();
        let dimension = records(&dimension.tags).into_iter().find(|(kind, _)| kind == "DIMENSION").unwrap().1;
        assert_eq!(point(&dimension, 12), Vec3::ZERO, "{name}: the regenerated picture must not repeat the source displacement");
        assert!((point(&dimension, 13) - Vec3::new(10.0, 20.0, 30.0)).len() < 1e-10, "{name}");
    }
}

#[test]
fn canonical_clone_dwg_follows_autocads_wcs_2rd_oracle() {
    let drawing = drawing(GeometryRevision::CanonicalV2);
    let bytes = crate::write(&drawing, "clone.dwg").unwrap();
    let sections = decoded(&bytes, "clone.dwg");
    let dimension = sections.iter().find(|section| section.name == "ENTITIES").unwrap();
    let dimension = records(&dimension.tags).into_iter().find(|(kind, _)| kind == "DIMENSION").unwrap().1;
    // AutoCAD OPEN/SAVEAS/reopen: OCS (4,5,6) -> WCS XY (-4,-0.4), implicit WCS Z=0 -> OCS (4,0.32,-0.24).
    assert!((point(&dimension, 12) - Vec3::new(4.0, 0.32, -0.24)).len() < 1e-10, "{:?}", point(&dimension, 12));
}

fn shared_clone_drawing() -> Drawing {
    let (source, line) = source_records();
    let mut drawing = Drawing::new_metric();
    let mut rx = Rx::default();
    let mut entity_tags = Vec::new();
    for (handle, offset) in [(Handle(0x200), Vec3::new(4.0, 5.0, 0.0)), (Handle(0x202), Vec3::new(14.0, 15.0, 0.0))] {
        let mut tags = source.clone();
        for (code, value) in [(12, offset.x), (22, offset.y), (32, offset.z), (210, 0.0), (220, 0.0), (230, 1.0)] {
            *tags.iter_mut().find(|tag| tag.code == code).unwrap() = Tag::f(code, value);
        }
        *tags.iter_mut().find(|tag| tag.code == 5).unwrap() = Tag::s(5, handle.hex());
        let (common, mut kind) = canonical_curve_from_groups("DIMENSION", &tags, &mut drawing, &rx).unwrap();
        extras::entity_extras("DIMENSION", &tags, handle, &mut kind, &mut rx, GeometryRevision::CanonicalV2);
        drawing.model.push(Entity { geometry_revision: GeometryRevision::CanonicalV2, handle, common, kind });
        entity_tags.push(Tag::s(0, "DIMENSION"));
        entity_tags.extend(tags);
    }
    let (common, kind) = canonical_curve_from_groups("LINE", &line, &mut drawing, &rx).unwrap();
    let mut block = Block::new("*D1");
    block.entities.push(Entity { geometry_revision: GeometryRevision::CanonicalV2, handle: Handle(0x301), common, kind });
    drawing.blocks.insert("*D1".into(), std::sync::Arc::new(block));
    shift_dimension_blocks(&mut drawing, &rx).unwrap();
    let mut block_tags = vec![Tag::s(0, "BLOCK"), Tag::s(5, "300"), Tag::s(2, "*D1"), Tag::i(70, 1), Tag::s(0, "LINE")];
    block_tags.extend(line);
    block_tags.extend([Tag::s(0, "ENDBLK"), Tag::s(5, "302")]);
    crate::dxf_raw::read(
        &[Section { name: "BLOCKS".into(), tags: block_tags }, Section { name: "ENTITIES".into(), tags: entity_tags }],
        &mut drawing,
    );
    drawing
}

#[test]
fn canonical_shared_block_keeps_each_dimension_clone_point_and_the_source_picture() {
    let drawing = shared_clone_drawing();
    for name in ["shared-clones.dxf", "shared-clones.dwg"] {
        let sections = decoded(&crate::write(&drawing, name).unwrap(), name);
        let entities = sections.iter().find(|section| section.name == "ENTITIES").unwrap();
        let dimensions: Vec<_> = records(&entities.tags).into_iter().filter(|(kind, _)| kind == "DIMENSION").collect();
        assert_eq!(dimensions.len(), 2, "{name}");
        for ((_, tags), expected) in dimensions.iter().zip([Vec3::new(4.0, 5.0, 0.0), Vec3::new(14.0, 15.0, 0.0)]) {
            assert_eq!(point(tags, 12), expected, "{name}");
            assert!(tags.iter().any(|tag| tag.code == 2 && tag.str() == "*D1"), "{name}");
        }
        let blocks = sections.iter().find(|section| section.name == "BLOCKS").unwrap();
        let lines: Vec<_> = records(&blocks.tags).into_iter().filter(|(kind, _)| kind == "LINE").collect();
        assert_eq!(lines.len(), 1, "{name}: shared picture must not split");
        assert_eq!(point(&lines[0].1, 10), Vec3::new(1.0, 2.0, 3.0), "{name}");
        assert_eq!(point(&lines[0].1, 11), Vec3::new(2.0, 4.0, 3.0), "{name}");
        let source = source_records().1;
        let retained: Vec<_> = lines[0].1.iter().filter(|tag| source.iter().any(|original| original.code == tag.code)).collect();
        assert_eq!(retained, source.iter().collect::<Vec<_>>(), "{name}: source block tags");
    }
    for (handle, expected) in [(Handle(0x200), Vec3::new(5.0, 7.0, 3.0)), (Handle(0x202), Vec3::new(15.0, 17.0, 3.0))] {
        let geometry = cadcraft_render::entity_world_geometry(&drawing, drawing.entity(handle).unwrap(), &Mat4::IDENTITY).unwrap();
        assert!(
            geometry
                .primitives()
                .iter()
                .any(|primitive| matches!(primitive, cadcraft_doc::WorldPrimitive::Line { a, .. } if (*a - expected).len() < 1e-10))
        );
    }
    let list = cadcraft_render::build(&drawing, &cadcraft_doc::Space::Model, &cadcraft_render::Options { text: false, ..Default::default() });
    assert!(list.geometry_refusals.is_empty(), "{:?}", list.geometry_refusals);
    for expected in [
        cadcraft_geom::Vec2::new(5.0, 7.0),
        cadcraft_geom::Vec2::new(6.0, 9.0),
        cadcraft_geom::Vec2::new(15.0, 17.0),
        cadcraft_geom::Vec2::new(16.0, 19.0),
    ] {
        assert!(list.verts.iter().any(|point| (*point - expected).len() < 1e-10), "missing {expected:?}");
    }
}

#[test]
fn legacy_shared_block_conflicts_keep_the_existing_no_normalization_rule() {
    use dimension_blocks::DimensionBlockShift;
    let mut drawing = drawing(GeometryRevision::LegacyProjectedV1);
    let before = drawing.clone();
    let rx = Rx {
        dim_block_shifts: vec![
            DimensionBlockShift { block: "*D1".into(), offset: cadcraft_geom::Vec2::new(1.0, 2.0) },
            DimensionBlockShift { block: "*D1".into(), offset: cadcraft_geom::Vec2::new(4.0, 5.0) },
        ],
        ..Default::default()
    };
    shift_dimension_blocks(&mut drawing, &rx).unwrap();
    assert_eq!(drawing, before);
}

#[test]
fn legacy_clone_normalization_keeps_the_frozen_output() {
    let drawing = drawing(GeometryRevision::LegacyProjectedV1);
    let bytes = crate::write(&drawing, "legacy-clone.dxf").unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    // Measured independently on the clean accepted 75c982ad base; re-measured when an MLEADERSTYLE began to keep its group 298, and when it moved last.
    assert_eq!(hash, "64ed84595be6f71d691d532ee63a6c6005a2be39dfb90573ba8a015070ddd9fd");
    assert_eq!(bytes.len(), 17767);
}
