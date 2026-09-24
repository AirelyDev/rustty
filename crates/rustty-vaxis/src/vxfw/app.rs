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
    Command, CommandList, DrawContext, Error, Event, EventContext, HitResult, MaxSize, Phase,
    Point, Size, SubSurface, Surface, Tick, VxfwArena, Widget,
};
use crate::key::KittyFlags;
use crate::loop_::Loop;
use crate::mouse::Mouse;
use crate::vaxis::Options as VaxisOptions;
use crate::window::ChildOptions;
use crate::{Tty, Vaxis};

/// `App = @This();` — the reference struct. `io` and `allocator` are dropped (Rust `Vec`s
/// own their memory); `timers` and `wants_focus` live in the run-scope [`AppRun`] (see
/// the module ALIASING DISCIPLINE). libvaxis 0.6.0 removed 0.5.1's `buffer: [1024]u8`
/// field: the caller now passes the tty write buffer to `init`.
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
    /// PORT-NOTE: libvaxis 0.6.0 `init(io, allocator, env_map, buffer)`: `.tty = try
    /// vaxis.Tty.init(io, buffer)`, then `vaxis.init(io, allocator, env_map, .{
    /// .system_clipboard_allocator = allocator, .kitty_keyboard_flags = .{ .report_events =
    /// true } })` → `Vaxis::init(Options)` (rustty drops `io`, `alloc` and `env_map`, see
    /// `Vaxis::enable_detected_features`; `system_clipboard: bool` replaces the allocator
    /// presence). The field order of the literal is the evaluation order, so the tty is
    /// opened first, as in the reference.
    pub fn init(buffer: Vec<u8>) -> Result<App<'a>, Error> {
        let tty = Tty::init(buffer).map_err(|_| std::io::Error::other("vaxis Tty::init failed"))?;
        let vx = Vaxis::init(VaxisOptions {
            system_clipboard: true,
            kitty_keyboard_flags: KittyFlags {
                report_events: true,
                ..KittyFlags::default()
            },
        });
        Ok(App { tty, vx })
    }

    pub fn deinit(mut self) {
        // self.vx.deinit(allocator, self.tty.writer());  self.tty.deinit();
        let writer = self.tty.writer();
        self.vx.deinit(writer);
        // `tty.deinit()` consumes the Tty (tcsetattr TCSAFLUSH termios restore +
        // close on non-macOS), so `deinit` consumes `self` and moves the field out
        // (App has no Drop impl). `vx.deinit` only emits reset escape sequences —
        // it does NOT undo raw mode; skipping this left the terminal raw
        // (W17-B05-NORMAL-01).
        self.tty.deinit();
    }

    pub fn run(&mut self, widget: Widget<'a>, opts: Options) -> Result<(), Error> {
        // Derive the stable raw pointers ONCE; all tty/vx access flows through these.
        let tty_ptr: *mut Tty = &raw mut self.tty;
        let vx_ptr: *mut Vaxis<'a> = &raw mut self.vx;

        // var loop: EventLoop = .init(self.io, tty, vx);
        let loop_ = Loop::<Event<'static>>::init(tty_ptr, vx_ptr);
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
            (*vx_ptr).query_terminal((*tty_ptr).writer(), std::time::Duration::from_secs(1))?;
            (*vx_ptr).set_bracketed_paste((*tty_ptr).writer(), true)?;
            (*vx_ptr).subscribe_to_color_scheme_updates((*tty_ptr).writer())?;
        }

        {
            // This part deserves a comment. loop.installResizeHandler installs
            // a signal handler for the tty. We wait to installResizeHandler the
            // loop until we know if we need this handler. We don't need it if the
            // terminal supports in-band-resize.
            if !unsafe { (*vx_ptr).state.in_band_resize } {
                loop_.install_resize_handler().map_err(|_| {
                    std::io::Error::other("vaxis Loop::installResizeHandler failed")
                })?;
            }
        }

        // NOTE: We don't use pixel mouse anywhere
        unsafe {
            (*vx_ptr).caps.sgr_pixels = false;
            (*vx_ptr).set_mouse_mode((*tty_ptr).writer(), true)?;
        }

        DrawContext::init(unsafe { (*vx_ptr).screen.width_method });

        // Calculate tick rate
        let framerate: u64 = if opts.framerate > 0 {
            opts.framerate as u64
        } else {
            60
        };
        // const tick: std.Io.Duration = .fromNanoseconds(@divFloor(std.time.ns_per_s, framerate));
        let tick = std::time::Duration::from_nanos(1_000_000_000 / framerate);

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
        // (`std.Io.Timestamp.now(self.io, .awake)`: the monotonic clock, as `Instant`.)
        let mut next_frame = std::time::Instant::now();

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
                let now = std::time::Instant::now();
                // const duration = now.durationTo(next_frame);
                // if (duration.nanoseconds <= 0) {
                if next_frame <= now {
                    // Deadline exceeded. Schedule the next frame
                    next_frame = now + tick;
                } else {
                    // Sleep until the deadline
                    std::thread::sleep(next_frame - now);
                    next_frame += tick;
                }

                app.check_timers(&mut ctx)?;

                // Drain the events currently available (Zig locks the queue and
                // `drain`s; `try_event` pops each available event — same effect).
                while let Some(event) = loop_.try_event() {
                    // defer resetEventState(&ctx);
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
                    reset_event_state(&mut ctx);
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
        // const now: std.Io.Timestamp = .now(self.io, .awake);
        let now = std::time::Instant::now();

        // timers are always sorted descending
        while let Some(tick) = self.timers.pop() {
            // const duration = now.durationTo(tick.deadline);
            // if (duration.nanoseconds > 0) {
            if tick.deadline > now {
                // re-add the timer as no more timers will trigger now
                self.timers.push(tick);
                break;
            }
            reset_event_state(ctx);
            ctx.phase = Phase::AtTarget;
            tick.widget.handle_event(ctx, Event::Tick)?;
            reset_event_state(ctx);
        }
        self.handle_command(&mut ctx.cmds)?;
        Ok(())
    }
}

