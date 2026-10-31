//! The canonical reading of a MULTILEADER (revision `CanonicalV2`): the groups' own WCS points and directions, the text
//! direction in full and the block scale as the record has it.
//!
//! Not part of `read_dxf` yet: the reader still reads legacy multileaders (`entities.rs`) until the Drawing boundary accepts
//! canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of what
//! `canonical_mleader_from_groups` returns. The style, text style, block and arrowhead names come from the file's objects
//! and are filled in by the caller's document pass, as for the legacy reader.

use cadcraft_doc::{Common, EntityKind};
use cadcraft_dxf::Tag;

use super::common::{T, canonical_common, common};
use super::{Drawing, Rx};

/// A MULTILEADER from the groups of a DXF record (without its `0` group), read canonically. `None` for a text or a block
/// content whose normal has no direction, or a text direction with no length.
pub fn canonical_mleader_from_groups(tags: &[Tag], drawing: &mut Drawing, rx: &Rx) -> Option<(Common, EntityKind)> {
    let leader = crate::dxf_mleader::read_canonical(tags)?;
    Some((canonical_common(common(&T(tags)), tags, drawing, rx)?, EntityKind::MLeader(leader)))
}
