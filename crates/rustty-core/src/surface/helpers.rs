//! PORTS: ghostty src/Surface.zig — the file-scope helpers: the `endKeySequence` param
//! enums, `CompleteClipboard`, `ScrollAmount`, `closingAction`, `Text`, `Link`, the
//! `CoreSurfaceHandle`/`ThreadStateSurface` impls, `SurfaceMailbox`
//! (`apprt.surface.Mailbox`), and the std.fs.path / std.base64 ports.

#![allow(non_snake_case)]

use super::*;

// PORT NOTE: the two file-scope param enums of `endKeySequence`
// (Surface.zig:3128-3129, ported ahead of their C10 row — updateConfig's
// `.drop`/`.free` callsite names them; the C10 key tier landed the
// `.flush`/`.retain` construction sites).
// const KeySequenceQueued = enum { flush, drop };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeySequenceQueued {
    Flush,
    Drop,
}
// const KeySequenceMemory = enum { retain, free };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeySequenceMemory {
    Retain,
    Free,
}

/// The payload for completing a clipboard request with
/// completeClipboardRequest.
//
// pub const CompleteClipboard = struct { ... };
// (Zig field defaults — every field — become the `Default` impl below;
// borrowed slices, never lossy Strings — R3.)
pub struct CompleteClipboard<'a> {
    /// The representations the apprt could serve for the request's MIME
    /// types. These are immediately copied as needed so they only need
    /// to live for the duration of the completion call. Requesters that
    /// only carry text (paste, OSC 52) use the first text-like
    /// representation.
    // contents: []const terminal.clipboard.Content = &.{},
    pub contents: &'a [terminal::clipboard::Content<'a>],

    /// The listing of MIME types available on the clipboard, only
    /// gathered when the request asked for it.
    // available: []const []const u8 = &.{},
    pub available: &'a [&'a [u8]],

    /// True if any clipboard confirmation prompt was already answered
    /// by the user, skipping further prompts:
    ///
    ///   - For "regular" pasting this means that unsafe pastes are
    ///     allowed. Unsafe data is defined as data that contains
    ///     newlines, though this definition may change later to detect
    ///     other scenarios.
    ///
    ///   - For OSC 52 and Kitty clipboard protocol reads and writes no
    ///     prompt is shown to the user when this is true.
    // confirmed: bool = false,
    pub confirmed: bool,

    /// True if the user asked to remember their decision. This is only
    /// honored by request types that support session grants (Kitty
    /// clipboard protocol requests carrying a password).
    // remember: bool = false,
    pub remember: bool,
}

impl Default for CompleteClipboard<'_> {
    fn default() -> Self {
        CompleteClipboard {
            contents: &[],
            available: &[],
            confirmed: false,
            remember: false,
        }
    }
}

/// The first text-like representation of the contents, if any.
//
// fn clipboardTextContent(contents: []const terminal.clipboard.Content) ?[]const u8 {
//     for (contents) |content| {
//         if (terminal.clipboard.isTextMime(content.mime)) return content.data;
//     }
//     return null;
// }
#[allow(non_snake_case)]
pub(super) fn clipboardTextContent<'a>(
    contents: &[terminal::clipboard::Content<'a>],
) -> Option<&'a [u8]> {
    for content in contents {
        if terminal::clipboard::is_text_mime(content.mime) {
            return Some(content.data);
        }
    }
    None
}

/// PORT NOTE: helper — Zig `@intFromFloat` for the scroll/selection GESTURE pixel
/// math (`scrollCallback`'s `@intFromFloat(@trunc(amount))` /
/// `@intFromFloat(@round(xoff))` and `performBindingAction`'s
/// `.scroll_page_fractional` `@intFromFloat(@trunc(fraction * rows))`).
///
/// RUST CONTRACT (differs from the Zig builtin it replaces): Zig
/// `@intFromFloat` is safety-checked — Debug/ReleaseSafe TRAP when the value
/// is infinite or outside the destination; ReleaseFast is UB. A NaN is NOT
/// caught by the check: it converts, in every mode, to a target-dependent
/// value — `0` on arm64, `isize::MIN` on x86_64 (measured with Zig 0.16.0,
/// task R13-P2; the x86_64 build under Rosetta 2).
///
/// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD62. The trap is
/// reachable from ordinary input, so it is NOT ported: `keybind =
/// …=scroll_page_fractional:1e30` panics the reference on the keypress, and a
/// C-ABI embedder's scroll offset reaches the three gesture casts. Measured
/// with Zig 0.16.0 (`r13p-reference-probe.zig`): Debug and ReleaseSafe panic
/// "integer part of floating point value out of bounds", ReleaseFast yields
/// `9223372036854775807`; `std.fmt.parseFloat` accepts `1e30`, `inf` and `nan`,
/// so the config can carry each of them (the first two trap; `nan` does not —
/// see above).
///
/// This returns `None` where the reference traps, and ALSO for NaN, which has
/// no `isize`; each caller takes the handling its own value means — see the
/// call sites. For NaN: `scroll_page_fractional` saturates it to `0` (arm64's
/// value; x86_64's `isize::MIN` would scroll toward the top), and the gesture
/// casts scroll nothing. On the discrete x path that is arm64's delta-0 result
/// (x86_64's `isize::MIN` would drive the per-row loops). On the y and
/// precision-x paths the reference's `assert(@abs(amount) >= 1)` precedes the
/// cast, so a NaN fails that assert in a safe reference build (and ReleaseFast
/// latches it into `pending_scroll_*`); the port's guard runs ahead of its
/// copy of that assert, so a NaN from the C ABI scrolls nothing in every
/// build (task R13-C).
#[inline]
pub(super) fn isize_from_float(v: f64) -> Option<isize> {
    // `@intFromFloat` converts the INTEGER PART (truncation toward zero) and
    // safety-checks that it fits the destination. `-(isize::MIN as f64)` is
    // exactly 2^63; inf fails a comparison, as it fails the reference's check.
    // NaN fails both too, although the reference's check lets it through (see
    // the doc comment).
    let t = v.trunc();
    if t >= isize::MIN as f64 && t < -(isize::MIN as f64) {
        Some(t as isize)
    } else {
        None
    }
}

