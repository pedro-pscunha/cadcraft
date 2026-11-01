//! Shared preservation assertions for ordered entities, authored blocks and referenced tables.
//! Every field of the explicitly supported entities is checked; converted angles alone allow
//! floating-point roundoff, never wrapping or geometric approximation. Blocks are visited once
//! as stored definitions, not recursively expanded, so cyclic references remain bounded.
//! Header defaults, generated objects and other drawing collections are outside this slice.

use std::fmt::Debug;

use cadcraft_doc::{Arc, Attrib, Block, Circle, Common, Drawing, Entity, EntityKind, EntityStore, Insert, Line, MText, Text};

// Read-back provenance is absent on an authored fixture: geometry snapshots/revisions,
// ATTRIB identities, POLYLINE child records/heavy representation/fit-record identities,
// WIPEOUT source image frames, SplineSource, MLineVectors, MLeaderContext codes,
// Leader native frame/links, Viewport source status, and MTEXT column extents.
// Clear only the read-side field when
// the authored side is empty. Two populated fields stay comparable, including resave checks.
// Canonical frames, planes, display settings and upstream semantic geometry stay intact.
// Materialize the implicit ATTRIB layer 0 instead of erasing explicit layers. Normalize only
// alpha bytes implied by transparency, default direction 1, and embedded ATTRIB defaults.
fn normalized(expected: &Entity, actual: &Entity) -> (Entity, Entity) {
    let mut expected = expected.clone();
    let mut actual = actual.clone();
    if expected.geometry_revision == Default::default() {
        actual.geometry_revision = Default::default();
    }
    normalize_common(&expected.common, &mut actual.common);
    match (&mut expected.kind, &mut actual.kind) {
        (EntityKind::LwPolyline(a), EntityKind::LwPolyline(b)) => {
            if a.records == Default::default() {
                b.records = Default::default();
            }
            if !a.heavy {
                b.heavy = false;
            }
            if let (Some(a), Some(b)) = (&a.fit, &mut b.fit) {
                for (a, b) in a.vertices.iter().zip(&mut b.vertices) {
                    if a.record == Default::default() {
                        b.record = Default::default();
                    }
                }
            }
        }
        (EntityKind::Polyline3d(a), EntityKind::Polyline3d(b)) => {
            if a.records == Default::default() {
                b.records = Default::default();
            }
            if let (Some(a), Some(b)) = (&a.fit, &mut b.fit) {
                for (a, b) in a.vertices.iter().zip(&mut b.vertices) {
                    if a.record == Default::default() {
                        b.record = Default::default();
                    }
                }
            }
        }
        (EntityKind::Wipeout(a), EntityKind::Wipeout(b)) => {
            if a.image.is_none() {
                b.image = None;
            }
        }
        (EntityKind::Viewport(a), EntityKind::Viewport(b)) => {
            if a.source_status.is_none() {
                b.source_status = None;
            }
        }
        (EntityKind::Spline(a), EntityKind::Spline(b)) => {
            if a.source.is_none() {
                b.source = None;
            }
        }
        (EntityKind::MLine(a), EntityKind::MLine(b)) => {
            for (a, b) in a.vertices.iter().zip(&mut b.vertices) {
                if a.vectors.is_none() {
                    b.vectors = None;
                }
            }
        }
        (EntityKind::Leader(a), EntityKind::Leader(b)) => {
            if a.frame.is_none() {
                b.frame = None;
            }
            if a.links == Default::default() {
                b.links = Default::default();
            }
        }
        (EntityKind::MText(a), EntityKind::MText(b)) => normalize_mtext(a, b),
        (EntityKind::AttDef(a), EntityKind::AttDef(b)) => normalize_attrib(a, b, true),
        (EntityKind::Insert(a), EntityKind::Insert(b)) => {
            for (a, b) in a.attribs.iter_mut().zip(&mut b.attribs) {
                normalize_attrib(a, b, false);
            }
        }
        (EntityKind::MLeader(a), EntityKind::MLeader(b)) => {
            if a.context.codes == Default::default() {
                b.context.codes = Default::default();
            }
            if let (Some(a), Some(b)) = (&a.text, &mut b.text) {
                normalize_mtext(a, b);
            }
            if let (Some(a), Some(b)) = (&mut a.block, &mut b.block) {
                for (a, b) in a.attribs.iter_mut().zip(&mut b.attribs) {
                    normalize_attrib(a, b, false);
                }
            }
        }
        _ => {}
    }
    (expected, actual)
}

