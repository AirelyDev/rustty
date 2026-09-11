//! rustty-apprt-c
//!
//! PORTS: the libghostty embedding C API — `src/apprt/embedded.zig`'s `CAPI`
//! block + the `src/main_c.zig` C API hub (the `ghostty_*` `@export` surface for
//! `apprt.embedded`). The Rust backends in `rustty-apprt` (the `embedded`/`none`/
//! `browser` modules) are the source of truth; this crate is a pure FFI
//! projection (R3). cdylib+staticlib+rlib; cbindgen -> include/rustty-apprt.h
//! with `rustty_*` symbols (R2).
//!
//! PORT STATUS: in-progress. The crate compiles/links as a cdylib/staticlib/rlib
//! and exports the global-state precursor `rustty_init`. E5 landed the CAPI extern
//! structs (`capi.rs`) with their D2 const-asserts. E6 lands the 13 app-lifecycle +
//! app/config exports (`rustty_app_new`/`_tick`/`_userdata`/`_free`/`_set_focus`/
//! `_key`/`_keyboard_changed`/`_open_config`/`_update_config`/`_needs_confirm_quit`/
//! `_has_global_keybinds`/`_set_color_scheme`, `rustty_config_key_is_binding`) — the
//! runtime-exercisable core (EMBEDDED_BLUEPRINT §1: no renderer/xev leaf). E7 lands
//! the surface lifecycle + state/query + render/window exports
//! (`rustty_surface_config_new`/`_new`/`_free`/`_userdata`/`_app`/`_inherited_config`/
//! `_update_config`/`_needs_confirm_quit`/`_process_exited`/`_has_selection`/
//! `_read_selection`/`_read_text`/`_free_text`/`_size`/`_foreground_pid`/`_tty_name`/
//! `_refresh`/`_draw`/`_set_size`/`_set_color_scheme`/`_set_content_scale`/
//! `_set_focus`/`_set_occlusion`) + the `main_c.zig` `String`/`rustty_string_free`
//! precursor `_tty_name` returns. Only `_config_new` is runtime-live; every other E7
//! export needs a live surface, so it reaches the Phase-3 GPU leaf past
//! `rustty_surface_new` (compiles + links now, panics when driven). E8 lands the
//! input exports (key/text/preedit/mouse); E9 lands the surface-action exports
//! (`_request_close`/`_split`/`_split_focus`/`_split_resize`/`_split_equalize`/
//! `_binding_action`), the `_complete_clipboard_request` sink, and the 10 inspector
//! exports (`_surface_inspector`/`_inspector_free`/`_set_size`/`_set_content_scale`/
//! `_mouse_button`/`_mouse_pos`/`_mouse_scroll`/`_key`/`_text`/`_set_focus`). All of
//! E8/E9 need a live surface (and the per-inspector sinks a live inspector, whose
//! dcimgui `Inspector::init` is LIVE as of WS6). E10
//! lands the misc + Darwin-gated tail — `rustty_set_window_background_blur` (a macOS
//! objc/CGS leaf; non-macOS no-op) plus the six isDarwin-gated
//! (`#[cfg(any(target_os = "macos", target_os = "ios"))]`) Darwin
//! exports (`_surface_set_display_id`/`_quicklook_font`/`_quicklook_word`/
//! `_inspector_metal_init`/`_metal_render`/`_metal_shutdown`): renderer/xev/coretext/
//! objc/Metal seams (the metal trio delegate to the E4 backend, LIVE as of WS6). With
//! E10 ALL 71 `ghostty_*` (`rustty_*`) exports exist (71 == 71 vs the reference).
//! cbindgen header generation + the C compile+link smoke KAT land in E11
//! (EMBEDDED_BLUEPRINT §3/§6/§7).
//!
//! PORT NOTE: this is the crate root, the rustty analog of `src/main_c.zig` (the
//! C API hub) together with `apprt.embedded`'s `CAPI` decl. `main_c.zig`
//! `@export`s the C API symbols and references `apprt.runtime.CAPI`; in rustty
//! each C API symbol is authored as a `#[no_mangle] pub unsafe extern "C" fn
//! rustty_<name>(...)` 1:1 with the reference `pub export fn ghostty_<name>` /
//! `@export(&fn, .{ .name = "ghostty_<name>" })` lines (rustty_ prefix per R2).
//! The `extern "C"` wrappers are the single point that crosses the C ABI; they
//! forward to the ported `rustty-apprt` backend.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

use core::ffi::{c_void, CStr};
use std::mem::ManuallyDrop;

/// `const log = std.log.scoped(.embedded_window)` (`embedded.zig:26`).
///
/// This crate carries the reference's `CAPI` block, which lives INSIDE
/// `embedded.zig` and therefore logs through that file's scoped alias — so the
/// scope tag here is `embedded_window`, not a name of this crate's own. Carried
/// as the `log::` facade's `target:` per `DECISION-logging.md` §6r4; the backend
/// is installed by [`rustty_init`] (`rustty_core::logging::install_app_log_backend`)
/// before any export that can log runs.
const LOG_SCOPE: &str = "embedded_window";

/// The `.default` scope — `src/main_c.zig`'s three sites call `std.log.*`
/// directly rather than through a scoped alias, so they carry NO scope tag.
/// `""` is the facade convention for it (`DECISION-logging.md` §6).
const DEFAULT_LOG_SCOPE: &str = "";

use allocator_api2::alloc::Global;

use libc::{c_char, c_int};

// The concrete embedded backend (`rustty-apprt`): `App` is the runtime `App` the
// exports drive; `Options` is its `#[repr(C)]` host-callback config; `KeyTarget`
// selects app- vs surface-scoped key events. `Surface`/`SurfaceOptions` are the
// backend surface + its `ghostty_surface_config_s` options (E7 surface lifecycle);
// `PlatformC`/`PlatformCMacOS` build the zeroed platform union for the default
// `ghostty_surface_config_new`.
use rustty_apprt::embedded::{
    App, ClipboardComplete, Inspector, KeyTarget, Options, PlatformC, PlatformCMacOS, Surface,
    SurfaceOptions,
};
// The `apprt_api::App` TRAIT (`perform_action`) the embedded `App` implements —
// imported under a distinct name so it does not shadow the concrete `App` above.
use rustty_apprt_api::App as ApprtApp;
// `Target`/`Action` (the `performAction` target+action) and `ColorScheme` (the
// `app_set_color_scheme` enum). `NewSurfaceContext` is the inherited-config context
// (`ghostty_surface_inherited_config`); `ApprtError` is the `App::newSurface`
// error the `ghostty_surface_new` path logs.
use rustty_apprt_api::{Action, ApprtError, ColorScheme, NewSurfaceContext, Target};
// The split-action value types the E9 surface-action exports pass BY VALUE across
// the C ABI (`ghostty_surface_split`/`_split_focus`/`_split_resize` take
// `apprt.action.{SplitDirection,GotoSplit,ResizeSplit.Direction}` directly). Each is
// an FFI-safe `#[repr(i32)]`/`#[repr(C)]` type (apprt-api action.rs); `ResizeSplit`
// is reassembled from its `{direction, amount}` params. (`ResizeSplit.Direction` is
// hoisted to the sibling `ResizeSplitDirection` — Rust cannot nest a type in a
// struct's value namespace.)
use rustty_apprt_api::action::{GotoSplit, ResizeSplit, ResizeSplitDirection, SplitDirection};
// `apprt.ClipboardRequest` — the heap request `ghostty_surface_complete_clipboard_request`
// receives back as `*mut ClipboardRequest` (the `Box` from `Surface::clipboard_request`, E3).
use rustty_apprt_api::ClipboardRequest;
// `ghostty_app_new`/`_update_config`/`config_key_is_binding` take a `*Config` — the
// `rustty_config::Config` the config C-ABI (`rustty_config_new`) hands the host.
use rustty_config::Config;
// The core `Text.Viewport` (the read-text viewport metadata `readTextLocked` reads
// off `dumpTextLocked`'s result).
use rustty_core::surface::TextViewport;
// The core `terminal.Selection` `readTextLocked` receives (resolved by the caller
// from the active screen's selection or the wire `Selection`).
use rustty_vt::selection::Selection as CoreSelection;
// The E8 input exports' mods/mouse wire types (all `input.*` in the reference):
// `Mods`/`ModsBacking` reinterpret the raw `c_int` mods a host passes
// (`ghostty_surface_{key_translation_mods,mouse_button,mouse_pos}`); `mouse`'s
// `ButtonState`/`Button`/`PressureStage`/`ScrollMods` are the mouse-event value
// types (`ghostty_surface_mouse_{button,scroll,pressure}`).
use rustty_vt::input::key;
use rustty_vt::input::key_mods::{Mods, ModsBacking};
use rustty_vt::input::mouse;
// `input.Binding.Flags.C` (`ghostty_surface_key_is_binding`'s out-param, written
// from `Flags.cval()`) and `input.Mods.translation` (`ghostty_surface_key_translation_mods`);
// `translation` is a `rustty-input` free fn (it needs the apprt-layer `OptionAsAlt`).
// `binding` (the module) is also brought in for `input.Binding.Action.parse` — the
// E9 `ghostty_surface_binding_action` parser; it is module-qualified (`binding::Action`)
// to avoid clashing with the apprt-api `Action` imported above.
use rustty_input::binding::{self, FlagsC};
use rustty_input::key_mods::translation;

// The raw C-API `KeyEvent` (E5, capi.rs) — the wire form `ghostty_app_key` /
// `ghostty_config_key_is_binding` receive, converted to the native form via
// `KeyEvent::keyEvent`. E7 adds `Selection`/`SurfaceSize`/`Text` — the wire structs
// the surface read/query exports consume/return.
use crate::capi::{KeyEvent, Selection, SurfaceSize, Text};

// const CoreApp = @import("../App.zig");  (embedded.zig:19)
//
// The core App instantiated at `A = embedded::App` — the FIRST concrete backend
// (EMBEDDED_BLUEPRINT §2). `app_new_` creates it (`CoreApp.create`) and the embedded
// `App` stores the stable pointer as `core_app: *CoreApp`.
type CoreApp = rustty_core::app::App<App>;
// const CoreSurface = @import("../Surface.zig");
//
// Only used to name the `C: CoreSurfaceHandle` type parameter of `perform_action`'s
// `Target::App` in `ghostty_app_open_config` (the app target carries no surface, so
// `C` is otherwise unconstrained and must be annotated).
type CoreSurface = rustty_core::surface::Surface<App>;

// The `apprt.embedded.CAPI` block's extern structs (`KeyEvent`/`SurfaceSize`/
// `Text`/`Point`/`Selection` + their D2 const-size/align-asserts). The reference
// nests these in `pub const CAPI = struct { ... }` alongside the exports; the
// exports (E6..E10) are authored as `#[no_mangle]` free fns in this crate, the
// extern structs live in the `capi` module (E5). `ClipboardContent` (a backend
// field type) stays in `rustty-apprt`; `capi` only re-asserts its ABI.
pub mod capi;

// reference (main_c.zig:31-46): the C API hub's `comptime` block force-references
// the sibling C-API decls so their `export` fns are truly emitted by the C API lib:
//   comptime {
//       _ = @import("config.zig").CApi;                              // Config C ABI
//       if (@hasDecl(apprt.runtime, "CAPI")) _ = apprt.runtime.CAPI; // apprt.embedded
//       _ = @import("benchmark/main.zig").CApi;                      // benchmark
//   }
//
// In rustty: `config.CApi` is force-linked already (this crate USES
// `rustty_config::Config`, pulling its rlib — incl. the `rustty_config_*`
// exports — into the staticlib); `apprt.runtime.CAPI` IS this crate
// (the exports below, plus the E5 `capi` structs). Only the benchmark slice needs an
// explicit force-link: `rustty_benchmark_cli` lives in rustty-bench and is otherwise
// unreferenced here, so `extern crate ... as _` links the rlib and pulls the symbol
// into the C API staticlib — the faithful analog of the reference
// `_ = @import("benchmark/main.zig").CApi`.
extern crate rustty_bench as _;

// ===================================================================
// C API exports — 1:1 with the reference `pub export fn ghostty_*` /
// `@export(..., .{ .name = "ghostty_*" })` lines (rustty_ prefix per R2).
// cbindgen emits these into include/rustty-apprt.h. The bodies forward to the
// ported `rustty-apprt` backend; the `extern "C"` boundary is the ONLY place
// the C ABI is crossed.
// ===================================================================

/// PARITY: reference `std.os.argv = argv[0..argc]` (`ghostty_init`,
/// main_c.zig) — the process argv the C host hands us, stored once. Consumed
/// by the CLI-action detection below (its only reference consumer).
static ARGV: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();

/// The concrete detected CLI action (`state.action`'s VALUE). rustty-core's
/// `GlobalState<Action>` is bound `Action = ()` (rustty-cli deps rustty-core —
/// the concrete type CANNOT live there without a cycle; R4). The ratified
/// split: `rustty_core::STATE.action: Option<()>` carries PRESENCE (set by
/// `init_with_action`), and this top-sink stash carries the VALUE for
/// `rustty_cli_try_action` — together they are the reference's one field.
static CLI_ACTION: std::sync::OnceLock<Option<rustty_cli::rustty::Action>> =
    std::sync::OnceLock::new();

/// The stored-argv view of the reference `std.process.args` iterator that
/// `cli.action.detectArgs` walks (all args INCLUDING argv0 — argv0 never
/// starts with `+` and never matches a special case, so it is harmless).
struct ArgvIter<'a> {
    argv: &'a [Vec<u8>],
    i: usize,
}

impl rustty_cli::action::ArgsIter for ArgvIter<'_> {
    fn next(&mut self) -> Option<Vec<u8>> {
        let v = self.argv.get(self.i)?.clone();
        self.i += 1;
        Some(v)
    }
}

/// `self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);`
/// (global.zig init) — the C-path variant over the STORED argv (the reference
/// detectArgs walks std.process.args, which on posix IS the stored
/// `std.os.argv`).
fn detect_from_argv(
    argv: &[Vec<u8>],
) -> Result<Option<rustty_cli::rustty::Action>, rustty_cli::action::DetectError> {
    let mut iter = ArgvIter { argv, i: 0 };
    rustty_cli::action::detect_iter::<rustty_cli::rustty::Action, _>(&mut iter)
}

