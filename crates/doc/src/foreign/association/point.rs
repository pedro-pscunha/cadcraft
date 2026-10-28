//! Semantic evaluation of preserved AutoCAD point references. Presentation/cache groups do not
//! decide whether a link can follow geometry; unsupported paths and snaps remain opaque.
use crate::geom::Vec3;
use crate::{AssocSnap, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, RawTag};

struct PointRef {
    osnap: i64,
    path: Vec<Handle>,
    other: Vec<Handle>,
    subentity: i64,
    marker: i64,
    parameter: Option<f64>,
    cached: Option<Vec3>,
}

impl PointRef {
    fn parse(tags: &[RawTag]) -> Option<Self> {
        let value = |code| tags.iter().find(|t| t.code == code).map(|t| t.value.trim());
        let integer = |code| value(code).and_then(|v| v.parse::<i64>().ok());
        let real = |code| value(code).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
        let path = |code| tags.iter().filter(|t| t.code == code).take(9).map(|t| Handle::parse_hex(&t.value)).collect::<Option<Vec<_>>>();
        let cached = real(10)
            .zip(real(20))
            .zip(real(30))
            .map(|((x, y), z)| Vec3::new(x, y, z))
            .filter(|p| p.x.abs() < 1e50 && p.y.abs() < 1e50 && p.z.abs() < 1e50);
        Some(Self {
            osnap: integer(72)?,
            path: path(331)?,
            other: path(332)?,
            subentity: integer(73).unwrap_or(0),
            marker: integer(91).unwrap_or(0),
            parameter: real(40),
            cached,
        })
    }

    fn matches(&self, d: &Drawing, link: &DimAssoc) -> bool {
        if self.path.as_slice() != [link.handle] || crate::association_point(link, |h| d.entity_including_blocks(h).map(|e| e.as_ref())).is_none() {
            return false;
        }
        match &link.snap {
            AssocSnap::Start => {
                self.osnap == 13
                    || self.osnap == 1
                        && self.cached.is_some_and(|p| {
                            crate::association_point(link, |h| d.entity_including_blocks(h).map(|e| e.as_ref()))
                                .is_some_and(|q| q.dist(p.xy()) < 1e-6)
                        })
            }
            AssocSnap::End => self.osnap == 1,
            AssocSnap::Mid => self.osnap == 2,
            AssocSnap::Center => matches!(self.osnap, 3 | 4),
            AssocSnap::Vertex { index } => {
                let Some(EntityKind::LwPolyline(poly)) = d.entity_including_blocks(link.handle).map(|e| &e.kind) else { return false };
                let Some(segment) = usize::try_from(self.marker).ok().filter(|i| *i > 0) else { return false };
                let n = poly.vertices.len();
                let segments = if poly.closed { n } else { n.saturating_sub(1) };
                self.subentity == 2
                    && segment <= segments
                    && ((self.osnap == 13 && segment - 1 == *index) || (self.osnap == 1 && n > 0 && segment % n == *index))
            }
            AssocSnap::OnCircle { angle } => match self.osnap {
                10 => {
                    self.parameter.is_some_and(|p| (p - angle).abs() < 1e-9)
                        || self.cached.is_some_and(|p| {
                            crate::association_point(link, |h| d.entity_including_blocks(h).map(|e| e.as_ref()))
                                .is_some_and(|q| q.dist(p.xy()) < 1e-6)
                        })
                }
                5 => self.cached.is_some_and(|p| {
                    crate::association_point(link, |h| d.entity_including_blocks(h).map(|e| e.as_ref())).is_some_and(|q| q.dist(p.xy()) < 1e-6)
                }),
                _ => false,
            },
            AssocSnap::Intersection { other } => self.osnap == 6 && self.other.as_slice() == [*other],
        }
    }
}

pub(super) fn evaluable(d: &Drawing, dm: &Dimension, tags: &[RawTag]) -> bool {
    let Some((start, _)) = super::super::dimassoc_reference(tags.iter().map(|t| (t.code, t.value.as_str()))) else { return false };
    let tail = tags.get(start..).unwrap_or(&[]);
    let integer = |code| tail.iter().find(|t| t.code == code).and_then(|t| t.value.trim().parse::<i64>().ok());
    if integer(70).unwrap_or(0) != 0 || integer(71).unwrap_or(0) != 0 {
        return false;
    }
    let starts: Vec<_> = tail.iter().enumerate().filter(|(_, t)| t.code == 1).map(|(i, _)| i).take(9).collect();
    if starts.is_empty() || starts.len() > 8 {
        return false;
    }
    let refs: Option<Vec<_>> = starts
        .iter()
        .enumerate()
        .map(|(i, start)| {
            if !tail.get(*start)?.value.trim().eq_ignore_ascii_case("AcDbOsnapPointRef") {
                return None;
            }
            PointRef::parse(tail.get(start + 1..starts.get(i + 1).copied().unwrap_or(tail.len()))?)
        })
        .collect();
    let Some(refs) = refs else { return false };
    if matches!(dm.kind, DimKind::Radius | DimKind::Diameter) {
        return refs.len() == 1
            && refs.first().is_some_and(|r| {
                matches!(r.osnap, 3 | 4)
                    && r.path.len() == 1
                    && dm.assoc.len() == 2
                    && dm.assoc.iter().all(|a| {
                        r.path.as_slice() == [a.handle] && crate::association_point(a, |h| d.entity_including_blocks(h).map(|e| e.as_ref())).is_some()
                    })
            });
    }
    let slots: &[&str] = match dm.kind {
        DimKind::Linear { .. } | DimKind::Aligned | DimKind::ArcLength => &["p13", "p14"],
        DimKind::Angular => &["p13", "p14", "defpt", "p15"],
        DimKind::Angular3P => &["p13", "p14", "p15"],
        DimKind::Ordinate { .. } => &["p13"],
        DimKind::Radius | DimKind::Diameter => &[],
    };
    let flags = integer(90).unwrap_or(0);
    let used: Vec<_> = slots.iter().enumerate().filter(|(i, _)| flags & (1 << i) != 0).map(|(_, s)| *s).collect();
    refs.len() == used.len()
        && dm.assoc.len() == used.len()
        && used.iter().zip(&refs).all(|(point, r)| dm.assoc.iter().find(|a| a.point == *point).is_some_and(|a| r.matches(d, a)))
}
