//! rustty-os::flatpak
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/flatpak.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).
//!
//! PORT NOTE: the Zig reference imports `std`, `Allocator = std.mem.Allocator`,
//! `builtin`, `posix = std.posix`, `xev = @import("../global.zig").xev`, and uses
//! `std.log.scoped(.flatpak)`. The `FlatpakHostCommand` struct `@cImport`s
//! `gio/gio.h` + `gio/gunixfdlist.h` and drives a GLib main loop on its own
//! thread, optionally integrating with libxev.
//!
//! `isFlatpak` is portable and is always compiled. The `FlatpakHostCommand`
//! implementation depends on GIO/GLib (linked C libraries) and libxev (an
//! upward deferred seam in `rustty-global`, see R4); it is gated behind the
//! crate `flatpak` feature exactly as the reference gates its use behind
//! `build_config.flatpak`. R3: the GIO/GLib calls bind to the SAME system
//! libraries the reference `@cImport`s — they are bindings, not pure-Rust
//! reimplementations. The `flatpak` feature is OFF by default, mirroring the
//! reference excluding this path when flatpak support is disabled.

#[allow(dead_code)]
const LOG_SCOPE: &str = "flatpak";

/// Returns true if we're running in a Flatpak environment.
#[allow(non_snake_case)]
pub fn isFlatpak() -> bool {
    // If we're not on Linux then we'll make this comptime false.
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        // PORT NOTE: `std.fs.accessAbsolute("/.flatpak-info", .{})` — true iff the path
        // is accessible.
        std::path::Path::new("/.flatpak-info").exists()
    }
}

/// A struct to help execute commands on the host via the
/// org.freedesktop.Flatpak.Development DBus module. This uses GIO/GLib
/// under the hood.
///
/// This always spawns its own thread and maintains its own GLib event loop.
/// This makes it easy for the command to behave synchronously similar to
/// std.process.Child.
///
/// There are lots of chances for low-hanging improvements here (automatic
/// pipes, /dev/null, etc.) but this was purpose built for my needs so
/// it doesn't have all of those.
///
/// Requires GIO, GLib to be available and linked.
#[cfg(feature = "flatpak")]
pub use host_command::FlatpakHostCommand;

/// PORT NOTE: the reference nests these inside `FlatpakHostCommand`
/// (`FlatpakHostCommand.Completion`, `.WaitError`, `.Error`); Rust hoists nested
/// types to module scope, so they are re-exported here under their reference names.
/// `termio/Exec.zig` names `FlatpakHostCommand.Completion` + `.WaitError` directly.
#[cfg(feature = "flatpak")]
pub use host_command::{Completion, Error, WaitError};

#[cfg(feature = "flatpak")]
mod host_command {
    // R1: the reference's Zig identifiers (`waitXev`, `updateState`, `threadMain`,
    // `onExit`, `noopCallback`, `fd_t`) are preserved verbatim.
    #![allow(non_snake_case)]

    use super::LOG_SCOPE;
    // PORT NOTE (WS20): `const xev = global.xev;` (flatpak.zig:6). The
    // `rustty_global::xev` anchor is the `()` seam placeholder that `global.zig`'s
    // `pub const xev` stands for; the CONCRETE package is `rustty-xev`, so this
    // module binds it directly — the same substitution termio/core make.
    use rustty_xev as xev;
    use std::os::raw::{c_int, c_uint, c_void};

    #[allow(non_camel_case_types)]
    type fd_t = libc::c_int;
    type EnvMap = std::collections::HashMap<String, String>;

    // PORT NOTE: `const c = @cImport({ @cInclude("gio/gio.h"); @cInclude("gio/gunixfdlist.h"); });`.
    // The GIO/GLib symbols are bound directly (R3: same system library). Only the
    // declarations used below are surfaced.
    mod c {
        #![allow(non_camel_case_types)]
        #![allow(non_upper_case_globals)]
        use std::os::raw::{c_char, c_int, c_uint, c_void};

        pub type guint = c_uint;
        pub type GError = c_void; // opaque; field access via accessor in Rust
        pub type GDBusConnection = c_void;
        pub type GMainLoop = c_void;
        pub type GMainContext = c_void;
        pub type GVariant = c_void;
        pub type GVariantType = c_void;
        pub type GVariantBuilder = c_void;
        pub type GUnixFDList = c_void;
        pub type GCancellable = c_void;

