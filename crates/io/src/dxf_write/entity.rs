use super::blocks::table_size;
use super::common::{
    attrib_common, common, common_x, dim_xdata, layer_and_props, multiline_tags, oblique_52, plot_style, plot_style_xdata, text_tags,
};
use super::ctx::Ctx;
use super::layout::paper_view;
use super::{
    Bounds2, Color, DimKind, Drawing, Entity, EntityKind, Gradient, Handle, HashMap, Hatch, Leader, MLeader, Point, Vec2, Vec3, Version, W, dxf_ext,
};

/// The persistent reactors of boundary objects: each source object of an associative hatch in
/// the same space or block → the hatches it bounds (AutoCAD links the two both ways; without the
/// reactor the hatch no longer follows its boundary there).
pub(super) fn hatch_reactors(d: &Drawing) -> HashMap<Handle, Vec<String>> {
    let mut out: HashMap<Handle, Vec<String>> = HashMap::new();
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    for st in stores {
        for e in st.iter() {
            let EntityKind::Hatch(h) = &e.kind else { continue };
            if !h.associative {
                continue;
            }
            for s in h.loops.iter().flat_map(|l| &l.sources) {
                if *s != e.handle && st.contains(*s) {
                    let r = out.entry(*s).or_default();
                    let own = e.handle.hex();
                    if !r.contains(&own) {
                        r.push(own);
                    }
                }
            }
        }
    }
    out
}

pub(super) fn entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    let start = w.t.len();
    write_basic_entity(w, d, e, owner, paper, cx);
    write_text_entity(w, d, e, owner, paper, cx);
    write_insert_entity(w, d, e, owner, paper, cx);
    write_dimension_entity(w, d, e, owner, paper, cx);
    write_table_entity(w, d, e, owner, paper, cx);
    write_annotations_entity(w, d, e, owner, paper, cx);
    write_hatch_entity(w, d, e, owner, paper, cx);
    write_viewport_entity(w, d, e, owner, paper, cx);
    write_wipeout_entity(w, d, e, owner, paper, cx);
    write_other_entity(w, d, e, owner, paper, cx);
    plot_style_xdata(w, &e.common.plot_style, start);
    let mut top_level = false;
    let mut handles = Vec::new();
    for tag in w.t.get(start..).unwrap_or_default() {
        if tag.code == 0 {
            top_level = !matches!(tag.str().as_str(), "VERTEX" | "ATTRIB" | "SEQEND");
        } else if top_level && tag.code == 5 {
            handles.extend(Handle::parse_hex(&tag.str()));
            top_level = false;
        }
    }
    w.emitted.insert(e.handle, handles);
}

fn heavy_polyline(w: &mut W, e: &Entity, owner: &str, paper: bool, p: &cadcraft_doc::LwPolyline) {
    w.s(0, "POLYLINE");
    common(w, e, owner, paper, "AcDb2dPolyline");
    w.i(66, 1);
    w.p(10, Vec3::new(0.0, 0.0, p.elevation));
    thickness(w, e);
    if let Some(fit) = p.saved_fit().filter(|fit| fit.extrusion == e.common.extrusion) {
        w.i(70, fit.flags);
        w.f(40, fit.start_width);
        w.f(41, fit.end_width);
        for (code, value) in [(71, fit.m_count), (72, fit.n_count), (73, fit.m_density), (74, fit.n_density), (75, fit.smooth_type)] {
            w.i(code, value);
        }
        w.p(210, fit.extrusion);
        for vertex in &fit.vertices {
            heavy_vertex(w, e, vertex);
        }
    } else {
        w.i(70, i64::from(p.closed) | if p.plinegen { 128 } else { 0 });
        w.f(40, p.const_width);
        w.f(41, p.const_width);
        w.p(210, e.common.extrusion);
        let ocs_x = if e.common.extrusion.z < 0.0 && canonical_plane(e).is_none() { -1.0 } else { 1.0 };
        for (index, vertex) in p.vertices.iter().enumerate() {
            heavy_vertex(
                w,
                e,
                &cadcraft_doc::PolylineFitVertex {
                    record: p.records.vertices.get(index).cloned().unwrap_or_default(),
                    position: Vec3::new(ocs_x * vertex.p.x, vertex.p.y, 0.0),
                    flags: 0,
                    start_width: if p.const_width > 0.0 { p.const_width } else { vertex.start_width },
                    end_width: if p.const_width > 0.0 { p.const_width } else { vertex.end_width },
                    bulge: ocs_x * vertex.bulge,
                    tangent: 0.0,
                },
            );
        }
    }
    polyline_child(w, e, p.records.seqend.as_ref(), "SEQEND", "");
}

fn polyline_child(w: &mut W, parent: &Entity, record: Option<&cadcraft_doc::PolylineRecord>, kind: &str, subclass: &str) -> usize {
    let handle = w.instance_handle(record.and_then(|r| r.handle));
    let child = Entity {
        geometry_revision: Default::default(),
        handle,
        common: record.map_or_else(|| cadcraft_doc::Common { layer: parent.common.layer.clone(), ..Default::default() }, |r| r.common.clone()),
        kind: EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }),
    };
    let start = w.t.len();
    w.s(0, kind);
    common(w, &child, &parent.handle.hex(), false, subclass);
    if subclass.is_empty() {
        plot_style_xdata(w, &child.common.plot_style, start);
    }
    start
}

fn heavy_vertex(w: &mut W, e: &Entity, vertex: &cadcraft_doc::PolylineFitVertex) {
    let start = polyline_child(w, e, Some(&vertex.record), "VERTEX", "AcDbVertex");
    w.s(100, "AcDb2dVertex");
    w.p(10, vertex.position);
    w.f(40, vertex.start_width);
    w.f(41, vertex.end_width);
    w.f(42, vertex.bulge);
    w.i(70, vertex.flags);
    w.f(50, vertex.tangent.to_degrees());
    plot_style_xdata(w, &vertex.record.common.plot_style, start);
}

