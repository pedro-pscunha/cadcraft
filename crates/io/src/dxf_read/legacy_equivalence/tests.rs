use super::*;
use cadcraft_dxf::Value;

fn unit(text: &str) -> Vec<(String, Vec<Tag>)> {
    cadcraft_dxf::records(&cadcraft_dxf::parse(text.as_bytes()).unwrap())
}
fn disposition(text: &str) -> LegacyDisposition {
    classify_legacy_unit(&unit(text), Version::R2018).unwrap().disposition
}

pub(super) const ELLIPSE_HATCH: &str = "0\nHATCH\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n1\n93\n1\n72\n3\n10\n2\n20\n3\n11\n4\n21\n0\n40\n0.5\n50\n0\n51\n90\n73\n1\n97\n0\n75\n0\n76\n1\n98\n0\n";

#[test]
fn family_lookup_is_shared_and_aliases_are_explicit() {
    assert_eq!(family_of_kind("mleader"), Some(Family::MLeader));
    assert_eq!(family_of_kind("MULTILEADER"), Some(Family::MLeader));
    assert_eq!(family_of_kind("ARC_DIMENSION"), Some(Family::Dimension));
    assert_eq!(family_of_kind("ACME_WIDGET"), None);
    assert!(classify_legacy_unit(&unit("0\nACME_WIDGET\n"), Version::R2018).is_none());
}

#[test]
fn table_and_dimension_graphics_have_owners_instead_of_independent_verdicts() {
    use crate::activation::records::{Container, NodeKey, RecordView};
    let tags =
        cadcraft_dxf::parse(b"0\nSECTION\n2\nENTITIES\n0\nACAD_TABLE\n5\nA1\n2\n*T1\n0\nDIMENSION\n5\nA2\n2\n*D1\n0\nENDSEC\n0\nEOF\n").unwrap();
    let classifier = LegacyClassifier::new(&cadcraft_dxf::sections(&tags), Version::R2018);
    let groups = unit("0\nLINE\n10\n1\n20\n2\n11\n3\n21\n4\n").remove(0).1;
    for (name, family, handle) in [("*t1", Family::Table, 0xA1), ("*d1", Family::Dimension, 0xA2)] {
        let key = NodeKey::record(Container::block(name), 0, None);
        let view = RecordView { key: &key, kind: "LINE", family: Family::Line, groups: &groups };
        assert_eq!(
            classifier.record(&view),
            LegacyRecordClassification::OwnerDerived(DerivedOwner { family, handle: Some(cadcraft_doc::Handle(handle)) })
        );
    }
    for container in [Container::Entities, Container::block("AUTHORED")] {
        let key = NodeKey::record(container, 0, None);
        let view = RecordView { key: &key, kind: "LINE", family: Family::Line, groups: &groups };
        assert!(matches!(
            classifier.record(&view),
            LegacyRecordClassification::Authored(LegacyClassification { disposition: LegacyDisposition::Plain, .. })
        ));
    }
    assert_eq!(disposition("0\nACAD_TABLE\n2\n*T1\n"), LegacyDisposition::Needs);
    assert_eq!(disposition("0\nDIMENSION\n2\n*D1\n"), LegacyDisposition::Needs);
}

#[test]
fn tested_zero_z_primitives_are_plain() {
    for text in ["0\nLINE\n10\n1\n20\n2\n30\n0\n11\n3\n21\n4\n31\n0\n", "0\nCIRCLE\n10\n1\n20\n2\n30\n0\n40\n3\n"] {
        assert_eq!(disposition(text), LegacyDisposition::Plain, "{text}");
    }
}

#[test]
fn positive_z_hatch_with_an_ellipse_edge_is_plain_after_the_legacy_fix() {
    assert_eq!(disposition(ELLIPSE_HATCH), LegacyDisposition::Plain);
}

#[test]
fn structural_loss_is_not_hidden_by_a_positive_z_plane_and_zero_elevation() {
    let inconsistent = ELLIPSE_HATCH.replace("93\n1\n", "93\n2\n");
    assert_eq!(disposition(&inconsistent), LegacyDisposition::Needs);
    let partial_spline = "0\nSPLINE\n70\n8\n71\n3\n72\n8\n73\n4\n74\n0\n10\n0\n20\n0\n30\n0\n";
    assert_eq!(disposition(partial_spline), LegacyDisposition::Needs);
}

