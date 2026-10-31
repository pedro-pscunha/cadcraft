# Golden refresh at 5f4eeb51

Every output of the corpus (63 drawings, DXF and DWG, 126) differs from the baseline made at be79e74b. The records that differ, compared one by one
(`corpus_golden`'s record keys; the DWG as the DXF rendition of `cadcraft_dwg`), are only these causes; no record is lost or added.

| record | cause | commit | format | drawings | records |
|---|---|---|---|---|---|
| OBJECTS/TABLESTYLE | table style form (groups 280, 90, 91, 1 added) | write a table style in AutoCAD's form | dxf | 63 | 199 |
| TABLES/DIMSTYLE | DIMSTYLE reactors (groups 102/330: the dimensions that use the style) | keep the dimensions that are a DIMSTYLE's persistent reactors | dxf | 13 | 34 |
| OBJECTS/LAYOUT | LAYOUT group 4: an empty paper size name stays empty | a layout with no paper size name stays without one | dxf | 5 | 11 |
| OBJECTS/LAYOUT | LAYOUT group 4: an empty paper size name stays empty | a layout with no paper size name stays without one | dwg | 5 | 11 |
| TABLES/LTYPE | LTYPE Continuous keeps the description of its file | a linetype every drawing has keeps its file's description | dxf | 3 | 3 |
| TABLES/LTYPE | LTYPE Continuous keeps the description of its file | a linetype every drawing has keeps its file's description | dwg | 3 | 3 |
| CLASSES/CLASS | CLASS record values of the file in the DWG (proxy flags 90, 280, application 3, C++ name 2) | pin acadrust ecdd07b8: CLASS records of the file replace the default classes | dwg | 62 | 2509 |

Other causes with no record of their own: the DesignCenter xdata of a block record (kept for blocks that had it beside group 70), the order of the reactors of an IMAGEDEF, an opaque layer's transparency
(packed zero in the DWG) changed no record of the corpus beyond the table above (the corpus has no such record, or it was the same in the baseline). The r5 raw fixture's
two lines move for the table style and the CLASS values only; its input change in 5f4eeb51 adds no other difference.

The DWG of seven drawings is not deterministic in this build (the same drawing saved twice differs in bytes, not in records): see the commit that fixes it.

## Per entry

| drawing | format | records that differ |
|---|---|---|
| crates/io/tests/data/attrib_fields.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/attrib_fields.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/clockwise_arc_edge.dwg | dxf | OBJECTS/LAYOUT x2, OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/clockwise_arc_edge.dwg | dwg | CLASSES/CLASS x8, OBJECTS/LAYOUT x2 |
| crates/io/tests/data/named_shape_style.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/named_shape_style.dxf | dwg | CLASSES/CLASS x2 |
| examples/apartment.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| examples/apartment.dxf | dwg | CLASSES/CLASS x3 |
| /tmp/cad/rt7/Citycon-Augusto_Miele.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Citycon-Augusto_Miele.dwg | dwg | CLASSES/CLASS x35 |
| /tmp/cad/rt7/Cyrela-Aura_Moema.dwg | dxf | OBJECTS/TABLESTYLE x46 |
| /tmp/cad/rt7/Cyrela-Aura_Moema.dwg | dwg | CLASSES/CLASS x52 |
| /tmp/cad/rt7/DPInc.dwg | dxf | OBJECTS/LAYOUT x3, OBJECTS/TABLESTYLE x1, TABLES/LTYPE x1 |
| /tmp/cad/rt7/DPInc.dwg | dwg | CLASSES/CLASS x58, OBJECTS/LAYOUT x3, TABLES/LTYPE x1 |
| /tmp/cad/rt7/Even-Manuel_de_Nobrega.dwg | dxf | OBJECTS/TABLESTYLE x2, TABLES/LTYPE x1 |
| /tmp/cad/rt7/Even-Manuel_de_Nobrega.dwg | dwg | CLASSES/CLASS x86, TABLES/LTYPE x1 |
| /tmp/cad/rt7/Firenze.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Firenze.dwg | dwg | CLASSES/CLASS x50 |
| /tmp/cad/rt7/Plaenge-Altier.dwg | dxf | OBJECTS/LAYOUT x2, OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Plaenge-Altier.dwg | dwg | CLASSES/CLASS x6, OBJECTS/LAYOUT x2 |
| /tmp/cad/rt7/Planeta-Toulouse.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Planeta-Toulouse.dwg | dwg | CLASSES/CLASS x42 |
| /tmp/cad/rt7/Rocontec-Vila_Einstein.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Rocontec-Vila_Einstein.dwg | dwg | CLASSES/CLASS x62 |
| /tmp/cad/rt7/Skr-Ayya.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Skr-Ayya.dwg | dwg | CLASSES/CLASS x32 |
| /tmp/cad/rt7/Tear_Incorporadora-Alvorada.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Tear_Incorporadora-Alvorada.dwg | dwg | CLASSES/CLASS x53 |
| /tmp/cad/rt7/Toledo_Ferrari-HBR51.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rt7/Toledo_Ferrari-HBR51.dwg | dwg | CLASSES/CLASS x12 |
| /tmp/cad/mc/Citycon-Augusto_Miele-2010.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x8 |
| /tmp/cad/mc/Citycon-Augusto_Miele-2010.dxf | dwg | CLASSES/CLASS x35 |
| /tmp/cad/mc/Citycon-Augusto_Miele-2018.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x8 |
| /tmp/cad/mc/Citycon-Augusto_Miele-2018.dxf | dwg | CLASSES/CLASS x43 |
| /tmp/cad/mc/DPInc-2018.dxf | dxf | OBJECTS/LAYOUT x3, OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x7, TABLES/LTYPE x1 |
| /tmp/cad/mc/DPInc-2018.dxf | dwg | CLASSES/CLASS x58, OBJECTS/LAYOUT x3, TABLES/LTYPE x1 |
| /tmp/cad/acad2/Planeta-Toulouse-acad.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x2 |
| /tmp/cad/acad2/Planeta-Toulouse-acad.dxf | dwg | CLASSES/CLASS x42 |
| /tmp/cad/rc2/cyrela-field.dwg | dxf | OBJECTS/TABLESTYLE x46 |
| /tmp/cad/rc2/cyrela-field.dwg | dwg | CLASSES/CLASS x1 |
| /tmp/cad/rc2/cyrela-field-group.dwg | dxf | OBJECTS/TABLESTYLE x46 |
| /tmp/cad/rc2/cyrela-field-group.dwg | dwg | CLASSES/CLASS x1 |
| /tmp/cad/rc2/plaenge-anon.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/plaenge-anon.dwg | dwg | CLASSES/CLASS x1 |
| /tmp/cad/rc2/skr-property.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/skr-property.dwg | dwg | CLASSES/CLASS x1 |
| /tmp/cad/rc2/tear-native.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/tear-native.dwg | dwg | CLASSES/CLASS x53 |
| /tmp/cad/rc2/tear-raw.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/tear-raw.dwg | dwg | CLASSES/CLASS x53 |
| /tmp/cad/rc2/tear-autocad-saved.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/tear-autocad-saved.dwg | dwg | CLASSES/CLASS x53 |
| /tmp/cad/rc2/group-bridge.dxf | dxf | OBJECTS/LAYOUT x1, OBJECTS/TABLESTYLE x1 |
| /tmp/cad/rc2/group-bridge.dxf | dwg | OBJECTS/LAYOUT x1 |
| /tmp/cad/dim1/dim1-blank.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-blank.dwg | dwg | CLASSES/CLASS x1 |
| /tmp/cad/dim1/dim1-mattrib-2018.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-mattrib-2018.dwg | dwg | CLASSES/CLASS x157 |
| /tmp/cad/dim1/dim1-mleader-2004.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-mleader-2004.dwg | dwg | CLASSES/CLASS x19 |
| /tmp/cad/dim1/dim1-table90-2018.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-table90-2018.dwg | dwg | CLASSES/CLASS x157 |
| /tmp/cad/dim1/dim1-tablefix-2007.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-tablefix-2007.dwg | dwg | CLASSES/CLASS x19 |
| /tmp/cad/dim1/dim1-tablefix-2018.dwg | dxf | OBJECTS/TABLESTYLE x1 |
| /tmp/cad/dim1/dim1-tablefix-2018.dwg | dwg | CLASSES/CLASS x157 |
| /tmp/cad/dim1/dim1-native-acad.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-native-acad.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-minsert-acad.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-minsert-acad.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-mleader-acad.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-mleader-acad.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-mattrib-2018.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-mattrib-2018.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-table90-2007.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-table90-2007.dxf | dwg | CLASSES/CLASS x17 |
| /tmp/cad/dim1/dim1-table90-2018.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-table90-2018.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-tablefix-2007.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-tablefix-2007.dxf | dwg | CLASSES/CLASS x17 |
| /tmp/cad/dim1/dim1-tablefix-2018.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-tablefix-2018.dxf | dwg | CLASSES/CLASS x154 |
| /tmp/cad/dim1/dim1-vp68-2018.dxf | dxf | OBJECTS/TABLESTYLE x1, TABLES/DIMSTYLE x1 |
| /tmp/cad/dim1/dim1-vp68-2018.dxf | dwg | CLASSES/CLASS x154 |
| crates/io/tests/data/activation/a10_cycle_all_plain.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a10_cycle_all_plain.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a11_cycle_with_needs_seed.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a11_cycle_with_needs_seed.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a1_tilted_insert_over_plain_definition.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a1_tilted_insert_over_plain_definition.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a2_needing_attribute_on_plain_insert.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a2_needing_attribute_on_plain_insert.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a3_canonical_container_reaches_nested_target.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a3_canonical_container_reaches_nested_target.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a4_plain_insert_over_definition_with_tilted_child.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a4_plain_insert_over_definition_with_tilted_child.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a5_nested_chain.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a5_nested_chain.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a6_shared_definition_in_model_and_in_another_definition.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a6_shared_definition_in_model_and_in_another_definition.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a7_mleader_block_content_tilted.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a7_mleader_block_content_tilted.dxf | dwg | CLASSES/CLASS x3 |
| crates/io/tests/data/activation/a8_dimension_graphics_block_tilted.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a8_dimension_graphics_block_tilted.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/a9_xref_with_tilted_child.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/a9_xref_with_tilted_child.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/b1_needs_member_in_component_that_cannot_activate.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/b1_needs_member_in_component_that_cannot_activate.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/b2_needs_insert_with_attributes_raw.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/b2_needs_insert_with_attributes_raw.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/b3_component_with_unmasked_family.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/b3_component_with_unmasked_family.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/b4_unreferenced_definition_with_needing_member.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/b4_unreferenced_definition_with_needing_member.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/b5_dimension_with_star_d_block.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/b5_dimension_with_star_d_block.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/r1_malformed_spatial_spline.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/r1_malformed_spatial_spline.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/r2_spatial_leader.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/r2_spatial_leader.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/r3_forced_canonical_read_failure.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/r3_forced_canonical_read_failure.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/r4_raw_insert_edit_refusal_and_save.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/r4_raw_insert_edit_refusal_and_save.dxf | dwg | CLASSES/CLASS x2 |
| crates/io/tests/data/activation/r5_raw_insert_with_full_tables.dxf | dxf | OBJECTS/TABLESTYLE x1 |
| crates/io/tests/data/activation/r5_raw_insert_with_full_tables.dxf | dwg | CLASSES/CLASS x2 |
