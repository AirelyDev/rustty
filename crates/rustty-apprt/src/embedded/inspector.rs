//! PORTS: ghostty src/apprt/embedded.zig — the `Inspector` (dcimgui + Metal
//! terminal inspector, chunk E4, embedded.zig 992-1243) and its `Backend`,
//! plus the `Key -> ImGuiKey` mapping (`key.zig` `imguiKey`).

use super::*;

/// Inspector is the state required for the terminal inspector. A terminal
/// inspector is 1:1 with a Surface.
//
// pub const Inspector = struct { ... }   (embedded.zig:994)
//
// [CHUNK E4 — the apprt-tier Metal render loop, complete] The inspector is the
// dcimgui (Dear ImGui) + Metal debug UI. The struct is inhabited with its real
// fields; `init` (`ImGui_CreateContext` + `CoreInspector.setup()` the shared IO
// once, INV-11), `deinit` (`ImGui_ImplMetal_Shutdown` if a backend +
// `ImGui_DestroyContext`), `Backend::deinit`, the REAL non-GPU
// `updateContentScale`/`updateSize`, `initMetal` (the dcimgui
// `ImGui_ImplMetal_Init` + backend management + the objc `device.release`
// msgSend balance), `newFrame` (the pure delta-time `io.DeltaTime` setup), and
// `renderMetal` (the double-frame loop + `rustty_inspector::Inspector::render`
// via the R4 acyclic `SurfaceData`/`wakeup` seam + the `MTLRenderCommandEncoder`
// objc2 `msg_send!` construction/`endEncoding`) are all LIVE. The objc/imgui
// input callbacks (mouse/scroll/cursor/focus/text/key) remain leaves that land
// with the input wiring (Task 6/9). `queueRender` forwards LIVE to
// [`Surface::queueInspectorRender`], which now itself dispatches the
// `render_inspector` action to the C host.
pub struct Inspector {
    // const cimgui = @import("dcimgui");
    //
    // PORT NOTE: the reference imports the `dcimgui` binding package for the whole
    // struct. In Rust that is the `dcimgui_sys` crate, referenced fully-qualified
    // at each call site below (plus the `ImGuiContext` import for `ig_ctx`).

    // surface: *Surface,
    pub surface: *mut Surface,
    // ig_ctx: *cimgui.c.ImGuiContext,
    //
    // PORT NOTE (LIFETIMES §1, GHOSTTY_C): the Dear ImGui context handle, OWNED
    // via `ImGui_CreateContext` -> `ImGui_DestroyContext`. Task 4 wired dcimgui-sys,
    // so it is now the real `*mut ImGuiContext`, no longer an opaque `*mut c_void`.
    pub ig_ctx: *mut ImGuiContext,
    // backend: ?Backend = null,
    pub backend: Option<Backend>,
    // content_scale: f64 = 1,
    pub content_scale: f64,

    /// Our previous instant used to calculate delta time for animations.
    // instant: ?std.time.Instant = null,
    pub instant: Option<std::time::Instant>,
}

// const Backend = enum { ... };
//
// PORT NOTE: the reference `Backend` is a file-private `const` enum; Rust's
// pub-field visibility (it appears in the `pub backend` field of the `pub`
// `Inspector`) requires it be at least as visible → `pub`. Its sole variant's
// render backend is Metal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    // metal,
    Metal,
}

impl Backend {
    // pub fn deinit(self: Backend) void { ... }
    //
    // PORT NOTE (Task 4): tears down the imgui_impl_metal global state. The
    // reference gates the `ImGui_ImplMetal_Shutdown()` call on `isDarwin()`, and so
    // does this (`macos || ios`, the workspace's `isDarwin()` spelling).
    //
    // CORRECTION (task 20, S46-F4): every Metal gate in this file — here, `Defer`,
    // `initMetal`, `renderMetal`, `newFrame` — and the dcimgui-sys Metal backend
    // were `#[cfg(target_os = "macos")]`, while the `rustty_inspector_metal_*` CAPI
    // exports that call them are `any(macos, ios)`. On iOS `initMetal` reported
    // success without initialising Metal and `renderMetal` hit its non-Darwin
    // `unimplemented!()`. The note here used to say the binding "is itself
    // `#[cfg(target_os = "macos")]`, so the `.metal` arm is a no-op off macOS".
    pub fn deinit(self) {
        // switch (self) {
        //     .metal => if (builtin.target.os.tag.isDarwin()) cimgui.ImGui_ImplMetal_Shutdown(),
        // }
        match self {
            Backend::Metal => {
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                unsafe {
                    dcimgui_sys::ImGui_ImplMetal_Shutdown()
                };
            }
        }
    }
}

/// Minimal `defer`-style scope guard — runs `f` on drop. Mirrors the private
/// `Defer` helper `rustty_inspector::widgets::surface` uses for its ImGui
/// `Begin`/`End` pairings; here it backs the objc release/`endEncoding`
/// balances in [`Inspector::initMetal`]/[`Inspector::renderMetal`] (the
/// reference's `defer { … }` blocks), so the release/`endEncoding` msgSend runs
/// on every exit path (including the early `return false` in `initMetal`).
/// panic=abort means no unwinding ever crosses these FFI calls, so
/// drop-on-scope-exit is the sole runtime path (no double-fire, no leak).
#[cfg(any(target_os = "macos", target_os = "ios"))]
struct Defer<F: FnMut()>(F);

#[cfg(any(target_os = "macos", target_os = "ios"))]
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

