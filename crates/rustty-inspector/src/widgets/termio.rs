//! VT event stream inspector widget.
//!
//! PORTS: ghostty src/inspector/widgets/termio.zig (the design-heaviest widget).
//!
//! PORT STATUS: LIVE (Task 16). This module carries the three design obligations
//! the plan gates on termio:
//!
//!  * **STEP 1 — the `@typeInfo` metadata-encoder hand-expansion (§3/S10).** The
//!    reference `encodeMetadata`/`encodeMetadataSingle` (termio.zig:575-687) switch
//!    on `@typeInfo(T)` over arbitrary OSC payloads. Rust has no `@typeInfo`, so
//!    [`VTEvent::encode_metadata_osc`] + the `encode_metadata_single_*` leaf
//!    helpers below hand-expand the visitor over EVERY concrete `osc::Command`
//!    variant, faithful to the reference's per-type `key=value` output (the
//!    `.optional`/`.bool`/`.@"enum"`→`@tagName`/`.@"union"`/`.@"struct"`→`@typeName`/
//!    `.int` arms).
//!  * **STEP 2 — the R1 `VTHandler.State.events` self-alias.** `recordPtyRead`
//!    wires `&raw mut self.events` (a raw `*mut VTEvent::Ring`, NEVER a `&mut`
//!    reference) into the handler and drives the disjoint sibling
//!    `parser_stream`; `vt_raw` appends through that raw alias. Proven SB/TB-clean
//!    on `.port/phase-2/mirihar-inspector-vthandler` (the shipped raw-self routing
//!    is CLEAN; the naive `&mut Ring` reference alias is UB) — the
//!    [[shaper-hook-sb-clean-interleave]] doctrine.
//!  * **STEP 3 — the faithful port**: the `Stream<VTHandler>` parser (the handler
//!    impls the rustty-vt `Handler` trait, §3), the `VTEvent` ring (INV-9 eviction:
//!    element drop before slot reuse), the arena-backed `VTEvent` (its
//!    raw_description/metadata own their bytes; INV-8 snapshots cursor/
//!    scrolling_region BY VALUE at event time), and the R4 selection sync (the
//!    reverse iterator MUTATES `ev.imgui_selected` during draw).
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11). Every `Terminal` deref happens under `surface.renderer_state.mutex`
//! (INV-1) and only inside the `recordPtyRead` capture window (INV-8).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const cimgui = @import("dcimgui");
// const terminal = @import("../../terminal/main.zig");
// const CircBuf = @import("../../datastruct/main.zig").CircBuf;
// const Surface = @import("../../Surface.zig");
// const screen = @import("screen.zig");
//
// PORT NOTE (screen.rs/key.rs precedent): `dcimgui` is reached via full
// `dcimgui_sys::` paths; `terminal.*` types live in rustty-vt; `CircBuf` is
// `rustty_datastruct::circ_buf::CircBuf`; `screen` is the sibling widget module
// (`super::screen`, its `cursorTable`/`cursorStyle` are LIVE, Task 11).
use core::ffi::{c_char, c_int, c_void, CStr};
use core::fmt::Write as _;

use allocator_api2::alloc::Global as Allocator;

use rustty_datastruct::circ_buf::{CircBuf, Direction};
use rustty_vt::color::Palette;
use rustty_vt::osc::Command as OscCommand;
use rustty_vt::page::SemanticContent;
use rustty_vt::parser::Action;
use rustty_vt::screen::{Cursor, CursorStyle};
use rustty_vt::stream::{Handler, Stream as VtStream};
use rustty_vt::style;
use rustty_vt::terminal::{ScrollingRegion, Terminal};

use super::screen;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`screen.rs`). panic=abort means no unwinding ever
/// crosses these FFI calls, so drop-on-scope-exit is the sole runtime path.
struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

// The process-global backing allocator for the inspector parser stream. Mirrors
// `Stream.initAlloc(alloc, handler)` (termio.zig:33): a single always-available
// allocator threaded into the OSC parser so large OSC captures can grow. Being a
// `'static` ZST it makes `parser_stream` a `Stream<'static, VTHandler>`, so the
// widget carries no lifetime parameter.
static TERMIO_STREAM_ALLOC: rustty_lib::allocator::GlobalZigAllocator =
    rustty_lib::allocator::GlobalZigAllocator;

/// VT event stream inspector widget.
// pub const Stream = struct {
pub struct Stream {
    // events: VTEvent.Ring,
    /// The event ring. R1: `recordPtyRead` takes a raw `*mut` alias of THIS field
    /// and appends through it while the disjoint sibling `parser_stream` is
    /// `&mut`-borrowed (proven SB/TB-clean, see the module doc STEP 2).
    events: VTEventRing,

    // parser_stream: VTHandler.Stream,
    parser_stream: VtStream<'static, VTHandler>,

    // selected_event_seq: ?u32 = null,
    /// The currently selected event sequence number for keyboard navigation
    selected_event_seq: Option<u32>,

    // need_scroll_to_selected: bool = false,
    /// Flag indicating whether we need to scroll to the selected item
    need_scroll_to_selected: bool,

    // is_keyboard_selection: bool = false,
    /// Flag indicating whether the selection was made by keyboard
    is_keyboard_selection: bool,
}

