//! PORTS: ghostty src/apprt/embedded.zig — the `Platform` union, its `MacOS`/
//! `IOS` payloads and C mirror, `PlatformTag`, and `EnvVar` (embedded.zig
//! 343-424, chunk E2).

use super::*;

// =====================================================================
// embedded::Surface backend — chunk E2 (embedded.zig 343-609)
// =====================================================================

/// Platform-specific configuration for librustty.
//
// PORT NOTE (blueprint §5/§8.1): the reference `MacOS`/`IOS` wrap the host view in
// an `objc.Object` (`nsview: objc.Object`); rustty stores the RAW `*mut c_void`
// instead — the view is never *messaged* until Phase-3 rendering, so the embedded
// milestone needs NO objc2/coretext dependency (the Darwin metal/quicklook block
// that WOULD need it is a deferred Phase-3 `#[cfg]` leaf). The reference
// `if isDarwin() struct {…} else void` gating is preserved via
// `#[cfg(any(target_os = "macos", target_os = "ios"))]` (the rustty-os `file.rs`
// isDarwin precedent).
//
// reference `pub const Platform = union(PlatformTag) { macos: MacOS, ios: IOS }`.
pub enum Platform {
    // macos: MacOS,
    MacOS(MacOS),
    // ios: IOS,
    IOS(IOS),
}

// If our build target for librustty is not darwin then we do
// not include macos support at all.
//
// pub const MacOS = if (builtin.target.os.tag.isDarwin()) struct {
//     /// The view to render the surface on.
//     nsview: objc.Object,
// } else void;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub struct MacOS {
    /// The view to render the surface on.
    //
    // reference `nsview: objc.Object`; the raw view pointer (blueprint §5/§8.1 —
    // never messaged pre-render, so no objc wrap).
    pub nsview: *mut c_void,
}
/// reference `MacOS = … else void` (non-darwin).
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub struct MacOS;

// pub const IOS = if (builtin.target.os.tag.isDarwin()) struct {
//     /// The view to render the surface on.
//     uiview: objc.Object,
// } else void;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub struct IOS {
    /// The view to render the surface on.
    //
    // reference `uiview: objc.Object`; the raw view pointer (blueprint §5/§8.1).
    pub uiview: *mut c_void,
}
/// reference `IOS = … else void` (non-darwin).
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub struct IOS;

/// The host view pointer of a [`Platform`] value — the exact mapping the
/// reference `Metal.init` performs (Metal.zig:99-102: `switch { .macos =>
/// v.nsview, .ios => v.uiview }`; both darwin arms yield a real view). The
/// `nsview`/`uiview` fields only exist on darwin (`MacOS`/`IOS` are ZSTs
/// off-darwin, §2.9), so both concrete arms are `#[cfg]`-gated and a
/// non-darwin wildcard returns null (the platform never renders there).
/// Extracted from `Surface::host_view` so the §2.6/§2.9 mapping is unit-testable.
pub(crate) fn platform_host_view(platform: &Platform) -> *mut c_void {
    match platform {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Platform::MacOS(m) => m.nsview,
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Platform::IOS(i) => i.uiview,
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        _ => core::ptr::null_mut(),
    }
}

// The C ABI compatible version of this union. The tag is expected
// to be stored elsewhere.
//
// PORT NOTE: the reference nests this as `Platform.C`; a top-level `PlatformC`
// here (Rust cannot nest a type in an enum's namespace). Modeled as a `#[repr(C)]`
// union mirroring the reference `extern union` (the apprt-api `TargetCValue`/
// `ActionCValue` D2 precedent), pinned by the size/align const-asserts below; both
// members are a single pointer, so no explicit `_padding` field is required.
//
// pub const C = extern union {
//     macos: extern struct { nsview: ?*anyopaque },
//     ios: extern struct { uiview: ?*anyopaque },
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub union PlatformC {
    pub macos: PlatformCMacOS,
    pub ios: PlatformCIOS,
}

/// reference `Platform.C.macos` — `extern struct { nsview: ?*anyopaque }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlatformCMacOS {
    // nsview: ?*anyopaque,
    pub nsview: *mut c_void,
}

/// reference `Platform.C.ios` — `extern struct { uiview: ?*anyopaque }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlatformCIOS {
    // uiview: ?*anyopaque,
    pub uiview: *mut c_void,
}