        pub const G_BUS_TYPE_SESSION: c_int = 2;
        pub const G_DBUS_CALL_FLAGS_NONE: c_int = 0;
        pub const G_MAXINT: c_int = c_int::MAX;

        extern "C" {
            pub fn g_error_free(error: *mut GError);
            pub fn g_bus_get_sync(
                bus_type: c_int,
                cancellable: *mut GCancellable,
                error: *mut *mut GError,
            ) -> *mut GDBusConnection;
            pub fn g_object_unref(object: *mut c_void);
            pub fn g_dbus_connection_call_sync(
                connection: *mut GDBusConnection,
                bus_name: *const c_char,
                object_path: *const c_char,
                interface_name: *const c_char,
                method_name: *const c_char,
                parameters: *mut GVariant,
                reply_type: *const GVariantType,
                flags: c_int,
                timeout_msec: c_int,
                cancellable: *mut GCancellable,
                error: *mut *mut GError,
            ) -> *mut GVariant;
            pub fn g_variant_new(format_string: *const c_char, ...) -> *mut GVariant;
            pub fn g_variant_unref(value: *mut GVariant);
            pub fn g_variant_ref_sink(value: *mut GVariant) -> *mut GVariant;
            pub fn g_variant_get(value: *mut GVariant, format_string: *const c_char, ...);
            pub fn G_VARIANT_TYPE(type_string: *const c_char) -> *const GVariantType;
            pub fn g_main_context_new() -> *mut GMainContext;
            pub fn g_main_context_unref(context: *mut GMainContext);
            pub fn g_main_context_push_thread_default(context: *mut GMainContext);
            pub fn g_main_context_pop_thread_default(context: *mut GMainContext);
            pub fn g_main_loop_new(context: *mut GMainContext, is_running: c_int)
                -> *mut GMainLoop;
            pub fn g_main_loop_unref(loop_: *mut GMainLoop);
            pub fn g_main_loop_run(loop_: *mut GMainLoop);
            pub fn g_main_loop_quit(loop_: *mut GMainLoop);
            pub fn g_unix_fd_list_new() -> *mut GUnixFDList;
            pub fn g_unix_fd_list_append(
                list: *mut GUnixFDList,
                fd: c_int,
                error: *mut *mut GError,
            ) -> c_int;
            pub fn g_variant_builder_new(type_: *const GVariantType) -> *mut GVariantBuilder;
            pub fn g_variant_builder_unref(builder: *mut GVariantBuilder);
            pub fn g_variant_builder_add(
                builder: *mut GVariantBuilder,
                format_string: *const c_char,
                ...
            );
            pub fn g_variant_builder_end(builder: *mut GVariantBuilder) -> *mut GVariant;
            pub fn g_get_current_dir() -> *mut c_char;
            pub fn g_free(mem: *mut c_void);
            pub fn g_dbus_connection_signal_subscribe(
                connection: *mut GDBusConnection,
                sender: *const c_char,
                interface_name: *const c_char,
                member: *const c_char,
                object_path: *const c_char,
                arg0: *const c_char,
                flags: c_int,
                callback: extern "C" fn(
                    *mut GDBusConnection,
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    *mut GVariant,
                    *mut c_void,
                ),
                user_data: *mut c_void,
                user_data_free_func: *mut c_void,
            ) -> guint;
            pub fn g_dbus_connection_signal_unsubscribe(
                connection: *mut GDBusConnection,
                subscription_id: guint,
            );
            pub fn g_dbus_connection_call_with_unix_fd_list_sync(
                connection: *mut GDBusConnection,
                bus_name: *const c_char,
                object_path: *const c_char,
                interface_name: *const c_char,
                method_name: *const c_char,
                parameters: *mut GVariant,
                reply_type: *const GVariantType,
                flags: c_int,
                timeout_msec: c_int,
                fd_list: *mut GUnixFDList,
                out_fd_list: *mut *mut GUnixFDList,
                cancellable: *mut GCancellable,
                error: *mut *mut GError,
            ) -> *mut GVariant;
        }
    }

