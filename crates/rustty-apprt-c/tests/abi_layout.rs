//! abi_layout.rs — the `include/ghostty.h` ABI-layout freeze gate (Task A3, I9).
//!
//! Branch A (PREPORT_SWIFT_APP §3): the reused Swift app `#import`s the UPSTREAM
//! `include/ghostty.h` verbatim, and every `ghostty_*_s`/`_u`/`_e` typedef must
//! resolve to a rustty `#[repr(C)]` type with byte-identical layout. This test is
//! the Rust half of that freeze: for every ABI struct/union/enum in the header it
//! asserts `size_of`/`align_of` and each pinned enum ordinal equals the value the
//! header's C declaration produces on the LP64 macOS/iOS ABI. The `.port` gate
//! `check_ghostty_h_abi.sh` is the complementary half: it byte-diffs the shipped
//! header against the pinned upstream (preserving the R-A/R-B/R-C typos).
//!
//! WHY BOTH compile-time `const _` asserts AND runtime `#[test]`s: the `const`
//! asserts make ANY layout drift a HARD BUILD FAILURE of the test target (the D2
//! build-failure discipline from `capi.rs` — drift can never even compile). The
//! `#[test]` functions re-check the same facts at runtime so `cargo test -p
//! rustty-apprt-c --test abi_layout` has runnable, human-readable assertions
//! (`assert_eq!` names the two numbers on failure, which a `const assert!` does
//! not).
//!
//! Expected sizes were computed by reading the header field types (see
//! `/Users/gael/projects/rustty/include/ghostty.h`, the pinned upstream copy) and
//! laying them out under the C rules for the 64-bit Apple ABI (`double`/pointer =
//! 8B align-8, `float`/`uint32_t`/C-`enum` = 4B align-4, `bool`/`uint8_t` = 1B,
//! `uint16_t` = 2B). Several are ALSO asserted at their definition sites in
//! `capi.rs`/`embedded.rs`/`action.rs`; re-pinning them here co-locates the whole
//! ghostty.h ABI surface behind one gate the Swift-app milestone can point at.

// The ported enum/struct names keep the reference camelCase/PascalCase; the crate
// convention silences the snake-case lint at test-module scope.
#![allow(non_snake_case)]

use core::ffi::{c_int, c_void};

use rustty_apprt::embedded::{
    ClipboardContent, EnvVar, Options, PlatformC, PlatformCIOS, PlatformCMacOS, PlatformTag,
    SurfaceOptions,
};
use rustty_apprt_api::action::{
    ActionC, ActionCValue, ActionKey, TargetC, TargetCValue, TargetKey,
};
use rustty_apprt_api::structs::{ClipboardRequestType, ColorScheme};
use rustty_apprt_api::surface::NewSurfaceContext;
use rustty_apprt_c::capi::{KeyEvent, Point, PointCoordTag, PointTag, Selection, SurfaceSize};
use rustty_apprt_c::String as GhosttyString;

// The C-`enum` width on the Apple LP64 ABI is `int` == 4 bytes. Every
// `enum(c_int)`/`#[repr(i32)]` type below is pinned to this.
const C_ENUM: usize = core::mem::size_of::<c_int>();
const PTR: usize = core::mem::size_of::<*mut c_void>();

// ============================================================================
// Compile-time freeze (the HARD gate — drift fails the build).
// ============================================================================

// ── ghostty_runtime_config_s (App.Options, ghostty.h:1019-1028) ──
// void* userdata@0(8); bool supports_selection_clipboard@8(1→pad); then 6 fn-ptr
// callbacks @16,24,32,40,48,56 → size 64, align 8.
const _: () = assert!(core::mem::size_of::<Options>() == 64);
const _: () = assert!(core::mem::align_of::<Options>() == 8);

// ── ghostty_surface_config_s (Surface.Options, ghostty.h:467-480) ──
// platform_tag(enum)@0(4→pad); platform(union)@8(8); userdata@16(8);
// scale_factor(double)@24(8); font_size(float)@32(4→pad); working_directory@40(8);
// command@48(8); env_vars@56(8); env_var_count(size_t)@64(8); initial_input@72(8);
// wait_after_command(bool)@80(1→pad); context(enum)@84(4) → size 88, align 8.
const _: () = assert!(core::mem::size_of::<SurfaceOptions>() == 88);
const _: () = assert!(core::mem::align_of::<SurfaceOptions>() == 8);

