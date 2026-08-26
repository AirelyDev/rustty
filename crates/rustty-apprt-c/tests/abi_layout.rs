//! abi_layout.rs — the `include/ghostty.h` ABI-layout freeze gate (Task A3, I9).
//!
//! Branch A (PREPORT_SWIFT_APP §3): the reused Swift app `#import`s the UPSTREAM
//! `include/ghostty.h` verbatim, and every `ghostty_*_s`/`_u`/`_e` typedef must
//! resolve to a rustty `#[repr(C)]` type with byte-identical layout. This test is
//! the Rust half of that freeze: for every ABI struct/union/enum in the header it
//! asserts `size_of`/`align_of` and each pinned enum ordinal equals the value the
//! header's C declaration produces on the LP64 macOS/iOS ABI. The `.port` gate
//! `check_rustty_h_abi.sh` is the complementary half: it verifies rustty-embed.h
//! is the pinned upstream oracle token-renamed (via ORACLE_HEADER_PATH + a
//! SHA-256 freeze-pin; preserving the R-A/R-B/R-C typos).
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
//! `/Users/gael/projects/ghostty/include/ghostty.h`, the pinned upstream oracle) and
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
    ClipboardComplete, ClipboardConfirm, ClipboardContent, EnvVar, Options, PlatformC,
    PlatformCIOS, PlatformCMacOS, PlatformTag,
    SurfaceOptions,
};
use rustty_apprt_api::action::{
    ActionC, ActionCValue, ActionKey, TargetC, TargetCValue, TargetKey,
};
use rustty_apprt_api::structs::{ClipboardRequestType, ColorScheme};
use rustty_apprt_api::surface::NewSurfaceContext;
use rustty_apprt_c::capi::{KeyEvent, Point, PointCoordTag, PointTag, Selection, SurfaceSize};
use rustty_apprt_c::String as GhosttyString;
use rustty_apprt_c::{BuildMode, Info};

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
// @embed-abi rustty_runtime_config_s 64
const _: () = assert!(core::mem::size_of::<Options>() == 64);
const _: () = assert!(core::mem::align_of::<Options>() == 8);

// ── ghostty_surface_config_s (Surface.Options, ghostty.h:467-480) ──
// platform_tag(enum)@0(4→pad); platform(union)@8(8); userdata@16(8);
// scale_factor(double)@24(8); font_size(float)@32(4→pad); working_directory@40(8);
// command@48(8); env_vars@56(8); env_var_count(size_t)@64(8); initial_input@72(8);
// wait_after_command(bool)@80(1→pad); context(enum)@84(4) → size 88, align 8.
// @embed-abi rustty_surface_config_s 88
const _: () = assert!(core::mem::size_of::<SurfaceOptions>() == 88);
const _: () = assert!(core::mem::align_of::<SurfaceOptions>() == 8);
// @embed-abi-field rustty_surface_config_s platform_tag 0
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, platform_tag) == 0);
// @embed-abi-field rustty_surface_config_s platform 8
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, platform) == 8);
// @embed-abi-field rustty_surface_config_s userdata 16
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, userdata) == 16);
// @embed-abi-field rustty_surface_config_s scale_factor 24
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, scale_factor) == 24);
// @embed-abi-field rustty_surface_config_s font_size 32
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, font_size) == 32);
// @embed-abi-field rustty_surface_config_s working_directory 40
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, working_directory) == 40);
// @embed-abi-field rustty_surface_config_s command 48
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, command) == 48);
// @embed-abi-field rustty_surface_config_s env_vars 56
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, env_vars) == 56);
// @embed-abi-field rustty_surface_config_s env_var_count 64
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, env_var_count) == 64);
// @embed-abi-field rustty_surface_config_s initial_input 72
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, initial_input) == 72);
// @embed-abi-field rustty_surface_config_s wait_after_command 80
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, wait_after_command) == 80);
// @embed-abi-field rustty_surface_config_s context 84
const _: () = assert!(core::mem::offset_of!(SurfaceOptions, context) == 84);

