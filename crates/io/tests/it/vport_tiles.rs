//! The VPORT table survives a save as AutoCAD saves it: a single `*Active` tile keeps its own
//! center, height and aspect (it is not replaced by a view of the drawing extents), and each tile
//! of a tiled configuration, which all share one name, keeps its own view direction, lens, UCS and
//! visual style. Through user DXF, DWG and a second save.

use cadcraft_dxf::Tag;

use super::foreign_passthrough::{dxf, saved_twice, section, val};

/// The VPORT records named `*Active` of `text`, in file order.
fn active_tiles(text: &str) -> Vec<Vec<Tag>> {
    let named = |t: &[Tag]| val(t, 2).eq_ignore_ascii_case("*Active");
    section(text, "TABLES").into_iter().filter(|(k, t)| k == "VPORT" && named(t)).map(|(_, t)| t).collect()
}

fn num(t: &[Tag], code: i32) -> f64 {
    val(t, code).trim().parse().unwrap_or(f64::NAN)
}

/// One `*Active` VPORT record, with the groups `more` after the aspect.
fn tile(handle: &str, min: (&str, &str), max: (&str, &str), center: (&str, &str), height: &str, more: &[(i32, &str)]) -> Vec<(i32, String)> {
    let mut g: Vec<(i32, &str)> = vec![
        (0, "VPORT"),
        (5, handle),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbViewportTableRecord"),
        (2, "*Active"),
        (70, "0"),
        (10, min.0),
        (20, min.1),
        (11, max.0),
        (21, max.1),
        (12, center.0),
        (22, center.1),
        (40, height),
        (41, "1.25"),
    ];
    g.extend(more);
    g.into_iter().map(|(c, v)| (c, v.to_string())).collect()
}

