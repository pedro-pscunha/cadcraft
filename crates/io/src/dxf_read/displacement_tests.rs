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

#[test]
fn canonical_shared_block_conflicts_refuse_before_any_model_mutation() {
    use dimension_blocks::{BlockOffset, DimensionBlockShift};
    let mut drawing = drawing(GeometryRevision::CanonicalV2);
    let mut other = Block::new("*OTHER");
    let mut child = Entity::new(Handle(0x501), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) }));
    child.geometry_revision = GeometryRevision::CanonicalV2;
    other.entities.push(child);
    drawing.blocks.insert("*OTHER".into(), std::sync::Arc::new(other));
    let before = drawing.clone();
    let rx = Rx {
        dim_block_shifts: vec![
            DimensionBlockShift { block: "*D1".into(), handle: Handle(0x200), offset: BlockOffset::Canonical(Vec3::ZERO) },
            DimensionBlockShift { block: "*OTHER".into(), handle: Handle(0x201), offset: BlockOffset::Canonical(Vec3::new(1.0, 2.0, 3.0)) },
            DimensionBlockShift { block: "*D1".into(), handle: Handle(0x202), offset: BlockOffset::Canonical(Vec3::new(4.0, 5.0, 6.0)) },
        ],
        ..Default::default()
    };
    let error = shift_dimension_blocks(&mut drawing, &rx).unwrap_err();
    assert!(matches!(error, IoError::DimensionBlockDisplacement { first: Handle(0x200), other: Handle(0x202), .. }));
    assert_eq!(drawing, before);
}

#[test]
fn legacy_shared_block_conflicts_keep_the_existing_no_normalization_rule() {
    use dimension_blocks::{BlockOffset, DimensionBlockShift};
    let mut drawing = drawing(GeometryRevision::LegacyProjectedV1);
    let before = drawing.clone();
    let rx = Rx {
        dim_block_shifts: vec![
            DimensionBlockShift { block: "*D1".into(), handle: Handle(0x200), offset: BlockOffset::Legacy(cadcraft_geom::Vec2::new(1.0, 2.0)) },
            DimensionBlockShift { block: "*D1".into(), handle: Handle(0x202), offset: BlockOffset::Legacy(cadcraft_geom::Vec2::new(4.0, 5.0)) },
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
    // Frozen at 5177f948 before the Canonical displacement change.
    assert_eq!(hash, "3b09a2204cd6a671401be7bada6c6800a912fb02d0be476818b7615635f09c27");
    assert_eq!(bytes.len(), 17676);
}
