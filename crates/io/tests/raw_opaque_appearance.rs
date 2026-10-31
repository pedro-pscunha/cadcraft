use cadcraft_doc::{Common, Drawing, Entity, Handle, RawCompound, RawRecord, RawTag};

fn opaque_group(nested_subclass: bool) -> Vec<RawTag> {
    let mut group = vec![
        RawTag { code: 102, value: "{APP_DATA".into() },
        RawTag { code: 8, value: "opaque layer text".into() },
        RawTag { code: 102, value: "{NESTED".into() },
        RawTag { code: 8, value: "nested opaque layer text".into() },
    ];
    if nested_subclass {
        group.push(RawTag { code: 100, value: "AcDbPrivateData".into() });
    }
    group.extend([RawTag { code: 102, value: "}".into() }, RawTag { code: 102, value: "}".into() }]);
    group
}

#[test]
fn a_raw_parent_layer_edit_keeps_each_nested_application_group_intact() {
    for nested_subclass in [false, true] {
        let opaque = opaque_group(nested_subclass);
        let mut tags = vec![
            RawTag { code: 5, value: "200".into() },
            RawTag { code: 330, value: "0".into() },
            RawTag { code: 100, value: "AcDbEntity".into() },
            RawTag { code: 8, value: "Before".into() },
        ];
        tags.extend(opaque.clone());
        tags.extend([
            RawTag { code: 100, value: "AcDbCircle".into() },
            RawTag { code: 10, value: "1.0000".into() },
            RawTag { code: 20, value: "2.0000".into() },
            RawTag { code: 30, value: "3.0000".into() },
            RawTag { code: 40, value: "4.0000".into() },
        ]);
        let common = Common { layer: "Before".into(), ..Default::default() };
        let raw = RawCompound::new(RawRecord::new("CIRCLE".into(), tags).unwrap(), vec![], common, "Unsupported source geometry".into()).unwrap();
        let mut parent = Entity::from_raw(raw);
        parent.common.layer = "0".into();
        let mut drawing = Drawing::new_metric();
        drawing.model.push(parent);
        let bytes = cadcraft_io::write(&drawing, "appearance.dxf").unwrap();
        let tags = cadcraft_dxf::parse(&bytes).unwrap();
        let sections = cadcraft_dxf::sections(&tags);
        let (_, actual) = sections
            .iter()
            .filter(|section| section.name == "ENTITIES")
            .flat_map(|section| cadcraft_dxf::records(&section.tags))
            .find(|(_, tags)| tags.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()) == Some(Handle(0x200))))
            .unwrap();
        let actual: Vec<_> = actual.iter().map(|tag| RawTag { code: tag.code, value: tag.str() }).collect();
        assert!(actual.windows(opaque.len()).any(|window| window == opaque), "the opaque group changed: {actual:?}");
        assert_eq!(
            actual.iter().filter(|tag| tag.code == 8).map(|tag| tag.value.as_str()).collect::<Vec<_>>(),
            ["0", "opaque layer text", "nested opaque layer text"]
        );
        assert!(std::str::from_utf8(&bytes).unwrap().contains("1.0000"), "the numeric source spelling must stay unchanged");
    }
}
