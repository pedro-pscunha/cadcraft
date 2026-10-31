//! DXF-owned field contracts. These are not the source-replay snapshot specifications.
use crate::activation::Family;
use cadcraft_dxf::{Tag, Version};

pub(super) struct Policy {
    pub revision: u16,
    pub subclasses: &'static [&'static str],
    pub headerless_groups: &'static [i32],
    pub reason: &'static str,
    pub source_version: Version,
}

impl Policy {
    pub fn accepts(&self, tags: &[Tag]) -> bool {
        if self.source_version < Version::R2000 {
            return false;
        }
        let classed = tags.iter().any(|tag| tag.code == 100);
        if classed && self.subclasses.iter().any(|marker| tags.iter().filter(|tag| tag.code == 100 && tag.str() == *marker).count() != 1) {
            return false;
        }
        if tags.iter().filter(|tag| tag.code == 100 && tag.str() == "AcDbEntity").count() > 1 {
            return false;
        }
        let mut subclass = None;
        for tag in tags.iter().take_while(|tag| tag.code < 1000) {
            if tag.code == 100 {
                let marker = match &tag.value {
                    cadcraft_dxf::Value::Str(marker) => marker.as_str(),
                    _ => return false,
                };
                if marker != "AcDbEntity" && !self.subclasses.contains(&marker) {
                    return false;
                }
                subclass = Some(marker);
            } else if self.headerless_groups.contains(&tag.code) {
                let valid = match subclass {
                    None => !classed,
                    Some("AcDbEntity") => false,
                    Some("AcDbCircle") => !matches!(tag.code, 50 | 51),
                    Some("AcDbArc") => matches!(tag.code, 50 | 51),
                    Some(_) => true,
                };
                if !valid {
                    return false;
                }
            } else if !matches!(
                tag.code,
                5 | 6 | 8 | 48 | 60 | 62 | 67 | 100 | 102 | 284 | 330 | 347 | 348 | 360 | 370 | 380 | 390 | 410 | 420 | 430 | 440
            ) {
                return false;
            }
        }
        true
    }
}

/// Optional zero fields may be omitted without changing their DXF-defined default.
#[cfg(test)]
pub(super) fn omitted_default(family: Family, code: i32) -> Option<(&'static str, &'static str)> {
    match (family, code) {
        (Family::Line | Family::Point | Family::Circle | Family::Arc | Family::Ellipse | Family::Ray | Family::XLine, 30 | 31)
        | (Family::Solid | Family::Trace | Family::Face3d, 30..=33) => Some(("0", "An omitted coordinate Z defaults to zero.")),
        (_, 210 | 220) if policy(family, Version::R2018).headerless_groups.contains(&code) => {
            Some(("0", "The default extrusion has zero X and Y components."))
        }
        (_, 230) if policy(family, Version::R2018).headerless_groups.contains(&code) => Some(("1", "The default extrusion is the positive Z axis.")),
        (_, 39) if policy(family, Version::R2018).headerless_groups.contains(&code) => Some(("0", "An omitted thickness defaults to zero.")),
        (Family::Face3d, 70) => Some(("0", "3DFACE has no invisible edges when group 70 is absent.")),
        (Family::Point, 50) => Some(("0", "POINT's omitted ECS display angle defaults to zero.")),
        (Family::LwPolyline, 38 | 40 | 41 | 42 | 43) => Some(("0", "LWPOLYLINE elevation, widths and each vertex bulge default to zero.")),
        _ => None,
    }
}

/// DXF angular fields are cyclic. Only reference-defined angle fields receive this equivalence.
#[cfg(test)]
pub(super) fn angular_period(family: Family, code: i32) -> Option<f64> {
    match (family, code) {
        (Family::Arc | Family::Hatch, 50 | 51) => Some(360.0),
        (Family::Ellipse, 41 | 42) => Some(std::f64::consts::TAU),
        _ => None,
    }
}

