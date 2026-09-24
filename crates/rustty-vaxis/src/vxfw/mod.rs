//! rustty-vaxis::vxfw — the vxfw (vaxis widget framework) SUBSET that `cli +boo`
//! consumes, ported from the vendored `libvaxis` 0.6.0 `src/vxfw/` (0.5.1 until S-10b).
//!
//! PORTS: `vaxis/src/vxfw/vxfw.zig` — the framework core: the value types
//! (`Size`/`MaxSize`/`Point`/`RelativePoint`/`HitResult`/`CursorState`/`UserEvent`/
//! `FlexItem`), the `Event` union (+ `LoopEvent` so `App` can queue it), the
//! `Command` union + `Tick`, `EventContext`, `DrawContext`, the type-erased
//! `Widget` interface, and `Surface`/`SubSurface`.
//!
//! YAGNI (design spec): `cli +boo` uses only `App` + `Text` + `Center`, so ONLY
//! those three widgets are ported (`pub mod app/text/center` below). The other 13
//! vxfw widgets (Border/Button/FlexColumn/…) are unused by ghostty and out of
//! scope; the `vxfw.zig` re-exports of them are intentionally omitted.
//!
//! DESIGN DECISIONS (signed off in the workstream spec):
//!  * `Surface.buffer`/`.children` are ported as **RAW slices** (`*mut [T]`), not
//!    borrow-checked `&'a`. This is forced by `App` storing `mouse_handler
//!    .last_frame = surface` across `arena.reset(.free_all)`: a lifetime-bound model
//!    would have to redesign `last_frame` storage and diverge from the reference
//!    control flow. `Surface` is `Copy` (all fields are `Copy`), faithful to Zig
//!    passing `Surface` by value. The memory-safety of this raw aliasing (the
//!    type-erased `Widget` re-entry H1, `last_frame`-across-`reset` H2, and the
//!    `Boo.buffer` alias vs `updateFrame` H3) is PROVEN on the isolated Miri
//!    harness under `.port/phase-2/mirihar-vxfw` (both Stacked + Tree Borrows).
//!  * `DrawContext.arena` is a `*const VxfwArena` — a NEW vxfw-local bump arena with
//!    `reset` (Rust std has no arena; `pretty_arena::Arena` is byte-only and never
//!    resets). It is the faithful image of `std.heap.ArenaAllocator` with
//!    `reset(.free_all)`, not a direct file port (justified + Miri-proven).
//!  * `DrawContext.width_method` (a Zig process-global `var`) is a module `AtomicU8`.
//!    Sound because render is single-threaded main-thread and the method is set once
//!    (`DrawContext::init`) before the loop starts.
//!  * Error surface: Zig widget handlers return `anyerror!void` and draw fns return
//!    `Allocator.Error!Surface`. Under `panic=abort` allocation failure aborts (so
//!    draw is effectively infallible); the only real errors in this subset are the
//!    tty IO errors in `App.run`. Both map to [`Error`] = `std::io::Error`.

use std::cell::UnsafeCell;
use std::os::raw::c_void;

use crate::cell::{self, Cell};
use crate::gwidth::{self, Method};
use crate::key::KeyOwned;
use crate::loop_::LoopEvent;
use crate::mouse::{Mouse, Shape};
use crate::unicode::{self, GraphemeIterator};
use crate::window::{ChildOptions, Window};
use crate::Winsize;

pub mod app;
pub mod center;
pub mod text;

pub use app::App;
pub use center::Center;
pub use text::Text;

/// The vxfw error surface. Zig's `anyerror` / `Allocator.Error` both collapse to
/// this: allocation failure aborts under `panic=abort`, so the only recoverable
/// errors are the tty IO errors surfaced by `App.run`.
pub type Error = std::io::Error;

/// `pub const CommandList = std.ArrayList(Command);`
pub type CommandList<'a> = Vec<Command<'a>>;

// ===========================================================================
// Value types
// ===========================================================================

/// `pub const Size = struct { width: u16 = 0, height: u16 = 0 };`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

/// `pub const MaxSize = struct { width: ?u16 = null, height: ?u16 = null };`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaxSize {
    pub width: Option<u16>,
    pub height: Option<u16>,
}

impl MaxSize {
    /// Returns true if the row would fall outside of this height. A null height value is infinite
    /// and always returns false
    pub fn outside_height(self, row: u16) -> bool {
        let Some(max) = self.height else {
            return false;
        };
        row >= max
    }

