//! Command launches sub-processes. This is an alternate implementation to the
//! Zig std.process.Child since at the time of authoring this, std.process.Child
//! didn't support the options necessary to spawn a shell attached to a pty.
//!
//! Consequently, I didn't implement a lot of features that std.process.Child
//! supports because we didn't need them. Cross-platform subprocessing is not
//! a trivial thing to implement (I've done it in three separate languages now)
//! so if we want to replatform onto std.process.Child I'd love to do that.
//! This was just the fastest way to get something built.
//!
//! Issues with std.process.Child:
//!
//!   * No pre_exec callback for logic after fork but before exec.
//!   * posix_spawn is used for Mac, but doesn't support the necessary
//!     features for tty setup.
//!
//! PORTS: ghostty src/Command.zig (10 `test{}` blocks — all ported).
//! RELOCATED into rustty-termio (break #13b, blueprint §1): placing Command in
//! rustty-core would cycle (termio/Exec constructs+starts Command while core
//! already deps termio). Command's only consumers are termio/Exec and core's
//! Surface, which already deps termio and reaches it as
//! `rustty_termio::Command` (crate-root re-export — the reference's flat
//! `Command` symbol).
//!
//! PORT STATUS: ported (integration-tier stage 3).

// PARITY: the reference's identifiers are camelCase (`startPosix`, `setupFd`,
// `setData`); preserved verbatim per R1/R2 (termio stage-2 precedent).
#![allow(non_snake_case)]

// const Command = @This();
//
// const std = @import("std");
// const builtin = @import("builtin");
// const configpkg = @import("config.zig");
// const global_state = &@import("global.zig").state;
// const internal_os = @import("os/main.zig");
// const windows = internal_os.windows;
// const TempDir = internal_os.TempDir;
// const mem = std.mem;
// const linux = std.os.linux;
// const posix = std.posix;
// const debug = std.debug;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const File = std.fs.File;
// const EnvMap = std.process.EnvMap;
// const apprt = @import("apprt.zig");
//
// PORT NOTE: `configpkg` -> rustty-config; `global_state` ->
// `rustty_global::STATE` (the OnceLock analogue of the reference's global);
// `internal_os.windows` -> `rustty_os::windows`; `TempDir` -> the rustty-os
// tempdir (tests); `linux`/`posix` -> the `libc` binding (R3: `libc` binds the
// SAME system library Zig's `std.posix`/`std.os.linux` call into — rustty-os
// precedent); `File` -> `std::fs::File`; `EnvMap` -> the `EnvMap` alias below.
// `apprt` is imported by the reference ONLY for the comptime
// `@hasDecl(apprt.runtime, "pre_exec"/"post_fork")` checks on
// `RtPreExecInfo`/`RtPostForkInfo` below — realized only by the GTK apprt
// (ported last); NO rustty-apprt-api edge is taken here (blueprint §1).
use allocator_api2::alloc::Global as Allocator;
use rustty_config::Config;
use std::ffi::{c_char, c_void, CStr};
use std::fs::File;

#[cfg(not(windows))]
use core::fmt::Write as _;
#[cfg(not(windows))]
use std::os::fd::{AsRawFd as _, RawFd};

/// PORT NOTE: `EnvMap = std.process.EnvMap` — a string-keyed hash map of
/// environment variables (the same shape rustty-os `getEnvMap` returns).
pub type EnvMap = std::collections::HashMap<String, String>;

/// Function prototype for a function executed /in the child process/ after the
/// fork, but before exec'ing the command. If the function returns a u8, the
/// child process will be exited with that error code.
//
// PORT NOTE: reference `const PreExecFn = fn (*Command) ?u8;` — the struct fields
// below carry `?*const PreExecFn`, i.e. optional function POINTERS, which is
// exactly Rust's `Option<fn(..)>`. (Zig leaks the file-private type through the
// pub fields; Rust requires the alias itself to be pub.)
pub type PreExecFn = for<'a, 'b> fn(&'b mut Command<'a>) -> Option<u8>;

/// Allowable set of errors that can be returned by a post fork function. Any
/// errors will result in the failure to create the surface.
//
// PORT NOTE: reference `pub const PostForkError = error{PostForkError};` — a
// single-member Zig error set; a unit struct carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostForkError;

impl core::fmt::Display for PostForkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "error.PostForkError")
    }
}

impl std::error::Error for PostForkError {}

/// Function prototype for a function executed /in the parent process/
/// after the fork.
//
// PORT NOTE: reference `const PostForkFn = fn (*Command) PostForkError!void;`.
pub type PostForkFn = for<'a, 'b> fn(&'b mut Command<'a>) -> Result<(), PostForkError>;

/// PORT NOTE (Rust-only glue): the reference's `start`/`wait` error sets are
/// *inferred* Zig error unions (`!void`/`!Exit`). Rust names the union
/// explicitly: the posix fork/dup/fcntl failures surfaced by
/// `startPosix`/`setupFd`, the very specific `error.ExecFailedInChild` child
/// marker, and the post-fork callback's `error.PostForkError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `error.ExecFailedInChild` — returned only in the forked child when the
    /// exec (or the pre-exec fd setup) failed; see `startPosix` and
    /// `testingStart`'s handling contract.
    ExecFailedInChild,
    /// `error.PostForkError` from the rt post-fork callback (`PostForkError`).
    PostForkError,
    /// Zig `posix.fork()`: EAGAIN/ENOMEM -> `error.SystemResources`.
    SystemResources,
    /// `error.Locked` (`setupFd` dup3 EAGAIN/EACCES, linux).
    Locked,
    /// `error.FileBusy` (`setupFd` dup3 EBUSY, linux).
    FileBusy,
    /// `error.PermissionDenied` (`setupFd` dup3 EPERM, linux).
    PermissionDenied,
    /// `error.ProcessFdQuotaExceeded` (dup3/dup2 EMFILE).
    ProcessFdQuotaExceeded,
    /// `error.DeadLock` (`setupFd` dup3 EDEADLK, linux).
    DeadLock,
    /// `error.LockedRegionLimitExceeded` (`setupFd` dup3 ENOLCK, linux).
    LockedRegionLimitExceeded,
    /// Zig `error.InvalidUtf8`/`error.InvalidWtf8` — the windows
    /// `utf8ToUtf16LeAllocZ` re-encode of cwd/command line.
    InvalidUtf8,
    /// `posix.unexpectedErrno(err)` — any other errno (payload carries it).
    Unexpected(i32),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // (Zig formats an error value as `error.Name`.)
        let name = match self {
            Error::ExecFailedInChild => "ExecFailedInChild",
            Error::PostForkError => "PostForkError",
            Error::SystemResources => "SystemResources",
            Error::Locked => "Locked",
            Error::FileBusy => "FileBusy",
            Error::PermissionDenied => "PermissionDenied",
            Error::ProcessFdQuotaExceeded => "ProcessFdQuotaExceeded",
            Error::DeadLock => "DeadLock",
            Error::LockedRegionLimitExceeded => "LockedRegionLimitExceeded",
            Error::InvalidUtf8 => "InvalidUtf8",
            Error::Unexpected(_) => "Unexpected",
        };
        write!(f, "error.{name}")
    }
}

impl std::error::Error for Error {}

// PORT NOTE: the reference is a top-level file-struct (`Command.zig`,
// `const Command = @This();`); the Rust port names it `Command`. `'a` carries
// the borrows of the caller-owned path/args/env/cwd/File fields (Zig's slices
// and copyable fd wrappers). Fields with Zig default values (`= null` /
// `= .{}`) are listed explicitly in Rust struct literals; `path`/`args` and
// the callback/info fields have NO defaults in the reference and must always
// be provided — the exhaustive Rust literal preserves that contract.
pub struct Command<'a> {
    /// Path to the command to run. This doesn't have to be an absolute path,
    /// because use exec functions that search the PATH, if necessary.
    ///
    /// This field is null-terminated to avoid a copy for the sake of
    /// adding a null terminator since POSIX systems are so common.
    //
    // PORT NOTE: `path: [:0]const u8` -> `&CStr` (Rust's borrowed null-terminated
    // byte slice; NEVER a lossy String — R1 byte-slice rule).
    pub path: &'a CStr,

    /// Command-line arguments. It is the responsibility of the caller to set
    /// args[0] to the command. If args is empty then args[0] will automatically
    /// be set to equal path.
    //
    // PORT NOTE: `args: []const [:0]const u8` -> `&[&CStr]`.
    pub args: &'a [&'a CStr],

    /// Environment variables for the child process. If this is null, inherits
    /// the environment variables from this process. These are the exact
    /// environment variables to set; these are /not/ merged.
    //
    // PORT NOTE: `env: ?*const EnvMap = null` -> borrowed `Option<&EnvMap>`.
    pub env: Option<&'a EnvMap>,

    /// Working directory to change to in the child process. If not set, the
    /// working directory of the calling process is preserved.
    //
    // PORT NOTE: `cwd: ?[]const u8 = null` -> `Option<&[u8]>` (raw path bytes).
    pub cwd: Option<&'a [u8]>,

    /// The file handle to set for stdin/out/err. If this isn't set, we do
    /// nothing explicitly so it is up to the behavior of the operating system.
    //
    // PORT NOTE: `?File = null` -> `Option<&File>`. Zig's `File` is a copyable fd
    // wrapper that Command never closes; the Rust borrow preserves exactly
    // that (the caller keeps ownership and closes).
    pub stdin: Option<&'a File>,
    pub stdout: Option<&'a File>,
    pub stderr: Option<&'a File>,

    /// If set, this will be executed /in the child process/ after fork but
    /// before exec. This is useful to setup some state in the child before the
    /// exec process takes over, such as signal handlers, setsid, setuid, etc.
    pub os_pre_exec: Option<PreExecFn>,

    /// If set, this will be executed /in the child process/ after fork but
    /// before exec. This is useful to setup some state in the child before the
    /// exec process takes over, such as signal handlers, setsid, setuid, etc.
    pub rt_pre_exec: Option<PreExecFn>,

    /// Configuration information needed by the apprt pre exec function. Note
    /// that this should be a trivially copyable struct and not require any
    /// allocation/deallocation.
    pub rt_pre_exec_info: RtPreExecInfo,

    /// If set, this will be executed in the /in the parent process/ after the fork.
    pub rt_post_fork: Option<PostForkFn>,

    /// Configuration information needed by the apprt post fork function. Note
    /// that this should be a trivially copyable struct and not require any
    /// allocation/deallocation.
    pub rt_post_fork_info: RtPostForkInfo,

    /// If set, then the process will be created attached to this pseudo console.
    /// `stdin`, `stdout`, and `stderr` will be ignored if set.
    //
    // PORT NOTE: reference `pseudo_console: if (builtin.os.tag == .windows)
    // ?windows.exp.HPCON else void = if (windows) null else {}` — the
    // comptime-windows field maps to a cfg-gated field (absent on posix, like
    // the reference's zero-size `void`).
    #[cfg(windows)]
    pub pseudo_console: Option<rustty_os::windows::exp::HPCON>,

    /// User data that is sent to the callback. Set with setData and getData
    /// for a more user-friendly API.
    //
    // PORT NOTE: `data: ?*anyopaque = null` -> `Option<*mut c_void>`.
    pub data: Option<*mut c_void>,

    /// Process ID is set after start is called.
    //
    // PORT NOTE: `pid: ?posix.pid_t = null`. On windows the reference stores the
    // process HANDLE here (`self.pid = process_information.hProcess`).
    #[cfg(not(windows))]
    pub pid: Option<libc::pid_t>,
    #[cfg(windows)]
    pub pid: Option<rustty_os::windows::HANDLE>,
}

