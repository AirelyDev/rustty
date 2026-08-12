//! rustty-wasm — the WASM/web target module.
//!
//! PORTS: ghostty `src/main_wasm.zig`.
//!
//! This is the main file for the WASM module. The WASM module has to
//! export a C ABI compatible API.
//!
//! PORT STATUS: ported — COMPLETE (WS14). This crate produces a REAL freestanding
//! artifact: `target/wasm32-unknown-unknown/release/rustty_wasm.wasm`, built by
//! `.port/tools/build-main-wasm.sh --release`, with ZERO wasi imports (only the
//! `env` Canvas imports of `Atlas.Wasm`'s debug canvas) and no C cross-compiled.
//! `crates/rustty-wasm/tests/artifact_shape.rs` asserts its shape.
//!
//! The reference's `comptime { _ = @import(...); }` retention block is modelled by
//! (a) the crate DEPENDENCY edges (`rustty-os`/`rustty-config`/`rustty-vt`/
//! `rustty-font` — the `@import` analog that pulls each module into the link)
//! plus (b) the `#[used]` fn-pointer retention statics below that pin the
//! `config_*`/`atlas_*` C-ABI exports so `--gc-sections`/`wasm-opt` cannot drop
//! them.

// PORT: `const std = @import("std");` / `const builtin = @import("builtin");`.
// The Rust std/builtins are used directly; the optimization-mode switch below
// reads `rustty_build_config` (the `builtin.mode` analog).
#![allow(dead_code)]

// PORT:
//   comptime {
//       _ = @import("os/wasm.zig");
//       _ = @import("font/main.zig");
//       _ = @import("terminal/main.zig");
//       _ = @import("config.zig").Wasm;
//       _ = @import("App.zig").Wasm;
//   }
//
// The `_ = @import(...)` idiom force-ANALYZES a module so its `export fn`s are
// retained in the wasm artifact. In Rust two mechanisms combine:
//
//   * The crate DEPENDENCY edges on `rustty-os` (`os/wasm.zig`), `rustty-font`
//     (`font/main.zig`), `rustty-vt` (`terminal/main.zig`) and `rustty-config`
//     (`config.zig`) pull those modules into the link — the `_ = @import` analog.
//
//   * `#[no_mangle]` symbols in an rlib DEPENDENCY are dropped by the linker
//     unless referenced, and `pub use` does NOT re-export a symbol from a
//     cdylib. So the `config.zig`.`Wasm` and `font/Atlas.zig`.`Wasm` C-ABI
//     exports are pinned by the `#[used]` retention tuples below (the faithful
//     `_ = ...Wasm` analog).
//
// RETENTION SET STATUS (WS14; completed by the codex-vet W16-FONT-01 fix): 4 of
// the reference's 5 retentions map to a crate edge plus, where the module
// carries `export fn`s, a `#[used]` static. `font/main.zig` retains FOUR wasm
// groups (`Atlas.Wasm` + `DeferredFace.Wasm` + `face.web_canvas.Wasm` +
// `shape.web_canvas.Wasm`); the latter three live in the D1 shaper tier, so the
// `rustty-font-shaper` edge (feature `ghostty-wasm-abi`) + `retain_font_shaper`
// below carry them. The 5th,
// `_ = @import("App.zig").Wasm`, needs NO edge: that block is an EMPTY struct
// whose three exports are commented out UPSTREAM (`App.zig:577-604`), so
// retaining it emits zero symbols and pulls no App implementation — see the
// completed port at `rustty-core/src/app.rs` and the Cargo.toml rationale. A
// `rustty-core` edge would over-retain an entire application tier and make the
// freestanding link impossible (rustty-inspector → dcimgui-sys → freetype-sys →
// zlib-sys).
//
// The `config.zig`.`Wasm` and `Atlas.Wasm` exports are compiled unconditionally
// here because both dependencies are declared with `features = ["wasm"]`
// (Cargo.toml), so their `wasm_exports` modules are always in this crate's link.
mod retain {
    use rustty_config::wasm::wasm_exports::{
        config_finalize, config_free, config_load_string, config_new,
    };
    use rustty_config::Config;
    use rustty_font::atlas::wasm_exports::{
        atlas_clear, atlas_free, atlas_grow, atlas_new, atlas_reserve, atlas_set,
    };
    use rustty_font::atlas::{Atlas, Region};

    // A `#[used]` static of the four `config_*` C-ABI function pointers. `#[used]`
    // forbids dead-symbol elimination of this static, and its initializers name
    // each export — together that keeps the unmangled `config_new`/`config_free`/
    // `config_load_string`/`config_finalize` symbols in the emitted `.wasm`.
    #[used]
    static RETAIN_CONFIG_WASM_EXPORTS: (
        extern "C" fn() -> *mut Config,
        unsafe extern "C" fn(*mut Config),
        unsafe extern "C" fn(*mut Config, *const u8, usize),
        unsafe extern "C" fn(*mut Config),
    ) = (config_new, config_free, config_load_string, config_finalize);

