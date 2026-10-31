//! The stage-0 consumer matrix (reader activation plan v3, section 6.3): one table of rows, each a tuple of command, input role,
//! revision, plane and parameter branch with an expected outcome, run through `Session::execute`.
//!
//! - A live row states `Ok(numbers)` or `Refuses(reason)` and is checked: an `Ok` row must reach the operand and give the independent
//!   number, a refusal must name the reason and leave the model and the handle counter as they were. A missing-argument error
//!   does not count: every row's control with the same parameters on a legacy drawing must succeed.
//! - A pending row belongs to an open finding of the plan (none is open: F3 closed with 5774fbf8). Its expectation is what the fix must deliver, it must still
//!   fail today, and the test that checks it fails the day it passes, so the row cannot outlive its fix silently.
//! - The registry lint: every registered command is declared here with the roles its rows cover, or is in
//!   `consumer_matrix/undeclared.txt`. A new command gets rows or an explicit entry; a declared command leaves the list.
//!
//! The layers a `run()` sweep cannot reach (interactive previews, UI adapters, paper-viewport display) stay as their own tests; the
//! DIMTEDIT preview of F3 is one of them (`it/canonical_previews.rs`).

mod consumer_support;

use std::collections::BTreeSet;

use cadcraft_doc::{Common, Drawing, EntityKind, Handle, Space};
use cadcraft_engine::{Session, command_specs};
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Primary,
    Secondary,
    Boundary,
    /// The rows for it are the block-content layer (snap, solver and preview over a legacy INSERT with a canonical child), which has its own
    /// tests (`tests/it/mixed_block_snap.rs`); a `run()` row for it arrives with the stage-6 INSERT reader.
    #[allow(dead_code)]
    BlockContent,
    SelectionMember,
    PickedPoint,
}

/// Which of the operands are canonical. With one operand only `Legacy` and `Canonical` apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Revision {
    Legacy,
    Canonical,
    /// The first operand legacy, the others canonical.
    LegacyFirst,
    /// The first operand canonical, the others legacy.
    CanonicalFirst,
    /// Every operand is a raw unit: kept as read, read-only.
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plane {
    PlusZ,
    /// The normal (0, 3, 4) / 5.
    Tilted,
    NegativeZ,
    /// The normal (0, 1, 0): the plane of the major axis (3, 0, 4).
    NormalY,
    /// The normal (0, 0, 1), with the operand lifted to Z 5: the elevation is real, not a zero.
    PlusZElevated,
}

impl Plane {
    fn normal(self) -> Vec3 {
        match self {
            Plane::PlusZ => Vec3::Z,
            Plane::Tilted => Vec3::new(0.0, 0.6, 0.8),
            Plane::NegativeZ => Vec3::new(0.0, 0.0, -1.0),
            Plane::NormalY => Vec3::new(0.0, 1.0, 0.0),
            Plane::PlusZElevated => Vec3::Z,
        }
    }

    fn elevation(self) -> f64 {
        if self == Plane::PlusZElevated { ELEVATION } else { 0.0 }
    }
}

const ELEVATION: f64 = 5.0;

type Check = fn(&Value) -> Result<(), String>;

enum Expect {
    /// The command succeeds and the check on its result holds.
    Ok(Check),
    /// The command succeeds and the drawing, read through the operand handles, holds the independent numbers.
    Edits(fn(&Drawing, &[Handle], Plane) -> Result<(), String>),
    /// The command is refused with an error that contains this text, and writes nothing.
    Refuses(&'static str),
}

/// A drawing, the command parameters and the entities the revision applies to, in role order.
struct Fixture {
    drawing: Drawing,
    params: Value,
    operands: Vec<Handle>,
    /// A `zoom` window that sets the pick radius: it is a world length that follows the view, so a spline's candidate spans depend on it.
    view: Option<Value>,
}

struct Row {
    command: &'static str,
    role: Role,
    branch: &'static str,
    revision: Revision,
    plane: Plane,
    fixture: fn() -> Fixture,
    expect: Expect,
}

impl Row {
    fn name(&self) -> String {
        format!("{} [{:?}, {}, {:?}, {:?}]", self.command, self.role, self.branch, self.revision, self.plane)
    }
}

/// The fixture's drawing opened in a session, with its operands in the row's revision and plane. `Err` names a malformed fixture.
fn prepare(fixture: &Fixture, revision: Revision, plane: Plane) -> Result<Session, String> {
    let mut session = session_of(&fixture.drawing);
    if let Some(view) = &fixture.view {
        session.execute("zoom", view).map_err(|error| format!("the view fails: {error}"))?;
    }
    for (index, handle) in fixture.operands.iter().enumerate() {
        let canonical = match revision {
            Revision::Legacy | Revision::Raw => false,
            Revision::Canonical => true,
            Revision::LegacyFirst => index > 0,
            Revision::CanonicalFirst => index == 0,
        };
        if canonical {
            make_canonical_in(&mut session, *handle, plane.normal());
            lift(&mut session, *handle, plane.elevation());
        }
        if revision == Revision::Raw {
            make_raw(&mut session, *handle);
        }
    }
    Ok(session)
}

/// Run a row. `Err` names the way it failed.
fn run_row(row: &Row) -> Result<(), String> {
    let fixture = (row.fixture)();
    // The control: the same command and parameters on the legacy drawing must succeed, so a row never passes on a malformed call.
    let mut control = prepare(&fixture, Revision::Legacy, Plane::PlusZ)?;
    control.execute(row.command, &fixture.params).map_err(|error| format!("the legacy control fails: {error}"))?;
    let mut session = prepare(&fixture, row.revision, row.plane)?;
    let before = session.doc().map_err(|e| e.to_string())?.model.clone();
    let seed = session.doc().map_err(|e| e.to_string())?.handseed;
    let result = session.execute(row.command, &fixture.params);
    match (&row.expect, result) {
        (Expect::Ok(check), Ok(value)) => check(&value),
        (Expect::Edits(check), Ok(_)) => check(session.doc().map_err(|e| e.to_string())?, &fixture.operands, row.plane),
        (Expect::Ok(_) | Expect::Edits(_), Err(error)) => Err(format!("refused: {error}")),
        (Expect::Refuses(_), Ok(value)) => Err(format!("accepted: {value}")),
        (Expect::Refuses(reason), Err(error)) => {
            if !error.to_string().contains(reason) {
                return Err(format!("the refusal says \"{error}\", not \"{reason}\""));
            }
            let doc = session.doc().map_err(|e| e.to_string())?;
            if doc.model != before || doc.handseed != seed {
                return Err("the refusal wrote to the drawing".into());
            }
            Ok(())
        }
    }
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value[key].as_f64().ok_or_else(|| format!("{key} missing in {value}"))
}

fn z_is_0(v: &Value) -> Result<(), String> {
    near(number(v, "z")?, 0.0)
}

fn z_is_3(v: &Value) -> Result<(), String> {
    near(number(v, "z")?, 3.0)
}

fn z_is_5(v: &Value) -> Result<(), String> {
    near(number(v, "z")?, 5.0)
}

fn near(value: f64, want: f64) -> Result<(), String> {
    if (value - want).abs() < 1e-6 { Ok(()) } else { Err(format!("{value} instead of {want}")) }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Fixtures: the independent numbers come from the plane geometry written beside each one, not from the code under test.

fn model_of(session: &Session) -> Drawing {
    session.doc().unwrap().clone()
}

fn circle() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [0, 0], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"handle": circle.hex()}), operands: vec![circle], view: None }
}

