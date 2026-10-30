//! A persistent reactor of a drawing entity stays on the entity it was on: AutoCAD keeps the hatch
//! that lists a boundary polyline as its reactor even when the hatch is not associative.

use cadcraft_doc::{Common, Drawing, EntityKind, Handle, Hatch, HatchLoop, LwPolyline, Space};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{PolyVertex, Vec2};

fn square() -> Vec<PolyVertex> {
    [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect()
}

fn hatch(loops: Vec<HatchLoop>) -> Hatch {
    Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops,
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
    }
}

fn reactors(text: &str, handle: Handle) -> Vec<String> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let body: Vec<Tag> = sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| records(&s.tags))
        .find(|(_, body)| body.iter().any(|t| t.code == 5 && t.str() == handle.hex()))
        .expect("entity record")
        .1;
    let mut inside = false;
    let mut out = Vec::new();
    for t in body {
        if t.code == 102 {
            inside = t.str().starts_with("{ACAD_REACTORS");
        } else if inside && t.code == 330 {
            out.push(t.str());
        }
    }
    out
}

#[test]
fn the_hatch_a_boundary_lists_stays_its_reactor_while_the_hatch_is_not_associative() {
    let mut d = Drawing::new_metric();
    let k = EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: square(),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
        heavy: false,
        fit: None,
    });
    let poly = d.add(&Space::Model, Common::default(), k).unwrap();
    let loops = vec![HatchLoop { vertices: square(), outer: true, ..Default::default() }];
    let hatch = d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops))).unwrap();
    // The file lists the hatch on the boundary, as AutoCAD does after the association is gone.
    let written = cadcraft_io::write_dxf(&d);
    let mut lines: Vec<String> = written.lines().map(str::to_string).collect();
    let at = lines.iter().position(|l| l.trim() == "LWPOLYLINE").expect("polyline record") + 3;
    assert_eq!(lines[at - 1].trim(), poly.hex());
    lines.splice(at..at, ["102", "{ACAD_REACTORS", "330", &hatch.hex(), "102", "}"].map(String::from));
    let text = lines.join("\n") + "\n";
    assert_eq!(reactors(&text, poly), [hatch.hex()]);
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_eq!(reactors(&cadcraft_io::write_dxf(&back), poly), [hatch.hex()]);
    // The hatch deleted, the reactor goes with it.
    let mut gone = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    gone.remove_entity(hatch);
    assert!(reactors(&cadcraft_io::write_dxf(&gone), poly).is_empty());
}
