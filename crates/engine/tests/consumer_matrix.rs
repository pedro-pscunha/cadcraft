//! The stage-0 consumer matrix (reader activation plan v3, section 6.3): one table of rows, each a tuple of command, input role,
//! revision, plane and parameter branch with an expected outcome, run through `Session::execute`.
//!
//! - A live row states `Ok(numbers)` or `Refuses(reason)` and is checked: an `Ok` row must reach the operand and give the independent
//!   number, a refusal must name the reason and leave the model and the handle counter as they were. A missing-argument error
//!   does not count: every row's control with the same parameters on a legacy drawing must succeed.
//! - A pending row belongs to an open finding of the plan (F1 to F3). Its expectation is what the fix must deliver, it must still
//!   fail today, and the test that checks it fails the day it passes, so the row cannot outlive its fix silently.
//! - The registry lint: every registered command is declared here with the roles its rows cover, or is in
//!   `consumer_matrix/undeclared.txt`. A new command gets rows or an explicit entry; a declared command leaves the list.
//!
//! The layers a `run()` sweep cannot reach (interactive previews, UI adapters, paper-viewport display) stay as their own tests; the
//! pending F3 entry names the DIMTEDIT preview row that belongs to the first of them.

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plane {
    PlusZ,
    /// The normal (0, 3, 4) / 5.
    Tilted,
    NegativeZ,
    /// The normal (0, 1, 0): the plane of the major axis (3, 0, 4).
    NormalY,
}

impl Plane {
    fn normal(self) -> Vec3 {
        match self {
            Plane::PlusZ => Vec3::Z,
            Plane::Tilted => Vec3::new(0.0, 0.6, 0.8),
            Plane::NegativeZ => Vec3::new(0.0, 0.0, -1.0),
            Plane::NormalY => Vec3::new(0.0, 1.0, 0.0),
        }
    }
}

type Check = fn(&Value) -> Result<(), String>;

enum Expect {
    /// The command succeeds and the check on its result holds.
    Ok(Check),
    /// The command is refused with an error that contains this text, and writes nothing.
    Refuses(&'static str),
}

/// A drawing, the command parameters and the entities the revision applies to, in role order.
struct Fixture {
    drawing: Drawing,
    params: Value,
    operands: Vec<Handle>,
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

/// Run a row. `Err` names the way it failed.
fn run_row(row: &Row) -> Result<(), String> {
    let fixture = (row.fixture)();
    // The control: the same command and parameters on the legacy drawing must succeed, so a row never passes on a malformed call.
    let mut control = session_of(&fixture.drawing);
    control.execute(row.command, &fixture.params).map_err(|error| format!("the legacy control fails: {error}"))?;
    let mut session = session_of(&fixture.drawing);
    for (index, handle) in fixture.operands.iter().enumerate() {
        let canonical = match row.revision {
            Revision::Legacy => false,
            Revision::Canonical => true,
            Revision::LegacyFirst => index > 0,
            Revision::CanonicalFirst => index == 0,
        };
        if canonical {
            make_canonical_in(&mut session, *handle, row.plane.normal());
        }
    }
    let before = session.doc().map_err(|e| e.to_string())?.model.clone();
    let seed = session.doc().map_err(|e| e.to_string())?.handseed;
    let result = session.execute(row.command, &fixture.params);
    match (&row.expect, result) {
        (Expect::Ok(check), Ok(value)) => check(&value),
        (Expect::Ok(_), Err(error)) => Err(format!("refused: {error}")),
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
    Fixture { drawing: model_of(&s), params: json!({"handle": circle.hex()}), operands: vec![circle] }
}

fn two_lines() -> Fixture {
    let mut s = Session::new();
    let a = spatial_line(&mut s, [10.0, 0.0, 0.0], [13.0, 0.0, 4.0]);
    let b = create(&mut s, "line", json!({"points": [[20, 4], [10, 4]]}));
    Fixture { drawing: model_of(&s), params: json!({"h1": a.hex(), "h2": b.hex()}), operands: vec![a, b] }
}

fn closed_polyline() -> Fixture {
    let mut s = Session::new();
    let vertices =
        json!([{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [10.0, 0.0], "bulge": 0.0}, {"p": [10.0, 5.0], "bulge": 0.0}, {"p": [0.0, 5.0], "bulge": 0.0}]);
    let polyline = create(&mut s, "pline", json!({"vertices": vertices, "closed": true}));
    Fixture { drawing: model_of(&s), params: json!({"handle": polyline.hex(), "erase": false}), operands: vec![polyline] }
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
    Fixture { drawing: model_of(&s), params: json!({"end": [2, 1]}), operands: vec![line] }
}

fn xline_offset() -> Fixture {
    let mut s = Session::new();
    let line = create(&mut s, "line", json!({"points": [[0, 0], [10, 0]]}));
    Fixture { drawing: model_of(&s), params: json!({"handle": line.hex(), "distance": 2, "side": [5, -1]}), operands: vec![line] }
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
    Fixture { drawing: model_of(&s), params, operands: vec![circle] }
}

fn dimbreak_automatic() -> Fixture {
    let mut s = Session::new();
    let dimension = create(&mut s, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]}));
    let line = create(&mut s, "line", json!({"points": [[4, -1], [4, 6]]}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [dimension.hex()]}), operands: vec![line] }
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
    Fixture { drawing: model_of(&s), params, operands: vec![a, b] }
}

