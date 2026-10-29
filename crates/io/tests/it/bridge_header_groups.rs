//! The header variables acadrust writes since its DWG writer gaps were closed (the current
//! material and visual styles, the plot style of new objects, the UCS names and orthographic
//! settings, the plot style table, the project name, the dimension linetypes) are kept by a
//! CADCraft save, as AutoCAD keeps them. A save numbers the objects and table records anew, so a
//! variable that names one points at the saved object.

use std::io::Cursor;

use acadrust::objects::{Material, ObjectType, VisualStyle};
use acadrust::tables::Ucs;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, Handle, TableEntry};

/// Add the object `make(handle, owner)` builds to the dictionary `dictionary` under `name`.
pub(super) fn add_to_dictionary(doc: &mut CadDocument, dictionary: Handle, name: &str, make: impl FnOnce(Handle, Handle) -> ObjectType) -> Handle {
    let handle = doc.allocate_handle();
    doc.objects.insert(handle, make(handle, dictionary));
    let Some(ObjectType::Dictionary(dict)) = doc.objects.get_mut(&dictionary) else { panic!("no dictionary {dictionary:?}") };
    dict.add_entry(name, handle);
    handle
}

pub(super) fn visual_style(description: &'static str) -> impl FnOnce(Handle, Handle) -> ObjectType {
    move |handle, owner| {
        let mut style = VisualStyle::new();
        style.handle = handle;
        style.owner = owner;
        style.description = description.to_string();
        ObjectType::VisualStyle(style)
    }
}

fn add_ucs(doc: &mut CadDocument, name: &str, origin: Vector3) -> Handle {
    let mut ucs = Ucs::new(name);
    ucs.origin = origin;
    ucs.set_handle(doc.allocate_handle());
    let handle = ucs.handle;
    doc.ucss.add(ucs).expect("add ucs");
    handle
}

/// A drawing whose header holds every variable that names an object or a UCS.
fn document() -> CadDocument {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let materials = doc.header.acad_material_dict_handle;
    let styles = doc.header.acad_visualstyle_dict_handle;
    doc.header.current_material_handle = add_to_dictionary(&mut doc, materials, "Brick", |handle, owner| {
        let mut material = Material::new();
        material.handle = handle;
        material.owner = owner;
        material.name = "Brick".to_string();
        ObjectType::Material(material)
    });
    doc.header.interference_object_visual_style_handle =
        add_to_dictionary(&mut doc, styles, "InterferenceObjects", visual_style("interference objects"));
    doc.header.interference_viewport_visual_style_handle =
        add_to_dictionary(&mut doc, styles, "InterferenceViewport", visual_style("interference viewport"));
    doc.header.drag_visual_style_handle = add_to_dictionary(&mut doc, styles, "Drag", visual_style("drag"));
    let Some(ObjectType::DictionaryWithDefault(plot_styles)) = doc.objects.get(&doc.header.acad_plotstylename_dict_handle) else {
        panic!("no ACAD_PLOTSTYLENAME dictionary")
    };
    let placeholder = plot_styles.default_handle;
    doc.header.current_plotstyle_type = 3;
    doc.header.current_plotstyle_handle = placeholder;
    doc.header.stylesheet = "monochrome.ctb".to_string();
    doc.header.project_name = "Synthetic".to_string();
    let plan = add_ucs(&mut doc, "Plan", Vector3::new(1.0, 2.0, 0.0));
    let site = add_ucs(&mut doc, "Site", Vector3::new(3.0, 4.0, 0.0));
    let tower = add_ucs(&mut doc, "Tower", Vector3::new(5.0, 6.0, 0.0));
    let sheet = add_ucs(&mut doc, "Sheet", Vector3::new(7.0, 8.0, 0.0));
    let sheet_base = add_ucs(&mut doc, "SheetBase", Vector3::new(9.0, 1.0, 0.0));
    let sheet_ref = add_ucs(&mut doc, "SheetRef", Vector3::new(2.0, 3.0, 0.0));
    let h = &mut doc.header;
    h.model_space_ucs_name = "Plan".to_string();
    h.model_space_ucs_handle = plan;
    h.ucs_base = "Site".to_string();
    h.ucs_base_handle = site;
    h.ucs_ortho_ref_name = "Tower".to_string();
    h.ucs_ortho_ref = tower;
    h.ucs_ortho_view = 3;
    h.ucs_ortho_origins[0] = Vector3::new(0.5, 1.5, 2.5);
    h.ucs_ortho_origins[5] = Vector3::new(-1.0, -2.0, -3.0);
    h.paper_space_ucs_name = "Sheet".to_string();
    h.paper_space_ucs_handle = sheet;
    h.paper_space_ucs_origin = Vector3::new(7.0, 8.0, 0.0);
    h.paper_space_ucs_x_axis = Vector3::new(0.0, 1.0, 0.0);
    h.paper_space_ucs_y_axis = Vector3::new(-1.0, 0.0, 0.0);
    h.paper_ucs_base = "SheetBase".to_string();
    h.paper_ucs_base_handle = sheet_base;
    h.paper_ucs_ortho_ref_name = "SheetRef".to_string();
    h.paper_ucs_ortho_ref = sheet_ref;
    h.paper_ucs_ortho_view = 5;
    h.paper_ucs_ortho_origins[2] = Vector3::new(4.5, 5.5, 6.5);
    doc
}