/// The various methods a process may exit.
//
// PORT NOTE: reference `pub const Exit = if (builtin.os.tag == .windows)
// union(enum) { Exited: u32 } else union(enum) { ... }` — cfg-gated enums.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Exited(u32),
}

/// The various methods a process may exit.
#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Exited by normal exit call, value is exit status
    Exited(u8),

    /// Exited by a signal, value is the signal
    Signal(u32),

    /// Exited by a stop signal, value is signal
    Stopped(u32),

    /// Unknown exit reason, value is the status from waitpid
    Unknown(u32),
}

#[cfg(not(windows))]
impl Exit {
    pub fn init(status: u32) -> Exit {
        // PORT NOTE: Zig `posix.W.IFEXITED/EXITSTATUS/IFSIGNALED/TERMSIG/IFSTOPPED/
        // STOPSIG` == the libc W* status macros (same libc).
        let st = status as libc::c_int;
        if libc::WIFEXITED(st) {
            Exit::Exited(libc::WEXITSTATUS(st) as u8)
        } else if libc::WIFSIGNALED(st) {
            Exit::Signal(libc::WTERMSIG(st) as u32)
        } else if libc::WIFSTOPPED(st) {
            Exit::Stopped(libc::WSTOPSIG(st) as u32)
        } else {
            Exit::Unknown(status)
        }
    }
}

/// Configuration information needed by the apprt pre exec function. Note
/// that this should be a trivially copyable struct and not require any
/// allocation/deallocation.
//
// PORT NOTE (GROWTH POINT — GTK seam): reference `pub const RtPreExecInfo =
// if (@hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.PreExecInfo
// else struct { pub inline fn init(_: *const configpkg.Config) @This() {
// return .{}; } };` — the comptime `@hasDecl` branch is realized ONLY by the
// GTK apprt (ported last). Until the GTK backend lands and specializes this
// type IN PLACE, the trivial fallback struct is the definition (blueprint §1:
// do NOT add this to rustty-apprt-api).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtPreExecInfo;

impl RtPreExecInfo {
    // pub inline fn init(_: *const configpkg.Config) @This() { return .{}; }
    #[inline]
    pub fn init(_: &Config) -> Self {
        Self
    }
}

/// Configuration information needed by the apprt post fork function. Note
/// that this should be a trivially copyable struct and not require any
/// allocation/deallocation.
//
// PORT NOTE (GROWTH POINT — GTK seam): reference `pub const RtPostForkInfo =
// if (@hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.PostForkInfo
// else struct { ... };` — same GTK-realized comptime branch as RtPreExecInfo
// above; trivial fallback until GTK lands (blueprint §1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtPostForkInfo;

impl RtPostForkInfo {
    // pub inline fn init(_: *const configpkg.Config) @This() { return .{}; }
    #[inline]
    pub fn init(_: &Config) -> Self {
        Self
    }
}

impl<'a> Command<'a> {
    /// Start the subprocess. This returns immediately once the child is started.
    ///
    /// After this is successful, self.pid is available.
    //
    // PORT NOTE: `pub fn start(self: *Command, alloc: Allocator) !void`.
    pub fn start(&mut self, _alloc: Allocator) -> Result<(), Error> {
        // Use an arena allocator for the temporary allocations we need in this func.
        // IMPORTANT: do all allocation prior to the fork(). I believe it is undefined
        // behavior if you malloc between fork and exec. The source of the Zig
        // stdlib seems to verify this as well as Go.
        //
        // PORT NOTE: `var arena_allocator = std.heap.ArenaAllocator.init(alloc);
        // defer arena_allocator.deinit(); const arena = arena_allocator.allocator();`
        // — the arena collapses to owned temporaries created in
        // startPosix/startWindows BEFORE the fork() and dropped when this call
        // returns (crash-envelope precedent). `_alloc` keeps the reference
        // parameter shape (Termio::init precedent).

        // switch (builtin.os.tag) {
        //     .windows => try self.startWindows(arena),
        //     else => try self.startPosix(arena),
        // }
        #[cfg(windows)]
        {
            self.startWindows()
        }
        #[cfg(not(windows))]
        {
            self.startPosix()
        }
    }

