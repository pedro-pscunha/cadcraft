//! The stage-1 consumer sweep (reader activation plan v3, section 6.3): every command that takes an operand and has no curated row in
//! `consumer_matrix.rs` runs once on a legacy circle and line, once with the two made canonical in a tilted plane and once with the
//! two made raw. The first parameter template that the legacy run accepts is the call.
//!
//! - The outcome per revision (works, or refused with the reason) is recorded in `consumer_matrix/sweep.txt`. A new operand command
//!   without a line there fails the matrix lint, and a changed outcome shows as a diff of that file. To accept a change, run this test
//!   with `CADCRAFT_SWEEP_UPDATE=1` and read the diff.
//! - Four invariants hold for every command, whatever its outcome: it does not panic; a refusal leaves the model and the handle counter
//!   as they were; a raw unit that survives keeps its record; a canonical object that survives stays canonical and of the same kind.
//! - A command with no accepted template is listed as unprobed. It has an operand, so it stays an explicit gap in the report and its
//!   count can only go down (`consumer_matrix.rs`, `UNPROBED_CEILING`).

mod consumer_support;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_doc::{Drawing, GeometryRevision, Handle};
use cadcraft_engine::{Session, command_specs};
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::{Value, json};

/// The templates, tried in order. A token names an operand of `setup`: `<H>` and `<H2>` circles, `<L>` and `<L2>` crossing lines, `<K>` a
/// line on layer L1, `<T>` text, `<P>` a closed polyline, `<S>` a spline, `<D>` and `<D2>` dimensions, `<G>` a hatch, `<I>` a block
/// reference, `<R>` and `<R2>` regions, `<M>` and `<M2>` multileaders, `<B>` a table.
const TEMPLATES: &[&str] = &[
    r#"{"handles":["<H>"]}"#,
    r#"{"handle":"<H>"}"#,
    r#"{"handles":["<L>"]}"#,
    r#"{"handle":"<L>"}"#,
    r#"{"handles":["<T>"]}"#,
    r#"{"handle":"<T>"}"#,
    r#"{"handles":["<P>"]}"#,
    r#"{"handle":"<P>"}"#,
    r#"{"handles":["<S>"]}"#,
    r#"{"handle":"<S>"}"#,
    r#"{"handles":["<D>"]}"#,
    r#"{"handle":"<D>"}"#,
    r#"{"handles":["<G>"]}"#,
    r#"{"handle":"<G>"}"#,
    r#"{"handles":["<I>"]}"#,
    r#"{"handle":"<I>"}"#,
    r#"{"handles":["<R>"]}"#,
    r#"{"handles":["<R>","<R2>"]}"#,
    r#"{"handles":["<M>"]}"#,
    r#"{"handle":"<M>"}"#,
    r#"{"handles":["<M>","<M2>"]}"#,
    r#"{"handle":"<B>"}"#,
    r#"{"handles":["<B>"]}"#,
    r#"{"handles":["<H>","<L>"]}"#,
    r#"{"handles":["<L>","<L2>"]}"#,
    r#"{"handles":["<H>"],"delta":[3,1]}"#,
    r#"{"handles":["<H>"],"base":[0,0]}"#,
    r#"{"handles":["<H>"],"base":[0,0],"angle":30}"#,
    r#"{"handles":["<H>"],"p1":[0,0],"p2":[1,0]}"#,
    r#"{"handles":["<H>"],"at":[0,20]}"#,
    r#"{"handles":["<H>"],"layer":"0"}"#,
    r#"{"handles":["<H>"],"color":"red"}"#,
    r#"{"handles":["<H>"],"order":"front"}"#,
    r#"{"handles":["<H>","<L>"],"at":[0,20]}"#,
    r#"{"handles":["<H>"],"width":0.5}"#,
    r#"{"handles":["<H>"],"rows":2,"cols":2,"rowSpacing":5,"colSpacing":5}"#,
    r#"{"handles":["<H>"],"center":[0,0],"count":4}"#,
    r#"{"handles":["<H>"],"s1":[0,0],"d1":[1,1]}"#,
    r#"{"handles":["<H>"],"base":[0,0],"ops":[{"move":[1,1]}]}"#,
    r#"{"handles":["<H>"],"path":"<L>","count":3}"#,
    r#"{"handles":["<H>"],"reference":["<L>"]}"#,
    r#"{"handles":["<H>"],"index":0,"to":[1,1]}"#,
    r#"{"handles":["<H>"],"base":[0,0],"factor":2}"#,
    r#"{"handles":["<H>"],"base":[0,0],"to":[1,0]}"#,
    r#"{"handles":["<H>"],"name":"G1"}"#,
    r#"{"handles":["<H>"],"scale":2}"#,
    r#"{"handles":["<H>"],"pattern":"SOLID"}"#,
    r#"{"handles":["<P>"],"pattern":"SOLID"}"#,
    r#"{"handles":["<G>"],"scale":2}"#,
    r#"{"handles":["<L>","<L2>"],"close":false}"#,
    r#"{"handles":["<D>"],"angle":10}"#,
    r#"{"handles":["<D>"],"mode":"new","text":"x"}"#,
    r#"{"handles":["<D>"],"at":[5,6]}"#,
    r#"{"handle":"<D>","at":[5,6]}"#,
    r#"{"handles":["<D2>"],"base":"<D>"}"#,
    r#"{"base":"<D>","handles":["<D2>"]}"#,
    r#"{"handle":"<D>","remove":true}"#,
    r#"{"handles":["<I>"],"tags":"*"}"#,
    r#"{"handle":"<I>","pick":[60,10]}"#,
    r#"{"handle":"<I>","delta":[1,1]}"#,
    r#"{"name":"B1","handle":"<I>"}"#,
    r#"{"handle":"<M>","points":[[79,0]]}"#,
    r#"{"handles":["<M>"],"option":"distribute"}"#,
    r#"{"handles":["<M>","<M2>"],"at":[0,20]}"#,
    r#"{"from":["<R>"],"remove":["<R2>"]}"#,
    r#"{"handle":"<B>","row":0,"col":0,"text":"x"}"#,
    r#"{"handle":"<B>","row":0}"#,
    r#"{"handle":"<B>","col":0}"#,
    r#"{"handle":"<B>","row":0,"col":0,"rows":1,"cols":1}"#,
    r#"{"handle":"<B>","style":""}"#,
    r#"{"handle":"<B>","row":0,"col":0,"function":"sum"}"#,
    r#"{"handle":"<B>","at":[0,30],"text":"x"}"#,
    r#"{"handle":"<B>","enabled":true}"#,
    r#"{"handle":"<T>","text":"y"}"#,
    r#"{"handle":"<H>","at":[0,20]}"#,
    r#"{"handle":"<H>","p1":[0,0],"p2":[1,0]}"#,
    r#"{"handle":"<H>","distance":1,"side":[0,0]}"#,
    r#"{"handle":"<H>","segments":4}"#,
    r#"{"handle":"<H>","option":"close"}"#,
    r#"{"handle":"<P>","option":"close"}"#,
    r#"{"handle":"<P>","polyline":true,"radius":1}"#,
    r#"{"handle":"<S>","option":"close"}"#,
    r#"{"handle":"<S>","option":"reverse"}"#,
    r#"{"handle":"<L>","pick":[1,0]}"#,
    r#"{"handle":"<L>","pick":[4,0],"edges":["<L2>"]}"#,
    r#"{"handle":"<L>","at":[1,0]}"#,
    r#"{"handle":"<L>","p1":[1,0],"p2":[3,0]}"#,
    r#"{"handle":"<L>","distance":1,"side":[0,5]}"#,
    r#"{"handle":"<L>","length":1}"#,
    r#"{"handle":"<L>","segments":4}"#,
    r#"{"handle":"<L>","through":[2,3]}"#,
    r#"{"h1":"<H>","h2":"<L>"}"#,
    r#"{"h1":"<L>","h2":"<L2>"}"#,
    r#"{"h1":"<H>","h2":"<H2>"}"#,
    r#"{"h1":"<L>","h2":"<H>","h3":"<H2>"}"#,
    r#"{"h1":"<L>","p1":[2,0],"h2":"<L2>","p2":[3,2],"radius":1}"#,
    r#"{"h1":"<L>","p1":[2,0],"h2":"<L2>","p2":[3,2]}"#,
    r#"{"h1":"<L>","p1":[2,0],"h2":"<L2>","p2":[3,2],"d1":1,"d2":1}"#,
    r#"{"h1":"<H>","p1":[12,5],"h2":"<L>","p2":[1,0],"radius":1}"#,
    r#"{"h1":"<H>","p1":[12,5],"h2":"<L>","p2":[1,0]}"#,
    r#"{"h1":"<H>","p1":[12,5],"h2":"<L>","p2":[1,0],"h3":"<H2>","p3":[28,5]}"#,
    r#"{"type":"coincident","h1":"<L>","p1":"end","h2":"<L2>","p2":"start"}"#,
    r#"{"type":"fix","h1":"<L>"}"#,
    r#"{"source":"<H>","targets":["<L>"]}"#,
    r#"{"add":["<H>"]}"#,
    r#"{"name":"G1","handles":["<H>"]}"#,
    r#"{"name":"G1","add":["<H>"]}"#,
    r#"{"name":"G1"}"#,
    r#"{"names":["L1"]}"#,
    r#"{"from":["L1"],"to":"0"}"#,
    r#"{"name":"B1","base":[0,0],"handles":["<H>"]}"#,
    r#"{"name":"B1"}"#,
    r#"{"window":[[0,0],[20,20]],"delta":[1,1]}"#,
    r#"{"points":[[45,2.5]],"pattern":"SOLID"}"#,
    r#"{"handles":["<G>"],"pattern":"ANSI31"}"#,
    r#"{"handles":["<M2>"],"to":"<M>"}"#,
    r#"{"object":"<L>","at":[5,3]}"#,
    r#"{"object":"<P>"}"#,
    r#"{"handle":"<P>","height":2}"#,
    r#"{"handle":"<P>","index":0,"option":"toArc"}"#,
    r#"{"handle":"<B>","row":1,"col":1,"function":"sum"}"#,
    r#"{"handle":"<B>","row":1,"col":0,"rows":2,"cols":2}"#,
    r#"{"name":"B1","description":"d"}"#,
    r#"{"find":"*","replace":"x"}"#,
    r#"{"code":"(+ 1 2)"}"#,
    r#"{"handles":["<H>"],"layer":"0","clear":true}"#,
];