fn two_lines() -> Fixture {
    let mut s = Session::new();
    let a = spatial_line(&mut s, [10.0, 0.0, 0.0], [13.0, 0.0, 4.0]);
    let b = create(&mut s, "line", json!({"points": [[20, 4], [10, 4]]}));
    Fixture { drawing: model_of(&s), params: json!({"h1": a.hex(), "h2": b.hex()}), operands: vec![a, b], view: None }
}

fn closed_polyline() -> Fixture {
    let mut s = Session::new();
    let vertices =
        json!([{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [10.0, 0.0], "bulge": 0.0}, {"p": [10.0, 5.0], "bulge": 0.0}, {"p": [0.0, 5.0], "bulge": 0.0}]);
    let polyline = create(&mut s, "pline", json!({"vertices": vertices, "closed": true}));
    Fixture { drawing: model_of(&s), params: json!({"handle": polyline.hex(), "erase": false}), operands: vec![polyline], view: None }
}

fn wipeout_erasing() -> Fixture {
    let mut f = closed_polyline();
    f.params["erase"] = json!(true);
    f
}

fn revcloud() -> Fixture {
    let mut f = circle();
    f.params["arcLength"] = json!(0.5);
    f
}

fn arc_continue() -> Fixture {
    let mut s = Session::new();
    let line = create(&mut s, "line", json!({"points": [[0, 0], [1, 0]]}));
    Fixture { drawing: model_of(&s), params: json!({"end": [2, 1]}), operands: vec![line], view: None }
}

fn xline_offset() -> Fixture {
    let mut s = Session::new();
    let line = create(&mut s, "line", json!({"points": [[0, 0], [10, 0]]}));
    Fixture { drawing: model_of(&s), params: json!({"handle": line.hex(), "distance": 2, "side": [5, -1]}), operands: vec![line], view: None }
}

fn xline_through() -> Fixture {
    let mut f = xline_offset();
    f.params = json!({"handle": f.operands[0].hex(), "through": [3, 7]});
    f
}

fn dimjogged() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [0, 0], "radius": 50}));
    let params = json!({"handle": circle.hex(), "centerOverride": [20, -10], "at": [40, 40], "jog": [30, 15]});
    Fixture { drawing: model_of(&s), params, operands: vec![circle], view: None }
}

fn dimbreak_automatic() -> Fixture {
    let mut s = Session::new();
    let dimension = create(&mut s, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]}));
    let line = create(&mut s, "line", json!({"points": [[4, -1], [4, 6]]}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [dimension.hex()]}), operands: vec![line], view: None }
}

fn dimbreak_chosen() -> Fixture {
    let mut f = dimbreak_automatic();
    f.params["objects"] = json!([f.operands[0].hex()]);
    f.params["size"] = json!(0.5);
    f
}

fn mleader_pair() -> Fixture {
    let mut s = Session::new();
    let line = create(&mut s, "line", json!({"points": [[0, 0], [2, 2]]}));
    s.execute("block", &json!({"name": "B", "base": [0, 0], "handles": [line.hex()], "keep": "delete"})).unwrap();
    let a = create(&mut s, "mleader", json!({"points": [[10, 0], [10, 5]], "block": "B"}));
    let b = create(&mut s, "mleader", json!({"points": [[20, 0], [20, 5]], "block": "B"}));
    let params = json!({"handles": [a.hex(), b.hex()], "at": [0, 20], "mode": "horizontal", "spacing": 1});
    Fixture { drawing: model_of(&s), params, operands: vec![a, b], view: None }
}

fn id_on_spatial_line() -> Fixture {
    let mut s = Session::new();
    let line = spatial_line(&mut s, [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
    Fixture { drawing: model_of(&s), params: json!({"at": [1.5, 0.0]}), operands: vec![line], view: None }
}

fn id_on_text() -> Fixture {
    let mut s = Session::new();
    let text = create(&mut s, "text", json!({"at": [2, 1], "height": 1, "text": "A"}));
    Fixture { drawing: model_of(&s), params: json!({"at": [2.3, 1.4]}), operands: vec![text], view: None }
}

fn vertical_spline() -> Fixture {
    // Degree 1, knots [0,0,1,1]: the controls (0,0,0) and (0,0,10). Its whole plan view is the point (0,0).
    let mut spline = cadcraft_geom::Spline::from_control(vec![cadcraft_geom::Vec2::new(0.0, 0.0), cadcraft_geom::Vec2::new(0.0, 0.0)], 1);
    spline.space = Some(cadcraft_geom::SplineSpace { control: vec![0.0, 10.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 });
    let mut s = Session::new();
    let handle = s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Spline(spline)).unwrap();
    Fixture { drawing: model_of(&s), params: json!({"at": [0.0, 0.0]}), operands: vec![handle], view: None }
}

fn long_spline() -> Fixture {
    // Cubic, clamped, uniform knots: the plan view runs along y = 0 at one unit per control and the Z of control i is i / 1000.
    let n = 100_000usize;
    let mut knots = vec![0.0; 4];
    knots.extend((1..=n - 4).map(|k| k as f64));
    knots.extend([(n - 3) as f64; 4]);
    let mut spline =
        cadcraft_geom::Spline { degree: 3, knots, control: (0..n).map(|i| cadcraft_geom::Vec2::new(i as f64, 0.0)).collect(), ..Default::default() };
    spline.space = Some(cadcraft_geom::SplineSpace {
        control: (0..n).map(|i| i as f64 / 1000.0).collect(),
        fit: Vec::new(),
        start_tangent: 0.0,
        end_tangent: 0.0,
    });
    let mut s = Session::new();
    let handle = s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Spline(spline)).unwrap();
    // The pick radius follows the view: zoomed to 2,000 units it is a few world units, so only a few spans of the spline reach the point.
    let view = json!({"mode": "window", "p1": [49000.0, -500.0], "p2": [51000.0, 500.0]});
    Fixture { drawing: model_of(&s), params: json!({"at": [50000.5, 0.0]}), operands: vec![handle], view: Some(view) }
}

fn long_spline_zoomed_out() -> Fixture {
    Fixture { view: None, ..long_spline() }
}

fn list_circle() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [circle.hex()]}), operands: vec![circle], view: None }
}

