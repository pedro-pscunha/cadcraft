use super::*;

fn dimension_tags() -> Vec<Tag> {
    cadcraft_dxf::parse(b"5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n10\n0\n20\n0\n30\n0\n11\n0\n21\n1\n31\n0\n70\n1\n3\nStandard\n210\n0\n220\n3\n230\n4\n100\nAcDbAlignedDimension\n13\n0\n23\n0\n33\n0\n14\n-5\n24\n0\n34\n0\n52\n30\n").unwrap()
}

#[test]
fn canonical_dimension_oblique_survives_the_document_extras_pass() {
    let tags = dimension_tags();
    let (_, mut kind) = canonical_curve_from_groups("DIMENSION", &tags, &mut Drawing::default(), &Rx::default()).unwrap();
    extras::entity_extras("DIMENSION", &tags, Handle(0x200), &mut kind, &mut Rx::default(), cadcraft_doc::GeometryRevision::CanonicalV2);
    let EntityKind::Dimension(dimension) = kind else { panic!("dimension") };
    // The OCS X axis of normal (0,3,4) is (-1,0,0). The dimension runs along +OCS X.
    // Its extension line is 30 degrees from that axis, not 210 degrees from WCS X.
    assert!((dimension.marks.oblique.unwrap() - std::f64::consts::FRAC_PI_6).abs() < 1e-12);
}

#[test]
fn reference_resolution_changes_only_reference_properties_in_either_revision() {
    for canonical in [false, true] {
        let mut tags = dimension_tags();
        tags.extend(
            cadcraft_dxf::parse(b"1001\nACAD\n1000\nDSTYLE\n1002\n{\n1070\n340\n1005\nA0\n1070\n342\n1005\nB0\n1070\n345\n1005\nC0\n1002\n}\n")
                .unwrap(),
        );
        let (_, mut kind) = if canonical {
            canonical_curve_from_groups("DIMENSION", &tags, &mut Drawing::default(), &Rx::default())
        } else {
            entities::entity("DIMENSION", &tags)
        }
        .unwrap();
        let mut expected = kind.clone();
        let EntityKind::Dimension(dimension) = &mut expected else { panic!("dimension") };
        dimension.overrides =
            serde_json::json!({"textStyle": "NamedText", "arrowBlock": "NamedArrow", "dimLinetype": "NamedDash"}).as_object().unwrap().clone();
        let mut rx = Rx::default();
        rx.styles.insert("A0".into(), "NamedText".into());
        rx.brs.insert("B0".into(), "NamedArrow".into());
        rx.ltypes.insert("C0".into(), "NamedDash".into());
        extras::resolve_references("DIMENSION", &tags, Handle(0x200), &mut kind, &mut rx);
        assert_eq!(kind, expected);
        assert!(rx.dim_block_shifts.is_empty());
    }
}

#[test]
fn canonical_geometry_keeps_marks_but_does_not_schedule_a_legacy_xy_shift() {
    let mut tags = dimension_tags();
    tags.extend(
        cadcraft_dxf::parse(
            b"2\n*D1\n12\n1\n22\n2\n32\n3\n1001\nCADCRAFT\n1000\nMARKS\n1002\n{\n1000\nJOGLINE\n1010\n4\n1020\n5\n1030\n6\n1002\n}\n",
        )
        .unwrap(),
    );
    let (_, mut kind) = canonical_curve_from_groups("DIMENSION", &tags, &mut Drawing::default(), &Rx::default()).unwrap();
    let mut rx = Rx::default();
    extras::entity_extras("DIMENSION", &tags, Handle(0x200), &mut kind, &mut rx, cadcraft_doc::GeometryRevision::CanonicalV2);
    let EntityKind::Dimension(dimension) = kind else { panic!("dimension") };
    assert_eq!(dimension.marks.jog_line, Some(Vec3::new(4.0, 5.0, 6.0)));
    assert!((dimension.marks.oblique.unwrap() - std::f64::consts::FRAC_PI_6).abs() < 1e-12);
    assert_eq!(rx.dim_block_shifts.len(), 1);
    assert!(matches!(rx.dim_block_shifts[0].offset, dimension_blocks::BlockOffset::Canonical(value) if value == Vec3::new(1.0, 2.0, 3.0)));
}

#[test]
fn legacy_dimension_extras_keep_the_projected_angle() {
    let tags = dimension_tags();
    let (_, mut kind) = entities::entity("DIMENSION", &tags).unwrap();
    extras::entity_extras("DIMENSION", &tags, Handle(0x200), &mut kind, &mut Rx::default(), cadcraft_doc::GeometryRevision::LegacyProjectedV1);
    let EntityKind::Dimension(dimension) = kind else { panic!("dimension") };
    // Frozen at 5981f44f: the projected legacy angle of this source record.
    assert_eq!(dimension.marks.oblique.unwrap().to_bits(), 4_615_435_696_177_383_921);
}
