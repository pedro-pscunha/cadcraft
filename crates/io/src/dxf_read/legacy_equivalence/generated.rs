//! Structural variants use the normal writer to produce complete synthetic source contexts.
use super::{LegacyDisposition, classify_legacy_unit, oracle};
use cadcraft_doc::{
    DimKind, DimMarks, Dimension, Drawing, Entity, EntityKind, GeometryRevision, Handle, Hatch, HatchEdge, HatchLoop, HatchSpline, Helix,
    HelixConstrain, HelixFrame, Image, Leader, LeaderFrame, LeaderLinks, MLine, MLineJust, MLineStyle, MLineVectors, MText, Table, TableCell,
    Wipeout, WipeoutFrame,
};
use cadcraft_geom::{Vec2, Vec3};

struct Variant {
    name: &'static str,
    handle: Handle,
    source: Vec<u8>,
}

fn add(drawing: &mut Drawing, kind: EntityKind, revision: GeometryRevision) -> Handle {
    let handle = drawing.new_handle();
    let mut entity = Entity::new(handle, kind);
    entity.geometry_revision = revision;
    drawing.model.push(entity);
    handle
}

fn variant(name: &'static str, kind: EntityKind) -> Variant {
    let mut drawing = Drawing::new_metric();
    let handle = add(&mut drawing, kind, GeometryRevision::CanonicalV2);
    Variant { name, handle, source: crate::write_dxf(&drawing).into_bytes() }
}

fn variants() -> Vec<Variant> {
    let spline = HatchSpline {
        degree: 1,
        rational: false,
        periodic: false,
        knots: vec![0.0, 0.0, 1.0, 1.0],
        control: vec![Vec2::ZERO, Vec2::X],
        weights: Vec::new(),
        fit: Vec::new(),
        start_tangent: Vec2::ZERO,
        end_tangent: Vec2::ZERO,
    };
    let hatch = Hatch {
        pattern: "SOLID".into(),
        solid: true,
        loops: vec![HatchLoop {
            flags: 1,
            outer: true,
            edges: vec![HatchEdge::Spline(spline), HatchEdge::Line { a: Vec2::X, b: Vec2::ZERO }],
            ..HatchLoop::default()
        }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    };
    let wipeout = Wipeout {
        boundary: Vec::new(),
        display: Default::default(),
        image: None,
        frame: Some(WipeoutFrame {
            origin: Vec3::ZERO,
            u: Vec3::new(4.0, 0.0, 0.0),
            v: Vec3::new(0.0, 2.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            clip_type: 2,
            clip: vec![Vec2::ZERO, Vec2::X, Vec2::new(1.0, 1.0), Vec2::Y, Vec2::ZERO],
        }),
    };
    let helix = Helix {
        center: Vec3::ZERO,
        base_radius: 1.0,
        top_radius: 1.0,
        height: 3.0,
        turns: 2.0,
        start_angle: 0.0,
        ccw: true,
        constrain: HelixConstrain::Height,
        frame: Some(HelixFrame { axis: Vec3::new(0.0, 1.0, 0.0), start: Vec3::new(1.0, 0.0, 0.0) }),
    };
    let image = Image {
        insert: Vec3::ZERO,
        u: Vec3::new(1.0, 0.0, 0.5),
        v: Vec3::new(0.0, 1.0, 0.25),
        size: Vec2::new(10.0, 10.0),
        path: "synthetic.png".into(),
        name: "Synthetic".into(),
        clip: Vec::new(),
        clipping: false,
        display: 7,
        brightness: 50,
        contrast: 50,
        fade: 0,
        pixel_size: Vec2::new(1.0, 1.0),
        resolution_units: 0,
    };
    let table = Table {
        insert: Vec3::ZERO,
        dir: Vec3::new(1.0, 0.0, 0.0),
        col_widths: vec![10.0],
        row_heights: vec![5.0],
        cells: vec![vec![TableCell { text: "SYNTHETIC".into(), rotation: 0.4, ..TableCell::default() }]],
        style: "Standard".into(),
        text_height: 1.0,
        title: false,
        header: false,
        row_styles: Vec::new(),
        breaks: None,
        data_link: None,
        overrides: Default::default(),
    };
    let dimension = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(10.0, 5.0, 0.0),
        text_mid: Vec3::new(5.0, 6.0, 0.0),
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 10.0,
        text_rotation: 0.0,
        user_text_pos: false,
        leader_length: 0.0,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: DimMarks::default(),
    };
    let mut dimension = variant("dimension_group_12_z", EntityKind::Dimension(dimension));
    let mut tags = cadcraft_dxf::parse(&dimension.source).unwrap();
    let start = tags.iter().position(|tag| tag.code == 0 && tag.str() == "DIMENSION").unwrap();
    let at =
        start + 1 + tags[start + 1..].iter().position(|tag| tag.code == 100 && tag.str().starts_with("AcDb") && tag.str() != "AcDbEntity").unwrap();
    // Group 12 is the clone displacement in the AcDbDimension subclass.
    let end = at + 1 + tags[at + 1..].iter().position(|tag| tag.code == 100 || tag.code == 0).unwrap();
    let mut body: Vec<_> = tags[at + 1..end].iter().filter(|tag| !matches!(tag.code, 12 | 22 | 32)).cloned().collect();
    body.extend([cadcraft_dxf::Tag::f(12, 0.0), cadcraft_dxf::Tag::f(22, 0.0), cadcraft_dxf::Tag::f(32, 2.0)]);
    tags.splice(at + 1..end, body);
    dimension.source = cadcraft_dxf::write_ascii(&tags).into_bytes();
    let mut result = vec![
        variant("hatch_spline_edge", EntityKind::Hatch(hatch)),
        variant("wipeout_nonunit_frame", EntityKind::Wipeout(wipeout)),
        variant("helix_non_z_axis", EntityKind::Helix(helix)),
        variant("image_uv_z", EntityKind::Image(image)),
        variant("table_rotated_cell", EntityKind::Table(table)),
        dimension,
    ];
    let mut drawing = Drawing::new_metric();
    let text = MText {
        insert: Vec3::new(8.0, 3.0, 0.0),
        height: 1.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "SYNTHETIC".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(1.0, 0.0, 0.0)),
        drawing_direction: None,
    };
    let annotation = add(&mut drawing, EntityKind::MText(text), GeometryRevision::CanonicalV2);
    let leader = Leader {
        vertices: vec![Vec3::ZERO, Vec3::new(4.0, 3.0, 0.0), Vec3::new(8.0, 3.0, 0.0)],
        frame: Some(LeaderFrame {
            direction: Vec3::new(1.0, 0.0, 0.0),
            block_offset: Vec3::ZERO,
            annotation_offset: Vec3::ZERO,
            text_height: 1.0,
            text_width: 0.0,
        }),
        links: LeaderLinks { creation: Some(0), hookline_direction: Some(1), hookline: Some(1), annotation: Some(annotation) },
        ..Leader::default()
    };
    let handle = add(&mut drawing, EntityKind::Leader(leader), GeometryRevision::CanonicalV2);
    result.push(Variant { name: "leader_annotation_link", handle, source: crate::write_dxf(&drawing).into_bytes() });
    let mut drawing = Drawing::new_metric();
    let mut mline = MLine::new(&MLineStyle::default(), &[Vec2::ZERO, Vec2::new(20.0, 0.0), Vec2::new(20.0, 10.0)], false, MLineJust::Zero, 1.0);
    for (index, vertex) in mline.vertices.iter_mut().enumerate() {
        vertex.vectors = Some(MLineVectors {
            p: vertex.p,
            dir: if index == 0 { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.0, 1.0, 0.0) },
            miter: if index == 0 { Vec3::new(0.6, 0.8, 0.0) } else { Vec3::new(1.0, 0.0, 0.0) },
        });
    }
    let handle = add(&mut drawing, EntityKind::MLine(mline), GeometryRevision::LegacyProjectedV1);
    result.push(Variant { name: "mline_source_miter", handle, source: crate::write_dxf(&drawing).into_bytes() });
    result
}

