//! A hatch boundary of lines, arcs, ellipse arcs and splines keeps those edges through a whole-object edit and a save: MOVE,
//! ROTATE, SCALE and MIRROR move each edge as the exact edge it is, and the DXF and the DWG hold the edges, not the polyline
//! that approximates them. A non-uniform scale keeps the lines and splines it can and writes the approximation otherwise.

use cadcraft_doc::{Drawing, Entity, EntityKind, HatchEdge, HatchLoop};
use cadcraft_geom::{Mat3, Mat4, Vec2};

/// One closed loop: a line, an arc, a cubic spline and an ellipse arc.
const SOURCE: &str = "0\nSECTION\n2\nENTITIES\n0\nHATCH\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n\
2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n1\n93\n4\n\
72\n1\n10\n0\n20\n0\n11\n4\n21\n0\n\
72\n2\n10\n4\n20\n2\n40\n2\n50\n270\n51\n90\n73\n1\n\
72\n4\n94\n3\n73\n0\n74\n0\n95\n8\n96\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n10\n4\n20\n4\n10\n3\n20\n6\n10\n1\n20\n6\n10\n0\n20\n4\n\
72\n3\n10\n0\n20\n2\n11\n0\n21\n2\n40\n0.5\n50\n0\n51\n180\n73\n1\n\
97\n0\n75\n1\n76\n1\n98\n0\n0\nENDSEC\n0\nEOF\n";

fn drawing() -> Drawing {
    cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap()
}

fn entity(d: &Drawing) -> Entity {
    d.model.iter().next().expect("a hatch").as_ref().clone()
}

fn loop_of(e: &Entity) -> HatchLoop {
    let EntityKind::Hatch(h) = &e.kind else { panic!("not a hatch") };
    h.loops[0].clone()
}

/// Start, middle and end of an arc or ellipse edge, in the order the edge runs (3 points), or the ends of a line, or the control
/// points of a spline: the points that fix the edge.
fn key_points(e: &HatchEdge) -> Vec<Vec2> {
    let sweep = |start: f64, end: f64, ccw: bool| {
        let tau = std::f64::consts::TAU;
        let up = (end - start).rem_euclid(tau);
        if ccw { up } else { -((start - end).rem_euclid(tau)) }
    };
    match e {
        HatchEdge::Line { a, b } => vec![*a, *b],
        HatchEdge::Arc { center, radius, start, end, ccw } => {
            let s = sweep(*start, *end, *ccw);
            [0.0, 0.5, 1.0].iter().map(|f| *center + Vec2::new((start + s * f).cos(), (start + s * f).sin()) * *radius).collect()
        }
        HatchEdge::Ellipse { center, major, ratio, start, end, ccw } => {
            let s = sweep(*start, *end, *ccw);
            let minor = major.perp() * *ratio;
            [0.0, 0.5, 1.0].iter().map(|f| *center + *major * (start + s * f).cos() + minor * (start + s * f).sin()).collect()
        }
        HatchEdge::Spline(sp) => sp.control.clone(),
        HatchEdge::Polyline { vertices, .. } => vertices.iter().map(|v| v.p).collect(),
    }
}

fn kind_of(e: &HatchEdge) -> &'static str {
    match e {
        HatchEdge::Line { .. } => "line",
        HatchEdge::Arc { .. } => "arc",
        HatchEdge::Ellipse { .. } => "ellipse",
        HatchEdge::Spline(_) => "spline",
        HatchEdge::Polyline { .. } => "polyline",
    }
}

fn near(a: &[Vec2], b: &[Vec2]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.dist(*q) < 1e-6)
}

fn maps() -> Vec<(&'static str, Mat3)> {
    vec![
        ("move", Mat3::translate(Vec2::new(10.0, 5.0))),
        ("rotate", Mat3::rotate_about(Vec2::new(1.0, 1.0), 0.6)),
        ("scale", Mat3::scale_about(Vec2::new(1.0, 1.0), 2.5)),
        ("mirror", Mat3::mirror(Vec2::new(0.0, 1.0), Vec2::new(3.0, 2.0))),
    ]
}

#[test]
fn the_source_boundary_is_read_as_its_four_exact_edges() {
    let l = loop_of(&entity(&drawing()));
    assert_eq!(l.edges.iter().map(kind_of).collect::<Vec<_>>(), ["line", "arc", "spline", "ellipse"]);
    assert!(l.exact_edges().is_some() && !l.vertices.is_empty());
    assert_eq!(l.flags, 1);
}

#[test]
fn each_edge_follows_a_move_a_rotation_a_scale_and_a_mirror_through_the_dxf_and_the_dwg() {
    let source = entity(&drawing());
    let original: Vec<Vec<Vec2>> = loop_of(&source).edges.iter().map(key_points).collect();
    for (name, m) in maps() {
        let moved = source.transformed(&Mat4::from_mat3(&m)).expect("the edit");
        // The expected key points are the original ones mapped (a reflection keeps the order of a point list).
        let want: Vec<Vec<Vec2>> = original.iter().map(|ps| ps.iter().map(|p| m.apply(*p)).collect()).collect();
        let in_model = loop_of(&moved);
        assert_eq!(in_model.edges.iter().map(kind_of).collect::<Vec<_>>(), ["line", "arc", "spline", "ellipse"], "{name}: the model");
        assert!(in_model.edges.iter().map(key_points).zip(&want).all(|(a, b)| near(&a, b)), "{name}: the model edges");
        let mut out = Drawing::new_metric();
        out.model.push(moved);
        for file in ["x.dxf", "x.dwg"] {
            let back = cadcraft_io::read(&cadcraft_io::write(&out, file).unwrap(), file).unwrap();
            let l = loop_of(&entity(&back));
            assert_eq!(l.edges.iter().map(kind_of).collect::<Vec<_>>(), ["line", "arc", "spline", "ellipse"], "{name}, {file}");
            for (i, (edge, want)) in l.edges.iter().zip(&want).enumerate() {
                assert!(near(&key_points(edge), want), "{name}, {file}, edge {i} ({}): {:?} != {want:?}", kind_of(edge), key_points(edge));
            }
        }
    }
}

#[test]
fn a_reshaped_boundary_is_written_from_its_vertices() {
    let source = entity(&drawing());
    let mut edited = source.clone();
    if let EntityKind::Hatch(h) = &mut edited.kind {
        h.loops[0].vertices[0].p = Vec2::new(-1.0, -1.0);
    }
    assert!(loop_of(&edited).exact_edges().is_none(), "the edges no longer describe the loop");
    let mut out = Drawing::new_metric();
    out.model.push(edited);
    let back = cadcraft_io::read_dxf(&cadcraft_io::write(&out, "x.dxf").unwrap()).unwrap();
    let l = loop_of(&entity(&back));
    assert!(l.edges.is_empty(), "written as a polyline loop");
    assert_eq!(l.vertices[0].p, Vec2::new(-1.0, -1.0));
}

#[test]
fn a_non_uniform_scale_keeps_what_it_can_and_writes_the_approximation_otherwise() {
    let source = entity(&drawing());
    let m = Mat3::scale(2.0, 1.0);
    let moved = source.transformed(&Mat4::from_mat3(&m)).expect("the edit");
    let l = loop_of(&moved);
    // Arcs and ellipses are no arcs or ellipses of the same kind under it: the loop falls back to its vertices.
    assert!(l.edges.is_empty() && !l.vertices.is_empty());
}