/// `isize_from_float` for a value whose out-of-range case MEANS "as far as it
/// goes": the viewport clamps the result anyway. NaN has no direction, so it
/// scrolls nothing. This is also what the reference's ReleaseFast produces
/// (measured), without its undefined behaviour.
#[inline]
pub(super) fn isize_from_float_saturating(v: f64) -> isize {
    match isize_from_float(v) {
        Some(v) => v,
        None if v.is_nan() => 0,
        None if v > 0.0 => isize::MAX,
        None => isize::MIN,
    }
}

/// `scrollCallback`'s `const y: ScrollAmount = if (yoff == 0) .{} else y: {
/// ... }` block (`Surface.zig:3469-3524`), unchanged but for its receiver.
///
/// PORT NOTE (task R13-P2, UD62): lifted out of `scrollCallback` so the
/// block's float cast is driven by a test — `Surface::init` needs a live
/// apprt, so the callback itself cannot run in one. `self.mouse`,
/// `self.size.cell` and `self.config.mouse_scroll_multiplier` are the only
/// `self` state the block touches, and they are its parameters.
pub(super) fn scroll_amount_y(
    mouse: &mut Mouse,
    cell: rendererpkg::size::CellSize,
    multiplier: configpkg::value_types::MouseScrollMultiplier,
    yoff: f64,
    scroll_mods: input_mouse::ScrollMods,
) -> ScrollAmount {
    // const y: ScrollAmount = if (yoff == 0) .{} else y: {
    if yoff == 0.0 {
        ScrollAmount::default()
    } else {
        'y: {
            // We use cell_size to determine if we have accumulated enough to trigger a scroll
            // const cell_size: f64 = @floatFromInt(self.size.cell.height);
            let cell_size: f64 = f64::from(cell.height);

            // If we have precision scroll, yoff is the number of pixels to scroll. In non-precision
            // scroll, yoff is the number of wheel ticks. Some mice are capable of reporting fractional
            // wheel ticks, which don't necessarily get reported as precision scrolls. We normalize all
            // scroll events to pixels by multiplying the wheel tick value and the cell size. This means
            // that a wheel tick of 1 results in single scroll event.
            // const yoff_adjusted: f64 = if (scroll_mods.precision)
            //     yoff * self.config.mouse_scroll_multiplier.precision
            // else yoff_adjusted: {
            let yoff_adjusted: f64 = if scroll_mods.precision {
                yoff * multiplier.precision
            } else {
                'yoff_adjusted: {
                    // if (comptime builtin.target.os.tag.isDarwin()) {
                    //
                    // (`cfg!` — the childExitedAbnormally isDarwin
                    // precedent.)
                    if cfg!(target_vendor = "apple") {
                        // Round out the yoff to an absolute minimum of 1. macos tries to
                        // simulate precision scrolling with non precision events by
                        // ramping up the magnitude of the offsets as it detects faster
                        // scrolling. Single click (very slow) scrolls are reported with a
                        // magnitude of 0.1 which would normally require a few clicks
                        // before we register an actual scroll event (depending on cell
                        // height and the mouse_scroll_multiplier setting).
                        // const yoff_max: f64 = if (yoff > 0)
                        //     @max(yoff, 1)
                        // else
                        //     @min(yoff, -1);
                        let yoff_max: f64 = if yoff > 0.0 {
                            yoff.max(1.0)
                        } else {
                            yoff.min(-1.0)
                        };

                        // break :yoff_adjusted yoff_max * cell_size * self.config.mouse_scroll_multiplier.discrete;
                        break 'yoff_adjusted yoff_max * cell_size * multiplier.discrete;
                    } else {
                        // break :yoff_adjusted yoff * cell_size * self.config.mouse_scroll_multiplier.discrete;
                        break 'yoff_adjusted yoff * cell_size * multiplier.discrete;
                    }
                }
            };

            // Add our previously saved pending amount to the offset to get the
            // new offset value. The signs of the pending and yoff should match
            // so that we move further away from zero, but we don't assert
            // this because in theory a user could scroll in the opposite
            // direction and undo a pending scroll.
            // const poff: f64 = self.mouse.pending_scroll_y + yoff_adjusted;
            let poff: f64 = mouse.pending_scroll_y + yoff_adjusted;

            // If the new offset is less than a single unit of scroll, we save
            // the new pending value and do not scroll yet.
            // if (@abs(poff) < cell_size) {
            if poff.abs() < cell_size {
                // self.mouse.pending_scroll_y = poff;
                mouse.pending_scroll_y = poff;
                // break :y .{};
                break 'y ScrollAmount::default();
            }

            // We scroll by the number of rows in the offset and save the remainder
            // const amount = poff / cell_size;
            let amount = poff / cell_size;
            // Round towards zero.
            // const delta: isize = @intFromFloat(@trunc(amount));
            //
            // PORT NOTE (R13, UD62): an amount with no `isize` representation
            // (a non-finite or astronomically large offset from the OS or a
            // C-ABI embedder) is not scrolled at all — the `.{}` arm this
            // function already takes for an offset too small to act on. It is
            // taken BEFORE the pending store, so `pending_scroll_y` can never
            // hold a NaN that poisons every later scroll, and BEFORE BOTH
            // asserts, which hold for every value that gets past here.
            // Saturating would be worse than the crash it replaces:
            // `scrollCallback`'s consumers of this amount loop `0..delta`
            // writing one report per row.
            //
            // The guard sits ahead of the reference's `assert(@abs(amount) >=
            // 1)` (task R13-C): a NaN offset from `rustty_surface_mouse_scroll`
            // makes `amount` NaN, which fails that assert, and a
            // `debug_assert!` panic inside an `extern "C"` entry point aborts
            // the process in debug and `release-safe` builds. Outside input
            // can violate it, so under R13 it is not an invariant until the
            // guard has run.
            let Some(delta) = isize_from_float(amount.trunc()) else {
                break 'y ScrollAmount::default();
            };
            // assert(@abs(amount) >= 1);
            debug_assert!(amount.abs() >= 1.0);
            // self.mouse.pending_scroll_y = poff - (amount * cell_size);
            mouse.pending_scroll_y = poff - (amount * cell_size);
            // assert(@abs(delta) >= 1);
            debug_assert!(delta.unsigned_abs() >= 1);

            // break :y .{ .delta = delta };
            break 'y ScrollAmount { delta };
        }
    }
}

