//! PageList inspector widget (`Inspector` / `CellChooser` / `CellInfo`).
//!
//! PORTS: ghostty src/inspector/widgets/pagelist.zig (852 LOC — the largest
//! widget submodule).
//!
//! PORT STATUS: fully LIVE (Task 13). Every function is a display-only read of a
//! `terminal.PageList` obtained per-frame under the surface's
//! `renderer_state.mutex` (INV-1). Two seams touch mutable pagelist state, both
//! faithful to the reference: `scrollbarInfo` calls `pages.scroll(.{ .delta_row
//! = ... })` (the already-ported `PageList::scroll`, the sole `*PageList`
//! mutation this widget performs — a viewport scroll, NOT a terminal-state
//! write-back, so INV-3 is untouched) and `CellChooser` edits stack-local
//! `lookup_*` fields only.
//!
//! INV-2 (dirty tracking is READ-ONLY): `trackedPinsTable` reads `pin.isDirty()`
//! (pagelist.zig:324) purely to render a "dirty"/"clean" state cell — the widget
//! OBSERVES the dirty state and NEVER clears it, so the renderer's dirty-clear
//! contract (D5) is untouched.
//!
//! §3 comptime hand-expansion (PREPORT_INSPECTOR §3): Rust has no `inline for` /
//! `comptime` block / `@tagName`, so the three reflection sites are
//! monomorphized by hand:
//!   * `std.meta.tags(terminal.point.Tag)` — the verbatim tag set is carried as
//!     the `POINT_TAGS` array + the per-tag `tag_name` match.
//!   * the `combo_width` `comptime blk:` block (scan tag-name lengths → a const)
//!     is reproduced by a `const {}` block computing the same `max_len + 4`.
//!   * `regionHelpText(comptime tag)` + `regionRow(pages, comptime tag)` — the
//!     `inline for` that instantiates them per tag is hand-expanded into the four
//!     explicit `region_row` calls, each dispatching through the per-tag matches.
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11).

// const std = @import("std");
// const cimgui = @import("dcimgui");
// const terminal = @import("../../terminal/main.zig");
// const stylepkg = @import("../../terminal/style.zig");
// const widgets = @import("../widgets.zig");
// const units = @import("../units.zig");
//
// const PageList = terminal.PageList;
//
// PORT NOTE (page.rs/style.rs precedent): `dcimgui` is the vendored Dear ImGui C
// API reached through `dcimgui-sys` (full `dcimgui_sys::` paths). `PageList` /
// its `Cell` / `Scrollbar` / `Scroll` live in rustty-vt's `pagelist` module;
// `terminal.point.{Tag, Point, Coordinate}` in `point`; `terminal.Pin` is
// `pagelist::Pin`; `terminal.size.CellCountInt` in `term_size`; the packed
// `Cell` (`cell.cell`) in `page`; the hyperlink `PageEntryId` in `hyperlink`;
// `stylepkg.default_id` is `style::DEFAULT_ID`. `widgets.page.{treeNode,
// inspector}` are the Task 12 sibling widget; `widgets.style.table` the Task 9
// helper; `widgets.helpMarker` the Task 7 helper; `units.toKibiBytes` the leaf.
// The intrusive page linked-list is walked via the `rustty_datastruct` `Node`
// trait's `prev()` accessor (rustty-vt keeps the `Node.prev`/`.next` fields
// crate-private).
use core::ffi::{c_char, c_int, c_uint, CStr};

use rustty_datastruct::intrusive_linked_list::Node as _;
use rustty_vt::hyperlink::PageEntryId;
use rustty_vt::pagelist::access::Cell;
use rustty_vt::pagelist::scroll::Scroll;
use rustty_vt::pagelist::Pin;
use rustty_vt::point::{Coordinate, Point, Tag};
use rustty_vt::style as stylepkg;
use rustty_vt::{PageList, Scrollbar};

use super::helpMarker;
use crate::units;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`page.rs`). Backs the ImGui `BeginTable`/`EndTable`,
/// `BeginListBox`/`EndListBox`, `BeginCombo`/`EndCombo`,
/// `BeginDisabled`/`EndDisabled`, `TreeNodeEx`/`TreePop`, and `PushID`/`PopID`
/// pairings below so they fire on every exit path. panic=abort means no
/// unwinding ever crosses these FFI calls, so drop-on-scope-exit is the sole
/// runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// The verbatim `std.meta.tags(terminal.point.Tag)` set, in declaration order
/// (point.zig: active, viewport, screen, history). Backs the hand-expanded
/// `inline for` loops (regionsTable, the CellChooser combo).
const POINT_TAGS: [Tag; 4] = [Tag::Active, Tag::Viewport, Tag::Screen, Tag::History];

/// `@tagName(terminal.point.Tag)` — Rust has no `@tagName`; the payload-free tag
/// name is produced by a match reproducing the Zig field spelling.
fn tag_name(tag: Tag) -> &'static CStr {
    match tag {
        Tag::Active => c"active",
        Tag::Viewport => c"viewport",
        Tag::Screen => c"screen",
        Tag::History => c"history",
    }
}

/// PageList inspector widget.
// pub const Inspector = struct {
pub struct Inspector {}

impl Inspector {
    // pub const empty: Inspector = .{};
    pub const EMPTY: Inspector = Inspector {};

    // pub fn draw(_: *const Inspector, pages: *PageList) void {
    //
    // PORT NOTE: `pages: *PageList` is a mutable pointer (scrollbarInfo scrolls
    // the viewport); ported as `&mut PageList`. `self` is `*const` → `&self`.
    #[allow(clippy::needless_range_loop)]
    pub fn draw(&self, pages: &mut PageList) {
        // cimgui.c.ImGui_TextWrapped(
        //     "PageList manages the backing pages that hold scrollback and the active " ++
        //         "terminal grid. Each page is a contiguous memory buffer with its " ++
        //         "own rows, cells, style set, grapheme map, and hyperlink storage.",
        // );
        unsafe {
            dcimgui_sys::ImGui_TextWrapped(
                c"PageList manages the backing pages that hold scrollback and the active terminal grid. Each page is a contiguous memory buffer with its own rows, cells, style set, grapheme map, and hyperlink storage."
                    .as_ptr(),
            )
        };

        // if (cimgui.c.ImGui_CollapsingHeader(
        //     "Overview",
        //     cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
        // )) {
        //     summaryTable(pages);
        // }
        if unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Overview".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_DefaultOpen
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            summaryTable(pages);
        }