fn polyline_mesh(w: &mut W, e: &Entity, owner: &str, paper: bool, mesh: &cadcraft_doc::PolylineMesh) {
    let (parent_class, vertex_class) = match mesh.kind {
        cadcraft_doc::MeshKind::Polyface => ("AcDbPolyFaceMesh", "AcDbPolyFaceMeshVertex"),
        cadcraft_doc::MeshKind::Polygon => ("AcDbPolygonMesh", "AcDbPolygonMeshVertex"),
    };
    w.s(0, "POLYLINE");
    common(w, e, owner, paper, parent_class);
    w.i(66, 1);
    w.p(10, Vec3::ZERO);
    thickness(w, e);
    w.p(210, e.common.extrusion);
    w.i(70, mesh.dxf_flags());
    for (code, value) in [(71, mesh.m_count), (72, mesh.n_count), (73, mesh.m_density), (74, mesh.n_density), (75, mesh.smooth_type)] {
        w.i(code, value);
    }
    for record in &mesh.vertices {
        let v = &record.vertex;
        let start = polyline_child(w, e, Some(&v.record), "VERTEX", if record.is_face() { "AcDbFaceRecord" } else { "AcDbVertex" });
        if !record.is_face() {
            w.s(100, vertex_class);
        }
        w.p(10, v.position);
        w.i(70, v.flags);
        if record.is_face() {
            for (code, value) in (71..=74).zip(record.indices) {
                w.i(code, value);
            }
        }
        plot_style_xdata(w, &v.record.common.plot_style, start);
    }
    polyline_child(w, e, mesh.seqend.as_ref(), "SEQEND", "");
}

fn write_basic_entity(w: &mut W, _d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    match &e.kind {
        EntityKind::Line(l) => {
            w.s(0, "LINE");
            common(w, e, owner, paper, "AcDbLine");
            thickness(w, e);
            w.p(10, l.a);
            w.p(11, l.b);
            canonical_normal(w, e);
        }
        EntityKind::Point(p) => {
            w.s(0, "POINT");
            common(w, e, owner, paper, "AcDbPoint");
            w.p(10, p.p);
            thickness(w, e);
            canonical_normal(w, e);
            if p.angle != 0.0 {
                w.f(50, p.angle.to_degrees());
            }
        }
        EntityKind::Circle(c) => {
            w.s(0, "CIRCLE");
            common(w, e, owner, paper, "AcDbCircle");
            thickness(w, e);
            w.p(10, c.center);
            w.f(40, c.radius);
            canonical_normal(w, e);
        }
        EntityKind::Arc(a) => {
            w.s(0, "ARC");
            common(w, e, owner, paper, "AcDbCircle");
            thickness(w, e);
            w.p(10, a.center);
            w.f(40, a.radius);
            canonical_normal(w, e);
            w.s(100, "AcDbArc");
            w.f(50, a.start.to_degrees());
            w.f(51, a.end.to_degrees());
        }
        EntityKind::Ellipse(el) => {
            w.s(0, "ELLIPSE");
            common(w, e, owner, paper, "AcDbEllipse");
            w.p(10, el.center);
            w.p(11, el.major);
            // A canonical ellipse has the plane of its entity; the legacy one the normal it was read with.
            w.p(210, canonical_plane(e).unwrap_or(el.normal));
            w.f(40, el.ratio);
            w.f(41, el.start);
            w.f(42, el.end);
        }
        EntityKind::LwPolyline(p) if p.heavy => heavy_polyline(w, e, owner, paper, p),
        EntityKind::LwPolyline(p) => {
            w.s(0, "LWPOLYLINE");
            common(w, e, owner, paper, "AcDbPolyline");
            w.i(90, p.vertices.len() as i64);
            w.i(70, i64::from(p.closed) | if p.plinegen { 128 } else { 0 });
            if p.const_width > 0.0 {
                w.f(43, p.const_width);
            }
            if p.elevation != 0.0 {
                w.f(38, p.elevation);
            }
            w.p(210, e.common.extrusion);
            thickness(w, e);
            // A canonical polyline holds its OCS coordinates as read; a legacy one holds them mirrored under -Z.
            let ocs_x = if e.common.extrusion.z < 0.0 && canonical_plane(e).is_none() { -1.0 } else { 1.0 };
            for v in &p.vertices {
                w.p2(10, Vec2::new(ocs_x * v.p.x, v.p.y));
                if v.start_width != 0.0 || v.end_width != 0.0 {
                    w.f(40, v.start_width);
                    w.f(41, v.end_width);
                }
                if v.bulge != 0.0 {
                    w.f(42, ocs_x * v.bulge);
                }
            }
        }
        EntityKind::MLine(m) => {
            w.s(0, "MLINE");
            common(w, e, owner, paper, "AcDbMline");
            w.t.extend(crate::dxf_mline::entity_tags(m, &cx.mlines.handle(&m.style), canonical_plane(e)));
        }
        EntityKind::PolylineMesh(mesh) => polyline_mesh(w, e, owner, paper, mesh),
        EntityKind::Polyline3d(p) => {
            w.s(0, "POLYLINE");
            common(w, e, owner, paper, "AcDb3dPolyline");
            w.i(66, 1);
            w.p(10, Vec3::ZERO);
            thickness(w, e);
            canonical_normal(w, e);
            w.i(70, 8 | i64::from(p.closed));
            for (index, q) in p.points.iter().enumerate() {
                let record = p.records.vertices.get(index);
                let start = polyline_child(w, e, record, "VERTEX", "AcDbVertex");
                w.s(100, "AcDb3dPolylineVertex");
                w.p(10, *q);
                w.i(70, record.and_then(|r| r.flags).unwrap_or(32));
                if let Some(record) = record {
                    plot_style_xdata(w, &record.common.plot_style, start);
                }
            }
            polyline_child(w, e, p.records.seqend.as_ref(), "SEQEND", "");
        }
        EntityKind::Spline(s) => {
            w.s(0, "SPLINE");
            common(w, e, owner, paper, "AcDbSpline");
            spline_tags(w, s, canonical_plane(e));
        }
        EntityKind::Helix(h) => {
            // A cubic spline through points on the helix, then the helix data (DXF Reference). A canonical helix writes
            // its own axis and start point and its control points in WCS; one whose frame cannot be built is not written.
            let (s, controls, start, axis) = match &h.frame {
                Some(f) => match h.world_spline3() {
                    Ok((s, controls)) => (s, controls, f.start, f.axis),
                    Err(_) => return,
                },
                None => {
                    let (s, z) = h.spline3();
                    let controls = s.control.iter().zip(&z).map(|(c, z)| c.to3(*z)).collect();
                    (s, controls, h.start_point(), Vec3::Z)
                }
            };
            w.s(0, "HELIX");
            common(w, e, owner, paper, "AcDbSpline");
            w.i(70, 0);
            w.i(71, s.degree as i64);
            w.i(72, s.knots.len() as i64);
            w.i(73, s.control.len() as i64);
            w.i(74, 0);
            w.f(42, 1e-10);
            w.f(43, 1e-10);
            for k in &s.knots {
                w.f(40, *k);
            }
            for c in &controls {
                w.p(10, *c);
            }
            w.s(100, "AcDbHelix");
            w.i(90, 29);
            w.i(91, 63);
            w.p(10, h.center);
            w.p(11, start);
            w.p(12, axis);
            w.f(40, h.base_radius);
            w.f(41, h.turns);
            w.f(42, h.turn_height());
            w.i(290, i64::from(h.ccw));
            w.i(280, h.constrain.to_dxf());
        }
        EntityKind::Ray(r) | EntityKind::XLine(r) => {
            let ray = matches!(e.kind, EntityKind::Ray(_));
            w.s(0, if ray { "RAY" } else { "XLINE" });
            common(w, e, owner, paper, if ray { "AcDbRay" } else { "AcDbXline" });
            w.p(10, r.base);
            w.p(11, r.dir);
        }
        _ => {}
    }
}

