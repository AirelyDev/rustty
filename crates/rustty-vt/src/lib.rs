//! rustty-vt
//!
//! PORTS: ghostty src/terminal/ (all except terminal/c/) + the VT input slice
//! (input/key, key_encode, mouse, mouse_encode, paste) + stb/ shim. The crown jewel
//! (2001 ported tests, ~62% of the port). Residual font/global leaks ride `sys` hooks.
//!
//! PORT STATUS: done (flipped per VALIDATION_REPORT.md M1 after the H4
//! matcher fix; every terminal/*.zig + VT-input-slice module is landed with
//! its tests. The single live seam is `input/key.rs` `imguiKey` — apprt
//! dcimgui binding, tracked at DEFERRALS.md "imguiKey".)
//!
//! (R1: every line/comment/test{} ported as the crate was carved,
//! leaves-first, under the acyclicity gate per R4.)
//!
//! This crate mirrors the reference ghostty `terminal` module, whose surface is
//! defined by `src/terminal/main.zig` (the terminal engine) plus the VT input
//! slice (`src/input/{key,key_encode,mouse,mouse_encode,paste}.zig`). The
//! module structure below is the verbatim-retargeted translation of
//! `terminal/main.zig`'s `@import`/`pub const` surface: every top-level
//! `terminal/*.zig` file gets a `pub mod`, and every `pub const` re-export is
//! carried here as its verbatim reference line. Those aliases whose Rust
//! spelling adds something the module path does not (a root name the port's
//! consumers or the reference's own `lib_vt.zig` surface use) additionally get a
//! `pub use`; the rest stay comment-only, reachable at `module::Type`, because a
//! Rust `pub use` of a type that is already public at its module path is a
//! second name for the same item rather than the single canonical name a Zig
//! `pub const` gives.
//!
//! NOTE: `terminal/size.zig` is NOT in this crate — its types are hoisted into
//! `rustty-render-types` (ambiguity A1) and imported from `rustty_render_types`.
//! `terminal/c/` is NOT here either — that is the `rustty-vt-c` crate.

// const charsets = @import("charsets.zig");
pub mod charsets;
// const stream = @import("stream.zig");
pub mod stream;
// PORT NOTE: like `utf8_decoder`, `terminal/stream_continuation.zig` has no
// top-level re-export in `main.zig` (it is an internal sibling imported by
// `stream.zig`). Declared here so the stream can reference it.
pub mod stream_continuation;
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
pub mod clipboard;
pub mod color;
// pub const compress = @import("compress.zig");
pub mod compress;
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
// PORT NOTE: no upstream counterpart — `fixed_writer` is the stand-in for Zig's
// `std.Io.Writer.fixed`, which the reference uses to encode VT responses into
// stack buffers of a statically-known maximum size. See the module docs.
pub mod fixed_writer;
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
// pub const mem = @import("mem.zig");
pub mod mem;

/// Whether this target supports terminal page compression.
///
/// PORTS: oracle@main src/terminal/main.zig
/// `pub const compression_enabled = @import("mem.zig").canReclaim(.strict);`
///
/// PORT NOTE (WS7): `can_reclaim` is a `cfg!`-chain shaped like a `const fn`,
/// but Rust cannot evaluate it in a `const` initializer while it reads
/// `cfg!(test)`, so this is a function. Call sites use
/// `if rustty_vt::compression_enabled()` exactly where the oracle writes
/// `if (comptime terminalpkg.compression_enabled)`.
#[inline]
pub fn compression_enabled() -> bool {
    mem::can_reclaim(mem::DecommitMode::Strict)
}

// pub const page = @import("page.zig");
pub mod page;
// pub const parse_table = @import("parse_table.zig");
pub mod parse_table;
// pub const search = @import("search.zig");
pub mod search;
// pub const snapshot = @import("snapshot/main.zig");
// PORT NOTE (phase-4 wsnap-1): live for the C1+C2 stage (scaffold +
// page/style/hyperlink records); `snapshot/main.zig` maps to `snapshot/mod.rs`,
// whose PORT STATUS lists the next-stage modules (grid, screen, terminal,
// history, codec drivers). Its PageList-side enablers (`pagelist::Builder`,
// `pagelist::PageAllocation`, `PageList::allocate_page`, the RefCountedSet
// iterator) are already ported below.
pub mod snapshot;
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

// lib_vt.zig: pub const unicode = struct {
//     const unicode_pkg = @import("unicode/main.zig");
//     pub const codepointWidth = unicode_pkg.codepointWidth;
//     pub const graphemeWidth = unicode_pkg.graphemeWidth;
// };
// PORT NOTE: `terminal/fraction.zig` (the custom decimal-fraction parser from
// upstream 946422dbc that replaced `std.fmt.parseFloat`) has no top-level
// re-export in `main.zig`; there it is referenced only via
// `_ = @import("fraction.zig")` for test discovery and used internally by
// `color.zig`. Ported here as a leaf; the call-site switch is a later wave.
pub mod fraction;