// ── ghostty_platform_u + macos/ios arms (ghostty.h:448-459) ──
// Each arm is a single `void*`; the union is exactly pointer-sized.
const _: () = assert!(core::mem::size_of::<PlatformC>() == PTR);
const _: () = assert!(core::mem::align_of::<PlatformC>() == PTR);
const _: () = assert!(core::mem::size_of::<PlatformCMacOS>() == PTR);
const _: () = assert!(core::mem::size_of::<PlatformCIOS>() == PTR);

// ── ghostty_env_var_s (ghostty.h:443-446) ── {const char* key, value} → 16, align 8.
const _: () = assert!(core::mem::size_of::<EnvVar>() == 16);
const _: () = assert!(core::mem::align_of::<EnvVar>() == 8);

// ── ghostty_clipboard_content_s (ghostty.h:76-79) ── {const char* mime, data} → 16.
const _: () = assert!(core::mem::size_of::<ClipboardContent>() == 16);
const _: () = assert!(core::mem::align_of::<ClipboardContent>() == 8);

// ── ghostty_text_s (ghostty.h:408-415) ──
// double tl_px_x@0(8); double tl_px_y@8(8); uint32_t offset_start@16(4);
// uint32_t offset_len@20(4); const char* text@24(8); uintptr_t text_len@32(8)
// → size 40, align 8.
const _: () = assert!(core::mem::size_of::<rustty_apprt_c::capi::Text>() == 40);
const _: () = assert!(core::mem::align_of::<rustty_apprt_c::capi::Text>() == 8);

// ── ghostty_string_s (main_c.zig / ghostty.h:402-406) ──
// const char* ptr@0(8); uintptr_t len@8(8); bool sentinel@16(1→pad) → size 24, align 8.
const _: () = assert!(core::mem::size_of::<GhosttyString>() == 24);
const _: () = assert!(core::mem::align_of::<GhosttyString>() == 8);

// ── ghostty_surface_size_s (ghostty.h:482-489) ──
// uint16_t columns,rows; uint32_t width_px,height_px,cell_width_px,cell_height_px
// → size 20, align 4.
const _: () = assert!(core::mem::size_of::<SurfaceSize>() == 20);
const _: () = assert!(core::mem::align_of::<SurfaceSize>() == 4);

// ── ghostty_point_s (ghostty.h:429-435) ── {enum tag, enum coord, u32 x, u32 y} → 16, align 4.
const _: () = assert!(core::mem::size_of::<Point>() == 16);
const _: () = assert!(core::mem::align_of::<Point>() == 4);

// ── ghostty_selection_s (ghostty.h:437-441) ── {Point tl@0(16), Point br@16(16), bool@32} → 36, align 4.
const _: () = assert!(core::mem::size_of::<Selection>() == 36);
const _: () = assert!(core::mem::align_of::<Selection>() == 4);

// ── ghostty_input_key_s wire form (KeyEvent, embedded.zig:1248) ──
// action@0(4) mods@4(4) consumed_mods@8(4) keycode@12(4) text@16(8)
// unshifted_codepoint@24(4) composing@28(1→pad) → size 32, align 8.
const _: () = assert!(core::mem::size_of::<KeyEvent>() == 32);
const _: () = assert!(core::mem::align_of::<KeyEvent>() == 8);

// ── ghostty_target_s / _u (ghostty.h:555-567) ──
// union{ ghostty_surface_t } → one pointer (8). struct{ enum key@0(4→pad), union@8(8) } → 16.
const _: () = assert!(core::mem::size_of::<TargetCValue>() == PTR);
const _: () = assert!(core::mem::size_of::<TargetC>() == 16);
const _: () = assert!(core::mem::align_of::<TargetC>() == 8);