fn area_ellipse() -> Fixture {
    use cadcraft_doc::Ellipse;
    let mut s = Session::new();
    let ellipse =
        Ellipse { center: Vec3::ZERO, major: Vec3::new(3.0, 0.0, 4.0), normal: Vec3::Z, ratio: 0.5, start: 0.0, end: std::f64::consts::TAU };
    let handle = s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Ellipse(ellipse)).unwrap();
    Fixture { drawing: model_of(&s), params: json!({"handle": handle.hex()}), operands: vec![handle], view: None }
}

fn angle_between_spatial_lines() -> Fixture {
    let mut s = Session::new();
    let a = spatial_line(&mut s, [0.0; 3], [1.0, 0.0, 1.0]);
    let b = spatial_line(&mut s, [0.0; 3], [0.0, 1.0, 1.0]);
    Fixture { drawing: model_of(&s), params: json!({"h1": a.hex(), "h2": b.hex()}), operands: vec![a, b], view: None }
}

fn properties_of_circle() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [circle.hex()]}), operands: vec![circle], view: None }
}

/// The edit commands over a selection of one circle, (10,5) and radius 2 in the plane of the row.
macro_rules! selection_fixture {
    ($name:ident, $params:expr) => {
        fn $name() -> Fixture {
            let mut f = list_circle();
            let extra: Value = $params;
            for (key, value) in extra.as_object().unwrap() {
                f.params[key] = value.clone();
            }
            f
        }
    };
}

selection_fixture!(erase_fixture, json!({}));
selection_fixture!(move_fixture, json!({"delta": [3, 1]}));
selection_fixture!(copy_fixture, json!({"delta": [3, 1]}));
selection_fixture!(rotate_fixture, json!({"base": [0, 0], "angle": 90}));
selection_fixture!(scale_fixture, json!({"base": [0, 0], "factor": 2}));
selection_fixture!(mirror_fixture, json!({"p1": [0, 0], "p2": [1, 0]}));

type V3 = [f64; 3];

fn arr(v: Vec3) -> V3 {
    [v.x, v.y, v.z]
}

fn unit(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A point of the OCS of `normal` in the world: the arbitrary-axis rule, written out here and not taken from the code under test.
fn world(normal: V3, p: V3) -> V3 {
    let n = unit(normal);
    let x = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], n) } else { cross([0.0, 0.0, 1.0], n) };
    let x = unit(x);
    let y = cross(n, x);
    [0, 1, 2].map(|i| x[i] * p[0] + y[i] * p[1] + n[i] * p[2])
}

fn close3(got: V3, want: V3, what: &str) -> Result<(), String> {
    if (0..3).all(|i| (got[i] - want[i]).abs() < 1e-9) { Ok(()) } else { Err(format!("{what} {got:?} instead of {want:?}")) }
}

/// The fixture circle before the command: the center (10,5) in the OCS of the plane, lifted by the elevation of the plane.
fn start(plane: Plane) -> V3 {
    world(arr(plane.normal()), [10.0, 5.0, plane.elevation()])
}

/// The circle `handle` holds the world center, the radius and the plane normal given.
fn circle_is(d: &Drawing, handle: Handle, center: V3, radius: f64, normal: V3) -> Result<(), String> {
    let entity = d.entity(handle).ok_or("the circle is gone")?;
    let EntityKind::Circle(c) = &entity.kind else { return Err(format!("not a circle: {:?}", entity.kind)) };
    let stored = arr(entity.common.extrusion);
    close3(world(stored, arr(c.center)), center, "center")?;
    close3(unit(stored), unit(normal), "normal")?;
    near(c.radius, radius)
}

fn after_move(d: &Drawing, ops: &[Handle], plane: Plane) -> Result<(), String> {
    let w = start(plane);
    circle_is(d, ops[0], [w[0] + 3.0, w[1] + 1.0, w[2]], 2.0, arr(plane.normal()))
}

fn after_copy(d: &Drawing, ops: &[Handle], plane: Plane) -> Result<(), String> {
    circle_is(d, ops[0], start(plane), 2.0, arr(plane.normal()))?;
    let copy = d.model.iter().last().ok_or("empty model")?;
    if copy.handle == ops[0] {
        return Err("no copy was made".into());
    }
    let w = start(plane);
    circle_is(d, copy.handle, [w[0] + 3.0, w[1] + 1.0, w[2]], 2.0, arr(plane.normal()))?;
    let original = d.entity(ops[0]).ok_or("the original is gone")?;
    if copy.geometry_revision == original.geometry_revision { Ok(()) } else { Err("the copy has another revision than its source".into()) }
}

/// The quarter turn about the world Z axis through (0,0): (x,y,z) goes to (-y,x,z), the normal with it.
fn turned(v: V3) -> V3 {
    [-v[1], v[0], v[2]]
}

fn after_rotate(d: &Drawing, ops: &[Handle], plane: Plane) -> Result<(), String> {
    circle_is(d, ops[0], turned(start(plane)), 2.0, turned(arr(plane.normal())))
}

fn after_scale(d: &Drawing, ops: &[Handle], plane: Plane) -> Result<(), String> {
    // SCALE scales all three axes: the world center, Z included, is twice the start.
    let w = start(plane);
    circle_is(d, ops[0], [2.0 * w[0], 2.0 * w[1], 2.0 * w[2]], 4.0, arr(plane.normal()))
}