// ── ghostty_platform_u + macos/ios arms (ghostty.h:448-459) ──
// Each arm is a single `void*`; the union is exactly pointer-sized.
const _: () = assert!(core::mem::size_of::<PlatformC>() == PTR);
const _: () = assert!(core::mem::align_of::<PlatformC>() == PTR);
const _: () = assert!(core::mem::size_of::<PlatformCMacOS>() == PTR);
const _: () = assert!(core::mem::size_of::<PlatformCIOS>() == PTR);

// ── ghostty_env_var_s (ghostty.h:443-446) ── {const char* key, value} → 16, align 8.
// @embed-abi rustty_env_var_s 16
const _: () = assert!(core::mem::size_of::<EnvVar>() == 16);
const _: () = assert!(core::mem::align_of::<EnvVar>() == 8);

// ── ghostty_clipboard_content_s ── {const char* mime; const uint8_t* data;
// size_t len} → 24 (8c7a34d4c: binary-safe data, the E2 LAYOUT change).
// @embed-abi rustty_clipboard_content_s 24
const _: () = assert!(core::mem::size_of::<ClipboardContent>() == 24);
const _: () = assert!(core::mem::align_of::<ClipboardContent>() == 8);
// @embed-abi-field rustty_clipboard_content_s mime 0
const _: () = assert!(core::mem::offset_of!(ClipboardContent, mime) == 0);
// @embed-abi-field rustty_clipboard_content_s data 8
const _: () = assert!(core::mem::offset_of!(ClipboardContent, data) == 8);
// @embed-abi-field rustty_clipboard_content_s len 16
const _: () = assert!(core::mem::offset_of!(ClipboardContent, len) == 16);

// ── ghostty_clipboard_complete_s ── {ptr, size_t, ptr, size_t, bool, bool} → 40.
// @embed-abi rustty_clipboard_complete_s 40
const _: () = assert!(core::mem::size_of::<ClipboardComplete>() == 40);
const _: () = assert!(core::mem::align_of::<ClipboardComplete>() == 8);
// @embed-abi-field rustty_clipboard_complete_s confirmed 32
const _: () = assert!(core::mem::offset_of!(ClipboardComplete, confirmed) == 32);
// @embed-abi-field rustty_clipboard_complete_s remember 33
const _: () = assert!(core::mem::offset_of!(ClipboardComplete, remember) == 33);

// ── ghostty_clipboard_confirm_s ── {ptr, size_t, ptr, size_t, const char*, bool} → 48.
// @embed-abi rustty_clipboard_confirm_s 48
const _: () = assert!(core::mem::size_of::<ClipboardConfirm>() == 48);
const _: () = assert!(core::mem::align_of::<ClipboardConfirm>() == 8);
// @embed-abi-field rustty_clipboard_confirm_s name 32
const _: () = assert!(core::mem::offset_of!(ClipboardConfirm, name) == 32);
// @embed-abi-field rustty_clipboard_confirm_s can_remember 40
const _: () = assert!(core::mem::offset_of!(ClipboardConfirm, can_remember) == 40);

// ── ghostty_text_s (ghostty.h:408-415) ──
// double tl_px_x@0(8); double tl_px_y@8(8); uint32_t offset_start@16(4);
// uint32_t offset_len@20(4); const char* text@24(8); uintptr_t text_len@32(8)
// → size 40, align 8.
// @embed-abi rustty_text_s 40
const _: () = assert!(core::mem::size_of::<rustty_apprt_c::capi::Text>() == 40);
const _: () = assert!(core::mem::align_of::<rustty_apprt_c::capi::Text>() == 8);

// ── ghostty_string_s (main_c.zig / ghostty.h:402-406) ──
// const char* ptr@0(8); uintptr_t len@8(8); bool sentinel@16(1→pad) → size 24, align 8.
// @embed-abi rustty_string_s 24
const _: () = assert!(core::mem::size_of::<GhosttyString>() == 24);
const _: () = assert!(core::mem::align_of::<GhosttyString>() == 8);
// @embed-abi-field rustty_string_s ptr 0
const _: () = assert!(core::mem::offset_of!(GhosttyString, ptr) == 0);
// @embed-abi-field rustty_string_s len 8
const _: () = assert!(core::mem::offset_of!(GhosttyString, len) == 8);
// @embed-abi-field rustty_string_s sentinel 16
const _: () = assert!(core::mem::offset_of!(GhosttyString, sentinel) == 16);

