//! The groups acadrust writes since its DWG writer gaps were closed on layers, layout viewports,
//! image reactors, plot styles, extension dictionaries and xrefs are kept by a CADCraft save, as
//! AutoCAD keeps them. A save numbers objects and table records anew, so every handle that one of
//! them holds names the saved object.

use acadrust::entities::{AttributeEntity, Insert, Leader, Line, MText, Viewport};
use acadrust::objects::{Dictionary, Material, ObjectType, PlaceHolder, Scale};
use acadrust::tables::{AppId, BlockRecord, Layer, LineType, Ucs};
use acadrust::types::{DxfVersion, Vector3};
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::{CadDocument, EntityType, Handle, TableEntry};

use super::bridge_header_groups::{add_to_dictionary, saves, visual_style};

/// Add the object `object` builds under `name` to the dictionary `dictionary`.
fn add_object(doc: &mut CadDocument, dictionary: Handle, name: &str, object: impl FnOnce(Handle, Handle) -> ObjectType) -> Handle {
    let handle = doc.allocate_handle();
    doc.objects.insert(handle, object(handle, dictionary));
    match doc.objects.get_mut(&dictionary) {
        Some(ObjectType::Dictionary(d)) => d.add_entry(name, handle),
        Some(ObjectType::DictionaryWithDefault(d)) => d.entries.push((name.to_string(), handle)),
        other => panic!("no dictionary {dictionary:?}: {other:?}"),
    }
    handle
}

fn placeholder(handle: Handle, owner: Handle) -> ObjectType {
    let mut p = PlaceHolder::new();
    p.handle = handle;
    p.owner = owner;
    ObjectType::PlaceHolder(p)
}

/// The name under which `handle` is entered in the plot style dictionary, when it is a placeholder.
fn plot_style_name(doc: &CadDocument, handle: Handle) -> Option<String> {
    let Some(ObjectType::PlaceHolder(_)) = doc.objects.get(&handle) else { return None };
    let Some(ObjectType::DictionaryWithDefault(d)) = doc.objects.get(&doc.header.acad_plotstylename_dict_handle) else { return None };
    d.entries.iter().find(|(_, h)| *h == handle).map(|(n, _)| n.clone())
}

fn add_ucs(doc: &mut CadDocument, name: &str) -> Handle {
    let mut ucs = Ucs::new(name);
    ucs.set_handle(doc.allocate_handle());
    let handle = ucs.handle;
    doc.ucss.add(ucs).expect("add ucs");
    handle
}

#[test]
fn a_layer_keeps_its_material_and_plot_style() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let materials = doc.header.acad_material_dict_handle;
    let material = add_object(&mut doc, materials, "Concrete", |handle, owner| {
        let mut m = Material::new();
        m.handle = handle;
        m.owner = owner;
        m.name = "Concrete".to_string();
        ObjectType::Material(m)
    });
    let styles = doc.header.acad_plotstylename_dict_handle;
    let style = add_object(&mut doc, styles, "Color_1", placeholder);
    let mut layer = Layer::new("Walls");
    layer.set_handle(doc.allocate_handle());
    layer.material = material;
    layer.plotstyle_handle = style;
    doc.layers.add(layer).expect("add layer");
    for (what, back) in saves(&doc) {
        let layer = back.layers.get("Walls").expect("layer Walls");
        assert!(
            matches!(back.objects.get(&layer.material), Some(ObjectType::Material(m)) if m.name == "Concrete"),
            "{what}: material {:?}",
            layer.material
        );
        assert_eq!(plot_style_name(&back, layer.plotstyle_handle).as_deref(), Some("Color_1"), "{what}: plot style {:?}", layer.plotstyle_handle);
    }
}