impl Stream {
    // pub fn init(alloc: Allocator) !Stream {
    //
    // PORT NOTE: under Rust's infallible global allocator the `!` collapses (OOM
    // aborts — panic=abort parity), so `init` is infallible. `alloc` retained for
    // call-shape parity. The reference `errdefer`s are subsumed by Rust `Drop`.
    pub fn init(_alloc: Allocator) -> Stream {
        // var events: VTEvent.Ring = try .init(alloc, 2);
        let events = VTEventRing::init(2, VTEvent::default_fill())
            .expect("VTEvent.Ring.init(2) is infallible under the global allocator");

        // var handler: VTHandler = .init;
        let handler = VTHandler::init();

        // return .{
        //     .events = events,
        //     .parser_stream = .init(.{
        //         .allocator = alloc,
        //         .handler = handler,
        //     }),
        // };
        Stream {
            events,
            parser_stream: VtStream::init(rustty_vt::stream::Options {
                allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                    &TERMIO_STREAM_ALLOC,
                )),
                handler,
                continuation_max_bytes: None,
            }),
            selected_event_seq: None,
            need_scroll_to_selected: false,
            is_keyboard_selection: false,
        }
    }

    // pub fn deinit(self: *Stream, alloc: Allocator) void {
    pub fn deinit(&mut self, alloc: Allocator) {
        // var it = self.events.iterator(.forward);
        // while (it.next()) |v| v.deinit(alloc);
        //
        // PORT NOTE (INV-9): each stored `VTEvent` owns its raw_description +
        // metadata bytes; they free on `Drop` when the ring drops. The explicit
        // per-element `deinit` is preserved for the call-shape (a no-op — the
        // arena→Rust-ownership collapse, mirroring lib.rs's VTEvent note).
        {
            let mut it = self.events.iterator(Direction::Forward);
            while let Some(v) = it.next() {
                v.deinit(alloc);
            }
        }
        // self.events.deinit(alloc);   — the ring (and its Events) drop with self.
        // self.parser_stream.deinit();
        self.parser_stream.deinit();
    }

    // pub fn recordPtyRead(self: *Stream, alloc, t: *terminal.Terminal, data: []const u8) !void {
    //
    // R1 (STEP 2). INV-1/INV-8: `t` is a raw `*mut Terminal` valid only for this
    // call, under the surface mutex. The capture window is exactly this fn.
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn recordPtyRead(&mut self, alloc: Allocator, t: *mut Terminal, data: &[u8]) {
        // self.parser_stream.handler.state = .{
        //     .alloc = alloc, .terminal = t, .events = &self.events,
        // };
        //
        // R1: `&raw mut self.events` — a RAW `*mut VTEventRing` alias of the
        // sibling field, NEVER a `&mut` reference (that would be UB across the
        // drive; mirihar-inspector-vthandler proves both). The drive below borrows
        // only the disjoint `parser_stream`, so the alias keeps its provenance for
        // the whole window.
        let events_ptr: *mut VTEventRing = &raw mut self.events;
        self.parser_stream.handler.state = Some(State {
            alloc,
            terminal: t as *const Terminal,
            events: events_ptr,
        });

        // defer self.parser_stream.handler.state = null;
        // self.parser_stream.nextSlice(data);
        //
        // PORT NOTE: Rust has no `defer`; we clear the capture state after the
        // drive on the single (panic=abort ⇒ non-unwinding) return path.
        self.parser_stream.next_slice(data);
        self.parser_stream.handler.state = None;
    }

    // pub fn draw(self: *Stream, alloc: Allocator, palette: *const terminal.color.Palette) void {
    pub fn draw(&mut self, alloc: Allocator, palette: &Palette) {
        // const events = &self.events;
        // const handler = &self.parser_stream.handler;
        // const popup_filter = "Filter";
        let popup_filter: &CStr = c"Filter";

        // Controls
        {
            // const pause_play: [:0]const u8 = if (!handler.paused) "Pause##pause_play" else "Resume##pause_play";
            let pause_play: &CStr = if !self.parser_stream.handler.paused {
                c"Pause##pause_play"
            } else {
                c"Resume##pause_play"
            };
            // if (cimgui.c.ImGui_Button(pause_play.ptr)) handler.paused = !handler.paused;
            if unsafe { dcimgui_sys::ImGui_Button(pause_play.as_ptr()) } {
                self.parser_stream.handler.paused = !self.parser_stream.handler.paused;
            }

            // cimgui.c.ImGui_SameLineEx(0, cimgui.c.ImGui_GetStyle().*.ItemInnerSpacing.x);
            unsafe {
                dcimgui_sys::ImGui_SameLineEx(
                    0.0,
                    (*dcimgui_sys::ImGui_GetStyle()).ItemInnerSpacing.x,
                )
            };
            // if (cimgui.c.ImGui_Button("Filter")) cimgui.c.ImGui_OpenPopup(popup_filter, .None);
            if unsafe { dcimgui_sys::ImGui_Button(c"Filter".as_ptr()) } {
                unsafe {
                    dcimgui_sys::ImGui_OpenPopup(
                        popup_filter.as_ptr(),
                        dcimgui_sys::ImGuiPopupFlags__ImGuiPopupFlags_None
                            as dcimgui_sys::ImGuiPopupFlags,
                    )
                };
            }

            // if (!events.empty()) { ... "Clear" ... }
            if !self.events.empty() {
                unsafe {
                    dcimgui_sys::ImGui_SameLineEx(
                        0.0,
                        (*dcimgui_sys::ImGui_GetStyle()).ItemInnerSpacing.x,
                    )
                };
                if unsafe { dcimgui_sys::ImGui_Button(c"Clear".as_ptr()) } {
                    // var it = events.iterator(.forward);
                    // while (it.next()) |v| v.deinit(alloc);
                    {
                        let mut it = self.events.iterator(Direction::Forward);
                        while let Some(v) = it.next() {
                            v.deinit(alloc);
                        }
                    }
                    // events.clear();
                    self.events.clear();
                    // handler.current_seq = 1;
                    self.parser_stream.handler.current_seq = 1;
                }
            }
        }

        // Events Table
        // if (events.empty()) cimgui.c.ImGui_Text("Waiting for events...") else { /* TODO eventTable */ }
        if self.events.empty() {
            unsafe { dcimgui_sys::ImGui_Text(c"Waiting for events...".as_ptr()) };
        } else {
            // TODO: Eventually
            // eventTable(events);
        }

        {
            // cimgui.c.ImGui_Separator();
            unsafe { dcimgui_sys::ImGui_Separator() };

            // _ = cimgui.c.ImGui_BeginTable("table_vt_events", 3, RowBg | Borders);
            unsafe {
                dcimgui_sys::ImGui_BeginTable(
                    c"table_vt_events".as_ptr(),
                    3,
                    (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg
                        | dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_Borders)
                        as dcimgui_sys::ImGuiTableFlags,
                )
            };
            // defer cimgui.c.ImGui_EndTable();
            let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

            // cimgui.c.ImGui_TableSetupColumn("Seq", WidthFixed);
            unsafe {
                dcimgui_sys::ImGui_TableSetupColumn(
                    c"Seq".as_ptr(),
                    dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                        as dcimgui_sys::ImGuiTableColumnFlags,
                )
            };
            // cimgui.c.ImGui_TableSetupColumn("Kind", WidthFixed);
            unsafe {
                dcimgui_sys::ImGui_TableSetupColumn(
                    c"Kind".as_ptr(),
                    dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                        as dcimgui_sys::ImGuiTableColumnFlags,
                )
            };
            // cimgui.c.ImGui_TableSetupColumn("Description", WidthStretch);
            unsafe {
                dcimgui_sys::ImGui_TableSetupColumn(
                    c"Description".as_ptr(),
                    dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                        as dcimgui_sys::ImGuiTableColumnFlags,
                )
            };

            // Handle keyboard navigation when window is focused
            // if (cimgui.c.ImGui_IsWindowFocused(RootAndChildWindows)) { ... }
            if unsafe {
                dcimgui_sys::ImGui_IsWindowFocused(
                    dcimgui_sys::ImGuiFocusedFlags__ImGuiFocusedFlags_RootAndChildWindows
                        as dcimgui_sys::ImGuiFocusedFlags,
                )
            } {
                self.handle_keyboard_nav();
            }

            // var it = events.iterator(.reverse);
            // while (it.next()) |ev| { ... ev.imgui_selected = ... }
            //
            // R4 (STEP 3): the reverse iterator MUTATES `ev.imgui_selected` during
            // draw. rustty-datastruct's `CircBuf::Iterator` is read-only, so we
            // drive the reverse walk by INDEX and take a fresh `&mut` into
            // `events.storage[..]` per row (the "index-based mut path", plan Task
            // 16 / risk R4). The selection state (`selected_event_seq` etc.) is
            // read into locals first so the per-row `&mut self.events` borrow does
            // not conflict with the sibling `self.selected_*` fields.
            let mut sel = self.selected_event_seq;
            let mut is_kbd = self.is_keyboard_selection;
            let mut need_scroll = self.need_scroll_to_selected;

            let n = self.events.len();
            for i in 0..n {
                // reverse order: newest (tail_idx = len-1) first.
                let tail_idx = n - 1 - i;
                let storage_idx = (self.events.tail + tail_idx) % self.events.capacity();
                let ev: &mut VTEvent = &mut self.events.storage[storage_idx];

                // Need to push an ID so that our selectable is unique.
                // cimgui.c.ImGui_PushIDPtr(ev);
                unsafe { dcimgui_sys::ImGui_PushIDPtr(ev as *const VTEvent as *const c_void) };
                // defer cimgui.c.ImGui_PopID();
                let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

                // cimgui.c.ImGui_TableNextRow();
                unsafe { dcimgui_sys::ImGui_TableNextRow() };
                // _ = cimgui.c.ImGui_TableNextColumn();
                let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };

                // Store the previous selection state to detect changes
                // const was_selected = ev.imgui_selected;
                let was_selected = ev.imgui_selected;

                // Update selection state based on keyboard navigation
                // if (self.selected_event_seq) |seq| ev.imgui_selected = (ev.seq == seq);
                if let Some(seq) = sel {
                    ev.imgui_selected = (ev.seq as u32) == seq;
                }

                // Handle selectable widget
                // If selection state changed, update keyboard navigation state
                // if (cimgui.c.ImGui_SelectableBoolPtr("##select", &ev.imgui_selected, SpanAllColumns)) {
                if unsafe {
                    dcimgui_sys::ImGui_SelectableBoolPtr(
                        c"##select".as_ptr(),
                        &mut ev.imgui_selected,
                        dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_SpanAllColumns
                            as dcimgui_sys::ImGuiSelectableFlags,
                    )
                } {
                    // if (ev.imgui_selected != was_selected) {
                    //     self.selected_event_seq = if (ev.imgui_selected) ev.seq else null;
                    //     self.is_keyboard_selection = false;
                    // }
                    if ev.imgui_selected != was_selected {
                        sel = if ev.imgui_selected {
                            Some(ev.seq as u32)
                        } else {
                            None
                        };
                        is_kbd = false;
                    }
                }

                // cimgui.c.ImGui_SameLine();
                unsafe { dcimgui_sys::ImGui_SameLine() };
                // cimgui.c.ImGui_Text("%d", ev.seq);
                unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), ev.seq as c_int) };
                // _ = cimgui.c.ImGui_TableNextColumn();
                let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };
                // cimgui.c.ImGui_Text("%s", @tagName(ev.kind).ptr);
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%s".as_ptr(),
                        ev.kind.tag_name().as_ptr() as *const c_char,
                    )
                };
                // _ = cimgui.c.ImGui_TableNextColumn();
                let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };
                // cimgui.c.ImGui_Text("%s", ev.raw_description.ptr);
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%s".as_ptr(),
                        ev.raw_description.as_ptr() as *const c_char,
                    )
                };

                // If the event is selected, we render info about it. For now
                // we put this in the last column because that's the widest and
                // imgui has no way to make a column span.
                // if (ev.imgui_selected) { ... }
                if ev.imgui_selected {
                    {
                        // screen.cursorTable(&ev.cursor);
                        screen::cursorTable(&ev.cursor);
                        // screen.cursorStyle(&ev.cursor, palette);
                        screen::cursorStyle(&ev.cursor, Some(palette));

                        // _ = cimgui.c.ImGui_BeginTable("details", 2, None);
                        unsafe {
                            dcimgui_sys::ImGui_BeginTable(
                                c"details".as_ptr(),
                                2,
                                dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None
                                    as dcimgui_sys::ImGuiTableFlags,
                            )
                        };
                        // defer cimgui.c.ImGui_EndTable();
                        let _end_details = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });
                        {
                            // cimgui.c.ImGui_TableNextRow();
                            unsafe { dcimgui_sys::ImGui_TableNextRow() };
                            {
                                // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
                                let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
                                // cimgui.c.ImGui_Text("Scroll Region");
                                unsafe { dcimgui_sys::ImGui_Text(c"Scroll Region".as_ptr()) };
                            }
                            {
                                // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
                                let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
                                // cimgui.c.ImGui_Text("T=%d B=%d L=%d R=%d", top, bottom, left, right);
                                unsafe {
                                    dcimgui_sys::ImGui_Text(
                                        c"T=%d B=%d L=%d R=%d".as_ptr(),
                                        ev.scrolling_region.top as c_int,
                                        ev.scrolling_region.bottom as c_int,
                                        ev.scrolling_region.left as c_int,
                                        ev.scrolling_region.right as c_int,
                                    )
                                };
                            }
                        }

                        // var md_it = ev.metadata.iterator();
                        // while (md_it.next()) |entry| { ... }
                        for (key_z, value_z) in ev.metadata.iter() {
                            // var buf: [256]u8; const key = bufPrintZ(&buf, "{s}", entry.key_ptr.*);
                            //
                            // PORT NOTE: our keys are already NUL-terminated byte
                            // vecs (`dupe_z`), so the reference's re-termination
                            // into a `[256]` scratch is subsumed — we hand the
                            // stored key pointer directly to ImGui.
                            // cimgui.c.ImGui_TableNextRow();
                            unsafe { dcimgui_sys::ImGui_TableNextRow() };
                            // _ = cimgui.c.ImGui_TableNextColumn();
                            let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };
                            // cimgui.c.ImGui_Text("%s", key.ptr);
                            unsafe {
                                dcimgui_sys::ImGui_Text(
                                    c"%s".as_ptr(),
                                    key_z.as_ptr() as *const c_char,
                                )
                            };
                            // _ = cimgui.c.ImGui_TableNextColumn();
                            let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };
                            // cimgui.c.ImGui_Text("%s", entry.value_ptr.ptr);
                            unsafe {
                                dcimgui_sys::ImGui_Text(
                                    c"%s".as_ptr(),
                                    value_z.as_ptr() as *const c_char,
                                )
                            };
                        }
                    }

                    // If this is the selected event and scrolling is needed, scroll to it
                    // if (self.need_scroll_to_selected and self.is_keyboard_selection) {
                    //     cimgui.c.ImGui_SetScrollHereY(0.5);
                    //     self.need_scroll_to_selected = false;
                    // }
                    if need_scroll && is_kbd {
                        unsafe { dcimgui_sys::ImGui_SetScrollHereY(0.5) };
                        need_scroll = false;
                    }
                }
            } // reverse row loop

            // write the selection locals back to self (they were read out above to
            // keep the per-row `&mut self.events` borrow disjoint).
            self.selected_event_seq = sel;
            self.is_keyboard_selection = is_kbd;
            self.need_scroll_to_selected = need_scroll;
        } // table

        // if (cimgui.c.ImGui_BeginPopupModal(popup_filter, null, AlwaysAutoResize)) { ... }
        if unsafe {
            dcimgui_sys::ImGui_BeginPopupModal(
                popup_filter.as_ptr(),
                core::ptr::null_mut(),
                dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize
                    as dcimgui_sys::ImGuiWindowFlags,
            )
        } {
            // defer cimgui.c.ImGui_EndPopup();
            let _end_popup = Defer(|| unsafe { dcimgui_sys::ImGui_EndPopup() });

            // cimgui.c.ImGui_Text("Changed filter settings will only affect future events.");
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"Changed filter settings will only affect future events.".as_ptr(),
                )
            };
            // cimgui.c.ImGui_Separator();
            unsafe { dcimgui_sys::ImGui_Separator() };

            {
                // _ = cimgui.c.ImGui_BeginTable("table_filter_kind", 3, None);
                unsafe {
                    dcimgui_sys::ImGui_BeginTable(
                        c"table_filter_kind".as_ptr(),
                        3,
                        dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None
                            as dcimgui_sys::ImGuiTableFlags,
                    )
                };
                // defer cimgui.c.ImGui_EndTable();
                let _end_filter_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

                // inline for (@typeInfo(terminal.Parser.Action.Tag).@"enum".fields) |field| { ... }
                //
                // §3 comptime hand-expansion: iterate every `ActionTag`, skipping
                // apc_put/dcs_put (termio.zig:298-311). Each is a checkbox whose
                // state is `!filter_exclude.contains(tag)`.
                for &tag in ActionTag::ALL {
                    // if (tag == .apc_put or tag == .dcs_put) continue;
                    if tag == ActionTag::ApcPut || tag == ActionTag::DcsPut {
                        continue;
                    }
                    // _ = cimgui.c.ImGui_TableNextColumn();
                    let _ = unsafe { dcimgui_sys::ImGui_TableNextColumn() };
                    // var value = !handler.filter_exclude.contains(tag);
                    let mut value = !self.parser_stream.handler.filter_exclude.contains(tag);
                    // if (cimgui.c.ImGui_Checkbox(@tagName(tag).ptr, &value)) { ... }
                    if unsafe {
                        dcimgui_sys::ImGui_Checkbox(
                            tag.tag_name().as_ptr() as *const c_char,
                            &mut value,
                        )
                    } {
                        // if (value) handler.filter_exclude.remove(tag) else handler.filter_exclude.insert(tag);
                        if value {
                            self.parser_stream.handler.filter_exclude.remove(tag);
                        } else {
                            self.parser_stream.handler.filter_exclude.insert(tag);
                        }
                    }
                }
            } // Filter kind table

            // cimgui.c.ImGui_Separator();
            unsafe { dcimgui_sys::ImGui_Separator() };

            // cimgui.c.ImGui_Text("Filter by string. ...");
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"Filter by string. Empty displays all, \"abc\" finds lines\ncontaining \"abc\", \"abc,xyz\" finds lines containing \"abc\"\nor \"xyz\", \"-abc\" excludes lines containing \"abc\"."
                        .as_ptr(),
                )
            };
            // _ = cimgui.c.ImGuiTextFilter_Draw(&handler.filter_text, "##filter_text", 0);
            let _ = unsafe {
                dcimgui_sys::ImGuiTextFilter_Draw(
                    &mut self.parser_stream.handler.filter_text,
                    c"##filter_text".as_ptr(),
                    0.0,
                )
            };

            // cimgui.c.ImGui_Separator();
            unsafe { dcimgui_sys::ImGui_Separator() };
            // if (cimgui.c.ImGui_Button("Close")) cimgui.c.ImGui_CloseCurrentPopup();
            if unsafe { dcimgui_sys::ImGui_Button(c"Close".as_ptr()) } {
                unsafe { dcimgui_sys::ImGui_CloseCurrentPopup() };
            }
        } // filter popup
    }

    /// The keyboard-navigation branch of `draw` (termio.zig:132-183), factored out
    /// so its read-only reverse/forward walks over `events` (which set
    /// `self.selected_event_seq`) do not tangle with the per-row `&mut` walk.
    fn handle_keyboard_nav(&mut self) {
        // const key_pressed = getKeyAction();
        let key_pressed = get_key_action();
        match key_pressed {
            // .none => {},
            KeyAction::None => {}
            // .up, .down => { ... }
            KeyAction::Up | KeyAction::Down => {
                // If no event is selected, select the first/last event based on direction
                // if (self.selected_event_seq == null) {
                if self.selected_event_seq.is_none() {
                    // if (!events.empty()) {
                    //     var it = events.iterator(if (key == .up) .forward else .reverse);
                    //     if (it.next()) |ev| self.selected_event_seq = ev.seq;
                    // }
                    if !self.events.empty() {
                        let dir = if key_pressed == KeyAction::Up {
                            Direction::Forward
                        } else {
                            Direction::Reverse
                        };
                        let mut it = self.events.iterator(dir);
                        if let Some(ev) = it.next() {
                            self.selected_event_seq = Some(ev.seq as u32);
                        }
                    }
                } else {
                    // Find next/previous event based on current selection
                    // var it = events.iterator(.reverse);
                    let cur = self.selected_event_seq.unwrap();
                    let mut it = self.events.iterator(Direction::Reverse);
                    match key_pressed {
                        // .down => { ... first ev AFTER the current one in reverse order }
                        KeyAction::Down => {
                            let mut found = false;
                            while let Some(ev) = it.next() {
                                if found {
                                    self.selected_event_seq = Some(ev.seq as u32);
                                    break;
                                }
                                if ev.seq as u32 == cur {
                                    found = true;
                                }
                            }
                        }
                        // .up => { ... the ev BEFORE the current one in reverse order }
                        KeyAction::Up => {
                            let mut prev_ev: Option<u32> = None;
                            while let Some(ev) = it.next() {
                                if ev.seq as u32 == cur {
                                    if let Some(prev) = prev_ev {
                                        self.selected_event_seq = Some(prev);
                                        break;
                                    }
                                }
                                prev_ev = Some(ev.seq as u32);
                            }
                        }
                        // .none => unreachable,
                        KeyAction::None => unreachable!(),
                    }
                }

                // Mark that we need to scroll to the newly selected item
                // self.need_scroll_to_selected = true;
                self.need_scroll_to_selected = true;
                // self.is_keyboard_selection = true;
                self.is_keyboard_selection = true;
            }
        }
    }
}

