//! Where the file a drawing was read from kept its blocks and symbols. A block's number in an
//! anonymous name (`*U12`, `*D3`) is its place among the block records, so a save writes the
//! blocks in the order the file had them, and the layout blocks under the names the file gave them.
//! A save also gives the tables, symbol records, blocks and layouts the handles the file gave them.

use std::collections::{BTreeMap, BTreeSet};

use crate::foreign::ForeignKey;
use crate::{Block, Handle, RawTag};

/// The graphics block (`*T1`) an ACAD_TABLE of the file drew itself with, and the key of the table (and of what its drawing depends on)
/// when it was read: a save writes the block as read while the key still matches, and draws it again once the table was edited.
#[derive(Clone, Debug, PartialEq)]
pub struct TableBlock {
    pub key: u64,
    pub block: std::sync::Arc<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceSymbols {
    /// The place of each block record in the file, by upper-case name (every block of the BLOCKS
    /// section, including the layout blocks and the ones CADCraft builds again on save).
    pub block_order: BTreeMap<String, usize>,
    /// The block each layout had, by upper-case layout name (`*Paper_Space`, `*Paper_Space0`…).
    pub layout_blocks: BTreeMap<String, String>,
    /// The file's handle of each table, symbol record, block, block end and dictionary entry
    /// CADCraft writes itself, by role (see [`ForeignKey`]). A role that is not here (a record made
    /// since) gets a new handle.
    pub handles: BTreeMap<ForeignKey, Handle>,
    /// The references to each block (the `{BLKREFS` list of its record) in the order the file listed them, by upper-case block
    /// name. AutoCAD lists them in the order they were made, which a handle does not say.
    pub block_refs: BTreeMap<String, Vec<Handle>>,
    /// The upper-case names of the layouts the file gave viewports but no paper-space viewport (id 1): a save does not add one.
    pub layouts_without_sheet: BTreeSet<String>,
    /// The CLASS records of the file in its order, with the groups the writer repeats (1 name, 2 C++ name, 3 application, 90 proxy
    /// flags, 91 instance count, 280, 281): a save keeps the application and the proxy flags AutoCAD gave each class, and the classes no object uses.
    pub classes: Vec<Vec<RawTag>>,
    /// The persistent reactors of each entity that had two or more, in the order the file listed them (AutoCAD notifies them in
    /// that order, and its own save keeps it).
    pub reactor_order: BTreeMap<Handle, Vec<Handle>>,
    /// The upper-case names of the blocks whose record held the `DesignCenter Data` xdata beside its group 70 (AutoCAD writes it in every
    /// version for the blocks that had it): a save keeps it for them, and before 2007 for every block with units.
    pub design_center_blocks: BTreeSet<String>,
    /// The graphics block of each ACAD_TABLE of the file, by the table's handle.
    pub table_blocks: BTreeMap<Handle, TableBlock>,
}

impl SourceSymbols {
    /// The place of block `name` in the file the drawing came from, `None` for a block it did not have.
    pub fn block_rank(&self, name: &str) -> Option<usize> {
        self.block_order.get(&name.trim().to_ascii_uppercase()).copied()
    }
}
