//! rustty-os::pty
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/pty.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).
//!
//! PORT: the Zig reference imports `std`, `builtin`,
//! `windows = @import("os/main.zig").windows`, `posix = std.posix`, and
//! `assert = quirks.inlineAssert`, plus `c = @import("pty-c")` (the vendored
//! `pty.c`, which is includes + `#define`s that surface the platform `TIOC*`
//! ioctl constants). In Rust the POSIX/PTY syscalls and constants come from
//! `libc` (R3: a binding to the SAME system libraries the reference's
//! `std.posix`/`pty-c` target — not a reimplementation); the `windows` module
//! is `crate::windows`; the assert seam is `rustty_util::quirks::inlineAssert`.
//! `pty.c` is still compiled standalone (see build.rs) per R3.
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

#[cfg(not(windows))]
use rustty_util::quirks::inlineAssert as assert;

#[allow(dead_code)]
const LOG_SCOPE: &str = "pty";

#[allow(dead_code)]
fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}

#[allow(dead_code)]
fn log_err(msg: &str) {
    eprintln!("[err] [{LOG_SCOPE}] {msg}");
}

/// Redeclare this winsize struct so we can just use a Zig struct. This
/// layout should be correct on all tested platforms. The defaults on this
/// are some reasonable screen size but you should probably not use them.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct winsize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

impl Default for winsize {
    fn default() -> Self {
        Self {
            ws_row: 100,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 600,
        }
    }
}

// PORT: `pub const Pty = switch (builtin.os.tag) { .windows => WindowsPty,
// .ios => NullPty, else => PosixPty };`. Selected at compile time via `cfg`.
#[cfg(target_os = "ios")]
pub use NullPty as Pty;
#[cfg(not(any(windows, target_os = "ios")))]
pub use PosixPty as Pty;
#[cfg(windows)]
pub use WindowsPty as Pty;

/// The modes of a pty. Not all of these modes are supported on
/// all platforms but all platforms share the same mode struct.
///
/// The default values of fields in this struct are set to the
/// most typical values for a pty. This makes it easier for cross-platform
/// code which doesn't support all of the modes to work correctly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mode {
    /// ICANON on POSIX
    pub canonical: bool,

    /// ECHO on POSIX
    pub echo: bool,
}

impl Default for Mode {
    fn default() -> Self {
        Self {
            canonical: true,
            echo: true,
        }
    }
}

// PARITY: the reference variant names are `foreground_pid` / `tty_name`
// (snake_case); preserved per R1.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcessInfo {
    /// The PID of the process that controls the PTY.
    foreground_pid,
    /// Gets the name of the slave PTY. Returned name points to an internal buffer
    /// so it should not be modified or freed.
    tty_name,
}

// PORT: `pub fn Type(comptime info: ProcessInfo) type` returns `u64` for
// `foreground_pid` and `[:0]const u8` for `tty_name`. Rust has no comptime
// type-from-value; the two `getProcessInfo` accessors below return the concrete
// types (`u64` / `&CStr`) matching this mapping.

// A pty implementation that does nothing.
//
// TODO: This should be removed. This is only temporary until we have
// a termio that doesn't use a pty. This isn't used in any user-facing
// artifacts, this is just a stopgap to get compilation to work on iOS.
#[cfg(target_os = "ios")]
pub struct NullPty {
    pub master: NullPtyFd,
    pub slave: NullPtyFd,
}

#[cfg(target_os = "ios")]
pub type NullPtyFd = libc::c_int;

#[cfg(target_os = "ios")]
#[derive(Debug)]
pub enum NullPtyError {}

#[cfg(target_os = "ios")]
impl NullPty {
    pub fn open(size: winsize) -> Result<Pty, NullPtyError> {
        let _ = size;
        Ok(Pty {
            master: 0,
            slave: 0,
        })
    }

    pub fn deinit(&mut self) {}

    pub fn getMode(&self) -> Result<Mode, NullPtyError> {
        Ok(Mode::default())
    }