fn write_text_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    match &e.kind {
        EntityKind::Text(t) => {
            w.s(0, "TEXT");
            common(w, e, owner, paper, "AcDbText");
            thickness(w, e);
            text_tags(w, d, t, None, None, canonical_plane(e));
        }
        // Closed polylines, one per loop (`dxf_region`): REGION's own data is ACIS.
        EntityKind::Region(r) => {
            for (i, tags) in crate::dxf_region::polylines(r).into_iter().enumerate() {
                w.s(0, "LWPOLYLINE");
                if i == 0 {
                    common(w, e, owner, paper, "AcDbPolyline");
                } else {
                    // The other loops get handles of their own (only the handle and properties are written).
                    let h = Handle::parse_hex(&w.h()).unwrap_or(e.handle);
                    let part = Entity {
                        geometry_revision: Default::default(),
                        handle: h,
                        common: e.common.clone(),
                        kind: EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }),
                    };
                    common(w, &part, owner, paper, "AcDbPolyline");
                }
                w.t.extend(tags);
            }
        }
        EntityKind::AttDef(a) => {
            w.s(0, "ATTDEF");
            common(w, e, owner, paper, "AcDbText");
            thickness(w, e);
            text_tags(w, d, &a.text, Some((a, cx.target)), Some(&a.prompt), canonical_plane(e));
            if cx.target >= Version::R2007 {
                multiline_tags(w, a, true);
            }
        }
        EntityKind::MText(t) => {
            w.s(0, "MTEXT");
            common(w, e, owner, paper, "AcDbMText");
            w.p(10, t.insert);
            w.f(40, t.height);
            // The width of the reference rectangle: all the columns and their gutters, as AutoCAD writes it.
            w.f(41, t.columns.as_ref().map_or(t.width, |c| c.total_width(t.width)));
            w.i(71, i64::from(t.attach));
            w.i(72, 1);
            // Long contents go in 250-char 3-chunks, the remainder in 1.
            let chars: Vec<char> = crate::dxf_read::mtext_dxf::encode_contents(&t.contents).chars().collect();
            let chunks: Vec<String> = chars.chunks(250).map(|c| c.iter().collect()).collect();
            let n = chunks.len();
            for (i, c) in chunks.iter().enumerate() {
                w.s(if i + 1 == n { 1 } else { 3 }, c.clone());
            }
            if n == 0 {
                w.s(1, "");
            }
            w.s(7, &t.style);
            match canonical_plane(e) {
                // The plane and the full WCS direction of a canonical text.
                Some(normal) => {
                    w.p(210, normal);
                    w.p(11, t.world_direction(normal).unwrap_or(Vec3::new(t.rotation.cos(), t.rotation.sin(), 0.0)));
                }
                None if t.rotation != 0.0 => w.f(50, t.rotation.to_degrees()),
                None => {}
            }
            // Line spacing style: 1 = at least, 2 = exactly.
            w.i(73, if t.line_spacing_exact { 2 } else { 1 });
            w.f(44, t.line_spacing);
            // From 2018 AutoCAD keeps the columns in the embedded object and misreads the column groups;
            // the DWG bridge reads the object whole (the column groups lose the heights of manual columns).
            if (cx.target >= Version::R2018 && !cx.entget) || cx.dwg_bridge {
                // The defined rectangle height (a DWG keeps it with the text, not with its columns).
                if let Some(c) = t.columns.as_ref().filter(|c| c.height > 0.0 && (c.kind == cadcraft_doc::MTextColumnType::Static || c.auto_height)) {
                    w.f(46, c.height);
                }
                // The laid-out extents: acadrust takes them from here into the object of its DWG.
                let (ew, eh) = t.column_extents();
                if cx.dwg_bridge && (ew > 0.0 || eh > 0.0) {
                    w.f(42, ew);
                    w.f(43, eh);
                }
                w.t.extend(crate::dxf_read::mtext_dxf::embedded_tags(t));
            } else if t.columns.is_some() && cx.target >= Version::R2007 && !cx.entget {
                // 2007 to 2017: the columns are the ACAD extended data of the entity.
                w.t.extend(crate::dxf_read::mtext_dxf::xdata_column_chunk(t, &e.common.xdata));
            } else {
                w.t.extend(crate::dxf_read::mtext_dxf::column_tags(t));
            }
        }
        _ => {}
    }
}

