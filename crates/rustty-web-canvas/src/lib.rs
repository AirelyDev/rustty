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

// ---------------------------------------------------------------------------
// LOG BACKEND (task-14c-4 fix round 1, F3):
// EVERY unmangled export below opens with
// `rustty_font_shaper::install_wasm_log_backend()`.
//
// `.port/parity-closure/DECISION-logging.md` §6r3 finding 3: a restored `log::`
// site emits nothing unless the RUNNING ARTIFACT registered a backend, and the
// registration belongs at the artifact's FIRST JS-callable entry point. That is
// an ARTIFACT-wide contract, and this artifact's entry points are split across
// two crates: `rustty-font-shaper` owns the seven `rustty_wc_face_*` /
// `rustty_wc_shaper_*` exports, this file owns the twelve below. Installing from
// only the first set left a real hole — `rustty_wc_deferred_has_codepoint` calls
// `DeferredFace::has_codepoint`, whose web-canvas arm emits
// `failed to init face for codepoint check`, so a host driving only the deferred
// surface lost it. The demo happened not to hit it only because it calls
// `rustty_wc_alloc` (silent) then `rustty_wc_face_new` (which installs).
//
// The installer is `Once`-guarded and leaves an already-installed logger's
// filter alone (§6r CRITICAL 1), so the repetition costs one atomic load per
// call and cannot clobber an embedder's own backend. It is a no-op on non-wasm
// targets and callable there, so no call site needs a `cfg`.
//
// IF YOU ADD A NEW EXPORT TO THIS ARTIFACT, FROM EITHER CRATE, CALL IT FIRST.
// ---------------------------------------------------------------------------

/// Create a texture `Atlas` (`size`×`size`, `format`: 0=grayscale, 1=BGR,
/// 2=BGRA). Returns null on OOM. Free with `rustty_wc_atlas_free`.
#[no_mangle]
pub extern "C" fn rustty_wc_atlas_new(size: u32, format: u8) -> *mut Atlas<Global> {
    rustty_font_shaper::install_wasm_log_backend();
    match Atlas::init(Global, size, format_from_u8(format)) {
        Ok(atlas) => Box::into_raw(Box::new(atlas)),
        Err(_) => core::ptr::null_mut(),
    }
}

/// Free an `Atlas` created by `rustty_wc_atlas_new`.
///
/// # Safety
/// `ptr` must be null, or a pointer returned by `rustty_wc_atlas_new` that has not
/// already been freed. WS19 (audit row N11): marked `unsafe` to match the
/// `rustty-vt-c` / `rustty-apprt-c` C-ABI convention (`pub unsafe extern "C" fn`)
/// rather than silencing `clippy::not_unsafe_ptr_arg_deref` — the pointer contract is
/// real and belongs in the signature.
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_atlas_free(ptr: *mut Atlas<Global>) {
    rustty_font_shaper::install_wasm_log_backend();
    if !ptr.is_null() {
        let atlas = unsafe { Box::from_raw(ptr) };
        atlas.deinit(Global);
    }
}

/// Return a pointer to the atlas texture bytes and write the byte length into
/// `out_len`, so the host can read the rendered glyph pixels out of wasm memory.
///
/// # Safety
/// `ptr` must be null or a live atlas from `rustty_wc_atlas_new`; `out_len` must be
/// null or point at writable `usize` storage.
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_atlas_data(
    ptr: *mut Atlas<Global>,
    out_len: *mut usize,
) -> *const u8 {
    rustty_font_shaper::install_wasm_log_backend();
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
///
/// # Safety
/// `ptr` must be null or a live atlas from `rustty_wc_atlas_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_atlas_size(ptr: *mut Atlas<Global>) -> u32 {
    rustty_font_shaper::install_wasm_log_backend();
    if ptr.is_null() {
        return 0;
    }
    unsafe { &*ptr }.size()
}

// --- DeferredFace (web_canvas payload) harness exports ---------------------
//
// The reference `DeferredFace.zig` carries NO `pub const Wasm = struct` C-ABI, so
// these are LOCAL harness exports of this deliverable (exactly like the
// `rustty_wc_atlas_*` helpers above) — not a port of anything. They exist so
// `.port/tools/web-canvas-instantiate.mjs` can drive the ported `web_canvas`
// `DeferredFace` arms (`familyName`/`name`/`load`/`hasCodepoint`) against the
// mock Canvas and assert their behaviour, instead of the port resting on a
// compile-only claim.

