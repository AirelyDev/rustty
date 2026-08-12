//! Port of `libvaxis` `tty.zig` (749 LOC, 0 `test{}`) — the TTY backend FFI. The
//! primary implementation is [`PosixTty`]: open `/dev/tty`, `make_raw` (the EXACT
//! termios flag clears/sets), the SIGWINCH `sigaction` global handler (with the
//! `handler_mutex` + handlers array), `read`/`writer`, `get_winsize` (`TIOCGWINSZ`
//! ioctl), and `deinit`
//! (restore the saved termios). [`windows_tty::WindowsTty`] is the FULL `#[cfg(windows)]`
//! console backend (`tty.zig:189-691`: console-mode/codepage setup, the `ReadConsoleInputW`
//! pump and the `INPUT_RECORD` key/mouse/resize/focus decode) — ported by WS20;
//! [`TestTty`] is the pipe-backed mock enabling the device-free Loop test path
//! (non-Windows only, as upstream's `posix.pipe`-based one is).
//!
//! PORT NOTE (R6): this module is the crate's ONLY FFI surface (`libc`). It cannot run
//! under Miri (see `.port/phase-2/MIRI_EXCEPTIONS.md`); `/dev/tty` is absent in the
//! sandbox, so any device smoke test is `#[ignore]`-on-device. The R6 gate here is
//! ASan/native. `TestTty` (pipe-backed) covers the device-free path.
//!
//! PORT NOTE (async-signal-safety): locking a mutex + `ioctl`/queue-push inside the
//! SIGWINCH handler is NOT async-signal-safe — but it is Zig's exact design and the
//! faithful port keeps it (the impure-backend contract, ASan/native-tested, never Miri).
//!
//! PORT NOTE (§3.4 buffer/writer): Zig `init(buffer: []u8)` hands a caller write buffer
//! to `std.fs.File.Writer`. Rust buffers writes over the RAW `fd` WITHOUT owning it (so
//! `deinit`'s `close` is the only close): a small [`TtyWriter`] (`fd` + owned `Vec<u8>`)
//! that impls `std::io::Write` and does NOT own/close `fd`. The caller `buffer` becomes
//! the writer's initial owned `Vec` (a documented divergence — owned, not borrowed).
//!
//! PORT NOTE (§10.16): `log.*` calls are dropped; `system_clipboard_allocator` is
//! dropped (paste owns its `Vec`).

#[cfg(not(windows))]
use crate::Winsize;
#[cfg(not(windows))]
use core::ffi::c_void;
#[cfg(not(windows))]
use libc::c_int;
#[cfg(not(windows))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(windows))]
use std::sync::Mutex;

/// The error surface of the tty FFI (Zig's `!T` error unions over the termios/ioctl/
/// open/read syscalls). `Errno` carries the raw OS errno; `OutOfMemory` is
/// `notifyWinsize`'s 8-handler cap; `IoctlError` is `getWinsize`'s sentinel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyError {
    Errno(i32),
    OutOfMemory,
    IoctlError,
    /// `error.InvalidHandle` — `getConsoleMode`/`setConsoleMode` saw
    /// `GetLastError() == ERROR_INVALID_HANDLE` (`tty.zig:283`/`290`).
    #[cfg(windows)]
    InvalidHandle,
    /// `windows.unexpectedError(e)` — the catch-all Win32 failure carrying the raw
    /// `GetLastError()` value.
    #[cfg(windows)]
    Unexpected(u32),
    /// The `Parser.parse` error set, which `nextEvent`'s inferred `!Event` union
    /// absorbs on the escape-sequence path (`tty.zig:428`).
    #[cfg(windows)]
    Parse(crate::parser::ParseError),
    /// `std.unicode.utf8Encode`'s error set, absorbed at `tty.zig:551`.
    #[cfg(windows)]
    InvalidUtf8,
}

/// `posix.errno` — the errno of the last failing syscall.
#[cfg(not(windows))]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// The target TTY implementation. Zig: `if is_test TestTty else switch(os){ .windows =>
/// WindowsTty, else => PosixTty }`.
///
/// PORT NOTE (windows + test): Zig's `TestTty` is `posix`-only (it calls `posix.pipe`),
/// so `builtin.is_test` on a Windows target would not compile upstream either. Rust
/// type-checks every arm, so the `test` alias is narrowed to non-Windows and a Windows
/// test build aliases `WindowsTty` (the real backend) instead.
#[cfg(all(not(windows), not(test)))]
pub type Tty = PosixTty;
#[cfg(all(test, not(windows)))]
pub type Tty = TestTty;
#[cfg(windows)]
pub type Tty = windows_tty::WindowsTty;

/// The restore-relevant state of the [`GLOBAL_TTY`]. Zig stores a COPY of the whole
/// `Tty` in `global_tty`; a Rust `static` cannot hold a non-`Copy` `Tty` (its
/// `TtyWriter` owns a `Vec`), so we keep only the panic-restore fields (`fd` +
/// `termios`) — a documented divergence.
// fd/termios are read by `crate::recover()` (the panic-time restore, `main.zig:66-78`);
// `crate::install_panic_recovery()` is the opt-in `Panic`-export analogue
// (W16-EXPLICIT-01).
#[cfg(not(windows))]
pub(crate) struct GlobalTty {
    pub(crate) fd: c_int,
    pub(crate) termios: libc::termios,
}

/// global tty instance, used in case of a panic. Not guaranteed to work if
/// for some reason there are multiple TTYs open under a single vaxis
/// compilation unit - but this is better than nothing
///
/// Zig `pub var global_tty: ?Tty` (`tty.zig:27`) — crate-visible so `crate::recover()`
/// (the `main.zig` port) can read it. Under `cfg(test)` the writer `PosixTty::init`
/// is not compiled (Tty = TestTty, and Zig's `TestTty.init` does not set `global_tty`
/// either), but the static itself exists so the recover tests can seed it.
#[cfg(not(windows))]
pub(crate) static GLOBAL_TTY: Mutex<Option<GlobalTty>> = Mutex::new(None);

/// A buffered writer over the raw tty `fd` that does NOT own/close it (see the module
/// PORT NOTE). `write` appends to `buf`; `flush` drains it via `libc::write`.
pub struct TtyWriter {
    /// PORT NOTE (windows): Zig's `tty_writer` is a `std.fs.File.Writer` over the
    /// `std.fs.File` whose payload on Windows is a `HANDLE`, not a POSIX fd. The
    /// backing handle is therefore `cfg`-switched here — POSIX targets keep the raw
    /// `fd` + `libc::write` drain, Windows keeps the console `HANDLE` + `WriteFile`.
    #[cfg(not(windows))]
    fd: c_int,
    #[cfg(windows)]
    handle: windows_tty::HANDLE,
    buf: Vec<u8>,
}

impl TtyWriter {
    #[cfg(not(windows))]
    fn new(fd: c_int, buf: Vec<u8>) -> Self {
        TtyWriter { fd, buf }
    }

    /// The Windows twin of [`TtyWriter::new`] — buffers over a console `HANDLE` that
    /// this writer does NOT own (`deinit`'s `CloseHandle` is the only close).
    #[cfg(windows)]
    fn new_handle(handle: windows_tty::HANDLE, buf: Vec<u8>) -> Self {
        TtyWriter { handle, buf }
    }
}