fn write_insert_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    if let EntityKind::Insert(i) = &e.kind {
        w.s(0, "INSERT");
        let subclass = if i.cols > 1 || i.rows > 1 { "AcDbMInsertBlock" } else { "AcDbBlockReference" };
        common(w, e, owner, paper, subclass);
        if !i.attribs.is_empty() {
            w.i(66, 1);
        }
        w.s(2, &i.block);
        w.p(10, i.insert);
        w.f(41, i.scale.x);
        w.f(42, i.scale.y);
        w.f(43, i.scale.z);
        w.f(50, i.rotation.to_degrees());
        if i.cols > 1 || i.rows > 1 {
            w.i(70, i64::from(i.cols));
            w.i(71, i64::from(i.rows));
            w.f(44, i.col_spacing);
            w.f(45, i.row_spacing);
        }
        // A canonical reference has the plane of its entity: its insertion point and rotation are OCS values of it.
        canonical_normal(w, e);
        if !i.attribs.is_empty() {
            for a in &i.attribs {
                let ah = w.instance_handle(a.handle).hex();
                let start = w.t.len();
                w.s(0, "ATTRIB");
                w.s(5, ah);
                w.s(330, e.handle.hex());
                w.s(100, "AcDbEntity");
                let (c, base) = attrib_common(d, e, i, a);
                layer_and_props(w, &c, Some(&base));
                plot_style(w, &c.plot_style);
                // Hidden as the attribute is: its own visibility, else its definition's (a dynamic block's visibility state hides it).
                if !c.visible {
                    w.i(60, 1);
                }
                w.s(100, "AcDbText");
                // An attribute has a thickness of its own; only a canonical one has a plane of its own.
                let own_plane = (a.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2).then_some(a.plane.normal);
                if a.plane.thickness != 0.0 {
                    w.f(39, a.plane.thickness);
                }
                text_tags(w, d, &a.text, Some((a, cx.target)), None, own_plane);
                if cx.target >= Version::R2007 {
                    multiline_tags(w, a, false);
                }
                plot_style_xdata(w, &c.plot_style, start);
            }
            let sh = w.h();
            w.s(0, "SEQEND");
            w.s(5, sh);
            w.s(330, e.handle.hex());
            w.s(100, "AcDbEntity");
            w.s(8, &e.common.layer);
        }
    }
}

fn write_dimension_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    if let EntityKind::Dimension(dm) = &e.kind {
        w.s(0, "DIMENSION");
        common_x(w, e, owner, paper, "AcDbDimension", cx.assoc.get(&e.handle));
        let bname = cx.dim_blocks.get(&e.handle).cloned().or_else(|| dm.block.clone()).unwrap_or_default();
        w.s(2, bname);
        w.p(10, dm.defpt);
        let style = d.dim_style(&dm.style).cloned().unwrap_or_default();
        // A canonical dimension is measured and laid out in its plane; its text midpoint is an OCS point with its Z.
        let plane = canonical_plane(e);
        let Some(drawn) = plane.map_or(Some(dm.clone()), |n| dm.localized(n).ok()) else { return };
        let g = cadcraft_render::dimension_geometry(&drawn, &style, d.header.f64("DIMSCALE", 1.0));
        w.p(11, if dm.user_text_pos { dm.text_mid } else { g.text_pos.to3(if plane.is_some() { dm.text_mid.z } else { 0.0 }) });
        let ty: i64 = match dm.kind {
            DimKind::Linear { .. } => 0,
            DimKind::Aligned => 1,
            DimKind::Angular => 2,
            DimKind::Diameter => 3,
            DimKind::Radius => 4,
            DimKind::Angular3P => 5,
            DimKind::Ordinate { x_type } => 6 | if x_type { 64 } else { 0 },
            // Written as aligned, marked by CADCraft xdata (see `dim_xdata`).
            DimKind::ArcLength => 1,
        };
        w.i(70, ty | 32 | if dm.user_text_pos { 128 } else { 0 });
        // The actual measurement (read-only in DXF): AutoLISP's entget shows it.
        w.f(42, g.measurement);
        if !dm.text.is_empty() {
            w.s(1, &dm.text);
        }
        w.s(3, &dm.style);
        if dm.text_rotation != 0.0 {
            w.f(53, dm.text_rotation.to_degrees());
        }
        canonical_normal(w, e);
        match dm.kind {
            DimKind::Linear { rotation } => {
                w.s(100, "AcDbAlignedDimension");
                w.p(13, dm.p13);
                w.p(14, dm.p14);
                w.f(50, rotation.to_degrees());
                oblique_52(w, dm, rotation);
                w.s(100, "AcDbRotatedDimension");
            }
            DimKind::Aligned | DimKind::ArcLength => {
                w.s(100, "AcDbAlignedDimension");
                w.p(13, dm.p13);
                w.p(14, dm.p14);
                if matches!(dm.kind, DimKind::Aligned) {
                    oblique_52(w, dm, (drawn.p14.xy() - drawn.p13.xy()).angle());
                }
            }
            DimKind::Radius => {
                w.s(100, "AcDbRadialDimension");
                w.p(15, dm.p15);
                w.f(40, 0.0);
            }
            DimKind::Diameter => {
                w.s(100, "AcDbDiametricDimension");
                w.p(15, dm.p15);
                w.f(40, 0.0);
            }
            DimKind::Angular => {
                w.s(100, "AcDb2LineAngularDimension");
                w.p(13, dm.p13);
                w.p(14, dm.p14);
                w.p(15, dm.p15);
                w.p(16, dm.p16);
            }
            DimKind::Angular3P => {
                w.s(100, "AcDb3PointAngularDimension");
                w.p(13, dm.p13);
                w.p(14, dm.p14);
                w.p(15, dm.p15);
            }
            DimKind::Ordinate { .. } => {
                w.s(100, "AcDbOrdinateDimension");
                w.p(13, dm.p13);
                w.p(14, dm.p14);
            }
        }
        dim_xdata(w, dm, cx);
    }
}

