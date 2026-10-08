//! rustty-vt-c sys
//!
//! PORTS: ghostty src/terminal/c/sys.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const lib = @import("../lib.zig");
//   const CAllocator = lib.alloc.Allocator;
//   const terminal_sys = @import("../sys.zig");
//   const Result = @import("result.zig").Result;
// `terminal_sys` is rustty-vt's `sys` module (the swappable `decode_png` slot,
// `Image`, `DecodeError`); `CAllocator` is rustty-lib's C `Allocator`.

#![allow(non_upper_case_globals)]

use crate::result::Result;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::sys as terminal_sys;
use std::sync::Mutex;

/// C: GhosttySysImage -> RusttySysImage (R2)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: *mut u8,
    pub data_len: usize,
}

/// C: GhosttySysDecodePngFn -> RusttySysDecodePngFn (R2)
// PORT NOTE: `*const fn (?*anyopaque, *const CAllocator, [*]const u8, usize,
// *Image) callconv(...) bool`. The userdata is `?*anyopaque`, the data buffer is
// `(ptr, len)`, and `*Image` is the out-image.
pub type DecodePngFn =
    extern "C" fn(*mut std::ffi::c_void, *const CAllocator, *const u8, usize, *mut Image) -> bool;

/// C: GhosttySysRandomSecureFn -> RusttySysRandomSecureFn (R2)
// pub const RandomSecureFn = *const fn (
//     ?*anyopaque,
//     [*]u8,
//     usize,
// ) callconv(lib.calling_conv) bool;
pub type RandomSecureFn = extern "C" fn(*mut std::ffi::c_void, *mut u8, usize) -> bool;

/// C: GhosttySysLogLevel -> RusttySysLogLevel (R2)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Error = 0,
    Warning = 1,
    Info = 2,
    Debug = 3,
}

// D11 (P3-W6 T2): proves taking this value as a `c_int` in the exported
// signature leaves the ABI unchanged.
const _: () =
    assert!(std::mem::size_of::<LogLevel>() == std::mem::size_of::<std::os::raw::c_int>());

impl LogLevel {
    /// Narrow an inbound C integer to a `LogLevel`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    ///
    /// D11 (P3-W6 T2): `rustty_sys_log_stderr` takes the level as a plain
    /// `c_int` and narrows it HERE. A `#[repr(C)]` enum parameter is
    /// materialized in the callee prologue, so an out-of-range tag is UB before
    /// any check of ours could run, and `include/rustty/vt/sys.h` itself
    /// declares `RUSTTY_SYS_LOG_LEVEL_MAX_VALUE`. See `modes.rs:19`.
    ///
    /// PRE-EXISTING: this defect predates the Phase 3 resync range; it is not
    /// attributable to any commit in it.
    pub fn from_c_int(v: std::os::raw::c_int) -> Option<LogLevel> {
        match v {
            0 => Some(LogLevel::Error),
            1 => Some(LogLevel::Warning),
            2 => Some(LogLevel::Info),
            3 => Some(LogLevel::Debug),
            _ => None,
        }
    }

    // pub fn fromStd(level: std.log.Level) LogLevel { ... }
    // PORT NOTE: rustty has no `std.log.Level`; the equivalent is the `StdLevel`
    // shim below mirroring Zig's `{ err, warn, info, debug }`.
    pub fn from_std(level: StdLevel) -> LogLevel {
        match level {
            StdLevel::Err => LogLevel::Error,
            StdLevel::Warn => LogLevel::Warning,
            StdLevel::Info => LogLevel::Info,
            StdLevel::Debug => LogLevel::Debug,
        }
    }
}

/// PORT NOTE: `std.log.Level` analog (`{ err, warn, info, debug }`) used by
/// `LogLevel::from_std` and `log_fn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdLevel {
    Err,
    Warn,
    Info,
    Debug,
}

/// C: GhosttySysLogFn -> RusttySysLogFn (R2)
pub type LogFn = extern "C" fn(*mut std::ffi::c_void, LogLevel, *const u8, usize, *const u8, usize);

/// C: GhosttySysOption -> RusttySysOption (R2)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    Userdata = 0,
    DecodePng = 1,
    Log = 2,
    RandomSecure = 3,
}

// pub fn InType(comptime self: Option) type { ... }
// PORT NOTE: the per-tag input type for `setTyped`:
//   .userdata      => ?*const anyopaque
//   .decode_png    => ?DecodePngFn
//   .log           => ?LogFn
//   .random_secure => ?RandomSecureFn
// Rust has no comptime type-returning fn; each `set` arm casts `value`
// accordingly (preserving the same tag/type correspondence).

/// Global state for the sys interface so we can call through to the C
/// callbacks from Zig.
// PORT NOTE: the reference is `const Global = struct { userdata, decode_png, log }`
// held in a mutable module `var global`. Rust forbids a plain mutable static, so
// the global is wrapped in a `Mutex` (mutated only at startup, exactly the Zig
// contract). The fn-pointer/userdata raw values are `Send`-safe to store here.
struct Global {
    userdata: *mut std::ffi::c_void,
    decode_png: core::option::Option<DecodePngFn>,
    log: core::option::Option<LogFn>,
    random_secure: core::option::Option<RandomSecureFn>,
}

// PORT NOTE: raw `*mut c_void`/`extern "C" fn` are not `Send` by default; the
// global is only mutated at startup and read under the lock, matching the Zig
// `var` contract. The unsafe `Send` is the minimal glue required to hold the
// state in a `Mutex` (no behavior change).
unsafe impl Send for Global {}

/// Global state for the C sys interface.
// var global: Global = .{};
static GLOBAL: Mutex<Global> = Mutex::new(Global {
    userdata: std::ptr::null_mut(),
    decode_png: None,
    log: None,
    random_secure: None,
});

/// PORT NOTE (`590d669c4` seam): views the seam's `RawAllocator` — the
/// `std.mem.Allocator` the caller hands the decoder (the kitty path passes a
/// `LimitedAllocator`) — as a rustty-lib `ZigAllocator` so it can be wrapped
/// into the C allocator handed to the callback. The reference forwards the SAME
/// Zig allocator (`CAllocator.fromZig(&alloc)`); this bridge is that forward,
/// so the callback's allocations are capped exactly like a native decoder's.
struct RawZigAllocator<'a>(&'a dyn terminal_sys::RawAllocator);

impl rustty_lib::allocator::ZigAllocator for RawZigAllocator<'_> {
    fn alloc(&self, len: usize, alignment: rustty_lib::allocator::Alignment, ra: usize) -> *mut u8 {
        self.0
            .raw_alloc(len, terminal_sys::Alignment(alignment.0), ra)
    }

    fn resize(
        &self,
        memory: rustty_lib::allocator::ZigSlice,
        alignment: rustty_lib::allocator::Alignment,
        new_len: usize,
        ra: usize,
    ) -> bool {
        self.0.raw_resize(
            terminal_sys::RawSlice::new(memory.ptr, memory.len),
            terminal_sys::Alignment(alignment.0),
            new_len,
            ra,
        )
    }

    fn remap(
        &self,
        memory: rustty_lib::allocator::ZigSlice,
        alignment: rustty_lib::allocator::Alignment,
        new_len: usize,
        ra: usize,
    ) -> *mut u8 {
        self.0.raw_remap(
            terminal_sys::RawSlice::new(memory.ptr, memory.len),
            terminal_sys::Alignment(alignment.0),
            new_len,
            ra,
        )
    }

    fn free(
        &self,
        memory: rustty_lib::allocator::ZigSlice,
        alignment: rustty_lib::allocator::Alignment,
        ra: usize,
    ) {
        self.0.raw_free(
            terminal_sys::RawSlice::new(memory.ptr, memory.len),
            terminal_sys::Alignment(alignment.0),
            ra,
        )
    }
}