        // if (cimgui.c.ImGui_CollapsingHeader(
        //     "Scrollbar & Regions",
        //     cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
        // )) {
        if unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Scrollbar & Regions".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_DefaultOpen
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            // cimgui.c.ImGui_SeparatorText("Scrollbar");
            unsafe { dcimgui_sys::ImGui_SeparatorText(c"Scrollbar".as_ptr()) };
            // scrollbarInfo(pages);
            scrollbarInfo(pages);
            // cimgui.c.ImGui_SeparatorText("Regions");
            unsafe { dcimgui_sys::ImGui_SeparatorText(c"Regions".as_ptr()) };
            // regionsTable(pages);
            regionsTable(pages);
        }

        // if (cimgui.c.ImGui_CollapsingHeader(
        //     "Tracked Pins",
        //     cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
        // )) {
        //     trackedPinsTable(pages);
        // }
        if unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Tracked Pins".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_DefaultOpen
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            trackedPinsTable(pages);
        }

        // if (cimgui.c.ImGui_CollapsingHeader(
        //     "Pages",
        //     cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
        // )) {
        if unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Pages".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_DefaultOpen
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            // widgets.helpMarker(
            //     "Pages are shown most-recent first. Each page holds a grid of rows/cells " ++
            //         "plus metadata tables for styles, graphemes, strings, and hyperlinks.",
            // );
            helpMarker(
                c"Pages are shown most-recent first. Each page holds a grid of rows/cells plus metadata tables for styles, graphemes, strings, and hyperlinks.",
            );

            // const active_pin = pages.getTopLeft(.active);
            // const viewport_pin = pages.getTopLeft(.viewport);
            let active_pin = pages.get_top_left(Tag::Active);
            let viewport_pin = pages.get_top_left(Tag::Viewport);

            // var row_offset = pages.total_rows;
            // var index: usize = pages.totalPages();
            // var node = pages.pages.last;
            let mut row_offset = pages.total_rows;
            let mut index: usize = pages.total_pages();
            let mut node = pages.pages.last;
            // while (node) |page_node| : (node = page_node.prev) {
            while !node.is_null() {
                let page_node = node;
                // const page = &page_node.data;
                let page = unsafe { &(*rustty_vt::pagelist::Node::page(page_node)) };
                // row_offset -= page.size.rows;
                row_offset -= page.size.rows as usize;
                // index -= 1;
                index -= 1;

                // We use our location as the ID so that even if reallocations
                // happen we remain open if we're open already.
                // cimgui.c.ImGui_PushIDInt(@intCast(index));
                unsafe { dcimgui_sys::ImGui_PushIDInt(index as c_int) };
                // defer cimgui.c.ImGui_PopID();
                let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

                // Open up the tree node.
                // if (!widgets.page.treeNode(.{
                //     .page = page,
                //     .index = index,
                //     .row_range = .{ row_offset, row_offset + page.size.rows - 1 },
                //     .active = node == active_pin.node,
                //     .viewport = node == viewport_pin.node,
                // })) continue;
                // defer cimgui.c.ImGui_TreePop();
                // widgets.page.inspector(page);
                //
                // PORT NOTE: the reference `continue` skips the `TreePop`/inspector
                // but still runs the loop-continue expression `node =
                // page_node.prev`. Ported as an `if open { .. }` so the
                // `node`-advance below always runs; the two `defer`s fire LIFO at
                // block exit (TreePop before PopID) exactly as the reference does.
                if super::page::treeNode(super::page::TreeNodeState {
                    page,
                    index,
                    row_range: [row_offset, row_offset + page.size.rows as usize - 1],
                    active: core::ptr::eq(page_node, active_pin.node),
                    viewport: core::ptr::eq(page_node, viewport_pin.node),
                }) {
                    let _tree_pop = Defer(|| unsafe { dcimgui_sys::ImGui_TreePop() });
                    super::page::inspector(page);
                }

                // : (node = page_node.prev)
                node = unsafe { (*page_node).prev() };
            }
        }
    }
}

// fn summaryTable(pages: *const PageList) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn summaryTable(pages: &PageList) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "pagelist_summary",
    //     3,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"pagelist_summary".as_ptr(),
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

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Active Grid");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Active viewport size in columns x rows.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%dc x %dr", pages.cols, pages.rows);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Active Grid".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Active viewport size in columns x rows.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%dc x %dr".as_ptr(),
            pages.cols as c_int,
            pages.rows as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Pages");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Total number of pages in the linked list.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", pages.totalPages());
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Pages".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Total number of pages in the linked list.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), pages.total_pages() as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Total Rows");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Total rows represented by scrollback + active area.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", pages.total_rows);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Total Rows".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Total rows represented by scrollback + active area.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), pages.total_rows as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Page Bytes");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Total bytes allocated for active pages.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d KiB", units.toKibiBytes(pages.page_size));
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Page Bytes".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Total bytes allocated for active pages.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d KiB".as_ptr(),
            units::toKibiBytes(pages.page_size) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Max Size");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker(
    //     \\Maximum bytes before pages must be evicated. The total
    //     \\used bytes may be higher due to minimum individual page
    //     \\sizes but the next allocation that would exceed this limit
    //     \\will evict pages from the front of the list to free up space.
    // );
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d KiB", units.toKibiBytes(pages.maxSize()));
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Max Size".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(
        c"Maximum bytes before pages must be evicated. The total\nused bytes may be higher due to minimum individual page\nsizes but the next allocation that would exceed this limit\nwill evict pages from the front of the list to free up space.",
    );
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d KiB".as_ptr(),
            units::toKibiBytes(pages.max_size()) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Viewport");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Current viewport anchoring mode.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%s", @tagName(pages.viewport).ptr);
    //
    // PORT NOTE (§3 `@tagName`): the `PageList.Viewport` tag name (active/top/pin)
    // is produced by a match reproducing the Zig field spelling.
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Viewport".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Current viewport anchoring mode.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    let viewport_name: &CStr = match pages.viewport {
        rustty_vt::pagelist::Viewport::Active => c"active",
        rustty_vt::pagelist::Viewport::Top => c"top",
        rustty_vt::pagelist::Viewport::Pin => c"pin",
    };
    unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), viewport_name.as_ptr()) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Tracked Pins");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Number of pins tracked for automatic updates.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", pages.countTrackedPins());
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Tracked Pins".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Number of pins tracked for automatic updates.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), pages.count_tracked_pins() as c_int) };
}

// fn scrollbarInfo(pages: *PageList) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn scrollbarInfo(pages: &mut PageList) {
    // const scrollbar = pages.scrollbar();
    let scrollbar = pages.scrollbar();

    // If we have a scrollbar, show it.
    // if (scrollbar.total > 0) {
    if scrollbar.total > 0 {
        // var delta_row: isize = 0;
        let mut delta_row: isize = 0;
        // scrollbarWidget(&scrollbar, &delta_row);
        scrollbarWidget(&scrollbar, &mut delta_row);
        // if (delta_row != 0) {
        //     pages.scroll(.{ .delta_row = delta_row });
        // }
        if delta_row != 0 {
            pages.scroll(Scroll::DeltaRow(delta_row));
        }
    }

    // if (!cimgui.c.ImGui_BeginTable(
    //     "scrollbar_info",
    //     3,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"scrollbar_info".as_ptr(),
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

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Total");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Total number of scrollable rows including scrollback and active area.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", scrollbar.total);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Total".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Total number of scrollable rows including scrollback and active area.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), scrollbar.total as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Offset");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Current scroll position as row offset from the top of scrollback.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", scrollbar.offset);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Offset".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Current scroll position as row offset from the top of scrollback.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), scrollbar.offset as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Length");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker("Number of rows visible in the viewport.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // cimgui.c.ImGui_Text("%d", scrollbar.len);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Length".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(c"Number of rows visible in the viewport.");
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), scrollbar.len as c_int) };
}

// fn regionsTable(pages: *PageList) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn regionsTable(pages: &PageList) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "pagelist_regions",
    //     4,
    //     cimgui.c.ImGuiTableFlags_BordersInnerV |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"pagelist_regions".as_ptr(),
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

    // cimgui.c.ImGui_TableSetupColumn("Region", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Top-Left", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Bottom-Right", cimgui.c.ImGuiTableColumnFlags_WidthStretch);
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Region".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        );
        dcimgui_sys::ImGui_TableSetupColumn(
            c"".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        );
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Top-Left".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                as dcimgui_sys::ImGuiTableColumnFlags,
        );
        dcimgui_sys::ImGui_TableSetupColumn(
            c"Bottom-Right".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                as dcimgui_sys::ImGuiTableColumnFlags,
        );
    };
    // cimgui.c.ImGui_TableHeadersRow();
    unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

    // inline for (comptime std.meta.tags(terminal.point.Tag)) |tag| {
    //     regionRow(pages, tag);
    // }
    //
    // PORT NOTE (§3 `inline for` hand-expansion): the compile-time loop over the
    // `point.Tag` set is unrolled into the four explicit `regionRow` calls (the
    // verbatim `POINT_TAGS` set), each monomorphized on its concrete tag exactly
    // as the reference's `inline for` instantiates `regionRow`/`regionHelpText`.
    for tag in POINT_TAGS {
        regionRow(pages, tag);
    }
}

