//! rustty-vaxis::vxfw::app — the vxfw `App` run loop.
//!
//! PORTS: `vaxis/src/vxfw/App.zig` — the event loop that drives a root `Widget`
//! over a `Tty`/`Vaxis`/`Loop`: frame pacing, the timer heap, command handling, the
//! `MouseHandler` (capture/target/bubble + mouse_enter/leave diffing) and the
//! `FocusHandler` (focus tree walk + capture/target/bubble).
//!
//! ALIASING DISCIPLINE (the C3b/loop_-lifecycle + mirihar-wire2 precedent):
//! the reference stores `tty`/`vx` as `App` fields and builds `vaxis.Loop` over
//! `&self.tty`/`&self.vx`; the Loop's read thread then holds those raw pointers for
//! its life. In Rust the sound image is:
//!  * `run` derives `tty_ptr`/`vx_ptr` ONCE (`&raw mut self.tty` / `&raw mut self.vx`)
//!    and EVERY subsequent tty/vx access — main thread and helpers — reborrows
//!    through those SAME pointers (`&mut *tty_ptr`), so each reborrow is a child of
//!    the reader's tag and never a `Unique` that pops it. We NEVER form a whole
//!    `&mut *self` (App) after the Loop is built (that would retag the tty/vx region
//!    and pop the reader), until `stop()` has joined the reader.
//!  * The run-scope state the handlers mutate lives in [`AppRun`], which holds
//!    `tty`/`vx` as RAW POINTERS (not by value): `&mut AppRun` retags only the raw-ptr
//!    fields + `timers`/`wants_focus`, never the `Tty`/`Vaxis` pointees. `MouseHandler`/
//!    `FocusHandler` therefore take `&mut AppRun` freely.
//!
//! This is the FFI+thread path — not Miri-executable (Miri can't run the tty
//! termios/ioctl + read thread); it rides the mirihar-wire2 proof (SB+TB) of exactly
//! this raw-ptr tty/vx/loop shape, plus the ASan zlib harness for the C calls.

use std::io::Write as _;

use super::{
    now_ms, Command, CommandList, DrawContext, Error, Event, EventContext, HitResult, MaxSize,
    Phase, Point, Size, SubSurface, Surface, Tick, VxfwArena, Widget,
};
use crate::key::KittyFlags;
use crate::loop_::Loop;
use crate::mouse::Mouse;
use crate::vaxis::Options as VaxisOptions;
use crate::window::ChildOptions;
use crate::{Tty, Vaxis};

/// `App = @This();` — the reference struct. `allocator` is dropped (Rust `Vec`s own
/// their memory); `buffer: [1024]u8` becomes the `Tty::init` scratch `Vec`.
pub struct App<'a> {
    pub tty: Tty,
    pub vx: Vaxis<'a>,
}

/// `pub const Options = struct { framerate: u8 = 60 };`
#[derive(Clone, Copy)]
pub struct Options {
    /// Frames per second
    pub framerate: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options { framerate: 60 }
    }
}

