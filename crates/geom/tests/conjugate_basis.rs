use cadcraft_geom::{Ellipse, Vec2};

#[test]
fn conjugate_axis_decomposition_is_scale_independent() {
    for scale in [1.0, 1e-200, 1e200] {
        let am = Vec2::new(scale, 0.0);
        let an = Vec2::new(0.0, 2.0 * scale);
        let ellipse = Ellipse::from_conjugate(Vec2::ZERO, am, an, 0.2, 1.4);
        assert!(ellipse.major.is_finite());
        assert!((ellipse.major.len() / scale - 2.0).abs() < 1e-12, "{scale}: {ellipse:?}");
        assert!((ellipse.ratio - 0.5).abs() < 1e-12, "{scale}: {ellipse:?}");
        for (parameter, actual) in [(0.2_f64, ellipse.at_param(ellipse.start)), (1.4_f64, ellipse.at_param(ellipse.end))] {
            let expected = am * parameter.cos() + an * parameter.sin();
            assert!((actual * (1.0 / scale) - expected * (1.0 / scale)).len() < 1e-12);
        }
    }
}

#[test]
fn a_flat_basis_keeps_its_true_ratio_in_either_axis_order() {
    for ratio in [0.0, 1e-20, 1e-200] {
        for scale in [1.0, 1e100, 1e-100] {
            let (a, b) = (Vec2::new(scale * ratio, 0.0), Vec2::new(0.0, scale));
            for (a, b) in [(a, b), (b, a)] {
                let ellipse = Ellipse::from_conjugate(Vec2::ZERO, a, b, 0.0, cadcraft_geom::TAU);
                if ratio == 0.0 {
                    assert_eq!(ellipse.ratio, 0.0, "a rank-one basis must not gain a second axis");
                } else {
                    assert!((ellipse.ratio / ratio - 1.0).abs() < 1e-12, "{ratio} at {scale}: {ellipse:?}");
                }
            }
        }
    }
}

#[test]
fn a_small_nonzero_parameter_span_is_not_a_full_conic() {
    let ellipse = Ellipse::from_conjugate(Vec2::ZERO, Vec2::new(1e12, 0.0), Vec2::new(0.0, 5e11), 0.0, 1e-12);
    assert!(!ellipse.is_full());
    assert!((ellipse.sweep() - 1e-12).abs() < 1e-24);
    let mut points = Vec::new();
    ellipse.tessellate(0.01, &mut points);
    assert!((points.last().unwrap().y - 0.5).abs() < 1e-12);
    let arc = cadcraft_geom::Arc::new(Vec2::ZERO, 1e12, 0.0, 1e-12);
    assert!((arc.sweep() - 1e-12).abs() < 1e-24);
}

#[test]
fn a_small_missing_span_is_not_a_closed_conic() {
    let ellipse = Ellipse::from_conjugate(Vec2::ZERO, Vec2::new(1e12, 0.0), Vec2::new(0.0, 5e11), 0.0, cadcraft_geom::TAU - 1e-12);
    assert!(!ellipse.is_full());
    assert!(ellipse.sweep() < cadcraft_geom::TAU);
}

#[test]
fn a_zero_projection_stays_a_finite_point() {
    let ellipse = Ellipse::from_conjugate(Vec2::new(2.0, 3.0), Vec2::ZERO, Vec2::ZERO, 0.2, 1.4);
    assert_eq!(ellipse.at_param(ellipse.start), Vec2::new(2.0, 3.0));
    assert_eq!(ellipse.at_param(ellipse.end), Vec2::new(2.0, 3.0));
    assert_eq!(ellipse.ratio, 0.0);
}