/// Helper function to check keyboard state and determine navigation action.
// fn getKeyAction() KeyAction { ... }
fn get_key_action() -> KeyAction {
    // const keys = .{ {J,down}, {DownArrow,down}, {K,up}, {UpArrow,up} };
    // inline for (keys) |k| if (cimgui.c.ImGui_IsKeyPressed(k.key)) return k.action;
    let keys: [(dcimgui_sys::ImGuiKey, KeyAction); 4] = [
        (
            dcimgui_sys::ImGuiKey_J as dcimgui_sys::ImGuiKey,
            KeyAction::Down,
        ),
        (
            dcimgui_sys::ImGuiKey_DownArrow as dcimgui_sys::ImGuiKey,
            KeyAction::Down,
        ),
        (
            dcimgui_sys::ImGuiKey_K as dcimgui_sys::ImGuiKey,
            KeyAction::Up,
        ),
        (
            dcimgui_sys::ImGuiKey_UpArrow as dcimgui_sys::ImGuiKey,
            KeyAction::Up,
        ),
    ];
    for (key, action) in keys {
        if unsafe { dcimgui_sys::ImGui_IsKeyPressed(key) } {
            return action;
        }
    }
    // return .none;
    KeyAction::None
}

/// Enum representing keyboard navigation actions
// const KeyAction = enum { down, none, up };
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    Down,
    None,
    Up,
}

// ============================================================================
// VTEvent — the recorded event + the metadata encoder (STEP 1)
// ============================================================================

/// Circular buffer of VT events.
// pub const Ring = CircBuf(VTEvent, undefined);
pub type VTEventRing = CircBuf<VTEvent>;

