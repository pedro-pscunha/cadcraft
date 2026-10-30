//! Persistent limitations of raw display, independent of command-log visibility.
use super::{Rect, Stroke, Tokens, Xf, vec2};
use cadcraft_render::{GeometryRefusal, UnresolvedRecord};
use std::collections::BTreeMap;

const MAX_MARKERS: usize = 4096;
pub(super) fn summary<'a>(records: impl Iterator<Item = &'a UnresolvedRecord>) -> Vec<GeometryRefusal> {
    let mut unique = BTreeMap::new();
    for record in records {
        unique.entry(record.handle).or_insert(record.reason.as_str());
    }
    unique.into_iter().map(|(handle, reason)| GeometryRefusal { handle, reason: reason.into() }).collect()
}
pub(super) fn markers(painter: &egui::Painter, xf: &Xf, records: &[UnresolvedRecord]) {
    let color = Tokens::get().warn;
    let visible = records.iter().filter_map(|record| {
        let at = xf.to_screen(record.location);
        if !at.is_finite() || !painter.clip_rect().expand(6.0).contains(at) {
            return None;
        }
        let clip = record
            .clip
            .map(|clip| Rect::from_two_pos(xf.to_screen(clip.min), xf.to_screen(clip.max)))
            .unwrap_or(painter.clip_rect())
            .intersect(painter.clip_rect());
        clip.expand(6.0).contains(at).then_some((at, clip))
    });
    for (at, clip) in visible.take(MAX_MARKERS) {
        let painter = painter.with_clip_rect(clip);
        let rect = Rect::from_center_size(at, vec2(12.0, 12.0));
        let stroke = Stroke::new(1.5, color);
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
        painter.line_segment([rect.left_bottom(), rect.right_top()], stroke);
    }
}
pub(super) fn indicator(ui: &mut egui::Ui, rect: Rect, records: &[GeometryRefusal], tokens: Tokens) {
    if records.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("unresolved_display")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + vec2(12.0, 36.0)).show(
        ui.ctx(),
        |ui| {
            egui::Frame::popup(ui.style()).stroke(Stroke::new(1.0, tokens.warn)).show(ui, |ui| {
                ui.set_max_width(360.0);
                let message = if records.len() == 1 {
                    crate::tl!("1 record not displayed exactly").to_string()
                } else {
                    crate::tf!("{count} records not displayed exactly", count = records.len())
                };
                ui.colored_label(tokens.warn, message);
                ui.label(crate::tl!("Raw records are excluded from extents."));
                egui::ScrollArea::vertical().id_salt("unresolved_handles").max_height(100.0).show_rows(ui, 18.0, records.len(), |ui, range| {
                    for index in range {
                        if let Some(record) = records.get(index) {
                            ui.label(record.handle.hex()).on_hover_text(&record.reason);
                        }
                    }
                });
                if records.len() > MAX_MARKERS {
                    ui.label(crate::tf!("Placeholder markers are limited to {count} per view.", count = MAX_MARKERS));
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(label) => out.push_str(label.galley.text()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text(shape, out);
                }
            }
            _ => {}
        }
    }
    #[test]
    fn indicator_uses_singular_for_one_record_and_plural_for_two() {
        for count in [1, 2] {
            let records: Vec<_> =
                (0..count).map(|i| GeometryRefusal { handle: cadcraft_doc::Handle(0xA0 + i), reason: "unresolved".into() }).collect();
            let context = egui::Context::default();
            for frame in 0..3 {
                let output = context.run_ui(egui::RawInput::default(), |ui| {
                    let rect = ui.max_rect();
                    indicator(ui, rect, &records, Tokens::get());
                });
                if frame == 2 {
                    let mut painted = String::new();
                    for shape in &output.shapes {
                        text(&shape.shape, &mut painted);
                    }
                    let expected = if count == 1 { "1 record not displayed exactly" } else { "2 records not displayed exactly" };
                    assert!(painted.contains(expected), "{painted}");
                    assert!(!painted.contains("1 records not displayed exactly"), "{painted}");
                }
                output.drop_without_applying_deltas();
            }
        }
    }
}