fn write_table_entity(w: &mut W, _d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    if let EntityKind::Table(t) = &e.kind {
        // Tables without a generated block (none expected) are not written.
        let Some((bname, brh)) = cx.tables.get(&e.handle) else { return };
        w.s(0, "ACAD_TABLE");
        common(w, e, owner, paper, "AcDbBlockReference");
        w.s(2, bname);
        w.p(10, t.insert);
        // A canonical table has the plane of its entity, kept as read.
        if let Some(normal) = canonical_plane(e) {
            w.p(210, normal);
        }
        w.s(100, "AcDbTable");
        w.i(280, 0);
        w.s(342, cx.table_style(&t.style));
        w.s(343, brh);
        w.p(11, t.dir);
        w.i(90, 0);
        let (rows, cols) = table_size(t);
        w.i(91, rows as i64);
        w.i(92, cols as i64);
        for c in [93, 94, 95, 96] {
            w.i(c, 0);
        }
        for h in t.row_heights.iter().take(rows) {
            w.f(141, *h);
        }
        for c in t.col_widths.iter().take(cols) {
            w.f(142, *c);
        }
        let cov = cadcraft_render::table_covered(t);
        let span = |v: u32| i64::from(v.min(i16::MAX as u32));
        for r in 0..rows {
            for c in 0..cols {
                let cell = t.cells.get(r).and_then(|x| x.get(c));
                let covered = cov.get(r).and_then(|x| x.get(c)).copied().unwrap_or(false);
                let merge = cell.and_then(|x| x.merged).filter(|&(a, b)| a >= 1 && b >= 1 && (a, b) != (1, 1));
                w.i(171, 1);
                w.i(172, 0);
                w.i(173, i64::from(covered || merge.is_some()));
                w.i(174, 0);
                let (rs, cs) = if covered { (0, 0) } else { merge.map(|(a, b)| (span(a), span(b))).unwrap_or((1, 1)) };
                w.i(175, cs);
                w.i(176, rs);
                w.i(91, 0);
                w.i(178, 0);
                w.f(145, 0.0);
                if let Some(h) = cell.and_then(|x| x.text_height).filter(|h| h.is_finite() && *h > 0.0) {
                    w.f(140, h);
                }
                // Long text: 250-character code-2 chunks, the rest in code 1.
                let chars: Vec<char> = cell.map(|x| x.text.chars().collect()).unwrap_or_default();
                let chunks: Vec<String> = chars.chunks(250).map(|c| c.iter().collect()).collect();
                let n = chunks.len();
                for (i, ch) in chunks.into_iter().enumerate() {
                    w.s(if i + 1 == n { 1 } else { 2 }, ch);
                }
                if n == 0 {
                    w.s(1, "");
                }
            }
        }
        w.s(1001, dxf_ext::APP);
        w.s(1000, "TABLE");
        w.i(1070, i64::from(t.title));
        w.i(1070, i64::from(t.header));
        w.f(1040, t.text_height);
        w.s(1000, &t.style);
        for c in crate::dxf_table::table_xdata(t) {
            w.s(1000, c);
        }
    }
}

fn write_annotations_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    match &e.kind {
        EntityKind::MLeader(m) if !cx.native_mleader() => mleader_fallback(w, d, e, m, owner, paper, cx),
        EntityKind::MLeader(m) => {
            w.s(0, "MULTILEADER");
            common(w, e, owner, paper, "AcDbMLeader");
            let refs = crate::dxf_mleader::Refs {
                style: cx.mleader_style(&m.style),
                text_style: m.text.as_ref().and_then(|t| cx.style(&t.style)),
                block: m.block.as_ref().and_then(|b| cx.block_records.get(&b.block.to_ascii_uppercase()).cloned()),
                arrow: cx.arrow(&m.arrow_block),
            };
            let canonical = e.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2 && m.require_canonical().is_ok();
            w.t.extend(crate::dxf_mleader::entity_tags(d, m, &refs, canonical));
        }
        EntityKind::Leader(l) => {
            w.s(0, "LEADER");
            common(w, e, owner, paper, "AcDbLeader");
            w.s(3, &l.style);
            w.i(71, i64::from(l.arrow));
            w.i(72, i64::from(l.spline));
            // A canonical leader writes its annotation size, its plane, direction and offsets as it holds them.
            if let (Some(f), Some(_)) = (&l.frame, canonical_plane(e)) {
                w.f(40, f.text_height);
                w.f(41, f.text_width);
            }
            w.i(76, l.vertices.len() as i64);
            for v in &l.vertices {
                w.p(10, *v);
            }
            if let (Some(f), Some(normal)) = (&l.frame, canonical_plane(e)) {
                w.p(210, normal);
                w.p(211, f.direction);
                w.p(212, f.block_offset);
                w.p(213, f.annotation_offset);
            }
        }
        EntityKind::Image(i) => {
            // Every image has its IMAGEDEF (planned in `write`); the fallback never happens.
            let Some((def, reactor)) = cx.images.by_entity.get(&e.handle) else { return };
            w.s(0, "IMAGE");
            common(w, e, owner, paper, "AcDbRasterImage");
            w.t.extend(crate::dxf_image::entity_tags(i, def, reactor));
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => {
            let solid = matches!(e.kind, EntityKind::Solid(_));
            w.s(0, if solid { "SOLID" } else { "TRACE" });
            common(w, e, owner, paper, "AcDbTrace");
            w.p(10, s.corners[0]);
            w.p(11, s.corners[1]);
            w.p(12, s.corners[2]);
            w.p(13, s.corners[3]);
            thickness(w, e);
            canonical_normal(w, e);
        }
        EntityKind::Face3d(f) => {
            w.s(0, "3DFACE");
            common(w, e, owner, paper, "AcDbFace");
            w.p(10, f.corners[0]);
            w.p(11, f.corners[1]);
            w.p(12, f.corners[2]);
            w.p(13, f.corners[3]);
            w.i(70, i64::from(f.hidden_edges));
        }
        _ => {}
    }
}

