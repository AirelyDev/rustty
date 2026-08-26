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

    // var out: Image = undefined;
    let mut out = Image {
        width: 0,
        height: 0,
        data: std::ptr::null_mut(),
        data_len: 0,
    };
    // if (!func(global.userdata, &c_alloc, data.ptr, data.len, &out)) return error.InvalidData;
    if !func(userdata, &c_alloc, data.as_ptr(), data.len(), &mut out) {
        return Err(terminal_sys::DecodeError::InvalidData);
    }

    // const result_data = out.data orelse return error.InvalidData;
    if out.data.is_null() {
        return Err(terminal_sys::DecodeError::InvalidData);
    }

    // return .{ .width = out.width, .height = out.height, .data = result_data[0..out.data_len] };
    // PORT NOTE (ownership model): the reference's `terminal_sys.Image.data` is a
    // `[]u8` it takes ownership of directly (the kitty image later frees it via
    // `alloc.free(data)` — a byte-aligned []u8 free through the same allocator the
    // callback allocated from). rustty's `Image.data` is a `Vec<u8>` (owned, frees
    // on drop via the Rust allocator), so we copy the callback-allocated bytes
    // into a Vec AND free the callback's buffer here through the same allocator we
    // handed it — the rustty analogue of the reference's downstream
    // `alloc.free(data)` (VALIDATION_REPORT §3c.4: without this free, every kitty
    // PNG decode leaked the C-callback buffer; regression-pinned Miri-fail-first by
    // `decode_png_wrapper_frees_callback_buffer` below).
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.data_len) }.to_vec();
    // Free the callback-allocated buffer: byte (1) alignment, exactly the layout
    // `alloc_bytes`/the reference's `[]u8` contract allocated with.
    use rustty_lib::allocator::ZigAllocator as _;
    zig_alloc.free(
        rustty_lib::allocator::ZigSlice::new(out.data, out.data_len),
        rustty_lib::allocator::Alignment(0),
        0,
    );
    Ok(terminal_sys::Image {
        width: out.width,
        height: out.height,
        data: bytes,
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

/// Emits logs in chunks. Almost all logs will be less than the chunk size
/// but this allows emitting larger logs without heap allocation.
// const LogEmitter = struct { c_level, scope_text, buf: [2048]u8, pos: usize, ... }
//
// `cfg(not(wasm32))`: see the `emit_log` port note.
#[cfg(not(target_arch = "wasm32"))]
struct LogEmitter<'a> {
    c_level: LogLevel,
    scope_text: &'a [u8],
    buf: [u8; 2048],
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
        buf: [0u8; 2048],
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
// PORT NOTE: the `std.debug.lockStderr` avoidance above is Zig binary-size
// machinery (`std.Io` vtable retention) with no Rust analogue; the port gets
// the same no-interleaving guarantee by holding `std::io::stderr().lock()`
// for the whole line (see the lock PORT NOTE in the body).
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

    // The reference formats the line into a stack buffer and emits it with a
    // single raw stderr write, falling back to piecewise writes when it does
    // not fit:
    //
    //     // Large enough for a full logFn chunk plus the level/scope prefix.
    //     var buffer: [LogEmitter.buffer_size + 128]u8 = undefined;
    //     const line: ?[]const u8 = ...bufPrint(&buffer, ...) catch null;
    //     if (line) |v| { stderr.write(v); return; }
    //
    //     // The line didn't fit in our buffer (an embedder called us directly
    //     // with a very large message). Fall back to writing the pieces
    //     // separately; interleaving with other threads is possible here but
    //     // this is a best-effort diagnostic path.
    //     stderr.write("["); ...
    //
    // PORT NOTE: the port instead holds the stderr lock for the duration of
    // the write — the same thread-safe, mutex-protected output with the same
    // no-interleaving observable, and with no fixed buffer there is no
    // didn't-fit fallback path to reproduce.
    use std::io::Write as _;
    let stderr = std::io::stderr();
    let mut lock = stderr.lock();
    // nosuspend { if (scope.len > 0) { ... } else { ... } }
    if !scope.is_empty() {
        // writer.print("[{s}]({s}): {s}\n", .{ level_text, scope, message });
        // PORT NOTE: the trailing `\n` is emitted by `writeln!` (identical bytes).
        let _ = writeln!(
            lock,
            "[{}]({}): {}",
            level_text,
            String::from_utf8_lossy(scope),
            String::from_utf8_lossy(message)
        );
    } else {
        // writer.print("[{s}]: {s}\n", .{ level_text, message });
        // PORT NOTE: the trailing `\n` is emitted by `writeln!` (identical bytes).
        let _ = writeln!(
            lock,
            "[{}]: {}",
            level_text,
            String::from_utf8_lossy(message)
        );
    }
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
    fn decode_png_wrapper_frees_callback_buffer() {
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
        // bytes copied out of the callback buffer before it is freed
        assert_eq!(vec![0x42u8, 0x43, 0x40, 0x41], img.data);

        // Clear the slot again (leave the global state clean for siblings).
        assert_eq!(Result::Success, unsafe {
            set(Option_::DecodePng, std::ptr::null())
        });
    }
}
