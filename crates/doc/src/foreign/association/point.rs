//! Semantic point references; caches and optional xref text never decide equivalence.
use crate::geom::Vec3;
use crate::{AssocSnap, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, RawTag};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Binding {
    evaluable: bool,
    targets: BTreeSet<Handle>,
}

impl Binding {
    pub(super) fn can_update(&self, changed: &impl Fn(Handle) -> bool) -> bool {
        self.evaluable || !self.targets.iter().any(|h| changed(*h))
    }
    pub(super) fn remap(&mut self, map: &BTreeMap<Handle, Handle>) {
        self.targets = self.targets.iter().map(|h| map.get(h).copied().unwrap_or(*h)).collect();
    }
}

struct PointRef {
    osnap: i64,
    path: Vec<Handle>,
    other: Vec<Handle>,
    subentity: i64,
    marker: i64,
    _parameter: Option<f64>,
    _cached: Option<Vec3>,
    last: Option<Box<PointRef>>,
}

// Autodesk DIMASSOC table: 8 = Perpendicular, 10 = Nearest. Neither is evaluated here.
fn supported_osnap(code: i64) -> bool {
    matches!(code, 1..=6 | 13)
}

impl PointRef {
    fn parse(chunks: &[&[RawTag]], cursor: &mut usize, depth: usize) -> Option<Self> {
        if depth >= super::super::MAX_FOREIGN_DEPTH {
            return None;
        }
        let tags = *chunks.get(*cursor)?;
        *cursor = cursor.checked_add(1)?;
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
            _parameter: real(40),
            _cached: cached,
            last: if integer(75).unwrap_or(0) != 0 { Some(Box::new(Self::parse(chunks, cursor, depth + 1)?)) } else { None },
        })
    }

    fn targets(&self) -> BTreeSet<Handle> {
        self.path.iter().chain(&self.other).copied().chain(self.last.iter().flat_map(|last| last.targets())).collect()
    }

    fn matches(&self, d: &Drawing, link: &DimAssoc) -> bool {
        if !supported_osnap(self.osnap)
            || self.last.is_some()
            || self.path.as_slice() != [link.handle]
            || crate::association_point(link, |h| d.entity_including_blocks(h).map(|e| e.as_ref())).is_none()
        {
            return false;
        }
        match &link.snap {
            AssocSnap::Start => matches!(self.osnap, 1 | 13),
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
            AssocSnap::OnCircle { angle } => {
                self.osnap == 5 && {
                    let quadrant = angle / std::f64::consts::FRAC_PI_2;
                    (quadrant - quadrant.round()).abs() < 1e-9
                }
            }
            AssocSnap::Intersection { other } => self.osnap == 6 && self.other.as_slice() == [*other],
        }
    }
}

fn parsed(tags: &[RawTag]) -> Option<(i64, Vec<PointRef>)> {
    let (start, _) = super::super::dimassoc_reference(tags.iter().map(|t| (t.code, t.value.as_str())))?;
    let tail = tags.get(start..)?;
    let integer = |code| tail.iter().find(|t| t.code == code).and_then(|t| t.value.trim().parse::<i64>().ok());
    if integer(70).unwrap_or(0) != 0 || integer(71).unwrap_or(0) != 0 {
        return None;
    }
    let starts: Vec<_> = tail.iter().enumerate().filter(|(_, t)| t.code == 1).map(|(i, _)| i).take(65).collect();
    if starts.is_empty() || starts.len() > 64 {
        return None;
    }
    let chunks: Option<Vec<_>> = starts
        .iter()
        .enumerate()
        .map(|(i, start)| {
            if !tail.get(*start)?.value.trim().eq_ignore_ascii_case("AcDbOsnapPointRef") {
                return None;
            }
            tail.get(start + 1..starts.get(i + 1).copied().unwrap_or(tail.len()))
        })
        .collect();
    let chunks = chunks?;
    let mut cursor = 0;
    let mut refs = Vec::new();
    while cursor < chunks.len() {
        refs.push(PointRef::parse(&chunks, &mut cursor, 0)?);
    }
    Some((integer(90).unwrap_or(0), refs))
}

pub(super) fn bindings(d: &Drawing, dm: &Dimension, tags: &[RawTag]) -> Vec<Binding> {
    let fallback = || {
        vec![Binding {
            evaluable: false,
            targets: tags.iter().filter(|t| matches!(t.code, 331 | 332)).filter_map(|t| Handle::parse_hex(&t.value)).collect(),
        }]
    };
    let Some((flags, refs)) = parsed(tags) else { return fallback() };
    if matches!(dm.kind, DimKind::Radius | DimKind::Diameter) {
        if refs.len() != 1 {
            return fallback();
        }
        return refs
            .iter()
            .map(|r| Binding {
                evaluable: matches!(r.osnap, 3 | 4)
                    && r.last.is_none()
                    && r.path.len() == 1
                    && dm.assoc.len() == 2
                    && dm.assoc.iter().all(|a| {
                        r.path.as_slice() == [a.handle] && crate::association_point(a, |h| d.entity_including_blocks(h).map(|e| e.as_ref())).is_some()
                    }),
                targets: r.targets(),
            })
            .collect();
    }
    let slots: &[&str] = match dm.kind {
        DimKind::Linear { .. } | DimKind::Aligned | DimKind::ArcLength => &["p13", "p14"],
        DimKind::Angular => &["p13", "p14", "defpt", "p15"],
        DimKind::Angular3P => &["p13", "p14", "p15"],
        DimKind::Ordinate { .. } => &["p13"],
        DimKind::Radius | DimKind::Diameter => &[],
    };
    let used: Vec<_> = slots.iter().enumerate().filter(|(i, _)| flags & (1 << i) != 0).map(|(_, s)| *s).collect();
    if refs.len() != used.len() {
        return fallback();
    }
    refs.iter()
        .zip(used)
        .map(|(r, point)| Binding { evaluable: dm.assoc.iter().find(|a| a.point == point).is_some_and(|a| r.matches(d, a)), targets: r.targets() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::supported_osnap;
    #[test]
    fn autodesk_osnap_table_excludes_perpendicular_and_nearest() {
        assert!(!supported_osnap(8), "8 is Perpendicular");
        assert!(!supported_osnap(10), "10 is Nearest");
        for code in [1, 2, 3, 4, 5, 6, 13] {
            assert!(supported_osnap(code));
        }
    }
}
