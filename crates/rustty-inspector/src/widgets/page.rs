//! Page-information inspector widget.
//!
//! PORTS: ghostty src/inspector/widgets/page.zig
//!
//! PORT STATUS: fully LIVE (Task 12). Every function is a pure display-only
//! read of a `terminal.Page` obtained per-frame under the surface's
//! `renderer_state.mutex` (INV-1) — the `*const Page` ports as a call-scoped
//! `&Page` (the screen.rs `*const Screen` → `&Screen` precedent, LIFETIMES §1
//! BORROW_PARAM). No live mutation happens here.
//!
//! INV-2 (dirty tracking is READ-ONLY): `treeNode` reads `page.isDirty()`
//! (page.zig:79) and `rowsTable` reads `row.dirty` (page.zig:263) purely to
//! render a "dirty" label / flag cell — the widget OBSERVES the dirty state and
//! NEVER clears it, so the renderer's dirty-clear contract (D5) is untouched.
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11).

// const std = @import("std");
// const cimgui = @import("dcimgui");
// const terminal = @import("../../terminal/main.zig");
// const units = @import("../units.zig");
// const widgets = @import("../widgets.zig");
//
// const PageList = terminal.PageList;
// const Page = terminal.Page;
//
// PORT NOTE (screen.rs/key.rs precedent): `dcimgui` is the vendored Dear ImGui
// C API, reached through `dcimgui-sys` (full `dcimgui_sys::` paths). The
// `terminal.Page`/`.Row`/`.Cell`/`.Row.SemanticPrompt` types live in
// rustty-vt's `page` module; the hyperlink `PageEntryId` (for the explicit /
// implicit ID switch) in `hyperlink`. `page.memory` (a `pub []u8` slice in the
// reference) is projected across the crate boundary by the page's public
// `memory_ptr()` (base) + `memory_len()` (length) accessors. `widgets.style` /
// `widgets.helpMarker` are the parent module's Task 7/9 helpers;
// `units.toKibiBytes` is the crate leaf. The reference's top-level `const
// PageList` alias is unused by this file, so it is carried only as a comment.
use core::ffi::{c_char, c_int, CStr};

use rustty_vt::hyperlink::PageEntryIdTag;
use rustty_vt::page::{Cell, Page, Row, SemanticPrompt};

use super::helpMarker;
use crate::units;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`screen.rs`). Backs the ImGui `BeginTable`/`EndTable`,
/// `BeginChild`/`EndChild`, and `PushID`/`PopID` / `TreeNodeEx`/`TreePop`
/// pairings below so they fire on every exit path. panic=abort means no
/// unwinding ever crosses these FFI calls, so drop-on-scope-exit is the sole
/// runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// `std.fmt.bufPrintZ` analogue backing `treeNode`'s label. Writes
/// `"Page {index}"` NUL-terminated into `buf`, returning the `*const c_char`;
/// on overflow it falls back to the literal `c"Page"` (the reference's `catch
/// "Page"`). The returned pointer aliases `buf`, so `buf` must outlive its use.
fn page_label(buf: &mut [u8; 160], index: usize) -> *const c_char {
    use core::fmt::Write as _;
    struct W<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl core::fmt::Write for W<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let b = s.as_bytes();
            let end = self.pos.checked_add(b.len()).ok_or(core::fmt::Error)?;
            if end > self.buf.len() {
                return Err(core::fmt::Error);
            }
            self.buf[self.pos..end].copy_from_slice(b);
            self.pos = end;
            Ok(())
        }
    }
    // Reserve the final byte for the NUL terminator (bufPrintZ semantics).
    let cap = buf.len() - 1;
    let pos = {
        let mut w = W {
            buf: &mut buf[..cap],
            pos: 0,
        };
        if write!(w, "Page {}", index).is_err() {
            return c"Page".as_ptr();
        }
        w.pos
    };
    buf[pos] = 0;
    buf.as_ptr() as *const c_char
}