/// C: rustty_init
///
/// Initialize rustty global state.
///
/// PORT NOTE: rustty analogue of `ghostty_init` (main_c.zig:105):
/// ```zig
/// pub export fn ghostty_init(argc: usize, argv: [*][*:0]u8) c_int {
///     assert(builtin.link_libc);
///     std.os.argv = argv[0..argc];
///     state.init() catch |err| {
///         std.log.err("failed to initialize ghostty error={}", .{err});
///         return 1;
///     };
///     return 0;
/// }
/// ```
/// The reference `assert(builtin.link_libc)` is a no-op here (Rust's `std`
/// always links libc). The `std.os.argv = argv[0..argc]` store feeds the
/// CLI-action detection inside the full `state.init()` (WS4, wired below);
/// `state.init()` maps to the `rustty_apprt::init_with_action()` bootstrap,
/// which sets `rustty_core::STATE` (the `global.zig` `state` analogue) so the
/// CAPI's `global.alloc`-reading paths (`ghostty_surface_free_text`/
/// `_tty_name`/`Text.deinit`) have a live state. EMBEDDED_BLUEPRINT §3
/// "Global-state precursor" + risk 4: the smoke KAT (E11) MUST call this
/// before `ghostty_app_new`.
///
/// # Safety
/// `argv` must point to `argc` valid, NUL-terminated C strings (the process
/// argv), as the reference `ghostty_init` requires.
#[no_mangle]
pub unsafe extern "C" fn rustty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    // task-14a review round 2 (CRITICAL 2 item 1): `rustty_init` is the port
    // of `main_c.zig`'s `ghostty_init`, the process init for the LIBRARY
    // artifact — `main_c.zig:32`'s `pub const std_options = main.std_options;`
    // means the oracle's C artifact inherits the SAME global `logFn` the app
    // binary installs, live from process entry. `rustty-build-config::apprt::
    // app_runtime` is `None` on everything but Linux/FreeBSD, so on macOS —
    // this project's primary platform — the shipped application IS
    // libghostty (this crate), not the `rustty` exe; `main_rustty.rs`'s
    // install alone left the real macOS app with no backend at all. Installed
    // FIRST, before `rustty_apprt::init_with_action` runs (same "earliest
    // point, bootstrap-window sites get the oracle-faithful
    // `Logging::default()` fallback" reasoning as `main_rustty.rs::main`).
    rustty_core::logging::install_app_log_backend();

    // reference: assert(builtin.link_libc);  (Rust `std` always links libc.)

    // The `.c` arm of `global.init`'s `args` field (`global.zig:62-77`, `e8525c0fd`):
    //
    //   // TODO: Using the C API from Windows is unsupported at this time.
    //   //
    //   // When do we plan on supporting Windows, it's recommended to
    //   // ensure that the C API can take a UNICODE_STRING (aka []16, a
    //   // WTF-16 string) so that it can just be passed into
    //   // std.process.Args.Vector directly.
    //   .c => |c| .{ .vector = if (builtin.os.tag == .windows)
    //       return error.UnsupportedOSForCApi
    //   else
    //       c.argv[0..c.argc] },
    //
    // `error.UnsupportedOSForCApi` propagates out of `state.init()`, which
    // `ghostty_init` turns into the logged `return 1`. rustty owns the argc/argv
    // seam here (see `rustty-global`'s `new_with_environ` PORT NOTE), so the
    // guard lives at this entry point and produces the same observable rc.
    //
    // PORT NOTE: `if cfg!(windows)`, NOT `#[cfg(target_os = "windows")]`. The
    // body is unreachable on this host either way, but `cfg!` keeps it COMPILED
    // and type-checked on every target, so the arm cannot rot into something
    // that does not build on the one platform where it is the live path. There
    // is no Windows toolchain here to run it, which is recorded verification
    // debt — compiling it everywhere is the most this host can do.
    if cfg!(windows) {
        // std.log.err("failed to initialize ghostty error={}", .{err});
        log::error!(
            target: DEFAULT_LOG_SCOPE,
            "failed to initialize rustty error=UnsupportedOSForCApi",
        );
        return 1;
    }

    // reference: std.os.argv = argv[0..argc];
    let stored: Vec<Vec<u8>> = if argv.is_null() {
        Vec::new()
    } else {
        (0..argc)
            .map(|i| {
                // SAFETY: caller contract — argv points to argc valid
                // NUL-terminated C strings (the process argv).
                let p = unsafe { *argv.add(i) };
                if p.is_null() {
                    Vec::new()
                } else {
                    unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec()
                }
            })
            .collect()
    };
    // Once-only like the reference `ghostty_init`; a repeat call keeps the
    // first argv (ignore the Err). Detection below uses the LOCAL copy so a
    // (contract-violating) second call cannot observe torn state.
    let _ = ARGV.set(stored.clone());

    // state.init():
    //   self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);
    //   (the try: a DetectError fails state.init() → main_c logs + returns 1.)
    let action = match detect_from_argv(&stored) {
        Ok(action) => action,
        Err(err) => {
            // std.log.err("failed to initialize ghostty error={}", .{err});
            log::error!(target: DEFAULT_LOG_SCOPE, "failed to initialize rustty error={err:?}");
            return 1;
        }
    };
    let _ = CLI_ACTION.set(action);

    //   ... the remainder of state.init() (logging config with the
    //   action-implies-no-stderr rule, signals, log lines, rlimits, crash,
    //   locale, glslang, oniguruma, resourcesDir, i18n) — the wired BREAK #1
    //   body; `action.is_some()` mirrors presence into STATE.action.
    match rustty_apprt::init_with_action(action.is_some()) {
        Ok(_) => 0,
        Err(err) => {
            // std.log.err("failed to initialize ghostty error={}", .{err});
            log::error!(target: DEFAULT_LOG_SCOPE, "failed to initialize rustty error={err:?}");
            1
        }
    }
}

/// C: rustty_cli_try_action
///
/// Runs an action if it is specified. If there is no action this returns
/// (the GUI-launch path). If there is an action then this doesn't return
/// (it `posix.exit`s with the action's result code).
///
/// PORT NOTE: rustty analogue of `ghostty_cli_try_action` (main_c.zig:120):
/// ```zig
/// pub export fn ghostty_cli_try_action() void {
///     const action = state.action orelse return;
///     std.log.info("executing CLI action={}", .{action});
///     posix.exit(action.run(state.alloc) catch |err| {
///         std.log.err("CLI action failed error={}", .{err});
///         posix.exit(1);
///     });
///     posix.exit(0);
/// }
/// ```
/// The reference `state.action` is `rustty_core::global_state().action`
/// (`Option<Action>`), stashed by `state.init()` (via `rustty_init`). rustty-core
/// binds the `GlobalState<Action>` type parameter to `()` (BREAK #1: rustty-cli
/// deps rustty-core, so core can never name the concrete Action type without a
/// cycle) — so presence lives in `rustty_core::STATE.action: Option<()>` and the
/// concrete VALUE lives in this crate's own `CLI_ACTION` stash (set together by
/// `rustty_init`); together they realize the reference's one field (WS4).
///
/// # Safety
/// Marked `unsafe extern` only for signature uniformity with the rest of the C API.
/// Reads process-global state that `rustty_init` must have initialized first (the
/// reference `ghostty_init` → `ghostty_cli_try_action` ordering).
#[no_mangle]
pub unsafe extern "C" fn rustty_cli_try_action() {
    // const action = state.action orelse return;
    //   Presence lives in core STATE (`Option<()>`); the concrete value lives
    //   in the top-sink CLI_ACTION stash (see its doc) — set together by
    //   `rustty_init`, so a Some presence ALWAYS has a Some stash.
    let action = match rustty_core::global_state().action {
        Some(()) => CLI_ACTION
            .get()
            .copied()
            .flatten()
            .expect("CLI_ACTION is stashed by rustty_init whenever state.action is Some"),
        // (no action → GUI launch: return WITHOUT exiting — the windowed path)
        None => return,
    };

    // std.log.info("executing CLI action={}", .{action});
    log::info!(target: DEFAULT_LOG_SCOPE, "executing CLI action={action:?}");

    // posix.exit(action.run(state.alloc) catch |err| {
    //     std.log.err("CLI action failed error={}", .{err});
    //     posix.exit(1);
    // });
    // posix.exit(0);
    //   PORT NOTE: `action.run()` takes no allocator (dropped tree-wide);
    //   `posix.exit(action.run(...))` exits with the action's u8 code, and the
    //   reference's trailing `posix.exit(0)` is unreachable (kept as comment).
    std::process::exit(match action.run() {
        Ok(code) => i32::from(code),
        Err(err) => {
            log::error!(target: DEFAULT_LOG_SCOPE, "CLI action failed error={err:?}");
            std::process::exit(1);
        }
    });
    // posix.exit(0); — unreachable after the exit above (reference dead tail).
}

// ===================================================================
// App lifecycle exports — embedded.zig `CAPI` block (embedded.zig:1398-1535),
// chunk E6. The 11 app-lifecycle exports + the 2 app/config exports. These are the
// runtime-EXERCISABLE core (EMBEDDED_BLUEPRINT §1): `app_new` builds `CoreApp` +
// keymap + config clone — NO renderer, NO xev — so the whole group is live NOW (the
// GPU/xev leaf boundary is `ghostty_surface_new`, chunk E7+). Each is authored 1:1
// with the reference `export fn ghostty_*` (rustty_ prefix per R2); the bodies
// forward to the ported `rustty-apprt` embedded backend / core App.
// ===================================================================

/// The error set of [`app_new_`] — `CoreApp::create`'s `CreateError` (the
/// reference `app_new_`'s inferred `!*App` error set). Kept as an enum so the
/// reference's single `catch |err|` log site in [`rustty_app_new`] is preserved
/// with the concrete `err` value.
//
// PORT NOTE: the payload IS read — [`rustty_app_new`] formats the whole error via
// `{err:?}` (the reference `log.err("… err={}", .{err})`) — but derived-`Debug` reads
// are invisible to dead-code analysis, so the lint is allowed here.
//
// PORT NOTE (S46-F1): a second variant, `Keymap(embedded::KeymapError)`, used to
// sit here because the port built `App.keymap` EAGERLY in `App::init`. Upstream's
// keymap is lazy and its `init` cannot fail that way; with the port's `init` now
// infallible, the variant had no constructor left.
#[allow(dead_code)]
#[derive(Debug)]
enum AppNewError {
    // const core_app = try CoreApp.create(global.alloc);
    // (CoreApp.create's error: font.SharedGridSet.InitError || Allocator.Error.)
    Create(rustty_core::app::CreateError),
}

/// C: rustty_app_new
///
/// Create a new app.
///
/// # Safety
/// `opts` must point to a valid [`Options`] and `config` to a valid [`Config`]
/// (the config the host built via the config C-ABI); both are read (not retained)
/// for the duration of the call. Returns null on error. The returned pointer, if
/// non-null, must be released with [`rustty_app_free`].
//
// PORT NOTE (improper_ctypes_definitions): the `*const Options` parameter carries
// the `action` callback whose `ActionC`/`TargetC` args transitively hold
// `rustty_input`'s `Mods` (apprt-api's deferred C-ABI pinning); the backend
// silences the same lint on `Options`. Pinned by the E5 D2 const-asserts + cbindgen
// (E11) — faithful to the reference `*const apprt.runtime.App.Options` signature.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_app_new(opts: *const Options, config: *const Config) -> *mut App {
    // return app_new_(opts, config) catch |err| {
    //     log.err("error initializing app err={}", .{err});
    //     return null;
    // };
    // SAFETY: `opts`/`config` validity is the caller's contract (see # Safety).
    match unsafe { app_new_(opts, config) } {
        Ok(app) => app,
        Err(err) => {
            log::error!(target: LOG_SCOPE, "error initializing app err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// # Safety
/// See [`rustty_app_new`].
//
// fn app_new_(opts, config) !*App { ... }  (embedded.zig:1409)
unsafe fn app_new_(opts: *const Options, config: *const Config) -> Result<*mut App, AppNewError> {
    // const core_app = try CoreApp.create(global.alloc);
    // errdefer core_app.destroy();
    //
    // PORT NOTE: the reference `global.alloc` is the process-global allocator; the
    // rustty seam is Rust's global allocator (`allocator_api2 Global`), matching
    // `rustty_apprt::init`'s `()` alloc seam. `CoreApp::create` returns a
    // `Box<CoreApp>`; `into_raw` yields the stable `*mut CoreApp` the embedded `App`
    // stores (reference `core_app: *CoreApp`). The `errdefer core_app.destroy()` is
    // realized manually on the `App::init` error path below.
    let core_app: *mut CoreApp =
        Box::into_raw(CoreApp::create(Global).map_err(AppNewError::Create)?);

    // Create our runtime app
    // var app = try global.alloc.create(App);
    // errdefer global.alloc.destroy(app);
    // try app.init(core_app, config, opts.*);
    // errdefer app.terminate();
    //
    // PORT NOTE: rustty `App::init` RETURNS the value (no placement-init), so the
    // reference's `alloc.create(App)` uninitialized slot + its `errdefer destroy`
    // collapse into boxing the returned value below (boxing is infallible, so the
    // `errdefer app.terminate()` never fires). `opts.*` copies the `Options` out of
    // the caller's pointer — `ptr::read` (Options is `#[repr(C)]`, trivially
    // copyable).
    //
    // PORT NOTE (S46-F1): `App::init` is now INFALLIBLE, matching the reference
    // once its keymap became lazy — the reference's `try app.init(...)` only ever
    // carried `config.clone`'s allocator error, and rustty's `clone_config` is
    // infallible. The `errdefer core_app.destroy()` therefore has no arm left to
    // fire on, exactly as `errdefer app.terminate()` already had none.
    // SAFETY: `opts`/`config` validity is the caller's contract.
    let opts_val = unsafe { core::ptr::read(opts) };
    let app = App::init(core_app, unsafe { &*config }, opts_val);

    // return app;
    Ok(Box::into_raw(Box::new(app)))
}

/// C: rustty_app_tick
///
/// Tick the event loop. This should be called whenever the "wakeup"
/// callback is invoked for the runtime.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_tick(v: *mut App) {
    // v.core_app.tick(v) catch |err| {
    //     log.err("error app tick err={}", .{err});
    // };
    //
    // PORT NOTE: `core_app` (a separate heap allocation) and the rt-app `&*v` never
    // alias, so the `&mut CoreApp` receiver + `&App` argument are sound.
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).tick(&*v) } {
        log::error!(target: LOG_SCOPE, "error app tick err={err:?}");
    }
}

/// C: rustty_app_userdata
///
/// Return the userdata associated with the app.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_userdata(v: *mut App) -> *mut c_void {
    // return v.opts.userdata;
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*v).opts.userdata }
}

/// C: rustty_app_free
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`]; it is invalid after
/// this call returns.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_free(v: *mut App) {
    // const core_app = v.core_app;
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    // v.terminate();
    unsafe { (*v).terminate() };
    // global.alloc.destroy(v);
    //
    // PORT NOTE: `terminate()` already tore down the App's keymap+config; Zig's
    // `destroy` then frees the raw memory WITHOUT running any destructor. `ManuallyDrop`
    // frees the `App` box without re-dropping its (already-deinit'd) fields — the
    // `closeSurface` precedent (embedded.rs).
    unsafe { drop(Box::from_raw(v as *mut ManuallyDrop<App>)) };
    // core_app.destroy();
    unsafe { CoreApp::destroy(Box::from_raw(core_app)) };
}

/// C: rustty_app_set_focus
///
/// Update the focused state of the app.
///
/// # Safety
/// `app` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_set_focus(app: *mut App, focused: bool) {
    // app.focusEvent(focused);
    // SAFETY: `app` validity is the caller's contract.
    unsafe { (*app).focusEvent(focused) };
}

/// C: rustty_app_key
///
/// Notify the app of a global keypress capture. This will return
/// true if the key was captured by the app, in which case the caller
/// should not process the key.
///
/// # Safety
/// `app` must be valid; `event.text`, if non-null, must be a valid NUL-terminated
/// C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_key(app: *mut App, event: KeyEvent) -> bool {
    // return app.keyEvent(.app, event.keyEvent()) catch |err| {
    //     log.warn("error processing key event err={}", .{err});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    // D11 (P3-W6 T2): `event.action` is the raw C integer; `keyEvent()` narrows
    // it and is `None` for an undeclared value, which takes the same
    // "error processing key event" exit the reference uses for a failed event.
    let kev = match unsafe { event.keyEvent() } {
        Some(v) => v,
        None => {
            log::warn!(target: LOG_SCOPE, "error processing key event: action out of range");
            return false;
        }
    };
    // SAFETY: `app` validity is the caller's contract.
    match unsafe { (*app).keyEvent(KeyTarget::App, kev) } {
        Ok(v) => v,
        Err(err) => {
            log::warn!(target: LOG_SCOPE, "error processing key event err={err:?}");
            false
        }
    }
}

/// C: rustty_config_key_is_binding
///
/// Returns true if the given key event would trigger a binding
/// if it were sent to the surface right now. The "right now"
/// is important because things like trigger sequences are only
/// valid until the next key event.
///
/// # Safety
/// `config` must be a valid [`Config`] pointer; `event.text`, if non-null, must be
/// a valid NUL-terminated C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_key_is_binding(
    config: *mut Config,
    event: KeyEvent,
) -> bool {
    // const core_event = event.keyEvent().core() orelse {
    //     log.warn("error processing key event", .{});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    // D11 (P3-W6 T2): `keyEvent()` narrows `event.action`; `None` folds into the
    // same "error processing key event" exit the reference already has here.
    let core_event = match unsafe { event.keyEvent() }.and_then(|e| e.core()) {
        Some(v) => v,
        None => {
            log::warn!(target: LOG_SCOPE, "error processing key event");
            return false;
        }
    };

    // return config.keyEventIsBinding(core_event);
    //
    // PORT NOTE: `keyEventIsBinding` -> `key_event_is_binding` (rustty-config's
    // snake_case method convention).
    // SAFETY: `config` validity is the caller's contract.
    unsafe { (*config).key_event_is_binding(core_event) }
}

/// C: rustty_app_keyboard_changed
///
/// Notify the app that the keyboard was changed. This causes the
/// keyboard layout to be reloaded from the OS.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_keyboard_changed(v: *mut App) {
    // v.reloadKeymap() catch |err| {
    //     log.err("error reloading keyboard map err={}", .{err});
    //     return;
    // };
    // SAFETY: `v` validity is the caller's contract.
    if let Err(err) = unsafe { (*v).reloadKeymap() } {
        log::error!(target: LOG_SCOPE, "error reloading keyboard map err={err:?}");
    }
}

/// C: rustty_app_open_config
///
/// Open the configuration.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_open_config(v: *mut App) {
    // _ = v.performAction(.app, .open_config, .new_window) catch |err| {
    //     log.err("error reloading config err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference `*App` method `performAction(.app, .open_config,
    // .new_window)` maps to the `apprt_api::App` trait's `perform_action(Target::App,
    // Action::OpenConfig(OpenConfig::NewWindow))` (the embedded `App` is the first
    // concrete backend). The `Target::App` carries no surface, so its
    // `C: CoreSurfaceHandle` type parameter is annotated as `CoreSurface`. The `bool`
    // result is discarded (reference `_ =`).
    // SAFETY: `v` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*v).perform_action(
            Target::<CoreSurface>::App,
            Action::OpenConfig(rustty_apprt_api::action::OpenConfig::NewWindow),
        )
    } {
        log::error!(target: LOG_SCOPE, "error reloading config err={err:?}");
    }
}