/// `scrollCallback`'s `const x: ScrollAmount = if (xoff == 0) .{} else x: {
/// ... }` block (`Surface.zig:3527-3546`), lifted out for the same reason as
/// `scroll_amount_y`. It holds two of the three gesture casts.
pub(super) fn scroll_amount_x(
    mouse: &mut Mouse,
    cell: rendererpkg::size::CellSize,
    xoff: f64,
    scroll_mods: input_mouse::ScrollMods,
) -> ScrollAmount {
    // const x: ScrollAmount = if (xoff == 0) .{} else x: {
    if xoff == 0.0 {
        ScrollAmount::default()
    } else {
        'x: {
            // if (!scroll_mods.precision) {
            if !scroll_mods.precision {
                // const x_delta_isize: isize = @intFromFloat(@round(xoff));
                //
                // (R13, UD62 — see the y calculation's note.)
                let Some(x_delta_isize) = isize_from_float(xoff.round()) else {
                    break 'x ScrollAmount::default();
                };
                // break :x .{ .delta = x_delta_isize };
                break 'x ScrollAmount {
                    delta: x_delta_isize,
                };
            }

            // const poff: f64 = self.mouse.pending_scroll_x + xoff;
            let poff: f64 = mouse.pending_scroll_x + xoff;
            // const cell_size: f64 = @floatFromInt(self.size.cell.width);
            let cell_size: f64 = f64::from(cell.width);
            // if (@abs(poff) < cell_size) {
            if poff.abs() < cell_size {
                // self.mouse.pending_scroll_x = poff;
                mouse.pending_scroll_x = poff;
                // break :x .{};
                break 'x ScrollAmount::default();
            }

            // const amount = poff / cell_size;
            let amount = poff / cell_size;
            // const delta: isize = @intFromFloat(@trunc(amount));
            //
            // (R13, UD62 — see the y calculation's note, including why the
            // guard runs ahead of the `amount` assert.)
            let Some(delta) = isize_from_float(amount.trunc()) else {
                break 'x ScrollAmount::default();
            };
            // assert(@abs(amount) >= 1);
            debug_assert!(amount.abs() >= 1.0);
            // self.mouse.pending_scroll_x = poff - (amount * cell_size);
            mouse.pending_scroll_x = poff - (amount * cell_size);
            // assert(@abs(delta) >= 1);
            debug_assert!(delta.unsigned_abs() >= 1);
            // break :x .{ .delta = delta };
            break 'x ScrollAmount { delta };
        }
    }
}

