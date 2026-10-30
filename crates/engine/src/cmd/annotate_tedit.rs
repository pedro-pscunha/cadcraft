//! DIMTEDIT at the command line: "Select dimension", then a new text location or
//! Left/Right/Center/Home/Angle. Each answer runs the JSON form on the picked dimension.

use cadcraft_doc::{Entity, EntityKind, Handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{dims_of, run_dimtedit, v3};
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

fn legacy_dimension(s: &Session, h: Handle) -> Result<&Entity> {
    let entity = s.doc()?.entity(h).ok_or_else(|| EngineError::Other(format!("no such object {}", h.hex())))?;
    entity
        .require_legacy_geometry()
        .map_err(|error| EngineError::Disabled("dimtedit".into(), format!("dimension {}: {error} Nothing was changed", h.hex())))?;
    Ok(entity)
}

#[derive(Default)]
pub(super) struct TEditM {
    dim: Option<Handle>,
    asking_angle: bool,
}

impl TEditM {
    /// Align Text > Angle: select the dimension (or use the preselected one), then ask for the angle.
    pub(super) fn angle() -> Self {
        Self { dim: None, asking_angle: true }
    }

    fn run(&self, s: &mut Session, mut p: Value) -> Result<Step> {
        let Some(h) = self.dim else { return Ok(Step::Cancel) };
        if let Some(o) = p.as_object_mut() {
            o.insert("handles".into(), json!([h.hex()]));
        }
        run_dimtedit(s, &p)?;
        Ok(Step::Done)
    }
}

impl Interactive for TEditM {
    fn name(&self) -> &'static str {
        "DIMTEDIT"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        // A single preselected dimension is taken at once.
        if s.settings.pickfirst
            && let [h] = dims_of(s, &s.selection())[..]
        {
            legacy_dimension(s, h)?;
            self.dim = Some(h);
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.dim.is_none() {
            Prompt::new("Select dimension", Accept::SELECT)
        } else if self.asking_angle {
            Prompt::new("Specify angle for dimension text", Accept::NUMBER)
        } else {
            Prompt::new("Specify new location for dimension text", Accept::POINT).kw(&["Left", "Right", "Center", "Home", "Angle"])
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.dim.is_none() {
            return Ok(match i {
                Input::Pick(hs) => {
                    match dims_of(s, &hs).first() {
                        Some(h) => {
                            legacy_dimension(s, *h)?;
                            self.dim = Some(*h);
                        }
                        None if hs.is_empty() => {}
                        None => s.echo("Object selected is not a dimension."),
                    }
                    Step::Continue
                }
                Input::Enter | Input::Cancel => Step::Cancel,
                _ => Step::Continue,
            });
        }
        if self.asking_angle {
            return match i {
                Input::Text(t) => match s.angle_settings().direction(&t) {
                    Some(a) => self.run(s, json!({ "mode": "angle", "angle": a.to_degrees() })),
                    None => {
                        s.echo("Requires a valid angle.");
                        Ok(Step::Continue)
                    }
                },
                Input::Enter | Input::Cancel => Ok(Step::Cancel),
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Point(p) => self.run(s, json!({ "at": [p.x, p.y] })),
            Input::Keyword(k) if k == "Angle" => {
                self.asking_angle = true;
                Ok(Step::Continue)
            }
            Input::Keyword(k) => self.run(s, json!({ "mode": k.to_ascii_lowercase() })),
            Input::Enter | Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    /// The text follows the cursor.
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<Entity> {
        if self.asking_angle {
            return Vec::new();
        }
        let Some(mut entity) = self.dim.and_then(|h| legacy_dimension(s, h).ok().cloned()) else { return Vec::new() };
        let EntityKind::Dimension(d) = &mut entity.kind else { return Vec::new() };
        d.text_mid = v3(c);
        d.user_text_pos = true;
        if let Ok(doc) = s.doc() {
            let st = doc.dim_style(&d.style).cloned().unwrap_or_default().with_overrides(&d.overrides);
            st.follow_text(d, doc.header.f64("DIMSCALE", 1.0));
        }
        d.block = None;
        vec![entity]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::GeometryRevision;

    #[test]
    fn canonical_dimension_admission_returns_a_typed_command_refusal() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut session = Session::new();
        session.script("dimlinear 0,0 10,0 5,3\n")?;
        let handle = session.doc()?.model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).ok_or("dimension")?.handle;
        let mut entity = (**session.doc()?.entity(handle).ok_or("dimension")?).clone();
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        assert!(session.doc_mut()?.model.replace(entity));
        assert!(matches!(legacy_dimension(&session,handle),Err(EngineError::Disabled(command,_)) if command=="dimtedit"));
        Ok(())
    }
}