/// C: rustty_app_update_config
///
/// Update the configuration to the provided config. This will propagate
/// to all surfaces as well.
///
/// # Safety
/// `v` must be valid; `config` must point to a valid [`Config`] the caller owns
/// (it may be freed as soon as this returns).
#[no_mangle]
pub unsafe extern "C" fn rustty_app_update_config(v: *mut App, config: *const Config) {
    // v.core_app.updateConfig(v, config) catch |err| {
    //     log.err("error updating config err={}", .{err});
    //     return;
    // };
    // SAFETY: `v`/`config` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).updateConfig(&*v, &*config) } {
        log::error!(target: LOG_SCOPE, "error updating config err={err:?}");
    }
}

/// C: rustty_app_needs_confirm_quit
///
/// Returns true if the app needs to confirm quitting.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_needs_confirm_quit(v: *mut App) -> bool {
    // return v.core_app.needsConfirmQuit();
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*(*v).core_app).needsConfirmQuit() }
}

/// C: rustty_app_has_global_keybinds
///
/// Returns true if the app has global keybinds.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_has_global_keybinds(v: *mut App) -> bool {
    // return v.hasGlobalKeybinds();
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*v).hasGlobalKeybinds() }
}

/// C: rustty_app_set_color_scheme
///
/// Update the color scheme of the app.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_set_color_scheme(v: *mut App, scheme_raw: c_int) {
    // const scheme = std.meta.intToEnum(apprt.ColorScheme, scheme_raw) catch {
    //     log.warn(
    //         "invalid color scheme to ghostty_surface_set_color_scheme value={}",
    //         .{scheme_raw},
    //     );
    //     return;
    // };
    //
    // PORT NOTE: `apprt.ColorScheme` is `enum(u2) { light = 0, dark = 1 }`; the
    // `intToEnum` guard lives here on the raw `c_int` crossing the C boundary (vt-c
    // `rustty_build_info` precedent).
    //
    // PORT NOTE (task-14d-3, INVERSE PARITY DEFECT — a rustty-ONLY emission,
    // now REMOVED): the commented reference call above is from a SUPERSEDED
    // oracle revision. At pin 20abdb50a this guard reads
    // `const scheme = std.enums.fromInt(apprt.ColorScheme, scheme_raw) orelse
    // return;` (embedded.zig:1788) — SILENT. The port emitted a warning here that no
    // pinned oracle revision emits, which is the same defect shape S50-F4
    // records and the same one this file's `rustty_surface_mouse_pressure`
    // already had removed for exactly this reason. The comment above is kept
    // (R1) as the record of what upstream used to do.
    let scheme = match scheme_raw {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => return,
    };

    // v.core_app.colorSchemeEvent(v, scheme) catch |err| {
    //     log.err("error setting color scheme err={}", .{err});
    //     return;
    // };
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).colorSchemeEvent(&*v, scheme) } {
        log::error!(target: LOG_SCOPE, "error setting color scheme err={err:?}");
    }
}

// ===================================================================
// main_c.zig precursor — the `String` (`ghostty_string_s`) C hub type +
// `ghostty_string_free`. embedded.zig imports it (`const String =
// @import("../main_c.zig").String;`, embedded.zig:23) and E7's
// `ghostty_surface_tty_name` returns it. `main_c.zig` is the top-level C API hub
// this crate root represents; its items are folded in as the exports that need
// them land (the E0 `rustty_init` precedent). rustty-config ships its own local
// deferred projection of this same struct (`c_api.rs`, no `deinit`); this is the
// canonical `main_c` port with the freeing pair (ADV-FIX 9 pub-widening).
// ===================================================================

/// PORTS: `main_c.zig`'s `pub const String = extern struct { ... }`
/// (`ghostty_string_s`) — a length-prefixed byte string handed to the C host and
/// reclaimed via [`rustty_string_free`].
//
// pub const String = extern struct {
//     ptr: ?[*]const u8,
//     len: usize,
//     sentinel: bool,
//     ...
// };
//
// PORT NOTE: the std prelude `String` is never used in this crate root (the config
// c_api precedent fully-qualifies `std::string::String` where it needs it), so this
// extern projection safely takes the `String` name to match the reference type.
// `?[*]const u8` → the raw `*const u8` the rustty CAPI convention uses for a nullable
// pointer (null == empty).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct String {
    // ptr: ?[*]const u8,
    pub ptr: *const u8,
    // len: usize,
    pub len: usize,
    // sentinel: bool,
    pub sentinel: bool,
}

impl String {
    // pub const empty: String = .{ .ptr = null, .len = 0, .sentinel = false };
    fn empty() -> String {
        String {
            ptr: core::ptr::null(),
            len: 0,
            sentinel: false,
        }
    }

    // pub fn fromSlice(slice: anytype) String { ... }
    //
    // PORT NOTE: the reference `fromSlice` records a slice's `.ptr`/`.len` and
    // derives `sentinel` from the slice TYPE (`[:0]` → true). Rust has no
    // compile-time slice-sentinel reflection, so `sentinel` is passed explicitly.
    // The caller's owned bytes (the reference `dupeZ` copy) are taken by value and
    // leaked here (fusing `dupeZ` + `fromSlice` + the sentinel append — the config
    // c_api `from_path` precedent) so the pointer stays valid for the C host, which
    // reclaims the allocation via [`rustty_string_free`]. For a sentinel string the
    // backing allocation is `len + 1` bytes (data + trailing NUL) with `len`
    // excluding the sentinel, matching the reference `[:0]const u8` convention (and
    // the E5 `Text` NUL-termination discipline).
    fn from_slice(mut v: Vec<u8>, sentinel: bool) -> String {
        let len = v.len();
        if sentinel {
            v.push(0);
        }
        let ptr = Box::leak(v.into_boxed_slice()).as_ptr();
        String { ptr, len, sentinel }
    }

    // pub fn deinit(self: *const String) void {
    //     const ptr = self.ptr orelse return;
    //     if (self.sentinel) {
    //         state.alloc.free(ptr[0..self.len :0]);
    //     } else {
    //         state.alloc.free(ptr[0..self.len]);
    //     }
    // }
    //
    // PORT NOTE: the rustty global-alloc seam is Rust's global allocator (the E5
    // `Text::deinit` precedent), so `state.alloc.free` reconstructs and drops the
    // leaked `Box<[u8]>` produced by [`String::from_slice`]. A sentinel string frees
    // `len + 1` bytes (`ptr[0..len :0]` — data + NUL); a plain string frees `len`.
    fn deinit(&self) {
        // const ptr = self.ptr orelse return;
        if self.ptr.is_null() {
            return;
        }
        // SAFETY: per the producer contract `self.ptr` is the start of a not-yet-freed
        // `Box<[u8]>` of `len + 1` (sentinel) or `len` (plain) bytes; reconstruct and
        // drop it to free via the global allocator.
        unsafe {
            let n = if self.sentinel {
                self.len + 1
            } else {
                self.len
            };
            drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                self.ptr as *mut u8,
                n,
            )));
        }
    }
}

/// C: rustty_string_free
///
/// Free a string allocated by Rustty.
///
/// PORT NOTE: rustty analogue of `main_c.zig`'s
/// `pub export fn ghostty_string_free(str: String) void { str.deinit(); }`
/// (main_c.zig:155). The `String` is passed by value (it is `#[repr(C)]` + `Copy`);
/// [`String::deinit`] frees the backing allocation. The producing export
/// ([`rustty_surface_tty_name`]) documents this as the required free.
///
/// # Safety
/// `str` must be a `String` returned by a rustty export (e.g.
/// [`rustty_surface_tty_name`]) and not already freed; it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn rustty_string_free(str: String) {
    // str.deinit();
    str.deinit();
}

// ===================================================================
// main_c.zig `ghostty_info` + `ghostty_translate` — the remaining two `main_c.zig`
// C API hub exports of the 90-symbol app C-ABI (its `main_c` slice; `ghostty_init`
// landed above, `ghostty_string_free` just above, and `ghostty_cli_try_action` stays
// deferred with the BREAK #1 global-state seam). Authored 1:1 with the reference
// `pub export fn ghostty_*` (rustty_ prefix per R2).
// ===================================================================

/// PORTS: `main_c.zig`'s `const Info = extern struct { ... }` (`ghostty_info_s`) —
/// the build metadata [`rustty_info`] returns by value.
//
// /// ghostty_info_s
// const Info = extern struct {
//     mode: BuildMode,
//     version: [*]const u8,
//     version_len: usize,
//     ...
// };
//
// PORT NOTE: the reference `Info` is a private (`const`, non-`pub`) file-local struct
// referenced only by `ghostty_info`'s return; the rustty projection is `pub` so cbindgen
// emits `ghostty_info_s` into the header. `[*]const u8` → a plain `*const u8` (a
// NON-sentinel byte pointer whose length is carried explicitly by `version_len`).
#[repr(C)]
pub struct Info {
    // mode: BuildMode,
    pub mode: BuildMode,
    // version: [*]const u8,
    pub version: *const u8,
    // version_len: usize,
    pub version_len: usize,
}

// `rustty_info_s` = `{ enum build_mode(4→pad8); const char* version(8);
// uintptr_t version_len(8) }` = 24, align 8. Returned by value by `rustty_info`
// (landed; abi_layout.rs's stale "DEFERRED" note is gone and that file now pins the
// same layout plus the `BuildMode` ordinals/width). Frozen here.
// @embed-abi rustty_info_s 24
const _: () = assert!(core::mem::size_of::<Info>() == 24);
const _: () = assert!(core::mem::align_of::<Info>() == 8);
// C field `build_mode` is the Rust field `mode` (same offset); the gate probes
// the C name, this const-assert the Rust name — they meet at the offset.
// @embed-abi-field rustty_info_s build_mode 0
const _: () = assert!(core::mem::offset_of!(Info, mode) == 0);
// @embed-abi-field rustty_info_s version 8
const _: () = assert!(core::mem::offset_of!(Info, version) == 8);
// @embed-abi-field rustty_info_s version_len 16
const _: () = assert!(core::mem::offset_of!(Info, version_len) == 16);

/// The `Info.BuildMode` optimization-mode enum (`ghostty_build_mode_e`). The ordinals
/// are ABI-frozen (ghostty.h:385-390 / PREPORT_SWIFT_APP §6): `debug = 0`,
/// `release_safe = 1`, `release_fast = 2`, `release_small = 3`.
//
// const BuildMode = enum(c_int) {
//     debug,
//     release_safe,
//     release_fast,
//     release_small,
// };
//
// PORT NOTE: a fieldless `#[repr(C)]` enum lowers to a C `int` (== the reference
// `enum(c_int)`); the variants keep the reference declaration order so the ordinals
// match bit-for-bit (I9).
#[repr(C)]
pub enum BuildMode {
    // debug,
    Debug,
    // release_safe,
    ReleaseSafe,
    // release_fast,
    ReleaseFast,
    // release_small,
    ReleaseSmall,
}

/// C: rustty_info
///
/// Return metadata about Rustty, such as version, build mode, etc.
//
// pub export fn ghostty_info() Info {
//     return .{
//         .mode = switch (builtin.mode) {
//             .Debug => .debug,
//             .ReleaseSafe => .release_safe,
//             .ReleaseFast => .release_fast,
//             .ReleaseSmall => .release_small,
//         },
//         .version = build_config.version_string.ptr,
//         .version_len = build_config.version_string.len,
//     };
// }
//
// PORT NOTE: Rust has no `builtin.mode` reflection over the four Zig optimization
// modes; a build is only ever debug xor optimized, distinguished by
// `cfg!(debug_assertions)` — exactly the split rustty-build-config's `mode_string()`
// already collapses the four Zig modes to ("Debug" xor "ReleaseFast"). So a debug build
// reports `.Debug` and an optimized build `.ReleaseFast` — the two modes rustty can
// actually be built in; the `release_safe`/`release_small` arms have no rustty analog.
// `version` is `rustty_build_config::VERSION_STRING` (the reference
// `build_config.version_string`): a `&'static str` whose `.as_ptr()`/`.len()` map the
// reference `[*]const u8` `.ptr`/`.len` — a plain (NON-NUL-terminated) byte pointer the
// host reads for exactly `version_len` bytes. The pointer is into static storage, valid
// for the whole program (no free).
#[no_mangle]
pub extern "C" fn rustty_info() -> Info {
    // return .{ .mode = switch (builtin.mode) { ... }, .version = ..., .version_len = ... };
    Info {
        mode: if cfg!(debug_assertions) {
            // .Debug => .debug
            BuildMode::Debug
        } else {
            // .ReleaseFast => .release_fast (the only optimized mode rustty builds as)
            BuildMode::ReleaseFast
        },
        // .version = build_config.version_string.ptr,
        version: rustty_build_config::VERSION_STRING.as_ptr(),
        // .version_len = build_config.version_string.len,
        version_len: rustty_build_config::VERSION_STRING.len(),
    }
}

/// C: rustty_translate
///
/// Translate a string maintained by librustty into the current
/// application language. This will return the same string (same pointer)
/// if no translation is found, so the pointer must be stable through
/// the function call.
///
/// This should only be used for singular strings maintained by Rustty.
//
// pub export fn ghostty_translate(msgid: [*:0]const u8) [*:0]const u8 {
//     return internal_os.i18n._(msgid);
// }
//
// PORT NOTE (WS13): this used to return `msgid` unchanged. The two reasons the old
// note gave are both resolved: (1) `_dgettext` IS now defined by rustty's own
// vendored libintl (`crates-sys/gettext-sys` -> `librustty_intl.a`, WS9 `9af41cc`,
// merged `509b1e3`), so linking `gettext_` no longer fails on `_dgettext`; (2)
// `i18n::init`'s `bindtextdomain` IS called during app bootstrap
// (`rustty-apprt/src/lib.rs:322`, WS4), right after `ensureLocale()`. So this now
// forwards for real, exactly as the reference does. The "same string (same pointer)
// if no translation is found" contract is gettext's OWN behavior, preserved
// verbatim: `dgettext` returns its `msgid` argument on a miss.
//
// DG5 (C-ABI panic policy is ABORT, so no unwind may cross this boundary):
// `rustty_os::i18n::gettext_` performs one `CString::new(BUNDLE_ID)` — infallible
// on a compile-time constant with no interior NUL, though it is spelled `expect` —
// and one `dgettext` call, which never panics. There is therefore no panic edge to
// catch, and no `catch_unwind` is added (that would be the only alternative).
//
// `[*:0]const u8` ↔ `*const c_char`. `dgettext`'s return points into
// gettext-managed storage valid for the program lifetime, so handing it back across
// the ABI is sound.
///
/// # Safety
/// `msgid` must be a valid NUL-terminated C string that stays valid through the call
/// (the reference `[*:0]const u8` contract — "the pointer must be stable through the
/// function call").
#[no_mangle]
pub unsafe extern "C" fn rustty_translate(msgid: *const c_char) -> *const c_char {
    // return internal_os.i18n._(msgid);
    let msgid_c = unsafe { std::ffi::CStr::from_ptr(msgid) };
    rustty_os::i18n::gettext_(msgid_c).as_ptr()
}

// ===================================================================
// Surface lifecycle + state/query + render/window exports — embedded.zig `CAPI`
// block (embedded.zig:1538-1760), chunk E7. `ghostty_surface_config_new` (a pure
// zeroed/defaulted `Surface.Options`) is runtime-live; every other export needs a
// LIVE surface, which can only exist past `ghostty_surface_new` → core
// `Surface::init` → the Phase-3 `renderer_seam::{Renderer,Thread}` leaf. So these
// exports COMPILE + LINK + header-generate NOW; `refresh`/`draw`/`set_size`/… reach
// the GPU leaf and PANIC when driven against a real surface (EMBEDDED_BLUEPRINT §1,
// the honest runtime boundary). Authored 1:1 with the reference `export fn
// ghostty_surface_*` (rustty_ prefix per R2); bodies forward to the ported backend.
// ===================================================================

