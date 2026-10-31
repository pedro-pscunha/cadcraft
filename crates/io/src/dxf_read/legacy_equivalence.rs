//! Static Legacy suitability, separate from canonical readability and activation policy.
use crate::activation::Family;
use cadcraft_dxf::{Tag, Version};

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

/// The activation family of an entity kind. An unrecognized kind is not implicitly plain.
pub fn family_from_dxf(kind: &str) -> Option<Family> {
    const KINDS: &[(&str, Family)] = &[
        ("CIRCLE", Family::Circle),
        ("ARC", Family::Arc),
        ("ELLIPSE", Family::Ellipse),
        ("LINE", Family::Line),
        ("POINT", Family::Point),
        ("RAY", Family::Ray),
        ("XLINE", Family::XLine),
        ("LWPOLYLINE", Family::LwPolyline),
        ("POLYLINE", Family::Polyline),
        ("SOLID", Family::Solid),
        ("TRACE", Family::Trace),
        ("3DFACE", Family::Face3d),
        ("SPLINE", Family::Spline),
        ("LEADER", Family::Leader),
        ("DIMENSION", Family::Dimension),
        ("ARC_DIMENSION", Family::Dimension),
        ("LARGE_RADIAL_DIMENSION", Family::Dimension),
        ("TEXT", Family::Text),
        ("MTEXT", Family::MText),
        ("ATTDEF", Family::AttDef),
        ("ATTRIB", Family::Attrib),
        ("TOLERANCE", Family::Tolerance),
        ("HATCH", Family::Hatch),
        ("WIPEOUT", Family::Wipeout),
        ("MLINE", Family::MLine),
        ("HELIX", Family::Helix),
        ("IMAGE", Family::Image),
        ("ACAD_TABLE", Family::Table),
        ("INSERT", Family::Insert),
        ("MULTILEADER", Family::MLeader),
        ("MLEADER", Family::MLeader),
        ("REGION", Family::Region),
        ("VIEWPORT", Family::Viewport),
    ];
    KINDS.iter().find(|(name, _)| name.eq_ignore_ascii_case(kind)).map(|(_, family)| *family)
}

/// Classify a parent and its ordered followers without constructing geometry or calling a canonical reader.
pub fn classify_legacy_unit(unit: &[(String, Vec<Tag>)], version: Version) -> Option<LegacyClassification> {
    let (kind, tags) = unit.first()?;
    let family = family_from_dxf(kind)?;
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
