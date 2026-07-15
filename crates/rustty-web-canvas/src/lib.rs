//! rustty-web-canvas — the browser-freestanding (`wasm32-unknown-unknown`)
//! deliverable.
//!
//! A small cdylib that pulls the VT terminal C-ABI (`rustty-vt-c`) + the
//! font-shaper `web_canvas` backend (`rustty-font-shaper`) and exports BOTH so a
//! browser can run a terminal AND shape/render text against a real (offscreen)
//! Canvas. It compiles to a PURE-RUST `.wasm` with ZERO `wasi_snapshot_preview1`
//! imports — the only imports are the `env` Canvas ops the `web_canvas` face
//! declares (`rustty_wc_*`, implemented by `.port/tools/web-canvas-glue.mjs`).
//!
//! RETENTION: `#[no_mangle]` symbols in an rlib DEPENDENCY are dropped by
//! `wasm-ld --gc-sections` unless referenced (same mechanism `rustty-wasm`
//! documents for the `config_*` exports). This crate is the cdylib ROOT, so its
//! OWN `#[no_mangle]` exports (the `rustty_wc_atlas_*` / `rustty_wc_alloc*`
//! helpers below) are always kept; the dependency C-ABIs — the `rustty-vt-c`
//! terminal exports and the `rustty-font-shaper` `web_canvas` exports — are
//! pinned by the `#[used]` function-pointer tuples below so they survive into
//! the emitted `.wasm`.
//!
//! PANIC=ABORT: `wasm32-unknown-unknown` has no unwinder, so panics abort (the
//! `panic = "abort"` semantics the port mandates) with no host import.

#![allow(clippy::missing_safety_doc)]

use allocator_api2::alloc::Global;
use rustty_font::atlas::{Atlas, Format};

// ---------------------------------------------------------------------------
// Retention of the dependency C-ABIs (wasm only — the exports only exist / are
// only meaningful in the wasm cdylib).
// ---------------------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
mod retain {
    /// A `Sync` wrapper for a retained C-ABI function address (raw `*const ()` is
    /// not `Sync`, but a function symbol's address is trivially shareable — it is
    /// only ever used to pin the symbol against dead-code elimination).
    #[repr(transparent)]
    struct SyncPtr(#[allow(dead_code)] *const ());
    // SAFETY: the pointer is a code address used solely for `#[used]` retention;
    // it is never dereferenced or mutated.
    unsafe impl Sync for SyncPtr {}

    /// Pin a representative slice of the `rustty-vt-c` terminal C-ABI (terminal
    /// lifecycle + `vt_write` + the plain/selection formatters + the type-layout
    /// JSON) so a browser host can drive the VT engine. The full C-ABI is
    /// available by building `rustty-vt-c` directly; this subset covers the
    /// terminal-in-browser + roundtrip flow.
    #[used]
    static RETAIN_VT_C: [SyncPtr; 11] = [
        SyncPtr(rustty_vt_c::rustty_alloc as *const ()),
        SyncPtr(rustty_vt_c::rustty_free as *const ()),
        SyncPtr(rustty_vt_c::rustty_type_json as *const ()),
        SyncPtr(rustty_vt_c::rustty_terminal_new as *const ()),
        SyncPtr(rustty_vt_c::rustty_terminal_free as *const ()),
        SyncPtr(rustty_vt_c::rustty_terminal_reset as *const ()),
        SyncPtr(rustty_vt_c::rustty_terminal_resize as *const ()),
        SyncPtr(rustty_vt_c::rustty_terminal_vt_write as *const ()),
        SyncPtr(rustty_vt_c::rustty_formatter_terminal_new as *const ()),
        SyncPtr(rustty_vt_c::rustty_formatter_format_alloc as *const ()),
        SyncPtr(rustty_vt_c::rustty_formatter_free as *const ()),
    ];

    /// Pin the font-shaper `web_canvas` C-ABI: the Canvas-backed `Face`
    /// (`rustty_wc_face_*`) and the `Shaper` (`rustty_wc_shaper_*`).
    #[used]
    static RETAIN_WEB_CANVAS: [SyncPtr; 7] = [
        SyncPtr(rustty_font_shaper::face::web_canvas::rustty_wc_face_new as *const ()),
        SyncPtr(rustty_font_shaper::face::web_canvas::rustty_wc_face_free as *const ()),
        SyncPtr(rustty_font_shaper::face::web_canvas::rustty_wc_face_render_glyph as *const ()),
        SyncPtr(rustty_font_shaper::face::web_canvas::rustty_wc_face_debug_canvas as *const ()),
        SyncPtr(rustty_font_shaper::shape::web_canvas::rustty_wc_shaper_new as *const ()),
        SyncPtr(rustty_font_shaper::shape::web_canvas::rustty_wc_shaper_free as *const ()),
        SyncPtr(rustty_font_shaper::shape::web_canvas::rustty_wc_shaper_shape as *const ()),
    ];
}

// ---------------------------------------------------------------------------
// Local helper exports (own `#[no_mangle]` in the cdylib root → always kept).
// ---------------------------------------------------------------------------

/// Convert a `u8` format tag into the `Atlas` `Format`.
fn format_from_u8(f: u8) -> Format {
    match f {
        2 => Format::Bgra,
        1 => Format::Bgr,
        _ => Format::Grayscale,
    }
}

/// Create a texture `Atlas` (`size`×`size`, `format`: 0=grayscale, 1=BGR,
/// 2=BGRA). Returns null on OOM. Free with `rustty_wc_atlas_free`.
#[no_mangle]
pub extern "C" fn rustty_wc_atlas_new(size: u32, format: u8) -> *mut Atlas<Global> {
    match Atlas::init(Global, size, format_from_u8(format)) {
        Ok(atlas) => Box::into_raw(Box::new(atlas)),
        Err(_) => core::ptr::null_mut(),
    }
}

/// Free an `Atlas` created by `rustty_wc_atlas_new`.
#[no_mangle]
pub extern "C" fn rustty_wc_atlas_free(ptr: *mut Atlas<Global>) {
    if !ptr.is_null() {
        let atlas = unsafe { Box::from_raw(ptr) };
        atlas.deinit(Global);
    }
}

/// Return a pointer to the atlas texture bytes and write the byte length into
/// `out_len`, so the host can read the rendered glyph pixels out of wasm memory.
#[no_mangle]
pub extern "C" fn rustty_wc_atlas_data(ptr: *mut Atlas<Global>, out_len: *mut usize) -> *const u8 {
    if ptr.is_null() {
        if !out_len.is_null() {
            unsafe { *out_len = 0 };
        }
        return core::ptr::null();
    }
    let atlas = unsafe { &*ptr };
    let data = atlas.data();
    if !out_len.is_null() {
        unsafe { *out_len = data.len() };
    }
    data.as_ptr()
}

/// The atlas dimension (it is square: `size`×`size`).
#[no_mangle]
pub extern "C" fn rustty_wc_atlas_size(ptr: *mut Atlas<Global>) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe { &*ptr }.size()
}

// --- wasm host-memory conveniences (the `rustty_wasm_*` pattern from vt-c) ---

/// Allocate `len` bytes in the module's linear memory (align 1). Free with
/// `rustty_wc_free`. Returns null on zero length or OOM.
#[no_mangle]
pub extern "C" fn rustty_wc_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: len > 0; align 1 is always valid.
    let layout = std::alloc::Layout::from_size_align(len, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Free a buffer returned by `rustty_wc_alloc` (`len` must match the allocation).
#[no_mangle]
pub extern "C" fn rustty_wc_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let layout = std::alloc::Layout::from_size_align(len, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) };
}
