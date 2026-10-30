//! The text generation flags (group 71: 2 backward, 4 upside down) of a TEXT, an ATTDEF and an ATTRIB read from a file are
//! data: they survive a DXF save and a DWG save, unedited, after a property-only edit and after a MOVE.

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{Mat4, Vec3};

fn dxf(flags: i64) -> String {
    format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nB\n70\n0\n10\n0\n20\n0\n30\n0\n3\nB\n1\n\n\
         0\nATTDEF\n5\n20\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\nDEF\n71\n{flags}\n7\nStandard\n100\nAcDbAttributeDefinition\n3\nPrompt\n2\nTAG\n70\n0\n\
         0\nENDBLK\n8\n0\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n\
         0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n5\n20\n3\n30\n0\n40\n2\n1\nHELLO\n71\n{flags}\n7\nStandard\n100\nAcDbText\n\
         0\nINSERT\n5\n50\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockReference\n66\n1\n2\nB\n10\n10\n20\n0\n30\n0\n\
         0\nATTRIB\n5\n60\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n10\n20\n0\n30\n0\n40\n1\n1\nVAL\n71\n{flags}\n7\nStandard\n100\nAcDbAttribute\n2\nTAG\n70\n0\n\
         0\nSEQEND\n5\n70\n100\nAcDbEntity\n8\n0\n\
         0\nENDSEC\n0\nEOF\n"
    )
}

/// The first group 71 of each TEXT, ATTDEF and ATTRIB record of a DXF, in file order, as (kind, value).
fn flags(text: &str) -> Vec<(String, Option<i64>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let mut out = Vec::new();
    for section in sections(&tags) {
        for (kind, rec) in records(&section.tags) {
            if matches!(kind.as_str(), "TEXT" | "ATTDEF" | "ATTRIB") {
                let at = rec.iter().position(|t: &Tag| t.code == 100 && t.str().starts_with("AcDbAttribute")).unwrap_or(rec.len());
                out.push((kind, rec[..at].iter().find(|t| t.code == 71).map(|t| t.i64())));
            }
        }
    }
    out.sort();
    out
}

fn expected(flags: i64) -> Vec<(String, Option<i64>)> {
    ["ATTDEF", "ATTRIB", "TEXT"].iter().map(|k| (k.to_string(), Some(flags))).collect()
}

fn saved_dxf(d: &Drawing) -> String {
    String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap()
}

fn through_dwg(d: &Drawing) -> String {
    String::from_utf8(cadcraft_dwg::dwg_to_dxf(&cadcraft_io::write(d, "x.dwg").unwrap()).unwrap()).unwrap()
}

#[test]
fn the_generation_flags_survive_a_dxf_and_a_dwg_save_unedited() {
    for f in [2, 4, 6] {
        let d = cadcraft_io::read_dxf(dxf(f).as_bytes()).unwrap();
        assert_eq!(flags(&saved_dxf(&d)), expected(f), "flags {f}: DXF save");
        assert_eq!(flags(&through_dwg(&d)), expected(f), "flags {f}: DWG save");
    }
}

#[test]
fn the_generation_flags_survive_a_layer_change_and_a_move() {
    for f in [2, 4, 6] {
        let mut d = cadcraft_io::read_dxf(dxf(f).as_bytes()).unwrap();
        let handles: Vec<_> = d.model.iter().map(|e| e.handle).collect();
        // A property-only edit.
        for h in &handles {
            d.model.modify_metadata(*h, |c| c.layer = "0".into()).unwrap();
        }
        assert_eq!(flags(&saved_dxf(&d)), expected(f), "flags {f}: after a layer change");
        // A MOVE of the text and of the block reference with its attribute.
        let shift = Mat4 { m: [[1.0, 0.0, 0.0, 4.0], [0.0, 1.0, 0.0, 2.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        for h in &handles {
            let moved = d.model.get(*h).unwrap().transformed(&shift).unwrap();
            assert!(d.model.replace(moved));
        }
        assert_eq!(flags(&saved_dxf(&d)), expected(f), "flags {f}: after a MOVE (DXF)");
        assert_eq!(flags(&through_dwg(&d)), expected(f), "flags {f}: after a MOVE (DWG)");
        let moved_text = d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.insert) } else { None }).unwrap();
        assert_eq!(moved_text, Vec3::new(9.0, 5.0, 0.0));
    }
}
