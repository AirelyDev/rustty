//! PageList access + introspection cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `getCell`, `getTopLeft`,
//! `getBottomRight`, `totalRows`, `totalPages`, `clearDirty`, `isDirty`,
//! `markDirty`, `Cell` struct (`isDirty`, `style`, `screenPoint`, ...), the
//! `Clone` struct (+ `TrackedPinsRemap`) and `clone`, `split`, `diagram`, and
//! `highlightSemanticContent` cluster, plus the `Scrollbar`/`Cell` `cval`/`eql`
//! helpers that belong to introspection.
//!
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + tests land with the
//! MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}