/// C: rustty_surface_config_new
///
/// # Safety
/// Pure value constructor — no pointers are dereferenced. Marked `unsafe extern`
/// only for signature uniformity with the rest of the C API.
//
// export fn ghostty_surface_config_new() apprt.Surface.Options {
//     return .{};
// }
//
// PORT NOTE: the reference `.{}` default-initializes `Surface.Options` from its Zig
// field defaults. Rust has no struct field defaults, so those exact defaults are
// spelled out below (the E3 `newSurfaceOptions` precedent, which does the same for
// its elided fields). `platform` is reference `undefined` (never read — the C host
// overwrites `platform`/`platform_tag` before `ghostty_surface_new`); a zeroed union
// is the faithful placeholder.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_config_new() -> SurfaceOptions {
    // return .{};
    SurfaceOptions {
        // platform_tag: c_int = 0,
        platform_tag: 0,
        // platform: Platform.C = undefined,
        platform: PlatformC {
            macos: PlatformCMacOS {
                nsview: core::ptr::null_mut(),
            },
        },
        // userdata: ?*anyopaque = null,
        userdata: core::ptr::null_mut(),
        // scale_factor: f64 = 1,
        scale_factor: 1.0,
        // font_size: f32 = 0,
        font_size: 0.0,
        // working_directory: ?[*:0]const u8 = null,
        working_directory: core::ptr::null(),
        // command: ?[*:0]const u8 = null,
        command: core::ptr::null(),
        // env_vars: ?[*]EnvVar = null,
        env_vars: core::ptr::null(),
        // env_var_count: usize = 0,
        env_var_count: 0,
        // initial_input: ?[*:0]const u8 = null,
        initial_input: core::ptr::null(),
        // wait_after_command: bool = false,
        wait_after_command: false,
        // context: apprt.surface.NewSurfaceContext = .window,
        context: NewSurfaceContext::Window,
    }
}

/// C: rustty_surface_new
///
/// Create a new surface as part of an app.
///
/// # Safety
/// `app` must be a valid pointer returned by [`rustty_app_new`]; `opts` must point
/// to a valid [`SurfaceOptions`]. Returns null on error. The returned pointer, if
/// non-null, is a heap-boxed + pinned surface (ADV-FIX 8) released with
/// [`rustty_surface_free`].
//
// SB/PIN (I2 / R-E — the self-referential `rt_surface` pin): `App::newSurface`
// (embedded.rs:467) boxes an uninitialized slot, lowers it IMMEDIATELY to a raw
// `*mut Surface`, and inits in place; core `Surface::init` writes
// `core_surface.rt_surface = <this container ptr>` (surface.rs:1953), so the
// container's HEAP ADDRESS must stay stable for its whole life — every action
// callback ships `v.rt_surface` (the container addr) to Swift and derefs it. This
// export returns that raw ptr verbatim; no movable `Box<Surface>` ever exists after
// init, and `rustty_surface_free` frees via `ManuallyDrop` WITHOUT moving. The pin
// (no-move) is PROVEN on the isolated non-FFI Miri harness
// `.port/phase-2/mirihar-surface-pin` (R5): `pin_shipped` (boxed + no-move) is CLEAN
// under BOTH Stacked Borrows and Tree Borrows; the `pin_move*` negatives (relocate
// the Surface after init, then follow the back-pointer) are flagged UB at the
// `rt_surface` deref under both. A refactor that returns the Surface by value or
// reallocates it post-init would reintroduce that UB.
//
// PORT NOTE (improper_ctypes_definitions): `*const SurfaceOptions` carries raw
// pointers + the `Platform.C` union; pinned by the E5/E2 D2 const-asserts + cbindgen
// (E11), faithful to the reference `*const apprt.Surface.Options` signature.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_new(
    app: *mut App,
    opts: *const SurfaceOptions,
) -> *mut Surface {
    // return surface_new_(app, opts) catch |err| {
    //     log.err("error initializing surface err={}", .{err});
    //     return null;
    // };
    // SAFETY: `app`/`opts` validity is the caller's contract.
    match unsafe { surface_new_(app, opts) } {
        Ok(surface) => surface,
        Err(err) => {
            log::error!(target: LOG_SCOPE, "error initializing surface err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// # Safety
/// See [`rustty_surface_new`].
//
// fn surface_new_(app: *App, opts: *const apprt.Surface.Options) !*Surface {
//     return try app.newSurface(opts.*);
// }
unsafe fn surface_new_(
    app: *mut App,
    opts: *const SurfaceOptions,
) -> Result<*mut Surface, ApprtError> {
    // return try app.newSurface(opts.*);
    //
    // PORT NOTE: `opts.*` copies the `Surface.Options` out of the caller's pointer
    // (`ptr::read` — `SurfaceOptions` is `#[repr(C)]`, trivially copyable).
    // `App::newSurface` heap-boxes + pins the backend `Surface` (ADV-FIX 8).
    // SAFETY: `app`/`opts` validity is the caller's contract.
    unsafe { (*app).newSurface(core::ptr::read(opts)) }
}

/// C: rustty_surface_free
///
/// # Safety
/// `ptr` must be a valid surface returned by [`rustty_surface_new`]; it is invalid
/// after this call returns.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_free(ptr: *mut Surface) {
    // ptr.app.closeSurface(ptr);
    //
    // PORT NOTE: capture the `app` raw pointer BEFORE handing `ptr` to
    // `closeSurface` (raw-pointer args, the `&mut`-args-strongly-protected
    // discipline). `closeSurface` deinits + frees the boxed surface.
    // SAFETY: `ptr` validity is the caller's contract.
    let app = unsafe { (*ptr).app };
    unsafe { (*app).closeSurface(ptr) };
}

/// C: rustty_surface_userdata
///
/// Returns the userdata associated with the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_userdata(surface: *mut Surface) -> *mut c_void {
    // return surface.userdata;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).userdata }
}

/// C: rustty_surface_app
///
/// Returns the app associated with a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_app(surface: *mut Surface) -> *mut App {
    // return surface.app;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).app }
}

/// C: rustty_surface_inherited_config
///
/// Returns the config to use for surfaces that inherit from this one.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_inherited_config(
    surface: *mut Surface,
    source: NewSurfaceContext,
) -> SurfaceOptions {
    // return surface.newSurfaceOptions(source);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).newSurfaceOptions(source) }
}

/// C: rustty_surface_update_config
///
/// Update the configuration to the provided config for only this surface.
///
/// # Safety
/// `surface` must be valid; `config` must point to a valid [`Config`] the caller
/// owns (it may be freed as soon as this returns).
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_update_config(
    surface: *mut Surface,
    config: *const Config,
) {
    // surface.core_surface.updateConfig(config) catch |err| {
    //     log.err("error updating config err={}", .{err});
    //     return;
    // };
    // SAFETY: `surface`/`config` validity is the caller's contract.
    if let Err(err) = unsafe { (*surface).core_surface.updateConfig(&*config) } {
        log::error!(target: LOG_SCOPE, "error updating config err={err:?}");
    }
}

/// C: rustty_surface_needs_confirm_quit
///
/// Returns true if the surface needs to confirm quitting.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_needs_confirm_quit(surface: *mut Surface) -> bool {
    // return surface.core_surface.needsConfirmQuit();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.needsConfirmQuit() }
}

/// C: rustty_surface_process_exited
///
/// Returns true if the surface process has exited.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_process_exited(surface: *mut Surface) -> bool {
    // return surface.core_surface.child_exited;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.child_exited }
}

/// C: rustty_surface_has_selection
///
/// Returns true if the surface has a selection.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_has_selection(surface: *mut Surface) -> bool {
    // return surface.core_surface.hasSelection();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.hasSelection() }
}

/// C: rustty_surface_read_selection
///
/// Same as rustty_surface_read_text but reads from the user selection,
/// if any.
///
/// # Safety
/// `surface` must be valid; `result` must point to a writable [`Text`]. On `true`
/// the written `Text` owns a heap C-string freed via [`rustty_surface_free_text`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_read_selection(
    surface: *mut Surface,
    result: *mut Text,
) -> bool {
    // const core_surface = &surface.core_surface;
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };

    // core_surface.renderer_state.mutex.lock();
    // defer core_surface.renderer_state.mutex.unlock();
    //
    // PORT NOTE: `renderer_state.mutex` is a `*mut Mutex`; the `defer unlock` runs on
    // every scope exit — realized here by computing the result into a local and
    // unlocking once before the single return (both the no-selection and the
    // read arms flow through it).
    let mutex = unsafe { (*core_surface).renderer_state.mutex };
    unsafe { (*mutex).lock() };

    // If we don't have a selection, do nothing.
    // const core_sel = core_surface.io.terminal.screens.active.selection orelse return false;
    let active = unsafe { (*core_surface).io.terminal.screens.active };
    let out = match unsafe { (*active).selection } {
        // (no selection → `orelse return false`)
        None => false,
        // Read the text from the selection.
        // return readTextLocked(surface, core_sel, result);
        Some(core_sel) => unsafe { readTextLocked(surface, core_sel, result) },
    };

    unsafe { (*mutex).unlock() };
    out
}

/// C: rustty_surface_read_text
///
/// Read some arbitrary text from the surface.
///
/// This is an expensive operation so it shouldn't be called too
/// often. We recommend that callers cache the result and throttle
/// calls to this function.
///
/// # Safety
/// `surface` must be valid; `result` must point to a writable [`Text`]. On `true`
/// the written `Text` owns a heap C-string freed via [`rustty_surface_free_text`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_read_text(
    surface: *mut Surface,
    sel: Selection,
    result: *mut Text,
) -> bool {
    // surface.core_surface.renderer_state.mutex.lock();
    // defer surface.core_surface.renderer_state.mutex.unlock();
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };
    let mutex = unsafe { (*core_surface).renderer_state.mutex };
    unsafe { (*mutex).lock() };

    // const core_sel = sel.core(
    //     surface.core_surface.renderer_state.terminal.screens.active,
    // ) orelse return false;
    //
    // PORT NOTE: `sel.core` (E5 `Selection::core`) borrows the active screen through
    // `renderer_state.terminal` (a `*mut Terminal`); the `defer unlock` is realized
    // as in `rustty_surface_read_selection` (single unlock before return).
    let active = unsafe { (*(*core_surface).renderer_state.terminal).screens.active };
    let out = match sel.core(unsafe { &*active }) {
        None => false,
        // return readTextLocked(surface, core_sel, result);
        Some(core_sel) => unsafe { readTextLocked(surface, core_sel, result) },
    };

    unsafe { (*mutex).unlock() };
    out
}

/// # Safety
/// The caller must hold `core_surface.renderer_state.mutex`; `surface`/`result`
/// must be valid (see [`rustty_surface_read_text`]).
//
// fn readTextLocked(surface: *Surface, core_sel: terminal.Selection, result: *Text) bool { ... }
//
// PORT NOTE (non_snake_case): the reference identifier `readTextLocked` is preserved
// 1:1 (R2/R3); silence the snake-case lint at this fn.
#[allow(non_snake_case)]
unsafe fn readTextLocked(
    surface: *mut Surface,
    core_sel: CoreSelection,
    result: *mut Text,
) -> bool {
    // const core_surface = &surface.core_surface;
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };

    // Get our text directly from the core surface.
    // const text = core_surface.dumpTextLocked(global.alloc, core_sel) catch |err| {
    //     log.warn("error reading text err={}", .{err});
    //     return false;
    // };
    //
    // PORT NOTE: the reference `global.alloc` is the process-global allocator; the
    // rustty seam is Rust's global allocator (`allocator_api2 Global`). The returned
    // `text.text` is a `Vec<u8>` whose length EXCLUDES the sentinel (the vt
    // `selection_string` convention); the E5 `Text::deinit` frees `text_len + 1`
    // bytes, so a trailing NUL is appended + the buffer leaked below.
    let mut text = match unsafe { (*core_surface).dumpTextLocked(Global, core_sel) } {
        Ok(text) => text,
        Err(err) => {
            // log.warn("error reading text err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error reading text err={err:?}");
            return false;
        }
    };

    // const vp: CoreSurface.Text.Viewport = text.viewport orelse .{
    //     .tl_px_x = -1,
    //     .tl_px_y = -1,
    //     .offset_start = 0,
    //     .offset_len = 0,
    // };
    let vp: TextViewport = text.viewport.unwrap_or(TextViewport {
        tl_px_x: -1.0,
        tl_px_y: -1.0,
        offset_start: 0,
        offset_len: 0,
    });

    // result.* = .{
    //     .tl_px_x = vp.tl_px_x,
    //     .tl_px_y = vp.tl_px_y,
    //     .offset_start = vp.offset_start,
    //     .offset_len = vp.offset_len,
    //     .text = text.text.ptr,
    //     .text_len = text.text.len,
    // };
    //
    // PORT NOTE: the reference `[:0]const u8` allocation already carries the sentinel;
    // the rustty `Vec<u8>` does not, so we push the NUL and leak the `Box<[u8]>` (of
    // exactly `text_len + 1` bytes) that the E5 `Text::deinit` reconstructs. `text_len`
    // is the DATA length (pre-push), matching the reference `text.text.len`.
    let text_len = text.text.len();
    // text.text.ptr / text.text.len  (append the sentinel + leak for the C host)
    text.text.push(0);
    let ptr = Box::leak(text.text.into_boxed_slice()).as_ptr();
    // SAFETY: `result` is a valid writable `Text` (caller contract).
    unsafe {
        *result = Text {
            tl_px_x: vp.tl_px_x,
            tl_px_y: vp.tl_px_y,
            offset_start: vp.offset_start,
            offset_len: vp.offset_len,
            text: ptr as *const c_char,
            text_len,
        };
    }

    // return true;
    true
}

/// C: rustty_surface_free_text
///
/// # Safety
/// `ptr` must point to a [`Text`] previously written by [`rustty_surface_read_text`]/
/// [`rustty_surface_read_selection`] and not already freed. `_surface` is unused.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_free_text(_surface: *mut Surface, ptr: *mut Text) {
    // ptr.deinit();
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).deinit() };
}

/// C: rustty_surface_refresh
///
/// Tell the surface that it needs to schedule a render
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_refresh(surface: *mut Surface) {
    // surface.refresh();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).refresh() };
}

/// C: rustty_surface_draw
///
/// Tell the surface that it needs to schedule a render
/// call as soon as possible (NOW if possible).
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
//
// PORT NOTE: `draw` reaches the Phase-3 GPU renderer leaf at RUNTIME (backend
// `Surface::draw` → core `Surface::draw` → `renderer_seam` `unimplemented!()`); it
// compiles + links now and panics only when driven against a live surface.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_draw(surface: *mut Surface) {
    // surface.draw();
    // SAFETY: `surface` validity is the caller's contract. W5 (`W5_DESIGN §3.5`):
    // `Surface::draw` is now raw-self, so call it on the raw ptr directly (no
    // `(*surface)` auto-ref, which would form a protected `&mut Surface`).
    unsafe { Surface::draw(surface) };
}

/// C: rustty_surface_set_size
///
/// Update the size of a surface. This will trigger resize notifications
/// to the pty and the renderer.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_size(surface: *mut Surface, w: u32, h: u32) {
    // surface.updateSize(w, h);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).updateSize(w, h) };
}

/// C: rustty_surface_size
///
/// Return the size information a surface has.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_size(surface: *mut Surface) -> SurfaceSize {
    // const grid_size = surface.core_surface.size.grid();
    // SAFETY: `surface` validity is the caller's contract.
    let grid_size = unsafe { (*surface).core_surface.size.grid() };
    // return .{
    //     .columns = grid_size.columns,
    //     .rows = grid_size.rows,
    //     .width_px = surface.core_surface.size.screen.width,
    //     .height_px = surface.core_surface.size.screen.height,
    //     .cell_width_px = surface.core_surface.size.cell.width,
    //     .cell_height_px = surface.core_surface.size.cell.height,
    // };
    let size = unsafe { (*surface).core_surface.size };
    SurfaceSize {
        columns: grid_size.columns,
        rows: grid_size.rows,
        width_px: size.screen.width,
        height_px: size.screen.height,
        cell_width_px: size.cell.width,
        cell_height_px: size.cell.height,
    }
}

/// C: rustty_surface_foreground_pid
///
/// Returns the PID of the foreground process for the surface PTY.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_foreground_pid(surface: *mut Surface) -> u64 {
    // return surface.core_surface.getProcessInfo(.foreground_pid) orelse 0;
    //
    // PORT NOTE: the reference comptime `getProcessInfo(.foreground_pid)` selector is
    // the core `getForegroundPid` half (the core comptime-return split).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.getForegroundPid() }.unwrap_or(0)
}

/// C: rustty_surface_tty_name
///
/// Returns the PTY name for the surface. The returned string must be
/// freed by the caller via rustty_string_free.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_tty_name(surface: *mut Surface) -> String {
    // const tty_name = surface.core_surface.getProcessInfo(.tty_name) orelse return .empty;
    //
    // PORT NOTE: the `.tty_name` half of the core comptime `getProcessInfo` split
    // (`getTtyName` → `Option<&CStr>`); its bytes are copied out immediately (the
    // reference `dupeZ`), releasing the borrow.
    // SAFETY: `surface` validity is the caller's contract.
    let copy: Vec<u8> = match unsafe { (*surface).core_surface.getTtyName() } {
        Some(tty_name) => tty_name.to_bytes().to_vec(),
        None => return String::empty(),
    };

    // const copy = surface.app.core_app.alloc.dupeZ(u8, tty_name) catch |err| {
    //     log.err("error allocating tty name err={}", .{err});
    //     return .empty;
    // };
    // return .fromSlice(copy);
    //
    // PORT NOTE: `dupeZ` is the `to_vec()` copy above; Rust's `Vec` allocation is
    // infallible, so the reference `catch |err| return .empty` alloc-error arm is
    // unreachable/elided. `.fromSlice(copy)` leaks the NUL-terminated copy as a
    // sentinel `String` (freed by the host via `rustty_string_free`).
    String::from_slice(copy, true)
}

