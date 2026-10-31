//! A read that reports its activation does one decode, one parse and one graph.
use crate::activation::records::RecordGraph;
use crate::activation::{FamilySet, ReaderMode, ReaderPolicy};
use crate::work_done::{DWG_DECODES, GRAPHS};
use cadcraft_doc::Drawing;

fn counts() -> (usize, usize) {
    (DWG_DECODES.with(|n| n.get()), GRAPHS.with(|n| n.get()))
}

fn reset() {
    DWG_DECODES.with(|n| n.set(0));
    GRAPHS.with(|n| n.set(0));
}

fn components(dxf: &[u8]) -> usize {
    let secs = cadcraft_dxf::sections(&cadcraft_dxf::parse(dxf).unwrap());
    RecordGraph::build(&secs, &|_| crate::activation::Need::Plain).unwrap().report(&ReaderPolicy::default().graph_policy()).components.len()
}

#[test]
fn a_dxf_report_builds_one_graph() {
    let dxf = crate::write_dxf(&Drawing::new_metric()).into_bytes();
    for policy in [ReaderPolicy::default(), ReaderPolicy::new(ReaderMode::Safe, FamilySet::NONE).with_raw_path()] {
        reset();
        let (_, report) = crate::read_report(&dxf, "a.dxf", policy).unwrap();
        assert_eq!(counts(), (0, 1), "{policy:?}");
        assert_eq!(report.components.len(), components(&dxf));
    }
}

#[test]
fn a_dwg_report_decodes_once_and_builds_one_graph() {
    let dwg = crate::write(&Drawing::new_metric(), "a.dwg").unwrap();
    let dxf = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
    for policy in [ReaderPolicy::default(), ReaderPolicy::new(ReaderMode::Safe, FamilySet::NONE).with_raw_path()] {
        reset();
        let (drawing, report) = crate::read_report(&dwg, "a.dwg", policy).unwrap();
        assert_eq!(counts(), (1, 1), "{policy:?}");
        // The same drawing as the drawing-only read, and a report of the same graph.
        assert_eq!(drawing, crate::read_with_policy(&dwg, "a.dwg", policy).unwrap());
        assert_eq!(report.components.len(), components(&dxf));
    }
}