// pub fn inspector(page: *const terminal.Page) void {
//
// PORT NOTE (INV-1): `*const terminal.Page` is a call-scoped shared borrow
// obtained under the surface mutex; ports as `&Page`.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn inspector(page: &Page) {
    // cimgui.c.ImGui_SeparatorText("Managed Memory");
    unsafe { dcimgui_sys::ImGui_SeparatorText(c"Managed Memory".as_ptr()) };
    // managedMemory(page);
    managedMemory(page);

    // cimgui.c.ImGui_SeparatorText("Styles");
    unsafe { dcimgui_sys::ImGui_SeparatorText(c"Styles".as_ptr()) };
    // stylesList(page);
    stylesList(page);

    // cimgui.c.ImGui_SeparatorText("Hyperlinks");
    unsafe { dcimgui_sys::ImGui_SeparatorText(c"Hyperlinks".as_ptr()) };
    // hyperlinksList(page);
    hyperlinksList(page);

    // cimgui.c.ImGui_SeparatorText("Rows");
    unsafe { dcimgui_sys::ImGui_SeparatorText(c"Rows".as_ptr()) };
    // rowsTable(page);
    rowsTable(page);
}

/// Draw a tree node header with metadata about this page. Returns if
/// the tree node is open or not. If it is open you must close it with
/// TreePop.
//
// pub fn treeNode(state: struct {
//     /// Page dimensions available without reading its backing memory.
//     cols: terminal.size.CellCountInt,
//     rows: terminal.size.CellCountInt,
//     /// The index of the page in a page list, used for headers.
//     index: usize,
//     /// The range of rows this page covers, inclusive.
//     row_range: [2]usize,
//     /// Whether this page is the active or viewport node.
//     active: bool,
//     viewport: bool,
//     /// Whether the page backing memory is currently compressed.
//     compressed: bool,
//     /// Dirty state is unavailable without restoring a compressed page.
//     dirty: ?bool,
// }) bool {
//
// PORT NOTE (screen.rs `DrawData` precedent): the anonymous `state` params
// struct becomes a named `TreeNodeState`.
//
// PORT NOTE (WS7): the state no longer carries the `Page` itself. Every field
// here is metadata a node can supply while its backing memory is COMPRESSED
// (`Node::cols`/`rows`/`storage`), so the header can be drawn without
// restoring the page. `dirty` is `Option` precisely because it is the one
// datum that needs the mapping.
pub struct TreeNodeState {
    /// Page dimensions available without reading its backing memory.
    pub cols: rustty_vt::size::CellCountInt,
    pub rows: rustty_vt::size::CellCountInt,
    /// The index of the page in a page list, used for headers.
    pub index: usize,
    /// The range of rows this page covers, inclusive.
    pub row_range: [usize; 2],
    /// Whether this page is the active or viewport node.
    pub active: bool,
    pub viewport: bool,
    /// Whether the page backing memory is currently compressed.
    pub compressed: bool,
    /// Dirty state is unavailable without restoring a compressed page.
    pub dirty: Option<bool>,
}

