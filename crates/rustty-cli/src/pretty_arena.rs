//! rustty-cli::pretty_arena
//!
//! The faithful analogue of the `std.heap.ArenaAllocator` the reference
//! `list_colors`/`list_keybinds` `prettyPrint` threads into its render body
//! (`arena.allocator()`): a bump-lifetime byte arena whose duped slices stay
//! valid for the arena's whole lifetime.
//!
//! WHY IT EXISTS (WIRE1): the vaxis `Window` stores each printed `Segment`'s
//! `text: &'a [u8]` BY REFERENCE into the screen's `Cell<'a>` buffer (§5). So the
//! transient bytes the reference builds with `std.fmt.allocPrint(alloc, ...)`
//! (the `#rrggbb` hex, the `{t}`/`{u}` key strings, the `{f}` action strings)
//! must outlive the screen — exactly what the reference arena guarantees. Each
//! `allocPrint(alloc, ...)` maps 1:1 to an [`Arena::dupe`], and the arena is
//! dropped after the `Vaxis` (declared before it), mirroring the reference
//! `defer arena.deinit()` firing after the vx/tty defers.

use crate::rustty::Error;
use rustty_vaxis::cell::{Segment, Style};
use rustty_vaxis::window::PrintOptions;
use rustty_vaxis::Winsize;
use std::cell::RefCell;

// const winsize: vaxis.Winsize = switch (builtin.os.tag) { ... };
//   PORT NOTE: the reference's `builtin.os.tag` switch — `.windows` uses a fixed
//   default (we don't wrap, so it doesn't matter), `else` reads the real winsize.
//   Shared verbatim by both `list_colors` and `list_keybinds` `prettyPrint`.
pub fn winsize_for(tty: &rustty_vaxis::Tty) -> Result<Winsize, Error> {
    // We use some default, it doesn't really matter for what
    // we're doing because we don't do any wrapping.
    // .windows => .{ .rows = 24, .cols = 120, .x_pixel = 1024, .y_pixel = 768 },
    #[cfg(windows)]
    {
        let _ = tty;
        Ok(Winsize {
            rows: 24,
            cols: 120,
            x_pixel: 1024,
            y_pixel: 768,
        })
    }
    // else => try vaxis.Tty.getWinsize(tty.fd),
    #[cfg(not(windows))]
    {
        Ok(rustty_vaxis::Tty::get_winsize(tty.fd())?)
    }
}

/// The tty operations the `prettyPrint` device lifecycle needs. Implemented by
/// the real `rustty_vaxis::Tty`; the lifecycle is generic over it so its
/// cleanup order can be tested with failures injected.
pub trait PrettyTty {
    type Writer: std::io::Write;
    /// `tty.writer()`.
    fn writer(&mut self) -> &mut Self::Writer;
    /// The `winsize` switch: `.windows` default, else `try tty.getWinsize()`.
    fn winsize(&self) -> Result<Winsize, Error>;
    /// `tty.deinit()`.
    fn deinit(self);
}

impl PrettyTty for rustty_vaxis::Tty {
    type Writer = rustty_vaxis::tty::TtyWriter;
    fn writer(&mut self) -> &mut Self::Writer {
        rustty_vaxis::Tty::writer(self)
    }
    fn winsize(&self) -> Result<Winsize, Error> {
        winsize_for(self)
    }
    fn deinit(self) {
        rustty_vaxis::Tty::deinit(self)
    }
}

/// The device lifecycle shared by `list_colors` and `list_keybinds`
/// `prettyPrint`, from just after `vaxis.init` to the end of the function:
///
/// ```zig
/// var tty = try vaxis.Tty.init(global.io(), &buf);
/// defer tty.deinit();
/// var vx = try vaxis.init(global.io(), alloc, &env_map, .{});
/// defer vx.deinit(alloc, tty.writer());
/// vx.caps.unicode = .unicode;
/// try tty.writer().writeAll(vaxis.ctlseqs.unicode_set);
/// defer tty.writer().writeAll(vaxis.ctlseqs.unicode_reset) catch {};
/// const winsize: vaxis.Winsize = switch (builtin.os.tag) { ... else => try tty.getWinsize() };
/// <body: try vx.resize(...) ... >
/// ```
///
/// Each `defer` runs on every exit after the point it is registered, LIFO:
/// a failed `unicode_set` write runs `vx.deinit` then `tty.deinit`; anything
/// later runs `unicode_reset`, `vx.deinit`, `tty.deinit`.
//
// PORT NOTE (parity S68-F1, S68-F2, task T-C): the two callers used to `?` the
// `unicode_set` write and the `getWinsize` call BEFORE their hand-placed cleanup
// block, so an error there returned with the terminal still raw and `/dev/tty`
// open (`PosixTty` has no `Drop`). Both now go through this function.
pub fn pretty_device_lifecycle<'a, T, F>(
    mut tty: T,
    vx: &mut rustty_vaxis::Vaxis<'a>,
    body: F,
) -> Result<(), Error>
where
    T: PrettyTty,
    F: FnOnce(&mut T::Writer, &mut rustty_vaxis::Vaxis<'a>, Winsize) -> std::io::Result<()>,
{
    use std::io::Write;
    // vx.caps.unicode = .unicode;
    vx.caps.unicode = rustty_vaxis::gwidth::Method::Unicode;
    // try tty.writer().writeAll(vaxis.ctlseqs.unicode_set);
    let res = match tty.writer().write_all(rustty_vaxis::ctlseqs::unicode_set) {
        Err(e) => Err(Error::from(e)),
        Ok(()) => {
            // const winsize = ... try tty.getWinsize();  then the body.
            let res = match tty.winsize() {
                Ok(winsize) => body(tty.writer(), vx, winsize).map_err(Error::from),
                Err(e) => Err(e),
            };
            // defer tty.writer().writeAll(vaxis.ctlseqs.unicode_reset) catch {};
            let _ = tty.writer().write_all(rustty_vaxis::ctlseqs::unicode_reset);
            res
        }
    };
    // defer vx.deinit(alloc, tty.writer());
    vx.deinit(tty.writer());
    // defer tty.deinit();
    tty.deinit();
    res
}

/// `.{ .text = t }` — a bare segment (default style + no hyperlink).
pub fn seg(text: &[u8]) -> Segment<'_> {
    Segment {
        text,
        style: Style::default(),
        link: Default::default(),
    }
}

