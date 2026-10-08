//! PORTS: ghostty src/terminal/Terminal.zig — the pwd / title, `switchScreen` / `switchScreenMode`, `plainString` and `fullReset` cluster.

use super::*;

impl Terminal {
    /// Set the pwd for the terminal.
    // PORT NOTE (aliasing): `pwd` MAY alias `self.pwd` — a caller can hand
    // `get_pwd()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream a14a619ce.
    //
    // PORT FINDING (WS8 T7, proven under Miri): a `&[u8]` parameter is a
    // STRONGLY PROTECTED `SharedReadOnly` argument for the whole call, so
    // writing into the bytes it points at is Stacked-Borrows UB even with a
    // memmove — the protector, not the memcpy, is the blocker (memory:
    // `rust-mut-args-strongly-protected`). The alias-tolerant entry point is
    // therefore `set_pwd_raw`, which takes the source as a raw pointer and
    // forms no reference to it. This `&[u8]` wrapper stays for the many
    // non-aliasing internal callers; the vt-c FFI seam — the one place a
    // caller can hand `get_pwd()`'s own bytes back — calls `set_pwd_raw`.
    pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory> {
        // SAFETY: `pwd` is a live slice readable for `pwd.len()` bytes.
        unsafe { self.set_pwd_raw(pwd.as_ptr(), pwd.len()) }
    }

    /// `set_pwd` over a raw source, tolerating a source that ALIASES
    /// `self.pwd` (see the PORT FINDING on `set_pwd`).
    ///
    /// # Safety
    ///
    /// `src` must be readable for `src_len` bytes. It MAY overlap `self.pwd`'s
    /// buffer; it must not be derived from a live Rust reference that outlives
    /// this call.
    pub unsafe fn set_pwd_raw(
        &mut self,
        src: *const u8,
        src_len: usize,
    ) -> Result<(), OutOfMemory> {
        let pwd_len = src_len;
        // if (pwd.len == 0) { self.pwd.clearRetainingCapacity(); return; }
        if pwd_len == 0 {
            self.pwd.clear_retaining_capacity();
            return Ok(());
        }

        // const capacity = std.math.add(usize, pwd.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match pwd_len.checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.pwd.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: `ensureTotalCapacity` is GROW-ONLY and amortized
        // (`growCapacity`), and fallible: it grows through `self.gpa()`, the
        // active screen's allocator, looked up at call time (parity tail S7-3).
        // Growing can reallocate, which would invalidate an aliased `pwd` —
        // that cannot happen in the aliasing case, because an aliased source is
        // a subslice of `self.pwd`, hence `capacity <= self.pwd.capacity()`.
        let gpa = self.gpa();
        let v: *mut crate::alloc::ArrayListUnmanaged<u8> = core::ptr::addr_of_mut!(self.pwd);
        // SAFETY: `v` is the live `self.pwd`; `src` is readable for `src_len`
        // bytes and may overlap `*v`'s buffer, which `core::ptr::copy`
        // (memmove) tolerates. `set_len` exposes `capacity` bytes that are all
        // written before any read, and `u8` has no drop glue.
        unsafe {
            (*v).ensure_total_capacity(gpa, capacity)
                .map_err(|_| OutOfMemory)?;
            let dst: *mut u8 = (*v).as_mut_ptr();
            // self.pwd.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.pwd.items[0..pwd.len], pwd);
            core::ptr::copy(src, dst, src_len);
            // self.pwd.items[pwd.len] = 0;
            *dst.add(src_len) = 0;
        }
        Ok(())
    }