/// `fn resetEventState(ctx: *vxfw.EventContext) void` (libvaxis 0.6.0; 0.5.1 inlined it
/// as a `defer` in the event loop).
fn reset_event_state(ctx: &mut EventContext) {
    ctx.consume_event = false;
    ctx.phase = Phase::Capturing;
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
/// `const mouse_point: vxfw.Point = .{ .row = @intCast(mouse.row), .col = @intCast(mouse.col) };`
///
/// PORT NOTE: reference defect (R13, UD263). libvaxis 0.6.0 made `Mouse.col`/`row` `i16`
/// so that negative positions can be reported, but `MouseHandler` still `@intCast`s them to
/// the `u16` `vxfw.Point`: a negative position traps in safe builds (undefined behaviour
/// in ReleaseFast). The parser produces one from any SGR report with a coordinate of 0 or
/// below (`CSI < 0 ; 0 ; 0 M` is column -1, MEASURED with libvaxis 0.6.0), which a
/// terminal sends while the pointer is dragged outside the window, and which a
/// buggy or hostile terminal (or typed input) can send at any time; `+boo` enables mouse
/// reporting. A negative position is outside every surface, which is what the
/// reference's own `containsPoint` guard decides for any other point outside the frame,
/// so the port returns `None` and the caller hit-tests nothing (the widgets last under
/// the mouse get their `mouse_leave`, as when the mouse moves off the frame).
fn mouse_point(mouse: Mouse) -> Option<Point> {
    Some(Point {
        row: u16::try_from(mouse.row).ok()?,
        col: u16::try_from(mouse.col).ok()?,
    })
}

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
        // const mouse_point: vxfw.Point = .{ .row = @intCast(mouse.row), .col = @intCast(mouse.col) };
        if let Some(mouse_point) = mouse_point(mouse) {
            if sub.contains_point(mouse_point) {
                last_frame.hit_test(&mut hits, mouse_point)?;
            }
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
        // const mouse_point: vxfw.Point = .{ .row = @intCast(mouse.row), .col = @intCast(mouse.col) };
        if let Some(mouse_point) = mouse_point(mouse) {
            if sub.contains_point(mouse_point) {
                last_frame.hit_test(&mut hits, mouse_point)?;
            }
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
            // `@intCast(u16 -> i16)`, checked as the reference checks it (R7). The local
            // point is the mouse position (an `i16`) minus the child origins on its path, so
            // it fits unless a child has a negative origin in a terminal more than 32767
            // columns or rows wide (not reached by the widgets `+boo` uses; S-10b report).
            m_local.col = i16::try_from(item.local.col).unwrap();
            m_local.row = i16::try_from(item.local.row).unwrap();
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
            // `@intCast(u16 -> i16)`, checked as the reference checks it (R7). The local
            // point is the mouse position (an `i16`) minus the child origins on its path, so
            // it fits unless a child has a negative origin in a terminal more than 32767
            // columns or rows wide (not reached by the widgets `+boo` uses; S-10b report).
            m_local.col = i16::try_from(target.local.col).unwrap();
            m_local.row = i16::try_from(target.local.row).unwrap();
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
            // `@intCast(u16 -> i16)`, checked as the reference checks it (R7). The local
            // point is the mouse position (an `i16`) minus the child origins on its path, so
            // it fits unless a child has a negative origin in a terminal more than 32767
            // columns or rows wide (not reached by the widgets `+boo` uses; S-10b report).
            m_local.col = i16::try_from(item.local.col).unwrap();
            m_local.row = i16::try_from(item.local.row).unwrap();
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

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// PORT NOTE (rustty-only, W17-B05-NORMAL-01): the reference `App.deinit`
    /// (vxfw/App.zig) calls BOTH `self.vx.deinit(...)` AND `self.tty.deinit()`.
    /// The port used to stop after `vx.deinit` (escape-sequence resets only),
    /// never restoring termios — `rustty +boo` left the terminal raw. Under
    /// cfg(test) `Tty = TestTty`, whose `deinit_observer` hook records that
    /// `deinit` ran; this test fails without the `self.tty.deinit()` call.
    /// libvaxis 0.6.0 `test "timer consume does not leak to the next event"`. The
    /// reference builds an `App` with `.tty = undefined, .vx = undefined`; here the
    /// run-scope [`AppRun`] (which owns the timers) gets null tty/vx pointers, which
    /// `check_timers` never dereferences because the tick queues no command.
    #[test]
    fn timer_consume_does_not_leak_to_the_next_event() {
        fn handle(
            _: *mut std::ffi::c_void,
            ctx: &mut EventContext<'_>,
            event: Event<'_>,
        ) -> Result<(), Error> {
            if matches!(event, Event::Tick) {
                ctx.consume_and_redraw();
            }
            Ok(())
        }
        fn draw<'a>(_: *mut std::ffi::c_void, _: DrawContext) -> Result<Surface<'a>, Error> {
            unreachable!()
        }

        let mut app = AppRun {
            tty: std::ptr::null_mut(),
            vx: std::ptr::null_mut(),
            timers: Vec::new(),
            wants_focus: None,
        };

        let mut userdata: u8 = 0;
        let widget = Widget {
            userdata: (&raw mut userdata).cast(),
            capture_handler: None,
            event_handler: Some(handle),
            draw_fn: draw,
        };

        let now = std::time::Instant::now();
        app.timers.push(Tick {
            // `now.addDuration(.fromMilliseconds(-1))`
            deadline: now - std::time::Duration::from_millis(1),
            widget,
        });

        let mut ctx = EventContext {
            phase: Phase::Capturing,
            cmds: CommandList::new(),
            consume_event: false,
            redraw: false,
            quit: false,
        };

        app.check_timers(&mut ctx).unwrap();

        assert!(ctx.redraw);
        assert!(!ctx.consume_event);
        assert_eq!(Phase::Capturing, ctx.phase);
    }

    /// UD263 (R13): a negative mouse position (libvaxis 0.6.0's `i16` coordinates; an SGR
    /// report of column 0 parses to -1) is outside every surface, where the reference's
    /// `@intCast` to the `u16` `vxfw.Point` traps.
    #[test]
    fn a_negative_mouse_position_hits_nothing() {
        let at = |col: i16, row: i16| Mouse {
            col,
            row,
            xoffset: 0,
            yoffset: 0,
            button: crate::mouse::Button::Left,
            mods: crate::mouse::Modifiers::default(),
            r#type: crate::mouse::Type::Press,
        };
        assert!(mouse_point(at(-1, 0)).is_none());
        assert!(mouse_point(at(0, -1)).is_none());
        assert!(mouse_point(at(i16::MIN, i16::MIN)).is_none());
        let p = mouse_point(at(3, 7)).expect("an on-screen point");
        assert_eq!((p.col, p.row), (3, 7));
        let p = mouse_point(at(i16::MAX, 0)).expect("the largest column");
        assert_eq!(p.col, 32767);
    }

    #[test]
    fn deinit_deinitializes_the_tty() {
        let observed = Arc::new(AtomicBool::new(false));
        let mut app = App::init(Vec::new()).expect("App::init");
        app.tty.deinit_observer = Some(Arc::clone(&observed));
        app.deinit();
        assert!(
            observed.load(Ordering::SeqCst),
            "App::deinit must call Tty::deinit (App.zig: vx.deinit then tty.deinit)"
        );
    }
}