    /// Returns true if the col would fall outside of this width. A null width value is infinite
    /// and always returns false
    pub fn outside_width(self, col: u16) -> bool {
        let Some(max) = self.width else {
            return false;
        };
        col >= max
    }

    /// Asserts that neither height nor width are null
    pub fn size(self) -> Size {
        assert!(self.width.is_some());
        assert!(self.height.is_some());
        Size {
            width: self.width.unwrap(),
            height: self.height.unwrap(),
        }
    }

    pub fn from_size(other: Size) -> MaxSize {
        MaxSize {
            width: Some(other.width),
            height: Some(other.height),
        }
    }
}

/// `pub const Point = struct { row: u16, col: u16 };`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub row: u16,
    pub col: u16,
}

/// `pub const RelativePoint = struct { row: i17, col: i17 };`
///
/// Zig `i17` has no Rust primitive; `i32` covers the range and the reference only
/// ever stores small non-negative offsets (`@intCast` from `u16` layout math), so
/// the wider integer is faithful to the values used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelativePoint {
    pub row: i32,
    pub col: i32,
}

/// Result of a hit test
#[derive(Clone, Copy)]
pub struct HitResult<'a> {
    pub local: Point,
    pub widget: Widget<'a>,
}

/// `pub const CursorState = struct { row: u16, col: u16, shape: ... = .default };`
#[derive(Clone, Copy, Debug)]
pub struct CursorState {
    /// Local coordinates
    pub row: u16,
    /// Local coordinates
    pub col: u16,
    pub shape: cell::CursorShape,
}

/// `pub const UserEvent = struct { name: []const u8, data: ?*const anyopaque = null };`
#[derive(Clone, Copy)]
pub struct UserEvent<'a> {
    pub name: &'a [u8],
    pub data: Option<*const c_void>,
}

/// `pub const FlexItem = struct { widget: Widget, flex: u8 = 1 };`
#[derive(Clone, Copy)]
pub struct FlexItem<'a> {
    pub widget: Widget<'a>,
    /// A value of zero means the child will have it's inherent size. Any value greater than zero
    /// and the remaining space will be proportioned to each item
    pub flex: u8,
}

impl<'a> FlexItem<'a> {
    pub fn init(child: Widget<'a>, flex: u8) -> FlexItem<'a> {
        FlexItem {
            widget: child,
            flex,
        }
    }
}

// ===========================================================================
// Event
// ===========================================================================

/// `pub const Event = union(enum) { ... }`.
///
/// PORT-NOTE: the queued type (`App` uses `vaxis.Loop(vxfw.Event)`), so `Event`
/// owns its payloads to be `Send + 'static` (the [`LoopEvent`] bound): `key_press`/
/// `key_release` carry a [`KeyOwned`] (the Loop copies the grapheme text out of the
/// transient parser buffer — §3b precedent), `paste` owns a `Vec<u8>`. Delivered to
/// handlers by value (Zig copies the tagged union); [`Clone`] supplies the multiple
/// deliveries a single event makes across the capture/target/bubble phases.
#[derive(Clone)]
pub enum Event<'a> {
    KeyPress(KeyOwned),
    KeyRelease(KeyOwned),
    Mouse(Mouse),
    FocusIn,  // window has gained focus
    FocusOut, // window has lost focus
    PasteStart,
    PasteEnd,
    Paste(Vec<u8>),            // osc 52 paste, caller must free
    ColorReport(cell::Report), // osc 4, 10, 11, 12 response
    ColorScheme(cell::Scheme), // light / dark OS theme changes
    Winsize(Winsize),          // the window size has changed
    App(UserEvent<'a>),        // A custom event from the app
    Tick,                      // An event from a Tick command
    Init,                      // sent when the application starts
    MouseLeave,                // The mouse has left the widget
    MouseEnter,                // The mouse has entered the widget
}

// SAFETY: the Loop read thread only ever constructs the pointer-free variants
// (KeyPress/KeyRelease/Mouse/Focus*/Paste*/Color*/Winsize) — see `LoopEvent` below.
// The only non-`Send` payload is `App(UserEvent { data: *const c_void })`, which is
// an app-main-thread-only event never enqueued in this subset. So the queued
// `Event` is `Send` in practice; the bound is satisfied by this promise.
unsafe impl Send for Event<'_> {}

impl LoopEvent for Event<'static> {
    fn key_press(k: KeyOwned) -> Option<Self> {
        Some(Event::KeyPress(k))
    }
    fn key_release(k: KeyOwned) -> Option<Self> {
        Some(Event::KeyRelease(k))
    }
    fn mouse(m: Mouse) -> Option<Self> {
        Some(Event::Mouse(m))
    }
    fn winsize(ws: Winsize) -> Option<Self> {
        Some(Event::Winsize(ws))
    }
    fn focus_in() -> Option<Self> {
        Some(Event::FocusIn)
    }
    fn focus_out() -> Option<Self> {
        Some(Event::FocusOut)
    }
    fn mouse_leave() -> Option<Self> {
        Some(Event::MouseLeave)
    }
    fn paste_start() -> Option<Self> {
        Some(Event::PasteStart)
    }
    fn paste_end() -> Option<Self> {
        Some(Event::PasteEnd)
    }
    fn paste(b: Vec<u8>) -> Option<Self> {
        Some(Event::Paste(b))
    }
    fn color_report(r: cell::Report) -> Option<Self> {
        Some(Event::ColorReport(r))
    }
    fn color_scheme(s: cell::Scheme) -> Option<Self> {
        Some(Event::ColorScheme(s))
    }
}