fn after_mirror(d: &Drawing, ops: &[Handle], plane: Plane) -> Result<(), String> {
    circle_is(d, ops[0], start(plane), 2.0, arr(plane.normal()))?;
    let w = start(plane);
    let copy = d.model.iter().last().ok_or("empty model")?;
    // The mirror line is the world X axis: y changes sign.
    circle_is(d, copy.handle, [w[0], -w[1], w[2]], 2.0, [0.0, 0.0, 1.0])
}

fn erased(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    if d.entity(ops[0]).is_none() { Ok(()) } else { Err("the circle is still there".into()) }
}

/// A flat LINE (0,0)-(3,0). Made canonical in a lifted plane it sits at that elevation.
fn flat_line() -> Fixture {
    let mut s = Session::new();
    let line = create(&mut s, "line", json!({"points": [[0, 0], [3, 0]]}));
    Fixture { drawing: model_of(&s), params: json!({"at": [1.5, 0.0]}), operands: vec![line], view: None }
}

/// A circle far from the pick, and ELEVATION 3: the pick finds nothing, so the point is at the current elevation.
fn id_in_empty_space() -> Fixture {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 3})).unwrap();
    let circle = create(&mut s, "circle", json!({"center": [100, 100], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"at": [1.5, 0.0]}), operands: vec![circle], view: None }
}

/// A solid fill over a closed polyline and ELEVATION 3: the interior of a fill is not under the pick.
fn id_in_fill_interior() -> Fixture {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 3})).unwrap();
    let vertices =
        json!([{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [10.0, 0.0], "bulge": 0.0}, {"p": [10.0, 5.0], "bulge": 0.0}, {"p": [0.0, 5.0], "bulge": 0.0}]);
    let polyline = create(&mut s, "pline", json!({"vertices": vertices, "closed": true}));
    let hatch = create(&mut s, "hatch", json!({"handles": [polyline.hex()], "pattern": "SOLID"}));
    Fixture { drawing: model_of(&s), params: json!({"at": [5.0, 2.5]}), operands: vec![hatch], view: None }
}

/// DIST from (0,0) to (3,0) over the spatial line (0,0,0)-(3,0,4): the picks take the Z of the line under them.
fn dist_on_spatial_line() -> Fixture {
    let mut f = id_on_spatial_line();
    f.params = json!({"p1": [0.0, 0.0], "p2": [3.0, 0.0]});
    f
}

fn dist_on_text() -> Fixture {
    let mut f = id_on_text();
    f.params = json!({"p1": [2.3, 1.4], "p2": [5.0, 5.0]});
    f
}

fn circle_at_elevation() -> Fixture {
    circle()
}

/// A point-based creation command over a spatial line: the new object is a legacy one on the plan view, at the current elevation, whatever is under the picks.
fn creation(params: Value) -> Fixture {
    let mut f = id_on_spatial_line();
    f.params = params;
    f
}

fn point_fixture() -> Fixture {
    creation(json!({"at": [1.5, 0.0]}))
}

fn ray_fixture() -> Fixture {
    creation(json!({"base": [1.5, 0.0], "through": [2.0, 5.0]}))
}

fn xline_through_points() -> Fixture {
    creation(json!({"base": [1.5, 0.0], "through": [2.0, 5.0]}))
}

fn xline_angle() -> Fixture {
    creation(json!({"base": [1.5, 0.0], "angle": 30}))
}

fn xline_horizontal() -> Fixture {
    creation(json!({"base": [1.5, 0.0], "hor": true}))
}

fn xline_bisector() -> Fixture {
    creation(json!({"vertex": [1.5, 0.0], "start": [5.0, 0.0], "end": [1.5, 5.0]}))
}

fn arc_start_center_end() -> Fixture {
    creation(json!({"start": [1.5, 0.0], "center": [1.5, 3.0], "end": [4.5, 3.0]}))
}

fn arc_center_start_angle() -> Fixture {
    creation(json!({"center": [1.5, 0.0], "start": [1.5, 3.0], "angle": 90}))
}

fn arc_three_points() -> Fixture {
    creation(json!({"p1": [1.5, 0.0], "p2": [3.0, 3.0], "p3": [4.5, 0.0]}))
}

/// The object a creation command made: the last one, new, legacy.
fn made(d: &Drawing, ops: &[Handle]) -> Result<std::sync::Arc<cadcraft_doc::Entity>, String> {
    let entity = d.model.iter().last().ok_or("empty model")?;
    if entity.handle == ops[0] {
        return Err("nothing was made".into());
    }
    if entity.geometry_revision != cadcraft_doc::GeometryRevision::LegacyProjectedV1 {
        return Err(format!("made a {:?} object", entity.geometry_revision));
    }
    Ok(entity.clone())
}

fn point_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    match &made(d, ops)?.kind {
        EntityKind::Point(p) => close3(arr(p.p), [1.5, 0.0, 0.0], "point"),
        other => Err(format!("made {other:?}")),
    }
}

fn ray_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    // The direction (0.5, 5) of length sqrt(25.25).
    let length = 25.25_f64.sqrt();
    match &made(d, ops)?.kind {
        EntityKind::Ray(r) => close3(arr(r.base), [1.5, 0.0, 0.0], "base").and(close3(arr(r.dir), [0.5 / length, 5.0 / length, 0.0], "direction")),
        other => Err(format!("made {other:?}")),
    }
}

fn xline_made(want: V3) -> impl Fn(&Drawing, &[Handle]) -> Result<(), String> {
    move |d, ops| match &made(d, ops)?.kind {
        EntityKind::XLine(r) => close3(arr(r.base), [1.5, 0.0, 0.0], "base").and(close3(arr(r.dir), want, "direction")),
        other => Err(format!("made {other:?}")),
    }
}

fn xline_through_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    let length = 25.25_f64.sqrt();
    xline_made([0.5 / length, 5.0 / length, 0.0])(d, ops)
}

fn xline_angle_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    xline_made([30.0_f64.to_radians().cos(), 0.5, 0.0])(d, ops)
}

fn xline_horizontal_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    xline_made([1.0, 0.0, 0.0])(d, ops)
}

fn xline_bisector_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    // The unit directions to (5,0) and to (1.5,5) from the vertex (1.5,0) are (1,0) and (0,1): their sum is the bisector.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    xline_made([h, h, 0.0])(d, ops)
}

fn arc_made(d: &Drawing, ops: &[Handle], center: V3, radius: f64, start: f64, end: f64) -> Result<(), String> {
    match &made(d, ops)?.kind {
        EntityKind::Arc(a) => {
            close3(arr(a.center), center, "center")?;
            near(a.radius, radius)?;
            near(a.start, start)?;
            near(a.end.rem_euclid(std::f64::consts::TAU), end)
        }
        other => Err(format!("made {other:?}")),
    }
}

