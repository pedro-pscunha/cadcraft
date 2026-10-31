use super::{LegacyClassification, classify_groups};
use crate::activation::{
    Family,
    records::{Container, NodeKey, RecordView},
};
use cadcraft_doc::Handle;
use cadcraft_dxf::{Section, Version, records};
use std::collections::BTreeMap;

/// The authored record that owns a generated graphics definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedOwner {
    pub family: Family,
    pub handle: Option<Handle>,
}

/// Derived graphics inherit their owner; they do not receive an independent plain verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyRecordClassification {
    Authored(LegacyClassification),
    OwnerDerived(DerivedOwner),
}

/// Per-load classification context. Construction scans references only, not geometry.
pub struct LegacyClassifier {
    version: Version,
    derived: BTreeMap<String, DerivedOwner>,
}

impl LegacyClassifier {
    pub fn new(sections: &[Section], version: Version) -> Self {
        let mut derived = BTreeMap::new();
        for section in sections.iter().filter(|section| matches!(section.name.as_str(), "ENTITIES" | "BLOCKS")) {
            for (kind, groups) in records(&section.tags) {
                let family = match kind.as_str() {
                    "ACAD_TABLE" => Family::Table,
                    "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => Family::Dimension,
                    _ => continue,
                };
                let Some(name) =
                    groups.iter().find(|tag| tag.code == 2).map(|tag| tag.str().trim().to_ascii_uppercase()).filter(|name| !name.is_empty())
                else {
                    continue;
                };
                let handle = groups.iter().find(|tag| tag.code == 5).and_then(|tag| Handle::parse_hex(&tag.str()));
                derived.insert(name, DerivedOwner { family, handle });
            }
        }
        Self { version, derived }
    }

    pub fn derived_owner(&self, container: &Container) -> Option<DerivedOwner> {
        match container {
            Container::Entities => None,
            Container::Block(name) => self.derived.get(name).copied(),
        }
    }

    pub fn record(&self, view: &RecordView<'_>) -> LegacyRecordClassification {
        if let NodeKey::Record { container, .. } = view.key
            && let Some(owner) = self.derived_owner(container)
        {
            return LegacyRecordClassification::OwnerDerived(owner);
        }
        LegacyRecordClassification::Authored(classify_groups(view.family, view.groups, false, self.version))
    }
}