#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn treeNode(state: TreeNodeState) -> bool {
    // Setup our node.
    // const open = open: {
    //     var label_buf: [160]u8 = undefined;
    //     const label = std.fmt.bufPrintZ(
    //         &label_buf,
    //         "Page {d}",
    //         .{state.index},
    //     ) catch "Page";
    //
    //     const flags = cimgui.c.ImGuiTreeNodeFlags_AllowOverlap |
    //         cimgui.c.ImGuiTreeNodeFlags_SpanFullWidth |
    //         cimgui.c.ImGuiTreeNodeFlags_FramePadding;
    //     break :open cimgui.c.ImGui_TreeNodeEx(label.ptr, flags);
    // };
    let open = {
        let mut label_buf: [u8; 160] = [0; 160];
        let label = page_label(&mut label_buf, state.index);

        let flags = (dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_AllowOverlap
            as dcimgui_sys::ImGuiTreeNodeFlags)
            | (dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_SpanFullWidth
                as dcimgui_sys::ImGuiTreeNodeFlags)
            | (dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_FramePadding
                as dcimgui_sys::ImGuiTreeNodeFlags);
        unsafe { dcimgui_sys::ImGui_TreeNodeEx(label, flags) }
    };

    // Move our cursor into the tree header so we can add extra info.
    // const header_min = cimgui.c.ImGui_GetItemRectMin();
    // const header_max = cimgui.c.ImGui_GetItemRectMax();
    // const header_height = header_max.y - header_min.y;
    // const text_line = cimgui.c.ImGui_GetTextLineHeight();
    // const y_center = header_min.y + (header_height - text_line) * 0.5;
    // cimgui.c.ImGui_SetCursorScreenPos(.{ .x = header_min.x + 170, .y = y_center });
    let header_min = unsafe { dcimgui_sys::ImGui_GetItemRectMin() };
    let header_max = unsafe { dcimgui_sys::ImGui_GetItemRectMax() };
    let header_height = header_max.y - header_min.y;
    let text_line = unsafe { dcimgui_sys::ImGui_GetTextLineHeight() };
    let y_center = header_min.y + (header_height - text_line) * 0.5;
    unsafe {
        dcimgui_sys::ImGui_SetCursorScreenPos(dcimgui_sys::ImVec2 {
            x: header_min.x + 170.0,
            y: y_center,
        })
    };

    // Metadata
    // cimgui.c.ImGui_TextDisabled(
    //     "%dc x %dr",
    //     state.cols,
    //     state.rows,
    // );
    unsafe {
        dcimgui_sys::ImGui_TextDisabled(
            c"%dc x %dr".as_ptr(),
            state.cols as c_int,
            state.rows as c_int,
        )
    };
    // cimgui.c.ImGui_SameLine();
    unsafe { dcimgui_sys::ImGui_SameLine() };
    // cimgui.c.ImGui_Text("rows %d..%d", state.row_range[0], state.row_range[1]);
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"rows %d..%d".as_ptr(),
            state.row_range[0] as c_int,
            state.row_range[1] as c_int,
        )
    };

    // Labels
    // if (state.active) {
    if state.active {
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.9, .z = 0.4, .w = 1.0 }, "active");
        unsafe {
            dcimgui_sys::ImGui_TextColoredPtr(
                &dcimgui_sys::ImVec4 {
                    x: 0.4,
                    y: 0.9,
                    z: 0.4,
                    w: 1.0,
                },
                c"active".as_ptr(),
            )
        };
    }
    // if (state.viewport) {
    if state.viewport {
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.8, .z = 1.0, .w = 1.0 }, "viewport");
        unsafe {
            dcimgui_sys::ImGui_TextColoredPtr(
                &dcimgui_sys::ImVec4 {
                    x: 0.4,
                    y: 0.8,
                    z: 1.0,
                    w: 1.0,
                },
                c"viewport".as_ptr(),
            )
        };
    }
    // if (state.compressed) {
    if state.compressed {
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_TextColored(.{ .x = 0.8, .y = 0.6, .z = 1.0, .w = 1.0 }, "compressed");
        unsafe {
            dcimgui_sys::ImGui_TextColoredPtr(
                &dcimgui_sys::ImVec4 {
                    x: 0.8,
                    y: 0.6,
                    z: 1.0,
                    w: 1.0,
                },
                c"compressed".as_ptr(),
            )
        };
    }
    // if (state.dirty orelse false) {
    //
    // PORT NOTE (INV-2): the caller's `page.isDirty()` is a pure observation of
    // the page's dirty state — read to render the "dirty" label, NEVER cleared.
    // It is `None` when the page is compressed, since observing it would mean
    // restoring the mapping.
    if state.dirty.unwrap_or(false) {
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.4, .z = 0.4, .w = 1.0 }, "dirty");
        unsafe {
            dcimgui_sys::ImGui_TextColoredPtr(
                &dcimgui_sys::ImVec4 {
                    x: 1.0,
                    y: 0.4,
                    z: 0.4,
                    w: 1.0,
                },
                c"dirty".as_ptr(),
            )
        };
    }

    // return open;
    open
}

