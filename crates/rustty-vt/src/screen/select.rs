//! PORTS: ghostty src/terminal/Screen.zig — the selection cluster: `select`,
//! `clearSelection`, `selectionString*`, `selectLine`, `selectAll`,
//! `selectWordBetween`, `selectWord`, `selectOutput`, `lineIterator`,
//! `promptClickMove`/`promptClickLine`.
//!
//! PORT NOTE: named `select` rather than `selection` because `screen.rs`
//! already imports `crate::selection` under that name.

use super::*;

impl Screen {
    /// Set the selection to the given selection. If this is a tracked selection
    /// then the screen will take ownership of the selection. If this is untracked
    /// then the screen will convert it to tracked internally. This will automatically
    /// untrack the prior selection (if any).
    ///
    /// Set the selection to null to clear any previous selection.
    ///
    /// This is always recommended over setting `selection` directly. Beyond
    /// managing memory for you, it also performs safety checks that the selection
    /// is always tracked.
    pub fn select(&mut self, sel_: Option<Selection>) -> Result<(), OutOfMemory> {
        let sel = match sel_ {
            Some(sel) => sel,
            None => {
                self.clear_selection();
                return Ok(());
            }
        };

        // If this selection is untracked then we track it.
        // errdefer if (!sel.tracked()) tracked_sel.deinit(self);
        let tracked_sel = if sel.tracked() { sel } else { sel.track(self)? };

        // Untrack prior selection pins that aren't also owned by the
        // replacement. A caller may pass our current tracked selection back to
        // us by value, so releasing both old pins unconditionally would leave
        // the replacement pointing at freed pool entries.
        if let Some(old) = self.selection.take() {
            // const new_bounds = tracked_sel.bounds.tracked;
            // PORT NOTE: `track` always yields `Bounds::Tracked`, so the
            // reference's unchecked `.tracked` field access is faithful; the
            // untracked arm is unreachable.
            let (new_start, new_end): (*mut Pin, *mut Pin) = match tracked_sel.bounds {
                selection::Bounds::Tracked { start, end } => (start, end),
                selection::Bounds::Untracked { .. } => unreachable!(),
            };
            match old.bounds {
                selection::Bounds::Untracked { .. } => old.deinit(self),
                selection::Bounds::Tracked { start, end } => {
                    if !core::ptr::eq(start, new_start) && !core::ptr::eq(start, new_end) {
                        self.pages.untrack_pin(start);
                    }
                    if !core::ptr::eq(end, new_start) && !core::ptr::eq(end, new_end) {
                        self.pages.untrack_pin(end);
                    }
                }
            }
        }
        self.selection = Some(tracked_sel);
        self.dirty.selection = true;
        Ok(())
    }

    /// Same as select(null) but can't fail.
    pub fn clear_selection(&mut self) {
        // if (self.selection) |*sel| {
        //     sel.deinit(self);
        //     self.dirty.selection = true;
        // }
        // self.selection = null;
        // PORT NOTE: `Selection::deinit` lands in the Selection-methods sub-batch
        // (vt-11.1). The foundation tests never set a selection, so this path is
        // exercised only with `self.selection == None`. The faithful body is
        // ported here; `deinit` is reached only once Selection is filled.
        if let Some(sel) = self.selection.take() {
            sel.deinit(self);
            self.dirty.selection = true;
        }
        self.selection = None;
    }

    /// Returns the raw text associated with a selection. This will unwrap
    /// soft-wrapped edges. The returned slice is owned by the caller and allocated
    /// using alloc, not the allocator associated with the screen (unless they match).
    ///
    /// For more flexibility, use a ScreenFormatter directly.
    // PORT NOTE: the reference returns `[:0]const u8` (NUL-terminated); the
    // faithful map is an UNSENTINELED `Vec<u8>` (never a lossy `String`) whose
    // `len()` equals the Zig slice's sentinel-EXCLUSIVE `len`. No NUL byte is
    // stored — see the sink note below.
    // pub fn selectionString(
    //     self: *Screen,
    //     alloc: Allocator,
    //     opts: SelectionString,
    // ) Allocator.Error![:0]const u8 {
    //     return self.selectionStringImpl(alloc, opts, null);
    // }
    pub fn selection_string(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
    ) -> Result<Vec<u8>, OutOfMemory> {
        self.selection_string_impl(alloc, opts, None)
    }