fn id_on_spatial_line() -> Fixture {
    let mut s = Session::new();
    let line = spatial_line(&mut s, [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
    Fixture { drawing: model_of(&s), params: json!({"at": [1.5, 0.0]}), operands: vec![line] }
}

fn id_on_text() -> Fixture {
    let mut s = Session::new();
    let text = create(&mut s, "text", json!({"at": [2, 1], "height": 1, "text": "A"}));
    Fixture { drawing: model_of(&s), params: json!({"at": [2.3, 1.4]}), operands: vec![text] }
}

fn vertical_spline() -> Fixture {
    // Degree 1, knots [0,0,1,1]: the controls (0,0,0) and (0,0,10). Its whole plan view is the point (0,0).
    let mut spline = cadcraft_geom::Spline::from_control(vec![cadcraft_geom::Vec2::new(0.0, 0.0), cadcraft_geom::Vec2::new(0.0, 0.0)], 1);
    spline.space = Some(cadcraft_geom::SplineSpace { control: vec![0.0, 10.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 });
    let mut s = Session::new();
    let handle = s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Spline(spline)).unwrap();
    Fixture { drawing: model_of(&s), params: json!({"at": [0.0, 0.0]}), operands: vec![handle] }
}

fn list_circle() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [circle.hex()]}), operands: vec![circle] }
}

fn area_ellipse() -> Fixture {
    use cadcraft_doc::Ellipse;
    let mut s = Session::new();
    let ellipse =
        Ellipse { center: Vec3::ZERO, major: Vec3::new(3.0, 0.0, 4.0), normal: Vec3::Z, ratio: 0.5, start: 0.0, end: std::f64::consts::TAU };
    let handle = s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Ellipse(ellipse)).unwrap();
    Fixture { drawing: model_of(&s), params: json!({"handle": handle.hex()}), operands: vec![handle] }
}

fn angle_between_spatial_lines() -> Fixture {
    let mut s = Session::new();
    let a = spatial_line(&mut s, [0.0; 3], [1.0, 0.0, 1.0]);
    let b = spatial_line(&mut s, [0.0; 3], [0.0, 1.0, 1.0]);
    Fixture { drawing: model_of(&s), params: json!({"h1": a.hex(), "h2": b.hex()}), operands: vec![a, b] }
}

fn properties_of_circle() -> Fixture {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    Fixture { drawing: model_of(&s), params: json!({"handles": [circle.hex()]}), operands: vec![circle] }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Rows

/// The commands the rows cover, with the roles each one's rows cover. The registry lint compares both with the rows and the registry.
const DECLARED: &[(&str, &[Role])] = &[
    ("angle.measure", &[Role::Primary, Role::Secondary]),
    ("area", &[Role::Primary]),
    ("arc.continue", &[Role::Boundary]),
    ("centerline", &[Role::Primary, Role::Secondary]),
    ("centermark", &[Role::Primary]),
    ("dimbreak", &[Role::Boundary]),
    ("dimjogged", &[Role::Primary]),
    ("id", &[Role::PickedPoint]),
    ("list", &[Role::Primary]),
    ("mleadercollect", &[Role::SelectionMember]),
    ("properties", &[Role::Primary]),
    ("revcloud", &[Role::Primary]),
    ("wipeout", &[Role::Primary]),
    ("xline", &[Role::Primary]),
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
    use Plane::{NegativeZ, PlusZ, Tilted};
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
    rows
}

/// An open finding of the plan. Its row, when it has one, states what the fix must deliver and must fail today.
struct Pending {
    finding: &'static str,
    what: &'static str,
    row: Option<Row>,
}

fn pending() -> Vec<Pending> {
    vec![
        Pending {
            finding: "F1",
            what: "ID on a vertical canonical spline: its whole plan view is one point, so the sampler may answer Ambiguous or Unsupported, never an arbitrary Z",
            row: Some(Row {
                command: "id",
                role: Role::PickedPoint,
                branch: "vertical spline (0,0,0)-(0,0,10)",
                revision: Revision::Canonical,
                plane: Plane::PlusZ,
                fixture: vertical_spline,
                expect: Expect::Refuses("no single Z"),
            }),
        },
        Pending {
            finding: "F2",
            what: "ID/DIST/snap on a canonical spatial spline with 10,000 controls: the sampler validates the knots once and bounds its search; \
                   the test counts the evaluations, not the elapsed time",
            row: None,
        },
        Pending {
            finding: "F3",
            what: "DIMTEDIT preview of a canonical dimension in the plane (0,3,4): the cursor is converted to the dimension plane (or the command refuses) \
                   and the preview's text position equals an independent world position; an interactive row, not a run() row",
            row: None,
        },
    ]
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
    assert_eq!(findings, vec!["F1", "F2", "F3"]);
    assert!(pending().iter().all(|p| !p.what.is_empty()));
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