impl std::io::Write for TtyWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    #[cfg(not(windows))]
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

    #[cfg(windows)]
    fn flush(&mut self) -> std::io::Result<()> {
        let mut off = 0;
        while off < self.buf.len() {
            let mut written: u32 = 0;
            let remaining = self.buf.len() - off;
            // `WriteFile`'s count is a `DWORD`; a >4 GiB escape buffer is impossible
            // here (the write buffer is a per-frame escape stream), but clamp rather
            // than truncate so the loop makes progress either way.
            let chunk = if remaining > u32::MAX as usize {
                u32::MAX
            } else {
                remaining as u32
            };
            let ok = unsafe {
                windows_tty::WriteFile(
                    self.handle,
                    self.buf[off..].as_ptr(),
                    chunk,
                    &mut written,
                    core::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            off += written as usize;
        }
        self.buf.clear();
        Ok(())
    }
}

/// A registered SIGWINCH callback: a type-erased context (was `*anyopaque`) plus an
/// `extern "C"` callback. The context is a type-erased `*mut Loop<T>` recovered by the
/// handler (the userdata round-trip that forbids an `Arc` split, §4.2).
#[cfg(not(windows))]
pub struct SignalHandler {
    pub context: *mut c_void,
    pub callback: extern "C" fn(context: *mut c_void),
}

// FFI-FIX (required to compile): a raw `*mut c_void` field makes `SignalHandler` neither
// `Send` nor `Sync`, so it cannot be placed in a `static Mutex<HandlerState>`. The
// pointer is a type-erased `*mut Loop<T>` whose pointee is pinned for the handler's whole
// registered life, and every cross-thread touch of it goes through the raw-self
// narrow-projection discipline of §4 — so this is sound.
#[cfg(not(windows))]
unsafe impl Send for SignalHandler {}

/// The global handlers array + insert index (Zig `handlers`/`handler_idx`). Guarded by a
/// `Mutex` so the whole struct is the `Sync` shared surface.
#[cfg(not(windows))]
struct HandlerState {
    handlers: [Option<SignalHandler>; 8],
    idx: usize,
}

/// global signal handlers
#[cfg(not(windows))]
static HANDLERS: Mutex<HandlerState> = Mutex::new(HandlerState {
    handlers: [const { None }; 8],
    idx: 0,
});

#[cfg(not(windows))]
static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

#[cfg(not(windows))]
pub struct PosixTty {
    /// the original state of the terminal, prior to calling makeRaw
    termios: libc::termios,

    /// The file descriptor of the tty
    fd: c_int,

    /// File.Writer for efficient buffered writing
    tty_writer: TtyWriter,
}

#[cfg(not(windows))]
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

    /// Narrow `tty_writer` projection off a raw Tty — `Loop::stop()`'s DSR-wake path.
    /// Zig `writer(self: *PosixTty) *std.Io.Writer` is pointer-in/pointer-out; the
    /// `&mut self` autoref of [`Self::writer`] is a whole-Tty `Unique` retag that pops
    /// the blocked read thread's call-protected tag (VALIDATION_REPORT H1; proven by
    /// mirihar-chunk3b `stopread_naive`), so cross-thread callers project the field
    /// raw instead.
    ///
    /// # Safety
    /// `this` must point to a live `PosixTty` (no dereference happens here — only the
    /// in-bounds field-address computation).
    pub unsafe fn writer_raw(this: *mut Self) -> *mut TtyWriter {
        unsafe { std::ptr::addr_of_mut!((*this).tty_writer) }
    }

    /// The file descriptor (Zig `self.tty.fd`) — the `getWinsize` argument the Loop
    /// read thread/winsize callback project out.
    pub fn fd(&self) -> c_int {
        self.fd
    }

    /// Read from the tty. Zig `read(self: *const PosixTty, buf: []u8) !usize` — a Zig
    /// `*const` carries NO protector, and the Loop read thread sits BLOCKED in this
    /// call while `Loop::stop()` writes the DSR wake into `tty_writer` (bytes inside
    /// this same allocation). A Rust `&self` receiver here would be a call-protected
    /// shared borrow of the WHOLE Tty for the duration of the blocked syscall, which
    /// that write pops (SB/TB protector UB — proven by mirihar-chunk3b
    /// `stopread_projonly`); the faithful-and-sound receiver is the raw `*const` with
    /// a narrow `fd` projection.
    ///
    /// # Safety
    /// `this` must point to a live `PosixTty`; concurrent access to it must stay
    /// narrow (no whole-`&mut Tty` while this call blocks — `tty_writer`-field
    /// projections are fine).
    pub unsafe fn read(this: *const Self, buf: &mut [u8]) -> Result<usize, TtyError> {
        let fd = unsafe { std::ptr::addr_of!((*this).fd).read() };
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
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

    /// Remove every registered winsize handler whose `context` matches, and
    /// compact the array.
    //
    // PORT NOTE (rustty-only, VALIDATION_REPORT §3c.5): upstream libvaxis has NO
    // deregistration — `handlers`/`handler_idx` only ever grow, so a destroyed
    // Zig Loop leaves its `*anyopaque` context dangling in the process-global
    // array and the next SIGWINCH is a use-after-free in signal context (a latent
    // upstream hazard, not behavior to preserve: R1 preserves behavior, UB is not
    // behavior). Rust's `Loop::drop` calls this with `self` as the context so the
    // registration can never outlive its pointee (UAF reproduced + fix proven on
    // the isolated Miri harness: .port/phase-2/mirihar-chunk3b `winch_naive` UB /
    // `winch_shipped` CLEAN). Divergence note: after a drop+re-init cycle rustty
    // frees the slot where Zig would keep consuming one of the 8 — strictly more
    // permissive, and unreachable upstream (upstream loops live for the app).
    // Locking the same Mutex `handle_winch` locks is the module's documented
    // (Zig-faithful) non-async-signal-safe design — see the module PORT NOTE.
    pub fn deregister_winsize(context: *mut c_void) {
        let mut state = HANDLERS.lock().unwrap();
        let mut w: usize = 0;
        for r in 0..state.idx {
            let keep = match &state.handlers[r] {
                Some(h) => h.context != context,
                None => false,
            };
            if keep {
                if w != r {
                    state.handlers[w] = state.handlers[r].take();
                }
                w += 1;
            } else {
                state.handlers[r] = None;
            }
        }
        state.idx = w;
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

/// `WindowsTty` — the Windows console backend (`tty.zig:189-691`, ~503 LOC): the
/// console-mode + codepage setup, the `ReadConsoleInputW` event pump, and the full
/// `INPUT_RECORD` key/mouse/resize/focus decode.
///
/// PORT NOTE (FFI, R3/R4): `libc` on Windows carries none of the console APIs, and the
/// workspace convention (`rustty-os::windows`, `rustty-xev::windows`) is a hand-rolled
/// `extern "system"` binding to the SAME system DLL (`kernel32`) — a binding to the
/// system library, not a reimplementation. No crate was added for this module.
///
/// PORT NOTE (the two upstream compile errors this port repairs): upstream's
/// `WindowsTty.read` calls `posix.read(self.fd, buf)` but `WindowsTty` has NO `fd`
/// field, and upstream has no `WindowsTty.getWinsize` at all. Both are unreachable in
/// Zig because `Loop.ttyRun`'s `.windows` arm calls only `nextEvent`, and Zig only
/// semantically analyzes referenced declarations. Rust analyzes every arm, and the
/// `Tty` alias must expose ONE surface, so both are implemented here against the
/// console handles (`ReadFile` on stdin; `GetConsoleScreenBufferInfo` on stdout — the
/// same call the resize arm of `event_from_record` already makes).
#[cfg(windows)]
pub mod windows_tty {
    #![allow(non_snake_case)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]

    use super::{TtyError, TtyWriter};
    use crate::event::Event;
    use crate::key::{self, Key};
    use crate::mouse::{self, Mouse};
    use crate::parser::Parser;
    use crate::Winsize;
    use core::ffi::c_void;
    use std::sync::Mutex;

    // -----------------------------------------------------------------------
    // The Win32 ABI surface (`std.os.windows` + the `zig-windows-console` console
    // records upstream declares inline at `tty.zig:600-691`).
    // -----------------------------------------------------------------------

    pub type HANDLE = *mut c_void;
    pub type BOOL = i32;
    pub type DWORD = u32;
    pub type UINT = u32;
    pub type WORD = u16;
    pub type WCHAR = u16;
    pub type CHAR = u8;
    pub type SHORT = i16;

    /// `windows.COORD`
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct COORD {
        pub X: SHORT,
        pub Y: SHORT,
    }

    /// `windows.SMALL_RECT`
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SMALL_RECT {
        pub Left: SHORT,
        pub Top: SHORT,
        pub Right: SHORT,
        pub Bottom: SHORT,
    }

    /// `windows.CONSOLE_SCREEN_BUFFER_INFO`
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CONSOLE_SCREEN_BUFFER_INFO {
        pub dwSize: COORD,
        pub dwCursorPosition: COORD,
        pub wAttributes: WORD,
        pub srWindow: SMALL_RECT,
        pub dwMaximumWindowSize: COORD,
    }

    // From gitub.com/ziglibs/zig-windows-console. Thanks :)
    //
    // Events
    //
    // PORT NOTE: `union_unnamed_248` is `extern union { UnicodeChar: WCHAR,
    // AsciiChar: CHAR }`. Rust has no untagged union with safe field access, so it is
    // a `#[repr(C)] union` and both reads are `unsafe` (they are always valid: every
    // bit pattern of `u16`/`u8` is valid for the other, the union is 2 bytes).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union union_unnamed_248 {
        pub UnicodeChar: WCHAR,
        pub AsciiChar: CHAR,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct KEY_EVENT_RECORD {
        pub bKeyDown: BOOL,
        pub wRepeatCount: WORD,
        pub wVirtualKeyCode: WORD,
        pub wVirtualScanCode: WORD,
        pub uChar: union_unnamed_248,
        pub dwControlKeyState: DWORD,
    }
    pub type PKEY_EVENT_RECORD = *mut KEY_EVENT_RECORD;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct MOUSE_EVENT_RECORD {
        pub dwMousePosition: COORD,
        pub dwButtonState: DWORD,
        pub dwControlKeyState: DWORD,
        pub dwEventFlags: DWORD,
    }
    pub type PMOUSE_EVENT_RECORD = *mut MOUSE_EVENT_RECORD;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct WINDOW_BUFFER_SIZE_RECORD {
        pub dwSize: COORD,
    }
    pub type PWINDOW_BUFFER_SIZE_RECORD = *mut WINDOW_BUFFER_SIZE_RECORD;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct MENU_EVENT_RECORD {
        pub dwCommandId: UINT,
    }
    pub type PMENU_EVENT_RECORD = *mut MENU_EVENT_RECORD;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FOCUS_EVENT_RECORD {
        pub bSetFocus: BOOL,
    }
    pub type PFOCUS_EVENT_RECORD = *mut FOCUS_EVENT_RECORD;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union union_unnamed_249 {
        pub KeyEvent: KEY_EVENT_RECORD,
        pub MouseEvent: MOUSE_EVENT_RECORD,
        pub WindowBufferSizeEvent: WINDOW_BUFFER_SIZE_RECORD,
        pub MenuEvent: MENU_EVENT_RECORD,
        pub FocusEvent: FOCUS_EVENT_RECORD,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct INPUT_RECORD {
        pub EventType: WORD,
        pub Event: union_unnamed_249,
    }
    pub type PINPUT_RECORD = *mut INPUT_RECORD;

    extern "system" {
        pub fn ReadConsoleInputW(
            hConsoleInput: HANDLE,
            lpBuffer: PINPUT_RECORD,
            nLength: DWORD,
            lpNumberOfEventsRead: *mut DWORD,
        ) -> BOOL;
        pub fn GetStdHandle(nStdHandle: DWORD) -> HANDLE;
        pub fn GetLastError() -> DWORD;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn GetConsoleOutputCP() -> UINT;
        pub fn SetConsoleOutputCP(wCodePageID: UINT) -> BOOL;
        pub fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *mut DWORD) -> BOOL;
        pub fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: DWORD) -> BOOL;
        pub fn GetConsoleScreenBufferInfo(
            hConsoleOutput: HANDLE,
            lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
        ) -> BOOL;
        pub fn ReadFile(
            hFile: HANDLE,
            lpBuffer: *mut u8,
            nNumberOfBytesToRead: DWORD,
            lpNumberOfBytesRead: *mut DWORD,
            lpOverlapped: *mut c_void,
        ) -> BOOL;
        pub fn WriteFile(
            hFile: HANDLE,
            lpBuffer: *const u8,
            nNumberOfBytesToWrite: DWORD,
            lpNumberOfBytesWritten: *mut DWORD,
            lpOverlapped: *mut c_void,
        ) -> BOOL;
    }

    /// `GetStdHandle` selectors (`std.fs.File.stdin()`/`stdout()`).
    pub const STD_INPUT_HANDLE: DWORD = 0xFFFF_FFF6; // (DWORD)-10
    pub const STD_OUTPUT_HANDLE: DWORD = 0xFFFF_FFF5; // (DWORD)-11

    /// `Win32Error.INVALID_HANDLE`
    pub const ERROR_INVALID_HANDLE: DWORD = 6;

    /// `windows.unexpectedError(err)` — upstream logs + returns
    /// `error.Unexpected`; here the raw code is carried so callers can report it.
    pub fn unexpectedError<T>(err: DWORD) -> Result<T, TtyError> {
        Err(TtyError::Unexpected(err))
    }

    // -----------------------------------------------------------------------
    // The console modes.
    //
    // PORT NOTE: upstream models these as `packed struct(u32)` with one `u1` per
    // documented flag. Rust has no packed bitfield struct, so each field becomes a
    // named bit constant of the SAME LSB-first layout and the mode itself is the
    // `u32` the Win32 API actually takes (which is exactly what upstream
    // `@bitCast`s to at every call site).
    // -----------------------------------------------------------------------

    /// `CONSOLE_MODE_INPUT` (`tty.zig:266-277`), LSB-first.
    pub mod console_mode_input {
        use super::DWORD;
        pub const PROCESSED_INPUT: DWORD = 1 << 0;
        pub const LINE_INPUT: DWORD = 1 << 1;
        pub const ECHO_INPUT: DWORD = 1 << 2;
        pub const WINDOW_INPUT: DWORD = 1 << 3;
        pub const MOUSE_INPUT: DWORD = 1 << 4;
        pub const INSERT_MODE: DWORD = 1 << 5;
        pub const QUICK_EDIT_MODE: DWORD = 1 << 6;
        pub const EXTENDED_FLAGS: DWORD = 1 << 7;
        pub const AUTO_POSITION: DWORD = 1 << 8;
        pub const VIRTUAL_TERMINAL_INPUT: DWORD = 1 << 9;
    }

    /// `CONSOLE_MODE_OUTPUT` (`tty.zig:278-285`), LSB-first.
    pub mod console_mode_output {
        use super::DWORD;
        pub const PROCESSED_OUTPUT: DWORD = 1 << 0;
        pub const WRAP_AT_EOL_OUTPUT: DWORD = 1 << 1;
        pub const VIRTUAL_TERMINAL_PROCESSING: DWORD = 1 << 2;
        pub const DISABLE_NEWLINE_AUTO_RETURN: DWORD = 1 << 3;
        pub const ENABLE_LVB_GRID_WORLDWIDE: DWORD = 1 << 4;
    }

    pub struct WindowsTty {
        pub stdin: HANDLE,
        pub stdout: HANDLE,

        pub initial_codepage: UINT,
        pub initial_input_mode: DWORD,
        pub initial_output_mode: DWORD,

        // a buffer to write key text into
        pub buf: [u8; 4],

        /// File.Writer for efficient buffered writing
        pub tty_writer: TtyWriter,

        /// The last mouse button that was pressed. We store the previous state of
        /// button presses on each mouse event so we can detect which button was
        /// released
        pub last_mouse_button_press: u16,

        /// PORT NOTE (rustty-only, SOUNDNESS): upstream's `nextEvent` keeps its
        /// `EventState` — which owns `ansi_buf`, the byte buffer the parser's
        /// returned `Event` BORROWS — as a `nextEvent` LOCAL, so an escape-sequence
        /// event returned by `nextEvent` points into a dead stack frame (a latent
        /// upstream use-after-free; UB is not behavior R1 preserves). Hoisting the
        /// state into the tty makes the returned `Event` borrow the tty instead —
        /// exactly like `buf` already does for `Key.text` — with NO behavioral
        /// difference, because `next_event` resets it on entry (below) so each call
        /// still starts from `.{}` as upstream's local did.
        pub event_state: EventState,
    }

    const utf8_codepage: UINT = 65001;

    /// The input mode set by init
    pub const input_raw_mode: DWORD = console_mode_input::WINDOW_INPUT   // resize events
        | console_mode_input::MOUSE_INPUT
        | console_mode_input::EXTENDED_FLAGS; // allow mouse events

    /// The output mode set by init
    pub const output_raw_mode: DWORD = console_mode_output::PROCESSED_OUTPUT // handle control sequences
        | console_mode_output::VIRTUAL_TERMINAL_PROCESSING // handle ANSI sequences
        | console_mode_output::DISABLE_NEWLINE_AUTO_RETURN // disable inserting a new line when we write at the last column
        | console_mode_output::ENABLE_LVB_GRID_WORLDWIDE; // enables reverse video and underline

    /// The restore-relevant state of the `global_tty` (the Windows twin of the POSIX
    /// [`super::GlobalTty`]). Upstream stores a COPY of the whole `Tty`; a Rust
    /// `static` cannot hold a non-`Copy` `Tty` (its `TtyWriter` owns a `Vec`), so only
    /// the panic-restore fields are kept. Handles are stored as `usize` so the static
    /// is `Send`; they are pure kernel handles, never dereferenced.
    // Read by `crate::recover()` (the panic-time restore, `main.zig:66-78`) —
    // W16-EXPLICIT-01.
    pub(crate) struct GlobalTty {
        pub(crate) stdin: usize,
        pub(crate) stdout: usize,
        pub(crate) initial_codepage: UINT,
        pub(crate) initial_input_mode: DWORD,
        pub(crate) initial_output_mode: DWORD,
    }

    /// global tty instance, used in case of a panic.
    pub(crate) static GLOBAL_TTY: Mutex<Option<GlobalTty>> = Mutex::new(None);

    /// `nextEvent`'s decode state: the partially-accumulated ANSI escape sequence and
    /// the pending UTF-16 high surrogate (`tty.zig:404-409`).
    #[derive(Clone, Copy)]
    pub struct EventState {
        pub ansi_buf: [u8; 128],
        pub ansi_idx: usize,
        pub utf16_buf: [u16; 2],
        pub utf16_half: bool,
    }

    impl Default for EventState {
        fn default() -> Self {
            EventState {
                ansi_buf: [0; 128],
                ansi_idx: 0,
                utf16_buf: [0; 2],
                utf16_half: false,
            }
        }
    }

    /// `std.unicode.utf16IsHighSurrogate`
    fn utf16_is_high_surrogate(c: u16) -> bool {
        (0xD800..0xDC00).contains(&c)
    }

    /// `std.unicode.utf16IsLowSurrogate`
    fn utf16_is_low_surrogate(c: u16) -> bool {
        (0xDC00..0xE000).contains(&c)
    }

    /// `std.unicode.utf16DecodeSurrogatePair` — `null` where Zig returns
    /// `error.ExpectedSecondSurrogateHalf`/`error.UnexpectedSecondSurrogateHalf`.
    fn utf16_decode_surrogate_pair(pair: &[u16; 2]) -> Option<u32> {
        if !utf16_is_high_surrogate(pair[0]) || !utf16_is_low_surrogate(pair[1]) {
            return None;
        }
        Some(0x1_0000 + (((pair[0] as u32) - 0xD800) << 10) + ((pair[1] as u32) - 0xDC00))
    }

    /// `std.unicode.utf8Encode` — `None` where Zig returns
    /// `error.CodepointTooLarge`/`error.Utf8CannotEncodeSurrogateHalf`.
    fn utf8_encode(cp: u32, out: &mut [u8; 4]) -> Option<usize> {
        let c = char::from_u32(cp)?;
        Some(c.encode_utf8(out).len())
    }

    /// `getConsoleMode(comptime T, handle)` — `T` is only the packed-struct view of
    /// the same `u32`, so the Rust signature drops the comptime parameter.
    ///
    /// # Safety
    /// `handle` must be a valid console handle (clippy requires `unsafe` on a public
    /// fn taking a raw pointer it hands to FFI).
    pub unsafe fn getConsoleMode(handle: HANDLE) -> Result<DWORD, TtyError> {
        let mut mode: DWORD = 0;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            return match unsafe { GetLastError() } {
                ERROR_INVALID_HANDLE => Err(TtyError::InvalidHandle),
                e => unexpectedError(e),
            };
        }
        Ok(mode)
    }

    /// # Safety
    /// `handle` must be a valid console handle.
    pub unsafe fn setConsoleMode(handle: HANDLE, mode: DWORD) -> Result<(), TtyError> {
        if unsafe { SetConsoleMode(handle, mode) } == 0 {
            return match unsafe { GetLastError() } {
                ERROR_INVALID_HANDLE => Err(TtyError::InvalidHandle),
                e => unexpectedError(e),
            };
        }
        Ok(())
    }

    fn translate_mods(mods: DWORD) -> key::Modifiers {
        const left_alt: DWORD = 0x0002;
        const right_alt: DWORD = 0x0001;
        const left_ctrl: DWORD = 0x0008;
        const right_ctrl: DWORD = 0x0004;

        const caps: DWORD = 0x0080;
        const num_lock: DWORD = 0x0020;
        const shift: DWORD = 0x0010;
        const alt: DWORD = left_alt | right_alt;
        const ctrl: DWORD = left_ctrl | right_ctrl;

        key::Modifiers {
            shift: mods & shift > 0,
            alt: mods & alt > 0,
            ctrl: mods & ctrl > 0,
            caps_lock: mods & caps > 0,
            num_lock: mods & num_lock > 0,
            ..Default::default()
        }
    }

    impl WindowsTty {
        pub fn init(buffer: Vec<u8>) -> Result<WindowsTty, TtyError> {
            // const stdin: std.fs.File = .stdin();
            // const stdout: std.fs.File = .stdout();
            let stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
            let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

            // get initial modes
            let initial_output_codepage = unsafe { GetConsoleOutputCP() };
            let initial_input_mode = unsafe { getConsoleMode(stdin)? };
            let initial_output_mode = unsafe { getConsoleMode(stdout)? };

            // set new modes
            unsafe { setConsoleMode(stdin, input_raw_mode)? };
            unsafe { setConsoleMode(stdout, output_raw_mode)? };
            if unsafe { SetConsoleOutputCP(utf8_codepage) } == 0 {
                return unexpectedError(unsafe { GetLastError() });
            }

            let self_ = WindowsTty {
                stdin,
                stdout,
                initial_codepage: initial_output_codepage,
                initial_input_mode,
                initial_output_mode,
                buf: [0; 4],
                tty_writer: TtyWriter::new_handle(stdout, buffer),
                last_mouse_button_press: 0,
                event_state: EventState::default(),
            };

            // save a copy of this tty as the global_tty for panic handling
            *GLOBAL_TTY.lock().unwrap() = Some(GlobalTty {
                stdin: stdin as usize,
                stdout: stdout as usize,
                initial_codepage: initial_output_codepage,
                initial_input_mode,
                initial_output_mode,
            });

            Ok(self_)
        }

        pub fn deinit(self) {
            unsafe { SetConsoleOutputCP(self.initial_codepage) };
            let _ = unsafe { setConsoleMode(self.stdin, self.initial_input_mode) };
            let _ = unsafe { setConsoleMode(self.stdout, self.initial_output_mode) };
            unsafe { CloseHandle(self.stdin) };
            unsafe { CloseHandle(self.stdout) };
        }

        pub fn writer(&mut self) -> &mut TtyWriter {
            &mut self.tty_writer
        }

        /// Narrow `tty_writer` projection (aliasing parity with
        /// `PosixTty::writer_raw`).
        ///
        /// # Safety
        /// `this` must point to a live `WindowsTty`.
        pub unsafe fn writer_raw(this: *mut Self) -> *mut TtyWriter {
            unsafe { std::ptr::addr_of_mut!((*this).tty_writer) }
        }

        /// Raw-receiver read (signature parity with `PosixTty::read`, H1).
        ///
        /// PORT NOTE: upstream is `return posix.read(self.fd, buf);` against a field
        /// `WindowsTty` does not have (see the module PORT NOTE). The console read is
        /// `ReadFile` on the stdin handle, which is what `posix.read` lowers to on
        /// Windows anyway.
        ///
        /// # Safety
        /// `this` must point to a live `WindowsTty`; concurrent access must stay
        /// narrow (no whole-`&mut Tty` while this call blocks).
        pub unsafe fn read(this: *const Self, buf: &mut [u8]) -> Result<usize, TtyError> {
            let handle = unsafe { std::ptr::addr_of!((*this).stdin).read() };
            let mut n: DWORD = 0;
            let len = if buf.len() > DWORD::MAX as usize {
                DWORD::MAX
            } else {
                buf.len() as DWORD
            };
            let ok =
                unsafe { ReadFile(handle, buf.as_mut_ptr(), len, &mut n, core::ptr::null_mut()) };
            if ok == 0 {
                return Err(TtyError::Unexpected(unsafe { GetLastError() }));
            }
            Ok(n as usize)
        }

        /// `nextEvent` — the blocking `ReadConsoleInputW` pump.
        ///
        /// PORT NOTE (raw receiver, H1): this BLOCKS inside `ReadConsoleInputW` while
        /// the main thread's `Loop::stop` projects `tty_writer` to write its wake, so
        /// a whole-`&mut self` here would pop the reader's call-protected tag (the
        /// same SB/TB hazard `PosixTty::read` is raw-received for).
        ///
        /// PORT NOTE (`paste_allocator`): dropped — the Rust `Parser` owns its paste
        /// `Vec` (the §5/3a precedent).
        ///
        /// # Safety
        /// `this`/`parser` must point to live, pinned values that outlive `'e` (the
        /// returned `Event` borrows `(*this).buf`, `(*this).event_state.ansi_buf` and
        /// the parser's internal buffer — see the `event_state` field note), and the
        /// caller must hold no conflicting whole-`&mut`.
        pub unsafe fn next_event<'e>(
            this: *mut Self,
            parser: *mut Parser,
        ) -> Result<Event<'e>, TtyError> {
            // We use a loop so we can ignore certain events
            // var state: EventState = .{};
            unsafe { std::ptr::addr_of_mut!((*this).event_state).write(EventState::default()) };
            loop {
                let mut event_count: DWORD = 0;
                let mut input_record: INPUT_RECORD = unsafe { core::mem::zeroed() };
                let stdin = unsafe { std::ptr::addr_of!((*this).stdin).read() };
                if unsafe { ReadConsoleInputW(stdin, &mut input_record, 1, &mut event_count) } == 0
                {
                    return Err(TtyError::Unexpected(unsafe { GetLastError() }));
                }

                let state = unsafe { std::ptr::addr_of_mut!((*this).event_state) };
                if let Some(ev) =
                    unsafe { Self::event_from_record(this, &input_record, state, parser)? }
                {
                    return Ok(ev);
                }
            }
        }

        /// `eventFromRecord` — the `INPUT_RECORD` decode.
        ///
        /// # Safety
        /// As [`WindowsTty::next_event`]; `record` must point to a live
        /// `INPUT_RECORD` and `state` into `(*this).event_state`.
        pub unsafe fn event_from_record<'e>(
            this: *mut Self,
            record: *const INPUT_RECORD,
            state: *mut EventState,
            parser: *mut Parser,
        ) -> Result<Option<Event<'e>>, TtyError> {
            match unsafe { (*record).EventType } {
                0x0001 => {
                    // Key event
                    let event = unsafe { (*record).Event.KeyEvent };

                    if unsafe { (*state).utf16_half } {
                        // `half:` labelled block — `break :half` falls through to the
                        // normal decode below.
                        'half: {
                            unsafe { (*state).utf16_half = false };
                            unsafe { (*state).utf16_buf[1] = event.uChar.UnicodeChar };
                            let utf16_buf = unsafe { (*state).utf16_buf };
                            let Some(codepoint) = utf16_decode_surrogate_pair(&utf16_buf) else {
                                break 'half;
                            };
                            let n = {
                                let buf = unsafe { &mut *std::ptr::addr_of_mut!((*this).buf) };
                                match utf8_encode(codepoint, buf) {
                                    Some(n) => n,
                                    None => return Ok(None),
                                }
                            };

                            let key = Key {
                                codepoint,
                                base_layout_codepoint: Some(codepoint),
                                mods: translate_mods(event.dwControlKeyState),
                                text: Some(unsafe { Self::text_slice(this, n) }),
                                shifted_codepoint: None,
                            };

                            return Ok(Some(match event.bKeyDown {
                                0 => Event::KeyRelease(key),
                                _ => Event::KeyPress(key),
                            }));
                        }
                    }

                    let base_layout: u16 = match event.wVirtualKeyCode {
                        // delivered when we get an escape sequence or a unicode codepoint
                        0x00 => {
                            if unsafe { (*state).ansi_idx } == 0
                                && unsafe { event.uChar.AsciiChar } != 27
                            {
                                unsafe { event.uChar.UnicodeChar }
                            } else {
                                unsafe {
                                    let idx = (*state).ansi_idx;
                                    (*state).ansi_buf[idx] = event.uChar.AsciiChar;
                                    (*state).ansi_idx += 1;
                                }
                                if unsafe { (*state).ansi_idx } <= 2 {
                                    return Ok(None);
                                }
                                let idx = unsafe { (*state).ansi_idx };
                                let input: &'e [u8] = unsafe {
                                    core::slice::from_raw_parts(
                                        std::ptr::addr_of!((*state).ansi_buf) as *const u8,
                                        idx,
                                    )
                                };
                                let result =
                                    unsafe { (*parser).parse(input) }.map_err(TtyError::Parse)?;
                                return Ok(if result.n == 0 {
                                    None
                                } else {
                                    unsafe { (*state).ansi_idx = 0 };
                                    result.event
                                });
                            }
                        }
                        0x08 => key::backspace as u16,
                        0x09 => key::tab as u16,
                        0x0D => key::enter as u16,
                        0x13 => key::pause as u16,
                        0x14 => key::caps_lock as u16,
                        0x1B => key::escape as u16,
                        0x20 => key::space as u16,
                        0x21 => key::page_up as u16,
                        0x22 => key::page_down as u16,
                        0x23 => key::end as u16,
                        0x24 => key::home as u16,
                        0x25 => key::left as u16,
                        0x26 => key::up as u16,
                        0x27 => key::right as u16,
                        0x28 => key::down as u16,
                        0x2c => key::print_screen as u16,
                        0x2d => key::insert as u16,
                        0x2e => key::delete as u16,
                        k @ 0x30..=0x39 => k,
                        k @ 0x41..=0x5a => k + 0x20, // translate to lowercase
                        0x5b => key::left_meta as u16,
                        0x5c => key::right_meta as u16,
                        0x60 => key::kp_0 as u16,
                        0x61 => key::kp_1 as u16,
                        0x62 => key::kp_2 as u16,
                        0x63 => key::kp_3 as u16,
                        0x64 => key::kp_4 as u16,
                        0x65 => key::kp_5 as u16,
                        0x66 => key::kp_6 as u16,
                        0x67 => key::kp_7 as u16,
                        0x68 => key::kp_8 as u16,
                        0x69 => key::kp_9 as u16,
                        0x6a => key::kp_multiply as u16,
                        0x6b => key::kp_add as u16,
                        0x6c => key::kp_separator as u16,
                        0x6d => key::kp_subtract as u16,
                        0x6e => key::kp_decimal as u16,
                        0x6f => key::kp_divide as u16,
                        0x70 => key::f1 as u16,
                        0x71 => key::f2 as u16,
                        0x72 => key::f3 as u16,
                        0x73 => key::f4 as u16,
                        0x74 => key::f5 as u16,
                        0x75 => key::f6 as u16,
                        // PORT NOTE (upstream typo, preserved): `0x76 => Key.f8` —
                        // VK_F7 maps to `f8` upstream. R1 keeps the behaviour.
                        0x76 => key::f8 as u16,
                        0x77 => key::f8 as u16,
                        0x78 => key::f9 as u16,
                        0x79 => key::f10 as u16,
                        0x7a => key::f11 as u16,
                        0x7b => key::f12 as u16,
                        0x7c => key::f13 as u16,
                        0x7d => key::f14 as u16,
                        0x7e => key::f15 as u16,
                        0x7f => key::f16_ as u16,
                        0x80 => key::f17 as u16,
                        0x81 => key::f18 as u16,
                        0x82 => key::f19 as u16,
                        0x83 => key::f20 as u16,
                        0x84 => key::f21 as u16,
                        0x85 => key::f22 as u16,
                        0x86 => key::f23 as u16,
                        0x87 => key::f24 as u16,
                        0x90 => key::num_lock as u16,
                        0x91 => key::scroll_lock as u16,
                        0xa0 => key::left_shift as u16,
                        0x10 => key::left_shift as u16,
                        0xa1 => key::right_shift as u16,
                        0xa2 => key::left_control as u16,
                        0x11 => key::left_control as u16,
                        0xa3 => key::right_control as u16,
                        0xa4 => key::left_alt as u16,
                        0x12 => key::left_alt as u16,
                        0xa5 => key::right_alt as u16,
                        0xad => key::mute_volume as u16,
                        0xae => key::lower_volume as u16,
                        0xaf => key::raise_volume as u16,
                        0xb0 => key::media_track_next as u16,
                        0xb1 => key::media_track_previous as u16,
                        0xb2 => key::media_stop as u16,
                        0xb3 => key::media_play_pause as u16,
                        0xba => b';' as u16,
                        0xbb => b'+' as u16,
                        0xbc => b',' as u16,
                        0xbd => b'-' as u16,
                        0xbe => b'.' as u16,
                        0xbf => b'/' as u16,
                        0xc0 => b'`' as u16,
                        0xdb => b'[' as u16,
                        0xdc => b'\\' as u16,
                        0xe2 => b'\\' as u16,
                        0xdd => b']' as u16,
                        0xde => b'\'' as u16,
                        _ => {
                            // log.warn("unknown wVirtualKeyCode: 0x{x}", .{...});
                            return Ok(None);
                        }
                    };

                    if utf16_is_high_surrogate(base_layout) {
                        unsafe {
                            (*state).utf16_buf[0] = base_layout;
                            (*state).utf16_half = true;
                        }
                        return Ok(None);
                    }
                    if utf16_is_low_surrogate(base_layout) {
                        return Ok(None);
                    }

                    let mut codepoint: u32 = base_layout as u32;
                    let mut text: Option<&'e [u8]> = None;
                    match unsafe { event.uChar.UnicodeChar } {
                        0x00..=0x1F => {}
                        cp => {
                            codepoint = cp as u32;
                            let n = {
                                let buf = unsafe { &mut *std::ptr::addr_of_mut!((*this).buf) };
                                // Zig `try std.unicode.utf8Encode(...)`: an unencodable
                                // codepoint propagates as an error. A lone surrogate is
                                // already filtered above, so only `char::from_u32`
                                // rejects here.
                                match utf8_encode(codepoint, buf) {
                                    Some(n) => n,
                                    None => return Err(TtyError::InvalidUtf8),
                                }
                            };
                            text = Some(unsafe { Self::text_slice(this, n) });
                        }
                    }

                    let key = Key {
                        codepoint,
                        base_layout_codepoint: Some(base_layout as u32),
                        mods: translate_mods(event.dwControlKeyState),
                        text,
                        shifted_codepoint: None,
                    };

                    Ok(Some(match event.bKeyDown {
                        0 => Event::KeyRelease(key),
                        _ => Event::KeyPress(key),
                    }))
                }
                0x0002 => {
                    // Mouse event
                    // see https://learn.microsoft.com/en-us/windows/console/mouse-event-record-str
                    let event = unsafe { (*record).Event.MouseEvent };

                    // High word of dwButtonState represents mouse wheel. Positive is
                    // wheel_up, negative is wheel_down
                    // Low word represents button state
                    let mouse_wheel_direction: i16 = {
                        let wheelu32: u32 = event.dwButtonState >> 16;
                        let wheelu16: u16 = wheelu32 as u16;
                        wheelu16 as i16
                    };

                    let buttons: u16 = event.dwButtonState as u16;
                    // save the current state when we are done
                    // defer self.last_mouse_button_press = buttons;
                    let last =
                        unsafe { std::ptr::addr_of!((*this).last_mouse_button_press).read() };
                    let button_xor = last ^ buttons;

                    let mut event_type = mouse::Type::Press;
                    let btn: mouse::Button = match button_xor {
                        0x0000 => 'blk: {
                            // Check wheel event
                            if event.dwEventFlags & 0x0004 > 0 {
                                if mouse_wheel_direction > 0 {
                                    break 'blk mouse::Button::WheelUp;
                                } else {
                                    break 'blk mouse::Button::WheelDown;
                                }
                            }

                            // If we have no change but one of the buttons is still
                            // pressed we have a drag event. Find out which button is
                            // held down
                            if buttons > 0 && event.dwEventFlags & 0x0001 > 0 {
                                event_type = mouse::Type::Drag;
                                if buttons & 0x0001 > 0 {
                                    break 'blk mouse::Button::Left;
                                }
                                if buttons & 0x0002 > 0 {
                                    break 'blk mouse::Button::Right;
                                }
                                if buttons & 0x0004 > 0 {
                                    break 'blk mouse::Button::Middle;
                                }
                                if buttons & 0x0008 > 0 {
                                    break 'blk mouse::Button::Button8;
                                }
                                if buttons & 0x0010 > 0 {
                                    break 'blk mouse::Button::Button9;
                                }
                            }

                            if event.dwEventFlags & 0x0001 > 0 {
                                event_type = mouse::Type::Motion;
                            }
                            mouse::Button::None
                        }
                        0x0001 => {
                            if buttons & 0x0001 == 0 {
                                event_type = mouse::Type::Release;
                            }
                            mouse::Button::Left
                        }
                        0x0002 => {
                            if buttons & 0x0002 == 0 {
                                event_type = mouse::Type::Release;
                            }
                            mouse::Button::Right
                        }
                        0x0004 => {
                            if buttons & 0x0004 == 0 {
                                event_type = mouse::Type::Release;
                            }
                            mouse::Button::Middle
                        }
                        0x0008 => {
                            if buttons & 0x0008 == 0 {
                                event_type = mouse::Type::Release;
                            }
                            mouse::Button::Button8
                        }
                        0x0010 => {
                            if buttons & 0x0010 == 0 {
                                event_type = mouse::Type::Release;
                            }
                            mouse::Button::Button9
                        }
                        _ => {
                            // log.warn("unknown mouse event: {}", .{event});
                            //
                            // PORT NOTE: the `defer` still runs on this early return
                            // in Zig, so the button state is saved before returning.
                            unsafe {
                                std::ptr::addr_of_mut!((*this).last_mouse_button_press)
                                    .write(buttons)
                            };
                            return Ok(None);
                        }
                    };

                    // defer self.last_mouse_button_press = buttons;
                    unsafe {
                        std::ptr::addr_of_mut!((*this).last_mouse_button_press).write(buttons)
                    };

                    const shift: DWORD = 0x0010;
                    const alt: DWORD = 0x0001 | 0x0002;
                    const ctrl: DWORD = 0x0004 | 0x0008;
                    let mods = mouse::Modifiers {
                        shift: event.dwControlKeyState & shift > 0,
                        alt: event.dwControlKeyState & alt > 0,
                        ctrl: event.dwControlKeyState & ctrl > 0,
                    };

                    let m = Mouse {
                        col: event.dwMousePosition.X as u16, // Windows reports with 0 index
                        row: event.dwMousePosition.Y as u16, // Windows reports with 0 index
                        // PORT NOTE: upstream's `Mouse` literal omits
                        // `xoffset`/`yoffset`, which default to 0.
                        xoffset: 0,
                        yoffset: 0,
                        mods,
                        r#type: event_type,
                        button: btn,
                    };
                    Ok(Some(Event::Mouse(m)))
                }
                0x0004 => {
                    // Screen resize events
                    // NOTE: Even though the event comes with a size, it may not be
                    // accurate. We ask for the size directly when we get this event
                    let stdout = unsafe { std::ptr::addr_of!((*this).stdout).read() };
                    let mut console_info: CONSOLE_SCREEN_BUFFER_INFO =
                        unsafe { core::mem::zeroed() };
                    if unsafe { GetConsoleScreenBufferInfo(stdout, &mut console_info) } == 0 {
                        return Err(TtyError::Unexpected(unsafe { GetLastError() }));
                    }
                    let window_rect = console_info.srWindow;
                    let width = window_rect.Right - window_rect.Left + 1;
                    let height = window_rect.Bottom - window_rect.Top + 1;
                    Ok(Some(Event::Winsize(Winsize {
                        cols: width as u16,
                        rows: height as u16,
                        x_pixel: 0,
                        y_pixel: 0,
                    })))
                }
                0x0010 => {
                    // Focus events
                    Ok(Some(
                        match unsafe { (*record).Event.FocusEvent.bSetFocus } {
                            0 => Event::FocusOut,
                            _ => Event::FocusIn,
                        },
                    ))
                }
                _ => Ok(None),
            }
        }

        /// `self.buf[0..n]` with the caller's `'e` — the text buffer lives as long as
        /// the tty (see [`WindowsTty::next_event`]'s safety contract).
        ///
        /// # Safety
        /// `this` must point to a live `WindowsTty` outliving `'e`; `n <= 4`.
        unsafe fn text_slice<'e>(this: *const Self, n: usize) -> &'e [u8] {
            unsafe { core::slice::from_raw_parts(std::ptr::addr_of!((*this).buf) as *const u8, n) }
        }

        /// Get the window size from the kernel.
        ///
        /// PORT NOTE: upstream has NO `WindowsTty.getWinsize` (see the module PORT
        /// NOTE); the `Tty` alias must expose one, and this is exactly the query the
        /// resize arm of `event_from_record` makes. `fd` is unused — the console has
        /// no fd; the stdout console handle is fetched directly.
        pub fn get_winsize(_fd: libc::c_int) -> Result<Winsize, TtyError> {
            let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
            let mut console_info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { core::mem::zeroed() };
            if unsafe { GetConsoleScreenBufferInfo(stdout, &mut console_info) } == 0 {
                return Err(TtyError::IoctlError);
            }
            let r = console_info.srWindow;
            Ok(Winsize {
                cols: (r.Right - r.Left + 1) as u16,
                rows: (r.Bottom - r.Top + 1) as u16,
                x_pixel: 0,
                y_pixel: 0,
            })
        }

        /// Upstream's `.windows` arm of `handleEventGeneric` never calls
        /// `resetSignalHandler` (there is no SIGWINCH), so this is the empty
        /// counterpart the `Tty` alias needs.
        pub fn reset_signal_handler() {}
    }
}