impl Inspector {
    // pub fn init(surface: *Surface) !Inspector { ... }
    //
    // PORT NOTE (Task 4): constructs the per-Surface Dear ImGui context, sets up the
    // shared IO once (`CoreInspector.setup()`), and activates the core inspector.
    // `!Inspector` (its only error is `error.OutOfMemory` from `ImGui_CreateContext`)
    // maps to `Result<Inspector, ApprtError>`. `CoreInspector` is
    // `rustty_inspector::Inspector`; `activateInspector` is a core `Surface` method
    // and is LIVE (it heap-boxes the core inspector, publishes it onto
    // `renderer_state` under the mutex, and notifies renderer + termio). Its
    // error is swallowed here exactly as the reference does.
    pub fn init(surface: *mut Surface) -> Result<Inspector, ApprtError> {
        // const ig_ctx = cimgui.c.ImGui_CreateContext(null) orelse return error.OutOfMemory;
        let ig_ctx = unsafe { dcimgui_sys::ImGui_CreateContext(core::ptr::null_mut()) };
        if ig_ctx.is_null() {
            return Err(ApprtError);
        }
        // errdefer cimgui.c.ImGui_DestroyContext(ig_ctx);
        //
        // PORT NOTE: no fallible op follows `ImGui_CreateContext` (the struct
        // construction is infallible and `activateInspector`'s error is swallowed
        // below), so the errdefer never actually fires — elided.

        // cimgui.c.ImGui_SetCurrentContext(ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };
        // io.BackendPlatformName = "rustty_embedded";
        unsafe { (*io).BackendPlatformName = c"rustty_embedded".as_ptr() };

        // // Setup our core inspector
        // CoreInspector.setup();
        rustty_inspector::Inspector::setup();
        // surface.core_surface.activateInspector() catch |err| {
        //     log.err("failed to activate inspector err={}", .{err});
        // };
        if let Err(err) = unsafe { (*surface).core_surface.activateInspector() } {
            // log.err("failed to activate inspector err={}", .{err});
            log::error!(target: LOG_SCOPE, "failed to activate inspector err={err:?}");
        }

        // return .{
        //     .surface = surface,
        //     .ig_ctx = ig_ctx,
        // };
        //
        // PORT NOTE: the reference relies on the struct field defaults for the
        // fields it does not set (`backend = null`, `content_scale = 1`,
        // `instant = null`); Rust has no field defaults, so they are spelled out.
        Ok(Inspector {
            surface,
            ig_ctx,
            backend: None,
            content_scale: 1.0,
            instant: None,
        })
    }

    // pub fn deinit(self: *Inspector) void { ... }
    //
    // PORT NOTE (Task 4): core `deactivateInspector` + the ImGui context teardown
    // (shut down the Metal backend if one was initialized, then destroy the
    // context). Reached by [`Surface::freeInspector`]. `deactivateInspector` is a
    // core `Surface` method and is LIVE (it clears the renderer-state aliases
    // under the mutex, notifies renderer + termio, and frees the core inspector).
    pub fn deinit(&mut self) {
        // self.surface.core_surface.deactivateInspector();
        unsafe { (*self.surface).core_surface.deactivateInspector() };
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // if (self.backend) |v| v.deinit();
        if let Some(v) = self.backend {
            v.deinit();
        }
        // cimgui.c.ImGui_DestroyContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_DestroyContext(self.ig_ctx) };
    }

    /// Queue a render for the next frame.
    //
    // pub fn queueRender(self: *Inspector) void { ... }
    //
    // PORT NOTE: a pure delegation to [`Surface::queueInspectorRender`] (no dcimgui);
    // ported LIVE — the panic is in that Phase-3 leaf, not here (the draw/refresh
    // delegation precedent).
    pub fn queueRender(&mut self) {
        // self.surface.queueInspectorRender();
        unsafe { (*self.surface).queueInspectorRender() };
    }

    /// Initialize the inspector for a metal backend.
    //
    // pub fn initMetal(self: *Inspector, device: objc.Object) bool { ... }
    //
    // PORT NOTE (Task 3): dcimgui `ImGui_ImplMetal_Init` + the backend
    // teardown/tag management. The objc `device: objc.Object` handle → an opaque
    // `*mut c_void` (the objc id crosses the C ABI untyped). Target of the Darwin
    // CAPI `rustty_inspector_metal_init`.
    pub fn initMetal(&mut self, device: *mut c_void) -> bool {
        // defer device.msgSend(void, objc.sel("release"), .{});
        //
        // PORT NOTE (INV-11 / I16 objc-id balance): balances the +1 retain the host
        // handed us by releasing `device` on scope exit — via the `Defer` guard so
        // it fires on every exit path, including the early `return false` below.
        // `device` is an opaque objc id crossing the C ABI untyped (never a
        // compile-time Metal type), so this uses the same raw-`AnyObject` +
        // `msg_send!` idiom as `renderMetal`'s command_buffer/desc release and the
        // `rustty_set_window_background_blur` blur leaf (rustty-apprt-c/src/lib.rs)
        // — no objc2-metal/objc2-foundation typed bindings needed. This fn (like
        // `renderMetal`) is reached only through the Darwin-gated
        // CAPI export, so the guard is likewise Darwin-only; off Darwin `device` is
        // untouched (never released — there is no host retain to balance there).
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let _release_device = Defer(|| {
            let ptr: *mut objc2::runtime::AnyObject = device.cast();
            // SAFETY: `device` is the host-owned `id<MTLDevice>` handed to this
            // call at +1 (the CAPI contract); `release` balances that exactly
            // once, on every exit path, and is never sent again afterward.
            let _: () = unsafe { objc2::msg_send![ptr, release] };
        });

        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };

        // if (self.backend) |v| {
        //     v.deinit();
        //     self.backend = null;
        // }
        if let Some(v) = self.backend {
            v.deinit();
            self.backend = None;
        }

        // if (!cimgui.ImGui_ImplMetal_Init(device.value)) {
        //     log.warn("failed to initialize metal backend", .{});
        //     return false;
        // }
        //
        // PORT NOTE: `ImGui_ImplMetal_Init` is Darwin-gated (`macos || ios`) in
        // dcimgui-sys (the imgui_impl_metal.mm backend is macOS-only), mirroring the
        // reference `Backend.deinit`'s `isDarwin()` guard. This method is only ever
        // reached through the Darwin-gated `rustty_inspector_metal_init`
        // export, so off Darwin the call is elided and the arm is unreachable in
        // practice.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            // SAFETY: `device` is the host-owned `id<MTLDevice>` the caller passed;
            // the current ImGui context was set above.
            if !unsafe { dcimgui_sys::ImGui_ImplMetal_Init(device) } {
                // log.warn("failed to initialize metal backend", .{});
                log::warn!(target: LOG_SCOPE, "failed to initialize metal backend");
                return false;
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = device;
        }

        // self.backend = .metal;
        self.backend = Some(Backend::Metal);

        // log.debug("initialized metal backend", .{});
        log::debug!(target: LOG_SCOPE, "initialized metal backend");

