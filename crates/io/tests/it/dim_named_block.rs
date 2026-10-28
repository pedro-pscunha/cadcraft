//! A dimension's graphics block does not need a `*D` name: other programs name it freely
//! (Plaenge Altier: 22 dimensions on `DIMBLOCK1-TÉRREO - …`). A save keeps that block as the
//! dimension's block, as read. It used to draw the dimension again into a new `*D` block and keep
//! the named one as an ordinary block that nothing used.

/// (code, value) pairs of an ASCII DXF.
fn pairs(dxf: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = dxf.lines().collect();
    lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim().to_string()))).collect()
}

/// A linear dimension from (0,0) to (10,0) whose graphics block `DIMBLOCK1-X` holds one marker
/// line CADCraft would never draw (from (3,7) to (4,8)).
fn named_block_dxf() -> String {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n\
     0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nDIMBLOCK1-X\n70\n0\n10\n0\n20\n0\n30\n0\n3\nDIMBLOCK1-X\n1\n\n\
     0\nLINE\n8\n0\n10\n3\n20\n7\n30\n0\n11\n4\n21\n8\n31\n0\n0\nENDBLK\n8\n0\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n0\nDIMENSION\n5\n40\n8\n0\n2\nDIMBLOCK1-X\n10\n10\n20\n5\n30\n0\n11\n5\n21\n6\n31\n0\n\
     70\n0\n1\n\n3\nStandard\n13\n0\n23\n0\n33\n0\n14\n10\n24\n0\n34\n0\n0\nENDSEC\n0\nEOF\n"
        .to_string()
}

#[test]
fn a_named_dimension_block_stays_the_dimensions_block() {
    let d = cadcraft_io::read_dxf(named_block_dxf().as_bytes()).unwrap();
    let out = cadcraft_io::write_dxf(&d);
    let p = pairs(&out);
    let mut kind = String::new();
    let (mut blocks, mut dim_refs) = (Vec::new(), Vec::new());
    for (code, v) in &p {
        if *code == 0 {
            kind = v.clone();
        } else if *code == 2 && kind == "BLOCK" {
            blocks.push(v.clone());
        } else if *code == 2 && kind == "DIMENSION" {
            dim_refs.push(v.clone());
        }
    }
    assert_eq!(dim_refs, ["DIMBLOCK1-X"], "the dimension keeps its block");
    assert!(!blocks.iter().any(|b| b.starts_with("*D")), "no regenerated *D block: {blocks:?}");
    assert_eq!(blocks.iter().filter(|b| *b == "DIMBLOCK1-X").count(), 1, "the block is written once");
    let marker = p
        .windows(4)
        .any(|w| w[0] == (10, "3".into()) && w[1] == (20, "7".into()) || w[0].0 == 10 && w[0].1.starts_with("3.0") && w[1].1.starts_with("7.0"));
    assert!(marker, "the block keeps its own geometry");
}

#[test]
fn a_named_dimension_block_survives_a_dwg_save() {
    let d = cadcraft_io::read_dxf(named_block_dxf().as_bytes()).unwrap();
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    let dim = back
        .model
        .iter()
        .find_map(|e| if let cadcraft_doc::EntityKind::Dimension(dm) = &e.kind { Some(dm.clone()) } else { None })
        .expect("the dimension");
    assert_eq!(dim.block.as_deref(), Some("DIMBLOCK1-X"));
    assert_eq!(back.blocks.keys().filter(|k| k.starts_with("*D")).count(), 0, "no extra *D block");
}
