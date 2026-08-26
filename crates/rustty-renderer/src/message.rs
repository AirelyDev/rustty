//! The renderer cross-thread message union.
//!
//! PORTS: ghostty src/renderer/message.zig (0 `test{}` blocks).
//!
//! PORT STATUS: ported (integration-tier stage 2 — the minimal cross-thread
//! surface termio needs). `initChangeConfig` is now REAL: it is generic over
//! `C: RendererConfig` (the renderer-defined accessor trait — the renderer may
//! not name `rustty_config::Config`, R4/G7), and the concrete
//! `C = rustty_config::Config` binds in core's `updateConfig`. It builds a
//! `ChangeConfig` by calling both `DerivedConfig::init`s (the renderer one +
//! `thread::DerivedConfig::init`), which are themselves real now that
//! `DerivedConfig::init<C: RendererConfig>` landed. See
//! .port/phase-2/DEFERRALS.md.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const configpkg = @import("../config.zig");
// const font = @import("../font/main.zig");
// const renderer = @import("../renderer.zig");
// const terminal = @import("../terminal/main.zig");
//
// PORT NOTE: `configpkg` is used (in the reference) only by `initChangeConfig`,
// which in rustty is now REAL but generic over `C: RendererConfig` (the renderer
// may not name `rustty_config::Config`, R4) — so `configpkg` is still not imported
// here; `font` maps to the rustty-font-shaper tier (the D1 split hosts
// `SharedGrid`/`SharedGridSet` there), `renderer` to this crate's root/`thread`
// modules, and `terminal` to rustty-vt.
use allocator_api2::alloc::Global as Allocator;
use rustty_font_shaper::shared_grid_set;
use rustty_font_shaper::{SharedGrid, SharedGridSet};
use rustty_vt::highlight;

use crate::config::RendererConfig;
use crate::thread;
use crate::DerivedConfig;

/// The messages that can be sent to a renderer thread.
//
// PORT NOTE: reference `Message = union(enum)`. The nested `SearchMatches`/
// `SearchMatch` types are hoisted to module level (Rust cannot nest type
// declarations inside an enum), mirroring the apprt-api `WriteReq` hoist.
//
// PARITY: the reference union likewise carries the two `SharedGridSet.Key`
// payloads BY VALUE (`new_key`/`old_key` — the dereference contract hands the
// keys themselves across the thread); boxing them to shrink the enum would
// change that ownership shape, so `large_enum_variant` is silenced.
#[allow(clippy::large_enum_variant)]
pub enum Message {
    /// Purposely crash the renderer. This is used for testing and debugging.
    /// See the "crash" binding action.
    Crash,

    /// A change in state in the window focus that this renderer is
    /// rendering within. This is only sent when a change is detected so
    /// the renderer is expected to handle all of these.
    Focus(bool),

    /// A change in the view occlusion state. This can be used to determine
    /// if the window is visible or not. A window can be not visible (occluded)
    /// and still have focus.
    Visible(bool),

    /// Reset the cursor blink by immediately showing the cursor then
    /// restarting the timer.
    ResetCursorBlink,

    /// Change the font grid. This can happen for any number of reasons
    /// including a font size change, family change, etc.
    FontGrid {
        grid: *mut SharedGrid,
        set: *mut SharedGridSet,

        // The key for the new grid. If adopting the new grid fails for any
        // reason, the old grid should be kept but the new key should be
        // dereferenced.
        new_key: shared_grid_set::Key,

        // After accepting the new grid, the old grid must be dereferenced
        // using the fields below.
        old_key: shared_grid_set::Key,
    },

    /// Changes the size. The screen size might change, padding, grid, etc.
    Resize(crate::size::Size),

    /// The derived configuration to update the renderer with.
    //
    // PORT NOTE: reference payload `struct { alloc: Allocator, thread:
    // *renderer.Thread.DerivedConfig, impl: *renderer.Renderer.DerivedConfig }`.
    // The `alloc` field collapses into `Box` ownership (the reference
    // heap-allocates both pointers via `alloc.create` and frees them in
    // `deinit` — precedent: rustty-os cf_release_thread's `Message.Release`
    // alloc collapse). `impl` is a Rust keyword, kept as the raw identifier
    // `r#impl` (precedent: rustty-lib's `r#enum` module). Both pointee types are
    // real inhabited structs (`thread::DerivedConfig` carries the
    // `custom_shader_animation` mirror; the GPU renderer's [`DerivedConfig`] since
    // GPU R-d Chunk 3a), and the variant is now constructible via
    // [`Message::initChangeConfig`] (below).
    ChangeConfig {
        thread: Box<thread::DerivedConfig>,
        r#impl: Box<DerivedConfig>,
    },

