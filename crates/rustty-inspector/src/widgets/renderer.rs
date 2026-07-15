//! Renderer information inspector widget.
//!
//! PORTS: ghostty src/inspector/widgets/renderer.zig
//!
//! PORT STATUS: partial — the R2 acyclic accessor-trait half (Task 5) is LIVE:
//! the `Info` struct (`features` set, INV-7), `empty`/`deinit`, and
//! `overlayFeatures` (ported as the renderer-owned [`InspectorOverlay`] trait
//! impl). The ImGui `draw` half (`draw`/`overlayHyperlinks`/
//! `overlaySemanticPrompts`, the checkboxes that WRITE `Info.features`) is Task
//! 10 — carried below as verbatim reference bodies.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Global as Allocator;
// const cimgui = @import("dcimgui");
// const widgets = @import("../widgets.zig");
//
// PORT NOTE (Task 10): `cimgui.c.*` crosses as the raw `dcimgui_sys` FFI; the
// reference `widgets.helpMarker` is the parent module's `super::helpMarker`.
// const renderer = @import("../../renderer.zig");
//
// R2 ACYCLIC EDGE (INV-7 / hazard R2, Task 5): the reference names the renderer's
// `renderer.Overlay.Feature` directly. The port names `rustty_renderer::overlay::
// Feature` via the `rustty-inspector -> rustty-renderer` dep edge (the renderer is
// the LOWER tier and never names an inspector type — acyclic; the `RendererConfig`
// precedent, memory: renderer-config-accessor-trait-acyclic). The renderer READS
// these features through the renderer-owned `InspectorOverlay` accessor trait
// (impl'd below), so no `renderer -> inspector` edge is created.
use rustty_renderer::overlay::{Color, Feature};
use rustty_renderer::state::InspectorOverlay;

// const log = std.log.scoped(.inspector_renderer);
//
// PORT NOTE (Task 10): the reference's two `log.warn("error enabling …")` sites
// guard the `features.put(...) catch` OOM path. In the port `Vec::push` cannot
// fail recoverably (OOM aborts under panic=abort), so those `catch log.warn`
// arms are structurally unreachable and preserved as comments at their sites; no
// logging facade is wired into the inspector crate yet.

/// Renderer information inspector widget.
//
// PORT NOTE: the reference `features` is
// `AutoArrayHashMapUnmanaged(std.meta.Tag(renderer.Overlay.Feature),
// renderer.Overlay.Feature)` — an INSERTION-ORDERED unique map keyed by the
// feature tag. `Feature` carries no payload, so `Tag(Feature) == Feature` and the
// map degenerates to an insertion-ordered SET of `Feature`; the port models it as
// a `Vec<Feature>` maintaining insertion order + uniqueness. `values()` (used by
// `overlayFeatures`) is the `Vec` in order; the Task-10 draw half will add the
// `contains`/`put`/`swapRemove` set ops the checkboxes drive.
pub struct Info {
    // features: std.AutoArrayHashMapUnmanaged(
    //     std.meta.Tag(renderer.Overlay.Feature),
    //     renderer.Overlay.Feature,
    // ),
    pub features: Vec<Feature>,
}

impl Info {
    // pub const empty: Info = .{
    //     .features = .empty,
    // };
    pub const fn empty() -> Info {
        Info {
            features: Vec::new(),
        }
    }

    // pub fn deinit(self: *Info, alloc: Allocator) void {
    //     self.features.deinit(alloc);
    // }
    //
    // PORT NOTE: the owned `Vec` frees itself on drop; `alloc` is carried for
    // parity (the reference's unmanaged map takes the allocator to free).
    pub fn deinit(&mut self, _alloc: Allocator) {
        self.features.clear();
    }

    // PORT NOTE (Task 10, INV-7): the reference's `features` is an insertion-
    // ordered unique map keyed by `Tag(Feature)`; the port models it as the
    // `Vec<Feature>` above (`Feature` is payloadless, so `Tag(Feature) ==
    // Feature` and the map degenerates to an ordered set — see the struct
    // PORT NOTE). These two private helpers realize the `contains`/`swapRemove`/
    // `put` map ops the checkboxes drive, faithful to the reference's semantics:
    //   • `features_contains` == `self.features.contains(key)`.
    //   • `features_swap_remove` == `self.features.swapRemove(key)`: remove the
    //     entry, moving the LAST element into the freed slot (`Vec::swap_remove`
    //     matches Zig's `swapRemove` order-perturbing removal exactly). The
    //     reference discards the returned optional (`_ = ...`); so does the port.
    //   • `features_put` == `self.features.put(alloc, key, value)`: insert if
    //     absent (values equal keys here, so a present key is a no-op update),
    //     preserving insertion order. `Vec::push` cannot fail recoverably, so
    //     the reference's `catch log.warn` OOM arm is unreachable (see the
    //     module-header PORT NOTE).