/// An invariant that a command breaks on purpose, with the reason. Empty: every command keeps all four.
const ALLOWED: &[(&str, &str, &str)] = &[];

struct Setup {
    drawing: Drawing,
    /// The operands by token, in creation order.
    operands: Vec<(&'static str, Handle)>,
}

fn setup() -> Setup {
    let mut s = Session::new();
    let mut operands = Vec::new();
    let mut put = |token: &'static str, handle: Handle| operands.push((token, handle));
    put("H", create(&mut s, "circle", json!({"center": [10, 5], "radius": 2})));
    put("H2", create(&mut s, "circle", json!({"center": [30, 5], "radius": 2})));
    put("L", create(&mut s, "line", json!({"points": [[0, 0], [5, 0]]})));
    put("L2", create(&mut s, "line", json!({"points": [[3, -2], [3, 3]]})));
    put("T", create(&mut s, "text", json!({"at": [2, 8], "height": 1, "text": "ab"})));
    let vertices = json!([{"p": [40.0, 0.0], "bulge": 0.0}, {"p": [50.0, 0.0], "bulge": 0.0}, {"p": [50.0, 5.0], "bulge": 0.0}, {"p": [40.0, 5.0], "bulge": 0.0}]);
    let polyline = create(&mut s, "pline", json!({"vertices": vertices, "closed": true}));
    put("P", polyline);
    put("S", create(&mut s, "spline", json!({"fit": [[0, 10], [2, 12], [4, 10]]})));
    put("D", create(&mut s, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]})));
    put("D2", create(&mut s, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 5]})));
    put("G", create(&mut s, "hatch", json!({"handles": [polyline.hex()], "pattern": "SOLID"})));
    let source = create(&mut s, "line", json!({"points": [[60, 0], [61, 1]]}));
    s.execute("block", &json!({"name": "B1", "base": [60, 0], "handles": [source.hex()], "keep": "delete"})).unwrap();
    put("I", create(&mut s, "insert", json!({"name": "B1", "at": [60, 10]})));
    for (token, x) in [("R", 70), ("R2", 71)] {
        let circle = create(&mut s, "circle", json!({"center": [x, 5], "radius": 2}));
        s.execute("region", &json!({"handles": [circle.hex()]})).unwrap();
        put(token, s.doc().unwrap().model.iter().last().unwrap().handle);
    }
    put("M", create(&mut s, "mleader", json!({"points": [[80, 0], [80, 5]], "block": "B1"})));
    put("M2", create(&mut s, "mleader", json!({"points": [[90, 0], [90, 5]], "block": "B1"})));
    put("B", create(&mut s, "table", json!({"at": [0, 30], "rows": 2, "cols": 2})));
    s.execute("layer", &json!({"option": "new", "name": "L1"})).ok();
    let kept = create(&mut s, "line", json!({"points": [[0, -5], [5, -5]]}));
    s.execute("properties.set", &json!({"handles": [kept.hex()], "layer": "L1"})).unwrap();
    put("K", kept);
    s.execute("group", &json!({"name": "G1", "handles": [kept.hex()]})).unwrap();
    Setup { drawing: s.doc().unwrap().clone(), operands }
}