// ── ghostty_surface_size_s (ghostty.h:482-489) ──
// uint16_t columns,rows; uint32_t width_px,height_px,cell_width_px,cell_height_px
// → size 20, align 4.
// @embed-abi rustty_surface_size_s 20
const _: () = assert!(core::mem::size_of::<SurfaceSize>() == 20);
const _: () = assert!(core::mem::align_of::<SurfaceSize>() == 4);
// @embed-abi-field rustty_surface_size_s columns 0
const _: () = assert!(core::mem::offset_of!(SurfaceSize, columns) == 0);
// @embed-abi-field rustty_surface_size_s rows 2
const _: () = assert!(core::mem::offset_of!(SurfaceSize, rows) == 2);
// @embed-abi-field rustty_surface_size_s width_px 4
const _: () = assert!(core::mem::offset_of!(SurfaceSize, width_px) == 4);
// @embed-abi-field rustty_surface_size_s height_px 8
const _: () = assert!(core::mem::offset_of!(SurfaceSize, height_px) == 8);
// @embed-abi-field rustty_surface_size_s cell_width_px 12
const _: () = assert!(core::mem::offset_of!(SurfaceSize, cell_width_px) == 12);
// @embed-abi-field rustty_surface_size_s cell_height_px 16
const _: () = assert!(core::mem::offset_of!(SurfaceSize, cell_height_px) == 16);

// ── ghostty_point_s (ghostty.h:429-435) ── {enum tag, enum coord, u32 x, u32 y} → 16, align 4.
// @embed-abi rustty_point_s 16
const _: () = assert!(core::mem::size_of::<Point>() == 16);
const _: () = assert!(core::mem::align_of::<Point>() == 4);

// ── ghostty_selection_s (ghostty.h:437-441) ── {Point tl@0(16), Point br@16(16), bool@32} → 36, align 4.
// @embed-abi rustty_selection_s 36
const _: () = assert!(core::mem::size_of::<Selection>() == 36);
const _: () = assert!(core::mem::align_of::<Selection>() == 4);

// ── ghostty_input_key_s wire form (KeyEvent, embedded.zig:1248) ──
// action@0(4) mods@4(4) consumed_mods@8(4) keycode@12(4) text@16(8)
// unshifted_codepoint@24(4) composing@28(1→pad) → size 32, align 8.
// @embed-abi rustty_input_key_s 32
const _: () = assert!(core::mem::size_of::<KeyEvent>() == 32);
const _: () = assert!(core::mem::align_of::<KeyEvent>() == 8);

// ── ghostty_target_s / _u (ghostty.h:555-567) ──
// union{ ghostty_surface_t } → one pointer (8). struct{ enum key@0(4→pad), union@8(8) } → 16.
const _: () = assert!(core::mem::size_of::<TargetCValue>() == PTR);
// @embed-abi rustty_target_s 16
const _: () = assert!(core::mem::size_of::<TargetC>() == 16);
const _: () = assert!(core::mem::align_of::<TargetC>() == 8);

// ── ghostty_action_s / _u (ghostty.h:955-998) ──
// The action value union's WIDEST arm is 24 bytes (e.g. open_url: {kind:i32,
// url:*const u8, len:usize}); struct{ enum key@0(4→pad), union@8(24) } → 32.
// NOTE: byte-exact parity of the value-union INTERIOR against the header's
// per-arm C structs is deferred to the embedded C-API milestone (see the
// `ActionCValue` note in apprt-api `action.rs`); this pins the Rust layout that
// the action_cb ships by value, which is what the Swift boundary reads.
// @embed-abi rustty_action_u 24
const _: () = assert!(core::mem::size_of::<ActionCValue>() == 24);
// @embed-abi rustty_action_s 32
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
// positional ABI (upstream reserves the right to renumber: this range's
// `ad96613a8`/`8b7c57c75` MID-ENUM inserts shifted every later tag, and
// `25b2d8a38` appended the new tail); the exhaustive positional map is
// checked in `action_key_positional_ordinals` below.
const _: () = assert!(ActionKey::Quit as c_int == 0);
const _: () = assert!(ActionKey::MoveTabToNewWindow as c_int == 68);

