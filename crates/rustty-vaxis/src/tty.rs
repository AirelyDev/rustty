//! Port of `libvaxis` `tty.zig` (749 LOC, 0 `test{}`) — the TTY backend FFI. The
//! primary implementation is [`PosixTty`]: open `/dev/tty`, `make_raw` (the EXACT
//! termios flag clears/sets), the SIGWINCH `sigaction` global handler (with the
//! `handler_mutex` + handlers array), `read`/`writer`, `get_winsize` (`TIOCGWINSZ`
//! ioctl), and `deinit`
//! (restore the saved termios). [`WindowsTty`] is a `#[cfg(windows)]` R5 seam;
//! [`TestTty`] is the pipe-backed mock enabling the device-free Loop test path.
//!
//! PORT-NOTE (R6): this module is the crate's ONLY FFI surface (`libc`). It cannot run
//! under Miri (see `.port/phase-2/MIRI_EXCEPTIONS.md`); `/dev/tty` is absent in the
//! sandbox, so any device smoke test is `#[ignore]`-on-device. The R6 gate here is
//! ASan/native. `TestTty` (pipe-backed) covers the device-free path.
//!
//! PORT-NOTE (async-signal-safety): locking a mutex + `ioctl`/queue-push inside the
//! SIGWINCH handler is NOT async-signal-safe — but it is Zig's exact design and the
//! faithful port keeps it (the impure-backend contract, ASan/native-tested, never Miri).
//!
//! PORT-NOTE (§3.4 buffer/writer): Zig `init(buffer: []u8)` hands a caller write buffer
//! to `std.fs.File.Writer`. Rust buffers writes over the RAW `fd` WITHOUT owning it (so
//! `deinit`'s `close` is the only close): a small [`TtyWriter`] (`fd` + owned `Vec<u8>`)
//! that impls `std::io::Write` and does NOT own/close `fd`. The caller `buffer` becomes
//! the writer's initial owned `Vec` (a documented divergence — owned, not borrowed).
//!
//! PORT-NOTE (§10.16): `log.*` calls are dropped; `system_clipboard_allocator` is
//! dropped (paste owns its `Vec`).

use crate::Winsize;
use core::ffi::c_void;
use libc::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The error surface of the tty FFI (Zig's `!T` error unions over the termios/ioctl/
/// open/read syscalls). `Errno` carries the raw OS errno; `OutOfMemory` is
/// `notifyWinsize`'s 8-handler cap; `IoctlError` is `getWinsize`'s sentinel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyError {
    Errno(i32),
    OutOfMemory,
    IoctlError,
}

/// `posix.errno` — the errno of the last failing syscall.
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// The target TTY implementation. Zig: `if is_test TestTty else switch(os){ .windows =>
/// WindowsTty, else => PosixTty }`.
#[cfg(all(not(windows), not(test)))]
pub type Tty = PosixTty;
#[cfg(test)]
pub type Tty = TestTty;
#[cfg(all(windows, not(test)))]
pub type Tty = windows_tty::WindowsTty;

/// The restore-relevant state of the [`global_tty`]. Zig stores a COPY of the whole
/// `Tty` in `global_tty`; a Rust `static` cannot hold a non-`Copy` `Tty` (its
/// `TtyWriter` owns a `Vec`), so we keep only the panic-restore fields (`fd` +
/// `termios`) — a documented divergence.
// fd/termios are stored for a future panic-time restore (Zig's `global_tty` is read by
// the panic handler); that handler is not yet ported, so they are write-only for now.
#[cfg(all(not(windows), not(test)))]
#[allow(dead_code)]
struct GlobalTty {
    fd: c_int,
    termios: libc::termios,
}

/// global tty instance, used in case of a panic. Not guaranteed to work if
/// for some reason there are multiple TTYs open under a single vaxis
/// compilation unit - but this is better than nothing
#[cfg(all(not(windows), not(test)))]
static GLOBAL_TTY: Mutex<Option<GlobalTty>> = Mutex::new(None);

