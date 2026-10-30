//! A raw unit is a save precondition, not a best-effort passthrough.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cadcraft_doc::foreign::{ForeignObject, MAX_FOREIGN_DEPTH, object_owner};
use cadcraft_doc::{Drawing, Entity, EntityKind, EntityStore, GeometryRevision, Handle, RawCompound, RawRecord, RawTag};
use cadcraft_dxf::{Tag, Value as TagValue};

use crate::dxf_raw::WrittenReferences;
use crate::{IoError, Result};

mod fields;
use fields::{Field, FieldError, Fields, Value};

const MAX_FIELDS: usize = 1_000_000;

struct Unit<'a> {
    entity: &'a Entity,
    source: &'a RawCompound,
    owner: String,
}

struct SourceRecord<'a> {
    kind: &'a str,
    handle: Handle,
    tags: &'a [RawTag],
    mapped_identity: Handle,
    parent: bool,
}

pub(crate) struct RawSaveCheck<'a> {
    drawing: &'a Drawing,
    references: &'a WrittenReferences,
    units: Vec<Unit<'a>>,
    identities: BTreeMap<Handle, usize>,
    foreign: BTreeSet<Handle>,
    owned: BTreeMap<Handle, Vec<(Handle, usize)>>,
}

fn failure(unit: &Unit<'_>, kind: &str, handle: Handle, field: impl Into<String>, reason: impl Into<String>) -> IoError {
    IoError::RawPreservation { unit: unit.entity.handle, record: kind.into(), handle, field: field.into(), reason: reason.into() }
}

fn collect<'a>(store: &'a EntityStore, owner: &str, units: &mut Vec<Unit<'a>>) {
    for entity in store.iter() {
        if let EntityKind::Raw(source) = &entity.kind {
            units.push(Unit { entity, source, owner: owner.to_ascii_uppercase() });
        }
    }
}

fn stores(drawing: &Drawing) -> impl Iterator<Item = &EntityStore> {
    std::iter::once(&drawing.model)
        .chain(drawing.layouts.iter().map(|layout| &layout.entities))
        .chain(drawing.blocks.values().map(|block| &block.entities))
}

pub(crate) fn require_ascii(drawing: &Drawing) -> Result<()> {
    for entity in stores(drawing).flat_map(EntityStore::iter) {
        if let EntityKind::Raw(raw) = &entity.kind {
            return Err(IoError::RawPreservation {
                unit: entity.handle,
                record: raw.parent().kind().into(),
                handle: entity.handle,
                field: "binary DXF".into(),
                reason: "raw numeric source text requires ASCII DXF".into(),
            });
        }
    }
    Ok(())
}

fn descendants(roots: impl Iterator<Item = Handle>, index: &BTreeMap<Handle, Vec<(Handle, usize)>>) -> std::result::Result<Vec<usize>, FieldError> {
    let mut seen = BTreeSet::new();
    let mut pending: VecDeque<_> = roots
        .map(|handle| {
            seen.insert(handle);
            (handle, 0usize)
        })
        .collect();
    let mut result = Vec::new();
    while let Some((owner, depth)) = pending.pop_front() {
        for (handle, record) in index.get(&owner).into_iter().flatten() {
            if depth >= MAX_FOREIGN_DEPTH || !seen.insert(*handle) {
                return Err(FieldError {
                    field: "metadata ownership".into(),
                    reason: "the required metadata has a cycle, duplicate identity or excessive depth".into(),
                });
            }
            result.push(*record);
            pending.push_back((*handle, depth + 1));
        }
    }
    Ok(result)
}