// fn regionRow(pages: *const PageList, comptime tag: terminal.point.Tag) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn regionRow(pages: &PageList, tag: Tag) {
    // const tl_pin = pages.getTopLeft(tag);
    // const br_pin = pages.getBottomRight(tag);
    let tl_pin = pages.get_top_left(tag);
    let br_pin = pages.get_bottom_right(tag);

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("%s", @tagName(tag).ptr);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), tag_name(tag).as_ptr()) };

    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // widgets.helpMarker(comptime regionHelpText(tag));
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    helpMarker(regionHelpText(tag));

    // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
    // if (pages.pointFromPin(tag, tl_pin)) |pt| {
    //     const coord = pt.coord();
    //     cimgui.c.ImGui_Text("(%d, %d)", coord.x, coord.y);
    // } else {
    //     cimgui.c.ImGui_TextDisabled("(n/a)");
    // }
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
    if let Some(pt) = pages.point_from_pin(tag, tl_pin) {
        let coord = pt.coord();
        unsafe {
            dcimgui_sys::ImGui_Text(c"(%d, %d)".as_ptr(), coord.x as c_int, coord.y as c_int)
        };
    } else {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(n/a)".as_ptr()) };
    }

    // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
    // if (br_pin) |br| {
    //     if (pages.pointFromPin(tag, br)) |pt| {
    //         const coord = pt.coord();
    //         cimgui.c.ImGui_Text("(%d, %d)", coord.x, coord.y);
    //     } else {
    //         cimgui.c.ImGui_TextDisabled("(n/a)");
    //     }
    // } else {
    //     cimgui.c.ImGui_TextDisabled("(empty)");
    // }
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
    if let Some(br) = br_pin {
        if let Some(pt) = pages.point_from_pin(tag, br) {
            let coord = pt.coord();
            unsafe {
                dcimgui_sys::ImGui_Text(c"(%d, %d)".as_ptr(), coord.x as c_int, coord.y as c_int)
            };
        } else {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"(n/a)".as_ptr()) };
        }
    } else {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(empty)".as_ptr()) };
    }
}

// fn regionHelpText(comptime tag: terminal.point.Tag) [:0]const u8 {
//
// PORT NOTE (§3): `regionHelpText` is a comptime function returning a
// compile-time string per tag; ported as a `match` returning a `&'static CStr`.
#[allow(non_snake_case)] // faithful Zig name (R1)
fn regionHelpText(tag: Tag) -> &'static CStr {
    // return switch (tag) {
    match tag {
        // .active => "The active area where a running program can jump the cursor " ++
        //     "and make changes. This is the 'editable' part of the screen. " ++
        //     "Bottom-right includes the full height of the screen, including " ++
        //     "rows that may not be written yet.",
        Tag::Active => {
            c"The active area where a running program can jump the cursor and make changes. This is the 'editable' part of the screen. Bottom-right includes the full height of the screen, including rows that may not be written yet."
        }
        // .viewport => "The visible viewport. If the user has scrolled, top-left changes. " ++
        //     "Bottom-right is the last written row from the top-left.",
        Tag::Viewport => {
            c"The visible viewport. If the user has scrolled, top-left changes. Bottom-right is the last written row from the top-left."
        }
        // .screen => "Top-left is the furthest back in scrollback history. Bottom-right " ++
        //     "is the last written row. Unlike 'active', this only contains " ++
        //     "written rows.",
        Tag::Screen => {
            c"Top-left is the furthest back in scrollback history. Bottom-right is the last written row. Unlike 'active', this only contains written rows."
        }
        // .history => "Same top-left as 'screen' but bottom-right is the line just before " ++
        //     "the top of 'active'. Contains only the scrollback history.",
        Tag::History => {
            c"Same top-left as 'screen' but bottom-right is the line just before the top of 'active'. Contains only the scrollback history."
        }
    }
}

// fn trackedPinsTable(pages: *const PageList) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn trackedPinsTable(pages: &PageList) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "tracked_pins",
    //     5,
    //     cimgui.c.ImGuiTableFlags_Borders |
    //         cimgui.c.ImGuiTableFlags_RowBg |
    //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"tracked_pins".as_ptr(),
            5,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_Borders as dcimgui_sys::ImGuiTableFlags)
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

    // cimgui.c.ImGui_TableSetupColumn("Index", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Pin", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Context", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("Dirty", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    // cimgui.c.ImGui_TableSetupColumn("State", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
    for name in [
        c"Index".as_ptr(),
        c"Pin".as_ptr(),
        c"Context".as_ptr(),
        c"Dirty".as_ptr(),
        c"State".as_ptr(),
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

    // const active_pin = pages.getTopLeft(.active);
    // const viewport_pin = pages.getTopLeft(.viewport);
    let active_pin = pages.get_top_left(Tag::Active);
    let viewport_pin = pages.get_top_left(Tag::Viewport);

    // for (pages.trackedPins(), 0..) |tracked, idx| {
    //
    // PORT NOTE: `trackedPins()` returns an owned `Vec<*mut Pin>` snapshot (the
    // Rust `PinSet` keys are not a contiguous slice — see PageList `tracked_pins`
    // PORT NOTE); iterated with its index exactly as the reference `0..`.
    for (idx, tracked) in pages.tracked_pins().iter().enumerate() {
        let tracked: *mut Pin = *tracked;
        // const pin = tracked.*;
        let pin = unsafe { *tracked };
        // cimgui.c.ImGui_TableNextRow();
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        // cimgui.c.ImGui_Text("%d", idx);
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), idx as c_int) };

        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        // if (pin.garbage) {
        //     cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.5, .z = 0.3, .w = 1.0 }, "(%d, %d)", pin.x, pin.y);
        // } else {
        //     cimgui.c.ImGui_Text("(%d, %d)", pin.x, pin.y);
        // }
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        if pin.garbage {
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    dcimgui_sys::ImVec4 {
                        x: 1.0,
                        y: 0.5,
                        z: 0.3,
                        w: 1.0,
                    },
                    c"(%d, %d)".as_ptr(),
                    pin.x as c_int,
                    pin.y as c_int,
                )
            };
        } else {
            unsafe {
                dcimgui_sys::ImGui_Text(c"(%d, %d)".as_ptr(), pin.x as c_int, pin.y as c_int)
            };
        }

        // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
        // if (pages.pointFromPin(.screen, pin)) |pt| {
        //     const coord = pt.coord();
        //     cimgui.c.ImGui_Text("screen (%d, %d)", coord.x, coord.y);
        // } else {
        //     cimgui.c.ImGui_TextDisabled("screen (out of range)");
        // }
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
        if let Some(pt) = pages.point_from_pin(Tag::Screen, pin) {
            let coord = pt.coord();
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"screen (%d, %d)".as_ptr(),
                    coord.x as c_int,
                    coord.y as c_int,
                )
            };
        } else {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"screen (out of range)".as_ptr()) };
        }

        // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
        // const dirty = pin.isDirty();
        // if (dirty) {
        //     cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.4, .z = 0.4, .w = 1.0 }, "dirty");
        // } else {
        //     cimgui.c.ImGui_TextDisabled("clean");
        // }
        //
        // PORT NOTE (INV-2): `pin.isDirty()` is a pure observation of the D5 dirty
        // state — read to render the "dirty"/"clean" cell, NEVER cleared here.
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
        let dirty = pin.is_dirty();
        if dirty {
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    dcimgui_sys::ImVec4 {
                        x: 1.0,
                        y: 0.4,
                        z: 0.4,
                        w: 1.0,
                    },
                    c"dirty".as_ptr(),
                )
            };
        } else {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"clean".as_ptr()) };
        }

        // _ = cimgui.c.ImGui_TableSetColumnIndex(4);
        // if (pin.eql(active_pin)) {
        //     cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.9, .z = 0.4, .w = 1.0 }, "active top");
        // } else if (pin.eql(viewport_pin)) {
        //     cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.8, .z = 1.0, .w = 1.0 }, "viewport top");
        // } else if (pin.garbage) {
        //     cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.5, .z = 0.3, .w = 1.0 }, "garbage");
        // } else if (tracked == pages.viewport_pin) {
        //     cimgui.c.ImGui_Text("viewport pin");
        // } else {
        //     cimgui.c.ImGui_TextDisabled("tracked");
        // }
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(4) };
        if pin.eql(active_pin) {
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    dcimgui_sys::ImVec4 {
                        x: 0.4,
                        y: 0.9,
                        z: 0.4,
                        w: 1.0,
                    },
                    c"active top".as_ptr(),
                )
            };
        } else if pin.eql(viewport_pin) {
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    dcimgui_sys::ImVec4 {
                        x: 0.4,
                        y: 0.8,
                        z: 1.0,
                        w: 1.0,
                    },
                    c"viewport top".as_ptr(),
                )
            };
        } else if pin.garbage {
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    dcimgui_sys::ImVec4 {
                        x: 1.0,
                        y: 0.5,
                        z: 0.3,
                        w: 1.0,
                    },
                    c"garbage".as_ptr(),
                )
            };
        } else if core::ptr::eq(tracked, pages.viewport_pin) {
            unsafe { dcimgui_sys::ImGui_Text(c"viewport pin".as_ptr()) };
        } else {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"tracked".as_ptr()) };
        }
    }
}

