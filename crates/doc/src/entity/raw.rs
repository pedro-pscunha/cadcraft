//! Opaque source records, kept without a geometry interpretation.

use serde::{Deserialize, Serialize};

use super::{Common, Handle, RawTag};
use crate::{DocError, Result};
use std::sync::Arc;

const MAX_RAW_TAGS: usize = 10_000_000;

/// One complete source record, excluding its leading group 0.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawRecordData")]
pub struct RawRecord {
    kind: String,
    tags: Vec<RawTag>,
    #[serde(skip)]
    handle: Handle,
    #[serde(skip)]
    owner: Option<Handle>,
}

#[derive(Deserialize)]
struct RawRecordData {
    kind: String,
    tags: Vec<RawTag>,
}

impl TryFrom<RawRecordData> for RawRecord {
    type Error = DocError;
    fn try_from(value: RawRecordData) -> Result<Self> {
        Self::new(value.kind, value.tags)
    }
}

impl RawRecord {
    pub fn new(kind: String, tags: Vec<RawTag>) -> Result<Self> {
        if kind.is_empty() || kind.len() > 255 || kind.trim() != kind || kind.contains(['\r', '\n']) {
            return Err(DocError::Invalid("A raw record needs one non-empty DXF type name.".into()));
        }
        if tags.len() > MAX_RAW_TAGS {
            return Err(DocError::Invalid("The raw record exceeds the source tag limit.".into()));
        }
        let mut handle = None;
        let mut owner = None;
        let mut depth = 0usize;
        for tag in &tags {
            if !(1..=1071).contains(&tag.code) || tag.value.contains(['\r', '\n']) {
                return Err(DocError::Invalid("A raw record group cannot contain a record boundary.".into()));
            }
            if tag.code == 102 {
                if tag.value.trim().starts_with('{') {
                    if depth == crate::foreign::MAX_FOREIGN_DEPTH {
                        return Err(DocError::Invalid("Raw application groups nest too deeply.".into()));
                    }
                    depth += 1;
                } else if tag.value.trim() == "}" {
                    depth =
                        depth.checked_sub(1).ok_or_else(|| DocError::Invalid("A raw application group closes without an opening group.".into()))?;
                }
            } else if depth == 0 && tag.code == 5 {
                let value = Handle::parse_hex(&tag.value)
                    .filter(|h| h.0 != 0)
                    .ok_or_else(|| DocError::Invalid("A raw record needs a nonzero hexadecimal identity.".into()))?;
                if handle.replace(value).is_some() {
                    return Err(DocError::Invalid("A raw record has more than one identity.".into()));
                }
            } else if depth == 0 && tag.code == 330 && owner.is_none() {
                owner =
                    Some(Handle::parse_hex(&tag.value).ok_or_else(|| DocError::Invalid("A raw record owner is not a hexadecimal handle.".into()))?);
            }
        }
        if depth != 0 {
            return Err(DocError::Invalid("A raw application group is not closed.".into()));
        }
        let handle = handle.ok_or_else(|| DocError::Invalid("A raw record has no identity.".into()))?;
        Ok(Self { kind, tags, handle, owner })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn tags(&self) -> &[RawTag] {
        &self.tags
    }
    pub fn handle(&self) -> Handle {
        self.handle
    }
    pub fn owner(&self) -> Option<Handle> {
        self.owner
    }
}

/// One opaque source unit, with its parent and ordered followers owned together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCompoundData")]
pub struct RawCompound {
    parent: Arc<RawRecord>,
    followers: Arc<[RawRecord]>,
    source_common: Common,
    reason: String,
}

#[derive(Deserialize)]
struct RawCompoundData {
    parent: RawRecord,
    followers: Vec<RawRecord>,
    source_common: Common,
    reason: String,
}

impl TryFrom<RawCompoundData> for RawCompound {
    type Error = DocError;
    fn try_from(value: RawCompoundData) -> Result<Self> {
        Self::new(value.parent, value.followers, value.source_common, value.reason)
    }
}

impl RawCompound {
    pub fn new(parent: RawRecord, followers: Vec<RawRecord>, source_common: Common, reason: String) -> Result<Self> {
        if followers.len() >= 1_000_000 || reason.trim().is_empty() {
            return Err(DocError::Invalid("A raw unit needs a reason and a bounded record sequence.".into()));
        }
        if source_common.geometry.is_some() || !crate::has_finite_numbers(&source_common) {
            return Err(DocError::Invalid("Raw source properties cannot own interpreted or non-finite geometry.".into()));
        }
        let kind = parent.kind.to_ascii_uppercase();
        let follower_kind = match kind.as_str() {
            "INSERT" => Some("ATTRIB"),
            "POLYLINE" => Some("VERTEX"),
            "SECTION" | "ENDSEC" | "EOF" | "TABLE" | "ENDTAB" | "BLOCK" | "ENDBLK" | "ATTRIB" | "VERTEX" | "SEQEND" | "CLASS" => {
                return Err(DocError::Invalid("A raw unit cannot start with a structural or follower record.".into()));
            }
            _ => None,
        };
        if followers.is_empty() {
            let has_attributes = parent.tags.iter().any(|t| t.code == 66 && t.value.trim().parse::<i64>().is_ok_and(|v| v != 0));
            if kind == "POLYLINE" || (kind == "INSERT" && has_attributes) {
                return Err(DocError::Invalid("The raw compound has no closing SEQEND record.".into()));
            }
        } else {
            let expected = follower_kind.ok_or_else(|| DocError::Invalid("This raw parent cannot own follower records.".into()))?;
            let mut records = followers.iter();
            if !records.next_back().is_some_and(|record| record.kind.eq_ignore_ascii_case("SEQEND"))
                || records.any(|record| !record.kind.eq_ignore_ascii_case(expected))
            {
                return Err(DocError::Invalid("Raw compound followers are out of order or have no closing SEQEND.".into()));
            }
        }
        let mut identities = std::collections::HashSet::with_capacity(followers.len() + 1);
        let mut tags = 0usize;
        for record in std::iter::once(&parent).chain(&followers) {
            if !identities.insert(record.handle) {
                return Err(DocError::Invalid(format!("Raw compound identity {} occurs twice.", record.handle.hex())));
            }
            tags = tags.saturating_add(record.tags.len());
            if tags > MAX_RAW_TAGS {
                return Err(DocError::Invalid("The raw unit exceeds the source tag limit.".into()));
            }
        }
        Ok(Self { parent: Arc::new(parent), followers: followers.into(), source_common, reason })
    }
    pub fn parent(&self) -> &RawRecord {
        &self.parent
    }
    pub fn followers(&self) -> &[RawRecord] {
        &self.followers
    }
    pub fn records(&self) -> impl Iterator<Item = &RawRecord> {
        std::iter::once(self.parent()).chain(self.followers.iter())
    }
    pub fn source_common(&self) -> &Common {
        &self.source_common
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// Only appearance fields that the parent-record emitter can rewrite may differ from the source.
    pub fn validate_common(&self, common: &Common) -> Result<()> {
        let plane =
            |value: &Common| [value.extrusion.x.to_bits(), value.extrusion.y.to_bits(), value.extrusion.z.to_bits(), value.thickness.to_bits()];
        if plane(common) != plane(&self.source_common)
            || common.geometry.is_some()
            || common.anno_scales != self.source_common.anno_scales
            || common.xdata != self.source_common.xdata
        {
            return Err(self.edit_refusal());
        }
        Ok(())
    }

    pub fn edit_refusal(&self) -> DocError {
        DocError::RawReadOnly { handle: self.parent.handle(), family: self.parent.kind().into(), reason: self.reason.clone() }
    }
    pub fn geometry_refusal(&self) -> DocError {
        DocError::RawGeometry { handle: self.parent.handle(), family: self.parent.kind().into(), reason: self.reason.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: &str, handle: u64, owner: u64) -> RawRecord {
        RawRecord::new(kind.into(), vec![RawTag { code: 5, value: format!("{handle:X}") }, RawTag { code: 330, value: format!("{owner:X}") }])
            .unwrap()
    }

    #[test]
    fn raw_copy_and_bind_refuse_before_identity_or_foreign_namespace_mutation() {
        use crate::{Drawing, DuplicationSource, Entity};
        let raw = RawCompound::new(record("INSERT", 0x200, 0x1F), vec![], Common::default(), "unsupported".into()).unwrap();
        let entity = Entity::from_raw(raw);
        for source in [DuplicationSource::SameDrawing, DuplicationSource::OtherDrawing] {
            let mut drawing = Drawing::new_metric();
            drawing.model.push(entity.clone());
            let line = Entity::new(
                Handle(0x90),
                crate::EntityKind::Line(crate::Line { a: cadcraft_geom::Vec3::ZERO, b: cadcraft_geom::Vec3::new(1.0, 0.0, 0.0) }),
            );
            drawing.model.push(line.clone());
            let before = drawing.clone();
            let result = drawing.duplicate_entities(vec![vec![line, entity.clone()]], source);
            assert!(matches!(result, Err(DocError::RawReadOnly { handle: Handle(0x200), .. })), "{result:?}");
            assert_eq!(drawing, before);
        }
        let mut drawing = Drawing::new_metric();
        let before = drawing.clone();
        let host = drawing.clone();
        let result = drawing.duplicate_block_content(vec![entity], "Source", "Target", &host);
        assert!(matches!(&result, Err(DocError::RawReadOnly { handle: Handle(0x200), .. })), "{:?}", result.as_ref().err());
        assert_eq!(drawing, before);
    }

    #[test]
    fn raw_metadata_edits_cannot_change_geometry_or_foreign_source_data() {
        use crate::{Drawing, Entity};
        let raw = RawCompound::new(record("INSERT", 0x200, 0x1F), vec![], Common::default(), "unsupported".into()).unwrap();
        let mut drawing = Drawing::new_metric();
        drawing.model.push(Entity::from_raw(raw));
        drawing.model.modify_metadata(Handle(0x200), |common| common.layer = "Edited".into()).unwrap();
        let before = drawing.clone();
        let result = drawing.model.modify_metadata(Handle(0x200), |common| common.extrusion.x = 1.0);
        assert!(matches!(result, Err(DocError::RawReadOnly { handle: Handle(0x200), .. })));
        assert_eq!(drawing, before);
        let result = drawing.model.modify_metadata(Handle(0x200), |common| common.xdata.push(RawTag { code: 1000, value: "changed".into() }));
        assert!(matches!(result, Err(DocError::RawReadOnly { handle: Handle(0x200), .. })));
        assert_eq!(drawing, before);
    }

    #[test]
    fn raw_geometry_edits_refuse_with_identity_family_and_reason_before_any_callback() {
        use crate::{Entity, EntityStore, GeometryRevision};
        use cadcraft_geom::{Mat3, Mat4, Vec2};
        let raw = RawCompound::new(record("INSERT", 0x200, 0x1F), vec![], Common::default(), "No exact INSERT reader.".into()).unwrap();
        let mut entity = Entity::from_raw(raw);
        assert_eq!(entity.geometry_revision, GeometryRevision::RawSource);
        let before = entity.clone();
        let transforms =
            [Mat3::IDENTITY, Mat3::translate(Vec2::new(1.0, 2.0)), Mat3::rotate(0.5), Mat3::scale(2.0, 2.0), Mat3::mirror(Vec2::ZERO, Vec2::X)];
        for transform in transforms {
            let error = entity.try_transform(&Mat4::from_mat3(&transform));
            assert!(
                matches!(error, Err(DocError::RawReadOnly { handle: Handle(0x200), ref family, ref reason }) if family == "INSERT" && reason == "No exact INSERT reader."),
                "{error:?}"
            );
            assert_eq!(entity, before);
        }
        let mut store = EntityStore::default();
        store.push(entity);
        let mut called = false;
        let result = store.modify(Handle(0x200), |_| called = true);
        assert!(matches!(result, Err(DocError::RawReadOnly { handle: Handle(0x200), .. })));
        assert!(!called);
        assert_eq!(store.get(Handle(0x200)).unwrap().as_ref(), &before);
    }

    #[test]
    fn serialized_raw_entities_cannot_claim_a_geometry_revision_or_another_identity() {
        use crate::{Entity, GeometryRevision};
        let raw = RawCompound::new(record("INSERT", 0x200, 0x1F), vec![], Common::default(), "unsupported".into()).unwrap();
        let entity = Entity::from_raw(raw);
        let value = serde_json::to_value(&entity).unwrap();
        assert_eq!(serde_json::from_value::<Entity>(value.clone()).unwrap(), entity);
        for revision in [GeometryRevision::LegacyProjectedV1, GeometryRevision::CanonicalV2] {
            let mut changed = value.clone();
            changed["geometryRevision"] = serde_json::to_value(revision).unwrap();
            assert!(serde_json::from_value::<Entity>(changed).is_err());
        }
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("geometryRevision");
        assert!(serde_json::from_value::<Entity>(missing).is_err());
        let mut changed = value;
        changed["handle"] = serde_json::json!(0x999);
        assert!(serde_json::from_value::<Entity>(changed).is_err());
    }

    #[test]
    fn a_compound_keeps_every_follower_and_source_property_in_order() {
        for (kind, follower) in [("INSERT", "ATTRIB"), ("POLYLINE", "VERTEX")] {
            let parent = record(kind, 0x200, 0x1F);
            let followers = vec![record(follower, 0x201, 0x200), record(follower, 0x202, 0x200), record("SEQEND", 0x203, 0x200)];
            let common = Common { layer: "SourceLayer".into(), ..Default::default() };
            let compound = RawCompound::new(parent, followers, common.clone(), "The source has no exact geometry adapter.".into());
            assert!(compound.is_ok(), "{compound:?}");
            let compound = compound.unwrap();
            assert_eq!(compound.records().map(|r| r.kind()).collect::<Vec<_>>(), vec![kind, follower, follower, "SEQEND"]);
            assert_eq!(compound.records().map(|r| r.handle()).collect::<Vec<_>>(), vec![Handle(0x200), Handle(0x201), Handle(0x202), Handle(0x203)]);
            assert_eq!(compound.source_common(), &common);
        }
    }

    #[test]
    fn malformed_source_identity_or_record_boundaries_are_refused() {
        let tag = |code, value: &str| RawTag { code, value: value.into() };
        for tags in [
            vec![],
            vec![tag(5, "0")],
            vec![tag(5, "not-hex")],
            vec![tag(5, "A"), tag(5, "B")],
            vec![tag(5, "A"), tag(330, "bad-owner")],
            vec![tag(5, "A"), tag(0, "ATTRIB")],
            vec![tag(5, "A"), tag(1, "bad\nrecord")],
            vec![tag(5, "A"), tag(102, "}")],
            vec![tag(5, "A"), tag(102, "{OPEN")],
        ] {
            assert!(RawRecord::new("INSERT".into(), tags).is_err());
        }
        let mut nested = vec![tag(5, "A")];
        nested.extend((0..=crate::foreign::MAX_FOREIGN_DEPTH).map(|_| tag(102, "{NESTED")));
        assert!(RawRecord::new("INSERT".into(), nested).is_err());
    }

    #[test]
    fn malformed_compound_grammar_and_duplicate_child_handles_are_refused() {
        for followers in [
            vec![record("ATTRIB", 0x201, 0x200)],
            vec![record("SEQEND", 0x201, 0x200), record("ATTRIB", 0x202, 0x200)],
            vec![record("VERTEX", 0x201, 0x200), record("SEQEND", 0x202, 0x200)],
            vec![record("ATTRIB", 0x200, 0x200), record("SEQEND", 0x202, 0x200)],
        ] {
            assert!(RawCompound::new(record("INSERT", 0x200, 0x1F), followers, Common::default(), "unsupported".into()).is_err());
        }
        assert!(RawCompound::new(record("POLYLINE", 0x200, 0x1F), vec![], Common::default(), "unsupported".into()).is_err());
        assert!(RawCompound::new(record("ATTRIB", 0x200, 0x1F), vec![], Common::default(), "unsupported".into()).is_err());
        assert!(
            RawCompound::new(record("LINE", 0x200, 0x1F), vec![record("SEQEND", 0x201, 0x200)], Common::default(), "unsupported".into()).is_err()
        );
    }

    #[test]
    fn serde_rechecks_source_invariants_and_does_not_accept_cached_identities() {
        let compound = RawCompound::new(
            record("INSERT", 0x200, 0x1F),
            vec![record("ATTRIB", 0x201, 0x200), record("SEQEND", 0x202, 0x200)],
            Common::default(),
            "unsupported".into(),
        )
        .unwrap();
        let serialized = serde_json::to_value(&compound).unwrap();
        assert_eq!(serde_json::from_value::<RawCompound>(serialized.clone()).unwrap(), compound);
        assert!(serialized["parent"].get("handle").is_none());
        let mut changed = serialized;
        changed["followers"][1]["kind"] = serde_json::json!("ATTRIB");
        assert!(serde_json::from_value::<RawCompound>(changed).is_err());
    }

    #[test]
    fn a_source_record_keeps_its_identity_owner_and_exact_ordered_tags() {
        let tags = vec![
            RawTag { code: 5, value: "002A".into() },
            RawTag { code: 102, value: "{ACAD_REACTORS".into() },
            RawTag { code: 330, value: "AB".into() },
            RawTag { code: 102, value: "}".into() },
            RawTag { code: 330, value: "1F".into() },
            RawTag { code: 10, value: "000.10000000000000001".into() },
            RawTag { code: 1001, value: "SOURCE_APP".into() },
            RawTag { code: 1000, value: "keep this string".into() },
        ];
        let record = RawRecord::new("INSERT".into(), tags.clone());
        assert!(record.is_ok(), "{record:?}");
        let record = record.unwrap();
        assert_eq!(record.kind(), "INSERT");
        assert_eq!(record.handle(), Handle(0x2A));
        assert_eq!(record.owner(), Some(Handle(0x1F)));
        assert_eq!(record.tags(), tags);
    }
}