#[cfg(target_arch = "wasm32")]
use rustty_font_shaper::deferred_face::WebCanvasData;
#[cfg(target_arch = "wasm32")]
use rustty_font_shaper::face::{
    DesiredSize, Face, Options, DEFAULT_DPI, FREETYPE_LOAD_FLAGS_DEFAULT,
};
#[cfg(target_arch = "wasm32")]
use rustty_font_shaper::library::Library;
#[cfg(target_arch = "wasm32")]
use rustty_font_shaper::{DeferredFace, Presentation};

/// `presentation`: 0 = text, non-zero = emoji.
#[cfg(target_arch = "wasm32")]
fn presentation_from_u16(p: u16) -> Presentation {
    if p == 0 {
        Presentation::Text
    } else {
        Presentation::Emoji
    }
}

/// Build a `DeferredFace` carrying a `web_canvas` payload for the CSS "font"
/// string `ptr[..len]`. Free with `rustty_wc_deferred_free`.
///
/// # Safety
/// `ptr[..len]` must be a readable byte range.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_new(
    ptr: *const u8,
    len: usize,
    presentation: u16,
) -> *mut DeferredFace {
    rustty_font_shaper::install_wasm_log_backend();
    let raw = unsafe { core::slice::from_raw_parts(ptr, len) };
    Box::into_raw(Box::new(DeferredFace {
        wc: Some(WebCanvasData {
            font_str: raw.to_vec(),
            presentation: presentation_from_u16(presentation),
        }),
    }))
}

/// Free a `DeferredFace` from `rustty_wc_deferred_new`.
///
/// # Safety
/// `ptr` must be null or a live pointer from `rustty_wc_deferred_new`.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_free(ptr: *mut DeferredFace) {
    rustty_font_shaper::install_wasm_log_backend();
    if !ptr.is_null() {
        let mut df = unsafe { Box::from_raw(ptr) };
        df.deinit();
    }
}

/// `DeferredFace.name` into `out[..cap]`; returns the byte length written, or
/// `usize::MAX` on error.
///
/// # Safety
/// `ptr` must be live; `out[..cap]` must be writable.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_name(
    ptr: *mut DeferredFace,
    out: *mut u8,
    cap: usize,
) -> usize {
    rustty_font_shaper::install_wasm_log_backend();
    let df = unsafe { &*ptr };
    let buf = unsafe { core::slice::from_raw_parts_mut(out, cap) };
    match df.name(buf) {
        Ok(s) => s.len(),
        Err(_) => usize::MAX,
    }
}

/// `DeferredFace.familyName` into `out[..cap]`; returns the byte length written,
/// or `usize::MAX` on error.
///
/// # Safety
/// `ptr` must be live; `out[..cap]` must be writable.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_family_name(
    ptr: *mut DeferredFace,
    out: *mut u8,
    cap: usize,
) -> usize {
    rustty_font_shaper::install_wasm_log_backend();
    let df = unsafe { &*ptr };
    let buf = unsafe { core::slice::from_raw_parts_mut(out, cap) };
    match df.family_name(buf) {
        Ok(s) => s.len(),
        Err(_) => usize::MAX,
    }
}

/// `DeferredFace.hasCodepoint`. `presentation`: 0 = text, 1 = emoji, 2 = null
/// (`?Presentation` = none). Returns 1/0.
///
/// # Safety
/// `ptr` must be a live pointer from `rustty_wc_deferred_new`.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_has_codepoint(
    ptr: *mut DeferredFace,
    cp: u32,
    presentation: u16,
) -> i32 {
    rustty_font_shaper::install_wasm_log_backend();
    let df = unsafe { &*ptr };
    let p = if presentation >= 2 {
        None
    } else {
        Some(presentation_from_u16(presentation))
    };
    i32::from(df.has_codepoint(cp, p))
}