/// A snapshot of `terminal.Terminal.ScrollingRegion` taken BY VALUE at event
/// time (INV-8). The reference stores the `ScrollingRegion` itself; the rustty-vt
/// `ScrollingRegion` derives no `Clone` (needed for `CircBuf<VTEvent>`), so we
/// snapshot its four `Copy` scalar fields under the same names the draw reads.
#[derive(Clone, Copy)]
struct ScrollRegionSnapshot {
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

impl ScrollRegionSnapshot {
    fn of(sr: &ScrollingRegion) -> ScrollRegionSnapshot {
        ScrollRegionSnapshot {
            top: sr.top as u32,
            bottom: sr.bottom as u32,
            left: sr.left as u32,
            right: sr.right as u32,
        }
    }
}

/// Insertion-ordered metadata map. The reference is `StringHashMap([:0]const u8)`
/// backed by the event's arena (keys `[]const u8`, values `[:0]const u8`). Ported
/// as owned `(key, value)` byte pairs BOTH NUL-terminated (`dupe_z`) so the draw
/// hands the stored pointers straight to ImGui `%s`. Insertion order is
/// deterministic (the reference hash-map iteration order is unspecified anyway).
// const Metadata = std.StringHashMap([:0]const u8);
#[derive(Clone, Default)]
struct Metadata {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Metadata {
    fn new() -> Metadata {
        Metadata {
            entries: Vec::new(),
        }
    }

    /// `md.put(key, value)` — `key` bytes and an already-NUL-terminated `value_z`.
    fn put(&mut self, key: &[u8], value_z: Vec<u8>) {
        self.entries.push((dupe_z(key), value_z));
    }

    fn iter(&self) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

/// `alloc.dupeZ(u8, src)` — copy `src` and append a NUL sentinel. Interior NULs
/// are preserved (C `%s` truncates at the first, matching ImGui's display of the
/// reference's arena-`dupeZ` bytes); this never panics (unlike `CString::new`).
fn dupe_z(src: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(src.len() + 1);
    v.extend_from_slice(src);
    v.push(0);
    v
}

/// VT event. This isn't public because this is just how we store internal events.
// const VTEvent = struct { ... };
//
// PORT NOTE (arena → Rust ownership): the reference's first field
//
//     /// The arena that all allocated memory for this event is stored.
//     arena_state: ArenaAllocator.State,
//
// backs raw_description + metadata in a per-event `ArenaAllocator` freed by
// `deinit`. Under Rust's global allocator (panic=abort) the arena collapses to
// owned fields that free on `Drop` — the same collapse lib.rs applies to
// `apprt.Inspector`. `Clone` is required by `CircBuf<VTEvent>`; every field is
// `Clone`/`Copy`.
#[derive(Clone)]
pub struct VTEvent {
    // seq: usize = 1,
    /// Sequence number, just monotonically increasing and wrapping if
    /// it ever overflows. It gives us a nice way to visualize progress.
    seq: usize,

    // kind: Kind,
    /// Kind of event, for filtering
    kind: Kind,

    // raw_description: [:0]const u8,
    /// The description of the raw event in a more human-friendly format.
    /// For example for control sequences this is the full sequence but
    /// control characters are replaced with human-readable names, e.g.
    /// 0x07 (bell) becomes BEL. (NUL-terminated, as the reference `[:0]const u8`.)
    raw_description: Vec<u8>,

    /// Various metadata at the time of the event (before processing).
    // cursor: terminal.Screen.Cursor,
    /// Cursor snapshot at event time (INV-8, BY VALUE).
    ///
    /// PORT NOTE: this is a faithful by-value copy of `t.screens.active.cursor`
    /// including its raw `hyperlink`/`page_*` back-pointers, exactly as the
    /// reference stores it; `cursorTable` may later deref `cursor.hyperlink`. The
    /// reference makes the same short-lived-snapshot assumption (events are drawn
    /// under the surface mutex soon after capture) — ported faithfully (R1).
    cursor: Cursor,

    // scrolling_region: terminal.Terminal.ScrollingRegion,
    scrolling_region: ScrollRegionSnapshot,

    // metadata: Metadata.Unmanaged = .{},
    metadata: Metadata,

    // imgui_selected: bool = false,
    /// imgui selection state
    imgui_selected: bool,
}

// const Kind = enum { print, execute, csi, esc, osc, dcs, apc };
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Print,
    Execute,
    Csi,
    Esc,
    Osc,
    Dcs,
    Apc,
}

impl Kind {
    // @tagName(ev.kind)
    fn tag_name(self) -> &'static CStr {
        match self {
            Kind::Print => c"print",
            Kind::Execute => c"execute",
            Kind::Csi => c"csi",
            Kind::Esc => c"esc",
            Kind::Osc => c"osc",
            Kind::Dcs => c"dcs",
            Kind::Apc => c"apc",
        }
    }
}

impl VTEvent {
    /// The `undefined`-slot fill value for `CircBuf<VTEvent>` (key.rs
    /// `Event::default()` precedent — the CircBuf `default:T`, never read as a
    /// live event). Uses the canonical default cursor (screen.rs `Cursor` ctor
    /// shape: block style, default style id, null page pointers).
    fn default_fill() -> VTEvent {
        VTEvent {
            seq: 0,
            kind: Kind::Print,
            raw_description: dupe_z(b""),
            cursor: default_cursor(),
            scrolling_region: ScrollRegionSnapshot {
                top: 0,
                bottom: 0,
                left: 0,
                right: 0,
            },
            metadata: Metadata::new(),
            imgui_selected: false,
        }
    }

    /// Initialize the event information for the given parser action.
    // pub fn init(alloc_gpa, t: *const terminal.Terminal, action: terminal.Parser.Action) !VTEvent {
    //
    // PORT NOTE (INV-8): `t` is a raw `*const Terminal` valid only during the
    // capture window; cursor + scrolling_region are snapshotted BY VALUE here. The
    // arena is collapsed (see the struct note); `!VTEvent` is infallible.
    // SAFETY: caller (`vt_raw`) holds `t` valid under the surface mutex (INV-1).
    fn init(_alloc_gpa: Allocator, t: *const Terminal, action: &Action) -> VTEvent {
        // var md = Metadata.init(alloc);
        let mut md = Metadata::new();
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // try encodeAction(alloc, &buf.writer, &md, action);
        // const desc = try buf.toOwnedSliceSentinel(0);
        let mut buf = String::new();
        encode_action(&mut buf, &mut md, action);
        let desc = dupe_z(buf.as_bytes());

        // const kind: Kind = switch (action) { ... };
        let kind: Kind = match action {
            Action::Print(_) => Kind::Print,
            Action::Execute(_) => Kind::Execute,
            Action::CsiDispatch(_) => Kind::Csi,
            Action::EscDispatch(_) => Kind::Esc,
            Action::OscDispatch(_) => Kind::Osc,
            Action::DcsHook(_) | Action::DcsPut(_) | Action::DcsUnhook => Kind::Dcs,
            Action::ApcStart | Action::ApcPut(_) | Action::ApcEnd => Kind::Apc,
        };

        // return .{ .kind = kind, .raw_description = desc,
        //           .cursor = t.screens.active.cursor,
        //           .scrolling_region = t.scrolling_region,
        //           .metadata = md.unmanaged };
        //
        // SAFETY: `t` and `t.screens.active` (a `*mut Screen`) are valid under the
        // surface mutex for the capture window (INV-1/INV-8).
        let cursor = unsafe { (*(*t).screens.active).cursor };
        let scrolling_region = ScrollRegionSnapshot::of(unsafe { &(*t).scrolling_region });

        VTEvent {
            seq: 1,
            kind,
            raw_description: desc,
            cursor,
            scrolling_region,
            metadata: md,
            imgui_selected: false,
        }
    }

    // pub fn deinit(self: *VTEvent, alloc_gpa: Allocator) void {
    //     var arena = self.arena_state.promote(alloc_gpa); arena.deinit();
    // }
    //
    // PORT NOTE: the arena collapse means raw_description/metadata free on `Drop`;
    // `deinit` is a no-op retained for the reference call-shape. Takes `&self`
    // (not the reference `*VTEvent`) so the read-only `CircBuf::Iterator` forward
    // walks in `Stream::deinit`/the Clear button can call it (nothing to free).
    fn deinit(&self, _alloc_gpa: Allocator) {}