    // PORT NOTE: `fn startPosix(self: *Command, arena: Allocator) !void` — the
    // arena parameter collapses into the owned temporaries below, which are
    // all created before the fork() (see `start`).
    #[cfg(not(windows))]
    fn startPosix(&mut self) -> Result<(), Error> {
        // Null-terminate all our arguments
        // PORT NOTE: `const argsZ = try arena.allocSentinel(?[*:0]const u8,
        // self.args.len, null); for (self.args, 0..) |arg, i| argsZ[i] = arg.ptr;`
        let mut argsZ: Vec<*const c_char> = Vec::with_capacity(self.args.len() + 1);
        for arg in self.args {
            argsZ.push(arg.as_ptr());
        }
        argsZ.push(std::ptr::null()); // (the allocSentinel null terminator)

        // Determine our env vars
        // PORT NOTE: `const envp = if (self.env) |env_map|
        //     (try createNullDelimitedEnvMap(arena, env_map)).ptr
        // else if (builtin.link_libc)
        //     std.c.environ
        // else
        //     @compileError("missing env vars");`
        // (Rust always links libc here — the `libc` binding — so the
        // compile-error branch cannot arise.)
        let env_owned = self.env.map(createNullDelimitedEnvMap);
        let envp: *const *const c_char = match &env_owned {
            Some(e) => e.ptr(),
            None => environ(),
        };

        // Fork.
        // PORT NOTE: Zig `posix.fork()` maps EAGAIN/ENOMEM to `error.SystemResources`
        // and anything else to `unexpectedErrno`.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(match errno() {
                libc::EAGAIN | libc::ENOMEM => Error::SystemResources,
                err => Error::Unexpected(err),
            });
        }

        if pid != 0 {
            // Parent, return immediately.
            // (Zig: `self.pid = @intCast(pid);`)
            self.pid = Some(pid);
            // if (self.rt_post_fork) |f| try f(self);
            if let Some(f) = self.rt_post_fork {
                f(self).map_err(|_| Error::PostForkError)?;
            }
            return Ok(());
        }

        // We are the child.

        // Setup our file descriptors for std streams.
        // if (self.stdin) |f| setupFd(f.handle, posix.STDIN_FILENO) catch
        //     return error.ExecFailedInChild;
        if let Some(f) = self.stdin {
            setupFd(f.as_raw_fd(), libc::STDIN_FILENO).map_err(|_| Error::ExecFailedInChild)?;
        }
        if let Some(f) = self.stdout {
            setupFd(f.as_raw_fd(), libc::STDOUT_FILENO).map_err(|_| Error::ExecFailedInChild)?;
        }
        if let Some(f) = self.stderr {
            setupFd(f.as_raw_fd(), libc::STDERR_FILENO).map_err(|_| Error::ExecFailedInChild)?;
        }

        // Setup our working directory
        if let Some(cwd) = self.cwd {
            // This can fail if we don't have permission to go to
            // this directory or if due to race conditions it doesn't
            // exist or any various other reasons. We don't want to
            // crash the entire process if this fails so we ignore it.
            // We don't log because that'll show up in the output.
            let _ = posix_chdir(cwd); // posix.chdir(cwd) catch {};
        }

        // Restore any rlimits that were set by Ghostty. This might fail but
        // any failures are ignored (its best effort).
        //
        // PORT NOTE: `global_state.rlimits.restore();` — rustty-global's `STATE`
        // OnceLock is the memory-safe analogue of the reference's
        // undefined-until-init global `state`; when unset (e.g. under
        // `cargo test`) there is nothing to restore (best effort, matching
        // the reference contract).
        if let Some(state) = rustty_global::STATE.get() {
            state.rlimits.restore();
        }

        // If there are pre exec callbacks, call them now.
        // if (self.os_pre_exec) |f| if (f(self)) |exitcode| posix.exit(exitcode);
        if let Some(f) = self.os_pre_exec {
            if let Some(exitcode) = f(self) {
                posix_exit(exitcode);
            }
        }
        // if (self.rt_pre_exec) |f| if (f(self)) |exitcode| posix.exit(exitcode);
        if let Some(f) = self.rt_pre_exec {
            if let Some(exitcode) = f(self) {
                posix_exit(exitcode);
            }
        }

        // Finally, replace our process.
        // Note: we must use the "p"-variant of exec here because we
        // do not guarantee our command is looked up already in the path.
        // const err = posix.execvpeZ(self.path, argsZ, envp);
        let err = execvpeZ(self.path, argsZ.as_ptr(), envp);

        // If we are executing this code, the exec failed. We're in the
        // child process so there isn't much we can do. We try to output
        // something reasonable. Its important to note we MUST NOT return
        // any other error condition from here on out.
        //
        // PORT NOTE: `var stderr_buf: [1024]u8 = undefined; var stderr_writer =
        // std.fs.File.stderr().writer(&stderr_buf); const stderr =
        // &stderr_writer.interface;` — a fixed stack-buffer writer over fd 2
        // (NO heap allocation in the forked child).
        let mut stderr_buf = [0u8; 1024];
        let mut stderr = StackWriter {
            buf: &mut stderr_buf,
            len: 0,
        };
        match err {
            // error.FileNotFound => stderr.print(...) catch {},
            ExecveError::FileNotFound => {
                let _ = writeln!(
                    stderr,
                    "Requested executable not found. Please verify the command is on\nthe PATH and try again."
                );
            }

            // else => stderr.print(...) catch {},
            _ => {
                let _ = writeln!(stderr, "exec syscall failed with unexpected error: {err}");
            }
        }
        stderr.flush(); // stderr.flush() catch {};

        // We return a very specific error that can be detected to determine
        // we're in the child.
        Err(Error::ExecFailedInChild)
    }

    // PORT NOTE: `fn startWindows(self: *Command, arena: Allocator) !void` — the
    // arena collapses into the owned temporaries below. cfg-gated structural
    // port (pty.rs WindowsPty precedent); unverified on a Windows target
    // (DEFERRALS.md, Windows spawn path).
    #[cfg(windows)]
    fn startWindows(&mut self) -> Result<(), Error> {
        use rustty_os::windows;
        use std::os::windows::io::AsRawHandle as _;

        // const cwd_w = if (self.cwd) |cwd| try std.unicode.utf8ToUtf16LeAllocZ(arena, cwd) else null;
        let cwd_w = match self.cwd {
            Some(cwd) => Some(utf8ToUtf16LeAllocZ(cwd)?),
            None => None,
        };

        // Pass null for lpApplicationName and put the program as the first
        // token of lpCommandLine. This lets CreateProcessW perform the
        // standard program search (parent-app dir, CWD, system dirs, PATH)
        // and append ".exe" when the name has no extension, which is what
        // users expect for bare commands like `wsl ~` or `pwsh.exe`.
        // It also preserves the child's argv[0] as written by the caller
        // rather than replacing it with the resolved absolute path.
        let command_line = if !self.args.is_empty() {
            windowsCreateCommandLine(self.args)
        } else {
            windowsCreateCommandLine(&[self.path])
        };
        let mut command_line_w = utf8ToUtf16LeAllocZ(&command_line)?;
        let env_w = self.env.map(createWindowsEnvBlock);

        let any_null_fd = self.stdin.is_none() || self.stdout.is_none() || self.stderr.is_none();
        // PORT NOTE: `try windows.OpenFile(&[_]u16{ '\\','D','e','v','i','c','e',
        // '\\','N','u','l','l' }, .{ .access_mask = windows.GENERIC_READ |
        // windows.SYNCHRONIZE, .share_access = windows.FILE_SHARE_READ,
        // .creation = windows.OPEN_EXISTING })` — std.os.windows.OpenFile is
        // the NtCreateFile wrapper and `\Device\Null` is the NT object the
        // Win32 name "NUL" aliases; realized as the identical-behavior Win32
        // CreateFileW("NUL", ...) with the same access/share/creation rather
        // than re-binding ntdll (DEFERRALS.md).
        let null_fd: Option<windows::HANDLE> = if any_null_fd {
            Some(openDeviceNull()?)
        } else {
            None
        };
        // defer if (null_fd) |fd| posix.close(fd);
        // (PORT NOTE: Zig `posix.close` on a Windows handle is CloseHandle; the
        // guard's Drop runs it on every exit path.)
        let _null_fd_guard = CloseHandleGuard(null_fd);

        // TODO: In the case of having FDs instead of pty, need to set up
        // attributes such that the child process only inherits these handles,
        // then set bInheritsHandles below.

        // const attribute_list, const stdin, const stdout, const stderr =
        //     if (self.pseudo_console) |pseudo_console| b: { ... } else b: { ... };
        let mut attribute_list_buf: Vec<u8> = Vec::new();
        let (attribute_list, stdin, stdout, stderr): (
            windows::exp::LPPROC_THREAD_ATTRIBUTE_LIST,
            Option<windows::HANDLE>,
            Option<windows::HANDLE>,
            Option<windows::HANDLE>,
        ) = if let Some(pseudo_console) = self.pseudo_console {
            // var attribute_list_size: usize = undefined;
            let mut attribute_list_size: usize = 0;
            let _ = unsafe {
                windows::exp::kernel32::InitializeProcThreadAttributeList(
                    std::ptr::null_mut(),
                    1,
                    0,
                    &mut attribute_list_size,
                )
            };

            // const attribute_list_buf = try arena.alloc(u8, attribute_list_size);
            attribute_list_buf = vec![0u8; attribute_list_size];
            if unsafe {
                windows::exp::kernel32::InitializeProcThreadAttributeList(
                    attribute_list_buf.as_mut_ptr() as windows::exp::LPPROC_THREAD_ATTRIBUTE_LIST,
                    1,
                    0,
                    &mut attribute_list_size,
                )
            } == 0
            {
                return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
            }

            if unsafe {
                windows::exp::kernel32::UpdateProcThreadAttribute(
                    attribute_list_buf.as_mut_ptr() as windows::exp::LPPROC_THREAD_ATTRIBUTE_LIST,
                    0,
                    windows::exp::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as windows::DWORD_PTR,
                    pseudo_console,
                    std::mem::size_of::<windows::exp::HPCON>(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
            }

            // break :b .{ attribute_list_buf.ptr, null, null, null };
            (
                attribute_list_buf.as_mut_ptr() as windows::exp::LPPROC_THREAD_ATTRIBUTE_LIST,
                None,
                None,
                None,
            )
        } else {
            let stdin = if let Some(f) = self.stdin {
                f.as_raw_handle()
            } else {
                null_fd.unwrap()
            };
            let stdout = if let Some(f) = self.stdout {
                f.as_raw_handle()
            } else {
                null_fd.unwrap()
            };
            let stderr = if let Some(f) = self.stderr {
                f.as_raw_handle()
            } else {
                null_fd.unwrap()
            };
            // break :b .{ null, stdin, stdout, stderr };
            (
                std::ptr::null_mut(),
                Some(stdin),
                Some(stdout),
                Some(stderr),
            )
        };

        let mut startup_info_ex = windows::exp::STARTUPINFOEX {
            StartupInfo: windows::STARTUPINFOW {
                cb: if !attribute_list.is_null() {
                    std::mem::size_of::<windows::exp::STARTUPINFOEX>() as windows::DWORD
                } else {
                    std::mem::size_of::<windows::STARTUPINFOW>() as windows::DWORD
                },
                hStdError: stderr.unwrap_or(std::ptr::null_mut()),
                hStdOutput: stdout.unwrap_or(std::ptr::null_mut()),
                hStdInput: stdin.unwrap_or(std::ptr::null_mut()),
                dwFlags: windows::STARTF_USESTDHANDLES,
                lpReserved: std::ptr::null_mut(),
                lpDesktop: std::ptr::null_mut(),
                lpTitle: std::ptr::null_mut(),
                dwX: 0,
                dwY: 0,
                dwXSize: 0,
                dwYSize: 0,
                dwXCountChars: 0,
                dwYCountChars: 0,
                dwFillAttribute: 0,
                wShowWindow: 0,
                cbReserved2: 0,
                lpReserved2: std::ptr::null_mut(),
            },
            lpAttributeList: attribute_list,
        };

        let mut flags: windows::DWORD = windows::exp::CREATE_UNICODE_ENVIRONMENT;
        if !attribute_list.is_null() {
            flags |= windows::exp::EXTENDED_STARTUPINFO_PRESENT;
        }

        // var process_information: windows.PROCESS_INFORMATION = undefined;
        let mut process_information: windows::PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            windows::exp::kernel32::CreateProcessW(
                std::ptr::null_mut(),
                command_line_w.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                windows::TRUE,
                flags,
                match &env_w {
                    Some(w) => w.as_ptr() as *mut c_void,
                    None => std::ptr::null_mut(),
                },
                match &cwd_w {
                    Some(w) => w.as_ptr() as *mut u16,
                    None => std::ptr::null_mut(),
                },
                &mut startup_info_ex.StartupInfo,
                &mut process_information,
            )
        } == 0
        {
            return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
        }

        self.pid = Some(process_information.hProcess);
        Ok(())
    }

    /// Wait for the command to exit and return information about how it exited.
    //
    // PORT NOTE: `pub fn wait(self: Command, block: bool) !Exit` — Zig takes the
    // struct by (copyable) value; `&self` is the Rust equivalent read.
    pub fn wait(&self, block: bool) -> Result<Exit, Error> {
        // if (comptime builtin.os.tag == .windows) { ... }
        #[cfg(windows)]
        {
            use rustty_os::windows;
            // Block until the process exits. This returns immediately if the
            // process already exited.
            let result =
                unsafe { win_kernel32::WaitForSingleObject(self.pid.unwrap(), windows::INFINITE) };
            if result == windows::WAIT_FAILED {
                return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
            }

            // var exit_code: windows.DWORD = undefined;
            let mut exit_code: windows::DWORD = 0;
            let has_code =
                unsafe { win_kernel32::GetExitCodeProcess(self.pid.unwrap(), &mut exit_code) } != 0;
            if !has_code {
                return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
            }

            Ok(Exit::Exited(exit_code))
        }

        #[cfg(not(windows))]
        {
            let res = if block {
                posix_waitpid(self.pid.unwrap(), 0)
            } else {
                // We specify NOHANG because its not our fault if the process we launch
                // for the tty doesn't properly waitpid its children. We don't want
                // to hang the terminal over it.
                // When NOHANG is specified, waitpid will return a pid of 0 if the process
                // doesn't have a status to report. When that happens, it is as though the
                // wait call has not been performed, so we need to keep trying until we get
                // a non-zero pid back, otherwise we end up with zombie processes.
                loop {
                    let res = posix_waitpid(self.pid.unwrap(), libc::WNOHANG as u32);
                    if res.pid != 0 {
                        break res;
                    }
                }
            };

            // return .init(res.status);
            Ok(Exit::init(res.status))
        }
    }

    /// Sets command->data to data.
    pub fn setData(&mut self, pointer: Option<*mut c_void>) {
        self.data = pointer;
    }

    /// Returns command->data.
    //
    // PORT NOTE: `pub fn getData(self: Command, comptime DT: type) ?*DT` — the
    // `@ptrCast(@alignCast(ptr))` is the raw-pointer cast (no dereference).
    pub fn getData<DT>(&self) -> Option<*mut DT> {
        // return if (self.data) |ptr| @ptrCast(@alignCast(ptr)) else null;
        self.data.map(|ptr| ptr.cast::<DT>())
    }
}