/// The pipe-backed test mock (Zig `TestTty`, compiled under `builtin.is_test`). A
/// `libc::pipe()` pair + a `TtyWriter` sink + a fixed `get_winsize`. `libc::pipe` works
/// in the sandbox (unlike `/dev/tty`), so the Loop scaffolding is exercisable here.
///
/// PORT NOTE: Zig's `tty_writer` is an allocating `Vec` sink; the Rust port uses the
/// same [`TtyWriter`] type as `PosixTty` (over the `pipe_write` fd) so `writer()` has
/// ONE return type across ttys and the generic Loop compiles uniformly.
#[cfg(all(test, not(windows)))]
pub struct TestTty {
    /// Used for API compat
    fd: c_int,
    pipe_read: c_int,
    pipe_write: c_int,
    tty_writer: TtyWriter,
    /// Rustty-only test hook: lets a test observe that `deinit` actually ran
    /// (the vxfw `App::deinit` tty-teardown parity test, W17-B05-NORMAL-01).
    pub(crate) deinit_observer: Option<std::sync::Arc<AtomicBool>>,
}

#[cfg(all(test, not(windows)))]
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
            deinit_observer: None,
        })
    }

    pub fn deinit(self) {
        if let Some(observer) = &self.deinit_observer {
            observer.store(true, Ordering::SeqCst);
        }
        unsafe {
            libc::close(self.pipe_read);
            libc::close(self.pipe_write);
        }
    }

    pub fn writer(&mut self) -> &mut TtyWriter {
        &mut self.tty_writer
    }

    /// Narrow `tty_writer` projection (API + aliasing parity with
    /// [`PosixTty::writer_raw`] — the `Tty` alias must expose ONE surface).
    ///
    /// # Safety
    /// `this` must point to a live `TestTty`.
    pub unsafe fn writer_raw(this: *mut Self) -> *mut TtyWriter {
        unsafe { std::ptr::addr_of_mut!((*this).tty_writer) }
    }

    /// The file descriptor (API compat with [`PosixTty::fd`]).
    pub fn fd(&self) -> c_int {
        self.fd
    }

    /// Raw-receiver read (aliasing parity with [`PosixTty::read`], H1 — see the
    /// rationale there).
    ///
    /// # Safety
    /// `this` must point to a live `TestTty`; concurrent access must stay narrow (no
    /// whole-`&mut Tty` while this call blocks).
    pub unsafe fn read(this: *const Self, buf: &mut [u8]) -> Result<usize, TtyError> {
        let fd = unsafe { std::ptr::addr_of!((*this).fd).read() };
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
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
    ///
    /// PORT NOTE (signature parity, WS20): raw-receiver + `Event`-returning, matching
    /// [`windows_tty::WindowsTty::next_event`] so the `Tty` alias has ONE surface.
    /// Upstream's body is `return error.SkipZigTest;`.
    ///
    /// # Safety
    /// Never called: this arm exists only for Windows-API compat and `TestTty` is
    /// aliased as `Tty` only on non-Windows targets.
    pub unsafe fn next_event<'e>(
        _this: *mut Self,
        _parser: *mut crate::parser::Parser,
    ) -> Result<crate::event::Event<'e>, TtyError> {
        // Zig returns `error.SkipZigTest`; this path is Windows-API-only and never
        // reached on non-windows targets.
        unimplemented!("TestTty.next_event: Windows-API compat only (Zig: error.SkipZigTest)")
    }

    pub fn reset_signal_handler() {}
}

