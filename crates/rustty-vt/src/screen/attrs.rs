//! PORTS: ghostty src/terminal/Screen.zig — the cursor attribute cluster:
//! `setAttribute`, `manualStyleUpdate`, `splitForCapacity`, `appendGrapheme`,
//! `startHyperlink`/`startHyperlinkOnce`/`endHyperlink`, `cursorSetHyperlink`,
//! `cursorSetSemanticContent`.

use super::*;

impl Screen {
    /// Set a style attribute for the current cursor.
    ///
    /// If the style can't be set due to any internal errors (memory-related),
    /// then this will revert back to the existing style and return an error.
    pub fn set_attribute(
        &mut self,
        attr: crate::sgr::Attribute<'_>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::sgr::Attribute;
        use crate::style::Color;

        // If we fail to set our style for any reason, we should revert
        // back to the old style. If we fail to do that, we revert back to
        // the default style.
        let old_style = self.cursor.style;
        // PORT NOTE: this closure mirrors the reference `errdefer` block. Rust
        // has no `errdefer`, so we invoke it on the error path below.
        let revert = |s: &mut Screen| {
            s.cursor.style = old_style;
            if let Err(err) = s.manual_style_update() {
                log::warn!(
                    target: LOG_SCOPE,
                    "setAttribute error restoring old style after failure err={err:?}"
                );
                s.cursor.style = style::Style::default();
                s.manual_style_update()
                    .expect("manualStyleUpdate to default cannot fail");
            }
        };

        match attr {
            Attribute::Unset => {
                self.cursor.style = style::Style::default();
            }

            Attribute::Bold => {
                self.cursor.style.flags.bold = true;
            }

            Attribute::ResetBold => {
                // Bold and faint share the same SGR code for this
                self.cursor.style.flags.bold = false;
                self.cursor.style.flags.faint = false;
            }

            Attribute::Italic => {
                self.cursor.style.flags.italic = true;
            }

            Attribute::ResetItalic => {
                self.cursor.style.flags.italic = false;
            }

            Attribute::Faint => {
                self.cursor.style.flags.faint = true;
            }

            Attribute::Underline(v) => {
                self.cursor.style.flags.underline = v;
            }

            Attribute::UnderlineColor(rgb) => {
                self.cursor.style.underline_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::Color256Underline(idx) => {
                self.cursor.style.underline_color = Color::Palette(idx);
            }

            Attribute::ResetUnderlineColor => {
                self.cursor.style.underline_color = Color::None;
            }

            Attribute::Overline => {
                self.cursor.style.flags.overline = true;
            }

            Attribute::ResetOverline => {
                self.cursor.style.flags.overline = false;
            }

            Attribute::Blink => {
                self.cursor.style.flags.blink = true;
            }

            Attribute::ResetBlink => {
                self.cursor.style.flags.blink = false;
            }

            Attribute::Inverse => {
                self.cursor.style.flags.inverse = true;
            }

            Attribute::ResetInverse => {
                self.cursor.style.flags.inverse = false;
            }

            Attribute::Invisible => {
                self.cursor.style.flags.invisible = true;
            }

            Attribute::ResetInvisible => {
                self.cursor.style.flags.invisible = false;
            }

            Attribute::Strikethrough => {
                self.cursor.style.flags.strikethrough = true;
            }

            Attribute::ResetStrikethrough => {
                self.cursor.style.flags.strikethrough = false;
            }

            Attribute::DirectColorFg(rgb) => {
                self.cursor.style.fg_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::DirectColorBg(rgb) => {
                self.cursor.style.bg_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::Color8Fg(n) => {
                self.cursor.style.fg_color = Color::Palette(n.cval());
            }

            Attribute::Color8Bg(n) => {
                self.cursor.style.bg_color = Color::Palette(n.cval());
            }

            Attribute::ResetFg => self.cursor.style.fg_color = Color::None,

            Attribute::ResetBg => self.cursor.style.bg_color = Color::None,

            Attribute::Color8BrightFg(n) => {
                self.cursor.style.fg_color = Color::Palette(n.cval());
            }

            Attribute::Color8BrightBg(n) => {
                self.cursor.style.bg_color = Color::Palette(n.cval());
            }

            Attribute::Color256Fg(idx) => {
                self.cursor.style.fg_color = Color::Palette(idx);
            }

            Attribute::Color256Bg(idx) => {
                self.cursor.style.bg_color = Color::Palette(idx);
            }

            Attribute::Unknown(_) => return Ok(()),
        }

        // If the attribute didn't change our style then we can skip the
        // style update entirely: our current style ID is already correct.
        // This is a common case in the wild where programs re-assert the
        // same style repeatedly (e.g. per span or per line).
        // PORT NOTE (parity cee35cabf).
        if self.cursor.style.eql(&old_style) {
            return Ok(());
        }

        match self.manual_style_update() {
            Ok(()) => Ok(()),
            Err(e) => {
                revert(self);
                Err(e)
            }
        }
    }

    /// Call this whenever you manually change the cursor style.
    ///
    /// This function can NOT fail if the cursor style is changing to the
    /// default style.
    ///
    /// If this returns an error, the style change did not take effect and
    /// the cursor style is reverted back to the default. The only scenario
    /// this returns an error is if there is a physical memory allocation failure
    /// or if there is no possible way to increase style capacity to store
    /// the style.
    ///
    /// This function WILL split pages as necessary to accommodate the new style.
    /// So if OutOfSpace is returned, it means that even after splitting the page
    /// there was still no room for the new style.
    pub fn manual_style_update(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        // PORT NOTE: `defer self.assertIntegrity()` — Zig's defer runs on ERROR
        // returns too, so the body lives in an inner fn and the integrity gate
        // runs unconditionally after it, on every exit path including the `?`
        // splitForCapacity early-return (VALIDATION_REPORT §3e / §3c.9).
        let result = self.manual_style_update_inner();
        self.assert_integrity();
        result
    }

    /// The body of `manual_style_update` (the reference fn minus its `defer`);
    /// see the PORT NOTE above.
    fn manual_style_update_inner(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::{IncreaseCapacity, IncreaseCapacityError};
        use crate::ref_counted_set::AddError;

        let mut page: *mut Page =
            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

        // std.log.warn("active styles={}", .{page.styles.count()});

        // Release our previous style if it was not default.
        if self.cursor.style_id != style::DEFAULT_ID {
            unsafe {
                let base = (*page).memory_ptr();
                (*page).styles.release(base, self.cursor.style_id);
            }
        }

        // If our new style is the default, just reset to that
        if self.cursor.style.default_style() {
            self.cursor.style_id = style::DEFAULT_ID;
            return Ok(());
        }

        // Clear the cursor style ID to prevent weird things from happening
        // if the page capacity has to be adjusted which would end up calling
        // manual_style_update again.
        //
        // This also ensures that if anything fails below, we fall back to
        // clearing our style.
        self.cursor.style_id = style::DEFAULT_ID;

        // After setting the style, we need to update our style map.
        // Note that we COULD lazily do this in print. We should look into
        // if that makes a meaningful difference. Our priority is to keep print
        // fast because setting a ton of styles that do nothing is uncommon
        // and weird.
        let add_result = unsafe {
            let base = (*page).memory_ptr();
            (*page).styles.add(base, self.cursor.style)
        };
        let id = match add_result {
            Ok(id) => id,
            Err(err) => {
                // Our style map is full or needs to be rehashed, so we need to
                // increase style capacity (or rehash).
                let node = match self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    match err {
                        AddError::OutOfMemory => Some(IncreaseCapacity::Styles),
                        AddError::NeedsRehash => None,
                    },
                ) {
                    Ok(node) => node,
                    Err(IncreaseCapacityError::OutOfMemory) => {
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(IncreaseCapacityError::OutOfSpace)
                        if self.reclaim_pinned_styles_before_split(page) =>
                    {
                        // PORT NOTE (R13, UD80): a same-capacity re-clone
                        // freed style slots; see the helper.
                        unsafe { (*self.cursor.page_pin).node }
                    }
                    Err(IncreaseCapacityError::OutOfSpace) => {
                        // Out of space, we need to split the page. Split wherever
                        // is using less capacity and hope that works. If it doesn't
                        // work, we tried.
                        // PORT NOTE: the reference `try self.splitForCapacity(..)`
                        // unifies `SplitError` into `IncreaseCapacityError` (the
                        // two error sets share `{OutOfMemory, OutOfSpace}`); we map
                        // explicitly.
                        self.split_for_capacity(unsafe { *self.cursor.page_pin })
                            .map_err(|e| match e {
                                pagelist::grow::SplitError::OutOfMemory => {
                                    IncreaseCapacityError::OutOfMemory
                                }
                                pagelist::grow::SplitError::OutOfSpace => {
                                    IncreaseCapacityError::OutOfSpace
                                }
                            })?;
                        unsafe { (*self.cursor.page_pin).node }
                    }
                };

                page = unsafe { crate::pagelist::Node::page(node) };
                match unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.add(base, self.cursor.style)
                } {
                    Ok(id) => id,
                    Err(AddError::OutOfMemory) => {
                        // This shouldn't happen because increaseCapacity is
                        // guaranteed to increase our capacity by at least one and
                        // we only need one space, but again, I don't want to crash
                        // here so let's log loudly and reset.
                        log::error!(target: LOG_SCOPE, "style addition failed after capacity increase");
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(AddError::NeedsRehash) => {
                        // This should be impossible because we rehash above
                        // and rehashing should never result in a duplicate. But
                        // we don't want to simply hard crash so log it and
                        // clear our style.
                        log::error!(target: LOG_SCOPE, "style rehash resulted in needs rehash");
                        return Ok(());
                    }
                }
            }
        };

        self.cursor.style_id = id;
        Ok(())
    }

    /// Before `manualStyleUpdate` splits the cursor's page because its full
    /// style set cannot grow (`OutOfSpace`), re-clone the page once at the
    /// SAME capacity if the set holds a pinned style, and report whether the
    /// re-clone freed at least one style slot.
    ///
    /// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD80. The
    /// reference has no such step. The port pins a style whose `u16` reference
    /// count saturates (`ref_counted_set.rs`, `bump_ref`), and a pinned style
    /// stays counted after its cells are overwritten. Page clones recount from
    /// the cells and drop such orphans, and below the ceiling every style-set
    /// growth is a clone, but the capacity projection counts the orphans, so
    /// the style capacity can climb to `maxInt(u16)`; there growth returns
    /// `OutOfSpace` and the reference's fallback splits the page. The cursor
    /// can stay on the full half, and every later new style then fails
    /// (`set_attribute` keeps the previous style) although the set holds
    /// styles nothing uses — phantom references the reference never has.
    /// This step is the same clone growth already performs, at the capacity
    /// the page has. Returns `true` only if the style count fell; the caller
    /// then adds on the re-cloned page. Returns `false` if the set holds no
    /// pinned style (every count is exact, a re-clone can reclaim nothing, and
    /// the reference path runs unchanged, with no clone), if the clone
    /// reclaimed nothing (the styles are genuinely in use), or if it could not
    /// allocate; the caller then splits as the reference does. It runs at most
    /// once per `manual_style_update` call, with no retry, so it cannot loop.
    fn reclaim_pinned_styles_before_split(&mut self, page: *mut Page) -> bool {
        if !pagelist::grow::styles_hold_pinned(page) {
            return false;
        }
        let before = unsafe { (*page).styles.count() };
        // `Screen::increase_capacity`, not the PageList's, so the cursor's
        // hyperlink reference and cached row/cell pointers are migrated. The
        // cursor's style was released and its id reset to the default above,
        // so there is no style to re-add.
        match self.increase_capacity(unsafe { (*self.cursor.page_pin).node }, None) {
            Ok(node) => unsafe { (*crate::pagelist::Node::page(node)).styles.count() < before },
            Err(_) => false,
        }
    }

    /// Split at the given pin so that the pinned row moves to the page
    /// with less used capacity after the split.
    ///
    /// The primary use case for this is to handle IncreaseCapacityError
    /// OutOfSpace conditions where we need to split the page in order
    /// to make room for more managed memory.
    ///
    /// If the caller cares about where the pin moves to, they should
    /// setup a tracked pin before calling this and then check that.
    /// In many calling cases, the input pin is tracked (e.g. the cursor
    /// pin).
    ///
    /// If this returns OOM then its a system OOM. If this returns OutOfSpace
    /// then it means the page can't be split further.
    fn split_for_capacity(&mut self, pin: Pin) -> Result<(), pagelist::grow::SplitError> {
        // Get our capacities. We include our target row because its
        // capacity will be preserved.
        let bytes_above = Page::layout(unsafe {
            (*crate::pagelist::Node::page(pin.node)).exact_row_capacity(0, pin.y as usize + 1)
        })
        .total_size;
        let bytes_below = Page::layout(unsafe {
            let rows = crate::pagelist::Node::rows(pin.node) as usize;
            (*crate::pagelist::Node::page(pin.node)).exact_row_capacity(pin.y as usize, rows)
        })
        .total_size;

        // We need to track the old cursor pin because if our split
        // moves the cursor pin we need to update our accounting.
        let old_cursor = unsafe { *self.cursor.page_pin };

        // If our bytes above are less than bytes below, we move the pin
        // to split down one since splitting includes the pinned row in
        // the new node.
        self.pages.split(if bytes_above < bytes_below {
            pin.down(1).unwrap_or(pin)
        } else {
            pin
        })?;

        // Cursor didn't change nodes, we're done.
        if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, old_cursor.node) {
            return Ok(());
        }

        // Cursor changed, we need to restore the old pin then use
        // cursorChangePin to move to the new pin. The old node is guaranteed
        // to still exist, just not the row.
        //
        // Note that page_row and all that will be invalid, it points to the
        // new node, but at the time of writing this we don't need any of that
        // to be right in cursorChangePin.
        let new_cursor = unsafe { *self.cursor.page_pin };
        unsafe { *self.cursor.page_pin = old_cursor };
        self.cursor_change_pin(new_cursor);
        Ok(())
    }

    /// Append a grapheme to the given cell within the current cursor row.
    pub fn append_grapheme(
        &mut self,
        cell: *mut Cell,
        cp: u21,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        // PORT NOTE: `defer self.cursor.page_pin.node.data.assertIntegrity()` —
        // Zig's defer runs on ERROR returns too (and reads the pin at DEFER
        // time, i.e. the possibly-reloaded pin), so the body lives in an inner
        // fn and the page integrity gate runs unconditionally after it against
        // the then-current pin (VALIDATION_REPORT §3e / §3c.9).
        let result = self.append_grapheme_inner(cell, cp);
        unsafe { (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).assert_integrity() };
        result
    }

    /// The body of `append_grapheme` (the reference fn minus its `defer`); see
    /// the PORT NOTE above.
    fn append_grapheme_inner(
        &mut self,
        cell: *mut Cell,
        cp: u21,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        let result = unsafe {
            (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).append_grapheme(
                self.cursor.page_row,
                cell,
                cp,
            )
        };
        match result {
            Ok(()) => {}
            Err(_oom) => {
                // We need to determine the actual cell index of the cell so
                // that after we adjust the capacity we can reload the cell.
                let cell_idx: usize = {
                    let cells: *mut Cell = self.cursor.page_cell;
                    let zero: *mut Cell = unsafe { cells.sub(self.cursor.x as usize) };
                    let target: *mut Cell = cell;
                    (target as usize - zero as usize) / core::mem::size_of::<Cell>()
                };

                // Adjust our capacity. This will update our cursor page pin and
                // force us to reload.
                let _ = self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    Some(IncreaseCapacity::GraphemeBytes),
                )?;

                // The cell pointer is now invalid, so we need to get it from
                // the reloaded cursor pointers.
                let reloaded_cell: *mut Cell = match cell_idx.cmp(&(self.cursor.x as usize)) {
                    core::cmp::Ordering::Equal => self.cursor.page_cell,
                    core::cmp::Ordering::Less => self.cursor_cell_left(
                        (self.cursor.x as usize - cell_idx) as size::CellCountInt,
                    ),
                    core::cmp::Ordering::Greater => self.cursor_cell_right(
                        (cell_idx - self.cursor.x as usize) as size::CellCountInt,
                    ),
                };

                unsafe {
                    (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).append_grapheme(
                        self.cursor.page_row,
                        reloaded_cell,
                        cp,
                    )
                }
                .map_err(|_err2| {
                    // This should never happen because we just increased capacity.
                    // Log loudly but still return an error so we don't just
                    // crash.
                    log::error!(target: LOG_SCOPE, "grapheme append failed after capacity increase");
                    pagelist::grow::IncreaseCapacityError::OutOfMemory
                })?;
            }
        }
        Ok(())
    }

    /// Start the hyperlink state. Future cells will be marked as hyperlinks with
    /// this state. Note that various terminal operations may clear the hyperlink
    /// state, such as switching screens (alt screen).
    ///
    /// `uri`/`id_` must NOT point into the screen's *current* hyperlink state
    /// (`cursor.hyperlink`), which this call releases. Use
    /// [`Screen::start_hyperlink_raw`] for that; see its `# Safety` section for
    /// why the distinction exists.
    ///
    /// This is sound as a safe function because that restriction is
    /// unviolatable from safe code: `cursor.hyperlink` is an
    /// `Option<*mut Hyperlink>`, so a slice into the current hyperlink's
    /// allocation can only be produced by `unsafe` code, which is then subject
    /// to `start_hyperlink_raw`'s contract instead.
    pub fn start_hyperlink(
        &mut self,
        uri: &[u8],
        id_: Option<&[u8]>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        // SAFETY: `uri`/`id_` are live readable slices for the whole call, and
        // per this function's documented restriction they do not alias the
        // current hyperlink state.
        unsafe { self.start_hyperlink_raw(uri as *const [u8], id_.map(|id| id as *const [u8])) }
    }

    /// [`Screen::start_hyperlink`] taking RAW slice pointers, so that the
    /// arguments may alias the screen's current hyperlink state.
    ///
    /// PORT NOTE (P3 F2): the oracle is
    /// `startHyperlink(self: *Screen, uri: []const u8, id_: ?[]const u8)` and
    /// its `startHyperlinkOnce` explicitly documents that "the source slices are
    /// allowed to reference our current hyperlink" — the reference even has a
    /// test for it (`"Screen: hyperlink accepts its current values"`). That
    /// contract is expressible in Zig, whose slices are unprotected pointers,
    /// but NOT with Rust `&[u8]` parameters: a reference-typed argument is
    /// STRONGLY PROTECTED for the entire duration of the call, and this call
    /// reaches `end_hyperlink`, which drops the `Box<Hyperlink>` those slices
    /// point into. Freeing a strongly-protected allocation is Undefined
    /// Behavior even though the reference is never read again — Miri: "not
    /// granting access to tag <N> because that would remove [SharedReadOnly for
    /// <M>] which is strongly protected", with `<M>` reported as `id_:
    /// Option<&[u8]>`. Raw pointers carry no protector, which is exactly the
    /// Zig parameter semantics, so the aliasing contract is ported by taking
    /// `*const [u8]`. (Same class as the `&mut Page`/`&mut [Cell]` raw-pointer
    /// argument conversions elsewhere in this crate.)
    ///
    /// # Safety
    ///
    /// `uri` and `id_` must point at readable `[u8]` slices on entry. They MAY
    /// alias the current hyperlink state: both are copied into owned buffers
    /// before that state is released. They must not be dangling.
    pub unsafe fn start_hyperlink_raw(
        &mut self,
        uri: *const [u8],
        id_: Option<*const [u8]>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::page::InsertHyperlinkError;
        use crate::pagelist::grow::IncreaseCapacity;

        // Create our pending entry. This copies both byte slices out of caller
        // memory BEFORE anything below can release the current hyperlink, which
        // is what makes the aliasing contract above work.
        //
        // PORT NOTE (OOM contract): these copies are `try alloc.dupe`'s Rust
        // equivalent (`hyperlink::try_dupe_bytes`), NOT `to_vec` — this function
        // ADVERTISES `IncreaseCapacityError!void` exactly like the reference's
        // `startHyperlink`, so an allocation failure must RETURN, not abort the
        // process. Both copies happen before the implicit-ID counter is bumped,
        // so their failure needs no rollback (the reference's `errdefer` covers
        // the same window because its `dupe` runs after the bump — the
        // observable state, "counter unchanged", is identical).
        // SAFETY: caller contract — `uri` is a readable slice on entry.
        let uri_owned = unsafe { hyperlink::try_dupe_bytes(&*uri) }
            .map_err(|_| pagelist::grow::IncreaseCapacityError::OutOfMemory)?;
        let link_id: hyperlink::HyperlinkId = if let Some(id) = id_ {
            // SAFETY: caller contract — `id_` is a readable slice on entry.
            hyperlink::HyperlinkId::Explicit(
                unsafe { hyperlink::try_dupe_bytes(&*id) }
                    .map_err(|_| pagelist::grow::IncreaseCapacityError::OutOfMemory)?,
            )
        } else {
            let implicit = self.cursor.hyperlink_implicit_id;
            // defer self.cursor.hyperlink_implicit_id +%= 1;
            self.cursor.hyperlink_implicit_id = self.cursor.hyperlink_implicit_id.wrapping_add(1);
            hyperlink::HyperlinkId::Implicit(implicit)
        };
        let link: hyperlink::Hyperlink = hyperlink::Hyperlink {
            uri: uri_owned,
            id: link_id,
        };
        // PORT NOTE: the reference `errdefer` decrements `hyperlink_implicit_id`
        // on the implicit path. The fallible work below is the per-attempt
        // `dupe` and the `increaseCapacity` calls; on those errors we restore
        // the counter to mirror the `errdefer`.
        let implicit_inc = matches!(link.id, hyperlink::HyperlinkId::Implicit(_));

        // Loop until we have enough page memory to add the hyperlink
        loop {
            // PORT NOTE: the reference passes `link` (borrowed slices, no
            // allocation) and `startHyperlinkOnce` does `try source.dupe(alloc)`
            // itself. We hand the callee an OWNED per-attempt copy instead (it
            // moves it straight into its heap allocation), so this is still
            // exactly one deep copy per attempt — but it must be the FALLIBLE
            // one, not `Clone`, which aborts.
            let attempt = match link.dupe() {
                Ok(v) => v,
                Err(hyperlink::OutOfMemory) => {
                    if implicit_inc {
                        // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                        self.cursor.hyperlink_implicit_id =
                            self.cursor.hyperlink_implicit_id.wrapping_sub(1);
                    }
                    return Err(pagelist::grow::IncreaseCapacityError::OutOfMemory);
                }
            };
            match self.start_hyperlink_once(attempt) {
                Ok(()) => return Ok(()),
                Err(StartHyperlinkOnceError::OutOfMemory) => {
                    // An actual self.alloc OOM is a fatal error.
                    if implicit_inc {
                        // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                        self.cursor.hyperlink_implicit_id =
                            self.cursor.hyperlink_implicit_id.wrapping_sub(1);
                    }
                    return Err(pagelist::grow::IncreaseCapacityError::OutOfMemory);
                }
                Err(StartHyperlinkOnceError::Insert(insert_err)) => {
                    let adj = match insert_err {
                        // strings table is out of memory, adjust it up
                        InsertHyperlinkError::StringsOutOfMemory => {
                            Some(IncreaseCapacity::StringBytes)
                        }
                        // hyperlink set is out of memory, adjust it up
                        InsertHyperlinkError::SetOutOfMemory => {
                            Some(IncreaseCapacity::HyperlinkBytes)
                        }
                        // hyperlink set is too full, rehash it
                        InsertHyperlinkError::SetNeedsRehash => None,
                    };
                    if let Err(e) =
                        self.increase_capacity(unsafe { (*self.cursor.page_pin).node }, adj)
                    {
                        if implicit_inc {
                            // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                            self.cursor.hyperlink_implicit_id =
                                self.cursor.hyperlink_implicit_id.wrapping_sub(1);
                        }
                        return Err(e);
                    }
                }
            }

            self.assert_integrity();
        }
    }

    /// This is like startHyperlink but if we have to adjust page capacities
    /// this returns error.PageAdjusted. This is useful so that we unwind
    /// all the previous state and try again.
    pub(super) fn start_hyperlink_once(
        &mut self,
        source: hyperlink::Hyperlink,
    ) -> Result<(), StartHyperlinkOnceError> {
        // Allocate our new Hyperlink entry in non-page memory. This
        // lets us quickly get access to URI, ID.
        // const link = try self.alloc.create(hyperlink.Hyperlink);
        // errdefer self.alloc.destroy(link);
        // link.* = try source.dupe(self.alloc);
        // errdefer link.deinit(self.alloc);
        //
        // PORT NOTE (parity tail S7-3): both go through the screen's allocator
        // (`create_cursor_hyperlink`), fallibly, as the reference's `try`s do.
        // `source` is the caller's owned argument (the reference's by-value
        // `link` with borrowed slices); it is dropped here, on the Rust heap.
        let link: *mut CursorHyperlink = create_cursor_hyperlink(self.alloc, &source)
            .map_err(|_| StartHyperlinkOnceError::OutOfMemory)?;

        // End any prior hyperlink only after duplicating the new value. The
        // source slices are allowed to reference our current hyperlink.
        self.end_hyperlink();

        // Insert the hyperlink into page memory
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
        // PORT NOTE: the reference passes `link.*` BY VALUE with borrowed slices
        // — no allocation. `Hyperlink` owns its `Vec`s here, so the by-value
        // `insert_hyperlink` argument needs a copy; it must be the fallible
        // `dupe`, not `Clone`, for the same reason as above.
        //
        // The copy is `source` itself (the caller's owned per-attempt value,
        // on the Rust heap): moving it in makes no allocation the reference
        // does not make.
        let id: hyperlink::Id = match unsafe { (*page).insert_hyperlink(source) } {
            Ok(id) => id,
            Err(e) => {
                // errdefer link.deinit(self.alloc); errdefer self.alloc.destroy(link);
                unsafe { destroy_cursor_hyperlink(link) };
                return Err(StartHyperlinkOnceError::Insert(e));
            }
        };

        // Save it all
        self.cursor.hyperlink = Some(link);
        self.cursor.hyperlink_id = id;
        Ok(())
    }

    /// End the hyperlink state so that future cells aren't part of the
    /// current hyperlink (if any). This is safe to call multiple times.
    pub fn end_hyperlink(&mut self) {
        // If we have no hyperlink state then do nothing
        if self.cursor.hyperlink_id == 0 {
            debug_assert!(self.cursor.hyperlink.is_none());
            return;
        }

        // Release the old hyperlink state. If there are cells using the
        // hyperlink this will work because the creation creates a reference
        // and all additional cells create a new reference. This release will
        // just release our initial reference.
        //
        // If the ref count reaches zero the set will not delete the item
        // immediately; it is kept around in case it is used again (this is
        // how RefCountedSet works). This causes some memory fragmentation but
        // is fine because if it is ever pruned the context deleted callback
        // will be called.
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
        unsafe {
            let base = (*page).memory_ptr();
            (*page)
                .hyperlink_set
                .release(base, self.cursor.hyperlink_id);
        }
        let link = self.cursor.hyperlink.unwrap();
        // self.cursor.hyperlink.?.deinit(self.alloc);
        // self.alloc.destroy(self.cursor.hyperlink.?);
        unsafe { destroy_cursor_hyperlink(link) };
        self.cursor.hyperlink_id = 0;
        self.cursor.hyperlink = None;
    }

    /// Set the current hyperlink state on the current cell.
    pub fn cursor_set_hyperlink(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        debug_assert!(self.cursor.hyperlink_id != 0);

        let mut page: *mut Page =
            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
        match unsafe {
            (*page).set_hyperlink(
                self.cursor.page_row,
                self.cursor.page_cell,
                self.cursor.hyperlink_id,
            )
        } {
            Ok(()) => {
                // Success, increase the refcount for the hyperlink.
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).hyperlink_set.use_id(base, self.cursor.hyperlink_id);
                }
                Ok(())
            }
            // hyperlink_map is out of space, realloc the page to be larger
            Err(_hyperlink_map_oom) => {
                // Attempt to allocate the space that would be required to
                // insert a new copy of the cursor hyperlink uri in to the
                // string alloc, since right now increaseCapacity always just
                // adds an extra copy even if one already exists in the page.
                // If this alloc fails then we know we also need to grow our
                // string bytes.
                //
                // FIXME: increaseCapacity should not do this.
                while let Some(link) = self.cursor.hyperlink {
                    let uri_len = unsafe { (*link).uri.len() };
                    if unsafe { Page::string_alloc_alloc(page, uri_len) }.is_ok() {
                        // We don't bother freeing because we're
                        // about to free the entire page anyway.
                        break;
                    }

                    // We didn't have enough room, let's increase string bytes
                    let new_node = self.increase_capacity(
                        unsafe { (*self.cursor.page_pin).node },
                        Some(IncreaseCapacity::StringBytes),
                    )?;
                    debug_assert!(std::ptr::eq(new_node, unsafe {
                        (*self.cursor.page_pin).node
                    }));
                    page = unsafe { crate::pagelist::Node::page(new_node) };
                }

                // The hyperlink map is fixed-capacity, so reaching this error
                // means live entries fill the usable map capacity and the page
                // must grow.
                let _ = self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    Some(IncreaseCapacity::HyperlinkBytes),
                )?;

                // Retry
                //
                // We check that the cursor hyperlink hasn't been destroyed
                // by the capacity adjustment first though- since despite the
                // terrible code above, that can still apparently happen ._.
                if self.cursor.hyperlink_id > 0 {
                    return self.cursor_set_hyperlink();
                }
                Ok(())
            }
        }
    }

    /// Modify the semantic content type of the cursor. This should
    /// be preferred over setting it manually since it handles all the
    /// proper accounting.
    pub fn cursor_set_semantic_content(&mut self, t: SemanticContentArg) {
        match t {
            SemanticContentArg::Output => {
                self.cursor.semantic_content = pagepkg::SemanticContent::Output;
                self.cursor.semantic_content_clear_eol = false;
            }

            SemanticContentArg::Input(clear) => {
                self.cursor.semantic_content = pagepkg::SemanticContent::Input;
                self.cursor.semantic_content_clear_eol = match clear {
                    SemanticContentInput::ClearExplicit => false,
                    SemanticContentInput::ClearEol => true,
                };
            }

            SemanticContentArg::Prompt(kind) => {
                self.semantic_prompt.seen = true;
                self.cursor.semantic_content = pagepkg::SemanticContent::Prompt;
                self.cursor.semantic_content_clear_eol = false;
                let sp = match kind {
                    osc::semantic_prompt::PromptKind::Initial
                    | osc::semantic_prompt::PromptKind::Right => pagepkg::SemanticPrompt::Prompt,
                    osc::semantic_prompt::PromptKind::Continuation
                    | osc::semantic_prompt::PromptKind::Secondary => {
                        pagepkg::SemanticPrompt::PromptContinuation
                    }
                };
                unsafe { (*self.cursor.page_row).set_semantic_prompt(sp) };
            }
        }
    }
}