pub(super) fn read_dwg(dwg: &[u8]) -> CadDocument {
    DwgReader::from_stream(Cursor::new(dwg)).read().expect("read dwg")
}

/// The drawing as acadrust's DWG writer and reader return it (the source of the saves), after a
/// CADCraft save, and after a save of that save.
pub(super) fn saves(doc: &CadDocument) -> [(&'static str, CadDocument); 3] {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let save = |dwg: &[u8]| cadcraft_io::write_with(&cadcraft_io::read(dwg, "x.dwg").expect("cadcraft read"), "x.dwg", &opts).expect("cadcraft save");
    let source = DwgWriter::write_to_vec(doc).expect("write dwg");
    let first = save(&source);
    let second = save(&first);
    [("source", read_dwg(&source)), ("first save", read_dwg(&first)), ("second save", read_dwg(&second))]
}

fn style_description(doc: &CadDocument, handle: Handle) -> String {
    match doc.objects.get(&handle) {
        Some(ObjectType::VisualStyle(s)) => s.description.clone(),
        other => format!("not a visual style: {other:?}"),
    }
}

fn ucs_name(doc: &CadDocument, handle: Handle) -> String {
    doc.ucss.iter().find(|u| u.handle == handle).map_or_else(|| format!("no UCS for {handle:?}"), |u| u.name.clone())
}

#[test]
fn the_object_pointers_of_the_header_name_the_saved_objects() {
    for (what, back) in saves(&document()) {
        let h = &back.header;
        let material = match back.objects.get(&h.current_material_handle) {
            Some(ObjectType::Material(m)) => m.name.clone(),
            other => format!("not a material: {other:?}"),
        };
        assert_eq!(material, "Brick", "{what}: CMATERIAL");
        assert_eq!(style_description(&back, h.interference_object_visual_style_handle), "interference objects", "{what}: INTERFEREOBJVS");
        assert_eq!(style_description(&back, h.interference_viewport_visual_style_handle), "interference viewport", "{what}: INTERFEREVPVS");
        assert_eq!(style_description(&back, h.drag_visual_style_handle), "drag", "{what}: DRAGVS");
    }
}

#[test]
fn the_plot_style_of_new_objects_is_kept() {
    for (what, back) in saves(&document()) {
        assert_eq!(back.header.current_plotstyle_type, 3, "{what}: CEPSNTYPE");
        assert!(
            matches!(back.objects.get(&back.header.current_plotstyle_handle), Some(ObjectType::PlaceHolder(_))),
            "{what}: CPSNID names {:?}",
            back.header.current_plotstyle_handle
        );
    }
}

#[test]
fn the_plot_style_table_and_the_project_name_are_kept() {
    for (what, back) in saves(&document()) {
        assert_eq!(back.header.stylesheet, "monochrome.ctb", "{what}: STYLESHEET");
        assert_eq!(back.header.project_name, "Synthetic", "{what}: PROJECTNAME");
    }
}

#[test]
fn the_ucs_header_names_the_saved_ucs_entries() {
    for (what, back) in saves(&document()) {
        let h = &back.header;
        for (variable, name, handle, expected) in [
            ("UCSNAME", &h.model_space_ucs_name, h.model_space_ucs_handle, "Plan"),
            ("UCSBASE", &h.ucs_base, h.ucs_base_handle, "Site"),
            ("UCSORTHOREF", &h.ucs_ortho_ref_name, h.ucs_ortho_ref, "Tower"),
            ("PUCSNAME", &h.paper_space_ucs_name, h.paper_space_ucs_handle, "Sheet"),
            ("PUCSBASE", &h.paper_ucs_base, h.paper_ucs_base_handle, "SheetBase"),
            ("PUCSORTHOREF", &h.paper_ucs_ortho_ref_name, h.paper_ucs_ortho_ref, "SheetRef"),
        ] {
            assert_eq!(name, expected, "{what}: {variable} name");
            assert_eq!(ucs_name(&back, handle), expected, "{what}: {variable} handle");
        }
        assert_eq!(h.ucs_ortho_view, 3, "{what}: UCSORTHOVIEW");
        assert_eq!(h.ucs_ortho_origins[0], Vector3::new(0.5, 1.5, 2.5), "{what}: UCSORGTOP");
        assert_eq!(h.ucs_ortho_origins[5], Vector3::new(-1.0, -2.0, -3.0), "{what}: UCSORGBACK");
        assert_eq!(h.paper_ucs_ortho_view, 5, "{what}: PUCSORTHOVIEW");
        assert_eq!(h.paper_ucs_ortho_origins[2], Vector3::new(4.5, 5.5, 6.5), "{what}: PUCSORGLEFT");
        assert_eq!(h.paper_space_ucs_origin, Vector3::new(7.0, 8.0, 0.0), "{what}: PUCSORG");
        assert_eq!(h.paper_space_ucs_x_axis, Vector3::new(0.0, 1.0, 0.0), "{what}: PUCSXDIR");
        assert_eq!(h.paper_space_ucs_y_axis, Vector3::new(-1.0, 0.0, 0.0), "{what}: PUCSYDIR");
    }
}

/// AutoCAD stores no dimension linetype in a fresh drawing: a null handle, an empty name. It
/// stays empty, never ByBlock.
#[test]
fn a_null_dimension_linetype_stays_null() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.header.dim_linetype_name = String::new();
    doc.header.dim_linetype1_name = String::new();
    doc.header.dim_linetype2_name = String::new();
    doc.header.dim_linetype_handle = Handle::NULL;
    doc.header.dim_linetype1_handle = Handle::NULL;
    doc.header.dim_linetype2_handle = Handle::NULL;
    for (what, back) in saves(&doc) {
        let h = back.header;
        for (variable, name, handle) in [
            ("DIMLTYPE", &h.dim_linetype_name, h.dim_linetype_handle),
            ("DIMLTEX1", &h.dim_linetype1_name, h.dim_linetype1_handle),
            ("DIMLTEX2", &h.dim_linetype2_name, h.dim_linetype2_handle),
        ] {
            assert_eq!(name, "", "{what}: {variable} name");
            assert!(handle.is_null(), "{what}: {variable} handle {handle:?}");
        }
    }
}

/// The name of the MATERIAL the header names, and of the visual styles it names.
fn header_names(doc: &CadDocument) -> [String; 4] {
    let h = &doc.header;
    let material = match doc.objects.get(&h.current_material_handle) {
        Some(ObjectType::Material(m)) => m.name.clone(),
        other => format!("not a material: {other:?}"),
    };
    [
        material,
        style_description(doc, h.interference_object_visual_style_handle),
        style_description(doc, h.interference_viewport_visual_style_handle),
        style_description(doc, h.drag_visual_style_handle),
    ]
}

/// A drawing AutoCAD wrote names its current material in a DWG header, and the CADCraft save names
/// the same one.
#[test]
fn an_autocad_dwg_keeps_its_current_material() {
    let source = include_bytes!("../data/clockwise_arc_edge.dwg");
    let before = header_names(&read_dwg(source));
    assert!(!before[0].starts_with("not a material"), "the fixture names its material: {before:?}");
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let saved = cadcraft_io::write_with(&cadcraft_io::read(source, "x.dwg").unwrap(), "x.dwg", &opts).unwrap();
    assert_eq!(header_names(&read_dwg(&saved)), before);
}
