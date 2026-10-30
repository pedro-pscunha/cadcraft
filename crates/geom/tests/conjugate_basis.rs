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
fn a_zero_projection_stays_a_finite_point() {
    let ellipse = Ellipse::from_conjugate(Vec2::new(2.0, 3.0), Vec2::ZERO, Vec2::ZERO, 0.2, 1.4);
    assert_eq!(ellipse.at_param(ellipse.start), Vec2::new(2.0, 3.0));
    assert_eq!(ellipse.at_param(ellipse.end), Vec2::new(2.0, 3.0));
    assert_eq!(ellipse.ratio, 0.0);
}