// PORT NOTE: the arena-owned return of `createNullDelimitedEnvMap`
// (`[:0]?[*:0]u8`): `ptrs` is the null-POINTER-terminated `envp` array (the
// `[:0]` sentinel is the trailing null pointer, NOT counted by `len()`), and
// `_bufs` owns the underlying `KEY=VALUE\0` allocations for the array's
// lifetime (the reference arena's lifetime == the caller's frame).
#[cfg_attr(windows, allow(dead_code))]
struct NullDelimitedEnvMap {
    _bufs: Vec<Vec<u8>>,
    ptrs: Vec<*const c_char>,
}

#[cfg_attr(windows, allow(dead_code))]
impl NullDelimitedEnvMap {
    /// The `environ.len` of the sentinel-terminated slice (sentinel excluded).
    // (Read by the createNullDelimitedEnvMap test — the reference's
    // `environ.len`; the non-test posix path only takes `.ptr`.)
    #[cfg_attr(not(test), allow(dead_code))]
    fn len(&self) -> usize {
        self.ptrs.len() - 1
    }

    /// `(...).ptr` — the raw `envp` pointer handed to execvpe.
    fn ptr(&self) -> *const *const c_char {
        self.ptrs.as_ptr()
    }
}

// Copied from Zig. This is a publicly exported function but there is no
// way to get it from the std package.
//
// PORT NOTE: `fn createNullDelimitedEnvMap(arena: mem.Allocator, env_map: *const
// EnvMap) ![:0]?[*:0]u8` — the arena parameter collapses into the owned
// `NullDelimitedEnvMap` (dropped with the caller's frame); infallible in
// Rust's allocation model (the `!` was `Allocator.Error`). All allocation
// happens BEFORE fork() (see `start`).
#[cfg_attr(windows, allow(dead_code))]
fn createNullDelimitedEnvMap(env_map: &EnvMap) -> NullDelimitedEnvMap {
    // const envp_count = env_map.count();
    let envp_count = env_map.len();
    // const envp_buf = try arena.allocSentinel(?[*:0]u8, envp_count, null);
    let mut bufs: Vec<Vec<u8>> = Vec::with_capacity(envp_count);
    let mut envp_buf: Vec<*const c_char> = Vec::with_capacity(envp_count + 1);

    // var it = env_map.iterator();
    // var i: usize = 0;
    let mut i: usize = 0;
    // while (it.next()) |pair| : (i += 1) { ... }
    for (key, value) in env_map {
        // const env_buf = try arena.allocSentinel(u8, pair.key_ptr.len + pair.value_ptr.len + 1, 0);
        let mut env_buf: Vec<u8> = Vec::with_capacity(key.len() + value.len() + 2);
        // @memcpy(env_buf[0..pair.key_ptr.len], pair.key_ptr.*);
        env_buf.extend_from_slice(key.as_bytes());
        // env_buf[pair.key_ptr.len] = '=';
        env_buf.push(b'=');
        // @memcpy(env_buf[pair.key_ptr.len + 1 ..], pair.value_ptr.*);
        env_buf.extend_from_slice(value.as_bytes());
        // (the allocSentinel 0 terminator)
        env_buf.push(0);
        // envp_buf[i] = env_buf.ptr;
        envp_buf.push(env_buf.as_ptr() as *const c_char);
        bufs.push(env_buf);
        i += 1;
    }
    // std.debug.assert(i == envp_count);
    debug_assert!(i == envp_count);

    // (the [:0]null sentinel)
    envp_buf.push(std::ptr::null());
    // return envp_buf;
    NullDelimitedEnvMap {
        _bufs: bufs,
        ptrs: envp_buf,
    }
}

// Copied from Zig. This is a publicly exported function but there is no
// way to get it from the std package.
//
// PORT NOTE: `fn createWindowsEnvBlock(allocator: mem.Allocator, env_map: *const
// EnvMap) ![]u16` — the allocator collapses into the returned Vec. Compiled
// on all targets (pure re-encode logic); only `startWindows` calls it.
#[cfg_attr(not(windows), allow(dead_code))]
fn createWindowsEnvBlock(env_map: &EnvMap) -> Vec<u16> {
    // count bytes needed
    let max_chars_needed = {
        let mut max_chars_needed: usize = 4; // 4 for the final 4 null bytes
        for (key, value) in env_map {
            // +1 for '='
            // +1 for null byte
            max_chars_needed += key.len() + value.len() + 2;
        }
        max_chars_needed
    };
    // const result = try allocator.alloc(u16, max_chars_needed);
    // errdefer allocator.free(result); — ownership/Drop frees on every path.
    let mut result: Vec<u16> = vec![0; max_chars_needed];

    // var it = env_map.iterator();
    let mut i: usize = 0;
    for (key, value) in env_map {
        // i += try std.unicode.utf8ToUtf16Le(result[i..], pair.key_ptr.*);
        for u in key.encode_utf16() {
            result[i] = u;
            i += 1;
        }
        result[i] = '=' as u16;
        i += 1;
        // i += try std.unicode.utf8ToUtf16Le(result[i..], pair.value_ptr.*);
        for u in value.encode_utf16() {
            result[i] = u;
            i += 1;
        }
        result[i] = 0;
        i += 1;
    }
    result[i] = 0;
    i += 1;
    result[i] = 0;
    i += 1;
    result[i] = 0;
    i += 1;
    result[i] = 0;
    i += 1;
    // return try allocator.realloc(result, i);
    result.truncate(i);
    result
}

/// Copied from Zig. This function could be made public in child_process.zig instead.
//
// PORT NOTE: `fn windowsCreateCommandLine(allocator: mem.Allocator, argv: []const
// []const u8) ![:0]u8` — the `std.Io.Writer.Allocating` buffer maps to an
// owned `Vec<u8>`; callers pass their `[:0]` args (`&CStr`), which Zig
// coerces to plain `[]const u8` here. The returned `[:0]` sentinel is not
// materialized (the caller immediately re-encodes to UTF-16 and appends its
// own sentinel). Compiled on all targets; only `startWindows` calls it.
#[cfg_attr(not(windows), allow(dead_code))]
fn windowsCreateCommandLine(argv: &[&CStr]) -> Vec<u8> {
    // var buf: std.Io.Writer.Allocating = .init(allocator);
    // defer buf.deinit();
    // const writer = &buf.writer;
    let mut buf: Vec<u8> = Vec::new();

    for (arg_i, arg) in argv.iter().enumerate() {
        let arg = arg.to_bytes();
        // if (arg_i != 0) try writer.writeByte(' ');
        if arg_i != 0 {
            buf.push(b' ');
        }
        // if (mem.indexOfAny(u8, arg, " \t\n\"") == null) { try writer.writeAll(arg); continue; }
        if !arg
            .iter()
            .any(|&byte| matches!(byte, b' ' | b'\t' | b'\n' | b'"'))
        {
            buf.extend_from_slice(arg);
            continue;
        }
        buf.push(b'"'); // try writer.writeByte('"');
        let mut backslash_count: usize = 0;
        for &byte in arg {
            match byte {
                b'\\' => backslash_count += 1,
                b'"' => {
                    // try writer.splatByteAll('\\', backslash_count * 2 + 1);
                    buf.extend(std::iter::repeat_n(b'\\', backslash_count * 2 + 1));
                    buf.push(b'"');
                    backslash_count = 0;
                }
                _ => {
                    // try writer.splatByteAll('\\', backslash_count);
                    buf.extend(std::iter::repeat_n(b'\\', backslash_count));
                    buf.push(byte);
                    backslash_count = 0;
                }
            }
        }
        // try writer.splatByteAll('\\', backslash_count * 2);
        buf.extend(std::iter::repeat_n(b'\\', backslash_count * 2));
        buf.push(b'"'); // try writer.writeByte('"');
    }

    // return buf.toOwnedSliceSentinel(0);
    buf
}