/// C: rustty_surface_set_color_scheme
///
/// Update the color scheme of the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_color_scheme(surface: *mut Surface, scheme_raw: c_int) {
    // const scheme = std.meta.intToEnum(apprt.ColorScheme, scheme_raw) catch {
    //     log.warn(
    //         "invalid color scheme to ghostty_surface_set_color_scheme value={}",
    //         .{scheme_raw},
    //     );
    //     return;
    // };
    //
    // PORT NOTE: `apprt.ColorScheme` is `enum(u2) { light = 0, dark = 1 }`; the
    // `intToEnum` guard lives here on the raw `c_int` crossing the C boundary (the
    // E6 `rustty_app_set_color_scheme` precedent).
    //
    // PORT NOTE (task-14d-3, INVERSE PARITY DEFECT — a rustty-ONLY emission,
    // now REMOVED): the commented reference call above is from a SUPERSEDED
    // oracle revision. At pin 20abdb50a this guard reads
    // `const scheme = std.enums.fromInt(apprt.ColorScheme, scheme_raw) orelse
    // return;` (embedded.zig:1992) — SILENT. The port emitted a warning here that no
    // pinned oracle revision emits, which is the same defect shape S50-F4
    // records and the same one this file's `rustty_surface_mouse_pressure`
    // already had removed for exactly this reason. The comment above is kept
    // (R1) as the record of what upstream used to do.
    let scheme = match scheme_raw {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => return,
    };

    // surface.colorSchemeCallback(scheme);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).colorSchemeCallback(scheme) };
}

/// C: rustty_surface_set_content_scale
///
/// Update the content scale of the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_content_scale(surface: *mut Surface, x: f64, y: f64) {
    // surface.updateContentScale(x, y);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).updateContentScale(x, y) };
}

/// C: rustty_surface_set_focus
///
/// Update the focused state of a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_focus(surface: *mut Surface, focused: bool) {
    // surface.focusCallback(focused);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).focusCallback(focused) };
}

/// C: rustty_surface_set_occlusion
///
/// Update the occlusion state of a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_occlusion(surface: *mut Surface, visible: bool) {
    // surface.occlusionCallback(visible);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).occlusionCallback(visible) };
}

// ===========================================================================
// CAPI input exports (embedded.zig 1758-1920, chunk E8): the raw key/text/preedit
// and mouse (button/pos/scroll/pressure/ime) sinks. Each wrapper marshals the wire
// form the host passes (`c_int` mods reinterpreted as `input.Mods`, `input.Mouse*`
// value types) and forwards to the ported `rustty-apprt` backend `Surface` callback
// methods (E3) or the core surface directly. Every export here needs a LIVE surface
// (created via `rustty_surface_new`, the Phase-3 GPU-leaf boundary), so it links now
// and reaches the renderer leaf only when driven — NO new `unimplemented!()` leaf in
// apprt-c (DEFERRALS.md, E7 precedent).
// ===========================================================================

/// C: rustty_surface_key_translation_mods
///
/// Filter the mods if necessary. This handles settings such as
/// `macos-option-as-alt`. The filtered mods should be used for
/// key translation but should NOT be sent back via the `_key`
/// function -- the original mods should be used for that.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key_translation_mods(
    surface: *mut Surface,
    mods_raw: c_int,
) -> c_int {
    // const mods: input.Mods = @bitCast(@as(
    //     input.Mods.Backing,
    //     @truncate(@as(c_uint, @bitCast(mods_raw))),
    // ));
    //
    // PORT NOTE: reinterpret the signed `c_int` as unsigned (`@bitCast` ->
    // `as c_uint`), truncate to the `u16` `Mods.Backing` (`@truncate` ->
    // `as ModsBacking`), then reinterpret the backing bits as `Mods`
    // (`@bitCast` -> `Mods::from_int`), the E5 `KeyEvent::keyEvent` precedent.
    let mods = Mods::from_int(mods_raw as core::ffi::c_uint as ModsBacking);
    // const result = mods.translation(
    //     surface.core_surface.config.macos_option_as_alt orelse
    //         surface.app.keyboardLayout().detectOptionAsAlt(),
    // );
    //
    // PORT NOTE: `Mods.translation` is ported as the `rustty-input` free fn
    // `key_mods::translation` (it needs the apprt-layer `OptionAsAlt`, so it cannot
    // live on `rustty-vt`'s `Mods`). The `orelse` is lazy — `keyboardLayout()`
    // (`detectOptionAsAlt`) is only evaluated when the config value is unset —
    // mirrored with `unwrap_or_else`.
    // SAFETY: `surface` validity is the caller's contract.
    let option_as_alt = unsafe { (*surface).core_surface.config.macos_option_as_alt }
        // SAFETY: `surface.app` is the stable backend `App` pointer set at init.
        .unwrap_or_else(|| unsafe { (*(*surface).app).keyboardLayout() }.detect_option_as_alt());
    let result = translation(mods, option_as_alt);
    // return @intCast(@as(input.Mods.Backing, @bitCast(result)));
    //
    // PORT NOTE: reinterpret `result` as its `u16` backing (`@bitCast` -> `int()`)
    // then widen to the `c_int` the ABI returns (`@intCast`).
    result.int() as c_int
}

/// C: rustty_surface_key
///
/// Send this for raw keypresses (i.e. the keyDown event on macOS).
/// This will handle the keymap translation and send the appropriate
/// key and char events.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `event.text`,
/// if non-null, must be a valid NUL-terminated C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key(surface: *mut Surface, event: KeyEvent) -> bool {
    // return surface.app.keyEvent(
    //     .{ .surface = surface },
    //     event.keyEvent(),
    // ) catch |err| {
    //     log.warn("error processing key event err={}", .{err});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    // D11 (P3-W6 T2): `event.action` is the raw C integer; `keyEvent()` narrows
    // it and is `None` for an undeclared value, which takes the same
    // "error processing key event" exit the reference uses for a failed event.
    let kev = match unsafe { event.keyEvent() } {
        Some(v) => v,
        None => {
            log::warn!(target: LOG_SCOPE, "error processing key event: action out of range");
            return false;
        }
    };
    // SAFETY: `surface`/`surface.app` validity is the caller's contract.
    match unsafe { (*(*surface).app).keyEvent(KeyTarget::Surface(surface), kev) } {
        Ok(v) => v,
        Err(err) => {
            log::warn!(target: LOG_SCOPE, "error processing key event err={err:?}");
            false
        }
    }
}

/// C: rustty_surface_key_is_binding
///
/// Returns true if the given key event would trigger a binding
/// if it were sent to the surface right now. The "right now"
/// is important because things like trigger sequences are only
/// valid until the next key event.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `event.text`,
/// if non-null, must be a valid NUL-terminated C string that outlives the call;
/// `c_flags`, if non-null, must be a valid writable [`FlagsC`] pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key_is_binding(
    surface: *mut Surface,
    event: KeyEvent,
    c_flags: *mut FlagsC,
) -> bool {
    // const core_event = event.keyEvent().core() orelse {
    //     log.warn("error processing key event", .{});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    // D11 (P3-W6 T2): `keyEvent()` narrows `event.action`; `None` folds into the
    // same "error processing key event" exit the reference already has here.
    let core_event = match unsafe { event.keyEvent() }.and_then(|e| e.core()) {
        Some(v) => v,
        None => {
            log::warn!(target: LOG_SCOPE, "error processing key event");
            return false;
        }
    };

    // const flags = surface.core_surface.keyEventIsBinding(
    //     core_event,
    // ) orelse return false;
    // SAFETY: `surface` validity is the caller's contract.
    let flags = match unsafe { (*surface).core_surface.keyEventIsBinding(core_event) } {
        Some(v) => v,
        None => return false,
    };
    // if (c_flags) |ptr| ptr.* = flags.cval();
    //
    // PORT NOTE: the reference `?*` nullable pointer is a raw `*mut FlagsC`; the
    // `if (c_flags) |ptr|` capture is a null-check before the write.
    if !c_flags.is_null() {
        // SAFETY: `c_flags` is non-null and the caller's contract makes it writable.
        unsafe { *c_flags = flags.cval() };
    }
    // return true;
    true
}

/// C: rustty_surface_text
///
/// Send raw text to the terminal. This is treated like a paste
/// so this isn't useful for sending escape sequences. For that,
/// individual key input should be used.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `ptr` must
/// point to at least `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_text(surface: *mut Surface, ptr: *const u8, len: usize) {
    // surface.textCallback(ptr[0..len]);
    // SAFETY: `ptr[0..len]` validity is the caller's contract.
    let text = unsafe { core::slice::from_raw_parts(ptr, len) };
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).textCallback(text) };
}

/// C: rustty_surface_preedit
///
/// Set the preedit text for the surface. This is used for IME
/// composition. If the length is 0, then the preedit text is cleared.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `ptr` must
/// point to at least `len` readable bytes when `len != 0`.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_preedit(surface: *mut Surface, ptr: *const u8, len: usize) {
    // surface.preeditCallback(if (len == 0) null else ptr[0..len]);
    //
    // PORT NOTE: the reference `?[]const u8` — `null` when `len == 0`, else the
    // `ptr[0..len]` slice — maps to `Option<&[u8]>`.
    let preedit = if len == 0 {
        None
    } else {
        // SAFETY: `ptr[0..len]` validity is the caller's contract (len != 0 here).
        Some(unsafe { core::slice::from_raw_parts(ptr, len) })
    };
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).preeditCallback(preedit) };
}

/// C: rustty_surface_mouse_captured
///
/// Returns true if the surface currently has mouse capturing
/// enabled.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_captured(surface: *mut Surface) -> bool {
    // return surface.core_surface.mouseCaptured();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.mouseCaptured() }
}

/// C: rustty_surface_mouse_button
///
/// Tell the surface that it needs to schedule a render
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_button(
    surface: *mut Surface,
    // D11 (P3-W6 T2): inbound as the C integers, narrowed below.
    action: c_int,
    button: c_int,
    mods: c_int,
) -> bool {
    // D11: `include/rustty-embed.h` declares both enums with a `_MAX_VALUE`
    // enumerator, so an out-of-range value is reachable from conforming C and
    // materializing it as a `#[repr(C)]` enum in the prologue would be UB.
    // This export returns `bool`; `false` ("event not consumed") is the same
    // answer the reference gives for an event it cannot act on.
    let (action, button) = match (
        mouse::ButtonState::from_c_int(action),
        mouse::Button::from_c_int(button),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    // return surface.mouseButtonCallback(
    //     action,
    //     button,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    //
    // PORT NOTE: the `c_int` mods reinterpret is the `key_translation_mods` /
    // `KeyEvent::keyEvent` precedent (`Mods::from_int(mods as c_uint as Backing)`).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).mouseButtonCallback(
            action,
            button,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    }
}

/// C: rustty_surface_mouse_pos
///
/// Update the mouse position within the view.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_pos(
    surface: *mut Surface,
    x: f64,
    y: f64,
    mods: c_int,
) {
    // surface.cursorPosCallback(
    //     x,
    //     y,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).cursorPosCallback(
            x,
            y,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    };
}

/// C: rustty_surface_mouse_scroll
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_scroll(
    surface: *mut Surface,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // surface.scrollCallback(
    //     x,
    //     y,
    //     @bitCast(@as(u8, @truncate(@as(c_uint, @bitCast(scroll_mods))))),
    // );
    //
    // PORT NOTE: reinterpret the signed `c_int` as unsigned, truncate to the `u8`
    // `ScrollMods` backing, then reinterpret the bits as `ScrollMods`
    // (`@bitCast` -> `ScrollMods::from_int`, the vt `Mods::from_int` idiom).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).scrollCallback(
            x,
            y,
            mouse::ScrollMods::from_int(scroll_mods as core::ffi::c_uint as u8),
        )
    };
}

/// C: rustty_surface_mouse_pressure
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_pressure(
    surface: *mut Surface,
    stage_raw: u32,
    pressure: f64,
) {
    // const stage = std.enums.fromInt(input.MousePressureStage, stage_raw) orelse return;
    //
    // PORT NOTE: `std.enums.fromInt` is the fallible tag guard on the raw `u32`
    // crossing the C boundary (the E7 `set_color_scheme` precedent).
    // `MousePressureStage` is `enum(u8) { none=0, normal=1, deep=2 }`. The guard is
    // SILENT: an out-of-range `stage_raw` returns with NO diagnostic. (The reference
    // did `std.meta.intToEnum ... catch { log.warn("invalid mouse pressure stage
    // value={}", .{stage_raw}); return; }` until upstream e8525c0fd ("Update to Zig
    // 0.16.0") replaced it with the silent `fromInt ... orelse return` that the
    // pinned oracle carries; the port had tracked that stale revision and emitted an
    // `eprintln!` here, which no oracle revision ever did — Ghostty's `log.warn`
    // routes through `std_options.logFn`, not raw stderr.)
    let stage = match stage_raw {
        0 => mouse::PressureStage::None,
        1 => mouse::PressureStage::Normal,
        2 => mouse::PressureStage::Deep,
        _ => return,
    };

    // surface.mousePressureCallback(stage, pressure);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).mousePressureCallback(stage, pressure) };
}

/// C: rustty_surface_ime_point
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `x`/`y`/
/// `width`/`height` must be valid writable `f64` pointers.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_ime_point(
    surface: *mut Surface,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) {
    // const pos = surface.core_surface.imePoint();
    // SAFETY: `surface` validity is the caller's contract.
    let pos = unsafe { (*surface).core_surface.imePoint() };
    // x.* = pos.x;
    // y.* = pos.y;
    // width.* = pos.width;
    // height.* = pos.height;
    // SAFETY: `x`/`y`/`width`/`height` writability is the caller's contract.
    unsafe {
        *x = pos.x;
        *y = pos.y;
        *width = pos.width;
        *height = pos.height;
    }
}

// ===========================================================================
// CAPI surface-action + clipboard + inspector exports (embedded.zig 1922-2100,
// chunk E9): the 6 surface-action sinks (request_close / split / split_focus /
// split_resize / split_equalize / binding_action), the 1 clipboard-completion
// sink, and the 10 inspector sinks (surface_inspector / inspector_free +
// set_size / set_content_scale + mouse_button / mouse_pos / mouse_scroll / key /
// text / set_focus). The surface-action exports forward to the backend `App`'s
// `perform_action` (the apprt-api trait — E6 `open_config` precedent, here with a
// `Target::Surface`) or the core surface directly; the clipboard sink forwards to
// the backend `Surface::completeClipboardRequest` (E3); the inspector sinks forward
// to the E4 `Inspector` methods. Every export here needs a LIVE surface (created via
// `rustty_surface_new`), and the inspector sinks additionally need a LIVE
// `Inspector` — which `rustty_surface_inspector` now really builds: the whole E4
// chain (`Inspector::{init,deinit,initMetal,renderMetal}`, `Backend::deinit`, the
// six input sinks and `newFrame`) is LIVE as of WS6. NO `unimplemented!()` leaf is
// added in apprt-c (DEFERRALS.md, E7/E8 precedent).
// ===========================================================================

/// C: rustty_surface_request_close
///
/// Request that the surface become closed. This will go through the
/// normal trigger process that a close surface input binding would.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_request_close(ptr: *mut Surface) {
    // ptr.core_surface.close();
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).core_surface.close() };
}

/// C: rustty_surface_split
///
/// Request that the surface split in the given direction.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split(ptr: *mut Surface, direction: SplitDirection) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .new_split,
    //     direction,
    // ) catch |err| {
    //     log.err("error creating new split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference `*App` method `performAction(.{ .surface =
    // &ptr.core_surface }, .new_split, direction)` maps to the `apprt_api::App` trait
    // `perform_action(Target::Surface(&core_surface), Action::NewSplit(direction))`
    // (the E6 `open_config` precedent — here the `Target` carries the surface, so `C`
    // is inferred as `CoreSurface`). The `bool` result is discarded (reference `_ =`).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::NewSplit(direction),
        )
    } {
        log::error!(target: LOG_SCOPE, "error creating new split err={err:?}");
    }
}

/// C: rustty_surface_split_focus
///
/// Focus on the next split (if any).
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_focus(ptr: *mut Surface, direction: GotoSplit) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .goto_split,
    //     direction,
    // ) catch |err| {
    //     log.err("error creating new split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference log message is "error creating new split" verbatim —
    // a copy-paste of `ghostty_surface_split`'s message; preserved faithfully (R3).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::GotoSplit(direction),
        )
    } {
        log::error!(target: LOG_SCOPE, "error creating new split err={err:?}");
    }
}