/// Zig-compatible wrapper that calls through to the stored C callback.
/// The C callback allocates the pixel data through the provided allocator,
/// so we can take ownership directly.
///
/// Returns `error.InvalidData` if no callback is installed, if the callback
/// returns false, or if it returns true without setting `data`.
// PORT NOTE: the reference signature is
//   fn decodePngWrapper(alloc: std.mem.Allocator, data: []const u8)
//       terminal_sys.DecodeError!terminal_sys.Image
// rustty's `terminal_sys.DecodePngFn` carries the allocator as the seam's
// `&dyn RawAllocator` (see `terminal_sys::DecodePngFn`); the C callback
// receives a `*const CAllocator` built over exactly that allocator via the
// `RawZigAllocator` bridge above.
fn decode_png_wrapper(
    alloc: &dyn terminal_sys::RawAllocator,
    data: &[u8],
) -> core::result::Result<terminal_sys::Image, terminal_sys::DecodeError> {
    // const func = global.decode_png orelse return error.InvalidData;
    let g = GLOBAL.lock().unwrap();
    let func = match g.decode_png {
        Some(f) => f,
        None => return Err(terminal_sys::DecodeError::InvalidData),
    };
    let userdata = g.userdata;
    drop(g);

    // const c_alloc = CAllocator.fromZig(&alloc);
    let zig_alloc = RawZigAllocator(alloc);
    let zref = rustty_lib::allocator::ZigAllocatorRef::new(&zig_alloc);
    let c_alloc = CAllocator::from_zig(&zref);

    // Hand the callback a zeroed struct instead of an undefined one. The
    // callback stores a pointer into it, and runtimes that track pointers
    // (Go, for example) require the memory to be initialized first. It
    // also means a callback that returns true without setting `data` is
    // caught by the null check below. The header documents this guarantee.
    // var out: Image = std.mem.zeroes(Image);
    // PORT NOTE (DECISIONS M5): Rust has no `undefined`, so the port always
    // built this zeroed literal; `aca9bf031` makes the reference match.
    let mut out = Image {
        width: 0,
        height: 0,
        data: std::ptr::null_mut(),
        data_len: 0,
    };
    // if (!func(global.userdata, &c_alloc, lib.String.init(data).ptr, data.len, &out)) return error.InvalidData;
    if !func(
        userdata,
        &c_alloc,
        rustty_lib::String::init(data).ptr,
        data.len(),
        &mut out,
    ) {
        return Err(terminal_sys::DecodeError::InvalidData);
    }

    // const result_data = out.data orelse return error.InvalidData;
    if out.data.is_null() {
        return Err(terminal_sys::DecodeError::InvalidData);
    }

    // return .{ .width = out.width, .height = out.height, .data = result_data[0..out.data_len] };
    // PORT NOTE (ownership model, parity tail S7-4b): the callback allocated the
    // buffer through `c_alloc`, which is `alloc`; `terminal_sys::Image.data` takes
    // it as is, and the caller frees it through `alloc` (`decodePng`'s
    // `defer decode_alloc.free(result.data)`). VALIDATION_REPORT §3c.4's leak
    // stays closed: `decode_png_wrapper_hands_over_the_callback_buffer` below.
    Ok(terminal_sys::Image {
        width: out.width,
        height: out.height,
        data: terminal_sys::RawSlice::new(out.data, out.data_len),
    })
}

/// Zig-compatible wrapper that calls through to the stored C callback.
// fn randomSecureWrapper(buffer: []u8) terminal_sys.RandomSecureError!void {
//     const func = global.random_secure orelse return error.EntropyUnavailable;
//     if (!func(global.userdata, buffer.ptr, buffer.len)) return error.EntropyUnavailable;
// }
fn random_secure_wrapper(
    buffer: &mut [u8],
) -> core::result::Result<(), terminal_sys::RandomSecureError> {
    let g = GLOBAL.lock().unwrap();
    let func = match g.random_secure {
        Some(f) => f,
        None => return Err(terminal_sys::RandomSecureError::EntropyUnavailable),
    };
    let userdata = g.userdata;
    drop(g);
    if !func(userdata, buffer.as_mut_ptr(), buffer.len()) {
        return Err(terminal_sys::RandomSecureError::EntropyUnavailable);
    }
    Ok(())
}

/// PORT NOTE: `pub fn set(option: Option, value: ?*const anyopaque)
/// callconv(...) Result`. The C calling convention is the `extern "C"` wrapper
/// in `lib.rs` (`rustty_sys_set`).
///
/// # Safety
/// `value` must be valid for the type required by `option` (a fn-pointer for
/// `DecodePng`/`Log`, an arbitrary userdata pointer for `Userdata`), or null.
pub unsafe fn set(option: Option_, value: *const std::ffi::c_void) -> Result {
    // if (comptime std.debug.runtime_safety) {
    //     _ = std.meta.intToEnum(Option, @intFromEnum(option)) catch {
    //         return .invalid_value;
    //     };
    // }
    // PORT NOTE: `option: Option_` is already a valid tag in this typed path; the
    // integer-range guard lives in the `extern "C"` wrapper in `lib.rs`.

    // return switch (option) {
    //     inline else => |comptime_option| setTyped(comptime_option, @ptrCast(@alignCast(value))),
    // };
    // SAFETY: `value` matches `option`'s input type (caller contract).
    unsafe { set_typed(option, value) }
}

// fn setTyped(comptime option: Option, value: option.InType()) Result { ... }
// # Safety: `value` must match `option`'s input type.
unsafe fn set_typed(option: Option_, value: *const std::ffi::c_void) -> Result {
    let mut g = GLOBAL.lock().unwrap();
    match option {
        // .userdata => global.userdata = @constCast(value),
        Option_::Userdata => g.userdata = value as *mut std::ffi::c_void,
        // .decode_png => {
        //     global.decode_png = value;
        //     terminal_sys.decode_png = if (value != null) &decodePngWrapper else null;
        // },
        Option_::DecodePng => {
            // PORT NOTE: `value` is `?DecodePngFn` (a fn-pointer or null).
            g.decode_png = if value.is_null() {
                None
            } else {
                // SAFETY: `value` is a `DecodePngFn` fn-pointer (caller contract).
                Some(unsafe { std::mem::transmute::<*const std::ffi::c_void, DecodePngFn>(value) })
            };
            *terminal_sys::decode_png.write().unwrap() = if value.is_null() {
                None
            } else {
                Some(decode_png_wrapper as terminal_sys::DecodePngFn)
            };
        }
        // .log => global.log = value,
        Option_::Log => {
            g.log = if value.is_null() {
                None
            } else {
                // SAFETY: `value` is a `LogFn` fn-pointer (caller contract).
                Some(unsafe { std::mem::transmute::<*const std::ffi::c_void, LogFn>(value) })
            };
        }
        // .random_secure => {
        //     global.random_secure = value;
        //     terminal_sys.random_secure = if (value != null) &randomSecureWrapper else null;
        // },
        Option_::RandomSecure => {
            g.random_secure = if value.is_null() {
                None
            } else {
                // SAFETY: `value` is a `RandomSecureFn` fn-pointer (caller
                // contract).
                Some(unsafe {
                    std::mem::transmute::<*const std::ffi::c_void, RandomSecureFn>(value)
                })
            };
            *terminal_sys::random_secure.write().unwrap() = if value.is_null() {
                None
            } else {
                Some(random_secure_wrapper as terminal_sys::RandomSecureFn)
            };
        }
    }
    // return .success;
    Result::Success
}

/// Dispatch a log message to the installed C callback, if any.
// fn emitLog(level: LogLevel, scope: []const u8, message: []const u8) void { ... }
//
// PORT NOTE (`d61920d80`): `cfg(not(wasm32))` on the C-ABI emit path mirrors
// Zig's lazy analysis — on wasm `std_options` never installs this sink, so the
// reference never compiles it there (see `log_fn`).
#[cfg(not(target_arch = "wasm32"))]
fn emit_log(level: LogLevel, scope: &[u8], message: &[u8]) {
    // const func = global.log orelse return;
    let g = GLOBAL.lock().unwrap();
    let func = match g.log {
        Some(f) => f,
        None => return,
    };
    let userdata = g.userdata;
    drop(g);
    // func(global.userdata, level, scope.ptr, scope.len, message.ptr, message.len);
    func(
        userdata,
        level,
        scope.as_ptr(),
        scope.len(),
        message.as_ptr(),
        message.len(),
    );
}

// const buffer_size = 2048;
// PORT NOTE: `LogEmitter.buffer_size`, hoisted to module scope because
// `log_stderr` (every target) sizes its line buffer from it while
// `LogEmitter` itself is `cfg(not(wasm32))`.
const LOG_EMITTER_BUFFER_SIZE: usize = 2048;