fn file(tables: &[Vec<(i32, String)>], objects: &[(i32, &str)]) -> String {
    let mut text = String::from("0\nSECTION\n2\nTABLES\n");
    let table = |name: &str, records: &[Vec<(i32, String)>]| {
        let mut t = format!("0\nTABLE\n2\n{name}\n70\n{}\n", records.len());
        t.extend(records.iter().flatten().map(|(c, v)| format!("{c}\n{v}\n")));
        t + "0\nENDTAB\n"
    };
    let ucs = |handle: &str, name: &str| -> Vec<(i32, String)> {
        let g = [(0, "UCS"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbUCSTableRecord"), (2, name), (70, "0")];
        let origin = [(10, "0"), (20, "0"), (30, "0"), (11, "1"), (21, "0"), (31, "0"), (12, "0"), (22, "1"), (32, "0")];
        g.into_iter().chain(origin).map(|(c, v)| (c, v.to_string())).collect()
    };
    text += &table("UCS", &[ucs("50", "Plan"), ucs("51", "Site")]);
    text += &table("VPORT", tables);
    text += "0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n";
    text += &dxf(&[(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_VISUALSTYLE"), (350, "60")]);
    text += &dxf(objects);
    text + "0\nENDSEC\n0\nEOF\n"
}

/// The visual styles `Wire` (61) and `Hidden` (62) of the named object dictionary.
const STYLES: &[(i32, &str)] = &[
    (0, "DICTIONARY"),
    (5, "60"),
    (330, "C"),
    (100, "AcDbDictionary"),
    (3, "Hidden"),
    (350, "62"),
    (3, "Wire"),
    (350, "61"),
    (0, "VISUALSTYLE"),
    (5, "61"),
    (330, "60"),
    (100, "AcDbVisualStyle"),
    (2, "Wire"),
    (70, "0"),
    (0, "VISUALSTYLE"),
    (5, "62"),
    (330, "60"),
    (100, "AcDbVisualStyle"),
    (2, "Hidden"),
    (70, "1"),
];

#[test]
fn a_single_active_viewport_keeps_its_center_height_and_aspect() {
    let source = file(&[tile("70", ("0", "0"), ("1", "1"), ("100", "200"), "50", &[(16, "0"), (26, "0"), (36, "1"), (42, "35")])], &[]);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let what = ["DXF", "DWG"][i];
        let tiles = active_tiles(&text);
        assert_eq!(tiles.len(), 1, "{what}: {text}");
        let t = &tiles[0];
        assert_eq!((num(t, 12), num(t, 22), num(t, 40), num(t, 41)), (100.0, 200.0, 50.0, 1.25), "{what}: center, height and aspect");
        assert_eq!(num(t, 42), 35.0, "{what}: lens");
        // A second save keeps them.
        let again = active_tiles(&cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()));
        assert_eq!(again.len(), 1, "{what}, second save");
        assert_eq!(
            (num(&again[0], 12), num(&again[0], 22), num(&again[0], 40), num(&again[0], 41)),
            (100.0, 200.0, 50.0, 1.25),
            "{what}, second save"
        );
    }
}

/// The name of the UCS record and of the visual style that group 345 and 348 of `tile` name.
fn named(text: &str, tile: &[Tag]) -> (String, String) {
    let ucs = section(text, "TABLES").into_iter().find(|(k, t)| k == "UCS" && val(t, 5) == val(tile, 345)).map(|(_, t)| val(&t, 2));
    let style = section(text, "OBJECTS").into_iter().find(|(k, t)| k == "VISUALSTYLE" && val(t, 5) == val(tile, 348)).map(|(_, t)| val(&t, 2));
    (ucs.unwrap_or_default(), style.unwrap_or_default())
}

fn assert_tiles(text: &str, what: &str) {
    let tiles = active_tiles(text);
    assert_eq!(tiles.len(), 2, "{what}: {text}");
    // (center x, view direction, lens, UCS name, visual style name) of each tile.
    let seen: Vec<_> = tiles.iter().map(|t| ((num(t, 12), num(t, 22)), (num(t, 16), num(t, 26), num(t, 36)), num(t, 42), named(text, t))).collect();
    let (plan, site) = (("Plan".to_string(), "Wire".to_string()), ("Site".to_string(), "Hidden".to_string()));
    assert_eq!(seen[0], ((10.0, 10.0), (0.0, 0.0, 1.0), 50.0, plan), "{what}: the first tile");
    assert_eq!(seen[1], ((30.0, 40.0), (1.0, 0.0, 0.0), 24.0, site), "{what}: the second tile");
}

#[test]
fn tiles_of_one_configuration_keep_their_own_view_ucs_and_visual_style() {
    let left = tile("70", ("0", "0"), ("0.5", "1"), ("10", "10"), "20", &[(16, "0"), (26, "0"), (36, "1"), (42, "50"), (345, "50"), (348, "61")]);
    let right = tile("71", ("0.5", "0"), ("1", "1"), ("30", "40"), "15", &[(16, "1"), (26, "0"), (36, "0"), (42, "24"), (345, "51"), (348, "62")]);
    let d = cadcraft_io::read_dxf(file(&[left, right], STYLES).as_bytes()).unwrap();
    assert_eq!(d.vports.first().map(|c| c.tiles.len()), Some(2), "both tiles are read");
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let what = ["DXF", "DWG"][i];
        assert_tiles(&text, what);
        let again = cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
        assert_tiles(&again, &format!("{what}, second save"));
    }
}

/// A tile the reader skips (its corners have no area) does not give its view to the next tile.
#[test]
fn a_skipped_tile_does_not_shift_the_views_of_the_next_ones() {
    let left = tile("70", ("0", "0"), ("0.5", "1"), ("10", "10"), "20", &[(16, "0"), (26, "0"), (36, "1"), (42, "50"), (345, "50"), (348, "61")]);
    let broken = tile("72", ("0.5", "0.5"), ("0.5", "0.5"), ("0", "0"), "5", &[(16, "0"), (26, "1"), (36, "0"), (42, "10")]);
    let right = tile("71", ("0.5", "0"), ("1", "1"), ("30", "40"), "15", &[(16, "1"), (26, "0"), (36, "0"), (42, "24"), (345, "51"), (348, "62")]);
    let d = cadcraft_io::read_dxf(file(&[left, broken, right], STYLES).as_bytes()).unwrap();
    assert_eq!(d.vports.first().map(|c| c.tiles.len()), Some(2), "the broken tile is skipped");
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        assert_tiles(&text, ["DXF", "DWG"][i]);
    }
}
