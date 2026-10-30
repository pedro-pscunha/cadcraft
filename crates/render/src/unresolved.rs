//! Unresolved source records contribute diagnostics and position hints, never geometry or extents.
use super::{Builder, Ctx, UnresolvedRecord};
use cadcraft_doc::{Entity, UnresolvedDisplay};
use cadcraft_geom::{Vec2, Vec3};

pub(crate) fn record(builder: &mut Builder, context: &Ctx, entity: &Entity, raw: UnresolvedDisplay<'_>) {
    let coordinate = |code| match raw.tags.iter().take_while(|tag| tag.code < 1000).find(|tag| tag.code == code) {
        Some(tag) => tag.value.trim().parse::<f64>().ok().filter(|value| value.is_finite()),
        None => Some(0.0),
    };
    let local = coordinate(10).zip(coordinate(20)).zip(coordinate(30)).map(|((x, y), z)| Vec3::new(x, y, z)).unwrap_or(Vec3::ZERO);
    let hint = context.world.apply(local).xy();
    let origin = context.world.apply(Vec3::ZERO).xy();
    let location = if hint.is_finite() {
        hint
    } else if origin.is_finite() {
        origin
    } else {
        Vec2::ZERO
    };
    builder.list.unresolved.push(UnresolvedRecord { handle: entity.handle, reason: raw.reason.into(), location, clip: None });
}
