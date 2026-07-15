//! rustty-cli::boo
//!
//! PORTS: ghostty src/cli/boo.zig (0t) — the `+boo` action.
//!
//! PORT STATUS: LIVE (was DEFERRALS.md "cli C5" — the crate's last vaxis-deferred
//! leaf). `boo` is a 100% vaxis TUI animation (the ghost animation from the
//! website). It now runs over the ported vxfw widget-framework subset
//! (`rustty_vaxis::vxfw::{App, Text, Center}`) and the reproduced `framedata`
//! compressed frame asset (build.rs raw-DEFLATEs `assets/framegen/frames/*.txt`
//! into `$OUT_DIR/framedata.compressed`; decompressed here with
//! `zlib_sys::inflate_raw`). The R2 product-name comments ("Ghostty website")
//! retarget to rustty.
//!
//! ALIASING (H1/H3, mirihar-vxfw): `Boo` is driven type-erased — `*mut Boo` in the
//! Widget's `userdata`, cast back per tick/draw. The draw produces a `Surface` whose
//! `buffer` is a RAW alias into `self.buffer`, stored as `App.mouse_handler.last_frame`
//! across the next tick's `update_frame`. `update_frame` therefore takes a `*mut Boo`
//! and writes cells through the buffer's OWN raw data pointer (`Vec::as_mut_ptr`, which
//! retags only the `Vec` field, never a `&mut [Cell]` over the heap the alias points
//! at). No whole-`&mut Boo` ever spans the outstanding buffer alias. Proven on
//! `.port/phase-2/mirihar-vxfw` under Stacked + Tree Borrows.

use std::os::raw::c_void;

use rustty_args::args;
use rustty_vaxis::cell::{Cell, Character, Color, Style};
use rustty_vaxis::key::{self, Key, KeyOwned, Modifiers};
use rustty_vaxis::vxfw::{
    App, Center, DrawContext, Error as VxfwError, Event, EventContext, RelativePoint, Size,
    SubSurface, Surface, Text, Widget,
};

use crate::rustty::Error;

// const framedata = @import("framedata").compressed;
//   The compressed asset generated at build time (build.rs raw-DEFLATEs the vendored
//   frame `.txt` files); `@embedFile` → `include_bytes!` from `$OUT_DIR`.
const FRAMEDATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/framedata.compressed"));

/// Generous cap on the decompressed frame data (the real payload is ~1.39 MB).
const MAX_DECOMPRESSED: usize = 8 * 1024 * 1024;

/// The `boo` command is used to display the animation from the Rustty website in
/// the terminal.
// pub const Options = struct {
//     pub fn deinit(self: Options) void { _ = self; }
//     /// Enables `-h` and `--help` to work.
//     pub fn help(self: Options) !void { _ = self; return Action.help_error; }
// };
#[derive(Default)]
pub struct Options {}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    fn deinit(&self) {}
}

impl args::Target for Options {
    // The reference `Options` has no non-`_` fields, so `parseIntoField` always
    // reports an unknown field.
    fn parse_into_field(
        &mut self,
        _key: &[u8],
        _value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        Err(args::Error::InvalidField)
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        Some(Err(args::Error::ActionHelpRequested))
    }
}

// Width of a single frame
const FRAME_WIDTH: usize = 100;
// Height of a single frame
const FRAME_HEIGHT: usize = 41;

/// The parser state for `update_frame`'s HTML-span walk.
//     const State = enum { normal, span, in_tag, in_closing_tag };
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Span,
    InTag,
    InClosingTag,
}

// const Boo = struct { frame: u8, framerate: u32, buffer: [...]vaxis.Cell,
//   ghostty_style: vaxis.Style, outline_style: vaxis.Style };
struct Boo<'a> {
    // frame: u8,
    frame: u8,
    // framerate: u32, // 30 fps
    framerate: u32,
    // We know the size of this at compile time, but we heap allocate the slice to prevent the
    // binary from increasing too much in size
    // buffer: [frame_width * frame_height]vaxis.Cell,
    //   PORT-NOTE (H3): a heap `Vec<Cell>` (len 4100). Accessed for aliasing/writes via
    //   its OWN raw data pointer (`as_ptr`/`as_mut_ptr`), never a `&mut [Cell]`.
    buffer: Vec<Cell<'a>>,
    // ghostty_style: vaxis.Style,
    ghostty_style: Style,
    // outline_style: vaxis.Style,
    outline_style: Style,
    /// The decompressed frames (the reference's global `frames`, held by reference so
    /// the grapheme bytes borrow `decompressed_data` for `'a`).
    frames: &'a [&'a [u8]],
}

