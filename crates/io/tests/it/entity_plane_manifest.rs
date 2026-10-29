//! Authored inputs for native AutoCAD plane measurements. Reference artifacts stay outside git.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, RawTag, Unknown};
use cadcraft_geom::Vec3;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    fixtures: Vec<Fixture>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    id: String,
    ucs: Frame,
    variables: Variables,
    records: Vec<Record>,
    setup: Vec<Operation>,
    operation: Operation,
    artifacts: Artifacts,
    reference: Reference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    view: [f64; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variables {
    mirrtext: u8,
    dimassoc: u8,
    osmode: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    name: Option<String>,
    kind: RecordKind,
    /// DXF group values, with coordinate triplets kept together. INSERT group 50 is degrees.
    groups: Vec<Group>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RecordKind {
    Block,
    Endblk,
    Ellipse,
    Line,
    Point,
    Circle,
    Insert,
    Multileader,
}

impl RecordKind {
    fn name(self) -> &'static str {
        match self {
            Self::Block => "BLOCK",
            Self::Endblk => "ENDBLK",
            Self::Ellipse => "ELLIPSE",
            Self::Line => "LINE",
            Self::Point => "POINT",
            Self::Circle => "CIRCLE",
            Self::Insert => "INSERT",
            Self::Multileader => "MULTILEADER",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    code: i32,
    value: GroupValue,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GroupValue {
    Integer(i64),
    Number(f64),
    Text(String),
    Point([f64; 3]),
    Reference { reference: ReferenceRole },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReferenceRole {
    TextStyle,
    MleaderStyle,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Inspect { points: Vec<[f64; 3]> },
    Move { target: String, delta: [f64; 3] },
    Copy { target: String, delta: [f64; 3] },
    Rotate { target: String, base: [f64; 3], degrees: f64 },
    Scale { target: String, base: [f64; 3], factor: f64 },
    Explode { target: String },
    AlignedDimension { name: String, first: [f64; 3], second: [f64; 3], line_point: [f64; 3] },
    Grip { target: String, index: u32, to: [f64; 3] },
    MleaderVertex { target: String, root: u32, branch: u32, vertex: u32, to: [f64; 3] },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifacts {
    directory: String,
    source: String,
    script: String,
    before: String,
    after: String,
    log: String,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum Reference {
    Unmeasured { reason: String },
    Measured { product: String, method: Method, assertions: Vec<Assertion> },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Method {
    NativeConsole,
    NativeUi,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Assertion {
    Point { target: String, group: i32, value: [f64; 3] },
    Number { target: String, group: i32, value: f64 },
    EntityType { target: String, value: String },
    RootCount { target: String, value: u32 },
    UnchangedRoot { target: String, root: u32 },
    Refusal { message: String },
    WorldPoint { index: u32, value: [f64; 3] },
    Erased { target: String },
    ArrayDimensions { block: String, cols: u32, rows: u32 },
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition { Ok(()) } else { Err(message.into()) }
}

fn identifier(value: &str) -> Result<(), String> {
    require(!value.is_empty() && value.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'), "invalid fixture entity name")
}

fn finite(point: [f64; 3]) -> Result<(), String> {
    require(point.into_iter().all(f64::is_finite), "non-finite fixture point")
}

fn point([x, y, z]: [f64; 3]) -> String {
    format!("'({x:?} {y:?} {z:?})")
}

fn world_point(value: [f64; 3]) -> String {
    format!("(trans {} 0 1)", point(value))
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let [ax, ay, az] = a;
    let [bx, by, bz] = b;
    [ax + bx, ay + by, az + bz]
}

impl Group {
    fn raw(&self) -> Result<Vec<RawTag>, String> {
        let code = self.code;
        require((1..=1071).contains(&code), "invalid fixture group code")?;
        let value = match &self.value {
            GroupValue::Integer(value) => value.to_string(),
            GroupValue::Number(value) => {
                require(value.is_finite(), "non-finite fixture number")?;
                format!("{value:?}")
            }
            GroupValue::Text(value) => {
                require(!value.chars().any(char::is_control), "control character in fixture text")?;
                value.clone()
            }
            GroupValue::Point(value) => {
                finite(*value)?;
                return Ok([0, 10, 20]
                    .into_iter()
                    .zip(*value)
                    .map(|(offset, value)| RawTag { code: code + offset, value: format!("{value:?}") })
                    .collect());
            }
            GroupValue::Reference { .. } => {
                require(cadcraft_doc::foreign::is_pointer(code), "fixture role needs a handle group")?;
                "0".into()
            }
        };
        Ok(vec![RawTag { code, value }])
    }
}

impl Record {
    fn text(&self, code: i32) -> Option<&str> {
        self.groups.iter().find_map(|group| match (&group.value, group.code) {
            (GroupValue::Text(value), found) if found == code => Some(value.as_str()),
            _ => None,
        })
    }

    fn position(&self, code: i32) -> Option<Vec3> {
        self.groups.iter().find_map(|group| match (&group.value, group.code) {
            (GroupValue::Point([x, y, z]), found) if found == code => Some(Vec3::new(*x, *y, *z)),
            _ => None,
        })
    }

    fn entity(&self, handle: Handle) -> Result<Entity, String> {
        let class = match self.kind {
            RecordKind::Ellipse => "AcDbEllipse",
            RecordKind::Line => "AcDbLine",
            RecordKind::Point => "AcDbPoint",
            RecordKind::Circle => "AcDbCircle",
            RecordKind::Insert => {
                let multiple = self.groups.iter().filter(|group| matches!(group.code, 70 | 71)).any(|group| match &group.value {
                    GroupValue::Integer(value) => *value > 1,
                    GroupValue::Number(value) => *value > 1.0,
                    _ => false,
                });
                if multiple { "AcDbMInsertBlock" } else { "AcDbBlockReference" }
            }
            RecordKind::Multileader => "AcDbMLeader",
            RecordKind::Block | RecordKind::Endblk => return Err("structural record is not an entity".into()),
        };
        let layer = self.text(8).unwrap_or("0").to_string();
        let mut tags = vec![
            RawTag { code: 100, value: "AcDbEntity".into() },
            RawTag { code: 8, value: layer.clone() },
            RawTag { code: 100, value: class.into() },
        ];
        for group in self.groups.iter().filter(|group| !matches!(group.code, 8 | 100)) {
            tags.extend(group.raw()?);
        }
        // Unknown is a raw transport here, not a CADCraft interpretation of the fixture geometry.
        Ok(Entity {
            geometry_revision: Default::default(),
            handle,
            common: Common { layer, ..Default::default() },
            kind: EntityKind::Unknown(Unknown { dxf_type: self.kind.name().into(), tags }),
        })
    }
}

impl Operation {
    fn target(&self) -> Option<&str> {
        match self {
            Self::Move { target, .. }
            | Self::Copy { target, .. }
            | Self::Rotate { target, .. }
            | Self::Scale { target, .. }
            | Self::Explode { target }
            | Self::Grip { target, .. }
            | Self::MleaderVertex { target, .. } => Some(target),
            Self::Inspect { .. } | Self::AlignedDimension { .. } => None,
        }
    }

    fn native(&self) -> Result<Option<String>, String> {
        let code = match self {
            Self::Inspect { points } => {
                let mut code = "(command \"_.ZOOM\" \"_Extents\")\n".to_string();
                for (index, value) in points.iter().enumerate() {
                    finite(*value)?;
                    code.push_str(&format!("(setq cf_snap (osnap {} \"_node\"))\n(progn (princ {:?}) (if cf_snap (foreach cf_value (trans cf_snap 1 0) (princ (strcat (rtos cf_value 2 16) \" \"))) (princ \"missing\")) (princ))\n", world_point(*value), format!("\nREFERENCE_WORLD_POINT {index} ")));
                }
                code
            }
            Self::Move { target, delta } | Self::Copy { target, delta } => {
                identifier(target)?;
                finite(*delta)?;
                let command = if matches!(self, Self::Move { .. }) { "MOVE" } else { "COPY" };
                let edit = format!("(command \"_.{command}\" cf_{target} \"\" \"_non\" {} \"_non\" {})", world_point([0.0; 3]), world_point(*delta));
                if matches!(self, Self::Copy { .. }) { format!("{edit}\n(setq cf_{target} (entlast))") } else { edit }
            }
            Self::Rotate { target, base, degrees } => {
                identifier(target)?;
                finite(*base)?;
                require(degrees.is_finite(), "non-finite fixture rotation")?;
                format!("(command \"_.ROTATE\" cf_{target} \"\" \"_non\" {} {degrees:?})", world_point(*base))
            }
            Self::Scale { target, base, factor } => {
                identifier(target)?;
                finite(*base)?;
                require(factor.is_finite() && *factor > 0.0, "invalid fixture scale")?;
                format!("(command \"_.SCALE\" cf_{target} \"\" \"_non\" {} {factor:?})", world_point(*base))
            }
            Self::Explode { target } => {
                identifier(target)?;
                format!("(command \"_.EXPLODE\" cf_{target})")
            }
            Self::AlignedDimension { name, first, second, line_point } => {
                identifier(name)?;
                for value in [first, second, line_point] {
                    finite(*value)?;
                }
                format!(
                    "(command \"_.DIMALIGNED\" \"_end\" {} \"_node\" {} \"_non\" {})\n(setq cf_{name} (entlast))",
                    world_point(*first),
                    world_point(*second),
                    world_point(*line_point)
                )
            }
            Self::Grip { target, index, to } => {
                identifier(target)?;
                finite(*to)?;
                require(*index < 1000, "invalid fixture grip index")?;
                return Ok(None);
            }
            Self::MleaderVertex { target, root, branch, vertex, to } => {
                identifier(target)?;
                finite(*to)?;
                require(*root < 1000 && *branch < 1000 && *vertex < 1000, "invalid fixture leader identity")?;
                return Ok(None);
            }
        };
        Ok(Some(code))
    }
}

impl ReferenceRole {
    fn handle(self, tags: &[cadcraft_dxf::Tag]) -> Result<String, String> {
        let mut starts: Vec<_> = tags.iter().enumerate().filter(|(_, tag)| tag.code == 0).map(|(index, _)| index).collect();
        starts.push(tags.len());
        let mut handles = Vec::new();
        for pair in starts.windows(2) {
            let [start, end] = pair else { continue };
            let record = tags.get(*start..*end).ok_or("invalid fixture record span")?;
            let kind = record.first().map(cadcraft_dxf::Tag::str).unwrap_or_default();
            let matches = match self {
                Self::TextStyle => kind == "STYLE" && record.iter().any(|tag| tag.code == 2 && tag.str().eq_ignore_ascii_case("Standard")),
                Self::MleaderStyle => kind == "MLEADERSTYLE",
            };
            if matches {
                let handle = record.iter().find(|tag| tag.code == 5).ok_or("fixture style has no handle")?.str();
                handles.push(handle);
            }
        }
        require(handles.len() == 1, "fixture style role is missing or ambiguous")?;
        handles.into_iter().next().ok_or_else(|| "fixture style role has no handle".into())
    }
}

impl Assertion {
    fn check(&self, names: &BTreeSet<String>) -> Result<(), String> {
        let (target, valid) = match self {
            Self::Point { target, group, value } => {
                finite(*value)?;
                (Some(target), (1..=1071).contains(group))
            }
            Self::Number { target, group, value } => (Some(target), (1..=1071).contains(group) && value.is_finite()),
            Self::EntityType { target, value } => (Some(target), !value.is_empty() && value.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')),
            Self::RootCount { target, value } => (Some(target), *value > 0 && *value < 1000),
            Self::UnchangedRoot { target, root } => (Some(target), *root < 1000),
            Self::Refusal { message } => (None, !message.is_empty()),
            Self::WorldPoint { index, value } => {
                finite(*value)?;
                (None, *index < 1000)
            }
            Self::Erased { target } => (Some(target), true),
            Self::ArrayDimensions { block, cols, rows } => {
                identifier(block)?;
                (None, *cols > 0 && *cols <= 1000 && *rows > 0 && *rows <= 1000 && (*cols > 1 || *rows > 1))
            }
        };
        require(valid, "invalid frozen assertion")?;
        require(target.is_none_or(|name| names.contains(name)), "unknown assertion target")
    }
}

impl Fixture {
    fn names(&self) -> Result<BTreeSet<String>, String> {
        let mut names = BTreeSet::new();
        for name in self.records.iter().filter_map(|record| record.name.as_ref()).chain(self.setup.iter().filter_map(|operation| match operation {
            Operation::AlignedDimension { name, .. } => Some(name),
            _ => None,
        })) {
            identifier(name)?;
            require(names.insert(name.clone()), "duplicate fixture entity name")?;
        }
        Ok(names)
    }

    fn validate(&self) -> Result<(), String> {
        require(!self.id.is_empty() && self.id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'), "invalid fixture id")?;
        for value in [self.ucs.origin, self.ucs.x, self.ucs.y, self.ucs.view] {
            finite(value)?;
        }
        require(self.ucs.x != [0.0; 3] && self.ucs.y != [0.0; 3] && self.ucs.view != [0.0; 3], "zero fixture axis")?;
        require(self.variables.mirrtext <= 1 && self.variables.dimassoc <= 2 && self.variables.osmode <= 32767, "invalid fixture variables")?;
        let names = self.names()?;
        for operation in self.setup.iter().chain(std::iter::once(&self.operation)) {
            if let Some(target) = operation.target() {
                require(names.contains(target), "unknown operation target")?;
            }
            operation.native()?;
        }
        let prefix = format!("normals-p2-{}", self.id);
        require(self.artifacts.directory == format!("/tmp/cad/normals-phase2/{}", self.id), "invalid artifact directory")?;
        for (name, ending) in [
            (&self.artifacts.source, ".dxf"),
            (&self.artifacts.script, ".scr"),
            (&self.artifacts.before, "-before.dxf"),
            (&self.artifacts.after, "-after.dxf"),
            (&self.artifacts.log, ".dxf.reference.log"),
        ] {
            require(*name == format!("{prefix}{ending}"), "invalid artifact name")?;
        }
        for record in &self.records {
            for group in &record.groups {
                group.raw()?;
            }
        }
        match &self.reference {
            Reference::Unmeasured { reason } => require(!reason.is_empty(), "missing unmeasured reason")?,
            Reference::Measured { product, method, assertions } => {
                require(!product.is_empty() && !assertions.is_empty(), "empty frozen reference")?;
                require(!matches!(method, Method::NativeConsole) || self.operation.native()?.is_some(), "UI operation cannot use a console oracle")?;
                for assertion in assertions {
                    assertion.check(&names)?;
                }
            }
        }
        Ok(())
    }

    fn source_drawing(&self) -> Result<(Drawing, BTreeMap<String, Handle>), String> {
        let mut drawing = Drawing::new_metric();
        let mut current: Option<Block> = None;
        let mut names = BTreeMap::new();
        for record in &self.records {
            match record.kind {
                RecordKind::Block => {
                    require(current.is_none(), "nested fixture block definitions")?;
                    let name = record.text(2).ok_or("missing fixture block name")?;
                    require(drawing.block(name).is_none(), "duplicate fixture block")?;
                    let mut block = Block::new(name);
                    block.base = record.position(10).ok_or("missing fixture block base")?;
                    current = Some(block);
                }
                RecordKind::Endblk => {
                    let block = current.take().ok_or("unmatched fixture ENDBLK")?;
                    drawing.blocks.insert(block.name.clone(), Arc::new(block));
                }
                _ => {
                    let handle = drawing.new_handle();
                    let entity = record.entity(handle)?;
                    if let Some(name) = &record.name {
                        names.insert(name.clone(), handle);
                    }
                    match &mut current {
                        Some(block) => {
                            block.entities.push(entity);
                        }
                        None => {
                            drawing.model.push(entity);
                        }
                    }
                }
            }
        }
        require(current.is_none(), "unclosed fixture block")?;
        Ok((drawing, names))
    }

    fn source_bytes(&self) -> Result<Vec<u8>, String> {
        let (drawing, names) = self.source_drawing()?;
        let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
        let bytes = cadcraft_io::write_with(&drawing, "reference.dxf", &options).map_err(|error| error.to_string())?;
        let mut tags = cadcraft_dxf::parse(&bytes).map_err(|error| error.to_string())?;
        for record in &self.records {
            if !record.groups.iter().any(|group| matches!(group.value, GroupValue::Reference { .. })) {
                continue;
            }
            let name = record.name.as_ref().ok_or("reference-bearing fixture record needs a name")?;
            let handle = names.get(name).ok_or("missing fixture reference owner")?.hex();
            let at =
                tags.iter().position(|tag| tag.code == 5 && tag.str().eq_ignore_ascii_case(&handle)).ok_or("missing fixture reference record")?;
            let end = tags.iter().enumerate().skip(at + 1).find(|(_, tag)| tag.code == 0).map_or(tags.len(), |(index, _)| index);
            let mut occurrences: BTreeMap<i32, usize> = BTreeMap::new();
            for group in &record.groups {
                let occurrence = occurrences.entry(group.code).or_default();
                if let GroupValue::Reference { reference } = group.value {
                    let value = reference.handle(&tags)?;
                    let index = tags
                        .iter()
                        .enumerate()
                        .take(end)
                        .skip(at)
                        .filter(|(_, tag)| tag.code == group.code)
                        .nth(*occurrence)
                        .map(|(index, _)| index)
                        .ok_or("missing fixture reference slot")?;
                    if let Some(tag) = tags.get_mut(index) {
                        *tag = cadcraft_dxf::Tag::s(group.code, value);
                    }
                }
                *occurrence += 1;
            }
        }
        Ok(cadcraft_dxf::write_ascii(&tags).into_bytes())
    }

    fn native_script(&self, remote: &str) -> Result<Option<String>, String> {
        self.validate()?;
        let Some(edit) = self.operation.native()? else { return Ok(None) };
        require(!remote.chars().any(|c| c.is_control() || c == '"' || c == '\\'), "invalid remote directory")?;
        let mut script = format!(
            "(setvar \"CMDECHO\" 1)\n(setvar \"TILEMODE\" 1)\n(setvar \"MIRRTEXT\" {})\n(setvar \"DIMASSOC\" {})\n(setvar \"OSMODE\" {})\n(command \"_.UCS\" \"_World\")\n",
            self.variables.mirrtext, self.variables.dimassoc, self.variables.osmode
        );
        let (_, handles) = self.source_drawing()?;
        for (name, handle) in handles {
            script.push_str(&format!("(setq cf_{name} (handent {:?}))\n", handle.hex()));
        }
        script.push_str(&format!("(command \"_.VPOINT\" {})\n", point(self.ucs.view)));
        script.push_str(&format!(
            "(command \"_.UCS\" \"_3point\" \"_non\" {} \"_non\" {} \"_non\" {})\n",
            point(self.ucs.origin),
            point(add(self.ucs.origin, self.ucs.x)),
            point(add(self.ucs.origin, self.ucs.y))
        ));
        for operation in &self.setup {
            let command = operation.native()?.ok_or_else(|| "setup requires the AutoCAD UI".to_string())?;
            script.push_str(&command);
            script.push('\n');
        }
        script.push_str("(princ \"\\nREFERENCE_VARIABLES \")\n");
        for variable in ["UCSORG", "UCSXDIR", "UCSYDIR", "VIEWDIR", "MIRRTEXT", "DIMASSOC", "OSMODE", "OSNAPZ", "APERTURE"] {
            script.push_str(&format!("(prin1 (list {:?} (getvar {:?})))\n", variable, variable));
        }
        for stage in ["BEFORE", "AFTER"] {
            if stage == "AFTER" {
                script.push_str(edit.trim_end());
                script.push('\n');
            }
            for name in self.names()? {
                script.push_str(&format!("(princ {:?})\n(prin1 (entget cf_{name}))\n", format!("\nREFERENCE_{stage} {name} ")));
            }
            let file = if stage == "BEFORE" { &self.artifacts.before } else { &self.artifacts.after };
            script.push_str(&format!("(command \"_.DXFOUT\" {:?} \"_V\" \"2018\" \"16\")\n", format!("{remote}/{file}")));
        }
        script.push_str(&format!("_.AUDIT\n_N\n(princ {:?})\n_.QUIT\n_Y\n", format!("\nREFERENCE_COMPLETE {}", self.id)));
        Ok(Some(script))
    }
}

fn manifest() -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(include_str!("../data/entity_planes.json")).map_err(|error| error.to_string())?;
    require(manifest.schema == 1, "unknown fixture schema")?;
    let mut ids = BTreeSet::new();
    for fixture in &manifest.fixtures {
        require(ids.insert(&fixture.id), "duplicate fixture id")?;
        fixture.validate()?;
    }
    Ok(manifest)
}

pub(crate) struct ExportedCases {
    pub(crate) scripts: Vec<PathBuf>,
    pub(crate) requires_ui: Vec<String>,
}

/// Export authored inputs, not oracle results. Only native AutoCAD runs can freeze assertions.
pub(crate) fn export_all(output: &Path, remote: &str) -> Result<ExportedCases, String> {
    let mut exported = ExportedCases { scripts: Vec::new(), requires_ui: Vec::new() };
    for fixture in manifest()?.fixtures {
        fixture.validate()?;
        let directory = output.join(&fixture.id);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let source = fixture.source_bytes()?;
        std::fs::write(directory.join(&fixture.artifacts.source), source).map_err(|error| error.to_string())?;
        let Some(script) = fixture.native_script(remote)? else {
            exported.requires_ui.push(fixture.id);
            continue;
        };
        let path = directory.join(&fixture.artifacts.script);
        std::fs::write(&path, script).map_err(|error| error.to_string())?;
        exported.scripts.push(path);
    }
    Ok(exported)
}

#[test]
fn plane_reference_manifest_covers_the_required_literal_cases() {
    let manifest = manifest().unwrap();
    for id in [
        "plane-move-edge-001",
        "plane-grip-edge-001",
        "insert-nested-shear-001",
        "minsert-signed-001",
        "dimension-assoc-tilted-001",
        "mleader-two-roots-001",
    ] {
        assert!(manifest.fixtures.iter().any(|fixture| fixture.id == id), "missing fixture {id}");
    }
}

#[test]
fn console_oracles_contain_the_measured_native_assertions() {
    let manifest = manifest().unwrap();
    for fixture in &manifest.fixtures {
        if fixture.operation.native().unwrap().is_some() {
            let Reference::Measured { method: Method::NativeConsole, assertions, .. } = &fixture.reference else {
                panic!("native assertions are not frozen for {}", fixture.id);
            };
            assert!(!assertions.is_empty());
        }
    }
}

#[test]
fn plane_reference_manifest_has_distinct_roots_and_doglegs() {
    let manifest = manifest().unwrap();
    let fixture = manifest.fixtures.iter().find(|fixture| fixture.id == "mleader-two-roots-001").unwrap();
    let record = fixture.records.iter().find(|record| record.kind == RecordKind::Multileader).unwrap();
    assert_eq!(record.groups.iter().filter(|group| group.code == 302).count(), 2);
    let doglegs: Vec<_> = record
        .groups
        .iter()
        .filter_map(|group| match (&group.value, group.code) {
            (GroupValue::Number(value), 40) => Some(*value),
            _ => None,
        })
        .collect();
    assert!(doglegs.contains(&2.0) && doglegs.contains(&3.0));
}

#[test]
fn unmeasured_ui_operations_never_become_console_measurements() {
    let manifest = manifest().unwrap();
    for fixture in &manifest.fixtures {
        if matches!(fixture.operation, Operation::Grip { .. } | Operation::MleaderVertex { .. }) {
            assert!(fixture.native_script("C:/reference").unwrap().is_none());
            assert!(matches!(fixture.reference, Reference::Unmeasured { .. }));
        } else {
            let script = fixture.native_script("C:/reference").unwrap().unwrap();
            assert!(script.contains(&format!("REFERENCE_COMPLETE {}", fixture.id)));
            assert!(!script.contains("entmod"));
            assert!(!script.contains("entmake"));
            assert!(script.lines().all(|line| !line.trim().is_empty()), "blank lines repeat native commands: {}", fixture.id);
        }
    }
}

#[test]
fn fixture_export_keeps_oracles_separate_from_authored_inputs() {
    let path = std::env::temp_dir().join(format!("cadcraft-plane-manifest-{}", std::process::id()));
    let exported = export_all(&path, "C:/reference").unwrap();
    assert_eq!(exported.scripts.len(), 10);
    assert_eq!(exported.requires_ui, ["plane-grip-edge-001", "mleader-two-roots-001"]);
    for file in exported.scripts {
        assert!(file.is_file());
        let source = std::fs::read(file.with_extension("dxf")).unwrap();
        let drawing = cadcraft_io::read_dxf(&source).unwrap();
        assert_eq!(drawing.header.str("ACADVER", ""), "AC1032");
        if file.file_name().unwrap().to_string_lossy().contains("minsert-signed-001") {
            assert!(std::str::from_utf8(&source).unwrap().contains("AcDbMInsertBlock"));
        }
        if file.file_name().unwrap().to_string_lossy().contains("plane-move-edge-001") {
            let entity = drawing.model.iter().next().expect("the source carries literal geometry, not entmake instructions");
            let cadcraft_doc::EntityKind::Ellipse(ellipse) = &entity.kind else { panic!("ellipse source") };
            assert_eq!(ellipse.normal, cadcraft_geom::Vec3::new(0.0, 1.0, 0.0));
        }
    }
    let mleader_source = std::fs::read(path.join("mleader-two-roots-001/normals-p2-mleader-two-roots-001.dxf")).unwrap();
    let tags = cadcraft_dxf::parse(&mleader_source).unwrap();
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|section| section.name == "ENTITIES").unwrap();
    let (_, entity) = cadcraft_dxf::records(&section.tags).into_iter().find(|(kind, _)| kind == "MULTILEADER").unwrap();
    let style = entity.iter().rfind(|tag| tag.code == 340).expect("the source has an explicit MLEADER style");
    assert!(Handle::parse_hex(&style.str()).is_some_and(|handle| handle.0 > 0));
    std::fs::remove_dir_all(path).unwrap();
}