/// C: rustty_surface_split_resize
///
/// Resize the current split by moving the split divider in the given
/// direction. `direction` specifies which direction the split divider will
/// move relative to the focused split. `amount` is a fractional value
/// between 0 and 1 that specifies by how much the divider will move.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_resize(
    ptr: *mut Surface,
    direction: ResizeSplitDirection,
    amount: u16,
) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .resize_split,
    //     .{ .direction = direction, .amount = amount },
    // ) catch |err| {
    //     log.err("error resizing split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: `.resize_split`'s value is the `ResizeSplit` struct, reassembled by
    // field name from the two params (`.{ .direction, .amount }`).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::ResizeSplit(ResizeSplit { direction, amount }),
        )
    } {
        log::error!(target: LOG_SCOPE, "error resizing split err={err:?}");
    }
}

/// C: rustty_surface_split_equalize
///
/// Equalize the size of all splits in the current window.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_equalize(ptr: *mut Surface) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .equalize_splits,
    //     {},
    // ) catch |err| {
    //     log.err("error equalizing splits err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference void value `{}` for `.equalize_splits` maps to the
    // fieldless `Action::EqualizeSplits` variant.
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::EqualizeSplits,
        )
    } {
        log::error!(target: LOG_SCOPE, "error equalizing splits err={err:?}");
    }
}

/// C: rustty_surface_binding_action
///
/// Invoke an action on the surface.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`]; `action_ptr`
/// must point to `action_len` valid bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_binding_action(
    ptr: *mut Surface,
    action_ptr: *const u8,
    action_len: usize,
) -> bool {
    // const action_str = action_ptr[0..action_len];
    // SAFETY: `action_ptr`/`action_len` validity is the caller's contract.
    let action_str = unsafe { core::slice::from_raw_parts(action_ptr, action_len) };

    // const action = input.Binding.Action.parse(action_str) catch |err| {
    //     log.err("error parsing binding action action={s} err={}", .{ action_str, err });
    //     return false;
    // };
    let action = match binding::Action::parse(action_str) {
        Ok(action) => action,
        Err(err) => {
            log::error!(
                target: LOG_SCOPE,
                "error parsing binding action action={} err={err:?}",
                // PORT NOTE: fully-qualified — the crate-local C-API `String` (the
                // `main_c.zig` heap-string type) shadows `std::string::String` here.
                std::string::String::from_utf8_lossy(action_str),
            );
            return false;
        }
    };

    // return ptr.core_surface.performBindingAction(action) catch |err| {
    //     log.err("error performing binding action action={f} err={}", .{ action, err });
    //     return false;
    // };
    //
    // PORT NOTE: Zig passes the `action` union to `performBindingAction` BY VALUE (a
    // copy), leaving the local `action` live for the `catch` error log. Rust's
    // `performBindingAction(&mut self, action: binding::Action)` CONSUMES its arg, so
    // `action.clone()` reproduces Zig's by-value copy and keeps `action` available for
    // the error branch's `action={f}` (rendered via `Debug`, the faithful analog of
    // the reference's custom `{f}` `Action.format`).
    // SAFETY: `ptr` validity is the caller's contract.
    match unsafe { (*ptr).core_surface.performBindingAction(action.clone()) } {
        Ok(v) => v,
        Err(err) => {
            log::error!(target: LOG_SCOPE, "error performing binding action action={action:?} err={err:?}");
            false
        }
    }
}

/// C: rustty_surface_complete_clipboard_request
///
/// Complete a clipboard read request started via the read callback
/// with the representations that could be served and, if requested,
/// the listing of available MIME types. All memory is borrowed for
/// the duration of the call. This can only be called once for a given
/// request. Once it is called with a request the request pointer will
/// be invalidated.
///
/// To deny a request use rustty_surface_deny_clipboard_request
/// instead.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`]; `complete`
/// must point at a valid `rustty_clipboard_complete_s` whose borrowed memory
/// lives for the call; `state` must be the request pointer handed to the
/// host by the read callback (it is invalidated by this call).
//
// export fn ghostty_surface_complete_clipboard_request(
//     ptr: *Surface,
//     complete: *const ClipboardComplete,
//     state: *apprt.ClipboardRequest,
// ) void {
//     ptr.completeClipboardRequest(complete, state);
// }
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_complete_clipboard_request(
    ptr: *mut Surface,
    complete: *const ClipboardComplete,
    state: *mut ClipboardRequest,
) {
    // SAFETY: `ptr`/`complete`/`state` validity is the caller's contract.
    unsafe {
        (*ptr).completeClipboardRequest(complete, state);
    }
}

/// C: rustty_surface_deny_clipboard_request
///
/// Deny a clipboard read request started via the read callback,
/// e.g. because the user rejected a confirmation prompt. Request
/// types whose protocol expects an answer have their denial reply
/// written to the pty. The request pointer is invalidated.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`]; `state`
/// must be the request pointer handed to the host by the read callback (it is
/// invalidated by this call).
//
// export fn ghostty_surface_deny_clipboard_request(
//     ptr: *Surface,
//     state: *apprt.ClipboardRequest,
// ) void {
//     ptr.denyClipboardRequest(state);
// }
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_deny_clipboard_request(
    ptr: *mut Surface,
    state: *mut ClipboardRequest,
) {
    // SAFETY: `ptr`/`state` validity is the caller's contract.
    unsafe {
        (*ptr).denyClipboardRequest(state);
    }
}

/// C: rustty_surface_inspector
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_inspector(ptr: *mut Surface) -> *mut Inspector {
    // return ptr.initInspector() catch |err| {
    //     log.err("error initializing inspector err={}", .{err});
    //     return null;
    // };
    //
    // PORT NOTE: `?*Inspector` → a nullable `*mut Inspector` (cbindgen emits the raw
    // pointer; `null` for the error case). `initInspector` -> `Inspector::init` is
    // FULLY LIVE (WS6): it creates the per-Surface Dear ImGui context, runs
    // `CoreInspector::setup()`, and activates the core inspector. The `null` return
    // is the reference's own init-failure path (the macOS host null-guards it in
    // SurfaceView_AppKit.swift), NOT a port placeholder.
    // SAFETY: `ptr` validity is the caller's contract.
    match unsafe { (*ptr).initInspector() } {
        Ok(v) => v,
        Err(err) => {
            log::error!(target: LOG_SCOPE, "error initializing inspector err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// C: rustty_inspector_free
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_free(ptr: *mut Surface) {
    // ptr.freeInspector();
    //
    // PORT NOTE: the reference `ghostty_inspector_free` takes a `*Surface` (NOT a
    // `*Inspector`) — it frees the surface-owned inspector via `freeInspector` (E4).
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).freeInspector() };
}

/// C: rustty_inspector_set_size
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_size(ptr: *mut Inspector, w: u32, h: u32) {
    // ptr.updateSize(w, h);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).updateSize(w, h) };
}

/// C: rustty_inspector_set_content_scale
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_content_scale(ptr: *mut Inspector, x: f64, y: f64) {
    // ptr.updateContentScale(x, y);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).updateContentScale(x, y) };
}

/// C: rustty_inspector_mouse_button
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_button(
    ptr: *mut Inspector,
    // D11 (P3-W6 T2): inbound as the C integers, narrowed below.
    action: c_int,
    button: c_int,
    mods: c_int,
) {
    // D11: see `rustty_inspector_key`. `void` return, so an undeclared value is
    // dropped rather than dispatched on.
    let (action, button) = match (
        mouse::ButtonState::from_c_int(action),
        mouse::Button::from_c_int(button),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return,
    };
    // ptr.mouseButtonCallback(
    //     action,
    //     button,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    //
    // PORT NOTE: the `c_int` mods reinterpret is the E8 `mouse_button` precedent
    // (`Mods::from_int(mods as c_uint as Backing)`).
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe {
        (*ptr).mouseButtonCallback(
            action,
            button,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    };
}

/// C: rustty_inspector_mouse_pos
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_pos(ptr: *mut Inspector, x: f64, y: f64) {
    // ptr.cursorPosCallback(x, y);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).cursorPosCallback(x, y) };
}

/// C: rustty_inspector_mouse_scroll
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_scroll(
    ptr: *mut Inspector,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // ptr.scrollCallback(
    //     x,
    //     y,
    //     @bitCast(@as(u8, @truncate(@as(c_uint, @bitCast(scroll_mods))))),
    // );
    //
    // PORT NOTE: the E8 `mouse_scroll` precedent — reinterpret the `c_int` as unsigned,
    // truncate to the `u8` `ScrollMods` backing, then reinterpret the bits.
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe {
        (*ptr).scrollCallback(
            x,
            y,
            mouse::ScrollMods::from_int(scroll_mods as core::ffi::c_uint as u8),
        )
    };
}

/// C: rustty_inspector_key
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_key(
    ptr: *mut Inspector,
    // D11 (P3-W6 T2): inbound as the C integers, narrowed below.
    action: c_int,
    key: c_int,
    c_mods: c_int,
) {
    // D11: `include/rustty-embed.h` declares both of these enums with a
    // `_MAX_VALUE` enumerator, so a conforming C caller can write a value
    // outside the declared range. Materializing it as the Rust `#[repr(C)]`
    // enum in the callee prologue would be instant UB, so both cross as
    // `c_int`. This export returns `void`; an undeclared value takes the same
    // "log and return" disposition the reference uses for a failed key event
    // just below.
    let (action, key) = match (key::Action::from_c_int(action), key::Key::from_c_int(key)) {
        (Some(a), Some(k)) => (a, k),
        _ => {
            log::error!(target: LOG_SCOPE, "error processing key event: action/key out of range");
            return;
        }
    };
    // ptr.keyCallback(
    //     action,
    //     key,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(c_mods))),
    //     )),
    // ) catch |err| {
    //     log.err("error processing key event err={}", .{err});
    //     return;
    // };
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*ptr).keyCallback(
            action,
            key,
            Mods::from_int(c_mods as core::ffi::c_uint as ModsBacking),
        )
    } {
        log::error!(target: LOG_SCOPE, "error processing key event err={err:?}");
    }
}

/// C: rustty_inspector_text
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`];
/// `str` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_text(ptr: *mut Inspector, str: *const c_char) {
    // ptr.textCallback(std.mem.sliceTo(str, 0));
    //
    // PORT NOTE: `str: [*:0]const u8` + `std.mem.sliceTo(str, 0)` → a borrowed `&CStr`
    // (`CStr::from_ptr`); the backend `textCallback` re-hands `text.ptr` to imgui (the
    // NUL sentinel is load-bearing, E4).
    // SAFETY: `ptr`/`str` validity is the caller's contract.
    unsafe { (*ptr).textCallback(CStr::from_ptr(str)) };
}

/// C: rustty_inspector_set_focus
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_focus(ptr: *mut Inspector, focused: bool) {
    // ptr.focusCallback(focused);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).focusCallback(focused) };
}

// ===========================================================================
// E10 — misc + Darwin-gated exports (embedded.zig 2108-2250)
//
// The final chunk of the 71-export libghostty C-ABI: the platform-conditional
// tail of the reference `CAPI` decl. After E10 ALL 71 `ghostty_*` (`rustty_*`)
// exports exist. Per EMBEDDED_BLUEPRINT §1/§3 (misc + Darwin-gated groups) and
// the E0 deferral (DEFERRALS.md: "macOS objc/Metal/CGS ... unimplemented!()
// leaves are CREATED by E3/E4/E10"), E10 is EXPLICITLY the leaf-CREATING chunk
// for the macOS objc/CGS/Metal/coretext seams — UNLIKE the pure-marshaling
// E6-E9, it lands `unimplemented!()` leaves (all `#[cfg(target_os = "macos")]`
// or macOS-branch-gated). Do NOT fabricate objc/CGS/Metal/coretext logic — the
// reference bodies are preserved VERBATIM as comments (R1 no-loss) and the
// Phase-3 targets are tracked in DEFERRALS.md.
// ===========================================================================

/// C: rustty_set_window_background_blur
///
/// Sets the window background blur on macOS to the desired value.
/// I do this in Zig as an extern function because I don't know how to
/// call these functions in Swift.
///
/// This uses an undocumented, non-public API because this is what
/// every terminal appears to use, including Terminal.app.
///
/// # Safety
/// `app` must be a valid [`App`] pointer returned by [`rustty_app_new`]; `window`
/// must be a valid `NSWindow` id (macOS only). On non-macOS this is a no-op.
//
// export fn ghostty_set_window_background_blur(app: *App, window: *anyopaque) void { ... }
//
// PORT NOTE [ADV-FIX 3]: this export is NOT pure and is NOT in the runtime-safe
// list — its macOS body does an objc `msgSend` (`windowNumber`) + a private,
// undocumented CoreGraphics call (`CGSSetWindowBackgroundBlurRadius`). It is a
// macOS objc/CGS Phase-3 leaf. The reference `if (comptime target != .macos)
// return;` comptime guard splits by `#[cfg]`: on non-macOS the whole body is the
// no-op early return; on macOS the objc/CGS body is an `unimplemented!()` leaf
// (no objc2/CGS dep this milestone — blueprint §5/§8.1). The smoke KAT (E11)
// MUST NOT call it. The export EXISTS on all platforms (matching the reference,
// which `@export`s it unconditionally — only its BODY is comptime-gated).
#[no_mangle]
pub unsafe extern "C" fn rustty_set_window_background_blur(app: *mut App, window: *mut c_void) {
    // This is only supported on macOS
    // if (comptime builtin.target.os.tag != .macos) return;
    #[cfg(not(target_os = "macos"))]
    {
        // (non-macOS: the reference `comptime` guard returns immediately — no-op.)
        let _ = (app, window);
    }
    #[cfg(target_os = "macos")]
    {
        // const config = &app.config;
        //
        // // Do nothing if we don't have background transparency enabled
        // if (config.@"background-opacity" >= 1.0) return;
        //
        // const nswindow = objc.Object.fromId(window);
        // _ = CGSSetWindowBackgroundBlurRadius(
        //     CGSDefaultConnectionForThread(),
        //     nswindow.msgSend(usize, objc.sel("windowNumber"), .{}),
        //     @intCast(config.@"background-blur".cval()),
        // );
        //
        // SAFETY: `app` is a live *mut App (the C host is single-threaded); `window`
        // is a live NSWindow id, only handed to msg_send!/CGS (never deref'd as a
        // Rust reference — no aliasing obligation).
        let config = unsafe { &*(*app).config.get() };

        // Do nothing if we don't have background transparency enabled.
        if config.background_opacity >= 1.0 {
            return;
        }

        let nswindow: *mut objc2::runtime::AnyObject = window.cast();
        // PARITY: `[nswindow windowNumber]` returns `NSInteger`. The reference reads
        // it straight as `usize` (`nswindow.msgSend(usize, sel("windowNumber"), .{})`)
        // and hands it to the CGS `window` arg. We read the ObjC value with its true
        // signed type (`isize`/`NSInteger`) and then `as usize` — a same-width
        // REINTERPRET of the identical bit pattern the reference passes (window
        // numbers are always small positive ints, so the value is unchanged either
        // way; the `as usize` documents that we are matching the reference's
        // `msgSend(usize, …)` byte-for-byte, not performing a conversion).
        let window_number: isize = unsafe { objc2::msg_send![nswindow, windowNumber] };
        let _ = unsafe {
            CGSSetWindowBackgroundBlurRadius(
                CGSDefaultConnectionForThread(),
                window_number as usize,
                // @intCast(config.@"background-blur".cval())
                rustty_config::value_types::background_blur_cval(config.background_blur)
                    as core::ffi::c_int,
            )
        };
    }
}

// /// See ghostty_set_window_background_blur
// extern "c" fn CGSSetWindowBackgroundBlurRadius(*anyopaque, usize, c_int) i32;
// extern "c" fn CGSDefaultConnectionForThread() *anyopaque;
//
// PORT NOTE: the two private CoreGraphics symbols (embedded.zig:2128-2130) — an
// undocumented, non-public SkyLight/CoreGraphics SPI (the same one Terminal.app
// uses). They are now REALIZED (W4-2) as a live `extern "C"` block. The explicit
// `#[link(name = "CoreGraphics", kind = "framework")]` on the block is what makes
// the cdylib link resolve them: it emits `-framework CoreGraphics` into
// `librustty.dylib`, from which both private symbols export. This is a
// self-contained link directive — it does NOT depend on any `objc2-core-graphics`
// dependency (that crate is dev-only, for the harness's `CGMainDisplayID`), and it
// does NOT rely on QuartzCore/Metal transitively pulling CoreGraphics (QuartzCore's
// `LC_LOAD_DYLIB` on CoreGraphics is not a reexport, and rustc's cdylib link does
// not pass `-undefined dynamic_lookup`, so the symbols would otherwise be
// unresolved). No crate exports these two symbols, so the manual extern is faithful
// to the reference decls.
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSSetWindowBackgroundBlurRadius(
        connection: *mut core::ffi::c_void,
        window: usize,
        radius: core::ffi::c_int,
    ) -> i32;
    fn CGSDefaultConnectionForThread() -> *mut core::ffi::c_void;
}