impl<'a> RawSaveCheck<'a> {
    pub(crate) fn new(drawing: &'a Drawing, references: &'a WrittenReferences) -> Result<Self> {
        let mut units = Vec::new();
        collect(&drawing.model, "*Model_Space", &mut units);
        for (owner, layout) in drawing.paper_space_blocks() {
            collect(&layout.entities, &owner, &mut units);
        }
        for block in drawing.blocks.values() {
            collect(&block.entities, &block.name, &mut units);
        }
        let mut identities = BTreeMap::new();
        let mut foreign = BTreeSet::new();
        let mut owned = BTreeMap::<Handle, Vec<(Handle, usize)>>::new();
        if !units.is_empty() {
            for entity in stores(drawing).flat_map(EntityStore::iter) {
                let mut add = |handle: Handle| {
                    if handle.0 != 0 {
                        *identities.entry(handle).or_default() += 1;
                    }
                };
                if let EntityKind::Raw(raw) = &entity.kind {
                    for record in raw.records() {
                        add(record.handle());
                    }
                } else {
                    add(entity.handle);
                    for handle in entity
                        .kind
                        .attributes()
                        .iter()
                        .filter_map(|attribute| attribute.handle)
                        .chain(entity.kind.polyline_records().filter_map(|record| record.handle))
                    {
                        add(handle);
                    }
                }
            }
            for object in drawing.foreign.objects.iter().chain(&drawing.foreign.table_records) {
                if object.handle.0 != 0 {
                    *identities.entry(object.handle).or_default() += 1;
                }
            }
            for (index, object) in drawing.foreign.objects.iter().enumerate() {
                foreign.insert(object.handle);
                if let Some(owner) = object_owner(object.tags.iter().map(|tag| (tag.code, tag.value.as_str()))) {
                    owned.entry(owner).or_default().push((object.handle, index));
                }
            }
            foreign.extend(drawing.foreign.targets.keys().copied());
        }
        let check = Self { drawing, references, units, identities, foreign, owned };
        for unit in &check.units {
            if unit.entity.geometry_revision != GeometryRevision::RawSource || unit.entity.handle != unit.source.parent().handle() {
                return Err(failure(
                    unit,
                    unit.source.parent().kind(),
                    unit.entity.handle,
                    "5 / geometry revision",
                    "the raw parent identity or revision changed",
                ));
            }
            unit.source
                .validate_common(&unit.entity.common)
                .map_err(|error| failure(unit, unit.source.parent().kind(), unit.entity.handle, "Common", error.to_string()))?;
            let mut fields = 0usize;
            for record in unit.source.records() {
                fields = fields.saturating_add(record.tags().len());
                if fields > MAX_FIELDS || check.identities.get(&record.handle()) != Some(&1) {
                    return Err(failure(
                        unit,
                        record.kind(),
                        record.handle(),
                        "5 / field count",
                        "the identity is not unique or the semantic field budget is exceeded",
                    ));
                }
            }
            for index in descendants(unit.source.records().map(RawRecord::handle), &check.owned)
                .map_err(|error| failure(unit, unit.source.parent().kind(), unit.entity.handle, error.field, error.reason))?
            {
                let object = &drawing.foreign.objects[index];
                fields = fields.saturating_add(object.tags.len());
                if fields > MAX_FIELDS || check.identities.get(&object.handle) != Some(&1) {
                    return Err(failure(
                        unit,
                        &object.kind,
                        object.handle,
                        "5 / field count",
                        "the metadata identity is not unique or the semantic field budget is exceeded",
                    ));
                }
            }
        }
        Ok(check)
    }

    fn mapped(&self, unit: &Unit<'_>, kind: &str, handle: Handle, field: &Field, old: Handle) -> Result<Handle> {
        if old.0 == 0 {
            return Ok(old);
        }
        if let Some(mapped) = self.references.handles.get(&old) {
            return Ok(*mapped);
        }
        if !self.foreign.contains(&old) && self.identities.get(&old) == Some(&1) {
            return Ok(old);
        }
        Err(failure(unit, kind, handle, field.label(), "the required reference has no unique written target"))
    }