/// A canonical loop: its flags and exact edges as its record held them (DXF Reference, HATCH group codes 92 to 74).
fn canonical_loop(w: &mut W, l: &cadcraft_doc::HatchLoop, cx: &Ctx) {
    use cadcraft_doc::HatchEdge;
    w.i(92, i64::from(l.flags));
    if let [HatchEdge::Polyline { vertices, closed }] = l.edges.as_slice() {
        let has_bulge = vertices.iter().any(|v| v.bulge != 0.0);
        w.i(72, i64::from(has_bulge));
        w.i(73, i64::from(*closed));
        w.i(93, vertices.len() as i64);
        for v in vertices {
            w.p2(10, v.p);
            if has_bulge {
                w.f(42, v.bulge);
            }
        }
        return;
    }
    w.i(93, l.edges.len() as i64);
    for edge in &l.edges {
        match edge {
            HatchEdge::Line { a, b } => {
                w.i(72, 1);
                w.p2(10, *a);
                w.p2(11, *b);
            }
            HatchEdge::Arc { center, radius, start, end, ccw } => {
                w.i(72, 2);
                w.p2(10, *center);
                w.f(40, *radius);
                w.f(50, start.to_degrees());
                w.f(51, end.to_degrees());
                w.i(73, i64::from(*ccw));
            }
            HatchEdge::Ellipse { center, major, ratio, start, end, ccw } => {
                w.i(72, 3);
                w.p2(10, *center);
                w.p2(11, *major);
                w.f(40, *ratio);
                w.f(50, start.to_degrees());
                w.f(51, end.to_degrees());
                w.i(73, i64::from(*ccw));
            }
            HatchEdge::Spline(sp) => {
                w.i(72, 4);
                w.i(94, i64::from(sp.degree));
                w.i(73, i64::from(sp.rational));
                w.i(74, i64::from(sp.periodic));
                w.i(95, sp.knots.len() as i64);
                w.i(96, sp.control.len() as i64);
                for k in &sp.knots {
                    w.f(40, *k);
                }
                for (i, c) in sp.control.iter().enumerate() {
                    w.p2(10, *c);
                    if sp.rational {
                        w.f(42, sp.weights.get(i).copied().unwrap_or(1.0));
                    }
                }
                // The fit data count (97) exists from AutoCAD 2010 on, and from a DWG made by the bridge.
                if cx.target >= Version::R2010 || cx.dwg_bridge {
                    w.i(97, sp.fit.len() as i64);
                    for f in &sp.fit {
                        w.p2(11, *f);
                    }
                    if !sp.fit.is_empty() {
                        w.p2(12, sp.start_tangent);
                        w.p2(13, sp.end_tangent);
                    }
                }
            }
            HatchEdge::Polyline { .. } => {}
        }
    }
}

fn write_hatch_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    if let EntityKind::Hatch(h) = &e.kind {
        w.s(0, "HATCH");
        common(w, e, owner, paper, "AcDbHatch");
        w.p(10, Vec3::new(0.0, 0.0, h.elevation));
        w.p(210, canonical_plane(e).unwrap_or(Vec3::Z));
        w.s(2, &h.pattern);
        w.i(70, i64::from(h.solid));
        w.i(71, i64::from(h.associative));
        w.i(91, h.loops.len() as i64);
        for l in &h.loops {
            if l.edges.is_empty() {
                w.i(92, 2 | if l.outer { 1 } else { 0 });
                let has_bulge = l.vertices.iter().any(|v| v.bulge != 0.0);
                w.i(72, i64::from(has_bulge));
                w.i(73, 1);
                w.i(93, l.vertices.len() as i64);
                for v in &l.vertices {
                    w.p2(10, v.p);
                    if has_bulge {
                        w.f(42, v.bulge);
                    }
                }
            } else {
                canonical_loop(w, l, cx);
            }
            // Source boundary objects of an associative hatch (the ones still beside it, in
            // its space or block, which carry the hatch as a reactor).
            let own = e.handle.hex();
            let sources: Vec<String> =
                l.sources.iter().filter(|s| h.associative && w.reactors.get(*s).is_some_and(|r| r.contains(&own))).map(|s| s.hex()).collect();
            w.i(97, sources.len() as i64);
            for s in &sources {
                w.s(330, s);
            }
        }
        w.i(75, i64::from(h.style));
        w.i(76, 1);
        if !h.solid {
            w.f(52, h.angle.to_degrees());
            w.f(41, h.scale);
            w.i(77, 0);
            // The lines read from the file, else the library's pattern of that name.
            let lines = if h.pattern_lines.is_empty() {
                cadcraft_doc::library::pattern_for(d, &h.pattern).map(|p| p.lines).unwrap_or_default()
            } else {
                h.pattern_lines.clone()
            };
            w.i(78, lines.len() as i64);
            for pl in lines {
                let ang = pl.angle.to_radians() + h.angle;
                w.f(53, ang.to_degrees());
                let o = h.origin + Vec2::new(pl.origin.0, pl.origin.1).rotate(h.angle) * h.scale;
                w.f(43, o.x);
                w.f(44, o.y);
                let dl = Vec2::new(pl.delta.0, pl.delta.1).rotate(ang) * h.scale;
                w.f(45, dl.x);
                w.f(46, dl.y);
                w.i(79, pl.dashes.len() as i64);
                for dsh in pl.dashes {
                    w.f(49, dsh * h.scale);
                }
            }
        }
        w.i(98, 0);
        // Gradient fills (450 to 470) exist from AutoCAD 2004; AutoCAD 2000 ends the hatch at the seed points
        // and the fill stays in CADCraft's xdata below.
        if let Some(g) = &h.gradient {
            gradient(w, g, cx.dwg_bridge || cx.target >= Version::R2004);
        }
        hatch_xdata(w, h);
    }
}

fn write_viewport_entity(w: &mut W, d: &Drawing, e: &Entity, owner: &str, paper: bool, cx: &Ctx) {
    if let EntityKind::Viewport(v) = &e.kind {
        paper_view(w, d, e, owner, cx);
        w.s(0, "VIEWPORT");
        common(w, e, owner, paper, "AcDbViewport");
        w.p(10, v.center);
        w.f(40, v.width);
        w.f(41, v.height);
        // Status 68: a positive value is "on" (the stacking order; 1 is the active viewport), 0 off.
        w.i(68, if v.on { i64::from(v.id.clamp(1, 32767)) } else { 0 });
        w.i(69, i64::from(v.id));
        w.p2(12, v.view_center);
        w.f(45, v.view_height);
        // Status flags: 16384 = display locked, 32768 = always set, 65536 = clipped by the
        // 340 entity, 131072 = off.
        let clip = v.clip.filter(|h| *h != e.handle && d.entity(*h).is_some());
        let flags = 32768 | if v.locked { 16384 } else { 0 } | if clip.is_some() { 65536 } else { 0 } | if v.on { 0 } else { 131072 };
        w.i(90, flags);
        if let Some(h) = clip {
            w.s(340, h.hex());
        }
        // AutoCAD's frozen-layer list: one soft pointer per layer, to its LAYER record. Other
        // programs read only this; CADCraft prefers its own xdata below when present.
        for name in &v.frozen_layers {
            if let Some(h) = w.layers.get(&name.to_ascii_uppercase()).cloned() {
                w.s(331, h);
            }
        }
        // One CADCraft xdata group holds both lists: readers stop at the first group of an app.
        if !v.frozen_layers.is_empty() || !v.layer_colors.is_empty() {
            w.s(1001, dxf_ext::APP);
            if !v.frozen_layers.is_empty() {
                w.xdata(dxf_ext::frozen_xdata(&v.frozen_layers));
            }
            if !v.layer_colors.is_empty() {
                w.xdata(dxf_ext::layer_colors_xdata(&v.layer_colors));
            }
        }
    }
}