/// `.{ .text = t, .style = s }` — a styled segment.
pub fn styled(text: &[u8], style: Style) -> Segment<'_> {
    Segment {
        text,
        style,
        link: Default::default(),
    }
}

/// `.{ .col_offset = c }` — the `printSegment`/`print` position option.
pub fn po(col: u16) -> PrintOptions {
    PrintOptions {
        col_offset: col,
        ..Default::default()
    }
}

/// A bump-lifetime byte arena. `dupe` copies bytes into an owned buffer and
/// returns a reference valid for the arena's lifetime.
#[derive(Default)]
pub struct Arena {
    // Each duped slice is a separate `Vec<u8>` heap buffer; the outer `Vec` only
    // stores the inner `Vec` handles. Growing the outer `Vec` MOVES the inner
    // `Vec` handles but never their heap buffers, and — unlike `Box` — moving a
    // `Vec` does not retag its pointee, so outstanding `dupe` references stay
    // valid under Stacked Borrows.
    chunks: RefCell<Vec<Vec<u8>>>,
    // `alloc`'s values: each is its own `Box` allocation, held as a raw pointer
    // (moving a raw pointer retags nothing) with the function that frees it.
    objects: RefCell<Vec<ArenaObject>>,
}

struct ArenaObject {
    ptr: *mut (),
    free: unsafe fn(*mut ()),
}

impl Drop for Arena {
    fn drop(&mut self) {
        for object in self.objects.get_mut().drain(..) {
            // SAFETY: `ptr` came from `Box::into_raw` of the type `free` was
            // instantiated with, and is freed exactly once, here; every
            // `&'a mut` handed out by `alloc` has ended because `'a` cannot
            // outlive the arena.
            unsafe { (object.free)(object.ptr) }
        }
    }
}

impl Arena {
    // var arena = std.heap.ArenaAllocator.init(alloc);
    pub fn init() -> Arena {
        Arena {
            chunks: RefCell::new(Vec::new()),
            objects: RefCell::new(Vec::new()),
        }
    }