fn filled(template: &str, setup: &Setup) -> Value {
    let mut text = template.to_string();
    for (token, handle) in &setup.operands {
        text = text.replace(&format!("<{token}>"), &handle.hex());
    }
    serde_json::from_str(&text).unwrap()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Legacy,
    Canonical,
    Raw,
}

/// The drawing in `state`, and the operands that the state reached.
fn session(setup: &Setup, state: State) -> (Session, Vec<Handle>) {
    let mut session = session_of(&setup.drawing);
    let mut reached = Vec::new();
    for (_, handle) in &setup.operands {
        match state {
            State::Legacy => {}
            State::Canonical => {
                make_canonical_in(&mut session, *handle, Vec3::new(0.0, 0.6, 0.8));
                reached.push(*handle);
            }
            // A record with followers (a block reference with attributes, a table) has no single-record raw unit: it stays legacy.
            State::Raw => {
                if catch_unwind(AssertUnwindSafe(|| make_raw(&mut session, *handle))).is_ok() {
                    reached.push(*handle);
                }
            }
        }
    }
    (session, reached)
}

fn short(text: &str) -> String {
    let one_line = text.replace('\n', " ");
    if one_line.chars().count() > 96 { format!("{}...", one_line.chars().take(96).collect::<String>()) } else { one_line }
}

