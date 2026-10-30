//! PROPERTIES reports a canonical entity by its world geometry: a spatial LINE has its 3D length, and a circle in a tilted plane names its
//! frame and its WCS center. A legacy entity reports as before.

mod consumer_support;

use cadcraft_doc::GeometryRevision;
use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::{Value, json};

fn properties(session: &mut Session, handle: cadcraft_doc::Handle) -> Value {
    session.execute("properties", &json!({"handles": [handle.hex()]})).unwrap()["objects"][0].clone()
}

#[test]
fn a_canonical_spatial_line_reports_its_three_dimensional_length() {
    let mut session = Session::new();
    let line = spatial_line(&mut session, [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
    let legacy = properties(&mut session, line);
    assert!((legacy["length"].as_f64().unwrap() - 3.0).abs() < 1e-12, "{legacy}");
    assert!(legacy.get("geometryRevision").is_none());
    make_canonical(&mut session, line);
    let canonical = properties(&mut session, line);
    assert!((canonical["length"].as_f64().unwrap() - 5.0).abs() < 1e-12, "{canonical}");
    assert!((canonical["deltaZ"].as_f64().unwrap() - 4.0).abs() < 1e-12, "{canonical}");
    assert_eq!(canonical["geometryRevision"], "canonical");
    assert_eq!(canonical["geometryFrame"], "wcs");
}

#[test]
fn a_canonical_circle_in_a_tilted_plane_names_its_frame_and_world_center() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    make_canonical(&mut session, circle);
    assert_eq!(session.doc().unwrap().entity(circle).unwrap().geometry_revision, GeometryRevision::CanonicalV2);
    let p = properties(&mut session, circle);
    assert_eq!(p["geometryFrame"], "ocs");
    assert_eq!(p["extrusion"], json!([0.0, 0.6, 0.8]));
    // The OCS center (10, 5, 0) in the plane of the normal (0, 0.6, 0.8): the OCS x axis is world X, y is the normal crossed with x.
    let center = p["worldCenter"].as_array().unwrap();
    let (nx, ny, nz) = (0.0_f64, 0.6_f64, 0.8_f64);
    // Arbitrary-axis algorithm: |nx|,|ny| < 1/64 is false here, so the x axis is Z cross N, normalised.
    let (ax, ay) = (-ny, nx);
    let norm = (ax * ax + ay * ay).sqrt();
    let (xx, xy) = (ax / norm, ay / norm);
    let (yx, yy, yz) = (ny * 0.0 - nz * xy, nz * xx - nx * 0.0, nx * xy - ny * xx);
    let expected = [10.0 * xx + 5.0 * yx, 10.0 * xy + 5.0 * yy, 5.0 * yz];
    for (got, want) in center.iter().zip(expected) {
        assert!((got.as_f64().unwrap() - want).abs() < 1e-9, "{center:?} vs {expected:?}");
    }
    assert!((p["circumference"].as_f64().unwrap() - 4.0 * std::f64::consts::PI).abs() < 1e-12);
}

fn row(rows: &[(&'static str, String)], label: &str) -> f64 {
    rows.iter().find(|(l, _)| *l == label).unwrap_or_else(|| panic!("no row {label}: {rows:?}")).1.parse().unwrap()
}

#[test]
fn the_palette_rows_of_a_canonical_line_are_world_values_and_a_legacy_line_has_none() {
    let mut session = Session::new();
    let line = spatial_line(&mut session, [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
    assert!(cadcraft_engine::canonical_geometry_rows(session.doc().unwrap(), session.doc().unwrap().entity(line).unwrap()).is_none());
    make_canonical(&mut session, line);
    let rows = cadcraft_engine::canonical_geometry_rows(session.doc().unwrap(), session.doc().unwrap().entity(line).unwrap()).unwrap();
    assert_eq!(row(&rows, "Length"), 5.0);
    assert_eq!(row(&rows, "Delta Z"), 4.0);
    assert_eq!(row(&rows, "End Z (WCS)"), 4.0);
}

#[test]
fn the_palette_rows_of_a_tilted_circle_give_the_world_center_and_the_plane_normal() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    make_canonical(&mut session, circle);
    let rows = cadcraft_engine::canonical_geometry_rows(session.doc().unwrap(), session.doc().unwrap().entity(circle).unwrap()).unwrap();
    // The OCS of the normal (0, 0.6, 0.8): x axis (-1, 0, 0), y axis (0, -0.8, 0.6), so the center (10, 5, 0) is (-10, -4, 3) in the world.
    assert!((row(&rows, "Center X (WCS)") + 10.0).abs() < 1e-4);
    assert!((row(&rows, "Center Y (WCS)") + 4.0).abs() < 1e-4);
    assert!((row(&rows, "Center Z (WCS)") - 3.0).abs() < 1e-4);
    assert!((row(&rows, "Normal Y") - 0.6).abs() < 1e-4 && (row(&rows, "Normal Z") - 0.8).abs() < 1e-4);
    assert!((row(&rows, "Area") - 4.0 * std::f64::consts::PI).abs() < 1e-3);
}