// pub fn managedMemory(page: *const Page) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn managedMemory(page: &Page) {
    // if (cimgui.c.ImGui_BeginTable(
    //     "##overview",
    //     3,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) {
    if unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##overview".as_ptr(),
            3,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_BordersInnerV
                as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                    as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                    as dcimgui_sys::ImGuiTableFlags),
        )
    } {
        // defer cimgui.c.ImGui_EndTable();
        let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Memory Size");
        unsafe { dcimgui_sys::ImGui_Text(c"Memory Size".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker(
        //     "Memory allocated for this page. Note the backing memory " ++
        //         "may be a larger allocation from which this page " ++
        //         "uses a portion.",
        // );
        helpMarker(
            c"Memory allocated for this page. Note the backing memory may be a larger allocation from which this page uses a portion.",
        );
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text(
        //     "%d KiB",
        //     units.toKibiBytes(page.memory.len),
        // );
        unsafe {
            dcimgui_sys::ImGui_Text(
                c"%d KiB".as_ptr(),
                units::toKibiBytes(page.memory_len()) as c_int,
            )
        };
    }

    // if (cimgui.c.ImGui_BeginTable(
    //     "##managed",
    //     4,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) {
    if unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##managed".as_ptr(),
            4,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_BordersInnerV
                as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                    as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                    as dcimgui_sys::ImGuiTableFlags),
        )
    } {
        // defer cimgui.c.ImGui_EndTable();
        let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

        // cimgui.c.ImGui_TableSetupColumn("Resource", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"Resource".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableSetupColumn("", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableSetupColumn("Used", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"Used".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableSetupColumn("Capacity", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"Capacity".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableHeadersRow();
        unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

        // const size = page.size;
        // const cap = page.capacity;
        let size = page.size;
        let cap = page.capacity;
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Columns");
        unsafe { dcimgui_sys::ImGui_Text(c"Columns".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Number of columns in the terminal grid.");
        helpMarker(c"Number of columns in the terminal grid.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", size.cols);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), size.cols as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", cap.cols);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), cap.cols as c_int) };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Rows");
        unsafe { dcimgui_sys::ImGui_Text(c"Rows".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Number of rows in this page.");
        helpMarker(c"Number of rows in this page.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", size.rows);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), size.rows as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", cap.rows);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), cap.rows as c_int) };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Styles");
        unsafe { dcimgui_sys::ImGui_Text(c"Styles".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Unique text styles (colors, attributes) currently in use.");
        helpMarker(c"Unique text styles (colors, attributes) currently in use.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", page.styles.count());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.styles.count() as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", page.styles.layout.cap);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.styles.layout.cap as c_int) };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Graphemes");
        unsafe { dcimgui_sys::ImGui_Text(c"Graphemes".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Extended grapheme clusters for multi-codepoint characters.");
        helpMarker(c"Extended grapheme clusters for multi-codepoint characters.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", page.graphemeCount());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.grapheme_count() as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", page.graphemeCapacity());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.grapheme_capacity() as c_int) };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Strings (bytes)");
        unsafe { dcimgui_sys::ImGui_Text(c"Strings (bytes)".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("String storage for hyperlink URIs and other text data.");
        helpMarker(c"String storage for hyperlink URIs and other text data.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", page.string_alloc.usedBytes(page.memory));
        //
        // PORT NOTE: `usedBytes(memory)` is `unsafe` in the port (it walks the
        // bitmap off the page's base pointer); the base is projected via
        // `memory_ptr()`.
        unsafe {
            dcimgui_sys::ImGui_Text(
                c"%d".as_ptr(),
                page.string_alloc.used_bytes(page.memory_ptr()) as c_int,
            )
        };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", page.string_alloc.capacityBytes());
        unsafe {
            dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.string_alloc.capacity_bytes() as c_int)
        };

        // const hyperlink_map = page.hyperlink_map.map(page.memory);
        //
        // PORT NOTE: `map(memory)` is `unsafe` (resolves the offset-based map off
        // the page base); the base is projected via `memory_ptr()`.
        let hyperlink_map = unsafe { page.hyperlink_map.map(page.memory_ptr()) };
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Hyperlink Map");
        unsafe { dcimgui_sys::ImGui_Text(c"Hyperlink Map".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Maps cell positions to hyperlink IDs.");
        helpMarker(c"Maps cell positions to hyperlink IDs.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", hyperlink_map.count());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), hyperlink_map.count() as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", hyperlink_map.capacity());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), hyperlink_map.capacity() as c_int) };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Hyperlink IDs");
        unsafe { dcimgui_sys::ImGui_Text(c"Hyperlink IDs".as_ptr()) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // widgets.helpMarker("Unique hyperlink definitions (URI + optional ID).");
        helpMarker(c"Unique hyperlink definitions (URI + optional ID).");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // cimgui.c.ImGui_Text("%d", page.hyperlink_set.count());
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.hyperlink_set.count() as c_int) };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // cimgui.c.ImGui_Text("%d", page.hyperlink_set.layout.cap);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), page.hyperlink_set.layout.cap as c_int) };
    }
}

// fn rowsTable(page: *const terminal.Page) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn rowsTable(page: &Page) {
    // const visible_rows: usize = @min(page.size.rows, 12);
    let visible_rows: usize = core::cmp::min(page.size.rows as usize, 12);
    // const row_height: f32 = cimgui.c.ImGui_GetTextLineHeightWithSpacing();
    let row_height: f32 = unsafe { dcimgui_sys::ImGui_GetTextLineHeightWithSpacing() };
    // const child_height: f32 = row_height * (@as(f32, @floatFromInt(visible_rows)) + 2.0);
    let child_height: f32 = row_height * (visible_rows as f32 + 2.0);

    // Child window so scrolling is separate.
    // This defer first is not a bug, EndChild always needs to be called.
    // defer cimgui.c.ImGui_EndChild();
    let _end_child = Defer(|| unsafe { dcimgui_sys::ImGui_EndChild() });
    // if (!cimgui.c.ImGui_BeginChild(
    //     "##page_rows",
    //     .{ .x = 0.0, .y = child_height },
    //     cimgui.c.ImGuiChildFlags_Borders,
    //     cimgui.c.ImGuiWindowFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginChild(
            c"##page_rows".as_ptr(),
            dcimgui_sys::ImVec2 {
                x: 0.0,
                y: child_height,
            },
            dcimgui_sys::ImGuiChildFlags__ImGuiChildFlags_Borders as dcimgui_sys::ImGuiChildFlags,
            dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_None as dcimgui_sys::ImGuiWindowFlags,
        )
    } {
        return;
    }

    // if (!cimgui.c.ImGui_BeginTable(
    //     "##page_rows_table",
    //     10,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##page_rows_table".as_ptr(),
            10,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_BordersInnerV
                as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                    as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                    as dcimgui_sys::ImGuiTableFlags),
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableSetupScrollFreeze(0, 1);
    unsafe { dcimgui_sys::ImGui_TableSetupScrollFreeze(0, 1) };
    // cimgui.c.ImGui_TableSetupColumn("Row", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Text", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Dirty", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Wrap", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Cont", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Styled", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Grapheme", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Link", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Prompt", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Kitty", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    for name in [
        c"Row".as_ptr(),
        c"Text".as_ptr(),
        c"Dirty".as_ptr(),
        c"Wrap".as_ptr(),
        c"Cont".as_ptr(),
        c"Styled".as_ptr(),
        c"Grapheme".as_ptr(),
        c"Link".as_ptr(),
        c"Prompt".as_ptr(),
        c"Kitty".as_ptr(),
    ] {
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                name,
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
    }
    // cimgui.c.ImGui_TableHeadersRow();
    unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

    // const rows = page.rows.ptr(page.memory)[0..page.size.rows];
    //
    // PORT NOTE (INV-1): the offset-based `rows` array is resolved off the page's
    // base pointer (`memory_ptr()`) and bounded by `size.rows`; a call-scoped
    // read under the surface mutex, never cached.
    let base = page.memory_ptr();
    let rows: &[Row] = unsafe {
        core::slice::from_raw_parts(page.rows.ptr(base) as *const Row, page.size.rows as usize)
    };
    // for (rows, 0..) |*row, row_index| {
    for (row_index, row) in rows.iter().enumerate() {
        // var text_cells: usize = 0;
        let mut text_cells: usize = 0;
        // const cells = page.getCells(row);
        let cells: &[Cell] = unsafe { &*page.get_cells(row as *const Row) };
        // for (cells) |cell| {
        //     if (cell.hasText()) {
        //         text_cells += 1;
        //     }
        // }
        for cell in cells {
            if cell.has_text() {
                text_cells += 1;
            }
        }

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("%d", row_index);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), row_index as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // if (text_cells == 0) {
        //     cimgui.c.ImGui_TextDisabled("0");
        // } else {
        //     cimgui.c.ImGui_Text("%d", text_cells);
        // }
        if text_cells == 0 {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"0".as_ptr()) };
        } else {
            unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), text_cells as c_int) };
        }

        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // flagCell(row.dirty);
        //
        // PORT NOTE (INV-2): `row.dirty` (a packed-struct accessor in the port)
        // is read purely to render the flag cell — never cleared.
        flagCell(row.dirty());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // flagCell(row.wrap);
        flagCell(row.wrap());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(4);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(4) };
        // flagCell(row.wrap_continuation);
        flagCell(row.wrap_continuation());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(5);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(5) };
        // flagCell(row.styled);
        flagCell(row.styled());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(6);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(6) };
        // flagCell(row.grapheme);
        flagCell(row.grapheme());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(7);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(7) };
        // flagCell(row.hyperlink);
        flagCell(row.hyperlink());

        // _ = cimgui.c.ImGui_TableSetColumnIndex(8);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(8) };
        // cimgui.c.ImGui_Text("%s", @tagName(row.semantic_prompt).ptr);
        //
        // PORT NOTE (§3 `@tagName`): the payload-free `Row.SemanticPrompt` tag
        // name string is produced by a match over the variants, reproducing the
        // Zig field spelling (none / prompt / prompt_continuation).
        let semantic_prompt_name: &CStr = match row.semantic_prompt() {
            SemanticPrompt::None => c"none",
            SemanticPrompt::Prompt => c"prompt",
            SemanticPrompt::PromptContinuation => c"prompt_continuation",
        };
        unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), semantic_prompt_name.as_ptr()) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(9);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(9) };
        // flagCell(row.kitty_virtual_placeholder);
        flagCell(row.kitty_virtual_placeholder());
    }
}