/// Emits logs in chunks. Almost all logs will be less than the chunk size
/// but this allows emitting larger logs without heap allocation.
// const LogEmitter = struct { c_level, scope_text, buf: [2048]u8, pos: usize, ... }
//
// `cfg(not(wasm32))`: see the `emit_log` port note.
#[cfg(not(target_arch = "wasm32"))]
struct LogEmitter<'a> {
    c_level: LogLevel,
    scope_text: &'a [u8],
    buf: [u8; LOG_EMITTER_BUFFER_SIZE],
    pos: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl LogEmitter<'_> {
    // fn write(self: *@This(), bytes: []const u8) error{}!usize { ... }
    fn write(&mut self, bytes: &[u8]) -> usize {
        // var remaining = bytes;
        let mut remaining = bytes;
        // while (remaining.len > 0) {
        while !remaining.is_empty() {
            // const space = self.buf.len - self.pos;
            let space = self.buf.len() - self.pos;
            // if (space == 0) { self.flush(); continue; }
            if space == 0 {
                self.flush();
                continue;
            }

            // const n = @min(remaining.len, space);
            let n = remaining.len().min(space);
            // @memcpy(self.buf[self.pos..][0..n], remaining[0..n]);
            self.buf[self.pos..self.pos + n].copy_from_slice(&remaining[0..n]);
            // self.pos += n;
            self.pos += n;
            // remaining = remaining[n..];
            remaining = &remaining[n..];
        }

        // return bytes.len;
        bytes.len()
    }

    // fn flush(self: *@This()) void { ... }
    fn flush(&mut self) {
        // if (self.pos == 0) return;
        if self.pos == 0 {
            return;
        }
        // emitLog(self.c_level, self.scope_text, self.buf[0..self.pos]);
        emit_log(self.c_level, self.scope_text, &self.buf[0..self.pos]);
        // self.pos = 0;
        self.pos = 0;
    }
}

// The `std.Io.Writer` the reference builds over the `LogEmitter` buffer
// (`sys.zig`'s `.writer = .{ .buffer = &log_emitter_buffer, .vtable = ... }`).
// `core::fmt::Write` is rustty's writer seam: `core::fmt::write` streams the
// formatted fragments straight into the stack buffer, exactly like Zig's
// `writer.print(format, args)`, so no heap allocation is involved.
#[cfg(not(target_arch = "wasm32"))]
impl core::fmt::Write for LogEmitter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}

/// Custom std.log sink for C ABI builds.
///
/// When a log callback is installed via rustty_sys_set(), messages are
/// dispatched through it. When no callback is installed, messages are
/// silently discarded. Large messages that exceed the stack buffer are
/// delivered across multiple callback invocations.
// pub fn logFn(comptime level, comptime scope, comptime format, args) void { ... }
// PORT NOTE: the reference is Zig's `std.log` sink: it takes the comptime level,
// scope, format string and runtime args, formats them via a `GenericWriter` into
// the `LogEmitter`, then flushes. rustty has no comptime log sink; `log_fn`
// takes the runtime level, the scope name, and the already-formatted message
// (`core::fmt::Arguments`), reproducing the same chunked dispatch.
pub fn log_fn(level: StdLevel, scope: &str, args: core::fmt::Arguments) {
    // PORT NOTE (`d61920d80`, `lib_vt.zig` `std_options`): on wasm the
    // installed `std.log` sink is NOT this C-ABI dispatcher — `lib_vt.zig`
    // selects `os/wasm/log.zig` instead:
    //
    //     if (builtin.target.cpu.arch.isWasm()) {
    //         // In non-debug modes, we want to ship effectively no logging
    //         // warn and lower add ~200KB at the time of this comment.
    //         if (builtin.mode == .Debug) {
    //             options.log_level = .debug;
    //             options.logFn = @import("os/wasm/log.zig").log;
    //         } else {
    //             options.log_level = .err;
    //             options.logFn = @import("os/wasm/log.zig").noop;
    //         }
    //     } else if (terminal.options.c_abi) { ... this sink ... }
    //
    // `log_fn` is rustty's single log funnel (the `std_options.logFn` analog),
    // so the arch selection lives at its head. `builtin.mode == .Debug` ports
    // as `cfg(debug_assertions)` (R8: build-switchable, never folded).
    #[cfg(all(target_arch = "wasm32", debug_assertions))]
    {
        // options.log_level = .debug;
        // options.logFn = @import("os/wasm/log.zig").log;
        //
        // `os/wasm/log.zig`'s `log` takes `comptime level.asText()` plus the
        // scope tag; the rustty port takes the already-formatted parts. The
        // default scope reaches this funnel as "" (see the callers) and maps
        // to the reference's `.default` tag.
        let level_txt = match level {
            StdLevel::Err => "error",
            StdLevel::Warn => "warning",
            StdLevel::Info => "info",
            StdLevel::Debug => "debug",
        };
        let scope_txt = if scope.is_empty() { "default" } else { scope };
        rustty_os::wasm::log::log(level_txt, scope_txt, &std::fmt::format(args));
    }
    #[cfg(all(target_arch = "wasm32", not(debug_assertions)))]
    {
        // options.log_level = .err;
        // options.logFn = @import("os/wasm/log.zig").noop;
        //
        // Discard WITHOUT formatting — the early return is the Rust spelling
        // of installing the comptime `noop` sink (see
        // `rustty_os::wasm::log::noop`), keeping the release wasm module free
        // of the formatting machinery this call would otherwise pull in.
        let _ = (level, scope, args);
    }

    // } else if (terminal.options.c_abi) {
    //     // For C ABI builds, use a custom log function that dispatches to an
    //     // embedder-provided callback (or silently discards when none is set).
    //     options.logFn = @import("terminal/c/sys.zig").logFn;
    // }
    #[cfg(not(target_arch = "wasm32"))]
    log_fn_c_abi(level, scope, args);
}

/// The non-wasm (C ABI) body of [`log_fn`]: dispatch to the embedder-installed
/// callback. This is `terminal/c/sys.zig`'s `logFn` proper — the sink the
/// reference installs for C ABI builds (see the `std_options` note above).
#[cfg(not(target_arch = "wasm32"))]
fn log_fn_c_abi(level: StdLevel, scope: &str, args: core::fmt::Arguments) {
    // if (global.log == null) return;
    if GLOBAL.lock().unwrap().log.is_none() {
        return;
    }

    // const scope_text: []const u8 = if (scope == .default) "" else @tagName(scope);
    // PORT NOTE: the `.default` scope maps to the empty string; any named scope
    // maps to its name. Callers pass "" for the default scope.
    let scope_text: &[u8] = scope.as_bytes();
    // const c_level = LogLevel.fromStd(level);
    let c_level = LogLevel::from_std(level);

    // var ctx: LogEmitter = .{ .c_level = c_level, .scope_text = scope_text };
    let mut ctx = LogEmitter {
        c_level,
        scope_text,
        buf: [0u8; LOG_EMITTER_BUFFER_SIZE],
        pos: 0,
    };
    // const writer: std.io.GenericWriter(...) = .{ .context = &ctx };
    // nosuspend writer.print(format, args) catch {};
    // PORT NOTE: `core::fmt::write` over the `core::fmt::Write` impl above is
    // the direct analogue of `writer.print(format, args)`: the formatted
    // fragments stream into the `LogEmitter`'s stack buffer with no heap
    // allocation. A formatting error is swallowed, mirroring the `catch {}`.
    let _ = core::fmt::write(&mut ctx, args);
    // ctx.flush();
    ctx.flush();
}

// =====================================================================
// The `log` facade backend (task-14a, `.port/parity-closure/DECISION-logging.md`
// Option A, ratified).
//
// PORT NOTE: this has no reference analog to comment against — the oracle's
// `std_options.logFn` is a COMPTIME function-pointer swap (`terminal.options.
// c_abi` picks `terminal/c/sys.zig`'s `logFn` at build time; see `sys::log_fn`'s
// own PORT NOTE), so every `std.log.scoped(...)` call in the whole `libghostty-
// vt` C-ABI artifact — including deep-internal ones in crates below this one in
// R4's acyclic layering (rustty-vt, rustty-font-shaper, rustty-renderer, ...) —
// already reaches this exact sink with no wiring of its own. Rust has no
// comptime `std.log` hook, and those crates cannot depend on `rustty-vt-c`
// (that would invert R4's layering into a cycle), so a runtime facade is the
// substitute: those crates call `log::warn!(target: "<scope>", "<format
// string>", ...)` (the ported call sites, restored verbatim by task 14b-14e),
// and THIS crate — the top of the C-ABI stack, exactly where the oracle installs
// `std_options.logFn` — registers itself as the single process-wide `log::Log`
// backend, forwarding every record into `log_fn` above. This makes `log_fn`'s
// existing wasm/non-wasm arch branching (and the embedder-callback gate in
// `log_fn_c_abi`) the funnel for BOTH this crate's own direct `log_fn` call
// sites and every `log`-macro call site in the crates beneath it — matching the
// oracle's "one global sink for the whole library" shape.
// task-14a review round 2: `install_log_backend` now calls `log::set_logger`
// on EVERY target unconditionally (only `max_level` differs by target/build —
// see that function's own PORT NOTE for why the earlier wasm-release
// early-return, which skipped `set_logger` entirely, was itself an
// unconditional-clobber bug in the opposite direction). So `Logger` is always
// referenced, on every configuration; no `allow(dead_code)` is needed here.
struct Logger;

