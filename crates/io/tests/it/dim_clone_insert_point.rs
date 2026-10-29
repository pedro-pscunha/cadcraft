//! A dimension's group 12 (DXF Reference: insertion point for clones of a dimension) moves its
//! `*D` block: AutoCAD draws the block's content shifted by that point. A dimension copied inside
//! a block keeps its old graphics and records the shift there (DP Inc PAIS-AP-R005: 4 such
//! dimensions, one block 5022 units away). CADCraft drew those graphics unshifted, and its save
//! wrote group 12 as zero, so AutoCAD then drew them far away too.

use cadcraft_doc::{EntityKind, Space};

/// A linear dimension from (0,0) to (10,0), line at y 5, whose `*D1` block holds its graphics
/// 1000 / 5000 units away, with group 12 shifting them back.
fn shifted_dimension_dxf() -> String {
    let line = |x1: f64, y1: f64, x2: f64, y2: f64| format!("0\nLINE\n8\n0\n10\n{x1}\n20\n{y1}\n30\n0\n11\n{x2}\n21\n{y2}\n31\n0\n");
    let (dx, dy) = (1000.0, 5000.0);
    let graphics = [line(dx, dy + 5.0, dx + 10.0, dy + 5.0), line(dx, dy, dx, dy + 5.0), line(dx + 10.0, dy, dx + 10.0, dy + 5.0)].concat();
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n\
         0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\n*D1\n70\n1\n10\n0\n20\n0\n30\n0\n3\n*D1\n1\n\n{graphics}0\nENDBLK\n8\n0\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n0\nDIMENSION\n5\n40\n8\n0\n2\n*D1\n10\n10\n20\n5\n30\n0\n11\n5\n21\n6\n31\n0\n\
         12\n{}\n22\n{}\n32\n0\n70\n0\n1\n\n3\nStandard\n13\n0\n23\n0\n33\n0\n14\n10\n24\n0\n34\n0\n0\nENDSEC\n0\nEOF\n",
        -dx, -dy
    )
}

fn check_in_place(d: &cadcraft_doc::Drawing, what: &str) {
    let ext = d.extents(&Space::Model);
    assert!(ext.max.x <= 11.0 && ext.max.y <= 6.0 && ext.min.x >= -1.0, "{what}: extents {ext:?}");
    let dim = d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm) } else { None }).expect("the dimension");
    let name = dim.block.clone().expect("its graphics block is kept");
    let block = d.blocks.get(name.as_str()).expect("the graphics block");
    for e in block.entities.iter() {
        if let EntityKind::Line(l) = &e.kind {
            assert!(l.a.y.abs() <= 5.0 + 1e-6 && l.a.x.abs() <= 10.0 + 1e-6, "{what}: graphics line at {:?}", l.a);
        }
    }
}

#[test]
fn a_dimensions_clone_point_moves_its_graphics_into_place() {
    let d = cadcraft_io::read_dxf(shifted_dimension_dxf().as_bytes()).unwrap();
    check_in_place(&d, "read");
}

#[test]
fn unsupported_graphics_keep_their_source_data_and_report_the_projection_limit() {
    let source = shifted_dimension_dxf()
        .replace("0\nENDBLK", "0\nSHAPE\n5\n99\n8\n0\n100\nAcDbEntity\n100\nAcDbShape\n10\n1000\n20\n5000\n30\n0\n40\n1\n2\nsynthetic\n0\nENDBLK");
    let drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    check_in_place(&drawing, "known graphics");
    assert!(drawing.read_notes.iter().any(|note| note.contains("99") && note.contains("unsupported geometry")));
    let unknown = drawing
        .blocks
        .values()
        .flat_map(|block| block.entities.iter())
        .find_map(|entity| match &entity.kind {
            EntityKind::Unknown(unknown) => Some(unknown),
            _ => None,
        })
        .unwrap();
    assert_eq!(unknown.tags.iter().find(|tag| tag.code == 10).unwrap().value.parse::<f64>().unwrap().to_bits(), 1000.0_f64.to_bits());
}

#[test]
fn the_graphics_stay_in_place_through_dxf_and_dwg_saves() {
    let d = cadcraft_io::read_dxf(shifted_dimension_dxf().as_bytes()).unwrap();
    let dxf = cadcraft_io::write_dxf(&d);
    check_in_place(&cadcraft_io::read_dxf(dxf.as_bytes()).unwrap(), "DXF save");
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    check_in_place(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "DWG save");
}
