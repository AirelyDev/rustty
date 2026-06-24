//! rustty-vt
//!
//! PORTS: ghostty src/terminal/ (all except terminal/c/) + the VT input slice
//! (input/key, key_encode, mouse, mouse_encode, paste) + stb/ shim. The crown jewel
//! (2001 ported tests, ~62% of the port). Residual font/global leaks ride `sys` hooks.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
//!
//! This crate mirrors the reference ghostty `terminal` module, whose surface is
//! defined by `src/terminal/main.zig` (the terminal engine) plus the VT input
//! slice (`src/input/{key,key_encode,mouse,mouse_encode,paste}.zig`). The
//! module structure below is the verbatim-retargeted translation of
//! `terminal/main.zig`'s `@import`/`pub const` surface: every top-level
//! `terminal/*.zig` file gets a `pub mod`, and every `pub const` re-export gets
//! a `pub use`. Re-exports whose target type is not yet ported (it lives in a
//! sibling file ported by a later vt sub-batch) are kept verbatim as commented
//! lines so this skeleton compiles (a structural blocker would be reported
//! otherwise). The full surface goes live as the leaves land, leaves-first.
//!
//! NOTE: `terminal/size.zig` is NOT in this crate — its types are hoisted into
//! `rustty-render-types` (ambiguity A1) and imported from `rustty_render_types`.
//! `terminal/c/` is NOT here either — that is the `rustty-vt-c` crate.

// const charsets = @import("charsets.zig");
pub mod charsets;
// const stream = @import("stream.zig");
pub mod stream;
// const ansi = @import("ansi.zig");
pub mod ansi;
// const csi = @import("csi.zig");
pub mod csi;
// const render = @import("render.zig");
pub mod render;
// const stream_terminal = @import("stream_terminal.zig");
pub mod stream_terminal;
// const style = @import("style.zig");
pub mod style;
// pub const apc = @import("apc.zig");
pub mod apc;
// pub const dcs = @import("dcs.zig");
pub mod dcs;
// pub const osc = @import("osc.zig");
pub mod osc;
// pub const point = @import("point.zig");
pub mod point;
// pub const color = @import("color.zig");
pub mod color;
// PORT NOTE: `terminal/cursor.zig` has no top-level re-export in `main.zig`
// (it is an internal sibling imported by `Screen.zig` for the cursor `Style`
// enum). Declared here so `screen` can reference it.
pub mod cursor;
// pub const device_attributes = @import("device_attributes.zig");
pub mod device_attributes;
// pub const device_status = @import("device_status.zig");
pub mod device_status;
// pub const focus = @import("focus.zig");
pub mod focus;
// pub const formatter = @import("formatter.zig");
pub mod formatter;
// pub const highlight = @import("highlight.zig");
pub mod highlight;
// PORT NOTE: `terminal/hyperlink.zig` has no top-level re-export in `main.zig`
// (it is an internal sibling of `page.zig`, with which it forms a mutually
// recursive unit: each `@import`s the other). It is ported alongside `page.rs`
// in this sub-batch because `Page` embeds `hyperlink.Map`/`hyperlink.Set` and
// many `Page` methods operate on hyperlink entries. Declared here so `page`
// can reference it.
pub mod hyperlink;
// pub const kitty = @import("kitty.zig");
pub mod kitty;
// pub const modes = @import("modes.zig");
pub mod modes;
// pub const page = @import("page.zig");
pub mod page;
// pub const parse_table = @import("parse_table.zig");
pub mod parse_table;
// pub const search = @import("search.zig");
pub mod search;
// pub const sgr = @import("sgr.zig");
pub mod sgr;
// pub const size = @import("size.zig");
// PORT NOTE: size types are hoisted into rustty-render-types (A1); the reference
// `terminal/size.zig` is re-exported there. Use `rustty_render_types::term_size`.
pub use rustty_render_types::term_size as size;
// pub const size_report = @import("size_report.zig");
pub mod size_report;
// pub const sys = @import("sys.zig");
pub mod sys;
// PORT NOTE: `terminal/Tabstops.zig` has no top-level re-export in `main.zig`
// (it is an internal sibling imported by `Terminal.zig` for tab handling).
// Declared here so the terminal can reference it; ported as a leaf in this
// sub-batch.
pub mod tabstops;
// pub const tmux = if (options.tmux_control_mode) @import("tmux.zig") else struct {};
pub mod tmux;
// pub const x11_color = @import("x11_color.zig");
pub mod x11_color;