fn normalize_alpha(transparency: cadcraft_doc::Transparency, expected: Option<u8>, actual: &mut Option<u8>) {
    if expected.is_none() {
        if let cadcraft_doc::Transparency::Percent(percent) = transparency {
            if *actual == Some(cadcraft_doc::Transparency::default_alpha(percent)) {
                *actual = None;
            }
        }
    }
}

fn normalize_common(expected: &Common, actual: &mut Common) {
    if expected.geometry.is_none() {
        actual.geometry = None;
    }
    normalize_alpha(expected.transparency, expected.transparency_alpha, &mut actual.transparency_alpha);
}

pub(crate) fn normalized_columns(
    expected: &Option<cadcraft_doc::MTextColumns>,
    actual: &Option<cadcraft_doc::MTextColumns>,
) -> Option<cadcraft_doc::MTextColumns> {
    let mut actual = actual.clone();
    if let (Some(a), Some(b)) = (expected, &mut actual) {
        if a.extents.is_none() {
            b.extents = None;
        }
    }
    actual
}

fn normalize_mtext(expected: &MText, actual: &mut MText) {
    if expected.drawing_direction.is_none() && actual.drawing_direction == Some(1) {
        actual.drawing_direction = None;
    }
    actual.columns = normalized_columns(&expected.columns, &actual.columns);
}

fn normalize_attrib(expected: &mut Attrib, actual: &mut Attrib, definition: bool) {
    if expected.geometry_revision == Default::default() {
        actual.geometry_revision = Default::default();
    }
    if expected.geometry.is_none() {
        actual.geometry = None;
    }
    if expected.handle.is_none() {
        actual.handle = None;
    }
    if expected.props.layer.is_none() {
        expected.props.layer = Some("0".into());
    }
    if actual.props.layer.is_none() {
        actual.props.layer = Some("0".into());
    }
    if let Some(transparency) = expected.props.transparency {
        normalize_alpha(transparency, expected.props.transparency_alpha, &mut actual.props.transparency_alpha);
    }
    if let (Some(a), Some(b)) = (&expected.multiline, &mut actual.multiline) {
        if a.geometry_revision == Default::default() {
            b.geometry_revision = Default::default();
        }
        if a.embedded_46.is_none() && b.embedded_46 == Some(0.0) {
            b.embedded_46 = None;
        }
        if a.type_72.is_none() && b.type_72 == Some(if definition { 0 } else { 1 }) {
            b.type_72 = None;
        }
        normalize_mtext(&a.mtext, &mut b.mtext);
        // AutoCAD's embedded ATTRIB MTEXT uses by-style direction 5 when none was authored.
        if a.mtext.drawing_direction.is_none() && b.mtext.drawing_direction == Some(5) {
            b.mtext.drawing_direction = None;
        }
    }
}

fn equal<T: PartialEq + Debug>(expected: &T, actual: &T, context: &str, field: &str) {
    assert_eq!(actual, expected, "{context}.{field}");
}

fn angle(expected: f64, actual: f64, context: &str, field: &str) {
    // Two floating-point unit conversions, not a geometric approximation or angle wrapping.
    let tolerance = 8.0 * f64::EPSILON * expected.abs().max(1.0);
    assert!(
        expected.is_finite() && actual.is_finite() && (actual - expected).abs() <= tolerance,
        "{context}.{field}: expected {expected:?}, actual {actual:?}, tolerance {tolerance:e}"
    );
}