/// Map a `log::Level` onto the reference's 4-level `std.log.Level`
/// (`StdLevel`). The facade crate has a 5th level, `Trace`, that `std.log`
/// does not; `install_log_backend` never enables it (max level caps at
/// `Debug`), so this arm is unreachable in practice — mapped to `Debug` rather
/// than panicking, since a filter-level mismatch is not a per-message defect.
fn std_level_from_log(level: log::Level) -> StdLevel {
    match level {
        log::Level::Error => StdLevel::Err,
        log::Level::Warn => StdLevel::Warn,
        log::Level::Info => StdLevel::Info,
        log::Level::Debug | log::Level::Trace => StdLevel::Debug,
    }
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // `log::set_max_level` (in `install_log_backend`) already gates which
        // records reach here at all (including the wasm-release `Off` cap that
        // mirrors the oracle's `os/wasm/log.zig` `noop` sink); this sink itself
        // has no independent filter, matching `log_fn`'s own unconditional
        // dispatch once a record arrives.
        true
    }

    fn log(&self, record: &log::Record) {
        // `record.target()` is the facade's carrier for the oracle's
        // `std.log.scoped(.foo)` tag: restored call sites pass
        // `target: "<scope>"` explicitly (`""` for the reference's `.default`
        // scope, matching `log_fn`'s existing "callers pass \"\" for the
        // default scope" convention above) rather than relying on the crate's
        // module path, which has no oracle analog.
        log_fn(
            std_level_from_log(record.level()),
            record.target(),
            *record.args(),
        );
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

/// Register [`Logger`] as the process-wide `log` backend and set the max
/// level. `std::sync::Once`-guarded: every call after the first (from any of
/// the crate's `*_new` entry points, all of which call this) is a genuine
/// no-op — it neither re-installs nor re-touches the max-level filter, so a
/// host that has since changed its own filter is never overridden by a later
/// `rustty_terminal_new`.
///
/// PORT NOTE: the wasm-release level mirrors `lib_vt.zig`'s own size guard
/// (see `log_fn`'s PORT NOTE) — `options.log_level = .err` there still passes
/// `.err`-and-above to the (no-op) sink; capping to `Off` here goes one step
/// further and matches what actually matters (`logFn = ... .noop`): no record
/// is ever dispatched to the sink on a non-debug wasm build.
///
/// This cap is a RUNTIME filter and nothing more. `log::set_max_level` writes
/// an `AtomicUsize` the compiler cannot fold, so it does NOT keep the restored
/// message-formatting code out of the artifact — an earlier version of this
/// comment claimed it did, and that claim was false. The compile-time gate
/// that actually erases those strings is `log`'s `release_max_level_off`
/// Cargo feature, declared in the `wasm32`-target-gated dependency tables of
/// this crate and of `rustty-wasm` (each wasm artifact root needs its own).
/// See `.port/parity-closure/DECISION-logging.md` §8 for the derivation and
/// the measured byte deltas. The runtime cap stays as defense-in-depth for
/// the debug-wasm build, where the compile-time gate does not apply.
/// Task-14a review round 2: this used to be an early return BEFORE `set_logger` was
/// even attempted (skipping registration entirely on wasm-release) — that was
/// itself an unconditional clobber, just inverted (silencing a wasm host's own
/// filter instead of raising it). `set_logger` is now attempted on every
/// target and `set_max_level` — `Off` on this one, one of the other two
/// levels elsewhere — runs only in the shared success arm below.
///
/// Non-wasm level (IMPORTANT 4, task-14a review): `lib_vt.zig`'s `std_options`
/// does NOT set `log_level` for the `terminal.options.c_abi` arm, so Zig's
/// build-mode default applies: `std.log.default_level` is `.debug` in Debug
/// and `.info` in ReleaseSafe, ReleaseFast and ReleaseSmall (Zig 0.16
/// `lib/std/log.zig:54-57`, the reference's `minimum_zig_version`).
/// `cfg!(debug_assertions) ? Debug : Info` is the direct port of that
/// default, not an independent choice: a release embedder receives the
/// info/warn/err records the reference delivers, and no debug records.
///
/// CORRECTION (task C-1, NEW-FINDINGS N390): this arm was `Error`, on the
/// claim that Zig's default resolves to `.err` in ReleaseFast. It does not
/// (the same holds in Zig 0.15), so a release embedder lost every warn and
/// info record the reference delivers.
///
/// CRITICAL 1 fix (task-14a review): `log::set_max_level` used to run
/// unconditionally, even when `log::set_logger` returned `Err` (an embedder
/// or an earlier call already installed a logger) — clobbering that host's
/// own filter back up to this crate's level on every `rustty_terminal_new`.
/// It now runs ONLY inside the success arm. If `set_logger` fails, this
/// function leaves the existing global logger AND its max-level filter
/// completely alone — the correct behavior when a host/embedder already
/// owns the `log` crate's global registration is containment, not clobber.
pub fn install_log_backend() {
    // This process is the lib artifact (libghostty-vt's analog): rustty-vt
    // compiled it with `terminal_options.artifact = .lib` in the reference
    // (`GhosttyZig.zig:67-72`). The port records that at run time, here,
    // because this is the first thing every `*_new` entry point does, and
    // every route to a mode lookup goes through a `*_new` (parity S6-F2,
    // Decision A; see `rustty_vt::options::runtime_artifact`). The only
    // caller of the setter; gate `check_runtime_artifact_setter.py`.
    rustty_vt::options::set_runtime_artifact_lib();
    // `lib_vt.zig`'s `pub const panic` (the `tinyPanicImpl` arm in release
    // builds): a Rust panic hook can only be installed at run time, and this is
    // the first thing every `*_new` entry point does (parity S43-F6). It is
    // installed together with the artifact above; see `panic.rs`.
    crate::panic::install();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // task-14a review round 2 (same class as CRITICAL 1): the wasm-release
        // arm used to call `set_max_level(Off)` UNCONDITIONALLY, with no
        // `set_logger` guard — the inverse of CRITICAL 1's bug (silencing
        // rather than raising a host's own filter). `set_logger` is now
        // attempted on every target, and `set_max_level` — whichever level is
        // appropriate for the target/build — runs ONLY in the success arm, so
        // a wasm host that already owns the `log` registration keeps its own
        // filter untouched here too.
        if log::set_logger(&LOGGER).is_ok() {
            let level = if cfg!(all(target_arch = "wasm32", not(debug_assertions))) {
                log::LevelFilter::Off
            } else if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            };
            log::set_max_level(level);
        }
        // else: a logger (an embedder's own, or another instance of this
        // one) is already installed. Leave its max_level filter as-is —
        // touching it here would be exactly the embedder-visible
        // regression this fix addresses.
    });
}

/// Built-in log callback that writes to stderr.
///
/// Formats each message as "[level](scope): message\n". Can be passed
/// directly to rustty_sys_set(RUSTTY_SYS_OPT_LOG, &rustty_sys_log_stderr).
///
/// Each log line is emitted with a single raw write to stderr, which keeps
/// concurrent log lines from interleaving. We intentionally avoid
/// `std.debug.lockStderr` because it routes through `std.Options.debug_io`
/// and would keep the entire `std.Io.Threaded` vtable alive in the binary
/// (see `os/stderr.zig`). On freestanding/wasm targets this is a no-op
/// (no stderr available).
// PORT NOTE: the raw write is `rustty_os::stderr::write`, the port of
// `os/stderr.zig`, so the port also bypasses `std::io::stderr()`'s lock.
// pub fn logStderr(_, level, scope_ptr, scope_len, message_ptr, message_len)
//     callconv(...) void { ... }
/// PORT NOTE: the C calling convention is the `extern "C"` wrapper in `lib.rs`
/// (`rustty_sys_log_stderr`).
///
/// # Safety
/// `scope_ptr`/`message_ptr` must point to `scope_len`/`message_len` valid bytes.
pub unsafe fn log_stderr(
    _userdata: *mut std::ffi::c_void,
    level: LogLevel,
    scope_ptr: *const u8,
    scope_len: usize,
    message_ptr: *const u8,
    message_len: usize,
) {
    // if (comptime builtin.target.cpu.arch.isWasm()) return;
    if cfg!(target_arch = "wasm32") {
        return;
    }

    // const scope = scope_ptr[0..scope_len];
    // SAFETY: `scope_ptr`/`message_ptr` describe valid byte regions (caller).
    let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
    // const message = message_ptr[0..message_len];
    let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };

    // const level_text = switch (level) { ... };
    let level_text = match level {
        LogLevel::Error => "error",
        LogLevel::Warning => "warning",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
    };

    log_stderr_to(
        &mut |bytes: &[u8]| rustty_os::stderr::write(bytes),
        level_text,
        scope,
        message,
    );
}

