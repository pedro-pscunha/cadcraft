use cadcraft_geom::{Mat4, Vec3};

fn close(actual: Vec3, expected: Vec3) {
    assert!((actual - expected).len() < 1e-12, "{actual:?} != {expected:?}");
}

fn spatial() -> Mat4 {
    Mat4 { m: [[2.0, 1.0, 0.0, 7.0], [0.0, -3.0, 0.0, -11.0], [0.0, 0.0, 4.0, 13.0], [0.0, 0.0, 0.0, 1.0]] }
}

#[test]
fn affine_vectors_do_not_include_translation() {
    let transform = spatial();
    let value = Vec3::new(1.0, 2.0, 3.0);
    close(transform.apply_vec(value), Vec3::new(4.0, -6.0, 12.0));
    close(transform.apply(value), Vec3::new(11.0, -17.0, 25.0));
}

#[test]
fn an_affine_inverse_restores_points_and_vectors() {
    let transform = spatial();
    let inverse = transform.inverse().unwrap();
    let value = Vec3::new(1.0, 2.0, 3.0);
    close(inverse.apply(transform.apply(value)), value);
    close(transform.apply(inverse.apply(value)), value);
    close(inverse.apply_vec(transform.apply_vec(value)), value);
}

#[test]
fn inverse_rank_does_not_use_a_drawing_unit_tolerance() {
    let transform = Mat4 { m: [[1e-200, 0.0, 0.0, 0.0], [0.0, 1e200, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let inverse = transform.inverse().unwrap();
    close(inverse.apply(transform.apply(Vec3::new(1.0, 2.0, 3.0))), Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn inverse_refuses_singular_non_affine_and_non_finite_matrices() {
    let mut singular = spatial();
    singular.m[2] = [0.0; 4];
    let mut projective = spatial();
    projective.m[3][0] = 0.01;
    let mut non_finite = spatial();
    non_finite.m[0][3] = f64::INFINITY;
    let mut overflow = Mat4::IDENTITY;
    overflow.m[0][0] = f64::from_bits(1);
    for matrix in [singular, projective, non_finite, overflow] {
        assert!(matrix.inverse().is_none());
    }
}

#[test]
fn inverse_transpose_normals_remain_perpendicular_to_transformed_tangents() {
    let transform = spatial();
    let x = Vec3::new(1.0, 0.0, 0.0);
    let y = Vec3::new(0.0, 0.6, -0.8);
    let normal = transform.apply_normal(x.cross(y)).unwrap();
    assert!(normal.dot(transform.apply_vec(x)).abs() < 1e-12);
    assert!(normal.dot(transform.apply_vec(y)).abs() < 1e-12);
    assert!((normal.len() - 1.0).abs() < 1e-12);
    assert!(transform.apply_normal(Vec3::ZERO).is_none());
}

#[test]
fn unrelated_axis_scales_do_not_erase_a_normal_direction() {
    let transform = Mat4 { m: [[1e200, 0.0, 0.0, 0.0], [0.0, 1e-200, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    close(transform.apply_normal(Vec3::new(1.0, 0.0, 0.0)).unwrap(), Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn normal_transform_uses_inverse_transpose_not_reflected_basis_orientation() {
    let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    close(mirror.apply_normal(Vec3::Z).unwrap(), Vec3::Z);
}
