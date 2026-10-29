//! Arrowhead blocks that multileaders, multileader styles and leader overrides name are kept
//! through a save. Citycon Augusto Miele: 18 multileaders and 3 multileader styles with the
//! `_None` arrowhead came back with no arrowhead block, so AutoCAD drew its default filled
//! arrow on each; Even Manuel de Nobrega: leaders whose DSTYLE override sets DIMLDRBLK to
//! `_None` came back pointing at the null handle.

use cadcraft_doc::*;
use cadcraft_geom::Vec3;

/// (code, value) pairs of an ASCII DXF.
fn pairs(dxf: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = dxf.lines().collect();
    lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim().to_string()))).collect()
}

/// The handle of the BLOCK_RECORD named `name`.
fn block_record(p: &[(i32, String)], name: &str) -> Option<String> {
    let mut kind = "";
    let mut handle = None;
    for (c, v) in p {
        match c {
            0 => {
                kind = v;
                handle = None;
            }
            5 if kind == "BLOCK_RECORD" => handle = Some(v.clone()),
            2 if kind == "BLOCK_RECORD" && v.eq_ignore_ascii_case(name) => return handle,
            _ => {}
        }
    }
    None
}

/// The groups of the first record of type `kind`.
fn record<'a>(p: &'a [(i32, String)], kind: &str) -> &'a [(i32, String)] {
    let Some(start) = p.iter().position(|(c, v)| *c == 0 && v == kind) else { return &[] };
    let end = p.iter().skip(start + 1).position(|(c, _)| *c == 0).map_or(p.len(), |n| start + 1 + n);
    &p[start + 1..end]
}

fn mleader() -> MLeader {
    MLeader {
        leaders: vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 3.0, 0.0)]],
        landing: Vec3::new(5.0, 4.0, 0.0),
        dogleg: 0.75,
        text: Some(MText {
            insert: Vec3::new(5.9, 4.0, 0.0),
            height: 0.4,
            width: 0.0,
            attach: 4,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "NOTE".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
        }),
        style: "NoArrow".into(),
        arrow_size: 0.3,
        spline: false,
        block: None,
        arrow_block: "_None".into(),
    }
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.mleader_styles.push(MLeaderStyle { name: "NoArrow".into(), arrow_block: "_None".into(), ..MLeaderStyle::metric() });
    d.model.push(Entity::new(Handle(0x80), EntityKind::MLeader(mleader())));
    d
}

fn arrows(d: &Drawing) -> (Vec<String>, Option<String>) {
    let ents = d.model.iter().filter_map(|e| if let EntityKind::MLeader(m) = &e.kind { Some(m.arrow_block.clone()) } else { None }).collect();
    (ents, d.mleader_styles.iter().find(|s| s.name == "NoArrow").map(|s| s.arrow_block.clone()))
}

#[test]
fn multileader_arrow_blocks_survive_dxf_and_dwg() {
    let d = drawing();
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2007), binary: false };
    let text = String::from_utf8(cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap()).unwrap();
    let p = pairs(&text);
    let none = block_record(&p, "_None").expect("the _None arrowhead block is written");
    assert!(record(&p, "MULTILEADER").iter().any(|(c, v)| *c == 342 && *v == none), "MULTILEADER 342 names _None");
    let styles: Vec<&[(i32, String)]> = p.split(|(c, _)| *c == 0).filter(|r| r.iter().any(|(c, v)| *c == 100 && v == "AcDbMLeaderStyle")).collect();
    assert_eq!(styles.iter().filter(|r| r.iter().any(|(c, v)| *c == 341 && *v == none)).count(), 1, "one MLEADERSTYLE 341 names _None");
    let want = (vec!["_None".to_string()], Some("_None".to_string()));
    assert_eq!(arrows(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()), want, "DXF");
    let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
    let back = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    assert_eq!(arrows(&back), want, "DWG");
    // A second save writes the same block again, not a second one.
    let again = pairs(&cadcraft_io::write_dxf(&back));
    assert_eq!(again.iter().filter(|(c, v)| *c == 2 && v == "_None").count(), 2, "one BLOCK_RECORD and one BLOCK");
}

/// A leader whose `ACAD` DSTYLE override names the `_None` block record (DIMLDRBLK, 341) and the
/// null handle for DIMTXSTY (340), as AutoCAD writes them.
fn leader_dxf() -> String {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n\
     0\nSECTION\n2\nTABLES\n\
     0\nTABLE\n2\nAPPID\n5\n9\n70\n1\n0\nAPPID\n5\n12\n100\nAcDbSymbolTableRecord\n100\nAcDbRegAppTableRecord\n2\nACAD\n70\n0\n0\nENDTAB\n\
     0\nTABLE\n2\nBLOCK_RECORD\n5\n1\n70\n1\n\
     0\nBLOCK_RECORD\n5\n50\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\n_None\n0\nENDTAB\n0\nENDSEC\n\
     0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n51\n330\n50\n8\n0\n2\n_None\n70\n0\n10\n0\n20\n0\n30\n0\n3\n_None\n1\n\n0\nENDBLK\n5\n52\n330\n50\n8\n0\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n0\nLEADER\n5\n60\n100\nAcDbEntity\n8\n0\n100\nAcDbLeader\n3\nStandard\n71\n1\n72\n0\n76\n2\n\
     10\n0\n20\n0\n30\n0\n10\n5\n20\n5\n30\n0\n\
     1001\nACAD\n1000\nDSTYLE\n1002\n{\n1070\n341\n1005\n50\n1070\n340\n1005\n0\n1070\n41\n1040\n2.5\n1002\n}\n\
     0\nENDSEC\n0\nEOF\n"
        .to_string()
}

/// The value after `1070 <var>` in a record's DSTYLE xdata (None when the next group is not a 1005).
fn dstyle_handle(rec: &[(i32, String)], var: &str) -> Option<String> {
    let i = rec.iter().position(|(c, v)| *c == 1070 && v == var)?;
    rec.get(i + 1).filter(|(c, _)| *c == 1005).map(|(_, v)| v.clone())
}

#[test]
fn a_leader_override_keeps_its_arrowhead_block() {
    let d = cadcraft_io::read_dxf(leader_dxf().as_bytes()).unwrap();
    let out = cadcraft_io::write_dxf(&d);
    let p = pairs(&out);
    let none = block_record(&p, "_None").expect("the _None block the leader names is written");
    let leader = record(&p, "LEADER");
    assert_eq!(dstyle_handle(leader, "341").as_deref(), Some(none.as_str()), "DIMLDRBLK still names _None");
    assert_eq!(dstyle_handle(leader, "340").as_deref(), Some("0"), "the null DIMTXSTY stays a 1005 0 pair");
    // Through DWG too: the DSTYLE pairs stay whole.
    let dwg = cadcraft_dwg::dxf_to_dwg(out.as_bytes()).unwrap();
    let back = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    let bp = pairs(&back);
    let none = block_record(&bp, "_None").expect("DWG: the _None block is kept");
    let leader = record(&bp, "LEADER");
    assert_eq!(dstyle_handle(leader, "341").as_deref(), Some(none.as_str()), "DWG: DIMLDRBLK");
    assert_eq!(dstyle_handle(leader, "340").as_deref(), Some("0"), "DWG: the null DIMTXSTY pair");
}
