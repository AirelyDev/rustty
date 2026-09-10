//! rustty-os::macos
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/macos.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `build_config`,
// `assert = quirks.inlineAssert`, `objc`, and `Allocator = std.mem.Allocator`.
// In Rust we use the standard library, `rustty_build_config` (the ported
// `build_config.zig`), `rustty_util::quirks::inlineAssert` for the assert seam,
// and `objc2`/`objc2-foundation` (R3: bindings to the SAME Objective-C
// runtime/Foundation framework the reference uses). This module is Darwin-only,
// matching the reference's `comptime assert(builtin.target.os.tag.isDarwin())`
// gates; the whole file is `cfg(target_os = "macos")`.
#![cfg(target_os = "macos")]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use rustty_build_config as build_config;
use rustty_util::quirks::inlineAssert as assert;

use objc2_foundation::{NSFileManager, NSProcessInfo};

/// Verifies that the running macOS system version is at least the given version.
pub fn isAtLeastVersion(major: i64, minor: i64, patch: i64) -> bool {
    // PORT NOTE: `comptime assert(builtin.target.os.tag.isDarwin())`. The whole
    // module is `cfg(target_os = "macos")`, so this asserts a constant truth on
    // the only target it compiles for, matching the reference's comptime gate.
    assert(cfg!(target_os = "macos"));

    let info = NSProcessInfo::processInfo();
    info.isOperatingSystemAtLeastVersion(objc2_foundation::NSOperatingSystemVersion {
        majorVersion: major as isize,
        minorVersion: minor as isize,
        patchVersion: patch as isize,
    })
}

pub type AppSupportDirError = AppleApiError;

// PORT NOTE: `pub const AppSupportDirError = Allocator.Error || error{AppleAPIFailed};`.
// Rust's allocation failure is not a recoverable error here; the parity-preserving
// equivalent is `AppleApiError` carrying the reference's `error.AppleAPIFailed`.

/// Return the path to the application support directory for Rustty
/// with the given sub path joined. This allocates the result using the
/// given allocator.
pub fn appSupportDir(sub_path: &str) -> Result<String, AppSupportDirError> {
    commonDir(
        // PORT NOTE: reference `.NSApplicationSupportDirectory`; the `objc2-foundation`
        // binding drops the `NS` prefix (`ApplicationSupportDirectory == 14`).
        NSSearchPathDirectory::ApplicationSupportDirectory,
        sub_path,
    )
}

pub type CacheDirError = AppleApiError;

// PORT NOTE: `pub const CacheDirError = Allocator.Error || error{AppleAPIFailed};`.
// See the note on `AppSupportDirError`.