    /// Returns the pwd for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setPwd.
    // pub fn getPwd(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_pwd(&self) -> Option<&[u8]> {
        // if (self.pwd.items.len == 0) return null;
        if self.pwd.is_empty() {
            return None;
        }
        // return self.pwd.items[0 .. self.pwd.items.len - 1 :0];
        Some(&self.pwd[0..self.pwd.len() - 1])
    }

    /// Set the title for the terminal, as set by escape sequences (e.g. OSC 0/2).
    // PORT NOTE (aliasing): `t` MAY alias `self.title` — a caller can hand
    // `get_title()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream dace6d1c6.
    //
    // PORT FINDING (WS8 T7): see the finding on `set_pwd` — the alias-tolerant
    // entry point is `set_title_raw`.
    pub fn set_title(&mut self, t: &[u8]) -> Result<(), OutOfMemory> {
        // SAFETY: `t` is a live slice readable for `t.len()` bytes.
        unsafe { self.set_title_raw(t.as_ptr(), t.len()) }
    }

    /// `set_title` over a raw source, tolerating a source that ALIASES
    /// `self.title` (see the PORT FINDING on `set_pwd`).
    ///
    /// # Safety
    ///
    /// `src` must be readable for `src_len` bytes. It MAY overlap
    /// `self.title`'s buffer; it must not be derived from a live Rust
    /// reference that outlives this call.
    pub unsafe fn set_title_raw(
        &mut self,
        src: *const u8,
        src_len: usize,
    ) -> Result<(), OutOfMemory> {
        let t_len = src_len;
        // if (t.len == 0) { self.title.clearRetainingCapacity(); return; }
        if t_len == 0 {
            self.title.clear_retaining_capacity();
            return Ok(());
        }

        // const capacity = std.math.add(usize, t.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match t_len.checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.title.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: grow-only + amortized, through `self.gpa()` (as `set_pwd`).
        let gpa = self.gpa();
        let v: *mut crate::alloc::ArrayListUnmanaged<u8> = core::ptr::addr_of_mut!(self.title);
        // SAFETY: as in `set_pwd` above.
        unsafe {
            (*v).ensure_total_capacity(gpa, capacity)
                .map_err(|_| OutOfMemory)?;
            let dst: *mut u8 = (*v).as_mut_ptr();
            // self.title.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.title.items[0..t.len], t);
            core::ptr::copy(src, dst, src_len);
            // self.title.items[t.len] = 0;
            *dst.add(src_len) = 0;
        }
        Ok(())
    }

    /// Returns the title for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setTitle.
    // pub fn getTitle(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_title(&self) -> Option<&[u8]> {
        // if (self.title.items.len == 0) return null;
        if self.title.is_empty() {
            return None;
        }
        // return self.title.items[0 .. self.title.items.len - 1 :0];
        Some(&self.title[0..self.title.len() - 1])
    }

    /// Switch to the given screen type (alternate or primary).
    ///
    /// This does NOT handle behaviors such as clearing the screen,
    /// copying the cursor, etc. This should be handled by downstream
    /// callers.
    ///
    /// After calling this function, the `self.screen` field will point
    /// to the current screen, and the returned value will be the previous
    /// screen. If the return value is null, then the screen was not
    /// switched because it was already the active screen.
    ///
    /// Note: This is written in a generic way so that we can support
    /// more than two screens in the future if needed. There isn't
    /// currently a spec for this, but it is something I think might
    /// be useful in the future.
    pub fn switch_screen(
        &mut self,
        key: crate::screen_set::Key,
    ) -> Result<Option<*mut Screen>, OutOfMemory> {
        // If we're already on the requested screen we do nothing.
        // if (self.screens.active_key == key) return null;
        if self.screens.active_key == key {
            return Ok(None);
        }
        // const old = self.screens.active;
        let old: *mut Screen = self.screens.active;

        // We always end hyperlink state when switching screens.
        // We need to do this on the original screen.
        // old.endHyperlink();
        // SAFETY: `old` is a stable heap pointer owned by `self.screens`;
        // `end_hyperlink` takes `&mut Screen` and does not mutate `self.screens`.
        unsafe { (*old).end_hyperlink() };

        // Switch the screens/
        // const new = self.screens.get(key) orelse new: {
        //     const primary = self.screens.get(.primary).?;
        //     break :new try self.screens.getInit(
        //         old.alloc,
        //         key,
        //         .{ ... },
        //     );
        // };
        let new: *mut Screen = match self.screens.get(key) {
            Some(s) => s,
            None => {
                let primary: *mut Screen =
                    self.screens.get(crate::screen_set::Key::Primary).unwrap();
                // SAFETY: `old`/`primary` are stable heap pointers owned by
                // `self.screens`; we only read `old.alloc` and
                // `primary.pages.limits.bytes.explicit`.
                let old_alloc = unsafe { (*old).alloc };
                let primary_max = unsafe { (*primary).pages.limits.bytes.explicit };
                // Inherit our Kitty image settings from the primary screen
                // (see the Options fields below).
                let (primary_kitty_limit, primary_kitty_loading_limits) =
                    if crate::options::KITTY_GRAPHICS {
                        unsafe {
                            (
                                (*primary).kitty_images.total_limit,
                                (*primary).kitty_images.image_limits.clone(),
                            )
                        }
                    } else {
                        (0, kitty::graphics_image::Limits::DIRECT)
                    };
                self.screens.get_init(
                    old_alloc,
                    key,
                    crate::screen::Options {
                        cols: self.cols,
                        rows: self.rows,
                        // .max_scrollback_bytes = switch (key) {
                        //     .primary => primary.pages.limits.bytes.explicit,
                        //     .alternate => 0,
                        // },
                        max_scrollback_bytes: match key {
                            crate::screen_set::Key::Primary => Some(primary_max),
                            crate::screen_set::Key::Alternate => Some(0),
                        },
                        max_scrollback_lines: None,

                        // Inherit our Kitty image settings from the primary
                        // screen if we have to initialize.
                        // .kitty_image_storage_limit = if (comptime build_options.kitty_graphics)
                        //     primary.kitty_images.total_limit
                        // else
                        //     0,
                        // .kitty_image_loading_limits = if (comptime build_options.kitty_graphics)
                        //     primary.kitty_images.image_limits
                        // else {},
                        // PORT NOTE (WS8 T16): the original vt-12 deferral
                        // hard-coded `0`/`DIRECT` here on the grounds that
                        // `build_options.kitty_graphics` was the disabled case.
                        // Kitty graphics IS enabled in rustty
                        // (`options::KITTY_GRAPHICS`), so hard-coding `0` gave the
                        // lazily-created alternate screen a DISABLED image storage
                        // — kitty graphics silently did not work on the alt
                        // screen, and `set_limit(0)` also marked the fresh storage
                        // mutated. Now inherits from the primary, as the reference
                        // does. The reference's `else {}` is the `void` field; the
                        // Rustty `Options` field is always `Limits`, so `DIRECT`
                        // (all-false, the empty representation) stands in for the
                        // unused `void` on the disabled branch.
                        kitty_image_storage_limit: primary_kitty_limit,
                        kitty_image_loading_limits: primary_kitty_loading_limits,
                    },
                )?
            }
        };

        // The new screen should not have any hyperlinks set
        // assert(new.cursor.hyperlink_id == 0);
        // SAFETY: `new` is a stable heap pointer owned by `self.screens`.
        debug_assert!(unsafe { (*new).cursor.hyperlink_id } == 0);

        // Bring our charset state with us
        // new.charset = old.charset;
        // SAFETY: `old`/`new` are distinct stable heap pointers owned by
        // `self.screens` (the early return above guarantees `key != active_key`,
        // and `new` is either a different initialized screen or freshly created).
        unsafe { (*new).charset = (*old).charset.clone() };

        // Clear our selection
        // new.clearSelection();
        unsafe { (*new).clear_selection() };

        // if (comptime build_options.kitty_graphics) {
        //     // Mark kitty images as dirty so they redraw. Without this set
        //     // the images will remain where they were (the dirty bit on
        //     // the screen only tracks the terminal grid, not the images).
        //     new.kitty_images.dirty = true;
        // }
        // PORT NOTE (WS8 T16, same correction as the storage-limit inheritance
        // above): Kitty graphics IS enabled in rustty
        // (`options::KITTY_GRAPHICS`), so this set is NOT the disabled case. It
        // matters for re-entry into an ALREADY-CREATED screen (the primary
        // always exists) whose `dirty` a renderer has since cleared: without it
        // the destination screen's images never redraw, because
        // `flags.dirty.clear` below only tracks the terminal grid.
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `new` is a stable heap pointer owned by `self.screens`
            // (either an existing screen or the one just created above).
            unsafe { (*new).kitty_images.dirty = true };
        }

        // Mark our terminal as dirty to redraw the grid.
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;

        // Finalize the switch
        // self.screens.switchTo(key);
        self.screens.switch_to(key);

        // return old;
        Ok(Some(old))
    }

    /// Switch screen via a mode switch (e.g. mode 47, 1047, 1049).
    /// This is a much more opinionated operation than `switchScreen`
    /// since it also handles the behaviors of the specific mode,
    /// such as clearing the screen, saving/restoring the cursor,
    /// etc.
    ///
    /// This should be used for legacy compatibility with VT protocols,
    /// but more modern usage should use `switchScreen` instead and handle
    /// details like clearing the screen, cursor saving, etc. manually.
    pub fn switch_screen_mode(
        &mut self,
        mode: SwitchScreenMode,
        enabled: bool,
    ) -> Result<(), OutOfMemory> {
        // The behavior in this function is completely based on reading
        // the xterm source, specifically "charproc.c" for
        // `srm_ALTBUF`, `srm_OPT_ALTBUF`, and `srm_OPT_ALTBUF_CURSOR`.
        // We shouldn't touch anything in here without adding a unit
        // test AND verifying the behavior with xterm.

        // switch (mode) {
        //     .@"47" => {},
        //     .@"1047" => if (!enabled and self.screens.active_key == .alternate) {
        //         self.eraseDisplay(.complete, false);
        //     },
        //     .@"1049" => if (enabled) self.saveCursor(),
        // }
        match mode {
            SwitchScreenMode::M47 => {}

            // If we're disabling 1047 and we're on alt screen then
            // we clear the screen.
            SwitchScreenMode::M1047 => {
                if !enabled && self.screens.active_key == crate::screen_set::Key::Alternate {
                    self.erase_display(csi::EraseDisplay::Complete, false);
                }
            }

            // 1049 unconditionally saves the cursor on enabling, even
            // if we're already on the alternate screen.
            SwitchScreenMode::M1049 => {
                if enabled {
                    self.save_cursor();
                }
            }
        }

        // Switch screens first to whatever we're going to.
        // const to: ScreenSet.Key = if (enabled) .alternate else .primary;
        // const old_ = try self.switchScreen(to);
        let to: crate::screen_set::Key = if enabled {
            crate::screen_set::Key::Alternate
        } else {
            crate::screen_set::Key::Primary
        };
        let old_: Option<*mut Screen> = self.switch_screen(to)?;

        match mode {
            // For these modes, we need to copy the cursor. We only copy
            // the cursor if the screen actually changed, otherwise the
            // cursor is already copied. The cursor is copied regardless
            // of destination screen.
            // .@"47", .@"1047" => if (old_) |old| {
            //     self.screens.active.cursorCopy(old.cursor, .{ .hyperlink = false }) catch |err| {
            //         log.warn("cursor copy failed entering alt screen err={}", .{err});
            //     };
            // },
            SwitchScreenMode::M47 | SwitchScreenMode::M1047 => {
                if let Some(old) = old_ {
                    // SAFETY: `old` and `self.screens.active` are stable heap
                    // pointers owned by `self.screens`; `cursorCopy` takes
                    // `&mut Screen` and `old.cursor` is a `Copy` value.
                    let old_cursor = unsafe { (*old).cursor };
                    let active: *mut Screen = self.screens.active;
                    if let Err(err) = unsafe {
                        (*active).cursor_copy(
                            old_cursor,
                            crate::screen::CursorCopyOptions { hyperlink: false },
                        )
                    } {
                        // log.warn("cursor copy failed entering alt screen err={}", .{err});
                        // CORRECTION (task S-1c): a PORT NOTE here said this
                        // crate has no logging facility; it logs through the
                        // `log` facade, so the reference's warning is live.
                        log::warn!(
                            target: LOG_SCOPE,
                            "cursor copy failed entering alt screen err={err:?}"
                        );
                    }
                }
            }

            // Mode 1049 restores cursor on the primary screen when
            // we disable it.
            // .@"1049" => if (enabled) {
            //     assert(self.screens.active_key == .alternate);
            //     self.eraseDisplay(.complete, false);
            //     if (old_) |old| {
            //         self.screens.active.cursorCopy(old.cursor, .{ .hyperlink = false }) catch |err| {
            //             log.warn("cursor copy failed entering alt screen err={}", .{err});
            //         };
            //     }
            // } else {
            //     assert(self.screens.active_key == .primary);
            //     self.restoreCursor();
            // },
            SwitchScreenMode::M1049 => {
                if enabled {
                    debug_assert!(self.screens.active_key == crate::screen_set::Key::Alternate);
                    self.erase_display(csi::EraseDisplay::Complete, false);

                    // When we enter alt screen with 1049, we always copy the
                    // cursor from the primary screen (if we weren't already
                    // on it).
                    if let Some(old) = old_ {
                        // SAFETY: see the M47/M1047 arm above.
                        let old_cursor = unsafe { (*old).cursor };
                        let active: *mut Screen = self.screens.active;
                        if let Err(err) = unsafe {
                            (*active).cursor_copy(
                                old_cursor,
                                crate::screen::CursorCopyOptions { hyperlink: false },
                            )
                        } {
                            // log.warn("cursor copy failed entering alt screen err={}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "cursor copy failed entering alt screen err={err:?}"
                            );
                        }
                    }
                } else {
                    debug_assert!(self.screens.active_key == crate::screen_set::Key::Primary);
                    self.restore_cursor();
                }
            }
        }

        Ok(())
    }

    /// Return the current string value of the terminal. Newlines are
    /// encoded as "\n". This omits any formatting such as fg/bg.
    ///
    /// The caller must free the string.
    // PORT NOTE: the reference's inferred error set here also carries
    // `error.UnknownPoint` from `dumpStringAlloc`, but that arm is unreachable
    // for a `.viewport` point (the viewport tag always resolves a bottom-right),
    // so these keep the `OutOfMemory`-only skeleton signature their callers
    // (e.g. rustty-core surface.rs) expect and fold the impossible variant in.
    pub fn plain_string(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAlloc(alloc, .{ .viewport = .{} });
        self.active_mut()
            .dump_string_alloc(alloc, point::Point::Viewport(point::Coordinate::default()))
            .map_err(|_| OutOfMemory)
    }

    /// Same as plainString, but respects row wrap state when building the string.
    pub fn plain_string_unwrapped(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAllocUnwrapped(alloc, .{ .viewport = .{} });
        self.active_mut()
            .dump_string_alloc_unwrapped(
                alloc,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .map_err(|_| OutOfMemory)
    }

    /// Full reset.
    ///
    /// This will attempt to free the existing screen memory but if that fails
    /// this will reuse the existing memory. In the latter case, memory may
    /// be wasted (since its unused) but it isn't leaked.
    pub fn full_reset(&mut self) {
        // Ensure we're back on primary screen
        // self.screens.switchTo(.primary);
        //
        // Remove alternate screen
        // self.screens.remove(self.screens.active.alloc, .alternate);
        self.screens.switch_to(crate::screen_set::Key::Primary);
        // SAFETY: `self.screens.active` is now the primary screen (a stable heap
        // pointer owned by `self.screens`); we read only its `alloc`. `remove`
        // mutates `self.screens.all[alternate]`, which is a distinct entry from
        // the active (primary) screen, so the read isn't invalidated.
        let alloc = unsafe { (*self.screens.active).alloc };
        self.screens
            .remove(alloc, crate::screen_set::Key::Alternate);

        // Reset primary screen
        // self.screens.active.reset();
        // SAFETY: `self.screens.active` is the primary screen; `reset` takes
        // `&mut Screen` and does not mutate `self.screens`.
        unsafe { (*self.screens.active).reset() };

        // Reset our basic state
        // self.flags = .{
        //     // Visibility belongs to the view rather than terminal state, so a
        //     // terminal reset must not make a hidden view potentially visible.
        //     .visible = self.flags.visible,
        //
        //     // This is configuration based on the pty rather than terminal
        //     // state, so a terminal reset must not change it.
        //     .resize_pull_scrollback = self.flags.resize_pull_scrollback,
        //
        //     .xt_checksum = self.default_xt_checksum,
        // };
        // self.modes.reset();
        let visible = self.flags.visible;
        let resize_pull_scrollback = self.flags.resize_pull_scrollback;
        self.flags = Flags {
            // Visibility belongs to the view rather than terminal state, so a
            // terminal reset must not make a hidden view potentially visible.
            visible,

            // This is configuration based on the pty rather than terminal
            // state, so a terminal reset must not change it.
            resize_pull_scrollback,

            xt_checksum: self.default_xt_checksum,

            ..Flags::default()
        };
        self.modes.reset();
        // self.tabstops.reset(TABSTOP_INTERVAL);
        self.tabstops.reset(TABSTOP_INTERVAL as usize);
        // self.previous_char = null;
        self.previous_char = None;
        // self.pwd.clearRetainingCapacity();
        // self.title.clearRetainingCapacity();
        self.pwd.clear_retaining_capacity();
        self.title.clear_retaining_capacity();
        // self.glyph_glossary.clearAndFree(self.gpa());
        // PORT NOTE (parity tail S7-3): `clear_and_free` takes no allocator: it
        // frees every entry and the map's storage through the allocator the
        // map was bound to at `init` (`Glossary::empty_in(alloc)`), which is
        // `self.gpa()` (checked in debug builds).
        let gpa = self.gpa();
        self.glyph_glossary.allocator().debug_assert_same(gpa);
        self.glyph_glossary.clear_and_free();
        // A reset only interrupts an in-progress chunked OSC 72 command;
        // drag and drop registration survives, matching kitty.
        // if (self.kitty_dnd) |dnd| dnd.chunking = .{};
        if let Some(dnd) = self.kitty_dnd.as_mut() {
            dnd.chunking = Default::default();
        }
        // self.status_display = .main;
        self.status_display = ansi::StatusDisplay::Main;
        // self.scrolling_region = .{
        //     .top = 0,
        //     .bottom = self.rows - 1,
        //     .left = 0,
        //     .right = self.cols - 1,
        // };
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: self.rows - 1,
            left: 0,
            right: self.cols - 1,
        };
        // self.setCursorStyle(.default);
        self.set_cursor_style(ansi::CursorStyle::Default);
        // self.colors.palette.resetAll();
        self.colors.palette.reset_all();

        // Always mark dirty so we redraw everything
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;
    }
}