// ===========================================================================
// Tick + Command
// ===========================================================================

/// `pub const Tick = struct { deadline: std.Io.Timestamp, widget: Widget };`
///
/// PORT NOTE (libvaxis 0.6.0): the deadline is a `std.Io.Timestamp` read from the `.awake`
/// (monotonic) clock; 0.5.1 kept `deadline_ms: i64` from `std.time.milliTimestamp()` (the
/// wall clock). `std::time::Instant` is Rust's monotonic clock.
#[derive(Clone, Copy)]
pub struct Tick<'a> {
    pub deadline: std::time::Instant,
    pub widget: Widget<'a>,
}

impl<'a> Tick<'a> {
    /// `pub fn lessThan(_: void, lhs: Tick, rhs: Tick) bool { return lhs.deadline.nanoseconds > rhs.deadline.nanoseconds; }`
    ///
    /// PORT-NOTE: DESCENDING by deadline (`>`), so the timer list stays sorted with
    /// the soonest deadline LAST (the reference `checkTimers` pops from the end).
    pub fn less_than(lhs: &Tick, rhs: &Tick) -> bool {
        lhs.deadline > rhs.deadline
    }

    /// `pub fn in(io: std.Io, ms: u32, widget: Widget) Command` (the `io` is dropped).
    pub fn in_ms(ms: u32, widget: Widget<'a>) -> Command<'a> {
        let now = std::time::Instant::now();
        let deadline = now + std::time::Duration::from_millis(u64::from(ms));
        Command::Tick(Tick { deadline, widget })
    }
}

/// `pub const Command = union(enum) { ... }`
pub enum Command<'a> {
    /// Callback the event with a tick event at the specified deadlline
    Tick(Tick<'a>),
    /// Change the mouse shape. This also has an implicit redraw
    SetMouseShape(Shape),
    /// Request that this widget receives focus
    RequestFocus(Widget<'a>),
    /// Try to copy the provided text to the host clipboard. Uses OSC 52. Silently fails if terminal
    /// doesn't support OSC 52
    CopyToClipboard(Vec<u8>),
    /// Set the title of the terminal
    SetTitle(Vec<u8>),
    /// Queue a refresh of the entire screen. Implicitly sets redraw
    QueueRefresh,
    /// Send a system notification
    Notify {
        title: Option<Vec<u8>>,
        body: Vec<u8>,
    },
    QueryColor(cell::Kind),
}

// ===========================================================================
// EventContext
// ===========================================================================

/// `pub const Phase = enum { capturing, at_target, bubbling };`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Capturing,
    AtTarget,
    Bubbling,
}

/// `pub const EventContext = struct { ... }`.
///
/// PORT-NOTE: the Zig `alloc: Allocator` field (and 0.6.0's `io: std.Io`, used only by
/// `tick` for the clock) are dropped — the `cmds` `Vec` and the
/// `sendNotification` dupes own their memory (Rust frees on drop / handler consumes).
pub struct EventContext<'a> {
    pub phase: Phase,
    pub cmds: CommandList<'a>,
    /// The event was handled, do not pass it on
    pub consume_event: bool,
    /// Tells the event loop to redraw the UI
    pub redraw: bool,
    /// Quit the application
    pub quit: bool,
}

impl<'a> EventContext<'a> {
    pub fn add_cmd(&mut self, cmd: Command<'a>) -> Result<(), Error> {
        self.cmds.push(cmd);
        Ok(())
    }