    pub fn setSize(&mut self, size: winsize) -> Result<(), NullPtyError> {
        let _ = size;
        Ok(())
    }

    pub fn childPreExec(&self) -> Result<(), NullPtyError> {
        Ok(())
    }

    /// Get information about the process(es) attached to the PTY. Returns
    /// `null` if there was an error getting the information or the information
    /// is not available on a particular platform.
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        None
    }

    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        None
    }
}

/// Posix PTY creation and management. This is just a thin layer on top
/// of Posix syscalls. The caller is responsible for detail-oriented handling
/// of the returned file handles.
#[cfg(not(any(windows, target_os = "ios")))]
pub struct PosixPty {
    /// The file descriptors for the master and slave side of the pty.
    /// The slave side is never closed automatically by this struct
    /// so the caller is responsible for closing it if things
    /// go wrong.
    pub master: PosixPtyFd,
    pub slave: PosixPtyFd,

    /// Buffer for storage of slave tty name so that we don't have to recompute
    /// it every time we need it.
    tty_name_buf: [u8; MAX_PATH_BYTES + 1],
    /// The name of slave tty. If `null` it has not yet been computed or
    /// may not be available. Should not be accessed directly, but through
    /// `self.getProcessInfo(.tty_name)`
    // PORT: the reference stores `?[:0]const u8` pointing into `tty_name_buf`. In
    // Rust we store the byte length (the NUL-terminated name is `&tty_name_buf[..len]`),
    // avoiding a self-referential pointer; `None` means not yet computed.
    tty_name: Option<usize>,
}

#[cfg(not(any(windows, target_os = "ios")))]
pub type PosixPtyFd = libc::c_int;

// PORT: `std.fs.max_path_bytes`. The reference's buffer is
// `[std.fs.max_path_bytes:0]u8` (NUL-terminated). We size for the path bytes
// plus the trailing NUL.
#[cfg(not(any(windows, target_os = "ios")))]
const MAX_PATH_BYTES: usize = libc::PATH_MAX as usize;

#[cfg(not(any(windows, target_os = "ios")))]
#[derive(Debug, PartialEq, Eq)]
pub enum PosixPtyError {
    OpenptyFailed,
    GetModeFailed,
    IoctlFailed,
    OperationNotSupported,
    ProcessGroupFailed,
    SetControllingTerminalFailed,
}