    pub(crate) fn verify(&self, bytes: &[u8]) -> Result<()> {
        let Some(first) = self.units.first() else { return Ok(()) };
        let inspection = cadcraft_dwg::inspect_dwg(bytes, cadcraft_dwg::Limits::DEFAULT)
            .map_err(|error| failure(first, first.source.parent().kind(), first.entity.handle, "independent DWG decode", error))?;
        let tags = cadcraft_dxf::parse(&inspection.dxf)
            .map_err(|error| failure(first, first.source.parent().kind(), first.entity.handle, "decoded fields", error.to_string()))?;
        let decoded =
            Decoded::new(&tags).map_err(|error| failure(first, first.source.parent().kind(), first.entity.handle, error.field, error.reason))?;
        for unit in &self.units {
            let start = decoded
                .by_handle
                .get(&unit.entity.handle)
                .copied()
                .ok_or_else(|| failure(unit, unit.source.parent().kind(), unit.entity.handle, "record", "the decoded parent is missing"))?;
            for (offset, source) in unit.source.records().enumerate() {
                let actual = decoded
                    .records
                    .get(start + offset)
                    .ok_or_else(|| failure(unit, source.kind(), source.handle(), "record order", "a follower or SEQEND is missing"))?;
                if actual.kind != source.kind() || actual.handle != Some(source.handle()) {
                    return Err(failure(
                        unit,
                        source.kind(),
                        source.handle(),
                        "record order / 5",
                        "record kinds, follower order or identities differ",
                    ));
                }
                self.compare_record(
                    unit,
                    SourceRecord {
                        kind: source.kind(),
                        handle: source.handle(),
                        tags: source.tags(),
                        mapped_identity: source.handle(),
                        parent: offset == 0,
                    },
                    actual,
                    &inspection,
                    &decoded,
                )?;
                if offset == 0 {
                    self.common(unit, actual)?;
                }
            }
            let owned = descendants(unit.source.records().map(RawRecord::handle), &self.owned)
                .map_err(|error| failure(unit, unit.source.parent().kind(), unit.entity.handle, error.field, error.reason))?;
            let mut expected_owned = BTreeSet::new();
            for index in owned {
                let source = &self.drawing.foreign.objects[index];
                let mapped = self
                    .references
                    .handles
                    .get(&source.handle)
                    .copied()
                    .ok_or_else(|| failure(unit, &source.kind, source.handle, "5", "owned metadata was not written"))?;
                expected_owned.insert(mapped);
                let actual = decoded
                    .get(mapped)
                    .ok_or_else(|| failure(unit, &source.kind, source.handle, "record", "owned metadata is missing from the decode"))?;
                self.compare_object(unit, source, mapped, actual, &inspection, &decoded)?;
            }
            let actual_owned = descendants(unit.source.records().map(RawRecord::handle), &decoded.owned)
                .map_err(|error| failure(unit, unit.source.parent().kind(), unit.entity.handle, error.field, error.reason))?;
            let actual_owned: BTreeSet<_> = actual_owned.into_iter().filter_map(|index| decoded.records[index].handle).collect();
            if expected_owned != actual_owned {
                return Err(failure(
                    unit,
                    unit.source.parent().kind(),
                    unit.entity.handle,
                    "metadata ownership",
                    "the decoded metadata graph has missing or additional records",
                ));
            }
        }
        Ok(())
    }

    fn compare_object(
        &self,
        unit: &Unit<'_>,
        source: &ForeignObject,
        mapped: Handle,
        actual: &WireRecord<'_>,
        inspection: &cadcraft_dwg::DwgInspection,
        decoded: &Decoded<'_>,
    ) -> Result<()> {
        if actual.kind != source.kind {
            return Err(failure(unit, &source.kind, source.handle, "record kind", "the decoded metadata type differs"));
        }
        self.compare_record(
            unit,
            SourceRecord { kind: &source.kind, handle: source.handle, tags: &source.tags, mapped_identity: mapped, parent: false },
            actual,
            inspection,
            decoded,
        )
    }