// PORT NOTE (windows): this module tests the POSIX SIGWINCH handler registry, which
// only exists on non-Windows targets (upstream registers no handler on Windows).
#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    // PORT NOTE (rustty-only, VALIDATION_REPORT §3c.5): pins the
    // `deregister_winsize` bookkeeping that `Loop::drop` relies on — every entry
    // with the matching context is removed and the survivors are compacted so
    // `handle_winch`'s `while i < idx` walk stays exact. No other test touches
    // the process-global HANDLERS (Loop::init is a no-op under cfg(test)), so no
    // serialization lock is needed. The UAF this deregistration prevents is
    // proven separately on the isolated Miri harness
    // (.port/phase-2/mirihar-chunk3b `winch_naive`/`winch_shipped`).
    #[test]
    fn deregister_winsize_removes_and_compacts() {
        extern "C" fn cb(_ctx: *mut c_void) {}
        let a = 0x1000_usize as *mut c_void;
        let b = 0x2000_usize as *mut c_void;

        PosixTty::notify_winsize(SignalHandler {
            context: a,
            callback: cb,
        })
        .unwrap();
        PosixTty::notify_winsize(SignalHandler {
            context: b,
            callback: cb,
        })
        .unwrap();
        // the same context may be registered more than once (init called twice);
        // deregistration must remove ALL of its entries.
        PosixTty::notify_winsize(SignalHandler {
            context: a,
            callback: cb,
        })
        .unwrap();
        {
            let st = HANDLERS.lock().unwrap();
            assert_eq!(3, st.idx);
        }

        PosixTty::deregister_winsize(a);
        {
            let st = HANDLERS.lock().unwrap();
            assert_eq!(1, st.idx);
            // the surviving handler compacted down to slot 0
            assert_eq!(b, st.handlers[0].as_ref().unwrap().context);
            assert!(st.handlers[1].is_none());
            assert!(st.handlers[2].is_none());
        }

        // deregistering an unknown context is a no-op
        PosixTty::deregister_winsize(0x3000_usize as *mut c_void);
        {
            let st = HANDLERS.lock().unwrap();
            assert_eq!(1, st.idx);
        }

        PosixTty::deregister_winsize(b);
        let st = HANDLERS.lock().unwrap();
        assert_eq!(0, st.idx);
        assert!(st.handlers.iter().all(|h| h.is_none()));
    }
}