    /// Matches for the current viewport from the search thread. These happen
    /// async so they may be off for a frame or two from the actually rendered
    /// viewport. The renderer must handle this gracefully.
    SearchViewportMatches(SearchMatches),

    /// The selected match from the search thread. May be null to indicate
    /// no match currently.
    SearchSelectedMatch(Option<SearchMatch>),

    /// Activate or deactivate the inspector.
    Inspector(bool),

    /// The macOS display ID has changed for the window.
    MacosDisplayId(u32),
}

// PORT NOTE: reference `pub const SearchMatches = struct { arena:
// ArenaAllocator, matches: []const terminal.highlight.Flattened }`. The arena
// that owns the flattened matches collapses into the owned `Vec` (drop frees),
// per the established arena-collapse precedent (crash sentry.rs).
pub struct SearchMatches {
    pub matches: Vec<highlight::Flattened>,
}

// PORT NOTE: reference `pub const SearchMatch = struct { arena: ArenaAllocator,
// match: terminal.highlight.Flattened }`. Same arena collapse as
// [`SearchMatches`]; `match` is a Rust keyword, kept as the raw identifier
// `r#match`.
pub struct SearchMatch {
    pub r#match: highlight::Flattened,
}

/// PORT NOTE: the inferred error set of [`Message::initChangeConfig`]
/// (`Allocator.Error` from the two `alloc.create`s +
/// `Renderer.DerivedConfig.init`'s inferred set). Now that the `DerivedConfig`
/// bodies have landed, the renderer `DerivedConfig::init` error is collapsed into
/// this opaque marker (precedent: `rustty_termio::Error` /
/// `rustty_apprt_api::ApprtError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitChangeConfigError;

impl Message {
    // The reference function + comments, preserved verbatim (R1):
    //
    // /// Initialize a change_config message.
    // pub fn initChangeConfig(alloc: Allocator, config: *const configpkg.Config) !Message {
    //     const thread_ptr = try alloc.create(renderer.Thread.DerivedConfig);
    //     errdefer alloc.destroy(thread_ptr);
    //     const config_ptr = try alloc.create(renderer.Renderer.DerivedConfig);
    //     errdefer alloc.destroy(config_ptr);
    //
    //     thread_ptr.* = renderer.Thread.DerivedConfig.init(config);
    //     config_ptr.* = try renderer.Renderer.DerivedConfig.init(alloc, config);
    //     errdefer config_ptr.deinit();
    //
    //     return .{
    //         .change_config = .{
    //             .alloc = alloc,
    //             .thread = thread_ptr,
    //             .impl = config_ptr,
    //         },
    //     };
    // }

