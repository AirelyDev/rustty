//! The Inspector is a development tool to debug the terminal. This is
//! useful for terminal application developers as well as people potentially
//! debugging issues in Ghostty itself.
//!
//! PORTS: ghostty src/inspector/Inspector.zig (the top-level inspector
//! aggregate + its `render`).
//!
//! PORT STATUS (Task 17 — the aggregate + top-level render):
//!
//! `Inspector.zig`'s value aggregate is
//!
//! ```zig
//! mouse: widgets.surface.Mouse = .{},   // extra mouse state we track
//! gui:   widgets.surface.Inspector,     // the whole ImGui widget tree
//! ```
//!
//! whose `gui` owns `surface_info` / `key_stream` / `terminal_info` /
//! `vt_stream` / `renderer_info` / `show_demo_window` (surface.zig:21-28). The
//! `gui` aggregate type (`widgets::surface::Inspector`) is STILL the DEFERRED
//! (preserved-verbatim) body in `widgets/surface.rs` — it is transitively
//! blocked on:
//!
//!   1. the cyclic core `Surface` accessor-trait seam (R4 — `Surface` lives in
//!      rustty-core, which already holds `Option<*mut inspectorpkg::Inspector>`,
//!      so a direct `rustty_core::Surface` name here would CYCLE; the edge needs
//!      a renderer/core-defined Surface accessor trait, the `InspectorOverlay`
//!      precedent),
//!   2. `widgets::terminal::Info` (the `terminal_info` field — still its own
//!      DEFERRED aggregate, gated on `widgets::screen::Info`, itself gated on the
//!      DockBuilder FFI below), and
//!   3. three unbound dcimgui-sys DockBuilder symbols that `surface::Inspector.
//!      createDockSpace` needs (`ImGui_DockBuilderGetNode`,
//!      `ImGui_DockBuilderAddNodeEx`, and the private
//!      `ImGuiDockNodeFlagsPrivate.DockSpace` flag).
//!
//! So per the placeholder-leaf **grows-in-place** doctrine, the aggregate is
//! carried WITHOUT a `gui` field: its two sub-objects that the core lifecycle
//! seam (Task 6) needs TODAY — `renderer_info` (INV-7 overlay coercion) and
//! `key_stream_events` (`recordKeyEvent`, Inspector.zig:82) — are carried as
//! direct fields (the reference reaches them as `self.gui.renderer_info` /
//! `self.gui.key_stream.events`). The `mouse` field IS ported LIVE (its type
//! `widgets::surface::Mouse` is the R3-proven live half of surface.rs). When
//! `surface::Inspector` inhabits (a later fold), `renderer_info` /
//! `key_stream_events` migrate under `gui` and the two accessors + core's
//! coercion path update in lock-step (`cargo build --workspace` is the backstop).
//!
//! Ported LIVE here: `setup` (static ImGui-IO mutation, INV-11), `init` /
//! `deinit`, `rendererInfo` (grows-in-place accessor), and `recordKeyEvent`
//! (INV-9 grow-then-evict). `recordPtyRead` and `render` touch `self.gui`
//! (`vt_stream` / `draw`) and the cyclic core `Surface` edge, so they are the
//! preserved-verbatim reference bodies in the DEFERRED block at the foot of this
//! file; they inhabit once `surface::Inspector` + the Surface accessor trait land.

// const Inspector = @This();
//
// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const builtin = @import("builtin");
// const cimgui = @import("dcimgui");
// const Surface = @import("../Surface.zig");
// const font = @import("../font/main.zig");
// const terminal = @import("../terminal/main.zig");
// const inspector = @import("main.zig");
// const widgets = @import("widgets.zig");
//
// PORT NOTE: `Allocator` is `allocator_api2::alloc::Global` (the crate's global
// allocator handle, carried for reference call-shape parity — Rust's infallible
// global allocator collapses Zig's `!`/OOM). `cimgui` is the vendored Dear ImGui
// C API reached through `dcimgui-sys`. `font.embedded` is `rustty_font::embedded`.
// `inspector` (main.zig) is this crate's root — `inspector.KeyEvent` is
// `crate::KeyEvent`. `Surface` / `terminal.Terminal` are the DEFERRED-block core
// edges. `widgets` is `crate::widgets`.
use crate::widgets;