pub(super) fn model(expected: &Drawing, actual: &Drawing, context: &str) {
    entities(expected, actual, &expected.model, &actual.model, &format!("{context}.model"));
    for (name, block) in &expected.blocks {
        let path = format!("{context}.block[{name}]");
        let back = actual.block(name).unwrap_or_else(|| panic!("{path}: missing block"));
        let Block {
            name,
            annotative,
            base,
            entities: contents,
            description,
            xref_overlay,
            xref_status,
            anonymous,
            xref_path,
            explodable,
            units,
            array,
            scale_uniformly,
            preview,
        } = block.as_ref();
        equal(name, &back.name, &path, "name");
        equal(annotative, &back.annotative, &path, "annotative");
        equal(base, &back.base, &path, "base");
        equal(description, &back.description, &path, "description");
        equal(xref_overlay, &back.xref_overlay, &path, "xref_overlay");
        equal(xref_status, &back.xref_status, &path, "xref_status");
        equal(anonymous, &back.anonymous, &path, "anonymous");
        equal(xref_path, &back.xref_path, &path, "xref_path");
        equal(explodable, &back.explodable, &path, "explodable");
        equal(units, &back.units, &path, "units");
        equal(array, &back.array, &path, "array");
        equal(scale_uniformly, &back.scale_uniformly, &path, "scale_uniformly");
        equal(preview, &back.preview, &path, "preview");
        entities(expected, actual, contents, &back.entities, &path);
    }
}

pub(super) fn entities(expected: &Drawing, actual: &Drawing, source: &EntityStore, back: &EntityStore, context: &str) {
    equal(&source.len(), &back.len(), context, "len");
    for (index, (a, b)) in source.iter().zip(back.iter()).enumerate() {
        let path = format!("{context}.entity[{index}]");
        entity(a, b, &path);
        layer_reference(expected, actual, &a.common.layer, &path);
        linetype_reference(expected, actual, &a.common.linetype, &path);
        let dimension_style = match &a.kind {
            EntityKind::Dimension(d) => Some(d.style.as_str()),
            EntityKind::Leader(l) => Some(l.style.as_str()),
            EntityKind::Tolerance(t) => Some(t.style.as_str()),
            _ => None,
        };
        if let Some(name) = dimension_style {
            equal(&expected.dim_style(name), &actual.dim_style(name), &path, "referenced_dimension_style");
            if let Some(style) = expected.dim_style(name) {
                equal(&expected.text_style(&style.text_style), &actual.text_style(&style.text_style), &path, "dimension_text_style");
            }
        }
        let style = match &a.kind {
            EntityKind::Text(t) => Some(t.style.as_str()),
            EntityKind::MText(t) => Some(t.style.as_str()),
            EntityKind::MLeader(m) => m.text.as_ref().map(|t| t.style.as_str()),
            _ => None,
        };
        if let Some(style) = style {
            equal(&expected.text_style(style), &actual.text_style(style), &path, "referenced_text_style");
        }
        let attributes: &[Attrib] = match &a.kind {
            EntityKind::AttDef(a) => std::slice::from_ref(a),
            EntityKind::Insert(i) => &i.attribs,
            EntityKind::MLeader(m) => m.block.as_ref().map_or(&[], |i| i.attribs.as_slice()),
            _ => &[],
        };
        for (index, a) in attributes.iter().enumerate() {
            let path = format!("{path}.attrib[{index}]");
            if let Some(name) = &a.props.layer {
                layer_reference(expected, actual, name, &path);
            }
            if let Some(name) = &a.props.linetype {
                linetype_reference(expected, actual, name, &path);
            }
            for style in std::iter::once(a.text.style.as_str()).chain(a.multiline.as_ref().map(|m| m.mtext.style.as_str())) {
                equal(&expected.text_style(style), &actual.text_style(style), &path, "referenced_text_style");
            }
        }
    }
}

fn layer_reference(expected: &Drawing, actual: &Drawing, name: &str, context: &str) {
    let layer = expected.layer(name);
    let mut back = actual.layer(name).cloned();
    if let (Some(a), Some(b)) = (layer, &mut back) {
        normalize_alpha(cadcraft_doc::Transparency::Percent(a.transparency), a.transparency_alpha, &mut b.transparency_alpha);
    }
    equal(&layer.cloned(), &back, context, "referenced_layer");
    if let Some(layer) = layer {
        linetype_reference(expected, actual, &layer.linetype, context);
    }
}

fn linetype_reference(expected: &Drawing, actual: &Drawing, name: &str, context: &str) {
    linetype(expected.linetype(name), actual.linetype(name), &format!("{context}.linetype[{name}]"));
    if let Some(linetype) = expected.linetype(name) {
        for element in &linetype.pattern {
            if let Some(style) = &element.style {
                equal(&expected.text_style(style), &actual.text_style(style), context, "linetype_text_style");
            }
        }
    }
}

