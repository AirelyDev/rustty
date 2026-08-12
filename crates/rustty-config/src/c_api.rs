//! PORTS: ghostty `src/config/CApi.zig` — the `Config` C ABI: `config_new`/
//! `free`/`clone`/`load_*`/`finalize`/`get`/`trigger` + the diagnostics C
//! iteration + `open_path`, exported as `rustty_config_*` (R2).
//!
//! PORT NOTE: the reference allocates the `Config` with the process-global
//! `state.alloc` (`@import("../global.zig").state`). The rustty C ABI instead
//! owns the `Config` on the Rust global heap via `Box::into_raw`/`Box::from_raw`
//! (the `lib_vt`/`vt-c` pattern), so no global allocator is threaded through.
//! `state.alloc.create`/`destroy` map to `Box::into_raw`/`Box::from_raw`, and
//! the reference's `?*Config` (null on allocation failure) is always non-null in
//! rustty (the global allocator aborts on OOM rather than returning an error).
//!
//! The `extern "C"` wrappers are the single point that crosses the C ABI; the
//! `rustty_config_*` symbols are 1:1 with the reference's `ghostty_config_*`
//! exports (rustty_ prefix per R2). cbindgen emits them into the config header.

use crate::config_struct::Config;
use rustty_input::binding::{Action, TriggerC};

// const log = std.log.scoped(.config);
//
// PORT NOTE: rustty-config has no scoped-log sink in this leaf; the reference's
// `log.err(...)` in each `catch` arm becomes an `eprintln!` (the `lib_vt`/`vt-c`
// convention for C-ABI error logging).

/// Create a new configuration filled with the initial default values.
// export fn ghostty_config_new() ?*Config {
#[no_mangle]
pub extern "C" fn rustty_config_new() -> *mut Config {
    // const result = state.alloc.create(Config) catch ...; — the heap slot.
    // result.* = Config.default(state.alloc) catch ...;
    Box::into_raw(Box::new(Config::default_config()))
}

// export fn ghostty_config_free(ptr: ?*Config) void {
/// # Safety
/// `ptr` must be null or a pointer returned by `rustty_config_new` /
/// `rustty_config_clone` that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_free(ptr: *mut Config) {
    // if (ptr) |v| { v.deinit(); state.alloc.destroy(v); }
    if !ptr.is_null() {
        // SAFETY: per the fn-level contract `ptr` is a live `Box`-allocated Config.
        let mut v = unsafe { Box::from_raw(ptr) };
        v.deinit();
        // `drop(v)` reclaims the heap slot (== `state.alloc.destroy(v)`).
    }
}

/// Deep clone the configuration.
// export fn ghostty_config_clone(self: *Config) ?*Config {
/// # Safety
/// `self` must be a valid pointer from `rustty_config_new`/`rustty_config_clone`.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_clone(self_: *mut Config) -> *mut Config {
    // result.* = self.clone(state.alloc) catch ...;
    // SAFETY: per contract `self_` is a live Config pointer.
    let self_ref = unsafe { &*self_ };
    Box::into_raw(Box::new(self_ref.clone_config()))
}

/// Load the configuration from the CLI args.
// export fn ghostty_config_load_cli_args(self: *Config) void {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_load_cli_args(self_: *mut Config) {
    // self.loadCliArgs(state.alloc) catch |err| { log.err(...); };
    // SAFETY: per contract `self_` is a live, uniquely-borrowed Config pointer.
    if let Err(err) = unsafe { &mut *self_ }.load_cli_args() {
        eprintln!("error loading config err={err:?}");
    }
}

/// Load the configuration from the default file locations. This
/// is usually done first. The default file locations are locations
/// such as the home directory.
// export fn ghostty_config_load_default_files(self: *Config) void {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_load_default_files(self_: *mut Config) {
    // self.loadDefaultFiles(state.alloc) catch |err| { log.err(...); };
    // SAFETY: per contract `self_` is a live, uniquely-borrowed Config pointer.
    if let Err(err) = unsafe { &mut *self_ }.load_default_files() {
        eprintln!("error loading config err={err:?}");
    }
}