// PORT NOTE: `src/fastprint.zig` is a top-level (non-terminal) helper imported
// by `formatter.zig` and `style.zig` as `@import("../fastprint.zig")`. It has
// no `main.zig` surface entry; ported here as a leaf sibling. The call-site
// switch is a later wave.
pub mod fastprint;

/// Unicode utilities that match the terminal's text layout semantics.
pub mod unicode {
    pub use rustty_unicode::codepoint_width;
    pub use rustty_unicode::grapheme::{grapheme_width, GraphemeWidth, GraphemeWidthEffect};
}

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
pub use pagelist::grow::Scrollbar;
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
pub use stream_terminal::Stream as TerminalStream;
// pub const Stream = stream.Stream;
pub use stream::Stream;
// pub const StreamAction = stream.Action;
// PORT NOTE: `stream.Action` has NO port counterpart, so there is nothing for
// `StreamAction` to name. The reference `Action` is a tagged union whose fields
// are the dispatch surface; this port models that surface directly as the
// `stream::Handler` trait (one method per union field) plus the payload structs,
// with the C-ABI mirror of the union living in `rustty-vt-c` — see the PORT NOTE
// at `stream.rs` ("The possible actions that can be emitted by the Stream"). The
// omission of a root `StreamAction` is therefore a consequence of that settled
// design fork, not an unported alias.
// pub const UnknownSequence = stream_terminal.Handler.UnknownSequence;
pub use stream_terminal::UnknownSequence;
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
// terminal/lib.zig — the terminal lib's C-ABI plumbing hub, kept verbatim
// (0 test blocks; its decls are consumed via `rustty-lib` where the terminal/c
// projections need them, so no live Rust module is required here).
// ---------------------------------------------------------------------------
// const std = @import("std");
// const build_options = @import("terminal_options");
// const lib = @import("../lib/main.zig");
//
// /// The target for the terminal lib in particular.
// pub const target: lib.Target = if (build_options.c_abi) .c else .zig;
//
// /// The calling convention to use for C APIs.
// ///
// /// This is always .c for now. I want to make this "Zig" when we're not
// /// building the C ABI but there are bigger issues we need to resolve to
// /// make that possible (change it and see for yourself).
// pub const calling_conv: std.builtin.CallingConvention = .c;
//
// /// Forwarded decls from lib that are used.
// pub const alloc = lib.allocator;
// pub const Buffer = lib.Buffer;
// pub const Enum = lib.Enum;
// pub const TaggedUnion = lib.TaggedUnion;
// pub const Struct = lib.Struct;
// pub const String = lib.String;
// pub const checkGhosttyHEnum = lib.checkGhosttyHEnum;
// pub const structSizedFieldFits = lib.structSizedFieldFits;
//
// PORT NOTE: `target`/`calling_conv` are Zig C-ABI build plumbing with no Rust
// analogue in this pure crate (`extern "C"` declarations live in
// `rustty-vt-c`); the forwarded `lib` decls are re-exported from the
// `rustty-lib` crate at their use sites (e.g. `osc` uses
// `rustty_lib::r#enum::check_ghostty_henum`).

// ---------------------------------------------------------------------------
// stb/main.zig — the stb image C-lib shim the reference kitty-graphics image
// decode uses, kept verbatim (0 test blocks). The port replaces the stb C
// dependency with the `sys::decode_png` seam (the wuffs binding rides a later
// FFI crate; see kitty/graphics_image.rs), so no Rust module is required here.
// ---------------------------------------------------------------------------
// const c = @cImport({
//     @cInclude("stb_image.h");
//     @cInclude("stb_image_resize.h");
// });
//
// We'll just add the exports of the functions or types we actually use
// here, no need to export everything from the C lib if we don't use it.
// pub const stbi_load_from_memory = c.stbi_load_from_memory;
// pub const stbi_image_free = c.stbi_image_free;
// pub const stbir_resize_uint8 = c.stbir_resize_uint8;

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

// PORT NOTE: port of the reference AFL++ harness package
// `test/fuzz-libghostty/src/fuzz_{osc,parser,stream}.zig` (which lives OUTSIDE
// the terminal module in the reference tree). Declared here so the cargo-fuzz
// targets (`crates/rustty-vt/fuzz/`) and the deterministic corpus-replay test
// (`tests/fuzz_corpus_replay.rs`) share one set of harness entry points.
// Test/fuzz infrastructure only — never called by production code.
pub mod fuzz_harness;