fn arc_sce_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    // The start (1.5,0) is straight below the center (1.5,3): 270 degrees. The end (4.5,3) is at 0 degrees. The radius is 3.
    arc_made(d, ops, [1.5, 3.0, 0.0], 3.0, 1.5 * std::f64::consts::PI, 0.0)
}

fn arc_csa_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    // The start (1.5,3) is straight above the center (1.5,0): 90 degrees, and the included angle 90 degrees counterclockwise.
    arc_made(d, ops, [1.5, 0.0, 0.0], 3.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI)
}

fn arc_three_points_made(d: &Drawing, ops: &[Handle], _: Plane) -> Result<(), String> {
    // The circle through (1.5,0), (3,3), (4.5,0): the center (3, y) with 2.25 + y^2 = (3 - y)^2, so y = 1.125 and the radius 1.875.
    // The picks run clockwise, so the stored arc goes counterclockwise from the last pick (-36.87 degrees) to the first (216.87).
    let first = (-1.125_f64).atan2(-1.5).rem_euclid(std::f64::consts::TAU);
    let last = (-1.125_f64).atan2(1.5).rem_euclid(std::f64::consts::TAU);
    arc_made(d, ops, [3.0, 1.125, 0.0], 1.875, last, first)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Rows

/// The commands the rows cover, with the roles each one's rows cover. The registry lint compares both with the rows and the registry.
const DECLARED: &[(&str, &[Role])] = &[
    ("angle.measure", &[Role::Primary, Role::Secondary]),
    ("arc", &[Role::PickedPoint]),
    ("area", &[Role::Primary]),
    ("arc.continue", &[Role::Boundary]),
    ("arc.csa", &[Role::PickedPoint]),
    ("arc.sce", &[Role::PickedPoint]),
    ("centerline", &[Role::Primary, Role::Secondary]),
    ("centermark", &[Role::Primary]),
    ("copy", &[Role::SelectionMember]),
    ("dimbreak", &[Role::Boundary]),
    ("dimjogged", &[Role::Primary]),
    ("dist", &[Role::PickedPoint]),
    ("erase", &[Role::SelectionMember]),
    ("id", &[Role::PickedPoint]),
    ("list", &[Role::Primary]),
    ("mirror", &[Role::SelectionMember]),
    ("mleadercollect", &[Role::SelectionMember]),
    ("move", &[Role::SelectionMember]),
    ("point", &[Role::PickedPoint]),
    ("properties", &[Role::Primary]),
    ("ray", &[Role::PickedPoint]),
    ("revcloud", &[Role::Primary]),
    ("rotate", &[Role::SelectionMember]),
    ("scale", &[Role::SelectionMember]),
    ("wipeout", &[Role::Primary]),
    ("xline", &[Role::Primary, Role::PickedPoint]),
];

/// What the rows of one refusal case share.
struct Case {
    command: &'static str,
    role: Role,
    branch: &'static str,
    fixture: fn() -> Fixture,
    reason: &'static str,
}

/// A refusal row for each of `revisions` in each of `planes`, and the legacy control row: the same call is fine on a legacy drawing.
fn refusal_rows(out: &mut Vec<Row>, case: Case, revisions: &[Revision], planes: &[Plane]) {
    let Case { command, role, branch, fixture, reason } = case;
    for revision in revisions {
        for plane in planes {
            out.push(Row { command, role, branch, revision: *revision, plane: *plane, fixture, expect: Expect::Refuses(reason) });
        }
    }
    out.push(Row { command, role, branch, revision: Revision::Legacy, plane: Plane::PlusZ, fixture, expect: Expect::Ok(|_| Ok(())) });
}

fn live_rows() -> Vec<Row> {
    use Plane::{NegativeZ, PlusZ, PlusZElevated, Tilted};
    use Revision::{Canonical, CanonicalFirst, LegacyFirst};
    let mut rows = Vec::new();
    let out = &mut rows;
    let planes = [Tilted, NegativeZ, PlusZ];
    // Refusals before any write (C1 to C3, M3 to M7): a canonical operand in any plane is refused with "canonical".
    refusal_rows(
        out,
        Case { command: "centermark", role: Role::Primary, branch: "circle", fixture: circle, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "centerline", role: Role::Primary, branch: "spatial line first", fixture: two_lines, reason: "canonical" },
        &[Canonical, CanonicalFirst],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "centerline", role: Role::Secondary, branch: "plain line second", fixture: two_lines, reason: "canonical" },
        &[LegacyFirst],
        &planes,
    );
    for erase in [("polyline", closed_polyline as fn() -> Fixture), ("polyline erase", wipeout_erasing)] {
        refusal_rows(
            out,
            Case { command: "wipeout", role: Role::Primary, branch: erase.0, fixture: erase.1, reason: "canonical" },
            &[Canonical],
            &planes,
        );
    }
    refusal_rows(
        out,
        Case { command: "revcloud", role: Role::Primary, branch: "circle", fixture: revcloud, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "arc.continue", role: Role::Boundary, branch: "last line", fixture: arc_continue, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "xline", role: Role::Primary, branch: "offset", fixture: xline_offset, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "xline", role: Role::Primary, branch: "through", fixture: xline_through, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "dimjogged", role: Role::Primary, branch: "circle", fixture: dimjogged, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "dimbreak", role: Role::Boundary, branch: "automatic", fixture: dimbreak_automatic, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "dimbreak", role: Role::Boundary, branch: "chosen objects", fixture: dimbreak_chosen, reason: "canonical" },
        &[Canonical],
        &planes,
    );
    refusal_rows(
        out,
        Case { command: "mleadercollect", role: Role::SelectionMember, branch: "pair, canonical first", fixture: mleader_pair, reason: "canonical" },
        &[CanonicalFirst],
        &[Tilted],
    );
    refusal_rows(
        out,
        Case { command: "mleadercollect", role: Role::SelectionMember, branch: "pair, canonical second", fixture: mleader_pair, reason: "canonical" },
        &[LegacyFirst],
        &[Tilted],
    );
    // Answers from the world geometry (M8 to M10, R2): the numbers come from the plane written beside each fixture.
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "spatial line, (0,0,0)-(3,0,4) at x 1.5",
        revision: Canonical,
        plane: PlusZ,
        fixture: id_on_spatial_line,
        // Z along the line at x = 1.5 is 4 * 1.5 / 3.
        expect: Expect::Ok(|v| near(number(v, "z")?, 2.0)),
    });
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "spatial line, legacy control",
        revision: Revision::Legacy,
        plane: PlusZ,
        fixture: id_on_spatial_line,
        expect: Expect::Ok(|v| near(number(v, "z")?, 2.0)),
    });
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "text, no single Z",
        revision: Canonical,
        plane: PlusZ,
        fixture: id_on_text,
        expect: Expect::Refuses("no single Z"),
    });
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "vertical spline (0,0,0)-(0,0,10): every Z at one plan-view point",
        revision: Canonical,
        plane: PlusZ,
        fixture: vertical_spline,
        expect: Expect::Refuses("no single Z"),
    });
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "spline of 100,000 controls (i, 0, i/1000), zoomed in: the Z along it",
        revision: Canonical,
        plane: PlusZ,
        fixture: long_spline,
        // The controls reproduce the line z = x / 1000, so the Z at x = 50000.5 is 50.0005. The work bound itself (evaluations, not time)
        // is checked by the sampler's own tests; a run() row can only prove that the long spline is answered.
        expect: Expect::Ok(|v| near(number(v, "z")?, 50.0005)),
    });
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "the same spline at the fit-all view: the pick radius reaches over 500 spans",
        revision: Canonical,
        plane: PlusZ,
        fixture: long_spline_zoomed_out,
        // Past the work budget the sample is refused, with the remedy in the text: never an arbitrary Z and never a long search.
        expect: Expect::Refuses("zoom in"),
    });
    out.push(Row {
        command: "list",
        role: Role::Primary,
        branch: "circle at (10,5), normal (0,0.6,0.8)",
        revision: Canonical,
        plane: Tilted,
        fixture: list_circle,
        // The OCS x axis is (-1,0,0) and y axis (0,-0.8,0.6): the center (10,5) is the world point (-10,-4,3).
        expect: Expect::Ok(|v| {
            let text = v["message"].as_str().ok_or("no message")?;
            if text.contains("X=-10.0000  Y=-4.0000  Z=3.0000") { Ok(()) } else { Err(text.to_string()) }
        }),
    });
    out.push(Row {
        command: "area",
        role: Role::Primary,
        branch: "ellipse, major (3,0,4), ratio 0.5, legacy control",
        revision: Revision::Legacy,
        plane: PlusZ,
        fixture: area_ellipse,
        // The legacy record keeps the plan view: the major axis (3,0) is 3 long, so the area is pi * 3 * 1.5.
        expect: Expect::Ok(|v| near(number(v, "area")?, 4.5 * std::f64::consts::PI)),
    });
    out.push(Row {
        command: "area",
        role: Role::Primary,
        branch: "ellipse, major (3,0,4), ratio 0.5, in its own plane",
        revision: Canonical,
        plane: Plane::NormalY,
        fixture: area_ellipse,
        // The major axis (3,0,4) is 5 long and the minor 2.5, so the area is pi * 5 * 2.5.
        expect: Expect::Ok(|v| near(number(v, "area")?, 12.5 * std::f64::consts::PI)),
    });
    out.push(Row {
        command: "angle.measure",
        role: Role::Primary,
        branch: "two spatial lines, legacy control",
        revision: Revision::Legacy,
        plane: PlusZ,
        fixture: angle_between_spatial_lines,
        // The plan view: the directions (1,0) and (0,1).
        expect: Expect::Ok(|v| near(number(v, "angle")?, 90.0)),
    });
    out.push(Row {
        command: "angle.measure",
        role: Role::Secondary,
        branch: "two spatial lines, both canonical",
        revision: Canonical,
        plane: PlusZ,
        fixture: angle_between_spatial_lines,
        // cos = (1*0 + 0*1 + 1*1) / (sqrt 2 * sqrt 2) = 1/2.
        expect: Expect::Ok(|v| near(number(v, "angle")?, 60.0)),
    });
    out.push(Row {
        command: "properties",
        role: Role::Primary,
        branch: "circle: the OCS frame",
        revision: Canonical,
        plane: Tilted,
        fixture: properties_of_circle,
        expect: Expect::Ok(|v| {
            let frame = &v["objects"][0]["geometryFrame"];
            if frame == "ocs" { Ok(()) } else { Err(format!("frame {frame}")) }
        }),
    });
    // Raw operands (plan 4.2): a geometry edit is refused with the typed read-only reason and writes nothing.
    for (case, reason) in [
        (Case { command: "centermark", role: Role::Primary, branch: "circle", fixture: circle, reason: "" }, "read-only"),
        (Case { command: "centerline", role: Role::Primary, branch: "spatial line first", fixture: two_lines, reason: "" }, "read-only"),
        (Case { command: "centerline", role: Role::Secondary, branch: "plain line second", fixture: two_lines, reason: "" }, "read-only"),
        (Case { command: "wipeout", role: Role::Primary, branch: "polyline", fixture: closed_polyline, reason: "" }, "read-only"),
        (Case { command: "wipeout", role: Role::Primary, branch: "polyline erase", fixture: wipeout_erasing, reason: "" }, "read-only"),
        (Case { command: "revcloud", role: Role::Primary, branch: "circle", fixture: revcloud, reason: "" }, "read-only"),
        (Case { command: "xline", role: Role::Primary, branch: "offset", fixture: xline_offset, reason: "" }, "read-only"),
        (Case { command: "xline", role: Role::Primary, branch: "through", fixture: xline_through, reason: "" }, "read-only"),
        (Case { command: "dimjogged", role: Role::Primary, branch: "circle", fixture: dimjogged, reason: "" }, "read-only"),
        (Case { command: "dimbreak", role: Role::Boundary, branch: "automatic", fixture: dimbreak_automatic, reason: "" }, "read-only"),
        (Case { command: "dimbreak", role: Role::Boundary, branch: "chosen objects", fixture: dimbreak_chosen, reason: "" }, "read-only"),
        // A raw line is not a line to continue from, and a raw leader is not a multileader with block content.
        (
            Case { command: "arc.continue", role: Role::Boundary, branch: "last line", fixture: arc_continue, reason: "" },
            "no line or arc to continue from",
        ),
        (
            Case { command: "mleadercollect", role: Role::SelectionMember, branch: "pair, canonical first", fixture: mleader_pair, reason: "" },
            "need two or more multileaders with block content",
        ),
        (
            Case { command: "mleadercollect", role: Role::SelectionMember, branch: "pair, canonical second", fixture: mleader_pair, reason: "" },
            "need two or more multileaders with block content",
        ),
    ] {
        out.push(Row {
            command: case.command,
            role: case.role,
            branch: case.branch,
            revision: Revision::Raw,
            plane: PlusZ,
            fixture: case.fixture,
            expect: Expect::Refuses(reason),
        });
    }
    // The inquiries read a raw unit as it is kept: LIST names its kind, PROPERTIES gives the record's groups.
    out.push(Row {
        command: "list",
        role: Role::Primary,
        branch: "circle at (10,5), kept as read",
        revision: Revision::Raw,
        plane: PlusZ,
        fixture: list_circle,
        expect: Expect::Ok(|v| {
            let text = v["message"].as_str().ok_or("no message")?;
            if text.contains("CIRCLE") { Ok(()) } else { Err(text.to_string()) }
        }),
    });
    out.push(Row {
        command: "properties",
        role: Role::Primary,
        branch: "circle: the kept record",
        revision: Revision::Raw,
        plane: PlusZ,
        fixture: properties_of_circle,
        expect: Expect::Ok(|v| {
            let kind = &v["objects"][0]["geometry"]["parent"]["kind"];
            if kind == "CIRCLE" { Ok(()) } else { Err(format!("kind {kind}")) }
        }),
    });
    // Editing the selection: the independent numbers come from the world transform of each plane (`start`, `turned`, `after_*`).
    let planes = [PlusZ, PlusZElevated, Tilted, NegativeZ];
    let edits: [(&str, fn() -> Fixture, Expect); 5] = [
        ("move", move_fixture, Expect::Edits(after_move)),
        ("copy", copy_fixture, Expect::Edits(after_copy)),
        ("rotate", rotate_fixture, Expect::Edits(after_rotate)),
        ("erase", erase_fixture, Expect::Edits(erased)),
        ("scale", scale_fixture, Expect::Edits(after_scale)),
    ];
    for (command, fixture, expect) in edits {
        let Expect::Edits(check) = expect else { unreachable!() };
        let row = |revision, plane, expect| Row { command, role: Role::SelectionMember, branch: "circle (10,5)", revision, plane, fixture, expect };
        out.push(row(Revision::Legacy, PlusZ, Expect::Edits(check)));
        for plane in planes {
            out.push(row(Canonical, plane, Expect::Edits(check)));
        }
        // ERASE removes any unit; the others edit its geometry, which a raw unit refuses.
        out.push(row(Revision::Raw, PlusZ, if command == "erase" { Expect::Edits(check) } else { Expect::Refuses("read-only") }));
    }
    // MIRROR keeps the source and adds the mirrored circle.
    let mirror = |revision, expect| Row {
        command: "mirror",
        role: Role::SelectionMember,
        branch: "circle (10,5)",
        revision,
        plane: PlusZ,
        fixture: mirror_fixture,
        expect,
    };
    out.push(mirror(Revision::Legacy, Expect::Edits(after_mirror)));
    // A reflection of canonical geometry has no measured convention (the normal of a mirrored circle, the sweep of a mirrored arc):
    // it is refused in every plane, and the reason names the reflection.
    for plane in planes {
        out.push(Row { plane, ..mirror(Canonical, Expect::Refuses("reflection")) });
    }
    out.push(mirror(Revision::Raw, Expect::Refuses("read-only")));
    // The object under a picked point gives its Z back; the interior of a fill and empty space give the current elevation.
    for (revision, plane, check) in [(Canonical, PlusZElevated, z_is_5 as Check), (Revision::Legacy, PlusZ, z_is_0)] {
        out.push(Row {
            command: "id",
            role: Role::PickedPoint,
            branch: "flat line (0,0)-(3,0) at its own elevation",
            revision,
            plane,
            fixture: flat_line,
            expect: Expect::Ok(check),
        });
    }
    for revision in [Revision::Legacy, Canonical, Revision::Raw] {
        out.push(Row {
            command: "id",
            role: Role::PickedPoint,
            branch: "empty space: the current elevation",
            revision,
            plane: PlusZ,
            fixture: id_in_empty_space,
            expect: Expect::Ok(z_is_3),
        });
    }
    for (revision, plane) in [(Revision::Legacy, PlusZ), (Canonical, PlusZ), (Canonical, Tilted), (Revision::Raw, PlusZ)] {
        out.push(Row {
            command: "id",
            role: Role::PickedPoint,
            branch: "fill interior: the current elevation",
            revision,
            plane,
            fixture: id_in_fill_interior,
            expect: Expect::Ok(z_is_3),
        });
    }
    out.push(Row {
        command: "id",
        role: Role::PickedPoint,
        branch: "spatial line, (0,0,0)-(3,0,4) at x 1.5",
        revision: Revision::Raw,
        plane: PlusZ,
        fixture: id_on_spatial_line,
        // A raw unit has no geometry to pick (the pick skips it): the point is at the current elevation, not on the line.
        expect: Expect::Ok(z_is_0),
    });
    out.push(Row {
        command: "dist",
        role: Role::PickedPoint,
        branch: "spatial line (0,0,0)-(3,0,4), from x 0 to x 3",
        revision: Revision::Legacy,
        plane: PlusZ,
        fixture: dist_on_spatial_line,
        // The picks take the Z of the line: (0,0,0) and (3,0,4), so 3-4-5.
        expect: Expect::Ok(|v| near(number(v, "distance")?, 5.0).and(near(number(v, "dz")?, 4.0))),
    });
    out.push(Row {
        command: "dist",
        role: Role::PickedPoint,
        branch: "spatial line (0,0,0)-(3,0,4), from x 0 to x 3",
        revision: Canonical,
        plane: PlusZ,
        fixture: dist_on_spatial_line,
        expect: Expect::Ok(|v| near(number(v, "distance")?, 5.0).and(near(number(v, "dz")?, 4.0))),
    });
    out.push(Row {
        command: "dist",
        role: Role::PickedPoint,
        branch: "spatial line (0,0,0)-(3,0,4), from x 0 to x 3",
        revision: Revision::Raw,
        plane: PlusZ,
        fixture: dist_on_spatial_line,
        // The same: the picks are at the current elevation, so the distance is the plan distance.
        expect: Expect::Ok(|v| near(number(v, "distance")?, 3.0).and(near(number(v, "dz")?, 0.0))),
    });
    out.push(Row {
        command: "dist",
        role: Role::PickedPoint,
        branch: "text, no single Z",
        revision: Canonical,
        plane: PlusZ,
        fixture: dist_on_text,
        expect: Expect::Refuses("no single Z"),
    });
    // The lifted plane: LIST reads the Z of the center, AREA does not depend on it.
    out.push(Row {
        command: "list",
        role: Role::Primary,
        branch: "circle at (10,5), elevation 5",
        revision: Canonical,
        plane: PlusZElevated,
        fixture: list_circle,
        expect: Expect::Ok(|v| {
            let text = v["message"].as_str().ok_or("no message")?;
            if text.contains("X=10.0000  Y=5.0000  Z=5.0000") { Ok(()) } else { Err(text.to_string()) }
        }),
    });
    out.push(Row {
        command: "area",
        role: Role::Primary,
        branch: "circle r2 at elevation 5",
        revision: Canonical,
        plane: PlusZElevated,
        fixture: circle_at_elevation,
        expect: Expect::Ok(|v| near(number(v, "area")?, 4.0 * std::f64::consts::PI)),
    });
    // Point-based creation: nothing under the picks changes the new object, in any revision of the operand.
    let creations: [(&str, &str, fn() -> Fixture, Expect); 9] = [
        ("point", "at", point_fixture, Expect::Edits(point_made)),
        ("ray", "base and through", ray_fixture, Expect::Edits(ray_made)),
        ("xline", "base and through", xline_through_points, Expect::Edits(xline_through_made)),
        ("xline", "base and angle", xline_angle, Expect::Edits(xline_angle_made)),
        ("xline", "base, horizontal", xline_horizontal, Expect::Edits(xline_horizontal_made)),
        ("xline", "bisector", xline_bisector, Expect::Edits(xline_bisector_made)),
        ("arc.sce", "start, center, end", arc_start_center_end, Expect::Edits(arc_sce_made)),
        ("arc.csa", "center, start, angle", arc_center_start_angle, Expect::Edits(arc_csa_made)),
        ("arc", "three points", arc_three_points, Expect::Edits(arc_three_points_made)),
    ];
    for (command, branch, fixture, expect) in creations {
        let Expect::Edits(check) = expect else { unreachable!() };
        for (revision, plane) in [(Revision::Legacy, PlusZ), (Canonical, Tilted), (Revision::Raw, PlusZ)] {
            out.push(Row { command, role: Role::PickedPoint, branch, revision, plane, fixture, expect: Expect::Edits(check) });
        }
    }
    rows
}

