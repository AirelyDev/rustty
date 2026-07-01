//! `sprite/draw/` — the per-glyph sprite drawing modules.
//!
//! PORT NOTE: this is the Rust module root for ghostty's `src/font/sprite/draw/`
//! directory. The shared `common` helpers plus the small/mid draw modules that
//! only need already-landed z2d primitives are ported here. `branch` and
//! `special` drive the z2d MANAGED Context drawing API
//! (`Context::set_source`/`move_to`/`curve_to`/`arc`/`fill`/`stroke`), which is
//! now landed in `rustty-z2d`. Modules deferred to later batches:
//!   - `box.zig` (929 lines) has its own dedicated batch; `r#box` currently
//!     carries only the diagonal helpers `powerline` imports plus `arc` (see box.rs).
//!   - the large legacy-computing / symbols modules remain in their own batches.

pub mod block;
pub mod branch;
// PORT NOTE: `box` is a Rust keyword; the module keeps the reference name via
// the raw identifier `r#box`.
pub mod r#box;
pub mod braille;
pub mod common;
pub mod geometric_shapes;
pub mod powerline;
pub mod special;
