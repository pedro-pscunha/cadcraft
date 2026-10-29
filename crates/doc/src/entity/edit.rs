//! Entity-level edit dispatch. Legacy payloads stay on their existing path until their family migrates.

use cadcraft_geom::{Mat3, Mat4};

use super::{Entity, EntityKind, MText, Text};
use crate::{DocError, Result, has_finite_numbers};

impl Entity {
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
        candidate.dispatch_transform(transform)?;
        if !has_finite_numbers(&candidate) {
            return Err(DocError::Invalid("The geometry result has infinite or undefined coordinates.".into()));
        }
        Ok(candidate)
    }

    fn dispatch_transform(&mut self, transform: &Mat4) -> Result<()> {
        let matrix = legacy_matrix(transform)?;
        // Each canonical family replaces its own arm, never a catch-all legacy fallback.
        match &self.kind {
            EntityKind::Unknown(_) => return Err(DocError::Invalid("This entity has no supported geometry transform.".into())),
            EntityKind::Ellipse(ellipse) => {
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
            | EntityKind::Tolerance(_) => {}
        }
        self.kind.transform_legacy(&matrix)
    }
}

pub(super) fn legacy_matrix(transform: &Mat4) -> Result<Mat3> {
    if !has_finite_numbers(transform) {
        return Err(DocError::Invalid("The geometry transform has infinite or undefined values.".into()));
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

pub(super) fn text_payload(text: &mut Text, transform: &Mat4, owner: TextOwner) -> Result<()> {
    let matrix = legacy_matrix(transform)?;
    let mirror = matches!(owner, TextOwner::Entity) && matrix.is_mirroring();
    super::transform_text(text, &matrix, matrix.scale_factor(), matrix.rotation(), mirror);
    Ok(())
}

/// All nested MTEXT edits enter here, even while the containing INSERT or MLEADER stays legacy.
pub(super) fn mtext_payload(text: &mut MText, transform: &Mat4, owner: TextOwner) -> Result<()> {
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