/// A buffered writer over the raw tty `fd` that does NOT own/close it (see the module
/// PORT-NOTE). `write` appends to `buf`; `flush` drains it via `libc::write`.
pub struct TtyWriter {
    fd: c_int,
    buf: Vec<u8>,
}

impl TtyWriter {
    fn new(fd: c_int, buf: Vec<u8>) -> Self {
        TtyWriter { fd, buf }
    }
}

impl std::io::Write for TtyWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut off = 0;
        while off < self.buf.len() {
            let n = unsafe {
                libc::write(
                    self.fd,
                    self.buf[off..].as_ptr() as *const c_void,
                    self.buf.len() - off,
                )
            };
            if n < 0 {
                return Err(std::io::Error::last_os_error());
            }
            off += n as usize;
        }
        self.buf.clear();
        Ok(())
    }
}

/// A registered SIGWINCH callback: a type-erased context (was `*anyopaque`) plus an
/// `extern "C"` callback. The context is a type-erased `*mut Loop<T>` recovered by the
/// handler (the userdata round-trip that forbids an `Arc` split, §4.2).
pub struct SignalHandler {
    pub context: *mut c_void,
    pub callback: extern "C" fn(context: *mut c_void),
}

// FFI-FIX (required to compile): a raw `*mut c_void` field makes `SignalHandler` neither
// `Send` nor `Sync`, so it cannot be placed in a `static Mutex<HandlerState>`. The
// pointer is a type-erased `*mut Loop<T>` whose pointee is pinned for the handler's whole
// registered life, and every cross-thread touch of it goes through the raw-self
// narrow-projection discipline of §4 — so this is sound.
unsafe impl Send for SignalHandler {}

/// The global handlers array + insert index (Zig `handlers`/`handler_idx`). Guarded by a
/// `Mutex` so the whole struct is the `Sync` shared surface.
struct HandlerState {
    handlers: [Option<SignalHandler>; 8],
    idx: usize,
}

/// global signal handlers
static HANDLERS: Mutex<HandlerState> = Mutex::new(HandlerState {
    handlers: [const { None }; 8],
    idx: 0,
});

static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

pub struct PosixTty {
    /// the original state of the terminal, prior to calling makeRaw
    termios: libc::termios,

    /// The file descriptor of the tty
    fd: c_int,

    /// File.Writer for efficient buffered writing
    tty_writer: TtyWriter,
}

impl PosixTty {
    /// initializes a Tty instance by opening /dev/tty and "making it raw". A
    /// signal handler is installed for SIGWINCH. No callbacks are installed, be
    /// sure to register a callback when initializing the event loop
    #[cfg(all(not(windows), not(test)))]
    pub fn init(buffer: Vec<u8>) -> Result<PosixTty, TtyError> {
        // Open our tty
        let fd = unsafe { libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR, 0) };
        if fd < 0 {
            return Err(TtyError::Errno(errno()));
        }

        // Set the termios of the tty
        let termios = Self::make_raw(fd)?;

        let mut act: libc::sigaction = unsafe { std::mem::zeroed() };
        // libc's `struct sigaction` exposes the handler as `sa_sigaction` (a usize-width
        // slot); put the 1-arg `extern "C" fn(c_int)` in via a fn-ptr cast, and DO NOT
        // set SA_SIGINFO (`sa_flags = 0` — the 1-arg form, not the 3-arg siginfo handler).
        act.sa_sigaction = Self::handle_winch as *const () as libc::sighandler_t;
        act.sa_mask = unsafe { std::mem::zeroed() };
        #[cfg(not(target_os = "macos"))]
        unsafe {
            libc::sigemptyset(&mut act.sa_mask);
        }
        act.sa_flags = 0;
        unsafe {
            libc::sigaction(libc::SIGWINCH, &act, std::ptr::null_mut());
        }
        HANDLER_INSTALLED.store(true, Ordering::SeqCst);

        let self_ = PosixTty {
            fd,
            termios,
            tty_writer: TtyWriter::new(fd, buffer),
        };