/// A canonical wipeout: the image frame and the clip as its record held them, with no closing vertex added.
fn write_canonical_wipeout(w: &mut W, e: &Entity, owner: &str, paper: bool, f: &cadcraft_doc::WipeoutFrame, display: &cadcraft_doc::WipeoutDisplay) {
    w.s(0, "WIPEOUT");
    common(w, e, owner, paper, "AcDbWipeout");
    w.i(90, 0);
    w.p(10, f.origin);
    w.p(11, f.u);
    w.p(12, f.v);
    w.p2(13, f.size);
    wipeout_display(w, display);
    w.i(71, i64::from(f.clip_type));
    w.i(91, f.clip.len() as i64);
    for q in &f.clip {
        w.p2(14, *q);
    }
}

/// Groups 70, 280, 281, 282 and 283 of a WIPEOUT.
fn wipeout_display(w: &mut W, d: &cadcraft_doc::WipeoutDisplay) {
    w.i(70, d.flags);
    w.i(280, d.clipping);
    w.i(281, d.brightness);
    w.i(282, d.contrast);
    w.i(283, d.fade);
}

fn write_wipeout_entity(w: &mut W, _d: &Drawing, e: &Entity, owner: &str, paper: bool, _cx: &Ctx) {
    if let EntityKind::Wipeout(wo) = &e.kind {
        if let Some(frame) = &wo.frame {
            write_canonical_wipeout(w, e, owner, paper, frame, &wo.display);
            return;
        }
        // A 1x1 image spanning the boundary's extents. Clip vertices are in pixel space, whose origin is
        // the image's top-left corner with y pointing down: the reader maps each vertex back through
        // insert + u * (x + 0.5) + v * (0.5 - y).
        // Polygonal clip boundaries are closed, so the first vertex is repeated when needed.
        let b = Bounds2::from_points(wo.boundary.iter().copied());
        let o = if b.is_empty() { Vec2::ZERO } else { b.min };
        let (sx, sy) = (if b.width() > 0.0 { b.width() } else { 1.0 }, if b.height() > 0.0 { b.height() } else { 1.0 });
        w.s(0, "WIPEOUT");
        common(w, e, owner, paper, "AcDbWipeout");
        w.i(90, 0);
        w.p(10, Vec3::new(o.x, o.y, 0.0));
        w.p(11, Vec3::new(sx, 0.0, 0.0));
        w.p(12, Vec3::new(0.0, sy, 0.0));
        w.p2(13, Vec2::new(1.0, 1.0));
        // The display settings as read; a new wipeout shows the image unaligned, with its clipping boundary, clipping on, default
        // brightness, contrast and fade.
        wipeout_display(w, &wo.display);
        w.i(71, 2);
        let mut pts = wo.boundary.clone();
        if let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied())
            && first != last
        {
            pts.push(first);
        }
        w.i(91, pts.len() as i64);
        for q in pts {
            w.p2(14, Vec2::new((q.x - o.x) / sx - 0.5, 0.5 - (q.y - o.y) / sy));
        }
    }
}

fn write_other_entity(w: &mut W, _d: &Drawing, e: &Entity, owner: &str, paper: bool, _cx: &Ctx) {
    match &e.kind {
        EntityKind::Tolerance(t) => {
            w.s(0, "TOLERANCE");
            common(w, e, owner, paper, "AcDbFcf");
            w.s(3, &t.style);
            w.p(10, t.insert);
            w.s(1, &t.text);
            w.p(11, t.dir);
            if let Some(normal) = canonical_plane(e) {
                w.p(210, normal);
            }
        }
        // Entity types CADCraft doesn't model aren't written; the save command reports them.
        EntityKind::Unknown(_) => {}
        _ => {}
    }
}

/// A multileader for a file older than 2007 (no MULTILEADER): a LEADER per leader line, ending
/// at the landing, plus its MTEXT or its block's INSERT, with the multileader's properties.
fn mleader_fallback(w: &mut W, d: &Drawing, e: &Entity, m: &MLeader, owner: &str, paper: bool, cx: &Ctx) {
    let style = d.header.str("DIMSTYLE", "Standard");
    // A canonical multileader splits into parts that each have their own plane.
    if e.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2 {
        let base = m.block.as_ref().and_then(|b| d.block(&b.block)).map_or(Vec3::ZERO, |b| b.base);
        if let Ok(parts) = m.split_canonical(&style, base) {
            let part = |w: &mut W, kind: EntityKind, normal: Vec3| Entity {
                geometry_revision: cadcraft_doc::GeometryRevision::CanonicalV2,
                handle: Handle::parse_hex(&w.h()).unwrap_or(Handle(0)),
                common: cadcraft_doc::Common { extrusion: normal, ..e.common.clone() },
                kind,
            };
            let mut kinds: Vec<(EntityKind, Vec3)> =
                parts.leaders.into_iter().take(64).map(|l| (EntityKind::Leader(l), parts.leader_normal)).collect();
            kinds.extend(parts.text.map(|(t, n)| (EntityKind::MText(t), n)));
            kinds.extend(parts.block.map(|(i, n)| (EntityKind::Insert(i), n)));
            for (kind, normal) in kinds {
                let pe = part(w, kind, normal);
                entity(w, d, &pe, owner, paper, cx);
            }
            return;
        }
    }
    let part = |w: &mut W, kind: EntityKind| Entity {
        geometry_revision: Default::default(),
        handle: Handle::parse_hex(&w.h()).unwrap_or(Handle(0)),
        common: e.common.clone(),
        kind,
    };
    for (root, arm) in m.lines().take(64) {
        let mut vertices = arm.vertices.clone();
        vertices.push(root.landing);
        let le = part(w, EntityKind::Leader(Leader { vertices, arrow: true, spline: m.spline, style: style.clone(), frame: None }));
        entity(w, d, &le, owner, paper, cx);
    }
    if let Some(t) = &m.text {
        let te = part(w, EntityKind::MText(t.clone()));
        entity(w, d, &te, owner, paper, cx);
    }
    if let Some(ins) = &m.block {
        let ie = part(w, EntityKind::Insert(ins.clone()));
        entity(w, d, &ie, owner, paper, cx);
    }
}

