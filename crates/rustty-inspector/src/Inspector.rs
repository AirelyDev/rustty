//! The Inspector is a development tool to debug the terminal. This is
//! useful for terminal application developers as well as people potentially
//! debugging issues in Ghostty itself.
//!
//! PORTS: ghostty src/inspector/Inspector.zig (the top-level inspector
//! aggregate + its `render`).
//!
//! PORT STATUS: all blockers landed, so the whole aggregate — including
//! `render`/`recordPtyRead` — is now INHABITED.
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
//! `gui: widgets::surface::Inspector` field below IS that aggregate (Task 15's
//! `surface::Inspector`, now inhabited) — folding `renderer_info` /
//! `key_stream_events` back under it, exactly as the module doc's grows-in-place
//! plan anticipated (`rendererInfo`/`recordKeyEvent` below update their field
//! path from the direct fields to `self.gui.renderer_info` /
//! `self.gui.key_stream.events` in lock-step with this fold).
//!
//! **R4 acyclic Surface seam.** `render`'s reference signature is `fn render(
//! self: *Inspector, surface: *Surface) void`, which both hands the whole
//! `Surface` to `self.gui.draw` (now `SurfaceData`, `widgets/surface.rs`'s own
//! R4 seam) and calls `surface.renderer_thread.wakeup.notify()` (INV-12). The
//! `renderer_thread: Thread<A>` field is a core-local generic type (over the
//! `apprt::App` trait) that cannot be named here without an inspector -> core
//! dependency cycle (core already depends on inspector). Since the ONLY thing
//! `render` does with it is call `.wakeup.notify()`, the port takes that
//! wakeup handle directly as a `&rustty_xev::Async` parameter (a leaf below
//! core `render_thread.wakeup`'s own field type) instead of the whole
//! `Thread<A>` — no new rustty-inspector -> core/apprt edge; `check_acyclic.sh`
//! stays green. `recordPtyRead` needs no such seam (it already took a raw
//! `*mut Terminal`, not `*Surface`).
//!
//! Ported LIVE here: `setup` (static ImGui-IO mutation, INV-11), `init` /
//! `deinit`, `rendererInfo` (accessor into `self.gui.renderer_info`),
//! `recordKeyEvent` (INV-9 grow-then-evict), `recordPtyRead`, and `render`
//! (INV-11/INV-12).

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
// `crate::KeyEvent`. `terminal.Terminal` is `rustty_vt::terminal::Terminal`
// (recordPtyRead's raw-pointer param); `Surface` has no direct port (the R4
// acyclic seam — see the module doc). `widgets` is `crate::widgets`.
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
    // PORT NOTE (fold complete): `widgets::surface::Inspector` (Task 15) is now
    // inhabited, so the whole widget tree lives under this one field — exactly
    // the reference shape. `rendererInfo`/`recordKeyEvent` below reach into it
    // (`self.gui.renderer_info` / `self.gui.key_stream.events`); core's
    // `activateInspector`/the KAT test updated their field path in lock-step
    // (`&mut (*ptr).gui.renderer_info`, `&mut insp.gui.renderer_info`).
    pub gui: widgets::surface::Inspector,
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
    // PORT NOTE (fold complete): the reference builds the whole `gui` aggregate
    // and returns `.{ .gui = gui }` (with `mouse` defaulting via `.{}`).
    // `surface::Inspector::init` is fallible (`key::Stream::init`'s CircBuf
    // alloc-error signal); the reference's own `!Inspector` collapses under
    // Rust's infallible global allocator everywhere ELSE in this crate (a real
    // OOM aborts — panic=abort parity), so this top-level `init` stays
    // infallible and `.expect()`s the inner `Result` (matching the reference's
    // own "OOM never actually happens under the global allocator" stance).
    // `alloc` retained for reference call-shape parity.
    pub fn init(alloc: allocator_api2::alloc::Global) -> Inspector {
        Inspector {
            mouse: widgets::surface::Mouse::default(),
            gui: widgets::surface::Inspector::init(alloc)
                .expect("surface::Inspector::init is infallible under the global allocator"),
        }
    }

    /// Tear down the inspector. Core's `deactivateInspector`/`deinit` calls this
    /// before freeing the heap allocation.
    //
    // pub fn deinit(self: *Inspector, alloc: Allocator) void {
    //     self.gui.deinit(alloc);
    // }
    pub fn deinit(&mut self, alloc: allocator_api2::alloc::Global) {
        self.gui.deinit(alloc);
    }

    /// Returns the renderer info panel. This is a convenience function
    /// to access and find this state to read and modify.
    //
    // pub fn rendererInfo(self: *Inspector) *widgets.renderer.Info {
    //     return &self.gui.renderer_info;
    // }
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn rendererInfo(&mut self) -> &mut widgets::renderer::Info {
        // return &self.gui.renderer_info;
        &mut self.gui.renderer_info
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
        let events = &mut self.gui.key_stream.events;

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

    /// Record data read from the pty.
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
    // PORT NOTE: `termio::Stream::recordPtyRead` is infallible in the port
    // (Task 16 design decision), unlike the reference's `try`.
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn recordPtyRead(
        &mut self,
        alloc: allocator_api2::alloc::Global,
        t: *mut rustty_vt::terminal::Terminal,
        data: &[u8],
    ) {
        // self.gui.vt_stream.recordPtyRead(alloc, t, data);
        self.gui.vt_stream.recordPtyRead(alloc, t, data);
    }

    /// Render the frame.
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
    // PORT NOTE (INV-11/INV-12, R4 acyclic Surface seam — module doc): `render`
    // is the top-level draw — it hands the whole widget tree to `self.gui.draw`
    // (which takes the single `renderer_state.mutex` lock across every
    // sub-window, INV-1), then notifies the renderer thread to force a rebuild
    // (INV-12; `let _ =` swallows the wakeup error — panic=abort parity, a lost
    // wakeup is non-fatal, matching the reference's `catch {}`). `data:
    // widgets::surface::SurfaceData` replaces the reference's `surface: *Surface`
    // (surface.rs's own R4 seam); `wakeup: &rustty_xev::Async` replaces
    // `surface.renderer_thread` (this file's module-doc R4 seam) — neither adds
    // a rustty-inspector -> core/apprt dependency edge.
    // NOTE: the ImGui `SetCurrentContext` (INV-11) is NOT here — it is the apprt
    // embedded `Inspector.render` wrapper's job (it sets the per-surface ig
    // context current, then calls THIS `render`); this crate stays main-thread-
    // affine but context-agnostic.
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn render(&mut self, data: widgets::surface::SurfaceData, wakeup: &rustty_xev::Async) {
        // Draw the UI
        // self.gui.draw(
        //     surface,
        //     self.mouse,
        // );
        self.gui.draw(data, self.mouse);

        // We always trigger a rebuild of the surface when the inspector
        // is focused because modifying the inspector can change the terminal
        // state. This is KIND OF expensive (wasted CPU if nothing was done)
        // but the inspector is a development tool and it expressly costs
        // more resources while open so its okay.
        // surface.renderer_thread.wakeup.notify() catch {};   // INV-12
        let _ = wakeup.notify();
    }
}