// ── ghostty_action_s / _u (ghostty.h:955-998) ──
// The action value union's WIDEST arm is 24 bytes (e.g. open_url: {kind:i32,
// url:*const u8, len:usize}); struct{ enum key@0(4→pad), union@8(24) } → 32.
// NOTE: byte-exact parity of the value-union INTERIOR against the header's
// per-arm C structs is deferred to the embedded C-API milestone (see the
// `ActionCValue` note in apprt-api `action.rs`); this pins the Rust layout that
// the action_cb ships by value, which is what the Swift boundary reads.
const _: () = assert!(core::mem::size_of::<ActionCValue>() == 24);
const _: () = assert!(core::mem::size_of::<ActionC>() == 32);
const _: () = assert!(core::mem::align_of::<ActionC>() == 8);

// ── Pinned enum widths (all C `enum`s are `int`-wide on this ABI) ──
const _: () = assert!(core::mem::size_of::<PlatformTag>() == C_ENUM);
const _: () = assert!(core::mem::size_of::<PointTag>() == C_ENUM);
const _: () = assert!(core::mem::size_of::<PointCoordTag>() == C_ENUM);
const _: () = assert!(core::mem::size_of::<TargetKey>() == C_ENUM);
const _: () = assert!(core::mem::size_of::<ActionKey>() == C_ENUM);
const _: () = assert!(core::mem::size_of::<NewSurfaceContext>() == C_ENUM);
// `ColorScheme` (Zig `enum(u2)`) and `ClipboardRequestType` (Zig `enum(u8)`) are
// the two narrow-backed enums: the port models them `#[repr(u8)]` (1 byte),
// faithful to the Zig source of truth. They never cross the ABI *as the enum
// type itself* — `ghostty_app_set_color_scheme` takes a raw `c_int` arg that is
// `intToEnum`-decoded (lib.rs:547), and the header's int-wide `ghostty_color_scheme_e`
// / `ghostty_clipboard_request_e` typedefs are preserved verbatim by the header
// byte-diff gate. So we pin the 1-byte Rust repr + the load-bearing ordinals.
const _: () = assert!(core::mem::size_of::<ColorScheme>() == 1);
const _: () = assert!(core::mem::size_of::<ClipboardRequestType>() == 1);

// ── Pinned enum ORDINALS (the values the header's C enumerators produce) ──
// ghostty_platform_e: INVALID=0 (reserved), MACOS=1, IOS=2 — the Rust enum omits
// the 0 arm so `intToEnum(0)` rejects (I10); assert the two live ordinals.
const _: () = assert!(PlatformTag::MacOS as c_int == 1);
const _: () = assert!(PlatformTag::IOS as c_int == 2);
// ghostty_point_tag_e (R-A: the C name at ordinal 3 is `GHOSTTY_POINT_SURFACE`,
// the Zig/core arm is `history` — same ordinal, header name preserved verbatim).
const _: () = assert!(PointTag::Active as c_int == 0);
const _: () = assert!(PointTag::Viewport as c_int == 1);
const _: () = assert!(PointTag::Screen as c_int == 2);
const _: () = assert!(PointTag::History as c_int == 3);
// ghostty_point_coord_e
const _: () = assert!(PointCoordTag::Exact as c_int == 0);
const _: () = assert!(PointCoordTag::TopLeft as c_int == 1);
const _: () = assert!(PointCoordTag::BottomRight as c_int == 2);
// ghostty_color_scheme_e
const _: () = assert!(ColorScheme::Light as u8 == 0);
const _: () = assert!(ColorScheme::Dark as u8 == 1);
// ghostty_clipboard_request_e
const _: () = assert!(ClipboardRequestType::Paste as u8 == 0);
const _: () = assert!(ClipboardRequestType::Osc52Read as u8 == 1);
const _: () = assert!(ClipboardRequestType::Osc52Write as u8 == 2);
// ghostty_target_tag_e
const _: () = assert!(TargetKey::App as c_int == 0);
const _: () = assert!(TargetKey::Surface as c_int == 1);
// ghostty_surface_context_e (SurfaceOptions.context)
const _: () = assert!(NewSurfaceContext::Window as c_int == 0);
const _: () = assert!(NewSurfaceContext::Tab as c_int == 1);
const _: () = assert!(NewSurfaceContext::Split as c_int == 2);
// ghostty_action_tag_e — positional. The first and last ordinals bracket the
// append-only ABI (action.zig:65-77 / ghostty.h:885-952); the exhaustive
// positional map is checked in `action_key_positional_ordinals` below.
const _: () = assert!(ActionKey::Quit as c_int == 0);
const _: () = assert!(ActionKey::CopyTitleToClipboard as c_int == 65);

