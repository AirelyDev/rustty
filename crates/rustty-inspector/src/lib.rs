//! rustty-inspector
//!
//! PORTS: ghostty src/inspector/ (Inspector, main, units, widgets, widgets/*); links dcimgui-sys;
//! core via refs/trait (breaks #11, #12). Fallback A6: may merge with renderer+termio.
//!
//! PORT STATUS: partial — core-tier chunk C1 (CORE_BLUEPRINT §4 / row C1)
//! carved the minimal surface core's input hot path needs: `widgets::key`'s
//! pure-data `Event` + `EventRing` + `label()` (partial-file, [ADV-FIX 7]),
//! this file's `KeyEvent` re-export, and the `Inspector` placeholder. Task 4
//! (inspector port plan) now INHABITS `Inspector` (was `pub enum Inspector {}`)
//! and ports its shared-IO setup (`Inspector.setup()`) LIVE over dcimgui-sys.
//! The `gui` aggregate (`widgets.surface.Inspector` owning
//! key_stream/vt_stream/renderer_info) and the record/render methods are the
//! later widget tasks (surface.rs T15, renderer.rs T10, termio.rs T16, the
//! top-level draw + R4 accessor-trait T17/T6).

// This file ports ghostty src/inspector/main.zig:

// pub const widgets = @import("widgets.zig");
pub mod widgets;

// pub const Inspector = @import("Inspector.zig");
//
// PORT NOTE (Task 4 — INHABITED PLACEHOLDER, placeholder-leaf doctrine):
// Inspector.zig's `Inspector` is a value aggregate:
//
//     mouse: widgets.surface.Mouse = .{},   // extra mouse state we track
//     gui:   widgets.surface.Inspector,     // the whole ImGui widget tree
//
// whose `gui` owns `key_stream` / `vt_stream` / `renderer_info` (surface.zig:
// 21-28). Both field types live in `widgets/surface.zig`, which is the Phase-3
// widget port (Task 15); and the record/render/rendererInfo methods reference
// core `Surface`/`Terminal` and `widgets.renderer.Info`, which rustty-inspector
// must reach only through the R4 accessor-trait (a direct edge to rustty-core
// would CYCLE: core already holds `Option<*mut inspectorpkg::Inspector>`). So
// until those tasks land, the aggregate is carried as an INHABITED (was:
// uninhabited `pub enum`) placeholder that GROWS its fields in Task 15/17. It is
// now constructible, so core's `activateInspector` (Task 6) can heap-allocate
// it; the single `renderer_state.inspector = ...` assignment still casts this
// pointee to `*mut rustty_renderer::state::Inspector` (a documented
// opaque-pointer cast; the edge points inspector -> renderer, never the
// reverse).
//
// The one method that is fully unblocked today — `setup()`, a *static* that
// mutates the process-global ImGui IO (INV-11) and needs only dcimgui-sys +
// the embedded font — is ported LIVE below. `init`/`deinit`/`rendererInfo`/
// `recordKeyEvent`/`recordPtyRead`/`render` are preserved as verbatim reference
// bodies (they touch the deferred `gui` aggregate / the R4 trait):
//
//     pub fn init(alloc: Allocator) !Inspector {
//         var gui: widgets.surface.Inspector = try .init(alloc);
//         errdefer gui.deinit(alloc);
//         return .{ .gui = gui };
//     }
//     pub fn deinit(self: *Inspector, alloc: Allocator) void {
//         self.gui.deinit(alloc);
//     }
//     pub fn rendererInfo(self: *Inspector) *widgets.renderer.Info {
//         return &self.gui.renderer_info;
//     }
//     pub fn recordKeyEvent(self, alloc, ev: inspector.KeyEvent) !void { ... }   // Inspector.zig:75-98
//     pub fn recordPtyRead(self, alloc, t: *terminal.Terminal, data: []const u8) !void {
//         try self.gui.vt_stream.recordPtyRead(alloc, t, data);
//     }
//     pub fn render(self: *Inspector, surface: *Surface) void {
//         self.gui.draw(surface, self.mouse);
//         surface.renderer_thread.wakeup.notify() catch {};   // INV-12
//     }
pub struct Inspector {
    // mouse: widgets.surface.Mouse = .{},      (Task 15 — widgets/surface.zig)
    // gui:   widgets.surface.Inspector,        (Task 15 — widgets/surface.zig)
    //
    // The aggregate fields land with the widget submodule ports (Task 15/17);
    // an inhabited zero-field placeholder here keeps the type constructible and
    // the crate R4-clean in the interim (the doctrine's "grows in place").
    _incomplete: (),
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
}

// pub const KeyEvent = widgets.key.Event;
pub use widgets::key::Event as KeyEvent;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    // PORT NOTE: the reference test reifies every public decl; ported as the
    // refAllDecls analogue over the crate's carved public surface (termio
    // lib.rs precedent). Grows as Phase 3 fills the crate in.
    #[test]
    fn ref_all_decls() {
        let _ = std::any::type_name::<crate::Inspector>();
        let _ = std::any::type_name::<crate::KeyEvent>();
        let _ = std::any::type_name::<crate::widgets::key::Event>();
        let _ = std::any::type_name::<crate::widgets::key::EventRing>();
        let _ = std::any::type_name::<crate::widgets::key::ImguiState>();
        let _ = std::any::type_name::<crate::widgets::key::Error>();
    }
}