/// Extra mouse state the inspector tracks per-surface (`widgets.surface.Mouse`).
/// LIVE — the R3-proven live half of `widgets/surface.rs`.
//
// mouse: widgets.surface.Mouse = .{},
//
// (declared as a struct field below; the `.{}` default is `Mouse::default()`.)

pub struct Inspector {
    /// Mouse state that we track in addition to normal mouse states that
    /// Ghostty always knows about.
    //
    // mouse: widgets.surface.Mouse = .{},
    pub mouse: widgets::surface::Mouse,

    // ImGui state
    //
    // gui: widgets.surface.Inspector,
    //
    // PORT NOTE (grows-in-place, Task 17): `widgets::surface::Inspector` is still
    // DEFERRED (see the module doc's blocker list), so the aggregate carries its
    // two core-reachable sub-objects as direct fields until the fold. Everything
    // else the reference reaches via `self.gui.*` (`surface_info`, `key_stream`
    // sans-events, `terminal_info`, `vt_stream`, `show_demo_window`) is display-
    // only inside the deferred `draw`, so it is not needed here yet.

    /// Renderer-info panel state (INV-7). Core's `activateInspector` (Task 6)
    /// unsize-coerces `&raw mut self.renderer_info` into `renderer_state.inspector`
    /// (`*mut dyn InspectorOverlay`) — the acyclic overlay edge (renderer `state.rs`
    /// Task-5 PORT NOTE; the renderer reads overlay features back through the
    /// trait). Reference field: `self.gui.renderer_info`.
    pub renderer_info: widgets::renderer::Info,

    /// Keyboard-event ring (reference `self.gui.key_stream.events`). `recordKeyEvent`
    /// appends into it with the `max_capacity = 50` grow-then-evict policy
    /// (Inspector.zig:80-97, INV-9). Each stored `Event` owns its utf8/binding/pty
    /// Vecs and drops with the ring.
    pub key_stream_events: widgets::key::EventRing,
}

impl Inspector {
    /// Setup the ImGui state. This requires an ImGui context to be set.
    //
    // pub fn setup() void { ... }   (Inspector.zig:25-56)
    //
    // PORT NOTE (INV-11): mutates the *process-global* ImGui IO (not per-Inspector
    // state) — docking + sRGB config flags, disables the ini/log files, and adds
    // the embedded regular font to the shared atlas at `16 * 2` px. The caller
    // (apprt `Inspector.init`) sets the current context first (embedded.zig:1018).
    // R4-clean: touches only dcimgui-sys + `rustty_font::embedded` (both lower
    // tier). Fully unblocked, ported LIVE.
    pub fn setup() {
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };

        // Enable docking, which we use heavily for the UI.
        // io.ConfigFlags |= cimgui.c.ImGuiConfigFlags_DockingEnable;
        unsafe {
            (*io).ConfigFlags |= dcimgui_sys::ImGuiConfigFlags__ImGuiConfigFlags_DockingEnable
                as dcimgui_sys::ImGuiConfigFlags;

            // Our colorspace is sRGB.
            // io.ConfigFlags |= cimgui.c.ImGuiConfigFlags_IsSRGB;
            (*io).ConfigFlags |= dcimgui_sys::ImGuiConfigFlags__ImGuiConfigFlags_IsSRGB
                as dcimgui_sys::ImGuiConfigFlags;

            // Disable the ini file to save layout
            // io.IniFilename = null;
            // io.LogFilename = null;
            (*io).IniFilename = core::ptr::null();
            (*io).LogFilename = core::ptr::null();
        }

