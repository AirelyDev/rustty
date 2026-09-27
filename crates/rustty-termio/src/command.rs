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
//!
//! TODO: This may have changed a lot now with the new I/O implementations in
//! >= 0.16.0, so this might warrant a recheck.
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
use rustty_config::value_types::{GtkSingleInstance, LinuxCgroup};
use rustty_config::Config;
use std::ffi::{c_char, c_void, CStr};
use std::fs::File;

#[cfg(not(windows))]
use std::os::fd::{AsRawFd as _, RawFd};

/// PORT NOTE: `EnvMap = std.process.EnvMap` — a hash map of environment
/// variables. Upstream retyped this to `std.process.Environ.Map` when
/// `os/env.zig` was deleted (`e8525c0fd`); rustty's single definition of that
/// map lives in `rustty-global` next to the captured `Environ` block.
///
/// P3-W7 T2: this was `HashMap<String, String>`, which could not represent a
/// non-UTF-8 environment variable at all — the defect that made
/// `rustty-os`'s `getEnvMap` **panic** on the surface-creation path. Zig
/// byte-slices become `Vec<u8>`/`&[u8]`, never lossy `String`.
pub type EnvMap = rustty_global::EnvMap;

/// PORT NOTE (P3-W7 T2): `std.process.EnvMap`'s own `put`/`get`/`remove` API
/// over byte slices, restored as an extension trait next to the map type in
/// `rustty-global`. Re-exported here because `Command`/`Exec` are its heaviest
/// users and the reference spells them on `EnvMap` itself.
pub use rustty_global::EnvMapExt;

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
    /// `error.Locked` (`setupFd` `PosixCall.f` EAGAIN/EACCES).
    Locked,
    /// `error.FileBusy` (`setupFd` `PosixCall.f` EBUSY).
    FileBusy,
    /// `error.PermissionDenied` (`setupFd` `PosixCall.f` EPERM).
    PermissionDenied,
    /// `error.ProcessFdQuotaExceeded` (`setupFd` `PosixCall.f` EMFILE).
    ProcessFdQuotaExceeded,
    /// `error.DeadLock` (`setupFd` `PosixCall.f` EDEADLK).
    DeadLock,
    /// `error.LockedRegionLimitExceeded` (`setupFd` `PosixCall.f` ENOLCK).
    LockedRegionLimitExceeded,
    /// Zig `error.InvalidUtf8`/`error.InvalidWtf8` — the windows
    /// `utf8ToUtf16LeAllocZ` re-encode of cwd/command line.
    InvalidUtf8,
    /// `error.NoChildProcess` — `waitPid` saw `ECHILD` (`a77c706a1`).
    NoChildProcess,
    /// `error.InvalidWaitOptions` — `waitPid` saw `EINVAL` (`a77c706a1`).
    InvalidWaitOptions,
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
            Error::NoChildProcess => "NoChildProcess",
            Error::InvalidWaitOptions => "InvalidWaitOptions",
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

// ============================================================================
// THE APPRT `rt_*` HOOK SEAM — PORT NOTE (divergence from the oracle's
// comptime branch). READ THIS BEFORE TOUCHING ANYTHING BELOW.
// ============================================================================
//
// The reference selects BOTH the payload TYPE and the hook FUNCTION at comptime
// from whichever apprt the build chose:
//
//     // Command.zig:139-155
//     pub const RtPreExecInfo = if (@hasDecl(apprt.runtime, "pre_exec"))
//         apprt.runtime.pre_exec.PreExecInfo
//     else struct {
//         pub inline fn init(_: *const configpkg.Config) @This() { return .{}; }
//     };
//     pub const RtPostForkInfo = if (@hasDecl(apprt.runtime, "post_fork"))
//         apprt.runtime.post_fork.PostForkInfo
//     else struct { ... };
//
//     // termio/Exec.zig:1034-1037
//     .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec"))
//         apprt.runtime.pre_exec.preExec else null,
//     .rt_pre_exec_info = self.rt_pre_exec_info,
//     .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork"))
//         apprt.runtime.post_fork.postFork else null,
//     .rt_post_fork_info = self.rt_post_fork_info,
//
// Rust has neither comptime type selection nor `@hasDecl`, and R4 forbids the
// edge this would need: `rustty-termio` must NOT depend on `rustty-apprt`
// (`rustty-apprt` -> `rustty-core` -> `rustty-termio` already exists, so naming
// the apprt from here is a crate cycle). The two halves are therefore split:
//
//   * THE PAYLOAD TYPES (`RtPreExecInfo`/`RtPostForkInfo`) are DEFINED HERE with
//     the reference's real GTK fields — `pre_exec.zig:11-23` and
//     `post_fork.zig:15-31` verbatim. This crate already depends on
//     `rustty-config`, so `init(&Config)` needs no new edge. The GTK apprt then
//     ALIASES these (`pub use rustty_termio::command::RtPreExecInfo as
//     PreExecInfo;`), which makes the reference's comptime identity
//     `RtPreExecInfo == apprt.runtime.pre_exec.PreExecInfo` literally true —
//     just established from the lower crate instead of the upper one. The
//     non-GTK apprts never construct a hook, so the fields are inert for them
//     exactly as the reference's `else struct {}` is.
//
//   * THE HOOK FUNCTIONS are INSTALLED FROM ABOVE via [`install_rt_hooks`]
//     rather than named from here. The apprt layer calls it once during its own
//     init; `Exec`'s `Command` construction reads [`rt_hooks`] at exactly the
//     site the reference reads `@hasDecl` (`Exec.zig:1034,1036`).
//
// WHAT THIS COSTS, STATED PLAINLY: the reference's choice is a build-time
// constant that cannot be wrong; ours is a process-global set once at startup,
// so there is a window before the install in which the hooks read as `None`.
// That window is closed in practice — the install happens in apprt init, and no
// `Command` exists before an apprt has a surface to spawn one for — and
// [`install_rt_hooks`] is deliberately not idempotent-silent: a SECOND install
// with different hooks panics, which is the runtime stand-in for "the build
// picked one apprt". This is the whole of the divergence; nothing else about
// the seam differs from the reference.

/// Configuration information needed by the apprt pre exec function. Note
/// that this should be a trivially copyable struct and not require any
/// allocation/deallocation.
//
// PORT NOTE: this is the reference's `apprt.runtime.pre_exec.PreExecInfo`
// (`src/apprt/gtk/pre_exec.zig:11-23`), hoisted into this crate for the reason
// spelled out in the seam note above. `rustty_apprt::gtk::pre_exec::PreExecInfo`
// is an alias OF this type, not a copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtPreExecInfo {
    /// `gtk_single_instance: configpkg.Config.GtkSingleInstance`
    /// (`pre_exec.zig:12`).
    pub gtk_single_instance: GtkSingleInstance,
    /// `linux_cgroup: configpkg.Config.LinuxCgroup` (`pre_exec.zig:13`).
    pub linux_cgroup: LinuxCgroup,
    /// `linux_cgroup_hard_fail: bool` (`pre_exec.zig:14`).
    pub linux_cgroup_hard_fail: bool,
}

impl RtPreExecInfo {
    /// PORTS: `pre_exec.zig:16-22` `pub fn init(cfg: *const configpkg.Config)`.
    /// (The reference's non-GTK `else` branch is the same `init(&Config)`
    /// signature returning an empty value — the signature is what `Exec` and
    /// `Surface` bind to, and it is unchanged.)
    #[inline]
    #[must_use]
    pub fn init(cfg: &Config) -> Self {
        Self {
            gtk_single_instance: cfg.gtk_single_instance,
            linux_cgroup: cfg.linux_cgroup,
            linux_cgroup_hard_fail: cfg.linux_cgroup_hard_fail,
        }
    }
}

impl Default for RtPreExecInfo {
    /// The INERT value — `linux_cgroup: .never` makes the hook return on its
    /// first branch (`pre_exec.zig:35`), which is what the reference's
    /// `.rt_pre_exec_info = undefined` stands in for at call sites that install
    /// no rt hook (its ten `test`s, `Command.zig:395,428,...`).
    fn default() -> Self {
        Self {
            gtk_single_instance: GtkSingleInstance::DEFAULT,
            linux_cgroup: LinuxCgroup::Never,
            linux_cgroup_hard_fail: false,
        }
    }
}