    /// Returns a StringMap associated with a selection. The map contains the
    /// selection's raw text and a mapping from each byte to its screen location.
    ///
    /// The returned map is owned by the caller.
    // pub fn selectionStringMap(
    //     self: *Screen,
    //     alloc: Allocator,
    //     opts: SelectionString,
    // ) Allocator.Error!StringMap {
    pub fn selection_string_map(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
    ) -> Result<StringMap, OutOfMemory> {
        // var pins: PinMap.Map = .empty;
        // errdefer pins.deinit(alloc);
        // PORT NOTE: `pins` is an owning value; the early `?` return below drops
        // it, which IS the faithful errdefer. On success ownership of the pin
        // map moves into the returned `StringMap`.
        let mut pins: formatter::PinMapMap = formatter::PinMapMap::EMPTY;
        Ok(StringMap {
            string: self.selection_string_impl(alloc, opts, Some(&mut pins))?,
            map: pins,
        })
    }

    fn selection_string_impl(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
        pins: Option<&mut formatter::PinMapMap>,
    ) -> Result<Vec<u8>, OutOfMemory> {
        // We'll use this as our buffer to build our string.
        // PORT NOTE: the reference builds a `std.Io.Writer.Allocating` over
        // `alloc` and returns `toOwnedSliceSentinel(0)`. A `Vec<u8>` is the
        // faithful sink (the same convention as `dumpStringAlloc`): its length
        // is the sentinel-EXCLUSIVE length `toOwnedSliceSentinel(0)` reports.
        // The port stores NO trailing NUL. That is safe because no consumer on
        // either side ever reads the sentinel: `StringMap`'s regex search
        // passes explicit start/end byte bounds into oniguruma, and
        // `rustty-core`'s `link_at_pin` goes through that same bounded API. See
        // the `StringMap.string` PORT NOTE (`string_map.rs`), which also explains
        // why appending a 0 byte here would BREAK parity rather than restore it.
        let mut aw: Vec<u8> = Vec::new();

        // Create a formatter and use that to emit our text.
        let mut formatter: ScreenFormatter = ScreenFormatter::init(
            self as *const Screen,
            formatter::Options {
                emit: formatter::Format::Plain,
                unwrap: true,
                trim: opts.trim,
                // PORT NOTE: the reference `.{ .emit = .plain, .unwrap = true,
                // .trim = opts.trim }` leaves the remaining `Options` fields at
                // their struct-literal defaults (`codepoint_map = .{}`,
                // background/foreground/palette = null). We mirror those here.
                codepoint_map: Some(Vec::new()),
                background: None,
                foreground: None,
                palette: None,
            },
        );
        formatter.content = Content::Selection(Some(opts.sel));

        // if (pins) |map| formatter.pin_map = .{
        //     .alloc = alloc,
        //     .map = map,
        // };
        if let Some(map) = pins {
            formatter.pin_map = Some(formatter::PinMap { alloc, map });
        }

        // Emit. Since this is an allocating writer, a failed write
        // just becomes an OOM.
        formatter.format(&mut aw).map_err(|_| OutOfMemory)?;
        // return try aw.toOwnedSliceSentinel(0);
        Ok(aw)
    }