fn linetype(expected: Option<&cadcraft_doc::Linetype>, actual: Option<&cadcraft_doc::Linetype>, context: &str) {
    let (Some(a), Some(b)) = (expected, actual) else {
        equal(&expected, &actual, context, "presence");
        return;
    };
    let cadcraft_doc::Linetype { name, description, pattern } = a;
    equal(name, &b.name, context, "name");
    equal(description, &b.description, context, "description");
    equal(&pattern.len(), &b.pattern.len(), context, "pattern.len");
    for (index, (a, b)) in pattern.iter().zip(&b.pattern).enumerate() {
        let path = format!("{context}.element[{index}]");
        let cadcraft_doc::DashElement { length, text, shape, style, scale, rotation, offset, absolute } = a;
        equal(length, &b.length, &path, "length");
        equal(text, &b.text, &path, "text");
        equal(shape, &b.shape, &path, "shape");
        equal(style, &b.style, &path, "style");
        equal(scale, &b.scale, &path, "scale");
        angle(*rotation, b.rotation, &path, "rotation");
        equal(offset, &b.offset, &path, "offset");
        equal(absolute, &b.absolute, &path, "absolute");
    }
}

fn entity(expected: &Entity, actual: &Entity, context: &str) {
    let (expected, actual) = normalized(expected, actual);
    let Entity { handle, common: properties, kind, geometry_revision } = &expected;
    equal(geometry_revision, &actual.geometry_revision, context, "geometry_revision");
    equal(handle, &actual.handle, context, "handle");
    common(properties, &actual.common, &format!("{context}.common"));
    match (kind, &actual.kind) {
        (EntityKind::Line(Line { a, b }), EntityKind::Line(other)) => {
            equal(a, &other.a, context, "line.a");
            equal(b, &other.b, context, "line.b");
        }
        (EntityKind::Circle(Circle { center, radius }), EntityKind::Circle(other)) => {
            equal(center, &other.center, context, "circle.center");
            equal(radius, &other.radius, context, "circle.radius");
        }
        (EntityKind::Text(a), EntityKind::Text(b)) => text(a, b, &format!("{context}.text")),
        (EntityKind::Arc(Arc { center, radius, start, end }), EntityKind::Arc(b)) => {
            equal(center, &b.center, context, "arc.center");
            equal(radius, &b.radius, context, "arc.radius");
            angle(*start, b.start, context, "arc.start");
            angle(*end, b.end, context, "arc.end");
        }
        (EntityKind::MText(a), EntityKind::MText(b)) => mtext(a, b, context),
        (EntityKind::AttDef(a), EntityKind::AttDef(b)) => attrib(a, b, context),
        (EntityKind::Insert(a), EntityKind::Insert(b)) => insert(a, b, context),
        (
            EntityKind::Point(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::Spline(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::Leader(_)
            | EntityKind::Hatch(_)
            | EntityKind::Table(_)
            | EntityKind::Image(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Helix(_)
            | EntityKind::MLine(_)
            | EntityKind::MLeader(_)
            | EntityKind::Region(_)
            | EntityKind::Dimension(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Viewport(_),
            _,
        ) => equal(kind, &actual.kind, context, "kind"),
        _ => panic!("{context}.kind: unsupported or changed type: expected {kind:?}, actual {:?}", actual.kind),
    }
}

pub(crate) fn common(expected: &Common, actual: &Common, context: &str) {
    let mut actual = actual.clone();
    normalize_common(expected, &mut actual);
    let actual = &actual;
    // No `..`: adding a field requires deciding its preservation contract here.
    let Common {
        layer,
        color,
        linetype,
        lineweight,
        ltscale,
        transparency,
        visible,
        thickness,
        extrusion,
        plot_style,
        anno_scales,
        xdata,
        transparency_alpha,
        paper_space,
        geometry,
    } = expected;
    equal(layer, &actual.layer, context, "layer");
    equal(color, &actual.color, context, "color");
    equal(linetype, &actual.linetype, context, "linetype");
    equal(lineweight, &actual.lineweight, context, "lineweight");
    equal(ltscale, &actual.ltscale, context, "ltscale");
    equal(transparency, &actual.transparency, context, "transparency");
    equal(transparency_alpha, &actual.transparency_alpha, context, "transparency_alpha");
    equal(geometry, &actual.geometry, context, "geometry");
    equal(paper_space, &actual.paper_space, context, "paper_space");
    equal(visible, &actual.visible, context, "visible");
    equal(thickness, &actual.thickness, context, "thickness");
    equal(extrusion, &actual.extrusion, context, "extrusion");
    equal(plot_style, &actual.plot_style, context, "plot_style");
    equal(anno_scales, &actual.anno_scales, context, "anno_scales");
    equal(xdata, &actual.xdata, context, "xdata");
}

fn text(expected: &Text, actual: &Text, context: &str) {
    let Text { insert, align_pt, height, value, rotation, width_factor, oblique, style, halign, valign, backward, upside_down } = expected;
    equal(insert, &actual.insert, context, "insert");
    equal(align_pt, &actual.align_pt, context, "align_pt");
    equal(height, &actual.height, context, "height");
    equal(value, &actual.value, context, "value");
    angle(*rotation, actual.rotation, context, "rotation");
    equal(width_factor, &actual.width_factor, context, "width_factor");
    angle(*oblique, actual.oblique, context, "oblique");
    equal(style, &actual.style, context, "style");
    equal(halign, &actual.halign, context, "halign");
    equal(valign, &actual.valign, context, "valign");
    equal(backward, &actual.backward, context, "backward");
    equal(upside_down, &actual.upside_down, context, "upside_down");
}

fn mtext(expected: &MText, actual: &MText, context: &str) {
    let MText { insert, height, width, attach, rotation, style, contents, line_spacing, line_spacing_exact, columns, direction, drawing_direction } =
        expected;
    equal(insert, &actual.insert, context, "mtext.insert");
    equal(height, &actual.height, context, "mtext.height");
    equal(width, &actual.width, context, "mtext.width");
    equal(attach, &actual.attach, context, "mtext.attach");
    angle(*rotation, actual.rotation, context, "mtext.rotation");
    equal(style, &actual.style, context, "mtext.style");
    equal(contents, &actual.contents, context, "mtext.contents");
    equal(line_spacing, &actual.line_spacing, context, "mtext.line_spacing");
    equal(line_spacing_exact, &actual.line_spacing_exact, context, "mtext.line_spacing_exact");
    equal(columns, &actual.columns, context, "mtext.columns");
    equal(direction, &actual.direction, context, "mtext.direction");
    equal(drawing_direction, &actual.drawing_direction, context, "mtext.drawing_direction");
}

fn attrib(expected: &Attrib, actual: &Attrib, context: &str) {
    let Attrib { tag, text: t, invisible, constant, prompt, props, multiline, modes, plane, geometry_revision, handle, geometry } = expected;
    equal(tag, &actual.tag, context, "attrib.tag");
    equal(geometry_revision, &actual.geometry_revision, context, "attrib.geometry_revision");
    equal(handle, &actual.handle, context, "attrib.handle");
    equal(geometry, &actual.geometry, context, "attrib.geometry");
    text(t, &actual.text, context);
    equal(invisible, &actual.invisible, context, "attrib.invisible");
    equal(constant, &actual.constant, context, "attrib.constant");
    equal(prompt, &actual.prompt, context, "attrib.prompt");
    equal(props, &actual.props, context, "attrib.props");
    equal(modes, &actual.modes, context, "attrib.modes");
    equal(plane, &actual.plane, context, "attrib.plane");
    match (multiline, &actual.multiline) {
        (Some(a), Some(b)) => {
            equal(&a.read_as, &b.read_as, context, "attrib.read_as");
            equal(&a.normal, &b.normal, context, "attrib.multiline.normal");
            equal(&a.geometry_revision, &b.geometry_revision, context, "attrib.multiline.geometry_revision");
            equal(&a.embedded_46, &b.embedded_46, context, "attrib.multiline.embedded_46");
            equal(&a.type_72, &b.type_72, context, "attrib.multiline.type_72");
            mtext(&a.mtext, &b.mtext, context);
        }
        _ => equal(multiline, &actual.multiline, context, "attrib.multiline"),
    }
}

fn insert(expected: &Insert, actual: &Insert, context: &str) {
    let Insert { block, insert, scale, rotation, attribs, cols, rows, col_spacing, row_spacing } = expected;
    equal(block, &actual.block, context, "insert.block");
    equal(insert, &actual.insert, context, "insert.insert");
    equal(scale, &actual.scale, context, "insert.scale");
    angle(*rotation, actual.rotation, context, "insert.rotation");
    equal(cols, &actual.cols, context, "insert.cols");
    equal(rows, &actual.rows, context, "insert.rows");
    equal(col_spacing, &actual.col_spacing, context, "insert.col_spacing");
    equal(row_spacing, &actual.row_spacing, context, "insert.row_spacing");
    equal(&attribs.len(), &actual.attribs.len(), context, "insert.attribs.len");
    for (index, (a, b)) in attribs.iter().zip(&actual.attribs).enumerate() {
        attrib(a, b, &format!("{context}.attrib[{index}]"));
    }
}

#[test]
fn comparison_rejects_lost_fields_and_changed_handles() {
    use cadcraft_doc::Handle;
    use cadcraft_geom::Vec3;

    let original = Entity::new(Handle(0x123), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(2.0, 3.0, 4.0) }));
    let mutations: [fn(&mut Entity); 4] = [
        |e| e.handle = Handle(0x456),
        |e| e.common.visible = false,
        |e| e.common.xdata.push(cadcraft_doc::RawTag { code: 1000, value: "unexpected".into() }),
        |e| e.kind = EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }),
    ];
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(std::panic::catch_unwind(|| entity(&original, &changed, "comparison self-test")).is_err());
    }
}