// The amount to scroll. This structure is always normalized so that
// negative is down, left and positive is up, right. Note that INTERNALLY,
// vertical scroll on our terminal uses positive for down (right is not
// supported by our screen since scrollback is only vertical).
//
// PORT NOTE: the reference declares this file-scope struct BETWEEN
// refreshCallback and scrollCallback (Surface.zig:3405); Rust impl blocks
// cannot hold type items, so it lives in this trailing file-scope cluster
// (the KeySequenceQueued precedent).
// const ScrollAmount = struct {
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ScrollAmount {
    // delta: isize = 0,
    pub(super) delta: isize,
}

/// PORT NOTE: the anonymous `enum { down_left, up_right }` return type of
/// `ScrollAmount.direction`, hoisted to a named enum (the TextViewport
/// hoist precedent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollAmountDirection {
    // down_left,
    DownLeft,
    // up_right,
    UpRight,
}

impl ScrollAmount {
    // pub fn direction(self: ScrollAmount) enum { down_left, up_right } {
    pub fn direction(self) -> ScrollAmountDirection {
        // return if (self.delta < 0) .down_left else .up_right;
        if self.delta < 0 {
            ScrollAmountDirection::DownLeft
        } else {
            ScrollAmountDirection::UpRight
        }
    }

    // pub fn magnitude(self: ScrollAmount) usize {
    pub fn magnitude(self) -> usize {
        // return @abs(self.delta);
        self.delta.unsigned_abs()
    }
}

/// The portion of the screen to write for writeScreenFile.
// const WriteScreenLoc = enum {
//     screen, // Full screen
//     history, // History (scrollback)
//     selection, // Selected text
// };
//
// PORT NOTE (chunk C14b forward-declaration): a file-scope enum (the
// reference declares it at file scope between `closingAction` and
// `writeScreenFile`, Surface.zig:5644 — a Rust `enum` cannot live inside the
// `impl` block). Consumed by C14b's `performBindingAction` write arms and the
// forward-declared `Surface::writeScreenFile` skeleton; C15 fills the body
// (which uses `@tagName(loc)` for the file name). Tracked in DEFERRALS.md
// "core C14b".
#[derive(Clone, Copy)]
pub(super) enum WriteScreenLoc {
    Screen,    // Full screen
    History,   // History (scrollback)
    Selection, // Selected text
}

/// Returns true if performing the given action result in closing
/// the surface. This is used to determine if our self pointer is
/// still valid after performing some binding action.
// fn closingAction(action: input.Binding.Action) bool {
//
// PORT NOTE (ported ahead of its C14 row — Surface.zig:5633;
// maybeHandleBinding's :3013 closing-check names it; the by-value `action`
// copy is a borrow, the posToViewport receiver precedent).
pub(super) fn closingAction(action: &binding::Action) -> bool {
    // return switch (action) {
    match action {
        // .close_surface,
        // .close_window,
        // .close_tab,
        // => true,
        //
        // (Zig tag-compare — `.close_tab`'s payload is ignored.)
        binding::Action::CloseSurface
        | binding::Action::CloseWindow
        | binding::Action::CloseTab(_) => true,

        // else => false,
        _ => false,
    }
    // };
}

/// Represents text read from the terminal and some metadata about it
/// that is often useful to apprts.
// pub const Text = struct {
pub struct Text {
    /// The text that was read from the terminal.
    // text: [:0]const u8,
    //
    // PORT NOTE: raw bytes (`Vec<u8>`, never a lossy `String`); the `[:0]`
    // NUL-termination invariant is carried with the length excluding the
    // sentinel (the vt `selectionString` return convention).
    pub text: Vec<u8>,

    /// The viewport information about this text, if it is visible in
    /// the viewport.
    // viewport: ?Viewport = null,
    pub viewport: Option<TextViewport>,
}

/// PORT NOTE: the nested `Text.Viewport` (Surface.zig:1875), hoisted to a
/// top-level name (the `DerivedConfigLink` precedent).
// pub const Viewport = struct {
pub struct TextViewport {
    /// The top-left corner of the selection in pixels within the viewport.
    // tl_px_x: f64,
    // tl_px_y: f64,
    pub tl_px_x: f64,
    pub tl_px_y: f64,