    /// Select the line under the given point. This will select across soft-wrapped
    /// lines and will omit the leading and trailing whitespace. If the point is
    /// over whitespace but the line has non-whitespace characters elsewhere, the
    /// line will be selected.
    pub fn select_line(&self, opts: SelectLine<'_>) -> Option<Selection> {
        // _ = self;

        // Get the current point semantic prompt state since that determines
        // boundary conditions too. This makes it so that line selection can
        // only happen within the same prompt state. For example, if you triple
        // click output, but the shell uses spaces to soft-wrap to the prompt
        // then the selection will stop prior to the prompt. See issue #1329.
        let semantic_prompt_state: Option<pagepkg::SemanticContent> = 'state: {
            if !opts.semantic_prompt_boundary {
                break 'state None;
            }
            let rac = opts.pin.row_and_cell();
            break 'state Some(unsafe { (*rac.cell).semantic_content() });
        };

        // The real start of the row is the first row in the soft-wrap.
        let start_pin: Pin = 'start_pin: {
            let mut it = opts
                .pin
                .row_iterator(pagelist::iter::Direction::LeftUp, None);
            let mut it_prev: Pin = it.next().unwrap(); // skip self

            // First, check the current row for semantic boundaries before the clicked position.
            if let Some(v) = semantic_prompt_state {
                let row = it_prev.row_and_cell().row;
                let cells = unsafe { (*crate::pagelist::Node::page(it_prev.node)).get_cells(row) };
                let cells = unsafe { &*cells };
                // Scan backwards from clicked position to find where our content starts
                for i in 0..(opts.pin.x as usize + 1) {
                    let x_rev = opts.pin.x as usize - i;
                    if cells[x_rev].semantic_content() != v {
                        let mut copy = it_prev;
                        copy.x = (x_rev + 1) as size::CellCountInt;
                        break 'start_pin copy;
                    }
                }

                // No boundary found before clicked position on current row.
                // If row doesn't wrap from above, start is at column 0.
                // Otherwise, continue checking previous rows.
            }

            // PORT NOTE: the reference `while (it.next()) |p| { ... } else { ... }`
            // runs the `else` block only if the loop finishes without `break`.
            // We mirror that with a labeled loop and a fall-through after it.
            loop {
                let p = match it.next() {
                    Some(p) => p,
                    None => {
                        let mut copy = it_prev;
                        copy.x = 0;
                        break 'start_pin copy;
                    }
                };
                let row = p.row_and_cell().row;

                if !unsafe { (*row).wrap() } {
                    let mut copy = it_prev;
                    copy.x = 0;
                    break 'start_pin copy;
                }

                if let Some(v) = semantic_prompt_state {
                    // We need to check every cell in this row in reverse
                    // order since we're going up and back.
                    let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(row) };
                    let cells = unsafe { &*cells };
                    for x in 0..cells.len() {
                        let x_rev = cells.len() - 1 - x;
                        let cell = cells[x_rev];
                        if cell.semantic_content() != v {
                            break 'start_pin it_prev;
                        }
                        it_prev = p;
                        it_prev.x = x_rev as size::CellCountInt;
                    }

                    continue;
                }