        // return true;
        true
    }

    // pub fn renderMetal(self: *Inspector, command_buffer: objc.Object, desc: objc.Object) !void { ... }
    //
    // PORT NOTE (Task 3/4 — live): the double-frame loop's dcimgui half
    // (`ImGui_ImplMetal_NewFrame`/`ImGui_NewFrame`/`ImGui_Render`/
    // `ImGui_GetDrawData`/`ImGui_ImplMetal_RenderDrawData`) and `self.newFrame()`
    // were already wired; this landed the two remaining blockers: (1)
    // `inspector.render(surface)` -> `rustty_inspector::Inspector::render` (Task
    // 15/17's widget aggregate, now inhabited) fed the R4 acyclic `SurfaceData`
    // gathered straight off `core_surface` (main-thread-affine, INV-11;
    // `render`'s own `gui.draw` takes `renderer_state.mutex` internally, INV-1 —
    // no lock is taken HERE) plus the `&rustty_xev::Async` wakeup handle; and (2)
    // the `MTLRenderCommandEncoder` construction
    // (`command_buffer.msgSend(renderCommandEncoderWithDescriptor:)`) + the
    // `release`/`endEncoding` msgSends via the objc2 raw-`AnyObject` + `msg_send!`
    // idiom (`Defer`-backed, matching the `rustty_set_window_background_blur`
    // blur leaf precedent). The two objc `objc.Object` handles stay opaque
    // `*mut c_void` (crossing the C ABI untyped, never a compile-time Metal
    // type). Target of the Darwin CAPI `rustty_inspector_metal_render`.
    //
    // FIDELITY NOTE: the reference `renderMetal` body does NOT call
    // `ImGui_SetCurrentContext` (unlike `initMetal`/`updateContentScale`/
    // `updateSize`, which all do) — verified against embedded.zig:1066-1126.
    // Ported faithfully as-is (R1); not adding a call the reference omits.
    //
    // CFG NOTE: Metal (and the `ImGui_ImplMetal_*` dcimgui-sys symbols /
    // macOS-only `objc2` msgSends this body uses) exists only on Darwin, so —
    // like the `MacOS`/`IOS` platform structs above — this fn is split into a
    // live Darwin (`macos || ios`) definition and a non-Darwin stub. The
    // sole caller is the Darwin-gated CAPI export
    // `rustty_inspector_metal_render`, so the stub is unreachable in practice.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub fn renderMetal(
        &mut self,
        command_buffer: *mut c_void,
        desc: *mut c_void,
    ) -> Result<(), ApprtError> {
        // defer {
        //     command_buffer.msgSend(void, objc.sel("release"), .{});
        //     desc.msgSend(void, objc.sel("release"), .{});
        // }
        //
        // PORT NOTE (INV-11/I16 objc-id balance): both ids arrive at +1 from the
        // host; declared first (Rust drops in REVERSE declaration order) so they
        // release LAST — after `_end_encoding` below — exactly mirroring the
        // reference's nesting (the later `defer encoder.endEncoding()` fires
        // first, LIFO). Declaring `desc` before `command_buffer` here means they
        // drop in `command_buffer`-then-`desc` order, matching the reference's
        // combined-block statement order; the two objects are independent, so
        // this ordering is not load-bearing, only faithful.
        let _release_desc = Defer(|| {
            let ptr: *mut objc2::runtime::AnyObject = desc.cast();
            // SAFETY: `desc` is the host-owned render-pass-descriptor id handed
            // to this call at +1; released exactly once, on every exit path.
            let _: () = unsafe { objc2::msg_send![ptr, release] };
        });
        let _release_command_buffer = Defer(|| {
            let ptr: *mut objc2::runtime::AnyObject = command_buffer.cast();
            // SAFETY: `command_buffer` is the host-owned command-buffer id
            // handed to this call at +1; released exactly once, on every exit.
            let _: () = unsafe { objc2::msg_send![ptr, release] };
        });

        // assert(self.backend == .metal);
        assert_eq!(self.backend, Some(Backend::Metal));
        // //log.debug("render", .{});

        // Setup our imgui frame. We need to render multiple frames to ensure
        // ImGui completes all its state processing. I don't know how to fix
        // this.
        // for (0..2) |_| {
        for _ in 0..2 {
            // cimgui.ImGui_ImplMetal_NewFrame(desc.value);
            // SAFETY: `desc` is the host-owned `MTLRenderPassDescriptor*`.
            unsafe { dcimgui_sys::ImGui_ImplMetal_NewFrame(desc) };
            // try self.newFrame();
            self.newFrame()?;
            // cimgui.c.ImGui_NewFrame();
            unsafe { dcimgui_sys::ImGui_NewFrame() };

            // Build our UI
            // render: {
            //     const surface = &self.surface.core_surface;
            //     const inspector = surface.inspector orelse break :render;
            //     inspector.render(surface);
            // }
            'render: {
                // const surface = &self.surface.core_surface;
                // SAFETY: `self.surface` is the live back-reference to the owning
                // apprt Surface (embedded.zig:997/1029 LIFETIMES row).
                let core_surface: *mut CoreSurface =
                    unsafe { &raw mut (*self.surface).core_surface };
                // const inspector = surface.inspector orelse break :render;
                // SAFETY: `core_surface` is valid for the duration of this call
                // (owned by the live `self.surface`).
                let inspector: *mut rustty_inspector::Inspector =
                    match unsafe { (*core_surface).inspector } {
                        Some(v) => v,
                        None => break 'render,
                    };
                // inspector.render(surface);
                //
                // PORT NOTE (R4 acyclic Surface seam, rustty_inspector::
                // Inspector.rs module doc): `SurfaceData` replaces the
                // reference's `surface: *Surface`; every field is a plain `Copy`
                // value or raw pointer read directly off `core_surface` (none of
                // them are terminal derefs — `Inspector::render`'s own
                // `gui.draw` takes `renderer_state.mutex` internally across
                // every sub-window, INV-1, so no lock is taken here). `wakeup`
                // replaces `surface.renderer_thread` (INV-12).
                let data = rustty_inspector::widgets::surface::SurfaceData {
                    renderer_state: unsafe { (*core_surface).renderer_state.as_ptr() },
                    alloc: allocator_api2::alloc::Global,
                    size: unsafe { (*core_surface).size },
                    font_size_points: unsafe { (*core_surface).font_size.points },
                    font_size_pixels: unsafe { (*core_surface).font_size.pixels() },
                    mouse_click_state: unsafe { (*core_surface).mouse.click_state },
                    selection_gesture: unsafe {
                        &raw const (*core_surface).mouse.selection_gesture
                    },
                    // The `ad96613a8` termio Copy/Export seam: the erased core
                    // `*Surface` itself plus core's monomorphized trampolines
                    // (the reference passes `surface` down into
                    // `vt_stream.draw`, which reaches `rt_surface`/`rt_app`
                    // through it).
                    surface: core_surface.cast(),
                    rt_set_clipboard: CoreSurface::inspector_rt_set_clipboard,
                    rt_export_terminal_io: CoreSurface::inspector_rt_export_terminal_io,
                };
                let wakeup: &rustty_xev::Async =
                    unsafe { &(*(*core_surface).renderer_thread.as_ptr()).wakeup };
                // SAFETY: `inspector` is non-null (validated above) and owned by
                // `core_surface`'s renderer state for the duration of this call.
                unsafe { (*inspector).render(data, wakeup) };
            }

            // Render
            // cimgui.c.ImGui_Render();
            unsafe { dcimgui_sys::ImGui_Render() };
        }

        // MTLRenderCommandEncoder
        // const encoder = command_buffer.msgSend(
        //     objc.Object,
        //     objc.sel("renderCommandEncoderWithDescriptor:"),
        //     .{desc.value},
        // );
        //
        // PORT NOTE: the raw-`AnyObject` + `msg_send!` idiom (no objc2-metal
        // typed `MTLCommandBuffer`/`MTLRenderCommandEncoder` bindings) — these
        // ids cross the C ABI untyped, exactly like the reference's untyped
        // `objc.Object.fromId(...).msgSend(...)`.
        let encoder: *mut objc2::runtime::AnyObject = {
            let cb_ptr: *mut objc2::runtime::AnyObject = command_buffer.cast();
            let desc_ptr: *mut objc2::runtime::AnyObject = desc.cast();
            // SAFETY: `command_buffer`/`desc` are the host-owned live ids above;
            // `renderCommandEncoderWithDescriptor:` is a Cocoa "get" method (the
            // returned encoder is autoreleased, NOT owned by us at +1 — it must
            // NOT be released, only `endEncoding`'d below).
            unsafe { objc2::msg_send![cb_ptr, renderCommandEncoderWithDescriptor: desc_ptr] }
        };
        // defer encoder.msgSend(void, objc.sel("endEncoding"), .{});
        let _end_encoding = Defer(|| {
            let _: () = unsafe { objc2::msg_send![encoder, endEncoding] };
        });
        // cimgui.ImGui_ImplMetal_RenderDrawData(
        //     cimgui.c.ImGui_GetDrawData(),
        //     command_buffer.value,
        //     encoder.value,
        // );
        // SAFETY: `command_buffer`/`encoder` are live objc ids for the duration
        // of this call; `ImGui_GetDrawData()` returns the current context's
        // per-frame draw data (never stored — PREPORT_INSPECTOR §1).
        unsafe {
            dcimgui_sys::ImGui_ImplMetal_RenderDrawData(
                dcimgui_sys::ImGui_GetDrawData(),
                command_buffer,
                encoder.cast(),
            )
        };

        Ok(())
    }

    /// Non-Darwin stub — see the CFG NOTE on the macOS `renderMetal` above.
    /// Metal does not exist off Darwin, so there is nothing to port here; this
    /// fn is unreachable off Darwin (its sole caller, `rustty_inspector_metal_render`,
    /// is itself `any(target_os = "macos", target_os = "ios")`).
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub fn renderMetal(
        &mut self,
        command_buffer: *mut c_void,
        desc: *mut c_void,
    ) -> Result<(), ApprtError> {
        let _ = (command_buffer, desc);
        unimplemented!(
            "embedded::Inspector::renderMetal — Metal is Darwin-only; unreachable off Darwin"
        )
    }

    // pub fn updateContentScale(self: *Inspector, x: f64, y: f64) void { ... }
    //
    // PORT NOTE (WS6 T3 — Phase-3 leaf CLOSED): both halves are now LIVE. The
    // pure-Rust half caches `content_scale` (read by `cursorPosCallback`'s pixel
    // scaling); the dcimgui half default-constructs an `ImGuiStyle` through the
    // `ext.cpp` constructor shim (a zero-initialized struct is NOT a faithful
    // substitute — Dear ImGui's `ImGuiStyle::ImGuiStyle()` sets non-zero defaults
    // such as `CurveTessellationTol`), scales it, and overwrites the context's
    // active style. Scaling from a FRESH default style each call is what makes
    // repeated calls idempotent rather than compounding.
    pub fn updateContentScale(&mut self, x: f64, y: f64) {
        // _ = y;   (the vertical scale is unused — `x` drives both axes)
        let _ = y;
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        //
        // INV-11: every dcimgui touch is gated on this Inspector's own context.
        // SAFETY: `self.ig_ctx` is the live context created by `Inspector::init`
        // and destroyed only by `Inspector::deinit`.
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };

        // Cache our scale because we use it for cursor position calculations.
        // self.content_scale = x;
        self.content_scale = x;

        // Setup a new style and scale it appropriately. We must use the
        // ImGuiStyle constructor to get proper default values (e.g.,
        // CurveTessellationTol) rather than zero-initialized values.
        // var style: cimgui.c.ImGuiStyle = undefined;
        // cimgui.ext.ImGuiStyle_ImGuiStyle(&style);
        // cimgui.c.ImGuiStyle_ScaleAllSizes(&style, @floatCast(x));
        // const active_style = cimgui.c.ImGui_GetStyle();
        // active_style.* = style;
        //
        // PORT NOTE: Zig's `var style: ImGuiStyle = undefined` -> `MaybeUninit`,
        // initialized in place by the `ext.cpp` constructor shim before any read
        // (so `assume_init` is sound). `@floatCast(x)` -> `x as f32`.
        // `active_style.* = style` -> a raw-pointer write; `ImGuiStyle` is a POD
        // C struct with no `Drop`, so overwriting through the pointer is the
        // exact Zig semantics.
        // SAFETY: the current context was set above, so `ImGui_GetStyle()`
        // returns this Inspector's live style; `style` is fully initialized by
        // the constructor shim before it is scaled and written back.
        unsafe {
            let mut style = core::mem::MaybeUninit::<dcimgui_sys::ImGuiStyle>::uninit();
            dcimgui_sys::ImGuiStyle_ImGuiStyle(style.as_mut_ptr());
            let mut style = style.assume_init();
            dcimgui_sys::ImGuiStyle_ScaleAllSizes(&raw mut style, x as f32);
            let active_style = dcimgui_sys::ImGui_GetStyle();
            core::ptr::write(active_style, style);
        }
    }

    // pub fn updateSize(self: *Inspector, width: u32, height: u32) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): the imgui IO `DisplaySize` write now
    // goes live — `ImGui_SetCurrentContext` first (INV-11: every dcimgui IO touch
    // is gated on the Inspector's own context), then `ImGui_GetIO()`, then the
    // `DisplaySize` field write.
    pub fn updateSize(&mut self, width: u32, height: u32) {
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };
        // io.DisplaySize = .{ .x = @floatFromInt(width), .y = @floatFromInt(height) };
        unsafe {
            (*io).DisplaySize = dcimgui_sys::ImVec2 {
                x: width as f32,
                y: height as f32,
            };
        }
    }

    // pub fn mouseButtonCallback(self, action, button, mods) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddMouseButtonEvent`
    // now goes live. Target of the E9 CAPI `rustty_inspector_mouse_button`.
    pub fn mouseButtonCallback(
        &mut self,
        action: mouse::ButtonState,
        button: mouse::Button,
        mods: Mods,
    ) {
        // _ = mods;
        let _ = mods;

        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };

        // const imgui_button = switch (button) {
        //     .left => cimgui.c.ImGuiMouseButton_Left,
        //     .middle => cimgui.c.ImGuiMouseButton_Middle,
        //     .right => cimgui.c.ImGuiMouseButton_Right,
        //     else => return, // unsupported
        // };
        //
        // PORT NOTE: `dcimgui_sys`'s bindgen surface names the (tagged, non-anonymous)
        // C enum's constants `ImGuiMouseButton__ImGuiMouseButton_{Left,Right,Middle}`
        // (the tag `ImGuiMouseButton_` is itself prefixed onto each already-prefixed
        // variant name — a bindgen naming artifact of the tagged C enum, confirmed
        // against the generated `bindings.rs`); `ImGuiIO_AddMouseButtonEvent`'s `button`
        // parameter is a plain `c_int` (not the enum type), matching the reference's
        // untyped `ImGuiMouseButton_Left` usage.
        let imgui_button: c_int = match button {
            mouse::Button::Left => dcimgui_sys::ImGuiMouseButton__ImGuiMouseButton_Left as c_int,
            mouse::Button::Middle => {
                dcimgui_sys::ImGuiMouseButton__ImGuiMouseButton_Middle as c_int
            }
            mouse::Button::Right => dcimgui_sys::ImGuiMouseButton__ImGuiMouseButton_Right as c_int,
            _ => return, // unsupported
        };

        // cimgui.c.ImGuiIO_AddMouseButtonEvent(io, imgui_button, action == .press);
        unsafe {
            dcimgui_sys::ImGuiIO_AddMouseButtonEvent(
                io,
                imgui_button,
                action == mouse::ButtonState::Press,
            )
        };
    }

    // pub fn scrollCallback(self, xoff, yoff, mods) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddMouseWheelEvent`
    // now goes live. Target of the E9 CAPI `rustty_inspector_mouse_scroll`.
    pub fn scrollCallback(&mut self, xoff: f64, yoff: f64, mods: mouse::ScrollMods) {
        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };

        // // For precision scrolling (trackpads), the values are in pixels which
        // // scroll way too fast. Scale them down to approximate discrete wheel
        // // notches. imgui expects 1.0 to scroll ~5 lines of text.
        // const scale: f64 = if (mods.precision) 0.1 else 1.0;
        let scale: f64 = if mods.precision { 0.1 } else { 1.0 };
        // cimgui.c.ImGuiIO_AddMouseWheelEvent(
        //     io,
        //     @floatCast(xoff * scale),
        //     @floatCast(yoff * scale),
        // );
        unsafe {
            dcimgui_sys::ImGuiIO_AddMouseWheelEvent(
                io,
                (xoff * scale) as f32,
                (yoff * scale) as f32,
            )
        };
    }

    // pub fn cursorPosCallback(self: *Inspector, x: f64, y: f64) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddMousePosEvent`
    // (scaled by `content_scale`) now goes live. Target of the E9 CAPI
    // `rustty_inspector_mouse_pos`.
    pub fn cursorPosCallback(&mut self, x: f64, y: f64) {
        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };
        // cimgui.c.ImGuiIO_AddMousePosEvent(
        //     io,
        //     @floatCast(x * self.content_scale),
        //     @floatCast(y * self.content_scale),
        // );
        unsafe {
            dcimgui_sys::ImGuiIO_AddMousePosEvent(
                io,
                (x * self.content_scale) as f32,
                (y * self.content_scale) as f32,
            )
        };
    }

    // pub fn focusCallback(self: *Inspector, focused: bool) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddFocusEvent` now
    // goes live — this is the crash-on-open fix (`becomeFirstResponder` →
    // `setFocus` → this callback). Target of the E9 CAPI
    // `rustty_inspector_set_focus`.
    pub fn focusCallback(&mut self, focused: bool) {
        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };
        // cimgui.c.ImGuiIO_AddFocusEvent(io, focused);
        unsafe { dcimgui_sys::ImGuiIO_AddFocusEvent(io, focused) };
    }

    // pub fn textCallback(self: *Inspector, text: [:0]const u8) void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddInputCharactersUTF8`
    // now goes live. The `[:0]const u8` NUL-terminated slice → `&CStr` (`text.ptr`
    // is the load-bearing sentinel handed to imgui, matched here by `CStr::as_ptr`).
    // Target of the E9 CAPI `rustty_inspector_text`.
    pub fn textCallback(&mut self, text: &CStr) {
        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };
        // cimgui.c.ImGuiIO_AddInputCharactersUTF8(io, text.ptr);
        unsafe { dcimgui_sys::ImGuiIO_AddInputCharactersUTF8(io, text.as_ptr()) };
    }

    // pub fn keyCallback(self, action, key, mods) !void { ... }
    //
    // PORT NOTE (Phase-3 un-defer, INV-11): dcimgui `ImGuiIO_AddKeyEvent` (modifiers
    // + `key.imguiKey()`) now goes live. The `Key -> ImGuiKey` mapping is ported as
    // the free fn [`key_to_imgui_key`] just below this `impl` block rather than
    // filling in `rustty_vt::input::key::Key::imgui_key` — that method stays the
    // documented `#[cfg(feature = "imgui")]` `unimplemented!()` leaf (DEFERRALS.md
    // "Key::imgui_key — dcimgui (cimgui) binding") because rustty-vt must stay a
    // dcimgui-sys-free, apprt-agnostic crate (D1: rustty-vt is pure and below the
    // apprt/GUI edge); DEFERRALS.md itself names "Target: apprt layer" for this
    // exact leaf, which is here. Target of the E9 CAPI `rustty_inspector_key`.
    pub fn keyCallback(
        &mut self,
        action: key::Action,
        key: key::Key,
        mods: Mods,
    ) -> Result<(), ApprtError> {
        // self.queueRender();
        self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };

        // // Update all our modifiers
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftShift, mods.shift);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftCtrl, mods.ctrl);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftAlt, mods.alt);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftSuper, mods.super);
        unsafe {
            dcimgui_sys::ImGuiIO_AddKeyEvent(
                io,
                dcimgui_sys::ImGuiKey_LeftShift as c_int,
                mods.shift,
            );
            dcimgui_sys::ImGuiIO_AddKeyEvent(
                io,
                dcimgui_sys::ImGuiKey_LeftCtrl as c_int,
                mods.ctrl,
            );
            dcimgui_sys::ImGuiIO_AddKeyEvent(io, dcimgui_sys::ImGuiKey_LeftAlt as c_int, mods.alt);
            dcimgui_sys::ImGuiIO_AddKeyEvent(
                io,
                dcimgui_sys::ImGuiKey_LeftSuper as c_int,
                mods.super_,
            );
        }

        // // Send our keypress
        // if (key.imguiKey()) |imgui_key| {
        //     cimgui.c.ImGuiIO_AddKeyEvent(
        //         io,
        //         imgui_key,
        //         action == .press or action == .repeat,
        //     );
        // }
        if let Some(imgui_key) = key_to_imgui_key(key) {
            unsafe {
                dcimgui_sys::ImGuiIO_AddKeyEvent(
                    io,
                    imgui_key,
                    action == key::Action::Press || action == key::Action::Repeat,
                )
            };
        }

        Ok(())
    }

    // fn newFrame(self: *Inspector) !void { ... }
    //
    // PORT NOTE (Task 3): the imgui frame delta-time setup (`io.DeltaTime`), ported
    // LIVE — pure dcimgui + `std::time` (no objc). A private helper called by
    // [`Inspector::renderMetal`]'s double-frame loop.
    //
    // PORT NOTE: carries `renderMetal`'s Darwin gate (its sole
    // caller, per the note there); the reference has no non-Darwin build of the
    // Metal inspector path at all, so an unconditional definition would be dead
    // code on every other target.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn newFrame(&mut self) -> Result<(), ApprtError> {
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        let io = unsafe { dcimgui_sys::ImGui_GetIO() };

        // // Determine our delta time
        // const now = try std.time.Instant.now();
        //
        // PORT NOTE: the reference `std.time.Instant.now()` is `try` (it can error on
        // platforms lacking a monotonic clock); Rust's `std::time::Instant::now()` is
        // infallible, so the `try`/error path collapses and this fn only ever returns
        // `Ok`. The `Result` signature is kept to mirror the reference's `!void` (its
        // sole caller `renderMetal` does `try self.newFrame()`).
        let now = std::time::Instant::now();

        // io.DeltaTime = if (self.instant) |prev| delta: {
        //     const since_ns: f64 = @floatFromInt(now.since(prev));
        //     const ns_per_s: f64 = @floatFromInt(std.time.ns_per_s);
        //     const since_s: f32 = @floatCast(since_ns / ns_per_s);
        //     break :delta @max(0.00001, since_s);
        // } else (1.0 / 60.0);
        //
        // PORT NOTE: `now.since(prev)` yields nanoseconds elapsed since `prev`;
        // `saturating_duration_since` gives the same non-negative delta (0 if the
        // monotonic clock ever reports `prev` as later, matching the reference's
        // now>=prev assumption). `std.time.ns_per_s` == 1e9.
        let delta_time: f32 = if let Some(prev) = self.instant {
            let since_ns: f64 = now.saturating_duration_since(prev).as_nanos() as f64;
            let ns_per_s: f64 = 1_000_000_000.0;
            let since_s: f32 = (since_ns / ns_per_s) as f32;
            since_s.max(0.00001)
        } else {
            1.0 / 60.0
        };
        unsafe { (*io).DeltaTime = delta_time };

        // self.instant = now;
        self.instant = Some(now);

        Ok(())
    }
}