    /// The linear offset of the start of the selection and the length.
    /// This is "linear" in the sense that it is the offset in the
    /// flattened viewport as a single array of text.
    ///
    /// Note: these values are currently wrong if there is a partially
    /// visible selection in the viewport (i.e. the top-left or
    /// bottom-right of the selection is outside the viewport). But the
    /// apprt usecase we have right now doesn't require these to be
    /// correct so... let's fix this later. The wrong values will always
    /// be within the text bounds so we aren't risking an overflow.
    // offset_start: u32,
    // offset_len: u32,
    pub offset_start: u32,
    pub offset_len: u32,
}

impl Text {
    // pub fn deinit(self: *Text, alloc: Allocator) void {
    //     alloc.free(self.text);
    // }
    //
    // PORT NOTE: the owned bytes free on drop; the explicit free maps to
    // clearing the field (the DerivedConfig deinit precedent — the alloc
    // param collapses with the Rust ownership).
    pub fn deinit(&mut self) {
        self.text = Vec::new();
    }
}

/// PORT NOTE: the file-scope `Link` returned by `linkAtPos` (Surface.zig:4271 —
/// declared between `maybePromptClick` and `linkAtPos`; Rust impl blocks
/// cannot hold type items, so it lives in this trailing cluster with a
/// position pointer at the reference site). NOT the `DerivedConfig.Link`
/// sibling ([`DerivedConfigLink`]).
// const Link = struct {
pub(super) struct Link {
    // action: input.Link.Action,
    pub(super) action: link::Action,
    // selection: terminal.Selection,
    pub(super) selection: terminal::selection::Selection,
}

/// The already-landed A5 seam (CORE_BLUEPRINT §1): `Target.cval` reads
/// `v.rt_surface` off the core `*CoreSurface` — rustty-core's `Surface`
/// hands the opaque pointer out through the interface trait.
impl<A: apprt::App + 'static> apprt::CoreSurfaceHandle for Surface<A> {
    fn rt_surface_ptr(&self) -> *mut core::ffi::c_void {
        self.rt_surface.cast()
    }
}

/// The termio Copy/Export seam trampolines (`ad96613a8` — the inspector
/// widget's `copyEvents`/`exportEvents` reach `surface.rt_surface`/
/// `surface.rt_app` through the `surface: *Surface` they receive).
/// rustty-inspector cannot name core (the R4 acyclic Surface seam,
/// `rustty_inspector::widgets::surface` module doc), so the apprt caller
/// stores the erased `*mut Surface<A>` plus these monomorphized trampolines
/// into the inspector's `SurfaceData`; the widget invokes them with the values
/// the reference names at its callsites.
impl<A: apprt::App + 'static> Surface<A> {
    /// `surface.rt_surface.setClipboard(.standard, &.{.{ .mime = ..., .data =
    /// ... }}, confirm)` (termio.zig `copyEvents`): the `.standard` clipboard
    /// target is the only kind the widget names, fixed here; the concrete
    /// backend error erases to `Err(())` (consumed by the widget's `catch
    /// log.err` arm). Matches `SurfaceData::rt_set_clipboard`.
    ///
    /// # Safety
    ///
    /// `surface` must be the erased live `*mut Surface<A>` stored into the
    /// same `SurfaceData` as this trampoline, valid for the inspector draw
    /// call (INV-1/INV-11); `data`'s backing memory must carry a NUL one past
    /// the slice (the `[:0]const u8` convention of the `setClipboard`
    /// callsites above).
    pub unsafe fn inspector_rt_set_clipboard(
        surface: *mut core::ffi::c_void,
        mime: &[u8],
        data: &[u8],
        confirm: bool,
    ) -> Result<(), ()> {
        let self_: *mut Surface<A> = surface.cast();
        // SAFETY: caller contract above — `self_` and its `rt_surface` are
        // live for this call; a narrow shared reborrow of the runtime surface
        // (the same shape as this file's other `&*self.rt_surface` callsites).
        unsafe { &*(*self_).rt_surface }
            .set_clipboard(
                apprt::Clipboard::Standard,
                &[apprt::ClipboardContent { mime, data }],
                confirm,
            )
            .map_err(|_| ())
    }

    /// `surface.rt_app.performAction(.{ .surface = surface },
    /// .export_terminal_io, .{ .contents = contents })` (termio.zig
    /// `exportEvents`): returns the reference `!bool` ("handled"); the
    /// concrete backend error erases to `Err(())`. Matches
    /// `SurfaceData::rt_export_terminal_io`.
    ///
    /// # Safety
    ///
    /// `surface` must be the erased live `*mut Surface<A>` stored into the
    /// same `SurfaceData` as this trampoline, valid for the inspector draw
    /// call (INV-1/INV-11).
    pub unsafe fn inspector_rt_export_terminal_io(
        surface: *mut core::ffi::c_void,
        contents: &[u8],
    ) -> Result<bool, ()> {
        let self_: *mut Surface<A> = surface.cast();
        // SAFETY: caller contract above — `self_` and its `rt_app` are live
        // for this call; the `&*self_` target reborrow is narrow (the action
        // dispatch), the same shape as this file's other `performAction`
        // callsites.
        unsafe {
            (*(*self_).rt_app).perform_action(
                apprt::Target::surface_ptr(self_),
                apprt::Action::ExportTerminalIO(apprt::action::ExportTerminalIO { contents }),
            )
        }
        .map_err(|_| ())
    }
}