// // Darwin-only C APIs.
// const Darwin = struct { ... };
//
// PORT NOTE: the reference groups the six Darwin exports in a `const Darwin =
// struct {…}` and force-references it via `comptime { if isDarwin() _ = Darwin; }`
// (embedded.zig:1390-1396) so its `export fn`s are pulled into the C API only on
// Darwin. That comptime force-reference has no runtime analogue (ported as a
// comment note in `capi.rs`, E5): in rustty the six Darwin exports are plain
// `#[cfg(any(target_os = "macos", target_os = "ios"))]`-gated free functions the
// compiler includes automatically when targeting Darwin. Each is a Phase-3 leaf
// per its subsystem (renderer/xev for `set_display_id`, coretext for
// `quicklook_font`, renderer_state/terminal for `quicklook_word`, objc/Metal for
// the three `metal_*`).
//
// The gate is `isDarwin()` — macOS **OR** iOS — exactly as the reference's
// `builtin.target.os.tag.isDarwin()`, matching the isDarwin gating already used
// for `Platform`/`MacOS`/`IOS` in `rustty-apprt/src/embedded.rs:730,747` and the
// `rustty-os` `file.rs` precedent. iOS is a first-class host of this C API: the
// reference ships `macos/Sources/App/iOS/iOSApp.swift`, ported at the same path
// here, which calls `rustty_init` and drives `Rustty.Terminal` through the very
// same exports. None of the six bodies is macOS-exclusive (CoreText, dcimgui's
// Metal backend and the renderer mailbox all exist on iOS); the ONE genuinely
// macOS-only symbol pair, the private `CGS*` window-blur calls above, keeps its
// narrower `target_os = "macos"` gate — matching the reference, whose
// `ghostty_set_window_background_blur` body is guarded by
// `builtin.target.os.tag != .macos`, not by `isDarwin()`.
//
// NOT VERIFIED BY BUILD: this repo has no iOS toolchain target installed, so the
// `target_os = "ios"` arm is not compiled by any gate in the suite. The gate
// shape is what is under review here, not iOS codegen.

/// C: rustty_surface_set_display_id
///
/// # Safety
/// `ptr` must be a valid [`Surface`] pointer returned by [`rustty_surface_new`].
//
// export fn ghostty_surface_set_display_id(ptr: *Surface, display_id: u32) void { ... }
//
// PORT NOTE: pushes a `.macos_display_id` message to the renderer thread's
// mailbox and wakes it (`renderer_thread.wakeup.notify()`). This is now REALIZED
// (W4-3): W2/W3 supply exactly the consumed seams — a live surface with a real
// render thread (`renderer_thread.mailbox: *mut thread::Mailbox` +
// `renderer_thread.wakeup: xev::Async`), and the drain side is live
// (`renderer_seam.rs` `Message::MacosDisplayId(v) => set_macos_display_id(v)?`).
// Body preserved verbatim above the realized code.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_display_id(ptr: *mut Surface, display_id: u32) {
    // const surface = &ptr.core_surface;
    // _ = surface.renderer_thread.mailbox.push(
    //     .{ .macos_display_id = display_id },
    //     .{ .forever = {} },
    // );
    // surface.renderer_thread.wakeup.notify() catch {};
    //
    // SAFETY: `ptr` validity is the caller's contract. `renderer_thread.mailbox` is
    // a *mut Mailbox (a Box pointee OUTSIDE the Surface struct's Stacked-Borrows
    // coverage — the render-thread's raw-alias discipline); pushing through
    // `&*mailbox` while the render thread holds its own raw alias is the sanctioned
    // construct-then-spawn pattern (no `&mut *self` whole-struct reborrow).
    let surface = unsafe { &(*ptr).core_surface };
    let _ = unsafe { &*surface.renderer_thread.mailbox }.push(
        rustty_renderer::Message::MacosDisplayId(display_id),
        rustty_datastruct::blocking_queue::Timeout::Forever,
    );
    // surface.renderer_thread.wakeup.notify() catch {};
    let _ = surface.renderer_thread.wakeup.notify();
}

/// C: rustty_surface_quicklook_font
///
/// This returns a CTFontRef that should be used for quicklook
/// highlighted text. This is always the primary font in use
/// regardless of the selected text. If coretext is not in use
/// then this will return nothing.
///
/// # Safety
/// `ptr` must be a valid [`Surface`] pointer returned by [`rustty_surface_new`].
/// The returned value, if non-null, is a `CTFontRef` owned by the caller.
//
// export fn ghostty_surface_quicklook_font(ptr: *Surface) ?*anyopaque { ... }
//
// PORT NOTE (P2.2): reads the shared font grid's primary CoreText face and
// returns a `copyWithAttributes`-resized `CTFontRef` (`?*anyopaque` → nullable
// `*mut c_void`). It reaches the renderer's `font_grid` (`renderer_seam::Renderer
// .0.font_grid`) AND messages CoreText via objc2-core-text (`Face.font.attribute`/
// `copy_with_attributes`). The returned CTFont is +1 — the host balances it with a
// CFRelease (I16). The reference non-CoreText early return (`return null`) is
// elided: this export is Darwin-gated and the coretext backend is the Darwin
// default, so there is never a non-CoreText grid here. Body ported faithfully.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_quicklook_font(ptr: *mut Surface) -> *mut c_void {
    // The shared-grid + CoreText symbols are macOS-only; scope the imports to the
    // ported body (this whole export is Darwin-gated).
    use objc2_core_foundation::{CFNumber, CFNumberType, CFRetained};
    use objc2_core_text::{kCTFontSizeAttribute, CTFont};
    use rustty_apprt_api::Surface as ApprtSurface;
    use rustty_font_shaper::collection::Index;
    use rustty_font_shaper::{SharedGrid, Style};

    // // For non-CoreText we just return null.
    // if (comptime font.options.backend != .coretext) {
    //     return null;
    // }
    //
    // (Elided — see the PORT NOTE above: this Darwin-gated export IS the CoreText
    // build, so the non-CoreText early return can never fire.)

    // We'll need content scale so fail early if we can't get it.
    // const content_scale = ptr.getContentScale() catch return null;
    // SAFETY: `ptr` validity is the caller's contract (`rustty_surface_new`).
    let content_scale = match unsafe { (*ptr).get_content_scale() } {
        Ok(cs) => cs,
        Err(_) => return core::ptr::null_mut(),
    };

    // Get the shared font grid. We acquire a read lock to
    // read the font face. It should not be deferred since
    // we're loading the primary face.
    // const grid = ptr.core_surface.renderer.font_grid;
    // SAFETY: the renderer's `font_grid` is the shared `*mut SharedGrid` handle
    // (`renderer_seam::Renderer` newtype `.0` → `rustty_renderer::Renderer`).
    let grid: *mut SharedGrid = unsafe { (*ptr).core_surface.renderer.0.font_grid };
    // grid.lock.lockShared();
    // defer grid.lock.unlockShared();
    //
    // SAFETY: `grid` is the external `*mut SharedGrid` handle; `.lock` is its
    // internal `RwLock` (the renderer `init` shared-read precedent, generic.rs).
    // The guard is bound to `_read` (NOT `_`) so the shared lock is held until
    // this function returns — the reference `defer grid.lock.unlockShared()`.
    let _read = unsafe { (*grid).lock.read().unwrap() };

    // const collection = &grid.resolver.collection;
    // const face = collection.getFace(.{}) catch return null;
    //
    // SAFETY: through the raw `*mut SharedGrid` handle we reach the resolver's
    // collection (a disjoint field from `.lock`, so the `&mut` reborrow does not
    // alias the read guard). `getFace` loads the primary face — already resident,
    // per the note above. `.{}` is `Index{ .style = .regular, .idx = 0 }`.
    let collection = unsafe { &mut (*grid).resolver.collection };
    let face = match collection.get_face(Index::new(Style::Regular, 0)) {
        Ok(f) => f,
        Err(_) => return core::ptr::null_mut(),
    };

    // We need to unscale the content scale. We apply the
    // content scale to our font stack because we are rendering
    // at 1x but callers of this should be using scaled or apply
    // scale themselves.
    // const size: f32 = size: {
    //     const num = face.font.copyAttribute(.size) orelse
    //         break :size 12;
    //     defer num.release();
    //     var v: f32 = 12;
    //     _ = num.getValue(.float, &v);
    //     break :size v;
    // };
    //
    // SAFETY: `face` is the `*mut Face` just returned; `.font` is its owned
    // `CFRetained<CTFont>`. `attribute` copies the size attribute at +1 (a
    // `CFRetained<CFType>` that releases on drop = the reference `defer
    // num.release()`); `getValue(.float, &v)` → `CFNumberType::FloatType` (the C
    // `float`). A missing/non-number attribute leaves the default 12.
    let size: f32 = match unsafe { (*face).font.attribute(kCTFontSizeAttribute) } {
        None => 12.0,
        Some(num) => {
            let mut v: f32 = 12.0;
            if let Some(n) = num.downcast_ref::<CFNumber>() {
                unsafe {
                    n.value(
                        CFNumberType::FloatType,
                        &mut v as *mut f32 as *mut core::ffi::c_void,
                    );
                }
            }
            v
        }
    };

    // const copy = face.font.copyWithAttributes(
    //     size / content_scale.y,
    //     null,
    //     null,
    // ) catch return null;
    //
    // PORT NOTE: objc2's `copy_with_attributes` folds the reference's null-check
    // (`catch return null`) into a non-null `expect` — the same contract the
    // CoreText face's own `init_font_copy` relies on (face/coretext.rs). The
    // result is the +1 CTFont (created via `CFRetained::from_raw`). `size` is f32,
    // `content_scale.y` f32; the quotient widens to the `CGFloat` (f64) argument.
    // SAFETY: `matrix` is a null `*const CGAffineTransform`; `attributes` is None.
    let copy: CFRetained<CTFont> = unsafe {
        (*face)
            .font
            .copy_with_attributes((size / content_scale.y) as f64, core::ptr::null(), None)
    };

    // return copy;
    //
    // Transfer the +1 CTFont to the host (I16): `into_raw` releases NOTHING, so
    // the CTFontRef crosses the ABI still at +1 and the host balances it with a
    // single CFRelease (`ghostty_surface_quicklook_font`'s Swift caller). Over- or
    // under-retaining here would leak or double-free the font.
    CFRetained::into_raw(copy).as_ptr() as *mut c_void
}

/// C: rustty_surface_quicklook_word
///
/// This returns the selected word for quicklook. This will populate
/// the buffer with the word under the cursor and the selection
/// info so that quicklook can be rendered.
///
/// This does not modify the selection active on the surface (if any).
///
/// # Safety
/// `ptr` must be a valid [`Surface`] pointer returned by [`rustty_surface_new`];
/// `result` must point to a writable [`Text`].
//
// export fn ghostty_surface_quicklook_word(ptr: *Surface, result: *Text) bool { ... }
//
// PORT NOTE: resolves the word under the cursor (`getCursorPos` + `posToViewport`
// + `pages.pin` + `selectWord`) then reads it via `readTextLocked` (the E7
// helper). It holds `renderer_state.mutex` and reads `renderer_state.terminal` —
// the live-surface renderer_state seam. It is objc-FREE: every step is core/vt
// (renderer_state.mutex, Screen::select_word, readTextLocked) already ported in
// rustty, so it functions the moment a live surface exists. Body ported 1:1.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_quicklook_word(
    ptr: *mut Surface,
    result: *mut Text,
) -> bool {
    // const surface = &ptr.core_surface;
    // SAFETY: `ptr` validity is the caller's contract.
    let core_surface: *mut CoreSurface = unsafe { core::ptr::addr_of_mut!((*ptr).core_surface) };

    // surface.renderer_state.mutex.lock();
    // defer surface.renderer_state.mutex.unlock();
    //
    // PORT NOTE: `renderer_state.mutex` is a `*mut Mutex`; the `defer unlock` runs on
    // every scope exit. As in `rustty_surface_read_selection`, the read arm computes
    // its result into a local and unlocks once before the single return; each early
    // `orelse return false` unlocks first (the codebase's explicit-unlock precedent
    // for a multi-exit `defer`, e.g. `Surface::clickLeft`).
    let mutex = unsafe { (*core_surface).renderer_state.mutex };
    unsafe { (*mutex).lock() };

    // // Get our word selection
    // const sel = sel: {
    //     const screen: *terminal.Screen = surface.renderer_state.terminal.screens.active;
    let screen: *mut rustty_vt::screen::Screen =
        unsafe { (*(*core_surface).renderer_state.terminal).screens.active };

    //     const pos = try ptr.getCursorPos();
    //
    // PORT NOTE: the reference `try` propagates the `getCursorPos` error out of a
    // `bool`-returning export; rustty's `get_cursor_pos` is infallible (returns
    // `Ok(self.cursor_pos)`), so the `Err` arm is effectively dead — mapped to the
    // faithful degrade (`unlock` + `return false`: no cursor → no quicklook word).
    let pos = match unsafe { <Surface as rustty_apprt_api::Surface>::get_cursor_pos(&*ptr) } {
        Ok(v) => v,
        Err(_) => {
            unsafe { (*mutex).unlock() };
            return false;
        }
    };

    //     const pt_viewport = surface.posToViewport(pos.x, pos.y);
    //
    // PORT NOTE: `posToViewport` takes `f64`; apprt `CursorPos` carries `f32`, so the
    // fields widen at the call (the reference's implicit float coercion).
    let pt_viewport = unsafe { (*core_surface).posToViewport(f64::from(pos.x), f64::from(pos.y)) };

    //     const pin = screen.pages.pin(.{
    //         .viewport = .{
    //             .x = pt_viewport.x,
    //             .y = pt_viewport.y,
    //         },
    //     }) orelse {
    //         if (comptime std.debug.runtime_safety) unreachable;
    //         return false;
    //     };
    //
    // PORT NOTE: `pages.pin` takes a read-only `&self`; the `orelse` block's
    // `if (comptime std.debug.runtime_safety) unreachable` is an invariant assert —
    // it aborts in debug (`cfg!(debug_assertions)`), and release returns `false`.
    let pin = match unsafe {
        (*screen).pages.pin(rustty_vt::point::Point::Viewport(
            rustty_vt::point::Coordinate {
                x: pt_viewport.x,
                y: pt_viewport.y,
            },
        ))
    } {
        Some(p) => p,
        None => {
            if cfg!(debug_assertions) {
                unreachable!("posToViewport produced an out-of-bounds viewport point");
            }
            unsafe { (*mutex).unlock() };
            return false;
        }
    };

    //     break :sel surface.io.terminal.screens.active.selectWord(
    //         pin,
    //         surface.config.selection_word_chars,
    //     ) orelse return false;
    // };
    let io_screen: *mut rustty_vt::screen::Screen =
        unsafe { (*core_surface).io.terminal.screens.active };
    let word_chars: &[u32] = &unsafe { &*core_surface }.config.selection_word_chars;
    let sel = match unsafe { &*io_screen }.select_word(pin, word_chars) {
        Some(v) => v,
        None => {
            unsafe { (*mutex).unlock() };
            return false;
        }
    };

    // // Read the selection
    // return readTextLocked(ptr, sel, result);
    //
    // PORT NOTE: `readTextLocked` requires the mutex HELD (the "Locked" contract); it
    // runs here still locked, and the single `unlock` below realizes the `defer`.
    let out = unsafe { readTextLocked(ptr, sel, result) };
    unsafe { (*mutex).unlock() };
    out
}

/// C: rustty_inspector_metal_init
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`];
/// `device` must be a valid `MTLDevice` id.
//
// export fn ghostty_inspector_metal_init(ptr: *Inspector, device: objc.c.id) bool { ... }
//
// PORT NOTE: delegates to the backend `Inspector::initMetal`, which is LIVE
// (WS6): it sets the Inspector's ImGui context, tears down any prior backend,
// and calls dcimgui's `ImGui_ImplMetal_Init`. The reference `.fromId(device)`
// wraps the raw objc id in an `objc.Object`; rustty's `initMetal` takes the
// opaque `*mut c_void` directly (the id crosses the C ABI untyped) and releases
// the host's +1 retain on every exit path via its `Defer` guard.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_metal_init(
    ptr: *mut Inspector,
    device: *mut c_void,
) -> bool {
    // return ptr.initMetal(.fromId(device));
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).initMetal(device) }
}

/// C: rustty_inspector_metal_render
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`];
/// `command_buffer` / `descriptor` must be valid `MTLCommandBuffer` / render-pass
/// descriptor ids.
//
// export fn ghostty_inspector_metal_render(ptr: *Inspector, command_buffer: objc.c.id, descriptor: objc.c.id) void { ... }
//
// PORT NOTE: delegates to the backend `Inspector::renderMetal`, which is LIVE
// (WS6): the double-frame ImGui loop, the `rustty_inspector::Inspector::render`
// widget aggregate, the `MTLRenderCommandEncoder` construction, and
// `ImGui_ImplMetal_RenderDrawData`. Catches + logs its error exactly as the
// reference `catch |err| { log.err(...); return; }`. The two `.fromId(...)`
// objc-id wraps → opaque `*mut c_void`.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_metal_render(
    ptr: *mut Inspector,
    command_buffer: *mut c_void,
    descriptor: *mut c_void,
) {
    // return ptr.renderMetal(
    //     .fromId(command_buffer),
    //     .fromId(descriptor),
    // ) catch |err| {
    //     log.err("error rendering inspector err={}", .{err});
    //     return;
    // };
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe { (*ptr).renderMetal(command_buffer, descriptor) } {
        log::error!(target: LOG_SCOPE, "error rendering inspector err={err:?}");
    }
}