// ── Info.BuildMode (ghostty_info_s / ghostty_build_mode_e, ghostty.h:385-396) ──
// DEFERRED: `ghostty_info()` and its `Info`/`build_mode` types are NOT yet ported
// into `rustty-apprt-c` (the app export set currently omits `rustty_info`), so
// there is no Rust `#[repr(C)]` type to `const`-assert here. The header side —
// `ghostty_info_s` {enum build_mode@0(4→pad), const char* version@8(8),
// uintptr_t version_len@16(8)} = 24B align-8, and `ghostty_build_mode_e`
// {DEBUG=0, RELEASE_SAFE=1, RELEASE_FAST=2, RELEASE_SMALL=3} — is frozen by the
// `check_ghostty_h_abi.sh` byte-diff against the pinned upstream header. When the
// port lands `rustty_info`, add its size/align/ordinal asserts here.

// ============================================================================
// Runtime re-checks (readable diagnostics + a runnable test target).
// ============================================================================

/// The full positional `ghostty_action_tag_e` → `ActionKey` map (ghostty.h:885-952).
/// This is the ABI contract: actions are append-only, and each key's ordinal is
/// its position. A reorder/insertion anywhere shifts every later ordinal and this
/// exhaustive check names the first divergence.
#[rustfmt::skip]
const ACTION_KEY_ORDINALS: &[(ActionKey, c_int)] = &[
    (ActionKey::Quit, 0),
    (ActionKey::NewWindow, 1),
    (ActionKey::NewTab, 2),
    (ActionKey::CloseTab, 3),
    (ActionKey::NewSplit, 4),
    (ActionKey::CloseAllWindows, 5),
    (ActionKey::ToggleMaximize, 6),
    (ActionKey::ToggleFullscreen, 7),
    (ActionKey::ToggleTabOverview, 8),
    (ActionKey::ToggleWindowDecorations, 9),
    (ActionKey::ToggleQuickTerminal, 10),
    (ActionKey::ToggleCommandPalette, 11),
    (ActionKey::ToggleVisibility, 12),
    (ActionKey::ToggleBackgroundOpacity, 13),
    (ActionKey::MoveTab, 14),
    (ActionKey::GotoTab, 15),
    (ActionKey::GotoSplit, 16),
    (ActionKey::GotoWindow, 17),
    (ActionKey::ResizeSplit, 18),
    (ActionKey::EqualizeSplits, 19),
    (ActionKey::ToggleSplitZoom, 20),
    (ActionKey::PresentTerminal, 21),
    (ActionKey::SizeLimit, 22),
    (ActionKey::ResetWindowSize, 23),
    (ActionKey::InitialSize, 24),
    (ActionKey::CellSize, 25),
    (ActionKey::Scrollbar, 26),
    (ActionKey::Render, 27),
    (ActionKey::Inspector, 28),
    (ActionKey::ShowGtkInspector, 29),
    (ActionKey::RenderInspector, 30),
    (ActionKey::DesktopNotification, 31),
    (ActionKey::SetTitle, 32),
    (ActionKey::SetTabTitle, 33),
    (ActionKey::PromptTitle, 34),
    (ActionKey::Pwd, 35),
    (ActionKey::MouseShape, 36),
    (ActionKey::MouseVisibility, 37),
    (ActionKey::MouseOverLink, 38),
    (ActionKey::RendererHealth, 39),
    (ActionKey::OpenConfig, 40),
    (ActionKey::QuitTimer, 41),
    (ActionKey::FloatWindow, 42),
    (ActionKey::SecureInput, 43),
    (ActionKey::KeySequence, 44),
    (ActionKey::KeyTable, 45),
    (ActionKey::ColorChange, 46),
    (ActionKey::ReloadConfig, 47),
    (ActionKey::ConfigChange, 48),
    (ActionKey::CloseWindow, 49),
    (ActionKey::RingBell, 50),
    (ActionKey::SelectionChanged, 51),
    (ActionKey::Undo, 52),
    (ActionKey::Redo, 53),
    (ActionKey::CheckForUpdates, 54),
    (ActionKey::OpenUrl, 55),
    (ActionKey::ShowChildExited, 56),
    (ActionKey::ProgressReport, 57),
    (ActionKey::ShowOnScreenKeyboard, 58),
    (ActionKey::CommandFinished, 59),
    (ActionKey::StartSearch, 60),
    (ActionKey::EndSearch, 61),
    (ActionKey::SearchTotal, 62),
    (ActionKey::SearchSelected, 63),
    (ActionKey::Readonly, 64),
    (ActionKey::CopyTitleToClipboard, 65),
];