/// The crash seam realized (CORE_BLUEPRINT §1; landed with C8's
/// `crashThreadState`, ahead of its C10 row): `beforeSend`'s surface
/// metadata reads — `surface.size.screen`, `surface.size.grid()`,
/// `surface.size.cell` — as plain `(u32, u32)` tuples (crash must not dep
/// rustty-renderer for the size types; the rustty-crash module doc).
impl<A: apprt::App + 'static> crash::sentry::ThreadStateSurface for Surface<A> {
    /// `surface.size.screen.{width, height}` (sentry.zig:217/221).
    fn screen_size(&self) -> (u32, u32) {
        (self.size.screen.width, self.size.screen.height)
    }

    /// `surface.size.grid().{columns, rows}` (sentry.zig:214).
    fn grid_size(&self) -> (u32, u32) {
        let g = self.size.grid();
        (u32::from(g.columns), u32::from(g.rows))
    }

    /// `surface.size.cell.{width, height}` (sentry.zig:233/237).
    fn cell_size(&self) -> (u32, u32) {
        (self.size.cell.width, self.size.cell.height)
    }
}

// PORT NOTE: `apprt.surface.Mailbox` (src/apprt/surface.zig:135) — relocated
// HERE per CORE_BLUEPRINT §1 (it names the core `Surface`, which apprt-api
// must not). Re-exported as [`Mailbox`] (Surface.zig:15 `pub const Mailbox =
// apprt.surface.Mailbox`); implements the apprt-api `AppMailbox` trait below
// (termio's M seam).

/// A surface mailbox.
// pub const Mailbox = struct {
//     surface: *Surface,
//     app: App.Mailbox,
//     ...
// };
pub struct SurfaceMailbox<A: apprt::App + 'static> {
    pub surface: *mut Surface<A>,
    pub app: crate::app::Mailbox<A>,
}

impl<A: apprt::App + 'static> AppMailbox for SurfaceMailbox<A> {
    type Surface = *mut Surface<A>;

    /// `self.surface` for termio's io-thread crash metadata (LEDGER S211-F2,
    /// S212-F2); `Surface<A>` is the `ThreadStateSurface` impl above.
    fn crash_surface(&self) -> *const dyn crash::sentry::ThreadStateSurface {
        self.surface as *const Surface<A>
    }

    /// Send a message to the surface.
    // pub fn push(
    //     self: Mailbox,
    //     msg: Message,
    //     timeout: App.Mailbox.Queue.Timeout,
    // ) App.Mailbox.Queue.Size {
    //
    // PORT NOTE: `&self` for the reference's by-value `self: Mailbox`
    // receiver (the type is `Copy`); `Message<'static>`/concrete
    // `Timeout`/`Size` per the CORE_BLUEPRINT §3 trait resolution.
    //
    // PORT NOTE (core C-T): the reference `push` is realized as the retain-on-
    // failure primitive `push_or_return` (the plain `AppMailbox::push` is the
    // trait's drop-on-failure default over it). termio's
    // `StreamHandler.surfaceMessageWriter` re-pushes its by-value message copy
    // after a failed instant push; since Rust moves the message into the app
    // queue, the failed push hands the App-level message back and we unwrap the
    // surface envelope so the caller regains its original `SurfaceMessage`.
    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        msg: Message<'static>,
        timeout: Timeout,
    ) -> Result<QueueSize, Message<'static>> {
        // Surface message sending is actually implemented on the app
        // thread, so we have to rewrap the message with our surface
        // pointer and send it to the app thread.
        // return self.app.push(.{
        //     .surface_message = .{
        //         .surface = self.surface,
        //         .message = msg,
        //     },
        // }, timeout);
        match self.app.push_or_return(
            crate::app::Message::SurfaceMessage(apprt::SurfaceMessageEnvelope {
                surface: self.surface,
                message: msg,
            }),
            timeout,
        ) {
            Ok(size) => Ok(size),
            // The app queue hands the App-level message back on failure; unwrap
            // the surface envelope to return the caller's original message.
            Err(crate::app::Message::SurfaceMessage(env)) => Err(env.message),
            // Only `SurfaceMessage` is ever constructed above, so no other
            // returned variant is possible.
            Err(_) => unreachable!(),
        }
    }
}

