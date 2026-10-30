//! The canonical reading of CIRCLE, ARC and the 2D polylines (revision `CanonicalV2`): the groups' own OCS coordinates,
//! with no conversion to WCS and no mirroring under a negative normal.
//!
//! Not part of `read_dxf` yet: the reader still reads legacy curves (`entities.rs`) until the Drawing boundary accepts
//! canonical payloads. A caller that activates them sets the `Entity.geometry_revision` of what they return.

use cadcraft_doc::{Arc, Circle, Common, Drawing, EntityKind, Solid};
use cadcraft_dxf::Tag;
use cadcraft_geom::OcsFrame;

use super::common::{T, common};

/// A CIRCLE, ARC, SOLID, TRACE, SPLINE, LEADER, DIMENSION, LINE, POINT, ELLIPSE, 3DFACE, RAY, XLINE, IMAGE or LWPOLYLINE from the groups of a DXF record (without its `0` group), read canonically. `None` for
/// another kind, a normal without direction, a radius that is not a positive finite number, or an LWPOLYLINE that is a
/// CADCraft region.
pub fn canonical_curve_from_groups(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
    let t = T(tags);
    let c = common(&t);
    OcsFrame::new(c.extrusion)?;
    let radius = || Some(t.fd(40, 1.0)).filter(|r| r.is_finite() && *r > 0.0);
    let kind = match kind.to_ascii_uppercase().as_str() {
        "CIRCLE" => EntityKind::Circle(Circle { center: t.p(10), radius: radius()? }),
        "ARC" => EntityKind::Arc(Arc { center: t.p(10), radius: radius()?, start: t.fd(50, 0.0).to_radians(), end: t.fd(51, 360.0).to_radians() }),
        name @ ("SOLID" | "TRACE") => {
            // The fourth corner repeats the third when the record has none: a triangle.
            let corners = [t.p(10), t.p(11), t.p(12), if t.f(13).is_some() { t.p(13) } else { t.p(12) }];
            if name == "SOLID" { EntityKind::Solid(Solid { corners }) } else { EntityKind::Trace(Solid { corners }) }
        }
        "LINE" => EntityKind::Line(cadcraft_doc::Line { a: t.p(10), b: t.p(11) }),
        "POINT" => EntityKind::Point(cadcraft_doc::Point { p: t.p(10), angle: t.fd(50, 0.0).to_radians() }),
        "ELLIPSE" => EntityKind::Ellipse(super::entities::ellipse(&c, &t)),
        name @ ("RAY" | "XLINE") => {
            let ray = cadcraft_doc::RayLine { base: t.p(10), dir: t.p(11) };
            if name == "RAY" { EntityKind::Ray(ray) } else { EntityKind::XLine(ray) }
        }
        // The file path comes from the IMAGEDEF object, as for the legacy reader.
        "IMAGE" => EntityKind::Image(crate::dxf_image::read_entity(tags)),
        "3DFACE" => {
            EntityKind::Face3d(cadcraft_doc::Face3d { corners: [t.p(10), t.p(11), t.p(12), t.p(13)], hidden_edges: t.i(70).unwrap_or(0) as u8 })
        }
        "SPLINE" => EntityKind::Spline(spatial_spline(&t)?),
        "LEADER" => EntityKind::Leader(spatial_leader(&t)?),
        "DIMENSION" => EntityKind::Dimension(spatial_dimension(&t, c.extrusion)),
        "LWPOLYLINE" => {
            // The legacy reader mirrors under a negative Z: read the vertices as if the plane were +Z.
            let k = super::entities::lwpolyline(tags, &Common { extrusion: cadcraft_geom::Vec3::Z, ..c.clone() }, &t)?;
            if !matches!(k, EntityKind::LwPolyline(_)) {
                return None;
            }
            k
        }
        _ => return None,
    };
    Some((c, kind))
}

/// A 2D or 3D POLYLINE or a polygon or polyface mesh from its parent record and the child records that follow it (VERTEX, then SEQEND), each with its groups
/// without the `0` group, read canonically: the vertices stay in the OCS, with the fit data and record identities as
/// the legacy reader keeps them. `None` for a 3D polyline or a mesh, or a normal without direction.
pub fn canonical_polyline_from_groups(parent: &[Tag], children: &[(String, Vec<Tag>)]) -> Option<(Common, EntityKind)> {
    let t = T(parent);
    OcsFrame::new(common(&t).extrusion)?;
    let (mut drawing, rx) = (Drawing::default(), Default::default());
    let (c, kind) = super::entities::polyline_entity_in(children, &mut 0, &t, &mut drawing, &rx, true);
    matches!(kind, EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) | EntityKind::PolylineMesh(_)).then_some((c, kind))
}