/// Configuration information needed by the apprt post fork function. Note
/// that this should be a trivially copyable struct and not require any
/// allocation/deallocation.
//
// PORT NOTE: the reference's `apprt.runtime.post_fork.PostForkInfo`
// (`src/apprt/gtk/post_fork.zig:15-31`) — see the seam note above and the
// `RtPreExecInfo` note; `rustty_apprt::gtk::post_fork::PostForkInfo` aliases it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtPostForkInfo {
    /// `gtk_single_instance: configpkg.Config.GtkSingleInstance`
    /// (`post_fork.zig:16`).
    pub gtk_single_instance: GtkSingleInstance,
    /// `linux_cgroup: configpkg.Config.LinuxCgroup` (`post_fork.zig:17`).
    pub linux_cgroup: LinuxCgroup,
    /// `linux_cgroup_hard_fail: bool` (`post_fork.zig:18`).
    pub linux_cgroup_hard_fail: bool,
    /// `linux_cgroup_memory_limit: ?u64` (`post_fork.zig:19`).
    pub linux_cgroup_memory_limit: Option<u64>,
    /// `linux_cgroup_processes_limit: ?u64` (`post_fork.zig:20`).
    pub linux_cgroup_processes_limit: Option<u64>,
}

impl RtPostForkInfo {
    /// PORTS: `post_fork.zig:22-30` `pub fn init(cfg: *const configpkg.Config)`.
    #[inline]
    #[must_use]
    pub fn init(cfg: &Config) -> Self {
        Self {
            gtk_single_instance: cfg.gtk_single_instance,
            linux_cgroup: cfg.linux_cgroup,
            linux_cgroup_hard_fail: cfg.linux_cgroup_hard_fail,
            linux_cgroup_memory_limit: cfg.linux_cgroup_memory_limit,
            linux_cgroup_processes_limit: cfg.linux_cgroup_processes_limit,
        }
    }
}

impl Default for RtPostForkInfo {
    /// The INERT value — see [`RtPreExecInfo::default`] (`post_fork.zig:43`).
    fn default() -> Self {
        Self {
            gtk_single_instance: GtkSingleInstance::DEFAULT,
            linux_cgroup: LinuxCgroup::Never,
            linux_cgroup_hard_fail: false,
            linux_cgroup_memory_limit: None,
            linux_cgroup_processes_limit: None,
        }
    }
}

/// The pair of apprt hooks the reference picks at comptime
/// (`Exec.zig:1034,1036`), carried as data because Rust must pick them from
/// above. See the seam note above [`RtPreExecInfo`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RtHooks {
    /// `apprt.runtime.pre_exec.preExec` when the apprt declares it, else `null`.
    pub pre_exec: Option<PreExecFn>,
    /// `apprt.runtime.post_fork.postFork` when the apprt declares it, else
    /// `null`.
    pub post_fork: Option<PostForkFn>,
}

/// PORT NOTE (Rust-only): `#[derive(PartialEq)]` on a struct of function
/// POINTERS is linted (`unpredictable_function_pointer_comparisons`) because
/// `==` on `fn` items is not guaranteed meaningful across codegen units.
/// `core::ptr::fn_addr_eq` is the sanctioned spelling; the only consumer is
/// [`install_rt_hooks`]'s double-install assert, whose two operands come from
/// the SAME call site when it is legitimate, so a false negative there would
/// require the same `fn` item to be emitted twice.
impl PartialEq for RtHooks {
    fn eq(&self, other: &Self) -> bool {
        fn same<T: Copy>(a: Option<T>, b: Option<T>, eq: impl Fn(T, T) -> bool) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => eq(a, b),
                _ => false,
            }
        }
        same(self.pre_exec, other.pre_exec, |a, b| {
            core::ptr::fn_addr_eq(a, b)
        }) && same(self.post_fork, other.post_fork, |a, b| {
            core::ptr::fn_addr_eq(a, b)
        })
    }
}

impl Eq for RtHooks {}

/// The process-global stand-in for the reference's comptime `apprt.runtime`
/// selection. `None` == the `@hasDecl` branch that yields `null` for both hooks,
/// which is what EVERY apprt other than GTK produces.
static RT_HOOKS: std::sync::RwLock<Option<RtHooks>> = std::sync::RwLock::new(None);

/// Install the apprt's `rt_pre_exec`/`rt_post_fork` hooks. Called ONCE by the
/// apprt during its own init; every `Command` built by `Exec` afterwards carries
/// them (`Exec.zig:1034-1037`).
///
/// # Panics
/// If called a second time with a DIFFERENT pair. The reference's selection is a
/// build-time constant and so cannot vary within a process; re-installing a
/// different apprt's hooks would mean two apprts live at once, which is not a
/// state the reference can represent. Re-installing the SAME pair is a no-op, so
/// an apprt that initializes twice (tests, `none`+embedded) stays fine.
pub fn install_rt_hooks(hooks: RtHooks) {
    let mut slot = RT_HOOKS.write().unwrap_or_else(|e| e.into_inner());
    match *slot {
        None => *slot = Some(hooks),
        Some(existing) => assert!(
            existing == hooks,
            "install_rt_hooks: a different apprt's hooks are already installed \
             (the reference selects these at comptime; they cannot change)",
        ),
    }
}