// ── Info.BuildMode (ghostty_info_s / ghostty_build_mode_e, ghostty.h:385-396) ──
// `rustty_info()` and its `Info`/`BuildMode` types ARE ported (lib.rs:945-1040), so
// the header side — `ghostty_info_s` {enum build_mode@0(4→pad), const char*
// version@8(8), uintptr_t version_len@16(8)} = 24B align-8, and
// `ghostty_build_mode_e` {DEBUG=0, RELEASE_SAFE=1, RELEASE_FAST=2,
// RELEASE_SMALL=3}, frozen by the `check_rustty_h_abi.sh` byte-diff against the
// pinned upstream header — is pinned on the Rust side here. (`Info`'s size/align
// and field offsets are ALSO const-asserted at their definition site with
// `@embed-abi` annotations that `check_embed_h_abi.sh` cross-checks against the
// clang-probed header; the `BuildMode` ordinals/width below are pinned ONLY here.)
const _: () = assert!(core::mem::size_of::<Info>() == 24);
const _: () = assert!(core::mem::align_of::<Info>() == 8);
const _: () = assert!(core::mem::offset_of!(Info, mode) == 0);
const _: () = assert!(core::mem::offset_of!(Info, version) == 8);
const _: () = assert!(core::mem::offset_of!(Info, version_len) == 16);
const _: () = assert!(core::mem::size_of::<BuildMode>() == C_ENUM);
const _: () = assert!(BuildMode::Debug as c_int == 0);
const _: () = assert!(BuildMode::ReleaseSafe as c_int == 1);
const _: () = assert!(BuildMode::ReleaseFast as c_int == 2);
const _: () = assert!(BuildMode::ReleaseSmall as c_int == 3);

// ============================================================================
// Runtime re-checks (readable diagnostics + a runnable test target).
// ============================================================================

