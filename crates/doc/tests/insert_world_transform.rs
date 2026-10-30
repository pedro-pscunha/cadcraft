use cadcraft_doc::Insert;
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
