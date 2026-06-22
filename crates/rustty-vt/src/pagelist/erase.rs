//! PageList erase cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `eraseRow`, `eraseRowBounded`,
//! `eraseHistory`, `eraseActive`, `eraseRows`, and `erasePage` cluster.
//!
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + tests land with the
//! MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}

// PORT NOTE (DEFERRED SEAM): the `eraseRow`, `eraseRowBounded`, `eraseHistory`,
// and `eraseActive` members belong to this file's owner doc header, but their
// bodies are driven by siblings that are genuinely unported at this point in
// the port. Deferring them keeps the crate compiling cumulatively. Each is
// deferred to the cluster that owns its missing siblings, where it lands fully
// ported — NOT stubbed.
//
// The genuinely-unported siblings these methods require:
//
//   pin cluster (`pin.rs`):
//     * `pin` (point->pin lookup)
//   grow cluster (`grow.rs`, itself DEFERRED there to the pin cluster):
//     * `fixupViewport` (its body needs the pin-cluster `pinIsActive`)
//   erase cluster (`erase.rs`, a later erase sub-batch):
//     * `eraseRows` (point-range erase) + `erasePage`
//
// Deferred members (re-homed fully ported when those siblings land):
//
//   * `eraseRow`        — needs `pin` and `fixupViewport`. (Also uses the
//                         already-ported `fastmem::rotateOnce`,
//                         `Page.cloneRowFrom`, `Page.clearCells`, and the
//                         `tracked_pins` map, but `pin`/`fixupViewport` are
//                         genuinely unported.)
//   * `eraseRowBounded` — needs `pin`. (Also uses the already-ported
//                         `fastmem::rotateOnce`, `Page.cloneRowFrom`,
//                         `Page.clearCells`, the `tracked_pins` map, and the
//                         `viewport`/`viewport_pin`/`viewport_pin_row_offset`
//                         fields, but `pin` is genuinely unported.)
//   * `eraseHistory`    — needs `eraseRows`.
//   * `eraseActive`     — needs `eraseRows`.
//
// `eraseRows`/`erasePage` are owned by this erase cluster but land in a later
// erase sub-batch (this sub-batch's task is `eraseRow`/`eraseRowBounded`/
// `eraseHistory`/`eraseActive` only). None of the deferred members has a ported
// caller in any cluster landed so far, so deferring them does not affect any
// method ported here. The 205-block test split lands in the dedicated test
// phase.