/// The command ids the sweep covers: the `operand` lines of `classes.txt`.
fn operand_commands() -> Vec<&'static str> {
    include_str!("consumer_matrix/classes.txt").lines().filter_map(|line| line.strip_suffix(" operand")).collect()
}

/// Run the command on the setup in `state`. `Err` is an invariant that the command broke.
fn run(command: &str, params: &Value, setup: &Setup, state: State) -> Result<String, String> {
    let (mut session, reached) = session(setup, state);
    let before = session.doc().unwrap().clone();
    let outcome = catch_unwind(AssertUnwindSafe(|| session.execute(command, params))).map_err(|_| "panicked".to_string())?;
    let after = session.doc().unwrap();
    let text = match &outcome {
        Ok(_) => "works".to_string(),
        Err(error) => {
            if after.model != before.model || after.handseed != before.handseed {
                return Err(format!("a refusal wrote to the drawing: {error}"));
            }
            format!("refused: {}", short(&error.to_string()))
        }
    };
    for handle in reached {
        let (old, new) = (before.entity(handle).unwrap(), after.entity(handle));
        let Some(new) = new else { continue };
        match state {
            State::Raw if new.kind != old.kind => return Err(format!("a raw unit was rewritten: {text}")),
            State::Canonical if new.geometry_revision != GeometryRevision::CanonicalV2 => {
                return Err(format!("a canonical object became {:?}: {text}", new.geometry_revision));
            }
            State::Canonical if std::mem::discriminant(&new.kind) != std::mem::discriminant(&old.kind) => {
                return Err(format!("a canonical object changed kind: {text}"));
            }
            _ => {}
        }
    }
    Ok(text)
}