#[cfg(not(any(windows, target_os = "ios")))]
impl PosixPty {
    /// Open a new PTY with the given initial size.
    pub fn open(size: winsize) -> Result<Pty, PosixPtyError> {
        // Need to copy so that it becomes non-const.
        let mut sizeCopy = size;

        let mut master_fd: PosixPtyFd = 0;
        let mut slave_fd: PosixPtyFd = 0;
        let rc = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                (&mut sizeCopy as *mut winsize).cast(),
            )
        };
        if rc < 0 {
            return Err(PosixPtyError::OpenptyFailed);
        }
        // PORT: `errdefer { _ = posix.system.close(master_fd); _ = posix.system.close(slave_fd); }`.
        let mut errdefer_guard = OpenErrdefer {
            master_fd,
            slave_fd,
            armed: true,
        };

        // Set CLOEXEC on the master fd, only the slave fd should be inherited
        // by the child process (shell/command).
        'cloexec: {
            let flags = unsafe { libc::fcntl(master_fd, libc::F_GETFD, 0) };
            if flags < 0 {
                log_warn(&format!(
                    "error getting flags for master fd err={}",
                    std::io::Error::last_os_error()
                ));
                break 'cloexec;
            }

            let rc = unsafe { libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
            if rc < 0 {
                log_warn(&format!(
                    "error setting CLOEXEC on master fd err={}",
                    std::io::Error::last_os_error()
                ));
                break 'cloexec;
            }
        }

        // Enable UTF-8 mode. I think this is on by default on Linux but it
        // is NOT on by default on macOS so we ensure that it is always set.
        let mut attrs: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(master_fd, &mut attrs) } != 0 {
            return Err(PosixPtyError::OpenptyFailed);
        }
        attrs.c_iflag |= libc::IUTF8;
        if unsafe { libc::tcsetattr(master_fd, libc::TCSANOW, &attrs) } != 0 {
            return Err(PosixPtyError::OpenptyFailed);
        }

        errdefer_guard.armed = false;
        Ok(Pty {
            master: master_fd,
            slave: slave_fd,
            tty_name_buf: [0; MAX_PATH_BYTES + 1],
            tty_name: None,
        })
    }

    pub fn deinit(&mut self) {
        unsafe {
            libc::close(self.master);
        }
        // PORT: `self.* = undefined;` — the value is left in an unspecified state;
        // Rust drops it after the borrow ends.
    }

    /// Build a throwaway `Pty` that carries only a `master` fd, for querying
    /// the termios `Mode` (see [`getMode`](Self::getMode)).
    //
    // PORT NOTE: the reference `termio.Exec.termiosTimer` builds an inline
    // throwaway `Pty{ .master = exec.read_thread_fd, .slave = undefined }` purely
    // to call `.getMode()`. Rust struct privacy (the `tty_name*` cache fields)
    // forbids that struct literal from outside this module, so this constructor
    // expresses it faithfully: `master` is set, `slave` is a placeholder (never
    // read — `getMode` reads only `self.master`), and the tty-name cache starts
    // empty. Returns the `Pty` alias (== `PosixPty` on this target).
    pub fn init_master_only(master: PosixPtyFd) -> PosixPty {
        PosixPty {
            master,
            // .slave = undefined — never read by `getMode`; a safe placeholder.
            slave: -1,
            tty_name_buf: [0; MAX_PATH_BYTES + 1],
            tty_name: None,
        }
    }

    pub fn getMode(&self) -> Result<Mode, PosixPtyError> {
        let mut attrs: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(self.master, &mut attrs) } != 0 {
            return Err(PosixPtyError::GetModeFailed);
        }

        Ok(Mode {
            canonical: (attrs.c_lflag & libc::ICANON) != 0,
            echo: (attrs.c_lflag & libc::ECHO) != 0,
        })
    }

    /// Return the size of the pty.
    pub fn getSize(&self) -> Result<winsize, PosixPtyError> {
        let mut ws: winsize = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::ioctl(
                self.master,
                libc::TIOCGWINSZ,
                &mut ws as *mut winsize as *mut std::os::raw::c_void,
            )
        } < 0
        {
            return Err(PosixPtyError::IoctlFailed);
        }

        Ok(ws)
    }

    /// Set the size of the pty.
    pub fn setSize(&mut self, size: winsize) -> Result<(), PosixPtyError> {
        if unsafe {
            libc::ioctl(
                self.master,
                libc::TIOCSWINSZ,
                &size as *const winsize as *const std::os::raw::c_void,
            )
        } < 0
        {
            return Err(PosixPtyError::IoctlFailed);
        }
        Ok(())
    }

    /// This should be called prior to exec in the forked child process
    /// in order to setup the tty properly.
    pub fn childPreExec(&self) -> Result<(), PosixPtyError> {
        // Reset our signals
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = libc::SIG_DFL;
        unsafe {
            sa.sa_mask = std::mem::zeroed();
        }
        sa.sa_flags = 0;
        let reset = |sig: libc::c_int| unsafe {
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        };
        reset(libc::SIGABRT);
        reset(libc::SIGALRM);
        reset(libc::SIGBUS);
        reset(libc::SIGCHLD);
        reset(libc::SIGFPE);
        reset(libc::SIGHUP);
        reset(libc::SIGILL);
        reset(libc::SIGINT);
        reset(libc::SIGPIPE);
        reset(libc::SIGSEGV);
        reset(libc::SIGTRAP);
        reset(libc::SIGTERM);
        reset(libc::SIGQUIT);

        // Create a new process group
        if unsafe { libc::setsid() } < 0 {
            return Err(PosixPtyError::ProcessGroupFailed);
        }

        // Set controlling terminal
        let rc = unsafe { libc::ioctl(self.slave, libc::TIOCSCTTY as _, 0 as libc::c_ulong) };
        if rc != 0 {
            log_err(&format!(
                "error setting controlling terminal errno={}",
                std::io::Error::last_os_error()
            ));
            return Err(PosixPtyError::SetControllingTerminalFailed);
        }

        // Can close master/slave pair now
        unsafe {
            libc::close(self.slave);
            libc::close(self.master);
        }
        Ok(())
    }

    /// Get information about the process(es) attached to the PTY. Returns
    /// `null` if there was an error getting the information or the information
    /// is not available on a particular platform.
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            let mut pgrp: i32 = 0;
            let rc = unsafe { libc::tcgetpgrp(self.master) };
            // PORT: the reference uses `linux.tcgetpgrp(master, &pgrp)` returning a
            // raw rc; libc's `tcgetpgrp` returns the pgid directly or -1 on error.
            let _ = &mut pgrp;
            if rc < 0 {
                return None;
            }
            Some(rc as u64)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let rc = unsafe { libc::tcgetpgrp(self.master) };
            if rc < 0 {
                return None;
            }
            Some(rc as u64)
        }
    }

    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        if let Some(len) = self.tty_name {
            return std::ffi::CStr::from_bytes_with_nul(&self.tty_name_buf[..len + 1]).ok();
        }

        #[cfg(target_os = "macos")]
        {
            // The macOS TIOCPTYGNAME ioctl does not allow us to
            // specify the length of the buffer passed to it, but
            // expects it to be at least 128 bytes long.
            assert(self.tty_name_buf.len() >= 128);
            let rc = unsafe {
                libc::ioctl(
                    self.master,
                    libc::TIOCPTYGNAME as _,
                    self.tty_name_buf.as_mut_ptr(),
                )
            };
            if rc == 0 {
                let len = sliceTo0(&self.tty_name_buf);
                self.tty_name = Some(len);
                std::ffi::CStr::from_bytes_with_nul(&self.tty_name_buf[..len + 1]).ok()
            } else {
                log_err(&format!(
                    "error getting name of slave PTY errno={}",
                    std::io::Error::last_os_error()
                ));
                None
            }
        }
        #[cfg(target_os = "linux")]
        {
            let rc = unsafe {
                libc::ptsname_r(
                    self.master,
                    self.tty_name_buf.as_mut_ptr() as *mut std::os::raw::c_char,
                    self.tty_name_buf.len(),
                )
            };
            if rc != 0 {
                return None;
            }
            let len = sliceTo0(&self.tty_name_buf);
            self.tty_name = Some(len);
            std::ffi::CStr::from_bytes_with_nul(&self.tty_name_buf[..len + 1]).ok()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }
}