/// `DeferredFace.load` at `pts` points. Returns a `Face` to be freed with
/// `rustty_wc_face_free`, or null on error.
///
/// # Safety
/// `ptr` must be a live pointer from `rustty_wc_deferred_new`.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_deferred_load(ptr: *mut DeferredFace, pts: f32) -> *mut Face {
    rustty_font_shaper::install_wasm_log_backend();
    let df = unsafe { &mut *ptr };
    let lib = match Library::init() {
        Ok(lib) => lib,
        Err(_) => return core::ptr::null_mut(),
    };
    let opts = Options {
        size: DesiredSize {
            points: pts,
            xdpi: DEFAULT_DPI,
            ydpi: DEFAULT_DPI,
        },
        freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
    };
    match df.load(lib, opts) {
        Ok(face) => Box::into_raw(Box::new(face)),
        Err(_) => core::ptr::null_mut(),
    }
}

// --- wasm host-memory conveniences (the `rustty_wasm_*` pattern from vt-c) ---

/// Allocate `len` bytes in the module's linear memory (align 1). Free with
/// `rustty_wc_free`. Returns null on zero length or OOM.
#[no_mangle]
pub extern "C" fn rustty_wc_alloc(len: usize) -> *mut u8 {
    rustty_font_shaper::install_wasm_log_backend();
    if len == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: len > 0; align 1 is always valid.
    let layout = std::alloc::Layout::from_size_align(len, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Free a buffer returned by `rustty_wc_alloc` (`len` must match the allocation).
///
/// # Safety
/// `ptr` must be null or a pointer returned by `rustty_wc_alloc` that has not already
/// been freed, and `len` must equal the length passed to that call.
#[no_mangle]
pub unsafe extern "C" fn rustty_wc_free(ptr: *mut u8, len: usize) {
    rustty_font_shaper::install_wasm_log_backend();
    if ptr.is_null() || len == 0 {
        return;
    }
    let layout = std::alloc::Layout::from_size_align(len, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// ---------------------------------------------------------------------------
// RUSTTY-ONLY guard for the artifact-wide log-backend contract documented above.
// Not a port — it has no ghostty analogue.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod install_contract_tests {
    /// Every unmangled export in this file must call
    /// `rustty_font_shaper::install_wasm_log_backend()` as its FIRST statement.
    ///
    /// This exists because the contract is artifact-wide but the artifact's
    /// entry points are split across two crates, so nothing in the type system
    /// or the build ties them together — exactly how the twelve exports below
    /// were shipped without it in task-14c-4's first pass, leaving
    /// `rustty_wc_deferred_has_codepoint`'s restored
    /// `failed to init face for codepoint check` unreachable for a host that
    /// drives only the deferred surface. `.port/parity-closure/
    /// DECISION-logging.md` §6r3 finding 3 / §8r is the same "one artifact over"
    /// shape that took the wasm gate three review rounds to close.
    ///
    /// Scans this file's own SOURCE rather than its symbols because there is no
    /// runtime way to observe "did this export install the backend" without a
    /// wasm host, which this environment does not have.
    #[test]
    fn every_unmangled_export_installs_the_log_backend() {
        const SRC: &str = include_str!("lib.rs");
        const CALL: &str = "rustty_font_shaper::install_wasm_log_backend();";

        let lines: Vec<&str> = SRC.lines().collect();
        let mut checked: Vec<&str> = Vec::new();
        let mut missing: Vec<&str> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if !(t.starts_with("pub extern \"C\" fn")
                || t.starts_with("pub unsafe extern \"C\" fn"))
            {
                continue;
            }
            // The exported name, for the failure message.
            let name = t
                .rsplit("fn ")
                .next()
                .unwrap_or(t)
                .split(['(', '<'])
                .next()
                .unwrap_or(t);

            // Walk forward to the first line ending the signature (`{`), then
            // take the next non-blank line: it must be the install call.
            let mut j = i;
            while j < lines.len() && !lines[j].trim_end().ends_with('{') {
                j += 1;
            }
            let first_stmt = lines
                .get(j + 1..)
                .unwrap_or(&[])
                .iter()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim())
                .unwrap_or("");

            checked.push(name);
            if first_stmt != CALL {
                missing.push(name);
            }
        }

        // The scan itself must not silently find nothing (the vacuous-pass trap).
        assert_eq!(
            12,
            checked.len(),
            "expected 12 unmangled exports in this file, scanned {checked:?}"
        );
        assert_eq!(
            Vec::<&str>::new(),
            missing,
            "these exports do not open with `{CALL}`"
        );
    }
}
