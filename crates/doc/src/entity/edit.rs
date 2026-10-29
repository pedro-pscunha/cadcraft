//! Entity-level edit dispatch. Legacy payloads stay on their existing path until their family migrates.

use cadcraft_geom::{Mat3, Mat4};

use super::{Common, Entity, EntityKind, GeometryRevision, MText, Text};
use crate::{DocError, Result, has_finite_numbers};

impl Entity {
    /// Refuse canonical geometry before an unmigrated routine can inspect or mutate its fields.
    pub fn require_legacy_geometry(&self) -> Result<()> {
        self.geometry_revision.require_legacy()?;
        match &self.kind {
            EntityKind::AttDef(attribute) => attribute.require_legacy_geometry()?,
            EntityKind::Insert(insert) => {
                for attribute in &insert.attribs {
                    attribute.require_legacy_geometry()?;
                }
            }
            EntityKind::MLeader(leader) => {
                leader.context.geometry_revision.require_legacy()?;
                if let Some(insert) = &leader.block {
                    for attribute in &insert.attribs {
                        attribute.require_legacy_geometry()?;
                    }
                }
            }
            EntityKind::Line(_)
            | EntityKind::Point(_)
            | EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::PolylineMesh(_)
            | EntityKind::Spline(_)
            | EntityKind::Helix(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::Dimension(_)
            | EntityKind::Leader(_)
            | EntityKind::MLine(_)
            | EntityKind::Hatch(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::Region(_)
            | EntityKind::Viewport(_)
            | EntityKind::Image(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Unknown(_) => {}
        }
        Ok(())
    }

    /// Run an unmigrated edit only on legacy payloads and commit a finite candidate atomically.
    pub fn try_legacy_edit<F, T>(&mut self, edit: F) -> Result<T>
    where
        F: FnOnce(&mut Entity) -> Result<T>,
    {
        let (candidate, result) = self.legacy_edited(edit)?;
        *self = candidate;
        Ok(result)
    }

    pub(crate) fn legacy_edited<F, T>(&self, edit: F) -> Result<(Self, T)>
    where
        F: FnOnce(&mut Entity) -> Result<T>,
    {
        self.require_legacy_geometry()?;
        let mut candidate = self.clone();
        let result = edit(&mut candidate).map_err(|e| if matches!(e, DocError::NonFinite(_)) { DocError::NonFinite(self.handle) } else { e })?;
        candidate.require_legacy_geometry()?;
        if !has_finite_numbers(&candidate) {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok((candidate, result))
    }

    pub(crate) fn metadata_edited<F, T>(&self, edit: F) -> Result<(Self, T)>
    where
        F: FnOnce(&mut Common) -> Result<T>,
    {
        let mut candidate = self.clone();
        let result =
            edit(&mut candidate.common).map_err(|e| if matches!(e, DocError::NonFinite(_)) { DocError::NonFinite(self.handle) } else { e })?;
        let plane =
            |common: &Common| [common.extrusion.x.to_bits(), common.extrusion.y.to_bits(), common.extrusion.z.to_bits(), common.thickness.to_bits()];
        let same_snapshot = match (&self.common.geometry, &candidate.common.geometry) {
            (Some(before), Some(after)) => std::sync::Arc::ptr_eq(before, after),
            (None, None) => true,
            _ => false,
        };
        if plane(&self.common) != plane(&candidate.common) || !same_snapshot {
            return Err(DocError::Invalid("A metadata edit cannot change the entity plane, thickness, or source geometry owner.".into()));
        }
        if !has_finite_numbers(&candidate) {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok((candidate, result))
    }

    /// Apply an affine world-space transform atomically, or leave this entity unchanged.
    pub fn try_transform(&mut self, transform: &Mat4) -> Result<()> {
        *self = self.transformed(transform)?;
        Ok(())
    }

    /// The transformed candidate, with source identity and properties, or an explicit refusal.
    pub fn transformed(&self, transform: &Mat4) -> Result<Self> {
        let mut candidate = self.clone();
        if *transform == Mat4::IDENTITY {
            return Ok(candidate);
        }
        // A result too large to represent names this entity, whichever check finds it.
        candidate
            .dispatch_transform(transform)
            .map_err(|e| if matches!(e, DocError::NonFinite(_)) { DocError::NonFinite(self.handle) } else { e })?;
        if !has_finite_numbers(&candidate) {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok(candidate)
    }

    fn dispatch_transform(&mut self, transform: &Mat4) -> Result<()> {
        // Each canonical family replaces its own arm, never a catch-all legacy fallback.
        match &self.kind {
            EntityKind::Unknown(_) => return Err(DocError::Invalid("This entity has no supported geometry transform.".into())),
            EntityKind::Ellipse(ellipse) => {
                self.geometry_revision.require_legacy()?;
                let matrix = legacy_matrix(transform)?;
                let translation = matrix.a == 1.0 && matrix.b == 0.0 && matrix.c == 0.0 && matrix.d == 1.0;
                if !translation && !ellipse.clone().flatten() {
                    return Err(DocError::Invalid("An edge-on or invalid ellipse cannot use the legacy 2D edit.".into()));
                }
            }
            EntityKind::Line(_)
            | EntityKind::Point(_)
            | EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::PolylineMesh(_)
            | EntityKind::Spline(_)
            | EntityKind::Helix(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::AttDef(_)
            | EntityKind::Insert(_)
            | EntityKind::Dimension(_)
            | EntityKind::Leader(_)
            | EntityKind::MLeader(_)
            | EntityKind::MLine(_)
            | EntityKind::Hatch(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::Region(_)
            | EntityKind::Viewport(_)
            | EntityKind::Image(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_) => self.geometry_revision.require_legacy()?,
        }
        // A multileader's content frame has a revision of its own.
        if let EntityKind::MLeader(m) = &self.kind {
            m.context.geometry_revision.require_legacy()?;
        }
        self.kind.transform_legacy(&legacy_matrix(transform)?)
    }
}

pub(super) fn legacy_matrix(transform: &Mat4) -> Result<Mat3> {
    if !has_finite_numbers(transform) {
        return Err(DocError::NonFinite(super::Handle(0)));
    }
    let rows = &transform.m;
    let matrix = Mat3 { a: rows[0][0], b: rows[1][0], c: rows[0][1], d: rows[1][1], e: rows[0][3], f: rows[1][3] };
    if *transform != Mat4::from_mat3(&matrix) {
        return Err(DocError::Invalid("This legacy geometry payload cannot use a spatial transform.".into()));
    }
    Ok(matrix)
}

pub(super) enum TextOwner {
    Entity,
    AttributeInstance,
    MLeaderContent,
}

pub(super) fn text_payload(text: &mut Text, revision: GeometryRevision, transform: &Mat4, owner: TextOwner) -> Result<()> {
    revision.require_legacy()?;
    let matrix = legacy_matrix(transform)?;
    let mirror = matches!(owner, TextOwner::Entity) && matrix.is_mirroring();
    super::transform_text(text, &matrix, matrix.scale_factor(), matrix.rotation(), mirror);
    Ok(())
}

/// All nested MTEXT edits enter here, even while the containing INSERT or MLEADER stays legacy.
pub(super) fn mtext_payload(text: &mut MText, revision: GeometryRevision, transform: &Mat4, owner: TextOwner) -> Result<()> {
    revision.require_legacy()?;
    let matrix = legacy_matrix(transform)?;
    let scale = matrix.scale_factor();
    let position = matrix.apply(text.insert.xy());
    text.insert.x = position.x;
    text.insert.y = position.y;
    text.height *= scale;
    text.width *= scale;
    if let Some(columns) = &mut text.columns {
        columns.scale(scale);
    }
    if !matches!(owner, TextOwner::MLeaderContent) {
        text.rotation = cadcraft_geom::norm_angle(text.rotation + matrix.rotation());
    }
    Ok(())
}