/// The body of [`log_stderr`] after the level is named, over a `write` sink
/// that is `stderr.write` (`rustty_os::stderr::write`) in production and a
/// recorder in the tests.
// CORRECTION (parity S178-F1, S178-F2): this used to hold
// `std::io::stderr().lock()` and `writeln!` the line with the scope and
// message passed through `String::from_utf8_lossy`. That replaced invalid
// UTF-8 with U+FFFD where the reference writes the bytes as they are, and
// `writeln!` issues one write per fragment, so the single-write guarantee in
// the doc comment was not backed. The reference's two paths are ported below.
fn log_stderr_to(write: &mut dyn FnMut(&[u8]), level_text: &str, scope: &[u8], message: &[u8]) {
    // Large enough for a full logFn chunk plus the level/scope prefix.
    // var buffer: [LogEmitter.buffer_size + 128]u8 = undefined;
    let mut buffer = [0u8; LOG_EMITTER_BUFFER_SIZE + 128];
    // const line: ?[]const u8 = if (scope.len > 0)
    //     std.fmt.bufPrint(&buffer, "[{s}]({s}): {s}\n", .{ level_text, scope, message }) catch null
    // else
    //     std.fmt.bufPrint(&buffer, "[{s}]: {s}\n", .{ level_text, message }) catch null;
    let line: Option<&[u8]> = if !scope.is_empty() {
        buf_print(
            &mut buffer,
            &[
                b"[",
                level_text.as_bytes(),
                b"](",
                scope,
                b"): ",
                message,
                b"\n",
            ],
        )
    } else {
        buf_print(
            &mut buffer,
            &[b"[", level_text.as_bytes(), b"]: ", message, b"\n"],
        )
    };
    // if (line) |v| { stderr.write(v); return; }
    if let Some(v) = line {
        write(v);
        return;
    }

    // The line didn't fit in our buffer (an embedder called us directly
    // with a very large message). Fall back to writing the pieces
    // separately; interleaving with other threads is possible here but
    // this is a best-effort diagnostic path.
    write(b"[");
    write(level_text.as_bytes());
    write(b"]");
    if !scope.is_empty() {
        write(b"(");
        write(scope);
        write(b")");
    }
    write(b": ");
    write(message);
    write(b"\n");
}