// fn scrollbarWidget(
//     scrollbar: *const PageList.Scrollbar,
//     delta_row: *isize,
// ) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn scrollbarWidget(scrollbar: &Scrollbar, delta_row: &mut isize) {
    // delta_row.* = 0;
    *delta_row = 0;

    // const avail_width = cimgui.c.ImGui_GetContentRegionAvail().x;
    // const bar_height: f32 = cimgui.c.ImGui_GetFrameHeight();
    // const cursor_pos = cimgui.c.ImGui_GetCursorScreenPos();
    let avail_width = unsafe { dcimgui_sys::ImGui_GetContentRegionAvail() }.x;
    let bar_height: f32 = unsafe { dcimgui_sys::ImGui_GetFrameHeight() };
    let cursor_pos = unsafe { dcimgui_sys::ImGui_GetCursorScreenPos() };

    // const total_f: f32 = @floatFromInt(scrollbar.total);
    // const offset_f: f32 = @floatFromInt(scrollbar.offset);
    // const len_f: f32 = @floatFromInt(scrollbar.len);
    let total_f: f32 = scrollbar.total as f32;
    let offset_f: f32 = scrollbar.offset as f32;
    let len_f: f32 = scrollbar.len as f32;

    // const grab_start = (offset_f / total_f) * avail_width;
    // const grab_width = @max((len_f / total_f) * avail_width, 4.0);
    let grab_start = (offset_f / total_f) * avail_width;
    let grab_width = ((len_f / total_f) * avail_width).max(4.0);

    // const draw_list = cimgui.c.ImGui_GetWindowDrawList();
    // const bg_color = cimgui.c.ImGui_GetColorU32(cimgui.c.ImGuiCol_ScrollbarBg);
    // const grab_color = cimgui.c.ImGui_GetColorU32(cimgui.c.ImGuiCol_ScrollbarGrab);
    let draw_list = unsafe { dcimgui_sys::ImGui_GetWindowDrawList() };
    let bg_color = unsafe {
        dcimgui_sys::ImGui_GetColorU32(
            dcimgui_sys::ImGuiCol__ImGuiCol_ScrollbarBg as dcimgui_sys::ImGuiCol,
        )
    };
    let grab_color = unsafe {
        dcimgui_sys::ImGui_GetColorU32(
            dcimgui_sys::ImGuiCol__ImGuiCol_ScrollbarGrab as dcimgui_sys::ImGuiCol,
        )
    };

    // const bg_min: cimgui.c.ImVec2 = cursor_pos;
    // const bg_max: cimgui.c.ImVec2 = .{ .x = cursor_pos.x + avail_width, .y = cursor_pos.y + bar_height };
    // cimgui.c.ImDrawList_AddRectFilledEx(draw_list, bg_min, bg_max, bg_color, 0, 0);
    let bg_min: dcimgui_sys::ImVec2 = cursor_pos;
    let bg_max: dcimgui_sys::ImVec2 = dcimgui_sys::ImVec2 {
        x: cursor_pos.x + avail_width,
        y: cursor_pos.y + bar_height,
    };
    unsafe {
        dcimgui_sys::ImDrawList_AddRectFilledEx(
            draw_list,
            bg_min,
            bg_max,
            bg_color,
            0.0,
            0 as dcimgui_sys::ImDrawFlags,
        )
    };

    // const grab_min: cimgui.c.ImVec2 = .{ .x = cursor_pos.x + grab_start, .y = cursor_pos.y };
    // const grab_max: cimgui.c.ImVec2 = .{ .x = cursor_pos.x + grab_start + grab_width, .y = cursor_pos.y + bar_height };
    // cimgui.c.ImDrawList_AddRectFilledEx(draw_list, grab_min, grab_max, grab_color, 0, 0);
    let grab_min: dcimgui_sys::ImVec2 = dcimgui_sys::ImVec2 {
        x: cursor_pos.x + grab_start,
        y: cursor_pos.y,
    };
    let grab_max: dcimgui_sys::ImVec2 = dcimgui_sys::ImVec2 {
        x: cursor_pos.x + grab_start + grab_width,
        y: cursor_pos.y + bar_height,
    };
    unsafe {
        dcimgui_sys::ImDrawList_AddRectFilledEx(
            draw_list,
            grab_min,
            grab_max,
            grab_color,
            0.0,
            0 as dcimgui_sys::ImDrawFlags,
        )
    };
    // _ = cimgui.c.ImGui_InvisibleButton(
    //     "scrollbar_drag",
    //     .{ .x = avail_width, .y = bar_height },
    //     0,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_InvisibleButton(
            c"scrollbar_drag".as_ptr(),
            dcimgui_sys::ImVec2 {
                x: avail_width,
                y: bar_height,
            },
            0 as dcimgui_sys::ImGuiButtonFlags,
        )
    };
    // if (cimgui.c.ImGui_IsItemActive()) {
    if unsafe { dcimgui_sys::ImGui_IsItemActive() } {
        // const drag_delta = cimgui.c.ImGui_GetMouseDragDelta(
        //     cimgui.c.ImGuiMouseButton_Left,
        //     0.0,
        // );
        let drag_delta = unsafe {
            dcimgui_sys::ImGui_GetMouseDragDelta(
                dcimgui_sys::ImGuiMouseButton__ImGuiMouseButton_Left
                    as dcimgui_sys::ImGuiMouseButton,
                0.0,
            )
        };
        // if (drag_delta.x != 0) {
        if drag_delta.x != 0.0 {
            // const row_delta = (drag_delta.x / avail_width) * total_f;
            // delta_row.* = @intFromFloat(row_delta);
            // cimgui.c.ImGui_ResetMouseDragDelta();
            let row_delta = (drag_delta.x / avail_width) * total_f;
            *delta_row = row_delta as isize;
            unsafe { dcimgui_sys::ImGui_ResetMouseDragDelta() };
        }
    }

    // if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_DelayShort)) {
    //     cimgui.c.ImGui_SetTooltip(
    //         "offset=%d len=%d total=%d",
    //         scrollbar.offset,
    //         scrollbar.len,
    //         scrollbar.total,
    //     );
    // }
    if unsafe {
        dcimgui_sys::ImGui_IsItemHovered(
            dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_DelayShort
                as dcimgui_sys::ImGuiHoveredFlags,
        )
    } {
        unsafe {
            dcimgui_sys::ImGui_SetTooltip(
                c"offset=%d len=%d total=%d".as_ptr(),
                scrollbar.offset as c_int,
                scrollbar.len as c_int,
                scrollbar.total as c_int,
            )
        };
    }
}