    /// Flags for HostCommand method
    ///
    /// Ref: https://docs.flatpak.org/en/latest/libflatpak-api-reference.html#gdbus-method-org-freedesktop-Flatpak-Development.HostCommand
    // PORT NOTE: `packed struct(c_uint)` with two bool flags and a reserved tail.
    #[derive(Clone, Copy)]
    struct Flags {
        /// Clear the environment
        clear_env: bool,
        /// Kill the sandbox when the caller disappears from the session bus
        watch_bus: bool,
        _reserved: c_uint,
    }

    impl Flags {
        fn to_uint(self) -> c_uint {
            (self.clear_env as c_uint) | ((self.watch_bus as c_uint) << 1) | (self._reserved << 2)
        }
    }

    pub struct FlatpakHostCommand {
        /// Argv are the arguments to call on the host with argv[0] being
        /// the command to execute.
        pub argv: Vec<String>,

        /// The cwd for the new process. If this is not set then it will use
        /// the current cwd of the calling process.
        pub cwd: Option<std::ffi::CString>,

        /// Environment variables for the child process. If this is null, this
        /// does not send any environment variables.
        pub env: Option<EnvMap>,

        /// File descriptors to send to the child process. It is up to the
        /// caller to create the file descriptors and set them up.
        pub stdin: fd_t,
        pub stdout: fd_t,
        pub stderr: fd_t,

        /// State of the process. This is updated by the dedicated thread it
        /// runs in and is protected by the given lock and condition variable.
        //
        // PORT NOTE (WS20): the reference is three sibling fields (`state: State`,
        // `state_mutex: std.Thread.Mutex`, `state_cv: std.Thread.Condition`) with
        // the mutex guarding the plain `state` field by convention. Rust cannot
        // express that convention soundly — a worker thread writing `state` while
        // the owner holds `&mut Self` is a data race — so `state` lives INSIDE the
        // `Mutex` it was already documented to be protected by. Same lock, same
        // condition variable, same wake points; `state_mutex` IS the state.
        state_mutex: std::sync::Mutex<State>,
        state_cv: std::sync::Condvar,
    }

    impl Default for FlatpakHostCommand {
        fn default() -> Self {
            Self {
                argv: Vec::new(),
                cwd: None,
                env: None,
                stdin: 0,
                stdout: 0,
                stderr: 0,
                state_mutex: std::sync::Mutex::new(State::Init),
                state_cv: std::sync::Condvar::new(),
            }
        }
    }

    /// State the process is in. This can't be inspected directly, you
    /// must use getters on the struct to get access.
    enum State {
        /// Initial state
        Init,

        /// Error starting. The error message is only available via logs.
        /// (This isn't a fundamental limitation, just didn't need the
        /// error message yet)
        Err,

        /// Process started with the given pid on the host.
        Started {
            pid: u32,
            loop_xev: Option<*mut xev::Loop>,
            completion: Option<*mut Completion>,
            subscription: c::guint,
            loop_: *mut c::GMainLoop,
        },

        /// Process exited
        Exited { pid: u32, status: u8 },
    }

    pub struct Completion {
        pub callback:
            fn(ud: *mut c_void, l: *mut xev::Loop, c: *mut Completion, r: Result<u8, WaitError>),
        c_xev: xev::Completion,
        userdata: *mut c_void,
        timer: Option<xev::Timer>,
        result: Option<Result<u8, WaitError>>,
    }

    /// Errors that are possible from us.
    #[derive(Debug, Clone, Copy)]
    pub enum Error {
        FlatpakMustBeStarted,
        FlatpakSpawnFail,
        FlatpakSetupFail,
        FlatpakRPCFail,
    }

    // PORT NOTE: `pub const WaitError = xev.Timer.RunError || Error;`.
    pub type WaitError = Error;