/// An open finding of the plan. Its row, when it has one, states what the fix must deliver and must fail today.
struct Pending {
    finding: &'static str,
    what: &'static str,
    row: Option<Row>,
}

fn pending() -> Vec<Pending> {
    Vec::new()
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Tests

#[test]
fn the_live_rows_hold() {
    let failures: Vec<String> = live_rows().iter().filter_map(|row| run_row(row).err().map(|e| format!("{}: {e}", row.name()))).collect();
    assert!(failures.is_empty(), "{} of {} rows failed:\n{}", failures.len(), live_rows().len(), failures.join("\n"));
}

#[test]
fn a_pending_row_still_fails_and_is_removed_when_its_fix_lands() {
    for pending in pending() {
        if let Some(row) = &pending.row {
            // The legacy control inside `run_row` must still succeed: only the expectation of the canonical call may fail.
            let outcome = run_row(row);
            assert!(outcome.is_err(), "{} now passes ({}): make the row live and drop it from `pending()`", pending.finding, row.name());
            let error = outcome.err().unwrap_or_default();
            assert!(!error.contains("the legacy control fails"), "{}: the row is malformed: {error}", pending.finding);
        }
    }
}

#[test]
fn the_pending_rows_are_exactly_the_open_findings() {
    let findings: Vec<&str> = pending().iter().map(|p| p.finding).collect();
    assert_eq!(findings, Vec::<&str>::new(), "F3 is closed (5774fbf8): a new finding gets its entry here");
    assert!(pending().iter().all(|p| !p.what.is_empty()));
}

#[test]
fn the_closed_f3_finding_is_held_by_the_canonical_preview_tests() {
    // F3 (the DIMTEDIT preview of a canonical dimension in a tilted plane) is an interactive row, not a run() row: its tests live in
    // the preview suite. If that test is renamed or removed, the finding is open again.
    let previews = include_str!("it/canonical_previews.rs");
    assert!(previews.contains("fn dimtedit_refuses_a_tilted_canonical_dimension_before_preselection_or_picking"));
}

#[test]
fn every_row_names_a_declared_command_and_role_and_every_declared_role_has_a_live_row() {
    let declared: BTreeSet<(&str, Role)> = DECLARED.iter().flat_map(|(command, roles)| roles.iter().map(move |role| (*command, *role))).collect();
    let rows = live_rows();
    let covered: BTreeSet<(&str, Role)> = rows.iter().map(|row| (row.command, row.role)).collect();
    assert_eq!(covered, declared, "rows without a declaration, or declarations without a row");
    for row in rows.iter().chain(pending().iter().filter_map(|p| p.row.as_ref())) {
        assert!(find(row.command), "{} is not a registered command", row.command);
    }
}

fn find(command: &str) -> bool {
    command_specs().iter().any(|spec| spec.id == command)
}

#[test]
fn every_registered_command_has_rows_or_an_explicit_entry_in_the_undeclared_list() {
    let registry: BTreeSet<&str> = command_specs().iter().map(|spec| spec.id).collect();
    let declared: BTreeSet<&str> = DECLARED.iter().map(|(command, _)| *command).collect();
    let undeclared: BTreeSet<&str> = include_str!("consumer_matrix/undeclared.txt").lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let unknown: Vec<_> = registry.iter().filter(|c| !declared.contains(**c) && !undeclared.contains(**c)).collect();
    assert!(unknown.is_empty(), "commands with no rows and no entry in consumer_matrix/undeclared.txt: {unknown:?}");
    let stale: Vec<_> = undeclared.iter().filter(|c| !registry.contains(**c)).collect();
    assert!(stale.is_empty(), "undeclared entries that are not registered commands: {stale:?}");
    let both: Vec<_> = undeclared.iter().filter(|c| declared.contains(**c)).collect();
    assert!(both.is_empty(), "commands that have rows and are still listed as undeclared: {both:?}");
}