        *GLOBAL_TTY.lock().unwrap() = Some(GlobalTty { fd, termios });

        Ok(self_)
    }

    /// release resources associated with the Tty return it to its original state
    pub fn deinit(self) {
        if unsafe { libc::tcsetattr(self.fd, libc::TCSAFLUSH, &self.termios) } != 0 {
            // couldn't restore terminal (Zig logs the error; log.* dropped, §10.16)
        }
        // closing /dev/tty may block indefinitely on macos
        #[cfg(not(target_os = "macos"))]
        unsafe {
            libc::close(self.fd);
        }
    }

    /// Resets the signal handler to it's default
    pub fn reset_signal_handler() {
        if !HANDLER_INSTALLED.load(Ordering::SeqCst) {
            return;
        }
        HANDLER_INSTALLED.store(false, Ordering::SeqCst);
        let mut act: libc::sigaction = unsafe { std::mem::zeroed() };
        act.sa_sigaction = libc::SIG_DFL;
        act.sa_mask = unsafe { std::mem::zeroed() };
        #[cfg(not(target_os = "macos"))]
        unsafe {
            libc::sigemptyset(&mut act.sa_mask);
        }
        act.sa_flags = 0;
        unsafe {
            libc::sigaction(libc::SIGWINCH, &act, std::ptr::null_mut());
        }
    }

    pub fn writer(&mut self) -> &mut TtyWriter {
        &mut self.tty_writer
    }

    /// The file descriptor (Zig `self.tty.fd`) — the `getWinsize` argument the Loop
    /// read thread/winsize callback project out.
    pub fn fd(&self) -> c_int {
        self.fd
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, TtyError> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if n < 0 {
            return Err(TtyError::Errno(errno()));
        }
        Ok(n as usize)
    }

    /// Install a signal handler for winsize. A maximum of 8 handlers may be
    /// installed
    pub fn notify_winsize(handler: SignalHandler) -> Result<(), TtyError> {
        let mut state = HANDLERS.lock().unwrap();
        if state.idx == state.handlers.len() {
            return Err(TtyError::OutOfMemory);
        }
        let idx = state.idx;
        state.handlers[idx] = Some(handler);
        state.idx += 1;
        Ok(())
    }

    // Used by `init` (non-windows, non-test); dead under `#[cfg(test)]` where Tty=TestTty.
    #[allow(dead_code)]
    extern "C" fn handle_winch(_sig: c_int) {
        let state = HANDLERS.lock().unwrap();
        let mut i: usize = 0;
        while i < state.idx {
            if let Some(handler) = &state.handlers[i] {
                (handler.callback)(handler.context);
            }
            i += 1;
        }
    }

    /// makeRaw enters the raw state for the terminal.
    pub fn make_raw(fd: c_int) -> Result<libc::termios, TtyError> {
        let mut state: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut state) } != 0 {
            return Err(TtyError::Errno(errno()));
        }
        let mut raw = state;
        // see termios(3)
        raw.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);

        raw.c_oflag &= !libc::OPOST;

        raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG | libc::IEXTEN);

        raw.c_cflag = (raw.c_cflag & !libc::CSIZE) | libc::CS8;
        raw.c_cflag &= !libc::PARENB;

        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } != 0 {
            return Err(TtyError::Errno(errno()));
        }
        Ok(state)
    }

    /// Get the window size from the kernel
    pub fn get_winsize(fd: c_int) -> Result<Winsize, TtyError> {
        let mut winsize = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let err = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut winsize) };
        if err != -1 {
            // note the field rename row->rows, col->cols, xpixel->x_pixel, ypixel->y_pixel
            return Ok(Winsize {
                rows: winsize.ws_row,
                cols: winsize.ws_col,
                x_pixel: winsize.ws_xpixel,
                y_pixel: winsize.ws_ypixel,
            });
        }
        Err(TtyError::IoctlError)
    }
}