/// Grid inspector widget for choosing and inspecting a specific cell.
// pub const CellChooser = struct {
pub struct CellChooser {
    // lookup_region: terminal.point.Tag,
    pub lookup_region: Tag,
    // lookup_coord: terminal.point.Coordinate,
    pub lookup_coord: Coordinate,
    // cell_info: CellInfo,
    pub cell_info: CellInfo,
}

impl CellChooser {
    // pub const empty: CellChooser = .{
    //     .lookup_region = .viewport,
    //     .lookup_coord = .{ .x = 0, .y = 0 },
    //     .cell_info = .empty,
    // };
    pub const EMPTY: CellChooser = CellChooser {
        lookup_region: Tag::Viewport,
        lookup_coord: Coordinate { x: 0, y: 0 },
        cell_info: CellInfo::EMPTY,
    };

    // pub fn draw(self: *CellChooser, pages: *const PageList) void {
    pub fn draw(&mut self, pages: &PageList) {
        // cimgui.c.ImGui_TextWrapped(
        //     "Inspect a cell by choosing a coordinate space and entering the X/Y position. " ++
        //         "The inspector resolves the point into the page list and displays the cell contents.",
        // );
        unsafe {
            dcimgui_sys::ImGui_TextWrapped(
                c"Inspect a cell by choosing a coordinate space and entering the X/Y position. The inspector resolves the point into the page list and displays the cell contents."
                    .as_ptr(),
            )
        };

        // cimgui.c.ImGui_SeparatorText("Cell Inspector");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Cell Inspector".as_ptr()) };

        // const region_max = maxCoord(pages, self.lookup_region);
        // if (region_max) |coord| {
        //     self.lookup_coord.x = @min(self.lookup_coord.x, coord.x);
        //     self.lookup_coord.y = @min(self.lookup_coord.y, coord.y);
        // } else {
        //     self.lookup_coord = .{ .x = 0, .y = 0 };
        // }
        let region_max = maxCoord(pages, self.lookup_region);
        if let Some(coord) = region_max {
            self.lookup_coord.x = self.lookup_coord.x.min(coord.x);
            self.lookup_coord.y = self.lookup_coord.y.min(coord.y);
        } else {
            self.lookup_coord = Coordinate { x: 0, y: 0 };
        }

        {
            // const disabled = region_max == null;
            // cimgui.c.ImGui_BeginDisabled(disabled);
            // defer cimgui.c.ImGui_EndDisabled();
            let disabled = region_max.is_none();
            unsafe { dcimgui_sys::ImGui_BeginDisabled(disabled) };
            let _end_disabled = Defer(|| unsafe { dcimgui_sys::ImGui_EndDisabled() });

            // const preview = @tagName(self.lookup_region);
            let preview = tag_name(self.lookup_region);
            // const combo_width = comptime blk: {
            //     var max_len: usize = 0;
            //     for (std.meta.tags(terminal.point.Tag)) |tag| {
            //         max_len = @max(max_len, @tagName(tag).len);
            //     }
            //     break :blk max_len + 4;
            // };
            //
            // PORT NOTE (§3 `comptime blk:`): the compile-time width scan is
            // reproduced by a `const {}` block computing the identical `max_len +
            // 4` over the verbatim `point.Tag` name lengths (active/viewport/
            // screen/history → max 8 → 12). `"X" ** combo_width` (a comptime string
            // repeat) becomes the const `X_STR` of `COMBO_WIDTH` 'X's, verified by
            // the `const _` static assert so the two stay in lockstep.
            const COMBO_WIDTH: usize = {
                let lens = [
                    b"active".len(),
                    b"viewport".len(),
                    b"screen".len(),
                    b"history".len(),
                ];
                let mut max_len: usize = 0;
                let mut i = 0;
                while i < lens.len() {
                    if lens[i] > max_len {
                        max_len = lens[i];
                    }
                    i += 1;
                }
                max_len + 4
            };
            const X_STR: &CStr = c"XXXXXXXXXXXX";
            const _: () = assert!(COMBO_WIDTH == 12);
            const _: () = assert!(X_STR.count_bytes() == COMBO_WIDTH);

            // cimgui.c.ImGui_SetNextItemWidth(cimgui.c.ImGui_CalcTextSize("X" ** combo_width).x);
            unsafe {
                dcimgui_sys::ImGui_SetNextItemWidth(
                    dcimgui_sys::ImGui_CalcTextSize(X_STR.as_ptr()).x,
                )
            };
            // if (cimgui.c.ImGui_BeginCombo(
            //     "##grid_region",
            //     preview.ptr,
            //     cimgui.c.ImGuiComboFlags_HeightSmall,
            // )) {
            if unsafe {
                dcimgui_sys::ImGui_BeginCombo(
                    c"##grid_region".as_ptr(),
                    preview.as_ptr(),
                    dcimgui_sys::ImGuiComboFlags__ImGuiComboFlags_HeightSmall
                        as dcimgui_sys::ImGuiComboFlags,
                )
            } {
                // inline for (comptime std.meta.tags(terminal.point.Tag)) |tag| {
                //     const selected = tag == self.lookup_region;
                //     if (cimgui.c.ImGui_SelectableEx(
                //         @tagName(tag).ptr,
                //         selected,
                //         cimgui.c.ImGuiSelectableFlags_None,
                //         .{ .x = 0, .y = 0 },
                //     )) {
                //         self.lookup_region = tag;
                //     }
                //     if (selected) cimgui.c.ImGui_SetItemDefaultFocus();
                // }
                //
                // PORT NOTE (§3 `inline for` hand-expansion): the compile-time loop
                // over the `point.Tag` set is unrolled over the verbatim
                // `POINT_TAGS`; each iteration is monomorphized on its concrete tag
                // (via `tag_name`), matching the reference instantiation.
                for tag in POINT_TAGS {
                    let selected = tag == self.lookup_region;
                    if unsafe {
                        dcimgui_sys::ImGui_SelectableEx(
                            tag_name(tag).as_ptr(),
                            selected,
                            dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_None
                                as dcimgui_sys::ImGuiSelectableFlags,
                            dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                        )
                    } {
                        self.lookup_region = tag;
                    }
                    if selected {
                        unsafe { dcimgui_sys::ImGui_SetItemDefaultFocus() };
                    }
                }
                // cimgui.c.ImGui_EndCombo();
                unsafe { dcimgui_sys::ImGui_EndCombo() };
            }

            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };

            // const width = cimgui.c.ImGui_CalcTextSize("00000").x;
            // var x_value: terminal.size.CellCountInt = self.lookup_coord.x;
            // var y_value: u32 = self.lookup_coord.y;
            // var changed = false;
            let width = unsafe { dcimgui_sys::ImGui_CalcTextSize(c"00000".as_ptr()) }.x;
            let mut x_value: rustty_vt::size::CellCountInt = self.lookup_coord.x;
            let mut y_value: u32 = self.lookup_coord.y;
            let mut changed = false;

            // cimgui.c.ImGui_AlignTextToFramePadding();
            // cimgui.c.ImGui_Text("x:");
            // cimgui.c.ImGui_SameLine();
            // cimgui.c.ImGui_SetNextItemWidth(width);
            // if (cimgui.c.ImGui_InputScalar("##grid_x", cimgui.c.ImGuiDataType_U16, &x_value)) changed = true;
            unsafe { dcimgui_sys::ImGui_AlignTextToFramePadding() };
            unsafe { dcimgui_sys::ImGui_Text(c"x:".as_ptr()) };
            unsafe { dcimgui_sys::ImGui_SameLine() };
            unsafe { dcimgui_sys::ImGui_SetNextItemWidth(width) };
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##grid_x".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U16 as dcimgui_sys::ImGuiDataType,
                    (&mut x_value as *mut rustty_vt::size::CellCountInt).cast(),
                )
            } {
                changed = true;
            }

            // cimgui.c.ImGui_SameLine();
            // cimgui.c.ImGui_AlignTextToFramePadding();
            // cimgui.c.ImGui_Text("y:");
            // cimgui.c.ImGui_SameLine();
            // cimgui.c.ImGui_SetNextItemWidth(width);
            // if (cimgui.c.ImGui_InputScalar("##grid_y", cimgui.c.ImGuiDataType_U32, &y_value)) changed = true;
            unsafe { dcimgui_sys::ImGui_SameLine() };
            unsafe { dcimgui_sys::ImGui_AlignTextToFramePadding() };
            unsafe { dcimgui_sys::ImGui_Text(c"y:".as_ptr()) };
            unsafe { dcimgui_sys::ImGui_SameLine() };
            unsafe { dcimgui_sys::ImGui_SetNextItemWidth(width) };
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##grid_y".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U32 as dcimgui_sys::ImGuiDataType,
                    (&mut y_value as *mut u32).cast(),
                )
            } {
                changed = true;
            }

            // cimgui.c.ImGui_SameLine();
            // widgets.helpMarker("Choose the coordinate space and X/Y position (0-indexed).");
            unsafe { dcimgui_sys::ImGui_SameLine() };
            helpMarker(c"Choose the coordinate space and X/Y position (0-indexed).");

            // if (changed) {
            //     if (region_max) |coord| {
            //         self.lookup_coord.x = @min(x_value, coord.x);
            //         self.lookup_coord.y = @min(y_value, coord.y);
            //     }
            // }
            if changed {
                if let Some(coord) = region_max {
                    self.lookup_coord.x = x_value.min(coord.x);
                    self.lookup_coord.y = y_value.min(coord.y);
                }
            }
        }

        // if (region_max) |coord| {
        //     cimgui.c.ImGui_TextDisabled("Range: x 0..%d, y 0..%d", coord.x, coord.y);
        // } else {
        //     cimgui.c.ImGui_TextDisabled("(region has no rows)");
        //     return;
        // }
        if let Some(coord) = region_max {
            unsafe {
                dcimgui_sys::ImGui_TextDisabled(
                    c"Range: x 0..%d, y 0..%d".as_ptr(),
                    coord.x as c_int,
                    coord.y as c_int,
                )
            };
        } else {
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"(region has no rows)".as_ptr()) };
            return;
        }

        // const pt = switch (self.lookup_region) {
        //     .active => terminal.Point{ .active = self.lookup_coord },
        //     .viewport => terminal.Point{ .viewport = self.lookup_coord },
        //     .screen => terminal.Point{ .screen = self.lookup_coord },
        //     .history => terminal.Point{ .history = self.lookup_coord },
        // };
        let pt = match self.lookup_region {
            Tag::Active => Point::Active(self.lookup_coord),
            Tag::Viewport => Point::Viewport(self.lookup_coord),
            Tag::Screen => Point::Screen(self.lookup_coord),
            Tag::History => Point::History(self.lookup_coord),
        };

        // const cell = pages.getCell(pt) orelse {
        //     cimgui.c.ImGui_TextDisabled("(cell out of range)");
        //     return;
        // };
        let cell = match pages.get_cell(pt) {
            Some(cell) => cell,
            None => {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(cell out of range)".as_ptr()) };
                return;
            }
        };

        // self.cell_info.draw(cell, pt);
        //
        // PORT NOTE: the reference `PageList.Cell` is passed by value; the Rust
        // `Cell` (raw-ptr aggregate, non-`Copy`) is a call-scoped borrow — passed
        // as `&Cell` throughout this widget (display-only reads under the mutex).
        self.cell_info.draw(&cell, pt);

        // if (cell.cell.style_id != stylepkg.default_id) {
        //     cimgui.c.ImGui_SeparatorText("Style");
        //     const style = cell.node.data.styles.get(cell.node.data.memory, cell.cell.style_id).*;
        //     widgets.style.table(style, null);
        // }
        if unsafe { (*cell.cell).style_id() } != stylepkg::DEFAULT_ID {
            unsafe { dcimgui_sys::ImGui_SeparatorText(c"Style".as_ptr()) };
            let style = unsafe {
                *(*rustty_vt::pagelist::Node::page(cell.node))
                    .styles
                    .get((*rustty_vt::pagelist::Node::page(cell.node)).memory_ptr(), (*cell.cell).style_id())
            };
            super::style::table(style, None);
        }

        // if (cell.cell.hyperlink) {
        //     cimgui.c.ImGui_SeparatorText("Hyperlink");
        //     hyperlinkTable(cell);
        // }
        if unsafe { (*cell.cell).hyperlink() } {
            unsafe { dcimgui_sys::ImGui_SeparatorText(c"Hyperlink".as_ptr()) };
            hyperlinkTable(&cell);
        }

        // if (cell.cell.hasGrapheme()) {
        //     cimgui.c.ImGui_SeparatorText("Grapheme");
        //     graphemeTable(cell);
        // }
        if unsafe { (*cell.cell).has_grapheme() } {
            unsafe { dcimgui_sys::ImGui_SeparatorText(c"Grapheme".as_ptr()) };
            graphemeTable(&cell);
        }
    }
}