        // Use our own embedded font
        {
            // TODO: This will have to be recalculated for different screen DPIs.
            // This is currently hardcoded to a 2x content scale.
            // const font_size = 16 * 2;
            let font_size: f32 = 16.0 * 2.0;

            // var font_config: cimgui.c.ImFontConfig = undefined;
            // cimgui.ext.ImFontConfig_ImFontConfig(&font_config);
            //
            // PORT NOTE: `ImFontConfig` has non-zero defaults (OversampleH/V,
            // RasterizerMultiply/Density = 1.0, …) so the ext-shim constructor is
            // load-bearing — a zeroed struct would break rasterization. See the
            // dcimgui-sys ext.cpp FFI block.
            let mut font_config: core::mem::MaybeUninit<dcimgui_sys::ImFontConfig> =
                core::mem::MaybeUninit::uninit();
            unsafe { dcimgui_sys::ImFontConfig_ImFontConfig(font_config.as_mut_ptr()) };
            let mut font_config = unsafe { font_config.assume_init() };

            // font_config.FontDataOwnedByAtlas = false;
            font_config.FontDataOwnedByAtlas = false;

            // _ = cimgui.c.ImFontAtlas_AddFontFromMemoryTTF(
            //     io.Fonts,
            //     @ptrCast(@constCast(font.embedded.regular.ptr)),
            //     @intCast(font.embedded.regular.len),
            //     font_size,
            //     &font_config,
            //     null,
            // );
            //
            // PORT NOTE: `FontDataOwnedByAtlas = false` above means the atlas does
            // NOT take ownership of the byte buffer, so handing it the `'static`
            // `rustty_font::embedded::REGULAR` slice (the reference's `@embedFile`
            // static) is sound — the atlas only borrows it. The `*const u8` -> the
            // C `void*` font-data arg is the `@constCast`+`@ptrCast`.
            let _ = unsafe {
                dcimgui_sys::ImFontAtlas_AddFontFromMemoryTTF(
                    (*io).Fonts,
                    rustty_font::embedded::REGULAR.as_ptr() as *mut core::ffi::c_void,
                    rustty_font::embedded::REGULAR.len() as core::ffi::c_int,
                    font_size,
                    &font_config,
                    core::ptr::null(),
                )
            };
        }
    }

    /// Construct an inspector. Core's `activateInspector` heap-allocates this.
    //
    // pub fn init(alloc: Allocator) !Inspector {
    //     var gui: widgets.surface.Inspector = try .init(alloc);
    //     errdefer gui.deinit(alloc);
    //     return .{ .gui = gui };
    // }
    //
    // PORT NOTE (grows-in-place, Task 6/17): the reference builds the whole `gui`
    // aggregate and returns `.{ .gui = gui }` (with `mouse` defaulting via `.{}`).
    // Until `surface::Inspector` inhabits, we build the carried fields directly:
    // `mouse` = `Mouse::default()` (the `.{}` default, LIVE); `renderer_info` =
    // `renderer.Info.empty` (renderer.zig `empty`); `key_stream_events` =
    // `EventRing.init(alloc, 2)` (key.zig `Stream.init`'s `.init(alloc, 2)`). The
    // reference `!Inspector` is Zig's `alloc.dupe`/CircBuf alloc error; under
    // Rust's infallible global allocator the `!` collapses (a real OOM aborts —
    // panic=abort parity), so `init` is infallible. The ring's grow-then-evict
    // OutOfMemory *signal* is still surfaced by `recordKeyEvent`. `alloc` retained
    // for reference call-shape parity.
    pub fn init(_alloc: allocator_api2::alloc::Global) -> Inspector {
        Inspector {
            mouse: widgets::surface::Mouse::default(),
            renderer_info: widgets::renderer::Info::empty(),
            // EventRing(Event) filled with `Event::default()` (the CircBuf
            // `undefined`-fill; see key.rs EventRing PORT NOTE).
            key_stream_events: widgets::key::EventRing::init(2, widgets::key::Event::default())
                .expect("EventRing.init(2) is infallible under the global allocator"),
        }
    }

    /// Tear down the inspector. Core's `deactivateInspector`/`deinit` calls this
    /// before freeing the heap allocation.
    //
    // pub fn deinit(self: *Inspector, alloc: Allocator) void {
    //     self.gui.deinit(alloc);
    // }
    //
    // PORT NOTE: the reference `gui.deinit` iterates+deinits every stored Event
    // (freeing utf8/binding/pty) then frees the ring/map. Here the carried
    // sub-fields free themselves on `Drop` — the `EventRing`'s Events (and their
    // owned Vecs) drop with the ring, and `renderer_info`'s owned `Vec<Feature>`
    // drops with it; `mouse` is `Copy` (no owned memory) — so `deinit` is a no-op
    // retained for the reference call-shape (`insp.deinit(self.alloc)` before
    // `destroy`). `alloc` retained for parity.
    pub fn deinit(&mut self, _alloc: allocator_api2::alloc::Global) {
        // mouse/renderer_info/key_stream_events drop with `self`; nothing to free.
    }

    /// Returns the renderer info panel. This is a convenience function
    /// to access and find this state to read and modify.
    //
    // pub fn rendererInfo(self: *Inspector) *widgets.renderer.Info {
    //     return &self.gui.renderer_info;
    // }
    //
    // PORT NOTE (grows-in-place, Task 17): the reference returns
    // `&self.gui.renderer_info`; the carried direct field is `self.renderer_info`,
    // so the accessor returns `&mut self.renderer_info`. LIVE (`renderer.Info` is
    // the Task-5 live half). When `surface::Inspector` inhabits, the field path
    // becomes `self.gui.renderer_info` and this body updates in lock-step.
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn rendererInfo(&mut self) -> &mut widgets::renderer::Info {
        // return &self.gui.renderer_info;
        &mut self.renderer_info
    }

    /// Record a keyboard event.
    //
    // pub fn recordKeyEvent(
    //     self: *Inspector,
    //     alloc: Allocator,
    //     ev: inspector.KeyEvent,
    // ) Allocator.Error!void {
    //
    // (`inspector.KeyEvent` = `widgets.key.Event`; the caller — core's keyCallback
    // inspector defer — hands over an owned `Event` built from the key event.)
    #[allow(non_snake_case)] // faithful Zig name (R1); core surface.rs allows crate-wide
    pub fn recordKeyEvent(
        &mut self,
        _alloc: allocator_api2::alloc::Global,
        ev: crate::KeyEvent,
    ) -> Result<(), rustty_datastruct::circ_buf::Error> {
        // const max_capacity = 50;
        let max_capacity: usize = 50;

        // const events: *widgets.key.EventRing = &self.gui.key_stream.events;
        let events = &mut self.key_stream_events;

        // events.append(ev) catch |err| switch (err) {
        //
        // PORT NOTE (move-vs-shallow-copy): Zig's `append(ev)` shallow-copies the
        // Event value (ring + local `ev` transiently share slice pointers; the local
        // is discarded WITHOUT deinit, transferring ownership). Rust's `append`
        // MOVES `ev` and, on the full-signal early return, DROPS it — so a retry
        // needs a fresh owned Event. We hand the first attempt a `clone()` and keep
        // the original `ev` for the (rare, ring-full) retry. The extra clone on the
        // common Ok path is negligible for a dev-tool key recorder (≤50 events,
        // keypress-rate).
        match events.append(ev.clone()) {
            Ok(()) => Ok(()),

            // error.OutOfMemory => ...
            //
            // (CircBuf.append's ONLY error is `OutOfMemory` — the ring-full signal,
            // not a heap-allocation failure — so the `else => return err` arm of the
            // reference switch is vacuous; the match is exhaustive.)
            Err(rustty_datastruct::circ_buf::Error::OutOfMemory) => {
                // if (events.capacity() < max_capacity) {
                if events.capacity() < max_capacity {
                    // We're out of memory, but we can allocate to our capacity.
                    // const new_capacity = @min(events.capacity() * 2, max_capacity);
                    let new_capacity = core::cmp::min(events.capacity() * 2, max_capacity);
                    // try events.resize(alloc, new_capacity);
                    events.resize(new_capacity)?;
                    // try events.append(ev);
                    events.append(ev)
                } else {
                    // var it = events.iterator(.forward);
                    // if (it.next()) |old_ev| old_ev.deinit(alloc);
                    // events.deleteOldest(1);
                    //
                    // PORT NOTE (INV-9 element-drop-before-reuse): Rust's
                    // `delete_oldest` overwrites the evicted slot with
                    // `default.clone()`, which DROPS the old Event — freeing its
                    // owned utf8/binding/pty Vecs — before the slot is reused. That
                    // subsumes the reference's explicit `old_ev.deinit(alloc)` ahead
                    // of `deleteOldest(1)`.
                    events.delete_oldest(1);
                    // try events.append(ev);
                    events.append(ev)
                }
            }
        }
    }

    // =======================================================================
    // DEFERRED (Task 17 blockers — preserved-verbatim reference body, R1).
    // `recordPtyRead` and `render` both dereference `self.gui` (the still-
    // DEFERRED `widgets::surface::Inspector`); `render` additionally names the
    // cyclic core `Surface` type (`surface.renderer_state.mutex` /
    // `surface.renderer_thread.wakeup` — the R4 accessor-trait seam). They
    // inhabit in place once `surface::Inspector` + the core `Surface` accessor
    // trait land (grows-in-place doctrine; the exact shape `surface.rs` /
    // `terminal.rs` use for their own deferred aggregates). The bodies below are
    // the faithful `Inspector.zig` source (lines 100-131).
    // =======================================================================
    //
    // /// Record data read from the pty.
    // pub fn recordPtyRead(
    //     self: *Inspector,
    //     alloc: Allocator,
    //     t: *terminal.Terminal,
    //     data: []const u8,
    // ) !void {
    //     try self.gui.vt_stream.recordPtyRead(
    //         alloc,
    //         t,
    //         data,
    //     );
    // }
    //
    // PORT NOTE: `self.gui.vt_stream` is the `widgets::termio::Stream` field of
    // the DEFERRED `surface::Inspector`. `termio::Stream::recordPtyRead` is itself
    // already LIVE (Task 16), so this method unblocks the moment the `gui`
    // aggregate inhabits — no further termio work is needed here.
    //
    // /// Render the frame.
    // pub fn render(
    //     self: *Inspector,
    //     surface: *Surface,
    // ) void {
    //     // Draw the UI
    //     self.gui.draw(
    //         surface,
    //         self.mouse,
    //     );
    //
    //     // We always trigger a rebuild of the surface when the inspector
    //     // is focused because modifying the inspector can change the terminal
    //     // state. This is KIND OF expensive (wasted CPU if nothing was done)
    //     // but the inspector is a development tool and it expressly costs
    //     // more resources while open so its okay.
    //     surface.renderer_thread.wakeup.notify() catch {};   // INV-12
    // }
    //
    // PORT NOTE (INV-11/INV-12): `render` is the top-level draw — it hands the
    // whole widget tree to `self.gui.draw(surface, self.mouse)` (which takes the
    // single `surface.renderer_state.mutex` lock across every sub-window, INV-1),
    // then notifies the renderer thread to force a rebuild (INV-12; the `catch {}`
    // swallows the wakeup error — panic=abort parity, a lost wakeup is non-fatal).
    // The `surface: *Surface` param is the cyclic core edge (R4): it must arrive
    // through a renderer/core-defined Surface accessor trait (the
    // `InspectorOverlay` precedent), NOT a direct `rustty_core::Surface` name.
    // NOTE: the ImGui `SetCurrentContext` (INV-11) is NOT here — it is the apprt
    // embedded `Inspector.render` wrapper's job (it sets the per-surface ig
    // context current, then calls THIS `render`); this crate stays main-thread-
    // affine but context-agnostic.
}