impl<'a> Boo<'a> {
    // fn widget(self: *Boo) vxfw.Widget { .{ .userdata = self, .eventHandler = ..., .drawFn = ... } }
    fn widget(&self) -> Widget<'a> {
        Self::widget_from_ptr(self as *const Boo<'a> as *mut Boo<'a>)
    }

    /// Build the `Widget` from a raw `*mut Boo` (the type-erased userdata). Used by
    /// `widget()` and by the handlers to reconstruct `self.widget()` from `ptr`
    /// without forming a `&Boo`/`&mut Boo` over the aliased buffer.
    fn widget_from_ptr(ptr: *mut Boo<'a>) -> Widget<'a> {
        Widget {
            userdata: ptr as *mut c_void,
            capture_handler: None,
            event_handler: Some(Boo::type_erased_event_handler),
            draw_fn: Boo::type_erased_draw_fn,
        }
    }

    // fn typeErasedEventHandler(ptr: *anyopaque, ctx: *vxfw.EventContext, event: vxfw.Event) anyerror!void {
    fn type_erased_event_handler(
        ptr: *mut c_void,
        ctx: &mut EventContext<'a>,
        event: Event<'a>,
    ) -> Result<(), VxfwError> {
        // const self: *Boo = @ptrCast(@alignCast(ptr));
        let self_ptr = ptr as *mut Boo<'a>;
        match event {
            // .init, .tick => {
            Event::Init | Event::Tick => {
                // self.updateFrame();
                // SAFETY (H1/H3): raw-self update writing through the buffer's own
                // data pointer; never a whole-`&mut Boo` spanning the buffer alias.
                unsafe { Boo::update_frame(self_ptr) };
                // ctx.redraw = true;
                ctx.redraw = true;
                // return ctx.tick(self.framerate, self.widget());
                let framerate = unsafe { (*self_ptr).framerate };
                let widget = Boo::widget_from_ptr(self_ptr);
                ctx.tick(framerate, widget)
            }
            // .key_press => |key| {
            Event::KeyPress(key) => {
                // if (key.matches('c', .{ .ctrl = true }) or key.matches(vaxis.Key.escape, .{}))
                let k = borrow_key(&key);
                let ctrl = Modifiers {
                    ctrl: true,
                    ..Default::default()
                };
                let none = Modifiers::default();
                if k.matches('c' as u32, ctrl) || k.matches(key::escape, none) {
                    // ctx.quit = true; return;
                    ctx.quit = true;
                }
                Ok(())
            }
            // else => {},
            _ => Ok(()),
        }
    }

    // fn typeErasedDrawFn(ptr: *anyopaque, ctx: vxfw.DrawContext) Allocator.Error!vxfw.Surface {
    fn type_erased_draw_fn(ptr: *mut c_void, ctx: DrawContext) -> Result<Surface<'a>, VxfwError> {
        // const self: *Boo = @ptrCast(@alignCast(ptr));
        let self_ptr = ptr as *mut Boo<'a>;
        // const max = ctx.max.size();
        let max = ctx.max.size();

        // Warn for screen size
        // if (max.width < frame_width or max.height < frame_height) {
        if (max.width as usize) < FRAME_WIDTH || (max.height as usize) < FRAME_HEIGHT {
            // const text: vxfw.Text = .{ .text = "Screen must be at least 100w x 41h" };
            let text = Text {
                text: b"Screen must be at least 100w x 41h",
                ..Text::default()
            };
            // const center: vxfw.Center = .{ .child = text.widget() };
            let center = Center {
                child: text.widget(),
            };
            // return center.draw(ctx);
            return center.draw(ctx);
        }

        // Calculate x and y offsets to center the animation frame
        // const offset_y = (max.height - frame_height) / 2;
        let offset_y = (max.height as usize - FRAME_HEIGHT) / 2;
        // const offset_x = (max.width - frame_width) / 2;
        let offset_x = (max.width as usize - FRAME_WIDTH) / 2;

        // Create the animation surface
        // const child: vxfw.Surface = .{ .size = ..., .widget = self.widget(),
        //   .buffer = &self.buffer, .children = &.{} };
        //   PORT-NOTE (H1/H3): `&self.buffer` is a RAW alias into `self.buffer`'s heap
        //   data, taken via `Vec::as_ptr` (shared `&Vec` field, no `&[Cell]` over the
        //   heap). Stored across the next tick — see the module ALIASING note.
        let buf_data: *mut Cell<'a> = unsafe { (*self_ptr).buffer.as_ptr() as *mut Cell<'a> };
        let buffer_alias: *mut [Cell<'a>] =
            core::ptr::slice_from_raw_parts_mut(buf_data, FRAME_WIDTH * FRAME_HEIGHT);
        let child = Surface {
            size: Size {
                width: FRAME_WIDTH as u16,
                height: FRAME_HEIGHT as u16,
            },
            widget: Boo::widget_from_ptr(self_ptr),
            cursor: None,
            buffer: buffer_alias,
            children: rustty_vaxis::vxfw::empty_children(),
        };

        // Allocate a slice of child surfaces
        // var children = try ctx.arena.alloc(vxfw.SubSurface, 1);
        // SAFETY: `ctx.arena` is a live arena; we init `children[0]` before return.
        let children: *mut [SubSurface<'a>] =
            unsafe { (*ctx.arena).alloc_slice::<SubSurface<'a>>(1) };
        // children[0] = .{ .origin = .{ .row = offset_y, .col = offset_x }, .surface = child };
        unsafe {
            children.cast::<SubSurface<'a>>().write(SubSurface {
                origin: RelativePoint {
                    row: offset_y as i32,
                    col: offset_x as i32,
                },
                surface: child,
                z_index: 0,
            });
        }

        // return .{ .size = max, .widget = self.widget(), .buffer = &.{}, .children = children };
        Ok(Surface {
            size: max,
            widget: Boo::widget_from_ptr(self_ptr),
            cursor: None,
            buffer: rustty_vaxis::vxfw::empty_cells(),
            children,
        })
    }

    /// Updates our internal buffer with the current frame, then advances the frame index
    ///
    /// # Safety
    /// `ptr` must be a valid `*mut Boo` whose `buffer` is the aliased animation buffer.
    /// Writes go through the buffer's own data pointer (narrow, never `&mut [Cell]`).
    // fn updateFrame(self: *Boo) void {
    unsafe fn update_frame(ptr: *mut Boo<'a>) {
        // const frame = frames[self.frame];
        // Read the `frames` reference + index out by value (no autoref on the raw deref).
        let frames: &'a [&'a [u8]] = unsafe { (*ptr).frames };
        let frame_idx = unsafe { (*ptr).frame } as usize;
        let frame: &'a [u8] = frames[frame_idx];
        // A frame is characters with html spans. When we encounter a span, we use the outline style
        // until the span ends. That is, when we find a '<', we parse until '>'. Then we use the
        // outline styule until the next '<', and skip until the next '>'
        let ghostty_style = unsafe { (*ptr).ghostty_style };
        let outline_style = unsafe { (*ptr).outline_style };
        // The buffer's OWN raw data pointer via an explicit `&mut Vec` (retags only the
        // `Vec` field, NOT a `&mut [Cell]` over the aliased heap — H3). NLL ends this
        // borrow immediately (`buf_data`/`buf_len` are `Copy`), so the loop's raw writes
        // and the later `frame` field update are unencumbered.
        let buf_ref: &mut Vec<Cell<'a>> = unsafe { &mut (*ptr).buffer };
        let buf_data: *mut Cell<'a> = buf_ref.as_mut_ptr();
        let buf_len = buf_ref.len();

        // var cell_idx: usize = 0;
        let mut cell_idx: usize = 0;

        // var line_iter = std.mem.splitScalar(u8, frame, '\n');
        for line in frame.split(|&b| b == b'\n') {
            // var state: State = .normal;
            let mut state = State::Normal;
            // var style = self.ghostty_style;
            let mut style = ghostty_style;
            // var cp_iter: std.unicode.Utf8Iterator = .{ .bytes = line, .i = 0 };
            let mut i = 0usize;
            // while (cp_iter.nextCodepointSlice()) |char| {
            while i < line.len() {
                let clen = utf8_codepoint_len(line[i]);
                let end = (i + clen).min(line.len());
                // char is a codepoint slice of `line`, recovered as a `&'a [u8]` into
                // the owning `frame` (preserving the zero-copy grapheme borrow).
                let char: &'a [u8] = frame_slice(frame, line, i, end);
                i = end;
                match state {
                    // .normal => if (std.mem.eql(u8, "<", char)) { state = .in_tag; style = outline; continue; }
                    State::Normal => {
                        if char == b"<" {
                            state = State::InTag;
                            // We will be entering a span
                            style = outline_style;
                            continue;
                        }
                    }
                    // .span => if (std.mem.eql(u8, "<", char)) { state = .in_tag; style = ghostty; continue; }
                    State::Span => {
                        if char == b"<" {
                            state = State::InTag;
                            style = ghostty_style;
                            continue;
                        }
                    }
                    // .in_tag => { if "/" -> in_closing_tag else if ">" -> span; continue; }
                    State::InTag => {
                        // If we encounter a '/', we are a closing tag
                        // If we parse all the way to a '>' we are an opening tag: we are now in a span
                        if char == b"/" {
                            state = State::InClosingTag;
                        } else if char == b">" {
                            state = State::Span;
                        }
                        continue;
                    }
                    // .in_closing_tag => { if ">" -> normal; continue; }
                    State::InClosingTag => {
                        // If we are closing a tag, we will enter the normal state
                        if char == b">" {
                            state = State::Normal;
                        }
                        continue;
                    }
                }
                // self.buffer[cell_idx] = .{ .char = .{ .grapheme = char, .width = 1 }, .style = style };
                assert!(cell_idx < buf_len);
                // SAFETY: `cell_idx < buf_len` (asserted); the write goes through the
                // buffer's own data pointer (no `&mut [Cell]` over the aliased heap).
                unsafe {
                    buf_data.add(cell_idx).write(Cell {
                        char: Character {
                            grapheme: char,
                            width: 1,
                        },
                        style,
                        ..Cell::default()
                    });
                }
                // cell_idx += 1;
                cell_idx += 1;
            }
        }
        // std.debug.assert(cell_idx == self.buffer.len);
        assert!(cell_idx == buf_len);

        // Lastly, update the frame index
        // self.frame += 1;
        let frames_len = frames.len();
        unsafe {
            (*ptr).frame += 1;
            // if (self.frame == frames.len) self.frame = 0;
            if (*ptr).frame as usize == frames_len {
                (*ptr).frame = 0;
            }
        }
    }
}