    pub fn tick(&mut self, ms: u32, widget: Widget<'a>) -> Result<(), Error> {
        self.add_cmd(Tick::in_ms(ms, widget))
    }

    pub fn consume_and_redraw(&mut self) {
        self.consume_event = true;
        self.redraw = true;
    }

    pub fn consume_event(&mut self) {
        self.consume_event = true;
    }

    pub fn set_mouse_shape(&mut self, shape: Shape) -> Result<(), Error> {
        self.add_cmd(Command::SetMouseShape(shape))?;
        self.redraw = true;
        Ok(())
    }

    pub fn request_focus(&mut self, widget: Widget<'a>) -> Result<(), Error> {
        self.add_cmd(Command::RequestFocus(widget))
    }

    /// Copy content to clipboard.
    /// content is duplicated using self.alloc.
    /// Caller retains ownership of their copy of content.
    ///
    /// (libvaxis 0.6.0; 0.5.1 queued the caller's slice. The port queues an owned copy.)
    pub fn copy_to_clipboard(&mut self, content: &[u8]) -> Result<(), Error> {
        self.add_cmd(Command::CopyToClipboard(content.to_vec()))
    }

    /// Set window title.
    /// title is duplicated using self.alloc.
    /// Caller retains ownership of their copy of title.
    pub fn set_title(&mut self, title: &[u8]) -> Result<(), Error> {
        self.add_cmd(Command::SetTitle(title.to_vec()))
    }

    pub fn queue_refresh(&mut self) -> Result<(), Error> {
        self.add_cmd(Command::QueueRefresh)?;
        self.redraw = true;
        Ok(())
    }

    /// Send a system notification. This function dupes title and body using it's own allocator.
    /// They will be freed once the notification has been sent
    pub fn send_notification(
        &mut self,
        maybe_title: Option<&[u8]>,
        body: &[u8],
    ) -> Result<(), Error> {
        if let Some(title) = maybe_title {
            return self.add_cmd(Command::Notify {
                title: Some(title.to_vec()),
                body: body.to_vec(),
            });
        }
        self.add_cmd(Command::Notify {
            title: None,
            body: body.to_vec(),
        })
    }

    pub fn query_color(&mut self, kind: cell::Kind) -> Result<(), Error> {
        self.add_cmd(Command::QueryColor(kind))
    }
}

// ===========================================================================
// DrawContext + the width-method module global
// ===========================================================================

// `var width_method: vaxis.gwidth.Method = .unicode;` — a Zig process-global. Modeled
// as a module `AtomicU8` (0=Unicode, 1=Wcwidth, 2=NoZwj). Sound because render runs
// single-threaded on the main thread and the method is set once via `DrawContext::init`
// before the loop starts (design decision 4).
static WIDTH_METHOD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn method_to_u8(m: Method) -> u8 {
    match m {
        Method::Unicode => 0,
        Method::Wcwidth => 1,
        Method::NoZwj => 2,
    }
}

fn method_from_u8(v: u8) -> Method {
    match v {
        1 => Method::Wcwidth,
        2 => Method::NoZwj,
        _ => Method::Unicode,
    }
}

/// `pub const DrawContext = struct { arena, min, max, cell_size, var width_method };`
///
/// `Copy` (Zig passes it by value); `arena` is a raw `*const VxfwArena` so the raw
/// slices a draw allocates from it carry no borrow-checker lifetime (see the module
/// header's RAW-slice decision).
#[derive(Clone, Copy)]
pub struct DrawContext {
    /// Allocator backed by an arena. Widgets do not need to free their own resources, they will be
    /// freed after rendering
    pub arena: *const VxfwArena,
    /// Constraints
    pub min: Size,
    pub max: MaxSize,
    /// Size of a single cell, in pixels
    pub cell_size: Size,
}

impl DrawContext {
    pub fn init(method: Method) {
        WIDTH_METHOD.store(method_to_u8(method), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn string_width(&self, str: &[u8]) -> usize {
        gwidth::gwidth(
            str,
            method_from_u8(WIDTH_METHOD.load(std::sync::atomic::Ordering::Relaxed)),
        ) as usize
    }

    pub fn grapheme_iterator<'s>(&self, str: &'s [u8]) -> GraphemeIterator<'s> {
        unicode::grapheme_iterator(str)
    }

    pub fn with_constraints(&self, min: Size, max: MaxSize) -> DrawContext {
        DrawContext {
            arena: self.arena,
            min,
            max,
            cell_size: self.cell_size,
        }
    }
}

