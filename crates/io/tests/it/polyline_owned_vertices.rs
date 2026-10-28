//! The DWG bridge must not add a last-to-first segment to open heavy polylines.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dwg_heavy_polyline_vertex_count_and_closed_flag_survive_the_full_bridge() {
    use std::io::Cursor;

    use acadrust::entities::{EntityType, Polyline2D, PolylineFlags, Vertex2D};
    use acadrust::{CadDocument, DwgReader, DwgWriter, DxfVersion, Vector3};
    use cadcraft_doc::EntityKind;
    use cadcraft_geom::Vec2;

    for version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        for flags in [0, 1, 128, 129] {
            let mut source = CadDocument::with_version(version);
            let points = [(0.0, 0.0), (0.0, 3.0), (5.0, 3.0)];
            let mut polyline = Polyline2D::new();
            polyline.flags = PolylineFlags::from_bits(flags);
            polyline.vertices = points.iter().map(|&(x, y)| Vertex2D::new(Vector3::new(x, y, 0.0))).collect();
            let handle = source.add_entity(EntityType::Polyline2D(polyline)).unwrap();
            let bytes = DwgWriter::write_to_vec(&source).unwrap();
            let read = DwgReader::from_stream(Cursor::new(bytes.clone())).read().unwrap();
            let EntityType::Polyline2D(polyline) = read.get_entity(handle).unwrap() else { panic!("heavy source polyline") };
            assert_eq!(polyline.vertices.len(), points.len());
            let drawing = cadcraft_io::read(&bytes, "source.dwg").unwrap();
            let bytes = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
            for drawing in [drawing, cadcraft_io::read(&bytes, "saved.dwg").unwrap()] {
                let entity = drawing.entity(cadcraft_doc::Handle(handle.value())).unwrap();
                let EntityKind::LwPolyline(polyline) = &entity.kind else { panic!("lowered polyline") };
                assert_eq!(polyline.vertices.iter().map(|v| v.p).collect::<Vec<_>>(), points.map(|(x, y)| Vec2::new(x, y)));
                assert_eq!(polyline.closed, flags & 1 != 0);
                assert_eq!(polyline.plinegen, flags & 128 != 0);
            }
        }
    }
}
