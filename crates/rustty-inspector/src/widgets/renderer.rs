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
// const cimgui = @import("dcimgui");                       (Task 10 — draw half)
// const widgets = @import("../widgets.zig");               (Task 10 — draw half)
// const renderer = @import("../../renderer.zig");
//
// R2 ACYCLIC EDGE (INV-7 / hazard R2, Task 5): the reference names the renderer's
// `renderer.Overlay.Feature` directly. The port names `rustty_renderer::overlay::
// Feature` via the `rustty-inspector -> rustty-renderer` dep edge (the renderer is
// the LOWER tier and never names an inspector type — acyclic; the `RendererConfig`
// precedent, memory: renderer-config-accessor-trait-acyclic). The renderer READS
// these features through the renderer-owned `InspectorOverlay` accessor trait
// (impl'd below), so no `renderer -> inspector` edge is created.
use rustty_renderer::overlay::Feature;
use rustty_renderer::state::InspectorOverlay;

// const log = std.log.scoped(.inspector_renderer);        (Task 10 — draw half)

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

    // Draw the renderer info window.  (Task 10 — ImGui draw half)
    //
    // pub fn draw(self: *Info, alloc: Allocator, open: bool) void {
    //     if (!open) return;
    //     cimgui.c.ImGui_SetNextItemOpen(true, cimgui.c.ImGuiCond_Once);
    //     if (!cimgui.c.ImGui_CollapsingHeader("Overlays", cimgui.c.ImGuiTreeNodeFlags_None)) return;
    //     cimgui.c.ImGui_SeparatorText("Hyperlinks");
    //     self.overlayHyperlinks(alloc);
    //     cimgui.c.ImGui_SeparatorText("Semantic Prompts");
    //     self.overlaySemanticPrompts(alloc);
    // }
    //
    // fn overlayHyperlinks(self: *Info, alloc: Allocator) void {
    //     var hyperlinks: bool = self.features.contains(.highlight_hyperlinks);
    //     _ = cimgui.c.ImGui_Checkbox("Overlay Hyperlinks", &hyperlinks);
    //     cimgui.c.ImGui_SameLine();
    //     widgets.helpMarker("When enabled, highlights OSC8 hyperlinks.");
    //     if (!hyperlinks) {
    //         _ = self.features.swapRemove(.highlight_hyperlinks);
    //     } else {
    //         self.features.put(
    //             alloc,
    //             .highlight_hyperlinks,
    //             .highlight_hyperlinks,
    //         ) catch log.warn("error enabling hyperlink overlay feature", .{});
    //     }
    // }
    //
    // fn overlaySemanticPrompts(self: *Info, alloc: Allocator) void {
    //     var semantic_prompts: bool = self.features.contains(.semantic_prompts);
    //     _ = cimgui.c.ImGui_Checkbox("Overlay Semantic Prompts", &semantic_prompts);
    //     cimgui.c.ImGui_SameLine();
    //     widgets.helpMarker("When enabled, highlights OSC 133 semantic prompts.");
    //     // Handle the checkbox results
    //     if (!semantic_prompts) {
    //         _ = self.features.swapRemove(.semantic_prompts);
    //     } else {
    //         self.features.put(
    //             alloc,
    //             .semantic_prompts,
    //             .semantic_prompts,
    //         ) catch log.warn("error enabling semantic prompt overlay feature", .{});
    //     }
    //     // Help
    //     cimgui.c.ImGui_Indent();
    //     defer cimgui.c.ImGui_Unindent();
    //     cimgui.c.ImGui_TextDisabled("Colors:");
    //     const prompt_rgb = renderer.Overlay.Color.semantic_prompt.rgb();
    //     const input_rgb = renderer.Overlay.Color.semantic_input.rgb();
    //     const prompt_col: cimgui.c.ImVec4 = .{ .x = ...r/255, .y = ...g/255, .z = ...b/255, .w = 1.0 };
    //     const input_col:  cimgui.c.ImVec4 = .{ .x = ...r/255, .y = ...g/255, .z = ...b/255, .w = 1.0 };
    //     _ = cimgui.c.ImGui_ColorButton("##prompt_color", prompt_col, cimgui.c.ImGuiColorEditFlags_NoTooltip);
    //     cimgui.c.ImGui_SameLine();
    //     cimgui.c.ImGui_Text("Prompt");
    //     _ = cimgui.c.ImGui_ColorButton("##input_color", input_col, cimgui.c.ImGuiColorEditFlags_NoTooltip);
    //     cimgui.c.ImGui_SameLine();
    //     cimgui.c.ImGui_Text("Input");
    // }
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