// fn stylesList(page: *const Page) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn stylesList(page: &Page) {
    // const items = page.styles.items.ptr(page.memory)[0..page.styles.layout.cap];
    let base = page.memory_ptr();
    // PORT NOTE (layout parity, N88): the items are the reference's stride
    // apart, not `size_of::<Item>()`, so they are addressed one by one with
    // `item_at` rather than viewed as a `&[Item]` slice.
    let items_ptr = unsafe { page.styles.items.ptr(base) };
    let items = (0..page.styles.layout.cap)
        .map(move |i| unsafe { &*rustty_vt::ref_counted_set::item_at(items_ptr, i) });

    // var count: usize = 0;
    // for (items, 0..) |item, index| {
    //     if (index == 0) continue;
    //     if (item.meta.ref == 0) continue;
    //     count += 1;
    // }
    let mut count: usize = 0;
    for (index, item) in items.clone().enumerate() {
        if index == 0 {
            continue;
        }
        if item.meta.ref_count == 0 {
            continue;
        }
        count += 1;
    }

    // if (count == 0) {
    //     cimgui.c.ImGui_TextDisabled("(no styles in use)");
    //     return;
    // }
    if count == 0 {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(no styles in use)".as_ptr()) };
        return;
    }

    // const visible_rows: usize = @min(count, 8);
    let visible_rows: usize = core::cmp::min(count, 8);
    // const row_height: f32 = cimgui.c.ImGui_GetTextLineHeightWithSpacing();
    let row_height: f32 = unsafe { dcimgui_sys::ImGui_GetTextLineHeightWithSpacing() };
    // const child_height: f32 = row_height * (@as(f32, @floatFromInt(visible_rows)) + 2.0);
    let child_height: f32 = row_height * (visible_rows as f32 + 2.0);

    // defer cimgui.c.ImGui_EndChild();
    let _end_child = Defer(|| unsafe { dcimgui_sys::ImGui_EndChild() });
    // if (!cimgui.c.ImGui_BeginChild(
    //     "##page_styles",
    //     .{ .x = 0.0, .y = child_height },
    //     cimgui.c.ImGuiChildFlags_Borders,
    //     cimgui.c.ImGuiWindowFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginChild(
            c"##page_styles".as_ptr(),
            dcimgui_sys::ImVec2 {
                x: 0.0,
                y: child_height,
            },
            dcimgui_sys::ImGuiChildFlags__ImGuiChildFlags_Borders as dcimgui_sys::ImGuiChildFlags,
            dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_None as dcimgui_sys::ImGuiWindowFlags,
        )
    } {
        return;
    }

    // if (!cimgui.c.ImGui_BeginTable(
    //     "##page_styles_table",
    //     3,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##page_styles_table".as_ptr(),
            3,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_BordersInnerV
                as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                    as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                    as dcimgui_sys::ImGuiTableFlags),
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableSetupScrollFreeze(0, 1);
    unsafe { dcimgui_sys::ImGui_TableSetupScrollFreeze(0, 1) };
    // cimgui.c.ImGui_TableSetupColumn("ID", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"ID".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableSetupColumn("Refs", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Refs".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableSetupColumn("Style", cimgui.c.ImGuiTableColumnFlags_WidthStretch);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Style".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableHeadersRow();
    unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

    // for (items, 0..) |item, index| {
    //     if (index == 0) continue;
    //     if (item.meta.ref == 0) continue;
    for (index, item) in items.clone().enumerate() {
        if index == 0 {
            continue;
        }
        if item.meta.ref_count == 0 {
            continue;
        }

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // cimgui.c.ImGui_PushIDInt(@intCast(index));
        unsafe { dcimgui_sys::ImGui_PushIDInt(index as c_int) };
        // defer cimgui.c.ImGui_PopID();
        let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("%d", index);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), index as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // cimgui.c.ImGui_Text("%d", item.meta.ref);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), item.meta.ref_count as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // if (cimgui.c.ImGui_TreeNodeEx("Details", cimgui.c.ImGuiTreeNodeFlags_None)) {
        //     defer cimgui.c.ImGui_TreePop();
        //     widgets.style.table(item.value, null);
        // }
        if unsafe {
            dcimgui_sys::ImGui_TreeNodeEx(
                c"Details".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_None
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            let _tree_pop = Defer(|| unsafe { dcimgui_sys::ImGui_TreePop() });
            super::style::table(item.value, None);
        }
    }
}

