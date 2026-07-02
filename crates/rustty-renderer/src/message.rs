//! The renderer cross-thread message union.
//!
//! PORTS: ghostty src/renderer/message.zig (0 `test{}` blocks).
//!
//! PORT STATUS: ported (integration-tier stage 2 — the minimal cross-thread
//! surface termio needs). `initChangeConfig` is config-coupled: core C0 gives
//! it a generic-`C` placeholder SIGNATURE (so core's updateConfig can name it)
//! while the body stays DEFERRED verbatim below (`rustty-config` already
//! depends on `rustty-renderer`, so a renderer -> config edge would cycle —
//! the same G7 wall as `shadertoy::loadFromFile`; see
//! .port/phase-2/DEFERRALS.md).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const configpkg = @import("../config.zig");
// const font = @import("../font/main.zig");
// const renderer = @import("../renderer.zig");
// const terminal = @import("../terminal/main.zig");
//
// PORT NOTE: `configpkg` is only used by the DEFERRED `initChangeConfig` (see
// its verbatim body below); `font` maps to the rustty-font-shaper tier (the D1
// split hosts `SharedGrid`/`SharedGridSet` there), `renderer` to this crate's
// root/`thread` modules, and `terminal` to rustty-vt.
use allocator_api2::alloc::Global as Allocator;
use rustty_font_shaper::shared_grid_set;
use rustty_font_shaper::{SharedGrid, SharedGridSet};
use rustty_vt::highlight;

use crate::thread;
use crate::RendererDerivedConfig;

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
    // `r#impl` (precedent: rustty-lib's `r#enum` module). Both pointee types
    // are deferred-seam placeholders today (`thread::DerivedConfig` — inhabited
    // since core C0 [ADV-FIX 2] — and the still-uninhabited
    // [`RendererDerivedConfig`]), so this variant is not yet constructible.
    ChangeConfig {
        thread: Box<thread::DerivedConfig>,
        r#impl: Box<RendererDerivedConfig>,
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

/// PORT: the inferred error set of [`Message::initChangeConfig`]
/// (`Allocator.Error` from the two `alloc.create`s +
/// `Renderer.DerivedConfig.init`'s inferred set) — an opaque growth-point
/// marker until the DerivedConfig bodies land (precedent:
/// `rustty_termio::Error` / `rustty_apprt_api::ApprtError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitChangeConfigError;

impl Message {
    // DEFERRED BODY (config-coupled, 0 tests — R1-safe): `initChangeConfig`
    // takes `*const configpkg.Config` and calls the two `DerivedConfig.init`s,
    // but `rustty-config` already depends on `rustty-renderer` so the edge
    // would cycle (G7) — hence the generic `C` on the placeholder signature
    // below (the concrete instantiation, `C` = `rustty_config::Config`,
    // happens in the crates above: core's updateConfig, Surface.zig:1782) —
    // and `RendererDerivedConfig` (`renderer.Renderer.DerivedConfig`) is still
    // an uninhabited GPU-tier placeholder. The reference function + comments
    // are preserved verbatim so nothing is lost (R1); the implementation lands
    // with the DerivedConfig types (see .port/phase-2/DEFERRALS.md).
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
    // (C0 placeholder signature — deferred verbatim body above.)
    #[allow(non_snake_case)]
    pub fn initChangeConfig<C>(
        alloc: Allocator,
        config: &C,
    ) -> Result<Message, InitChangeConfigError> {
        let _ = (alloc, config);
        unimplemented!(
            "renderer.Message.initChangeConfig deferred (config-coupled placeholder); \
             verbatim body above"
        )
    }

    // pub fn deinit(self: *const Message) void {
    //
    // PORT NOTE: the reference `deinit` destroys the `change_config` heap
    // allocations (`v.impl.deinit(); v.alloc.destroy(v.impl);
    // v.alloc.destroy(v.thread);`). With the alloc collapse those frees are the
    // `Box` drops below; `v.impl.deinit()` (the GPU renderer DerivedConfig's own
    // deinit) is deferred WITH its placeholder type — `RendererDerivedConfig`
    // is uninhabited today, so a `ChangeConfig` message cannot be constructed
    // and this arm is statically unreachable until it lands.
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
            Message::ChangeConfig { .. } => {}

            // else => {},
            _ => {}
        }
    }
}