/// The installed hooks, or the all-`null` pair when no apprt installed any.
///
/// Read by `Exec` at the `Command` construction site, mirroring the reference's
/// `if (comptime @hasDecl(apprt.runtime, "pre_exec")) ... else null`
/// (`Exec.zig:1034,1036`).
#[must_use]
pub fn rt_hooks() -> RtHooks {
    RT_HOOKS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_default()
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
        //
        // Wrapper for the raw fork syscall. This preserves the error handling from
        // the std.posix wrapper that was removed in Zig 0.16.
        //
        // PORT NOTE: Zig `posix.fork()` maps EAGAIN/ENOMEM to `error.SystemResources`
        // and anything else to `unexpectedErrno`; `a77c706a1` hoisted that mapping
        // into a local `fn fork()` when the std wrapper went away. rustty has
        // always had the mapping inline here, so the extraction is cosmetic.
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

        // const err: posix.E = execve: { ... };  (Command.zig:227-257)
        let err = execvpe(self.path, argsZ.as_ptr(), envp);

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
        write_exec_failure(&mut stderr, err);
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
        let mut attribute_list_buf: Vec<u8>;
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
            // (The reference's Windows arm always waits; `block` is unused.)
            let _ = block;
            // Block until the process exits. This returns immediately if the
            // process already exited.
            //
            // NOTE: We can use the pid directly as posix.system.pid_t is still an
            // alias for a handle under Windows. We might want to keep an eye on if
            // this changes, though.
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
                // _ = try waitPid(self.pid.?, &status, 0);
                posix_waitpid(self.pid.unwrap(), 0)?
            } else {
                // We specify NOHANG because its not our fault if the process we launch
                // for the tty doesn't properly waitpid its children. We don't want
                // to hang the terminal over it.
                // When NOHANG is specified, waitpid will return a pid of 0 if the process
                // doesn't have a status to report. When that happens, it is as though the
                // wait call has not been performed, so we need to keep trying until we get
                // a non-zero pid back, otherwise we end up with zombie processes.
                loop {
                    // const pid = try waitPid(self.pid.?, &status, posix.system.W.NOHANG);
                    let res = posix_waitpid(self.pid.unwrap(), libc::WNOHANG as u32)?;
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
        env_buf.extend_from_slice(key);
        // env_buf[pair.key_ptr.len] = '=';
        env_buf.push(b'=');
        // @memcpy(env_buf[pair.key_ptr.len + 1 ..], pair.value_ptr.*);
        env_buf.extend_from_slice(value);
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
/// PORT NOTE (P3-W7 T2): `std.unicode.utf8ToUtf16Le` over an `EnvMap` entry.
/// The map is byte-typed (`Vec<u8>`), and on Windows those bytes are the
/// lossless WTF-8 encoding `OsStr::as_encoded_bytes` produces, so the round
/// trip back to UTF-16 goes through `OsStr` rather than `str` — a non-UTF-8
/// (unpaired-surrogate) value must not be corrupted here.
#[cfg_attr(not(windows), allow(dead_code))]
fn encode_wide(bytes: &[u8]) -> Vec<u16> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        rustty_global::environ::os_string_from_bytes(bytes)
            .as_os_str()
            .encode_wide()
            .collect()
    }
    #[cfg(not(windows))]
    {
        // Compiled on all targets (the reference's `createWindowsEnvBlock` is
        // too); off Windows the bytes are plain POSIX bytes.
        String::from_utf8_lossy(bytes).encode_utf16().collect()
    }
}

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
        for u in encode_wide(key) {
            result[i] = u;
            i += 1;
        }
        result[i] = '=' as u16;
        i += 1;
        // i += try std.unicode.utf8ToUtf16Le(result[i..], pair.value_ptr.*);
        for u in encode_wide(value) {
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

/// `setupFd`'s local `PosixCall.f` (`Command.zig:441-459`): issue `call`
/// until it does not fail with `EINTR`, mapping a failure's errno the way the
/// reference does. `call` returns the libc result (`-1` on failure, errno set).
///
/// S39-F4: the port used this mapping for `dup3` only; the macOS/FreeBSD arm
/// issued its two `fcntl`s bare (no `EINTR` retry, every errno `Unexpected`)
/// and a `posix.dup2` shim that retried `EBUSY`, where `PosixCall` returns
/// `error.FileBusy`.
#[cfg(not(windows))]
fn posix_call(mut call: impl FnMut() -> libc::c_int) -> Result<usize, Error> {
    // while (true) {
    loop {
        // const rc = @call(.auto, func, args);
        let rc = call();
        // switch (posix.errno(rc)) {
        if rc >= 0 {
            // .SUCCESS => return @intCast(rc),
            return Ok(rc as usize);
        }
        match errno() {
            // .INTR => continue,
            libc::EINTR => continue,
            // .AGAIN, .ACCES => return error.Locked,
            libc::EAGAIN | libc::EACCES => return Err(Error::Locked),
            // .BADF => unreachable,
            libc::EBADF => unreachable!(),
            // .BUSY => return error.FileBusy,
            libc::EBUSY => return Err(Error::FileBusy),
            // .INVAL => unreachable, // invalid parameters
            libc::EINVAL => unreachable!(), // invalid parameters
            // .PERM => return error.PermissionDenied,
            libc::EPERM => return Err(Error::PermissionDenied),
            // .MFILE => return error.ProcessFdQuotaExceeded,
            libc::EMFILE => return Err(Error::ProcessFdQuotaExceeded),
            // .NOTDIR => unreachable, // invalid parameter
            libc::ENOTDIR => unreachable!(), // invalid parameter
            // .DEADLK => return error.DeadLock,
            libc::EDEADLK => return Err(Error::DeadLock),
            // .NOLCK => return error.LockedRegionLimitExceeded,
            libc::ENOLCK => return Err(Error::LockedRegionLimitExceeded),
            // else => |err| return posix.unexpectedErrno(err),
            err => return Err(Error::Unexpected(err)),
        }
    }
}

/// The syscalls `setupFd` issues, as the test-only fail-point names them.
#[cfg(not(windows))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetupFdCall {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Dup3,
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    GetFd,
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    SetFd,
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Dup2,
}

/// Issue one of `setupFd`'s syscalls. In production this is the call itself.
#[cfg(all(not(windows), not(test)))]
#[inline(always)]
fn setup_fd_syscall(_which: SetupFdCall, call: impl FnOnce() -> libc::c_int) -> libc::c_int {
    call()
}

/// RUSTTY-ONLY, TEST-ONLY (task T-F2, S39-F4): the same, plus a per-thread
/// fail-point. While armed (`setup_fd_fault::arm`), each call is recorded, and
/// a call whose turn in the fault list has come fails with that errno instead
/// of being issued. Unarmed, it reads only a `Cell<bool>` (no allocation, no
/// TLS destructor), so the forking tests that reach `setupFd` in a child are
/// unaffected.
#[cfg(all(not(windows), test))]
fn setup_fd_syscall(which: SetupFdCall, call: impl FnOnce() -> libc::c_int) -> libc::c_int {
    match setup_fd_fault::next(which) {
        Some(errno) => {
            // SAFETY: writes this thread's errno.
            unsafe {
                #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
                {
                    *libc::__error() = errno;
                }
                #[cfg(target_os = "linux")]
                {
                    *libc::__errno_location() = errno;
                }
            }
            -1
        }
        None => call(),
    }
}

// Only the macOS/FreeBSD arm's test arms it.
#[cfg(all(not(windows), test))]
#[cfg_attr(target_os = "linux", allow(dead_code))]
mod setup_fd_fault {
    use super::SetupFdCall;
    use std::cell::Cell;