/// Load the configuration from a specific file path.
/// The path must be null-terminated.
// export fn ghostty_config_load_file(self: *Config, path: [*:0]const u8) void {
/// # Safety
/// `self` must be a valid `Config` pointer and `path` a valid NUL-terminated
/// C string.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_load_file(
    self_: *mut Config,
    path: *const std::os::raw::c_char,
) {
    // const path_slice = std.mem.span(path);
    // SAFETY: per contract `path` is a valid NUL-terminated C string.
    let path_slice = unsafe { std::ffi::CStr::from_ptr(path) }.to_bytes();
    // self.loadFile(state.alloc, path_slice) catch |err| { log.err(...); };
    // SAFETY: per contract `self_` is a live, uniquely-borrowed Config pointer.
    if let Err(err) = unsafe { &mut *self_ }.load_file(path_slice) {
        eprintln!(
            "error loading config from file path={} err={err:?}",
            std::string::String::from_utf8_lossy(path_slice)
        );
    }
}

/// Load the configuration from the user-specified configuration
/// file locations in the previously loaded configuration. This will
/// recursively continue to load up to a built-in limit.
// export fn ghostty_config_load_recursive_files(self: *Config) void {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_load_recursive_files(self_: *mut Config) {
    // self.loadRecursiveFiles(state.alloc) catch |err| { log.err(...); };
    // SAFETY: per contract `self_` is a live, uniquely-borrowed Config pointer.
    if let Err(err) = unsafe { &mut *self_ }.load_recursive_files() {
        eprintln!("error loading config err={err:?}");
    }
}

// export fn ghostty_config_finalize(self: *Config) void {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_finalize(self_: *mut Config) {
    // self.finalize() catch |err| { log.err(...); };
    // SAFETY: per contract `self_` is a live, uniquely-borrowed Config pointer.
    if let Err(err) = unsafe { &mut *self_ }.finalize() {
        eprintln!("error finalizing config err={err:?}");
    }
}

// export fn ghostty_config_get(self, ptr, key_str, len) bool {
/// # Safety
/// `self` must be a valid `Config` pointer, `key_str`/`len` a readable byte
/// range, and `ptr` writable storage matching the key's value type (see
/// [`crate::c_get::get`]).
#[no_mangle]
pub unsafe extern "C" fn rustty_config_get(
    self_: *mut Config,
    ptr: *mut std::ffi::c_void,
    key_str: *const u8,
    len: usize,
) -> bool {
    // @setEvalBranchQuota(10_000);
    // const key = std.meta.stringToEnum(Key, key_str[0..len]) orelse return false;
    // SAFETY: per contract `key_str[0..len]` is a readable byte range.
    let key_bytes = unsafe { std::slice::from_raw_parts(key_str, len) };
    let key = match crate::config_struct::Key::string_to_enum(key_bytes) {
        Some(k) => k,
        None => return false,
    };
    // return c_get.get(self, key, ptr);
    // SAFETY: `self_` is a live Config pointer; `ptr` matches the key type per
    // this fn's contract (forwarded to `c_get::get`'s identical contract).
    unsafe { crate::c_get::get(&*self_, key, ptr) }
}

// export fn ghostty_config_trigger(self, str, len) inputpkg.Binding.Trigger.C {
/// # Safety
/// `self` must be a valid `Config` pointer and `str`/`len` a readable byte range.
//
// PORT NOTE: the reference's `Trigger.C` is `extern struct { tag, key, mods:
// key.Mods }` where `key.Mods` is a `packed struct(u16)` — FFI-safe in Zig.
// `TriggerC.mods` now carries that packed integer (widened to the header's
// `int`-wide `rustty_input_mods_e` slot) instead of the non-`#[repr(C)]` `Mods`
// struct, so `TriggerC` is the header's 12-byte, register-returned layout and the
// `improper_ctypes_definitions` lint is no longer triggered. (Embedding the raw
// `Mods` struct made this 20 bytes → x8 sret return vs the caller's register
// return → a hard crash; see rustty-input `TriggerC` + MACOS_UI_PARITY_GAPS.md.)
#[no_mangle]
pub unsafe extern "C" fn rustty_config_trigger(
    self_: *mut Config,
    str_: *const u8,
    len: usize,
) -> TriggerC {
    // return config_trigger_(self, str[0..len]) catch |err| err: {
    //     log.err("error finding trigger err={}", .{err});
    //     break :err .{};
    // };
    // SAFETY: per contract `str_[0..len]` is a readable byte range and `self_`
    // a live Config pointer.
    let s = unsafe { std::slice::from_raw_parts(str_, len) };
    match config_trigger_(unsafe { &*self_ }, s) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("error finding trigger err={err:?}");
            // `.{}` — the default (empty) trigger projected to C.
            rustty_input::binding::Trigger::default().cval()
        }
    }
}

