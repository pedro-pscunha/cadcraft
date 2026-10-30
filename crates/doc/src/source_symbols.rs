//! Where the file a drawing was read from kept its blocks and symbols. A block's number in an
//! anonymous name (`*U12`, `*D3`) is its place among the block records, so a save writes the
//! blocks in the order the file had them, and the layout blocks under the names the file gave them.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceSymbols {
    /// The place of each block record in the file, by upper-case name (every block of the BLOCKS
    /// section, including the layout blocks and the ones CADCraft builds again on save).
    pub block_order: BTreeMap<String, usize>,
    /// The block each layout had, by upper-case layout name (`*Paper_Space`, `*Paper_Space0`…).
    pub layout_blocks: BTreeMap<String, String>,
}

impl SourceSymbols {
    /// The place of block `name` in the file the drawing came from, `None` for a block it did not have.
    pub fn block_rank(&self, name: &str) -> Option<usize> {
        self.block_order.get(&name.trim().to_ascii_uppercase()).copied()
    }
}