    /// Returns true if the event passes the given filter.
    // pub fn passFilter(self: *const VTEvent, filter: *const cimgui.c.ImGuiTextFilter) bool {
    fn pass_filter(&self, filter: *const dcimgui_sys::ImGuiTextFilter) -> bool {
        // Check our main string
        // if (ImGuiTextFilter_PassFilter(filter, self.raw_description.ptr, null)) return true;
        if unsafe {
            dcimgui_sys::ImGuiTextFilter_PassFilter(
                filter,
                self.raw_description.as_ptr() as *const c_char,
                core::ptr::null(),
            )
        } {
            return true;
        }

        // We also check all metadata keys and values
        // var it = self.metadata.iterator();
        // while (it.next()) |entry| { ... }
        for (key_z, value_z) in self.metadata.iter() {
            // (our keys are already NUL-terminated; the reference's bufPrintZ into
            // a [256] scratch is subsumed.)
            if unsafe {
                dcimgui_sys::ImGuiTextFilter_PassFilter(
                    filter,
                    key_z.as_ptr() as *const c_char,
                    core::ptr::null(),
                )
            } {
                return true;
            }
            if unsafe {
                dcimgui_sys::ImGuiTextFilter_PassFilter(
                    filter,
                    value_z.as_ptr() as *const c_char,
                    core::ptr::null(),
                )
            } {
                return true;
            }
        }

        false
    }
}

// ----------------------------------------------------------------------------
// The description encoders (termio.zig:497-573)
// ----------------------------------------------------------------------------

/// Encode a parser action as a string that we show in the logs.
// fn encodeAction(alloc, writer, md, action: terminal.Parser.Action) !void {
fn encode_action(writer: &mut String, md: &mut Metadata, action: &Action) {
    match action {
        // .print => try encodePrint(writer, action),
        Action::Print(ch) => encode_print(writer, *ch),
        // .execute => try encodeExecute(writer, action),
        Action::Execute(ch) => encode_execute(writer, *ch),
        // .csi_dispatch => |v| try encodeCSI(writer, v),
        Action::CsiDispatch(v) => encode_csi(writer, v),
        // .esc_dispatch => |v| try encodeEsc(writer, v),
        Action::EscDispatch(v) => encode_esc(writer, v),
        // .osc_dispatch => |v| try encodeOSC(alloc, writer, md, v),
        Action::OscDispatch(v) => encode_osc(writer, md, v),
        // else => try writer.print("{f}", .{action}),
        other => format_action_default(writer, other),
    }
}

// fn encodePrint(writer, action) !void { const ch = action.print; writer.print("'{u}' (U+{X})", ch, ch); }
fn encode_print(writer: &mut String, ch: u32) {
    let c = char::from_u32(ch).unwrap_or('\u{FFFD}');
    let _ = write!(writer, "'{}' (U+{:X})", c, ch);
}

// fn encodeExecute(writer, action) !void { ... }
fn encode_execute(writer: &mut String, ch: u8) {
    // switch (ch) { 0x00 => "NUL", ... 0x0F => "SI", else => "?" }
    let name = match ch {
        0x00 => "NUL",
        0x01 => "SOH",
        0x02 => "STX",
        0x03 => "ETX",
        0x04 => "EOT",
        0x05 => "ENQ",
        0x06 => "ACK",
        0x07 => "BEL",
        0x08 => "BS",
        0x09 => "HT",
        0x0A => "LF",
        0x0B => "VT",
        0x0C => "FF",
        0x0D => "CR",
        0x0E => "SO",
        0x0F => "SI",
        _ => "?",
    };
    let _ = writer.write_str(name);
    // try writer.print(" (0x{X})", .{ch});
    let _ = write!(writer, " (0x{:X})", ch);
}

// fn encodeCSI(writer, csi: terminal.Parser.Action.CSI) !void { ... }
fn encode_csi(writer: &mut String, csi: &rustty_vt::parser::CSI) {
    // for (csi.intermediates) |v| try writer.print("{c} ", .{v});
    for &v in &csi.intermediates {
        let _ = write!(writer, "{} ", v as char);
    }
    // for (csi.params, 0..) |v, i| { if (i != 0) writeByte(';'); writer.print("{d}", v); }
    for (i, &v) in csi.params.iter().enumerate() {
        if i != 0 {
            let _ = writer.write_char(';');
        }
        let _ = write!(writer, "{}", v);
    }
    // if (csi.intermediates.len > 0 or csi.params.len > 0) writeByte(' ');
    if !csi.intermediates.is_empty() || !csi.params.is_empty() {
        let _ = writer.write_char(' ');
    }
    // try writer.writeByte(csi.final);
    let _ = writer.write_char(csi.final_ as char);
}

// fn encodeEsc(writer, esc: terminal.Parser.Action.ESC) !void { ... }
fn encode_esc(writer: &mut String, esc: &rustty_vt::parser::ESC) {
    // for (esc.intermediates) |v| try writer.print("{c} ", .{v});
    for &v in &esc.intermediates {
        let _ = write!(writer, "{} ", v as char);
    }
    // try writer.writeByte(esc.final);
    let _ = writer.write_char(esc.final_ as char);
}

/// The `else => try writer.print("{f}", .{action})` fallback (termio.zig:510) for
/// the dcs_*/apc_* variants, mirroring the reference `Action.format` union
/// formatter (`Parser.Action{ .<tag> = <payload> }`, u8 payloads as `0x{x}`).
fn format_action_default(writer: &mut String, action: &Action) {
    let _ = writer.write_str("Parser.Action{ .");
    match action {
        Action::DcsHook(v) => {
            let _ = write!(writer, "dcs_hook = {:?}", v);
        }
        Action::DcsPut(b) => {
            let _ = write!(writer, "dcs_put = 0x{:x}", b);
        }
        Action::DcsUnhook => {
            let _ = writer.write_str("dcs_unhook = void");
        }
        Action::ApcStart => {
            let _ = writer.write_str("apc_start = void");
        }
        Action::ApcPut(b) => {
            let _ = write!(writer, "apc_put = 0x{:x}", b);
        }
        Action::ApcEnd => {
            let _ = writer.write_str("apc_end = void");
        }
        // (print/execute/csi/esc/osc are handled by encode_action's explicit arms.)
        _ => {
            let _ = write!(writer, "{:?}", action);
        }
    }
    let _ = writer.write_str(" }");
}

// ----------------------------------------------------------------------------
// The metadata encoder — the `@typeInfo` hand-expansion (STEP 1, §3/S10)
// ----------------------------------------------------------------------------

// fn encodeOSC(alloc, writer, md, osc: terminal.osc.Command) !void {
fn encode_osc(writer: &mut String, md: &mut Metadata, osc: &OscCommand) {
    // The description is just the tag
    // try writer.print("{s} ", .{@tagName(osc)});
    let _ = write!(writer, "{} ", osc_tag_name(osc));

    // Add additional fields to metadata
    // switch (osc) { inline else => |v, tag| if (tag == osc) try encodeMetadata(alloc, md, v) }
    encode_metadata_osc(md, osc);
}

/// `@tagName(osc)` — the union tag name (Zig snake_case field identifiers).
fn osc_tag_name(osc: &OscCommand) -> &'static str {
    match osc {
        OscCommand::Invalid => "invalid",
        OscCommand::ChangeWindowTitle(_) => "change_window_title",
        OscCommand::ChangeWindowIcon(_) => "change_window_icon",
        OscCommand::SemanticPrompt(_) => "semantic_prompt",
        OscCommand::ClipboardContents { .. } => "clipboard_contents",
        OscCommand::ReportPwd { .. } => "report_pwd",
        OscCommand::MouseShape { .. } => "mouse_shape",
        OscCommand::ColorOperation { .. } => "color_operation",
        OscCommand::KittyColorProtocol(_) => "kitty_color_protocol",
        OscCommand::ShowDesktopNotification { .. } => "show_desktop_notification",
        OscCommand::HyperlinkStart { .. } => "hyperlink_start",
        OscCommand::HyperlinkEnd => "hyperlink_end",
        OscCommand::ConemuSleep { .. } => "conemu_sleep",
        OscCommand::ConemuShowMessageBox(_) => "conemu_show_message_box",
        OscCommand::ConemuChangeTabTitle(_) => "conemu_change_tab_title",
        OscCommand::ConemuProgressReport(_) => "conemu_progress_report",
        OscCommand::ConemuWaitInput => "conemu_wait_input",
        OscCommand::ConemuGuimacro(_) => "conemu_guimacro",
        OscCommand::ConemuRunProcess(_) => "conemu_run_process",
        OscCommand::ConemuOutputEnvironmentVariable(_) => "conemu_output_environment_variable",
        OscCommand::ConemuXtermEmulation { .. } => "conemu_xterm_emulation",
        OscCommand::ConemuComment(_) => "conemu_comment",
        OscCommand::KittyTextSizing(_) => "kitty_text_sizing",
        OscCommand::KittyClipboardProtocol(_) => "kitty_clipboard_protocol",
        OscCommand::KittyDndProtocol(_) => "kitty_dnd_protocol",
        OscCommand::ContextSignal(_) => "context_signal",
    }
}