// fn config_trigger_(self: *Config, str: []const u8) !inputpkg.Binding.Trigger.C {
fn config_trigger_(self_: &Config, str_: &[u8]) -> Result<TriggerC, rustty_input::binding::Error> {
    // const action = try inputpkg.Binding.Action.parse(str);
    let action = Action::parse(str_)?;
    // const trigger: inputpkg.Binding.Trigger = self.keybind.set.getTrigger(action) orelse .{};
    let trigger = self_.keybind.set.get_trigger(&action).unwrap_or_default();
    // return trigger.cval();
    Ok(trigger.cval())
}

// export fn ghostty_config_diagnostics_count(self: *Config) u32 {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_diagnostics_count(self_: *mut Config) -> u32 {
    // return @intCast(self._diagnostics.items().len);
    // SAFETY: per contract `self_` is a live Config pointer.
    unsafe { &*self_ }._diagnostics.items().len() as u32
}

// export fn ghostty_config_get_diagnostic(self: *Config, idx: u32) Diagnostic {
/// # Safety
/// `self` must be a valid `Config` pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_get_diagnostic(self_: *mut Config, idx: u32) -> Diagnostic {
    // SAFETY: per contract `self_` is a live Config pointer.
    let cfg = unsafe { &*self_ };
    // const items = self._diagnostics.items();
    let items = cfg._diagnostics.items();
    // if (idx >= items.len) return .{};
    if idx as usize >= items.len() {
        return Diagnostic::default();
    }
    // const message = self._diagnostics.precompute.messages.items[idx];
    // return .{ .message = message.ptr };
    //
    // PORT NOTE: the reference's precomputed message is a `[:0]const u8`; the
    // rustty precompute stores the same bytes in a `Vec<u8>` WITH a trailing
    // NUL that `precompute_message` excludes from the returned slice (exactly
    // the reference's `toOwnedSliceSentinel(0)` view), so `as_ptr()` is a valid
    // `[*:0]const u8` for C. `None` (precompute disabled, i.e. a non-`.lib`
    // artifact) keeps the empty default rather than the reference's
    // comptime-impossible `void` field access.
    match cfg._diagnostics.precompute_message(idx as usize) {
        Some(message) => Diagnostic {
            message: message.as_ptr().cast(),
        },
        None => Diagnostic::default(),
    }
}

// export fn ghostty_config_open_path() String {
#[no_mangle]
pub extern "C" fn rustty_config_open_path() -> String {
    // const path = edit.openPath(state.alloc) catch |err| {
    //     log.err("error opening config in editor err={}", .{err});
    //     return .empty;
    // };
    match crate::edit::open_path() {
        // return .fromSlice(path);
        Ok(path) => String::from_path(path),
        Err(err) => {
            eprintln!("error opening config in editor err={err:?}");
            String::empty()
        }
    }
}

/// Sync with ghostty_diagnostic_s
// const Diagnostic = extern struct { message: [*:0]const u8 = "" };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Diagnostic {
    pub message: *const std::os::raw::c_char,
}

// `rustty_diagnostic_s` = `{ const char* message }` = one pointer. Returned
// by value by `rustty_config_get_diagnostic`; frozen so it stays register-sized.
// @embed-abi rustty_diagnostic_s 8
const _: () = assert!(core::mem::size_of::<Diagnostic>() == 8);
// @embed-abi-field rustty_diagnostic_s message 0
const _: () = assert!(core::mem::offset_of!(Diagnostic, message) == 0);

impl Default for Diagnostic {
    fn default() -> Self {
        // .message = "" — an empty NUL-terminated string.
        Diagnostic {
            message: c"".as_ptr(),
        }
    }
}