#[test]
fn a_layout_viewport_keeps_its_ucs() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let named = add_ucs(&mut doc, "Sheet");
    let base = add_ucs(&mut doc, "SheetBase");
    let mut vp = Viewport::new();
    vp.id = 2;
    vp.width = 160.0;
    vp.height = 100.0;
    vp.ucs_per_viewport = true;
    vp.ucs_handle = named;
    vp.base_ucs_handle = base;
    vp.ucs_ortho_type = 3;
    doc.add_entity_to_layout(EntityType::Viewport(Box::new(vp)), "Layout1").expect("add viewport");
    for (what, back) in saves(&doc) {
        let vp = back
            .entities()
            .find_map(|e| match e {
                EntityType::Viewport(v) if v.width == 160.0 => Some(v.as_ref()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: viewport lost"));
        let name_of = |h: Handle| back.ucss.iter().find(|u| u.handle == h).map_or_else(|| format!("no UCS for {h:?}"), |u| u.name.clone());
        assert_eq!(name_of(vp.ucs_handle), "Sheet", "{what}: 345");
        assert_eq!(name_of(vp.base_ucs_handle), "SheetBase", "{what}: 346");
        assert_eq!(vp.ucs_ortho_type, 3, "{what}: 79");
    }
}

/// A drawing with one image and its IMAGEDEF and IMAGEDEF_REACTOR, as AutoCAD writes them: the
/// reactor names the image as its owner and again after the subclass marker.
const IMAGE_DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nIMAGE\n5\nA0\n100\nAcDbEntity\n8\n0\n100\nAcDbRasterImage\n90\n0\n10\n0\n20\n0\n30\n0\n11\n0.1\n21\n0\n31\n0\n12\n0\n22\n0.1\n32\n0\n\
13\n100\n23\n50\n340\nB0\n70\n15\n280\n0\n281\n50\n282\n50\n283\n0\n360\nA2\n0\nENDSEC\n\
0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nACAD_IMAGE_DICT\n350\nB1\n\
0\nDICTIONARY\n5\nB1\n330\nC\n100\nAcDbDictionary\n281\n1\n3\npicture\n350\nB0\n\
0\nIMAGEDEF\n5\nB0\n102\n{ACAD_REACTORS\n330\nB1\n330\nA2\n102\n}\n330\nB1\n100\nAcDbRasterImageDef\n90\n0\n1\nC:\\pictures\\picture.png\n10\n100\n20\n50\n11\n0.01\n21\n0.01\n280\n1\n281\n0\n\
0\nIMAGEDEF_REACTOR\n5\nA2\n330\nA0\n100\nAcDbRasterImageDefReactor\n90\n2\n330\nA0\n0\nENDSEC\n0\nEOF\n";

#[test]
fn an_image_reactor_keeps_its_owner_and_its_image() {
    let d = cadcraft_io::read_dxf(IMAGE_DXF.as_bytes()).expect("read");
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let dxf = String::from_utf8(cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap()).unwrap();
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let at = lines.windows(2).position(|w| w == ["0", "IMAGEDEF_REACTOR"]).expect("the reactor is written");
    let record: Vec<(&str, &str)> = lines[at + 2..].chunks(2).take_while(|p| p[0] != "0").map(|p| (p[0], p[1])).collect();
    let owners: Vec<&str> = record.iter().filter(|(c, _)| *c == "330").map(|(_, v)| *v).collect();
    assert_eq!(owners.len(), 2, "the owner and the image in {record:?}");
    assert_eq!(owners[0], owners[1], "both name the image in {record:?}");
}

#[test]
fn an_image_reactor_keeps_its_owner_through_dwg() {
    let source = acadrust::DxfReader::from_reader(std::io::Cursor::new(IMAGE_DXF.as_bytes())).expect("reader").read().expect("read dxf");
    for (what, back) in saves(&source) {
        let owners: Vec<Handle> = back
            .objects
            .values()
            .filter_map(|o| match o {
                ObjectType::ImageDefinitionReactor(r) => Some(r.owner),
                _ => None,
            })
            .collect();
        assert_eq!(owners.len(), 1, "{what}: the reactor is kept");
        assert!(matches!(back.get_entity(owners[0]), Some(EntityType::RasterImage(_))), "{what}: owner {:?} is the image", owners[0]);
    }
}

/// A line, a line with ByBlock plot style and an INSERT whose ATTRIB names a plot style.
fn plot_styled() -> CadDocument {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let styles = doc.header.acad_plotstylename_dict_handle;
    let color_1 = add_object(&mut doc, styles, "Color_1", placeholder);
    let color_2 = add_object(&mut doc, styles, "Color_2", placeholder);
    let mut block = BlockRecord::new("LABEL");
    block.set_handle(doc.allocate_handle());
    doc.block_records.add(block).expect("add block");
    let mut line = Line::from_coords(0.0, 0.0, 0.0, 5.0, 5.0, 0.0);
    line.common.plotstyle_flags = 3;
    line.common.plotstyle_handle = Some(color_1);
    doc.add_entity(EntityType::Line(line)).expect("add line");
    let mut by_block = Line::from_coords(0.0, 0.0, 0.0, 6.0, 6.0, 0.0);
    by_block.common.plotstyle_flags = 1;
    doc.add_entity(EntityType::Line(by_block)).expect("add line");
    let mut attribute = AttributeEntity::new("TAG".to_string(), "value".to_string());
    attribute.common.plotstyle_flags = 3;
    attribute.common.plotstyle_handle = Some(color_2);
    let mut insert = Insert::new("LABEL", Vector3::new(0.0, 0.0, 0.0));
    insert.attributes.push(attribute);
    doc.add_entity(EntityType::Insert(insert)).expect("add insert");
    doc
}

#[test]
fn entities_and_attributes_keep_their_plot_style() {
    for (what, back) in saves(&plot_styled()) {
        let name = |h: Option<Handle>| h.and_then(|h| plot_style_name(&back, h));
        let line = |y: f64| {
            back.entities()
                .find_map(|e| match e {
                    EntityType::Line(l) if l.end.y == y => Some((l.common.plotstyle_flags, name(l.common.plotstyle_handle))),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{what}: no line at {y}"))
        };
        assert_eq!(line(5.0), (3, Some("Color_1".to_string())), "{what}: line");
        assert_eq!(line(6.0), (1, None), "{what}: ByBlock line");
        let attribute = back
            .entities()
            .find_map(|e| match e {
                EntityType::Insert(i) => i.attributes.first(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: attribute lost"));
        assert_eq!(
            (attribute.common.plotstyle_flags, name(attribute.common.plotstyle_handle)),
            (3, Some("Color_2".to_string())),
            "{what}: attribute"
        );
    }
}

const KEY: &str = "ACAD_XREC_ROUNDTRIP";

#[derive(Clone, Copy, Debug)]
enum Kind {
    MLeaderStyle,
    TableStyle,
    Scale,
}

/// A document whose object of `kind` has an extension dictionary holding an XRECORD under
/// ACAD_XREC_ROUNDTRIP and, when `xdata`, an XDATA record of the application ACME.
fn with_extension_dictionary(kind: Kind, xdata: bool) -> (CadDocument, Handle) {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let handle = match kind {
        Kind::MLeaderStyle => doc.objects.iter().find_map(|(h, o)| matches!(o, ObjectType::MultiLeaderStyle(_)).then_some(*h)),
        Kind::TableStyle => doc.objects.iter().find_map(|(h, o)| matches!(o, ObjectType::TableStyle(_)).then_some(*h)),
        Kind::Scale => {
            let root = doc.header.named_objects_dict_handle;
            let list = add_object(&mut doc, root, "ACAD_SCALELIST", |handle, owner| {
                let mut d = Dictionary::new();
                d.handle = handle;
                d.owner = owner;
                ObjectType::Dictionary(d)
            });
            Some(add_object(&mut doc, list, "A0", |handle, owner| {
                let mut scale = Scale::new("1:7", 1.0, 7.0);
                scale.handle = handle;
                scale.owner_handle = owner;
                ObjectType::Scale(scale)
            }))
        }
    }
    .unwrap_or_else(|| panic!("{kind:?}: no such object in a new drawing"));
    doc.ensure_xrecord(handle, KEY);
    if xdata {
        doc.app_ids.add(AppId::new("ACME")).expect("add appid");
        let mut record = ExtendedDataRecord::new("ACME");
        record.values = vec![XDataValue::String("kept".into()), XDataValue::Integer16(7)];
        doc.set_object_xdata_record(handle, record);
    }
    (doc, handle)
}

fn check_extension_dictionary(doc: &CadDocument, kind: Kind, xdata: bool, what: &str) {
    // The save numbers the object anew: find it by what it is.
    let handle = doc
        .objects
        .iter()
        .find_map(|(h, o)| match (kind, o) {
            (Kind::MLeaderStyle, ObjectType::MultiLeaderStyle(_)) | (Kind::TableStyle, ObjectType::TableStyle(_)) => Some(*h),
            (Kind::Scale, ObjectType::Scale(s)) if s.name == "1:7" => Some(*h),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{what}, {kind:?}: the object is lost"));
    let dictionary = doc.extension_dictionary_handle(handle).unwrap_or_else(|| panic!("{what}, {kind:?}: no extension dictionary"));
    let Some(ObjectType::Dictionary(entries)) = doc.objects.get(&dictionary) else {
        panic!("{what}, {kind:?}: the extension dictionary is not a dictionary")
    };
    let record = entries.get(KEY).unwrap_or_else(|| panic!("{what}, {kind:?}: no {KEY} entry"));
    assert!(matches!(doc.objects.get(&record), Some(ObjectType::XRecord(_))), "{what}, {kind:?}: {KEY} is not an XRECORD");
    if xdata {
        let data = doc.object_xdata(handle).and_then(|x| x.get_record("ACME")).unwrap_or_else(|| panic!("{what}, {kind:?}: no ACME xdata"));
        assert_eq!(data.values, vec![XDataValue::String("kept".into()), XDataValue::Integer16(7)], "{what}: {kind:?}");
    }
}

#[test]
fn an_mleader_style_keeps_its_extension_dictionary_and_xdata() {
    for (what, back) in saves(&with_extension_dictionary(Kind::MLeaderStyle, true).0) {
        check_extension_dictionary(&back, Kind::MLeaderStyle, true, what);
    }
}

#[test]
fn a_table_style_keeps_its_extension_dictionary_and_xdata() {
    for (what, back) in saves(&with_extension_dictionary(Kind::TableStyle, true).0) {
        check_extension_dictionary(&back, Kind::TableStyle, true, what);
    }
}

#[test]
fn a_scale_keeps_its_extension_dictionary() {
    for (what, back) in saves(&with_extension_dictionary(Kind::Scale, false).0) {
        check_extension_dictionary(&back, Kind::Scale, false, what);
    }
}

/// A document with the xref block "SITE" and a dependent layer and linetype that the layer uses.
/// A save keeps no other symbol of an xref (CADCraft's choice, see `dxf_xref`).
fn with_xref_symbols() -> CadDocument {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut block = BlockRecord::new("SITE");
    block.set_handle(doc.allocate_handle());
    block.flags.is_xref = true;
    block.xref_path = "site.dwg".to_string();
    doc.block_records.add(block).expect("add xref block");
    let mut layer = Layer::new("SITE|Walls");
    layer.set_handle(doc.allocate_handle());
    layer.flags.xref_dependent = true;
    layer.line_type = "SITE|Dashed".to_string();
    doc.layers.add(layer).expect("add layer");
    let mut linetype = LineType::new("SITE|Dashed");
    linetype.set_handle(doc.allocate_handle());
    linetype.xref_dependent = true;
    doc.line_types.add(linetype).expect("add linetype");
    doc
}

#[test]
fn xref_symbols_point_at_the_block_record_of_their_xref() {
    for (what, back) in saves(&with_xref_symbols()) {
        let block = back.block_records.iter().find(|b| b.name == "SITE").unwrap_or_else(|| panic!("{what}: xref block lost"));
        assert!(block.flags.is_xref, "{what}: the block keeps its xref flag");
        assert_eq!(block.xref_path, "site.dwg", "{what}: the block keeps its path");
        let xref = block.handle;
        assert_eq!(back.layers.get("SITE|Walls").map(|l| l.xref_block_record_handle), Some(xref), "{what}: layer");
        assert_eq!(back.line_types.get("SITE|Dashed").map(|l| l.xref_block_record_handle), Some(xref), "{what}: linetype");
    }
}

#[test]
fn a_layout_viewport_keeps_its_view_and_visual_style() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let styles = doc.header.acad_visualstyle_dict_handle;
    let style = add_to_dictionary(&mut doc, styles, "Realistic", visual_style("realistic"));
    let mut vp = Viewport::new();
    vp.id = 2;
    vp.width = 160.0;
    vp.height = 100.0;
    vp.view_direction = Vector3::new(1.0, 1.0, 1.0);
    vp.view_target = Vector3::new(4.0, 5.0, 6.0);
    vp.twist_angle = 0.5;
    vp.lens_length = 80.0;
    vp.visual_style_handle = style;
    doc.add_entity_to_layout(EntityType::Viewport(Box::new(vp)), "Layout1").expect("add viewport");
    for (what, back) in saves(&doc) {
        let vp = back
            .entities()
            .find_map(|e| match e {
                EntityType::Viewport(v) if v.width == 160.0 => Some(v.as_ref()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: viewport lost"));
        assert_eq!(vp.view_direction, Vector3::new(1.0, 1.0, 1.0), "{what}: view direction");
        assert_eq!(vp.view_target, Vector3::new(4.0, 5.0, 6.0), "{what}: view target");
        assert_eq!(vp.twist_angle, 0.5, "{what}: twist");
        assert_eq!(vp.lens_length, 80.0, "{what}: lens");
        let described = match back.objects.get(&vp.visual_style_handle) {
            Some(ObjectType::VisualStyle(s)) => s.description.clone(),
            other => format!("not a visual style: {other:?}"),
        };
        assert_eq!(described, "realistic", "{what}: visual style");
    }
}

#[test]
fn a_leader_keeps_its_annotation_and_hook_line() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut text = MText::new();
    text.common.handle = doc.allocate_handle();
    text.value = "note".to_string();
    let annotation = text.common.handle;
    doc.add_entity(EntityType::MText(Box::new(text))).expect("add mtext");
    let mut leader = Leader::new();
    leader.vertices = vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(5.0, 5.0, 0.0)];
    leader.annotation_handle = annotation;
    leader.hookline_enabled = true;
    leader.text_height = 2.5;
    doc.add_entity(EntityType::Leader(leader)).expect("add leader");
    for (what, back) in saves(&doc) {
        let leader = back
            .entities()
            .find_map(|e| match e {
                EntityType::Leader(l) => Some(l),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: leader lost"));
        assert!(matches!(back.get_entity(leader.annotation_handle), Some(EntityType::MText(_))), "{what}: annotation {:?}", leader.annotation_handle);
        assert!(leader.hookline_enabled, "{what}: hook line");
        assert_eq!(leader.text_height, 2.5, "{what}: text height");
    }
}

#[test]
fn the_active_vport_keeps_its_view_and_visual_style() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let styles = doc.header.acad_visualstyle_dict_handle;
    let style = add_to_dictionary(&mut doc, styles, "Realistic", visual_style("realistic"));
    let ucs = add_ucs(&mut doc, "Plan");
    let active = doc.vports.get_mut("*Active").expect("*Active VPORT");
    active.view_direction = Vector3::new(1.0, 1.0, 1.0);
    active.lens_length = 80.0;
    active.visual_style_handle = style;
    active.named_ucs_handle = ucs;
    for (what, back) in saves(&doc) {
        let active = back.vports.get("*Active").unwrap_or_else(|| panic!("{what}: *Active VPORT lost"));
        assert_eq!(active.view_direction, Vector3::new(1.0, 1.0, 1.0), "{what}: view direction");
        assert_eq!(active.lens_length, 80.0, "{what}: lens");
        let described = match back.objects.get(&active.visual_style_handle) {
            Some(ObjectType::VisualStyle(s)) => s.description.clone(),
            other => format!("not a visual style: {other:?}"),
        };
        assert_eq!(described, "realistic", "{what}: visual style");
        let ucs = back.ucss.iter().find(|u| u.handle == active.named_ucs_handle).map(|u| u.name.as_str());
        assert_eq!(ucs, Some("Plan"), "{what}: named UCS");
    }
}

#[test]
fn a_layout_keeps_its_active_viewport_and_ucs() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let named = add_ucs(&mut doc, "Sheet");
    let base = add_ucs(&mut doc, "SheetBase");
    let mut vp = Viewport::new();
    vp.id = 2;
    vp.width = 160.0;
    vp.height = 100.0;
    let viewport = doc.add_entity_to_layout(EntityType::Viewport(Box::new(vp)), "Layout1").expect("add viewport");
    let layout = doc
        .objects
        .values_mut()
        .find_map(|o| match o {
            ObjectType::Layout(l) if l.name == "Layout1" => Some(l),
            _ => None,
        })
        .expect("Layout1");
    layout.viewport = viewport;
    layout.named_ucs = named;
    layout.base_ucs = base;
    for (what, back) in saves(&doc) {
        let layout = back
            .objects
            .values()
            .find_map(|o| match o {
                ObjectType::Layout(l) if l.name == "Layout1" => Some(l),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: Layout1 lost"));
        assert!(
            matches!(back.get_entity(layout.viewport), Some(EntityType::Viewport(v)) if v.width == 160.0),
            "{what}: active viewport {:?}",
            layout.viewport
        );
        let name_of = |h: Handle| back.ucss.iter().find(|u| u.handle == h).map_or_else(|| format!("no UCS for {h:?}"), |u| u.name.clone());
        assert_eq!(name_of(layout.named_ucs), "Sheet", "{what}: named UCS");
        assert_eq!(name_of(layout.base_ucs), "SheetBase", "{what}: base UCS");
    }
}

#[test]
fn a_layout_viewport_keeps_its_frozen_layers() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut layer = Layer::new("Walls");
    layer.set_handle(doc.allocate_handle());
    let walls = layer.handle;
    doc.layers.add(layer).expect("add layer");
    let mut vp = Viewport::new();
    vp.id = 2;
    vp.width = 160.0;
    vp.height = 100.0;
    vp.frozen_layers = vec![walls];
    doc.add_entity_to_layout(EntityType::Viewport(Box::new(vp)), "Layout1").expect("add viewport");
    for (what, back) in saves(&doc) {
        let vp = back
            .entities()
            .find_map(|e| match e {
                EntityType::Viewport(v) if v.width == 160.0 => Some(v.as_ref()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: viewport lost"));
        let names: Vec<&str> = vp.frozen_layers.iter().filter_map(|h| back.layers.iter().find(|l| l.handle == *h)).map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Walls"], "{what}: frozen layers");
    }
}

/// A table style's name is in its DWG object, as well as in its dictionary entry.
#[test]
fn a_table_style_keeps_its_name() {
    let doc = CadDocument::with_version(DxfVersion::AC1032);
    for (what, back) in saves(&doc) {
        let names: Vec<String> = back
            .objects
            .values()
            .filter_map(|o| match o {
                ObjectType::TableStyle(s) => Some(s.name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["Standard"], "{what}: table style names");
    }
}


/// The cell style borders of a table style point at the ByBlock linetype, as AutoCAD writes them
/// (acadrust `TableCellBorder::line_type`).
#[test]
fn a_table_style_keeps_the_byblock_linetype_of_its_borders() {
    let doc = CadDocument::with_version(DxfVersion::AC1032);
    for (what, back) in saves(&doc) {
        let by_block = back.line_types.get("ByBlock").expect("ByBlock").handle;
        let style = back
            .objects
            .values()
            .find_map(|o| match o {
                ObjectType::TableStyle(s) if s.name == "Standard" => Some(s),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: Standard table style lost"));
        assert!(!style.modern_overrides.is_empty(), "{what}: cell style overrides");
        for (key, s) in &style.modern_overrides {
            let handles: Vec<Handle> = s.cell_style.borders.iter().map(|g| g.line_type).collect();
            assert_eq!(handles, vec![by_block; 6], "{what}: override {key}");
        }
    }
}