pub(super) fn policy(family: Family, source_version: Version) -> Policy {
    use Family::*;
    let (revision, subclasses, headerless_groups, reason): (u16, &'static [&'static str], &'static [i32], &'static str) = match family {
        Line => (1, &["AcDbLine"], &[10, 20, 30, 11, 21, 31, 39, 210, 220, 230], "WCS endpoints, thickness and extrusion belong to LINE."),
        Point => (1, &["AcDbPoint"], &[10, 20, 30, 39, 50, 210, 220, 230], "Position, display angle, thickness and extrusion belong to POINT."),
        Circle => (1, &["AcDbCircle"], &[10, 20, 30, 40, 39, 210, 220, 230], "The OCS center, radius and plane belong to CIRCLE."),
        Arc => (
            1,
            &["AcDbCircle", "AcDbArc"],
            &[10, 20, 30, 40, 50, 51, 39, 210, 220, 230],
            "ARC inherits the circle fields and adds its endpoint angles.",
        ),
        Ellipse => (
            1,
            &["AcDbEllipse"],
            &[10, 20, 30, 11, 21, 31, 40, 41, 42, 210, 220, 230],
            "The center, major vector, ratio, parameters and normal define ELLIPSE.",
        ),
        Ray => (1, &["AcDbRay"], &[10, 20, 30, 11, 21, 31], "The WCS origin and unit direction define RAY."),
        XLine => (1, &["AcDbXline"], &[10, 20, 30, 11, 21, 31], "The WCS origin and unit direction define XLINE."),
        LwPolyline => (
            1,
            &["AcDbPolyline"],
            &[90, 70, 43, 38, 39, 10, 20, 40, 41, 42, 91, 210, 220, 230],
            "Vertex order, identifiers, widths, bulges and structural flags are LWPOLYLINE data.",
        ),
        Polyline => (
            1,
            &[
                "AcDb2dPolyline",
                "AcDb3dPolyline",
                "AcDbPolygonMesh",
                "AcDbPolyFaceMesh",
                "AcDb2dVertex",
                "AcDb3dPolylineVertex",
                "AcDbPolygonMeshVertex",
                "AcDbPolyFaceMeshVertex",
                "AcDbFaceRecord",
            ],
            &[10, 20, 30, 39, 70, 40, 41, 42, 50, 71, 72, 73, 74, 75, 91, 210, 220, 230],
            "POLYLINE is a complete parent, ordered VERTEX members and SEQEND unit.",
        ),
        Solid | Trace => (
            1,
            &["AcDbTrace"],
            &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 39, 210, 220, 230],
            "Four OCS vertices and the plane define SOLID/TRACE.",
        ),
        Face3d => {
            (1, &["AcDbFace"], &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 70], "Four WCS vertices and invisible-edge flags define 3DFACE.")
        }
        Spline => (
            1,
            &["AcDbSpline"],
            &[70, 71, 72, 73, 74, 42, 43, 44, 12, 22, 32, 13, 23, 33, 40, 41, 10, 20, 30, 11, 21, 31, 210, 220, 230],
            "SPLINE counts, tolerances, knots, weights, controls, fits and tangents are structural data.",
        ),
        Leader => (
            2,
            &["AcDbLeader"],
            &[3, 71, 72, 73, 74, 75, 76, 10, 20, 30, 40, 41, 210, 220, 230, 211, 221, 231, 212, 222, 232, 213, 223, 233, 340],
            "LEADER includes annotation links, offsets, basis, dimensions and ordered vertices.",
        ),
        Dimension => (
            1,
            &[
                "AcDbDimension",
                "AcDbAlignedDimension",
                "AcDbRotatedDimension",
                "AcDbRadialDimension",
                "AcDbDiametricDimension",
                "AcDb2LineAngularDimension",
                "AcDb3PointAngularDimension",
                "AcDbOrdinateDimension",
                "AcDbArcDimension",
                "AcDbRadialDimensionLarge",
            ],
            &[
                2, 3, 10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 14, 24, 34, 15, 25, 35, 16, 26, 36, 70, 71, 72, 73, 74, 75, 280, 1, 40, 41, 42,
                50, 51, 52, 53, 54, 210, 220, 230,
            ],
            "DIMENSION uses mixed WCS/OCS fields and subtype-owned definition points.",
        ),
        Text => (
            1,
            &["AcDbText"],
            &[1, 7, 10, 20, 30, 11, 21, 31, 39, 40, 41, 50, 51, 71, 72, 73, 210, 220, 230],
            "Both AcDbText occurrences, alignment, generation flags and the OCS frame are retained.",
        ),
        MText => (
            1,
            &["AcDbMText"],
            &[
                1, 3, 7, 10, 20, 30, 11, 21, 31, 40, 41, 42, 43, 44, 45, 46, 48, 49, 50, 63, 71, 72, 73, 75, 76, 78, 79, 90, 210, 220, 230, 420, 421,
                441,
            ],
            "MTEXT includes direction, columns, attachment, flow and background fields.",
        ),
        AttDef | Attrib => (
            1,
            &["AcDbText", "AcDbAttribute", "AcDbAttributeDefinition"],
            &[1, 2, 3, 7, 10, 20, 30, 11, 21, 31, 39, 40, 41, 46, 50, 51, 70, 71, 72, 73, 74, 280, 210, 220, 230, 101],
            "Attribute text, modes, plane and embedded MTEXT belong to the attribute record.",
        ),
        Tolerance => (1, &["AcDbFcf"], &[1, 3, 10, 20, 30, 11, 21, 31, 210, 220, 230], "TOLERANCE stores a WCS point, direction and plane."),
        Hatch => (
            3,
            &["AcDbHatch"],
            &[
                2, 10, 20, 30, 11, 21, 40, 41, 42, 43, 44, 45, 46, 47, 49, 50, 51, 52, 53, 63, 70, 71, 72, 73, 74, 75, 76, 77, 78, 91, 92, 93, 94,
                95, 96, 97, 98, 99, 210, 220, 230, 330, 450, 451, 452, 453, 460, 461, 462, 463, 470, 421,
            ],
            "HATCH owns complete nested loops, exact edges, pattern, gradient and seed data.",
        ),
        Wipeout => (
            2,
            &["AcDbWipeout"],
            &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 70, 71, 90, 91, 14, 24, 280, 281, 282, 283, 290, 340, 360],
            "WIPEOUT owns its full image basis and ordered clipping boundary.",
        ),
        MLine => (
            2,
            &["AcDbMline"],
            &[2, 10, 20, 30, 11, 21, 31, 12, 22, 32, 40, 41, 42, 70, 71, 72, 73, 74, 75, 210, 220, 230, 340],
            "MLINE includes segment directions and source miters, not only vertex positions.",
        ),
        Helix => (
            1,
            &["AcDbSpline", "AcDbHelix"],
            &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 40, 41, 42, 43, 44, 70, 71, 72, 73, 74, 90, 91, 210, 220, 230, 280, 290],
            "HELIX includes the spline prefix and its spatial axis and constraint fields.",
        ),
        Image => (
            1,
            &["AcDbRasterImage"],
            &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 14, 24, 70, 71, 90, 91, 280, 281, 282, 283, 290, 340, 360],
            "IMAGE stores independent WCS U/V basis vectors and image/clipping references.",
        ),
        Table => (
            1,
            &["AcDbBlockReference", "AcDbTable"],
            &[
                1, 2, 3, 7, 10, 20, 30, 11, 21, 31, 40, 41, 42, 43, 44, 45, 46, 50, 51, 52, 53, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74,
                75, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 140, 141, 142, 143, 144, 145, 146, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 210,
                220, 230, 280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303,
                304, 305, 306, 307, 308, 309, 310, 340, 341, 342, 343, 344, 345, 346, 347, 348, 349, 350,
            ],
            "TABLE includes nested cell content, rotations, merges, styles, row heights and columns.",
        ),
        Insert => (
            1,
            &["AcDbBlockReference", "AcDbMInsertBlock"],
            &[2, 10, 20, 30, 41, 42, 43, 44, 45, 50, 66, 70, 71, 210, 220, 230],
            "INSERT/MINSERT includes reference-local basis, arrays and complete attribute followers.",
        ),
        MLeader => (
            1,
            &["AcDbMLeader"],
            &[
                10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 14, 24, 34, 15, 25, 35, 16, 26, 36, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 90, 91,
                92, 93, 94, 95, 140, 141, 142, 143, 144, 145, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 271, 272, 273, 290, 291, 292, 293,
                294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 304, 305, 340, 341, 342, 343, 344, 345,
            ],
            "MULTILEADER is a nested context, leader and annotation structure.",
        ),
        Region => (
            1,
            &["AcDbModelerGeometry", "AcDbRegion"],
            &[1, 3, 70, 290, 2, 350],
            "REGION modeler data is opaque; the activation policy never enables its canonical family.",
        ),
        Viewport => (
            1,
            &["AcDbViewport"],
            &[
                10, 20, 30, 12, 22, 13, 23, 14, 24, 15, 25, 16, 26, 36, 17, 27, 37, 40, 41, 42, 43, 44, 45, 50, 51, 61, 63, 68, 69, 71, 72, 74, 79,
                90, 110, 120, 130, 111, 121, 131, 112, 122, 132, 141, 142, 143, 144, 145, 146, 170, 281, 282, 283, 284, 285, 286, 292, 331, 332, 333,
                334, 335, 336, 337, 338, 339, 340, 341, 342, 343, 344, 345, 346, 347, 348, 349, 361,
            ],
            "VIEWPORT includes camera, UCS, clipping and frozen-layer state; it never activates.",
        ),
    };
    Policy { revision, subclasses, headerless_groups, reason, source_version }
}
