//! PORTS: ghostty src/Surface.zig — the text, selection and font-size cluster: `dumpText`,
//! `hasSelection`, `selectionString`, `pwd`, `resolvePathForOpening`, `imePoint`,
//! `clipboardWrite`, `copySelectionToClipboards`, `setSelection`, `setCellSize`,
//! `setFontSize`, `queueRender`, `sizeCallback`, `resize`, `balancePaddingIfNeeded`.

#![allow(non_snake_case)]

use super::*;

impl<A> Surface<A>
where
    A: apprt::App + 'static,
    // C0 DECISION (CORE_BLUEPRINT §1, option 1 — the app.rs impl-block
    // precedent): the self-referential associated-type bound; Surface's
    // method tier needs it to call into `App<A>`'s constrained methods
    // (maybeHandleBinding's `performAllChainedAction`, C10).
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    /// Grab the value of text at the given selection point. Note that the
    /// selection structure is used as a way to determine the area of the
    /// screen to read from, it doesn't have to match the user's current
    /// selection state.
    ///
    /// The returned value contains allocated data and must be deinitialized.
    // pub fn dumpText(
    //     self: *Surface,
    //     alloc: Allocator,
    //     sel: terminal.Selection,
    // ) !Text {
    //
    // (the inferred `!Text` error set is `dumpTextLocked`'s — the [`Error`]
    // union.)
    pub fn dumpText(
        &mut self,
        alloc: Allocator,
        sel: terminal::selection::Selection,
    ) -> Result<Text, Error> {
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock below.)
        // return try self.dumpTextLocked(alloc, sel);
        let result = self.dumpTextLocked(alloc, sel);
        // (the deferred mutex unlock)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result
    }

    /// Same as `dumpText` but assumes the renderer state mutex is already
    /// held.
    // pub fn dumpTextLocked(
    //     self: *Surface,
    //     alloc: Allocator,
    //     sel: terminal.Selection,
    // ) !Text {
    pub fn dumpTextLocked(
        &mut self,
        alloc: Allocator,
        sel: terminal::selection::Selection,
    ) -> Result<Text, Error> {
        // Read out the text
        // const text = try self.io.terminal.screens.active.selectionString(alloc, .{
        //     .sel = sel,
        //     .trim = false,
        // });
        let text = unsafe { &mut *(*self.io.as_ptr()).terminal.screens.active }.selection_string(
            alloc.into(),
            terminal::screen::SelectionString { sel, trim: false },
        )?;
        // errdefer alloc.free(text);
        // (the owned `text` drops on any early error return.)

        // Calculate our viewport info if we can.
        // const vp: ?Text.Viewport = viewport: {
        let vp: Option<TextViewport> = 'viewport: {
            // If our bottom right pin is before the viewport, then we can't
            // possibly have this text be within the viewport.
            // const vp_tl_pin = self.io.terminal.screens.active.pages.getTopLeft(.viewport);
            let vp_tl_pin = unsafe {
                (*(*self.io.as_ptr()).terminal.screens.active)
                    .pages
                    .get_top_left(terminal::point::Tag::Viewport)
            };
            // const br_pin = sel.bottomRight(self.io.terminal.screens.active);
            let br_pin = sel.bottom_right(unsafe { &*(*self.io.as_ptr()).terminal.screens.active });
            // if (br_pin.before(vp_tl_pin)) break :viewport null;
            if br_pin.before(vp_tl_pin) {
                break 'viewport None;
            }

            // If our top-left pin is after the viewport, then we can't possibly
            // have this text be within the viewport.
            // const vp_br_pin = self.io.terminal.screens.active.pages.getBottomRight(.viewport) orelse {
            let vp_br_pin = match unsafe {
                (*(*self.io.as_ptr()).terminal.screens.active)
                    .pages
                    .get_bottom_right(terminal::point::Tag::Viewport)
            } {
                Some(v) => v,
                None => {
                    // I don't think this is possible but I don't want to crash on
                    // that assertion so let's just break out...
                    // log.warn("viewport bottom-right pin not found, bug?", .{});
                    log::warn!(target: LOG_SCOPE, "viewport bottom-right pin not found, bug?");
                    break 'viewport None;
                }
            };
            // const tl_pin = sel.topLeft(self.io.terminal.screens.active);
            let tl_pin = sel.top_left(unsafe { &*(*self.io.as_ptr()).terminal.screens.active });
            // if (vp_br_pin.before(tl_pin)) break :viewport null;
            if vp_br_pin.before(tl_pin) {
                break 'viewport None;
            }

            // We established that our top-left somewhere before the viewport
            // bottom-right and that our bottom-right is somewhere after
            // the top-left. This means that at least some portion of our
            // selection is within the viewport.

            // Our top-left point. If it doesn't exist in the viewport it must
            // be before and we can return (0,0).
            // const tl_pt: terminal.Point = self.io.terminal.screens.active.pages.pointFromPin(
            //     .viewport,
            //     tl_pin,
            // ) orelse tl: {
            let tl_pt: terminal::point::Point = match unsafe {
                (*(*self.io.as_ptr()).terminal.screens.active)
                    .pages
                    .point_from_pin(terminal::point::Tag::Viewport, tl_pin)
            } {
                Some(v) => v,
                None => {
                    // if (comptime std.debug.runtime_safety) {
                    //     assert(tl_pin.before(vp_tl_pin));
                    // }
                    //
                    // (the runtime-safety-gated assert IS `debug_assert!` —
                    // the established quirks.inlineAssert mapping.)
                    debug_assert!(tl_pin.before(vp_tl_pin));

                    // break :tl .{ .viewport = .{} };
                    terminal::point::Point::Viewport(terminal::point::Coordinate { x: 0, y: 0 })
                }
            };

            // Our bottom-right point. If it doesn't exist in the viewport
            // it must be the bottom-right of the viewport.
            // const br_pt = self.io.terminal.screens.active.pages.pointFromPin(
            //     .viewport,
            //     br_pin,
            // ) orelse br: {
            let br_pt = match unsafe {
                (*(*self.io.as_ptr()).terminal.screens.active)
                    .pages
                    .point_from_pin(terminal::point::Tag::Viewport, br_pin)
            } {
                Some(v) => v,
                None => {
                    // if (comptime std.debug.runtime_safety) {
                    //     assert(vp_br_pin.before(br_pin));
                    // }
                    debug_assert!(vp_br_pin.before(br_pin));

                    // break :br self.io.terminal.screens.active.pages.pointFromPin(
                    //     .viewport,
                    //     vp_br_pin,
                    // ).?;
                    unsafe {
                        (*(*self.io.as_ptr()).terminal.screens.active)
                            .pages
                            .point_from_pin(terminal::point::Tag::Viewport, vp_br_pin)
                    }
                    .unwrap()
                }
            };

            // const tl_coord = tl_pt.coord();
            let tl_coord = tl_pt.coord();
            // const br_coord = br_pt.coord();
            let br_coord = br_pt.coord();

            // Our sizes are all scaled so we need to send the unscaled values back.
            // const content_scale = self.rt_surface.getContentScale() catch .{ .x = 1, .y = 1 };
            let content_scale = unsafe { &*self.rt_surface }
                .get_content_scale()
                .unwrap_or(apprt::ContentScale { x: 1.0, y: 1.0 });
            // const x: f64 = x: {
            let x: f64 = {
                // Simple x * cell width gives the left
                // var x: f64 = @floatFromInt(tl_coord.x * self.size.cell.width);
                let mut x: f64 = f64::from(u32::from(tl_coord.x) * self.size.cell.width);

                // Add padding
                // x += @floatFromInt(self.size.padding.left);
                x += f64::from(self.size.padding.left);

                // Scale
                // x /= content_scale.x;
                x /= f64::from(content_scale.x);

                // break :x x;
                x
            };
            // const y: f64 = y: {
            let y: f64 = {
                // Simple y * cell height gives the top
                // var y: f64 = @floatFromInt(tl_coord.y * self.size.cell.height);
                let mut y: f64 = f64::from(tl_coord.y * self.size.cell.height);

                // We want the text baseline
                // y += @floatFromInt(self.size.cell.height);
                y += f64::from(self.size.cell.height);
                // y -= @floatFromInt(self.font_metrics.cell_baseline);
                y -= f64::from(self.font_metrics.cell_baseline);

                // Add padding
                // y += @floatFromInt(self.size.padding.top);
                y += f64::from(self.size.padding.top);

                // Scale
                // y /= content_scale.y;
                y /= f64::from(content_scale.y);

                // break :y y;
                y
            };

            // Utilize viewport sizing to convert to offsets
            // const start = tl_coord.y * self.io.terminal.screens.active.pages.cols + tl_coord.x;
            let start = tl_coord.y
                * u32::from(unsafe { (*(*self.io.as_ptr()).terminal.screens.active).pages.cols })
                + u32::from(tl_coord.x);
            // const end = br_coord.y * self.io.terminal.screens.active.pages.cols + br_coord.x;
            let end = br_coord.y
                * u32::from(unsafe { (*(*self.io.as_ptr()).terminal.screens.active).pages.cols })
                + u32::from(br_coord.x);

            // break :viewport .{
            //     .tl_px_x = x,
            //     .tl_px_y = y,
            //     .offset_start = start,
            //     .offset_len = end - start,
            // };
            break 'viewport Some(TextViewport {
                tl_px_x: x,
                tl_px_y: y,
                offset_start: start,
                offset_len: end - start,
            });
        };

        // return .{
        //     .text = text,
        //     .viewport = vp,
        // };
        Ok(Text { text, viewport: vp })
    }

    /// Returns true if the terminal has a selection.
    // pub fn hasSelection(self: *const Surface) bool {
    pub fn hasSelection(&self) -> bool {
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock below.)
        // return self.io.terminal.screens.active.selection != null;
        let result = unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection }.is_some();
        // (the deferred mutex unlock)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result
    }

    /// Returns the selected text. This is allocated.
    // pub fn selectionString(self: *Surface, alloc: Allocator) !?[:0]const u8 {
    //
    // (the return carries raw bytes — the vt `selectionString` `Vec<u8>`
    // convention; the inferred `!` set is the screen's OOM.)
    pub fn selectionString(&mut self, alloc: Allocator) -> Result<Option<Vec<u8>>, Error> {
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        // const sel = self.io.terminal.screens.active.selection orelse return null;
        let sel = match unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection } {
            Some(v) => v,
            None => {
                // (the deferred mutex unlock — the null exit)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(None);
            }
        };
        // return try self.io.terminal.screens.active.selectionString(alloc, .{
        //     .sel = sel,
        //     .trim = false,
        // });
        let result = unsafe { &mut *(*self.io.as_ptr()).terminal.screens.active }.selection_string(
            alloc.into(),
            terminal::screen::SelectionString { sel, trim: false },
        );
        // (the deferred mutex unlock)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        match result {
            Ok(v) => Ok(Some(v)),
            Err(err) => Err(err.into()),
        }
    }

    /// Returns the pwd of the terminal, if any. This is always copied because
    /// the pwd can change at any point from termio. If we are calling from the IO
    /// thread you should just check the terminal directly.
    // pub fn pwd(
    //     self: *const Surface,
    //     alloc: Allocator,
    // ) Allocator.Error!?[]const u8 {
    //
    // PORT NOTE: the `Allocator.Error!` collapses (infallible Rust
    // allocation — the app.rs precedent); the owned `Vec<u8>` carries the
    // raw pwd bytes.
    pub fn pwd(&self, alloc: Allocator) -> Option<Vec<u8>> {
        // (the DerivedConfig.init precedent: the allocator param is carried
        // for signature parity; the `Vec` below allocates via the global
        // allocator.)
        let _ = alloc;
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        // const terminal_pwd = self.io.terminal.getPwd() orelse return null;
        let terminal_pwd = match unsafe { (*self.io.as_ptr()).terminal.get_pwd() } {
            Some(v) => v,
            None => {
                // (the deferred mutex unlock — the null exit)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return None;
            }
        };
        // return try alloc.dupe(u8, terminal_pwd);
        //
        // (`dupe` — an owned copy of the raw pwd bytes; the OOM-only `try`
        // collapses.)
        let result = terminal_pwd.to_vec();
        // (the deferred mutex unlock)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        Some(result)
    }

    /// Resolves a relative file path to an absolute path using the terminal's pwd.
    // fn resolvePathForOpening(
    //     self: *Surface,
    //     path: []const u8,
    // ) Allocator.Error!?[]const u8 {
    //
    // PORT NOTE: the `Allocator.Error!` collapses (infallible Rust
    // allocation); raw path bytes in and out (`&[u8]`/`Vec<u8>`, never a
    // lossy `String`). The C13 processLinks callsite (:4396) is live.
    pub(super) fn resolvePathForOpening(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        // if (!std.fs.path.isAbsolute(path)) {
        if !is_absolute(path) {
            // const terminal_pwd = self.io.terminal.getPwd() orelse {
            //     return null;
            // };
            let terminal_pwd = match unsafe { (*self.io.as_ptr()).terminal.get_pwd() } {
                Some(v) => v,
                None => {
                    return None;
                }
            };

            // const resolved = try std.fs.path.resolve(self.alloc, &.{ terminal_pwd, path });
            let resolved = resolve(&[terminal_pwd, path]);

            // std.fs.accessAbsolute(resolved, .{}) catch {
            //     self.alloc.free(resolved);
            //     return null;
            // };
            //
            // (the rustty-os resourcesdir `accessAbsolute` precedent; the
            // reference's free is the owned `Vec`'s drop.)
            if !std::path::Path::new(os_str_from_bytes(&resolved)).exists() {
                return None;
            }

            // return resolved;
            return Some(resolved);
        }

        // return null;
        None
    }

    /// Returns the x/y coordinate of where the IME (Input Method Editor)
    /// keyboard should be rendered.
    // pub fn imePoint(self: *const Surface) apprt.IMEPos {
    pub fn imePoint(&self) -> apprt::IMEPos {
        // self.renderer_state.mutex.lock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the reference locks
        // and unlocks WITHOUT defer, and so does this body: the unlock below
        // pairs with this lock, verbatim.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // const cursor = self.renderer_state.terminal.screens.active.cursor;
        //
        // (`Cursor` is `Copy` — the Zig value-copy, read under the lock.)
        let cursor =
            unsafe { (*(*(*self.renderer_state.as_ptr()).terminal).screens.active).cursor };
        // const preedit_width: usize = if (self.renderer_state.preedit) |preedit| preedit.width() else 0;
        let preedit_width: usize =
            if let Some(preedit) = unsafe { &(*self.renderer_state.as_ptr()).preedit } {
                preedit.width()
            } else {
                0
            };
        // self.renderer_state.mutex.unlock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };

        // TODO: need to handle when scrolling and the cursor is not
        // in the visible portion of the screen.

        // Our sizes are all scaled so we need to send the unscaled values back.
        // const content_scale = self.rt_surface.getContentScale() catch .{ .x = 1, .y = 1 };
        let content_scale = unsafe { &*self.rt_surface }
            .get_content_scale()
            .unwrap_or(apprt::ContentScale { x: 1.0, y: 1.0 });

        // const x: f64 = x: {
        let x: f64 = {
            // Simple x * cell width gives the top-left corner, then add padding offset
            // var x: f64 = @floatFromInt(cursor.x * self.size.cell.width + self.size.padding.left);
            //
            // PORT NOTE: reference defect (R13), UD76 (task R13-C2). The
            // reference's `u32` product and sum are unchecked; a saturated
            // `padding.left` (UD71) stored by `contentScaleCallback` overflows
            // them for any `cursor.x >= 1` until the IO thread's resize
            // shrinks the grid. Saturate, as the padding sums do (UD71): the
            // point is only an IME position.
            let mut x: f64 = f64::from(
                u32::from(cursor.x)
                    .saturating_mul(self.size.cell.width)
                    .saturating_add(self.size.padding.left),
            );

            // We want the midpoint
            // x += @as(f64, @floatFromInt(self.size.cell.width)) / 2;
            x += f64::from(self.size.cell.width) / 2.0;

            // And scale it
            // x /= content_scale.x;
            x /= f64::from(content_scale.x);

            // break :x x;
            x
        };

        // const y: f64 = y: {
        let y: f64 = {
            // Simple y * cell height gives the top-left corner, then add padding offset
            // var y: f64 = @floatFromInt(cursor.y * self.size.cell.height + self.size.padding.top);
            // (R13, UD76: saturating, as for `x` above.)
            let mut y: f64 = f64::from(
                u32::from(cursor.y)
                    .saturating_mul(self.size.cell.height)
                    .saturating_add(self.size.padding.top),
            );

            // We want the bottom
            // y += @floatFromInt(self.size.cell.height);
            y += f64::from(self.size.cell.height);

            // And scale it
            // y /= content_scale.y;
            y /= f64::from(content_scale.y);

            // break :y y;
            y
        };

        // Our height for now is always just the cell height because our preedit
        // rendering only renders in a single line.
        // const height: f64 = height: {
        let height: f64 = {
            // var height: f64 = @floatFromInt(self.size.cell.height);
            let mut height: f64 = f64::from(self.size.cell.height);
            // height /= content_scale.y;
            height /= f64::from(content_scale.y);
            // break :height height;
            height
        };
        // const width: f64 = width: {
        let width: f64 = {
            // var width: f64 = @floatFromInt(preedit_width * self.size.cell.width);
            let mut width: f64 = (preedit_width * (self.size.cell.width as usize)) as f64;

            // Our max width is the remaining screen width after the cursor.
            // We don't have to deal with wrapping because the preedit doesn't
            // wrap right now.
            // const screen_width: f64 = @floatFromInt(self.size.terminal().width);
            let screen_width: f64 = f64::from(self.size.terminal().width);
            // const x_offset: f64 = @floatFromInt((cursor.x + 1) * self.size.cell.width);
            let x_offset: f64 = f64::from((u32::from(cursor.x) + 1) * self.size.cell.width);
            // const max = screen_width - x_offset;
            let max = screen_width - x_offset;
            // width = @min(width, max);
            width = width.min(max);

            // Note: we don't apply content scale here because it looks like
            // for some reason in macOS its already scaled. I'm not sure why
            // that is so I'm going to just leave this comment here so its known
            // that I left this out on purpose pending more investigation.

            // break :width width;
            width
        };

        // return .{
        //     .x = x,
        //     .y = y,
        //     .width = width,
        //     .height = height,
        // };
        apprt::IMEPos {
            x,
            y,
            width,
            height,
        }
    }

    // fn clipboardWrite(self: *const Surface, data: []const u8, loc: apprt.Clipboard) !void {
    //
    // (the inferred `!void` is `allocSentinel`'s OOM — collapsed; the
    // `Result` shape carries the handleMessage callers' `try`s.)
    pub(super) fn clipboardWrite(&self, data: &[u8], loc: apprt::Clipboard) -> Result<(), Error> {
        // if (self.config.clipboard_write == .deny) {
        if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Deny {
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            return Ok(());
        }

        // Decode with the SIMD decoder, strict per the Kitty clipboard
        // spec that also governs OSC 52 base64 handling: a request with
        // characters outside the base64 alphabet is discarded entirely
        // (never partially decoded), while a missing-padding tail is
        // tolerated since OSC 52 has no way to report errors.
        // var buf = try self.alloc.allocSentinel(u8, simd.base64.maxLen(data), 0);
        // defer self.alloc.free(buf);
        //
        // (bc2f7d7d2: the std.base64 Decoder + `zig_base64` calc/decode pair is
        // replaced by the strict SIMD decode; `allocSentinel` — the max_len
        // bytes plus the NUL sentinel slot, and the OOM-only `try` collapses.)
        let max_len = rustty_simd::base64::max_len(data);
        let mut buf: Vec<u8> = vec![0u8; max_len + 1];
        // const decoded = simd.base64.decodeStrict(data, buf, .optional) catch {
        //     log.info("application sent invalid base64 data for OSC 52", .{});
        //     return;
        // };
        let decoded_len = match rustty_simd::base64::decode_strict(
            data,
            &mut buf[0..max_len],
            rustty_simd::base64::Padding::Optional,
        ) {
            Ok(decoded) => decoded.len(),
            Err(_) => {
                // log.info("application sent invalid base64 data for OSC 52", .{});
                log::info!(target: LOG_SCOPE, "application sent invalid base64 data for OSC 52");
                return Ok(());
            }
        };
        // buf[decoded.len] = 0;
        buf[decoded_len] = 0;

        // When clipboard-write is "ask" a prompt is displayed to the user asking
        // them to confirm the clipboard access. Each app runtime handles this
        // differently.
        // const confirm = self.config.clipboard_write == .ask;
        let confirm = self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Ask;
        // self.rt_surface.setClipboard(loc, &.{.{
        //     .mime = "text/plain",
        //     .data = buf[0..decoded.len :0],
        // }}, confirm) catch |err| {
        //     log.err("error setting clipboard string err={}", .{err});
        //     return;
        // };
        if let Err(err) = unsafe { &*self.rt_surface }.set_clipboard(
            loc,
            &[apprt::ClipboardContent {
                mime: b"text/plain".as_slice(),
                data: &buf[0..decoded_len],
            }],
            confirm,
        ) {
            // log.err("error setting clipboard string err={}", .{err});
            log::error!(target: LOG_SCOPE, "error setting clipboard string err={err:?}");
            return Ok(());
        }

        Ok(())
    }

    // fn copySelectionToClipboards(
    //     self: *Surface,
    //     sel: terminal.Selection,
    //     clipboards: []const apprt.Clipboard,
    //     format: input.Binding.Action.CopyToClipboard,
    // ) !void {
    //
    // (the inferred `!void` set is the allocating-writer OOM — the [`Error`]
    // union.)
    pub(super) fn copySelectionToClipboards(
        &mut self,
        sel: terminal::selection::Selection,
        clipboards: &[apprt::Clipboard],
        format: binding::CopyToClipboard,
    ) -> Result<(), Error> {
        // Create an arena to simplify memory management here.
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the owned buffers below free on drop
        // (the childExitedAbnormally precedent); the `Vec` sinks allocate
        // via the global allocator, so the arena allocator is not
        // materialized.

        // The options we'll use for all formatting. We'll just override the
        // emit format.
        // const opts: terminal.formatter.Options = .{
        let opts: terminal::formatter::Options = terminal::formatter::Options {
            // .emit = .plain, // We'll override this below
            emit: terminal::formatter::Format::Plain, // We'll override this below
            // .unwrap = true,
            unwrap: true,
            // .trim = self.config.clipboard_trim_trailing_spaces,
            trim: self.config.clipboard_trim_trailing_spaces,
            // .codepoint_map = self.config.clipboard_codepoint_map.map.list,
            //
            // (Zig copies the MultiArrayList header shallowly; the owned
            // `Options` clones the entries — the formatter.rs `CodepointMap`
            // Clone PORT NOTE.)
            codepoint_map: Some(self.config.clipboard_codepoint_map.map.list.clone()),
            // .background = self.io.terminal.colors.background.get(),
            background: unsafe { (*self.io.as_ptr()).terminal.colors.background.get() },
            // .foreground = self.io.terminal.colors.foreground.get(),
            foreground: unsafe { (*self.io.as_ptr()).terminal.colors.foreground.get() },
            // .palette = &self.io.terminal.colors.palette.current,
            palette: Some(unsafe {
                &raw const (*self.io.as_ptr()).terminal.colors.palette.current
            }),
        };

        // const ScreenFormatter = terminal.formatter.ScreenFormatter;
        use rustty_vt::formatter::ScreenFormatter;
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        //
        // (the `Vec<u8>` allocating-writer sink — the vt selectionString
        // precedent; each `toOwnedSliceSentinel(0)` below is a `mem::take`,
        // the length excluding the NUL sentinel.)
        let mut aw: Vec<u8> = Vec::new();
        // var contents: std.ArrayList(apprt.ClipboardContent) = .empty;
        //
        // PORT NOTE (arena ownership): `ClipboardContent` BORROWS its bytes;
        // the reference arena-allocates each buffer. The owned (mime, data)
        // pairs collected here are those arena allocations; the borrowed
        // `ClipboardContent` view is materialized at the `setClipboard` loop
        // below, once the buffers stop moving.
        let mut contents: Vec<(&[u8], Vec<u8>)> = Vec::new();
        // switch (format) {
        match format {
            // .plain => {
            binding::CopyToClipboard::Plain => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts);
                let mut formatter: ScreenFormatter = ScreenFormatter::init(
                    unsafe { (*self.io.as_ptr()).terminal.screens.active },
                    copy_options(&opts),
                );
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                //
                // (allocating writer: a failed write is an OOM — the vt
                // selectionString precedent.)
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));
            }

            // .vt => {
            binding::CopyToClipboard::Vt => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .vt;
                //     break :opts copy;
                // });
                let mut formatter: ScreenFormatter = ScreenFormatter::init(
                    unsafe { (*self.io.as_ptr()).terminal.screens.active },
                    {
                        let mut copy = copy_options(&opts);
                        copy.emit = terminal::formatter::Format::Vt;
                        copy
                    },
                );
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to VT format since it contains
                // escape sequences that should be preserved as-is
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));
            }

            // .html => {
            binding::CopyToClipboard::Html => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .html;
                //     break :opts copy;
                // });
                let mut formatter: ScreenFormatter = ScreenFormatter::init(
                    unsafe { (*self.io.as_ptr()).terminal.screens.active },
                    {
                        let mut copy = copy_options(&opts);
                        copy.emit = terminal::formatter::Format::Html;
                        copy
                    },
                );
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to HTML format since HTML
                // has its own character encoding and entity system
                // try contents.append(alloc, .{
                //     .mime = "text/html",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/html".as_slice(), std::mem::take(&mut aw)));
            }

            // .mixed => {
            binding::CopyToClipboard::Mixed => {
                // First, generate plain text with codepoint mappings applied
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts);
                let mut formatter: ScreenFormatter = ScreenFormatter::init(
                    unsafe { (*self.io.as_ptr()).terminal.screens.active },
                    copy_options(&opts),
                );
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));

                // assert(aw.written().len == 0);
                debug_assert!(aw.is_empty());
                // Second, generate HTML without codepoint mappings
                // formatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .html;
                //
                //     ...
                //     break :opts copy;
                // });
                formatter = ScreenFormatter::init(
                    unsafe { (*self.io.as_ptr()).terminal.screens.active },
                    {
                        let mut copy = copy_options(&opts);
                        copy.emit = terminal::formatter::Format::Html;

                        // We purposely don't emit background/foreground for mixed
                        // mode because the HTML contents is often used for rich text
                        // input and with trimmed spaces it looks pretty bad.
                        // copy.background = null;
                        copy.background = None;
                        // copy.foreground = null;
                        copy.foreground = None;

                        copy
                    },
                );
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to HTML format
                // try contents.append(alloc, .{
                //     .mime = "text/html",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/html".as_slice(), std::mem::take(&mut aw)));
            }
        }

        // assert(contents.items.len > 0);
        debug_assert!(!contents.is_empty());
        // (the borrowed `ClipboardContent` view over the owned pairs — the
        // arena-ownership PORT NOTE above.)
        let contents_items: Vec<apprt::ClipboardContent<'_>> = contents
            .iter()
            .map(|c| apprt::ClipboardContent {
                mime: c.0,
                data: &c.1,
            })
            .collect();
        // for (clipboards) |clipboard| self.rt_surface.setClipboard(
        //     clipboard,
        //     contents.items,
        //     false,
        // ) catch |err| {
        for clipboard in clipboards {
            if let Err(err) =
                unsafe { &*self.rt_surface }.set_clipboard(*clipboard, &contents_items, false)
            {
                // log.err(
                //     "error setting clipboard string clipboard={} err={}",
                //     .{ clipboard, err },
                // );
                log::error!(
                    target: LOG_SCOPE,
                    "error setting clipboard string clipboard={clipboard:?} err={err:?}"
                );
            }
        }

        Ok(())
    }

    /// Set the active selection and notify the apprt on a genuine state
    /// transition. All selection mutations route through here rather than
    /// `screen.select` directly so the notification fires consistently. To
    /// also copy per `copy_on_select`, use `setSelectionAndCopy`.
    ///
    /// This must be called with the renderer mutex held.
    // fn setSelection(self: *Surface, sel_: ?terminal.Selection) !void {
    pub(super) fn setSelection(
        &mut self,
        sel_: Option<terminal::selection::Selection>,
    ) -> Result<(), Error> {
        // Compute the transition before `select` below, which untracks (frees)
        // the previous selection's tracked pins; reading them after would be a
        // use-after-free.
        // const prev_ = self.io.terminal.screens.active.selection;
        let prev_ = unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection };
        // const changed = changed: {
        //     const prev = prev_ orelse break :changed sel_ != null;
        //     const sel = sel_ orelse break :changed true;
        //     break :changed !sel.eql(prev);
        // };
        let changed = 'changed: {
            let prev = match prev_ {
                Some(v) => v,
                None => break 'changed sel_.is_some(),
            };
            let sel = match sel_ {
                Some(v) => v,
                None => break 'changed true,
            };
            !sel.eql(prev)
        };

        // try self.io.terminal.screens.active.select(sel_);
        unsafe { (*(*self.io.as_ptr()).terminal.screens.active).select(sel_) }?;

        // if (changed) {
        if changed {
            // _ = self.rt_app.performAction(
            //     .{ .surface = self },
            //     .selection_changed,
            //     {},
            // ) catch |err| {
            //     log.warn("apprt failed selection_changed notification err={}", .{err});
            // };
            if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::surface_ref(&*self),
                apprt::Action::SelectionChanged,
            ) {
                // log.warn("apprt failed selection_changed notification err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "apprt failed selection_changed notification err={err:?}"
                );
            }
        }

        Ok(())
    }

    /// Set a selection and, per `copy_on_select`, copy it to the clipboard.
    /// For committing selection gestures (mouse release, select-all binding).
    ///
    /// This must be called with the renderer mutex held.
    // fn setSelectionAndCopy(self: *Surface, sel: terminal.Selection) !void {
    //
    // PORT NOTE: the C12 mouseButtonCallback (:3866/:4086/:4092) callsites
    // are live; the C14 performBindingAction (:5450) callsite lands later.
    pub(super) fn setSelectionAndCopy(
        &mut self,
        sel: terminal::selection::Selection,
    ) -> Result<(), Error> {
        // try self.setSelection(sel);
        self.setSelection(Some(sel))?;

        // switch (self.config.copy_on_select) {
        match self.config.copy_on_select {
            // .none => {},
            configpkg::value_types::CopyOnSelect::None => {}

            // The selection clipboard is set if supported, otherwise nothing is copied.
            // .primary => try self.copySelectionToClipboards(
            //     sel,
            //     &.{.selection},
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Primary => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Selection],
                binding::CopyToClipboard::Mixed,
            )?,

            // Only the standard clipboard is set.
            // .clipboard => try self.copySelectionToClipboards(
            //     sel,
            //     &.{.standard},
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Clipboard => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard],
                binding::CopyToClipboard::Mixed,
            )?,

            // Both standard and selection clipboards are set.
            // .both => try self.copySelectionToClipboards(
            //     sel,
            //     &.{ .standard, .selection },
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Both => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard, apprt::Clipboard::Selection],
                binding::CopyToClipboard::Mixed,
            )?,
        }

        Ok(())
    }

    /// Change the cell size for the terminal grid. This can happen as
    /// a result of changing the font size at runtime.
    // fn setCellSize(self: *Surface, size: rendererpkg.CellSize) !void {
    fn setCellSize(&mut self, size: rendererpkg::size::CellSize) -> Result<(), Error> {
        // Update our cell size within our size struct
        // self.size.cell = size;
        self.size.cell = size;
        // self.balancePaddingIfNeeded();
        self.balancePaddingIfNeeded();

        // Notify the terminal
        // self.queueIo(.{ .resize = self.size }, .unlocked);
        self.queueIo(
            termio::Message::Resize(self.size),
            termio::termio_impl::MutexState::Unlocked,
        );

        // Update our terminal default size if necessary.
        // self.recomputeInitialSize() catch |err| {
        //     // We don't treat this as a fatal error because not setting
        //     // an initial size shouldn't stop our terminal from working.
        //     log.warn("unable to recompute initial window size: {}", .{err});
        // };
        if let Err(err) = self.recomputeInitialSize() {
            // We don't treat this as a fatal error because not setting
            // an initial size shouldn't stop our terminal from working.
            // log.warn("unable to recompute initial window size: {}", .{err});
            log::warn!(target: LOG_SCOPE, "unable to recompute initial window size: {err:?}");
        }

        // Notify the window
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .cell_size,
        //     .{ .width = size.width, .height = size.height },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::CellSize(CellSize {
                width: size.width,
                height: size.height,
            }),
        )?;

        Ok(())
    }

    /// Change the font size.
    ///
    /// This can only be called from the main thread.
    // pub fn setFontSize(self: *Surface, size: font.face.DesiredSize) !void {
    pub fn setFontSize(&mut self, size: DesiredSize) -> Result<(), Error> {
        // log.debug("set font size size={}", .{size.points});
        log::debug!(target: LOG_SCOPE, "set font size size={}", size.points);

        // Update our font size so future changes work
        // self.font_size = size;
        self.font_size = size;

        // We need to build up a new font stack for this font size.
        // const font_grid_key, const font_grid = try self.app.font_grid_set.ref(
        //     &self.config.font,
        //     self.font_size,
        // );
        let (font_grid_key, font_grid) = unsafe {
            (*self.app)
                .font_grid_set
                .r#ref(&self.config.font, self.font_size)
        }?;
        // errdefer self.app.font_grid_set.deref(font_grid_key);
        // (fired explicitly at the failure site below — the only fallible
        // site in its live range.)

        // Set our cell size
        // try self.setCellSize(.{
        //     .width = font_grid.metrics.cell_width,
        //     .height = font_grid.metrics.cell_height,
        // });
        if let Err(err) = self.setCellSize(rendererpkg::size::CellSize {
            width: unsafe { &*font_grid }.metrics.cell_width,
            height: unsafe { &*font_grid }.metrics.cell_height,
        }) {
            // errdefer self.app.font_grid_set.deref(font_grid_key);
            unsafe { (*self.app).font_grid_set.deref(font_grid_key) };
            return Err(err);
        }

        // Notify our render thread of the new font stack. The renderer
        // MUST accept the new font grid and deref the old.
        // _ = self.renderer_thread.mailbox.push(.{
        //     .font_grid = .{
        //         .grid = font_grid,
        //         .set = &self.app.font_grid_set,
        //         .old_key = self.font_grid_key,
        //         .new_key = font_grid_key,
        //     },
        // }, .{ .forever = {} });
        //
        // PORT NOTE (Rust ownership): the reference key copies are SHALLOW
        // (Zig value-copies; the key memory belongs to the set's map). The
        // Rustty `Key` is OWNED (the shared_grid_set PORT NOTE), so the
        // message's copies are `Clone`s (the Key `Clone` PORT NOTE) and the
        // surface's old key value is dropped at the replacement below (a
        // plain free the reference doesn't need).
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }.push(
            rendererpkg::Message::FontGrid {
                grid: font_grid,
                set: unsafe { &raw mut (*self.app).font_grid_set },
                old_key: self.font_grid_key.clone(),
                new_key: font_grid_key.clone(),
            },
            Timeout::Forever,
        );

        // Once we've sent the key we can replace our key
        // self.font_grid_key = font_grid_key;
        self.font_grid_key = font_grid_key;
        // self.font_metrics = font_grid.metrics;
        self.font_metrics = unsafe { &*font_grid }.metrics;

        // Schedule render which also drains our mailbox
        // self.queueRender() catch unreachable;
        self.queueRender().unwrap_or_else(|_| unreachable!());

        Ok(())
    }

    /// This queues a render operation with the renderer thread. The render
    /// isn't guaranteed to happen immediately but it will happen as soon as
    /// practical.
    //
    // fn queueRender(self: *Surface) !void {
    //
    // PORT NOTE (ported with C7, ahead of its C8 row — Surface.zig:2461-2465;
    // selectionScrollTick/passwordInput call it): the inferred `!void` is the
    // xev notify error (the CORE_BLUEPRINT §5 xev-blocked one-line leaf).
    pub(super) fn queueRender(&mut self) -> Result<(), rustty_xev::NotifyError> {
        // try self.renderer_thread.wakeup.notify();
        unsafe { (*self.renderer_thread.as_ptr()).wakeup.notify() }?;
        Ok(())
    }

    /// Called by the apprt when the surface's display is realized.
    /// Notifies the renderer so it can begin rendering.
    /// Safe to call from the main thread.
    //
    // pub fn displayRealized(self: *Surface) !void {
    //     try self.renderer.displayRealized();
    // }   (`2f0b65346`, Surface.zig:2541-2546)
    //
    // Raw-self per `draw` (the renderer is aliased raw by the render thread).
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation; no protected `&mut Surface` may
    /// span the call.
    pub unsafe fn displayRealized(
        self_: *mut Surface<A>,
    ) -> Result<(), crate::renderer_seam::RendererError> {
        unsafe { crate::renderer_seam::Renderer::displayRealized((*self_).renderer.as_ptr()) }
    }

    /// Called by the apprt when the surface's display is unrealized (the surface
    /// is being destroyed or reparented). Safe to call from the main thread.
    //
    // pub fn displayUnrealized(self: *Surface) void {
    //     self.renderer.displayUnrealized();
    //
    //     // Wake the render thread so it notices `display_realized` is now false
    //     // and releases GPU resources (swap chain and shaders).
    //     self.renderer_thread.wakeup.notify() catch |err| {
    //         log.warn("failed to notify renderer thread of unrealize err={}", .{err});
    //     };
    // }   (`2f0b65346`, Surface.zig:2548-2558)
    ///
    /// # Safety
    /// As [`Surface::displayRealized`].
    pub unsafe fn displayUnrealized(self_: *mut Surface<A>) {
        unsafe { crate::renderer_seam::Renderer::displayUnrealized((*self_).renderer.as_ptr()) };

        // Wake the render thread so it notices `display_realized` is now false
        // and releases GPU resources (swap chain and shaders).
        if let Err(err) = unsafe { (*(*self_).renderer_thread.as_ptr()).wakeup.notify() } {
            // log.warn("failed to notify renderer thread of unrealize err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to notify renderer thread of unrealize err={err:?}"
            );
        }
    }

    // pub fn sizeCallback(self: *Surface, size: apprt.SurfaceSize) !void {
    //
    // (the inferred `!void` is `resize`'s — the [`Error`] union.)
    pub fn sizeCallback(&mut self, size: apprt::SurfaceSize) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // const new_screen_size: rendererpkg.ScreenSize = .{
        //     .width = size.width,
        //     .height = size.height,
        // };
        let new_screen_size: rendererpkg::size::ScreenSize = rendererpkg::size::ScreenSize {
            width: size.width,
            height: size.height,
        };

        // Update our screen size, but only if it actually changed. And if
        // the screen size didn't change, then our grid size could not have
        // changed, so we just return.
        // if (self.size.screen.equals(new_screen_size)) return;
        if self.size.screen.equals(new_screen_size) {
            return Ok(());
        }

        // try self.resize(new_screen_size);
        self.resize(new_screen_size)?;
        Ok(())
    }

    // fn resize(self: *Surface, size: rendererpkg.ScreenSize) !void {
    //
    // (the reference body has no failing site — the inferred `!void` set is
    // empty today; the `Result` shape carries the callers' `try`s.)
    pub(super) fn resize(&mut self, size: rendererpkg::size::ScreenSize) -> Result<(), Error> {
        // Save our screen size
        // self.size.screen = size;
        self.size.screen = size;
        // self.balancePaddingIfNeeded();
        self.balancePaddingIfNeeded();

        // Recalculate our grid size. Because Rustty supports fluid resizing,
        // its possible the grid doesn't change at all even if the screen size changes.
        // We have to update the IO thread no matter what because we send
        // pixel-level sizing to the subprocess.
        // const grid_size = self.size.grid();
        let grid_size = self.size.grid();
        // if (grid_size.columns < 5 and (self.size.padding.left > 0 or self.size.padding.right > 0)) {
        if grid_size.columns < 5 && (self.size.padding.left > 0 || self.size.padding.right > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
            log::warn!(
                target: LOG_SCOPE,
                "WARNING: very small terminal grid detected with padding set. Is your padding reasonable?"
            );
        }
        // if (grid_size.rows < 2 and (self.size.padding.top > 0 or self.size.padding.bottom > 0)) {
        if grid_size.rows < 2 && (self.size.padding.top > 0 || self.size.padding.bottom > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
            log::warn!(
                target: LOG_SCOPE,
                "WARNING: very small terminal grid detected with padding set. Is your padding reasonable?"
            );
        }

        // Mail the IO thread
        // self.queueIo(.{ .resize = self.size }, .unlocked);
        self.queueIo(
            termio::Message::Resize(self.size),
            termio::termio_impl::MutexState::Unlocked,
        );

        // Mail the render thread so it updates its padding and screen size.
        // _ = self.renderer_thread.mailbox.push(
        //     global.io(),
        //     .{ .resize = self.size },
        //     .forever,
        // );
        // self.queueRender() catch |err| {
        //     log.warn("failed to notify renderer of resize err={}", .{err});
        // };   (`2f0b65346`, Surface.zig:2599-2608)
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(rendererpkg::Message::Resize(self.size), Timeout::Forever);
        if let Err(err) = self.queueRender() {
            // log.warn("failed to notify renderer of resize err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify renderer of resize err={err:?}");
        }

        Ok(())
    }

    /// Recalculate the balanced padding if needed.
    // fn balancePaddingIfNeeded(self: *Surface) void {
    fn balancePaddingIfNeeded(&mut self) {
        // if (self.config.window_padding_balance == .false) return;
        if self.config.window_padding_balance == configpkg::value_types::WindowPaddingBalance::False
        {
            return;
        }
        // const content_scale = try self.rt_surface.getContentScale();
        //
        // PORT NOTE: the reference `try` inside a `void` fn type-checks only
        // because the comptime backend's `getContentScale` error set is EMPTY
        // (an infallible field read — embedded.zig:650); the seam trait
        // returns the opaque `ApprtError`, so the statically-empty `try` maps
        // to the infallible unwrap (the catch-unreachable convention).
        let content_scale = unsafe { &*self.rt_surface }
            .get_content_scale()
            .unwrap_or_else(|_| unreachable!());
        // const x_dpi = content_scale.x * font.face.default_dpi;
        let x_dpi = content_scale.x * f32::from(face::DEFAULT_DPI);
        // const y_dpi = content_scale.y * font.face.default_dpi;
        let y_dpi = content_scale.y * f32::from(face::DEFAULT_DPI);
        // self.size.balancePadding(self.config.scaledPadding(x_dpi, y_dpi), self.config.window_padding_balance);
        self.size.balance_padding(
            self.config.scaledPadding(x_dpi, y_dpi),
            self.config.window_padding_balance,
        );
    }
}
