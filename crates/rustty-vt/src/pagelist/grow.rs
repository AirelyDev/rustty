//! PageList growth + capacity + scrollbar cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `grow`, `increaseCapacity`
//! (+ `IncreaseCapacity` enum), `compact`, `scrollbar` (+ `Scrollbar` struct /
//! `cval` / `eql`), `maxSize`, `viewportRowOffset`, `fixupViewport`, and
//! `growRows` cluster.
//!
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + the 205-block test
//! split land with the MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}