// ===========================================================================
// Widget — the type-erased interface
// ===========================================================================

/// A capture/event handler fn pointer: `?*const fn(userdata, *EventContext, Event) anyerror!void`.
pub type EventFn<'a> = fn(*mut c_void, &mut EventContext<'a>, Event<'a>) -> Result<(), Error>;
/// The draw fn pointer: `*const fn(userdata, DrawContext) Allocator.Error!Surface`.
pub type DrawFn<'a> = fn(*mut c_void, DrawContext) -> Result<Surface<'a>, Error>;

/// The Widget interface
#[derive(Clone, Copy)]
pub struct Widget<'a> {
    pub userdata: *mut c_void,
    pub capture_handler: Option<EventFn<'a>>,
    pub event_handler: Option<EventFn<'a>>,
    pub draw_fn: DrawFn<'a>,
}

impl<'a> Widget<'a> {
    pub fn capture_event(self, ctx: &mut EventContext<'a>, event: Event<'a>) -> Result<(), Error> {
        if let Some(handle) = self.capture_handler {
            return handle(self.userdata, ctx, event);
        }
        Ok(())
    }

    pub fn handle_event(self, ctx: &mut EventContext<'a>, event: Event<'a>) -> Result<(), Error> {
        if let Some(handle) = self.event_handler {
            return handle(self.userdata, ctx, event);
        }
        Ok(())
    }

    pub fn draw(self, ctx: DrawContext) -> Result<Surface<'a>, Error> {
        (self.draw_fn)(self.userdata, ctx)
    }

    /// Returns true if the Widgets point to the same widget instance. To be considered the same,
    /// the userdata and drawFn fields must point to the same values in both widgets
    pub fn eql(self, other: Widget<'a>) -> bool {
        std::ptr::eq(self.userdata, other.userdata)
            && self.draw_fn as usize == other.draw_fn as usize
    }
}

// ===========================================================================
// Surface + SubSurface — RAW slices (see module header)
// ===========================================================================

/// An empty raw cell slice (`&.{}`).
pub fn empty_cells<'a>() -> *mut [Cell<'a>] {
    core::ptr::slice_from_raw_parts_mut(core::ptr::NonNull::<Cell<'a>>::dangling().as_ptr(), 0)
}

/// An empty raw sub-surface slice (`&.{}`).
pub fn empty_children<'a>() -> *mut [SubSurface<'a>] {
    core::ptr::slice_from_raw_parts_mut(
        core::ptr::NonNull::<SubSurface<'a>>::dangling().as_ptr(),
        0,
    )
}