// D2 const-asserts (blueprint §3): the `Platform.C` union crosses the C ABI, so
// its size/align are pinned. Both members are one pointer wide → the union is
// exactly pointer-sized (width-portable assert; cf. apprt-api `ActionCValue`).
const _: () = assert!(core::mem::size_of::<PlatformC>() == core::mem::size_of::<*mut c_void>());
const _: () = assert!(core::mem::align_of::<PlatformC>() == core::mem::align_of::<*mut c_void>());

/// The error set of [`Platform::init`].
//
// PORT NOTE: the reference `Platform.init` returns `!Platform` with an inferred
// error set — `std.meta.intToEnum`'s `InvalidEnumTag`, plus `NSViewMustBeSet` /
// `UIViewMustBeSet` / `UnsupportedPlatform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformError {
    // std.meta.intToEnum failure
    InvalidEnumTag,
    // error.NSViewMustBeSet
    NSViewMustBeSet,
    // error.UIViewMustBeSet
    UIViewMustBeSet,
    // error.UnsupportedPlatform
    UnsupportedPlatform,
}

impl Platform {
    /// Initialize a Platform a tag and configuration from the C ABI.
    //
    // reference `pub fn init(tag_int: c_int, c_platform: C) !Platform`.
    pub fn init(tag_int: c_int, c_platform: PlatformC) -> Result<Platform, PlatformError> {
        // On non-darwin both branches are `UnsupportedPlatform` and never read
        // `c_platform`; mark it used so `-D warnings` stays green there (on darwin
        // the union is read below).
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let _ = &c_platform;

        // const tag = try std.meta.intToEnum(PlatformTag, tag_int);
        let tag = match tag_int {
            // macos = 1,
            1 => PlatformTag::MacOS,
            // ios = 2,
            2 => PlatformTag::IOS,
            // std.meta.intToEnum → error.InvalidEnumTag on an out-of-range value.
            _ => return Err(PlatformError::InvalidEnumTag),
        };

        // return switch (tag) {
        match tag {
            // .macos => if (MacOS != void) macos: { ... } else error.UnsupportedPlatform,
            PlatformTag::MacOS => {
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                {
                    // const config = c_platform.macos;
                    let config = unsafe { c_platform.macos };
                    // const nsview = objc.Object.fromId(config.nsview orelse
                    //     break :macos error.NSViewMustBeSet);
                    //
                    // PORT NOTE: no objc wrap (blueprint §5); the raw pointer is
                    // stored directly. `?*anyopaque orelse error` → null-check.
                    if config.nsview.is_null() {
                        return Err(PlatformError::NSViewMustBeSet);
                    }
                    // break :macos .{ .macos = .{ .nsview = nsview } };
                    Ok(Platform::MacOS(MacOS {
                        nsview: config.nsview,
                    }))
                }
                #[cfg(not(any(target_os = "macos", target_os = "ios")))]
                {
                    // else error.UnsupportedPlatform
                    Err(PlatformError::UnsupportedPlatform)
                }
            }

            // .ios => if (IOS != void) ios: { ... } else error.UnsupportedPlatform,
            PlatformTag::IOS => {
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                {
                    // const config = c_platform.ios;
                    let config = unsafe { c_platform.ios };
                    // const uiview = objc.Object.fromId(config.uiview orelse
                    //     break :ios error.UIViewMustBeSet);
                    if config.uiview.is_null() {
                        return Err(PlatformError::UIViewMustBeSet);
                    }
                    // break :ios .{ .ios = .{ .uiview = uiview } };
                    Ok(Platform::IOS(IOS {
                        uiview: config.uiview,
                    }))
                }
                #[cfg(not(any(target_os = "macos", target_os = "ios")))]
                {
                    // else error.UnsupportedPlatform
                    Err(PlatformError::UnsupportedPlatform)
                }
            }
        }
    }
}

// pub const PlatformTag = enum(c_int) { ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)] // enum(c_int)
pub enum PlatformTag {
    // "0" is reserved for invalid so we can detect unset values
    // from the C API.

    // macos = 1,
    MacOS = 1,
    // ios = 2,
    IOS = 2,
}

// pub const EnvVar = extern struct { ... };
#[repr(C)]
pub struct EnvVar {
    /// The name of the environment variable.
    // key: [*:0]const u8,
    pub key: *const c_char,

    /// The value of the environment variable.
    // value: [*:0]const u8,
    pub value: *const c_char,
}