    /// `self.features.contains(f)` on the ordered-set `Vec` model.
    fn features_contains(&self, f: Feature) -> bool {
        self.features.contains(&f)
    }

    /// `self.features.swapRemove(f)` on the ordered-set `Vec` model.
    fn features_swap_remove(&mut self, f: Feature) {
        if let Some(i) = self.features.iter().position(|&x| x == f) {
            self.features.swap_remove(i);
        }
    }

    /// `self.features.put(alloc, f, f)` on the ordered-set `Vec` model.
    fn features_put(&mut self, f: Feature) {
        if !self.features.contains(&f) {
            self.features.push(f);
        }
    }

    /// Draw the renderer info window.
    //
    // pub fn draw(
    //     self: *Info,
    //     alloc: Allocator,
    //     open: bool,
    // ) void {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn draw(&mut self, alloc: Allocator, open: bool) {
        // if (!open) return;
        if !open {
            return;
        }

        // cimgui.c.ImGui_SetNextItemOpen(true, cimgui.c.ImGuiCond_Once);
        unsafe {
            dcimgui_sys::ImGui_SetNextItemOpen(
                true,
                dcimgui_sys::ImGuiCond__ImGuiCond_Once as dcimgui_sys::ImGuiCond,
            )
        };
        // if (!cimgui.c.ImGui_CollapsingHeader("Overlays", cimgui.c.ImGuiTreeNodeFlags_None)) return;
        if !unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Overlays".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_None
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            return;
        }

        // cimgui.c.ImGui_SeparatorText("Hyperlinks");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Hyperlinks".as_ptr()) };
        // self.overlayHyperlinks(alloc);
        self.overlayHyperlinks(alloc);
        // cimgui.c.ImGui_SeparatorText("Semantic Prompts");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Semantic Prompts".as_ptr()) };
        // self.overlaySemanticPrompts(alloc);
        self.overlaySemanticPrompts(alloc);
    }

    // fn overlayHyperlinks(self: *Info, alloc: Allocator) void {
    //
    // PORT NOTE: `alloc` is carried for reference parity (the map's `put` takes
    // it); the `Vec` model needs no allocator, so it is unused here.
    #[allow(non_snake_case)] // faithful Zig name (R1)
    fn overlayHyperlinks(&mut self, _alloc: Allocator) {
        // var hyperlinks: bool = self.features.contains(.highlight_hyperlinks);
        let mut hyperlinks: bool = self.features_contains(Feature::HighlightHyperlinks);
        // _ = cimgui.c.ImGui_Checkbox("Overlay Hyperlinks", &hyperlinks);
        unsafe { dcimgui_sys::ImGui_Checkbox(c"Overlay Hyperlinks".as_ptr(), &mut hyperlinks) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("When enabled, highlights OSC8 hyperlinks.");
        super::helpMarker(c"When enabled, highlights OSC8 hyperlinks.");

        // if (!hyperlinks) {
        if !hyperlinks {
            // _ = self.features.swapRemove(.highlight_hyperlinks);
            self.features_swap_remove(Feature::HighlightHyperlinks);
        } else {
            // self.features.put(
            //     alloc,
            //     .highlight_hyperlinks,
            //     .highlight_hyperlinks,
            // ) catch log.warn("error enabling hyperlink overlay feature", .{});
            self.features_put(Feature::HighlightHyperlinks);
        }
    }

    // fn overlaySemanticPrompts(self: *Info, alloc: Allocator) void {
    //
    // PORT NOTE: `alloc` carried for parity (see `overlayHyperlinks`).
    #[allow(non_snake_case)] // faithful Zig name (R1)
    fn overlaySemanticPrompts(&mut self, _alloc: Allocator) {
        // var semantic_prompts: bool = self.features.contains(.semantic_prompts);
        let mut semantic_prompts: bool = self.features_contains(Feature::SemanticPrompts);
        // _ = cimgui.c.ImGui_Checkbox("Overlay Semantic Prompts", &semantic_prompts);
        unsafe {
            dcimgui_sys::ImGui_Checkbox(c"Overlay Semantic Prompts".as_ptr(), &mut semantic_prompts)
        };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("When enabled, highlights OSC 133 semantic prompts.");
        super::helpMarker(c"When enabled, highlights OSC 133 semantic prompts.");

        // Handle the checkbox results
        // if (!semantic_prompts) {
        if !semantic_prompts {
            // _ = self.features.swapRemove(.semantic_prompts);
            self.features_swap_remove(Feature::SemanticPrompts);
        } else {
            // self.features.put(
            //     alloc,
            //     .semantic_prompts,
            //     .semantic_prompts,
            // ) catch log.warn("error enabling semantic prompt overlay feature", .{});
            self.features_put(Feature::SemanticPrompts);
        }

        // Help
        // cimgui.c.ImGui_Indent();
        unsafe { dcimgui_sys::ImGui_Indent() };
        // defer cimgui.c.ImGui_Unindent();
        let _unindent = super::Defer(|| unsafe { dcimgui_sys::ImGui_Unindent() });

        // cimgui.c.ImGui_TextDisabled("Colors:");
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"Colors:".as_ptr()) };

        // const prompt_rgb = renderer.Overlay.Color.semantic_prompt.rgb();
        let prompt_rgb = Color::SemanticPrompt.rgb();
        // const input_rgb = renderer.Overlay.Color.semantic_input.rgb();
        let input_rgb = Color::SemanticInput.rgb();
        // const prompt_col: cimgui.c.ImVec4 = .{
        //     .x = @as(f32, @floatFromInt(prompt_rgb.r)) / 255.0,
        //     .y = @as(f32, @floatFromInt(prompt_rgb.g)) / 255.0,
        //     .z = @as(f32, @floatFromInt(prompt_rgb.b)) / 255.0,
        //     .w = 1.0,
        // };
        let prompt_col: dcimgui_sys::ImVec4 = dcimgui_sys::ImVec4 {
            x: f32::from(prompt_rgb.r) / 255.0,
            y: f32::from(prompt_rgb.g) / 255.0,
            z: f32::from(prompt_rgb.b) / 255.0,
            w: 1.0,
        };
        // const input_col: cimgui.c.ImVec4 = .{
        //     .x = @as(f32, @floatFromInt(input_rgb.r)) / 255.0,
        //     .y = @as(f32, @floatFromInt(input_rgb.g)) / 255.0,
        //     .z = @as(f32, @floatFromInt(input_rgb.b)) / 255.0,
        //     .w = 1.0,
        // };
        let input_col: dcimgui_sys::ImVec4 = dcimgui_sys::ImVec4 {
            x: f32::from(input_rgb.r) / 255.0,
            y: f32::from(input_rgb.g) / 255.0,
            z: f32::from(input_rgb.b) / 255.0,
            w: 1.0,
        };

        // _ = cimgui.c.ImGui_ColorButton("##prompt_color", prompt_col, cimgui.c.ImGuiColorEditFlags_NoTooltip);
        unsafe {
            dcimgui_sys::ImGui_ColorButton(
                c"##prompt_color".as_ptr(),
                prompt_col,
                dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoTooltip
                    as dcimgui_sys::ImGuiColorEditFlags,
            )
        };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_Text("Prompt");
        unsafe { dcimgui_sys::ImGui_Text(c"Prompt".as_ptr()) };

        // _ = cimgui.c.ImGui_ColorButton("##input_color", input_col, cimgui.c.ImGuiColorEditFlags_NoTooltip);
        unsafe {
            dcimgui_sys::ImGui_ColorButton(
                c"##input_color".as_ptr(),
                input_col,
                dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoTooltip
                    as dcimgui_sys::ImGuiColorEditFlags,
            )
        };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_Text("Input");
        unsafe { dcimgui_sys::ImGui_Text(c"Input".as_ptr()) };
    }
}

/// Grab the features into a new allocated slice. This is used by
//
// the renderer to read the enabled overlay features (INV-7). Ported as the
// renderer-owned [`InspectorOverlay`] accessor trait (hazard R2) — the renderer
// calls this through the trait object stored in `state.inspector`, never naming
// this `Info` type.
//
// pub fn overlayFeatures(
//     self: *const Info,
//     alloc: Allocator,
// ) Allocator.Error![]renderer.Overlay.Feature {
impl InspectorOverlay for Info {
    fn overlay_features(&self) -> Vec<Feature> {
        // The features from our internal state.
        // const features = self.features.values();
        //
        // For now we do a dumb copy since the features have no managed
        // memory.
        // const result = try alloc.dupe(
        //     renderer.Overlay.Feature,
        //     features,
        // );
        // errdefer alloc.free(result);
        // return result;
        //
        // PORT NOTE: the reference dupes into an arena-owned slice returned to the
        // renderer; the port returns a plain owned `Vec` (Global alloc — the
        // reference's `Allocator.Error`/`catch &.{}` OOM path is subsumed by
        // panic=abort). `Feature: Copy`, so the clone is the "dumb copy".
        self.features.clone()
    }
}
