use cadcraft_doc::{Space, legacy_entity_bounds};
use cadcraft_geom::Bounds2;
use cadcraft_render::{entity_extents, space_extents};

fn bits(bounds: Bounds2) -> [u64; 4] {
    [bounds.min.x.to_bits(), bounds.min.y.to_bits(), bounds.max.x.to_bits(), bounds.max.y.to_bits()]
}

#[test]
fn revision_aware_extents_keep_legacy_corpus_bounds_bit_identical() {
    for (name, bytes) in [
        ("apartment.dxf", include_bytes!("../../../examples/apartment.dxf").as_slice()),
        ("attrib_fields.dxf", include_bytes!("data/attrib_fields.dxf").as_slice()),
    ] {
        let drawing = cadcraft_io::read(bytes, name).unwrap();
        let spaces = std::iter::once(Space::Model).chain(drawing.layouts.iter().map(|layout| Space::Paper(layout.name.clone())));
        for space in spaces {
            assert_eq!(bits(space_extents(&drawing, &space).unwrap()), bits(drawing.legacy_extents(&space).unwrap()), "{name}: {space:?}");
            if let Some(store) = drawing.space(&space) {
                for entity in store.iter() {
                    assert_eq!(
                        bits(entity_extents(&drawing, entity, 0).unwrap()),
                        bits(legacy_entity_bounds(&drawing, entity, 0).unwrap()),
                        "{name}: {:?}",
                        entity.handle
                    );
                }
            }
        }
        for block in drawing.blocks.values() {
            for entity in block.entities.iter() {
                assert_eq!(
                    bits(entity_extents(&drawing, entity, 0).unwrap()),
                    bits(legacy_entity_bounds(&drawing, entity, 0).unwrap()),
                    "{name}: block {} {:?}",
                    block.name,
                    entity.handle
                );
            }
        }
    }
}
