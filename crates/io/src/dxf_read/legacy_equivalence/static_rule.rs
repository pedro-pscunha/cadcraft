use crate::activation::Family;
use cadcraft_dxf::{Tag, Value};

fn number(tag: &Tag) -> Option<f64> {
    match &tag.value {
        Value::Real(value) => value.is_finite().then_some(*value),
        Value::Int(value) => Some(*value as f64),
        Value::Bool(value) => Some(f64::from(u8::from(*value))),
        Value::Str(value) => value.trim().parse::<f64>().ok().filter(|value| value.is_finite()),
        Value::Hex(_) => None,
    }
}
fn scalar(tags: &[Tag], code: i32, default: f64) -> Option<f64> {
    tags.iter().find(|tag| tag.code == code).map_or(Some(default), number)
}
fn count(tags: &[Tag], code: i32) -> usize {
    tags.iter().filter(|tag| tag.code == code).count()
}
fn required(tags: &[Tag], codes: &[i32]) -> bool {
    codes.iter().all(|code| count(tags, *code) == 1 && scalar(tags, *code, f64::NAN).is_some())
}
fn count_matches(tags: &[Tag], declared: i32, values: i32) -> bool {
    scalar(tags, declared, -1.0) == Some(count(tags, values) as f64)
}
fn flag(tags: &[Tag], code: i32, allowed: i64) -> bool {
    scalar(tags, code, 0.0).is_some_and(|value| value >= 0.0 && value.fract() == 0.0 && (value as i64) & !allowed == 0)
}
fn all_zero(tags: &[Tag], codes: &[i32]) -> bool {
    tags.iter().filter(|tag| codes.contains(&tag.code)).all(|tag| number(tag) == Some(0.0))
}
fn planar(tags: &[Tag], z: &[i32]) -> bool {
    tags.iter().take_while(|tag| tag.code < 1000).all(|tag| match cadcraft_dxf::kind_of(tag.code) {
        cadcraft_dxf::Kind::Real => number(tag).is_some(),
        cadcraft_dxf::Kind::Int16 | cadcraft_dxf::Kind::Int32 | cadcraft_dxf::Kind::Int64 | cadcraft_dxf::Kind::Bool => {
            number(tag).is_some_and(|value| value.fract() == 0.0)
        }
        _ => true,
    }) && scalar(tags, 210, 0.0) == Some(0.0)
        && scalar(tags, 220, 0.0) == Some(0.0)
        && scalar(tags, 230, 1.0) == Some(1.0)
        && scalar(tags, 39, 0.0) == Some(0.0)
        && all_zero(tags, z)
}

pub(super) fn plain(family: Family, tags: &[Tag], has_followers: bool) -> bool {
    use Family::*;
    if has_followers || !planar(tags, &[30, 31, 32, 33, 34, 35, 36, 231, 232, 233]) {
        return false;
    }
    match family {
        Line => required(tags, &[10, 20, 11, 21]),
        Point => false, // The angle fix has a witness, but the full field contract remains unproved.
        Circle => required(tags, &[10, 20, 40]) && scalar(tags, 40, 0.0).is_some_and(|value| value > 0.0),
        Arc => {
            required(tags, &[10, 20, 40, 50, 51])
                && scalar(tags, 40, 0.0).is_some_and(|value| value > 0.0)
                && [50, 51].iter().all(|code| scalar(tags, *code, -1.0).is_some_and(|value| (0.0..=360.0).contains(&value)))
        }
        Ellipse => false, // Mirror uses a different plane/parameter representation; not yet proved plain.
        Ray | XLine => required(tags, &[10, 20, 11, 21]) && (scalar(tags, 11, 0.0) != Some(0.0) || scalar(tags, 21, 0.0) != Some(0.0)),
        Solid | Trace => required(tags, &[10, 20, 11, 21, 12, 22, 13, 23]),
        Face3d => required(tags, &[10, 20, 11, 21, 12, 22, 13, 23]) && flag(tags, 70, 15),
        LwPolyline => {
            flag(tags, 70, 1 | 128)
                && count_matches(tags, 90, 10)
                && count(tags, 10) == count(tags, 20)
                && count(tags, 10) >= 2
                && count(tags, 91) == 0
                && scalar(tags, 38, 0.0) == Some(0.0)
        }
        Hatch => hatch(tags),
        // These families stay conservative until their structural/edit oracle cases prove a plain subset.
        Polyline | Spline | Leader | Dimension | Text | MText | AttDef | Attrib | Tolerance | Wipeout | MLine | Helix | Image | Table | Insert
        | MLeader | Region | Viewport => false,
    }
}

fn hatch(tags: &[Tag]) -> bool {
    if scalar(tags, 70, 0.0) != Some(1.0)
        || scalar(tags, 71, 0.0) != Some(0.0)
        || scalar(tags, 98, 0.0) != Some(0.0)
        || tags.iter().any(|tag| (450..=470).contains(&tag.code))
        || !count_matches(tags, 91, 92)
        || count(tags, 92) == 0
        || !all_zero(tags, &[97])
    {
        return false;
    }
    let mut cursor = 0;
    while let Some(offset) = tags.get(cursor..).and_then(|rest| rest.iter().position(|tag| tag.code == 92)) {
        let start = cursor + offset;
        let end = tags
            .get(start + 1..)
            .and_then(|rest| rest.iter().position(|tag| matches!(tag.code, 92 | 75)))
            .map_or(tags.len(), |offset| start + 1 + offset);
        let Some(body) = tags.get(start + 1..end) else { return false };
        let Some(flags) = number(&tags[start]).filter(|value| value.fract() == 0.0 && *value >= 0.0 && *value <= 31.0) else { return false };
        if (flags as i64) & 2 != 0 {
            if !count_matches(body, 93, 10) || count(body, 10) != count(body, 20) || !flag(body, 72, 1) || !flag(body, 73, 1) {
                return false;
            }
        } else {
            if !count_matches(body, 93, 72) || count(body, 72) == 0 {
                return false;
            }
            let mut edge = 0;
            while let Some(offset) = body.get(edge..).and_then(|rest| rest.iter().position(|tag| tag.code == 72)) {
                let at = edge + offset;
                let stop = body
                    .get(at + 1..)
                    .and_then(|rest| rest.iter().position(|tag| matches!(tag.code, 72 | 97)))
                    .map_or(body.len(), |offset| at + 1 + offset);
                let Some(fields) = body.get(at + 1..stop) else { return false };
                let valid = match number(&body[at]) {
                    Some(1.0) => required(fields, &[10, 20, 11, 21]),
                    Some(2.0) => {
                        required(fields, &[10, 20, 40, 50, 51, 73]) && flag(fields, 73, 1) && scalar(fields, 40, 0.0).is_some_and(|value| value > 0.0)
                    }
                    Some(3.0) => {
                        required(fields, &[10, 20, 11, 21, 40, 50, 51, 73])
                            && flag(fields, 73, 1)
                            && scalar(fields, 40, 0.0).is_some_and(|value| value > 0.0 && value <= 1.0)
                    }
                    _ => false,
                };
                if !valid {
                    return false;
                }
                edge = stop;
            }
        }
        cursor = end;
    }
    true
}
