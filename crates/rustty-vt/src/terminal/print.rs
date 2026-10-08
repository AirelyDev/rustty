//! PORTS: ghostty src/terminal/Terminal.zig — the `printString` / `printRepeat` / `printSlice` (fast path, fill, SIMD helpers) / `print` / `printCell` / `printWrap` cluster.

use super::*;

/// The width class of a printSlice batch. Each batch contains only
/// codepoints of a single width class because they fill cells
/// differently: wide codepoints occupy a (wide, spacer_tail) cell
/// pair while narrow codepoints occupy a single cell.
//
// PORT NOTE: the reference is `const PrintSliceWidth = enum(u1) { narrow, wide }`
// used as a `comptime` parameter of `printSliceFill`/`printSliceEligible`.
// Stable Rust cannot use a user enum as a const-generic parameter, so the two
// tag VALUES of the `enum(u1)` are the const-generic argument directly and the
// enum's one method (`cellsPerCp`) is a free `const fn` over that tag.
const PRINT_SLICE_NARROW: u8 = 0;
const PRINT_SLICE_WIDE: u8 = 1;

/// The number of cells each codepoint of this width class occupies.
// fn cellsPerCp(comptime self: PrintSliceWidth) usize
const fn print_slice_cells_per_cp(width: u8) -> usize {
    match width {
        PRINT_SLICE_WIDE => 2,
        _ => 1,
    }
}

/// `simd.lanes(u32)` for the codepoint scans, and `simd.lanes(u64)` for the
/// cell scans/stores (Cell's backing integer). Hoisted to module consts so the
/// loop bodies below read like the reference's `if (simd.lanes(T)) |lanes|`.
const CP_LANES: Option<usize> = rustty_simd::lanes(core::mem::size_of::<u32>());
const CELL_LANES: Option<usize> = rustty_simd::lanes(core::mem::size_of::<u64>());

/// Whether a codepoint above 0xFF is eligible for the batched print
/// fast path with the given width class.
#[inline]
fn print_slice_eligible<const WIDTH: u8>(cp: u32) -> bool {
    assert!(cp > 0xFF);
    // if (comptime build_options.kitty_graphics) {
    //     if (cp == kitty.graphics.unicode.placeholder) return false;
    // }
    if crate::options::KITTY_GRAPHICS && cp == kitty::graphics_unicode::PLACEHOLDER {
        return false;
    }

    unicode::table().get(cp).width
        == match WIDTH {
            PRINT_SLICE_WIDE => 2,
            _ => 1,
        }
}

/// The expected value of a simple cell (per the check mask built from
/// fieldMask in printSliceFill) that already has the given style
/// (so no ref-counting is needed).
#[inline]
fn print_slice_check_expected(style_id: style::Id) -> u64 {
    // var e: Cell = @bitCast(@as(u64, 0));
    // e.style_id = style_id;
    // return @bitCast(e);
    let mut e = pagepkg::Cell::from_bits(0);
    e.set_style_id(style_id);
    e.to_bits()
}

/// Store narrow cells using a template with zeroed codepoint bits.
///
/// Store a run of narrow codepoint cells built from a bit template:
/// for each `idx` in `[from, to)`, `cells[idx]` is assigned the bits
/// `template_bits | (cps[idx] << cp_shift)`.
///
/// The template is a complete Cell (content tag, style, wide state,
/// etc. already baked in by the caller) whose codepoint content bits
/// are zero. Since Cell is a packed struct(u64), OR-ing a codepoint
/// into the content field's bit position yields a finished cell as a
/// single integer, keeping the loop pure data movement: no per-cell
/// field assignments and no branches.
///
/// If a charset table is provided, all input codepoints must fit in a byte.
///
/// The unmapped loop is manually vectorized: Zig 0.16 (LLVM 21) no longer
/// auto-vectorizes it as Zig 0.15 (LLVM 20) did.
//
// PORT NOTE: `cells`/`cps` are RAW pointers, not `&mut [Cell]`/`&[u32]`, per
// the `rust-mut-args-strongly-protected` rule: the caller keeps its own raw
// alias to the same row for the scan loops that bracket this call, and a
// `&mut` argument here would be a Stacked-Borrows-protected retag that
// invalidates it.
///
/// # Safety
///
/// `cells[from..to]` and `cps[from..to]` must both be in bounds. If
/// `charset_table` is `Some`, every codepoint in `cps[from..to]` must be
/// `<= 0xFF` (an index into the 256-entry table).
#[inline]
unsafe fn print_slice_store_run(
    cells: *mut pagepkg::Cell,
    cps: *const u32,
    from: usize,
    to: usize,
    template_bits: u64,
    charset_table: Option<&[u16]>,
) {
    // The bit position of the `content` field within the packed
    // Cell. A codepoint occupies the low bits of `content`, so
    // shifting a codepoint left by this amount places it exactly
    // where `.content = .{ .codepoint = .{ .data = cp } }` would.
    const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

    // Since codepoints are OR'd into the content field rather than
    // assigned, any nonzero content bits in the template would
    // corrupt the stored codepoints.
    const CONTENT_MASK: u64 = ((1u64 << pagepkg::CELL_CONTENT_BITS) - 1) << CP_SHIFT;
    debug_assert!(template_bits & CONTENT_MASK == 0);

    if let Some(table) = charset_table {
        for idx in from..to {
            let cp = table[unsafe { *cps.add(idx) } as usize] as u64;
            unsafe { *cells.add(idx) = pagepkg::Cell::from_bits(template_bits | (cp << CP_SHIFT)) };
        }
        return;
    }

    let mut idx = from;

    // Vectorized bulk of the run.
    if let Some(lanes) = CELL_LANES {
        // The template and shift amount are loop-invariant, so
        // broadcast them to every lane once up front.
        //
        // PORT NOTE: `template_bits`/`CP_SHIFT` are already scalar constants of
        // the loop in this port, so the reference's explicit `@splat` of each
        // into a `@Vector(lanes, ...)` has no separate spelling here — LLVM
        // hoists and broadcasts them from the fixed-length window below.
        while idx + lanes <= to {
            // Load `lanes` decoded codepoints...
            //
            // ...widen each u32 lane to the u64 cell size...
            //
            // ...shift each codepoint into the content field's bit
            // position and merge with the template, producing
            // `lanes` finished cells (coerced from vector to array
            // so they can be bitcast for the store below)...
            //
            // ...and store them contiguously. Cell is a packed
            // struct(u64) so an array of u64 bit patterns has
            // identical layout to an array of cells.
            //
            // Written over a fixed-length window so LLVM emits the same vector
            // stores the reference's `@Vector(lanes, u64)` does.
            for j in 0..lanes {
                let cp = unsafe { *cps.add(idx + j) } as u64;
                unsafe {
                    *cells.add(idx + j) =
                        pagepkg::Cell::from_bits(template_bits | (cp << CP_SHIFT));
                }
            }
            idx += lanes;
        }
    }

    // Scalar tail: the final `< lanes` cells of the run, or the
    // entire run on targets without SIMD. Note `idx` carries over
    // from the vector loop above. This is the same computation as
    // the vector body, one cell at a time.
    while idx < to {
        let cp = unsafe { *cps.add(idx) } as u64;
        unsafe { *cells.add(idx) = pagepkg::Cell::from_bits(template_bits | (cp << CP_SHIFT)) };
        idx += 1;
    }
}

/// Store `(wide, spacer_tail)` cell pairs for `[from, to)` (which must be
/// pair-aligned).
///
/// Manually vectorized for the same reason as
/// printSliceStoreRun: build a vector of wide cells,
/// interleave with spacer tails, and store (wide,
/// spacer) pairs several at a time.
///
/// # Safety
///
/// `cells[from..to]` and `cps[from/2..to/2]` must both be in bounds.
#[inline]
unsafe fn print_slice_store_pairs(
    cells: *mut pagepkg::Cell,
    cps: *const u32,
    from: usize,
    to: usize,
    wide_bits: u64,
    spacer_bits: u64,
) {
    const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

    let mut idx = from;
    if let Some(lanes) = CELL_LANES {
        let pair_lanes = lanes / 2;
        while idx + 2 * pair_lanes <= to {
            for j in 0..pair_lanes {
                let cp = unsafe { *cps.add(idx / 2 + j) } as u64;
                unsafe {
                    *cells.add(idx + 2 * j) =
                        pagepkg::Cell::from_bits(wide_bits | (cp << CP_SHIFT));
                    *cells.add(idx + 2 * j + 1) = pagepkg::Cell::from_bits(spacer_bits);
                }
            }
            idx += 2 * pair_lanes;
        }
    }
    while idx < to {
        let cp = unsafe { *cps.add(idx / 2) } as u64;
        unsafe {
            *cells.add(idx) = pagepkg::Cell::from_bits(wide_bits | (cp << CP_SHIFT));
            *cells.add(idx + 1) = pagepkg::Cell::from_bits(spacer_bits);
        }
        idx += 2;
    }
}

