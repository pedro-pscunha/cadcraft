//! `DWG_KEEPS_UNKNOWN` lists the entity types CADCraft keeps as read that a DWG save keeps too, so
//! the save does not say it dropped them. acadrust has no list of what its DWG writer writes: this
//! saves a record of each candidate type to DWG, at every version, and reads the file back.

use cadcraft_doc::foreign::DWG_KEEPS_UNKNOWN;
use cadcraft_io::{SaveOptions, Version};

/// Types the DWG writer writes (acadrust's DXF reader turns each into an entity of its own).
const WRITTEN: &[&str] = &[
    "ARC_DIMENSION",
    "LARGE_RADIAL_DIMENSION",
    "MPOLYGON",
    "SHAPE",
    "MESH",
    "LIGHT",
    "SECTIONLINE",
    "DRAWINGVIEW",
    "PDFUNDERLAY",
    "DWFUNDERLAY",
    "DGNUNDERLAY",
    "OLE2FRAME",
    "OLEFRAME",
    "CAMERA",
    "SECTIONOBJECT",
    "ARCALIGNEDTEXT",
    "RTEXT",
    "POSITIONMARKER",
    "GEOPOSITIONMARKER",
    "COORDINATION_MODEL",
    "NAVISWORKSMODEL",
    "ACDBPOINTCLOUD",
    "POINTCLOUD",
    "ACDBPOINTCLOUDEX",
    "POINTCLOUDEX",
    "FORMAT",
    "ALIGNMENTPARAMETERENTITY",
    "BASEPOINTPARAMETERENTITY",
    "FLIPPARAMETERENTITY",
    "LINEARPARAMETERENTITY",
    "POINTPARAMETERENTITY",
    "ROTATIONPARAMETERENTITY",
    "VISIBILITYPARAMETERENTITY",
    "XYPARAMETERENTITY",
    "BLOCKANGULARCONSTRAINTPARAMETERENTITY",
    "FLIPGRIPENTITY",
    "LINEARGRIPENTITY",
    "POLARGRIPENTITY",
    "ROTATIONGRIPENTITY",
    "VISIBILITYGRIPENTITY",
    "XYGRIPENTITY",
    "WALL",
    "MCSDBOBJECT",
    "NOTEPOSITION",
    "SPDSLEVELMARK",
    "SPDSRELATIONMARK",
    "ACAD_PROXY_ENTITY_WRAPPER",
];

/// Types the writer writes as a record, but whose geometry is ACIS data that only the data store
/// carries (`MODELER_TYPES` in the engine): they are not listed, so a save still reports them.
const ACIS: &[&str] = &["3DSOLID", "REGION", "BODY", "SURFACE", "EXTRUDEDSURFACE", "LOFTEDSURFACE", "REVOLVEDSURFACE", "SWEPTSURFACE", "NURBSURFACE"];

/// Types the DWG bridge leaves out of a save (acadrust reads them as unknown entities, or its
/// DWG writer has no object for them).
const DROPPED: &[&str] = &["ACAD_PROXY_ENTITY", "LAYOUTPRINTCONFIG", "REPEAT", "ENDREP", "LOAD", "JUMP", "ACME_WIDGET"];

const VERSIONS: [Version; 6] = [Version::R2000, Version::R2004, Version::R2007, Version::R2010, Version::R2013, Version::R2018];

/// Whether an entity of type `dxf_type` is in the file after a DWG save at `version`.
fn survives(dxf_type: &str, version: Version) -> Result<bool, cadcraft_io::IoError> {
    let text = format!("0\nSECTION\n2\nENTITIES\n0\n{dxf_type}\n5\n30\n100\nAcDbEntity\n8\n0\n0\nENDSEC\n0\nEOF\n");
    let d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &SaveOptions { version: Some(version), binary: false })?;
    let back = cadcraft_io::read(&dwg, "x.dwg")?;
    Ok(back.model.iter().chain(back.layouts.iter().flat_map(|l| l.entities.iter())).count() > 0)
}

fn listed(dxf_type: &str) -> bool {
    DWG_KEEPS_UNKNOWN.iter().any(|t| t.eq_ignore_ascii_case(dxf_type))
}

#[test]
fn the_list_is_what_the_dwg_writer_writes() {
    for &t in WRITTEN {
        for v in VERSIONS {
            assert!(survives(t, v).unwrap(), "{t} is not in a {v:?} DWG");
        }
        assert!(listed(t), "{t} survives a DWG save but a save reports it as dropped");
    }
    for &t in DROPPED {
        for v in VERSIONS {
            match survives(t, v) {
                Ok(kept) => assert!(!kept, "{t} survives a {v:?} DWG save but is not listed"),
                Err(error) => {
                    let error = error.to_string();
                    assert!(error.contains(t) && error.contains("cannot be preserved"), "{t} {v:?}: {error}");
                }
            }
        }
        assert!(!listed(t), "{t} is dropped by a DWG save but listed as kept");
    }
    for &t in ACIS {
        assert!(!listed(t), "{t} keeps its geometry only in the data store");
    }
    for t in DWG_KEEPS_UNKNOWN {
        assert!(WRITTEN.contains(t), "{t} is listed but this test does not check it");
    }
}