                it_prev = p;
            }
        };

        // The real end of the row is the final row in the soft-wrap.
        let end_pin: Pin = 'end_pin: {
            let mut it = opts
                .pin
                .row_iterator(pagelist::iter::Direction::RightDown, None);
            while let Some(p) = it.next() {
                let row = p.row_and_cell().row;

                if let Some(v) = semantic_prompt_state {
                    // We need to check every cell in this row
                    let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(row) };
                    let cells = unsafe { &*cells };

                    // If this is our pin row we can start from our x because
                    // the start_pin logic already found the real start.
                    let start_offset = if core::ptr::eq(p.node, opts.pin.node) && p.y == opts.pin.y
                    {
                        opts.pin.x as usize
                    } else {
                        0
                    };

                    // Handle the zero case specially because if the first
                    // col doesn't match then we end at the end of the prior
                    // row. But if this is the first row, we can't go back,
                    // so we scan forward to find where our content ends.
                    if start_offset == 0 && cells[0].semantic_content() != v {
                        let mut prev = p.up(1).unwrap();
                        // prev.x = prev.node.cols() - 1;
                        prev.x = unsafe { crate::pagelist::Node::cols(prev.node) } - 1;
                        break 'end_pin prev;
                    }

                    // For every other case, we end at the prior cell.
                    for (x, cell) in cells.iter().enumerate().skip(start_offset) {
                        if cell.semantic_content() != v {
                            let mut copy = p;
                            copy.x = (x - 1) as size::CellCountInt;
                            break 'end_pin copy;
                        }
                    }
                }

                if !unsafe { (*row).wrap() } {
                    let mut copy = p;
                    copy.x = unsafe { crate::pagelist::Node::cols(p.node) } - 1;
                    break 'end_pin copy;
                }
            }

            return None;
        };

        // Go forward from the start to find the first non-whitespace character.
        let start: Pin = 'start: {
            let whitespace = match opts.whitespace {
                Some(w) => w,
                None => break 'start start_pin,
            };
            let mut it =
                start_pin.cell_iterator(pagelist::iter::Direction::RightDown, Some(end_pin));
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'start p;
            }

            return None;
        };

        // Go backward from the end to find the first non-whitespace character.
        let end: Pin = 'end: {
            let whitespace = match opts.whitespace {
                Some(w) => w,
                None => break 'end end_pin,
            };
            let mut it = end_pin.cell_iterator(pagelist::iter::Direction::LeftUp, Some(start_pin));
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'end p;
            }

            return None;
        };

        Some(Selection::init(start, end, false))
    }

    /// Return the selection for all contents on the screen. Surrounding
    /// whitespace is omitted. If there is no selection, this returns null.
    pub fn select_all(&mut self) -> Option<Selection> {
        let whitespace: &[u32] = &[0, b' ' as u32, b'\t' as u32];

        let start: Pin = 'start: {
            let mut it = self.pages.cell_iterator(
                pagelist::iter::Direction::RightDown,
                point::Point::Screen(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'start p;
            }

            return None;
        };

        let end: Pin = 'end: {
            let mut it = self.pages.cell_iterator(
                pagelist::iter::Direction::LeftUp,
                point::Point::Screen(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'end p;
            }

            return None;
        };

        Some(Selection::init(start, end, false))
    }

    /// Select the nearest word to start point that is between start_pt and
    /// end_pt (inclusive). Because it selects "nearest" to start point, start
    /// point can be before or after end point.
    ///
    /// The boundary_codepoints parameter should be a slice of u21 codepoints that
    /// mark word boundaries, passed through to selectWord.
    ///
    /// TODO: test this
    pub fn select_word_between(
        &mut self,
        start: Pin,
        end: Pin,
        boundary_codepoints: &[u32],
    ) -> Option<Selection> {
        let dir: pagelist::iter::Direction = if start.before(end) {
            pagelist::iter::Direction::RightDown
        } else {
            pagelist::iter::Direction::LeftUp
        };
        let mut it = start.cell_iterator(dir, Some(end));
        while let Some(pin) = it.next() {
            // Boundary conditions
            match dir {
                pagelist::iter::Direction::RightDown => {
                    if end.before(pin) {
                        return None;
                    }
                }
                pagelist::iter::Direction::LeftUp => {
                    if pin.before(end) {
                        return None;
                    }
                }
            }

            // If we found a word, then return it
            if let Some(sel) = self.select_word(pin, boundary_codepoints) {
                return Some(sel);
            }
        }

        None
    }

    /// Select the word under the given point. A word is any consecutive series
    /// of characters that are exclusively whitespace or exclusively non-whitespace.
    /// A selection can span multiple physical lines if they are soft-wrapped.
    ///
    /// This will return null if a selection is impossible. The only scenario
    /// this happens is if the point pt is outside of the written screen space.
    ///
    /// The boundary_codepoints parameter should be a slice of u21 codepoints that
    /// mark word boundaries. This is expected to be pre-parsed from the config.
    pub fn select_word(&self, pin: Pin, boundary_codepoints: &[u32]) -> Option<Selection> {
        // _ = self;

        // If our cell is empty we can't select a word, because we can't select
        // areas where the screen is not yet written.
        let start_codepoint = select_word_codepoint(pin)?;

        // Determine if we are a boundary or not to determine what our boundary is.
        let expect_boundary = boundary_codepoints.contains(&start_codepoint);

        // Go forwards to find our end boundary
        let end: Pin = 'end: {
            let mut it = pin.cell_iterator(pagelist::iter::Direction::RightDown, None);
            let mut prev = it.next().unwrap(); // Consume one, our start
            while let Some(p) = it.next() {
                // Only cross a row boundary if the previous row wraps.
                if prev.x == unsafe { crate::pagelist::Node::cols(prev.node) } - 1
                    && !unsafe { (*prev.row_and_cell().row).wrap() }
                {
                    break 'end prev;
                }

                // If we reached an empty cell its always a boundary
                let Some(codepoint) = select_word_codepoint(p) else {
                    break 'end prev;
                };

                // If we do not match our expected set, we hit a boundary
                let this_boundary = boundary_codepoints.contains(&codepoint);
                if this_boundary != expect_boundary {
                    break 'end prev;
                }

                prev = p;
            }

            break 'end prev;
        };

        // Go backwards to find our start boundary
        let start: Pin = 'start: {
            let mut it = pin.cell_iterator(pagelist::iter::Direction::LeftUp, None);
            let mut prev = it.next().unwrap(); // Consume one, our start
            while let Some(p) = it.next() {
                let rac = p.row_and_cell();

                // If we are going to the next row and it isn't wrapped, we
                // return the previous.
                if p.x == unsafe { crate::pagelist::Node::cols(p.node) } - 1
                    && !unsafe { (*rac.row).wrap() }
                {
                    break 'start prev;
                }

                // If we reached an empty cell its always a boundary
                let Some(codepoint) = select_word_codepoint(p) else {
                    break 'start prev;
                };

                // If we do not match our expected set, we hit a boundary
                let this_boundary = boundary_codepoints.contains(&codepoint);
                if this_boundary != expect_boundary {
                    break 'start prev;
                }

                prev = p;
            }

            break 'start prev;
        };

        Some(Selection::init(start, end, false))
    }

    /// Select the command output under the given point. The limits of the output
    /// are determined by semantic prompt information provided by shell integration.
    /// A selection can span multiple physical lines if they are soft-wrapped.
    ///
    /// This will return null if a selection is impossible:
    ///  - the point pt is outside of the written screen space.
    ///  - the point pt is on a prompt / input line.
    pub fn select_output(&mut self, pin: Pin) -> Option<Selection> {
        // If our pin right now is not on output, then we return nothing.
        if unsafe { (*pin.row_and_cell().cell).semantic_content() }
            != pagepkg::SemanticContent::Output
        {
            return None;
        }

        // Get the post prior prompt from this pin. This is the prompt whose
        // output we'll be capturing.
        let prompt_pin: Pin = 'prompt: {
            // If we have a prompt above this point (including this point),
            // then thats the prompt we want to capture output from.
            let mut it = pin.prompt_iterator(pagelist::iter::Direction::LeftUp, None);
            if let Some(p) = it.next() {
                break 'prompt p;
            }

            // If we don't have a prompt, then we assume that we're
            // capturing all the output up to the next prompt.
            let mut it = pin.prompt_iterator(pagelist::iter::Direction::RightDown, None);
            let next = it.next()?;

            // We'll capture from the start of the screen to just above
            // the prompt and will trim the trailing whitespace.
            let start_pin = self.pages.get_top_left(point::Tag::Screen);
            let mut end_pin = next.up(1)?;
            end_pin.x = unsafe { crate::pagelist::Node::cols(end_pin.node) } - 1;
            let mut cell_it =
                end_pin.cell_iterator(pagelist::iter::Direction::LeftUp, Some(start_pin));
            while let Some(p) = cell_it.next() {
                let cell = p.row_and_cell().cell;
                end_pin = p;
                if unsafe { (*cell).has_text() } {
                    break;
                }
            }

            return Some(Selection::init(start_pin, end_pin, false));
        };

        // Grab our content
        let mut hl = self
            .pages
            .highlight_semantic_content(prompt_pin, pagepkg::SemanticContent::Output)?;

        // Trim our trailing whitespace
        let mut cell_it = hl
            .end
            .cell_iterator(pagelist::iter::Direction::LeftUp, Some(hl.start));
        while let Some(p) = cell_it.next() {
            let cell = p.row_and_cell().cell;
            hl.end = p;
            if unsafe { (*cell).has_text() } {
                break;
            }
        }

        Some(Selection::init(hl.start, hl.end, false))
    }

    /// Returns an iterator to move through the soft-wrapped lines starting
    /// from pin.
    pub fn line_iterator(&self, start: Pin) -> LineIterator<'_> {
        LineIterator {
            screen: self,
            current: Some(start),
        }
    }

    /// Determine the inputs necessary to move the cursor to the given
    /// click location within a prompt input area.
    ///
    /// If the cursor isn't currently at a prompt input location, this
    /// returns no movement.
    ///
    /// This feature depends on well-behaved OSC133 shell integration. Specifically,
    /// this only moves over designated input areas (OSC 133 B). It is assumed
    /// that the shell will only move the cursor to input cells, so prompt cells
    /// and other blank cells are ignored as part of the movement calculation.
    pub fn prompt_click_move(&mut self, click_pin: Pin) -> PromptClickMove {
        // If we're not at an input cell with our cursor, no movement will
        // ever be possible.
        if self.cursor.semantic_content != pagepkg::SemanticContent::Input
            && unsafe { (*self.cursor.page_cell).semantic_content() }
                != pagepkg::SemanticContent::Input
        {
            return PromptClickMove::ZERO;
        }

        match &self.semantic_prompt.click {
            // None doesn't support movement and click_events must use a
            // different mechanism (SGR mouse events) that callers must handle.
            SemanticClick::None | SemanticClick::ClickEvents(_) => PromptClickMove::ZERO,
            SemanticClick::Cl(cl) => match cl {
                // All of these currently use dumb line-based navigation.
                // But eventually we'll support more.
                osc::semantic_prompt::Click::Line
                | osc::semantic_prompt::Click::Multiple
                | osc::semantic_prompt::Click::ConservativeVertical
                | osc::semantic_prompt::Click::SmartVertical => self.prompt_click_line(click_pin),
            },
        }
    }

    /// Determine the inputs required to move from the cursor to the given
    /// click location. If the cursor isn't currently at a prompt input
    /// location, this will return zero.
    ///
    /// This currently only supports moving a single line.
    fn prompt_click_line(&mut self, click_pin: Pin) -> PromptClickMove {
        // If our click pin is our cursor pin, no movement is needed.
        // Do this early so we can assume later that they are different.
        let cursor_pin = unsafe { *self.cursor.page_pin };
        if cursor_pin.eql(click_pin) {
            return PromptClickMove::ZERO;
        }

        // If our cursor is before our click, we're only emitting right inputs.
        if cursor_pin.before(click_pin) {
            let mut count: usize = 0;

            // We go row-by-row because soft-wrapped rows are still a single
            // line to a shell, so we can't just look at our page row.
            let mut row_it =
                cursor_pin.row_iterator(pagelist::iter::Direction::RightDown, Some(click_pin));
            'row_it: while let Some(row_pin) = row_it.next() {
                let rac = row_pin.row_and_cell();
                let cells: &[pagepkg::Cell] =
                    unsafe { &*(*crate::pagelist::Node::page(row_pin.node)).get_cells(rac.row) };

                // Determine if this row is our cursor.
                let is_cursor_row =
                    std::ptr::eq(row_pin.node, cursor_pin.node) && row_pin.y == cursor_pin.y;

                // If this is not the cursor row, verify it's still part of the
                // continuation of our starting prompt.
                if !is_cursor_row
                    && unsafe { (*rac.row).semantic_prompt() }
                        != pagepkg::SemanticPrompt::PromptContinuation
                {
                    break;
                }

                // Determine where our input starts.
                let start_x: usize = 'start_x: {
                    // If this is our cursor row then we start after the cursor.
                    if is_cursor_row {
                        break 'start_x cursor_pin.x as usize + 1;
                    }

                    // Otherwise, we start at the first input cell, because
                    // we expect the shell to properly translate arrows across
                    // lines to the start of the input. Some shells indent
                    // where input starts on subsequent lines so we must do
                    // this.
                    for (x, cell) in cells.iter().enumerate() {
                        if cell.semantic_content() == pagepkg::SemanticContent::Input {
                            break 'start_x x;
                        }
                    }

                    // We never found an input cell, so we need to move to the
                    // next row.
                    break 'start_x cells.len();
                };

                // Iterate over the input cells and assume arrow keys only
                // jump to input cells.
                for (x, cell) in cells.iter().enumerate().skip(start_x) {
                    // Ignore non-input cells, but allow breaks. We assume
                    // the shell will translate arrow keys to only input
                    // areas.
                    if cell.semantic_content() != pagepkg::SemanticContent::Input {
                        continue;
                    }

                    // Increment our input count
                    count += 1;

                    // If this is our target, we're done.
                    if std::ptr::eq(row_pin.node, click_pin.node)
                        && row_pin.y == click_pin.y
                        && x == click_pin.x as usize
                    {
                        break 'row_it;
                    }
                }

                // If this row isn't soft-wrapped, we need to break out
                // because line based moving only handles single lines.
                // We're done!
                if !unsafe { (*rac.row).wrap() } {
                    // If we never found our pin, that means we clicked further
                    // right/beyond it. If we're already on a non-empty input cell
                    // then we add one so we can move to the newest, empty cell
                    // at the end, matching typical editor behavior.
                    if unsafe { (*self.cursor.page_cell).semantic_content() }
                        == pagepkg::SemanticContent::Input
                    {
                        count += 1;
                    }

                    break;
                }
            }

            return PromptClickMove {
                left: 0,
                right: count,
            };
        }

        // Otherwise, cursor is after click, so we're emitting left inputs.
        let mut count: usize = 0;

        // We go row-by-row because soft-wrapped rows are still a single
        // line to a shell, so we can't just look at our page row.
        let mut row_it =
            cursor_pin.row_iterator(pagelist::iter::Direction::LeftUp, Some(click_pin));
        'row_it: while let Some(row_pin) = row_it.next() {
            let rac = row_pin.row_and_cell();
            let cells: &[pagepkg::Cell] =
                unsafe { &*(*crate::pagelist::Node::page(row_pin.node)).get_cells(rac.row) };

            // Determine the length of the cells we look at in this row.
            let end_len: usize = 'end_len: {
                // If this is our cursor row then we end before the cursor.
                if std::ptr::eq(row_pin.node, cursor_pin.node) && row_pin.y == cursor_pin.y {
                    break 'end_len cursor_pin.x as usize;
                }

                // Otherwise, we end at the last cell in the row.
                break 'end_len cells.len();
            };

            // Iterate backwards over the input cells.
            for rev_x in 0..end_len {
                let x: usize = end_len - 1 - rev_x;
                let cell = &cells[x];

                // Ignore non-input cells.
                if cell.semantic_content() != pagepkg::SemanticContent::Input {
                    continue;
                }

                // Increment our input count
                count += 1;

                // If this is our target, we're done.
                if std::ptr::eq(row_pin.node, click_pin.node)
                    && row_pin.y == click_pin.y
                    && x == click_pin.x as usize
                {
                    break 'row_it;
                }
            }

            // If this row is not a wrap continuation, then break out
            if !unsafe { (*rac.row).wrap_continuation() } {
                break;
            }
        }

        PromptClickMove {
            left: count,
            right: 0,
        }
    }
}