/// A SPLINE as a spatial spline: the X and Y values in the planar fields and the Z values beside them, the end tangents
/// as the record holds them. `None` when its counts do not match or the knots do not fit the control points: a canonical
/// spline is never rebuilt from its fit points.
fn spatial_spline(t: &T) -> Option<cadcraft_geom::Spline> {
    use cadcraft_geom::{FitOptions, Spline, SplineSpace, Vec2};
    let points = |code: i32| -> Option<(Vec<Vec2>, Vec<f64>)> {
        let (xs, ys, zs) = (t.all_f(code), t.all_f(code + 10), t.all_f(code + 20));
        (xs.len() == ys.len() && xs.len() == zs.len()).then(|| (xs.into_iter().zip(ys).map(|(x, y)| Vec2::new(x, y)).collect(), zs))
    };
    let (control, control_z) = points(10)?;
    let (fit, fit_z) = points(11)?;
    let tangent = |code: i32| t.f(code).map(|_| t.p(code));
    let (start, end) = (tangent(12), tangent(13));
    let fit_opts =
        FitOptions { knots: Default::default(), start_tangent: start.map(|v| v.xy()), end_tangent: end.map(|v| v.xy()), tolerance: t.fd(44, 0.0) };
    let mut spline = Spline {
        degree: usize::try_from(t.i(71).unwrap_or(3)).ok()?,
        knots: t.all_f(40),
        control,
        weights: t.all_f(41),
        fit,
        closed: t.i(70).unwrap_or(0) & 1 != 0,
        fit_opts,
        space: Some(SplineSpace { control: control_z, fit: fit_z, start_tangent: start.map_or(0.0, |v| v.z), end_tangent: end.map_or(0.0, |v| v.z) }),
    };
    if !spline.is_valid() || !spline.space_is_consistent() || !(spline.weights.is_empty() || spline.weights.len() == spline.control.len()) {
        return None;
    }
    // DXF keeps the knots, not how they were spaced: recover it for later refits.
    if spline.fit.len() >= 3 {
        spline.fit_opts.knots = spline.infer_knot_param();
    }
    Some(spline)
}

/// A LEADER with its vertices as WCS points (their Z kept), and its direction, offsets and annotation size as the record
/// holds them. `None` when the vertex coordinates do not match in count.
fn spatial_leader(t: &T) -> Option<cadcraft_doc::Leader> {
    use cadcraft_doc::{Leader, LeaderFrame};
    use cadcraft_geom::Vec3;
    let (xs, ys, zs) = (t.all_f(10), t.all_f(20), t.all_f(30));
    if xs.len() != ys.len() || xs.len() != zs.len() {
        return None;
    }
    let vector = |code: i32, default: Vec3| if t.f(code).is_some() { t.p(code) } else { default };
    Some(Leader {
        vertices: xs.into_iter().zip(ys).zip(zs).map(|((x, y), z)| Vec3::new(x, y, z)).collect(),
        arrow: t.i(71).unwrap_or(1) != 0,
        spline: t.i(72).unwrap_or(0) != 0,
        style: t.s(3).unwrap_or_else(|| "Standard".into()),
        frame: Some(LeaderFrame {
            direction: vector(211, Vec3::new(1.0, 0.0, 0.0)),
            block_offset: vector(212, Vec3::ZERO),
            annotation_offset: vector(213, Vec3::ZERO),
            text_height: t.fd(40, 0.0),
            text_width: t.fd(41, 0.0),
        }),
    })
}

/// A DIMENSION whose OCS text midpoint and arc point are kept as the record holds them, with its oblique angle (52) in the
/// OCS of the plane. Associations, overrides and the marks of CADCraft's extended data are read by the caller with the
/// legacy extras (`entity_extras`), which must not recompute the oblique angle of a canonical dimension.
fn spatial_dimension(t: &T, normal: cadcraft_geom::Vec3) -> cadcraft_doc::Dimension {
    use cadcraft_doc::DimKind;
    let mut dm = super::entities::dimension_from(t, None);
    let oblique = t.f(52).filter(|o| o.is_finite() && o.rem_euclid(180.0) != 0.0);
    let base = match dm.kind {
        DimKind::Linear { rotation } => Some(rotation),
        DimKind::Aligned => OcsFrame::new(normal).map(|f| (f.to_ocs(dm.p14) - f.to_ocs(dm.p13)).xy().angle()),
        _ => None,
    };
    if let (Some(base), Some(o)) = (base, oblique) {
        dm.marks.oblique = Some(cadcraft_geom::norm_angle(base + o.to_radians()));
    }
    dm
}
