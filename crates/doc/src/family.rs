//! The entity families the canonical readers cover, and sets of them (the mask of a reader or of a load).

use crate::EntityKind;

/// The families the canonical readers cover (the stages of the plan turn them on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Family {
    Circle,
    Arc,
    Ellipse,
    Line,
    Point,
    Ray,
    XLine,
    LwPolyline,
    Polyline,
    Solid,
    Trace,
    Face3d,
    Spline,
    Leader,
    Dimension,
    Text,
    MText,
    AttDef,
    Attrib,
    Tolerance,
    Hatch,
    Wipeout,
    MLine,
    Helix,
    Image,
    Table,
    Insert,
    MLeader,
    Region,
    Viewport,
}

impl Family {
    /// The family of an entity kind, `None` for a kind without a canonical reader (unknown and raw records).
    pub fn of(kind: &EntityKind) -> Option<Family> {
        Some(match kind {
            EntityKind::Line(_) => Family::Line,
            EntityKind::Point(_) => Family::Point,
            EntityKind::Circle(_) => Family::Circle,
            EntityKind::Arc(_) => Family::Arc,
            EntityKind::Ellipse(_) => Family::Ellipse,
            EntityKind::LwPolyline(_) => Family::LwPolyline,
            EntityKind::Polyline3d(_) | EntityKind::PolylineMesh(_) => Family::Polyline,
            EntityKind::Spline(_) => Family::Spline,
            EntityKind::Helix(_) => Family::Helix,
            EntityKind::Ray(_) => Family::Ray,
            EntityKind::XLine(_) => Family::XLine,
            EntityKind::Text(_) => Family::Text,
            EntityKind::MText(_) => Family::MText,
            EntityKind::AttDef(_) => Family::AttDef,
            EntityKind::Insert(_) => Family::Insert,
            EntityKind::Dimension(_) => Family::Dimension,
            EntityKind::MLine(_) => Family::MLine,
            EntityKind::Leader(_) => Family::Leader,
            EntityKind::MLeader(_) => Family::MLeader,
            EntityKind::Hatch(_) => Family::Hatch,
            EntityKind::Solid(_) => Family::Solid,
            EntityKind::Trace(_) => Family::Trace,
            EntityKind::Face3d(_) => Family::Face3d,
            EntityKind::Region(_) => Family::Region,
            EntityKind::Viewport(_) => Family::Viewport,
            EntityKind::Image(_) => Family::Image,
            EntityKind::Wipeout(_) => Family::Wipeout,
            EntityKind::Table(_) => Family::Table,
            EntityKind::Tolerance(_) => Family::Tolerance,
            EntityKind::Unknown(_) | EntityKind::Raw(_) => return None,
        })
    }
}

/// A set of families.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FamilySet(u64);

impl FamilySet {
    pub const NONE: FamilySet = FamilySet(0);
    pub const ALL: FamilySet = FamilySet(u64::MAX);

    pub fn of(families: &[Family]) -> FamilySet {
        FamilySet(families.iter().fold(0, |bits, f| bits | (1 << (*f as u8))))
    }

    pub fn contains(self, family: Family) -> bool {
        self.0 & (1 << (family as u8)) != 0
    }

    /// This set without `family`.
    pub fn without(self, family: Family) -> FamilySet {
        FamilySet(self.0 & !(1 << (family as u8)))
    }
}