    fn compare_record(
        &self,
        unit: &Unit<'_>,
        source: SourceRecord<'_>,
        actual: &WireRecord<'_>,
        inspection: &cadcraft_dwg::DwgInspection,
        records: &Decoded<'_>,
    ) -> Result<()> {
        let expected = fields::parse(source.tags, true).map_err(|error| failure(unit, source.kind, source.handle, error.field, error.reason))?;
        let SourceRecord { kind, handle, mapped_identity, parent, .. } = source;
        let changed = if parent { crate::dxf_write::raw_changed_properties(unit.source, unit.entity) } else { Vec::new() };
        if inspection.unexposed.contains(&mapped_identity.0) || !inspection.handles.contains(&mapped_identity.0) {
            return Err(failure(unit, kind, handle, "semantic payload / 5", "the independent decoder cannot expose the required record"));
        }
        let mut expected = expected;
        let mut raw = Vec::with_capacity(actual.tags.len());
        for tag in actual.tags {
            if let TagValue::Real(value) = tag.value
                && !value.is_finite()
            {
                return Err(failure(unit, kind, handle, tag.code.to_string(), "the decoded number is not finite"));
            }
            raw.push(RawTag { code: tag.code, value: tag.str() });
        }
        let mut decoded = fields::parse(&raw, false).map_err(|error| failure(unit, kind, handle, error.field, error.reason))?;
        records.check_names(unit, &source, &decoded, &inspection.handles)?;
        if parent {
            self.plot_style(unit, &decoded, inspection)?;
        }
        expected.retain(|field, _| !(field.common() && changed.contains(&field.code)));
        decoded.retain(|field, _| !(field.common() && changed.contains(&field.code)));
        for (field, value) in &mut expected {
            if field.code == 5 && field.class.is_empty() {
                *value = Value::Handle(mapped_identity);
            }
            if fields::reference(field.code)
                && let Value::Handle(old) = value
            {
                if parent && field.code == 330 && field.class.is_empty() && field.groups.is_empty() && field.application.is_none() && old.0 == 0 {
                    *old = self
                        .references
                        .roles
                        .get(&("BLOCK_RECORD".into(), unit.owner.clone()))
                        .copied()
                        .ok_or_else(|| failure(unit, kind, handle, field.label(), "the containing space has no written block record"))?;
                } else {
                    *old = self.mapped(unit, kind, handle, field, *old)?;
                }
                if old.0 != 0 && !inspection.handles.contains(&old.0) {
                    return Err(failure(unit, kind, handle, field.label(), "the reference target is absent from the decoded object inventory"));
                }
            }
        }
        compare_fields(unit, kind, handle, &expected, &decoded)
    }

    fn plot_style(&self, unit: &Unit<'_>, actual: &Fields, inspection: &cadcraft_dwg::DwgInspection) -> Result<()> {
        let get = |code| actual.iter().find(|(field, _)| field.common() && field.code == code).map(|(_, value)| value);
        let style = unit.entity.common.plot_style.trim();
        let (kind, target) = if style.eq_ignore_ascii_case("bylayer") {
            (None, None)
        } else if style.eq_ignore_ascii_case("byblock") {
            (Some(Value::Integer(1)), None)
        } else if style.eq_ignore_ascii_case("bydictionarydefault") {
            (Some(Value::Integer(2)), None)
        } else {
            let target = self.references.roles.get(&("NOD/ACAD_PLOTSTYLENAME".into(), style.to_ascii_uppercase())).copied();
            let Some(target) = target.filter(|handle| inspection.handles.contains(&handle.0)) else {
                return Err(failure(
                    unit,
                    unit.source.parent().kind(),
                    unit.entity.handle,
                    "plot style / 390",
                    "the named plot style has no decoded target",
                ));
            };
            (Some(Value::Integer(3)), Some(Value::Handle(target)))
        };
        if get(380) != kind.as_ref() || get(390) != target.as_ref() {
            return Err(failure(
                unit,
                unit.source.parent().kind(),
                unit.entity.handle,
                "plot style / 380 / 390",
                "the decoded plot style differs from the model",
            ));
        }
        Ok(())
    }

    fn common(&self, unit: &Unit<'_>, actual: &WireRecord<'_>) -> Result<()> {
        let source = &unit.entity.common;
        let decoded = crate::dxf_read::common_from_groups(actual.tags);
        let same = [
            ("layer", source.layer == decoded.layer),
            ("color", source.color == decoded.color),
            ("linetype", source.linetype == decoded.linetype),
            ("lineweight", source.lineweight == decoded.lineweight),
            ("linetype scale", source.ltscale.to_bits() == decoded.ltscale.to_bits()),
            ("visibility", source.visible == decoded.visible),
            ("transparency", source.transparency == decoded.transparency),
        ];
        for (field, equal) in same {
            if !equal {
                return Err(failure(
                    unit,
                    unit.source.parent().kind(),
                    unit.entity.handle,
                    field,
                    "the decoded parent property differs from the model",
                ));
            }
        }
        Ok(())
    }
}

fn compare_fields(unit: &Unit<'_>, kind: &str, handle: Handle, expected: &Fields, actual: &Fields) -> Result<()> {
    for (field, value) in expected {
        match actual.get(field) {
            Some(actual) if actual == value => {}
            None if fields::default_value(kind, field, expected).as_ref() == Some(value) => {}
            _ => return Err(failure(unit, kind, handle, field.label(), "a required value is missing or differs")),
        }
    }
    for (field, value) in actual {
        if !expected.contains_key(field) && fields::default_value(kind, field, expected).as_ref() != Some(value) {
            return Err(failure(unit, kind, handle, field.label(), "the decode adds a field without a documented wire equivalence"));
        }
    }
    Ok(())
}

