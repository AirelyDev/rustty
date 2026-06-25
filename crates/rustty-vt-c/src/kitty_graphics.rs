//! rustty-vt-c kitty_graphics
//!
//! PORTS: ghostty src/terminal/c/kitty_graphics.zig
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3, OPAQUE-ONLY). This sub-batch
//! lands ONLY the `KittyGraphics` handle type, which `terminal.zig` needs to
//! name (`TerminalData.kitty_graphics` returns it and `KittyGraphics =
//! kitty_gfx_c.KittyGraphics`). The full kitty graphics C API (ImageHandle,
//! PlacementIterator, every `kitty_*` fn + the wrapper structs and `test {}`
//! blocks) fills in a later sub-batch.
//!
//! PORT NOTE (opaque handle): the reference is
//!   pub const KittyGraphics = if (build_options.kitty_graphics)
//!       *kitty_storage.ImageStorage
//!   else
//!       *anyopaque;
//! i.e. a non-null pointer to the screen's `ImageStorage` (or an opaque pointer
//! when kitty graphics are compiled out). The `ImageStorage` C type is not part
//! of this mutual atom, so the handle is modelled as a raw opaque pointer
//! (`*mut c_void`). The fill sub-batch that lands `graphics_storage` swaps the
//! pointee to the concrete `ImageStorage` type without changing the ABI (it is a
//! pointer either way).

use std::ffi::c_void;

/// C: GhosttyKittyGraphics -> RusttyKittyGraphics (R2)
// PORT NOTE: opaque pointer handle. See module PORT NOTE above.
pub type KittyGraphics = *mut c_void;