/// C: rustty_inspector_metal_shutdown
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
//
// export fn ghostty_inspector_metal_shutdown(ptr: *Inspector) void { ... }
//
// PORT NOTE: tears down the Metal backend if one was initialized.
// `Backend::deinit` is LIVE (WS6) and calls dcimgui's `ImGui_ImplMetal_Shutdown`
// on the `.metal` arm. Faithful to the reference
// `if (ptr.backend) |v| { v.deinit(); ptr.backend = null; }`.
//
// ABI NOTE (R1): the reference's own `ghostty.h` declares this `bool` while
// `embedded.zig`'s `export fn` returns `void`; `include/rustty-embed.h` mirrors
// that discrepancy verbatim rather than "fixing" it. The Swift host never calls
// this export, so nothing reads the return.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_metal_shutdown(ptr: *mut Inspector) {
    // if (ptr.backend) |v| {
    //     v.deinit();
    //     ptr.backend = null;
    // }
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe {
        if let Some(v) = (*ptr).backend {
            v.deinit();
            (*ptr).backend = None;
        }
    }
}

// ===================================================================
// main_c.zig test tail — re-homed to rustty-apprt-c (CLI_BLUEPRINT.md ADV-FIX 6).
//
// The 3 `ghostty_string_s` tests exercise the `String` (`ghostty_string_s`) C hub
// type that main_c.zig defines and this crate ports (above). Per ADV-FIX 6 (§6
// option b) the main_c.zig test blocks are crate_map-re-homed here — the `rustty`
// bin does NOT dep apprt-c (apprt-c is the top-sink), so its libghostty-entry
// tests live with the ported `String` they exercise. Ported assertion-for-
// assertion (R1); the reference `state.alloc = testing.allocator` install has no
// analog because the rustty `String` frees via the global allocator (see
// `String::deinit`, Box::from_raw).
// ===================================================================
#[cfg(test)]
mod main_c_tests {
    use super::String;

    // test "ghostty_string_s empty string" {
    #[test]
    fn ghostty_string_s_empty_string() {
        // const testing = std.testing;
        // const empty_string = String.empty;
        let empty_string = String::empty();
        // defer empty_string.deinit();

        // try testing.expect(empty_string.len == 0);
        assert!(empty_string.len == 0);
        // try testing.expect(empty_string.sentinel == false);
        assert!(!empty_string.sentinel);

        // defer empty_string.deinit(); — runs at scope end (null ptr -> no-op).
        empty_string.deinit();
    }

    // test "ghostty_string_s c string" {
    #[test]
    fn ghostty_string_s_c_string() {
        // const testing = std.testing;
        // state.alloc = testing.allocator;
        //   PORT NOTE: the rustty `String` frees via the global allocator
        //   (`Box::from_raw` in `deinit`), so there is no `state.alloc` to install;
        //   the reference's `testing.allocator` install is unnecessary.

        // const slice: [:0]const u8 = "hello";
        let slice: &[u8] = b"hello";
        // const allocated_slice = try testing.allocator.dupeZ(u8, slice);
        //   the `dupeZ` copy; the rustty `from_slice` appends the sentinel NUL
        //   itself, so the owned copy is just the `hello` bytes with `sentinel=true`.
        let allocated_slice = slice.to_vec();
        // const c_null_string = String.fromSlice(allocated_slice);
        let c_null_string = String::from_slice(allocated_slice, true);
        // defer c_null_string.deinit();

        // try testing.expect(allocated_slice[5] == 0);
        //   the sentinel NUL `from_slice` appended lives at index `len` (== 5).
        //   SAFETY: `from_slice(_, true)` allocated `len + 1` bytes with a trailing NUL.
        assert!(unsafe { *c_null_string.ptr.add(5) } == 0);
        // try testing.expect(@TypeOf(slice) == [:0]const u8);
        // try testing.expect(@TypeOf(allocated_slice) == [:0]u8);
        //   compile-time type assertions; the Rust types are `&[u8]` / `Vec<u8>`
        //   with the sentinel tracked by the explicit `true` arg (no runtime analog).
        // try testing.expect(c_null_string.len == 5);
        assert!(c_null_string.len == 5);
        // try testing.expect(c_null_string.sentinel == true);
        assert!(c_null_string.sentinel);

        // defer c_null_string.deinit(); — frees `len + 1` bytes at scope end.
        c_null_string.deinit();
    }

    // test "ghostty_string_s zig string" {
    #[test]
    fn ghostty_string_s_zig_string() {
        // const testing = std.testing;
        // state.alloc = testing.allocator; (see the c-string note — no analog needed.)

        // const slice: []const u8 = "hello";
        let slice: &[u8] = b"hello";
        // const allocated_slice = try testing.allocator.dupe(u8, slice);
        let allocated_slice = slice.to_vec();
        // const zig_string = String.fromSlice(allocated_slice);
        let zig_string = String::from_slice(allocated_slice, false);
        // defer zig_string.deinit();

        // try testing.expect(@TypeOf(slice) == []const u8);
        // try testing.expect(@TypeOf(allocated_slice) == []u8);
        //   compile-time type assertions; no runtime analog.
        // try testing.expect(zig_string.len == 5);
        assert!(zig_string.len == 5);
        // try testing.expect(zig_string.sentinel == false);
        assert!(!zig_string.sentinel);

        // defer zig_string.deinit(); — frees `len` bytes at scope end.
        zig_string.deinit();
    }

    // The null-action GUI-launch path of `rustty_cli_try_action` (main_c.zig
    // `ghostty_cli_try_action`): after `rustty_init` (which stashes `action: None`
    // — the BREAK #1 detectArgs seam), `rustty_cli_try_action` must take the
    // `state.action orelse return` branch and RETURN WITHOUT exiting (a windowed
    // launch must not `posix.exit`). If the non-null `unreachable!` fired, or if the
    // fn exited the process, this test would abort rather than complete.
    #[test]
    fn cli_try_action_null_action_returns() {
        // reference `ghostty_init` sets the global state (with `action = null`).
        // SAFETY: no argv is dereferenced by `rustty_init` (argc = 0).
        let rc = unsafe { crate::rustty_init(0, core::ptr::null_mut()) };
        assert_eq!(rc, 0, "rustty_init must initialize the global state");

        // With no CLI action stashed, this returns cleanly (the GUI-launch path).
        // SAFETY: `rustty_init` above initialized the global state this reads.
        unsafe { crate::rustty_cli_try_action() };

        // Reaching here proves the fn returned (did not exit / panic).
    }
}

// ===================================================================
// Focused unit test for the objc-FREE quicklook_word path (P2.1).
//
// The full `rustty_surface_quicklook_word` export needs a LIVE surface
// (`renderer_state.mutex` + `dumpTextLocked` over a real `CoreSurface`), which
// reaches the Phase-3 renderer seam and is impractical to stand up in-crate. Its
// objc-free BODY, however, is pure core/vt: resolve a *viewport* `point.Coordinate`
// (exactly what `posToViewport` yields for the cursor) → `pages.pin` → `select_word`
// → read the word text. This test drives that exact wiring against a real vt
// `Screen`, proving the ported path selects the word under the cursor coordinate and
// reads back its text (the value the export ownership-transfers to the host as a
// `Text`, I6).
// ===================================================================
#[cfg(test)]
mod quicklook_word_tests {
    use allocator_api2::alloc::Global;
    use rustty_vt::point::{Coordinate, Point};
    use rustty_vt::screen::{Options, Screen, SelectionString};

    // The `selectWord` default word-boundary codepoints (subset: NUL + space are all
    // that a `"hello world"` boundary needs; mirrors the vt `SELECT_WORD_BOUNDARY`).
    const WORD_BOUNDARY: [u32; 2] = [0, b' ' as u32];

    #[test]
    fn quicklook_word_selects_word_under_cursor() {
        // A small live screen with a two-word line.
        let mut s = Screen::init(
            Global,
            Options {
                cols: 20,
                rows: 5,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.test_write_string(b"hello world").unwrap();

        // Build the *viewport* point the export forms from the cursor position
        // (`posToViewport` → `point.Coordinate` → `.{ .viewport = .{ .x, .y } }`).
        // Column 7 ('o' in "world") sits inside the second word.
        let pt = Point::Viewport(Coordinate { x: 7, y: 0 });

        // screen.pages.pin(...) orelse return false;
        let pin = s
            .pages
            .pin(pt)
            .expect("viewport point must resolve to a pin");

        // screen.selectWord(pin, config.selection_word_chars) orelse return false;
        let sel = s
            .select_word(pin, &WORD_BOUNDARY)
            .expect("a word under the cursor must select");

        // readTextLocked → dumpTextLocked reads the selection text; the vt-level
        // `selection_string` is that same read (the non-null `Text` the host gets).
        let text = s
            .selection_string(Global, SelectionString { sel, trim: true })
            .unwrap();
        assert_eq!(text.as_slice(), b"world");

        sel.deinit(&mut s);
        s.deinit();
    }

    #[test]
    fn quicklook_word_empty_cell_yields_no_word() {
        // No text written: the cell under the cursor is empty, so `select_word`
        // returns `None` — the export's `orelse return false` ("no word") degrade.
        let mut s = Screen::init(
            Global,
            Options {
                cols: 20,
                rows: 5,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let pt = Point::Viewport(Coordinate { x: 3, y: 0 });
        let pin = s
            .pages
            .pin(pt)
            .expect("viewport point must resolve to a pin");
        assert!(s.select_word(pin, &WORD_BOUNDARY).is_none());

        s.deinit();
    }
}

// Focused CTFont ownership KAT for the quicklook_font path (P2.2).
//
// The full `rustty_surface_quicklook_font` export needs a LIVE surface with a
// GPU renderer + populated `font_grid` (Metal), so its end-to-end behavior is
// verified in the macOS app shakedown, not here. What IS unit-testable — and is
// the one load-bearing ownership decision in the port — is the +1 CTFont handoff
// (I16): the export resizes the primary face with `copy_with_attributes` (a +1
// CTFontRef) and hands it to the host via `CFRetained::into_raw` (which must NOT
// release), the host balancing it with exactly one CFRelease. A regression to
// `CFRetained::as_ptr` (no ownership transfer) would double-free; an extra
// retain would leak. This test pins that balance using a real, standalone CTFont
// (CoreText font creation needs neither the grid nor Metal nor the main thread).
#[cfg(all(test, target_os = "macos"))]
mod quicklook_font_tests {
    use objc2_core_foundation::{CFRetained, CFString};
    use objc2_core_text::CTFont;

    #[test]
    fn quicklook_font_handoff_is_plus_one_the_host_releases() {
        // A real CTFont, standing in for the collection's primary face
        // (`Face.font`). Menlo is present on every macOS; the exact face is
        // irrelevant — only the retain balance is under test.
        let name = CFString::from_str("Menlo");
        // SAFETY: `name` is a valid CFString; `matrix` is null.
        let base: CFRetained<CTFont> = unsafe { CTFont::with_name(&name, 12.0, core::ptr::null()) };

        // Mirror the export's tail exactly: resize via copyWithAttributes → +1.
        // SAFETY: `matrix` null; `attributes` None.
        let copy: CFRetained<CTFont> =
            unsafe { base.copy_with_attributes(13.0, core::ptr::null(), None) };

        // Hold an EXTRA retain so the object survives the "host" release and we
        // can observe the count afterward (this `keeper` is NOT part of the
        // export contract — it is the test's observation handle).
        let keeper: CFRetained<CTFont> = copy.clone();
        let rc_before = keeper.retain_count();

        // The export hands the +1 to the host: `into_raw` transfers ownership
        // WITHOUT releasing, so the retain count is unchanged by the handoff.
        let raw = CFRetained::into_raw(copy);
        // SAFETY: `raw` is the live CTFont just handed off (kept alive by `keeper`).
        assert_eq!(
            unsafe { raw.as_ref() }.retain_count(),
            rc_before,
            "into_raw must transfer ownership without releasing (I16)"
        );

        // The host balances the +1 with exactly ONE CFRelease. Modeling that as
        // `from_raw` (the inverse of `into_raw`) + drop IS a single CFRelease.
        // SAFETY: `raw` is the +1 the export handed the host; reclaiming it here
        // consumes exactly that one reference (the host's `CFRelease`).
        drop(unsafe { CFRetained::from_raw(raw) });

        // The host's single release consumed exactly the export's +1: the count
        // is back down by one, and `keeper`'s reference is untouched.
        assert_eq!(
            keeper.retain_count(),
            rc_before - 1,
            "the host's one CFRelease must consume exactly the export's +1 (no leak, no double-free)"
        );
    }
}

#[cfg(test)]
mod tests {
    use libc::c_char;

    // The detectArgs slice of the C-path `state.init()`: detection over the
    // argv STORED by `rustty_init` (reference `std.os.argv = argv[0..argc]`
    // then `cli.action.detectArgs` walking std.process.args == std.os.argv).
    // Pure over its input — no process-global involved — so it is directly
    // unit-testable (the global-state paths are pinned by subprocess KATs).
    #[test]
    fn detect_from_argv_plus_version() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec(), b"+version".to_vec()];
        let action = crate::detect_from_argv(&argv).unwrap();
        assert_eq!(action, Some(rustty_cli::rustty::Action::Version));
    }

    #[test]
    fn detect_from_argv_none() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec()];
        assert_eq!(crate::detect_from_argv(&argv).unwrap(), None);
        assert_eq!(crate::detect_from_argv(&[]).unwrap(), None);
    }

    #[test]
    fn detect_from_argv_invalid() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec(), b"+bogus".to_vec()];
        assert_eq!(
            crate::detect_from_argv(&argv),
            Err(rustty_cli::action::DetectError::InvalidAction),
        );
    }

    #[test]
    fn detect_from_argv_multiple() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec(), b"+version".to_vec(), b"+help".to_vec()];
        assert_eq!(
            crate::detect_from_argv(&argv),
            Err(rustty_cli::action::DetectError::MultipleActions),
        );
    }

    // reference `ghostty_init`: a detect failure means `state.init()` fails →
    // log err + return 1 (main_c.zig:105). No global state is stamped.
    #[test]
    fn rustty_init_invalid_action_returns_1() {
        let argv0 = std::ffi::CString::new("rustty").unwrap();
        let arg1 = std::ffi::CString::new("+bogus").unwrap();
        let mut argv = [argv0.as_ptr() as *mut c_char, arg1.as_ptr() as *mut c_char];
        // SAFETY: argv points at 2 valid NUL-terminated strings.
        let rc = unsafe { crate::rustty_init(2, argv.as_mut_ptr()) };
        assert_eq!(rc, 1);
    }

    // The NON-NULL path of `rustty_cli_try_action` (main_c.zig): with `+version`
    // in argv, `rustty_init` stashes the action and `rustty_cli_try_action`
    // RUNS it and EXITS with its code — so the whole flow runs in a CHILD
    // process (the panic_policy.rs re-exec pattern); the parent asserts exit 0
    // and the version output. This is the CLI-action-THROUGH-C KAT.
    #[test]
    fn cli_try_action_version_runs_and_exits() {
        const MARKER: &str = "RUSTTY_CLI_ACTION_CHILD";
        if std::env::var_os(MARKER).is_some() {
            // Child: exactly what the Swift host does (main.swift:8,31).
            let argv0 = std::ffi::CString::new("rustty").unwrap();
            let arg1 = std::ffi::CString::new("+version").unwrap();
            let mut argv = [argv0.as_ptr() as *mut c_char, arg1.as_ptr() as *mut c_char];
            // SAFETY: argv points at 2 valid NUL-terminated strings.
            let rc = unsafe { crate::rustty_init(2, argv.as_mut_ptr()) };
            assert_eq!(rc, 0, "rustty_init with +version must succeed");
            // SAFETY: global state initialized by rustty_init above.
            unsafe { crate::rustty_cli_try_action() };
            unreachable!("+version must exit the process");
        }
        let exe = std::env::current_exe().expect("current_exe");
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "tests::cli_try_action_version_runs_and_exits",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .output()
            .expect("spawn child test process");
        assert!(
            out.status.success(),
            "child failed: status={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(rustty_identity::VERSION),
            "version output missing from: {stdout}",
        );
    }
}