#[test]
fn unsupported_owned_fields_and_disabled_gradient_data_need_activation() {
    assert_eq!(disposition("0\nLINE\n100\nAcDbLine\n10\n1\n20\n2\n11\n3\n21\n4\n49\n2\n"), LegacyDisposition::Needs);
    assert_eq!(disposition(&format!("{ELLIPSE_HATCH}450\n0\n452\n1\n460\n1.5707963267948966\n")), LegacyDisposition::Needs);
}

#[test]
fn repeated_or_incorrect_subclass_paths_need_activation() {
    for source in [
        "0\nLINE\n100\nAcDbEntity\n10\n1\n20\n2\n100\nAcDbLine\n11\n3\n21\n4\n",
        "0\nLINE\n100\nAcDbLine\n10\n1\n20\n2\n100\nAcDbLine\n11\n3\n21\n4\n",
        "0\nARC\n100\nAcDbCircle\n10\n1\n20\n2\n40\n3\n50\n20\n51\n120\n100\nAcDbArc\n",
    ] {
        assert_eq!(disposition(source), LegacyDisposition::Needs, "{source}");
    }
}

#[test]
fn annotative_hatch_needs_the_unsupported_annotation_path() {
    let source = format!("{ELLIPSE_HATCH}1001\nAcadAnnotative\n1000\nAnnotativeData\n1002\n{{\n1070\n1\n1070\n1\n1002\n}}\n");
    assert_eq!(disposition(&source), LegacyDisposition::Needs);
}

#[test]
fn repeated_scalar_fields_and_misaligned_vertex_fields_need_activation() {
    for source in [
        "0\nLINE\n10\n1\n20\n2\n11\n3\n21\n4\n230\n1\n230\n2\n",
        "0\nLWPOLYLINE\n90\n2\n70\n0\n10\n0\n20\n0\n40\n1\n40\n2\n10\n1\n20\n1\n",
        "0\nLWPOLYLINE\n90\n2\n70\n0\n10\n0\n10\n1\n20\n0\n20\n1\n",
    ] {
        assert_eq!(disposition(source), LegacyDisposition::Needs, "{source}");
    }
    assert_eq!(disposition(&ELLIPSE_HATCH.replace("91\n1\n", "91\n1\n91\n1\n")), LegacyDisposition::Needs);
}

#[test]
fn compound_source_units_stay_needs_until_their_followers_are_proved() {
    for text in [
        "0\nINSERT\n5\nA1\n2\nDEF\n66\n1\n10\n0\n20\n0\n30\n0\n0\nATTRIB\n5\nA2\n10\n0\n20\n0\n30\n0\n40\n1\n1\nVALUE\n2\nTAG\n0\nSEQEND\n5\nA3\n",
        "0\nPOLYLINE\n5\nA1\n66\n1\n70\n0\n0\nVERTEX\n5\nA2\n10\n0\n20\n0\n30\n0\n0\nVERTEX\n5\nA3\n10\n1\n20\n1\n30\n0\n0\nSEQEND\n5\nA4\n",
    ] {
        let source = unit(text);
        for version in Version::ALL {
            assert_eq!(classify_legacy_unit(&source, version).unwrap().disposition, LegacyDisposition::Needs, "{version:?} {text}");
        }
    }
}

#[test]
fn spatial_and_malformed_source_fields_fail_closed() {
    assert_eq!(disposition("0\nLINE\n10\n1\n20\n2\n30\n5\n11\n3\n21\n4\n31\n5\n"), LegacyDisposition::Needs);
    let mut source = unit("0\nLINE\n10\n1\n20\n2\n30\n0\n11\n3\n21\n4\n31\n0\n");
    source[0].1.push(Tag::new(230, Value::Real(f64::NAN)));
    assert_eq!(classify_legacy_unit(&source, Version::R2018).unwrap().disposition, LegacyDisposition::Needs);
}