    const MAX: usize = 16;
    type Faults = [Option<(SetupFdCall, i32)>; MAX];
    type Log = ([Option<SetupFdCall>; MAX], usize);
    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static FAULTS: Cell<Faults> = const { Cell::new([None; MAX]) };
        static LOG: Cell<Log> = const { Cell::new(([None; MAX], 0)) };
    }

    /// Arm the fail-point: the faults are used in order, each failing the
    /// next call of its kind once the earlier ones are used. Clears the log.
    pub(super) fn arm(faults: &[(SetupFdCall, i32)]) {
        assert!(faults.len() <= MAX);
        let mut f = [None; MAX];
        for (slot, &fault) in f.iter_mut().zip(faults) {
            *slot = Some(fault);
        }
        FAULTS.set(f);
        LOG.set(([None; MAX], 0));
        ARMED.set(true);
    }

    /// Disarm, returning the calls attempted since `arm` and whether every
    /// fault was used.
    pub(super) fn disarm() -> (Vec<SetupFdCall>, bool) {
        ARMED.set(false);
        let (log, n) = LOG.get();
        let used = FAULTS.get().iter().all(Option::is_none);
        (log[..n].iter().map(|c| c.unwrap()).collect(), used)
    }

    pub(super) fn next(which: SetupFdCall) -> Option<i32> {
        if !ARMED.get() {
            return None;
        }
        let (mut log, n) = LOG.get();
        assert!(
            n < MAX,
            "setupFd made more calls than the fail-point records"
        );
        log[n] = Some(which);
        LOG.set((log, n + 1));
        let mut faults = FAULTS.get();
        let first = faults.iter().position(Option::is_some)?;
        match faults[first] {
            Some((call, errno)) if call == which => {
                faults[first] = None;
                FAULTS.set(faults);
                Some(errno)
            }
            _ => None,
        }
    }
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
        // _ = try PosixCall.f(linux.dup3, .{ src, target, 0 });
        posix_call(|| {
            setup_fd_syscall(SetupFdCall::Dup3, || unsafe { libc::dup3(src, target, 0) })
        })?;
        Ok(())
    }
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    {
        // Mac doesn't support dup3 so we use dup2. We purposely clear
        // CLO_ON_EXEC for this fd.
        // const flags = try PosixCall.f(posix.system.fcntl, .{ src, posix.F.GETFD });
        let flags = posix_call(|| {
            setup_fd_syscall(SetupFdCall::GetFd, || unsafe {
                libc::fcntl(src, libc::F_GETFD)
            })
        })? as libc::c_int;
        if flags & libc::FD_CLOEXEC != 0 {
            // _ = try PosixCall.f(posix.system.fcntl,
            //     .{ src, posix.F.SETFD, flags & ~@as(u32, posix.FD_CLOEXEC) });
            posix_call(|| {
                setup_fd_syscall(SetupFdCall::SetFd, || unsafe {
                    libc::fcntl(src, libc::F_SETFD, flags & !libc::FD_CLOEXEC)
                })
            })?;
        }

        // _ = try PosixCall.f(posix.system.dup2, .{ src, target });
        posix_call(|| setup_fd_syscall(SetupFdCall::Dup2, || unsafe { libc::dup2(src, target) }))?;
        Ok(())
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

/// Wrapper for the raw waitpid syscall. Status is only initialized on success;
/// interrupted waits are retried and all other errors are propagated.
///
/// PORT NOTE (`a77c706a1`): at the pin this went through Zig std's
/// `posix.waitpid`, whose contract makes `ECHILD`/`EINVAL` `unreachable` — and
/// rustty reproduced that as a `unreachable!()` panic. Zig 0.16 removed the
/// wrapper, so upstream hand-rolled this one and took the opportunity to
/// PROPAGATE those two instead:
///
/// ```zig
/// .CHILD => return error.NoChildProcess,
/// .INVAL => return error.InvalidWaitOptions,
/// ```
///
/// rustty follows, which removes a reachable panic: `ECHILD` is genuinely
/// reachable here because rustty has a second reaper in
/// `crates/rustty-xev/src/process.rs` (see [`posix_waitpid_checked`]).
#[cfg(not(windows))]
pub(crate) fn posix_waitpid(pid: libc::pid_t, flags: u32) -> Result<WaitPidResult, Error> {
    loop {
        let mut status: libc::c_int = 0;
        let rc = unsafe { libc::waitpid(pid, &mut status, flags as libc::c_int) };
        if rc >= 0 {
            return Ok(WaitPidResult {
                pid: rc,
                status: status as u32,
            });
        }
        match errno() {
            libc::EINTR => continue,
            // .CHILD => return error.NoChildProcess,
            libc::ECHILD => return Err(Error::NoChildProcess),
            // .INVAL => return error.InvalidWaitOptions,
            libc::EINVAL => return Err(Error::InvalidWaitOptions),
            // else => |err| return posix.unexpectedErrno(err),
            err => return Err(Error::Unexpected(err)),
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

/// `@tagName(e)` for the reference's `posix.E`: the errno's tag name in Zig
/// 0.16's enum for this target, which the child's exec-failure report prints
/// after an `E` (`EACCES`, `ENOEXEC`, `E2BIG`). S39-F2: the port printed the
/// name of a Zig-std error-set member instead (`error.AccessDenied`).
///
/// The tables are the enums' fields as Zig 0.16.0 itself enumerates them
/// (`@typeInfo(std.posix.E).@"enum".fields`, dumped at comptime for
/// `aarch64-macos`, `aarch64-linux` (identical to `x86_64-linux`: every Linux
/// arch but mips and sparc shares one enum) and `x86_64-freebsd`).
///
/// PORT NOTE: a value with no name (none of them is an errno `execve` returns)
/// is `None`. The reference's `@tagName` of such a value is a safety panic in
/// the forked child; the caller prints the number instead.
#[cfg(not(windows))]
fn errno_tag_name(e: i32) -> Option<&'static str> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const TAGS: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "DEADLK"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "AGAIN"),
        (36, "INPROGRESS"),
        (37, "ALREADY"),
        (38, "NOTSOCK"),
        (39, "DESTADDRREQ"),
        (40, "MSGSIZE"),
        (41, "PROTOTYPE"),
        (42, "NOPROTOOPT"),
        (43, "PROTONOSUPPORT"),
        (44, "SOCKTNOSUPPORT"),
        (45, "OPNOTSUPP"),
        (46, "PFNOSUPPORT"),
        (47, "AFNOSUPPORT"),
        (48, "ADDRINUSE"),
        (49, "ADDRNOTAVAIL"),
        (50, "NETDOWN"),
        (51, "NETUNREACH"),
        (52, "NETRESET"),
        (53, "CONNABORTED"),
        (54, "CONNRESET"),
        (55, "NOBUFS"),
        (56, "ISCONN"),
        (57, "NOTCONN"),
        (58, "SHUTDOWN"),
        (59, "TOOMANYREFS"),
        (60, "TIMEDOUT"),
        (61, "CONNREFUSED"),
        (62, "LOOP"),
        (63, "NAMETOOLONG"),
        (64, "HOSTDOWN"),
        (65, "HOSTUNREACH"),
        (66, "NOTEMPTY"),
        (67, "PROCLIM"),
        (68, "USERS"),
        (69, "DQUOT"),
        (70, "STALE"),
        (71, "REMOTE"),
        (72, "BADRPC"),
        (73, "RPCMISMATCH"),
        (74, "PROGUNAVAIL"),
        (75, "PROGMISMATCH"),
        (76, "PROCUNAVAIL"),
        (77, "NOLCK"),
        (78, "NOSYS"),
        (79, "FTYPE"),
        (80, "AUTH"),
        (81, "NEEDAUTH"),
        (82, "PWROFF"),
        (83, "DEVERR"),
        (84, "OVERFLOW"),
        (85, "BADEXEC"),
        (86, "BADARCH"),
        (87, "SHLIBVERS"),
        (88, "BADMACHO"),
        (89, "CANCELED"),
        (90, "IDRM"),
        (91, "NOMSG"),
        (92, "ILSEQ"),
        (93, "NOATTR"),
        (94, "BADMSG"),
        (95, "MULTIHOP"),
        (96, "NODATA"),
        (97, "NOLINK"),
        (98, "NOSR"),
        (99, "NOSTR"),
        (100, "PROTO"),
        (101, "TIME"),
        (103, "NOPOLICY"),
        (104, "NOTRECOVERABLE"),
        (105, "OWNERDEAD"),
        (106, "QFULL"),
    ];
    #[cfg(all(
        target_os = "linux",
        not(any(
            target_arch = "mips",
            target_arch = "mips64",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    const TAGS: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "AGAIN"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "DEADLK"),
        (36, "NAMETOOLONG"),
        (37, "NOLCK"),
        (38, "NOSYS"),
        (39, "NOTEMPTY"),
        (40, "LOOP"),
        (42, "NOMSG"),
        (43, "IDRM"),
        (44, "CHRNG"),
        (45, "L2NSYNC"),
        (46, "L3HLT"),
        (47, "L3RST"),
        (48, "LNRNG"),
        (49, "UNATCH"),
        (50, "NOCSI"),
        (51, "L2HLT"),
        (52, "BADE"),
        (53, "BADR"),
        (54, "XFULL"),
        (55, "NOANO"),
        (56, "BADRQC"),
        (57, "BADSLT"),
        (59, "BFONT"),
        (60, "NOSTR"),
        (61, "NODATA"),
        (62, "TIME"),
        (63, "NOSR"),
        (64, "NONET"),
        (65, "NOPKG"),
        (66, "REMOTE"),
        (67, "NOLINK"),
        (68, "ADV"),
        (69, "SRMNT"),
        (70, "COMM"),
        (71, "PROTO"),
        (72, "MULTIHOP"),
        (73, "DOTDOT"),
        (74, "BADMSG"),
        (75, "OVERFLOW"),
        (76, "NOTUNIQ"),
        (77, "BADFD"),
        (78, "REMCHG"),
        (79, "LIBACC"),
        (80, "LIBBAD"),
        (81, "LIBSCN"),
        (82, "LIBMAX"),
        (83, "LIBEXEC"),
        (84, "ILSEQ"),
        (85, "RESTART"),
        (86, "STRPIPE"),
        (87, "USERS"),
        (88, "NOTSOCK"),
        (89, "DESTADDRREQ"),
        (90, "MSGSIZE"),
        (91, "PROTOTYPE"),
        (92, "NOPROTOOPT"),
        (93, "PROTONOSUPPORT"),
        (94, "SOCKTNOSUPPORT"),
        (95, "OPNOTSUPP"),
        (96, "PFNOSUPPORT"),
        (97, "AFNOSUPPORT"),
        (98, "ADDRINUSE"),
        (99, "ADDRNOTAVAIL"),
        (100, "NETDOWN"),
        (101, "NETUNREACH"),
        (102, "NETRESET"),
        (103, "CONNABORTED"),
        (104, "CONNRESET"),
        (105, "NOBUFS"),
        (106, "ISCONN"),
        (107, "NOTCONN"),
        (108, "SHUTDOWN"),
        (109, "TOOMANYREFS"),
        (110, "TIMEDOUT"),
        (111, "CONNREFUSED"),
        (112, "HOSTDOWN"),
        (113, "HOSTUNREACH"),
        (114, "ALREADY"),
        (115, "INPROGRESS"),
        (116, "STALE"),
        (117, "UCLEAN"),
        (118, "NOTNAM"),
        (119, "NAVAIL"),
        (120, "ISNAM"),
        (121, "REMOTEIO"),
        (122, "DQUOT"),
        (123, "NOMEDIUM"),
        (124, "MEDIUMTYPE"),
        (125, "CANCELED"),
        (126, "NOKEY"),
        (127, "KEYEXPIRED"),
        (128, "KEYREVOKED"),
        (129, "KEYREJECTED"),
        (130, "OWNERDEAD"),
        (131, "NOTRECOVERABLE"),
        (132, "RFKILL"),
        (133, "HWPOISON"),
        (160, "NSRNODATA"),
        (161, "NSRFORMERR"),
        (162, "NSRSERVFAIL"),
        (163, "NSRNOTFOUND"),
        (164, "NSRNOTIMP"),
        (165, "NSRREFUSED"),
        (166, "NSRBADQUERY"),
        (167, "NSRBADNAME"),
        (168, "NSRBADFAMILY"),
        (169, "NSRBADRESP"),
        (170, "NSRCONNREFUSED"),
        (171, "NSRTIMEOUT"),
        (172, "NSROF"),
        (173, "NSRFILE"),
        (174, "NSRNOMEM"),
        (175, "NSRDESTRUCTION"),
        (176, "NSRQUERYDOMAINTOOLONG"),
        (177, "NSRCNAMELOOP"),
    ];
    #[cfg(target_os = "freebsd")]
    const TAGS: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "DEADLK"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "AGAIN"),
        (36, "INPROGRESS"),
        (37, "ALREADY"),
        (38, "NOTSOCK"),
        (39, "DESTADDRREQ"),
        (40, "MSGSIZE"),
        (41, "PROTOTYPE"),
        (42, "NOPROTOOPT"),
        (43, "PROTONOSUPPORT"),
        (44, "SOCKTNOSUPPORT"),
        (45, "OPNOTSUPP"),
        (46, "PFNOSUPPORT"),
        (47, "AFNOSUPPORT"),
        (48, "ADDRINUSE"),
        (49, "ADDRNOTAVAIL"),
        (50, "NETDOWN"),
        (51, "NETUNREACH"),
        (52, "NETRESET"),
        (53, "CONNABORTED"),
        (54, "CONNRESET"),
        (55, "NOBUFS"),
        (56, "ISCONN"),
        (57, "NOTCONN"),
        (58, "SHUTDOWN"),
        (59, "TOOMANYREFS"),
        (60, "TIMEDOUT"),
        (61, "CONNREFUSED"),
        (62, "LOOP"),
        (63, "NAMETOOLONG"),
        (64, "HOSTDOWN"),
        (65, "HOSTUNREACH"),
        (66, "NOTEMPTY"),
        (67, "PROCLIM"),
        (68, "USERS"),
        (69, "DQUOT"),
        (70, "STALE"),
        (71, "REMOTE"),
        (72, "BADRPC"),
        (73, "RPCMISMATCH"),
        (74, "PROGUNAVAIL"),
        (75, "PROGMISMATCH"),
        (76, "PROCUNAVAIL"),
        (77, "NOLCK"),
        (78, "NOSYS"),
        (79, "FTYPE"),
        (80, "AUTH"),
        (81, "NEEDAUTH"),
        (82, "IDRM"),
        (83, "NOMSG"),
        (84, "OVERFLOW"),
        (85, "CANCELED"),
        (86, "ILSEQ"),
        (87, "NOATTR"),
        (88, "DOOFUS"),
        (89, "BADMSG"),
        (90, "MULTIHOP"),
        (91, "NOLINK"),
        (92, "PROTO"),
        (93, "NOTCAPABLE"),
        (94, "CAPMODE"),
        (95, "NOTRECOVERABLE"),
        (96, "OWNERDEAD"),
        (97, "INTEGRITY"),
    ];
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        all(
            target_os = "linux",
            not(any(
                target_arch = "mips",
                target_arch = "mips64",
                target_arch = "sparc",
                target_arch = "sparc64"
            ))
        )
    )))]
    const TAGS: &[(i32, &str)] = &[];

    TAGS.iter().find(|&&(v, _)| v == e).map(|&(_, name)| name)
}

