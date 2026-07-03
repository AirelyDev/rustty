//! rustty-cli::boo
//!
//! PORTS: ghostty src/cli/boo.zig (0t) — the `+boo` action.
//!
//! PORT STATUS: C5 (CLI_BLUEPRINT.md §1/§8 Risk 1). `boo` is a 100% vaxis TUI
//! animation (the ghost animation from the website). `vaxis` (+ the vxfw widget
//! framework and the `framedata` compressed asset) is an unported Zig-source
//! dependency (same class as libxev/z2d), so the whole action is a documented
//! vaxis-deferred placeholder leaf (DEFERRALS.md "cli C5"; GPU/GTK/xev precedent):
//! `run` is an `unimplemented!()` dependency marker and the reference body +
//! comments are preserved VERBATIM in the commented block below. 0 tests;
//! comment-fidelity (scoped-G3b) applies.

use crate::ghostty::Error;

/// The `boo` command is used to display the animation from the rustty website in
/// the terminal.
// pub fn run(gpa: Allocator) !u8 {
//   VAXIS-DEFERRED: the reference body (platform gate, arg parse, frame
//   decompression, `vxfw.App` run loop) is preserved verbatim below.
pub fn run() -> Result<u8, Error> {
    unimplemented!("cli/boo.zig — vaxis-deferred leaf (DEFERRALS.md cli C5)")
}

// ===========================================================================
// VAXIS-DEFERRED placeholder leaf (DEFERRALS.md "cli C5") — the full reference,
// preserved VERBATIM as a commented block. `vaxis`/`vxfw` + the `framedata`
// compressed frame asset are unported Zig-source deps; when they land, this body
// (Boo widget, frame span-parser, flate decompression) is ported in place. The
// R2 product-name comments ("Ghostty website") retarget to rustty on porting.
// ===========================================================================