/// Return the path to the system cache directory with the given sub path joined.
/// This allocates the result using the given allocator.
pub fn cacheDir(sub_path: &str) -> Result<String, CacheDirError> {
    commonDir(
        // PORT NOTE: reference `.NSCachesDirectory`; the `objc2-foundation` binding
        // drops the `NS` prefix (`CachesDirectory == 13`).
        NSSearchPathDirectory::CachesDirectory,
        sub_path,
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetQosClassError {
    // The thread can't have its QoS class changed usually because
    // a different pthread API was called that makes it an invalid
    // target.
    ThreadIncompatible,
}

/// Set the QoS class of the running thread.
///
/// https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon?preferredLanguage=occ
pub fn setQosClass(class: QosClass) -> Result<(), SetQosClassError> {
    // PORT NOTE: `std.posix.errno(pthread_set_qos_class_self_np(class, 0))`. The C
    // function returns an errno-style `c_int`; we switch on it directly.
    match unsafe { pthread_set_qos_class_self_np(class, 0) } {
        0 => Ok(()),                                              // .SUCCESS
        libc::EPERM => Err(SetQosClassError::ThreadIncompatible), // .PERM

        // EPERM is the only known error that can happen based on
        // the man pages for pthread_set_qos_class_self_np. I haven't
        // checked the XNU source code to see if there are other
        // possible errors.
        _ => panic!("unexpected pthread_set_qos_class_self_np error"),
    }
}

/// https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheTaskLevel.html#//apple_ref/doc/uid/TP40013929-CH35-SW1
//
// PORT NOTE (task 14d-1): `Debug` is derived so that
// `renderer/Thread.zig:289`'s restored `log.debug("thread QoS class set
// class={}", .{class})` can render the class. The variants are already
// spelled with the REFERENCE's own tag names, so `{:?}` prints exactly what
// Zig's `@tagName` would; Zig's `{}` additionally prefixes the `@typeName`,
// which the port drops (the same cosmetic divergence every `{:?}` site in
// this workspace carries).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum QosClass {
    user_interactive = 0x21,
    user_initiated = 0x19,
    default = 0x15,
    utility = 0x11,
    background = 0x09,
    unspecified = 0x00,
}

extern "C" {
    fn pthread_set_qos_class_self_np(
        qos_class: QosClass,
        relative_priority: std::ffi::c_int,
    ) -> std::ffi::c_int;
}

extern "C" {
    pub fn pthread_setname_np(name: *const std::ffi::c_char);
}

// PORT NOTE: `pub const NSOperatingSystemVersion = extern struct { major, minor, patch: i64 }`.
// We use `objc2_foundation::NSOperatingSystemVersion` (fields
// `majorVersion`/`minorVersion`/`patchVersion: NSInteger`), the binding to the
// SAME Foundation struct (R3), so it can be passed to
// `isOperatingSystemAtLeastVersion:`.
pub use objc2_foundation::NSOperatingSystemVersion;

// PORT NOTE: `pub const NSSearchPathDirectory = enum(c_ulong) { NSCachesDirectory =
// 13, NSApplicationSupportDirectory = 14 }` and `pub const
// NSSearchPathDomainMask = enum(c_ulong) { NSUserDomainMask = 1 }`. We use the
// `objc2_foundation` bindings to the SAME Foundation enums (R3); they are
// re-exported here to mirror the reference's public surface.
pub use objc2_foundation::{NSSearchPathDirectory, NSSearchPathDomainMask};

fn commonDir(directory: NSSearchPathDirectory, sub_path: &str) -> Result<String, AppleApiError> {
    // PORT NOTE: `comptime assert(builtin.target.os.tag.isDarwin())`. See the note in
    // `isAtLeastVersion`.
    assert(cfg!(target_os = "macos"));

    let manager = NSFileManager::defaultManager();

    let url = manager.URLForDirectory_inDomain_appropriateForURL_create_error(
        directory,
        // PORT NOTE: reference `NSSearchPathDomainMask.NSUserDomainMask`; the
        // `objc2-foundation` binding drops the `NS` prefix (`UserDomainMask == 1`).
        NSSearchPathDomainMask::UserDomainMask,
        None,
        true,
    );

    let url = match url {
        Ok(url) => url,
        Err(_) => return Err(AppleApiError::AppleAPIFailed),
    };

    let path = url.path();
    let path = match path {
        Some(p) => p,
        None => return Err(AppleApiError::AppleAPIFailed),
    };
    let c_str = path.UTF8String();
    if c_str.is_null() {
        return Err(AppleApiError::AppleAPIFailed);
    }
    let base_dir = unsafe { std::ffi::CStr::from_ptr(c_str) }
        .to_str()
        .map_err(|_| AppleApiError::AppleAPIFailed)?;

    // return try std.fs.path.join(
    //     alloc,
    //     &.{ base_dir, build_config.bundle_id, sub_path },
    // );
    //
    // (PORT NOTE (a436a9edc): upstream dropped the temporary `paths` array
    // alloc/`@memcpy`/free for a single direct join — same output bytes.)
    Ok(fs_path_join(&[base_dir, build_config::BUNDLE_ID, sub_path]))
}

/// PORT NOTE: reproduces `std.fs.path.join` for POSIX (Darwin): joins the components
/// with the platform separator, collapsing the separators between adjacent
/// components (a single separator between non-empty parts) exactly like the Zig
/// std implementation.
fn fs_path_join(paths: &[&str]) -> String {
    // Filter out empty components (Zig's join skips zero-length parts).
    let mut out = String::new();
    let mut prev_nonempty = false;
    for (i, p) in paths.iter().enumerate() {
        if p.is_empty() {
            continue;
        }
        if prev_nonempty {
            // Add a separator if the previous part didn't end with one and this
            // part doesn't start with one.
            let need_sep = !out.ends_with('/') && !p.starts_with('/');
            if need_sep {
                out.push('/');
            } else if out.ends_with('/') && p.starts_with('/') {
                // Avoid doubling separators.
                out.pop();
            }
        }
        let _ = i;
        out.push_str(p);
        prev_nonempty = true;
    }
    out
}

/// PORT NOTE: `error{AppleAPIFailed}` (a member of `AppSupportDirError`/`CacheDirError`).
#[derive(Debug, PartialEq, Eq)]
pub enum AppleApiError {
    AppleAPIFailed,
}

// ---------------------------------------------------------------------------
// PORTS: ghostty pkg/macos/os/log.zig — the unified-logging (os_log) binding
// consumed by main_ghostty.zig's `logFn` macOS sink (WS4). R3: binds the SAME
// libSystem os_log API the reference binds; `os_log_with_type` is a C macro,
// shimmed in cbits/log_shim.c (the reference's `zig_os_log_with_type`).
// ---------------------------------------------------------------------------
pub use self::oslog::{Log, LogType};

mod oslog {
    use std::ffi::{c_void, CStr, CString};
    // Only the `extern` block below names `c_char`, and that block is
    // `cfg(not(miri))` (see the MIRI ACCOMMODATION note).
    #[cfg(not(miri))]
    use std::os::raw::c_char;

    #[cfg(not(miri))]
    extern "C" {
        // os_log_t os_log_create(const char *subsystem, const char *category);
        fn os_log_create(subsystem: *const c_char, category: *const c_char) -> *mut c_void;
        // void os_release(void *object);
        fn os_release(object: *mut c_void);
        // bool os_log_type_enabled(os_log_t log, os_log_type_t type);
        fn os_log_type_enabled(log: *mut c_void, typ: u8) -> bool;
        // cbits/log_shim.c
        fn rustty_os_log_with_type(log: *mut c_void, typ: u8, msg: *const c_char);
    }

    // ----------------------------------------------------------------------
    // MIRI ACCOMMODATION (not a port deviation). Miri hard-errors the moment a
    // foreign function is CALLED ("can't call foreign function `os_log_create`
    // on OS `macos`"), which aborts the whole test binary — so every crate that
    // reaches a log site (rustty-core's `logging::log`, and through it much of
    // the workspace) would have to leave the Miri lane entirely.
    //
    // Unified logging is a SINK: nothing a test asserts on can observe whether
    // a line reached `os_log`. So under `cfg(miri)` the four libSystem entry
    // points become no-ops over an opaque non-null handle, and every line of
    // PORTED Rust above them — the `CString` construction, the interior-NUL
    // truncation path, the handle lifetime — still runs, and is still
    // Stacked-Borrows-checked. `not(miri)` keeps the real calls, so no shipped
    // build is affected. Same class and same rationale as the `cfg(miri)`
    // futex stand-ins in `rustty-xev/src/thread_pool.rs` and the `cfg(miri)`
    // page allocator in `rustty-os/src/page_allocator.rs`.
    //
    // The alternative — `#[cfg_attr(miri, ignore)]` on every test that can
    // reach a log call — would delete real Miri coverage of pure-Rust code to
    // work around a limitation in a sink. That is the trade this arm refuses.
    #[cfg(miri)]
    mod miri_ffi {
        use std::ffi::c_void;
        use std::os::raw::c_char;

        /// An opaque, never-dereferenced stand-in for `os_log_t`. Non-null so
        /// the real code's null checks behave as they do on a live handle.
        // `without_provenance_mut` rather than `1 as *mut _`: Miri warns on
        // integer-to-pointer casts, and this pointer is never dereferenced.
        const MIRI_LOG: *mut c_void = std::ptr::without_provenance_mut(1);

        pub unsafe fn os_log_create(
            _subsystem: *const c_char,
            _category: *const c_char,
        ) -> *mut c_void {
            MIRI_LOG
        }
        pub unsafe fn os_release(_object: *mut c_void) {}
        /// `true`, so the emit path above is exercised rather than short-circuited.
        pub unsafe fn os_log_type_enabled(_log: *mut c_void, _typ: u8) -> bool {
            true
        }
        pub unsafe fn rustty_os_log_with_type(_log: *mut c_void, _typ: u8, _msg: *const c_char) {}
    }
    #[cfg(miri)]
    use miri_ffi::{os_log_create, os_log_type_enabled, os_release, rustty_os_log_with_type};

    // pub const Log = opaque { ... };
    pub struct Log(*mut c_void);

    impl Log {
        // pub fn create(subsystem: [:0]const u8, category: [:0]const u8) *Log
        pub fn create(subsystem: &CStr, category: &CStr) -> Log {
            // SAFETY: both pointers are valid NUL-terminated C strings for the
            // duration of the call; os_log_create never returns NULL (the
            // reference unwraps with `.?`).
            Log(unsafe { os_log_create(subsystem.as_ptr(), category.as_ptr()) })
        }

        /// The raw `os_log_t`. The reference's `Log` is a Zig `opaque {}`, so
        /// every signpost call site simply writes `@ptrCast(log)`
        /// (`pkg/macos/os/signpost.zig:64,113`); Rust needs an explicit
        /// accessor. Crate-internal: the handle is not part of the public API.
        pub(super) fn as_raw(&self) -> *mut c_void {
            self.0
        }

        // pub fn release(self: *Log) void { c.os_release(self); }
        pub fn release(self) {
            // SAFETY: self.0 came from os_log_create and is released once
            // (by-value receiver — the handle is consumed).
            unsafe { os_release(self.0) }
        }

        // pub fn typeEnabled(self: *Log, typ: LogType) bool
        pub fn type_enabled(&self, typ: LogType) -> bool {
            // SAFETY: live os_log_t handle.
            unsafe { os_log_type_enabled(self.0, typ as u8) }
        }

        // pub fn log(self: *Log, alloc, typ: LogType, comptime format, args) void
        //   The reference allocPrints format+args to a NUL-terminated string and
        //   forwards it; the port takes the pre-formatted message (log sites
        //   format at the call, the same collapse as the bin's `log_fn`). A
        //   message with an interior NUL is logged truncated at the NUL (the
        //   reference cannot produce one from its format machinery either).
        pub fn log(&self, typ: LogType, message: &str) {
            let Ok(cmsg) = CString::new(message.as_bytes()) else {
                let cleaned: Vec<u8> = message.bytes().take_while(|&b| b != 0).collect();
                let cmsg = CString::new(cleaned).expect("NUL-free by construction");
                // SAFETY: live handle + valid C string.
                unsafe { rustty_os_log_with_type(self.0, typ as u8, cmsg.as_ptr()) };
                return;
            };
            // SAFETY: live handle + valid C string.
            unsafe { rustty_os_log_with_type(self.0, typ as u8, cmsg.as_ptr()) }
        }
    }

    /// https://developer.apple.com/documentation/os/os_log_type_t?language=objc
    // pub const LogType = enum(c.os_log_type_t) { ... };
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum LogType {
        Default = 0x00, // OS_LOG_TYPE_DEFAULT
        Info = 0x01,    // OS_LOG_TYPE_INFO
        Debug = 0x02,   // OS_LOG_TYPE_DEBUG
        Err = 0x10,     // OS_LOG_TYPE_ERROR
        Fault = 0x11,   // OS_LOG_TYPE_FAULT
    }
}

// ---------------------------------------------------------------------------
// PORTS: ghostty pkg/macos/os/signpost.zig — the `os_signpost` (Instruments)
// binding layered on the `os_log` handle above. R3: binds the SAME libSystem
// API the reference binds (`os_signpost_enabled`,
// `os_signpost_id_{generate,make_with_pointer}`,
// `_os_signpost_emit_with_name_impl`), all of which are real exported functions
// (`<os/signpost.h>`), so no C shim is needed.
// ---------------------------------------------------------------------------
pub mod signpost {
    use std::ffi::{c_void, CStr};
    use std::mem::MaybeUninit;
    use std::os::raw::c_char;
    // `c_int` is only the `dladdr` return type, and that `extern` block is
    // `cfg(not(miri))` (see the MIRI ACCOMMODATION note).
    #[cfg(not(miri))]
    use std::os::raw::c_int;
    use std::sync::atomic::{AtomicPtr, Ordering};

    use super::Log;

    #[cfg(not(miri))]
    extern "C" {
        // extern "c" fn dladdr(addr: ?*const anyopaque, info: *DlInfo) c_int;
        fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;
        // bool os_signpost_enabled(os_log_t log);
        fn os_signpost_enabled(log: *mut c_void) -> bool;
        // os_signpost_id_t os_signpost_id_generate(os_log_t log);
        fn os_signpost_id_generate(log: *mut c_void) -> u64;
        // os_signpost_id_t os_signpost_id_make_with_pointer(os_log_t, const void *);
        fn os_signpost_id_make_with_pointer(log: *mut c_void, ptr: *const c_void) -> u64;
        // void _os_signpost_emit_with_name_impl(void *dso, os_log_t log,
        //     os_signpost_type_t type, os_signpost_id_t spid, const char *name,
        //     const char *format, uint8_t *buf, uint32_t size);
        fn _os_signpost_emit_with_name_impl(
            dso: *mut c_void,
            log: *mut c_void,
            typ: u8,
            spid: u64,
            name: *const c_char,
            format: *const c_char,
            buf: *mut u8,
            size: u32,
        );
    }

    // ----------------------------------------------------------------------
    // MIRI ACCOMMODATION (not a port deviation) — see the identical note on the
    // `oslog` FFI above. `Benchmark::run` opens a signpost interval on Darwin
    // (faithful to `Benchmark.zig:44-66`), so under Miri EVERY rustty-bench
    // test aborts on `dladdr` before its parser ever runs. Instruments
    // signposts are pure instrumentation: no test observes them. Under
    // `cfg(miri)` the five libSystem entry points become no-ops over an opaque
    // handle — `init` still stores a non-null DSO handle, so its idempotence
    // and `emit_with_name`'s `assert(!dso.is_null())` still bind — and every
    // line of ported Rust around them stays in the Miri lane.
    #[cfg(miri)]
    mod miri_ffi {
        use std::ffi::c_void;
        use std::os::raw::{c_char, c_int};

        use super::DlInfo;

        /// Never dereferenced; only stored as the DSO handle / hashed into ids.
        const MIRI_HANDLE: *mut c_void = std::ptr::without_provenance_mut(1);

        /// Fills only `dli_fbase`, the one field `init` reads, and reports
        /// success — the shape `init`'s `assert(result != 0)` requires.
        pub unsafe fn dladdr(_addr: *const c_void, info: *mut DlInfo) -> c_int {
            unsafe {
                (*info).dli_fname = std::ptr::null();
                (*info).dli_fbase = MIRI_HANDLE;
                (*info).dli_sname = std::ptr::null();
                (*info).dli_saddr = std::ptr::null_mut();
            }
            1
        }
        /// `true`, so callers take the emitting path rather than skipping it.
        pub unsafe fn os_signpost_enabled(_log: *mut c_void) -> bool {
            true
        }
        pub unsafe fn os_signpost_id_generate(_log: *mut c_void) -> u64 {
            1
        }
        pub unsafe fn os_signpost_id_make_with_pointer(
            _log: *mut c_void,
            ptr: *const c_void,
        ) -> u64 {
            // The real call hashes the address (null -> the null id).
            ptr as u64
        }
        #[allow(clippy::too_many_arguments)]
        pub unsafe fn _os_signpost_emit_with_name_impl(
            _dso: *mut c_void,
            _log: *mut c_void,
            _typ: u8,
            _spid: u64,
            _name: *const c_char,
            _format: *const c_char,
            _buf: *mut u8,
            _size: u32,
        ) {
        }
    }
    #[cfg(miri)]
    use miri_ffi::{
        _os_signpost_emit_with_name_impl, dladdr, os_signpost_enabled, os_signpost_id_generate,
        os_signpost_id_make_with_pointer,
    };

    // const DlInfo = extern struct { ... };
    #[repr(C)]
    struct DlInfo {
        /// Pathname of shared object
        dli_fname: *const c_char,
        /// Base address of shared object
        dli_fbase: *mut c_void,
        /// Name of nearest symbol
        dli_sname: *const c_char,
        /// Address of nearest symbol
        dli_saddr: *mut c_void,
    }

    /// PORTS: `signpost.zig:44` `var __dso_handle: ?*c.mach_header = null`.
    ///
    /// Note this is NOT the linker-provided `__dso_handle` symbol — the
    /// reference deliberately shadows it with a module-private variable it
    /// populates itself (its comment: "This should REALLY be an extern var that
    /// is populated by the linker, but there is a Zig bug"). The port keeps the
    /// same self-populated shape rather than depending on the linker symbol, so
    /// the handle is correct whether the caller is an executable, a `cdylib` or
    /// a test binary. `AtomicPtr` because Zig's global is reachable from any
    /// thread and Rust has no `static mut` without `unsafe`.
    static DSO_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

    /// PORTS: `signpost.zig:12-41` `init`. This should be called once at the
    /// start of the program to initialize some required state for signpost
    /// logging. Idempotent (the reference's `if (__dso_handle != null) return`).
    pub fn init() {
        // if (__dso_handle != null) return;
        if !DSO_HANDLE.load(Ordering::Acquire).is_null() {
            return;
        }

        // The reference resolves the root module's `main` (or, in a library, the
        // first function declared in the root module) purely to have SOME symbol
        // that lives inside our own image; its own comment says "I actually
        // don't know if this is all required". Rust has no root-module
        // reflection, and does not need it: `init` itself is such a symbol, and
        // it is in the same image by construction.
        let sym = init as *const () as *const c_void;

        // var info: DlInfo = undefined;
        let mut info: MaybeUninit<DlInfo> = MaybeUninit::uninit();
        // const result = dladdr(sym, &info);
        // SAFETY: `sym` is a live code address in this image and `info` is a
        // properly aligned, writable `DlInfo`. `dladdr` only writes `info` on
        // success (nonzero return), which the assert below requires.
        let result = unsafe { dladdr(sym, info.as_mut_ptr()) };
        // assert(result != 0);
        assert!(result != 0);
        // SAFETY: `dladdr` returned nonzero, so it filled `info`.
        let info = unsafe { info.assume_init() };

        // __dso_handle = @ptrCast(@alignCast(info.dli_fbase));
        DSO_HANDLE.store(info.dli_fbase, Ordering::Release);
    }

    /// PORTS: `signpost.zig:63-65` `enabled`. Checks whether signpost logging is
    /// enabled for the given log handle. Returns true if signposts will be
    /// recorded for this log, false otherwise. This can be used to avoid
    /// expensive operations when signpost logging is disabled.
    ///
    /// <https://developer.apple.com/documentation/os/os_signpost_enabled?language=objc>
    pub fn enabled(log: &Log) -> bool {
        // SAFETY: `log` is a live `os_log_t` from `os_log_create`.
        unsafe { os_signpost_enabled(log.as_raw()) }
    }

    /// PORTS: `signpost.zig:73-79` `emitEvent`. Emits a signpost event — a
    /// single point in time marker. Events are useful for marking when specific
    /// actions occur, such as user interactions, state changes, or other
    /// discrete occurrences. The event will appear as a vertical line in
    /// Instruments.
    ///
    /// <https://developer.apple.com/documentation/os/os_signpost_event_emit?language=objc>
    pub fn emit_event(log: &Log, id: Id, name: &CStr) {
        emit_with_name(log, id, Type::Event, name);
    }

    /// PORTS: `signpost.zig:88-90` `intervalBegin`. Marks the beginning of a
    /// time interval. Use this with [`interval_end`] to measure the duration of
    /// operations. The same ID must be used for both the begin and end calls.
    /// Intervals appear as horizontal bars in the Instruments timeline.
    ///
    /// <https://developer.apple.com/documentation/os/os_signpost_interval_begin?language=objc>
    pub fn interval_begin(log: &Log, id: Id, name: &CStr) {
        emit_with_name(log, id, Type::IntervalBegin, name);
    }

    /// PORTS: `signpost.zig:98-100` `intervalEnd`. Marks the end of a time
    /// interval. Must be paired with a prior [`interval_begin`] call using the
    /// same ID. The name should match the name used in `interval_begin`.
    /// Instruments will calculate and display the duration between begin and
    /// end.
    ///
    /// <https://developer.apple.com/documentation/os/os_signpost_interval_end?language=objc>
    pub fn interval_end(log: &Log, id: Id, name: &CStr) {
        emit_with_name(log, id, Type::IntervalEnd, name);
    }

    /// PORTS: `signpost.zig:103-124` `emitWithName`. The internal function to
    /// emit a signpost with a specific name.
    fn emit_with_name(log: &Log, id: Id, typ: Type, name: &CStr) {
        // Init must be called by this point.
        // assert(__dso_handle != null);
        let dso = DSO_HANDLE.load(Ordering::Acquire);
        assert!(!dso.is_null());

        // var buf: [2]u8 = @splat(0);
        let mut buf: [u8; 2] = [0; 2];
        // SAFETY: `dso` is this image's mach header (asserted non-null), `log`
        // is a live `os_log_t`, `name` and the empty format string are valid
        // NUL-terminated C strings for the call, and `buf` is a writable
        // 2-byte scratch buffer whose length is passed alongside it — exactly
        // the reference's argument shape.
        unsafe {
            _os_signpost_emit_with_name_impl(
                dso,
                log.as_raw(),
                typ as u8,
                id.0,
                name.as_ptr(),
                c"".as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            );
        }
    }

    /// PORTS: `signpost.zig:127-165` `Id`.
    ///
    /// <https://developer.apple.com/documentation/os/os_signpost_id_t?language=objc>
    ///
    /// The reference is a non-exhaustive `enum(u64)` (the trailing `_`), i.e. any
    /// `u64` is a valid value; a Rust `enum` cannot model that, so this is the
    /// equivalent newtype with the three named values as associated constants.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Id(pub u64);

    impl Id {
        /// `null = 0` — `OS_SIGNPOST_ID_NULL`.
        pub const NULL: Id = Id(0);
        /// `invalid = 0xFFFFFFFFFFFFFFFF` — `OS_SIGNPOST_ID_INVALID`.
        pub const INVALID: Id = Id(0xFFFF_FFFF_FFFF_FFFF);
        /// `exclusive = 0xEEEEB0B5B2B2EEEE` — `OS_SIGNPOST_ID_EXCLUSIVE`.
        pub const EXCLUSIVE: Id = Id(0xEEEE_B0B5_B2B2_EEEE);

        /// PORTS: `signpost.zig:137-139` `generate`. Generates a new signpost ID
        /// for use with signpost operations. The ID is unique for the given log
        /// handle and can be used to track asynchronous operations or mark
        /// specific points of interest in the code.
        ///
        /// <https://developer.apple.com/documentation/os/os_signpost_id_generate?language=objc>
        pub fn generate(log: &Log) -> Id {
            // SAFETY: `log` is a live `os_log_t`.
            Id(unsafe { os_signpost_id_generate(log.as_raw()) })
        }

        /// PORTS: `signpost.zig:148-153` `forPointer`. Creates a signpost ID
        /// based on a pointer value. This is useful for tracking operations
        /// associated with a specific object or memory location. The same
        /// pointer will always generate the same ID for a given log handle,
        /// allowing correlation of signpost events. Pass null to get the null
        /// signpost ID.
        ///
        /// <https://developer.apple.com/documentation/os/os_signpost_id_for_pointer?language=objc>
        // The reference is a SAFE fn taking `?*anyopaque`, and rightly so:
        // `os_signpost_id_make_with_pointer` only hashes the address into an id,
        // it never dereferences it (null is explicitly allowed and yields the
        // null id). Clippy cannot see through the FFI boundary, so it assumes
        // the worst; marking this `unsafe` would be a false contract.
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn for_pointer(log: &Log, ptr: *const c_void) -> Id {
            // SAFETY: `log` is a live `os_log_t`; `ptr` is only hashed, never
            // dereferenced (it may be null, as the reference's `?*anyopaque`
            // allows).
            Id(unsafe { os_signpost_id_make_with_pointer(log.as_raw(), ptr) })
        }
    }

    /// PORTS: `signpost.zig:168-175` `Type`.
    ///
    /// <https://developer.apple.com/documentation/os/ossignposttype?language=objc>
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum Type {
        Event = 0,         // OS_SIGNPOST_EVENT
        IntervalBegin = 1, // OS_SIGNPOST_INTERVAL_BEGIN
        IntervalEnd = 2,   // OS_SIGNPOST_INTERVAL_END
    }

    impl Type {
        /// `pub const mask: u8 = 0x03;` — `OS_SIGNPOST_TYPE_MASK`.
        pub const MASK: u8 = 0x03;
    }

    /// PORTS: `signpost.zig:177-195` `Category`. Special `os_log` category values
    /// that surface in Instruments and other tooling.
    pub mod category {
        use std::ffi::CStr;

        /// Points of Interest appear as a dedicated track in Instruments.
        /// Use this for high-level application events that help understand
        /// the flow of your application.
        pub const POINTS_OF_INTEREST: &CStr = c"PointsOfInterest";

        /// Dynamic Tracing category enables runtime-configurable logging.
        /// Signposts in this category can be enabled/disabled dynamically
        /// without recompiling.
        pub const DYNAMIC_TRACING: &CStr = c"DynamicTracing";

        /// Dynamic Stack Tracing category captures call stacks at signpost
        /// events. This provides deeper debugging information but has higher
        /// performance overhead.
        pub const DYNAMIC_STACK_TRACING: &CStr = c"DynamicStackTracing";
    }

    // test { _ = Id; } / test enabled / test "intervals" (signpost.zig:197-215)
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::macos::Log;

        // R2: the reference's literal subsystem is "com.mitchellh.ghostty"; the
        // KAT uses rustty's bundle id, exactly as the sibling `log_tests` does.
        const SUBSYSTEM: &CStr = c"io.airely.rustty";

        // test "generate ID"
        #[test]
        #[cfg_attr(miri, ignore)] // ffi: real libSystem os_signpost calls
        fn generate_id() {
            // We can't really test the return value because it may return null
            // if signposts are disabled.
            let log = Log::create(SUBSYSTEM, c"test");
            let id = Id::generate(&log);
            assert_ne!(id, Id::INVALID);
        }

        // test "generate ID for pointer"
        #[test]
        #[cfg_attr(miri, ignore)] // ffi: real libSystem os_signpost calls
        fn generate_id_for_pointer() {
            let foo: usize = 0x1234;
            let log = Log::create(SUBSYSTEM, c"test");
            let id = Id::for_pointer(&log, std::ptr::addr_of!(foo).cast());
            assert_ne!(id, Id::NULL);
        }

        // test enabled
        #[test]
        #[cfg_attr(miri, ignore)] // ffi: real libSystem os_signpost calls
        fn enabled_smoke() {
            let _ = enabled(&Log::create(SUBSYSTEM, c"test"));
        }

        // test "intervals"
        #[test]
        #[cfg_attr(miri, ignore)] // ffi: real libSystem os_signpost calls
        fn intervals() {
            init();
            // `init` populated the image handle; without it `emit_with_name`
            // would assert (and the reference would too).
            assert!(!DSO_HANDLE.load(Ordering::Acquire).is_null());

            let log = Log::create(SUBSYSTEM, category::POINTS_OF_INTEREST);

            // Test that we can begin and end an interval.
            let id = Id::generate(&log);
            interval_begin(&log, id, c"Test Interval");
            // The reference's test stops at `intervalBegin`; closing the
            // interval and emitting an event exercises the other two arms of
            // `emitWithName` over the same live handle.
            emit_event(&log, id, c"Test Event");
            interval_end(&log, id, c"Test Interval");

            log.release();
        }

        // `init` is idempotent (`signpost.zig:13`) and safe to call from any
        // order relative to the other tests, which cargo runs in parallel
        // threads.
        #[test]
        #[cfg_attr(miri, ignore)] // ffi: real libSystem os_signpost calls
        fn init_is_idempotent() {
            init();
            let first = DSO_HANDLE.load(Ordering::Acquire);
            assert!(!first.is_null());
            init();
            assert_eq!(first, DSO_HANDLE.load(Ordering::Acquire));
        }
    }
}