// fn setupFd(src: File.Handle, target: i32) !void
#[cfg(not(windows))]
fn setupFd(src: RawFd, target: i32) -> Result<(), Error> {
    // switch (builtin.os.tag) { .linux => ..., .freebsd, .ios, .macos => ...,
    // else => @compileError("unsupported platform") }
    #[cfg(target_os = "linux")]
    {
        // We use dup3 so that we can clear CLO_ON_EXEC. We do NOT want this
        // file descriptor to be closed on exec since we're exactly exec-ing after
        // this.
        loop {
            let rc = unsafe { libc::dup3(src, target, 0) };
            if rc >= 0 {
                // .SUCCESS => break,
                break;
            }
            match errno() {
                libc::EINTR => continue,
                libc::EAGAIN | libc::EACCES => return Err(Error::Locked),
                libc::EBADF => unreachable!(),
                libc::EBUSY => return Err(Error::FileBusy),
                libc::EINVAL => unreachable!(), // invalid parameters
                libc::EPERM => return Err(Error::PermissionDenied),
                libc::EMFILE => return Err(Error::ProcessFdQuotaExceeded),
                libc::ENOTDIR => unreachable!(), // invalid parameter
                libc::EDEADLK => return Err(Error::DeadLock),
                libc::ENOLCK => return Err(Error::LockedRegionLimitExceeded),
                err => return Err(Error::Unexpected(err)), // else => |err| return posix.unexpectedErrno(err),
            }
        }
        Ok(())
    }
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    {
        // Mac doesn't support dup3 so we use dup2. We purposely clear
        // CLO_ON_EXEC for this fd.
        // const flags = try posix.fcntl(src, posix.F.GETFD, 0);
        let flags = unsafe { libc::fcntl(src, libc::F_GETFD, 0) };
        if flags < 0 {
            // (Zig posix.fcntl surfaces the errno as an error)
            return Err(Error::Unexpected(errno()));
        }
        if flags & libc::FD_CLOEXEC != 0 {
            // _ = try posix.fcntl(src, posix.F.SETFD, flags & ~@as(u32, posix.FD_CLOEXEC));
            if unsafe { libc::fcntl(src, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } < 0 {
                return Err(Error::Unexpected(errno()));
            }
        }

        // try posix.dup2(src, target);
        posix_dup2(src, target)
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    )))]
    compile_error!("unsupported platform");
}

// ---------------------------------------------------------------------------
// PORT NOTE (Zig-std shims, Rust-only glue): the `std.posix`/`std.c` surface the
// reference calls into, reproduced faithfully over the same libc (R3). None of
// these heap-allocate — several run in the forked child where allocation is
// forbidden (see `start`).
// ---------------------------------------------------------------------------

/// PORT NOTE (Zig-std shim): `posix.errno(rc)` — the calling thread's errno value.
#[cfg(not(windows))]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// PORT NOTE (Zig-std shim): `posix.exit(code)` — the reference links libc, so this
/// is `std.c.exit(status)`: the libc `exit(3)` (atexit handlers + C-stdio
/// flush) and NOTHING more. Deliberately NOT Rust's `std::process::exit`,
/// which first runs `rt::cleanup()` — that takes the Rust stdout lock and
/// drops/reallocates the stdout `LineWriter` (malloc/free) — work that is not
/// async-signal-safe and therefore illegal in the forked child (between
/// `fork` and `exec`) where this shim runs (see `start`'s allocation
/// contract), and that would also flush the child's inherited copy of the
/// parent's buffered stdout (duplicated output). The Zig child performs
/// neither; matching it is the T8 fidelity fix.
//
// (pub(crate): Exec's `Subprocess::start` in_child arm is the reference's
// `posix.exit(1)` — it must use this same shim.)
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn posix_exit(status: u8) -> ! {
    unsafe { libc::exit(i32::from(status)) }
}

/// PORT NOTE (Zig-std shim): `std.c.environ` — the libc-provided environment block
/// used when `self.env` is null (inherit). On Apple platforms the dylib-safe
/// accessor is `_NSGetEnviron`; elsewhere the global `environ` symbol.
#[cfg(not(windows))]
fn environ() -> *const *const c_char {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    unsafe {
        *libc::_NSGetEnviron() as *const *const c_char
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        extern "C" {
            static environ: *const *const c_char;
        }
        unsafe { environ }
    }
}

/// PORT NOTE (Zig-std shim): `posix.chdir([]const u8)` — copies the (non-sentinel)
/// path into a stack `PATH_MAX` buffer (no heap allocation; called in the
/// forked child) and issues the chdir syscall. Over-long paths map to
/// `error.NameTooLong` (the caller ignores every failure).
#[cfg(not(windows))]
fn posix_chdir(dir_path: &[u8]) -> Result<(), i32> {
    let mut path_with_null = [0u8; libc::PATH_MAX as usize];
    if dir_path.len() >= path_with_null.len() {
        return Err(libc::ENAMETOOLONG);
    }
    path_with_null[..dir_path.len()].copy_from_slice(dir_path);
    path_with_null[dir_path.len()] = 0;
    if unsafe { libc::chdir(path_with_null.as_ptr() as *const c_char) } != 0 {
        return Err(errno());
    }
    Ok(())
}

/// PORT NOTE (Zig-std shim): `posix.dup2` — loops on EBUSY/EINTR; EMFILE maps to
/// `error.ProcessFdQuotaExceeded`; EINVAL/EBADF are unreachable (invalid
/// parameters / invalid file descriptor).
#[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
fn posix_dup2(old_fd: RawFd, new_fd: RawFd) -> Result<(), Error> {
    loop {
        if unsafe { libc::dup2(old_fd, new_fd) } >= 0 {
            return Ok(());
        }
        match errno() {
            libc::EBUSY | libc::EINTR => continue,
            libc::EMFILE => return Err(Error::ProcessFdQuotaExceeded),
            libc::EINVAL => unreachable!(), // invalid parameters passed to dup2
            libc::EBADF => unreachable!(),  // invalid file descriptor
            err => return Err(Error::Unexpected(err)),
        }
    }
}

/// PORT NOTE (Zig-std shim): `std.posix.WaitPidResult` — the `{ pid, status }`
/// pair `waitpid` yields.
//
// (pub(crate): Exec.zig's killPid calls the same `posix.waitpid` shim —
// stage 4.)
#[cfg(not(windows))]
pub(crate) struct WaitPidResult {
    pub(crate) pid: libc::pid_t,
    pub(crate) status: u32,
}

/// PORT NOTE (Zig-std shim): `posix.waitpid` — loops on EINTR; ECHILD/EINVAL are
/// unreachable per the Zig std contract (the process exists and is ours; the
/// flags are valid).
///
/// Kept VERBATIM (`unreachable!`) for the `Command::wait` path, which reproduces
/// the Zig-std contract this port models elsewhere. `killPid` uses
/// [`posix_waitpid_checked`] instead — see its docs for why the two callers need
/// different contracts.
#[cfg(not(windows))]
pub(crate) fn posix_waitpid(pid: libc::pid_t, flags: u32) -> WaitPidResult {
    loop {
        let mut status: libc::c_int = 0;
        let rc = unsafe { libc::waitpid(pid, &mut status, flags as libc::c_int) };
        if rc >= 0 {
            return WaitPidResult {
                pid: rc,
                status: status as u32,
            };
        }
        match errno() {
            libc::EINTR => continue,
            // .CHILD => unreachable, // The process specified does not exist.
            // .INVAL => unreachable, // Invalid flags.
            _ => unreachable!("waitpid: unexpected errno"),
        }
    }
}

/// `posix_waitpid`, but reporting rather than aborting when the child is already
/// gone: `None` means the wait failed with a real errno (in practice `ECHILD` —
/// somebody else reaped it).
///
/// **Why this exists (WS19 T17b).** rustty has a SECOND reaper:
/// `crates/rustty-xev/src/process.rs` calls `waitpid(pid, &status, 0)` in the
/// `.proc` completion trampoline. If that completion fires while `killPid` is in
/// its SIGHUP/`waitpid(WNOHANG)` loop, the loop's next `waitpid` returns `ECHILD`
/// — and the verbatim shim above turns that race into an unconditional Rust
/// panic, on the teardown path.
///
/// **This is fidelity, not a divergence, against upstream head.** At the pin,
/// ghostty's `killPid` went through Zig std's `posix.waitpid`, whose
/// `.CHILD => unreachable` the shim above reproduces faithfully. Upstream has
/// since dropped that wrapper for the raw syscall (`a77c706a1` / `e8525c0fd`):
///
/// ```zig
/// const rc = posix.system.waitpid(pid, null, std.c.W.NOHANG);
/// if (posix.errno(rc) == .INTR) continue;
/// break rc;                  // any other error -> rc == -1
/// ...
/// if (res_pid != 0) break;   // -1 != 0, so the loop BREAKS
/// ```
///
/// so an `ECHILD` now ends upstream's loop gracefully. Adopting it here therefore
/// REMOVES a divergence (and a reachable panic) rather than adding one. See
/// `.port/phase-3/assessments/E-os-termio-platform.md` §5.2.
///
/// `EINTR` still retries, exactly as before and as upstream still does.
#[cfg(not(windows))]
pub(crate) fn posix_waitpid_checked(pid: libc::pid_t, flags: u32) -> Option<WaitPidResult> {
    loop {
        let mut status: libc::c_int = 0;
        let rc = unsafe { libc::waitpid(pid, &mut status, flags as libc::c_int) };
        if rc >= 0 {
            return Some(WaitPidResult {
                pid: rc,
                status: status as u32,
            });
        }
        match errno() {
            libc::EINTR => continue,
            // ECHILD: the child is gone — which is what the caller was waiting
            // for. EINVAL etc. are equally unrecoverable-by-retry; upstream's
            // `rc == -1` break makes no distinction, so neither do we.
            _ => return None,
        }
    }
}

/// PORT NOTE (Zig-std shim): the `ExecveError` set of `posix.execveZ` — the errno
/// mapping the reference's failed-exec reporting switches over.
#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecveError {
    SystemResources,
    ProcessFdQuotaExceeded,
    NameTooLong,
    SystemFdQuotaExceeded,
    AccessDenied,
    InvalidExe,
    FileSystem,
    IsDir,
    FileNotFound,
    NotDir,
    FileBusy,
    /// `posix.unexpectedErrno` (payload carried for debugging parity with
    /// Zig's `unexpected_error_tracing`; Zig formats it as `error.Unexpected`).
    Unexpected(#[allow(dead_code)] i32),
}

