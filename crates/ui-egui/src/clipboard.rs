//! The system clipboard: Cut/Copy/Paste key presses, and the drawing clipboard mirrored as DXF
//! text so it can be pasted into another CADCraft window.
//!
//! egui turns Cmd/Ctrl+C, X and V into `Copy`, `Cut` and `Paste(text)` events instead of key
//! presses, so the clipboard shortcuts are read from those. A `Paste` arrives only when the system
//! clipboard holds text; every copy therefore puts its DXF text there.

use std::hash::{DefaultHasher, Hash, Hasher};

use cadcraft_engine::cmd::clipboard;

use crate::CadApp;

/// egui memory: the clipboard copy last mirrored, and the hash of the text it became.
fn serial_id() -> egui::Id {
    egui::Id::unique("cc_clipboard_serial")
}
fn text_id() -> egui::Id {
    egui::Id::unique("cc_clipboard_text")
}

/// Line endings normalised (pasted text arrives with `\n`), so our own text is recognised.
fn hash(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.replace("\r\n", "\n").hash(&mut h);
    h.finish()
}

/// Put a new copy (COPYCLIP, CUTCLIP, COPYBASE from anywhere) on the system clipboard, once.
pub fn mirror(app: &mut CadApp, ctx: &egui::Context) {
    let Some(src) = &app.session.clipboard_source else { return };
    if src.uid.is_none() || ctx.data(|d| d.get_temp::<u64>(serial_id())) == Some(src.serial) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(serial_id(), src.serial));
    match clipboard::system_text(&app.session) {
        Ok(Some(text)) => {
            ctx.data_mut(|d| d.insert_temp(text_id(), hash(&text)));
            ctx.copy_text(text);
        }
        Ok(None) => {}
        Err(error) => app.session.echo(error.to_string()),
    }
}

/// The commands the Cut/Copy/Paste events of this frame ask for. Pasted DXF text that isn't our
/// own last copy (another CADCraft's) becomes the clipboard first.
pub fn shortcut_commands(app: &mut CadApp, ctx: &egui::Context) -> Vec<&'static str> {
    let (events, shift) = ctx.input(|i| {
        let ev: Vec<egui::Event> =
            i.events.iter().filter(|e| matches!(e, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_))).cloned().collect();
        (ev, i.modifiers.shift)
    });
    let mut out = Vec::new();
    for e in events {
        match e {
            egui::Event::Copy => out.push(if shift { "copybase" } else { "copyclip" }),
            egui::Event::Cut => out.push("cutclip"),
            egui::Event::Paste(text) => {
                let h = hash(&text);
                if ctx.data(|d| d.get_temp::<u64>(text_id())) != Some(h) && clipboard::load_system_text(&mut app.session, &text) {
                    ctx.data_mut(|d| d.insert_temp(text_id(), h));
                }
                out.push(if shift { "pasteblock" } else { "pasteclip" });
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Common, Drawing, Entity, RawCompound, RawRecord, RawTag};
    use cadcraft_engine::{Session, cmd::clipboard::ClipSource};
    use std::sync::Arc;

    #[test]
    fn a_refused_raw_clipboard_copy_reports_the_handle_and_publishes_no_text() {
        let record = RawRecord::new("INSERT".into(), vec![RawTag { code: 5, value: "200".into() }]).unwrap();
        let unit = RawCompound::new(record, vec![], Common::default(), "The source cannot be copied exactly.".into()).unwrap();
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        app.session.clipboard = vec![Entity::from_raw(unit)];
        app.session.clipboard_source = Some(ClipSource { uid: Some(1), defs: Arc::new(Drawing::new_metric()), serial: 1 });
        let context = egui::Context::default();
        let mut output = context.run_ui(Default::default(), |ui| mirror(&mut app, ui.ctx()));
        assert!(app.session.log.iter().any(|line| line.contains("200") && line.contains("read-only")));
        assert!(!output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(_))));
        output.textures_delta.clear();
    }
}
