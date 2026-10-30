use super::{CadApp, Handle, Input, Pos2, Rect, Xf, vec2};

/// A click on grip `g` (idle): Shift+click makes it hot or cold again; a plain click starts
/// editing it, with the other hot grips when it is one of them.
pub(super) fn click_grip(app: &mut CadApp, g: (Handle, usize), shift: bool) {
    let hot = &mut app.canvas.hot_grips;
    if shift {
        match hot.iter().position(|x| *x == g) {
            Some(i) => {
                hot.remove(i);
            }
            None => hot.push(g),
        }
        return;
    }
    if !hot.contains(&g) {
        hot.clear();
    }
    hot.retain(|x| *x != g);
    hot.insert(0, g);
    let hs = hot.clone();
    if let Err(e) = app.session.start_grip_edit(&hs) {
        app.canvas.hot_grips.clear();
        app.session.echo(e.to_string());
    }
}

/// The multifunctional options of grip `g` (Add Vertex, Lengthen…; empty: Stretch only).
pub(super) fn grip_options(app: &CadApp, (h, i): (Handle, usize)) -> Vec<cadcraft_engine::grips::GripAction> {
    let Ok(st) = app.session.state() else { return Vec::new() };
    st.doc.entity(h).map(|e| cadcraft_engine::grips::grip_actions(&e.kind, i)).unwrap_or_default()
}

/// The menu of a multifunctional grip: Stretch and its options. A choice makes the grip hot and
/// runs the option at the grip prompt, as typing its keyword would.
pub(super) fn grip_menu(app: &mut CadApp, resp: &egui::Response) {
    let Some(g) = app.canvas.grip_menu else { return };
    let options = grip_options(app, g);
    let mut chosen: Option<Option<cadcraft_engine::grips::GripAction>> = None;
    let open = resp
        .context_menu(|ui| {
            let vertex = options.contains(&cadcraft_engine::grips::GripAction::RemoveVertex);
            if ui.button(crate::i18n::t(if vertex { "Stretch Vertex" } else { "Stretch" })).clicked() {
                chosen = Some(None);
                ui.close();
            }
            for a in &options {
                if ui.button(crate::i18n::t(a.label())).clicked() {
                    chosen = Some(Some(*a));
                    ui.close();
                }
            }
        })
        .is_some();
    if !open || chosen.is_some() {
        app.canvas.grip_menu = None;
    }
    let Some(choice) = chosen else { return };
    app.canvas.hot_grips = vec![g];
    let r = app.session.start_grip_edit(&[g]).and_then(|()| match choice {
        Some(a) => app.session.input(Input::Keyword(a.keyword().to_string())),
        None => Ok(()),
    });
    if let Err(e) = r {
        app.canvas.hot_grips.clear();
        app.session.echo(e.to_string());
    }
}

/// Polyline segment midpoint grips are rectangular; vertex grips remain square.
pub(super) fn grip_rect(kind: &cadcraft_doc::EntityKind, count: usize, index: usize, center: Pos2, size: f32) -> Rect {
    // The grip list interleaves vertices and available segment midpoints, then any remaining vertices.
    let midpoint = matches!(kind, cadcraft_doc::EntityKind::LwPolyline(p) if index % 2 == 1 && index / 2 < count.saturating_sub(p.vertices.len()));
    Rect::from_center_size(center, vec2(size * 2.0, if midpoint { size } else { size * 2.0 }))
}

/// The grip of a selected object under the cursor, if any: (object, grip index).
pub(super) fn grip_at(app: &CadApp, xf: &Xf, hover: Option<Pos2>) -> Option<(Handle, usize)> {
    let hp = hover?;
    let st = app.session.state().ok()?;
    let gs = app.session.settings.gripsize as f32;
    let sel = app.session.selection();
    if !app.session.shows_grips(sel.len()) {
        return None;
    }
    for h in sel.iter().take(200) {
        let e = st.doc.entity(*h)?;
        if st.doc.layer(&e.common.layer).is_some_and(|l| l.locked) {
            continue;
        }
        let Ok(grips) = app.session.grips_with_world(*h) else { continue };
        let count = grips.len();
        for (i, g) in grips.into_iter().enumerate() {
            let sp = xf.to_screen(g.world.map_or(g.point, |world| world.xy()));
            if grip_rect(&e.kind, count, i, sp, gs).expand(3.0).contains(hp) {
                return Some((*h, i));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Circle, Entity, EntityKind, GeometryRevision};
    use cadcraft_engine::Session;
    use cadcraft_geom::{Vec2, Vec3};

    fn selected_circle(revision: GeometryRevision, normal: Vec3) -> (CadApp, Handle, Xf) {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        let drawing = app.session.doc_mut().unwrap();
        let handle = drawing.new_handle();
        let mut entity = Entity::new(handle, EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }));
        entity.geometry_revision = revision;
        entity.common.extrusion = normal;
        drawing.model.push(entity);
        app.session.set_selection(vec![handle]);
        let xf = Xf { rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 800.0)), center: Vec2::ZERO, scale: 50.0 };
        (app, handle, xf)
    }

    #[test]
    fn canonical_grip_hit_testing_uses_the_world_center() {
        let (app, handle, xf) = selected_circle(GeometryRevision::CanonicalV2, Vec3::new(0.0, 3.0, 4.0));
        assert_eq!(grip_at(&app, &xf, Some(xf.to_screen(Vec2::new(-1.0, 1.4)))), Some((handle, 0)));
        assert_eq!(grip_at(&app, &xf, Some(xf.to_screen(Vec2::new(1.0, 2.0)))), None);
    }

    #[test]
    fn the_canvas_paints_a_canonical_grip_at_its_world_center() {
        let (mut app, handle, _) = selected_circle(GeometryRevision::CanonicalV2, Vec3::new(0.0, 3.0, 4.0));
        app.session.zoom_extents().unwrap();
        let ctx = egui::Context::default();
        app.logic(&ctx);
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| app.ui(ui));
        output.textures_delta.clear();
        let xf = app.canvas.xf.unwrap();
        let at = xf.to_screen(Vec2::new(-1.0, 1.4));
        let drawing = app.session.doc().unwrap();
        let kind = &drawing.entity(handle).unwrap().kind;
        let expected = grip_rect(kind, 5, 0, at, app.session.settings.gripsize as f32);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.rect == expected && rect.fill != egui::Color32::TRANSPARENT)),
            "the canonical center grip must be painted at {expected:?}"
        );
    }

    #[test]
    fn an_invalid_canonical_plane_has_no_projected_grip_fallback() {
        let (app, _, xf) = selected_circle(GeometryRevision::CanonicalV2, Vec3::ZERO);
        assert_eq!(grip_at(&app, &xf, Some(xf.to_screen(Vec2::new(1.0, 2.0)))), None);
    }

    #[test]
    fn legacy_grip_hit_testing_keeps_the_projected_center() {
        let (app, handle, xf) = selected_circle(GeometryRevision::LegacyProjectedV1, Vec3::new(0.0, 3.0, 4.0));
        assert_eq!(grip_at(&app, &xf, Some(xf.to_screen(Vec2::new(1.0, 2.0)))), Some((handle, 0)));
    }
}