impl<'a> App<'a> {
    /// Create an application. We require stable pointers to do the set up, so this will create an App
    /// object on the heap. Call destroy when the app is complete to reset terminal state and release
    /// resources
    ///
    /// PORT-NOTE: `vaxis.init(allocator, .{ .system_clipboard_allocator = allocator,
    /// .kitty_keyboard_flags = .{ .report_events = true } })` → `Vaxis::init(Options)`
    /// (rustty drops the `alloc` arg; `system_clipboard: bool` replaces the allocator
    /// presence). `Tty::init(&app.buffer)` → `Tty::init(vec![0; 1024])`.
    pub fn init() -> Result<App<'a>, Error> {
        let vx = Vaxis::init(VaxisOptions {
            system_clipboard: true,
            kitty_keyboard_flags: KittyFlags {
                report_events: true,
                ..KittyFlags::default()
            },
        });
        let tty = Tty::init(vec![0u8; 1024])
            .map_err(|_| std::io::Error::other("vaxis Tty::init failed"))?;
        Ok(App { tty, vx })
    }

    pub fn deinit(&mut self) {
        // self.vx.deinit(allocator, self.tty.writer());  self.tty.deinit();
        let writer = self.tty.writer();
        self.vx.deinit(writer);
        // `tty.deinit()` consumes the Tty; performed by the caller dropping `self`
        // (Rust frees the tty on drop — the reset already happened via vx.deinit).
    }

    pub fn run(&mut self, widget: Widget<'a>, opts: Options) -> Result<(), Error> {
        // Derive the stable raw pointers ONCE; all tty/vx access flows through these.
        let tty_ptr: *mut Tty = &raw mut self.tty;
        let vx_ptr: *mut Vaxis<'a> = &raw mut self.vx;

        // var loop: EventLoop = .{ .tty = tty, .vaxis = vx };
        let loop_ = Loop::<Event<'static>>::new(tty_ptr, vx_ptr);
        // try loop.start();
        loop_.start()?;
        // NOTE: `defer loop.stop()` — we call stop() at every return path below
        // (Rust has no `defer`; the reader must be joined before the raw ptrs die).

        // Send the init event / always initialize the app with a focus event
        loop_.post_event(Event::Init);
        loop_.post_event(Event::FocusIn);

        // SAFETY: all these are reborrows through the SAME tty_ptr/vx_ptr the reader
        // holds — children of its tag, never popping it.
        unsafe {
            (*vx_ptr).enter_alt_screen((*tty_ptr).writer())?;
            (*vx_ptr).query_terminal((*tty_ptr).writer(), 1_000_000_000)?;
            (*vx_ptr).set_bracketed_paste((*tty_ptr).writer(), true)?;
            (*vx_ptr).subscribe_to_color_scheme_updates((*tty_ptr).writer())?;
        }

        // This part deserves a comment. loop.init installs a signal handler for the tty. We wait to
        // init the loop until we know if we need this handler. We don't need it if the terminal
        // supports in-band-resize
        if !unsafe { (*vx_ptr).state.in_band_resize } {
            loop_
                .init()
                .map_err(|_| std::io::Error::other("vaxis Loop::init failed"))?;
        }

        // NOTE: We don't use pixel mouse anywhere
        unsafe {
            (*vx_ptr).caps.sgr_pixels = false;
            (*vx_ptr).set_mouse_mode((*tty_ptr).writer(), true)?;
        }

        DrawContext::init(unsafe { (*vx_ptr).screen.width_method });

        let framerate: u64 = if opts.framerate > 0 {
            opts.framerate as u64
        } else {
            60
        };
        // Calculate tick rate
        let tick_ms: u64 = 1000 / framerate;

        // Set up arena and context
        let mut arena = VxfwArena::new();

        // The run-scope handler state, holding tty/vx as RAW pointers (see the
        // module ALIASING DISCIPLINE): `&mut AppRun` never retags the tty/vx pointees.
        let mut app = AppRun {
            tty: tty_ptr,
            vx: vx_ptr,
            timers: Vec::new(),
            wants_focus: None,
        };

        let mut mouse_handler = MouseHandler::init(widget);
        let mut focus_handler = FocusHandler::init(widget);
        focus_handler.path_to_focused.push(widget);

        // Timestamp of our next frame
        let mut next_frame_ms: u64 = now_ms() as u64;

        // Create our event context
        let mut ctx = EventContext {
            phase: Phase::Capturing,
            cmds: CommandList::new(),
            consume_event: false,
            redraw: false,
            quit: false,
        };

        let result = (|| -> Result<(), Error> {
            loop {
                let now: u64 = now_ms() as u64;
                if now >= next_frame_ms {
                    // Deadline exceeded. Schedule the next frame
                    next_frame_ms = now + tick_ms;
                } else {
                    // Sleep until the deadline
                    std::thread::sleep(std::time::Duration::from_millis(next_frame_ms - now));
                    next_frame_ms += tick_ms;
                }

                app.check_timers(&mut ctx)?;

                // Drain the events currently available (Zig locks the queue and
                // `drain`s; `try_event` pops each available event — same effect).
                while let Some(event) = loop_.try_event() {
                    // Reset our context after each event
                    let reset = |ctx: &mut EventContext| {
                        ctx.consume_event = false;
                        ctx.phase = Phase::Capturing;
                    };
                    match event {
                        Event::KeyPress(_) => {
                            focus_handler.handle_event(&mut ctx, event)?;
                            app.handle_command(&mut ctx.cmds)?;
                        }
                        Event::FocusOut => {
                            mouse_handler.mouse_exit(&mut app, &mut ctx)?;
                            focus_handler.handle_event(&mut ctx, Event::FocusOut)?;
                            app.handle_command(&mut ctx.cmds)?;
                        }
                        Event::FocusIn => {
                            focus_handler.handle_event(&mut ctx, Event::FocusIn)?;
                            app.handle_command(&mut ctx.cmds)?;
                        }
                        Event::Mouse(mouse) => {
                            mouse_handler.handle_mouse(&mut app, &mut ctx, mouse)?;
                        }
                        Event::Winsize(ws) => {
                            // SAFETY: reborrow through the reader's ptrs.
                            unsafe {
                                (*vx_ptr).resize((*tty_ptr).writer(), ws)?;
                            }
                            ctx.redraw = true;
                        }
                        other => {
                            focus_handler.handle_event(&mut ctx, other)?;
                            app.handle_command(&mut ctx.cmds)?;
                        }
                    }
                    reset(&mut ctx);
                }

                // If we have a focus change, handle that event before we layout
                if let Some(wants) = app.wants_focus {
                    focus_handler.focus_widget(&mut ctx, wants)?;
                    app.handle_command(&mut ctx.cmds)?;
                    app.wants_focus = None;
                }

                // Check if we should quit
                if ctx.quit {
                    return Ok(());
                }

                // Check if we need a redraw
                if !ctx.redraw {
                    continue;
                }
                ctx.redraw = false;
                // Clear the arena.
                arena.reset();
                // Assert that we have handled all commands
                assert!(ctx.cmds.is_empty());

                let surface: Surface = {
                    // Draw the root widget
                    let surface = app.do_layout(&arena, widget)?;

                    // Check if any hover or mouse effects changed
                    mouse_handler.update_mouse(&mut app, surface, &mut ctx)?;
                    // Our focus may have changed. Handle that here
                    if let Some(wants) = app.wants_focus {
                        focus_handler.focus_widget(&mut ctx, wants)?;
                        app.handle_command(&mut ctx.cmds)?;
                        app.wants_focus = None;
                    }

                    assert!(ctx.cmds.is_empty());
                    if !ctx.redraw {
                        surface
                    } else {
                        // If updating the mouse required a redraw, we do the layout again
                        app.do_layout(&arena, widget)?
                    }
                };

                // Store the last frame
                mouse_handler.last_frame = surface;
                // Update the focus handler list
                focus_handler.update(surface)?;
                app.render(surface, focus_handler.focused_widget)?;
            }
        })();

        // PORT NOTE: join the read thread before teardown (the reference relies on
        // process exit; Rust must stop/join so no `*mut tty`/`*mut vx` outlives run).
        loop_.stop();
        result
    }
}