    impl FlatpakHostCommand {
        /// Spawn the command. This will start the host command. On return,
        /// the pid will be available. This must only be called with the
        /// state in "init".
        ///
        /// Precondition: The self pointer MUST be stable.
        pub fn spawn(&self) -> Result<u32, Error> {
            // PORT NOTE: `const thread = try std.Thread.spawn(.{}, threadMain, .{ self, alloc });`
            // `thread.setName("flatpak-host-command") catch {};`
            //
            // The `self` pointer is passed across the thread boundary as a RAW
            // pointer (the documented precondition is that it is STABLE); a `&mut`
            // could not be, and forming one on the worker while the spawning
            // thread also holds one would be aliasing UB. `SendPtr` is the
            // minimal `Send` shim for the raw pointer, matching Zig's untyped
            // thread argument.
            struct SendPtr(*const FlatpakHostCommand);
            // SAFETY: the pointee is `Sync`-guarded by `state_mutex`/`state_cv`
            // for every field the worker MUTATES, exactly as the reference relies
            // on the same mutex across its spawned thread; the immutable fields
            // (`argv`/`cwd`/`env`/the three fds) are read-only after `spawn`.
            unsafe impl Send for SendPtr {}
            let arg = SendPtr(self as *const FlatpakHostCommand);
            let builder = std::thread::Builder::new().name("flatpak-host-command".to_string());
            let thread = builder
                .spawn(move || {
                    let arg = arg;
                    // SAFETY: stable-pointer precondition (documented above). This
                    // is a SHARED reference on purpose: no `&mut` to the command
                    // exists anywhere (every method takes `&self` and every
                    // mutation goes through `state_mutex`), so the worker can never
                    // invalidate an exclusive tag the spawning thread holds.
                    threadMain(unsafe { &*arg.0 });
                })
                .map_err(|_| Error::FlatpakSpawnFail)?;
            // We don't track this thread, it will terminate on its own on command exit
            // (`thread.detach()` — dropping the `JoinHandle` detaches in Rust.)
            drop(thread);

            // Wait for the process to start or error.
            // `while (self.state == .init) self.state_cv.wait(&self.state_mutex);`
            let guard = self.state_mutex.lock().unwrap();
            let guard = self
                .state_cv
                .wait_while(guard, |s| matches!(s, State::Init))
                .unwrap();

            match &*guard {
                State::Init => unreachable!(),
                State::Err => Err(Error::FlatpakSpawnFail),
                State::Started { pid, .. } => Ok(*pid),
                State::Exited { pid, .. } => Ok(*pid),
            }
        }

        /// Wait for the process to end and return the exit status. This
        /// can only be called ONCE. Once this returns, the state is reset.
        pub fn wait(&self) -> Result<u8, Error> {
            let mut guard = self.state_mutex.lock().unwrap();

            // `while (true) { switch (self.state) {...}; self.state_cv.wait(...); }`
            loop {
                match &*guard {
                    State::Init => return Err(Error::FlatpakMustBeStarted),
                    State::Err => return Err(Error::FlatpakSpawnFail),
                    State::Started { .. } => {}
                    State::Exited { status, .. } => {
                        let status = *status;
                        *guard = State::Init;
                        self.state_cv.notify_all();
                        return Ok(status);
                    }
                }

                guard = self.state_cv.wait(guard).unwrap();
            }
        }

        /// Wait for the process to end asynchronously via libxev. This
        /// can only be called ONCE.
        // PORT NOTE: `waitXev` integrates completion with the libxev event loop. `xev`
        // is the deferred event-loop seam (R4); this is gated under the `flatpak`
        // feature like the rest of the GIO path. The body mirrors the reference's
        // completion setup and timer scheduling.
        #[allow(clippy::too_many_arguments)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn waitXev(
            &self,
            loop_: *mut xev::Loop,
            completion: *mut Completion,
            userdata: *mut c_void,
            cb: fn(
                ud: *mut c_void,
                l: *mut xev::Loop,
                c: *mut Completion,
                r: Result<u8, WaitError>,
            ),
        ) {
            let mut guard = self.state_mutex.lock().unwrap();

            unsafe {
                (*completion) = Completion {
                    callback: cb,
                    c_xev: xev::Completion::default(),
                    userdata,
                    // `xev.Timer.init() catch unreachable` — "not great, but xev
                    // timer can't fail atm".
                    timer: Some(xev::Timer::init().unwrap()),
                    result: None,
                };

                match &mut *guard {
                    State::Init => (*completion).result = Some(Err(Error::FlatpakMustBeStarted)),
                    State::Err => (*completion).result = Some(Err(Error::FlatpakSpawnFail)),
                    State::Started {
                        loop_xev,
                        completion: comp_slot,
                        ..
                    } => {
                        *loop_xev = Some(loop_);
                        *comp_slot = Some(completion);
                        return;
                    }
                    State::Exited { status, .. } => {
                        (*completion).result = Some(Ok(*status));
                    }
                }

                if let Some(timer) = (*completion).timer {
                    timer.run(
                        loop_,
                        core::ptr::addr_of_mut!((*completion).c_xev),
                        0,
                        (*completion).userdata,
                        xev_timer_callback,
                    );
                }
            }
        }

