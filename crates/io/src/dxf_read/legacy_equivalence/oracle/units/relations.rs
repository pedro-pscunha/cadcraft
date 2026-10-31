//! References resolve to source identities, not allocator-specific output handles.
use super::super::identity::{MetadataIdentity, Placement, Position, Record, RecordIndex};
use super::{Unit, UnitIndex};
use crate::activation::records::Container;
use crate::raw_preservation::fields::{self, Field, Value};
use cadcraft_doc::RawTag;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Null,
    Record(Position),
    Metadata(MetadataIdentity),
}

pub(in crate::dxf_read::legacy_equivalence::oracle) struct Relations {
    reverse: BTreeMap<Position, Position>,
}

impl Relations {
    pub fn new(source: &UnitIndex<'_>, target: &UnitIndex<'_>) -> Self {
        let mut reverse = BTreeMap::new();
        for unit in &source.units {
            let Some(written) = target.corresponding(source, unit) else { continue };
            if !target.same_shape(source, unit, written) {
                continue;
            }
            for (left, right) in unit.records.iter().zip(&written.records) {
                let previous = reverse.insert(right.position.clone(), left.position.clone());
                assert!(previous.is_none(), "two source records claimed one output occurrence");
            }
        }
        Self { reverse }
    }

    pub fn conserved(&self, source: &UnitIndex<'_>, target: &UnitIndex<'_>, unit: &Unit<'_>, written: &Unit<'_>) -> bool {
        if !target.same_shape(source, unit, written) {
            return false;
        }
        if !matches!(unit.parent().kind.as_str(), "INSERT" | "POLYLINE") {
            return true;
        }
        unit.records.iter().zip(&written.records).enumerate().all(|(slot, (left, right))| {
            let implicit_owner = if slot == 0 { container_owner(left) } else { Some(Target::Record(unit.parent().position.clone())) };
            let original = references(source.index, left, implicit_owner.as_ref(), None);
            let actual = references(target.index, right, implicit_owner.as_ref(), Some(&self.reverse));
            if slot != 0 && original.as_ref().and_then(|fields| fields.get(&owner_field())) != implicit_owner.as_ref() {
                return false;
            }
            original.is_some() && original == actual && names(source.index, left).is_some_and(|names| Some(names) == self::names(target.index, right))
        })
    }
}

fn container_owner(record: &Record) -> Option<Target> {
    let name = match &record.position.placement {
        Placement::Container(Container::Block(name)) => name.clone(),
        Placement::Container(Container::Entities) if !record.groups.iter().any(|tag| tag.code == 67 && tag.i64() != 0) => "*MODEL_SPACE".into(),
        _ => return None,
    };
    Some(Target::Metadata(MetadataIdentity::Named { kind: "BLOCK_RECORD".into(), name }))
}

fn names(index: &RecordIndex, record: &Record) -> Option<BTreeMap<i32, String>> {
    let mut result = BTreeMap::new();
    let mut symbols = vec![(8, "LAYER", Some("0")), (6, "LTYPE", Some("BYLAYER"))];
    if record.kind == "INSERT" {
        symbols.push((2, "BLOCK_RECORD", None));
    }
    if record.kind == "ATTRIB" {
        symbols.push((7, "STYLE", Some("STANDARD")));
    }
    for (code, kind, default) in symbols {
        let values: Vec<_> = record.groups.iter().filter(|tag| tag.code == code).collect();
        if values.len() > 1 {
            return None;
        }
        let name = match values.first() {
            Some(tag) => cadcraft_dxf::unescape_unicode(&tag.str()).trim().to_ascii_uppercase(),
            None => default?.to_string(),
        };
        if !index.named_target(kind, &name) {
            return None;
        }
        result.insert(code, name);
    }
    Some(result)
}

fn owner_field() -> Field {
    Field { class: String::new(), groups: Vec::new(), application: None, embedded: 0, code: 330, index: 0 }
}

fn references(
    index: &RecordIndex,
    record: &Record,
    implicit_owner: Option<&Target>,
    reverse: Option<&BTreeMap<Position, Position>>,
) -> Option<BTreeMap<Field, Target>> {
    let tags: Vec<_> = record.groups.iter().map(|tag| RawTag { code: tag.code, value: tag.str() }).collect();
    let parsed = fields::parse(&tags, true).ok()?;
    let mut result = BTreeMap::new();
    if let Some(owner) = implicit_owner {
        result.insert(owner_field(), owner.clone());
    }
    for (field, value) in parsed {
        let Value::Handle(handle) = value else { continue };
        if field.code == 5 {
            continue;
        }
        let target = if handle.0 == 0 {
            Target::Null
        } else if index.handle_count(handle) != 0 {
            let record = index.unique_handle(handle)?;
            let position = match reverse {
                Some(reverse) => reverse.get(&record.position)?.clone(),
                None => record.position.clone(),
            };
            Target::Record(position)
        } else {
            Target::Metadata(index.metadata(handle)?.clone())
        };
        result.insert(field, target);
    }
    result.contains_key(&owner_field()).then_some(result)
}
