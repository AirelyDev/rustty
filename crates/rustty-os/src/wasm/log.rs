//! rustty-os::wasm::log
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/wasm/log.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std` and `wasm = @import("../wasm.zig")`. The
// implementation is wasm-only (freestanding browser target) and calls an
// `extern "env" fn log`. In Rust the module body is `cfg(target_arch = "wasm32")`;
// `std.log`'s level/scope formatting maps to a formatted byte string handed to
// the imported `env.log`.
#![allow(dead_code)]

// Use the correct implementation
#[cfg(target_arch = "wasm32")]
pub use freestanding::log;

/// Freestanding implementation calls an extern "log" function.
#[cfg(target_arch = "wasm32")]
pub mod freestanding {
    // The function std.log will call.
    //
    // PORT NOTE: the reference is a `comptime`-generic `fn log(level, scope, format,
    // args)` matching the `std.log` signature. In Rust there is no equivalent
    // comptime log hook; the parity-preserving shape is a function that takes the
    // already-formatted level text, scope, format string, and formatted args and
    // emits the same `level(scope): message` line to the host.
    pub fn log(level_txt: &str, scope: &str, formatted: &str) {
        // The buffer for putting our log message. We try to use a stack-allocated
        // buffer first because we want to avoid allocation. If we are logging
        // an error DUE to an OOM, allocating will of course fail and we'd like
        // to see the error message so we prefer to use this.
        let mut buf = [0u8; 2048];

        // Build the string
        // PORT NOTE: `const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope)
        // ++ "): ";` then `const txt = level_txt ++ prefix ++ format;`.
        let prefix = if scope == "default" {
            ": ".to_string()
        } else {
            format!("({scope}): ")
        };
        let txt = format!("{level_txt}{prefix}{formatted}");

        // Format. We attempt to use a stack-allocated string first and if that
        // fails we'll try to allocate.
        let bytes = txt.as_bytes();
        let str: &[u8] = if bytes.len() <= buf.len() {
            buf[..bytes.len()].copy_from_slice(bytes);
            &buf[..bytes.len()]
        } else {
            bytes
        };

        // Send it over to the JS side
        unsafe { js::log(str.as_ptr(), str.len()) };
    }

    // We wrap our externs in this namespace so we can reuse symbols, otherwise
    // "log" would collide.
    //
    // PORT NOTE: `#[link(wasm_import_module = "env")]` is REQUIRED, not
    // decoration. The reference declares this as `extern "env" fn log(...)`, and
    // Zig's freestanding link has nothing else named `log`. Without the explicit
    // import module, rustc emits a plain undefined symbol named `log`, which
    // `wasm-ld` resolves against the Rust sysroot's libm `log` (`f64 -> f64`):
    // the link then silently produces a `signature_mismatch:log` trap stub and
    // NO `env.log` import at all, so every logged line would trap instead of
    // reaching the host. The sibling wasm externs use the same attribute
    // (`rustty-font` `atlas.rs`, `rustty-font-shaper` `face/web_canvas.rs`).
    mod js {
        #[link(wasm_import_module = "env")]
        extern "C" {
            #[link_name = "log"]
            pub fn log(ptr: *const u8, len: usize);
        }
    }
}