/// The full positional `ghostty_action_tag_e` → `ActionKey` map (rustty-embed.h,
/// mirroring the pinned ghostty.h). This is the ABI contract: each key's ordinal
/// is its position. A reorder/insertion anywhere shifts every later ordinal and
/// this exhaustive check names the first divergence. (Upstream does NOT promise
/// append-only here — this range's `ad96613a8`/`8b7c57c75` inserted
/// `export_terminal_io`/`set_window_title` MID-ENUM, renumbering tags 31+ — so
/// the map must be regenerated from the header whenever it advances.)
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
    (ActionKey::ExportTerminalIO, 31),
    (ActionKey::DesktopNotification, 32),
    (ActionKey::SetTitle, 33),
    (ActionKey::SetTabTitle, 34),
    (ActionKey::SetWindowTitle, 35),
    (ActionKey::PromptTitle, 36),
    (ActionKey::Pwd, 37),
    (ActionKey::MouseShape, 38),
    (ActionKey::MouseVisibility, 39),
    (ActionKey::MouseOverLink, 40),
    (ActionKey::RendererHealth, 41),
    (ActionKey::OpenConfig, 42),
    (ActionKey::QuitTimer, 43),
    (ActionKey::FloatWindow, 44),
    (ActionKey::SecureInput, 45),
    (ActionKey::KeySequence, 46),
    (ActionKey::KeyTable, 47),
    (ActionKey::ColorChange, 48),
    (ActionKey::ReloadConfig, 49),
    (ActionKey::ConfigChange, 50),
    (ActionKey::CloseWindow, 51),
    (ActionKey::RingBell, 52),
    (ActionKey::SelectionChanged, 53),
    (ActionKey::Undo, 54),
    (ActionKey::Redo, 55),
    (ActionKey::CheckForUpdates, 56),
    (ActionKey::OpenUrl, 57),
    (ActionKey::ShowChildExited, 58),
    (ActionKey::ProgressReport, 59),
    (ActionKey::ShowOnScreenKeyboard, 60),
    (ActionKey::CommandFinished, 61),
    (ActionKey::StartSearch, 62),
    (ActionKey::EndSearch, 63),
    (ActionKey::SearchTotal, 64),
    (ActionKey::SearchSelected, 65),
    (ActionKey::Readonly, 66),
    (ActionKey::CopyTitleToClipboard, 67),
    (ActionKey::MoveTabToNewWindow, 68),
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
    // (`8c7a34d4c` LAYOUT CHANGE: + `size_t len`.)
    assert_eq!(
        core::mem::size_of::<ClipboardContent>(),
        24,
        "ghostty_clipboard_content_s size"
    );
    assert_eq!(
        core::mem::size_of::<ClipboardComplete>(),
        40,
        "ghostty_clipboard_complete_s size"
    );
    assert_eq!(
        core::mem::size_of::<ClipboardConfirm>(),
        48,
        "ghostty_clipboard_confirm_s size"
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
    assert_eq!(core::mem::size_of::<Info>(), 24, "ghostty_info_s size");
    assert_eq!(core::mem::align_of::<Info>(), 8, "ghostty_info_s align");
    assert_eq!(
        core::mem::offset_of!(Info, mode),
        0,
        "ghostty_info_s.build_mode offset"
    );
    assert_eq!(
        core::mem::offset_of!(Info, version),
        8,
        "ghostty_info_s.version offset"
    );
    assert_eq!(
        core::mem::offset_of!(Info, version_len),
        16,
        "ghostty_info_s.version_len offset"
    );
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
    assert_eq!(
        core::mem::size_of::<BuildMode>(),
        C_ENUM,
        "ghostty_build_mode_e width"
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
    assert_eq!(
        ClipboardRequestType::KittyRead as u8,
        3,
        "GHOSTTY_CLIPBOARD_REQUEST_KITTY_READ"
    );
    assert_eq!(
        ClipboardRequestType::KittyWrite as u8,
        4,
        "GHOSTTY_CLIPBOARD_REQUEST_KITTY_WRITE"
    );
    assert_eq!(
        ClipboardRequestType::List as u8,
        5,
        "GHOSTTY_CLIPBOARD_REQUEST_LIST"
    );
    // ghostty_clipboard_read_result_e.
    assert_eq!(
        rustty_apprt_api::ClipboardReadResult::Started as c_int,
        0,
        "GHOSTTY_CLIPBOARD_READ_STARTED"
    );
    assert_eq!(
        rustty_apprt_api::ClipboardReadResult::Unavailable as c_int,
        1,
        "GHOSTTY_CLIPBOARD_READ_UNAVAILABLE"
    );
    assert_eq!(
        rustty_apprt_api::ClipboardReadResult::Unsupported as c_int,
        2,
        "GHOSTTY_CLIPBOARD_READ_UNSUPPORTED"
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
    // ghostty_build_mode_e (ghostty.h:385-390). The four ordinals are the ABI: a
    // Rust-side variant reorder would silently mis-report the build mode to the
    // host (the header is frozen by the byte-diff gate and would NOT catch it).
    assert_eq!(BuildMode::Debug as c_int, 0, "GHOSTTY_BUILD_MODE_DEBUG");
    assert_eq!(
        BuildMode::ReleaseSafe as c_int,
        1,
        "GHOSTTY_BUILD_MODE_RELEASE_SAFE"
    );
    assert_eq!(
        BuildMode::ReleaseFast as c_int,
        2,
        "GHOSTTY_BUILD_MODE_RELEASE_FAST"
    );
    assert_eq!(
        BuildMode::ReleaseSmall as c_int,
        3,
        "GHOSTTY_BUILD_MODE_RELEASE_SMALL"
    );
}

#[test]
fn action_key_positional_ordinals() {
    // The last enumerator's ordinal doubles as the variant count − 1; pinning it
    // catches an append that forgets to update the header (and vice-versa).
    assert_eq!(
        ACTION_KEY_ORDINALS.len(),
        69,
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
        ActionKey::MoveTabToNewWindow as c_int,
        68,
        "GHOSTTY_ACTION_MOVE_TAB_TO_NEW_WINDOW (positional ABI tail)"
    );
}