/// `std.fmt.bufPrint` for a format made only of `{s}` byte-slice arguments:
/// the concatenated `parts` in `buf`, or `None` (`error.NoSpaceLeft`) when
/// they do not fit.
fn buf_print<'b>(buf: &'b mut [u8], parts: &[&[u8]]) -> Option<&'b [u8]> {
    let mut pos = 0usize;
    for part in parts {
        let end = pos.checked_add(part.len())?;
        buf.get_mut(pos..end)?.copy_from_slice(part);
        pos = end;
    }
    Some(&buf[..pos])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // PORT NOTE: the reference tests share the module `var global` and Zig runs
    // tests serially. Rust runs tests in parallel, so every sys test that touches
    // the shared GLOBAL state acquires this lock first to serialize them (the
    // moral equivalent of Zig's serial execution). Poison is recovered because a
    // panicking test holds it only transiently.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
        match TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    // Reset GLOBAL + the rustty-vt decode_png slot to a known empty state. Holds
    // and returns the serialization guard so the whole test body runs without
    // another sys test mutating GLOBAL concurrently (mirroring Zig's serial run).
    // `pub(crate)`: other modules' tests that install a log callback (e.g. the
    // render-state scoped-log tests) must serialize against these, since GLOBAL
    // is process-wide.
    pub(crate) fn reset_global() -> std::sync::MutexGuard<'static, ()> {
        let guard = lock_tests();
        GLOBAL.clear_poison();
        let mut g = GLOBAL.lock().unwrap();
        g.userdata = std::ptr::null_mut();
        g.decode_png = None;
        g.log = None;
        drop(g);
        *terminal_sys::decode_png.write().unwrap() = None;
        guard
    }

    // Parity S178-F1 / S178-F2 (no reference test): `logStderr` writes the
    // scope and message bytes as they are, and a line that fits the
    // `LogEmitter.buffer_size + 128` buffer goes out in ONE write.
    #[test]
    fn log_stderr_writes_raw_bytes_in_one_write() {
        let mut writes: Vec<Vec<u8>> = Vec::new();
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "warning",
            b"sc\xffpe",
            b"m\xfe\x80sg",
        );
        assert_eq!(writes, vec![b"[warning](sc\xffpe): m\xfe\x80sg\n".to_vec()]);

        writes.clear();
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "info",
            b"",
            b"\xc3",
        );
        assert_eq!(writes, vec![b"[info]: \xc3\n".to_vec()]);
    }

    /// The length of the message that makes `[warning](s): <message>\n`
    /// exactly fill the `LogEmitter.buffer_size + 128` line buffer.
    const EXACT_FIT_MESSAGE_LEN: usize =
        LOG_EMITTER_BUFFER_SIZE + 128 - b"[warning](s): ".len() - b"\n".len();

    // Parity S178-F2: the single-write/piecewise boundary is the buffer size
    // `LogEmitter.buffer_size + 128` exactly: a line of that many bytes is one
    // write, one byte more is the piecewise fallback.
    #[test]
    fn log_stderr_single_write_boundary_is_the_buffer_size() {
        let mut writes: Vec<Vec<u8>> = Vec::new();
        let message = vec![b'y'; EXACT_FIT_MESSAGE_LEN];
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "warning",
            b"s",
            &message,
        );
        assert_eq!(1, writes.len());
        assert_eq!(LOG_EMITTER_BUFFER_SIZE + 128, writes[0].len());

        writes.clear();
        let message = vec![b'y'; EXACT_FIT_MESSAGE_LEN + 1];
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "warning",
            b"s",
            &message,
        );
        assert_eq!(9, writes.len(), "one byte over the buffer is piecewise");
        assert_eq!(message, writes[7]);
    }

    // Parity S178-F2, the production sink: `log_stderr` hands each line to
    // `stderr.write` (`rustty_os::stderr::write`, one `write(2)` on fd 2).
    // The child points fd 2 at a Unix datagram socket, where every `write(2)`
    // is one datagram, so it can count the writes; a parent/child split keeps
    // the redirection away from the other tests' stderr.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
    #[cfg_attr(miri, ignore)] // spawns a child process and redirects fd 2
    fn log_stderr_production_sink_writes_each_fitting_line_once_to_fd_2() {
        const CHILD: &str = "RUSTTY_VTC_LOG_STDERR_SINK_CHILD";
        if std::env::var_os(CHILD).is_some() {
            log_stderr_sink_child();
            return;
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sys::tests::log_stderr_production_sink_writes_each_fitting_line_once_to_fd_2",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("LOG_STDERR_SINK_CHILD_OK"),
            "child failed: {:?}\nstdout: {stdout}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
    fn log_stderr_sink_child() {
        use std::ffi::{c_int, c_void};
        use std::os::unix::io::AsRawFd;
        use std::os::unix::net::UnixDatagram;
        extern "C" {
            fn dup(fd: c_int) -> c_int;
            fn dup2(fd: c_int, fd2: c_int) -> c_int;
            fn close(fd: c_int) -> c_int;
            fn setsockopt(
                socket: c_int,
                level: c_int,
                name: c_int,
                value: *const c_void,
                len: u32,
            ) -> c_int;
        }
        #[cfg(target_os = "macos")]
        const SOL_SOCKET: c_int = 0xffff;
        #[cfg(target_os = "macos")]
        const SO_SNDBUF: c_int = 0x1001;
        #[cfg(target_os = "macos")]
        const SO_RCVBUF: c_int = 0x1002;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const SOL_SOCKET: c_int = 1;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const SO_SNDBUF: c_int = 7;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const SO_RCVBUF: c_int = 8;

        let (tx, rx) = UnixDatagram::pair().unwrap();
        let size: c_int = 64 * 1024;
        for (sock, opt) in [(tx.as_raw_fd(), SO_SNDBUF), (rx.as_raw_fd(), SO_RCVBUF)] {
            // SAFETY: a valid socket and a live `c_int` option value.
            let rc = unsafe {
                setsockopt(
                    sock,
                    SOL_SOCKET,
                    opt,
                    &size as *const c_int as *const c_void,
                    std::mem::size_of::<c_int>() as u32,
                )
            };
            assert_eq!(0, rc);
        }

        let log = |message: &[u8]| {
            // SAFETY: fd 2 is saved, pointed at the socket for this one call,
            // and restored; the byte slices are live.
            unsafe {
                let saved = dup(2);
                assert!(saved >= 0);
                assert_eq!(2, dup2(tx.as_raw_fd(), 2));
                log_stderr(
                    std::ptr::null_mut(),
                    LogLevel::Warning,
                    b"s".as_ptr(),
                    1,
                    message.as_ptr(),
                    message.len(),
                );
                assert_eq!(2, dup2(saved, 2));
                close(saved);
            }
            rx.set_nonblocking(true).unwrap();
            let mut grams: Vec<Vec<u8>> = Vec::new();
            let mut buf = vec![0u8; 64 * 1024];
            while let Ok(n) = rx.recv(&mut buf) {
                grams.push(buf[..n].to_vec());
            }
            grams
        };

        let message = vec![b'y'; EXACT_FIT_MESSAGE_LEN];
        let mut line = b"[warning](s): ".to_vec();
        line.extend_from_slice(&message);
        line.push(b'\n');
        assert_eq!(vec![line], log(&message), "a fitting line is one write(2)");

        let message = vec![b'y'; EXACT_FIT_MESSAGE_LEN + 1];
        let grams = log(&message);
        assert_eq!(
            9,
            grams.len(),
            "an oversized line is the piecewise fallback"
        );
        assert_eq!(message, grams[7]);

        println!("LOG_STDERR_SINK_CHILD_OK");
    }

    // The reference's fallback: a line that does not fit is written piecewise.
    #[test]
    fn log_stderr_writes_an_oversized_line_in_pieces() {
        let message = vec![b'x'; LOG_EMITTER_BUFFER_SIZE + 128];
        let mut writes: Vec<Vec<u8>> = Vec::new();
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "error",
            b"s",
            &message,
        );
        let expected: Vec<&[u8]> = vec![
            b"[", b"error", b"]", b"(", b"s", b")", b": ", &message, b"\n",
        ];
        assert_eq!(
            writes,
            expected.iter().map(|b| b.to_vec()).collect::<Vec<_>>()
        );

        writes.clear();
        log_stderr_to(
            &mut |b: &[u8]| writes.push(b.to_vec()),
            "debug",
            b"",
            &message,
        );
        let expected: Vec<&[u8]> = vec![b"[", b"debug", b"]", b": ", &message, b"\n"];
        assert_eq!(
            writes,
            expected.iter().map(|b| b.to_vec()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn set_decode_png_with_null_clears() {
        // Start from a known state.
        // global.decode_png = null;
        // terminal_sys.decode_png = null;
        let _guard = reset_global();

        // try std.testing.expectEqual(Result.success, set(.decode_png, null));
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, std::ptr::null())
        });
        // try std.testing.expect(terminal_sys.decode_png == null);
        assert!(terminal_sys::decode_png.read().unwrap().is_none());
    }

    #[test]
    fn set_decode_png_installs_wrapper() {
        let _guard = reset_global();
        // const S = struct { fn decode(...) bool { out.* = .{...}; return true; } };
        extern "C" fn decode(
            _u: *mut std::ffi::c_void,
            _a: *const CAllocator,
            _d: *const u8,
            _l: usize,
            out: *mut Image,
        ) -> bool {
            unsafe {
                *out = Image {
                    width: 1,
                    height: 1,
                    data: std::ptr::null_mut(),
                    data_len: 0,
                };
            }
            true
        }

        // try std.testing.expectEqual(Result.success, set(.decode_png, @ptrCast(&S.decode)));
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, decode as *const std::ffi::c_void)
        });
        // try std.testing.expect(terminal_sys.decode_png != null);
        assert!(terminal_sys::decode_png.read().unwrap().is_some());

        // Clear it again.
        // try std.testing.expectEqual(Result.success, set(.decode_png, null));
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, std::ptr::null())
        });
        // try std.testing.expect(terminal_sys.decode_png == null);
        assert!(terminal_sys::decode_png.read().unwrap().is_none());
    }

    // test "decode_png output is zeroed before the callback" {
    #[test]
    fn decode_png_output_is_zeroed_before_the_callback() {
        let _guard = reset_global();
        // const S = struct {
        //     var seen: ?Image = null;
        //     fn decode(_: ?*anyopaque, _: *const CAllocator, _: [*]const u8, _: usize, out: *Image) callconv(lib.calling_conv) bool {
        //         // Record what we were given, then report success without
        //         // filling anything in.
        //         seen = out.*;
        //         return true;
        //     }
        // };
        static SEEN: std::sync::Mutex<Option<(u32, u32, usize, usize)>> =
            std::sync::Mutex::new(None);
        extern "C" fn decode(
            _u: *mut std::ffi::c_void,
            _a: *const CAllocator,
            _d: *const u8,
            _l: usize,
            out: *mut Image,
        ) -> bool {
            // Record what we were given, then report success without
            // filling anything in.
            // SAFETY: `out` is the wrapper's live `Image`.
            let seen = unsafe { *out };
            *SEEN.lock().unwrap() =
                Some((seen.width, seen.height, seen.data as usize, seen.data_len));
            true
        }
        *SEEN.lock().unwrap() = None;

        // try std.testing.expectEqual(Result.success, set(.decode_png, @ptrCast(&S.decode)));
        // defer _ = set(.decode_png, null);
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, decode as *const std::ffi::c_void)
        });

        // Success with no data is rejected rather than read as garbage.
        // try std.testing.expectError(error.InvalidData, decodePngWrapper(std.testing.allocator, "png"));
        assert!(matches!(
            decode_png_wrapper(&terminal_sys::GlobalRawAllocator, b"png"),
            Err(terminal_sys::DecodeError::InvalidData)
        ));

        // The callback saw an all-zero struct.
        // try std.testing.expectEqual(std.mem.zeroes(Image), S.seen.?);
        assert_eq!(Some((0, 0, 0, 0)), *SEEN.lock().unwrap());

        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, std::ptr::null())
        });
    }

    #[test]
    fn set_log_with_null_clears() {
        // global.log = null;
        let _guard = reset_global();

        // try std.testing.expectEqual(Result.success, set(.log, null));
        assert_eq!(Result::Success, unsafe {
            set(Option_::Log, std::ptr::null())
        });
        // try std.testing.expect(global.log == null);
        assert!(GLOBAL.lock().unwrap().log.is_none());
    }

    // PORT NOTE (R14): `emit_log` is `cfg(not(wasm32))` (see its note), so the
    // test that drives it compiles only there; this keeps `cargo check --tests
    // --target wasm32-unknown-unknown` building (P7-W5 T2 fix round 2).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn set_log_installs_callback() {
        let _guard = reset_global();
        // const S = struct { var called: bool = false; fn logCb(...) void { called = true; } };
        static CALLED: Mutex<bool> = Mutex::new(false);
        extern "C" fn log_cb(
            _u: *mut std::ffi::c_void,
            _lvl: LogLevel,
            _sp: *const u8,
            _sl: usize,
            _mp: *const u8,
            _ml: usize,
        ) {
            *CALLED.lock().unwrap() = true;
        }
        *CALLED.lock().unwrap() = false;

        // try std.testing.expectEqual(Result.success, set(.log, @ptrCast(&S.logCb)));
        assert_eq!(Result::Success, unsafe {
            set(Option_::Log, log_cb as *const std::ffi::c_void)
        });
        // try std.testing.expect(global.log != null);
        assert!(GLOBAL.lock().unwrap().log.is_some());

        // emitLog(.info, "test", "hello");
        emit_log(LogLevel::Info, b"test", b"hello");
        // try std.testing.expect(S.called);
        assert!(*CALLED.lock().unwrap());

        // Clear it again.
        // S.called = false;
        *CALLED.lock().unwrap() = false;
        // try std.testing.expectEqual(Result.success, set(.log, null));
        assert_eq!(Result::Success, unsafe {
            set(Option_::Log, std::ptr::null())
        });
        // try std.testing.expect(global.log == null);
        assert!(GLOBAL.lock().unwrap().log.is_none());

        // emitLog(.info, "test", "should not arrive");
        emit_log(LogLevel::Info, b"test", b"should not arrive");
        // try std.testing.expect(!S.called);
        assert!(!*CALLED.lock().unwrap());
    }

    // PORT NOTE: the four logFn tests use a callback that accumulates the call
    // count + total message length into module-static counters. Rust closures
    // can't be `extern "C"`, so we use a `Mutex`-guarded static pair, matching
    // the Zig `var call_count`/`var total_len`. The shared GLOBAL `TEST_LOCK`
    // (held via `reset_global`) serializes them so the statics don't race.
    //
    // The reference nests a fresh `S` struct with its own `logCb` in EVERY
    // logFn test; the three chunked tests' `logCb` additionally carries a
    // per-chunk `std.debug.assert(msg_len <= 2048)`. Each Rust test mirrors
    // that by passing its own callback (the chunked ones a locally-nested
    // `extern "C" fn` with the per-chunk debug_assert, assertion-for-assertion
    // with the reference's block).
    static CALL_COUNT: Mutex<usize> = Mutex::new(0);
    static TOTAL_LEN: Mutex<usize> = Mutex::new(0);

    // fn logCb(...) { call_count += 1; total_len += msg_len; }
    // (the small-message test's S.logCb — no per-chunk size assert)
    extern "C" fn counting_log_cb(
        _u: *mut std::ffi::c_void,
        _lvl: LogLevel,
        _sp: *const u8,
        _sl: usize,
        _mp: *const u8,
        msg_len: usize,
    ) {
        *CALL_COUNT.lock().unwrap() += 1;
        *TOTAL_LEN.lock().unwrap() += msg_len;
    }

    fn install_counting_log(cb: LogFn) {
        // Recover any poison left by a previously panicking assertion (the
        // TEST_LOCK serializes us, so this is a clean reset point).
        CALL_COUNT.clear_poison();
        TOTAL_LEN.clear_poison();
        *CALL_COUNT.lock().unwrap() = 0;
        *TOTAL_LEN.lock().unwrap() = 0;
        GLOBAL.lock().unwrap().log = Some(cb);
    }

    #[test]
    fn logfn_small_message_single_chunk() {
        let _guard = reset_global();
        install_counting_log(counting_log_cb);

        // logFn(.info, .default, "hello", .{});
        log_fn(StdLevel::Info, "", format_args!("hello"));

        // try std.testing.expectEqual(@as(usize, 1), S.call_count);
        assert_eq!(1usize, *CALL_COUNT.lock().unwrap());
        // try std.testing.expectEqual(@as(usize, 5), S.total_len);
        assert_eq!(5usize, *TOTAL_LEN.lock().unwrap());

        GLOBAL.lock().unwrap().log = None;
    }

    #[test]
    fn logfn_message_exceeding_chunk_size_is_split() {
        let _guard = reset_global();
        // The reference's per-test S.logCb (nested in the test block), whose
        // per-chunk size check is part of this test's assertion set:
        //   fn logCb(...) { call_count += 1; total_len += msg_len;
        //       // Each chunk must not exceed the buffer size.
        //       std.debug.assert(msg_len <= 2048); }
        extern "C" fn chunked_log_cb(
            _u: *mut std::ffi::c_void,
            _lvl: LogLevel,
            _sp: *const u8,
            _sl: usize,
            _mp: *const u8,
            msg_len: usize,
        ) {
            *CALL_COUNT.lock().unwrap() += 1;
            *TOTAL_LEN.lock().unwrap() += msg_len;
            // Each chunk must not exceed the buffer size.
            debug_assert!(msg_len <= 2048);
        }
        install_counting_log(chunked_log_cb);

        // Format a message larger than the 2048-byte buffer.
        // 'A' repeated 3000 times via a fill format.
        // const fill: [3000]u8 = .{0x41} ** 3000;
        let fill = [0x41u8; 3000];
        // logFn(.info, .default, "{s}", .{@as([]const u8, &fill)});
        log_fn(
            StdLevel::Info,
            "",
            format_args!("{}", core::str::from_utf8(&fill).unwrap()),
        );

        // try std.testing.expect(S.call_count >= 2);
        assert!(*CALL_COUNT.lock().unwrap() >= 2);
        // try std.testing.expectEqual(@as(usize, 3000), S.total_len);
        assert_eq!(3000usize, *TOTAL_LEN.lock().unwrap());

        GLOBAL.lock().unwrap().log = None;
    }

    #[test]
    fn logfn_message_exactly_at_chunk_boundary() {
        let _guard = reset_global();
        // The reference's per-test S.logCb (nested in the test block), whose
        // per-chunk size check is part of this test's assertion set:
        //   fn logCb(...) { call_count += 1; total_len += msg_len;
        //       // Each chunk must not exceed the buffer size.
        //       std.debug.assert(msg_len <= 2048); }
        extern "C" fn chunked_log_cb(
            _u: *mut std::ffi::c_void,
            _lvl: LogLevel,
            _sp: *const u8,
            _sl: usize,
            _mp: *const u8,
            msg_len: usize,
        ) {
            *CALL_COUNT.lock().unwrap() += 1;
            *TOTAL_LEN.lock().unwrap() += msg_len;
            // Each chunk must not exceed the buffer size.
            debug_assert!(msg_len <= 2048);
        }
        install_counting_log(chunked_log_cb);

        // Exactly 2048 bytes — should emit one full chunk, no remainder.
        // const fill: [2048]u8 = .{0x42} ** 2048;
        let fill = [0x42u8; 2048];
        log_fn(
            StdLevel::Info,
            "",
            format_args!("{}", core::str::from_utf8(&fill).unwrap()),
        );

        // try std.testing.expectEqual(@as(usize, 1), S.call_count);
        assert_eq!(1usize, *CALL_COUNT.lock().unwrap());
        // try std.testing.expectEqual(@as(usize, 2048), S.total_len);
        assert_eq!(2048usize, *TOTAL_LEN.lock().unwrap());

        GLOBAL.lock().unwrap().log = None;
    }

    #[test]
    fn logfn_message_exactly_double_chunk_size() {
        let _guard = reset_global();
        // The reference's per-test S.logCb (nested in the test block), whose
        // per-chunk size check is part of this test's assertion set:
        //   fn logCb(...) { call_count += 1; total_len += msg_len;
        //       // Each chunk must not exceed the buffer size.
        //       std.debug.assert(msg_len <= 2048); }
        extern "C" fn chunked_log_cb(
            _u: *mut std::ffi::c_void,
            _lvl: LogLevel,
            _sp: *const u8,
            _sl: usize,
            _mp: *const u8,
            msg_len: usize,
        ) {
            *CALL_COUNT.lock().unwrap() += 1;
            *TOTAL_LEN.lock().unwrap() += msg_len;
            // Each chunk must not exceed the buffer size.
            debug_assert!(msg_len <= 2048);
        }
        install_counting_log(chunked_log_cb);

        // Exactly 4096 bytes — should emit exactly two full chunks.
        // const fill: [4096]u8 = .{0x43} ** 4096;
        let fill = [0x43u8; 4096];
        log_fn(
            StdLevel::Info,
            "",
            format_args!("{}", core::str::from_utf8(&fill).unwrap()),
        );

        // try std.testing.expectEqual(@as(usize, 2), S.call_count);
        assert_eq!(2usize, *CALL_COUNT.lock().unwrap());
        // try std.testing.expectEqual(@as(usize, 4096), S.total_len);
        assert_eq!(4096usize, *TOTAL_LEN.lock().unwrap());

        GLOBAL.lock().unwrap().log = None;
    }

    #[test]
    fn set_userdata() {
        let _guard = reset_global();
        // var data: u32 = 42;
        let mut data: u32 = 42;
        // try std.testing.expectEqual(Result.success, set(.userdata, @ptrCast(&data)));
        assert_eq!(Result::Success, unsafe {
            set(
                Option_::Userdata,
                &mut data as *mut u32 as *const std::ffi::c_void,
            )
        });
        // try std.testing.expect(global.userdata == @as(?*anyopaque, @ptrCast(&data)));
        assert_eq!(
            GLOBAL.lock().unwrap().userdata,
            &mut data as *mut u32 as *mut std::ffi::c_void
        );

        // Clear it.
        // try std.testing.expectEqual(Result.success, set(.userdata, null));
        assert_eq!(Result::Success, unsafe {
            set(Option_::Userdata, std::ptr::null())
        });
        // try std.testing.expect(global.userdata == null);
        assert!(GLOBAL.lock().unwrap().userdata.is_null());
    }

    // task-14a (rustty-only proving test, NOT a reference test — the `log`
    // facade backend has no oracle analog; see `install_log_backend`'s PORT
    // NOTE): proves the whole chain end to end — `log::warn!` (the mechanical
    // shape 14b-14e's restored call sites will use) reaches an embedder's
    // installed C callback via `install_log_backend`'s `Logger` and the
    // existing `log_fn` sink, with no direct call to `sys::log_fn` at the call
    // site. This is the infrastructure task 14a establishes for stages that
    // restore sites in crates below this one (rustty-vt, rustty-font-shaper,
    // ...), which cannot call `sys::log_fn` directly (R4 layering).
    #[test]
    fn log_facade_backend_reaches_the_c_sink() {
        let _guard = reset_global();
        install_log_backend();

        static CALLED: Mutex<Vec<(LogLevel, String, String)>> = Mutex::new(Vec::new());
        extern "C" fn cb(
            _u: *mut std::ffi::c_void,
            level: LogLevel,
            scope_ptr: *const u8,
            scope_len: usize,
            msg_ptr: *const u8,
            msg_len: usize,
        ) {
            // SAFETY: the sink hands us the scope/message byte ranges.
            let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
            // SAFETY: as above.
            let msg = unsafe { std::slice::from_raw_parts(msg_ptr, msg_len) };
            CALLED.lock().unwrap().push((
                level,
                String::from_utf8_lossy(scope).into_owned(),
                String::from_utf8_lossy(msg).into_owned(),
            ));
        }
        CALLED.lock().unwrap().clear();

        assert_eq!(Result::Success, unsafe {
            set(Option_::Log, cb as *const std::ffi::c_void)
        });

        // The mechanical shape a restored `log.warn("...", .{...})` site takes
        // under Option A: `log::warn!(target: "<scope>", "<format>", ...)`.
        log::warn!(target: "facade_test_scope", "facade proving message {}", 42);
        // The level filter is Zig's `std.log.default_level` (`install_log_backend`):
        // `.info` in every release mode, `.debug` in Debug. So `info` reaches the
        // sink on both profiles and `debug` only in a debug build.
        log::info!(target: "facade_test_scope", "facade info {}", 1);
        log::debug!(target: "facade_test_scope", "facade debug {}", 2);

        GLOBAL.lock().unwrap().log = None;

        let captured = CALLED.lock().unwrap().clone();
        let mut expected = vec![
            (
                LogLevel::Warning,
                "facade_test_scope".to_string(),
                "facade proving message 42".to_string(),
            ),
            (
                LogLevel::Info,
                "facade_test_scope".to_string(),
                "facade info 1".to_string(),
            ),
        ];
        if cfg!(debug_assertions) {
            expected.push((
                LogLevel::Debug,
                "facade_test_scope".to_string(),
                "facade debug 2".to_string(),
            ));
        }
        assert_eq!(
            captured, expected,
            "the facade backend did not deliver the reference's levels to the C-ABI log sink"
        );
    }

    // PORT NOTE (rustty-only leak regression, NOT a reference test — VALIDATION_REPORT
    // §3c.4): the reference `decodePngWrapper` hands the callback-allocated buffer to
    // `terminal_sys.Image.data` ([]u8, ownership transfers; the kitty image later frees
    // it as `alloc.free(data)`). rustty's `terminal_sys::Image.data` is an OWNED
    // `Vec<u8>`, so the wrapper copies the bytes and must then free the C-callback
    // buffer itself — before this fix, every kitty PNG decode leaked it. This test
    // drives the installed wrapper through the `terminal_sys::decode_png` slot with a
    // callback that allocates through the PROVIDED allocator (the reference contract);
    // the leak detector is the crate's G6 lane: this crate is pure Rust, so
    // `cargo +nightly miri test -p rustty-vt-c decode_png_wrapper_frees` reports
    // "memory leaked" pre-fix and runs clean post-fix (fail-first proven; LSan is
    // unavailable on arm64 macOS per R6, Miri IS this crate's leak gate).
    #[test]
    fn decode_png_wrapper_hands_over_the_callback_buffer() {
        let _guard = reset_global();

        // A decode callback that allocates the pixel buffer through the provided
        // C allocator (byte-aligned, exactly like the reference contract where the
        // callback allocates through the passed `*const CAllocator`).
        extern "C" fn decode_alloc(
            _u: *mut std::ffi::c_void,
            a: *const CAllocator,
            d: *const u8,
            l: usize,
            out: *mut Image,
        ) -> bool {
            let n = 4usize;
            // SAFETY: `a` is the allocator the wrapper passed us (live for the call).
            let buf = unsafe { (*a).zig().alloc_bytes(n) };
            if buf.is_null() {
                return false;
            }
            // SAFETY: `buf` is n bytes; `d`/`l` is the wrapper's input slice.
            unsafe {
                let seed = if l > 0 { *d } else { 0 };
                for i in 0..n {
                    *buf.add(i) = (i as u8) ^ seed;
                }
                *out = Image {
                    width: 2,
                    height: 2,
                    data: buf,
                    data_len: n,
                };
            }
            true
        }

        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, decode_alloc as *const std::ffi::c_void)
        });
        let f = terminal_sys::decode_png.read().unwrap().unwrap();
        let img = f(&terminal_sys::GlobalRawAllocator, &[0x42]).expect("decode must succeed");
        assert_eq!(2, img.width);
        assert_eq!(2, img.height);
        // The callback's own buffer, handed over uncopied; the caller frees it
        // through the allocator it passed (as `decodePng` does).
        assert_eq!(&[0x42u8, 0x43, 0x40, 0x41][..], unsafe { img.bytes() });
        use terminal_sys::RawAllocator as _;
        terminal_sys::GlobalRawAllocator.free_bytes(img.data);

        // Clear the slot again (leave the global state clean for siblings).
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, std::ptr::null())
        });
    }

    /// Parity S5-F2: the built-in stderr sink writes the scope and message
    /// BYTES verbatim, as the reference's `bufPrint("{s}")` does, including
    /// bytes that are not UTF-8. The parent re-runs this test in a child whose
    /// stderr it captures, so the assertion is on the real fd-2 output of the
    /// `rustty_sys_log_stderr` export, not on a helper.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation (re-runs the test binary as a child); runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn log_stderr_writes_non_utf8_scope_and_message_bytes_verbatim() {
        const CHILD: &str = "RUSTTY_T_H_LOG_STDERR_CHILD";
        const NAME: &str =
            "sys::tests::log_stderr_writes_non_utf8_scope_and_message_bytes_verbatim";
        let scope: &[u8] = b"sc\xffpe";
        let message: &[u8] = b"m\xc3\x28sg \xfe";
        if std::env::var_os(CHILD).is_some() {
            // Child arm: the parent asserts on this process's stderr.
            // SAFETY: both byte ranges are live slices.
            unsafe {
                crate::rustty_sys_log_stderr(
                    std::ptr::null_mut(),
                    LogLevel::Warning as std::os::raw::c_int,
                    scope.as_ptr(),
                    scope.len(),
                    message.as_ptr(),
                    message.len(),
                );
                crate::rustty_sys_log_stderr(
                    std::ptr::null_mut(),
                    LogLevel::Error as std::os::raw::c_int,
                    scope.as_ptr(),
                    0,
                    message.as_ptr(),
                    message.len(),
                );
            }
            return;
        }
        let bb = std::hint::black_box;
        assert!(
            std::str::from_utf8(bb(scope)).is_err() && std::str::from_utf8(bb(message)).is_err()
        );
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .expect("spawn the child test");
        assert!(out.status.success(), "child failed: {out:?}");
        let mut warning = b"[warning](".to_vec();
        warning.extend_from_slice(scope);
        warning.extend_from_slice(b"): ");
        warning.extend_from_slice(message);
        warning.push(b'\n');
        let mut error = b"[error]: ".to_vec();
        error.extend_from_slice(message);
        error.push(b'\n');
        let find = |needle: &[u8]| out.stderr.windows(needle.len()).any(|w| w == needle);
        assert!(
            find(&warning),
            "scoped line not verbatim in the child's stderr: {:?}",
            out.stderr
        );
        assert!(
            find(&error),
            "unscoped line not verbatim in the child's stderr: {:?}",
            out.stderr
        );
    }
}