// PORT: `errdefer` helper for `PosixPty.open` — closes both fds if the open path
// fails before returning success.
#[cfg(not(any(windows, target_os = "ios")))]
struct OpenErrdefer {
    master_fd: PosixPtyFd,
    slave_fd: PosixPtyFd,
    armed: bool,
}

#[cfg(not(any(windows, target_os = "ios")))]
impl Drop for OpenErrdefer {
    fn drop(&mut self) {
        if self.armed {
            unsafe {
                libc::close(self.master_fd);
                libc::close(self.slave_fd);
            }
        }
    }
}

// PORT: `std.mem.sliceTo(&buf, 0)` — length of the bytes before the first NUL.
#[cfg(not(any(windows, target_os = "ios")))]
fn sliceTo0(buf: &[u8]) -> usize {
    buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
}

/// Windows PTY creation and management.
#[cfg(windows)]
pub struct WindowsPty {
    out_pipe: crate::windows::HANDLE,
    in_pipe: crate::windows::HANDLE,
    out_pipe_pty: crate::windows::HANDLE,
    in_pipe_pty: crate::windows::HANDLE,
    pseudo_console: crate::windows::exp::HPCON,
    size: winsize,
}

#[cfg(windows)]
pub type WindowsPtyFd = crate::windows::HANDLE;

