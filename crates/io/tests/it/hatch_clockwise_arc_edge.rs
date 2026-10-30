//! A hatch whose boundary has a clockwise arc edge keeps its shape through the DWG bridge. AutoCAD
//! stores such an edge as 360 minus each real angle; the bridge used to convert it twice, and the
//! arc became a near-full circle (Eztec Firenze hatch 5190D drew a circle of radius 1991).
//! `clockwise_arc_edge.dwg` is acadrust's `tests/fixtures/hatch/clockwise-arc-edge.dwg`: AutoCAD
//! LT 2025 drew a 40 x 40 square with a round bite of radius 10 on its right side, then hatched it.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_clockwise_arc_edge_keeps_the_hatch_inside_its_boundary() {
    let d = cadcraft_io::read(include_bytes!("../data/clockwise_arc_edge.dwg"), "clockwise_arc_edge.dwg").unwrap();
    let hatch = d
        .model
        .iter()
        .find_map(|e| if let cadcraft_doc::EntityKind::Hatch(h) = &e.kind { Some(h) } else { None })
        .expect("the fixture has one hatch");
    let ext = cadcraft_render::space_extents(&d, &cadcraft_doc::Space::Model).unwrap();
    assert!(ext.max.x <= 20.0 + 1e-6 && ext.max.y <= 26.0 && ext.min.y >= -20.0 - 1e-6, "extents {ext:?}");
    let points: Vec<_> = hatch.loops.iter().flat_map(|l| l.vertices.iter().map(|v| v.p)).collect();
    assert!(points.iter().any(|p| p.near(cadcraft_geom::Vec2::new(20.0, -10.0), 1e-6)), "the bite starts at (20,-10): {points:?}");
    for w in hatch.loops[0].vertices.windows(2) {
        if w[0].p.near(cadcraft_geom::Vec2::new(20.0, -10.0), 1e-6) {
            assert!(w[1].p.near(cadcraft_geom::Vec2::new(20.0, 10.0), 1e-6), "the bite ends at (20,10)");
            assert!(w[0].bulge < -0.9 && w[0].bulge > -1.1, "a clockwise half circle into the square: bulge {}", w[0].bulge);
        }
    }
}