#[test]
fn struct_sizes_and_aligns() {
    assert_eq!(
        core::mem::size_of::<Options>(),
        64,
        "ghostty_runtime_config_s size"
    );
    assert_eq!(
        core::mem::align_of::<Options>(),
        8,
        "ghostty_runtime_config_s align"
    );
    assert_eq!(
        core::mem::size_of::<SurfaceOptions>(),
        88,
        "ghostty_surface_config_s size"
    );
    assert_eq!(
        core::mem::align_of::<SurfaceOptions>(),
        8,
        "ghostty_surface_config_s align"
    );
    assert_eq!(
        core::mem::size_of::<PlatformC>(),
        PTR,
        "ghostty_platform_u size"
    );
    assert_eq!(
        core::mem::size_of::<PlatformCMacOS>(),
        PTR,
        "ghostty_platform_macos_s size"
    );
    assert_eq!(
        core::mem::size_of::<PlatformCIOS>(),
        PTR,
        "ghostty_platform_ios_s size"
    );
    assert_eq!(core::mem::size_of::<EnvVar>(), 16, "ghostty_env_var_s size");
    assert_eq!(
        core::mem::size_of::<ClipboardContent>(),
        16,
        "ghostty_clipboard_content_s size"
    );
    assert_eq!(
        core::mem::size_of::<rustty_apprt_c::capi::Text>(),
        40,
        "ghostty_text_s size"
    );
    assert_eq!(
        core::mem::size_of::<GhosttyString>(),
        24,
        "ghostty_string_s size"
    );
    assert_eq!(
        core::mem::align_of::<GhosttyString>(),
        8,
        "ghostty_string_s align"
    );
    assert_eq!(
        core::mem::size_of::<SurfaceSize>(),
        20,
        "ghostty_surface_size_s size"
    );
    assert_eq!(core::mem::size_of::<Point>(), 16, "ghostty_point_s size");
    assert_eq!(
        core::mem::size_of::<Selection>(),
        36,
        "ghostty_selection_s size"
    );
    assert_eq!(
        core::mem::size_of::<KeyEvent>(),
        32,
        "ghostty_input_key_s wire size"
    );
    assert_eq!(
        core::mem::size_of::<TargetCValue>(),
        PTR,
        "ghostty_target_u size"
    );
    assert_eq!(core::mem::size_of::<TargetC>(), 16, "ghostty_target_s size");
    assert_eq!(
        core::mem::size_of::<ActionCValue>(),
        24,
        "ghostty_action_u size"
    );
    assert_eq!(core::mem::size_of::<ActionC>(), 32, "ghostty_action_s size");
}

#[test]
fn enum_widths() {
    assert_eq!(
        core::mem::size_of::<PlatformTag>(),
        C_ENUM,
        "ghostty_platform_e width"
    );
    assert_eq!(
        core::mem::size_of::<PointTag>(),
        C_ENUM,
        "ghostty_point_tag_e width"
    );
    assert_eq!(
        core::mem::size_of::<PointCoordTag>(),
        C_ENUM,
        "ghostty_point_coord_e width"
    );
    assert_eq!(
        core::mem::size_of::<TargetKey>(),
        C_ENUM,
        "ghostty_target_tag_e width"
    );
    assert_eq!(
        core::mem::size_of::<ActionKey>(),
        C_ENUM,
        "ghostty_action_tag_e width"
    );
    assert_eq!(
        core::mem::size_of::<NewSurfaceContext>(),
        C_ENUM,
        "ghostty_surface_context_e width"
    );
    // Zig-narrow-backed enums (enum(u2)/enum(u8)) → 1-byte Rust repr.
    assert_eq!(core::mem::size_of::<ColorScheme>(), 1, "ColorScheme repr");
    assert_eq!(
        core::mem::size_of::<ClipboardRequestType>(),
        1,
        "ClipboardRequestType repr"
    );
}

