//! The VERTEX flags (group 70) of a 3D polyline survive a save: 32 marks a 3D polyline vertex, 8 adds a spline frame control point.
//! A vertex made in CADCraft keeps writing 32.

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, Polyline3d, PolylineRecord, PolylineRecords};
use cadcraft_geom::Vec3;

fn flags_of(d: &Drawing) -> Vec<Option<i64>> {
    d.model
        .iter()
        .find_map(|e| if let EntityKind::Polyline3d(p) = &e.kind { Some(p.records.vertices.iter().map(|r| r.flags).collect()) } else { None })
        .expect("a 3D polyline")
}

fn drawing(flags: [Option<i64>; 3]) -> Drawing {
    let mut d = Drawing::new_metric();
    let records = PolylineRecords {
        vertices: flags
            .into_iter()
            .zip(0x7001..)
            .map(|(flags, h)| PolylineRecord { handle: Some(Handle(h)), common: Common::default(), flags })
            .collect(),
        seqend: None,
    };
    let points = vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 4.0), Vec3::new(5.0, 1.0, -2.0)];
    d.model.push(Entity::new(Handle(0x7000), EntityKind::Polyline3d(Polyline3d { records, points, closed: false })));
    d
}

fn written_flags(d: &Drawing) -> Vec<String> {
    let text = String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out = Vec::new();
    let mut in_vertex = false;
    for pair in lines.chunks(2) {
        if let [code, value] = pair {
            if *code == "0" {
                in_vertex = *value == "VERTEX";
            } else if in_vertex && *code == "70" {
                out.push((*value).to_string());
            }
        }
    }
    out
}

#[test]
fn source_vertex_flags_reach_the_dxf_and_come_back_from_the_dxf_and_the_dwg() {
    let d = drawing([Some(32), Some(40), Some(32 | 128)]);
    assert_eq!(written_flags(&d), ["32", "40", "160"]);
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    assert_eq!(flags_of(&back), [Some(32), Some(40), Some(160)], "DXF round trip");
    let again = cadcraft_io::read(&cadcraft_io::write(&back, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(flags_of(&again), [Some(32), Some(40), Some(160)], "DWG round trip");
}

#[test]
fn a_vertex_made_in_cadcraft_writes_32() {
    assert_eq!(written_flags(&drawing([None, None, None])), ["32", "32", "32"]);
}