// test { ... } (pkg/macos/os/log.zig)
#[cfg(test)]
mod log_tests {
    use super::{Log, LogType};

    #[test]
    fn os_log_create_and_log() {
        // const log = Log.create("com.mitchellh.ghostty", "test");
        //   R2: bundle id retargeted via rustty-identity at the CALLER; the
        //   binding takes any subsystem — the KAT uses a literal like the oracle.
        let log = Log::create(c"io.airely.rustty", c"test");
        // try testing.expect(log.typeEnabled(.fault));
        assert!(log.type_enabled(LogType::Fault));
        // log.log(testing.allocator, .default, "hello {d}", .{12});
        log.log(LogType::Default, "hello 12");
        // defer log.release();
        log.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // ffi: NSFileManager/Foundation calls hit the real OS (ASan, not Miri)
    fn cacheDir_paths() {
        // PORT NOTE: `if (!builtin.target.os.tag.isDarwin()) return;`. The whole
        // module is `cfg(target_os = "macos")` so this test only compiles on
        // Darwin; the early-return guard is therefore always-false here.

        // Test base path
        {
            let cache_path = cacheDir("").unwrap();
            assert!(cache_path.contains("Caches"));
            assert!(cache_path.contains(build_config::BUNDLE_ID));
        }

        // Test with subdir
        {
            let cache_path = cacheDir("test").unwrap();
            assert!(cache_path.contains("Caches"));
            assert!(cache_path.contains(build_config::BUNDLE_ID));

            let bundle_path = format!("{}/test", build_config::BUNDLE_ID);
            assert!(cache_path.contains(&bundle_path));
        }
    }
}