// fn maxCoord(
//     pages: *const PageList,
//     tag: terminal.point.Tag,
// ) ?terminal.point.Coordinate {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn maxCoord(pages: &PageList, tag: Tag) -> Option<Coordinate> {
    // const br_pin = pages.getBottomRight(tag) orelse return null;
    // const br_point = pages.pointFromPin(tag, br_pin) orelse return null;
    // return br_point.coord();
    let br_pin = pages.get_bottom_right(tag)?;
    let br_point = pages.point_from_pin(tag, br_pin)?;
    Some(br_point.coord())
}

// fn hyperlinkTable(cell: PageList.Cell) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn hyperlinkTable(cell: &Cell) {
    // if (!cimgui.c.ImGui_BeginTable("cell_hyperlink", 2, cimgui.c.ImGuiTableFlags_None)) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"cell_hyperlink".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // const page = &cell.node.data;
    let page = unsafe { &(*rustty_vt::pagelist::Node::page(cell.node)) };
    let base = page.memory_ptr();
    // const link_id = page.lookupHyperlink(cell.cell) orelse {
    //     cimgui.c.ImGui_TableNextRow();
    //     _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    //     cimgui.c.ImGui_Text("Status");
    //     _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    //     cimgui.c.ImGui_TextDisabled("(missing link data)");
    //     return;
    // };
    let link_id = match page.lookup_hyperlink(cell.cell) {
        Some(id) => id,
        None => {
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Status".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"(missing link data)".as_ptr()) };
            return;
        }
    };

    // const entry = page.hyperlink_set.get(page.memory, link_id);
    let entry = unsafe { &*page.hyperlink_set.get(base, link_id) };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("ID");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"ID".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // switch (entry.id) {
    //     .implicit => |value| cimgui.c.ImGui_Text("implicit %d", value),
    //     .explicit => |slice| {
    //         const id = slice.slice(page.memory);
    //         if (id.len == 0) {
    //             cimgui.c.ImGui_TextDisabled("(empty)");
    //         } else {
    //             cimgui.c.ImGui_Text("%.*s", id.len, id.ptr);
    //         }
    //     },
    // }
    match entry.id {
        PageEntryId::Implicit(value) => {
            unsafe { dcimgui_sys::ImGui_Text(c"implicit %d".as_ptr(), value as c_int) };
        }
        PageEntryId::Explicit(slice) => {
            let id = unsafe { slice.slice(base) };
            if id.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(empty)".as_ptr()) };
            } else {
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%.*s".as_ptr(),
                        id.len() as c_int,
                        id.as_ptr() as *const c_char,
                    )
                };
            }
        }
    }

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("URI");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // const uri = entry.uri.slice(page.memory);
    // if (uri.len == 0) {
    //     cimgui.c.ImGui_TextDisabled("(empty)");
    // } else {
    //     cimgui.c.ImGui_Text("%.*s", uri.len, uri.ptr);
    // }
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"URI".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    let uri = unsafe { entry.uri.slice(base) };
    if uri.is_empty() {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(empty)".as_ptr()) };
    } else {
        unsafe {
            dcimgui_sys::ImGui_Text(
                c"%.*s".as_ptr(),
                uri.len() as c_int,
                uri.as_ptr() as *const c_char,
            )
        };
    }

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Ref Count");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // const refs = page.hyperlink_set.refCount(page.memory, link_id);
    // cimgui.c.ImGui_Text("%d", refs);
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Ref Count".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    let refs = unsafe { page.hyperlink_set.ref_count(base, link_id) };
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), refs as c_int) };
}

