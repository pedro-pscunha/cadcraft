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

The DWG of seven drawings (the `cyrela-field` pair, the three `tear` files, `Cyrela-Aura_Moema`, `Tear_Incorporadora-Alvorada`) was not deterministic (the same drawing saved twice differed in bytes, not in records): acadrust 9ae5de35 fixes it, and their DWG hashes are recaptured at that pin.

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

# Golden refresh for the table graphics block

An ACAD_TABLE's `*T` graphics block is now kept as the file had it (name, records, handles) while the table is not edited. Seven drawings have an unedited ACAD_TABLE with a `*T` block
(`examples/apartment.dxf` and six `dim1` files); each of their DXF and DWG outputs changed (14 of 126). The records that differ, compared one by one, are only these; outside the `*T` block and the handles that follow it, no record is lost or added.

| record | cause | drawings | records |
|---|---|---|---|
| BLOCKS/LINE, BLOCKS/MTEXT | the source `*T` records come back (apartment: 29 LINE and 11 MTEXT in place of 10 merged LINE and 11 MTEXT with new handles) | apartment (dxf, dwg), 6 `dim1` files | 39 + 22, and 19 per `dim1` output |
| BLOCKS/BLOCK, BLOCKS/ENDBLK, TABLES/BLOCK_RECORD | the block, its end and its record keep the handle of the file | 6 `dim1` files (dxf, dwg) | 2 each |
| ENTITIES/ACAD_TABLE | group 340 names the kept block record | 6 `dim1` files (dxf, dwg) | 1 each |
| OBJECTS/DICTIONARY, DICTIONARYVAR, MLEADERSTYLE | an object that took a new handle after the rebuilt block (apartment: the dictionary 2253 is now 223E, and the references to it follow) | apartment (dxf: 3 + 1, dwg: 5 + 14 + 1) | as listed |

The six `dim1` drawings are `dim1-mattrib-2018.dwg`, `dim1-table90-2018.dwg`, `dim1-mattrib-2018.dxf`, `dim1-table90-2007.dxf`, `dim1-table90-2018.dxf` and `dim1-vp68-2018.dxf`.

# Golden refresh for the apartment sample's table style

`examples/apartment.dxf` was written by the old writer (a5991373), which put the cell styles of its TABLESTYLE in the order Data, Header, Title. AutoCAD, and the reader and
writer of today (5d67f225), use Data, Title, Header, so a read took the headings for the title and a regenerated table drew "ROOM SCHEDULE" at 160 and "Room" at 224
(the file draws them at 224 and 160). The sample now has the second and third cell-style blocks swapped (two lines: the text heights 140). Only the TABLESTYLE record moves:

| record | cause | format | drawings | records |
|---|---|---|---|---|
| OBJECTS/TABLESTYLE (1150) | the cell-style blocks of the sample in AutoCAD's order | dxf, dwg | examples/apartment.dxf | 1 each |

# Golden refresh for the findings of the final compare (7a4b0edc)

Seven fixes move 74 of the 126 outputs. The records that differ from the golden, compared one by one, are only these; no record is lost or added.

| record | cause | fix commit | format | drawings | records |
|---|---|---|---|---|---|
| BLOCKS/MTEXT, ENTITIES/MTEXT | MTEXT text cut in 250 characters before the `^I` coding (groups 1 and 3) | cut the MTEXT text in 250 characters before the ^I coding | dxf | 11 (BLOCKS), 7 (ENTITIES) | 454, 24 |
| BLOCKS/MTEXT | the same, seen through the DWG | the same | dwg | 1 | 1 |
| OBJECTS/MLINESTYLE | the built-in style is STANDARD in group 2 | write the built-in multiline style as STANDARD in its object | dxf, dwg | 24 | 24 each |
| OBJECTS/MLEADERSTYLE | group 330: the multileaders that use the style stay among its reactors | keep the multileaders that are an MLEADERSTYLE's persistent reactors | dxf, dwg | 24 | 38 each |
| OBJECTS/IMAGEDEF | the reactors in the order the file listed them | keep the order of the reactors of an IMAGEDEF | dxf | 3 | 6 |
| TABLES/TABLE | groups 71 and 340 of the DIMSTYLE table header | list the DIMSTYLE records the file listed | dxf | 13 | 13 |
| CLASSES/CLASS | group 280: the file's zombie flag | pin acadrust 35bc458 (flags_from_file) | dwg | 24 | 757 |

# Golden refresh for the raster variables