#[cfg(not(windows))]
impl core::fmt::Display for ExecveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // (Zig formats an error value as `error.Name` — the shape the child's
        // "unexpected error" stderr report prints.)
        let name = match self {
            ExecveError::SystemResources => "SystemResources",
            ExecveError::ProcessFdQuotaExceeded => "ProcessFdQuotaExceeded",
            ExecveError::NameTooLong => "NameTooLong",
            ExecveError::SystemFdQuotaExceeded => "SystemFdQuotaExceeded",
            ExecveError::AccessDenied => "AccessDenied",
            ExecveError::InvalidExe => "InvalidExe",
            ExecveError::FileSystem => "FileSystem",
            ExecveError::IsDir => "IsDir",
            ExecveError::FileNotFound => "FileNotFound",
            ExecveError::NotDir => "NotDir",
            ExecveError::FileBusy => "FileBusy",
            ExecveError::Unexpected(_) => "Unexpected",
        };
        write!(f, "error.{name}")
    }
}

/// PORT NOTE (Zig-std shim): `posix.execveZ` — issues execve(2) and maps the errno
/// exactly as the Zig std does (only returns on failure).
#[cfg(not(windows))]
fn execveZ(
    path: *const c_char,
    child_argv: *const *const c_char,
    envp: *const *const c_char,
) -> ExecveError {
    let _ = unsafe { libc::execve(path, child_argv, envp) };
    let err = errno();
    match err {
        // .SUCCESS => unreachable, .FAULT => unreachable,
        libc::E2BIG => ExecveError::SystemResources,
        libc::EMFILE => ExecveError::ProcessFdQuotaExceeded,
        libc::ENAMETOOLONG => ExecveError::NameTooLong,
        libc::ENFILE => ExecveError::SystemFdQuotaExceeded,
        libc::ENOMEM => ExecveError::SystemResources,
        libc::EACCES => ExecveError::AccessDenied,
        libc::EPERM => ExecveError::AccessDenied,
        libc::EINVAL => ExecveError::InvalidExe,
        libc::ENOEXEC => ExecveError::InvalidExe,
        libc::EIO => ExecveError::FileSystem,
        libc::ELOOP => ExecveError::FileSystem,
        libc::EISDIR => ExecveError::IsDir,
        libc::ENOENT => ExecveError::FileNotFound,
        libc::ENOTDIR => ExecveError::NotDir,
        libc::ETXTBSY => ExecveError::FileBusy,
        _ => {
            // (Zig: per-OS extras — linux ELIBBAD/EBADARCH, darwin
            // EBADARCH/EBADEXEC -> error.InvalidExe; else unexpectedErrno.)
            #[cfg(target_os = "linux")]
            if err == libc::ELIBBAD {
                return ExecveError::InvalidExe;
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if err == libc::EBADARCH || err == libc::EBADEXEC {
                return ExecveError::InvalidExe;
            }
            ExecveError::Unexpected(err)
        }
    }
}

/// PORT NOTE (Zig-std shim): `posix.execvpeZ` — Zig implements the `p`-variant in
/// userspace (macOS has no execvpe(3)): a file containing '/' execs directly;
/// otherwise every PATH segment is tried with the candidate built in a stack
/// buffer. NO heap allocation (we are between fork and exec — see `start`).
/// Mirrors `std.posix.execvpeZ_expandArg0(.no_expand, ...)`.
#[cfg(not(windows))]
fn execvpeZ(
    file: &CStr,
    child_argv: *const *const c_char,
    envp: *const *const c_char,
) -> ExecveError {
    let file_slice = file.to_bytes();
    // if (mem.indexOfScalarPos(u8, file_slice, 0, '/') != null) return execveZ(...);
    if file_slice.contains(&b'/') {
        return execveZ(file.as_ptr(), child_argv, envp);
    }

    // const PATH = posix.getenvZ("PATH") orelse "/usr/local/bin:/bin/:/usr/bin";
    // (libc getenv: reads the environ block in place, no allocation.)
    let path_env = unsafe { libc::getenv(c"PATH".as_ptr()) };
    let path_bytes: &[u8] = if path_env.is_null() {
        b"/usr/local/bin:/bin/:/usr/bin"
    } else {
        unsafe { CStr::from_ptr(path_env) }.to_bytes()
    };

    // var path_buf: [PATH_MAX]u8 = undefined;
    let mut path_buf = [0u8; libc::PATH_MAX as usize];
    // var it = mem.tokenizeScalar(u8, PATH, ':');
    let mut seen_eacces = false;
    // var err: ExecveError = error.FileNotFound;
    let err = ExecveError::FileNotFound;
    for search_path in path_bytes.split(|&b| b == b':').filter(|s| !s.is_empty()) {
        // const path_len = search_path.len + file_slice.len + 1;
        let path_len = search_path.len() + file_slice.len() + 1;
        // if (path_buf.len < path_len + 1) return error.NameTooLong;
        if path_buf.len() < path_len + 1 {
            return ExecveError::NameTooLong;
        }
        path_buf[..search_path.len()].copy_from_slice(search_path);
        path_buf[search_path.len()] = b'/';
        path_buf[search_path.len() + 1..][..file_slice.len()].copy_from_slice(file_slice);
        path_buf[path_len] = 0;
        let full_path = path_buf.as_ptr() as *const c_char;
        match execveZ(full_path, child_argv, envp) {
            ExecveError::AccessDenied => seen_eacces = true,
            ExecveError::FileNotFound | ExecveError::NotDir => {}
            e => return e,
        }
    }
    if seen_eacces {
        return ExecveError::AccessDenied;
    }
    err
}

/// PORT NOTE: the reference's fixed `stderr_buf: [1024]u8` +
/// `File.stderr().writer(&stderr_buf)` — a stack-buffered `fmt` writer so the
/// failed-exec message is formatted WITHOUT heap allocation in the forked
/// child; `flush` issues the single write(2) to fd 2. (Content beyond the
/// buffer is truncated — the reference messages fit well within 1024 bytes.)
#[cfg(not(windows))]
struct StackWriter<'b> {
    buf: &'b mut [u8],
    len: usize,
}

#[cfg(not(windows))]
impl core::fmt::Write for StackWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let avail = self.buf.len() - self.len;
        let n = s.len().min(avail);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

#[cfg(not(windows))]
impl StackWriter<'_> {
    fn flush(&self) {
        let _ = unsafe {
            libc::write(
                libc::STDERR_FILENO,
                self.buf.as_ptr() as *const c_void,
                self.len,
            )
        };
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE (kernel32 shims local to Command, windows-only): the Win32 calls the
// reference reaches through `std.os.windows` that rustty-os::windows does not
// yet bind — declared here against the same DLL.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win_kernel32 {
    use rustty_os::windows::{BOOL, DWORD, HANDLE};
    extern "system" {
        pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
        pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
    }
}

/// PORT NOTE: `windows.unexpectedError(windows.kernel32.GetLastError())` — mapped
/// into the named `Error` union (payload carries the Win32 error code).
#[cfg(windows)]
fn win_unexpected(err: rustty_os::windows::DWORD) -> Error {
    Error::Unexpected(err as i32)
}

/// PORT NOTE (Zig-std shim): `std.unicode.utf8ToUtf16LeAllocZ` — validates UTF-8
/// and re-encodes into a null-terminated UTF-16 buffer.
#[cfg(windows)]
fn utf8ToUtf16LeAllocZ(s: &[u8]) -> Result<Vec<u16>, Error> {
    let s = std::str::from_utf8(s).map_err(|_| Error::InvalidUtf8)?;
    let mut out: Vec<u16> = s.encode_utf16().collect();
    out.push(0);
    Ok(out)
}