// fn graphemeTable(cell: PageList.Cell) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn graphemeTable(cell: &Cell) {
    // if (!cimgui.c.ImGui_BeginTable("cell_grapheme", 2, cimgui.c.ImGuiTableFlags_None)) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"cell_grapheme".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // const page = &cell.node.data;
    // const cps = page.lookupGrapheme(cell.cell) orelse {
    //     cimgui.c.ImGui_TableNextRow();
    //     _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    //     cimgui.c.ImGui_Text("Status");
    //     _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    //     cimgui.c.ImGui_TextDisabled("(missing grapheme data)");
    //     return;
    // };
    let page = unsafe { &(*rustty_vt::pagelist::Node::page(cell.node)) };
    let cps = match page.lookup_grapheme(cell.cell) {
        Some(cps) => cps,
        None => {
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Status".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            unsafe { dcimgui_sys::ImGui_TextDisabled(c"(missing grapheme data)".as_ptr()) };
            return;
        }
    };

    // cimgui.c.ImGui_TableNextRow();
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    // cimgui.c.ImGui_Text("Extra Codepoints");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    // if (cps.len == 0) {
    //     cimgui.c.ImGui_TextDisabled("(none)");
    //     return;
    // }
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    unsafe { dcimgui_sys::ImGui_Text(c"Extra Codepoints".as_ptr()) };
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    if cps.is_empty() {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr()) };
        return;
    }

    // var buf: [96]u8 = undefined;
    // if (cimgui.c.ImGui_BeginListBox("##grapheme_list", .{ .x = 0, .y = 0 })) {
    //     defer cimgui.c.ImGui_EndListBox();
    //     for (cps) |cp| {
    //         const label = std.fmt.bufPrintZ(&buf, "U+{X}", .{cp}) catch "U+?";
    //         _ = cimgui.c.ImGui_SelectableEx(label.ptr, false, cimgui.c.ImGuiSelectableFlags_None, .{ .x = 0, .y = 0 });
    //     }
    // }
    let mut buf: [u8; 96] = [0; 96];
    if unsafe {
        dcimgui_sys::ImGui_BeginListBox(
            c"##grapheme_list".as_ptr(),
            dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
        )
    } {
        let _end_list_box = Defer(|| unsafe { dcimgui_sys::ImGui_EndListBox() });
        for &cp in cps {
            let label = cp_label(&mut buf, cp);
            let _ = unsafe {
                dcimgui_sys::ImGui_SelectableEx(
                    label,
                    false,
                    dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_None
                        as dcimgui_sys::ImGuiSelectableFlags,
                    dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                )
            };
        }
    }
}

/// `std.fmt.bufPrintZ(&buf, "U+{X}", .{cp}) catch "U+?"` analogue: writes the
/// uppercase-hex codepoint label NUL-terminated into `buf`, returning the
/// `*const c_char`; on overflow it falls back to `c"U+?"`. The returned pointer
/// aliases `buf`, so `buf` must outlive its use.
fn cp_label(buf: &mut [u8; 96], cp: u32) -> *const c_char {
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
        // "U+{X}" — uppercase hex, no zero-padding.
        if write!(w, "U+{:X}", cp).is_err() {
            return c"U+?".as_ptr();
        }
        w.pos
    };
    buf[pos] = 0;
    buf.as_ptr() as *const c_char
}

/// Cell inspector widget.
// pub const CellInfo = struct {
pub struct CellInfo {}

impl CellInfo {
    // pub const empty: CellInfo = .{};
    pub const EMPTY: CellInfo = CellInfo {};