A file that listed image definitions but no RASTERVARIABLES object now saves without one (AutoCAD's own SAVEAS adds none), and the CLASS record of RASTERVARIABLES is no longer made for it. 15 of 126 outputs changed.
Compared record by record (the DWG as the DXF rendition of `cadcraft_dwg`) against the outputs before the change, the records that differ are only these causes; no other record changed.

| record | cause | format | drawings | records |
|---|---|---|---|---|
| CLASSES/CLASS | the file's own RASTERVARIABLES class stays where the file had it, at the end, instead of the generated one at the front; every CLASS record after it changes its position (the record key is the position) | dxf | 13 (Citycon 3 files 15, Even 18, Planeta 2 files 20 and 17, Toledo 6, Skr-Ayya 24, `skr-property` 243, `cyrela-field` pair 243, Cyrela 62, Firenze 63) | 1 to 243 each |
| OBJECTS/ACAD_PROXY_OBJECT | group 91 (the class number of the proxy) follows the class order | dxf | Skr-Ayya, `skr-property` | 1 each |
| OBJECTS/RASTERVARIABLES, DICTIONARY (root), XRECORD, TABLES/APPID, CLASSES/CLASS | the object is no longer written: one handle fewer is allocated, so the XRECORD and the APPID written after it take the previous handle, and the root dictionary loses its ACAD_IMAGE_VARS entry | dxf, dwg | Cyrela-Aura_Moema, Firenze | 1 RASTERVARIABLES, 1 DICTIONARY, 3 XRECORD, 2 APPID, 1 CLASS |

The DWG of the other drawings is unchanged. The golden directory was written again at this commit.

# Golden refresh for the empty image dictionary

An ACAD_IMAGE_DICT with no image definition is kept (AutoCAD's own save keeps it). 10 of 126 outputs changed: the DXF and the DWG of five drawings that have such a dictionary (Rocontec-Vila_Einstein, Tear_Incorporadora-Alvorada, and `tear-autocad-saved`, `tear-native` and `tear-raw` of rc2).
Compared record by record (the DWG as the DXF rendition of `cadcraft_dwg`) against the outputs before the change, the records that differ are only these; no other record changed.

| record | cause | format | drawings | records |
|---|---|---|---|---|
| OBJECTS/DICTIONARY (root, handle C) | the named object dictionary lists ACAD_IMAGE_DICT again | dxf, dwg | 5 | 1 each |
| OBJECTS/DICTIONARY (the empty image dictionary: 3FB209 in Rocontec, 4E6B in the four tear files) | the dictionary of the file comes back, with its handle | dxf, dwg | 5 | 1 each |

# Golden refresh for the acadrust-side findings

Five fixes move 64 of the 126 outputs, compared record by record with the outputs of the base (b0f04e7d, whose golden the strict check passed). No record is lost or added.

| record | cause | fix | format | drawings | records |
|---|---|---|---|---|---|
| OBJECTS/MLEADERSTYLE | group 298 kept (R2013+ and the DWG) | keep group 298 of an MLEADERSTYLE | dwg | 6 | 13 |
| TABLES/BLOCK_RECORD | the ACAD DesignCenter Data xdata through the DWG bridge | keep the DesignCenter Data xdata of a block record through the DWG bridge | dwg | 10 | 51 |
| TABLES/VPORT | group 63: ambient color ByLayer (256) instead of 250 | acadrust c6753f9 | dwg | 32 | 32 |
| ENTITIES/VERTEX | a ByLayer vertex keeps its own properties | acadrust b416245 | dxf | 1 | 13 |
| TABLES/LAYER | the xdata applications of a layer keep their order | acadrust b1921b8 | dxf | 12 | 1618 |

The width axis (5f7a992), the 1005 handle (579d393) and the same-release inspection (58fabeb) moved no output of the corpus.

# Golden refresh for the combined acadrust pin (1e5eaa6)

Two of the 126 outputs move, compared record by record with the outputs of aede6b44 (whose golden the strict check passed). The pin adds bridge-raw's imported HATCH
reverse reactors to the pin of the earlier refresh. No record is lost or added.

| record | cause | pin or fix | format | drawings | records |
|---|---|---|---|---|---|
| ENTITIES/LWPOLYLINE | the persistent reactors of the polyline a hatch is associated with keep the order of the imported link (DPInc 50C0AB: 50C0AC, 50C0AA, where aede6b44 had 50C0AA, 50C0AC) | acadrust da83d2d / 1e5eaa6 (imported hatch reverse reactors) | dxf, dwg | 1 (rt7/DPInc.dwg) | 1 each |

The measurement was made on a head that also merged raw280-zero-default (a659db03), which moved no output.