/// The child's exec-failure report (`Command.zig:259-280`), formatted into the
/// stack writer: the friendly message for `NOENT`, else
/// `exec syscall failed with unexpected error: E{s}` with the errno's tag.
#[cfg(not(windows))]
fn write_exec_failure(stderr: &mut impl core::fmt::Write, err: i32) {
    match err {
        // posix.system.E.NOENT => stderr.print(...) catch {},
        libc::ENOENT => {
            let _ = writeln!(
                stderr,
                "Requested executable not found. Please verify the command is on\nthe PATH and try again."
            );
        }

        // else => |e| stderr.print("exec syscall failed with unexpected error: E{s}\n", .{@tagName(e)}) catch {},
        e => match errno_tag_name(e) {
            Some(tag) => {
                let _ = writeln!(stderr, "exec syscall failed with unexpected error: E{tag}");
            }
            None => {
                let _ = writeln!(stderr, "exec syscall failed with unexpected error: E{e}");
            }
        },
    }
}

/// `posix.errno(posix.system.execve(path, argsZ, envp))`: execve(2) returns
/// only on failure, and the reference switches over the raw errno.
#[cfg(not(windows))]
fn execve_errno(
    path: *const c_char,
    child_argv: *const *const c_char,
    envp: *const *const c_char,
) -> i32 {
    let _ = unsafe { libc::execve(path, child_argv, envp) };
    errno()
}

/// The `const err: posix.E = execve: { ... }` block of `startPosix`
/// (`Command.zig:227-257`): "This functionality has been taken from Zig
/// stdlib, a simplified version of the exec bits with PATH search so that we
/// can just offload to execve below." A file containing '/' execs directly;
/// otherwise every PATH segment is tried with the candidate built in a stack
/// buffer. NO heap allocation (we are between fork and exec — see `start`).
///
/// S39-F1: this used to be a port of Zig std's `posix.execvpeZ`, which maps the
/// errno into an error set first (`EPERM` and `EACCES` both become
/// `AccessDenied`) so an `EPERM` continued the PATH search. The reference
/// switches on the raw errno, and `EPERM` ends the search.
#[cfg(not(windows))]
fn execvpe(file: &CStr, child_argv: *const *const c_char, envp: *const *const c_char) -> i32 {
    let file_slice = file.to_bytes();
    // if (std.mem.findScalar(u8, file_slice, '/') != null) {
    //     break :execve posix.errno(posix.system.execve(self.path, argsZ, envp));
    // }
    if file_slice.contains(&b'/') {
        return execve_errno(file.as_ptr(), child_argv, envp);
    }

    // const PATH = global.environ().getPosix("PATH") orelse "/usr/local/bin:/bin/:/usr/bin";
    // (libc getenv: reads the environ block in place, no allocation.)
    //
    // PORT NOTE (`e8525c0fd`): upstream moved this read from `posix.getenvZ`
    // to the captured global environ. rustty deliberately keeps `libc::getenv`:
    // this runs BETWEEN fork and exec in the child, where the only safe
    // operations are async-signal-safe ones. `rustty_global::environ()` takes
    // an `RwLock` read guard, and a lock held by another thread at fork time is
    // not released in the child — a deadlock. The two reads see the same block
    // because `sync_environ` publishes from `environ` in the first place.
    let path_env = unsafe { libc::getenv(c"PATH".as_ptr()) };
    let path_bytes: &[u8] = if path_env.is_null() {
        b"/usr/local/bin:/bin/:/usr/bin"
    } else {
        unsafe { CStr::from_ptr(path_env) }.to_bytes()
    };

    execvpe_search(file_slice, path_bytes, |full_path| {
        execve_errno(full_path, child_argv, envp)
    })
}