#[test]
fn comparison_rejects_lost_or_changed_referenced_linetypes() {
    use cadcraft_doc::{Linetype, Space};
    use cadcraft_geom::Vec3;

    for (name, entity_linetype, layer_linetype) in [
        ("direct", "ReviewDashed", "Continuous"),
        ("mixed-case", "reviewdashed", "Continuous"),
        ("inherited", "ByLayer", "ReviewDashed"),
        ("inherited-mixed-case", "ByLayer", "reviewdashed"),
    ] {
        let mut original = Drawing::new_metric();
        original.linetypes.push(Linetype::simple("ReviewDashed", "Synthetic", &[2.0, -1.0]));
        original.layer_mut("0").unwrap().linetype = layer_linetype.into();
        original
            .add(
                &Space::Model,
                Common { linetype: entity_linetype.into(), ..Common::default() },
                EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) }),
            )
            .unwrap();
        let referenced = if entity_linetype == "ByLayer" { layer_linetype } else { entity_linetype };
        assert!(original.linetype(referenced).is_some(), "{name}: valid reference");
        model(&original, &original, name);
        let mutations: [fn(&mut Drawing); 2] = [
            |d| d.linetypes.retain(|l| l.name != "ReviewDashed"),
            |d| d.linetypes.iter_mut().find(|l| l.name == "ReviewDashed").unwrap().pattern.clear(),
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut changed = original.clone();
            mutate(&mut changed);
            assert!(std::panic::catch_unwind(|| model(&original, &changed, name)).is_err(), "{name}: mutation {index} must be rejected");
        }
    }
}

