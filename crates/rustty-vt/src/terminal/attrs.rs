//! PORTS: ghostty src/terminal/Terminal.zig — the kitty graphics / glyph protocol entry points and the `setAttribute` / `printAttributes` cluster.

use super::*;

impl Terminal {
    /// Execute a kitty graphics command. The buf is used to populate with
    /// the response that should be sent as an APC sequence. The response will
    /// be a full, valid APC sequence.
    ///
    /// If an error occurs, the caller should response to the pty that a
    /// an error occurred otherwise the behavior of the graphics protocol is
    /// undefined.
    pub fn kitty_graphics(
        &mut self,
        alloc: Allocator,
        cmd: &mut kitty::graphics::Command,
    ) -> Option<kitty::graphics::Response> {
        // return kitty.graphics.execute(alloc, self, cmd);
        kitty::graphics::execute(alloc, self, cmd)
    }

    /// Execute a Glyph Protocol APC command against this terminal's per-session
    /// glossary. The returned response, if any, should be sent back to the pty as
    /// a complete APC sequence via `Response.formatWire`.
    // pub fn glyphProtocol(self: *Terminal, alloc: Allocator,
    //     req: *const glyph.Request) ?glyph.Response {
    //     const resp = glyph.execute(alloc, &self.glyph_glossary, req);
    //     switch (req.*) {
    //         .register, .clear => self.flags.dirty.glyph_glossary = true,
    //         .support, .query => {},
    //     }
    //     return resp;
    // }
    // PORT NOTE (parity tail S7-3, S7-4): the glossary map grows through the
    // allocator it was bound to at `init`; `glyph::execute(alloc, ..)`
    // allocates each entry's decoded outline through `alloc`. The reference's
    // per-call `alloc` is that bound allocator, checked here in debug builds
    // (callers pass `terminal.gpa()` or termio's allocator).
    pub fn glyph_protocol(
        &mut self,
        alloc: Allocator,
        req: &glyph::Request,
    ) -> Option<glyph::Response> {
        self.glyph_glossary.allocator().debug_assert_same(alloc);
        let resp = glyph::execute(alloc, &mut self.glyph_glossary, req);
        match req {
            glyph::Request::Register(_) | glyph::Request::Clear(_) => {
                self.flags.dirty.glyph_glossary = true;
            }
            glyph::Request::Support | glyph::Request::Query(_) => {}
        }
        resp
    }

    /// Set the storage size limit for Kitty graphics across all screens.
    // PORT NOTE (`4371871bc`): `setLimit` no longer allocates, so this is
    // infallible (`!void` → `void`).
    pub fn set_kitty_graphics_size_limit(&mut self, alloc: Allocator, limit: usize) {
        // if (comptime !build_options.kitty_graphics) return;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }
        // var it = self.screens.all.iterator();
        // while (it.next()) |entry| {
        //     const screen: *Screen = entry.value.*;
        //     screen.kitty_images.setLimit(self.io(), alloc, screen, limit);
        // }
        // PORT NOTE: `screen.kitty_images.setLimit(io, alloc, screen, limit)`
        // borrows `screen.kitty_images` mutably while also handing `screen`
        // itself to `set_limit` (now as a raw `*mut Screen`, per the storage's
        // `s: *mut Screen` PORT NOTE). As in `Screen::deinit`, we MOVE
        // `kitty_images` out of the screen (leaving a default in its slot), run
        // `set_limit` against the raw screen pointer, then move the updated
        // storage back. The screen pointers in `self.screens.all` are stable
        // heap allocations.
        for slot in self.screens.all.iter() {
            if let Some(screen) = *slot {
                // SAFETY: `screen` is a stable, initialized heap allocation owned
                // by the screen set; we hold `&mut self` so no other alias exists.
                let mut images = core::mem::take(unsafe { &mut (*screen).kitty_images });
                images.set_limit(alloc, screen, limit);
                unsafe { (*screen).kitty_images = images };
            }
        }
    }

    /// Set the allowed medium types for Kitty graphics image loading
    /// across all screens.
    pub fn set_kitty_graphics_loading_limits(&mut self, limits: kitty::graphics_image::Limits) {
        // if (comptime !build_options.kitty_graphics) return;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }
        // var it = self.screens.all.iterator();
        // while (it.next()) |entry| {
        //     const screen: *Screen = entry.value.*;
        //     screen.kitty_images.image_limits = limits;
        // }
        for slot in self.screens.all.iter() {
            if let Some(screen) = *slot {
                // SAFETY: stable initialized heap screen; `&mut self` excludes
                // other aliases.
                let screen: &mut Screen = unsafe { &mut *screen };
                screen.kitty_images.image_limits = limits.clone();
            }
        }
    }

    /// Set a style attribute.
    pub fn set_attribute(&mut self, attr: sgr::Attribute) -> Result<(), OutOfMemory> {
        // try self.screens.active.setAttribute(attr);
        // PORT NOTE: `Screen::set_attribute` returns the reference
        // `PageList.IncreaseCapacityError`; the reference `setAttribute` returns
        // `!void` inferred from that single call, and the only failure mode that
        // can propagate here is allocation failure, so it maps to `OutOfMemory`.
        self.active_mut()
            .set_attribute(attr)
            .map_err(|_| OutOfMemory)
    }