/// The PATH-search half of [`execvpe`], split out so the search's errno
/// switch is testable without mutating the process environment. `execve` is
/// the exec call (the real one returns `posix.errno(posix.system.execve(..))`);
/// the result is the errno the search ends on.
#[cfg(not(windows))]
fn execvpe_search(
    file_slice: &[u8],
    path_bytes: &[u8],
    mut execve: impl FnMut(*const c_char) -> i32,
) -> i32 {
    // var path_expanded_buf: [std.fs.max_path_bytes]u8 = undefined;
    let mut path_expanded_buf = [0u8; libc::PATH_MAX as usize];
    // var it = std.mem.tokenizeScalar(u8, PATH, ':');
    // var err: posix.system.E = .NOENT;
    let mut err = libc::ENOENT;
    // var seen_eacces = false;
    let mut seen_eacces = false;

    for search_path in path_bytes.split(|&b| b == b':').filter(|s| !s.is_empty()) {
        // const path_len = search_path.len + file_slice.len + 1;
        let path_len = search_path.len() + file_slice.len() + 1;
        // if (path_expanded_buf.len < path_len + 1) break :execve .NAMETOOLONG;
        if path_expanded_buf.len() < path_len + 1 {
            return libc::ENAMETOOLONG;
        }
        path_expanded_buf[..search_path.len()].copy_from_slice(search_path);
        path_expanded_buf[search_path.len()] = b'/';
        path_expanded_buf[search_path.len() + 1..][..file_slice.len()].copy_from_slice(file_slice);
        path_expanded_buf[path_len] = 0;
        let full_path = path_expanded_buf.as_ptr() as *const c_char;
        // Replace here, switch on error (any error means that replace
        // failed, but we might need to retry).
        err = execve(full_path);
        match err {
            // .ACCES => seen_eacces = true,
            libc::EACCES => seen_eacces = true,
            // .NOENT, .NOTDIR => {},
            libc::ENOENT | libc::ENOTDIR => {}
            // else => break :execve err,
            _ => return err,
        }
    }

    // if (seen_eacces) break :execve .ACCES;
    if seen_eacces {
        return libc::EACCES;
    }
    // break :execve err;
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

    /// KAT (rustty-only margin; V13). `RtPreExecInfo::init` is the reference's
    /// `pre_exec.zig:16-22` — it must copy the THREE named config fields, and
    /// exactly those. (Pre-V13 it discarded the config entirely, so the GTK
    /// hook could not have read a thing.)
    #[test]
    fn rt_pre_exec_info_init_copies_the_config_fields() {
        let mut cfg = Config {
            gtk_single_instance: GtkSingleInstance::True,
            linux_cgroup: LinuxCgroup::Always,
            linux_cgroup_hard_fail: true,
            ..Config::default()
        };

        assert_eq!(
            RtPreExecInfo::init(&cfg),
            RtPreExecInfo {
                gtk_single_instance: GtkSingleInstance::True,
                linux_cgroup: LinuxCgroup::Always,
                linux_cgroup_hard_fail: true,
            }
        );

        // The other polarity, so a hard-coded constant cannot pass.
        cfg.gtk_single_instance = GtkSingleInstance::False;
        cfg.linux_cgroup = LinuxCgroup::SingleInstance;
        cfg.linux_cgroup_hard_fail = false;
        assert_eq!(
            RtPreExecInfo::init(&cfg),
            RtPreExecInfo {
                gtk_single_instance: GtkSingleInstance::False,
                linux_cgroup: LinuxCgroup::SingleInstance,
                linux_cgroup_hard_fail: false,
            }
        );
    }

    /// KAT (rustty-only margin; V13). `RtPostForkInfo::init` is
    /// `post_fork.zig:22-30` — the same three fields plus the two cgroup
    /// limits.
    #[test]
    fn rt_post_fork_info_init_copies_the_config_fields() {
        let mut cfg = Config {
            gtk_single_instance: GtkSingleInstance::True,
            linux_cgroup: LinuxCgroup::Always,
            linux_cgroup_hard_fail: true,
            linux_cgroup_memory_limit: Some(1234),
            linux_cgroup_processes_limit: Some(56),
            ..Config::default()
        };

        assert_eq!(
            RtPostForkInfo::init(&cfg),
            RtPostForkInfo {
                gtk_single_instance: GtkSingleInstance::True,
                linux_cgroup: LinuxCgroup::Always,
                linux_cgroup_hard_fail: true,
                linux_cgroup_memory_limit: Some(1234),
                linux_cgroup_processes_limit: Some(56),
            }
        );

        cfg.linux_cgroup_memory_limit = None;
        cfg.linux_cgroup_processes_limit = Some(7);
        cfg.linux_cgroup_hard_fail = false;
        assert_eq!(
            RtPostForkInfo::init(&cfg),
            RtPostForkInfo {
                gtk_single_instance: GtkSingleInstance::True,
                linux_cgroup: LinuxCgroup::Always,
                linux_cgroup_hard_fail: false,
                linux_cgroup_memory_limit: None,
                linux_cgroup_processes_limit: Some(7),
            }
        );
    }

    /// KAT (rustty-only margin; V13). The `Default` both `Info` types carry for
    /// the reference's `.rt_*_info = undefined` call sites must be INERT — a
    /// `.never` cgroup mode, which is the first branch of both hooks
    /// (`pre_exec.zig:35` / `post_fork.zig:43`). If this ever drifted to
    /// `.always`, every test that spawns a `Command` would start polling
    /// `/proc` for 250 ms.
    #[test]
    fn rt_info_defaults_are_inert() {
        assert_eq!(RtPreExecInfo::default().linux_cgroup, LinuxCgroup::Never);
        assert!(!RtPreExecInfo::default().linux_cgroup_hard_fail);
        assert_eq!(RtPostForkInfo::default().linux_cgroup, LinuxCgroup::Never);
        assert!(!RtPostForkInfo::default().linux_cgroup_hard_fail);
        assert_eq!(RtPostForkInfo::default().linux_cgroup_memory_limit, None);
        assert_eq!(RtPostForkInfo::default().linux_cgroup_processes_limit, None);
    }

    /// NEGATIVE CONTROL (P3-W7 T5 Step 3 / area-E `E29`): the errno of the LAST
    /// PATH entry must be the one reported.
    ///
    /// `execvpeZ`'s `err` was initialized to `FileNotFound` and **never
    /// reassigned**, so a PATH whose last entry is a *file* (not a directory)
    /// reported `NOENT` where upstream reports `NOTDIR`. Before the two-line
    /// fix this test is RED — verified, see the P3-W7 report — and it is the
    /// only assertion that can see the difference, because the value is
    /// observable solely in the child's stderr text.
    ///
    /// No `fork` and no environment mutation: `execve` on a path that cannot
    /// resolve always returns, so the search can be driven in-process.
    #[test]
    #[cfg(not(windows))]
    #[cfg_attr(miri, ignore)] // real execve(2) syscalls
    fn execvpe_latches_the_last_path_entry_errno() {
        // A PATH whose FIRST entry does not exist (-> ENOENT) and whose LAST
        // entry is a regular file (-> ENOTDIR when used as a directory).
        let mut path = std::env::temp_dir();
        path.push(format!("rustty-w7-e29-{}", std::process::id()));
        std::fs::write(&path, b"not a directory").expect("write probe file");

        let file_bytes = b"rustty-w7-definitely-not-a-real-binary";
        let argv: [*const c_char; 1] = [std::ptr::null()];
        let envp: [*const c_char; 1] = [std::ptr::null()];

        let mut path_env: Vec<u8> = b"/rustty-w7-no-such-dir:".to_vec();
        path_env.extend_from_slice(path.as_os_str().as_encoded_bytes());

        let got = execvpe_search(file_bytes, &path_env, |full_path| {
            execve_errno(full_path, argv.as_ptr(), envp.as_ptr())
        });
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            got,
            libc::ENOTDIR,
            "the errno of the FINAL PATH entry must be reported (got {got}); \
             `ENOENT` means `err` is not being latched per iteration"
        );
    }

    /// S39-F1: the PATH search switches on the RAW errno (`Command.zig:249-253`):
    /// `ACCES` is remembered and the search goes on, `NOENT`/`NOTDIR` go on,
    /// and any other errno, `PERM` included, ends the search with that errno.
    /// The port mapped `EPERM` to `AccessDenied` first, so an `EPERM` kept
    /// searching (and could exec a later PATH entry the reference never tries).
    ///
    /// `execve` is replaced by a recorder: no real candidate can be made to
    /// fail with `EPERM` on demand.
    #[test]
    #[cfg(not(windows))]
    fn path_search_ends_on_eperm_and_remembers_eacces() {
        fn search(errnos: &[i32]) -> (i32, Vec<Vec<u8>>) {
            let mut tried: Vec<Vec<u8>> = Vec::new();
            let got = execvpe_search(b"prog", b"/a:/b:/c", |full_path| {
                // SAFETY: the search hands a NUL-terminated path in its buffer.
                tried.push(unsafe { CStr::from_ptr(full_path) }.to_bytes().to_vec());
                errnos[tried.len() - 1]
            });
            (got, tried)
        }

        // EPERM from the first candidate: the search stops there.
        let (got, tried) = search(&[libc::EPERM, 0, 0]);
        assert_eq!(got, libc::EPERM);
        assert_eq!(tried, vec![b"/a/prog".to_vec()]);

        // Any other unexpected errno stops it the same way.
        let (got, tried) = search(&[libc::ENOENT, libc::ENOEXEC, 0]);
        assert_eq!(got, libc::ENOEXEC);
        assert_eq!(tried.len(), 2);

        // EACCES is remembered while the search goes on, and wins over a
        // final ENOENT.
        let (got, tried) = search(&[libc::EACCES, libc::ENOENT, libc::ENOTDIR]);
        assert_eq!(got, libc::EACCES);
        assert_eq!(
            tried,
            vec![
                b"/a/prog".to_vec(),
                b"/b/prog".to_vec(),
                b"/c/prog".to_vec()
            ]
        );

        // Nothing found: the last candidate's errno.
        let (got, _) = search(&[libc::ENOENT, libc::ENOENT, libc::ENOTDIR]);
        assert_eq!(got, libc::ENOTDIR);
    }

    // (POSIX: `write_exec_failure` is the fork/exec child's report.)
    #[cfg(not(windows))]
    fn exec_failure_text(err: i32) -> String {
        let mut out = String::new();
        write_exec_failure(&mut out, err);
        out
    }

    /// S39-F2: the non-`NOENT` report prints `E` and the errno's tag name
    /// (`Command.zig:271-276`), not a Zig-std error-set name.
    #[test]
    #[cfg(not(windows))]
    fn exec_failure_report_prints_the_errno_tag() {
        assert_eq!(
            exec_failure_text(libc::EACCES),
            "exec syscall failed with unexpected error: EACCES\n"
        );
        assert_eq!(
            exec_failure_text(libc::EPERM),
            "exec syscall failed with unexpected error: EPERM\n"
        );
        assert_eq!(
            exec_failure_text(libc::ENOEXEC),
            "exec syscall failed with unexpected error: ENOEXEC\n"
        );
        assert_eq!(
            exec_failure_text(libc::E2BIG),
            "exec syscall failed with unexpected error: E2BIG\n"
        );
        assert_eq!(
            exec_failure_text(libc::ENAMETOOLONG),
            "exec syscall failed with unexpected error: ENAMETOOLONG\n"
        );
        // NOENT keeps its own message.
        assert_eq!(
            exec_failure_text(libc::ENOENT),
            "Requested executable not found. Please verify the command is on\n\
             the PATH and try again.\n"
        );
    }

    /// The tag table spells every name the way Zig 0.16's enum does,
    /// including the ones that differ from the C macro (`EWOULDBLOCK` is
    /// `AGAIN`, `ENOTSUP` is `OPNOTSUPP` on Linux).
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn errno_tag_names_follow_the_zig_enum() {
        assert_eq!(errno_tag_name(0), Some("SUCCESS"));
        assert_eq!(errno_tag_name(libc::EPERM), Some("PERM"));
        assert_eq!(errno_tag_name(libc::EAGAIN), Some("AGAIN"));
        assert_eq!(errno_tag_name(libc::ETXTBSY), Some("TXTBSY"));
        assert_eq!(errno_tag_name(libc::ELOOP), Some("LOOP"));
        #[cfg(target_os = "macos")]
        {
            assert_eq!(errno_tag_name(libc::EBADEXEC), Some("BADEXEC"));
            assert_eq!(errno_tag_name(libc::EBADARCH), Some("BADARCH"));
            assert_eq!(errno_tag_name(libc::EDEADLK), Some("DEADLK"));
            // 102 is not a field of Zig's darwin `E`.
            assert_eq!(errno_tag_name(102), None);
        }
        #[cfg(target_os = "linux")]
        {
            assert_eq!(errno_tag_name(libc::ELIBBAD), Some("LIBBAD"));
            assert_eq!(errno_tag_name(libc::EOPNOTSUPP), Some("OPNOTSUPP"));
            assert_eq!(errno_tag_name(41), None);
        }
    }

    /// Set the calling thread's errno (what a failing libc call leaves).
    #[cfg(not(windows))]
    fn set_errno(e: i32) {
        // SAFETY: the thread-local errno location is always valid.
        unsafe {
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
            {
                *libc::__error() = e;
            }
            #[cfg(target_os = "linux")]
            {
                *libc::__errno_location() = e;
            }
        }
    }

    /// S39-F4: `setupFd`'s `PosixCall.f` (`Command.zig:441-459`), which the
    /// macOS/FreeBSD arm now uses for `fcntl` and `dup2` too: `EINTR` retries,
    /// `EBUSY` is `error.FileBusy` at once (the port's `dup2` shim retried it),
    /// and the other errnos map as the reference maps them.
    #[test]
    #[cfg(not(windows))]
    fn posix_call_retries_eintr_and_maps_errnos() {
        fn call(errnos: &[i32]) -> (Result<usize, Error>, usize) {
            let mut calls = 0usize;
            let got = posix_call(|| {
                calls += 1;
                match errnos.get(calls - 1) {
                    Some(&e) => {
                        set_errno(e);
                        -1
                    }
                    None => 7,
                }
            });
            (got, calls)
        }

        // EINTR is retried until the call succeeds.
        assert_eq!(call(&[libc::EINTR, libc::EINTR]), (Ok(7), 3));
        // EBUSY is not retried.
        assert_eq!(call(&[libc::EBUSY]), (Err(Error::FileBusy), 1));
        assert_eq!(call(&[libc::EAGAIN]), (Err(Error::Locked), 1));
        assert_eq!(call(&[libc::EACCES]), (Err(Error::Locked), 1));
        assert_eq!(call(&[libc::EPERM]), (Err(Error::PermissionDenied), 1));
        assert_eq!(
            call(&[libc::EMFILE]),
            (Err(Error::ProcessFdQuotaExceeded), 1)
        );
        assert_eq!(call(&[libc::EDEADLK]), (Err(Error::DeadLock), 1));
        assert_eq!(
            call(&[libc::ENOLCK]),
            (Err(Error::LockedRegionLimitExceeded), 1)
        );
        assert_eq!(
            call(&[libc::ENOSPC]),
            (Err(Error::Unexpected(libc::ENOSPC)), 1)
        );
        assert_eq!(call(&[]), (Ok(7), 1));
    }

    /// `setupFd` on macOS clears `FD_CLOEXEC` on the source and dups it onto
    /// the target through `PosixCall.f` (a real fd round trip).
    #[test]
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    #[cfg_attr(miri, ignore)] // real fcntl/dup2 syscalls
    fn setup_fd_clears_cloexec_and_dups() {
        use std::os::fd::AsRawFd as _;
        let src = std::fs::File::open("/dev/null").unwrap(); // Rust opens with O_CLOEXEC
        let src_fd = src.as_raw_fd();
        // SAFETY: plain fcntl/dup on fds this test owns.
        let target = unsafe { libc::dup(src_fd) };
        assert!(target >= 0);
        assert_ne!(
            unsafe { libc::fcntl(src_fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );

        setupFd(src_fd, target).unwrap();

        assert_eq!(
            unsafe { libc::fcntl(src_fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        // dup2 leaves the target without FD_CLOEXEC, open on the same file.
        assert_eq!(
            unsafe { libc::fcntl(target, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        unsafe { libc::close(target) };
    }

    /// S39-F4, through `setupFd` itself (task T-F2): each of the macOS arm's
    /// syscalls goes through `PosixCall.f` (`Command.zig:475-483`). A
    /// test-only fail-point fails chosen calls with chosen errnos before they
    /// are issued, and records every call attempted:
    /// - `EINTR` on `fcntl(F_GETFD)`, `fcntl(F_SETFD)` and `dup2` is retried,
    ///   and the fd round trip then completes (the port's bare `fcntl`s
    ///   answered `Unexpected`);
    /// - `EBUSY` on `dup2` is `error.FileBusy` after one attempt (the port's
    ///   `dup2` shim retried it);
    /// - the `fcntl`s map their errnos as `PosixCall.f` does.
    #[test]
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    #[cfg_attr(miri, ignore)] // real fcntl/dup2 syscalls
    fn setup_fd_retries_eintr_and_fails_on_ebusy_through_posix_call() {
        use super::setup_fd_fault;
        use super::SetupFdCall::{Dup2, GetFd, SetFd};
        use std::os::fd::AsRawFd as _;

        let cloexec = |fd| unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC;
        let fresh = || {
            let src = std::fs::File::open("/dev/null").unwrap(); // O_CLOEXEC
                                                                 // SAFETY: plain dup of an fd this test owns.
            let target = unsafe { libc::dup(src.as_raw_fd()) };
            assert!(target >= 0);
            assert_ne!(cloexec(src.as_raw_fd()), 0);
            (src, target)
        };

        // EINTR everywhere is retried; the round trip completes.
        let (src, target) = fresh();
        setup_fd_fault::arm(&[
            (GetFd, libc::EINTR),
            (GetFd, libc::EINTR),
            (SetFd, libc::EINTR),
            (Dup2, libc::EINTR),
        ]);
        let got = setupFd(src.as_raw_fd(), target);
        let (calls, used) = setup_fd_fault::disarm();
        assert_eq!(got, Ok(()));
        assert!(used, "every injected EINTR was hit");
        assert_eq!(calls, [GetFd, GetFd, GetFd, SetFd, SetFd, Dup2, Dup2]);
        assert_eq!(cloexec(src.as_raw_fd()), 0, "FD_CLOEXEC cleared");
        assert_eq!(cloexec(target), 0, "dup2 ran");
        unsafe { libc::close(target) };

        // EBUSY on dup2 is FileBusy at once, not retried.
        let (src, target) = fresh();
        setup_fd_fault::arm(&[(Dup2, libc::EBUSY)]);
        let got = setupFd(src.as_raw_fd(), target);
        let (calls, used) = setup_fd_fault::disarm();
        assert!(used);
        assert_eq!(got, Err(Error::FileBusy));
        assert_eq!(calls, [GetFd, SetFd, Dup2]);
        unsafe { libc::close(target) };

        // The fcntls' errnos map as PosixCall.f maps them.
        for (fault, want, want_calls) in [
            ((GetFd, libc::EAGAIN), Error::Locked, &[GetFd][..]),
            (
                (GetFd, libc::EMFILE),
                Error::ProcessFdQuotaExceeded,
                &[GetFd][..],
            ),
            (
                (SetFd, libc::EPERM),
                Error::PermissionDenied,
                &[GetFd, SetFd][..],
            ),
            ((SetFd, libc::EDEADLK), Error::DeadLock, &[GetFd, SetFd][..]),
        ] {
            let (src, target) = fresh();
            setup_fd_fault::arm(&[fault]);
            let got = setupFd(src.as_raw_fd(), target);
            let (calls, used) = setup_fd_fault::disarm();
            assert!(used);
            assert_eq!(got, Err(want), "{fault:?}");
            assert_eq!(calls, want_calls, "{fault:?}");
            unsafe { libc::close(target) };
        }
    }

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
        envmap.put(b"HOME", b"/home/ifreund");
        envmap.put(b"WAYLAND_DISPLAY", b"wayland-1");
        envmap.put(b"DISPLAY", b":1");
        envmap.put(b"DEBUGINFOD_URLS", b" ");
        envmap.put(b"XCURSOR_SIZE", b"24");

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

    /// N384 (task W-5b): the child's `envp` is the map in its iteration order,
    /// which for the reference's `ArrayHashMap` is insertion order; an updated
    /// entry keeps its place, `orderedRemove` closes the gap and `swapRemove`
    /// moves the last entry into it.
    #[test]
    fn create_null_delimited_env_map_keeps_the_map_order() {
        let envp = |m: &EnvMap| -> Vec<Vec<u8>> {
            let environ = createNullDelimitedEnvMap(m);
            environ.ptrs[..environ.len()]
                .iter()
                .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes().to_vec())
                .collect()
        };
        let mut m = EnvMap::new();
        for k in [&b"ZED"[..], b"HOME", b"MID", b"ALPHA", b"TAIL"] {
            m.put(k, b"1");
        }
        m.put(b"HOME", b"2");
        assert_eq!(
            envp(&m),
            [&b"ZED=1"[..], b"HOME=2", b"MID=1", b"ALPHA=1", b"TAIL=1"]
        );
        m.removev(b"HOME");
        assert_eq!(envp(&m), [&b"ZED=1"[..], b"MID=1", b"ALPHA=1", b"TAIL=1"]);
        m.swap_remove(b"ZED");
        assert_eq!(envp(&m), [&b"TAIL=1"[..], b"MID=1", b"ALPHA=1"]);
    }

    // test "Command: os pre exec 1" {
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls (termio's G6 tool is ASan, not Miri)
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: Some({
                    fn r#do(_: &mut Command<'_>) -> Result<(), PostForkError> {
                        Err(PostForkError) // return error.PostForkError;
                    }
                    r#do
                }),
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            // (`code` is already `u32` on Windows.)
            #[allow(clippy::useless_conversion)]
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
        env.put(b"VALUE", b"hello");

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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
            rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
                rt_pre_exec_info: RtPreExecInfo::default(), // .rt_pre_exec_info = undefined,
                rt_post_fork: None,
                rt_post_fork_info: RtPostForkInfo::default(), // .rt_post_fork_info = undefined,
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

    /// S39-F2 end to end: a command whose exec fails with `EACCES` (the path
    /// names a directory) makes the forked child write the reference's
    /// report, `E` plus the errno's tag, to its stderr.
    #[test]
    #[cfg_attr(miri, ignore)] // real fork/exec/waitpid syscalls
    #[cfg(not(windows))]
    fn exec_failure_in_child_reports_the_errno_tag() {
        use std::io::{Read as _, Seek as _};
        let _serial = fork_serial();

        let mut td = TempDir::init().unwrap();
        let stdout = createTestStdout(td.dir()).unwrap();
        let mut stderr = createTestStderr(td.dir()).unwrap();

        let mut cmd: Command = Command {
            // A directory: execve fails with EACCES.
            path: c"/",
            args: &[c"/"],
            env: None,
            cwd: None,
            stdin: None,
            stdout: Some(&stdout),
            stderr: Some(&stderr),
            os_pre_exec: None,
            rt_pre_exec: None,
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork: None,
            rt_post_fork_info: RtPostForkInfo::default(),
            data: None,
            pid: None,
        };

        cmd.testingStart().unwrap();
        let exit = cmd.wait(true).unwrap();
        assert!(matches!(exit, Exit::Exited(1)));

        let mut contents = String::new();
        stderr.rewind().unwrap();
        stderr.read_to_string(&mut contents).unwrap();
        assert_eq!(
            contents,
            "exec syscall failed with unexpected error: EACCES\n"
        );

        drop(stderr);
        drop(stdout);
        td.deinit();
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