    // The same mechanism for `font/Atlas.zig`'s `Wasm` block (WS14 T4) — the
    // retention the reference's `_ = @import("font/main.zig")` performs.
    // `atlas_debug_canvas` is deliberately NOT in this tuple: it is additionally
    // `#[cfg(target_arch = "wasm32")]` (it needs the browser's Canvas via `env`
    // imports), and a `#[used]` static naming it would not compile on the host,
    // where this crate must also build as an rlib. On wasm32 it is a `#[no_mangle]`
    // export of a cdylib crate's own dependency graph reached through this same
    // module, and the artifact-shape test asserts the six names below directly.
    // A named tuple type keeps clippy::type_complexity quiet without changing the
    // mechanism (the `#[used]` static must still NAME each export).
    type AtlasWasmExports = (
        extern "C" fn(u32, u8) -> *mut Atlas,
        unsafe extern "C" fn(*mut Atlas),
        unsafe extern "C" fn(*mut Atlas, u32, u32) -> *mut Region,
        unsafe extern "C" fn(*mut Atlas, *const Region, *const u8, usize),
        unsafe extern "C" fn(*mut Atlas, u32) -> bool,
        unsafe extern "C" fn(*mut Atlas),
    );

    #[used]
    static RETAIN_ATLAS_WASM_EXPORTS: AtlasWasmExports = (
        atlas_new,
        atlas_free,
        atlas_reserve,
        atlas_set,
        atlas_grow,
        atlas_clear,
    );
}

// The rest of the `_ = @import("font/main.zig")` retention: that comptime block
// retains FOUR wasm groups —
//
//   _ = Atlas.Wasm;              // `mod retain` above (rustty-font)
//   _ = DeferredFace.Wasm;       // `deferred_face_new`/`free`/`load`
//   _ = face.web_canvas.Wasm;    // `face_new`/`free`/`render_glyph`/`debug_canvas`
//   _ = shape.web_canvas.Wasm;   // `shaper_new`/`free`/`test`
//
// The latter three live in the SHAPER tier (D1 split: `rustty-font-shaper`,
// feature `ghostty-wasm-abi`), and are `cfg(target_arch = "wasm32")` there (the
// browser Canvas backend), so — unlike the tuples above — this retention module
// is wasm32-gated: on the host the symbols do not exist to name.
#[cfg(target_arch = "wasm32")]
mod retain_font_shaper {
    use rustty_font::atlas::Atlas;
    use rustty_font::glyph::Glyph;
    use rustty_font_shaper::deferred_face::{
        deferred_face_free, deferred_face_load, deferred_face_new, DeferredFace,
    };
    use rustty_font_shaper::face::web_canvas::{
        face_debug_canvas, face_free, face_new, face_render_glyph, Face,
    };
    use rustty_font_shaper::shape::web_canvas::{shaper_free, shaper_new, shaper_test, Shaper};
    use rustty_font_shaper::shared_grid::SharedGrid;

    // `_ = DeferredFace.Wasm;` (`DeferredFace.zig:423-466`).
    type DeferredFaceWasmExports = (
        extern "C" fn(*const u8, usize, u16) -> *mut DeferredFace,
        extern "C" fn(*mut DeferredFace),
        extern "C" fn(*mut DeferredFace, f32),
    );

    #[used]
    static RETAIN_DEFERRED_FACE_WASM_EXPORTS: DeferredFaceWasmExports =
        (deferred_face_new, deferred_face_free, deferred_face_load);

    // `_ = face.web_canvas.Wasm;` (`face/web_canvas.zig:489-557`).
    type FaceWasmExports = (
        extern "C" fn(*const u8, usize, u16, u16) -> *mut Face,
        extern "C" fn(*mut Face),
        extern "C" fn(*mut Face, *mut Atlas, u32) -> *mut Glyph,
        extern "C" fn(*mut Face),
    );

    #[used]
    static RETAIN_FACE_WASM_EXPORTS: FaceWasmExports =
        (face_new, face_free, face_render_glyph, face_debug_canvas);

    // `_ = shape.web_canvas.Wasm;` (`shaper/web_canvas.zig:231-305`).
    type ShaperWasmExports = (
        extern "C" fn(usize) -> *mut Shaper,
        extern "C" fn(*mut Shaper),
        extern "C" fn(*mut Shaper, *mut SharedGrid, *const u8, usize),
    );

    #[used]
    static RETAIN_SHAPER_WASM_EXPORTS: ShaperWasmExports = (shaper_new, shaper_free, shaper_test);
}