/// PORTS: `@import("../main_c.zig").String` (`ghostty_string_s`).
//
// PORT NOTE: `String` lives in the top-level `main_c.zig` C hub, which is above
// `config` in the port order and not yet ported. The reference `CApi.zig`
// imports it from there; rustty defines the faithful extern projection locally
// (a deferred seam) so `rustty_config_open_path` can return it. It will unify
// with the ported `main_c` String when that lands. `deinit` (which frees via
// `state.alloc`) is owned by `main_c`'s `ghostty_string_free`, so it is not
// re-exported here.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct String {
    pub ptr: *const u8,
    pub len: usize,
    pub sentinel: bool,
}

impl String {
    // pub const empty: String = .{ .ptr = null, .len = 0, .sentinel = false };
    fn empty() -> String {
        String {
            ptr: std::ptr::null(),
            len: 0,
            sentinel: false,
        }
    }

    /// `String.fromSlice(path)` for the `[:0]const u8` returned by `openPath`.
    //
    // PORT NOTE: `edit::open_path` returns an owned `Vec<u8>` (the reference's
    // arena-duped `[:0]const u8`). The buffer is leaked here so the pointer
    // stays valid for the C consumer; ownership transfers to C, which reclaims
    // it via `main_c`'s `ghostty_string_free` (`String.deinit`), exactly as the
    // reference hands off the `alloc_gpa`-owned slice. A trailing NUL is appended
    // (not counted in `len`) to honor the reference's sentinel-terminated type,
    // so `sentinel = true`.
    fn from_path(mut path: Vec<u8>) -> String {
        let len = path.len();
        path.push(0);
        let boxed = path.into_boxed_slice();
        let ptr = Box::leak(boxed).as_ptr();
        String {
            ptr,
            len,
            sentinel: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{config_trigger_, rustty_config_get};
    use crate::config_struct::Config;
    use crate::value_types::{Color, ColorC, WindowTheme};
    use rustty_input::binding::TriggerCTag;
    use std::os::raw::c_char;

    // test "ghostty_config_get: bool"
    #[test]
    fn ghostty_config_get_bool() {
        let mut cfg = Config::default_config();
        // cfg.maximize = true;
        cfg.maximize = true;

        // var out = false;
        let mut out = false;
        // const key = "maximize";
        let key = b"maximize";
        // try testing.expect(ghostty_config_get(&cfg, &out, key, key.len));
        assert!(unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
        // try testing.expect(out);
        assert!(out);
    }

    // test "ghostty_config_get: enum"
    #[test]
    fn ghostty_config_get_enum() {
        let mut cfg = Config::default_config();
        // cfg.@"window-theme" = .dark;
        cfg.window_theme = WindowTheme::Dark;

        // var out: [*:0]const u8 = undefined;
        let mut out: *const c_char = std::ptr::null();
        // const key = "window-theme";
        let key = b"window-theme";
        // try testing.expect(ghostty_config_get(&cfg, @ptrCast(&out), key, key.len));
        assert!(unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
        // const str = std.mem.sliceTo(out, 0);
        // try testing.expectEqualStrings("dark", str);
        let str_ = unsafe { std::ffi::CStr::from_ptr(out) };
        assert_eq!(str_.to_bytes(), b"dark");
    }

    // test "ghostty_config_get: optional null returns false"
    #[test]
    fn ghostty_config_get_optional_null_returns_false() {
        let mut cfg = Config::default_config();
        // cfg.@"unfocused-split-fill" = null;
        cfg.unfocused_split_fill = None;

        // var out: Config.Color.C = undefined;
        let mut out = ColorC { r: 0, g: 0, b: 0 };
        // const key = "unfocused-split-fill";
        let key = b"unfocused-split-fill";
        // try testing.expect(!ghostty_config_get(&cfg, @ptrCast(&out), key, key.len));
        assert!(!unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
    }

    // test "ghostty_config_get: unknown key returns false"
    #[test]
    fn ghostty_config_get_unknown_key_returns_false() {
        let mut cfg = Config::default_config();

        // var out = false;
        let mut out = false;
        // const key = "not-a-real-key";
        let key = b"not-a-real-key";
        // try testing.expect(!ghostty_config_get(&cfg, &out, key, key.len));
        assert!(!unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
    }

    // test "ghostty_config_get: optional string null returns true"
    #[test]
    fn ghostty_config_get_optional_string_null_returns_true() {
        let mut cfg = Config::default_config();
        // cfg.title = null;
        cfg.title = None;

        // var out: ?[*:0]const u8 = undefined;
        let mut out: *const c_char = c"".as_ptr();
        // const key = "title";
        let key = b"title";
        // try testing.expect(ghostty_config_get(&cfg, @ptrCast(&out), key, key.len));
        assert!(unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
        // try testing.expect(out == null);
        assert!(out.is_null());
    }

    // test "ghostty_config_get: float"
    #[test]
    fn ghostty_config_get_float() {
        let mut cfg = Config::default_config();
        // cfg.@"background-opacity" = 0.42;
        cfg.background_opacity = 0.42;

        // var out: f64 = 0;
        let mut out: f64 = 0.0;
        // const key = "background-opacity";
        let key = b"background-opacity";
        // try testing.expect(ghostty_config_get(&cfg, &out, key, key.len));
        assert!(unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
        // try testing.expectApproxEqAbs(@as(f64, 0.42), out, 0.000001);
        assert!((out - 0.42).abs() < 0.000_001);
    }

    // test "ghostty_config_get: struct cval conversion"
    #[test]
    fn ghostty_config_get_struct_cval_conversion() {
        let mut cfg = Config::default_config();
        // cfg.background = .{ .r = 12, .g = 34, .b = 56 };
        cfg.background = Color {
            r: 12,
            g: 34,
            b: 56,
        };

        // var out: Config.Color.C = undefined;
        let mut out = ColorC { r: 0, g: 0, b: 0 };
        // const key = "background";
        let key = b"background";
        // try testing.expect(ghostty_config_get(&cfg, @ptrCast(&out), key, key.len));
        assert!(unsafe {
            rustty_config_get(
                &mut cfg,
                std::ptr::from_mut(&mut out).cast(),
                key.as_ptr(),
                key.len(),
            )
        });
        // try testing.expectEqual(@as(u8, 12), out.r);
        assert_eq!(out.r, 12);
        assert_eq!(out.g, 34);
        assert_eq!(out.b, 56);
    }

    // test "ghostty_config_trigger: default keybind"
    #[test]
    fn ghostty_config_trigger_default_keybind() {
        let cfg = Config::default_config();

        // Default commands should be fetchable through config_trigger_
        {
            // const trigger = try config_trigger_(&cfg, "open_config");
            let trigger = config_trigger_(&cfg, b"open_config").unwrap();
            // try testing.expectEqual(.unicode, trigger.tag);
            assert_eq!(trigger.tag, TriggerCTag::Unicode);
            // try testing.expectEqual(@as(u32, ','), trigger.key.unicode);
            assert_eq!(unsafe { trigger.key.unicode }, u32::from(b','));
        }
        {
            // const trigger = try config_trigger_(&cfg, "reload_config");
            let trigger = config_trigger_(&cfg, b"reload_config").unwrap();
            assert_eq!(trigger.tag, TriggerCTag::Unicode);
            assert_eq!(unsafe { trigger.key.unicode }, u32::from(b','));
        }
        // Performable bindings are not tracked in the reverse map,
        // so config_trigger_ should return a default (empty) trigger.
        #[cfg(target_os = "macos")]
        {
            // const next = try config_trigger_(&cfg, "navigate_search:next");
            let next = config_trigger_(&cfg, b"navigate_search:next").unwrap();
            // try testing.expectEqual(.physical, next.tag);
            assert_eq!(next.tag, TriggerCTag::Physical);
            // try testing.expectEqual(.unidentified, next.key.physical);
            assert_eq!(
                unsafe { next.key.physical },
                rustty_vt::input::key::Key::Unidentified
            );

            // const prev = try config_trigger_(&cfg, "navigate_search:previous");
            let prev = config_trigger_(&cfg, b"navigate_search:previous").unwrap();
            assert_eq!(prev.tag, TriggerCTag::Physical);
            assert_eq!(
                unsafe { prev.key.physical },
                rustty_vt::input::key::Key::Unidentified
            );
        }
        {
            // const trigger = try config_trigger_(&cfg, "adjust_selection:left");
            let trigger = config_trigger_(&cfg, b"adjust_selection:left").unwrap();
            // try testing.expectEqual(.physical, trigger.tag);
            assert_eq!(trigger.tag, TriggerCTag::Physical);
            // try testing.expectEqual(.unidentified, trigger.key.physical);
            assert_eq!(
                unsafe { trigger.key.physical },
                rustty_vt::input::key::Key::Unidentified
            );
        }
    }
}
