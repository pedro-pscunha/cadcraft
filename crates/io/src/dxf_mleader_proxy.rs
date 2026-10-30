//! Replay source MULTILEADER graphics only while their geometry and display dependencies agree.
use cadcraft_doc::{Drawing, Entity, EntityKind, MAX_BLOCK_DEPTH, RawTag, foreign::ProxyGraphics, geometry_key};
use cadcraft_dxf::{Tag, Version};
use std::collections::HashSet;
use std::fmt::Write;
use std::hash::{DefaultHasher, Hasher};
use std::sync::Arc;

struct Signature(DefaultHasher);
impl Write for Signature {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0.write(value.as_bytes());
        Ok(())
    }
}
fn dependencies(kind: &EntityKind) -> Vec<String> {
    match kind {
        EntityKind::Insert(insert) => vec![insert.block.clone()],
        EntityKind::MLeader(leader) => leader.block.iter().map(|b| b.block.clone()).chain(std::iter::once(leader.arrow_block.clone())).collect(),
        EntityKind::Dimension(dimension) => dimension.block.iter().cloned().collect(),
        _ => Vec::new(),
    }
}
fn display_key(d: &Drawing, entity: &Entity) -> Option<u64> {
    let EntityKind::MLeader(leader) = &entity.kind else { return None };
    let mut key = Signature(DefaultHasher::new());
    // Geometry keys deliberately exclude text values and styles. A graphics cache cannot exclude them.
    let _ = write!(
        key,
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        leader,
        entity.geometry_revision,
        entity.common,
        d.header,
        d.layers,
        d.linetypes,
        d.text_styles,
        d.dim_styles,
        d.mleader_styles,
        d.table_styles,
        d.mline_styles
    );
    let mut pending: Vec<_> = dependencies(&entity.kind).into_iter().map(|name| (name, 0)).collect();
    for style in &d.mleader_styles {
        pending.extend([style.arrow_block.clone(), style.block.clone()].into_iter().map(|name| (name, 0)));
    }
    for style in &d.dim_styles {
        pending.extend(
            [style.arrow_block.clone(), style.arrow_block1.clone(), style.arrow_block2.clone(), style.leader_arrow_block.clone()]
                .into_iter()
                .map(|name| (name, 0)),
        );
    }
    let mut seen = HashSet::new();
    let mut remaining = 1_000_000usize;
    while let Some((name, depth)) = pending.pop() {
        if depth > MAX_BLOCK_DEPTH {
            return None;
        }
        let Some(block) = d.block(&name) else { continue };
        if !seen.insert(Arc::as_ptr(block)) {
            continue;
        }
        remaining = remaining.checked_sub(block.entities.len())?;
        let _ = write!(key, "{:?}{:?}", block.name, block.content_key());
        for child in block.entities.iter() {
            pending.extend(dependencies(&child.kind).into_iter().map(|name| (name, depth + 1)));
        }
    }
    Some(key.0.finish())
}
pub(crate) fn capture_all<'a>(d: &Drawing, records: impl Iterator<Item = (&'a str, &'a [Tag])>, foreign: &mut cadcraft_doc::foreign::Foreign) {
    for (_, tags) in records.filter(|(kind, _)| matches!(*kind, "MULTILEADER" | "MLEADER")) {
        if let Some(handle) = tags.iter().find(|tag| tag.code == 5).and_then(|tag| cadcraft_doc::Handle::parse_hex(&tag.str()))
            && let Some(entity) = d.entity_including_blocks(handle)
            && let Some(proxy) = capture(d, entity, tags)
        {
            foreign.entities.entry(handle).or_default().proxy_graphics = Some(proxy);
        }
    }
}
fn valid_cache(tags: &[RawTag]) -> bool {
    let mut counts = tags.iter().filter(|tag| matches!(tag.code, 92 | 160));
    let Some(declared) = counts.next().and_then(|tag| tag.value.trim().parse::<usize>().ok()).filter(|count| *count > 0) else { return false };
    if counts.next().is_some() {
        return false;
    }
    let size = tags.iter().filter(|tag| tag.code == 310).try_fold(0usize, |size, tag| {
        let hex = tag.value.trim();
        if !hex.len().is_multiple_of(2) || hex.len() / 2 > usize::from(u8::MAX) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        size.checked_add(hex.len() / 2)
    });
    size == Some(declared)
}
fn capture(d: &Drawing, entity: &Entity, source: &[Tag]) -> Option<ProxyGraphics> {
    let start = source.iter().position(|tag| tag.code == 100 && tag.str() == "AcDbEntity")? + 1;
    let mut common: Vec<_> = source.iter().skip(start).take_while(|tag| tag.code != 100 && tag.code < 1000).collect();
    if !common.iter().any(|tag| matches!(tag.code, 92 | 160) && tag.i64() > 0) {
        let start = source.iter().position(|tag| tag.code == 100 && tag.str() == "AcDbMLeader")? + 1;
        common = source.iter().skip(start).take_while(|tag| tag.code != 300 && tag.code < 1000).collect();
    }
    if !common.iter().any(|tag| matches!(tag.code, 92 | 160) && tag.i64() > 0) || !common.iter().any(|tag| tag.code == 310) {
        return None;
    }
    let tags: Vec<_> =
        common.into_iter().filter(|tag| matches!(tag.code, 92 | 160 | 310)).map(|tag| RawTag { code: tag.code, value: tag.str() }).collect();
    let display = if valid_cache(&tags) { display_key(d, entity) } else { None };
    Some(ProxyGraphics { geometry: geometry_key(&entity.kind, &entity.common)?, display, tags })
}
pub(crate) fn eligible<'a>(d: &'a Drawing, entity: &Entity) -> Option<&'a ProxyGraphics> {
    let proxy = d.foreign.entities.get(&entity.handle)?.proxy_graphics.as_ref()?;
    if proxy.display.is_none()
        || !valid_cache(&proxy.tags)
        || geometry_key(&entity.kind, &entity.common).as_deref() != Some(proxy.geometry.as_str())
        || display_key(d, entity) != proxy.display
    {
        return None;
    }
    Some(proxy)
}
pub(crate) fn tags(d: &Drawing, entity: &Entity, target: Version) -> Vec<Tag> {
    let Some(proxy) = eligible(d, entity) else { return Vec::new() };
    proxy
        .tags
        .iter()
        .map(|raw| {
            let mut raw = raw.clone();
            if target < Version::R2007 && matches!(raw.code, 92 | 160) {
                raw.code = 92;
            }
            crate::dxf_raw::tag(&raw)
        })
        .collect()
}