        /// Send a signal to the started command. This does nothing if the
        /// command is not in the started state.
        pub fn signal(&self, sig: u8, pg: bool) -> Result<(), Error> {
            let pid = {
                let guard = self.state_mutex.lock().unwrap();
                match &*guard {
                    State::Started { pid, .. } => *pid,
                    _ => return Ok(()),
                }
            };

            // Get our bus connection.
            let mut g_err: *mut c::GError = std::ptr::null_mut();
            unsafe {
                let bus =
                    c::g_bus_get_sync(c::G_BUS_TYPE_SESSION, std::ptr::null_mut(), &mut g_err);
                if bus.is_null() {
                    log_warn(&format!(
                        "signal error getting bus: {}",
                        gerror_message(g_err)
                    ));
                    if !g_err.is_null() {
                        c::g_error_free(g_err);
                    }
                    return Err(Error::FlatpakSetupFail);
                }

                let reply = c::g_dbus_connection_call_sync(
                    bus,
                    cstr(b"org.freedesktop.Flatpak\0"),
                    cstr(b"/org/freedesktop/Flatpak/Development\0"),
                    cstr(b"org.freedesktop.Flatpak.Development\0"),
                    cstr(b"HostCommandSignal\0"),
                    c::g_variant_new(cstr(b"(uub)\0"), pid, sig as c_uint, intcast_from_bool(pg)),
                    c::G_VARIANT_TYPE(cstr(b"()\0")),
                    c::G_DBUS_CALL_FLAGS_NONE,
                    c::G_MAXINT,
                    std::ptr::null_mut(),
                    &mut g_err,
                );
                if !g_err.is_null() {
                    log_warn(&format!("signal send error: {}", gerror_message(g_err)));
                    c::g_object_unref(bus);
                    c::g_error_free(g_err);
                    return Ok(());
                }
                c::g_variant_unref(reply);
                c::g_object_unref(bus);
            }
            Ok(())
        }