    /// Initialize a change_config message.
    //
    // PORT NOTE: reference `initChangeConfig(alloc, config: *const configpkg.Config)`.
    // `C` is bounded by the renderer-defined `RendererConfig` accessor trait (R4/G7 —
    // the renderer may not name `rustty_config::Config`); the concrete
    // `C = rustty_config::Config` binds in core (`updateConfig`, Surface.zig:1782).
    // The two `alloc.create` + write pairs collapse into `Box::new`, exactly as the
    // renderer/thread `DerivedConfig::init` calls fold their arena/heap allocs into
    // owned Rust values. `errdefer config_ptr.deinit()` is subsumed: the two
    // `DerivedConfig::init` calls are the only fallible/moving steps and the `?`
    // early-return drops nothing that owns a C resource before `r#impl` is built —
    // if `Renderer.DerivedConfig::init` returns `Err`, the already-built `thread`
    // `Box` drops (freeing the heap slot); `thread` carries no C resource, so
    // nothing leaks, faithful to the reference errdefer ordering (`alloc: Allocator`
    // here is `allocator_api2::alloc::Global`, threaded through for signature
    // fidelity — `DerivedConfig::init` owns its arena/CStrings and does nothing
    // with it).
    #[allow(non_snake_case)]
    pub fn initChangeConfig<C: RendererConfig>(
        alloc: Allocator,
        config: &C,
    ) -> Result<Message, InitChangeConfigError> {
        // thread_ptr.* = renderer.Thread.DerivedConfig.init(config);
        let thread = Box::new(thread::DerivedConfig::init(config));

        // config_ptr.* = try renderer.Renderer.DerivedConfig.init(alloc, config);
        let r#impl =
            Box::new(DerivedConfig::init(alloc, config).map_err(|_| InitChangeConfigError)?);

        Ok(Message::ChangeConfig { thread, r#impl })
    }

    // pub fn deinit(self: *const Message) void {
    //
    // PORT NOTE: the reference `deinit` destroys the `change_config` heap
    // allocations (`v.impl.deinit(); v.alloc.destroy(v.impl);
    // v.alloc.destroy(v.thread);`). With the alloc collapse the two
    // `alloc.destroy`s are the `Box` drops below; the reference `v.impl.deinit()`
    // (the GPU renderer `DerivedConfig::deinit`) is load-bearing and MUST run
    // first — it frees the onig regex handles owned by `impl.links` (neither
    // `DerivedConfig`, `link::Set`, nor `oni::Regex` impl `Drop`, so `Box`-drop
    // alone would leak them). The `thread` box carries no C resource; it just
    // drops. Now that `initChangeConfig` lands, a `ChangeConfig` message is
    // constructible via the public API, so this arm is reachable and the free
    // must be correct.
    // PARITY: the reference body is an exhaustive `switch` with an `else => {}`
    // arm; the `match` mirrors that shape, so `single_match` is silenced.
    #[allow(clippy::single_match)]
    pub fn deinit(self: Message) {
        match self {
            // .change_config => |v| {
            //     v.impl.deinit();
            //     v.alloc.destroy(v.impl);
            //     v.alloc.destroy(v.thread);
            // },
            Message::ChangeConfig { r#impl, thread: _ } => {
                // v.impl.deinit(); (then r#impl + thread boxes drop == the two
                // alloc.destroy's). Rebind the moved box as mutable so
                // `deinit(&mut self)` can be called (auto-deref
                // `Box<DerivedConfig>` -> `&mut DerivedConfig`). Faithful
                // ordering: deinit-then-free.
                let mut r#impl = r#impl;
                r#impl.deinit();
            }

            // else => {},
            _ => {}
        }
    }
}

// ===========================================================================
// initChangeConfig / deinit tests — mock-`C` (§6.T1/T2).
// ===========================================================================
//
// The renderer CANNOT dep rustty-config (that IS the R4 cycle), so the
// `RendererConfig`-bounded constructor is exercised through the shared
// `#[cfg(test)]` `MockConfig` (`crate::test_support`).
#[cfg(test)]
mod change_config_tests {
    use super::*;
    use crate::test_support::MockConfig;
    use allocator_api2::alloc::Global;
    use rustty_input::link::{Action, Highlight, Link as InputLink};
    use rustty_vt::color::RGB;

    // T1: initChangeConfig builds BOTH DerivedConfigs.
    #[test]
    fn init_change_config_builds_both_derived() {
        let msg = Message::initChangeConfig(Global, &MockConfig::default()).expect("init");
        match &msg {
            Message::ChangeConfig { thread, r#impl } => {
                // seam 3: the thread DerivedConfig carries the extracted flag.
                // (PORT NOTE `aee7bf347`: `custom_shader_animation` moved to the
                // renderer DerivedConfig — asserted below.)
                assert!(thread.scrollback_compression);
                assert_eq!(
                    r#impl.custom_shader_animation,
                    thread::CustomShaderAnimation::True
                );
                // seam 1: the renderer DerivedConfig has the extracted fields
                // (spot-check the non-trivial transforms the generic.rs oracle
                // asserts: faint_opacity @ceil(x*255), Color::to_terminal_rgb).
                assert_eq!(r#impl.faint_opacity, 255);
                assert_eq!(
                    r#impl.foreground,
                    RGB {
                        r: 0xFF,
                        g: 0xFF,
                        b: 0xFF
                    }
                );
            }
            _ => panic!("expected ChangeConfig"),
        }
        // Clean up (also exercises T2's deinit path).
        msg.deinit();
    }

    // T2: the ChangeConfig deinit frees the onig regex handle owned by
    // `impl.links` (the leak / free assertion). A double-free / use-after-free
    // here would abort; a correct single-free returns.
    #[test]
    fn change_config_deinit_frees_links() {
        // A live link whose `oni_regex()` succeeds, so `DerivedConfig::init`
        // allocates a real `OnigRegex` handle that `deinit` must free.
        let cfg = MockConfig {
            links: vec![InputLink {
                regex: b"foo".to_vec(),
                action: Action::Open,
                highlight: Highlight::Always,
            }],
            ..Default::default()
        };
        let msg = Message::initChangeConfig(Global, &cfg).expect("init");
        // Sanity: the link made it into the impl DerivedConfig.
        if let Message::ChangeConfig { ref r#impl, .. } = msg {
            assert_eq!(r#impl.links.links.len(), 1);
        } else {
            panic!("expected ChangeConfig");
        }
        // The free: deinit runs r#impl.deinit() (frees the regex), then the
        // boxes drop.
        msg.deinit();
    }
}