    /// `arena.allocator().create(T)` + store: move `value` into the arena and
    /// return it, living as long as the arena. Used for a per-frame widget
    /// copy that the screen borrows for the arena's lifetime (`list_themes`'
    /// search box, parity S69-F1).
    #[allow(clippy::mut_from_ref)] // each call returns a distinct allocation
    pub fn alloc<T: 'static>(&self, value: T) -> &mut T {
        unsafe fn free<T>(ptr: *mut ()) {
            // SAFETY: the caller (`Drop`) passes a pointer `alloc::<T>` made
            // with `Box::into_raw`, once.
            drop(unsafe { Box::from_raw(ptr.cast::<T>()) });
        }
        let ptr: *mut T = Box::into_raw(Box::new(value));
        self.objects.borrow_mut().push(ArenaObject {
            ptr: ptr.cast(),
            free: free::<T>,
        });
        // SAFETY: `ptr` is a fresh, unaliased heap allocation that the arena
        // frees only in its own `Drop`, after this borrow has ended.
        unsafe { &mut *ptr }
    }

    /// `std.fmt.allocPrint(arena.allocator(), ...)` / `arena.allocator().dupe(...)`:
    /// copy `bytes` into the arena and return a reference living as long as the arena.
    pub fn dupe<'a>(&'a self, bytes: &[u8]) -> &'a [u8] {
        let owned: Vec<u8> = bytes.to_vec();
        // Raw pointer to the heap buffer (stable for the arena's lifetime).
        let ptr: *const [u8] = owned.as_slice();
        self.chunks.borrow_mut().push(owned);
        // SAFETY: `owned`'s heap buffer that `ptr` points at is not moved when the
        // `Vec` handle is pushed into `chunks` (only the 3-word handle moves, and
        // moving a `Vec` — not a `Box` — performs no `noalias` retag of the
        // pointee). Later `dupe`s may reallocate the outer `Vec`, but that again
        // only moves inner handles, never the buffers. The arena never frees a
        // buffer before its own `Drop`, which happens after the borrow `'a` ends.
        // This is the standard typed-arena chunk pattern; Stacked-Borrows / Miri clean.
        unsafe { &*ptr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vaxis::ctlseqs;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Everything the lifecycle does to the tty, in order.
    #[derive(Debug, PartialEq, Eq)]
    enum Ev {
        Write(Vec<u8>),
        Winsize,
        Body,
        TtyDeinit,
    }

    struct FakeWriter {
        log: Rc<RefCell<Vec<Ev>>>,
        fail_unicode_set: bool,
    }
    impl std::io::Write for FakeWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_unicode_set && buf == ctlseqs::unicode_set {
                return Err(std::io::Error::other("write failed"));
            }
            self.log.borrow_mut().push(Ev::Write(buf.to_vec()));
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FakeTty {
        w: FakeWriter,
        fail_winsize: bool,
        deinits: Rc<Cell<u32>>,
    }
    impl PrettyTty for FakeTty {
        type Writer = FakeWriter;
        fn writer(&mut self) -> &mut FakeWriter {
            &mut self.w
        }
        fn winsize(&self) -> Result<Winsize, Error> {
            self.w.log.borrow_mut().push(Ev::Winsize);
            if self.fail_winsize {
                return Err(Error::from(std::io::Error::other("no winsize")));
            }
            Ok(Winsize {
                rows: 24,
                cols: 80,
                x_pixel: 0,
                y_pixel: 0,
            })
        }
        fn deinit(self) {
            self.deinits.set(self.deinits.get() + 1);
            self.w.log.borrow_mut().push(Ev::TtyDeinit);
        }
    }

    fn run(fail_unicode_set: bool, fail_winsize: bool) -> (Result<(), Error>, Vec<Ev>, u32) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let deinits = Rc::new(Cell::new(0));
        let tty = FakeTty {
            w: FakeWriter {
                log: log.clone(),
                fail_unicode_set,
            },
            fail_winsize,
            deinits: deinits.clone(),
        };
        let mut vx = rustty_vaxis::Vaxis::init(rustty_vaxis::vaxis::Options::default());
        let body_log = log.clone();
        let res = pretty_device_lifecycle(tty, &mut vx, |_w, _vx, _ws| {
            body_log.borrow_mut().push(Ev::Body);
            Ok(())
        });
        let events = std::mem::take(&mut *log.borrow_mut());
        (res, events, deinits.get())
    }

    fn index(events: &[Ev], ev: &Ev) -> Option<usize> {
        events.iter().position(|e| e == ev)
    }

    // parity S68-F1 / S68-F2: `try tty.getWinsize()` failing still runs the
    // three defers registered before it, LIFO: `unicode_reset`, `vx.deinit`,
    // `tty.deinit` (which restores the termios).
    #[test]
    fn winsize_failure_still_restores_the_tty() {
        let (res, events, deinits) = run(false, true);
        assert!(res.is_err());
        let set = index(&events, &Ev::Write(ctlseqs::unicode_set.to_vec()));
        let winsize = index(&events, &Ev::Winsize);
        let reset = index(&events, &Ev::Write(ctlseqs::unicode_reset.to_vec()));
        // Positive reach: the failing call really ran, after `unicode_set`.
        assert!(set.is_some() && winsize > set);
        assert_eq!(None, index(&events, &Ev::Body));
        assert!(reset > winsize);
        assert_eq!(1, deinits);
        assert_eq!(Some(&Ev::TtyDeinit), events.last());
    }

    // A failed `unicode_set` write runs only the two defers registered before
    // it (`vx.deinit`, `tty.deinit`); `unicode_reset` is not yet registered and
    // `getWinsize` is not reached.
    #[test]
    fn unicode_set_failure_still_deinits_the_tty() {
        let (res, events, deinits) = run(true, false);
        assert!(res.is_err());
        assert_eq!(None, index(&events, &Ev::Winsize));
        assert_eq!(
            None,
            index(&events, &Ev::Write(ctlseqs::unicode_reset.to_vec()))
        );
        assert_eq!(1, deinits);
        assert_eq!(Some(&Ev::TtyDeinit), events.last());
    }

    // The success path: set, winsize, body, reset, then the tty is released last.
    #[test]
    fn success_runs_every_defer_in_order() {
        let (res, events, deinits) = run(false, false);
        assert!(res.is_ok());
        let set = index(&events, &Ev::Write(ctlseqs::unicode_set.to_vec()));
        let body = index(&events, &Ev::Body);
        let reset = index(&events, &Ev::Write(ctlseqs::unicode_reset.to_vec()));
        assert!(set.is_some() && body > set && reset > body);
        assert_eq!(1, deinits);
        assert_eq!(Some(&Ev::TtyDeinit), events.last());
    }
}