/// The `encodeMetadata(alloc, md, v)` hand-expansion over the ACTIVE OSC payload
/// (termio.zig:575-615). The reference visits `@typeInfo(@TypeOf(v))`:
///   * `void` ⇒ nothing;
///   * `[]const u8`/`[:0]const u8` ⇒ `md.put("data", v)`;
///   * `.@"struct"` ⇒ per field: `encode_metadata_single(field.name, field_value)`;
///   * `.@"union"` ⇒ tag name; active-void ⇒ `md.put("data", tag)`, else
///     `encode_metadata_single(tag, payload)`.
///
/// Every arm below is that visit specialized to the concrete `osc::Command`
/// payload — verified against the reference union (osc.zig:25-96): the payloads
/// are exactly void / byte-slice / struct / union, so no `else => @compileError`
/// arm is reachable (matching the reference, which compiles).
fn encode_metadata_osc(md: &mut Metadata, osc: &OscCommand) {
    match osc {
        // invalid / hyperlink_end / conemu_wait_input : void ⇒ {}
        OscCommand::Invalid | OscCommand::HyperlinkEnd | OscCommand::ConemuWaitInput => {}

        // *_ : [:0]const u8 ⇒ md.put("data", v)
        OscCommand::ChangeWindowTitle(v)
        | OscCommand::ChangeWindowIcon(v)
        | OscCommand::ConemuShowMessageBox(v)
        | OscCommand::ConemuGuimacro(v)
        | OscCommand::ConemuRunProcess(v)
        | OscCommand::ConemuOutputEnvironmentVariable(v)
        | OscCommand::ConemuComment(v) => md.put(b"data", dupe_z(v)),

        // semantic_prompt: struct { action: Action(enum), options_unvalidated: []const u8 }
        OscCommand::SemanticPrompt(sp) => {
            md.put(b"action", dupe_z(sp_action_tag(sp.action).as_bytes()));
            md.put(b"options_unvalidated", dupe_z(&sp.options_unvalidated));
        }

        // clipboard_contents: struct { kind: u8, data: [:0]const u8 }
        OscCommand::ClipboardContents { kind, data } => {
            md.put(b"kind", single_int(*kind as i64));
            md.put(b"data", dupe_z(data));
        }

        // report_pwd: struct { value: [:0]const u8 }
        OscCommand::ReportPwd { value } => md.put(b"value", dupe_z(value)),
        // mouse_shape: struct { value: [:0]const u8 }
        OscCommand::MouseShape { value } => md.put(b"value", dupe_z(value)),

        // color_operation: struct { op: Operation(enum), requests: color.List(struct), terminator: Terminator(enum) }
        OscCommand::ColorOperation {
            op,
            requests: _,
            terminator,
        } => {
            md.put(b"op", dupe_z(color_op_tag(*op).as_bytes()));
            // requests: a struct (SegmentedList) ⇒ @typeName(Value). See single_struct.
            md.put(b"requests", single_struct::<rustty_vt::osc::color::List>());
            md.put(
                b"terminator",
                dupe_z(terminator_tag(*terminator).as_bytes()),
            );
        }

        // kitty_color_protocol: OSC { list: ArrayList(Request)(struct), terminator: Terminator(enum) }
        OscCommand::KittyColorProtocol(o) => {
            md.put(
                b"list",
                single_struct::<Vec<rustty_vt::kitty::color::Request>>(),
            );
            md.put(
                b"terminator",
                dupe_z(terminator_tag(o.terminator).as_bytes()),
            );
        }

        // show_desktop_notification: struct { title: [:0]const u8, body: [:0]const u8 }
        OscCommand::ShowDesktopNotification { title, body } => {
            md.put(b"title", dupe_z(title));
            md.put(b"body", dupe_z(body));
        }

        // hyperlink_start: struct { id: ?[:0]const u8, uri: [:0]const u8 }
        OscCommand::HyperlinkStart { id, uri } => {
            // id: optional byte-slice
            match id {
                Some(bytes) => md.put(b"id", dupe_z(bytes)),
                None => md.put(b"id", dupe_z(b"(unset)")),
            }
            md.put(b"uri", dupe_z(uri));
        }

        // conemu_sleep: struct { duration_ms: u16 }
        OscCommand::ConemuSleep { duration_ms } => {
            md.put(b"duration_ms", single_int(*duration_ms as i64));
        }

        // conemu_change_tab_title: union(enum) { reset, value: [:0]const u8 }
        OscCommand::ConemuChangeTabTitle(v) => match v {
            // active-void ⇒ md.put("data", tag_name)
            rustty_vt::osc::ConemuChangeTabTitle::Reset => md.put(b"data", dupe_z(b"reset")),
            // active-payload ⇒ encode_metadata_single(tag_name, value) ⇒ md.put("value", bytes)
            rustty_vt::osc::ConemuChangeTabTitle::Value(bytes) => md.put(b"value", dupe_z(bytes)),
        },

        // conemu_progress_report: ProgressReport { state: State(enum), progress: ?u8 }
        OscCommand::ConemuProgressReport(pr) => {
            md.put(b"state", dupe_z(progress_state_tag(pr.state).as_bytes()));
            match pr.progress {
                Some(p) => md.put(b"progress", single_int(p as i64)),
                None => md.put(b"progress", dupe_z(b"(unset)")),
            }
        }

        // conemu_xterm_emulation: struct { keyboard: ?bool, output: ?bool }
        OscCommand::ConemuXtermEmulation { keyboard, output } => {
            md.put(b"keyboard", single_opt_bool(*keyboard));
            md.put(b"output", single_opt_bool(*output));
        }

        // kitty_text_sizing: OSC { scale,width,numerator,denominator: int, valign,halign: enum, text: [:0]const u8 }
        OscCommand::KittyTextSizing(o) => {
            md.put(b"scale", single_int(o.scale as i64));
            md.put(b"width", single_int(o.width as i64));
            md.put(b"numerator", single_int(o.numerator as i64));
            md.put(b"denominator", single_int(o.denominator as i64));
            md.put(b"valign", dupe_z(valign_tag(o.valign).as_bytes()));
            md.put(b"halign", dupe_z(halign_tag(o.halign).as_bytes()));
            md.put(b"text", dupe_z(&o.text));
        }

        // kitty_clipboard_protocol: OSC { metadata: []const u8, payload: ?[]const u8, terminator: enum }
        OscCommand::KittyClipboardProtocol(o) => {
            md.put(b"metadata", dupe_z(&o.metadata));
            match &o.payload {
                Some(bytes) => md.put(b"payload", dupe_z(bytes)),
                None => md.put(b"payload", dupe_z(b"(unset)")),
            }
            md.put(
                b"terminator",
                dupe_z(terminator_tag(o.terminator).as_bytes()),
            );
        }

        // kitty_dnd_protocol: OSC { metadata: []const u8, payload: ?[]const u8, terminator: enum }
        OscCommand::KittyDndProtocol(o) => {
            md.put(b"metadata", dupe_z(&o.metadata));
            match &o.payload {
                Some(bytes) => md.put(b"payload", dupe_z(bytes)),
                None => md.put(b"payload", dupe_z(b"(unset)")),
            }
            md.put(
                b"terminator",
                dupe_z(terminator_tag(o.terminator).as_bytes()),
            );
        }

        // context_signal: Command { action: Action(enum), id: []const u8, metadata: []const u8 }
        OscCommand::ContextSignal(cmd) => {
            md.put(b"action", dupe_z(context_action_tag(cmd.action).as_bytes()));
            md.put(b"id", dupe_z(&cmd.id));
            md.put(b"metadata", dupe_z(&cmd.metadata));
        }
    }
}

// ---- encode_metadata_single leaf helpers (termio.zig:617-687) --------------
//
// The reference `encodeMetadataSingle(key, value)` visits `@typeInfo(@TypeOf(
// value))`. Each concrete leaf type above dispatches to one of these:

// .int => md.put(key, allocPrintSentinel("{}", value))
fn single_int(value: i64) -> Vec<u8> {
    dupe_z(format!("{}", value).as_bytes())
}

// .bool => md.put(key, if (value) "true" else "false");  wrapped by .optional:
//   .optional => if (value) |u| single(u) else md.put(key, "(unset)")
fn single_opt_bool(value: Option<bool>) -> Vec<u8> {
    match value {
        Some(true) => dupe_z(b"true"),
        Some(false) => dupe_z(b"false"),
        None => dupe_z(b"(unset)"),
    }
}

// .@"struct" => md.put(key, @typeName(Value))
//
// PORT NOTE (fidelity): `@typeName` yields a Zig-internal type-name string
// (e.g. `array_list.ArrayListAligned(...)`) that is not reproducible in Rust; we
// emit `core::any::type_name` — the faithful analogue (the declared type's name).
// The value is display-only diagnostic; the KIND (a type name) is preserved.
fn single_struct<T: ?Sized>() -> Vec<u8> {
    dupe_z(core::any::type_name::<T>().as_bytes())
}

// ---- @tagName helpers for the enums the encoder emits -----------------------
// Zig `@tagName` returns the snake_case field identifier.

// terminal.osc.SemanticPrompt.Action (semantic_prompt.zig:Action)
fn sp_action_tag(a: rustty_vt::osc::semantic_prompt::Action) -> &'static str {
    use rustty_vt::osc::semantic_prompt::Action as A;
    match a {
        A::FreshLine => "fresh_line",
        A::FreshLineNewPrompt => "fresh_line_new_prompt",
        A::NewCommand => "new_command",
        A::PromptStart => "prompt_start",
        A::EndPromptStartInput => "end_prompt_start_input",
        A::EndPromptStartInputTerminateEol => "end_prompt_start_input_terminate_eol",
        A::EndInputStartOutput => "end_input_start_output",
        A::EndCommand => "end_command",
    }
}

// terminal.osc.color.Operation (color.zig:Operation) — variants already snake_case.
fn color_op_tag(op: rustty_vt::osc::color::Operation) -> &'static str {
    use rustty_vt::osc::color::Operation as O;
    match op {
        O::osc_4 => "osc_4",
        O::osc_5 => "osc_5",
        O::osc_10 => "osc_10",
        O::osc_11 => "osc_11",
        O::osc_12 => "osc_12",
        O::osc_13 => "osc_13",
        O::osc_14 => "osc_14",
        O::osc_15 => "osc_15",
        O::osc_16 => "osc_16",
        O::osc_17 => "osc_17",
        O::osc_18 => "osc_18",
        O::osc_19 => "osc_19",
        O::osc_104 => "osc_104",
        O::osc_105 => "osc_105",
        O::osc_110 => "osc_110",
        O::osc_111 => "osc_111",
        O::osc_112 => "osc_112",
        O::osc_113 => "osc_113",
        O::osc_114 => "osc_114",
        O::osc_115 => "osc_115",
        O::osc_116 => "osc_116",
        O::osc_117 => "osc_117",
        O::osc_118 => "osc_118",
        O::osc_119 => "osc_119",
    }
}

// terminal.osc.Terminator
fn terminator_tag(t: rustty_vt::osc::Terminator) -> &'static str {
    match t {
        rustty_vt::osc::Terminator::St => "st",
        rustty_vt::osc::Terminator::Bel => "bel",
    }
}

// terminal.osc.ProgressReport.State (Zig `@"error"` ⇒ "error")
fn progress_state_tag(s: rustty_vt::osc::ProgressReportState) -> &'static str {
    use rustty_vt::osc::ProgressReportState as S;
    match s {
        S::Remove => "remove",
        S::Set => "set",
        S::Error => "error",
        S::Indeterminate => "indeterminate",
        S::Pause => "pause",
    }
}