    /// Print the active attributes as a string. This is used to respond to DECRQSS
    /// requests.
    ///
    /// Boolean attributes are printed first, followed by foreground color, then
    /// background color. Each attribute is separated by a semicolon.
    pub fn print_attributes<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], OutOfMemory> {
        // var stream = std.io.fixedBufferStream(buf);
        // const writer = stream.writer();
        // PORT NOTE: the reference uses `std.io.fixedBufferStream`, whose writer
        // returns `error.NoSpaceLeft` on overflow. We mirror that exactly: `pos`
        // tracks the bytes written and any write past the buffer end is the
        // `NoSpaceLeft` error, which is the only error this `![]const u8` can
        // produce and so maps to `OutOfMemory`.
        let mut pos: usize = 0;
        // `writeByte`: write a single byte, NoSpaceLeft if it doesn't fit.
        macro_rules! write_byte {
            ($b:expr) => {{
                if pos >= buf.len() {
                    return Err(OutOfMemory);
                }
                buf[pos] = $b;
                pos += 1;
            }};
        }
        // `writer.print(fmt, args)`: format into a scratch buffer then copy. The
        // reference formats directly into the stream; copying the formatted bytes
        // is byte-for-byte equivalent and keeps the same NoSpaceLeft semantics.
        macro_rules! write_print {
            ($($arg:tt)*) => {{
                let s = format!($($arg)*);
                if pos + s.len() > buf.len() {
                    return Err(OutOfMemory);
                }
                buf[pos..pos + s.len()].copy_from_slice(s.as_bytes());
                pos += s.len();
            }};
        }

        // The SGR response always starts with a 0. See https://vt100.net/docs/vt510-rm/DECRPSS
        // try writer.writeByte('0');
        write_byte!(b'0');

        // const pen = self.screens.active.cursor.style;
        let pen = self.active_mut().cursor.style;
        // var attrs: [9]u8 = @splat(0);
        let mut attrs: [u8; 9] = [0; 9];
        // var i: usize = 0;
        let mut i: usize = 0;

        if pen.flags.bold {
            attrs[i] = 1;
            i += 1;
        }

        if pen.flags.faint {
            attrs[i] = 2;
            i += 1;
        }

        if pen.flags.italic {
            attrs[i] = 3;
            i += 1;
        }

        if pen.flags.underline != sgr::Underline::None {
            attrs[i] = 4;
            i += 1;
        }

        if pen.flags.overline {
            attrs[i] = 53;
            i += 1;
        }

        if pen.flags.blink {
            attrs[i] = 5;
            i += 1;
        }

        if pen.flags.inverse {
            attrs[i] = 7;
            i += 1;
        }

        if pen.flags.invisible {
            attrs[i] = 8;
            i += 1;
        }

        if pen.flags.strikethrough {
            attrs[i] = 9;
            i += 1;
        }

        // for (attrs[0..i]) |attr| {
        //     // Preserve underline styles. Kind of a hack to special case 4
        //     // here but its easier than changing how we do all attributes.
        //     if (attr == 4 and pen.flags.underline != .single) {
        //         try writer.print(";4:{}", .{@intFromEnum(pen.flags.underline)});
        //         continue;
        //     }
        //
        //     try writer.print(";{}", .{attr});
        // }
        //
        // PORT NOTE (oracle cb2fef390, `P3-W1 T6`): the buffer stays
        // `&mut [u8]` in, `&[u8]` out — nothing here formats through `String`.
        for &attr in &attrs[0..i] {
            // Preserve underline styles. Kind of a hack to special case 4
            // here but its easier than changing how we do all attributes.
            if attr == 4 && pen.flags.underline != sgr::Underline::Single {
                write_print!(";4:{}", pen.flags.underline as u8);
                continue;
            }

            write_print!(";{}", attr);
        }

        // switch (pen.fg_color) {
        match pen.fg_color.tag() {
            style::ColorTag::None => {}
            style::ColorTag::Palette => {
                let idx = pen.fg_color.palette();
                if idx >= 16 {
                    write_print!(";38:5:{}", idx)
                } else if idx >= 8 {
                    write_print!(";9{}", idx - 8)
                } else {
                    write_print!(";3{}", idx)
                }
            }
            style::ColorTag::Rgb => {
                let rgb = pen.fg_color.rgb();
                write_print!(";38:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)
            }
        }

        // switch (pen.bg_color) {
        match pen.bg_color.tag() {
            style::ColorTag::None => {}
            style::ColorTag::Palette => {
                let idx = pen.bg_color.palette();
                if idx >= 16 {
                    write_print!(";48:5:{}", idx)
                } else if idx >= 8 {
                    write_print!(";10{}", idx - 8)
                } else {
                    write_print!(";4{}", idx)
                }
            }
            style::ColorTag::Rgb => {
                let rgb = pen.bg_color.rgb();
                write_print!(";48:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)
            }
        }

        // return stream.getWritten();
        Ok(&buf[..pos])
    }
}
