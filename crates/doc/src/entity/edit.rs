//! Entity-level edit dispatch. Legacy payloads stay on their existing path until their family migrates.

use cadcraft_geom::{Mat3, Mat4};

use super::{Common, Entity, EntityKind, GeometryRevision, MText, Similarity, Text};
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
        if candidate.handle != self.handle {
            return Err(DocError::Invalid("A geometry edit cannot change the record identity.".into()));
        }
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

    /// Whether the kind of a canonical payload has a `transformed_canonical` hook.
    fn has_canonical_transform(&self) -> bool {
        matches!(
            self.kind,
            EntityKind::Text(_)
                | EntityKind::MText(_)
                | EntityKind::AttDef(_)
                | EntityKind::Tolerance(_)
                | EntityKind::Table(_)
                | EntityKind::Wipeout(_)
                | EntityKind::Helix(_)
                | EntityKind::MLine(_)
                | EntityKind::Hatch(_)
                | EntityKind::Circle(_)
                | EntityKind::Arc(_)
                | EntityKind::LwPolyline(_)
                | EntityKind::Solid(_)
                | EntityKind::Trace(_)
                | EntityKind::Spline(_)
                | EntityKind::Leader(_)
                | EntityKind::Dimension(_)
                | EntityKind::Ellipse(_)
                | EntityKind::Line(_)
                | EntityKind::Point(_)
                | EntityKind::Polyline3d(_)
                | EntityKind::PolylineMesh(_)
                | EntityKind::Face3d(_)
                | EntityKind::Ray(_)
                | EntityKind::XLine(_)
                | EntityKind::Image(_)
                | EntityKind::MLeader(_)
                | EntityKind::Insert(_)
        )
    }

    /// Transform a canonical payload through its family's hook: the payload, the plane normal (`Common.extrusion`) and the
    /// thickness follow together, or the call fails with the entity as it was (the caller works on a candidate).
    fn transform_canonical(&mut self, transform: &Mat4) -> Result<()> {
        let normal = self.common.extrusion;
        // Families that stay exact under a similarity also scale the thickness with it.
        let scale = || Similarity::from_mat4(transform).map(|s| s.scale);
        let new_normal = match &mut self.kind {
            EntityKind::Text(t) => {
                let (moved, n) = t.transformed_canonical(normal, transform)?;
                *t = moved;
                Some((n, scale()?))
            }
            EntityKind::MText(t) => {
                let (moved, n) = t.transformed_canonical(normal, transform)?;
                *t = moved;
                Some((n, scale()?))
            }
            EntityKind::AttDef(a) => {
                let (text, n) = a.text.transformed_canonical(normal, transform)?;
                let multiline = a.multiline.as_ref().map(|m| m.transformed_canonical(transform)).transpose()?;
                a.text = text;
                a.multiline = multiline;
                Some((n, scale()?))
            }
            EntityKind::Tolerance(t) => {
                let (moved, n) = t.transformed_canonical(normal, transform)?;
                *t = moved;
                Some((n, scale()?))
            }
            EntityKind::Table(t) => {
                let (moved, n) = t.transformed_canonical(normal, transform)?;
                *t = moved;
                Some((n, scale()?))
            }
            EntityKind::Wipeout(w) => {
                *w = w.transformed_canonical(transform)?;
                None
            }
            EntityKind::Helix(h) => {
                *h = h.transformed_canonical(transform)?;
                None
            }
            EntityKind::MLine(m) => {
                let (moved, n) = m.transformed_canonical(normal, transform)?;
                *m = moved;
                Some((n, scale()?))
            }
            EntityKind::Hatch(h) => {
                let (moved, n) = h.transformed_canonical(normal, transform)?;
                *h = moved;
                Some((n, scale()?))
            }
            EntityKind::Circle(c) => {
                let (moved, n) = c.transformed_canonical(normal, transform)?;
                *c = moved;
                Some((n, scale()?))
            }
            EntityKind::Arc(a) => {
                let (moved, n) = a.transformed_canonical(normal, transform)?;
                *a = moved;
                Some((n, scale()?))
            }
            EntityKind::LwPolyline(p) => {
                let (moved, n) = p.transformed_canonical(normal, transform)?;
                *p = moved;
                Some((n, scale()?))
            }
            EntityKind::Solid(s) | EntityKind::Trace(s) => {
                let (moved, n) = s.transformed_canonical(normal, transform)?;
                *s = moved;
                Some((n, scale()?))
            }
            EntityKind::Spline(s) => {
                let (moved, n) = super::spline_canonical::spline_transformed_canonical(s, normal, transform)?;
                *s = moved;
                Some((n, 1.0))
            }
            EntityKind::Leader(l) => {
                let (moved, n) = l.transformed_canonical(normal, transform)?;
                *l = moved;
                Some((n, scale()?))
            }
            EntityKind::Dimension(d) => {
                let (moved, n) = d.transformed_canonical(normal, transform)?;
                *d = moved;
                Some((n, scale()?))
            }
            // An ellipse has no thickness; any invertible affine map gives an ellipse.
            EntityKind::Ellipse(e) => {
                let (moved, n) = e.transformed_canonical(normal, transform)?;
                *e = moved;
                Some((n, 1.0))
            }
            // The extrusion of a line or a point is a vector: it follows the linear part and the thickness its length.
            EntityKind::Line(l) => {
                let (moved, extrusion, factor) = l.transformed_canonical(normal, transform)?;
                *l = moved;
                Some((extrusion, factor))
            }
            EntityKind::Point(p) => {
                let (moved, extrusion, factor) = p.transformed_canonical(normal, transform)?;
                *p = moved;
                Some((extrusion, factor))
            }
            // WCS positions only: the plane and the thickness of these stay as they are.
            EntityKind::Polyline3d(p) => {
                *p = p.transformed_canonical(transform)?;
                None
            }
            EntityKind::PolylineMesh(m) => {
                *m = m.transformed_canonical(transform)?;
                None
            }
            EntityKind::Face3d(f) => {
                *f = f.transformed_canonical(transform)?;
                None
            }
            EntityKind::Ray(r) | EntityKind::XLine(r) => {
                *r = r.transformed_canonical(transform)?;
                None
            }
            EntityKind::Image(i) => {
                *i = i.transformed_canonical(transform)?;
                None
            }
            // A block reference keeps its thickness; its normal follows the edit.
            EntityKind::Insert(i) => {
                let (moved, n) = i.transformed_canonical(normal, transform)?;
                *i = moved;
                Some((n, 1.0))
            }
            // WCS positions and directions only: a multileader has no plane or thickness of its own.
            EntityKind::MLeader(m) => {
                *m = m.transformed_canonical(transform)?;
                None
            }
            _ => return Err(DocError::Invalid("This canonical entity has no supported geometry transform.".into())),
        };
        if let Some((n, k)) = new_normal {
            self.common.extrusion = n;
            self.common.thickness *= k;
        }
        Ok(())
    }

    fn dispatch_transform(&mut self, transform: &Mat4) -> Result<()> {
        if self.geometry_revision == GeometryRevision::CanonicalV2 && self.has_canonical_transform() {
            return self.transform_canonical(transform);
        }
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