// const std = @import("std");
// const builtin = @import("builtin");
// const args = @import("args.zig");
// const Action = @import("ghostty.zig").Action;
// const Allocator = std.mem.Allocator;
// const vaxis = @import("vaxis");
//
// const framedata = @import("framedata").compressed;
//
// const vxfw = vaxis.vxfw;
//
// pub const Options = struct {
//     pub fn deinit(self: Options) void {
//         _ = self;
//     }
//
//     /// Enables `-h` and `--help` to work.
//     pub fn help(self: Options) !void {
//         _ = self;
//         return Action.help_error;
//     }
// };
//
// const Boo = struct {
//     frame: u8,
//     framerate: u32, // 30 fps
//     // We know the size of this at compile time, but we heap allocate the slice to prevent the
//     // binary from increasing too much in size
//     buffer: [frame_width * frame_height]vaxis.Cell = undefined,
//
//     ghostty_style: vaxis.Style,
//     outline_style: vaxis.Style,
//
//     // Width of a single frame
//     const frame_width = 100;
//     // Height of a single frame
//     const frame_height = 41;
//
//     fn widget(self: *Boo) vxfw.Widget {
//         return .{
//             .userdata = self,
//             .eventHandler = Boo.typeErasedEventHandler,
//             .drawFn = Boo.typeErasedDrawFn,
//         };
//     }
//
//     fn typeErasedEventHandler(ptr: *anyopaque, ctx: *vxfw.EventContext, event: vxfw.Event) anyerror!void {
//         const self: *Boo = @ptrCast(@alignCast(ptr));
//         switch (event) {
//             .init,
//             .tick,
//             => {
//                 self.updateFrame();
//                 ctx.redraw = true;
//                 return ctx.tick(self.framerate, self.widget());
//             },
//             .key_press => |key| {
//                 if (key.matches('c', .{ .ctrl = true }) or
//                     key.matches(vaxis.Key.escape, .{}))
//                 {
//                     ctx.quit = true;
//                     return;
//                 }
//             },
//             else => {},
//         }
//     }
//
//     fn typeErasedDrawFn(ptr: *anyopaque, ctx: vxfw.DrawContext) Allocator.Error!vxfw.Surface {
//         const self: *Boo = @ptrCast(@alignCast(ptr));
//         const max = ctx.max.size();
//
//         // Warn for screen size
//         if (max.width < frame_width or max.height < frame_height) {
//             const text: vxfw.Text = .{ .text = "Screen must be at least 100w x 41h" };
//             const center: vxfw.Center = .{ .child = text.widget() };
//             return center.draw(ctx);
//         }
//
//         // Calculate x and y offsets to center the animation frame
//         const offset_y = (max.height - frame_height) / 2;
//         const offset_x = (max.width - frame_width) / 2;
//
//         // Create the animation surface
//         const child: vxfw.Surface = .{
//             .size = .{ .width = @intCast(frame_width), .height = @intCast(frame_height) },
//             .widget = self.widget(),
//             .buffer = &self.buffer,
//             .children = &.{},
//         };
//
//         // Allocate a slice of child surfaces
//         var children = try ctx.arena.alloc(vxfw.SubSurface, 1);
//         children[0] = .{
//             .origin = .{ .row = @intCast(offset_y), .col = @intCast(offset_x) },
//             .surface = child,
//         };
//
//         return .{
//             .size = max,
//             .widget = self.widget(),
//             .buffer = &.{},
//             .children = children,
//         };
//     }
//
//     /// Updates our internal buffer with the current frame, then advances the frame index
//     fn updateFrame(self: *Boo) void {
//         const frame = frames[self.frame];
//         // A frame is characters with html spans. When we encounter a span, we use the outline style
//         // until the span ends. That is, when we find a '<', we parse until '>'. Then we use the
//         // outline styule until the next '<', and skip until the next '>'
//
//         const State = enum {
//             normal,
//             span,
//             in_tag,
//             in_closing_tag,
//         };
//
//         var cell_idx: usize = 0;
//
//         var line_iter = std.mem.splitScalar(u8, frame, '\n');
//         while (line_iter.next()) |line| {
//             var state: State = .normal;
//             var style = self.ghostty_style;
//             var cp_iter: std.unicode.Utf8Iterator = .{ .bytes = line, .i = 0 };
//             while (cp_iter.nextCodepointSlice()) |char| {
//                 switch (state) {
//                     .normal => if (std.mem.eql(u8, "<", char)) {
//                         state = .in_tag;
//                         // We will be entering a span
//                         style = self.outline_style;
//                         continue;
//                     },
//                     .span => if (std.mem.eql(u8, "<", char)) {
//                         state = .in_tag;
//                         style = self.ghostty_style;
//                         continue;
//                     },
//                     .in_tag => {
//                         // If we encounter a '/', we are a closing tag
//                         // If we parse all the way to a '>' we are an opening tag: we are now in a span
//                         if (std.mem.eql(u8, "/", char))
//                             state = .in_closing_tag
//                         else if (std.mem.eql(u8, ">", char))
//                             state = .span;
//                         continue;
//                     },
//                     .in_closing_tag => {
//                         // If we are closing a tag, we will enter the normal state
//                         if (std.mem.eql(u8, ">", char)) state = .normal;
//                         continue;
//                     },
//                 }
//                 self.buffer[cell_idx] = .{
//                     .char = .{
//                         .grapheme = char,
//                         .width = 1,
//                     },
//                     .style = style,
//                 };
//                 cell_idx += 1;
//             }
//         }
//         std.debug.assert(cell_idx == self.buffer.len);
//
//         // Lastly, update the frame index
//         self.frame += 1;
//         if (self.frame == frames.len) self.frame = 0;
//     }
// };
//
// /// The `boo` command is used to display the animation from the Ghostty website in the terminal
// pub fn run(gpa: Allocator) !u8 {
//     // Disable on non-desktop systems.
//     switch (builtin.os.tag) {
//         .windows, .macos, .linux, .freebsd => {},
//         else => return 1,
//     }
//
//     var opts: Options = .{};
//     defer opts.deinit();
//
//     {
//         var iter = try args.argsIterator(gpa);
//         defer iter.deinit();
//         try args.parse(Options, gpa, &opts, &iter);
//     }
//
//     try decompressFrames(gpa);
//     defer {
//         gpa.free(frames);
//         gpa.free(decompressed_data);
//     }
//
//     var app = try vxfw.App.init(gpa);
//     defer app.deinit();
//
//     var boo: Boo = undefined;
//     boo.frame = 0;
//     boo.framerate = 1000 / 30;
//     boo.ghostty_style = .{};
//     boo.outline_style = .{ .fg = .{ .index = 4 } };
//     @memset(&boo.buffer, .{});
//
//     try app.run(boo.widget(), .{});
//
//     return 0;
// }
//
// /// We store a global ref to the decompressed data. All of our frames reference into this data
// var decompressed_data: []const u8 = undefined;
//
// /// Heap allocated list of frames. The underlying frame data references decompressed_data
// var frames: []const []const u8 = undefined;
//
// /// Decompress the frames into a slice of individual frames
// fn decompressFrames(gpa: Allocator) !void {
//     var src: std.Io.Reader = .fixed(framedata);
//
//     // var buf: [std.compress.flate.max_window_len]u8 = undefined;
//     var decompress: std.compress.flate.Decompress = .init(&src, .raw, &.{});
//
//     var out: std.Io.Writer.Allocating = .init(gpa);
//     _ = try decompress.reader.streamRemaining(&out.writer);
//     decompressed_data = try out.toOwnedSlice();
//
//     var frame_list: std.ArrayList([]const u8) = try .initCapacity(gpa, 235);
//
//     var frame_iter = std.mem.splitScalar(u8, decompressed_data, '\x01');
//     while (frame_iter.next()) |frame| {
//         try frame_list.append(gpa, frame);
//     }
//     frames = try frame_list.toOwnedSlice(gpa);
// }