impl Terminal {
    /// Print UTF-8 encoded string to the terminal.
    pub fn print_string(&mut self, str: &[u8]) -> Result<(), PrintStringError> {
        // const view = try std.unicode.Utf8View.init(str);
        // var it = view.iterator();
        // while (it.nextCodepoint()) |cp| { ... }
        // PORT NOTE: `std.unicode.Utf8View.init` validates UTF-8 then iterates
        // codepoints; an invalid sequence is `error.InvalidUtf8`. We decode via
        // `str::from_utf8` (the same validation) and iterate `chars()`.
        let view = core::str::from_utf8(str).map_err(|_| PrintStringError::InvalidUtf8)?;
        for cp in view.chars() {
            match cp {
                '\n' => {
                    self.carriage_return();
                    self.linefeed()?;
                }

                _ => self.print(cp as u32)?,
            }
        }
        Ok(())
    }

    /// Print the previous printed character a repeated amount of times.
    pub fn print_repeat(&mut self, count_req: usize) -> Result<(), OutOfMemory> {
        // const c = self.previous_char orelse return;
        let Some(c) = self.previous_char else {
            return Ok(());
        };
        // var remaining = @max(count_req, 1);
        let mut remaining = core::cmp::max(count_req, 1);

        // Print the repeated codepoint in slices so that eligible runs
        // take the batched printSlice fast path. printSlice is semantically
        // identical to calling print per codepoint: ineligible characters
        // or terminal states (insert mode, grapheme clustering, hyperlinks,
        // etc.) fall back to the per-codepoint print() path internally.
        //
        // The buffer is filled with a runtime-bounded loop rather than
        // `= @splat(c)`: a comptime-known 4096-element splat gets fully
        // unrolled into ~33KB of consecutive stores (LLVM won't re-roll
        // or vectorize it, see quirks_memset.zig), and it would fill the
        // whole buffer even for the typical small repeat counts.
        //
        // PORT NOTE: `var buf: [4096]u32 = undefined;` — Rust has no
        // `undefined` locals; the array is zero-initialized, and only the
        // `min(remaining, buf.len)` prefix the reference fills is ever read
        // (`print_slice` only sees `buf[0..n]` with `n <= remaining`).
        let mut buf: [u32; 4096] = [0; 4096];
        // for (buf[0..@min(remaining, buf.len)]) |*cp| cp.* = c;
        for cp in buf[0..core::cmp::min(remaining, 4096)].iter_mut() {
            *cp = c;
        }
        while remaining > 0 {
            // const n = @min(remaining, buf.len);
            let n = core::cmp::min(remaining, buf.len());
            self.print_slice(&buf[0..n])?;
            remaining -= n;
        }
        Ok(())
    }

    /// Print multiple codepoints to the terminal at once. This is
    /// semantically identical to calling `print` for each codepoint in
    /// order, but is much faster because it can batch cell writes and
    /// hoist per-codepoint checks out of the hot loop.
    ///
    /// The codepoints must all be printable: it is illegal for any
    /// codepoint in this slice to be a C0 control character. Therefore,
    /// this should only be called as a result of a proper VT parser
    /// (like our own).
    ///
    /// This is optimized for the common case: ASCII, soft-wrap, etc.
    /// Sequences of codepoints that require special handling (e.g. wide characters,
    /// grapheme clustering) are handled correctly but fall back to the
    /// slower per-codepoint path. They're less common and this is optimized
    /// for the aforementioned cases.
    pub fn print_slice(&mut self, cps: &[u32]) -> Result<(), OutOfMemory> {
        // Check if we can do the fast path up front. If we can't
        // we need to go back to scalar `print`.
        let fast = 'fast: {
            // Only the main display is supported.
            if self.status_display != ansi::StatusDisplay::Main {
                break 'fast false;
            }

            // Modes that require per-codepoint handling in print().
            // Wraparound is required (its the default) so that our
            // row-fill logic below can assume soft-wrap semantics. Insert
            // mode shifts cells per print.
            if self.modes.get(modespkg::Mode::Insert) {
                break 'fast false;
            }
            if !self.modes.get(modespkg::Mode::Wraparound) {
                break 'fast false;
            }

            // Single shifts require per-codepoint charset handling.
            let screen: *mut Screen = self.screens.active;
            if unsafe { (*screen).charset.single_shift }.is_some() {
                break 'fast false;
            }

            // Hyperlinks require per-cell map bookkeeping.
            if unsafe { (*screen).cursor.hyperlink_id } != 0 {
                break 'fast false;
            }

            true
        };
        if !fast {
            // for (cps) |cp| try self.print(@intCast(cp));
            for &cp in cps {
                self.print(cp)?;
            }
            return Ok(());
        }

        let grapheme_cluster = self.modes.get(modespkg::Mode::GraphemeCluster);

        // When grapheme clustering is enabled and a left margin is set,
        // print() consults the cell left of the margin after wrapping,
        // which we can't reason about here. Restrict the fast path to
        // the [0x10, 0xFF] range in that case (those never cluster).
        let screen: *mut Screen = self.screens.active;
        let allow_unicode = match unsafe { (*screen).charset.charsets.get((*screen).charset.gl) } {
            charsets::Charset::Utf8 | charsets::Charset::Ascii => {
                !grapheme_cluster || self.scrolling_region.left == 0
            }
            // print() handles Unicode width and clustering before charset mapping.
            _ => false,
        };