/// `KeyOwned` → `Key<'_>` — the queued `Event::KeyPress` owns a `KeyOwned`; `matches`
/// needs a borrowed `Key<'_>` view (§5b, the list_themes precedent).
fn borrow_key(k: &KeyOwned) -> Key<'_> {
    Key {
        codepoint: k.codepoint,
        text: k.text.as_deref(),
        shifted_codepoint: k.shifted_codepoint,
        base_layout_codepoint: k.base_layout_codepoint,
        mods: k.mods,
    }
}

/// `std.unicode.utf8CodepointSequenceLength` from a lead byte (Utf8Iterator's slicing).
fn utf8_codepoint_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

/// Return `line[i..end]` as a `&'a [u8]` into the OWNING `frame` (the `line` is a
/// subslice of `frame`; we recover `'a` provenance by indexing `frame` at the line's
/// byte offset). Preserves the reference's zero-copy grapheme borrow.
#[inline]
fn frame_slice<'a>(frame: &'a [u8], line: &[u8], i: usize, end: usize) -> &'a [u8] {
    // Byte offset of `line` within `frame`.
    let base = line.as_ptr() as usize - frame.as_ptr() as usize;
    &frame[base + i..base + end]
}

/// The `boo` command is used to display the animation from the Rustty website in the terminal
// pub fn run(gpa: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // Disable on non-desktop systems.
    // switch (builtin.os.tag) { .windows, .macos, .linux, .freebsd => {}, else => return 1 }
    match std::env::consts::OS {
        "windows" | "macos" | "linux" | "freebsd" => {}
        _ => return Ok(1),
    }

    // var opts: Options = .{};  defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(gpa); defer iter.deinit();
        // try args.parse(Options, gpa, &opts, &iter);
        let mut iter = args::args_iterator()?;
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // try decompressFrames(gpa);  defer { gpa.free(frames); gpa.free(decompressed_data); }
    //   `decompressed_data` + `frames` are run-scope locals (the reference's globals);
    //   `frames` borrows `decompressed_data` — both live until `run` returns.
    let decompressed_data: Vec<u8> = decompress_frames()?;
    // var frame_iter = std.mem.splitScalar(u8, decompressed_data, '\x01');
    let frames: Vec<&[u8]> = decompressed_data.split(|&b| b == 0x01).collect();

    // var app = try vxfw.App.init(gpa);  defer app.deinit();
    let mut app = App::init()?;

    // var boo: Boo = undefined;
    // boo.frame = 0;
    // boo.framerate = 1000 / 30;
    // boo.ghostty_style = .{};
    // boo.outline_style = .{ .fg = .{ .index = 4 } };
    // @memset(&boo.buffer, .{});
    let boo = Boo {
        frame: 0,
        framerate: 1000 / 30,
        buffer: vec![Cell::default(); FRAME_WIDTH * FRAME_HEIGHT],
        ghostty_style: Style::default(),
        outline_style: Style {
            fg: Color::Index(4),
            ..Style::default()
        },
        frames: &frames,
    };

    // try app.run(boo.widget(), .{});
    let widget = boo.widget();
    let result = app.run(widget, Default::default());

    // app.deinit();  (the reference `defer`; run before propagating any run error so
    // the terminal state is restored.)
    app.deinit();
    // keep `boo`/`frames`/`decompressed_data` alive until here (the widget aliases them).
    drop(boo);
    opts.deinit();
    result?;

    // return 0;
    Ok(0)
}