#[test]
fn comparison_rejects_block_and_attribute_dependency_losses() {
    use super::cases::{blocks, properties};
    let original = blocks::attributes("overrides").remove(0).drawing;
    let mutations: [fn(&mut Drawing); 4] = [
        |d| {
            d.blocks.remove("Z_Leaf");
        },
        |d| std::sync::Arc::make_mut(d.blocks.get_mut("Z_Leaf").unwrap()).base.x += 1.0,
        |d| d.layer_mut("Survey_Ä").unwrap().color = cadcraft_color::Color::Index(5),
        |d| {
            let handle = d.model.iter().next().unwrap().handle;
            d.model
                .modify(handle, |e| {
                    let EntityKind::Insert(i) = &mut e.kind else { panic!("expected INSERT") };
                    i.attribs.reverse();
                })
                .unwrap();
        },
    ];
    model(&original, &original, "unchanged block and attributes");
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(std::panic::catch_unwind(|| model(&original, &changed, "block dependency self-test")).is_err());
    }
    let original = properties::complex_linetypes(false).remove(0).drawing;
    let mut changed = original.clone();
    changed.text_styles.retain(|s| s.name != "Notes");
    assert!(std::panic::catch_unwind(|| model(&original, &changed, "linetype style self-test")).is_err());
    let cyclic = blocks::nested(true).remove(0).drawing;
    model(&cyclic, &cyclic, "cycles compare without expansion");
}