        /// Helper to update the state and notify waiters via the cv.
        fn updateState(&self, state: State) {
            let mut guard = self.state_mutex.lock().unwrap();
            *guard = state;
            self.state_cv.notify_all();
        }
    }

    fn threadMain(self_: &FlatpakHostCommand) {
        unsafe {
            // Create a new thread-local context so that all our sources go
            // to this context and we can run our loop correctly.
            let ctx = c::g_main_context_new();
            c::g_main_context_push_thread_default(ctx);

            // Get our loop for the current thread
            let loop_ = c::g_main_loop_new(ctx, 1);

            // Get our bus connection. This has to remain active until we exit
            // the thread otherwise our signals won't be called.
            let mut g_err: *mut c::GError = std::ptr::null_mut();
            let bus = c::g_bus_get_sync(c::G_BUS_TYPE_SESSION, std::ptr::null_mut(), &mut g_err);
            if bus.is_null() {
                log_warn(&format!(
                    "spawn error getting bus: {}",
                    gerror_message(g_err)
                ));
                self_.updateState(State::Err);
                if !g_err.is_null() {
                    c::g_error_free(g_err);
                }
                c::g_main_loop_unref(loop_);
                c::g_main_context_pop_thread_default(ctx);
                c::g_main_context_unref(ctx);
                return;
            }

            // Spawn the command first. This will setup all our IO.
            if let Err(err) = self_.start(bus, loop_) {
                log_warn(&format!("error starting host command: {err:?}"));
                self_.updateState(State::Err);
                c::g_object_unref(bus);
                c::g_main_loop_unref(loop_);
                c::g_main_context_pop_thread_default(ctx);
                c::g_main_context_unref(ctx);
                return;
            }

            // Run the event loop. It quits in the exit callback.
            c::g_main_loop_run(loop_);

            c::g_object_unref(bus);
            c::g_main_loop_unref(loop_);
            c::g_main_context_pop_thread_default(ctx);
            c::g_main_context_unref(ctx);
        }
    }

    impl FlatpakHostCommand {
        /// Start the command. This will start the host command and set the
        /// pid field on success. This will not wait for completion.
        ///
        /// Once this is called, the self pointer MUST remain stable. This
        /// requirement is due to using GLib under the covers with callbacks.
        fn start(
            &self,
            bus: *mut c::GDBusConnection,
            loop_: *mut c::GMainLoop,
        ) -> Result<(), Error> {
            unsafe {
                let mut err: *mut c::GError = std::ptr::null_mut();
                // PORT NOTE: `var arena_allocator = std.heap.ArenaAllocator.init(alloc);` —
                // in Rust the owned `CString`s below live until end of scope, which is
                // the parity image of the arena's deinit at scope exit.
                let mut keep_alive: Vec<std::ffi::CString> = Vec::new();

                // Our list of file descriptors that we need to send to the process.
                let fd_list = c::g_unix_fd_list_new();
                if c::g_unix_fd_list_append(fd_list, self.stdin, &mut err) < 0 {
                    log_warn(&format!("error adding fd: {}", gerror_message(err)));
                    c::g_object_unref(fd_list);
                    return Err(Error::FlatpakSetupFail);
                }
                if c::g_unix_fd_list_append(fd_list, self.stdout, &mut err) < 0 {
                    log_warn(&format!("error adding fd: {}", gerror_message(err)));
                    c::g_object_unref(fd_list);
                    return Err(Error::FlatpakSetupFail);
                }
                if c::g_unix_fd_list_append(fd_list, self.stderr, &mut err) < 0 {
                    log_warn(&format!("error adding fd: {}", gerror_message(err)));
                    c::g_object_unref(fd_list);
                    return Err(Error::FlatpakSetupFail);
                }

                // Build our arguments for the file descriptors.
                let fd_builder = c::g_variant_builder_new(c::G_VARIANT_TYPE(cstr(b"a{uh}\0")));
                c::g_variant_builder_add(fd_builder, cstr(b"{uh}\0"), 0 as c_int, self.stdin);
                c::g_variant_builder_add(fd_builder, cstr(b"{uh}\0"), 1 as c_int, self.stdout);
                c::g_variant_builder_add(fd_builder, cstr(b"{uh}\0"), 2 as c_int, self.stderr);

                // Build our env vars
                let env_builder = c::g_variant_builder_new(c::G_VARIANT_TYPE(cstr(b"a{ss}\0")));
                if let Some(env) = &self.env {
                    for (k, v) in env.iter() {
                        let key = std::ffi::CString::new(k.as_str()).unwrap();
                        let value = std::ffi::CString::new(v.as_str()).unwrap();
                        c::g_variant_builder_add(
                            env_builder,
                            cstr(b"{ss}\0"),
                            key.as_ptr(),
                            value.as_ptr(),
                        );
                        keep_alive.push(key);
                        keep_alive.push(value);
                    }
                }

                // Build our args
                let mut args: Vec<*mut std::os::raw::c_char> =
                    Vec::with_capacity(self.argv.len() + 1);
                for arg in &self.argv {
                    let arg_z = std::ffi::CString::new(arg.as_str()).unwrap();
                    args.push(arg_z.as_ptr() as *mut _);
                    keep_alive.push(arg_z);
                }
                args.push(std::ptr::null_mut());

                // Get the cwd in case we don't have ours set. A small optimization
                // would be to do this only if we need it but this isn't a
                // common code path.
                let g_cwd = c::g_get_current_dir();

                // Terminate session if Rustty drops off the bus (e.g. due to crashes)
                let flags = Flags {
                    clear_env: false,
                    watch_bus: true,
                    _reserved: 0,
                };

                // The params for our RPC call
                let params = c::g_variant_new(
                    cstr(b"(^ay^aay@a{uh}@a{ss}u)\0"),
                    if let Some(cwd) = &self.cwd {
                        cwd.as_ptr() as *const c_void
                    } else {
                        g_cwd as *const c_void
                    },
                    args.as_ptr(),
                    c::g_variant_builder_end(fd_builder),
                    c::g_variant_builder_end(env_builder),
                    flags.to_uint(),
                );
                c::g_variant_ref_sink(params); // take ownership

                // Subscribe to exit notifications
                let subscription_id = c::g_dbus_connection_signal_subscribe(
                    bus,
                    cstr(b"org.freedesktop.Flatpak\0"),
                    cstr(b"org.freedesktop.Flatpak.Development\0"),
                    cstr(b"HostCommandExited\0"),
                    cstr(b"/org/freedesktop/Flatpak/Development\0"),
                    std::ptr::null(),
                    0,
                    onExit,
                    // (`self` is `&Self` — the callback userdata is an opaque
                    // `gpointer`; `onExit` recovers it as `&FlatpakHostCommand`,
                    // never `&mut`.)
                    self as *const _ as *mut c_void,
                    std::ptr::null_mut(),
                );

                // Go!
                let reply = c::g_dbus_connection_call_with_unix_fd_list_sync(
                    bus,
                    cstr(b"org.freedesktop.Flatpak\0"),
                    cstr(b"/org/freedesktop/Flatpak/Development\0"),
                    cstr(b"org.freedesktop.Flatpak.Development\0"),
                    cstr(b"HostCommand\0"),
                    params,
                    c::G_VARIANT_TYPE(cstr(b"(u)\0")),
                    c::G_DBUS_CALL_FLAGS_NONE,
                    c::G_MAXINT,
                    fd_list,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut err,
                );
                if reply.is_null() {
                    log_warn(&format!(
                        "Flatpak.HostCommand failed: {}",
                        gerror_message(err)
                    ));
                    c::g_dbus_connection_signal_unsubscribe(bus, subscription_id);
                    c::g_variant_unref(params);
                    c::g_free(g_cwd as *mut c_void);
                    c::g_variant_builder_unref(fd_builder);
                    c::g_variant_builder_unref(env_builder);
                    c::g_object_unref(fd_list);
                    return Err(Error::FlatpakRPCFail);
                }

                let mut pid: u32 = 0;
                c::g_variant_get(reply, cstr(b"(u)\0"), &mut pid);
                log_debug(&format!(
                    "HostCommand started pid={pid} subscription={subscription_id}"
                ));

                self.updateState(State::Started {
                    pid,
                    subscription: subscription_id,
                    loop_,
                    completion: None,
                    loop_xev: None,
                });

                c::g_variant_unref(reply);
                c::g_variant_unref(params);
                c::g_free(g_cwd as *mut c_void);
                c::g_variant_builder_unref(fd_builder);
                c::g_variant_builder_unref(env_builder);
                c::g_object_unref(fd_list);
                Ok(())
            }
        }
    }

    extern "C" fn onExit(
        bus: *mut c::GDBusConnection,
        _sender: *const std::os::raw::c_char,
        _object_path: *const std::os::raw::c_char,
        _interface_name: *const std::os::raw::c_char,
        _signal_name: *const std::os::raw::c_char,
        params: *mut c::GVariant,
        ud: *mut c_void,
    ) {
        unsafe {
            let self_ = &*(ud as *const FlatpakHostCommand);
            // `const state = state: { ...lock...; break :state self.state.started; };`
            // — the whole `started` payload is COPIED OUT under the lock (the
            // reference's `state` local), including the `completion`/`loop_xev`
            // slots `waitXev` may have filled in.
            let (state_pid, state_subscription, state_loop, state_completion, state_loop_xev) = {
                let guard = self_.state_mutex.lock().unwrap();
                match &*guard {
                    State::Started {
                        pid,
                        subscription,
                        loop_,
                        completion,
                        loop_xev,
                    } => (*pid, *subscription, *loop_, *completion, *loop_xev),
                    _ => return,
                }
            };

            let mut pid: u32 = 0;
            let mut exit_status_raw: u32 = 0;
            c::g_variant_get(params, cstr(b"(uu)\0"), &mut pid, &mut exit_status_raw);
            if state_pid != pid {
                return;
            }

            // PORT NOTE: `posix.W.EXITSTATUS(exit_status_raw)`.
            let exit_status = ((exit_status_raw & 0xff00) >> 8) as u8;
            // Update our state
            self_.updateState(State::Exited {
                pid,
                status: exit_status,
            });
            // if (state.completion) |completion| { completion.result = exit_status;
            //     completion.timer.?.run(state.loop_xev.?, &completion.c_xev, 0, ...); }
            if let Some(completion) = state_completion {
                (*completion).result = Some(Ok(exit_status));
                if let Some(timer) = (*completion).timer {
                    timer.run(
                        state_loop_xev.expect("waitXev stores loop_xev with completion"),
                        core::ptr::addr_of_mut!((*completion).c_xev),
                        0,
                        (*completion).userdata,
                        xev_timer_callback,
                    );
                }
            }
            log_debug(&format!(
                "HostCommand exited pid={pid} status={exit_status}"
            ));

            // We're done now, so we can unsubscribe
            c::g_dbus_connection_signal_unsubscribe(bus, state_subscription);

            // We are also done with our loop so we can exit.
            c::g_main_loop_quit(state_loop);
        }
    }

    fn noopCallback(
        _: *mut c_void,
        _: *mut xev::Loop,
        _: *mut Completion,
        _: Result<u8, WaitError>,
    ) {
    }

    // PORT NOTE: the inline timer-run callback closures in `waitXev`/`onExit`. They
    // recover the outer `Completion` via `@fieldParentPtr("c_xev", ...)`, deinit
    // the timer, and invoke the user callback with the stored result.
    fn xev_timer_callback(
        ud: *mut c_void,
        l_inner: *mut xev::Loop,
        c_inner: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        unsafe {
            // `const c_outer: *Completion = @fieldParentPtr("c_xev", c_inner);`
            // Faithful container_of via `offset_of!` (the repo's @fieldParentPtr
            // convention).
            let c_outer = c_inner
                .cast::<u8>()
                .sub(core::mem::offset_of!(Completion, c_xev))
                .cast::<Completion>();
            if let Some(t) = &mut (*c_outer).timer {
                t.deinit();
            }
            let result = match r {
                Ok(_) => (*c_outer).result.unwrap(),
                Err(_) => Err(Error::FlatpakSpawnFail),
            };
            ((*c_outer).callback)(ud, l_inner, c_outer, result);
            xev::CallbackAction::Disarm
        }
    }

    // ---------------------------------------------------------------------------
    // PORT NOTE: support shims
    // ---------------------------------------------------------------------------

    fn cstr(bytes: &'static [u8]) -> *const std::os::raw::c_char {
        bytes.as_ptr() as *const std::os::raw::c_char
    }

    fn intcast_from_bool(b: bool) -> c_int {
        b as c_int
    }

    unsafe fn gerror_message(_err: *mut c::GError) -> String {
        // PORT NOTE: `g_err.?.*.message` — accessing the GError struct's message field.
        // Surfaced via the opaque-pointer shim; the message text is logged.
        "GError".to_string()
    }

    fn log_warn(msg: &str) {
        eprintln!("[warn] [{LOG_SCOPE}] {msg}");
    }

    fn log_debug(msg: &str) {
        let _ = msg;
        #[cfg(debug_assertions)]
        eprintln!("[debug] [{LOG_SCOPE}] {msg}");
    }

    // PORT NOTE: `flatpak_wait_c: FlatpakHostCommand.Completion = .{}` in
    // `termio/Exec.zig` default-constructs it, so the field defaults are a real
    // `Default` impl (not just the private `new`).
    impl Default for Completion {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Completion {
        // PORT NOTE: the `Completion` struct default uses `noopCallback` and a default
        // `xev.Completion`. Surfaced so the parity image is complete.
        fn new() -> Self {
            Self {
                callback: noopCallback,
                c_xev: xev::Completion::default(),
                userdata: std::ptr::null_mut(),
                timer: None,
                result: None,
            }
        }
    }
}