/// `WindowsTty` — the `#[cfg(windows)]` R5 seam (§3.5). The console-mode +
/// `ReadConsoleInputW` + `INPUT_RECORD` key/mouse/resize/focus decode (~450 LOC) lands
/// when a Windows target is in scope; the non-windows build never names it (which also
/// sidesteps its latent `self.fd` bug — its Zig `read` references a field it lacks). See
/// `.port/phase-2/DEFERRALS.md`.
#[cfg(windows)]
mod windows_tty {
    use super::TtyError;
    use crate::parser::Parser;
    use crate::Winsize;

    pub struct WindowsTty {
        pub stdin: *mut core::ffi::c_void,
        pub stdout: *mut core::ffi::c_void,
        pub initial_codepage: u32,
        pub initial_input_mode: u32,
        pub initial_output_mode: u32,
        // a buffer to write key text into
        pub buf: [u8; 4],
        pub tty_writer: super::TtyWriter,
        /// The last mouse button that was pressed. We store the previous state of button
        /// presses on each mouse event so we can detect which button was released
        pub last_mouse_button_press: u16,
    }

    impl WindowsTty {
        pub fn init(_buffer: Vec<u8>) -> Result<WindowsTty, TtyError> {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn deinit(self) {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn writer(&mut self) -> &mut super::TtyWriter {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn read(&self, _buf: &mut [u8]) -> Result<usize, TtyError> {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn next_event(&mut self, _parser: &mut Parser) -> Result<(), TtyError> {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn get_winsize(_fd: i32) -> Result<Winsize, TtyError> {
            unimplemented!("WindowsTty: R5 #[cfg(windows)] seam (DEFERRALS.md)")
        }
        pub fn reset_signal_handler() {}
    }
}

/// The pipe-backed test mock (Zig `TestTty`, compiled under `builtin.is_test`). A
/// `libc::pipe()` pair + a `TtyWriter` sink + a fixed `get_winsize`. `libc::pipe` works
/// in the sandbox (unlike `/dev/tty`), so the Loop scaffolding is exercisable here.
///
/// PORT-NOTE: Zig's `tty_writer` is an allocating `Vec` sink; the Rust port uses the
/// same [`TtyWriter`] type as `PosixTty` (over the `pipe_write` fd) so `writer()` has
/// ONE return type across ttys and the generic Loop compiles uniformly.
#[cfg(test)]
pub struct TestTty {
    /// Used for API compat
    fd: c_int,
    pipe_read: c_int,
    pipe_write: c_int,
    tty_writer: TtyWriter,
}

#[cfg(test)]
impl TestTty {
    /// Initializes a TestTty.
    pub fn init(buffer: Vec<u8>) -> Result<TestTty, TtyError> {
        let mut fds = [0 as c_int; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(TtyError::Errno(errno()));
        }
        let r = fds[0];
        let w = fds[1];
        Ok(TestTty {
            fd: r,
            pipe_read: r,
            pipe_write: w,
            tty_writer: TtyWriter::new(w, buffer),
        })
    }

    pub fn deinit(self) {
        unsafe {
            libc::close(self.pipe_read);
            libc::close(self.pipe_write);
        }
    }

    pub fn writer(&mut self) -> &mut TtyWriter {
        &mut self.tty_writer
    }

    /// The file descriptor (API compat with [`PosixTty::fd`]).
    pub fn fd(&self) -> c_int {
        self.fd
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, TtyError> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if n < 0 {
            return Err(TtyError::Errno(errno()));
        }
        Ok(n as usize)
    }

    /// Get the window size from the kernel
    pub fn get_winsize(_fd: c_int) -> Result<Winsize, TtyError> {
        Ok(Winsize {
            rows: 40,
            cols: 80,
            x_pixel: 40 * 8,
            y_pixel: 40 * 8 * 2,
        })
    }

    /// Implemented for the Windows API
    pub fn next_event(&mut self, _parser: &mut crate::parser::Parser) -> Result<(), TtyError> {
        // Zig returns `error.SkipZigTest`; this path is Windows-API-only and never
        // reached on non-windows targets.
        unimplemented!("TestTty.next_event: Windows-API compat only")
    }

    pub fn reset_signal_handler() {}
}