#[test]
fn normalization_keeps_observable_fields_and_nondefault_alpha() {
    use cadcraft_doc::{Handle, Transparency};

    let mut original = Entity::new(Handle(0x123), EntityKind::Text(super::cases::text()));
    original.common.transparency = Transparency::Percent(50);
    let mut explicit_default = original.clone();
    explicit_default.common.transparency_alpha = Some(Transparency::default_alpha(50));
    entity(&original, &explicit_default, "derived alpha is not a difference");
    let mutations: [fn(&mut Entity); 9] = [
        |e| e.common.layer = "Other".into(),
        |e| e.common.color = cadcraft_color::Color::Index(3),
        |e| e.common.linetype = "Other".into(),
        |e| e.common.lineweight = cadcraft_doc::Lineweight::Mm100(25),
        |e| e.common.transparency = Transparency::Percent(37),
        |e| e.common.transparency_alpha = Some(128),
        |e| {
            let EntityKind::Text(text) = &mut e.kind else { panic!("expected TEXT") };
            text.style = "Other".into();
        },
        |e| {
            let EntityKind::Text(text) = &mut e.kind else { panic!("expected TEXT") };
            text.insert.x += 1.0;
        },
        |e| e.handle = Handle(0x456),
    ];
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(std::panic::catch_unwind(|| entity(&original, &changed, "observable mutation")).is_err());
    }
}

#[test]
fn normalization_keeps_attribute_layers_planes_and_wipeout_display() {
    use cadcraft_doc::{AttribPlane, Handle, Wipeout};
    use cadcraft_geom::{Vec2, Vec3};

    let mut attribute = Attrib::new("ID", super::cases::text());
    let original = Entity::new(Handle(0x123), EntityKind::AttDef(attribute.clone()));
    attribute.props.layer = Some("0".into());
    entity(&original, &Entity::new(original.handle, EntityKind::AttDef(attribute.clone())), "explicit layer 0");
    attribute.props.layer = Some("Other".into());
    assert!(
        std::panic::catch_unwind(|| entity(&original, &Entity::new(original.handle, EntityKind::AttDef(attribute.clone())), "attribute layer"))
            .is_err()
    );
    attribute.props.layer = None;
    attribute.plane = AttribPlane { normal: Vec3::new(0.0, 1.0, 0.0), thickness: 0.5 };
    assert!(
        std::panic::catch_unwind(|| entity(&original, &Entity::new(original.handle, EntityKind::AttDef(attribute.clone())), "attribute plane"))
            .is_err()
    );

    let wipeout = Wipeout { boundary: vec![Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)], ..Default::default() };
    let original = Entity::new(Handle(0x456), EntityKind::Wipeout(wipeout.clone()));
    let mut changed = wipeout;
    changed.display.brightness = 20;
    assert!(std::panic::catch_unwind(|| entity(&original, &Entity::new(original.handle, EntityKind::Wipeout(changed)), "wipeout display")).is_err());
}

#[test]
fn normalization_keeps_two_populated_source_fields() {
    use cadcraft_doc::{GeometrySnapshot, Handle};
    use cadcraft_geom::Vec3;
    use std::sync::Arc;

    let mut original = Entity::new(Handle(0x123), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 0.0) }));
    original.common.geometry = Some(Arc::new(GeometrySnapshot { records: Vec::new(), model: "original".into(), text: None, dimension: None }));
    let mut changed = original.clone();
    Arc::make_mut(changed.common.geometry.as_mut().unwrap()).model = "changed".into();
    assert!(std::panic::catch_unwind(|| entity(&original, &changed, "two populated snapshots")).is_err());
    let authored = Entity::new(original.handle, original.kind.clone());
    entity(&authored, &original, "read-filled snapshot");
}

#[test]
fn angle_tolerance_does_not_hide_real_changes_or_nonfinite_values() {
    angle(0.7, 0.7_f64.to_degrees().to_radians(), "self-test", "angle");
    for actual in [0.700001, f64::NAN, f64::INFINITY] {
        assert!(std::panic::catch_unwind(|| angle(0.7, actual, "self-test", "angle")).is_err());
    }
}
