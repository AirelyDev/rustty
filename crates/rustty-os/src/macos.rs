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
        &[build_config::BUNDLE_ID, sub_path],
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
        &[build_config::BUNDLE_ID, sub_path],
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
#[derive(Clone, Copy, PartialEq, Eq)]
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

fn commonDir(
    directory: NSSearchPathDirectory,
    sub_paths: &[&str],
) -> Result<String, AppleApiError> {
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

    // Create a new array with base_dir as the first element
    let mut paths: Vec<&str> = Vec::with_capacity(sub_paths.len() + 1);
    paths.push(base_dir);
    paths.extend_from_slice(sub_paths);

    Ok(fs_path_join(&paths))
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
    use std::os::raw::c_char;

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
                let cleaned: Vec<u8> =
                    message.bytes().take_while(|&b| b != 0).collect();
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