// terminal.osc.parsers.kitty_text_sizing.VAlign / HAlign
fn valign_tag(v: rustty_vt::osc::parsers::kitty_text_sizing::VAlign) -> &'static str {
    use rustty_vt::osc::parsers::kitty_text_sizing::VAlign as V;
    match v {
        V::Top => "top",
        V::Bottom => "bottom",
        V::Center => "center",
    }
}
fn halign_tag(h: rustty_vt::osc::parsers::kitty_text_sizing::HAlign) -> &'static str {
    use rustty_vt::osc::parsers::kitty_text_sizing::HAlign as H;
    match h {
        H::Left => "left",
        H::Right => "right",
        H::Center => "center",
    }
}

// terminal.osc.parsers.context_signal.Action
fn context_action_tag(a: rustty_vt::osc::parsers::context_signal::Action) -> &'static str {
    use rustty_vt::osc::parsers::context_signal::Action as A;
    match a {
        A::Start => "start",
        A::End => "end",
    }
}

/// The canonical default `Cursor` for the `CircBuf` fill slot (screen.rs `Cursor`
/// ctor shape). Never observed as a live event.
fn default_cursor() -> Cursor {
    Cursor {
        x: 0,
        y: 0,
        cursor_style: CursorStyle::Block,
        pending_wrap: false,
        protected: false,
        style: style::Style::default(),
        style_id: style::DEFAULT_ID,
        hyperlink_id: 0,
        hyperlink_implicit_id: 0,
        hyperlink: None,
        semantic_content: SemanticContent::Output,
        semantic_content_clear_eol: false,
        page_pin: core::ptr::null_mut(),
        page_row: core::ptr::null_mut(),
        page_cell: core::ptr::null_mut(),
    }
}

// ============================================================================
// VTHandler — the parser handler + the R1 self-alias capture state (STEP 2)
// ============================================================================

/// Our VT stream handler for the Stream widget. This isn't public
/// because there is no reason to use this directly.
// const VTHandler = struct { ... };
pub struct VTHandler {
    // state: ?State,
    /// The capture state, must be set before use. If null, events are dropped.
    state: Option<State>,

    // paused: bool,
    /// True to pause this artificially.
    paused: bool,

    // current_seq: usize,
    /// Current sequence number
    current_seq: usize,

    // filter_exclude: ActionTagSet,
    /// Exclude certain actions by tag.
    filter_exclude: ActionTagSet,

    // filter_text: cimgui.c.ImGuiTextFilter,
    filter_text: dcimgui_sys::ImGuiTextFilter,
}

// pub const State = struct { alloc, terminal: *const Terminal, events: *VTEvent.Ring };
struct State {
    // alloc: Allocator,
    /// The allocator to use for the events.
    alloc: Allocator,
    // terminal: *const terminal.Terminal,
    /// The terminal state at the time of the event.
    terminal: *const Terminal,
    // events: *VTEvent.Ring,   — R1: RAW `*mut` self-alias of `Stream.events`.
    /// The event ring to write events to.
    events: *mut VTEventRing,
}

impl VTHandler {
    // pub const init: VTHandler = .{ ... };
    fn init() -> VTHandler {
        VTHandler {
            // .state = null,
            state: None,
            // .paused = false,
            paused: false,
            // .current_seq = 1,
            current_seq: 1,
            // .filter_exclude = .initMany(&.{.print}),
            filter_exclude: ActionTagSet::init_many(&[ActionTag::Print]),
            // .filter_text = .{},
            //
            // PORT NOTE: the reference zero-inits the ImGuiTextFilter. A zeroed
            // `ImGuiTextFilter_t` (empty InputBuf, null/empty Filters vector,
            // CountGrep=0) is exactly the C++ default-constructed empty filter.
            filter_text: unsafe { core::mem::zeroed() },
        }
    }

    /// This is called with every single terminal action.
    // pub fn vtRaw(self: *VTHandler, action: terminal.Parser.Action) !bool {
    //
    // R1 (STEP 2): appends through the RAW `*mut VTEventRing` self-alias copied
    // out of `state.events`; never forms `&mut (the ring)` other than the narrow
    // per-append `&mut *ptr` reborrow. Proven SB/TB-clean on
    // mirihar-inspector-vthandler.
    fn vt_raw_impl(&mut self, action: &Action) -> bool {
        // const state: *State = if (self.state) |*s| s else return true;
        let (alloc, terminal, vt_events): (Allocator, *const Terminal, *mut VTEventRing) =
            match self.state.as_ref() {
                Some(s) => (s.alloc, s.terminal, s.events),
                None => return true,
            };

        // We always increment the sequence number, even if we're paused or
        // filter out the event. This helps show the user that there is a gap
        // between events and roughly how large that gap was.
        // defer self.current_seq +%= 1;
        //
        // PORT NOTE: the reference `defer` fires on every return path; because the
        // bump reads+writes `self.current_seq` (which `VTEvent::init` also reads
        // via `ev.seq = self.current_seq`), we apply it explicitly just before
        // each of the four `return` exits rather than via a `self`-borrowing guard.

        // If we're manually paused, we ignore all events.
        // if (self.paused) return true;
        if self.paused {
            self.current_seq = self.current_seq.wrapping_add(1);
            return true;
        }

        // We ignore certain action types that are too noisy.
        // switch (action) { .dcs_put, .apc_put => return true, else => {} }
        if matches!(action, Action::DcsPut(_) | Action::ApcPut(_)) {
            self.current_seq = self.current_seq.wrapping_add(1);
            return true;
        }

        // If we requested a specific type to be ignored, ignore it.
        // We return true because we did "handle" it by ignoring it.
        // if (self.filter_exclude.contains(std.meta.activeTag(action))) return true;
        if self.filter_exclude.contains(ActionTag::of(action)) {
            self.current_seq = self.current_seq.wrapping_add(1);
            return true;
        }

        // Build our event
        // var ev: VTEvent = try .init(alloc, state.terminal, action);
        let mut ev = VTEvent::init(alloc, terminal, action);
        // ev.seq = self.current_seq;
        ev.seq = self.current_seq;
        // errdefer ev.deinit(alloc);  — subsumed by Rust Drop on the early-return.

        // Check if the event passes the filter
        // if (!ev.passFilter(&self.filter_text)) { ev.deinit(alloc); return true; }
        if !ev.pass_filter(&self.filter_text) {
            ev.deinit(alloc);
            self.current_seq = self.current_seq.wrapping_add(1);
            return true;
        }

        // const max_capacity = 100;
        let max_capacity: usize = 100;
        // vt_events.append(ev) catch |err| switch (err) { ... };
        //
        // SAFETY (R1): `vt_events` is the raw `*mut VTEventRing` self-alias, live
        // for this capture window; every ring op below is a narrow `&mut *ptr`
        // reborrow — never a `&mut self.events` reference. See the module doc /
        // mirihar-inspector-vthandler.
        match unsafe { (*vt_events).append(ev.clone()) } {
            Ok(()) => {}
            // error.OutOfMemory => if (capacity() < max) grow-and-append else evict-and-append.
            Err(rustty_datastruct::circ_buf::Error::OutOfMemory) => {
                if unsafe { (*vt_events).capacity() } < max_capacity {
                    // const new_capacity = @min(capacity() * 2, max_capacity);
                    let new_capacity =
                        core::cmp::min(unsafe { (*vt_events).capacity() } * 2, max_capacity);
                    // try vt_events.resize(alloc, new_capacity); try vt_events.append(ev);
                    let _ = unsafe { (*vt_events).resize(new_capacity) };
                    let _ = unsafe { (*vt_events).append(ev) };
                } else {
                    // INV-9: element drop before slot reuse.
                    // var it = vt_events.iterator(.forward); if (it.next()) |old| old.deinit(alloc);
                    // vt_events.deleteOldest(1); try vt_events.append(ev);
                    //
                    // PORT NOTE (INV-9): `delete_oldest` overwrites the evicted
                    // slot with the default-fill (dropping the old event's owned
                    // description/metadata) before the slot is reused — subsuming
                    // the reference's explicit `old_ev.deinit` ahead of
                    // `deleteOldest(1)`.
                    unsafe { (*vt_events).delete_oldest(1) };
                    let _ = unsafe { (*vt_events).append(ev) };
                }
            }
        }

        // Do NOT skip it, because we want to record more information.
        // return false;
        self.current_seq = self.current_seq.wrapping_add(1);
        false
    }
}

// PORT NOTE: the reference VTHandler is consumed by `terminal.Stream(VTHandler)`
// via its `vtRaw`/`vt`/`deinit` decls. rustty-vt models that dispatch surface as
// the `Handler` trait (stream.rs); we impl it here. Only `vt_raw` carries
// behavior — the per-action `vt` hooks are the trait's default no-ops (matching
// the reference `vt` no-op).
impl Handler for VTHandler {
    // pub fn deinit(self: *VTHandler) void { _ = self; }
    // Required for the parser stream interface
    fn deinit(&mut self) {}

    // PORT NOTE (WS10 T14): this handler DOES declare `vtRaw` in the reference,
    // so `@hasDecl(T, "vtRaw")` is true for it and `1a88f3622`/`253e4f9c3`/
    // `300f42c7a` disable every CSI fast path at comptime for this stream —
    // "those handlers keep the exact previous behavior". `HAS_VT_RAW` is
    // rustty's spelling of that `@hasDecl`, and it MUST be set here: without
    // it, the fast paths would dispatch CSIs without producing a
    // `parser::Action`, and the inspector's VT-event log would silently lose
    // every CSI. Guarded by `stream_fast_paths_bypassed_when_vt_raw_hooked`.
    const HAS_VT_RAW: bool = true;

    // pub fn vtRaw(self, action) !bool — the raw-action hook (returns skip flag).
    fn vt_raw(&mut self, action: &Action) -> bool {
        self.vt_raw_impl(action)
    }
}

// ============================================================================
// ActionTag + ActionTagSet — `terminal.Parser.Action.Tag` + `EnumSet(Tag)`
// ============================================================================

