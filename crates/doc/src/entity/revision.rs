use serde::{Deserialize, Serialize};

/// Coordinate semantics of one independently editable geometry payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GeometryRevision {
    #[default]
    LegacyProjectedV1,
    CanonicalV2,
    /// An opaque source unit with no geometry interpretation.
    RawSource,
}

impl GeometryRevision {
    pub(crate) fn require_legacy(self) -> crate::Result<()> {
        match self {
            Self::LegacyProjectedV1 => Ok(()),
            Self::CanonicalV2 => Err(crate::DocError::Invalid("This canonical geometry payload has no implementation for this edit.".into())),
            Self::RawSource => Err(crate::DocError::Invalid("A raw source unit has no editable geometry payload.".into())),
        }
    }
}
