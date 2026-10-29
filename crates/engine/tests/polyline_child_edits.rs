use cadcraft_engine::Session;
use serde_json::json;

fn source() -> &'static [u8] {
    b"0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n0\nAPPID\n2\nVENDOR\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n70\n0\n0\nVERTEX\n5\n101\n8\n0\n10\n0\n20\n0\n1001\nVENDOR\n1000\nA\n0\nVERTEX\n5\n102\n8\n0\n10\n5\n20\n3\n1001\nVENDOR\n1000\nB\n0\nVERTEX\n5\n103\n8\n0\n10\n10\n20\n0\n1001\nVENDOR\n1000\nC\n0\nSEQEND\n5\n190\n8\n0\n0\nENDSEC\n0\nEOF\n"
}

#[test]
fn polyline_edits_keep_each_surviving_vertex_identity_with_its_data() {
    for (command, params, retained) in [
        ("move", json!({"handles": ["100"], "delta": [3, 4]}), vec![("101", "A"), ("102", "B"), ("103", "C")]),
        ("reverse", json!({"handles": ["100"]}), vec![("101", "A"), ("102", "B"), ("103", "C")]),
        (
            "pedit",
            json!({"handles": ["100"], "option": "vertex", "action": "insert", "index": 0, "to": [2, 1]}),
            vec![("101", "A"), ("102", "B"), ("103", "C")],
        ),
        ("grip.option", json!({"handle": "100", "index": 2, "option": "removeVertex"}), vec![("101", "A"), ("103", "C")]),
    ] {
        let mut session = Session::new();
        session.open_drawing(cadcraft_io::read_dxf(source()).unwrap(), "x.dxf", None);
        session.execute(command, &params).unwrap();
        let output = cadcraft_io::write_dxf(session.doc().unwrap());
        let tags = cadcraft_dxf::parse(output.as_bytes()).unwrap();
        let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|section| cadcraft_dxf::records(&section.tags)).collect();
        for (handle, marker) in retained {
            let (_, vertex) = records
                .iter()
                .find(|(kind, tags)| kind == "VERTEX" && tags.iter().any(|t| t.code == 5 && t.str() == handle))
                .unwrap_or_else(|| panic!("{command}: missing vertex {handle}"));
            assert!(vertex.iter().any(|t| t.code == 1000 && t.str() == marker), "{command}: vertex {handle} owns {marker}");
            let x = vertex.iter().find(|t| t.code == 10).unwrap().f64();
            let original_x = match marker {
                "A" => 0.0,
                "B" => 5.0,
                "C" => 10.0,
                _ => unreachable!(),
            };
            assert_eq!(x, original_x + if command == "move" { 3.0 } else { 0.0 }, "{command}: vertex {handle} geometry");
        }
    }
}
