//! PageList pins + tracking cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Pin` struct (with
//! `rowAndCell`, `cells` + `CellSubset`, `isBetween`, `before`, `leftWrap`,
//! `rightWrap`, `downOverflow`, `upOverflow`, and the remaining navigation
//! helpers) plus the tracking API: `pin`, `trackPin`, `untrackPin`,
//! `countTrackedPins`, `trackedPins`, `pinIsValid`, `pinIsActive`, `pinIsTop`,
//! and `pointFromPin`.
//!
//! PORT NOTE: the `Pin` struct *fields* are forward-declared in `mod.rs` so the
//! `PageList` struct fields (`viewport_pin: *mut Pin`) can reference them; this
//! sub-file owns the full `impl Pin` + the `PinSet` hashmap type and the
//! tracking methods.
//!
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + tests land with the
//! MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}