#[cfg(windows)]
#[derive(Debug)]
pub enum WindowsPtyError {
    Unexpected,
    ResizeFailed,
}

// Process-wide counter for pipe names
#[cfg(windows)]
static pipe_name_counter: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

#[cfg(windows)]
impl WindowsPty {
    /// Open a new PTY with the given initial size.
    pub fn open(size: winsize) -> Result<Pty, Box<dyn std::error::Error>> {
        use crate::windows;
        use std::sync::atomic::Ordering;

        let mut pty = WindowsPty {
            out_pipe: std::ptr::null_mut(),
            in_pipe: std::ptr::null_mut(),
            out_pipe_pty: std::ptr::null_mut(),
            in_pipe_pty: std::ptr::null_mut(),
            pseudo_console: std::ptr::null_mut(),
            size,
        };

        let pipe_path = format!(
            "\\\\.\\pipe\\LOCAL\\ghostty-pty-{}-{}",
            windows::GetCurrentProcessId(),
            pipe_name_counter.fetch_add(1, Ordering::Relaxed),
        );

        let mut pipe_path_w: Vec<u16> = pipe_path.encode_utf16().collect();
        pipe_path_w.push(0);

        let security_attributes = windows::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows::SECURITY_ATTRIBUTES>() as windows::DWORD,
            bInheritHandle: windows::FALSE,
            lpSecurityDescriptor: std::ptr::null_mut(),
        };

        unsafe {
            pty.in_pipe = windows::kernel32::CreateNamedPipeW(
                pipe_path_w.as_ptr(),
                windows::PIPE_ACCESS_OUTBOUND
                    | windows::exp::FILE_FLAG_FIRST_PIPE_INSTANCE
                    | windows::FILE_FLAG_OVERLAPPED,
                windows::PIPE_TYPE_BYTE,
                1,
                4096,
                4096,
                0,
                &security_attributes,
            );
            if pty.in_pipe == windows::INVALID_HANDLE_VALUE {
                return windows::unexpectedError(windows::kernel32::GetLastError())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
            }

            let mut security_attributes_read = windows::SECURITY_ATTRIBUTES {
                nLength: security_attributes.nLength,
                bInheritHandle: security_attributes.bInheritHandle,
                lpSecurityDescriptor: security_attributes.lpSecurityDescriptor,
            };
            pty.in_pipe_pty = windows::kernel32::CreateFileW(
                pipe_path_w.as_ptr(),
                windows::GENERIC_READ,
                0,
                &mut security_attributes_read,
                windows::OPEN_EXISTING,
                windows::FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            );
            if pty.in_pipe_pty == windows::INVALID_HANDLE_VALUE {
                return windows::unexpectedError(windows::kernel32::GetLastError())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
            }

            // The in_pipe needs to be created as a named pipe, since anonymous
            // pipes created with CreatePipe do not support overlapped operations,
            // and the IOCP backend of libxev only uses overlapped operations on files.
            //
            // It would be ideal to use CreatePipe here, so that our pipe isn't
            // visible to any other processes.

            // if (windows.exp.kernel32.CreatePipe(&pty.in_pipe_pty, &pty.in_pipe, null, 0) == 0) {
            //     return windows.unexpectedError(windows.kernel32.GetLastError());
            // }
            // errdefer {
            //     _ = windows.CloseHandle(pty.in_pipe_pty);
            //     _ = windows.CloseHandle(pty.in_pipe);
            // }

            if windows::exp::kernel32::CreatePipe(
                &mut pty.out_pipe,
                &mut pty.out_pipe_pty,
                std::ptr::null(),
                0,
            ) == 0
            {
                return windows::unexpectedError(windows::kernel32::GetLastError())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
            }

            windows::SetHandleInformation(pty.in_pipe, windows::HANDLE_FLAG_INHERIT, 0)?;
            windows::SetHandleInformation(pty.in_pipe_pty, windows::HANDLE_FLAG_INHERIT, 0)?;
            windows::SetHandleInformation(pty.out_pipe, windows::HANDLE_FLAG_INHERIT, 0)?;
            windows::SetHandleInformation(pty.out_pipe_pty, windows::HANDLE_FLAG_INHERIT, 0)?;

            let result = windows::exp::kernel32::CreatePseudoConsole(
                windows::COORD {
                    X: size.ws_col as i16,
                    Y: size.ws_row as i16,
                },
                pty.in_pipe_pty,
                pty.out_pipe_pty,
                0,
                &mut pty.pseudo_console,
            );
            if result != windows::S_OK {
                return Err(Box::new(WindowsPtyError::Unexpected));
            }
        }