/// `pub const Surface = struct { size, widget, cursor, buffer, children };`
///
/// `Copy`: every field is `Copy` (the raw slices are `*mut [T]`), faithful to Zig
/// passing `Surface` by value.
#[derive(Clone, Copy)]
pub struct Surface<'a> {
    /// Size of this surface
    pub size: Size,
    /// The widget this surface belongs to
    pub widget: Widget<'a>,
    /// Cursor state
    pub cursor: Option<CursorState>,
    /// Contents of this surface. Must be len == 0 or  len == size.width * size.height
    pub buffer: *mut [Cell<'a>],
    pub children: *mut [SubSurface<'a>],
}

impl<'a> Surface<'a> {
    pub fn empty(widget: Widget<'a>) -> Surface<'a> {
        Surface {
            size: Size::default(),
            widget,
            cursor: None,
            buffer: empty_cells(),
            children: empty_children(),
        }
    }

    /// Creates a slice of vaxis.Cell's equal to size.width * size.height
    // The arena raw ptr's validity is a documented precondition (the App's live,
    // pinned frame arena); marking the fn `unsafe` would cascade through the widget
    // draw signatures, which mirror Zig's non-unsafe `Surface.createBuffer`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn create_buffer(arena: *const VxfwArena, size: Size) -> *mut [Cell<'a>] {
        // SAFETY: `arena` is a live `VxfwArena` pinned for the frame (the App's arena).
        let n = size.width as usize * size.height as usize;
        let buffer: *mut [Cell<'a>] = unsafe { (*arena).alloc_slice::<Cell<'a>>(n) };
        // @memset(buffer, .{ .default = true });
        let default_cell = Cell {
            default: true,
            ..Cell::default()
        };
        // SAFETY: `buffer` is `n` uninitialized `Cell` slots from the arena; we
        // initialize every one before any read.
        unsafe {
            let ptr = buffer.cast::<Cell<'a>>();
            for i in 0..n {
                ptr.add(i).write(default_cell);
            }
        }
        buffer
    }

    pub fn init(arena: *const VxfwArena, widget: Widget<'a>, size: Size) -> Surface<'a> {
        Surface {
            size,
            widget,
            cursor: None,
            buffer: Surface::create_buffer(arena, size),
            children: empty_children(),
        }
    }

    pub fn init_with_children(
        arena: *const VxfwArena,
        widget: Widget<'a>,
        size: Size,
        children: *mut [SubSurface<'a>],
    ) -> Surface<'a> {
        Surface {
            size,
            widget,
            cursor: None,
            buffer: Surface::create_buffer(arena, size),
            children,
        }
    }

    pub fn write_cell(self, col: u16, row: u16, cell: Cell<'a>) {
        if self.size.width <= col {
            return;
        }
        if self.size.height <= row {
            return;
        }
        let i = (row as usize * self.size.width as usize) + col as usize;
        assert!(i < self.buffer.len());
        // SAFETY: `i` is in-bounds (asserted); the buffer slot is initialized (either
        // `create_buffer` memset it, or the widget owns the backing array).
        unsafe {
            self.buffer.cast::<Cell<'a>>().add(i).write(cell);
        }
    }

    pub fn read_cell(self, col: usize, row: usize) -> Cell<'a> {
        assert!(col < self.size.width as usize && row < self.size.height as usize);
        let i = (row * self.size.width as usize) + col;
        assert!(i < self.buffer.len());
        // SAFETY: `i` is in-bounds (asserted); the slot is initialized.
        unsafe { self.buffer.cast::<Cell<'a>>().add(i).read() }
    }

    /// Creates a new surface of the same width, with the buffer trimmed to a given height
    pub fn trim_height(self, height: u16) -> Surface<'a> {
        assert!(height <= self.size.height);
        let n = self.size.width as usize * height as usize;
        // self.buffer[0 .. self.size.width * height]
        let trimmed: *mut [Cell<'a>] =
            core::ptr::slice_from_raw_parts_mut(self.buffer.cast::<Cell<'a>>(), n);
        Surface {
            size: Size {
                width: self.size.width,
                height,
            },
            widget: self.widget,
            cursor: self.cursor,
            buffer: trimmed,
            children: self.children,
        }
    }

    /// Walks the Surface tree to produce a list of all widgets that intersect Point. Point will
    /// always be translated to local Surface coordinates. Asserts that this Surface does contain Point
    pub fn hit_test(self, list: &mut Vec<HitResult<'a>>, point: Point) -> Result<(), Error> {
        assert!(point.col < self.size.width && point.row < self.size.height);
        // Add this widget to the hit list if it has an event or capture handler
        if self.widget.event_handler.is_some() || self.widget.capture_handler.is_some() {
            list.push(HitResult {
                local: point,
                widget: self.widget,
            });
        }
        // SAFETY: `children` is a live raw slice (arena- or widget-owned); we take a
        // transient shared view to iterate. No `&mut` alias is outstanding here.
        let children = unsafe { &*self.children };
        for child in children {
            if !child.contains_point(point) {
                continue;
            }
            let child_point = Point {
                row: (point.row as i32 - child.origin.row) as u16,
                col: (point.col as i32 - child.origin.col) as u16,
            };
            child.surface.hit_test(list, child_point)?;
        }
        Ok(())
    }

    /// Copies all cells from Surface to Window
    pub fn render(self, win: Window<'a>, focused: Widget<'a>) {
        // render self first
        if self.buffer.len() > 0 {
            assert!(self.buffer.len() == self.size.width as usize * self.size.height as usize);
            // SAFETY: buffer is a live raw slice of `len` initialized cells.
            let buffer = unsafe { &*self.buffer };
            for (i, cell) in buffer.iter().enumerate() {
                let row = i / self.size.width as usize;
                let col = i % self.size.width as usize;
                win.write_cell(col as u16, row as u16, *cell);
            }
        }

        if let Some(cursor) = self.cursor {
            if self.widget.eql(focused) {
                win.show_cursor(cursor.col, cursor.row);
                win.set_cursor_shape(cursor.shape);
            }
        }

        // Sort children by z-index.
        // SAFETY (H2): the children raw slice is arena-backed and may also be
        // reachable via a `last_frame` Surface copy, but that copy is not
        // dereferenced while this transient `&mut` view lives (the sort completes
        // before render reads any child, and before any later `hit_test`). Proven
        // on mirihar-vxfw under Stacked + Tree Borrows.
        let children = unsafe { &mut *self.children };
        children.sort_by(|a, b| SubSurface::z_cmp(a, b));

        // for each child, we make a window and render to it
        for child in children.iter() {
            let child_win = win.child(ChildOptions {
                x_off: child.origin.col,
                y_off: child.origin.row,
                width: Some(child.surface.size.width),
                height: Some(child.surface.size.height),
                border: Default::default(),
            });
            child.surface.render(child_win, focused);
        }
    }

    /// Returns true if the surface satisfies a set of constraints
    pub fn satisfies_constraints(self, min: Size, max: Size) -> bool {
        self.size.width < max.width
            && self.size.width > min.width
            && self.size.height < max.height
            && self.size.height > min.height
    }
}

/// `pub const SubSurface = struct { origin, surface, z_index = 0 };`
#[derive(Clone, Copy)]
pub struct SubSurface<'a> {
    /// Origin relative to parent
    pub origin: RelativePoint,
    /// This surface
    pub surface: Surface<'a>,
    /// z-index relative to siblings
    pub z_index: u8,
}

impl<'a> SubSurface<'a> {
    /// `pub fn lessThan(_: void, lhs, rhs) bool { return lhs.z_index < rhs.z_index; }`
    /// as a stable `Ordering` comparator for `sort_by`.
    pub fn z_cmp(lhs: &SubSurface, rhs: &SubSurface) -> std::cmp::Ordering {
        lhs.z_index.cmp(&rhs.z_index)
    }

    /// Returns true if this SubSurface contains Point. Point must be in parent local units
    pub fn contains_point(self, point: Point) -> bool {
        (point.col as i32) >= self.origin.col
            && (point.row as i32) >= self.origin.row
            && (point.col as i32) < (self.origin.col + self.surface.size.width as i32)
            && (point.row as i32) < (self.origin.row + self.surface.size.height as i32)
    }
}

// ===========================================================================
// VxfwArena — the faithful image of std.heap.ArenaAllocator (+ reset(.free_all))
// ===========================================================================

/// A bump arena with `reset` (`reset(.free_all)`): allocations are handed out by
/// bumping a pointer into heap chunks; `reset` frees every chunk. It is the faithful
/// image of `std.heap.ArenaAllocator` used for `DrawContext.arena` (design decision).
/// `alloc_slice` takes `&self` (many allocations happen during a single draw pass via
/// the shared `*const VxfwArena`); `reset` takes `&mut self` (exclusive, between
/// frames). Memory-safety of the raw slices it hands out is proven on mirihar-vxfw.
pub struct VxfwArena {
    inner: UnsafeCell<ArenaInner>,
}

struct ArenaInner {
    chunks: Vec<Chunk>,
    /// byte offset into the last (active) chunk
    off: usize,
}

struct Chunk {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

const DEFAULT_CHUNK: usize = 64 * 1024;

impl VxfwArena {
    pub fn new() -> VxfwArena {
        VxfwArena {
            inner: UnsafeCell::new(ArenaInner {
                chunks: Vec::new(),
                off: 0,
            }),
        }
    }

    /// Allocate `n` contiguous `T` slots (uninitialized). Returns a raw fat pointer;
    /// the caller MUST initialize before reading (faithful to `arena.alloc(T, n)`
    /// returning undefined memory). `T: Copy` (the only shapes vxfw allocates —
    /// `Cell`/`SubSurface` — are `Copy` and need no drop).
    ///
    /// # Safety
    /// The returned slots outlive only until the next [`reset`](Self::reset) or drop.
    pub unsafe fn alloc_slice<T: Copy>(&self, n: usize) -> *mut [T] {
        let size = core::mem::size_of::<T>().checked_mul(n).unwrap();
        if size == 0 {
            return core::ptr::slice_from_raw_parts_mut(
                core::ptr::NonNull::<T>::dangling().as_ptr(),
                n,
            );
        }
        let align = core::mem::align_of::<T>();
        // SAFETY: `&self` is unique for the duration of this call (no reentrancy —
        // single-threaded draw); the UnsafeCell projection is sound.
        let inner = unsafe { &mut *self.inner.get() };
        let ptr = inner.bump(size, align);
        core::ptr::slice_from_raw_parts_mut(ptr.cast::<T>(), n)
    }

    /// `_ = arena.reset(.free_all);` — free every chunk. The arena stays usable.
    pub fn reset(&mut self) {
        let inner = self.inner.get_mut();
        for chunk in inner.chunks.drain(..) {
            // SAFETY: `chunk.ptr`/`chunk.layout` were produced by `alloc` in `bump`.
            unsafe {
                std::alloc::dealloc(chunk.ptr, chunk.layout);
            }
        }
        inner.off = 0;
    }
}

impl Default for VxfwArena {
    fn default() -> Self {
        VxfwArena::new()
    }
}

impl Drop for VxfwArena {
    fn drop(&mut self) {
        self.reset();
    }
}

impl ArenaInner {
    /// Bump `size` bytes at alignment `align`; provenance is preserved by deriving the
    /// returned pointer via `chunk.ptr.add(offset)` (never int→ptr).
    fn bump(&mut self, size: usize, align: usize) -> *mut u8 {
        if let Some(chunk) = self.chunks.last() {
            let start = chunk.ptr;
            // pad the current offset up to `align`
            let misalign = (start as usize).wrapping_add(self.off) & (align - 1);
            let pad = if misalign == 0 { 0 } else { align - misalign };
            let aligned_off = self.off + pad;
            if aligned_off + size <= chunk.layout.size() {
                self.off = aligned_off + size;
                // SAFETY: `aligned_off + size` is within the chunk (checked); the
                // add preserves `chunk.ptr`'s provenance.
                return unsafe { start.add(aligned_off) };
            }
        }
        // Need a fresh chunk large enough for this allocation.
        let chunk_size = size.max(DEFAULT_CHUNK);
        let chunk_align = align.max(16);
        let layout = std::alloc::Layout::from_size_align(chunk_size, chunk_align).unwrap();
        // SAFETY: `layout` has non-zero size.
        let ptr = unsafe { std::alloc::alloc(layout) };
        assert!(!ptr.is_null(), "vxfw arena OOM");
        self.chunks.push(Chunk { ptr, layout });
        // The fresh chunk base is `chunk_align`-aligned (>= `align`), so off starts at 0.
        self.off = size;
        ptr
    }
}

// ===========================================================================
// helpers
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: a dummy widget whose handlers/draw are never called by these tests
    // (Zig's `.widget = undefined`). We need a valid `Widget` value, so point at a
    // no-op draw fn and null userdata.
    fn dummy_draw(_: *mut c_void, _: DrawContext) -> Result<Surface<'static>, Error> {
        unreachable!()
    }
    fn dummy_widget() -> Widget<'static> {
        Widget {
            userdata: core::ptr::null_mut(),
            capture_handler: None,
            event_handler: None,
            draw_fn: dummy_draw,
        }
    }

    // test "SubSurface: containsPoint"
    #[test]
    fn subsurface_contains_point() {
        let surf = SubSurface {
            origin: RelativePoint { row: 2, col: 2 },
            surface: Surface {
                size: Size {
                    width: 10,
                    height: 10,
                },
                widget: dummy_widget(),
                cursor: None,
                children: empty_children(),
                buffer: empty_cells(),
            },
            z_index: 0,
        };

        assert!(surf.contains_point(Point { row: 2, col: 2 }));
        assert!(surf.contains_point(Point { row: 3, col: 3 }));
        assert!(surf.contains_point(Point { row: 11, col: 11 }));

        assert!(!surf.contains_point(Point { row: 1, col: 1 }));
        assert!(!surf.contains_point(Point { row: 12, col: 12 }));
        assert!(!surf.contains_point(Point { row: 2, col: 12 }));
        assert!(!surf.contains_point(Point { row: 12, col: 2 }));
    }

    // test "Surface: satisfiesConstraints"
    #[test]
    fn surface_satisfies_constraints() {
        let surf = Surface {
            size: Size {
                width: 10,
                height: 10,
            },
            widget: dummy_widget(),
            cursor: None,
            children: empty_children(),
            buffer: empty_cells(),
        };

        assert!(surf.satisfies_constraints(
            Size {
                width: 1,
                height: 1
            },
            Size {
                width: 20,
                height: 20
            }
        ));
        assert!(!surf.satisfies_constraints(
            Size {
                width: 10,
                height: 10
            },
            Size {
                width: 20,
                height: 20
            }
        ));
        assert!(!surf.satisfies_constraints(
            Size {
                width: 1,
                height: 1
            },
            Size {
                width: 10,
                height: 10
            }
        ));
    }
}
