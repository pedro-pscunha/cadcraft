mod acad_interop;
#[cfg(not(target_arch = "wasm32"))]
mod acis_data_store;
mod activation_graph;
mod angle_header;
mod annotative_roundtrip;
mod appid_case;
mod arrow_block_refs;
mod assoc_array_roundtrip;
mod attrib_fields_roundtrip;
mod attrib_handles;
mod attrib_modes;
mod attrib_props;
mod block_entity_paper_flag;
mod block_entity_properties;
mod block_expansion;
mod block_order;
mod block_references;
mod bridge_header_groups;
mod bridge_object_groups;
mod class_records;
mod closed_spline;
mod codepages;
mod curve_canonical;
mod dim_block_record_roundtrip;
mod dim_block_resave_roundtrip;
mod dim_block_text;
mod dim_clone_insert_point;
mod dim_marks_roundtrip;
mod dim_moved_roundtrip;
mod dim_named_block;
mod dim_oblique_90;
mod dimassoc_foreign_roundtrip;
mod dimension_and_face_flags;
mod dimension_association_xdata;
mod dimension_group_order;
mod dimstyle_fidelity;
mod dimstyle_vars_roundtrip;
mod dimtfill_roundtrip;
mod drafting_spacing;
mod dstyle_order;
#[cfg(not(target_arch = "wasm32"))]
mod dwg_header_vars;
#[cfg(not(target_arch = "wasm32"))]
mod dwg_keeps_unknown;
mod dwg_layout_owner;
#[cfg(not(target_arch = "wasm32"))]
mod dwg_roundtrip_losses;
mod dwg_text_code_page;
mod dwt_template;
mod dxf_version_gates;
mod dxf_versions;
mod dynblock_read;
mod empty_xdictionary;
mod entity_normals;
mod entity_plane_manifest;
mod entity_props_roundtrip;
mod entity_subclass_groups;
mod fill_canonical;
mod followups_626;
mod foreign_passthrough;
mod geometry_snapshot;
mod groups_roundtrip;
mod handle_collision;
mod hatch_angles_edit;
mod hatch_assoc_roundtrip;
mod hatch_clockwise_arc_edge;
mod hatch_exact_edges;
mod hatch_groups_roundtrip;
mod hatch_pattern_def;
mod header_fidelity;
mod header_pointers;
#[cfg(not(target_arch = "wasm32"))]
mod heavy_polyline;
mod helix_roundtrip;
mod layer_transparency_alpha;
mod layout_canonical;
mod layout_paper_units;
mod layout_plot_pointers;
mod layout_plot_window;
mod layout_without_sheet;
mod leader_default_flags;
mod leader_links;
mod leader_links_roundtrip;
#[cfg(not(target_arch = "wasm32"))]
mod legacy_text_flags;
mod low_handseed;
mod minsert_subclass;
mod mleader_canonical;
mod mleader_codes;
mod mleader_content;
mod mleader_entity_overrides;
mod mleader_entity_tail;
mod mleader_pre2007;
mod mleader_proxy_graphics;
mod mleader_roots;
mod mleader_roundtrip;
mod mline_dictionary_names;
mod mline_roundtrip;
mod mline_vectors_edit;
#[cfg(not(target_arch = "wasm32"))]
mod model_layout_plot_type;
mod mtext_background_fill;
mod mtext_columns_dwg;
mod mtext_columns_roundtrip;
mod mtext_embedded_columns;
mod mtext_embedded_object;
mod mtext_line_spacing;
mod mtext_style_group;
mod multiline_attributes;
mod paperview_marker;
mod passthrough_image_roundtrip;
mod pdf_followups;
mod pdf_layers;
#[path = "../support/pdf_read.rs"]
mod pdf_read;
mod plot_layout_scale;
mod plot_model_png_scale;
mod plot_styles;
mod polyline3d_vertex_flags;
#[cfg(not(target_arch = "wasm32"))]
mod polyline_fit;
mod polyline_owned_vertices;
mod polyline_parent_plot_style;
#[cfg(not(target_arch = "wasm32"))]
mod raw_dwg_bridge;
mod region_roundtrip;
mod roundtrip_audit;
mod skipped_records;
mod sortents;
mod sortents_emission;
mod sortents_read;
mod sortents_roundtrip;
mod source_handles;
mod spline_fit_data;
mod spline_groups_roundtrip;
mod split_entities_section;
mod style_typeface;
mod svg_arc_budget;
mod svg_tiny_extents;
mod table_cell_blocks;
mod table_cell_fields;
mod table_cellstyle_roundtrip;
mod table_codes;
mod table_counts;
mod table_style_codes;
mod tables_objects_roundtrip;
mod tablestyle_border_dwg;
mod tablestyle_cell_order;
mod tablestyle_true_colors;
mod text_canonical;
mod text_direction_roundtrip;
mod text_generation_flags;
mod text_insert_moved;
mod text_left_justify;
mod text_style_group;
mod tolerance_roundtrip;
mod transparency_alpha;
mod unknown_entity_block;
mod unused_definitions;
mod unused_symbol_names;
mod user_lin_pat_roundtrip;
mod view_tables_roundtrip;
mod viewport_clip_roundtrip;
mod vport_tiles;
mod vx_table;
mod wipeout_image_edit;
#[cfg(not(target_arch = "wasm32"))]
mod writer_aci_fallback;
#[cfg(not(target_arch = "wasm32"))]
mod writer_block_fields;
#[cfg(not(target_arch = "wasm32"))]
mod writer_dictionary_policy;
#[cfg(not(target_arch = "wasm32"))]
mod writer_plot_fields;
#[cfg(not(target_arch = "wasm32"))]
mod writer_style_fields;
#[cfg(not(target_arch = "wasm32"))]
mod writer_symbol_flags;
#[cfg(not(target_arch = "wasm32"))]
mod writer_view_fields;
#[cfg(not(target_arch = "wasm32"))]
mod writer_viewport_status;
#[cfg(not(target_arch = "wasm32"))]
mod writer_vport_snap;
mod xref_roundtrip;