/// The sweep, as the text of `sweep.txt`, and the invariant breaks.
fn sweep() -> (String, Vec<String>) {
    // Nothing a command does with a path may land in the working directory of the tests.
    let dir = std::env::temp_dir().join(format!("cadcraft-sweep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    let setup = setup();
    let specs = command_specs();
    let mut table = String::new();
    let mut unprobed = Vec::new();
    let mut breaks = Vec::new();
    for command in operand_commands() {
        let spec = specs.iter().find(|spec| spec.id == command).unwrap_or_else(|| panic!("{command} is not registered"));
        // A command that writes or reads a file is not run: its path parameter would reach the disk. ARRAYPATH's `path` is a handle.
        let files = command != "arraypath"
            && spec.params.split('→').next().unwrap_or_default().split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "path" || w == "file");
        let accepted = (!files)
            .then(|| TEMPLATES.iter().enumerate().find(|(_, t)| run(command, &filled(t, &setup), &setup, State::Legacy) == Ok("works".into())));
        let Some(Some((index, template))) = accepted else {
            unprobed.push(command);
            continue;
        };
        let params = filled(template, &setup);
        let mut line = format!("{command}\ttemplate={index}");
        for (name, state) in [("legacy", State::Legacy), ("canonical", State::Canonical), ("raw", State::Raw)] {
            match run(command, &params, &setup, state) {
                Ok(text) => write!(line, "\t{name}={text}").unwrap(),
                Err(why) => {
                    write!(line, "\t{name}=BROKEN {why}").unwrap();
                    if !ALLOWED.iter().any(|(c, s, _)| *c == command && *s == name) {
                        breaks.push(format!("{command} [{name}]: {why}"));
                    }
                }
            }
        }
        table.push_str(&line);
        table.push('\n');
    }
    for command in unprobed {
        writeln!(table, "{command}\tunprobed").unwrap();
    }
    (table, breaks)
}

#[test]
fn the_sweep_keeps_its_invariants_and_matches_the_recorded_outcomes() {
    let (table, breaks) = sweep();
    assert!(breaks.is_empty(), "{} invariant breaks:\n{}", breaks.len(), breaks.join("\n"));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/consumer_matrix/sweep.txt");
    if std::env::var_os("CADCRAFT_SWEEP_UPDATE").is_some() {
        std::fs::write(&path, &table).unwrap();
        return;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    let (old, new): (BTreeSet<&str>, BTreeSet<&str>) = (recorded.lines().collect(), table.lines().collect());
    let removed: Vec<_> = old.difference(&new).collect();
    let added: Vec<_> = new.difference(&old).collect();
    assert!(
        removed.is_empty() && added.is_empty(),
        "the sweep changed (CADCRAFT_SWEEP_UPDATE=1 accepts it):\nrecorded only:\n{}\nnow only:\n{}",
        removed.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n"),
        added.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n")
    );
}