// fn hyperlinksList(page: *const Page) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn hyperlinksList(page: &Page) {
    // const items = page.hyperlink_set.items.ptr(page.memory)[0..page.hyperlink_set.layout.cap];
    let base = page.memory_ptr();
    // PORT NOTE (layout parity, N88): addressed with `item_at`, see
    // `stylesList`.
    let items_ptr = unsafe { page.hyperlink_set.items.ptr(base) };
    let items = (0..page.hyperlink_set.layout.cap)
        .map(move |i| unsafe { &*rustty_vt::ref_counted_set::item_at(items_ptr, i) });

    // var count: usize = 0;
    // for (items, 0..) |item, index| {
    //     if (index == 0) continue;
    //     if (item.meta.ref == 0) continue;
    //     count += 1;
    // }
    let mut count: usize = 0;
    for (index, item) in items.clone().enumerate() {
        if index == 0 {
            continue;
        }
        if item.meta.ref_count == 0 {
            continue;
        }
        count += 1;
    }

    // if (count == 0) {
    //     cimgui.c.ImGui_TextDisabled("(no hyperlinks in use)");
    //     return;
    // }
    if count == 0 {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(no hyperlinks in use)".as_ptr()) };
        return;
    }

    // const visible_rows: usize = @min(count, 8);
    let visible_rows: usize = core::cmp::min(count, 8);
    // const row_height: f32 = cimgui.c.ImGui_GetTextLineHeightWithSpacing();
    let row_height: f32 = unsafe { dcimgui_sys::ImGui_GetTextLineHeightWithSpacing() };
    // const child_height: f32 = row_height * (@as(f32, @floatFromInt(visible_rows)) + 2.0);
    let child_height: f32 = row_height * (visible_rows as f32 + 2.0);

    // defer cimgui.c.ImGui_EndChild();
    let _end_child = Defer(|| unsafe { dcimgui_sys::ImGui_EndChild() });
    // if (!cimgui.c.ImGui_BeginChild(
    //     "##page_hyperlinks",
    //     .{ .x = 0.0, .y = child_height },
    //     cimgui.c.ImGuiChildFlags_Borders,
    //     cimgui.c.ImGuiWindowFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginChild(
            c"##page_hyperlinks".as_ptr(),
            dcimgui_sys::ImVec2 {
                x: 0.0,
                y: child_height,
            },
            dcimgui_sys::ImGuiChildFlags__ImGuiChildFlags_Borders as dcimgui_sys::ImGuiChildFlags,
            dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_None as dcimgui_sys::ImGuiWindowFlags,
        )
    } {
        return;
    }

    // if (!cimgui.c.ImGui_BeginTable(
    //     "##page_hyperlinks_table",
    //     4,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##page_hyperlinks_table".as_ptr(),
            4,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_BordersInnerV
                as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                    as dcimgui_sys::ImGuiTableFlags)
                | (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                    as dcimgui_sys::ImGuiTableFlags),
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableSetupScrollFreeze(0, 1);
    unsafe { dcimgui_sys::ImGui_TableSetupScrollFreeze(0, 1) };
    // cimgui.c.ImGui_TableSetupColumn("ID", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"ID".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableSetupColumn("Refs", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Refs".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableSetupColumn("Explicit ID", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Explicit ID".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableSetupColumn("URI", cimgui.c.ImGuiTableColumnFlags_WidthStretch);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"URI".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };
    // cimgui.c.ImGui_TableHeadersRow();
    unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

    // for (items, 0..) |item, index| {
    //     if (index == 0) continue;
    //     if (item.meta.ref == 0) continue;
    for (index, item) in items.clone().enumerate() {
        if index == 0 {
            continue;
        }
        if item.meta.ref_count == 0 {
            continue;
        }

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("%d", index);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), index as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // cimgui.c.ImGui_Text("%d", item.meta.ref);
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), item.meta.ref_count as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        // switch (item.value.id) {
        //     .explicit => |slice| {
        //         const explicit_id = slice.slice(page.memory);
        //         cimgui.c.ImGui_Text("%.*s", explicit_id.len, explicit_id.ptr);
        //     },
        //     .implicit => cimgui.c.ImGui_TextDisabled("-"),
        // }
        //
        // PORT NOTE: `slice.slice(memory)` is `unsafe` in the port (resolves the
        // offset-slice off the page base, projected via `memory_ptr()`); `%.*s`
        // takes `(int precision, char* ptr)` so the (possibly non-NUL-terminated)
        // byte slice is bounded by its length.
        match item.value.id.tag() {
            PageEntryIdTag::Explicit => {
                let explicit_id = unsafe { item.value.id.explicit().slice(base) };
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%.*s".as_ptr(),
                        explicit_id.len() as c_int,
                        explicit_id.as_ptr() as *const c_char,
                    )
                };
            }
            PageEntryIdTag::Implicit => {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
            }
        }

        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        // const uri = item.value.uri.slice(page.memory);
        // cimgui.c.ImGui_Text("%.*s", uri.len, uri.ptr);
        let uri = unsafe { item.value.uri.slice(base) };
        unsafe {
            dcimgui_sys::ImGui_Text(
                c"%.*s".as_ptr(),
                uri.len() as c_int,
                uri.as_ptr() as *const c_char,
            )
        };
    }
}

// fn flagCell(value: bool) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn flagCell(value: bool) {
    // if (value) {
    //     cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.9, .z = 0.4, .w = 1.0 }, "yes");
    // } else {
    //     cimgui.c.ImGui_TextDisabled("-");
    // }
    if value {
        unsafe {
            dcimgui_sys::ImGui_TextColoredPtr(
                &dcimgui_sys::ImVec4 {
                    x: 0.4,
                    y: 0.9,
                    z: 0.4,
                    w: 1.0,
                },
                c"yes".as_ptr(),
            )
        };
    } else {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
    }
}
