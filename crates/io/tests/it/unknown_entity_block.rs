//! An entity of a type CADCraft doesn't model is written back as read, so the anonymous block it
//! names stays too: an ARC_DIMENSION keeps its `*D` block through DXF and DWG saves, instead of
//! pointing at a block the writer dropped as unreferenced (AutoCAD then draws nothing for it).

/// A DXF as AutoCAD writes it: block `*D1` (a line and a text) and an ARC_DIMENSION that names it.
fn arc_dimension_dxf() -> String {
    let groups: &[(i32, &str)] = &[
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (5, "20"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockBegin"),
        (2, "*D1"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (3, "*D1"),
        (0, "LINE"),
        (5, "21"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "10"),
        (20, "0"),
        (30, "0"),
        (11, "10"),
        (21, "12"),
        (31, "0"),
        (0, "MTEXT"),
        (5, "22"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbMText"),
        (10, "5"),
        (20, "14"),
        (30, "0"),
        (40, "1"),
        (1, "15.71"),
        (0, "ENDBLK"),
        (5, "23"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockEnd"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "ARC_DIMENSION"),
        (5, "30"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbDimension"),
        (2, "*D1"),
        (10, "0"),
        (20, "12"),
        (30, "0"),
        (11, "5"),
        (21, "14"),
        (31, "0"),
        (70, "5"),
        (1, ""),
        (3, "Standard"),
        (100, "AcDbArcDimension"),
        (13, "10"),
        (23, "0"),
        (33, "0"),
        (14, "0"),
        (24, "10"),
        (34, "0"),
        (15, "0"),
        (25, "0"),
        (35, "0"),
        (16, "0"),
        (26, "0"),
        (36, "0"),
        (40, "0"),
        (41, "1.5707963267949"),
        (70, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// The names of the BLOCK definitions, and the group 2 of every ARC_DIMENSION, of an ASCII DXF.
fn blocks_and_arc_dims(dxf: &str) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let (mut blocks, mut dims, mut kind) = (Vec::new(), Vec::new(), "");
    for pair in lines.chunks(2) {
        let [code, value] = pair else { break };
        match *code {
            "0" => kind = value,
            "2" if kind == "BLOCK" => blocks.push(value.to_string()),
            "2" if kind == "ARC_DIMENSION" => dims.push(value.to_string()),
            _ => {}
        }
    }
    (blocks, dims)
}

#[test]
fn arc_dimension_keeps_its_dimension_block_through_dxf() {
    let d = cadcraft_io::read_dxf(arc_dimension_dxf().as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let (blocks, dims) = blocks_and_arc_dims(&text);
    assert_eq!(dims, ["*D1"], "{text}");
    assert!(blocks.iter().any(|b| b == "*D1"), "the ARC_DIMENSION's block was dropped: {blocks:?}");
    // And again: a second save keeps it as well.
    let (blocks, _) = blocks_and_arc_dims(&cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()));
    assert!(blocks.iter().any(|b| b == "*D1"), "{blocks:?}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn arc_dimension_keeps_its_dimension_block_through_dwg() {
    let d = cadcraft_io::read_dxf(arc_dimension_dxf().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let back = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
    // The DWG writer numbers anonymous blocks again (`*D1` can come back as `*D0`): the dimension
    // must name a block that exists.
    let (blocks, dims) = blocks_and_arc_dims(&String::from_utf8_lossy(&back));
    assert_eq!(dims.len(), 1, "{dims:?}");
    assert!(dims.iter().all(|n| n.starts_with("*D") && blocks.contains(n)), "{dims:?} not in {blocks:?}");
}

/// An OLE2FRAME (an embedded object, such as an Excel table) with `n` bytes of data, in 127-byte
/// 310 groups as AutoCAD writes them.
fn ole_dxf(data: &[u8]) -> String {
    let hex: String = data.iter().map(|b| format!("{b:02X}")).collect();
    let mut groups: Vec<(i32, String)> = [
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "OLE2FRAME"),
        (5, "40"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbOle2Frame"),
        (70, "2"),
        (3, "Microsoft Excel Worksheet"),
        (10, "0"),
        (20, "50"),
        (30, "0"),
        (11, "80"),
        (21, "0"),
        (31, "0"),
        (71, "2"),
        (72, "0"),
    ]
    .iter()
    .map(|(c, v)| (*c, v.to_string()))
    .collect();
    groups.push((90, data.len().to_string()));
    for chunk in hex.as_bytes().chunks(254) {
        groups.push((310, String::from_utf8_lossy(chunk).into_owned()));
    }
    groups.extend([(1, "OLE"), (0, "ENDSEC"), (0, "EOF")].iter().map(|(c, v)| (*c, v.to_string())));
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// The 310 data of the first OLE2FRAME of an ASCII DXF.
fn ole_data(dxf: &str) -> String {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let mut inside = false;
    let mut out = String::new();
    for pair in lines.chunks(2) {
        let [code, value] = pair else { break };
        if *code == "0" {
            if inside {
                break;
            }
            inside = *value == "OLE2FRAME";
        } else if inside && *code == "310" {
            out.push_str(value);
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn unknown_dwg_types_are_checked_at_2000_and_2004_targets() {
    for version in [cadcraft_io::Version::R2000, cadcraft_io::Version::R2004] {
        let opts = cadcraft_io::SaveOptions { version: Some(version), ..Default::default() };
        let arc = cadcraft_io::read_dxf(arc_dimension_dxf().as_bytes()).unwrap();
        let bytes = cadcraft_io::write_with(&arc, "x.dwg", &opts).unwrap();
        assert_eq!(bytes.get(..6), Some(version.code().as_bytes()), "bridge must use the actual requested DWG target");
        let text = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&bytes).unwrap()).unwrap();
        let (blocks, dims) = blocks_and_arc_dims(&text);
        assert_eq!(dims.len(), 1, "ARC_DIMENSION was dropped at {}", version.code());
        assert!(dims.iter().all(|name| blocks.contains(name)), "ARC_DIMENSION lost its geometry block at {}", version.code());
        let ole = cadcraft_io::read_dxf(ole_dxf(&[1, 2, 3, 4]).as_bytes()).unwrap();
        let bytes = cadcraft_io::write_with(&ole, "x.dwg", &opts).unwrap();
        let text = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&bytes).unwrap()).unwrap();
        assert_eq!(ole_data(&text), "01020304", "OLE2FRAME data was dropped at {}", version.code());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ole_frame_keeps_its_data_through_dwg() {
    let data: Vec<u8> = (0..700u32).map(|i| (i * 37 % 251) as u8).collect();
    let source = ole_dxf(&data);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let back = String::from_utf8_lossy(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).into_owned();
    assert_eq!(ole_data(&back), ole_data(&source));
    assert_eq!(ole_data(&source).len(), 1400);
}