// PORTS: ghostty `src/input/key.zig`'s `Key.imguiKey` (called from
// `Inspector.keyCallback` above as `key.imguiKey()`).
//
// PORT NOTE: this is the apprt-layer landing spot DEFERRALS.md names for
// `Key::imgui_key` ("Target: apprt layer") — rustty-vt's own
// `input::key::Key::imgui_key` (gated behind the off-by-default `imgui` cargo
// feature) stays the documented `unimplemented!()` placeholder because rustty-vt
// must remain free of the dcimgui-sys (apprt GUI-glue) dependency (D1: rustty-vt
// is pure and sits below the apprt/GUI edge). `Inspector::keyCallback` is the one
// caller, and it lives in rustty-apprt, which already depends on dcimgui-sys — so
// the mapping is reproduced here instead, verbatim against the reference switch.
//
// Reference body (key.zig `imguiKey`), reproduced faithfully — every arm below is
// a direct transliteration; the final `=> null` arm's variant list is preserved
// 1:1 (including the reference's own stale-looking "We map KP_SEPARATOR to Comma"
// comment ahead of the `numpad_left`/`numpad_right`/... arms). Two arms differ:
// the reference maps BOTH `numpad_page_up` and `numpad_page_down` to
// `ImGuiKey_PageUp`; the port maps `numpad_page_down` to `ImGuiKey_PageDown`
// (reference defect, R13, UD27; PORT NOTE at the arm). And `numpad_begin`, which
// the reference maps to the range marker `ImGuiKey_NamedKey_BEGIN` (== Tab), is
// unmapped (UD27 (2)).
pub(super) fn key_to_imgui_key(key: key::Key) -> Option<c_int> {
    use key::Key;
    Some(match key {
        Key::KeyA => dcimgui_sys::ImGuiKey_A,
        Key::KeyB => dcimgui_sys::ImGuiKey_B,
        Key::KeyC => dcimgui_sys::ImGuiKey_C,
        Key::KeyD => dcimgui_sys::ImGuiKey_D,
        Key::KeyE => dcimgui_sys::ImGuiKey_E,
        Key::KeyF => dcimgui_sys::ImGuiKey_F,
        Key::KeyG => dcimgui_sys::ImGuiKey_G,
        Key::KeyH => dcimgui_sys::ImGuiKey_H,
        Key::KeyI => dcimgui_sys::ImGuiKey_I,
        Key::KeyJ => dcimgui_sys::ImGuiKey_J,
        Key::KeyK => dcimgui_sys::ImGuiKey_K,
        Key::KeyL => dcimgui_sys::ImGuiKey_L,
        Key::KeyM => dcimgui_sys::ImGuiKey_M,
        Key::KeyN => dcimgui_sys::ImGuiKey_N,
        Key::KeyO => dcimgui_sys::ImGuiKey_O,
        Key::KeyP => dcimgui_sys::ImGuiKey_P,
        Key::KeyQ => dcimgui_sys::ImGuiKey_Q,
        Key::KeyR => dcimgui_sys::ImGuiKey_R,
        Key::KeyS => dcimgui_sys::ImGuiKey_S,
        Key::KeyT => dcimgui_sys::ImGuiKey_T,
        Key::KeyU => dcimgui_sys::ImGuiKey_U,
        Key::KeyV => dcimgui_sys::ImGuiKey_V,
        Key::KeyW => dcimgui_sys::ImGuiKey_W,
        Key::KeyX => dcimgui_sys::ImGuiKey_X,
        Key::KeyY => dcimgui_sys::ImGuiKey_Y,
        Key::KeyZ => dcimgui_sys::ImGuiKey_Z,

        Key::Digit0 => dcimgui_sys::ImGuiKey_0,
        Key::Digit1 => dcimgui_sys::ImGuiKey_1,
        Key::Digit2 => dcimgui_sys::ImGuiKey_2,
        Key::Digit3 => dcimgui_sys::ImGuiKey_3,
        Key::Digit4 => dcimgui_sys::ImGuiKey_4,
        Key::Digit5 => dcimgui_sys::ImGuiKey_5,
        Key::Digit6 => dcimgui_sys::ImGuiKey_6,
        Key::Digit7 => dcimgui_sys::ImGuiKey_7,
        Key::Digit8 => dcimgui_sys::ImGuiKey_8,
        Key::Digit9 => dcimgui_sys::ImGuiKey_9,

        Key::Semicolon => dcimgui_sys::ImGuiKey_Semicolon,
        Key::Space => dcimgui_sys::ImGuiKey_Space,
        Key::Quote => dcimgui_sys::ImGuiKey_Apostrophe,
        Key::Comma => dcimgui_sys::ImGuiKey_Comma,
        Key::Backquote => dcimgui_sys::ImGuiKey_GraveAccent,
        Key::Period => dcimgui_sys::ImGuiKey_Period,
        Key::Slash => dcimgui_sys::ImGuiKey_Slash,
        Key::Minus => dcimgui_sys::ImGuiKey_Minus,
        Key::Equal => dcimgui_sys::ImGuiKey_Equal,
        Key::BracketLeft => dcimgui_sys::ImGuiKey_LeftBracket,
        Key::BracketRight => dcimgui_sys::ImGuiKey_RightBracket,
        Key::Backslash => dcimgui_sys::ImGuiKey_Backslash,

        Key::ArrowUp => dcimgui_sys::ImGuiKey_UpArrow,
        Key::ArrowDown => dcimgui_sys::ImGuiKey_DownArrow,
        Key::ArrowLeft => dcimgui_sys::ImGuiKey_LeftArrow,
        Key::ArrowRight => dcimgui_sys::ImGuiKey_RightArrow,
        Key::Home => dcimgui_sys::ImGuiKey_Home,
        Key::End => dcimgui_sys::ImGuiKey_End,
        Key::Insert => dcimgui_sys::ImGuiKey_Insert,
        Key::Delete => dcimgui_sys::ImGuiKey_Delete,
        Key::CapsLock => dcimgui_sys::ImGuiKey_CapsLock,
        Key::ScrollLock => dcimgui_sys::ImGuiKey_ScrollLock,
        Key::NumLock => dcimgui_sys::ImGuiKey_NumLock,
        Key::PageUp => dcimgui_sys::ImGuiKey_PageUp,
        Key::PageDown => dcimgui_sys::ImGuiKey_PageDown,
        Key::Escape => dcimgui_sys::ImGuiKey_Escape,
        Key::Enter => dcimgui_sys::ImGuiKey_Enter,
        Key::Tab => dcimgui_sys::ImGuiKey_Tab,
        Key::Backspace => dcimgui_sys::ImGuiKey_Backspace,
        Key::PrintScreen => dcimgui_sys::ImGuiKey_PrintScreen,
        Key::Pause => dcimgui_sys::ImGuiKey_Pause,
        Key::ContextMenu => dcimgui_sys::ImGuiKey_Menu,

        Key::F1 => dcimgui_sys::ImGuiKey_F1,
        Key::F2 => dcimgui_sys::ImGuiKey_F2,
        Key::F3 => dcimgui_sys::ImGuiKey_F3,
        Key::F4 => dcimgui_sys::ImGuiKey_F4,
        Key::F5 => dcimgui_sys::ImGuiKey_F5,
        Key::F6 => dcimgui_sys::ImGuiKey_F6,
        Key::F7 => dcimgui_sys::ImGuiKey_F7,
        Key::F8 => dcimgui_sys::ImGuiKey_F8,
        Key::F9 => dcimgui_sys::ImGuiKey_F9,
        Key::F10 => dcimgui_sys::ImGuiKey_F10,
        Key::F11 => dcimgui_sys::ImGuiKey_F11,
        Key::F12 => dcimgui_sys::ImGuiKey_F12,

        Key::Numpad0 => dcimgui_sys::ImGuiKey_Keypad0,
        Key::Numpad1 => dcimgui_sys::ImGuiKey_Keypad1,
        Key::Numpad2 => dcimgui_sys::ImGuiKey_Keypad2,
        Key::Numpad3 => dcimgui_sys::ImGuiKey_Keypad3,
        Key::Numpad4 => dcimgui_sys::ImGuiKey_Keypad4,
        Key::Numpad5 => dcimgui_sys::ImGuiKey_Keypad5,
        Key::Numpad6 => dcimgui_sys::ImGuiKey_Keypad6,
        Key::Numpad7 => dcimgui_sys::ImGuiKey_Keypad7,
        Key::Numpad8 => dcimgui_sys::ImGuiKey_Keypad8,
        Key::Numpad9 => dcimgui_sys::ImGuiKey_Keypad9,
        Key::NumpadDecimal => dcimgui_sys::ImGuiKey_KeypadDecimal,
        Key::NumpadDivide => dcimgui_sys::ImGuiKey_KeypadDivide,
        Key::NumpadMultiply => dcimgui_sys::ImGuiKey_KeypadMultiply,
        Key::NumpadSubtract => dcimgui_sys::ImGuiKey_KeypadSubtract,
        Key::NumpadAdd => dcimgui_sys::ImGuiKey_KeypadAdd,
        Key::NumpadEnter => dcimgui_sys::ImGuiKey_KeypadEnter,
        Key::NumpadEqual => dcimgui_sys::ImGuiKey_KeypadEqual,
        // We map KP_SEPARATOR to Comma because traditionally a numpad would
        // have a numeric separator key. Most modern numpads do not
        Key::NumpadLeft => dcimgui_sys::ImGuiKey_LeftArrow,
        Key::NumpadRight => dcimgui_sys::ImGuiKey_RightArrow,
        Key::NumpadUp => dcimgui_sys::ImGuiKey_UpArrow,
        Key::NumpadDown => dcimgui_sys::ImGuiKey_DownArrow,
        Key::NumpadPageUp => dcimgui_sys::ImGuiKey_PageUp,
        // PORT NOTE: reference defect (R13). The reference maps `.numpad_page_down`
        // to `ImGuiKey_PageUp` (key.zig:654), so keypad Page Down acts as Page Up in
        // the inspector. Mapped to Page Down, as `.page_down` is (same fix as
        // rustty-vt's `Key::imgui_key`). Test `numpad_page_down_maps_to_page_down`.
        // UPSTREAM-DEFECTS.md UD27.
        Key::NumpadPageDown => dcimgui_sys::ImGuiKey_PageDown,
        Key::NumpadHome => dcimgui_sys::ImGuiKey_Home,
        Key::NumpadEnd => dcimgui_sys::ImGuiKey_End,
        Key::NumpadInsert => dcimgui_sys::ImGuiKey_Insert,
        Key::NumpadDelete => dcimgui_sys::ImGuiKey_Delete,
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD27 (2). The reference
        // maps `.numpad_begin` to `ImGuiKey_NamedKey_BEGIN` (key.zig:659), the start-of-range
        // marker of imgui's named keys, which equals `ImGuiKey_Tab`: keypad 5 with NumLock
        // off moved inspector focus like Tab. imgui has no Begin key, so the port leaves it
        // unmapped, the reference's own answer (its `=> null` arm) for keys imgui lacks.
        // Test `numpad_begin_is_unmapped`.
        Key::NumpadBegin => return None,

        Key::ShiftLeft => dcimgui_sys::ImGuiKey_LeftShift,
        Key::ControlLeft => dcimgui_sys::ImGuiKey_LeftCtrl,
        Key::AltLeft => dcimgui_sys::ImGuiKey_LeftAlt,
        Key::MetaLeft => dcimgui_sys::ImGuiKey_LeftSuper,
        Key::ShiftRight => dcimgui_sys::ImGuiKey_RightShift,
        Key::ControlRight => dcimgui_sys::ImGuiKey_RightCtrl,
        Key::AltRight => dcimgui_sys::ImGuiKey_RightAlt,
        Key::MetaRight => dcimgui_sys::ImGuiKey_RightSuper,

        // These keys aren't represented in cimgui
        Key::F13
        | Key::F14
        | Key::F15
        | Key::F16
        | Key::F17
        | Key::F18
        | Key::F19
        | Key::F20
        | Key::F21
        | Key::F22
        | Key::F23
        | Key::F24
        | Key::F25
        | Key::IntlBackslash
        | Key::IntlRo
        | Key::IntlYen
        | Key::Convert
        | Key::KanaMode
        | Key::NonConvert
        | Key::NumpadSeparator
        | Key::NumpadBackspace
        | Key::NumpadClear
        | Key::NumpadClearEntry
        | Key::NumpadComma
        | Key::NumpadMemoryAdd
        | Key::NumpadMemoryClear
        | Key::NumpadMemoryRecall
        | Key::NumpadMemoryStore
        | Key::NumpadMemorySubtract
        | Key::NumpadParenLeft
        | Key::NumpadParenRight
        | Key::Fn
        | Key::FnLock
        | Key::BrowserBack
        | Key::BrowserFavorites
        | Key::BrowserForward
        | Key::BrowserHome
        | Key::BrowserRefresh
        | Key::BrowserSearch
        | Key::BrowserStop
        | Key::Eject
        | Key::LaunchApp1
        | Key::LaunchApp2
        | Key::LaunchMail
        | Key::MediaPlayPause
        | Key::MediaSelect
        | Key::MediaStop
        | Key::MediaTrackNext
        | Key::MediaTrackPrevious
        | Key::Power
        | Key::Sleep
        | Key::AudioVolumeDown
        | Key::AudioVolumeMute
        | Key::AudioVolumeUp
        | Key::WakeUp
        | Key::Help
        | Key::Copy
        | Key::Cut
        | Key::Paste
        | Key::Unidentified => return None,
    } as c_int)
}
