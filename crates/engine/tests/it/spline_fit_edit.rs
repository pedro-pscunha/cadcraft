//! A spline-fit polyline (3D and 2D) is edited by its frame control points: its grips are the frame, a grip drag moves the frame
//! point and refits the curve (SPLINESEGS segments per span, quadratic or cubic by the curve type), and the polyline stays
//! spline-fit with its flags, curve type and frame count. REVERSE keeps the fit data and reverses frame and curve.

use cadcraft_doc::{Entity, EntityKind, Handle, LwPolyline, Polyline3d, Polyline3dFit, PolylineFit, PolylineFitVertex, PolylineRecord};
use cadcraft_engine::Session;
use cadcraft_engine::grips::stretch_grip_in;
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde_json::json;

const SEGS: usize = 8;

/// The B-spline basis function `i` of degree `d` at `u` (Cox-de Boor), on `knots`.
fn basis(i: usize, d: usize, u: f64, knots: &[f64]) -> f64 {
    if d == 0 {
        let last = knots.len() - 1;
        let in_span = knots[i] <= u && u < knots[i + 1];
        // The end of the domain belongs to the last non-empty span.
        let at_end = u == knots[last] && knots[i] < knots[i + 1] && knots[i + 1] == knots[last];
        return f64::from(in_span || at_end);
    }
    let left = if knots[i + d] > knots[i] { (u - knots[i]) / (knots[i + d] - knots[i]) * basis(i, d - 1, u, knots) } else { 0.0 };
    let right = if knots[i + d + 1] > knots[i + 1] {
        (knots[i + d + 1] - u) / (knots[i + d + 1] - knots[i + 1]) * basis(i + 1, d - 1, u, knots)
    } else {
        0.0
    };
    left + right
}

/// The open (clamped, uniform) B-spline of degree `d` through the frame `f`, SEGS segments per span: computed here, not by the
/// code under test.
fn curve(f: &[Vec3], d: usize) -> Vec<Vec3> {
    let n = f.len();
    let mut knots = vec![0.0; d + 1];
    knots.extend((1..n - d).map(|k| k as f64));
    knots.extend(vec![(n - d) as f64; d + 1]);
    let m = SEGS * (n - 1);
    (0..=m)
        .map(|k| {
            let u = (n - d) as f64 * k as f64 / m as f64;
            (0..n).fold(Vec3::ZERO, |acc, i| acc + f[i] * basis(i, d, u, &knots))
        })
        .collect()
}

fn degree(smooth: i64) -> usize {
    if smooth == 5 { 2 } else { 3 }
}

fn frame3() -> Vec<Vec3> {
    vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 1.0), Vec3::new(6.0, 4.0, 2.0), Vec3::new(9.0, 0.0, 3.0), Vec3::new(12.0, 3.0, 5.0)]
}

fn vertex(position: Vec3, flags: i64) -> PolylineFitVertex {
    PolylineFitVertex { record: PolylineRecord::default(), position, flags, start_width: 0.0, end_width: 0.0, bulge: 0.0, tangent: 0.0 }
}

fn polyline3d(smooth: i64) -> Polyline3d {
    let frame = frame3();
    let points = curve(&frame, degree(smooth));
    let vertices = frame.iter().map(|p| vertex(*p, 48)).chain(points.iter().map(|p| vertex(*p, 40))).collect();
    Polyline3d {
        records: Default::default(),
        points: points.clone(),
        closed: false,
        fit: Some(Polyline3dFit { flags: 12, smooth_type: smooth, vertices, display_points: points }),
    }
}

fn polyline2d(smooth: i64) -> LwPolyline {
    let frame: Vec<Vec3> = frame3().iter().map(|p| Vec3::new(p.x, p.y, 0.0)).collect();
    let points = curve(&frame, degree(smooth));
    let vertices: Vec<PolylineFitVertex> = frame.iter().map(|p| vertex(*p, 16)).chain(points.iter().map(|p| vertex(*p, 8))).collect();
    let shown: Vec<PolyVertex> = points.iter().map(|p| PolyVertex::new(p.xy())).collect();
    LwPolyline {
        records: Default::default(),
        vertices: shown.clone(),
        closed: false,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
        heavy: true,
        fit: Some(PolylineFit {
            flags: 4,
            smooth_type: smooth,
            m_count: 0,
            n_count: 0,
            m_density: 0,
            n_density: 0,
            start_width: 0.0,
            end_width: 0.0,
            elevation: 0.0,
            extrusion: Vec3::Z,
            vertices,
            display_vertices: shown,
        }),
    }
}