/// rustty-only host readback helpers — NOT a port.
///
/// `Atlas.Wasm` exports no way to READ the atlas back: the reference's only
/// readback path is `atlas_debug_canvas`, which paints the atlas into a DOM
/// canvas for a human to look at. A falsifiable pixel gate needs the bytes
/// themselves (the WS14 browser gate computes its expectation from the atlas the
/// MODULE holds, never from a JS-side copy of what it wrote), so these two
/// accessors expose `Atlas::data()`/`Atlas::size()` over the C ABI.
///
/// They keep the `rustty_wasm_` prefix precisely because they are rustty-only:
/// the unprefixed names in this artifact (`config_*`, `atlas_*`, `malloc`,
/// `free`) are exactly the reference's exports, and nothing else.
// PORT STATUS: rustty-only (no reference counterpart) — additive, wasm-only.
pub mod host_readback {
    use rustty_font::atlas::Atlas;

    /// Return a pointer to the atlas texture bytes and write the byte length into
    /// `out_len`, so the host can read the rendered pixels out of wasm memory.
    ///
    /// # Safety
    /// `atlas` must be null or a live atlas from `atlas_new`; `out_len` must be
    /// null or point at writable `usize` storage.
    #[no_mangle]
    pub unsafe extern "C" fn rustty_wasm_atlas_data(
        atlas: *const Atlas,
        out_len: *mut usize,
    ) -> *const u8 {
        if atlas.is_null() {
            if !out_len.is_null() {
                unsafe { *out_len = 0 };
            }
            return core::ptr::null();
        }
        let data = unsafe { &*atlas }.data();
        if !out_len.is_null() {
            unsafe { *out_len = data.len() };
        }
        data.as_ptr()
    }

    /// The atlas's width == height in pixels (`Atlas.size`). 0 for a null atlas.
    ///
    /// # Safety
    /// `atlas` must be null or a live atlas from `atlas_new`.
    #[no_mangle]
    pub unsafe extern "C" fn rustty_wasm_atlas_size(atlas: *const Atlas) -> u32 {
        if atlas.is_null() {
            return 0;
        }
        unsafe { &*atlas }.size()
    }

    // Same `#[used]` retention mechanism as `mod retain` above: these are
    // `#[no_mangle]` exports of an rlib+cdylib crate, so the static pins them.
    type HostReadbackExports = (
        unsafe extern "C" fn(*const Atlas, *mut usize) -> *const u8,
        unsafe extern "C" fn(*const Atlas) -> u32,
    );

    #[used]
    static RETAIN_HOST_READBACK: HostReadbackExports =
        (rustty_wasm_atlas_data, rustty_wasm_atlas_size);
}

/// The `std.log.Level` analog for the wasm module's compile-time log level.
// PORT: `std.log.Level` — the level enum `std_options.log_level` selects from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Err,
}

// PORT:
//   pub const std_options = struct {
//       // Set our log level. We try to get as much logging as possible but in
//       // ReleaseSmall mode where we're optimizing for space, we elevate the
//       // log level.
//       pub const log_level: std.log.Level = switch (builtin.mode) {
//           .Debug => .debug,
//           .ReleaseSmall => .warn,
//           else => .info,
//       };
//
//       // Set our log function
//       pub const logFn = @import("os/wasm/log.zig").log;
//   };
//
// Rust has no global `std_options`; the level is a plain const and the log sink
// is the `os/wasm/log` freestanding fn (installed/retained below). Per the
// build-config precedent (`mode_string`), the Rust build model distinguishes
// only `Debug` from optimized (`ReleaseFast`) — there is no `ReleaseSmall`
// cfg — so the `.ReleaseSmall => .warn` arm collapses into the `else => .info`
// path (documented divergence; a debug build still reports `.debug`).

/// Set our log level. We try to get as much logging as possible but in
/// ReleaseSmall mode where we're optimizing for space, we elevate the
/// log level.
pub const LOG_LEVEL: LogLevel = if cfg!(debug_assertions) {
    // .Debug => .debug
    LogLevel::Debug
} else {
    // .ReleaseSmall => .warn, else => .info — see the note above: the Rust
    // build model does not surface `ReleaseSmall`, so optimized builds take the
    // `.info` arm.
    LogLevel::Info
};

// Set our log function.
//
// PORT: `pub const logFn = @import("os/wasm/log.zig").log;`. The freestanding
// wasm log sink lives in `rustty_os::wasm::log::freestanding::log` and is only
// compiled for `wasm32` (it calls the imported `env.log`). Installing it as the
// process-wide sink is a wasm-only concern; here we RETAIN its symbol (the
// `logFn` binding analog) so it is present in the wasm artifact for the JS host
// to drive. On host builds the freestanding sink does not exist, so this is
// `wasm32`-gated.
#[cfg(target_arch = "wasm32")]
mod log_sink {
    /// The installed log function: `os/wasm/log`'s freestanding sink.
    // PORT: `pub const logFn = @import("os/wasm/log.zig").log;` — the binding
    // that routes formatted log lines to the host's imported `env.log`.
    #[used]
    static RETAIN_LOG_FN: fn(&str, &str, &str) = rustty_os::wasm::log::freestanding::log;
}