/// See the `windows.OpenFile` PORT NOTE at the call site in `startWindows`.
#[cfg(windows)]
fn openDeviceNull() -> Result<rustty_os::windows::HANDLE, Error> {
    use rustty_os::windows;
    let name_w: [u16; 4] = [b'N' as u16, b'U' as u16, b'L' as u16, 0];
    let handle = unsafe {
        windows::kernel32::CreateFileW(
            name_w.as_ptr(),
            windows::GENERIC_READ | windows::SYNCHRONIZE,
            windows::FILE_SHARE_READ,
            std::ptr::null_mut(),
            windows::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == windows::INVALID_HANDLE_VALUE {
        return Err(win_unexpected(unsafe { windows::kernel32::GetLastError() }));
    }
    Ok(handle)
}

/// PORT NOTE: the `defer if (null_fd) |fd| posix.close(fd);` of `startWindows`
/// (Zig `posix.close` on windows is CloseHandle) as a Drop guard.
#[cfg(windows)]
struct CloseHandleGuard(Option<rustty_os::windows::HANDLE>);

#[cfg(windows)]
impl Drop for CloseHandleGuard {
    fn drop(&mut self) {
        if let Some(fd) = self.0 {
            let _ = rustty_os::windows::CloseHandle(fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_os::tempdir::TempDir;
    use std::io::{Read as _, Seek as _, SeekFrom};

    // [T8] Every test below forks. Take the crate-wide fork-serialization lock
    // (lib.rs `FORK_KAT_SERIAL`) for the test's duration: an unserialized
    // `fork()` snapshots every open fd, leaking a concurrently-running exec
    // KAT's pty master/slave into this test's child — on macOS a process
    // holding a leaked slave copy can wedge SIGKILL-immune in kernel exit
    // (pending-output drain), which is the ci-run-2026-07-10 hang. Pure
    // serialization: every test still runs unchanged.
    #[cfg(not(windows))]
    fn fork_serial() -> std::sync::MutexGuard<'static, ()> {
        crate::FORK_KAT_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    // test "createNullDelimitedEnvMap" {
    #[test]
    fn create_null_delimited_env_map() {
        // const allocator = testing.allocator;
        // var envmap = EnvMap.init(allocator);
        // defer envmap.deinit();
        let mut envmap = EnvMap::new();

        // try envmap.put(...);
        envmap.insert("HOME".to_string(), "/home/ifreund".to_string());
        envmap.insert("WAYLAND_DISPLAY".to_string(), "wayland-1".to_string());
        envmap.insert("DISPLAY".to_string(), ":1".to_string());
        envmap.insert("DEBUGINFOD_URLS".to_string(), " ".to_string());
        envmap.insert("XCURSOR_SIZE".to_string(), "24".to_string());

        // var arena = std.heap.ArenaAllocator.init(allocator);
        // defer arena.deinit();
        // (PORT NOTE: the arena collapses — NullDelimitedEnvMap owns its buffers.)
        // const environ = try createNullDelimitedEnvMap(arena.allocator(), &envmap);
        let environ = createNullDelimitedEnvMap(&envmap);

        // try testing.expectEqual(@as(usize, 5), environ.len);
        assert_eq!(5usize, environ.len());

        // inline for (.{ ... }) |target| { for (environ) |variable| { ... } else { ... } }
        for target in [
            "HOME=/home/ifreund",
            "WAYLAND_DISPLAY=wayland-1",
            "DISPLAY=:1",
            "DEBUGINFOD_URLS= ",
            "XCURSOR_SIZE=24",
        ] {
            let mut found = false;
            for &variable in &environ.ptrs[..environ.len()] {
                // if (mem.eql(u8, mem.span(variable orelse continue), target)) break;
                if variable.is_null() {
                    continue;
                }
                if unsafe { CStr::from_ptr(variable) }.to_bytes() == target.as_bytes() {
                    found = true;
                    break;
                }
            }
            // try testing.expect(false); // Environment variable not found
            assert!(found, "Environment variable not found");
        }
    }

    // test "Command: os pre exec 1" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_os_pre_exec_1() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        // (PORT NOTE: the reference's windows SkipZigTest maps to cfg-gating the
        // body — on windows the test compiles to the skip, mirroring the
        // F3 DirectWrite precedent.)
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            let mut cmd: Command = Command {
                path: c"/bin/sh",
                args: &[c"/bin/sh", c"-v"],
                // (Zig field defaults are explicit in Rust struct literals.)
                env: None,
                cwd: None,
                stdin: None,
                stdout: None,
                stderr: None,
                os_pre_exec: Some({
                    fn r#do(_: &mut Command<'_>) -> Option<u8> {
                        // This runs in the child, so we can exit and it won't
                        // kill the test runner.
                        posix_exit(42);
                    }
                    r#do
                }),
                rt_pre_exec: None,
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try cmd.testingStart();
            cmd.testingStart().unwrap();
            // try testing.expect(cmd.pid != null);
            assert!(cmd.pid.is_some());
            // const exit = try cmd.wait(true);
            let exit = cmd.wait(true).unwrap();
            // try testing.expect(exit == .Exited);
            assert!(matches!(exit, Exit::Exited(_)));
            // try testing.expect(exit.Exited == 42);
            match exit {
                Exit::Exited(code) => assert!(code == 42),
                _ => unreachable!(),
            }
        }
    }

    // test "Command: os pre exec 2" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_os_pre_exec_2() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            let mut cmd: Command = Command {
                path: c"/bin/sh",
                args: &[c"/bin/sh", c"-v"],
                env: None,
                cwd: None,
                stdin: None,
                stdout: None,
                stderr: None,
                os_pre_exec: Some({
                    fn r#do(_: &mut Command<'_>) -> Option<u8> {
                        // This runs in the child, so we can exit and it won't
                        // kill the test runner.
                        Some(42) // return 42;
                    }
                    r#do
                }),
                rt_pre_exec: None,
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try cmd.testingStart();
            cmd.testingStart().unwrap();
            // try testing.expect(cmd.pid != null);
            assert!(cmd.pid.is_some());
            // const exit = try cmd.wait(true);
            let exit = cmd.wait(true).unwrap();
            // try testing.expect(exit == .Exited);
            assert!(matches!(exit, Exit::Exited(_)));
            // try testing.expect(exit.Exited == 42);
            match exit {
                Exit::Exited(code) => assert!(code == 42),
                _ => unreachable!(),
            }
        }
    }

    // test "Command: rt pre exec 1" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_rt_pre_exec_1() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            let mut cmd: Command = Command {
                path: c"/bin/sh",
                args: &[c"/bin/sh", c"-v"],
                env: None,
                cwd: None,
                stdin: None,
                stdout: None,
                stderr: None,
                os_pre_exec: None,
                rt_pre_exec: Some({
                    fn r#do(_: &mut Command<'_>) -> Option<u8> {
                        // This runs in the child, so we can exit and it won't
                        // kill the test runner.
                        posix_exit(42);
                    }
                    r#do
                }),
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try cmd.testingStart();
            cmd.testingStart().unwrap();
            // try testing.expect(cmd.pid != null);
            assert!(cmd.pid.is_some());
            // const exit = try cmd.wait(true);
            let exit = cmd.wait(true).unwrap();
            // try testing.expect(exit == .Exited);
            assert!(matches!(exit, Exit::Exited(_)));
            // try testing.expect(exit.Exited == 42);
            match exit {
                Exit::Exited(code) => assert!(code == 42),
                _ => unreachable!(),
            }
        }
    }

    // test "Command: rt pre exec 2" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_rt_pre_exec_2() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            let mut cmd: Command = Command {
                path: c"/bin/sh",
                args: &[c"/bin/sh", c"-v"],
                env: None,
                cwd: None,
                stdin: None,
                stdout: None,
                stderr: None,
                os_pre_exec: None,
                rt_pre_exec: Some({
                    fn r#do(_: &mut Command<'_>) -> Option<u8> {
                        // This runs in the child, so we can exit and it won't
                        // kill the test runner.
                        Some(42) // return 42;
                    }
                    r#do
                }),
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try cmd.testingStart();
            cmd.testingStart().unwrap();
            // try testing.expect(cmd.pid != null);
            assert!(cmd.pid.is_some());
            // const exit = try cmd.wait(true);
            let exit = cmd.wait(true).unwrap();
            // try testing.expect(exit == .Exited);
            assert!(matches!(exit, Exit::Exited(_)));
            // try testing.expect(exit.Exited == 42);
            match exit {
                Exit::Exited(code) => assert!(code == 42),
                _ => unreachable!(),
            }
        }
    }

    // test "Command: rt post fork 1" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_rt_post_fork_1() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            let mut cmd: Command = Command {
                path: c"/bin/sh",
                args: &[c"/bin/sh", c"-c", c"sleep 1"],
                env: None,
                cwd: None,
                stdin: None,
                stdout: None,
                stderr: None,
                os_pre_exec: None,
                rt_pre_exec: None,
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: Some({
                    fn r#do(_: &mut Command<'_>) -> Result<(), PostForkError> {
                        Err(PostForkError) // return error.PostForkError;
                    }
                    r#do
                }),
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try testing.expectError(error.PostForkError, cmd.testingStart());
            assert_eq!(Err(Error::PostForkError), cmd.testingStart());
        }
    }

    // fn createTestStdout(dir: std.fs.Dir) !File {
    fn createTestStdout(dir: &rustty_os::tempdir::Dir) -> std::io::Result<File> {
        // const file = try dir.createFile("stdout.txt", .{ .read = true });
        let file = dir.createFile("stdout.txt", true)?;
        // if (builtin.os.tag == .windows) {
        //     try windows.SetHandleInformation(
        //         file.handle,
        //         windows.HANDLE_FLAG_INHERIT,
        //         windows.HANDLE_FLAG_INHERIT,
        //     );
        // }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            rustty_os::windows::SetHandleInformation(
                file.as_raw_handle(),
                rustty_os::windows::HANDLE_FLAG_INHERIT,
                rustty_os::windows::HANDLE_FLAG_INHERIT,
            )?;
        }

        Ok(file)
    }

    // fn createTestStderr(dir: std.fs.Dir) !File {
    #[cfg(not(windows))] // (only the posix-gated execveZ-failure test uses it)
    fn createTestStderr(dir: &rustty_os::tempdir::Dir) -> std::io::Result<File> {
        // const file = try dir.createFile("stderr.txt", .{ .read = true });
        let file = dir.createFile("stderr.txt", true)?;
        // if (builtin.os.tag == .windows) { try windows.SetHandleInformation(...); }
        // (PORT NOTE: the windows branch mirrors createTestStdout's above; this
        // helper is compiled only on posix — the sole caller is
        // windows-skipped, matching the reference's lazy compilation.)

        Ok(file)
    }

    // test "Command: redirect stdout to file" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_redirect_stdout_to_file() {
        // [T8] serialize with every other fork KAT in this binary.
        #[cfg(not(windows))]
        let _serial = fork_serial();

        // var td = try TempDir.init();
        // defer td.deinit();
        let mut td = TempDir::init().unwrap();
        // var stdout = try createTestStdout(td.dir);
        // defer stdout.close();
        let mut stdout = createTestStdout(td.dir()).unwrap();

        // var cmd: Command = if (builtin.os.tag == .windows) .{ ... } else .{ ... };
        #[cfg(windows)]
        let mut cmd: Command = Command {
            path: c"C:\\Windows\\System32\\whoami.exe",
            args: &[c"C:\\Windows\\System32\\whoami.exe"],
            env: None,
            cwd: None,
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            pseudo_console: None,
            data: None,
            pid: None,
        };
        #[cfg(not(windows))]
        let mut cmd: Command = Command {
            path: c"/bin/sh",
            args: &[c"/bin/sh", c"-c", c"echo hello"],
            env: None,
            cwd: None,
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            data: None,
            pid: None,
        };

        // try cmd.testingStart();
        cmd.testingStart().unwrap();
        // try testing.expect(cmd.pid != null);
        assert!(cmd.pid.is_some());
        // const exit = try cmd.wait(true);
        let exit = cmd.wait(true).unwrap();
        // try testing.expect(exit == .Exited);
        assert!(matches!(exit, Exit::Exited(_)));
        // try testing.expectEqual(@as(u32, 0), @as(u32, exit.Exited));
        match exit {
            Exit::Exited(code) => assert_eq!(0u32, u32::from(code)),
            #[cfg(not(windows))]
            _ => unreachable!(),
        }

        // Read our stdout
        // try stdout.seekTo(0);
        stdout.seek(SeekFrom::Start(0)).unwrap();
        // const contents = try stdout.readToEndAlloc(testing.allocator, 1024 * 128);
        // defer testing.allocator.free(contents);
        let mut contents = Vec::new();
        stdout
            .by_ref()
            .take(1024 * 128)
            .read_to_end(&mut contents)
            .unwrap();
        // try testing.expect(contents.len > 0);
        assert!(!contents.is_empty());

        // (defers run in reverse order: stdout.close(), then td.deinit())
        drop(stdout);
        td.deinit();
    }

    // test "Command: custom env vars" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_custom_env_vars() {
        // [T8] serialize with every other fork KAT in this binary.
        #[cfg(not(windows))]
        let _serial = fork_serial();

        // var td = try TempDir.init();
        // defer td.deinit();
        let mut td = TempDir::init().unwrap();
        // var stdout = try createTestStdout(td.dir);
        // defer stdout.close();
        let mut stdout = createTestStdout(td.dir()).unwrap();

        // var env = EnvMap.init(testing.allocator);
        // defer env.deinit();
        let mut env = EnvMap::new();
        // try env.put("VALUE", "hello");
        env.insert("VALUE".to_string(), "hello".to_string());

        // var cmd: Command = if (builtin.os.tag == .windows) .{ ... } else .{ ... };
        #[cfg(windows)]
        let mut cmd: Command = Command {
            path: c"C:\\Windows\\System32\\cmd.exe",
            args: &[c"C:\\Windows\\System32\\cmd.exe", c"/C", c"echo %VALUE%"],
            env: Some(&env),
            cwd: None,
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            pseudo_console: None,
            data: None,
            pid: None,
        };
        #[cfg(not(windows))]
        let mut cmd: Command = Command {
            path: c"/bin/sh",
            args: &[c"/bin/sh", c"-c", c"echo $VALUE"],
            env: Some(&env),
            cwd: None,
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            data: None,
            pid: None,
        };

        // try cmd.testingStart();
        cmd.testingStart().unwrap();
        // try testing.expect(cmd.pid != null);
        assert!(cmd.pid.is_some());
        // const exit = try cmd.wait(true);
        let exit = cmd.wait(true).unwrap();
        // try testing.expect(exit == .Exited);
        assert!(matches!(exit, Exit::Exited(_)));
        // try testing.expect(exit.Exited == 0);
        match exit {
            Exit::Exited(code) => assert!(code == 0),
            #[cfg(not(windows))]
            _ => unreachable!(),
        }

        // Read our stdout
        // try stdout.seekTo(0);
        stdout.seek(SeekFrom::Start(0)).unwrap();
        // const contents = try stdout.readToEndAlloc(testing.allocator, 4096);
        // defer testing.allocator.free(contents);
        let mut contents = Vec::new();
        stdout
            .by_ref()
            .take(4096)
            .read_to_end(&mut contents)
            .unwrap();

        // if (builtin.os.tag == .windows) {
        //     try testing.expectEqualStrings("hello\r\n", contents);
        // } else {
        //     try testing.expectEqualStrings("hello\n", contents);
        // }
        #[cfg(windows)]
        assert_eq!(b"hello\r\n".as_slice(), contents.as_slice());
        #[cfg(not(windows))]
        assert_eq!(b"hello\n".as_slice(), contents.as_slice());

        // (defers run in reverse order: env.deinit()/stdout.close(), then td.deinit())
        drop(stdout);
        td.deinit();
    }

    // test "Command: custom working directory" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_custom_working_directory() {
        // [T8] serialize with every other fork KAT in this binary.
        #[cfg(not(windows))]
        let _serial = fork_serial();

        // var td = try TempDir.init();
        // defer td.deinit();
        let mut td = TempDir::init().unwrap();
        // var stdout = try createTestStdout(td.dir);
        // defer stdout.close();
        let mut stdout = createTestStdout(td.dir()).unwrap();

        // var cmd: Command = if (builtin.os.tag == .windows) .{ ... } else .{ ... };
        #[cfg(windows)]
        let mut cmd: Command = Command {
            path: c"C:\\Windows\\System32\\cmd.exe",
            args: &[c"C:\\Windows\\System32\\cmd.exe", c"/C", c"cd"],
            env: None,
            cwd: Some(b"C:\\Windows\\System32".as_slice()),
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            pseudo_console: None,
            data: None,
            pid: None,
        };
        #[cfg(not(windows))]
        let mut cmd: Command = Command {
            path: c"/bin/sh",
            args: &[c"/bin/sh", c"-c", c"pwd"],
            env: None,
            cwd: Some(b"/tmp".as_slice()),
            stdin: None,
            stdout: Some(&stdout),
            stderr: None,
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
            data: None,
            pid: None,
        };

        // try cmd.testingStart();
        cmd.testingStart().unwrap();
        // try testing.expect(cmd.pid != null);
        assert!(cmd.pid.is_some());
        // const exit = try cmd.wait(true);
        let exit = cmd.wait(true).unwrap();
        // try testing.expect(exit == .Exited);
        assert!(matches!(exit, Exit::Exited(_)));
        // try testing.expect(exit.Exited == 0);
        match exit {
            Exit::Exited(code) => assert!(code == 0),
            #[cfg(not(windows))]
            _ => unreachable!(),
        }

        // Read our stdout
        // try stdout.seekTo(0);
        stdout.seek(SeekFrom::Start(0)).unwrap();
        // const contents = try stdout.readToEndAlloc(testing.allocator, 4096);
        // defer testing.allocator.free(contents);
        let mut contents = Vec::new();
        stdout
            .by_ref()
            .take(4096)
            .read_to_end(&mut contents)
            .unwrap();

        // if (builtin.os.tag == .windows) {
        //     try testing.expectEqualStrings("C:\\Windows\\System32\r\n", contents);
        // } else if (builtin.os.tag == .macos) {
        //     try testing.expectEqualStrings("/private/tmp\n", contents);
        // } else {
        //     try testing.expectEqualStrings("/tmp\n", contents);
        // }
        #[cfg(windows)]
        assert_eq!(b"C:\\Windows\\System32\r\n".as_slice(), contents.as_slice());
        #[cfg(target_os = "macos")]
        assert_eq!(b"/private/tmp\n".as_slice(), contents.as_slice());
        #[cfg(not(any(windows, target_os = "macos")))]
        assert_eq!(b"/tmp\n".as_slice(), contents.as_slice());

        // (defers run in reverse order: stdout.close(), then td.deinit())
        drop(stdout);
        td.deinit();
    }

    // Test validate an execveZ failure correctly terminates when error.ExecFailedInChild is correctly handled
    //
    // Incorrectly handling an error.ExecFailedInChild results in a second copy of the test process running.
    // Duplicating the test process leads to weird behavior
    // zig build test will hang
    // test binary created via -Demit-test-exe will run 2 copies of the test suite
    // test "Command: posix fork handles execveZ failure" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    fn command_posix_fork_handles_execvez_failure() {
        // if (builtin.os.tag == .windows) { return error.SkipZigTest; }
        #[cfg(not(windows))]
        {
            // [T8] serialize with every other fork KAT in this binary.
            let _serial = fork_serial();

            // var td = try TempDir.init();
            // defer td.deinit();
            let mut td = TempDir::init().unwrap();
            // var stdout = try createTestStdout(td.dir);
            // defer stdout.close();
            let stdout = createTestStdout(td.dir()).unwrap();
            // var stderr = try createTestStderr(td.dir);
            // defer stderr.close();
            let stderr = createTestStderr(td.dir()).unwrap();

            let mut cmd: Command = Command {
                path: c"/not/a/binary",
                args: &[c"/not/a/binary", c""],
                env: None,
                cwd: Some(b"/bin".as_slice()),
                stdin: None,
                stdout: Some(&stdout),
                stderr: Some(&stderr),
                os_pre_exec: None,
                rt_pre_exec: None,
                rt_pre_exec_info: RtPreExecInfo, // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo, // .rt_post_fork_info = undefined,
                data: None,
                pid: None,
            };

            // try cmd.testingStart();
            cmd.testingStart().unwrap();
            // try testing.expect(cmd.pid != null);
            assert!(cmd.pid.is_some());
            // const exit = try cmd.wait(true);
            let exit = cmd.wait(true).unwrap();
            // try testing.expect(exit == .Exited);
            assert!(matches!(exit, Exit::Exited(_)));
            // try testing.expect(exit.Exited == 1);
            match exit {
                Exit::Exited(code) => assert!(code == 1),
                _ => unreachable!(),
            }

            // (defers run in reverse order: stderr/stdout close, then td.deinit())
            drop(stderr);
            drop(stdout);
            td.deinit();
        }
    }

    // If cmd.start fails with error.ExecFailedInChild it's the _child_ process that is running. If it does not
    // terminate in response to that error both the parent and child will continue as if they _are_ the test suite
    // process.
    // fn testingStart(self: *Command) !void {
    impl Command<'_> {
        fn testingStart(&mut self) -> Result<(), Error> {
            // self.start(testing.allocator) catch |err| { ... return err; };
            match self.start(Allocator) {
                Ok(()) => Ok(()),
                Err(err) => {
                    if err == Error::ExecFailedInChild {
                        // I am a child process, I must not get confused and continue running the rest of the test suite.
                        posix_exit(1);
                    }
                    Err(err)
                }
            }
        }
    }
}
