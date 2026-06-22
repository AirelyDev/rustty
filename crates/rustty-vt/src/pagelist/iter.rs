//! PageList iterators cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Direction` enum and the
//! prompt/cell/row/page iterators: `PromptIterator` (`next`, `nextRightDown`,
//! `nextLeftUp`) + `promptIterator`, `CellIterator` (`next`) + `cellIterator`,
//! `RowIterator` (`next`) + `rowIterator`, and `PageIterator` (`next`,
//! `nextDown`, `nextUp`, `Chunk`) + `pageIterator`.
//!
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + tests land with the
//! MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}
