//! A spline-fit 3D polyline keeps its fit data through a save: the polyline flags (group 70 bit 4), the curve type (75), the
//! frame control points (vertex flag 16) and the generated curve (vertex flag 8). A whole-polyline move keeps it all with the
//! polyline; reshaping the polyline drops it (it no longer describes the curve). A vertex made in CADCraft writes 32.

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, Polyline3d, Polyline3dFit, PolylineFitVertex, PolylineRecord};
use cadcraft_geom::Vec3;

fn vertex(h: u64, p: Vec3, flags: i64) -> PolylineFitVertex {
    let record = PolylineRecord { handle: Some(Handle(h)), common: Common::default() };
    PolylineFitVertex { record, position: p, flags, start_width: 0.0, end_width: 0.0, bulge: 0.0, tangent: 0.0 }
}

/// Four frame control points (16 | 32) and the three points of the generated curve between (8 | 32).
fn spline_fit() -> Polyline3d {
    let frame = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 1.0), Vec3::new(6.0, 4.0, 2.0), Vec3::new(9.0, 0.0, 3.0)];
    let curve = [Vec3::new(1.0, 1.5, 0.4), Vec3::new(4.5, 3.5, 1.5), Vec3::new(8.0, 1.5, 2.6)];
    let vertices: Vec<_> =
        (0x7001..).zip(frame.iter().map(|p| (*p, 48)).chain(curve.iter().map(|p| (*p, 40)))).map(|(h, (p, f))| vertex(h, p, f)).collect();
    let points = curve.to_vec();
    Polyline3d {
        records: Default::default(),
        points: points.clone(),
        closed: false,
        fit: Some(Polyline3dFit { flags: 8 | 4, smooth_type: 6, vertices, display_points: points }),
    }
}

fn drawing(p: Polyline3d) -> Drawing {
    let mut d = Drawing::new_metric();
    d.model.push(Entity::new(Handle(0x7000), EntityKind::Polyline3d(p)));
    d
}

fn polyline(d: &Drawing) -> Polyline3d {
    d.model.iter().find_map(|e| if let EntityKind::Polyline3d(p) = &e.kind { Some(p.clone()) } else { None }).expect("a 3D polyline")
}

fn reread(d: &Drawing, name: &str) -> Polyline3d {
    polyline(&cadcraft_io::read(&cadcraft_io::write(d, name).unwrap(), name).unwrap())
}

fn flags(p: &Polyline3d) -> Vec<i64> {
    p.fit.as_ref().map(|f| f.vertices.iter().map(|v| v.flags).collect()).unwrap_or_default()
}

#[test]
fn a_spline_fit_polyline_keeps_its_frame_its_curve_and_its_flags() {
    let d = drawing(spline_fit());
    let want = spline_fit();
    let back = reread(&d, "x.dxf");
    assert_eq!(back.points, want.points, "DXF points");
    let fit = back.fit.as_ref().expect("DXF: the fit data");
    assert_eq!((fit.flags, fit.smooth_type), (12, 6), "DXF header");
    assert_eq!(flags(&back), [48, 48, 48, 48, 40, 40, 40], "DXF vertex flags");
    assert_eq!(
        fit.vertices.iter().map(|v| v.position).collect::<Vec<_>>(),
        want.fit.as_ref().unwrap().vertices.iter().map(|v| v.position).collect::<Vec<_>>()
    );
    let dwg = reread(&d, "x.dwg");
    assert_eq!(dwg.points, want.points, "DWG points");
    let fit = dwg.fit.as_ref().expect("DWG: the fit data");
    assert_eq!((fit.flags & 4, fit.smooth_type), (4, 6), "DWG header");
    assert_eq!(flags(&dwg), [48, 48, 48, 48, 40, 40, 40], "DWG vertex flags");
}

#[test]
fn a_moved_spline_fit_polyline_moves_its_frame_with_it() {
    let mut d = drawing(spline_fit());
    let by = Vec3::new(10.0, 20.0, 30.0);
    let h = Handle(0x7000);
    d.model
        .modify(h, |e| {
            if let EntityKind::Polyline3d(p) = &mut e.kind {
                p.map_points(|q| *q = *q + by);
            }
        })
        .unwrap();
    let back = reread(&d, "x.dxf");
    let fit = back.fit.as_ref().expect("the fit data after a move");
    assert_eq!(fit.vertices[0].position, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(back.points[0], spline_fit().points[0] + by);
}

#[test]
fn a_reshaped_spline_fit_polyline_is_written_as_the_plain_polyline_it_draws() {
    let mut d = drawing(spline_fit());
    d.model
        .modify(Handle(0x7000), |e| {
            if let EntityKind::Polyline3d(p) = &mut e.kind {
                p.points[1].z += 5.0;
            }
        })
        .unwrap();
    let back = reread(&d, "x.dxf");
    assert!(back.fit.is_none());
    assert_eq!(back.points.len(), 3);
    assert_eq!(back.points[1].z, spline_fit().points[1].z + 5.0);
}

#[test]
fn a_plain_polyline_made_in_cadcraft_writes_32_and_no_fit_data() {
    let plain = Polyline3d {
        records: Default::default(),
        points: vec![Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 0.0, 2.0)],
        closed: false,
        fit: None,
    };
    let back = reread(&drawing(plain.clone()), "x.dxf");
    assert!(back.fit.is_none());
    assert_eq!(back.points, plain.points);
    // The vertices are plain 3D polyline vertices.
    let text = String::from_utf8(cadcraft_io::write(&drawing(plain), "x.dxf").unwrap()).unwrap();
    assert_eq!(text.matches("AcDb3dPolylineVertex").count(), 3);
}

/// A 3D polyline as AutoCAD writes a spline-fit one: header 70 = 12 and 75 = 6, the frame control points flagged 48 and the
/// generated curve 40.
fn source() -> String {
    let mut text = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n100\nAcDb3dPolyline\n66\n1\n10\n0\n20\n0\n30\n0\n70\n12\n75\n6\n",
    );
    let vertices = [(0.0, 0.0, 0.0, 48), (3.0, 4.0, 1.0, 48), (6.0, 4.0, 2.0, 48), (1.0, 1.5, 0.4, 40), (4.5, 3.5, 1.5, 40), (8.0, 1.5, 2.6, 40)];
    for (x, y, z, f) in vertices {
        text.push_str(&format!("0\nVERTEX\n8\n0\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n{x}\n20\n{y}\n30\n{z}\n70\n{f}\n"));
    }
    text.push_str("0\nSEQEND\n8\n0\n0\nENDSEC\n0\nEOF\n");
    text
}

#[test]
fn a_spline_fit_polyline_read_from_a_file_survives_two_saves_in_both_formats() {
    let read = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert!(read.with_unique_handles().is_none(), "the vertex records of the fit data are not repeated handles");
    let first = polyline(&read);
    assert_eq!(first.points.len(), 3, "the drawn points are the generated curve");
    assert_eq!(flags(&first), [48, 48, 48, 40, 40, 40]);
    for format in ["fitted.dxf", "fitted.dwg"] {
        let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
        for round in 0..2 {
            d = cadcraft_io::read(&cadcraft_io::write(&d, format).unwrap(), format).unwrap();
            let p = polyline(&d);
            assert_eq!(flags(&p), [48, 48, 48, 40, 40, 40], "{format}, save {round}");
            assert_eq!(p.fit.as_ref().map(|f| f.smooth_type), Some(6), "{format}, save {round}");
            assert_eq!(p.points, first.points, "{format}, save {round}");
        }
    }
}