/// A hatch's gradient fill (DXF Reference, HATCH group codes 450–470): two-colour gradient
/// with its rotation (radians), shift (0 = centered, 1 = shifted) and per-colour 463 records
/// carrying ACI (63) and, for true colours, the RGB value (421). Those groups belong to R2004+
/// files, so the same gradient also travels as `CADCRAFT` xdata, which survives this R2000
/// file's conversion to DWG (where the native fields don't exist).
fn gradient(w: &mut W, g: &Gradient, groups: bool) {
    if groups {
        gradient_groups(w, g);
    }
    gradient_xdata(w, g);
}

fn gradient_groups(w: &mut W, g: &Gradient) {
    w.i(450, 1);
    w.i(451, 0);
    w.f(460, g.angle);
    w.f(461, if g.centered { 0.0 } else { 1.0 });
    w.i(452, 0);
    w.f(462, 0.0);
    w.i(453, 2);
    for (k, c) in [(0.0, g.color1), (1.0, g.color2)] {
        w.f(463, k);
        match c {
            Color::True(rgb) => {
                w.i(63, i64::from(cadcraft_color::nearest_aci(rgb)));
                w.i(421, i64::from(rgb.to_u32()));
            }
            c => w.i(63, i64::from(c.to_aci())),
        }
    }
    w.s(470, &g.name);
}

fn gradient_xdata(w: &mut W, g: &Gradient) {
    w.s(1001, dxf_ext::APP);
    w.s(1000, "GRADIENT");
    // A 1000 group holds at most 255 bytes; cut at a character boundary.
    let cut = g.name.char_indices().map(|(i, c)| i + c.len_utf8()).take_while(|end| *end <= 255).last().unwrap_or(0);
    w.s(1000, g.name.get(..cut).unwrap_or_default());
    w.f(1040, g.angle);
    w.i(1070, i64::from(g.centered));
    w.s(1000, g.color1.name());
    w.s(1000, g.color2.name());
}

/// The groups of a SPLINE after its subclass marker. A canonical spatial spline (`plane` is its normal) writes its Z
/// values, its tangents' Z and the planar flag only when its points lie in that plane; a planar spline writes Z = 0.
fn spline_tags(w: &mut W, s: &cadcraft_geom::Spline, plane: Option<Vec3>) {
    if let Some(normal) = plane {
        w.p(210, normal);
    }
    let z = |zs: Option<&Vec<f64>>, i: usize| zs.and_then(|v| v.get(i)).copied().unwrap_or(0.0);
    let space = s.space.as_ref().filter(|_| plane.is_some() && s.space_is_consistent());
    let planar = plane.filter(|_| space.is_some()).is_none_or(|normal| {
        let points = s.control3().unwrap_or_default();
        let n = normal.normalized();
        let base = points.first().copied().unwrap_or(Vec3::ZERO);
        points.iter().all(|p| ((*p - base).dot(n)).abs() <= 1e-9 * (1.0 + p.len()))
    });
    let flags = if planar { 8 } else { 0 } | i64::from(s.closed) | if s.is_periodic() { 2 } else { 0 } | if s.weights.is_empty() { 0 } else { 4 };
    w.i(70, flags);
    w.i(71, s.degree as i64);
    w.i(72, s.knots.len() as i64);
    w.i(73, s.control.len() as i64);
    w.i(74, s.fit.len() as i64);
    w.f(42, 1e-10);
    w.f(43, 1e-10);
    // Fit data: tolerance and end tangents (the knot parametrisation is implied by the knots).
    if !s.fit.is_empty() {
        w.f(44, s.fit_opts.tolerance);
        if let Some(t) = s.fit_opts.start_tangent {
            w.p(12, t.to3(space.map_or(0.0, |z| z.start_tangent)));
        }
        if let Some(t) = s.fit_opts.end_tangent {
            w.p(13, t.to3(space.map_or(0.0, |z| z.end_tangent)));
        }
    }
    for k in &s.knots {
        w.f(40, *k);
    }
    for (i, c) in s.control.iter().enumerate() {
        w.p(10, c.to3(z(space.map(|z| &z.control), i)));
        if let Some(wt) = s.weights.get(i) {
            w.f(41, *wt);
        }
    }
    for (i, f) in s.fit.iter().enumerate() {
        w.p(11, f.to3(z(space.map(|z| &z.fit), i)));
    }
}

/// Group 210 of a canonical entity: its normal, kept as read. A legacy entity writes none.
fn canonical_normal(w: &mut W, e: &Entity) {
    if let Some(normal) = canonical_plane(e) {
        w.p(210, normal);
    }
}

/// The plane of a canonical text-family entity: its normal, kept as read. Legacy text has none.
pub(super) fn canonical_plane(e: &Entity) -> Option<Vec3> {
    (e.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2).then_some(e.common.extrusion)
}

/// Group 39 (thickness) of the entities whose DXF Reference record has one; only when set.
fn thickness(w: &mut W, e: &Entity) {
    if e.common.thickness != 0.0 {
        w.f(39, e.common.thickness);
    }
}

/// Hatch origin and background colour as `CADCRAFT` xdata: `1000 HATCH`, the origin as a
/// world position (`1011`, moved with the hatch by other programs) and the background colour
/// name (`1000`, empty for none). The DXF Reference has no HATCH groups for either: other
/// readers get the origin through the pattern lines' base points (43/44), the background isn't
/// carried. Appended to the gradient's `CADCRAFT` xdata when there is one.
fn hatch_xdata(w: &mut W, h: &Hatch) {
    if h.origin == Vec2::ZERO && h.background.is_none() {
        return;
    }
    if h.gradient.is_none() {
        w.s(1001, dxf_ext::APP);
    }
    w.s(1000, "HATCH");
    w.p(1011, h.origin.to3(0.0));
    w.s(1000, h.background.map(Color::name).unwrap_or_default());
}
