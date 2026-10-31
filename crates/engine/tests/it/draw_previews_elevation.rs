//! A draw command previews at the current ELEVATION, where it will commit.
use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_geom::Vec2;
use serde_json::json;

fn z_of(kind: &EntityKind) -> Option<f64> {
    Some(match kind {
        EntityKind::Line(l) => l.a.z,
        EntityKind::Circle(c) => c.center.z,
        EntityKind::Arc(a) => a.center.z,
        EntityKind::Ellipse(e) => e.center.z,
        EntityKind::Ray(r) | EntityKind::XLine(r) => r.base.z,
        EntityKind::Text(t) => t.insert.z,
        EntityKind::MText(t) => t.insert.z,
        EntityKind::Insert(i) => i.insert.z,
        EntityKind::Helix(h) => h.center.z,
        EntityKind::Table(t) => t.insert.z,
        EntityKind::LwPolyline(p) => p.elevation,
        _ => return None,
    })
}

/// A command line that leaves the command waiting for its last point, the cursor that previews it, and the last input that commits
/// it (empty when the command also needs text or a dialog).
const COMMANDS: &[(&str, &str, (f64, f64), &[&str])] = &[
    ("CIRCLE", "CIRCLE 1,2", (3.0, 2.0), &["3,2"]),
    ("ARC", "ARC 0,0 2,2", (4.0, 0.0), &["4,0"]),
    ("ELLIPSE", "ELLIPSE 0,0 4,0", (2.0, 1.0), &["2,1"]),
    ("RAY", "RAY 0,0", (3.0, 1.0), &["3,1"]),
    ("XLINE", "XLINE 0,0", (3.0, 1.0), &["3,1"]),
    ("DONUT", "DONUT 1 2", (3.0, 1.0), &["3,1"]),
    ("POLYGON", "POLYGON 5 0,0 I", (3.0, 1.0), &["3,1"]),
    ("RECTANG", "RECTANG 0,0", (3.0, 2.0), &["3,2"]),
    ("MTEXT", "MTEXT 1,1", (3.0, 2.0), &[]),
    ("HELIX", "HELIX 0,0 1", (3.0, 1.0), &["1", "2"]),
    ("REVCLOUD", "REVCLOUD 0,0", (3.0, 3.0), &[]),
];

fn session() -> Session {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 3})).unwrap();
    s
}

#[test]
fn every_draw_preview_sits_at_the_current_elevation() {
    for (name, line, cursor, _) in COMMANDS {
        let mut s = session();
        s.cmdline(line).unwrap();
        let preview = s.preview(Vec2::new(cursor.0, cursor.1));
        assert!(!preview.is_empty(), "{name} has no preview");
        for entity in &preview {
            let z = z_of(&entity.kind).unwrap_or_else(|| panic!("{name}: no Z known for {:?}", entity.kind));
            assert!((z - 3.0).abs() < 1e-9, "{name}: the preview is at Z {z}, not 3: {:?}", entity.kind);
        }
    }
}

#[test]
fn a_draw_preview_and_the_object_it_commits_have_the_same_elevation() {
    for (name, line, cursor, last) in COMMANDS {
        if last.is_empty() {
            continue;
        }
        let mut s = session();
        s.cmdline(line).unwrap();
        let preview = s.preview(Vec2::new(cursor.0, cursor.1));
        let shown = preview.iter().filter_map(|e| z_of(&e.kind)).fold(f64::NAN, f64::max);
        let before = s.doc().unwrap().model.len();
        for input in *last {
            s.cmdline(input).unwrap();
        }
        let made: Vec<f64> = s.doc().unwrap().model.iter().skip(before).filter_map(|e| z_of(&e.kind)).collect();
        assert!(!made.is_empty(), "{name} committed nothing");
        assert!(made.iter().all(|z| (z - shown).abs() < 1e-9), "{name}: previewed at Z {shown}, committed at {made:?}");
    }
}
