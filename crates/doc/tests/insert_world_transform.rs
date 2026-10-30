use cadcraft_doc::{Attrib, GeometryRevision, Insert, Text};
use cadcraft_geom::{Mat4, Vec3};

fn insert() -> Insert {
    Insert {
        block: "B".into(),
        insert: Vec3::new(10.0, 20.0, 30.0),
        scale: Vec3::new(-2.0, 3.0, -4.0),
        rotation: std::f64::consts::FRAC_PI_2,
        attribs: Vec::new(),
        cols: 2,
        rows: 2,
        col_spacing: 5.0,
        row_spacing: 7.0,
    }
}

fn near(a: Vec3, b: Vec3) {
    assert!((a - b).len() < 1e-10, "{a:?} != {b:?}");
}

#[test]
fn insert_world_product_keeps_ocs_base_z_signed_scales_and_unscaled_spacing() {
    let source = insert();
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let base = Vec3::new(1.0, 2.0, 3.0);
    let point = Vec3::new(2.0, 4.0, 6.0);
    let first = source.world_transform(normal, base, 0, 0).unwrap();
    let next = source.world_transform(normal, base, 1, 1).unwrap();
    near(first.apply(point), Vec3::new(-4.0, 18.0, 18.0));
    near(next.apply(point), Vec3::new(3.0, 18.0, 23.0));
    near(next.apply(base), Vec3::new(-3.0, 30.0, 25.0));
    near(next.apply(point) - first.apply(point), Vec3::new(7.0, 0.0, 5.0));
    assert_eq!(source, insert());
}

#[test]
fn a_nested_affine_parent_acts_before_the_xy_projection() {
    let local = insert().world_transform(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 2.0, 3.0), 1, 1).unwrap();
    let parent = Mat4 { m: [[1.0, 0.0, 2.0, 5.0], [0.0, -1.0, 0.0, 7.0], [0.5, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    near(parent.mul(&local).apply(Vec3::new(2.0, 4.0, 6.0)), Vec3::new(54.0, -11.0, 24.5));
}

#[test]
fn collapsed_scales_remain_valid_display_matrices() {
    let mut source = insert();
    source.scale = Vec3::ZERO;
    let matrix = source.world_transform(Vec3::Z, Vec3::new(4.0, 5.0, 6.0), 0, 0).unwrap();
    near(matrix.apply(Vec3::new(100.0, 200.0, 300.0)), source.insert);
}

#[test]
fn a_persisted_insert_accepts_axis_aligned_nonuniform_scale() {
    let source = insert();
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let matrix = Mat4 { m: [[2.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 4.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let (moved, plane) = source.transformed_canonical(normal, &matrix).unwrap();
    near(moved.insert, Vec3::new(20.0, 80.0, 90.0));
    near(moved.scale, Vec3::new(-8.0, 6.0, -12.0));
    assert_eq!((moved.col_spacing, moved.row_spacing), (20.0, 14.0));
    near(moved.world_transform(plane, Vec3::new(1.0, 2.0, 3.0), 1, 1).unwrap().apply(Vec3::new(2.0, 4.0, 6.0)), Vec3::new(6.0, 54.0, 92.0));
}

#[test]
fn translation_keeps_insert_plane_scale_rotation_and_spacing_bits() {
    let source = insert();
    let normal = Vec3::new(-0.0, 2.0, 0.0);
    let matrix = Mat4 { m: [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 2.0], [0.0, 0.0, 1.0, 3.0], [0.0, 0.0, 0.0, 1.0]] };
    let (moved, plane) = source.transformed_canonical(normal, &matrix).unwrap();
    near(moved.insert, Vec3::new(9.0, 23.0, 32.0));
    assert_eq!([plane.x.to_bits(), plane.y.to_bits(), plane.z.to_bits()], [normal.x.to_bits(), normal.y.to_bits(), normal.z.to_bits()]);
    assert_eq!(moved.scale, source.scale);
    assert_eq!(moved.rotation.to_bits(), source.rotation.to_bits());
    assert_eq!(moved.col_spacing.to_bits(), source.col_spacing.to_bits());
    assert_eq!(moved.row_spacing.to_bits(), source.row_spacing.to_bits());
}

#[test]
fn a_reflection_is_representable_but_a_sheared_insert_frame_refuses() {
    let source = insert();
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let (moved, plane) = source.transformed_canonical(normal, &mirror).unwrap();
    near(moved.world_transform(plane, Vec3::new(1.0, 2.0, 3.0), 1, 1).unwrap().apply(Vec3::new(2.0, 4.0, 6.0)), Vec3::new(-3.0, 18.0, 23.0));
    let shear = Mat4 { m: [[1.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    assert!(source.transformed_canonical(normal, &shear).is_err());
    assert_eq!(source, insert());
}

#[test]
fn persisted_attributes_keep_their_own_planes_and_refuse_as_one_candidate() {
    let text = Text {
        insert: Vec3::new(1.0, 2.0, 3.0),
        align_pt: None,
        height: 1.0,
        value: "A".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
        backward: false,
        upside_down: false,
    };
    let mut source = insert();
    source.attribs.push(Attrib::new("A", text));
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let translation = Mat4 { m: [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 2.0], [0.0, 0.0, 1.0, 3.0], [0.0, 0.0, 0.0, 1.0]] };
    let before = source.clone();
    assert!(source.transformed_canonical(normal, &translation).is_err(), "legacy attributes must not enter the canonical edit");
    assert_eq!(source, before);
    source.attribs.first_mut().unwrap().geometry_revision = GeometryRevision::CanonicalV2;
    let (moved, _) = source.transformed_canonical(normal, &translation).unwrap();
    near(moved.attribs.first().unwrap().text.insert, Vec3::new(2.0, 4.0, 6.0));
    let scale = Mat4 { m: [[2.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 4.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let before = source.clone();
    assert!(source.transformed_canonical(normal, &scale).is_err(), "the attribute must independently support the edit");
    assert_eq!(source, before);
}

#[test]
fn invalid_frames_indices_and_nonfinite_products_refuse() {
    let source = insert();
    assert!(source.world_transform(Vec3::ZERO, Vec3::ZERO, 0, 0).is_err());
    assert!(source.world_transform(Vec3::Z, Vec3::ZERO, 2, 0).is_err());
    assert!(source.world_transform(Vec3::Z, Vec3::ZERO, 0, 2).is_err());
    assert!(source.world_transform(Vec3::Z, Vec3::new(f64::NAN, 0.0, 0.0), 0, 0).is_err());
    let mut overflow = source;
    overflow.scale.x = f64::MAX;
    assert!(overflow.world_transform(Vec3::Z, Vec3::new(2.0, 0.0, 0.0), 0, 0).is_err());
}