#[test]
fn pinned_enum_ordinals() {
    // ghostty_platform_e (0=invalid reserved).
    assert_eq!(PlatformTag::MacOS as c_int, 1, "GHOSTTY_PLATFORM_MACOS");
    assert_eq!(PlatformTag::IOS as c_int, 2, "GHOSTTY_PLATFORM_IOS");
    // ghostty_point_tag_e.
    assert_eq!(PointTag::Active as c_int, 0, "GHOSTTY_POINT_ACTIVE");
    assert_eq!(PointTag::Viewport as c_int, 1, "GHOSTTY_POINT_VIEWPORT");
    assert_eq!(PointTag::Screen as c_int, 2, "GHOSTTY_POINT_SCREEN");
    assert_eq!(
        PointTag::History as c_int,
        3,
        "GHOSTTY_POINT_SURFACE (R-A: history)"
    );
    // ghostty_point_coord_e.
    assert_eq!(
        PointCoordTag::Exact as c_int,
        0,
        "GHOSTTY_POINT_COORD_EXACT"
    );
    assert_eq!(
        PointCoordTag::TopLeft as c_int,
        1,
        "GHOSTTY_POINT_COORD_TOP_LEFT"
    );
    assert_eq!(
        PointCoordTag::BottomRight as c_int,
        2,
        "GHOSTTY_POINT_COORD_BOTTOM_RIGHT"
    );
    // ghostty_color_scheme_e.
    assert_eq!(ColorScheme::Light as u8, 0, "GHOSTTY_COLOR_SCHEME_LIGHT");
    assert_eq!(ColorScheme::Dark as u8, 1, "GHOSTTY_COLOR_SCHEME_DARK");
    // ghostty_clipboard_request_e.
    assert_eq!(
        ClipboardRequestType::Paste as u8,
        0,
        "GHOSTTY_CLIPBOARD_REQUEST_PASTE"
    );
    assert_eq!(
        ClipboardRequestType::Osc52Read as u8,
        1,
        "GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ"
    );
    assert_eq!(
        ClipboardRequestType::Osc52Write as u8,
        2,
        "GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE"
    );
    // ghostty_target_tag_e.
    assert_eq!(TargetKey::App as c_int, 0, "GHOSTTY_TARGET_APP");
    assert_eq!(TargetKey::Surface as c_int, 1, "GHOSTTY_TARGET_SURFACE");
    // ghostty_surface_context_e.
    assert_eq!(
        NewSurfaceContext::Window as c_int,
        0,
        "GHOSTTY_SURFACE_CONTEXT_WINDOW"
    );
    assert_eq!(
        NewSurfaceContext::Tab as c_int,
        1,
        "GHOSTTY_SURFACE_CONTEXT_TAB"
    );
    assert_eq!(
        NewSurfaceContext::Split as c_int,
        2,
        "GHOSTTY_SURFACE_CONTEXT_SPLIT"
    );
}

#[test]
fn action_key_positional_ordinals() {
    // The last enumerator's ordinal doubles as the variant count − 1; pinning it
    // catches an append that forgets to update the header (and vice-versa).
    assert_eq!(
        ACTION_KEY_ORDINALS.len(),
        66,
        "ghostty_action_tag_e cardinality"
    );
    for &(key, ord) in ACTION_KEY_ORDINALS {
        assert_eq!(
            key as c_int, ord,
            "ghostty_action_tag_e positional ordinal for {key:?}"
        );
    }
    assert_eq!(ActionKey::Quit as c_int, 0, "GHOSTTY_ACTION_QUIT");
    assert_eq!(
        ActionKey::CopyTitleToClipboard as c_int,
        65,
        "GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD (append-only ABI tail)"
    );
}