#[test]
fn structural_variants_stay_needs_under_the_replay_disabled_edit_oracle() {
    for variant in variants() {
        let parsed = cadcraft_dxf::parse(&variant.source).unwrap();
        let record = cadcraft_dxf::sections(&parsed)
            .into_iter()
            .filter(|section| section.name == "ENTITIES")
            .flat_map(|section| cadcraft_dxf::records(&section.tags))
            .find(|(_, groups)| groups.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()) == Some(variant.handle)))
            .unwrap();
        let has = |code, value: f64| record.1.iter().any(|tag| tag.code == code && (tag.f64() - value).abs() < 1e-14);
        let feature_exists = match variant.name {
            "hatch_spline_edge" => has(72, 4.0),
            "wipeout_nonunit_frame" => has(11, 4.0) && has(22, 2.0),
            "helix_non_z_axis" => has(22, 1.0) && has(32, 0.0),
            "image_uv_z" => has(31, 0.5) && has(32, 0.25),
            "table_rotated_cell" => record.1.iter().any(|tag| tag.code == 145 && tag.f64() != 0.0),
            "dimension_group_12_z" => has(32, 2.0) && record.1.iter().filter(|tag| tag.code == 12).count() == 1,
            "leader_annotation_link" => {
                record.1.iter().any(|tag| tag.code == 340 && Handle::parse_hex(&tag.str()).is_some_and(|handle| handle.0 != 0))
            }
            "mline_source_miter" => has(13, 0.6) && has(23, 0.8),
            _ => panic!("unknown structural variant"),
        };
        assert!(feature_exists, "{} source lacks its structural feature", variant.name);
        assert_eq!(classify_legacy_unit(&[record], cadcraft_dxf::Version::R2018).unwrap().disposition, LegacyDisposition::Needs, "{}", variant.name);
        for operation in oracle::Operation::ALL {
            let measured = oracle::measure(&variant.source, operation);
            assert!(measured.contains_key(&variant.handle), "{} {operation:?}", variant.name);
            eprintln!("LEGACY_VARIANT name={} operation={operation:?} legacy_lossless={}", variant.name, measured[&variant.handle]);
        }
    }
}