fn session_with(kind: EntityKind) -> (Session, Handle) {
    let mut session = Session::new();
    let handle = session.doc_mut().unwrap().new_handle();
    session.doc_mut().unwrap().model.push(Entity::new(handle, kind));
    (session, handle)
}

fn entity(s: &Session, h: Handle) -> Entity {
    s.doc().unwrap().entity(h).unwrap().as_ref().clone()
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

#[test]
fn the_grips_of_a_spline_fit_3d_polyline_are_its_frame_and_a_drag_refits_the_curve() {
    for smooth in [6, 5] {
        let (s, h) = session_with(EntityKind::Polyline3d(polyline3d(smooth)));
        let e = entity(&s, h);
        assert_eq!(e.kind.grips(), frame3().iter().map(|p| p.xy()).collect::<Vec<_>>(), "curve type {smooth}: the grips are the frame");
        let to = Vec2::new(6.5, 7.0);
        let moved = stretch_grip_in(s.doc().unwrap(), &e, 2, to).unwrap().expect("a candidate");
        let EntityKind::Polyline3d(p) = &moved.kind else { panic!("not a 3D polyline") };
        let fit = p.saved_fit().expect("still spline-fit");
        assert_eq!((fit.flags, fit.smooth_type), (12, smooth), "curve type {smooth}: header and curve type kept");
        assert_eq!(fit.vertices.iter().filter(|v| v.flags == 48).count(), 5, "curve type {smooth}: the frame count");
        let mut frame = frame3();
        frame[2] = Vec3::new(to.x, to.y, frame[2].z);
        assert!(fit.vertices.iter().filter(|v| v.flags == 48).zip(&frame).all(|(v, f)| near(v.position, *f)), "the dragged frame point");
        let want = curve(&frame, degree(smooth));
        assert_eq!(p.points.len(), want.len(), "curve type {smooth}");
        assert!(p.points.iter().zip(&want).all(|(a, b)| near(*a, *b)), "curve type {smooth}: the curve passes the independent refit");
        assert_eq!(fit.vertices.iter().filter(|v| v.flags == 40).count(), want.len(), "the generated vertices are the curve");
    }
}

#[test]
fn a_dragged_spline_fit_3d_polyline_survives_a_save_in_dxf_and_dwg() {
    let (mut s, h) = session_with(EntityKind::Polyline3d(polyline3d(6)));
    let moved = stretch_grip_in(s.doc().unwrap(), &entity(&s, h), 1, Vec2::new(3.0, 6.0)).unwrap().unwrap();
    cadcraft_engine::grips::commit_stretch(s.doc_mut().unwrap(), h, moved).unwrap();
    let want = entity(&s, h);
    let EntityKind::Polyline3d(want) = want.kind else { panic!() };
    for name in ["x.dxf", "x.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(s.doc().unwrap(), name).unwrap(), name).unwrap();
        let EntityKind::Polyline3d(p) = &back.model.iter().next().unwrap().kind else { panic!("not a 3D polyline") };
        let fit = p.fit.as_ref().expect("the fit data");
        assert_eq!((fit.flags & 4, fit.smooth_type), (4, 6), "{name}");
        assert_eq!(fit.vertices.iter().filter(|v| v.flags & 16 != 0).count(), 5, "{name}: the frame");
        assert!(p.points.iter().zip(&want.points).all(|(a, b)| near(*a, *b)) && p.points.len() == want.points.len(), "{name}: the curve");
    }
}

#[test]
fn the_grips_of_a_spline_fit_2d_polyline_are_its_frame_and_a_drag_refits_the_curve() {
    for smooth in [6, 5] {
        let (s, h) = session_with(EntityKind::LwPolyline(polyline2d(smooth)));
        let e = entity(&s, h);
        assert_eq!(e.kind.grips(), frame3().iter().map(|p| p.xy()).collect::<Vec<_>>(), "curve type {smooth}: the grips are the frame");
        let to = Vec2::new(6.5, 7.0);
        let moved = stretch_grip_in(s.doc().unwrap(), &e, 2, to).unwrap().expect("a candidate");
        let EntityKind::LwPolyline(p) = &moved.kind else { panic!("not a polyline") };
        let fit = p.saved_fit().expect("still spline-fit");
        assert_eq!((fit.flags, fit.smooth_type), (4, smooth));
        assert_eq!(fit.vertices.iter().filter(|v| v.flags & 16 != 0).count(), 5, "the frame count");
        let mut frame = frame3().iter().map(|p| Vec3::new(p.x, p.y, 0.0)).collect::<Vec<_>>();
        frame[2] = Vec3::new(to.x, to.y, 0.0);
        let want = curve(&frame, degree(smooth));
        assert_eq!(p.vertices.len(), want.len(), "curve type {smooth}");
        assert!(p.vertices.iter().zip(&want).all(|(a, b)| near(a.p.to3(0.0), *b)), "curve type {smooth}: the curve passes the independent refit");
        assert_eq!(p.records.vertices.len(), p.vertices.len());
    }
}

#[test]
fn a_spline_fit_polyline_made_here_or_curve_fit_keeps_its_vertex_grips() {
    let mut plain = polyline2d(6);
    plain.fit = None;
    let grips = EntityKind::LwPolyline(plain.clone()).grips();
    assert_eq!(grips.len(), plain.vertices.len() + plain.vertices.len() - 1, "vertices and segment midpoints");
    let mut curve_fit = polyline3d(6);
    curve_fit.fit.as_mut().unwrap().flags = 8 | 2;
    curve_fit.fit.as_mut().unwrap().smooth_type = 0;
    assert_eq!(EntityKind::Polyline3d(curve_fit.clone()).grips().len(), curve_fit.points.len(), "a curve-fit polyline is edited by its drawn points");
}

#[test]
fn reverse_keeps_the_fit_data_of_3d_and_2d_polylines_and_reverses_frame_and_curve() {
    let (mut s, h3) = session_with(EntityKind::Polyline3d(polyline3d(6)));
    let h2 = s.doc_mut().unwrap().new_handle();
    s.doc_mut().unwrap().model.push(Entity::new(h2, EntityKind::LwPolyline(polyline2d(6))));
    let before3 = polyline3d(6);
    let before2 = polyline2d(6);
    s.execute("reverse", &json!({"handles": [h3, h2]})).unwrap();
    let EntityKind::Polyline3d(p) = entity(&s, h3).kind else { panic!() };
    let fit = p.saved_fit().expect("3D: still spline-fit");
    let frame: Vec<Vec3> = fit.vertices.iter().filter(|v| v.flags == 48).map(|v| v.position).collect();
    assert_eq!(frame, frame3().into_iter().rev().collect::<Vec<_>>(), "3D: the frame is reversed");
    assert_eq!(p.points, before3.points.iter().rev().copied().collect::<Vec<_>>(), "3D: the curve is reversed");
    assert_eq!((fit.flags, fit.smooth_type), (12, 6));
    let EntityKind::LwPolyline(q) = entity(&s, h2).kind else { panic!() };
    let fit = q.saved_fit().expect("2D: still spline-fit");
    let frame: Vec<Vec2> = fit.vertices.iter().filter(|v| v.flags & 16 != 0).map(|v| v.position.xy()).collect();
    assert_eq!(frame, frame3().into_iter().rev().map(|p| p.xy()).collect::<Vec<_>>(), "2D: the frame is reversed");
    let shown: Vec<Vec2> = q.vertices.iter().map(|v| v.p).collect();
    assert_eq!(shown, before2.vertices.iter().rev().map(|v| v.p).collect::<Vec<_>>(), "2D: the curve is reversed");
}