        pty.size = size;
        Ok(pty)
    }

    pub fn deinit(&mut self) {
        use crate::windows;
        windows::CloseHandle(self.in_pipe_pty);
        windows::CloseHandle(self.in_pipe);
        windows::CloseHandle(self.out_pipe_pty);
        windows::CloseHandle(self.out_pipe);
        unsafe {
            windows::exp::kernel32::ClosePseudoConsole(self.pseudo_console);
        }
    }

    /// Return the size of the pty.
    pub fn getSize(&self) -> Result<winsize, WindowsPtyError> {
        Ok(self.size)
    }

    /// Set the size of the pty.
    pub fn setSize(&mut self, size: winsize) -> Result<(), WindowsPtyError> {
        use crate::windows;
        let result = unsafe {
            windows::exp::kernel32::ResizePseudoConsole(
                self.pseudo_console,
                windows::COORD {
                    X: size.ws_col as i16,
                    Y: size.ws_row as i16,
                },
            )
        };

        if result != windows::S_OK {
            return Err(WindowsPtyError::ResizeFailed);
        }
        self.size = size;
        Ok(())
    }

    /// Get information about the process(es) attached to the PTY. Returns
    /// `null` if there was an error getting the information or the information
    /// is not available on a particular platform.
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        None
    }

    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS pty: openpty/ioctl/ptsname syscalls
    fn pty_open_size_and_name() {
        let mut ws = winsize {
            ws_row: 50,
            ws_col: 80,
            ws_xpixel: 1,
            ws_ypixel: 1,
        };

        let mut pty = Pty::open(ws).unwrap();
        // PORT: `defer pty.deinit();` — the reference closes the master fd at
        // scope exit. We call `pty.deinit()` explicitly at the end of the test so
        // the borrows for the assertions stay simple; the effect is identical.

        // Initialize size should match what we gave it
        assert_eq!(ws, pty.getSize().unwrap());

        // Can set and read new sizes
        ws.ws_row *= 2;
        pty.setSize(ws).unwrap();
        assert_eq!(ws, pty.getSize().unwrap());

        // PORT: the reference switches on `builtin.os.tag` to assert the tty name
        // prefix per platform. The independent expected prefixes come from the OS
        // contract (POSIX `/dev/...`; Linux `/dev/pts/...`), not from the code
        // under test.
        #[cfg(any(target_os = "freebsd", target_os = "linux", target_os = "macos"))]
        {
            let name_bytes = pty.getTtyName().unwrap().to_bytes().to_vec();
            #[cfg(target_os = "freebsd")]
            assert!(name_bytes.starts_with(b"/dev/"));
            #[cfg(target_os = "linux")]
            assert!(name_bytes.starts_with(b"/dev/pts/"));
            #[cfg(target_os = "macos")]
            assert!(name_bytes.starts_with(b"/dev/"));
        }
        #[cfg(not(any(target_os = "freebsd", target_os = "linux", target_os = "macos")))]
        assert!(pty.getTtyName().is_none());

        pty.deinit();
    }
}