// pub const Charset = charsets.Charset;
// pub const CharsetSlot = charsets.Slots;
// pub const CharsetActiveSlot = charsets.ActiveSlot;
// pub const charsetTable = charsets.table;
// pub const Cell = page.Cell;
// pub const Coordinate = point.Coordinate;
// pub const CSI = Parser.Action.CSI;
// pub const DCS = Parser.Action.DCS;
// pub const mouse = @import("mouse.zig");
pub mod mouse;
// pub const MouseEvent = mouse.Event;
// pub const MouseFormat = mouse.Format;
// pub const MouseShape = mouse.Shape;
// pub const Page = page.Page;
// pub const PageList = @import("PageList.zig");
pub mod pagelist;
pub use pagelist::PageList;
// pub const Parser = @import("Parser.zig");
pub mod parser;
// pub const Pin = PageList.Pin;
// pub const Point = point.Point;
// pub const RenderState = render.RenderState;
// pub const Screen = @import("Screen.zig");
pub mod screen;
// pub const ScreenSet = @import("ScreenSet.zig");
pub mod screen_set;
// pub const Scrollbar = PageList.Scrollbar;
// pub const Selection = @import("Selection.zig");
pub mod selection;
pub mod selection_codepoints;
// pub const SelectionGesture = @import("SelectionGesture.zig");
pub mod selection_gesture;
// pub const SizeReportStyle = csi.SizeReportStyle;
// pub const StringMap = @import("StringMap.zig");
pub mod string_map;
// pub const Style = style.Style;
// pub const Terminal = @import("Terminal.zig");
pub mod terminal;
// PORT NOTE: `terminal/UTF8Decoder.zig` has no top-level re-export in `main.zig`
// (it is an internal sibling imported by the stream path). Declared here so the
// stream can reference it; ported as a leaf in this sub-batch.
pub mod utf8_decoder;
// pub const TerminalStream = stream_terminal.Stream;
// pub const Stream = stream.Stream;
// pub const StreamAction = stream.Action;
// pub const Cursor = Screen.Cursor;
// pub const CursorStyle = Screen.CursorStyle;
// pub const CursorStyleReq = ansi.CursorStyle;
// pub const DeviceAttributeReq = device_attributes.Req;
// pub const Mode = modes.Mode;
// pub const ModePacked = modes.ModePacked;
// pub const ModifyKeyFormat = ansi.ModifyKeyFormat;
// pub const ProtectedMode = ansi.ProtectedMode;
// pub const StatusLineType = ansi.StatusLineType;
// pub const StatusDisplay = ansi.StatusDisplay;
// pub const EraseDisplay = csi.EraseDisplay;
// pub const EraseLine = csi.EraseLine;
// pub const TabClear = csi.TabClear;
// pub const Attribute = sgr.Attribute;

// pub const Options = @import("build_options.zig").Options;
pub mod build_options;
// pub const options = @import("terminal_options");
// PORT NOTE: `terminal_options` is a build-generated module in the reference;
// the ported equivalent lives in this crate as `options` (a later sub-batch
// wires the build-time option values).
pub mod options;

// /// This is set to true when we're building the C library.
// pub const c_api = if (options.c_abi) @import("c/main.zig") else void;
// PORT NOTE: the C ABI surface (terminal/c/) is the separate `rustty-vt-c` crate.

// ---------------------------------------------------------------------------
// VT input slice (ghostty src/input/*). `terminal/main` re-exports `focus`
// from terminal; the remaining input primitives that belong to the VT engine
// are ported here: input/{key, key_encode, mouse, mouse_encode, paste}.zig.
// PORT NOTE: these are grouped under an `input` module to mirror the reference
// `src/input/` directory.
// ---------------------------------------------------------------------------
pub mod input;

// test {
//     @import("std").testing.refAllDecls(@This());
//
//     // Internals
//     _ = @import("bitmap_allocator.zig");
//     _ = @import("hash_map.zig");
//     _ = @import("ref_counted_set.zig");
//     _ = @import("size.zig");
// }
// PORT NOTE: the internal-only modules referenced by the reference root test
// block (bitmap_allocator, hash_map, ref_counted_set) are private internals
// ported by later vt sub-batches.
pub mod bitmap_allocator;
pub mod hash_map;
pub mod ref_counted_set;
