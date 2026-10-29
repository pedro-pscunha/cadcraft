//! Every canonical TOLERANCE and TABLE layout field is part of the replay key, so changing it alone retires the source
//! snapshot; a property edit (frame text, style, cell text, formula) keeps the key.

use cadcraft_doc::{Common, EntityKind, Table, TableCell, Tolerance, geometry_key};
use cadcraft_geom::Vec3;

fn tolerance() -> Tolerance {
    Tolerance { insert: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.6, 0.8), style: "Standard".into(), text: "{x}".into() }
}

fn table() -> Table {
    let cell = |text: &str| TableCell { text: text.into(), ..TableCell::default() };
    Table {
        insert: Vec3::new(1.0, 2.0, 3.0),
        dir: Vec3::new(0.0, 0.6, 0.8),
        col_widths: vec![2.0, 3.0],
        row_heights: vec![1.0, 1.5],
        cells: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
        ..Table::default()
    }
}

fn all_distinct(keys: &[String]) {
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn each_tolerance_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 0.8, 0.6), ..Common::default() };
    let key = |t: &Tolerance, c: &Common| geometry_key(&EntityKind::Tolerance(t.clone()), c).unwrap();
    let mutations: Vec<(&str, Box<dyn Fn(&mut Tolerance, &mut Common)>)> = vec![
        ("insert z", Box::new(|t, _| t.insert.z += 0.5)),
        ("direction x", Box::new(|t, _| t.dir.x += 0.1)),
        ("direction z", Box::new(|t, _| t.dir.z += 0.1)),
        ("normal x", Box::new(|_, c| c.extrusion.x += 0.5)),
        ("normal z", Box::new(|_, c| c.extrusion.z += 0.5)),
    ];
    let mut keys = vec![key(&tolerance(), &common)];
    for (name, mutate) in &mutations {
        let (mut t, mut c) = (tolerance(), common.clone());
        mutate(&mut t, &mut c);
        keys.push(key(&t, &c));
        assert_ne!(keys[0], *keys.last().unwrap(), "{name}");
    }
    all_distinct(&keys);
    let mut edited = tolerance();
    edited.text = "{y}".into();
    edited.style = "Other".into();
    assert_eq!(key(&edited, &common), keys[0], "the frame text and the style are properties");
}

#[test]
fn each_table_layout_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 0.8, 0.6), ..Common::default() };
    let key = |t: &Table, c: &Common| geometry_key(&EntityKind::Table(t.clone()), c).unwrap();
    let mutations: Vec<(&str, Box<dyn Fn(&mut Table, &mut Common)>)> = vec![
        ("insert z", Box::new(|t, _| t.insert.z += 0.5)),
        ("direction x", Box::new(|t, _| t.dir.x += 0.1)),
        ("direction z", Box::new(|t, _| t.dir.z += 0.1)),
        ("column width", Box::new(|t, _| t.col_widths[1] += 0.5)),
        ("row height", Box::new(|t, _| t.row_heights[0] += 0.5)),
        ("merge", Box::new(|t, _| t.cells[0][0].merged = Some((1, 2)))),
        ("cell text height", Box::new(|t, _| t.cells[1][1].text_height = Some(0.3))),
        ("normal x", Box::new(|_, c| c.extrusion.x += 0.5)),
        ("normal z", Box::new(|_, c| c.extrusion.z += 0.5)),
    ];
    let mut keys = vec![key(&table(), &common)];
    for (name, mutate) in &mutations {
        let (mut t, mut c) = (table(), common.clone());
        mutate(&mut t, &mut c);
        keys.push(key(&t, &c));
        assert_ne!(keys[0], *keys.last().unwrap(), "{name}");
    }
    all_distinct(&keys);
    let mut edited = table();
    edited.cells[0][0].text = "changed".into();
    edited.cells[1][0].formula = Some("=A1".into());
    edited.style = "Other".into();
    edited.title = true;
    assert_eq!(key(&edited, &common), keys[0], "cell text, formulas and style are content properties");
}