/// We store a global ref to the decompressed data. All of our frames reference into this data
/// (ported to a run-scope local `Vec<u8>`; see `run`).
// fn decompressFrames(gpa: Allocator) !void {
fn decompress_frames() -> Result<Vec<u8>, Error> {
    // var src: std.Io.Reader = .fixed(framedata);
    // var decompress: std.compress.flate.Decompress = .init(&src, .raw, &.{});
    // _ = try decompress.reader.streamRemaining(&out.writer);
    // decompressed_data = try out.toOwnedSlice();
    //   → raw DEFLATE (windowBits=-15) via the vendored zlib 1.3.1.
    let decompressed = zlib_sys::inflate_raw(FRAMEDATA, MAX_DECOMPRESSED).map_err(|_| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "boo framedata inflate_raw failed",
        ))
    })?;
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Round-trip the embedded framedata: decompress, split into frames, and confirm
    // each frame's span-parser yields exactly FRAME_WIDTH*FRAME_HEIGHT (4100) cells —
    // the reference's `std.debug.assert(cell_idx == self.buffer.len)`. Proves the
    // build.rs generator + `inflate_raw` reproduce the asset byte-faithfully.
    #[test]
    fn decompress_frames_round_trip() {
        let decompressed = decompress_frames().expect("inflate framedata");
        let frames: Vec<&[u8]> = decompressed.split(|&b| b == 0x01).collect();
        // 235 frames (the vendored `assets/framegen/frames/*.txt` count).
        assert_eq!(frames.len(), 235);

        // Drive `update_frame` over a real Boo for every frame; the internal assert
        // `cell_idx == buffer.len()` (4100) fires if any frame is malformed.
        let mut boo = Boo {
            frame: 0,
            framerate: 1000 / 30,
            buffer: vec![Cell::default(); FRAME_WIDTH * FRAME_HEIGHT],
            ghostty_style: Style::default(),
            outline_style: Style {
                fg: Color::Index(4),
                ..Style::default()
            },
            frames: &frames,
        };
        let ptr: *mut Boo = &mut boo;
        for _ in 0..frames.len() {
            // SAFETY: `ptr` is a valid, exclusively-held `*mut Boo` in this test (no
            // outstanding buffer alias — the aliasing hazard is proven in mirihar-vxfw).
            unsafe { Boo::update_frame(ptr) };
        }
        // After frames.len() ticks, the index wrapped back to 0.
        assert_eq!(boo.frame, 0);
    }
}