/// The run-scope handler state. Holds `tty`/`vx` as RAW pointers so `&mut AppRun`
/// never retags the `Tty`/`Vaxis` pointees the reader thread aliases.
struct AppRun<'a> {
    tty: *mut Tty,
    vx: *mut Vaxis<'a>,
    /// `timers: std.ArrayList(vxfw.Tick)` (a run-scope local, not an App field, so
    /// the loop never forms `&mut *self`).
    timers: Vec<Tick<'a>>,
    /// `wants_focus: ?vxfw.Widget`
    wants_focus: Option<Widget<'a>>,
}

impl<'a> AppRun<'a> {
    fn do_layout(&mut self, arena: &VxfwArena, widget: Widget<'a>) -> Result<Surface<'a>, Error> {
        // SAFETY: reborrow through the reader's vx ptr.
        let vx = unsafe { &mut *self.vx };
        let width = vx.screen.width;
        let height = vx.screen.height;
        // cell_size = width_pix / width (guard div-by-zero before the first winsize;
        // the reference does not, but pre-resize screen is 0×0).
        let cell_w = if width == 0 {
            0
        } else {
            vx.screen.width_pix / width
        };
        let cell_h = if height == 0 {
            0
        } else {
            vx.screen.height_pix / height
        };
        let draw_context = DrawContext {
            arena: arena as *const VxfwArena,
            min: Size {
                width: 0,
                height: 0,
            },
            max: MaxSize {
                width: Some(width),
                height: Some(height),
            },
            cell_size: Size {
                width: cell_w,
                height: cell_h,
            },
        };
        widget.draw(draw_context)
    }

    fn render(&mut self, surface: Surface<'a>, focused_widget: Widget<'a>) -> Result<(), Error> {
        // SAFETY: reborrow through the reader's ptrs.
        let vx = unsafe { &mut *self.vx };
        let win = vx.window();
        win.clear();
        win.hide_cursor();
        win.set_cursor_shape(crate::cell::CursorShape::Default);

        let root_win = win.child(ChildOptions {
            x_off: 0,
            y_off: 0,
            width: Some(surface.size.width),
            height: Some(surface.size.height),
            border: Default::default(),
        });
        surface.render(root_win, focused_widget);

        unsafe {
            (*self.vx).render((*self.tty).writer())?;
            // (the reference relies on vx.render flushing; flush explicitly so the
            // buffered TtyWriter reaches the fd — mirrors list_themes.)
            (*self.tty).writer().flush()?;
        }
        Ok(())
    }

    fn add_tick(&mut self, tick: Tick<'a>) -> Result<(), Error> {
        self.timers.push(tick);
        // std.sort.insertion(Tick, items, {}, Tick.lessThan) — stable insertion sort.
        insertion_sort(&mut self.timers, |a, b| Tick::less_than(a, b));
        Ok(())
    }

    fn handle_command(&mut self, cmds: &mut CommandList<'a>) -> Result<(), Error> {
        // defer cmds.clearRetainingCapacity();
        // We drain the commands, processing each; on any error the remaining commands
        // are still cleared (drain empties the Vec).
        let drained: Vec<Command<'a>> = std::mem::take(cmds);
        for cmd in drained {
            match cmd {
                Command::Tick(tick) => self.add_tick(tick)?,
                Command::SetMouseShape(shape) => unsafe { (*self.vx).set_mouse_shape(shape) },
                Command::RequestFocus(widget) => self.wants_focus = Some(widget),
                Command::CopyToClipboard(content) => {
                    // SAFETY: reborrow through the reader's ptrs.
                    let r = unsafe {
                        (*self.vx).copy_to_system_clipboard((*self.tty).writer(), &content)
                    };
                    if let Err(e) = r {
                        eprintln!("copy error: {e}");
                    }
                }
                Command::SetTitle(title) => {
                    let r = unsafe { (*self.vx).set_title((*self.tty).writer(), &title) };
                    if let Err(e) = r {
                        eprintln!("set_title error: {e}");
                    }
                }
                Command::QueueRefresh => unsafe { (*self.vx).queue_refresh() },
                Command::Notify { title, body } => {
                    let r =
                        unsafe { (*self.vx).notify((*self.tty).writer(), title.as_deref(), &body) };
                    if let Err(e) = r {
                        eprintln!("notify error: {e}");
                    }
                    // title/body owned Vecs drop here (Zig frees them explicitly).
                }
                Command::QueryColor(kind) => {
                    let r = unsafe { (*self.vx).query_color((*self.tty).writer(), kind) };
                    if let Err(e) = r {
                        eprintln!("queryColor error: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    fn check_timers(&mut self, ctx: &mut EventContext<'a>) -> Result<(), Error> {
        let now = now_ms();

        // timers are always sorted descending; pop the soonest (last) deadline.
        while let Some(tick) = self.timers.pop() {
            if now < tick.deadline_ms {
                // re-add the timer
                self.timers.push(tick);
                break;
            }
            tick.widget.handle_event(ctx, Event::Tick)?;
        }
        self.handle_command(&mut ctx.cmds)?;
        Ok(())
    }
}

/// `std.sort.insertion` — a stable insertion sort by the `less_than` predicate.
fn insertion_sort<T: Copy>(items: &mut [T], less_than: impl Fn(&T, &T) -> bool) {
    let mut i = 1;
    while i < items.len() {
        let x = items[i];
        let mut j = i;
        while j > 0 && less_than(&x, &items[j - 1]) {
            items[j] = items[j - 1];
            j -= 1;
        }
        items[j] = x;
        i += 1;
    }
}

/// `MouseHandler` — hit-tests the last frame and dispatches mouse events through the
/// capture/target/bubble phases, diffing hit lists for mouse_enter/mouse_leave.
struct MouseHandler<'a> {
    last_frame: Surface<'a>,
    last_hit_list: Vec<HitResult<'a>>,
    mouse: Option<Mouse>,
}

impl<'a> MouseHandler<'a> {
    fn init(root: Widget<'a>) -> MouseHandler<'a> {
        MouseHandler {
            last_frame: Surface {
                size: Size {
                    width: 0,
                    height: 0,
                },
                widget: root,
                cursor: None,
                buffer: super::empty_cells(),
                children: super::empty_children(),
            },
            last_hit_list: Vec::new(),
            mouse: None,
        }
    }

    fn update_mouse(
        &mut self,
        app: &mut AppRun<'a>,
        surface: Surface<'a>,
        ctx: &mut EventContext<'a>,
    ) -> Result<(), Error> {
        let Some(mouse) = self.mouse else {
            return Ok(());
        };
        // For mouse events we store the last frame and use that for hit testing
        let last_frame = surface;

        let mut hits: Vec<HitResult<'a>> = Vec::new();
        let sub = SubSurface {
            origin: super::RelativePoint { row: 0, col: 0 },
            surface: last_frame,
            z_index: 0,
        };
        let mouse_point = Point {
            row: mouse.row,
            col: mouse.col,
        };
        if sub.contains_point(mouse_point) {
            last_frame.hit_test(&mut hits, mouse_point)?;
        }

        self.diff_hits(app, ctx, &hits)?;

        // Store a copy of this hit list for next frame
        self.last_hit_list = hits;
        Ok(())
    }

    /// The shared mouse_enter/mouse_leave diffing:
    /// - Widgets in a (last) but not in b (current) get a mouse_leave event
    /// - Widgets in b but not in a get a mouse_enter event
    /// - Widgets in both receive nothing
    fn diff_hits(
        &self,
        app: &mut AppRun<'a>,
        ctx: &mut EventContext<'a>,
        b: &[HitResult<'a>],
    ) -> Result<(), Error> {
        let a = &self.last_hit_list;
        // Find widgets in a but not b
        for a_item in a {
            let a_widget = a_item.widget;
            if !b.iter().any(|b_item| a_widget.eql(b_item.widget)) {
                a_widget.handle_event(ctx, Event::MouseLeave)?;
                app.handle_command(&mut ctx.cmds)?;
            }
        }
        // Widgets in b but not in a
        for b_item in b {
            let b_widget = b_item.widget;
            if !a.iter().any(|a_item| b_widget.eql(a_item.widget)) {
                b_widget.handle_event(ctx, Event::MouseEnter)?;
                app.handle_command(&mut ctx.cmds)?;
            }
        }
        Ok(())
    }

    fn handle_mouse(
        &mut self,
        app: &mut AppRun<'a>,
        ctx: &mut EventContext<'a>,
        mouse: Mouse,
    ) -> Result<(), Error> {
        // For mouse events we store the last frame and use that for hit testing
        let last_frame = self.last_frame;
        self.mouse = Some(mouse);

        let mut hits: Vec<HitResult<'a>> = Vec::new();
        let sub = SubSurface {
            origin: super::RelativePoint { row: 0, col: 0 },
            surface: last_frame,
            z_index: 0,
        };
        let mouse_point = Point {
            row: mouse.row,
            col: mouse.col,
        };
        if sub.contains_point(mouse_point) {
            last_frame.hit_test(&mut hits, mouse_point)?;
        }

        // Handle mouse_enter and mouse_leave events
        self.diff_hits(app, ctx, &hits)?;
        self.last_hit_list = hits.clone();

        let Some(target) = hits.pop() else {
            return Ok(());
        };

        // capturing phase
        ctx.phase = Phase::Capturing;
        for item in &hits {
            let mut m_local = mouse;
            m_local.col = item.local.col;
            m_local.row = item.local.row;
            item.widget.capture_event(ctx, Event::Mouse(m_local))?;
            app.handle_command(&mut ctx.cmds)?;
            if ctx.consume_event {
                return Ok(());
            }
        }

        // target phase
        ctx.phase = Phase::AtTarget;
        {
            let mut m_local = mouse;
            m_local.col = target.local.col;
            m_local.row = target.local.row;
            target.widget.handle_event(ctx, Event::Mouse(m_local))?;
            app.handle_command(&mut ctx.cmds)?;
            if ctx.consume_event {
                return Ok(());
            }
        }

        // Bubbling phase
        ctx.phase = Phase::Bubbling;
        while let Some(item) = hits.pop() {
            let mut m_local = mouse;
            m_local.col = item.local.col;
            m_local.row = item.local.row;
            item.widget.handle_event(ctx, Event::Mouse(m_local))?;
            app.handle_command(&mut ctx.cmds)?;
            if ctx.consume_event {
                return Ok(());
            }
        }
        Ok(())
    }

    /// sends .mouse_leave to all of the widgets from the last_hit_list
    fn mouse_exit(
        &mut self,
        app: &mut AppRun<'a>,
        ctx: &mut EventContext<'a>,
    ) -> Result<(), Error> {
        for item in &self.last_hit_list {
            item.widget.handle_event(ctx, Event::MouseLeave)?;
            app.handle_command(&mut ctx.cmds)?;
        }
        Ok(())
    }
}

/// Maintains a tree of focusable nodes. Delivers events to the currently focused node, walking up
/// the tree until the event is handled
struct FocusHandler<'a> {
    root: Widget<'a>,
    focused_widget: Widget<'a>,
    path_to_focused: Vec<Widget<'a>>,
}

impl<'a> FocusHandler<'a> {
    fn init(root: Widget<'a>) -> FocusHandler<'a> {
        FocusHandler {
            root,
            focused_widget: root,
            path_to_focused: Vec::new(),
        }
    }

    /// Update the focus list
    fn update(&mut self, surface: Surface<'a>) -> Result<(), Error> {
        // clear path
        self.path_to_focused.clear();

        // Find the path to the focused widget. This builds a list that has the first element as the
        // focused widget, and walks backward to the root. It's possible our focused widget is *not*
        // in this tree. If this is the case, we refocus to the root widget
        self.child_has_focus(surface)?;

        if !self.root.eql(surface.widget) {
            // If the root of surface is not the initial widget, we append the initial widget
            self.path_to_focused.push(self.root);
        }

        // reverse path_to_focused so that it is root first
        self.path_to_focused.reverse();
        Ok(())
    }

    /// Returns true if a child of surface is the focused widget
    fn child_has_focus(&mut self, surface: Surface<'a>) -> Result<bool, Error> {
        // Check if we are the focused widget
        if self.focused_widget.eql(surface.widget) {
            self.path_to_focused.push(surface.widget);
            return Ok(true);
        }
        // SAFETY: transient shared view of the arena-backed children slice.
        let children = unsafe { &*surface.children };
        for child in children {
            // Add child to list if it is the focused widget or one of it's own children is
            if self.child_has_focus(child.surface)? {
                self.path_to_focused.push(surface.widget);
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn focus_widget(
        &mut self,
        ctx: &mut EventContext<'a>,
        widget: Widget<'a>,
    ) -> Result<(), Error> {
        // Focusing a widget requires it to have an event handler
        assert!(widget.event_handler.is_some());
        if self.focused_widget.eql(widget) {
            return Ok(());
        }

        ctx.phase = Phase::AtTarget;
        self.focused_widget.handle_event(ctx, Event::FocusOut)?;
        self.focused_widget = widget;
        self.focused_widget.handle_event(ctx, Event::FocusIn)?;
        Ok(())
    }

    fn handle_event(&mut self, ctx: &mut EventContext<'a>, event: Event<'a>) -> Result<(), Error> {
        let path = &self.path_to_focused;
        assert!(!path.is_empty());

        // Capturing phase. We send capture events from the root to the target (inclusive of target)
        ctx.phase = Phase::Capturing;
        for widget in path {
            widget.capture_event(ctx, event.clone())?;
            if ctx.consume_event {
                return Ok(());
            }
        }

        // Target phase. This is only sent to the target
        ctx.phase = Phase::AtTarget;
        let target = *path.last().unwrap();
        target.handle_event(ctx, event.clone())?;
        if ctx.consume_event {
            return Ok(());
        }

        // Bubbling phase. Bubbling phase moves from target (exclusive) to the root
        ctx.phase = Phase::Bubbling;
        let target_idx = path.len() - 1;
        for widget in path[0..target_idx].iter().rev() {
            widget.handle_event(ctx, event.clone())?;
            if ctx.consume_event {
                return Ok(());
            }
        }
        Ok(())
    }
}