struct WireRecord<'a> {
    kind: String,
    tags: &'a [Tag],
    handle: Option<Handle>,
}
struct Decoded<'a> {
    records: Vec<WireRecord<'a>>,
    by_handle: BTreeMap<Handle, usize>,
    owned: BTreeMap<Handle, Vec<(Handle, usize)>>,
    names: BTreeMap<(String, String), Handle>,
}

impl<'a> Decoded<'a> {
    fn new(tags: &'a [Tag]) -> std::result::Result<Self, FieldError> {
        let starts: Vec<_> = tags.iter().enumerate().filter(|(_, tag)| tag.code == 0).map(|(index, _)| index).collect();
        let mut records = Vec::new();
        let mut by_handle = BTreeMap::new();
        let mut owned = BTreeMap::<Handle, Vec<(Handle, usize)>>::new();
        let mut section = String::new();
        let mut names = BTreeMap::new();
        for (position, start) in starts.iter().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(tags.len());
            let Some(body) = tags.get(start + 1..end) else { continue };
            let kind = tags.get(*start).map(Tag::str).unwrap_or_default();
            if kind == "SECTION" {
                section = body.iter().find(|tag| tag.code == 2).map(Tag::str).unwrap_or_default();
            }
            if matches!(section.as_str(), "ENTITIES" | "BLOCKS" | "OBJECTS" | "TABLES") {
                let handle = body.iter().find(|tag| matches!(tag.code, 5 | 105)).and_then(|tag| Handle::parse_hex(&tag.str()));
                let index = records.len();
                if let Some(handle) = handle {
                    if by_handle.insert(handle, index).is_some() {
                        return Err(FieldError { field: "5".into(), reason: "the independent decode has duplicate record identities".into() });
                    }
                    if section == "TABLES"
                        && matches!(kind.as_str(), "LAYER" | "LTYPE" | "STYLE" | "BLOCK_RECORD" | "APPID")
                        && let Some(name) = body.iter().find(|tag| tag.code == 2).map(Tag::str)
                        && names.insert((kind.clone(), name.to_ascii_uppercase()), handle).is_some()
                    {
                        return Err(FieldError { field: "2".into(), reason: "the independent decode has duplicate symbol names".into() });
                    }
                    if section == "OBJECTS"
                        && let Some(owner) = object_owner(body.iter().map(|tag| (tag.code, tag.str())))
                    {
                        owned.entry(owner).or_default().push((handle, index));
                    }
                }
                records.push(WireRecord { kind, tags: body, handle });
            }
            if tags.get(*start).is_some_and(|tag| tag.str() == "ENDSEC") {
                section.clear();
            }
        }
        Ok(Self { records, by_handle, owned, names })
    }

    fn check_names(&self, unit: &Unit<'_>, record: &SourceRecord<'_>, fields: &Fields, inventory: &BTreeSet<u64>) -> Result<()> {
        for (field, value) in fields {
            let Value::Text(name) = value else { continue };
            let table = match (field.code, field.class.as_str()) {
                (1001, _) => "APPID",
                (8, "AcDbEntity") if field.common() => "LAYER",
                (6, "AcDbEntity") if field.common() && !["BYLAYER", "BYBLOCK"].contains(&name.to_ascii_uppercase().as_str()) => "LTYPE",
                (7, "AcDbText") => "STYLE",
                (2, "AcDbBlockReference") if record.kind == "INSERT" => "BLOCK_RECORD",
                _ => continue,
            };
            if !self.names.get(&(table.into(), name.to_ascii_uppercase())).is_some_and(|handle| inventory.contains(&handle.0)) {
                return Err(failure(unit, record.kind, record.handle, field.label(), "the named reference has no unique decoded target"));
            }
        }
        Ok(())
    }

    fn get(&self, handle: Handle) -> Option<&WireRecord<'_>> {
        self.by_handle.get(&handle).and_then(|index| self.records.get(*index))
    }
}

#[cfg(test)]
#[path = "raw_preservation_tests.rs"]
mod tests;