    // pub fn draw(
    //     _: *const CellInfo,
    //     cell: PageList.Cell,
    //     point: terminal.Point,
    // ) void {
    //
    // PORT NOTE: `cell` is the non-`Copy` raw-ptr aggregate → `&Cell`; `point`
    // (`terminal.Point`, a `Copy` enum) is passed by value.
    pub fn draw(&self, cell: &Cell, point: Point) {
        // if (!cimgui.c.ImGui_BeginTable(
        //     "cell_info",
        //     3,
        //     cimgui.c.ImGuiTableFlags_BordersInnerV |
        //         cimgui.c.ImGuiTableFlags_RowBg |
        //         cimgui.c.ImGuiTableFlags_SizingFixedFit,
        // )) return;
        if !unsafe {
            dcimgui_sys::ImGui_BeginTable(
                c"cell_info".as_ptr(),
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

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Grid Position");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("The cell's X/Y coordinates in the selected region.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // const coord = point.coord();
            // cimgui.c.ImGui_Text("(%d, %d)", coord.x, coord.y);
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Grid Position".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"The cell's X/Y coordinates in the selected region.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let coord = point.coord();
            unsafe {
                dcimgui_sys::ImGui_Text(c"(%d, %d)".as_ptr(), coord.x as c_int, coord.y as c_int)
            };
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Page Location");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Row and column indices within the backing page.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // cimgui.c.ImGui_Text("row=%d col=%d", cell.row_idx, cell.col_idx);
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Page Location".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Row and column indices within the backing page.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"row=%d col=%d".as_ptr(),
                    cell.row_idx as c_int,
                    cell.col_idx as c_int,
                )
            };
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Content");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Content tag describing how the cell data is stored.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // cimgui.c.ImGui_Text("%s", @tagName(cell.cell.content_tag).ptr);
            //
            // PORT NOTE (§3 `@tagName`): the `Cell.ContentTag` name string is
            // produced by a match over the variants (codepoint /
            // codepoint_grapheme / bg_color_palette / bg_color_rgb).
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Content".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Content tag describing how the cell data is stored.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let content_tag_name: &CStr = match unsafe { (*cell.cell).content_tag() } {
                rustty_vt::page::ContentTag::Codepoint => c"codepoint",
                rustty_vt::page::ContentTag::CodepointGrapheme => c"codepoint_grapheme",
                rustty_vt::page::ContentTag::BgColorPalette => c"bg_color_palette",
                rustty_vt::page::ContentTag::BgColorRgb => c"bg_color_rgb",
            };
            unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), content_tag_name.as_ptr()) };
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Codepoint");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Primary Unicode codepoint for the cell.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // const cp = cell.cell.codepoint();
            // if (cp == 0) {
            //     cimgui.c.ImGui_TextDisabled("(empty)");
            // } else {
            //     cimgui.c.ImGui_Text("U+%04X", @as(u32, cp));
            // }
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Codepoint".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Primary Unicode codepoint for the cell.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let cp = unsafe { (*cell.cell).codepoint() };
            if cp == 0 {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(empty)".as_ptr()) };
            } else {
                unsafe { dcimgui_sys::ImGui_Text(c"U+%04X".as_ptr(), cp as c_uint) };
            }
        }

        // if (cell.cell.hasGrapheme()) {
        if unsafe { (*cell.cell).has_grapheme() } {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Grapheme");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Extra codepoints that combine with the primary codepoint to form the grapheme cluster.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Grapheme".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(
                c"Extra codepoints that combine with the primary codepoint to form the grapheme cluster.",
            );
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            // if (cimgui.c.ImGui_BeginListBox("##cell_grapheme", .{ .x = 0, .y = 0 })) {
            //     defer cimgui.c.ImGui_EndListBox();
            //     if (cell.node.data.lookupGrapheme(cell.cell)) |cps| {
            //         var buf: [96]u8 = undefined;
            //         for (cps) |cp| {
            //             const label = std.fmt.bufPrintZ(&buf, "U+{X}", .{cp}) catch "U+?";
            //             _ = cimgui.c.ImGui_SelectableEx(label.ptr, false, cimgui.c.ImGuiSelectableFlags_None, .{ .x = 0, .y = 0 });
            //         }
            //     } else {
            //         _ = cimgui.c.ImGui_SelectableEx("(missing)", false, cimgui.c.ImGuiSelectableFlags_None, .{ .x = 0, .y = 0 });
            //     }
            // }
            if unsafe {
                dcimgui_sys::ImGui_BeginListBox(
                    c"##cell_grapheme".as_ptr(),
                    dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                )
            } {
                let _end_list_box = Defer(|| unsafe { dcimgui_sys::ImGui_EndListBox() });
                if let Some(cps) = unsafe { (*rustty_vt::pagelist::Node::page(cell.node)).lookup_grapheme(cell.cell) } {
                    let mut buf: [u8; 96] = [0; 96];
                    for &cp in cps {
                        let label = cp_label(&mut buf, cp);
                        let _ = unsafe {
                            dcimgui_sys::ImGui_SelectableEx(
                                label,
                                false,
                                dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_None
                                    as dcimgui_sys::ImGuiSelectableFlags,
                                dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                            )
                        };
                    }
                } else {
                    let _ = unsafe {
                        dcimgui_sys::ImGui_SelectableEx(
                            c"(missing)".as_ptr(),
                            false,
                            dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_None
                                as dcimgui_sys::ImGuiSelectableFlags,
                            dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                        )
                    };
                }
            }
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Width Property");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Character width property (narrow, wide, spacer, etc.).");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // cimgui.c.ImGui_Text("%s", @tagName(cell.cell.wide).ptr);
            //
            // PORT NOTE (§3 `@tagName`): the `Cell.Wide` name string is produced by
            // a match over the variants (narrow / wide / spacer_tail / spacer_head).
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Width Property".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Character width property (narrow, wide, spacer, etc.).");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let wide_name: &CStr = match unsafe { (*cell.cell).wide() } {
                rustty_vt::page::Wide::Narrow => c"narrow",
                rustty_vt::page::Wide::Wide => c"wide",
                rustty_vt::page::Wide::SpacerTail => c"spacer_tail",
                rustty_vt::page::Wide::SpacerHead => c"spacer_head",
            };
            unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), wide_name.as_ptr()) };
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Row Flags");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Flags set on the row containing this cell.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // const row = cell.row;
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Row Flags".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Flags set on the row containing this cell.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let row = unsafe { *cell.row };
            // if (row.wrap or row.wrap_continuation or row.grapheme or row.styled or row.hyperlink) {
            if row.wrap()
                || row.wrap_continuation()
                || row.grapheme()
                || row.styled()
                || row.hyperlink()
            {
                // if (row.wrap) { cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.8, .z = 1.0, .w = 1.0 }, "wrap"); cimgui.c.ImGui_SameLine(); }
                if row.wrap() {
                    unsafe {
                        dcimgui_sys::ImGui_TextColored(
                            dcimgui_sys::ImVec4 {
                                x: 0.4,
                                y: 0.8,
                                z: 1.0,
                                w: 1.0,
                            },
                            c"wrap".as_ptr(),
                        )
                    };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (row.wrap_continuation) { cimgui.c.ImGui_TextColored(.{ .x = 0.4, .y = 0.8, .z = 1.0, .w = 1.0 }, "cont"); cimgui.c.ImGui_SameLine(); }
                if row.wrap_continuation() {
                    unsafe {
                        dcimgui_sys::ImGui_TextColored(
                            dcimgui_sys::ImVec4 {
                                x: 0.4,
                                y: 0.8,
                                z: 1.0,
                                w: 1.0,
                            },
                            c"cont".as_ptr(),
                        )
                    };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (row.grapheme) { cimgui.c.ImGui_TextColored(.{ .x = 0.9, .y = 0.7, .z = 0.3, .w = 1.0 }, "grapheme"); cimgui.c.ImGui_SameLine(); }
                if row.grapheme() {
                    unsafe {
                        dcimgui_sys::ImGui_TextColored(
                            dcimgui_sys::ImVec4 {
                                x: 0.9,
                                y: 0.7,
                                z: 0.3,
                                w: 1.0,
                            },
                            c"grapheme".as_ptr(),
                        )
                    };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (row.styled) { cimgui.c.ImGui_TextColored(.{ .x = 0.7, .y = 0.9, .z = 0.5, .w = 1.0 }, "styled"); cimgui.c.ImGui_SameLine(); }
                if row.styled() {
                    unsafe {
                        dcimgui_sys::ImGui_TextColored(
                            dcimgui_sys::ImVec4 {
                                x: 0.7,
                                y: 0.9,
                                z: 0.5,
                                w: 1.0,
                            },
                            c"styled".as_ptr(),
                        )
                    };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (row.hyperlink) { cimgui.c.ImGui_TextColored(.{ .x = 0.8, .y = 0.6, .z = 1.0, .w = 1.0 }, "link"); cimgui.c.ImGui_SameLine(); }
                if row.hyperlink() {
                    unsafe {
                        dcimgui_sys::ImGui_TextColored(
                            dcimgui_sys::ImVec4 {
                                x: 0.8,
                                y: 0.6,
                                z: 1.0,
                                w: 1.0,
                            },
                            c"link".as_ptr(),
                        )
                    };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
            } else {
                // cimgui.c.ImGui_TextDisabled("(none)");
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr()) };
            }
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Style ID");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Internal style reference ID for this cell.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // cimgui.c.ImGui_Text("%d", cell.cell.style_id);
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Style ID".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Internal style reference ID for this cell.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let style_id = unsafe { (*cell.cell).style_id() };
            unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), style_id as c_int) };
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Style");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("Resolved style for the cell (colors, attributes, etc.).");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // if (cell.cell.style_id == stylepkg.default_id) {
            //     cimgui.c.ImGui_TextDisabled("(default)");
            // } else {
            //     cimgui.c.ImGui_TextDisabled("(see below)");
            // }
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Style".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"Resolved style for the cell (colors, attributes, etc.).");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            if unsafe { (*cell.cell).style_id() } == stylepkg::DEFAULT_ID {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(default)".as_ptr()) };
            } else {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(see below)".as_ptr()) };
            }
        }

        // if (cell.cell.hyperlink) {
        if unsafe { (*cell.cell).hyperlink() } {
            // cimgui.c.ImGui_TableNextRow();
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            // cimgui.c.ImGui_Text("Hyperlink");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            // widgets.helpMarker("OSC8 hyperlink ID associated with this cell.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            // const link_id = cell.node.data.lookupHyperlink(cell.cell) orelse 0;
            // cimgui.c.ImGui_Text("id=%d", link_id);
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            unsafe { dcimgui_sys::ImGui_Text(c"Hyperlink".as_ptr()) };
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            helpMarker(c"OSC8 hyperlink ID associated with this cell.");
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            let link_id = unsafe { (*rustty_vt::pagelist::Node::page(cell.node)).lookup_hyperlink(cell.cell) }.unwrap_or(0);
            unsafe { dcimgui_sys::ImGui_Text(c"id=%d".as_ptr(), link_id as c_int) };
        }
    }
}