        let mut i: usize = 0;
        while i < cps.len() {
            // Try the fast-path print first. This will return the number of
            // codepoints it consumed.
            let consumed = self.print_slice_fast(&cps[i..], grapheme_cluster, allow_unicode)?;
            if consumed > 0 {
                i += consumed;
                continue;
            }

            // Consuming zero bytes means that the fast path can't handle
            // the next codepoint or the terminal is in a state we can't
            // fast-path. Fall back to the slow cp-by-cp print then try
            // fast paths again.
            self.print(cps[i])?;
            i += 1;
        }
        Ok(())
    }

    /// Attempt to print a prefix of `cps` using a batched fast path that
    /// writes cells directly. Returns the number of codepoints consumed.
    /// A return value of zero means the caller must print the first
    /// codepoint via the normal `print` path.
    ///
    /// The fast path handles runs of narrow (width 1) and wide (width 2)
    /// codepoints being written to simple cells. Everything else (zero
    /// width codepoints, grapheme cluster continuations, complex cells,
    /// etc.) is rejected so `print` can handle it with full generality.
    pub(super) fn print_slice_fast(
        &mut self,
        cps: &[u32],
        grapheme_cluster: bool,
        allow_unicode: bool,
    ) -> Result<usize, OutOfMemory> {
        // const screen: *Screen = self.screens.active;
        let screen: *mut Screen = self.screens.active;

        // Codepoints in [0x10, 0xFF] are always narrow (width 1, matching
        // the c <= 0xFF fast path in print) and can never interact with
        // grapheme clustering (which requires a codepoint > 0xFF).
        //
        // Codepoints above 0xFF are batchable if their width is 1 or 2
        // (excluding zero-width characters such as combining marks, ZWJ,
        // and variation selectors) and, when grapheme clustering (mode
        // 2027) is enabled, if they are a grapheme break from the
        // previously printed codepoint (so print would never attach them
        // to the previous cell).

        // Codepoints in [0x10, 0xFF] are always narrow: print()
        // hardcodes width 1 for c <= 0xFF (no width table lookup).
        // They also can never interact with grapheme clustering,
        // which print() only performs for c > 0xFF, so they're
        // immediately eligible for the narrow fill with no further
        // checks.
        let cp0 = cps[0];
        if cp0 <= 0xFF {
            // C0 control characters (0x00-0x0F) aren't printable. The
            // stream never sends these (they're routed to execute), but
            // printSlice is a public API so defer to print() for safety.
            if cp0 < 0x10 {
                return Ok(0);
            }
            return self.print_slice_fill::<PRINT_SLICE_NARROW>(
                cps,
                grapheme_cluster,
                allow_unicode,
            );
        }

        if !allow_unicode {
            return Ok(0);
        }
        // if (comptime build_options.kitty_graphics) {
        //     // The Kitty graphics placeholder requires row bookkeeping.
        //     if (cp0 == kitty.graphics.unicode.placeholder) return 0;
        // }
        if crate::options::KITTY_GRAPHICS && cp0 == kitty::graphics_unicode::PLACEHOLDER {
            return Ok(0);
        }

        // The first codepoint requires care when grapheme clustering is
        // enabled: print() may attach it to the previous *cell* instead
        // of writing a new one. Take the first codepoint only when we can
        // determine — computing exactly what print() would — that it
        // starts a new cluster. At column zero with no pending wrap,
        // print() skips clustering entirely. Otherwise resolve the
        // previous cell the way print() does and check for a break.
        //
        // Note the pending-wrap rejection: print() may attach to the
        // pending cell *instead of wrapping*, which we can't model here.
        if grapheme_cluster && unsafe { (*screen).cursor.x } != 0 {
            'gate: {
                if unsafe { (*screen).cursor.pending_wrap } {
                    return Ok(0);
                }

                // Resolve the content cell to our left exactly like print():
                // if the immediate left cell is a wide spacer tail, the
                // content lives one further left. (A spacer tail can never
                // be at column zero — its wide half would have to be in the
                // previous row — so the second cursorCellLeft is in bounds.)
                let immediate: *mut pagepkg::Cell = unsafe { (*screen).cursor_cell_left(1) };
                let prev: *mut pagepkg::Cell = match unsafe { (*immediate).wide() } {
                    pagepkg::Wide::SpacerTail => unsafe { (*screen).cursor_cell_left(2) },
                    _ => immediate,
                };

                // An empty previous cell is necessarily a grapheme break.
                if unsafe { (*prev).codepoint() } == 0 {
                    break 'gate;
                }

                // Grapheme data on the previous cell requires the full
                // cluster state machine replay; only print() can do that.
                if unsafe { (*prev).has_grapheme() } {
                    return Ok(0);
                }

                // A simple single-codepoint previous cell: print() would run
                // exactly this break check from the default state.
                let mut state = unicode::grapheme::BreakState::Default;
                if !unicode::graphemeBreak(unsafe { (*prev).content_codepoint() }, cp0, &mut state)
                {
                    return Ok(0);
                }
            }
        } else if grapheme_cluster {
            // PORT NOTE: kept as the reference's nested `if` (not collapsed
            // into the `else if` condition) so the branch structure matches
            // line-for-line.
            #[allow(clippy::collapsible_if)]
            if unsafe { (*screen).cursor.pending_wrap } {
                return Ok(0);
            }
        }

        // The width lookup is a runtime value while printSliceFill is
        // specialized at comptime by width class, so this switch selects
        // between the two instantiations rather than passing the width
        // through as an argument.
        match unicode::table().get(cp0).width {
            1 => self.print_slice_fill::<PRINT_SLICE_NARROW>(cps, grapheme_cluster, allow_unicode),
            2 => self.print_slice_fill::<PRINT_SLICE_WIDE>(cps, grapheme_cluster, allow_unicode),
            _ => Ok(0),
        }
    }

    /// The row-filling portion of the printSlice fast path, specialized by
    /// width class. The first codepoint must already be validated by the
    /// caller (printSliceFast).
    fn print_slice_fill<const WIDTH: u8>(
        &mut self,
        cps: &[u32],
        grapheme_cluster: bool,
        allow_unicode: bool,
    ) -> Result<usize, OutOfMemory> {
        let screen: *mut Screen = self.screens.active;
        let charset_table: Option<&'static [u16]> =
            match unsafe { (*screen).charset.charsets.get((*screen).charset.gl) } {
                charsets::Charset::Utf8 | charsets::Charset::Ascii => None,
                set => Some(charsets::table(set)),
            };
        assert!(charset_table.is_none() || !allow_unicode);

        // PORT NOTE: the reference declares `simple_mask` and `cp_shift` here
        // and uses them in the row-fill body below; that body is
        // `print_slice_fill_inner` in this port (see its doc comment), so both
        // consts live there.

        // Determine the run of codepoints in the same width class that we
        // can batch. For codepoints after the first, the previous codepoint
        // in the run is always written as a fresh, single-codepoint cell,
        // so the grapheme break check against it is exact.
        let run_len: usize = 'run: {
            let mut idx: usize = 1;

            // Vectorized scan for the narrow class: codepoints in
            // [0x10, 0xFF] are always eligible with no further checks
            // and dominate real-world input, so scan for the first
            // codepoint outside that range several lanes at a time.
            // Anything else (including eligible unicode) proceeds via
            // the scalar loop below.
            if WIDTH == PRINT_SLICE_NARROW {
                if let Some(lanes) = CP_LANES {
                    // PERF(port): branchless per lane like the reference
                    // (vector range-compare -> @reduce -> @ctz); see the
                    // printable-run scan in `stream.rs` for the rationale —
                    // the early-exit form was 4.4x the reference's
                    // instruction count.
                    //
                    // The per-group test is the `@reduce(.And, in_range)`
                    // alone; the lane bitmask is built only on the group that
                    // ends the run, as the reference does (task S-4: building
                    // it on every group cost a scalar extract per lane on
                    // NEON, which has no movemask).
                    while idx + lanes <= cps.len() {
                        // const v: V = cps[idx..][0..lanes].*;
                        let window = &cps[idx..idx + lanes];
                        // const in_range = (v >= lo) & (v <= hi);
                        // if (!@reduce(.And, in_range)) {
                        //
                        // PERF(port): the `.And` reduction is spelled as an
                        // unsigned max of `v - 0x10` (every lane is in
                        // [0x10, 0xFF] iff the largest `v -% 0x10` is at most
                        // 0xEF), which LLVM lowers to one vector subtract and
                        // an across-lanes max; the boolean-AND spelling
                        // lowered to a partly scalar sequence here.
                        let mut max_off: u32 = 0;
                        for &v in window {
                            max_off = max_off.max(v.wrapping_sub(0x10));
                        }
                        if max_off > 0xFF - 0x10 {
                            // const bits = @bitCast(in_range);
                            // idx += @ctz(~bits);
                            // break;
                            let mut bad: u32 = 0;
                            for (j, &v) in window.iter().enumerate() {
                                bad |= ((!(0x10..=0xFF).contains(&v)) as u32) << j;
                            }
                            idx += bad.trailing_zeros() as usize;
                            break;
                        }
                        idx += lanes;
                    }
                }
            }

            while idx < cps.len() {
                let cp = cps[idx];
                if WIDTH == PRINT_SLICE_NARROW && (0x10..=0xFF).contains(&cp) {
                    idx += 1;
                    continue;
                }
                if cp > 0xFF && allow_unicode && print_slice_eligible::<WIDTH>(cp) {
                    if !grapheme_cluster {
                        idx += 1;
                        continue;
                    }
                    let mut state = unicode::grapheme::BreakState::Default;
                    if unicode::graphemeBreak(cps[idx - 1], cp, &mut state) {
                        idx += 1;
                        continue;
                    }
                }
                break 'run idx;
            }
            cps.len()
        };
        assert!(run_len > 0);

        // After doing any printing, wrapping, scrolling, etc. we want to
        // ensure that our screen remains in a consistent state.
        // defer screen.assertIntegrity();
        let result = self.print_slice_fill_inner::<WIDTH>(cps, run_len, charset_table);
        unsafe { (*screen).assert_integrity() };
        result
    }

    /// The body of `print_slice_fill` after the run scan; split out only so
    /// the reference's `defer screen.assertIntegrity()` (which covers every
    /// exit below it, including the error return from `printWrap`) has a Rust
    /// model. Same split as `print`/`print_inner`.
    fn print_slice_fill_inner<const WIDTH: u8>(
        &mut self,
        cps: &[u32],
        run_len: usize,
        charset_table: Option<&'static [u16]>,
    ) -> Result<usize, OutOfMemory> {
        // Our fast path can only handle "simple" cells. A simple cell is
        // a codepoint cell (no grapheme data or bg-color tag), narrow, and
        // not a hyperlink. The mask covers every field that must match
        // the expected value (see printSliceCheckExpected) exactly.
        const SIMPLE_MASK: u64 = pagepkg::cell_field_mask(&[
            pagepkg::CellField::ContentTag,
            pagepkg::CellField::StyleId,
            pagepkg::CellField::Wide,
            pagepkg::CellField::Hyperlink,
        ]);

        // The bit offset of the codepoint content within a Cell, used to
        // construct cell values from a template without field assignments.
        const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

        // The number of cells each codepoint occupies.
        // const cells_per_cp: usize = comptime width.cellsPerCp();
        let cells_per_cp: usize = print_slice_cells_per_cp(WIDTH);

        let mut printed: usize = 0;
        'outer: while printed < run_len {
            // If we're soft-wrapping, handle that first so that our cursor
            // is in the row/column that will receive the next codepoint.
            if unsafe { (*self.screens.active).cursor.pending_wrap } {
                self.print_wrap()?;
            }

            let screen: *mut Screen = self.screens.active;

            // Our right margin depends on where our cursor is now,
            // matching the logic in print().
            let right_limit: usize = if unsafe { (*screen).cursor.x } > self.scrolling_region.right
            {
                self.cols as usize
            } else {
                self.scrolling_region.right as usize + 1
            };

            // A degenerate 1-wide region can't hold a wide char; print()
            // has special handling so fall back to it.
            if WIDTH == PRINT_SLICE_WIDE && right_limit - self.scrolling_region.left as usize <= 1 {
                break;
            }

            // const cursor = &screen.cursor;
            let cursor: *mut crate::screen::Cursor = unsafe { &raw mut (*screen).cursor };
            let avail: usize = right_limit - unsafe { (*cursor).x } as usize;
            assert!(avail > 0);

            // The cursor caches live row and cell pointers into this mapping, so
            // its page cannot be compressed while this print path is active.
            // const page = cursor.page_pin.node.pageAssumeResident();
            //
            // PERF(port): `pageAssumeResident` like the reference — the
            // restore-capable `Node::page` was an out-of-line call (twice per
            // row) that also forced register spills around it.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page_assume_resident((*(*cursor).page_pin).node) };
            // const cells: [*]Cell = @ptrCast(cursor.page_cell);
            let cells: *mut pagepkg::Cell = unsafe { (*cursor).page_cell };
            let style_id = unsafe { (*cursor).style_id };
            let template: pagepkg::Cell = {
                let mut t = pagepkg::Cell::zeroed();
                t.set_content_tag(pagepkg::ContentTag::Codepoint);
                t.set_content_codepoint(0);
                t.set_style_id(style_id);
                t.set_wide(pagepkg::Wide::Narrow);
                t.set_protected(unsafe { (*cursor).protected });
                t.set_semantic_content(unsafe { (*cursor).semantic_content });
                t
            };
            let template_bits: u64 = template.to_bits();
            let check_expected: u64 = print_slice_check_expected(style_id);

            if WIDTH == PRINT_SLICE_WIDE && avail == 1 {
                // Only one cell left in the row: print() writes a
                // spacer head (or a blank narrow cell if we're inside
                // a right margin) and wraps. We require a simple cell,
                // otherwise fall back to print() for the cleanup.
                let bits: u64 = unsafe { (*cells).to_bits() };
                if (bits & SIMPLE_MASK) != check_expected {
                    break;
                }

                let mut spacer = template;
                if right_limit == self.cols as usize {
                    unsafe { (*(*cursor).page_row).set_wrap(true) };
                    spacer.set_wide(pagepkg::Wide::SpacerHead);
                }
                unsafe { (*(*cursor).page_row).set_dirty(true) };
                if style_id != style::DEFAULT_ID {
                    unsafe { (*(*cursor).page_row).set_styled(true) };
                }
                unsafe { *cells = spacer };
                self.print_wrap()?;
                continue 'outer;
            }

            // Number of codepoints and cells we're writing to this row.
            let count = core::cmp::min(avail / cells_per_cp, run_len - printed);
            assert!(count > 0);
            let cell_count = count * cells_per_cp;

            // Wide cells always come in (wide, spacer_tail) pairs.
            let spacer_bits: u64 = if WIDTH == PRINT_SLICE_WIDE {
                let mut spacer = template;
                spacer.set_wide(pagepkg::Wide::SpacerTail);
                spacer.to_bits()
            } else {
                0 // `undefined` in the reference; never read for the narrow class.
            };
            let wide_bits: u64 = if WIDTH == PRINT_SLICE_WIDE {
                let mut w = template;
                w.set_wide(pagepkg::Wide::Wide);
                w.to_bits()
            } else {
                0 // `undefined` in the reference; never read for the narrow class.
            };

            let mut k: usize = 0; // cells written
            'fill: while k < cell_count {
                // Find the run of simple cells so the store loop below is
                // branch-free (and vectorizable). This is an early-exit
                // search loop that LLVM won't auto-vectorize, and reused
                // rows typically match the whole way through, so scan
                // several cells at a time manually.
                let mut simple = k;
                {
                    if let Some(lanes) = CELL_LANES {
                        // PERF(port): the reference's `SimpleMask.eqlPrefix`
                        // idiom — a BRANCHLESS whole-group compare, paying
                        // the exact-index extraction only on a run's final
                        // group (here: falling through to the scalar tail,
                        // which walks at most lanes-1 cells). A per-lane
                        // early-exit `break` blocked vectorization.
                        while simple + lanes <= cell_count {
                            let mut all = true;
                            for j in 0..lanes {
                                let bits = unsafe { (*cells.add(simple + j)).to_bits() };
                                all &= (bits & SIMPLE_MASK) == check_expected;
                            }
                            if !all {
                                break;
                            }
                            simple += lanes;
                        }
                    }
                    while simple < cell_count {
                        let bits: u64 = unsafe { (*cells.add(simple)).to_bits() };
                        if (bits & SIMPLE_MASK) != check_expected {
                            break;
                        }
                        simple += 1;
                    }
                }

                if WIDTH == PRINT_SLICE_WIDE {
                    // We can only write whole (wide, spacer) pairs.
                    let pair_end = k + (simple - k) / 2 * 2;
                    unsafe {
                        print_slice_store_pairs(
                            cells,
                            cps.as_ptr().add(printed),
                            k,
                            pair_end,
                            wide_bits,
                            spacer_bits,
                        )
                    };
                    // If the simple run ended mid-pair we stop at the pair
                    // boundary and handle the offending cell below.
                    k = pair_end;
                    if simple != pair_end {
                        // The first cell of the next pair is simple but the
                        // second isn't; handle both via the general path.
                        //
                        // PORT NOTE: this assignment is dead in the reference
                        // too — nothing below reads `simple` and the next
                        // `fill` iteration re-derives it. Kept verbatim
                        // (rustc's `unused_assignments` allowed at the
                        // statement) rather than dropped, so the ported body
                        // matches the reference line for line.
                        #[allow(unused_assignments)]
                        {
                            simple = pair_end;
                        }
                    }
                } else {
                    unsafe {
                        print_slice_store_run(
                            cells,
                            cps.as_ptr().add(printed),
                            k,
                            simple,
                            template_bits,
                            charset_table,
                        )
                    };
                    k = simple;
                }
                if k >= cell_count {
                    break;
                }

                // Bulk path for runs of cells that differ from the
                // expected simple cell only by their style: this is the
                // common case when styled text overwrites previously
                // styled (or default-styled) rows, e.g. TUI redraws.
                // These runs are handled wholesale: one scan to find the
                // run of identical old styles, two ref-count updates,
                // and a branch-free fill.
                if WIDTH == PRINT_SLICE_NARROW {
                    'bulk: {
                        let first = unsafe { (*cells.add(k)).to_bits() } & SIMPLE_MASK;

                        // The old cell must be a plain narrow codepoint cell
                        // with no hyperlink whose only difference is the
                        // style id (see printSliceCheckExpected: every other
                        // masked field must be zero).
                        let style_shift = pagepkg::CELL_STYLE_ID_SHIFT;
                        let old_style: style::Id = (first >> style_shift) as style::Id;
                        if first != print_slice_check_expected(old_style) {
                            break 'bulk;
                        }
                        assert!(old_style != style_id); // it failed the simple check

                        // Find the run of cells with identical masked bits.
                        let mut m = k + 1;
                        {
                            if let Some(lanes) = CELL_LANES {
                                // PERF(port): branchless whole-group compare;
                                // see the simple-cell scan above.
                                while m + lanes <= cell_count {
                                    let mut all = true;
                                    for j in 0..lanes {
                                        let bits = unsafe { (*cells.add(m + j)).to_bits() };
                                        all &= (bits & SIMPLE_MASK) == first;
                                    }
                                    if !all {
                                        break;
                                    }
                                    m += lanes;
                                }
                            }
                            while m < cell_count {
                                if (unsafe { (*cells.add(m)).to_bits() } & SIMPLE_MASK) != first {
                                    break;
                                }
                                m += 1;
                            }
                        }

                        // Fix up the style ref counts for the whole run at
                        // once. Each of the old cells held a reference to
                        // old_style so the release is safe by construction.
                        let n = m - k;
                        unsafe {
                            let base = (*page).memory_ptr();
                            if old_style != style::DEFAULT_ID {
                                (*page)
                                    .styles
                                    .release_multiple(base, old_style, n as style::Id);
                            }
                            if style_id != style::DEFAULT_ID {
                                (*page).styles.use_multiple(base, style_id, n as style::Id);
                            }
                        }

                        unsafe {
                            print_slice_store_run(
                                cells,
                                cps.as_ptr().add(printed),
                                k,
                                m,
                                template_bits,
                                charset_table,
                            )
                        };
                        k = m;
                        continue 'fill;
                    }
                }

                // General path for cells that failed the masked check:
                // style-only mismatches are handled inline; anything that
                // needs cleanup (wide chars and their spacers, grapheme
                // data, hyperlinks) falls back to print().
                let general_count: usize = cells_per_cp;
                for offset in 0..general_count {
                    let cell = unsafe { &*cells.add(k + offset) };
                    if cell.wide() != pagepkg::Wide::Narrow
                        || cell.has_grapheme()
                        || cell.hyperlink()
                    {
                        break 'fill;
                    }
                }
                for offset in 0..general_count {
                    let cell = unsafe { &*cells.add(k + offset) };
                    if cell.style_id() != style_id {
                        unsafe {
                            let base = (*page).memory_ptr();
                            if cell.style_id() != style::DEFAULT_ID {
                                (*page).styles.release(base, cell.style_id());
                            }
                            if style_id != style::DEFAULT_ID {
                                (*page).styles.use_id(base, style_id);
                            }
                        }
                    }
                }
                if WIDTH == PRINT_SLICE_WIDE {
                    unsafe {
                        *cells.add(k) = pagepkg::Cell::from_bits(
                            wide_bits | ((cps[printed + k / 2] as u64) << CP_SHIFT),
                        );
                        *cells.add(k + 1) = pagepkg::Cell::from_bits(spacer_bits);
                    }
                } else {
                    let cp: u32 = match charset_table {
                        Some(table) => table[cps[printed + k] as usize] as u32,
                        None => cps[printed + k],
                    };
                    unsafe {
                        *cells.add(k) =
                            pagepkg::Cell::from_bits(template_bits | ((cp as u64) << CP_SHIFT));
                    }
                }
                k += cells_per_cp;
            }

            if k > 0 {
                assert!(k.is_multiple_of(cells_per_cp));
                unsafe { (*(*cursor).page_row).set_dirty(true) };
                if style_id != style::DEFAULT_ID {
                    unsafe { (*(*cursor).page_row).set_styled(true) };
                }
                self.previous_char = Some(cps[printed + k / cells_per_cp - 1]);
                printed += k / cells_per_cp;

                // Advance the cursor. If we filled through the right limit
                // then the cursor stays on the last cell with the pending
                // wrap flag set, matching print().
                if unsafe { (*cursor).x } as usize + k >= right_limit {
                    assert!(unsafe { (*cursor).x } as usize + k == right_limit);
                    unsafe { (*screen).cursor_right((k - 1) as size::CellCountInt) };
                    unsafe { (*cursor).pending_wrap = true };
                } else {
                    unsafe { (*screen).cursor_right(k as size::CellCountInt) };
                }
            }

            // We hit a cell that requires the slow path. The cursor is
            // exactly at that cell so return and let the caller print the
            // next codepoint via print().
            if k < cell_count {
                break;
            }
        }

        Ok(printed)
    }

    pub fn print(&mut self, c: u32) -> Result<(), OutOfMemory> {
        // log.debug("print={x} y={} x={}", .{ c, self.screens.active.cursor.y, self.screens.active.cursor.x });

        // If we're not on the main display, do nothing for now
        if self.status_display != ansi::StatusDisplay::Main {
            return Ok(());
        }

        // After doing any printing, wrapping, scrolling, etc. we want to ensure
        // that our screen remains in a consistent state.
        // defer self.screens.active.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; the reference's `defer
        // assertIntegrity()` covers every path after the status check, so we run
        // `print_inner` and assert on the way out (both Ok and Err).
        let result = self.print_inner(c);
        self.active_mut().assert_integrity();
        result
    }

    fn print_inner(&mut self, c: u32) -> Result<(), OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        // The charset this character is printed with. A single shift applies to
        // exactly one character, so it is used up here, whether the character
        // takes a cell or attaches to the previous one: xterm clears it for a
        // combining character and so do we.
        let charset: charsets::Charset = {
            let state = unsafe { &mut (*active).charset };
            let key = state.single_shift.unwrap_or(state.gl);
            state.single_shift = None;
            state.charsets.get(key)
        };

        // Our right margin depends where our cursor is now.
        let right_limit = if unsafe { (*active).cursor.x } > self.scrolling_region.right {
            self.cols
        } else {
            self.scrolling_region.right + 1
        };

        // Perform grapheme clustering if grapheme support is enabled (mode 2027).
        // This is MUCH slower than the normal path so the conditional below is
        // purposely ordered in least-likely to most-likely so we can drop out
        // as quickly as possible.
        if c > 255
            && self.modes.get(modespkg::Mode::GraphemeCluster)
            && unsafe { (*active).cursor.x } > 0
            && self.print_inner_grapheme(c, right_limit)?
        {
            return Ok(());
        }
        let active: *mut Screen = self.screens.active;

        // Determine the width of this character so we can handle
        // non-single-width characters properly. We have a fast-path for
        // byte-sized characters since they're so common. We can ignore
        // control characters because they're always filtered prior.
        let width: usize = if c <= 0xFF {
            1
        } else {
            unicode::table().get(c).width as usize
        };

        // Note: it is possible to have a width of "3" and a width of "-1" from
        // uucode.x's wcwidth. We should look into those cases and handle them
        // appropriately.
        debug_assert!(width <= 2);
        // log.debug("c={x} width={}", .{ c, width });

        // Attach zero-width characters to our cell as grapheme data.
        if width == 0 {
            return self.print_inner_zero_width(c);
        }
        // We have a printable character, save it
        self.previous_char = Some(c);

        // If we're soft-wrapping, then handle that first.
        if unsafe { (*active).cursor.pending_wrap } && self.modes.get(modespkg::Mode::Wraparound) {
            self.print_wrap()?;
        }

        let active: *mut Screen = self.screens.active;

        // If we have insert mode enabled then we need to handle that. We
        // only do insert mode if we're not at the end of the line.
        if self.modes.get(modespkg::Mode::Insert)
            && unsafe { (*active).cursor.x } + (width as size::CellCountInt) < self.cols
        {
            self.insert_blanks(width);
        }

        let active: *mut Screen = self.screens.active;

        match width {
            // Single cell is very easy: just write in the cell
            1 => {
                self.active_mut().cursor_mark_dirty();
                self.print_cell(c, pagepkg::Wide::Narrow, charset);
            }

            // Wide character requires a spacer. We print this by
            // using two cells: the first is flagged "wide" and has the
            // wide char. The second is guaranteed to be a spacer if
            // we're not at the end of the line.
            2 => {
                if self.print_inner_wide(c, right_limit, charset)? {
                    return Ok(());
                }
            }
            _ => unreachable!(),
        }

        let active: *mut Screen = self.screens.active;

        // If we're at the column limit, then we need to wrap the next time.
        // In this case, we don't move the cursor.
        if unsafe { (*active).cursor.x } == right_limit - 1 {
            unsafe { (*active).cursor.pending_wrap = true };
            return Ok(());
        }

        // Move the cursor
        self.active_mut().cursor_right(1);
        Ok(())
    }

    // PERF(port) RECONCILED (WS10 T16, `47e26df60`): upstream's `print_slice`
    // fast path does NOT subsume this outlining, and BOTH STAY. They are
    // complementary, not alternatives:
    //   - `print_slice_fast`'s eligibility check is a WHOLE-BATCH gate. Every
    //     codepoint it rejects — zero-width marks, grapheme continuations,
    //     insert mode, DEC charsets, hyperlinks, the Kitty placeholder, wide
    //     chars in a 1-column region, and every cell needing real cleanup —
    //     is handed straight back to `print` one codepoint at a time
    //     (`print_slice`'s `self.print(cps[i])` arm, and `print_slice_fill`
    //     deliberately returns with the cursor parked on the offending cell).
    //     So `print`'s own hot path is exactly as hot as before for all
    //     non-batchable input, which is the input the outlining was measured
    //     on.
    //   - The outlining is about `print`'s FRAME (6 callee-saved pairs on
    //     every printed character); `print_slice` never enters `print` at all
    //     on the batched path, so it neither benefits from nor conflicts with
    //     the reshape.
    // Deleting the outlining to "match upstream" would be a silent
    // pessimisation of the fallback path with no upstream counterpart — the
    // reference has no equivalent frame cost because Zig inlines those blocks
    // differently. Recorded here rather than in a ledger because it is
    // discharged inside this workstream.
    //
    // PERF(port): the three cold blocks of `print` are outlined into
    // per-codepoint helpers so the hot narrow-print path (the overwhelming
    // majority of printed cells) does not carry their register pressure —
    // without this the fully-inlined slow arms force a 0xc0-byte frame with
    // 6 callee-saved pairs saved/restored on EVERY printed character
    // (instruction-level profile: >50% of `print` self-time). Bodies moved
    // VERBATIM from `print_inner` (same calls, same order, same error
    // mapping); only the block-exit control flow is encoded in the return
    // value. Same reshape precedent as r3-clone (PERF_LOG round 3).

    /// The grapheme clustering path of `print` (reference Terminal.zig:342-...,
    /// the `grapheme:` labeled block). Returns Ok(true) if `c` was consumed
    /// (attached to the previous cell / handled), Ok(false) if the normal
    /// print path must continue (the reference's `break :grapheme`).
    #[cold]
    #[inline(never)]
    fn print_inner_grapheme(
        &mut self,
        c: u32,
        right_limit: size::CellCountInt,
    ) -> Result<bool, OutOfMemory> {
        let active: *mut Screen = self.screens.active;
        'grapheme: {
            // We need the previous cell to determine if we're at a grapheme
            // break or not. If we are NOT, then we are still combining the
            // same grapheme, and will be appending to prev.cell. Otherwise, we are
            // in a new cell.
            // const Prev = struct { cell: *Cell, left: size.CellCountInt };
            let left: size::CellCountInt = {
                // If we have wraparound, then we use the prev col unless
                // there's a pending wrap, in which case we use the current.
                if self.modes.get(modespkg::Mode::Wraparound) {
                    u8::from(!unsafe { (*active).cursor.pending_wrap }) as size::CellCountInt
                } else if unsafe { (*active).cursor.x } != right_limit - 1 {
                    // If we do not have wraparound, the logic is trickier. If
                    // we're not on the last column, then we just use the previous
                    // column. Otherwise, we need to check if there is text to
                    // figure out if we're attaching to the prev or current.
                    1
                } else {
                    u8::from(unsafe { (*(*active).cursor.page_cell).codepoint() } == 0)
                        as size::CellCountInt
                }
            };

            // If the previous cell is a wide spacer tail, then we actually
            // want to use the cell before that because that has the actual
            // content.
            let immediate = self.active_mut().cursor_cell_left(left);
            let (mut prev_cell, prev_left): (*mut pagepkg::Cell, size::CellCountInt) =
                match unsafe { (*immediate).wide() } {
                    pagepkg::Wide::SpacerTail => {
                        (self.active_mut().cursor_cell_left(left + 1), left + 1)
                    }
                    _ => (immediate, left),
                };

            // If our cell has no content, then this is a new cell and
            // necessarily a grapheme break.
            if unsafe { (*prev_cell).codepoint() } == 0 {
                break 'grapheme;
            }

            let active: *mut Screen = self.screens.active;

            let mut previous_codepoint: u21 = unsafe { (*prev_cell).content_codepoint() };
            let grapheme_break = {
                let mut state: unicode::grapheme::BreakState =
                    unicode::grapheme::BreakState::Default;
                if unsafe { (*prev_cell).has_grapheme() } {
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let cps = unsafe { (*page).lookup_grapheme(prev_cell) }.unwrap();
                    for &cp2 in cps {
                        // log.debug("cp1={x} cp2={x}", .{ previous_codepoint, cp2 });
                        // _ = unicode.graphemeBreak(previous_codepoint, cp2, &state);
                        //
                        // With mode 2027 disabled, zero-width codepoints are
                        // attached without applying grapheme boundary rules. If
                        // the mode is enabled later, an existing cell can
                        // therefore contain one or more breaks. Feed those breaks
                        // into the state machine so it can reset its context and
                        // determine the boundary for the new codepoint.
                        //
                        // PORT NOTE (oracle b287f6d1a, `P3-W1 T4`): this used to
                        // be `debug_assert!(!brk)`, which is exactly the
                        // assertion upstream deleted. The pre-existing DG4 note
                        // here — that the call MUTATES `state` and must run
                        // unconditionally outside the macro — was and remains
                        // correct; it is the ASSERT that was wrong, not the call.
                        // Keep the call.
                        let _brk = unicode::graphemeBreak(previous_codepoint, cp2, &mut state);
                        previous_codepoint = cp2;
                    }
                }

                // log.debug("cp1={x} cp2={x} end", .{ previous_codepoint, c });
                unicode::graphemeBreak(previous_codepoint, c, &mut state)
            };

            // If we can NOT break, this means that "c" is part of a grapheme
            // with the previous char.
            if !grapheme_break {
                // The width-decision kernel is shared with the buffered
                // `unicode::grapheme::grapheme_width` API so the streaming
                // printer and the measurement API stay in lockstep. The
                // `Ignore` result covers invalid emoji variation selectors:
                // the terminal does not store them, so we return without
                // appending (leaving break state / prev as they were).
                use unicode::grapheme::GraphemeWidthEffect;
                match unicode::grapheme::grapheme_width_effect(previous_codepoint, c) {
                    GraphemeWidthEffect::Ignore => return Ok(true),
                    GraphemeWidthEffect::Wide => 'wide: {
                        if unsafe { (*prev_cell).wide() } == pagepkg::Wide::Wide {
                            break 'wide;
                        }

                        // Move our cursor back to the previous. We'll move
                        // the cursor within this block to the proper location.
                        self.active_mut().cursor_left(prev_left);

                        let active: *mut Screen = self.screens.active;

                        // If we don't have space for the wide char, we need to
                        // insert spacers and wrap. We need special handling if the
                        // previous cell has grapheme data.
                        if unsafe { (*active).cursor.x } == right_limit - 1 {
                            if !self.modes.get(modespkg::Mode::Wraparound) {
                                return Ok(true);
                            }

                            // This path can write a spacer_head before printWrap
                            // which can trigger integrity violations so mark
                            // the wrap first to keep the intermediary state valid
                            // if we're wrapping.
                            let row_wrap = right_limit == self.cols;
                            if row_wrap {
                                unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                            }

                            let prev_cp = unsafe { (*prev_cell).content_codepoint() };
                            if unsafe { (*prev_cell).has_grapheme() } {
                                // This is like printCell but without clearing the
                                // grapheme data from the cell, so we can move it
                                // later.
                                unsafe {
                                    (*prev_cell).set_wide(if row_wrap {
                                        pagepkg::Wide::SpacerHead
                                    } else {
                                        pagepkg::Wide::Narrow
                                    })
                                };
                                unsafe { (*prev_cell).set_content_codepoint(0) };

                                self.print_wrap()?;
                                self.write_cell(prev_cp, pagepkg::Wide::Wide);

                                let active: *mut Screen = self.screens.active;
                                let new_pin = unsafe { *(*active).cursor.page_pin };
                                let new_rac = new_pin.row_and_cell();

                                'transfer_graphemes: {
                                    let mut old_pin = match new_pin.up(1) {
                                        Some(p) => p,
                                        None => break 'transfer_graphemes,
                                    };
                                    old_pin.x = right_limit - 1;
                                    let old_rac = old_pin.row_and_cell();

                                    if std::ptr::eq(new_pin.node, old_pin.node) {
                                        let page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(new_pin.node) };
                                        unsafe {
                                            (*page).move_grapheme(old_rac.cell, new_rac.cell)
                                        };
                                        unsafe {
                                            (*old_rac.cell)
                                                .set_content_tag(pagepkg::ContentTag::Codepoint)
                                        };
                                        unsafe {
                                            (*new_rac.cell).set_content_tag(
                                                pagepkg::ContentTag::CodepointGrapheme,
                                            )
                                        };
                                        unsafe { (*new_rac.row).set_grapheme(true) };
                                    } else {
                                        let old_page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(old_pin.node) };
                                        // PORT NOTE: the reference iterates the
                                        // old page's grapheme slice in place. Rust
                                        // cannot hold that borrow across
                                        // `append_grapheme` (which takes
                                        // `&mut Screen`), so we keep it as a raw
                                        // ptr+len (the port's raw-pointer doctrine)
                                        // rather than copying it into a `Vec`: a
                                        // copy would add a Rust-only allocation
                                        // whose failure ABORTS instead of
                                        // propagating as `OutOfMemory`. This is
                                        // sound for exactly the reason the
                                        // reference's in-place iteration is: this
                                        // arm requires `old_pin.node !=
                                        // new_pin.node`, and `Screen::append_grapheme`
                                        // only mutates/reallocates the CURSOR's
                                        // node, so the old page (a separate
                                        // allocation) is untouched across the loop.
                                        let (cps_ptr, cps_len): (*const u21, usize) = {
                                            let cps = unsafe {
                                                (*old_page).lookup_grapheme(old_rac.cell)
                                            }
                                            .unwrap();
                                            (cps.as_ptr(), cps.len())
                                        };
                                        for i in 0..cps_len {
                                            let cp: u21 = unsafe { *cps_ptr.add(i) };
                                            // appendGrapheme can grow the cursor
                                            // page, so read the destination from
                                            // the cursor each time rather than
                                            // holding a pointer across the call.
                                            let dst: *mut pagepkg::Cell =
                                                unsafe { (*self.screens.active).cursor.page_cell };
                                            self.active_mut()
                                                .append_grapheme(dst, cp)
                                                .map_err(|_| OutOfMemory)?;
                                        }
                                        let old_page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(old_pin.node) };
                                        unsafe { (*old_page).clear_grapheme(old_rac.cell) };
                                    }

                                    let old_page: *mut Page =
                                        unsafe { crate::pagelist::Node::page(old_pin.node) };
                                    unsafe { (*old_page).update_row_grapheme_flag(old_rac.row) };
                                }

                                // Point prev.cell to our new previous cell that
                                // we'll be appending graphemes to
                                prev_cell = unsafe { (*self.screens.active).cursor.page_cell };
                            } else {
                                self.write_cell(
                                    0,
                                    if row_wrap {
                                        pagepkg::Wide::SpacerHead
                                    } else {
                                        pagepkg::Wide::Narrow
                                    },
                                );
                                self.print_wrap()?;
                                self.write_cell(prev_cp, pagepkg::Wide::Wide);

                                // Point prev.cell to our new previous cell that
                                // we'll be appending graphemes to
                                prev_cell = unsafe { (*self.screens.active).cursor.page_cell };
                            }
                        } else {
                            unsafe { (*prev_cell).set_wide(pagepkg::Wide::Wide) };
                        }

                        // Write our spacer, since prev.cell is now wide
                        self.active_mut().cursor_right(1);

                        // Writing the spacer can grow the page to make room for
                        // the cursor hyperlink. Growing replaces the page, which
                        // invalidates `prev.cell`. Record the page identity first
                        // so the common case where nothing grows stays free.
                        //
                        // A pointer comparison alone isn't enough: pages are
                        // pooled, so a replacement can reuse the same address.
                        // The serial makes the pair a unique identity.
                        let spacer_node: *mut crate::pagelist::Node =
                            unsafe { (*(*self.screens.active).cursor.page_pin).node };
                        let spacer_serial = unsafe { (*spacer_node).serial };

                        self.write_cell(0, pagepkg::Wide::SpacerTail);

                        let active: *mut Screen = self.screens.active;
                        if !std::ptr::eq(unsafe { (*(*active).cursor.page_pin).node }, spacer_node)
                            || unsafe { (*(*(*active).cursor.page_pin).node).serial }
                                != spacer_serial
                        {
                            // @branchHint(.unlikely);

                            // The cursor is on the spacer tail we just wrote, so
                            // the wide cell we append to is the one to its left.
                            prev_cell = self.active_mut().cursor_cell_left(1);
                        }

                        // Move the cursor again so we're beyond our spacer
                        let active: *mut Screen = self.screens.active;
                        if unsafe { (*active).cursor.x } == right_limit - 1 {
                            unsafe { (*active).cursor.pending_wrap = true };
                        } else {
                            self.active_mut().cursor_right(1);
                        }
                    }

                    GraphemeWidthEffect::Narrow => 'narrow: {
                        // Prev cell is no longer wide
                        if unsafe { (*prev_cell).wide() } != pagepkg::Wide::Wide {
                            break 'narrow;
                        }
                        unsafe { (*prev_cell).set_wide(pagepkg::Wide::Narrow) };

                        // Remove the wide spacer tail. The previous cell may be
                        // under the cursor, so locate the tail from the wide base
                        // rather than by subtracting from the cursor distance.
                        let prev_x = unsafe { (*self.screens.active).cursor.x } - prev_left;
                        if prev_x < self.cols - 1 {
                            // const cells: [*]Cell = @ptrCast(prev.cell);
                            // cells[1].wide = .narrow;
                            unsafe { (*prev_cell.add(1)).set_wide(pagepkg::Wide::Narrow) };
                        }

                        // Place the cursor one cell after the now-narrow base,
                        // clamped to the right edge. Usually this moves the cursor
                        // back from after the old tail, but saved cursor state or
                        // changed margins can leave it directly on the base.
                        unsafe { (*self.screens.active).cursor.pending_wrap = false };
                        self.active_mut()
                            .cursor_horizontal_absolute((prev_x + 1).min(right_limit - 1));

                        #[allow(clippy::needless_return)]
                        {
                            break 'narrow;
                        }
                    }

                    GraphemeWidthEffect::NoChange => {}
                }

                // log.debug("c={X} grapheme attach to left={} primary_cp={X}", ...);
                log::debug!(
                    target: LOG_SCOPE,
                    "c={:X} grapheme attach to left={} primary_cp={:X}",
                    c,
                    prev_left,
                    unsafe { (*prev_cell).codepoint() }
                );
                self.active_mut().cursor_mark_dirty();
                self.active_mut()
                    .append_grapheme(prev_cell, c)
                    .map_err(|_| OutOfMemory)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The zero-width attach path of `print` (reference Terminal.zig `if
    /// (width == 0)` block). Every exit consumes the codepoint.
    #[cold]
    #[inline(never)]
    fn print_inner_zero_width(&mut self, c: u32) -> Result<(), OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        // If we have grapheme clustering enabled, we don't blindly attach
        // any zero width character to our cells and we instead just ignore
        // it.
        if self.modes.get(modespkg::Mode::GraphemeCluster) {
            return Ok(());
        }

        // If we have wraparound enabled and a pending wrap, the character
        // we're attaching to is still under the cursor. Otherwise, it's the
        // cell to the left.
        let left: size::CellCountInt = if self.modes.get(modespkg::Mode::Wraparound)
            && unsafe { (*active).cursor.pending_wrap }
        {
            0
        } else {
            1
        };

        // If we're at cell zero and not pending a wrap, then this is malformed
        // data and we don't print anything or even store this. Zero-width
        // characters are ALWAYS attached to some other non-zero-width
        // character at the time of writing.
        if unsafe { (*active).cursor.x } == 0 && left == 1 {
            log::warn!(target: LOG_SCOPE, "zero-width character with no prior character, ignoring");
            return Ok(());
        }

        // Find our previous cell
        let prev: *mut pagepkg::Cell = {
            let immediate = self.active_mut().cursor_cell_left(left);
            if unsafe { (*immediate).wide() } != pagepkg::Wide::SpacerTail {
                immediate
            } else {
                self.active_mut().cursor_cell_left(left + 1)
            }
        };

        // If our previous cell has no text, just ignore the zero-width character
        if !unsafe { (*prev).has_text() } {
            log::warn!(target: LOG_SCOPE, "zero-width character with no prior character, ignoring");
            return Ok(());
        }

        // If this is a emoji variation selector, prev must be an emoji
        if c == 0xFE0F || c == 0xFE0E {
            let prev_props = unicode::table().get(unsafe { (*prev).content_codepoint() });
            let emoji = prev_props.grapheme_break
                == unicode::props::GraphemeBreakNoControl::ExtendedPictographic;
            if !emoji {
                return Ok(());
            }
        }

        self.active_mut()
            .append_grapheme(prev, c)
            .map_err(|_| OutOfMemory)?;
        Ok(())
    }

    /// The wide-character arm of `print` (reference Terminal.zig `2 => ...`).
    /// Returns Ok(true) for the xterm no-wraparound early return, Ok(false)
    /// to continue to the shared cursor-advance tail.
    #[cold]
    #[inline(never)]
    fn print_inner_wide(
        &mut self,
        c: u32,
        right_limit: size::CellCountInt,
        charset: charsets::Charset,
    ) -> Result<bool, OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        if (right_limit - self.scrolling_region.left) > 1 {
            // If we don't have space for the wide char, we need
            // to insert spacers and wrap. Then we just print the wide
            // char as normal.
            if unsafe { (*active).cursor.x } == right_limit - 1 {
                // If we don't have wraparound enabled then we don't print
                // this character at all and don't move the cursor. This is
                // how xterm behaves.
                if !self.modes.get(modespkg::Mode::Wraparound) {
                    return Ok(true);
                }

                // We only create a spacer head if we're at the real edge
                // of the screen. Otherwise, we clear the space with a narrow.
                // This allows soft wrapping to work correctly.
                if right_limit == self.cols {
                    // Special-case: we need to set wrap to true even
                    // though we call printWrap below because if there is
                    // a page resize during writeCell then it'll fail
                    // integrity checks.
                    unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                    self.write_cell(0, pagepkg::Wide::SpacerHead);
                } else {
                    self.write_cell(0, pagepkg::Wide::Narrow);
                }
                self.print_wrap()?;
            }

            self.active_mut().cursor_mark_dirty();
            self.print_cell(c, pagepkg::Wide::Wide, charset);
            self.active_mut().cursor_right(1);
            self.write_cell(0, pagepkg::Wide::SpacerTail);
        } else {
            // This is pretty broken, terminals should never be only 1-wide.
            // We should prevent this downstream.
            self.active_mut().cursor_mark_dirty();
            self.write_cell(0, pagepkg::Wide::Narrow);
        }

        Ok(false)
    }

    /// Prints the character unmapped_c into the cell under the cursor, mapped
    /// through set, the charset print chose for it. Spacers and cells that
    /// already hold a mapped character are written with write_cell instead.
    #[inline(always)]
    fn print_cell(&mut self, unmapped_c: u32, wide: pagepkg::Wide, set: charsets::Charset) {
        let c: u32 = 'c: {
            // TODO: non-utf8 handling, gr

            // UTF-8 or ASCII is used as-is
            if set == charsets::Charset::Utf8 || set == charsets::Charset::Ascii {
                break 'c unmapped_c;
            }

            // The tables only cover 0x00-0xFF. Like xterm, which never remaps a
            // character above 255 in UTF-8 mode, we print anything above as is,
            // which also keeps it at the width it was measured with.
            if unmapped_c > u8::MAX as u32 {
                break 'c unmapped_c;
            }

            // Get our lookup table and map it
            let table = charsets::table(set);
            table[unmapped_c as usize] as u32
        };

        self.write_cell(c, wide);
    }

    /// Writes c into the cell under the cursor as it is, without a charset.
    #[inline(always)]
    fn write_cell(&mut self, c: u32, wide: pagepkg::Wide) {
        // defer self.screens.active.assertIntegrity();

        // TODO: spacers should use a bgcolor only cell

        let active: *mut Screen = self.screens.active;

        let cell: *mut pagepkg::Cell = unsafe { (*active).cursor.page_cell };

        // If the wide property of this cell is the same, then we don't
        // need to do the special handling here because the structure will
        // be the same. If it is NOT the same, then we may need to clear some
        // cells.
        if unsafe { (*cell).wide() } != wide {
            match unsafe { (*cell).wide() } {
                // Previous cell was narrow. Do nothing.
                pagepkg::Wide::Narrow => {}

                // Previous cell was wide. We need to clear the tail and head.
                pagepkg::Wide::Wide => 'wide: {
                    if unsafe { (*active).cursor.x } >= self.cols - 1 {
                        break 'wide;
                    }

                    let spacer_cell = self.active_mut().cursor_cell_right(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(spacer_cell, 1),
                    );

                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                pagepkg::Wide::SpacerTail => {
                    debug_assert!(unsafe { (*active).cursor.x } > 0);

                    // So integrity checks pass. We fix this up later so we don't
                    // need to do this without safety checks.
                    if cfg!(debug_assertions) {
                        unsafe { (*cell).set_wide(pagepkg::Wide::Narrow) };
                    }

                    let wide_cell = self.active_mut().cursor_cell_left(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(wide_cell, 1),
                    );
                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                // TODO: this case was not handled in the old terminal implementation
                // but it feels like we should do something. investigate other
                // terminals (xterm mainly) and see what's up.
                pagepkg::Wide::SpacerHead => {}
            }
        }

        let active: *mut Screen = self.screens.active;

        // If the prior value had graphemes, clear those
        if unsafe { (*cell).has_grapheme() } {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
            unsafe { (*page).clear_grapheme(cell) };
            unsafe { (*page).update_row_grapheme_flag((*active).cursor.page_row) };
        }

        // We don't need to update the style refs unless the
        // cell's new style will be different after writing.
        let style_changed = unsafe { (*cell).style_id() } != unsafe { (*active).cursor.style_id };
        if style_changed {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };

            // Release the old style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                debug_assert!(unsafe { (*(*active).cursor.page_row).styled() });
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.release(base, (*cell).style_id());
                }
            }
        }

        // Keep track if we had a hyperlink so we can unset it.
        let had_hyperlink = unsafe { (*cell).hyperlink() };

        // Write
        let mut new_cell = pagepkg::Cell::zeroed();
        new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
        new_cell.set_content_codepoint(c as u21);
        new_cell.set_style_id(unsafe { (*active).cursor.style_id });
        new_cell.set_wide(wide);
        new_cell.set_protected(unsafe { (*active).cursor.protected });
        new_cell.set_semantic_content(unsafe { (*active).cursor.semantic_content });
        unsafe { *cell = new_cell };

        if style_changed {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };

            // Use the new style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_id(base, (*cell).style_id());
                }
                unsafe { (*(*active).cursor.page_row).set_styled(true) };
            }
        }

        // If this is a Kitty unicode placeholder then we need to mark the
        // row so that the renderer can lookup rows with these much faster.
        if crate::options::KITTY_GRAPHICS && c == kitty::graphics_unicode::PLACEHOLDER {
            unsafe { (*(*active).cursor.page_row).set_kitty_virtual_placeholder(true) };
        }

        // We check for an active hyperlink first because setHyperlink
        // handles clearing the old hyperlink and an optimization if we're
        // overwriting the same hyperlink.
        if unsafe { (*active).cursor.hyperlink_id } > 0 {
            if let Err(err) = self.active_mut().cursor_set_hyperlink() {
                // @branchHint(.unlikely);
                log::warn!(
                    target: LOG_SCOPE,
                    "error reallocating for more hyperlink space, ignoring hyperlink err={err:?}"
                );

                // A partially successful grow can replace the page even when the
                // call fails, so `cell` may be stale here. The cursor pointers are
                // always reloaded, so read the cell through the cursor.
                debug_assert!(!unsafe { (*(*active).cursor.page_cell).hyperlink() });
            }
        } else if had_hyperlink {
            // If the previous cell had a hyperlink then we need to clear it.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
            unsafe { (*page).clear_hyperlink(cell) };
            unsafe { (*page).update_row_hyperlink_flag((*active).cursor.page_row) };
        }

        self.active_mut().assert_integrity();
    }

    fn print_wrap(&mut self) -> Result<(), OutOfMemory> {
        // We only mark that we soft-wrapped if we're at the edge of our
        // full screen. We don't mark the row as wrapped if we're in the
        // middle due to a right margin.
        // const cursor: *Screen.Cursor = &self.screens.active.cursor;
        // const mark_wrap = cursor.x == self.cols - 1;
        // if (mark_wrap) cursor.page_row.wrap = true;
        let cols = self.cols;
        let active: *mut Screen = self.screens.active;
        let mark_wrap = unsafe { (*active).cursor.x } == cols - 1;
        if mark_wrap {
            unsafe { (*(*active).cursor.page_row).set_wrap(true) };
        }

        // Get the old semantic prompt so we can extend it to the next
        // line. We need to do this before we index() because we may
        // modify memory.
        let old_semantic = unsafe { (*active).cursor.semantic_content };
        let old_semantic_clear = unsafe { (*active).cursor.semantic_content_clear_eol };

        // Move to the next line
        self.index()?;
        let active: *mut Screen = self.screens.active;
        let sr_left = self.scrolling_region.left;
        self.active_mut().cursor_horizontal_absolute(sr_left);

        // Our pointer should never move
        // assert(cursor == &self.screens.active.cursor);

        // We always reset our semantic prompt state
        unsafe { (*active).cursor.semantic_content = old_semantic };
        unsafe { (*active).cursor.semantic_content_clear_eol = old_semantic_clear };
        match old_semantic {
            pagepkg::SemanticContent::Output | pagepkg::SemanticContent::Input => {}
            pagepkg::SemanticContent::Prompt => unsafe {
                (*(*active).cursor.page_row)
                    .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
            },
        }

        if mark_wrap {
            let row = unsafe { (*active).cursor.page_row };
            // Always mark the row as a continuation
            unsafe { (*row).set_wrap_continuation(true) };
        }

        // Assure that our screen is consistent
        self.active_mut().assert_integrity();
        Ok(())
    }
}