// PORT NOTE ([ADV-FIX 6a]): like `app::Mailbox<A>`, the reference copies
// `apprt.surface.Mailbox` BY VALUE (`.surface_mailbox = .{ .surface = self,
// .app = app_mailbox }`, Surface.zig:555; by-value `self: Mailbox` receiver
// in the reference `push`). MANUAL `Clone`/`Copy` with no bound on `A` (the
// fields are a raw pointer + a `Copy` mailbox handle).
impl<A: apprt::App + 'static> Clone for SurfaceMailbox<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: apprt::App + 'static> Copy for SurfaceMailbox<A> {}

// ===========================================================================
// PORT NOTE: helpers — std analogues used above (std.fs.path / std.fs.access /
// std.base64 / the Zig `Options` value-copy). These add nothing the reference
// doesn't already do (R1): they are the minimal Rust glue for the std calls
// the reference makes inline (the rustty-config theme.rs / path.rs and
// rustty-os file.rs precedents).
// ===========================================================================

/// `std.fs.path.isAbsolute` for the POSIX targets the parity harness runs
/// (the rustty-config theme.rs/path.rs helper).
pub(super) fn is_absolute(path: &[u8]) -> bool {
    !path.is_empty() && path[0] == b'/'
}

/// PORTS: `std.fs.path.resolve(alloc, &.{ base, path })` for the simple
/// two-component POSIX case used by `resolvePathForOpening`.
//
// PORT NOTE: `resolve` joins the components and normalizes `.`/`..` against
// the (absolute) base, yielding an absolute path WITHOUT touching the
// filesystem (unlike `realpath`/`canonicalize`) — the rustty-config path.rs
// helper, verbatim.
pub(super) fn resolve(components: &[&[u8]]) -> Vec<u8> {
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut is_abs = false;
    for comp in components {
        if comp.is_empty() {
            continue;
        }
        if comp[0] == b'/' {
            // An absolute component resets the stack.
            is_abs = true;
            stack.clear();
        }
        for seg in comp.split(|&b| b == b'/') {
            match seg {
                b"" | b"." => {}
                b".." => {
                    stack.pop();
                }
                _ => stack.push(seg.to_vec()),
            }
        }
    }

    let mut out: Vec<u8> = Vec::new();
    if is_abs {
        out.push(b'/');
    }
    for (i, seg) in stack.iter().enumerate() {
        if i > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(seg);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    out
}

/// Convert raw path bytes to an `OsStr` (POSIX bytes are `OsStr` directly;
/// R3/fidelity: byte-slices stay bytes, never a lossy round-trip).
#[cfg(unix)]
pub(super) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
pub(super) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    // PORT NOTE: on non-unix the bytes are interpreted as UTF-8 (lossy
    // fallback); the path-opening flow is primarily exercised on POSIX.
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// Raw bytes of a `Path` (the inverse of [`os_str_from_bytes`]): the reference
/// `std.fs.Dir.realpath` returns a `[]const u8` path slice; the Rustty port
/// returns an owning `PathBuf` and `writeScreenFile` recovers its bytes here
/// (POSIX paths are bytes directly — R3/fidelity, never a lossy round-trip).
#[cfg(unix)]
pub(super) fn path_to_bytes(path: &std::path::Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

#[cfg(not(unix))]
pub(super) fn path_to_bytes(path: &std::path::Path) -> &[u8] {
    // PORT NOTE: on non-unix the path is UTF-8 (lossy fallback); the
    // scrollback-file flow is primarily exercised on POSIX.
    path.to_str().unwrap_or("").as_bytes()
}

/// PORT NOTE (Zig value-copy): `terminal.formatter.Options` is a Zig value
/// type freely copied at each `.init(..., opts)` / `var copy = opts` site
/// (`copySelectionToClipboards`); Rust `Options` owns its `codepoint_map`
/// `Vec`, so this helper materializes each reference copy — unlike the vt
/// formatter.rs `copy_options` (extras-only path), the map is KEPT (cloned;
/// Zig's header copy borrows the same entries).
pub(super) fn copy_options(opts: &terminal::formatter::Options) -> terminal::formatter::Options {
    terminal::formatter::Options {
        emit: opts.emit,
        unwrap: opts.unwrap,
        trim: opts.trim,
        codepoint_map: opts.codepoint_map.clone(),
        background: opts.background,
        foreground: opts.foreground,
        palette: opts.palette,
    }
}

/// PORTS: the Zig `std.base64` STANDARD codec's `Encoder` surface
/// (`calcSize` + `encode`) used by `completeClipboardReadOSC52` (the OSC 52
/// reply) — semantics reproduced bit-for-bit from lib/std/base64.zig
/// (Zig 0.15.2). The encoder's 128-/32-bit `readInt` SIMD-ish fast loops are
/// accelerative only; the scalar tails below ARE the semantic core, ported
/// verbatim (the rustty-os file.rs `b64_encoder` precedent).
///
/// PORT NOTE (bc2f7d7d2): the `Decoder` surface (`calcSizeForSlice` +
/// `decode`) that `clipboardWrite` consumed is GONE with the oracle — the
/// OSC 52 write path now decodes through `simd.base64.decodeStrict`.
pub(super) mod zig_base64 {
    // pub const standard_alphabet_chars = "ABC...xyz0123456789+/".*;
    const ALPHABET_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // .pad_char = '=',
    const PAD_CHAR: u8 = b'=';

    // ---- Base64Encoder (std.base64.standard.Encoder) ----

    /// Compute the encoded length.
    // pub fn calcSize(encoder: *const Base64Encoder, source_len: usize) usize {
    //     if (encoder.pad_char != null) {
    //         return @divTrunc(source_len + 2, 3) * 4;
    //     } else { ... }
    // }
    //
    // (the standard codec HAS a pad char — the no-pad arm is dead for this
    // codec and elided, matching the decoder's dead-arm elisions above.)
    //
    // PARITY: clippy suggests `.div_ceil(3)` for `(source_len + 2) / 3`, but
    // rewriting would diverge from the reference `@divTrunc` expression; kept
    // as-is for fidelity (the rustty-os file.rs `b64_encoder::calc_size`
    // precedent).
    #[allow(clippy::manual_div_ceil)]
    pub const fn calc_size(source_len: usize) -> usize {
        // return @divTrunc(source_len + 2, 3) * 4;
        (source_len + 2) / 3 * 4
    }

    /// dest.len must at least be what you get from ::calcSize.
    // pub fn encode(encoder: *const Base64Encoder, dest: []u8, source: []const u8) []const u8 {
    pub fn encode<'a>(dest: &'a mut [u8], source: &[u8]) -> &'a [u8] {
        // const out_len = encoder.calcSize(source.len);
        let out_len = calc_size(source.len());
        // assert(dest.len >= out_len);
        debug_assert!(dest.len() >= out_len);

        // var idx: usize = 0;
        let mut idx: usize = 0;
        // var out_idx: usize = 0;
        let mut out_idx: usize = 0;
        // (the 128-bit big-endian `readInt` fast loop is accelerative only — it
        // requires a 4-byte-aligned lookahead and produces output identical to
        // the scalar 3-byte grouping below; elided as with the decoder's fast
        // loops. The 32-bit `readInt` loop below is scalarized: `readInt(u32,
        // .big)` reads a lookahead 4th byte whose low 8 bits are unused, so the
        // four 6-bit fields it extracts equal the byte-shuffle here.)
        // while (idx + 3 < source.len) : (idx += 3) {
        while idx + 3 < source.len() {
            // dest[out_idx] = alphabet[(bits >> 26) & 0x3f];   (= source[idx] >> 2)
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[(bits >> 20) & 0x3f];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(bits >> 14) & 0x3f];
            dest[out_idx + 2] =
                ALPHABET_CHARS[(((source[idx + 1] & 0xf) << 2) | (source[idx + 2] >> 6)) as usize];
            // dest[out_idx + 3] = alphabet[(bits >> 8) & 0x3f];   (= source[idx+2] & 0x3f)
            dest[out_idx + 3] = ALPHABET_CHARS[(source[idx + 2] & 0x3f) as usize];
            // out_idx += 4;
            out_idx += 4;
            idx += 3;
        }
        // if (idx + 2 < source.len) {
        if idx + 2 < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(source[idx + 1] & 0xf) << 2 | (source[idx + 2] >> 6)];
            dest[out_idx + 2] =
                ALPHABET_CHARS[(((source[idx + 1] & 0xf) << 2) | (source[idx + 2] >> 6)) as usize];
            // dest[out_idx + 3] = alphabet[source[idx + 2] & 0x3f];
            dest[out_idx + 3] = ALPHABET_CHARS[(source[idx + 2] & 0x3f) as usize];
            // out_idx += 4;
            out_idx += 4;
        // } else if (idx + 1 < source.len) {
        } else if idx + 1 < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(source[idx + 1] & 0xf) << 2];
            dest[out_idx + 2] = ALPHABET_CHARS[((source[idx + 1] & 0xf) << 2) as usize];
            // out_idx += 3;
            out_idx += 3;
        // } else if (idx < source.len) {
        } else if idx < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[(source[idx] & 0x3) << 4];
            dest[out_idx + 1] = ALPHABET_CHARS[((source[idx] & 0x3) << 4) as usize];
            // out_idx += 2;
            out_idx += 2;
        }
        // if (encoder.pad_char) |pad_char| {
        //     for (dest[out_idx..out_len]) |*pad| pad.* = pad_char;
        // }
        for pad in &mut dest[out_idx..out_len] {
            *pad = PAD_CHAR;
        }
        // return dest[0..out_len];
        &dest[0..out_len]
    }
}