/// `terminal.Parser.Action.Tag` (`std.meta.FieldEnum(Action)`). rustty-vt's
/// `Action` has no derived tag enum, so this is the explicit field-order tag set
/// the filter table + `filter_exclude` need.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ActionTag {
    Print,
    Execute,
    CsiDispatch,
    EscDispatch,
    OscDispatch,
    DcsHook,
    DcsPut,
    DcsUnhook,
    ApcStart,
    ApcPut,
    ApcEnd,
}

impl ActionTag {
    /// All tags in field order (for the `inline for` filter table).
    const ALL: &'static [ActionTag] = &[
        ActionTag::Print,
        ActionTag::Execute,
        ActionTag::CsiDispatch,
        ActionTag::EscDispatch,
        ActionTag::OscDispatch,
        ActionTag::DcsHook,
        ActionTag::DcsPut,
        ActionTag::DcsUnhook,
        ActionTag::ApcStart,
        ActionTag::ApcPut,
        ActionTag::ApcEnd,
    ];

    // std.meta.activeTag(action)
    fn of(action: &Action) -> ActionTag {
        match action {
            Action::Print(_) => ActionTag::Print,
            Action::Execute(_) => ActionTag::Execute,
            Action::CsiDispatch(_) => ActionTag::CsiDispatch,
            Action::EscDispatch(_) => ActionTag::EscDispatch,
            Action::OscDispatch(_) => ActionTag::OscDispatch,
            Action::DcsHook(_) => ActionTag::DcsHook,
            Action::DcsPut(_) => ActionTag::DcsPut,
            Action::DcsUnhook => ActionTag::DcsUnhook,
            Action::ApcStart => ActionTag::ApcStart,
            Action::ApcPut(_) => ActionTag::ApcPut,
            Action::ApcEnd => ActionTag::ApcEnd,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    // @tagName(tag) — snake_case field identifiers.
    fn tag_name(self) -> &'static CStr {
        match self {
            ActionTag::Print => c"print",
            ActionTag::Execute => c"execute",
            ActionTag::CsiDispatch => c"csi_dispatch",
            ActionTag::EscDispatch => c"esc_dispatch",
            ActionTag::OscDispatch => c"osc_dispatch",
            ActionTag::DcsHook => c"dcs_hook",
            ActionTag::DcsPut => c"dcs_put",
            ActionTag::DcsUnhook => c"dcs_unhook",
            ActionTag::ApcStart => c"apc_start",
            ActionTag::ApcPut => c"apc_put",
            ActionTag::ApcEnd => c"apc_end",
        }
    }
}

/// `std.EnumSet(terminal.Parser.Action.Tag)` — a per-tag membership set (the
/// apc/mod.rs `EnumSet ⇒ [bool; COUNT]` precedent).
#[derive(Clone, Copy)]
struct ActionTagSet {
    bits: [bool; 11],
}

impl ActionTagSet {
    // .initMany(&.{...})
    fn init_many(tags: &[ActionTag]) -> ActionTagSet {
        let mut s = ActionTagSet { bits: [false; 11] };
        for &t in tags {
            s.bits[t.index()] = true;
        }
        s
    }
    // set.contains(tag)
    fn contains(&self, tag: ActionTag) -> bool {
        self.bits[tag.index()]
    }
    // set.insert(tag)
    fn insert(&mut self, tag: ActionTag) {
        self.bits[tag.index()] = true;
    }
    // set.remove(tag)
    fn remove(&mut self, tag: ActionTag) {
        self.bits[tag.index()] = false;
    }
}

#[cfg(test)]
mod tests {
    //! KATs for the STEP-1 metadata encoder (the `@typeInfo` hand-expansion) and
    //! the description encoders. These exercise the pure encoding path (no ImGui /
    //! no Terminal), asserting the reference's per-type `raw_description` /
    //! `key=value` metadata output.
    use super::*;
    use rustty_vt::osc::Command as C;

    // Encode an action's description into a plain String (drop the trailing NUL).
    fn desc(action: &Action) -> String {
        let mut md = Metadata::new();
        let mut buf = String::new();
        encode_action(&mut buf, &mut md, action);
        buf
    }

    // Encode only the OSC metadata into a Vec of (key, value) String pairs.
    fn osc_md(osc: &C) -> Vec<(String, String)> {
        let mut md = Metadata::new();
        encode_metadata_osc(&mut md, osc);
        md.entries
            .iter()
            .map(|(k, v)| {
                // strip the trailing NUL sentinel `dupe_z` appends.
                let ks = String::from_utf8_lossy(&k[..k.len() - 1]).into_owned();
                let vs = String::from_utf8_lossy(&v[..v.len() - 1]).into_owned();
                (ks, vs)
            })
            .collect()
    }

    #[test]
    fn encode_print_and_execute() {
        // "'{u}' (U+{X})"
        assert_eq!(desc(&Action::Print('A' as u32)), "'A' (U+41)");
        // control-char name + " (0x{X})"
        assert_eq!(desc(&Action::Execute(0x07)), "BEL (0x7)");
        assert_eq!(desc(&Action::Execute(0x0A)), "LF (0xA)");
        assert_eq!(desc(&Action::Execute(0x99)), "? (0x99)");
    }

    #[test]
    fn encode_csi_and_esc() {
        let csi = rustty_vt::parser::CSI {
            intermediates: rustty_vt::parser::InlineVec::from_slice(b"?"),
            params: rustty_vt::parser::InlineVec::from_slice(&[1, 49]),
            params_sep: rustty_vt::parser::SepList::init_empty(),
            final_: b'h',
        };
        // "{c} " per intermediate, "{d}" params `;`-joined, trailing space, final.
        assert_eq!(desc(&Action::CsiDispatch(Box::new(csi))), "? 1;49 h");

        let esc = rustty_vt::parser::ESC {
            intermediates: rustty_vt::parser::InlineVec::from_slice(b"("),
            final_: b'B',
        };
        assert_eq!(desc(&Action::EscDispatch(Box::new(esc))), "( B");
    }

    #[test]
    fn osc_tag_prefix_in_description() {
        let osc = C::ChangeWindowTitle(b"hi".to_vec());
        // encodeOSC writes "{tag} " then the metadata is separate.
        assert_eq!(
            desc(&Action::OscDispatch(Box::new(osc))),
            "change_window_title "
        );
    }

    #[test]
    fn metadata_byte_slice_payload_is_data() {
        // change_window_title: [:0]const u8 ⇒ md.put("data", v)
        let md = osc_md(&C::ChangeWindowTitle(b"my title".to_vec()));
        assert_eq!(md, vec![("data".into(), "my title".into())]);
    }

    #[test]
    fn metadata_struct_payload_per_field() {
        // clipboard_contents: struct { kind: u8, data: [:0]const u8 }
        let md = osc_md(&C::ClipboardContents {
            kind: b'c',
            data: b"xyz".to_vec(),
        });
        assert_eq!(
            md,
            vec![("kind".into(), "99".into()), ("data".into(), "xyz".into())]
        );
    }

    #[test]
    fn metadata_optional_and_enum_fields() {
        // hyperlink_start: struct { id: ?[:0]const u8, uri: [:0]const u8 }
        let some = osc_md(&C::HyperlinkStart {
            id: Some(b"7".to_vec()),
            uri: b"http://x".to_vec(),
        });
        assert_eq!(
            some,
            vec![("id".into(), "7".into()), ("uri".into(), "http://x".into())]
        );
        let none = osc_md(&C::HyperlinkStart {
            id: None,
            uri: b"u".to_vec(),
        });
        assert_eq!(
            none,
            vec![("id".into(), "(unset)".into()), ("uri".into(), "u".into())]
        );
    }

    #[test]
    fn metadata_union_payload_tag_and_value() {
        // conemu_change_tab_title: union(enum) { reset, value: [:0]const u8 }
        let reset = osc_md(&C::ConemuChangeTabTitle(
            rustty_vt::osc::ConemuChangeTabTitle::Reset,
        ));
        // active-void ⇒ md.put("data", tag_name)
        assert_eq!(reset, vec![("data".into(), "reset".into())]);

        let value = osc_md(&C::ConemuChangeTabTitle(
            rustty_vt::osc::ConemuChangeTabTitle::Value(b"tab".to_vec()),
        ));
        // active-payload ⇒ md.put("value", bytes)
        assert_eq!(value, vec![("value".into(), "tab".into())]);
    }

    #[test]
    fn metadata_optional_bool_fields() {
        // conemu_xterm_emulation: struct { keyboard: ?bool, output: ?bool }
        let md = osc_md(&C::ConemuXtermEmulation {
            keyboard: Some(true),
            output: None,
        });
        assert_eq!(
            md,
            vec![
                ("keyboard".into(), "true".into()),
                ("output".into(), "(unset)".into()),
            ]
        );
    }

    #[test]
    fn void_payload_has_no_metadata() {
        assert!(osc_md(&C::HyperlinkEnd).is_empty());
        assert!(osc_md(&C::Invalid).is_empty());
        assert!(osc_md(&C::ConemuWaitInput).is_empty());
    }

    #[test]
    fn action_tag_set_roundtrip() {
        let mut s = ActionTagSet::init_many(&[ActionTag::Print]);
        assert!(s.contains(ActionTag::Print));
        assert!(!s.contains(ActionTag::OscDispatch));
        s.insert(ActionTag::OscDispatch);
        assert!(s.contains(ActionTag::OscDispatch));
        s.remove(ActionTag::Print);
        assert!(!s.contains(ActionTag::Print));
        // activeTag mapping
        assert_eq!(ActionTag::of(&Action::Print(0)), ActionTag::Print);
        assert_eq!(ActionTag::of(&Action::DcsUnhook), ActionTag::DcsUnhook);
    }
}
