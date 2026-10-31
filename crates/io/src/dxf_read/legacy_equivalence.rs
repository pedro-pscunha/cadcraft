//! Static Legacy suitability, separate from canonical readability and activation policy.
use crate::activation::{Family, records::family_of_kind};
use cadcraft_dxf::{Tag, Version};

#[cfg(test)]
mod generated;
#[cfg(test)]
mod oracle;
mod ownership;
mod policy;
mod static_rule;
pub use ownership::{DerivedOwner, LegacyClassifier, LegacyRecordClassification};
#[cfg(test)]
mod tests;

/// Whether a source unit is within the tested lossless Legacy subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyDisposition {
    Plain,
    Needs,
}

/// The family and rule revision used to classify one source unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyClassification {
    pub family: Family,
    pub disposition: LegacyDisposition,
    pub policy_revision: u16,
    pub policy_reason: &'static str,
}

/// Classify a parent and its ordered followers without constructing geometry or calling a canonical reader.
pub fn classify_legacy_unit(unit: &[(String, Vec<Tag>)], version: Version) -> Option<LegacyClassification> {
    let (kind, tags) = unit.first()?;
    let family = family_of_kind(kind)?;
    Some(classify_groups(family, tags, unit.len() > 1, version))
}

fn classify_groups(family: Family, tags: &[Tag], has_followers: bool, version: Version) -> LegacyClassification {
    let policy = policy::policy(family, version);
    let plain = policy.accepts(tags) && static_rule::plain(family, tags, has_followers);
    LegacyClassification {
        family,
        disposition: if plain { LegacyDisposition::Plain } else { LegacyDisposition::Needs },
        policy_revision: policy.revision,
        policy_reason: policy.reason,
    }
}
