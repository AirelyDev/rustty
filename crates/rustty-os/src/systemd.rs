//! rustty-os::systemd
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/systemd.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, and uses
// `std.log.scoped(.systemd)`. The OS switch is `builtin.os.tag`; here it is
// `cfg!(target_os = "linux")`. The Linux paths use raw `libc` syscalls
// (R3: a binding to the SAME system library `std.os.linux` targets) for the
// AF_UNIX socket notify protocol; environment lookups use `std::env::var_os`
// (the reference's `std.posix.getenv`).

#[allow(dead_code)]
const LOG_SCOPE: &str = "systemd";

/// Returns true if the program was launched as a systemd service.
///
/// On Linux, this returns true if the program was launched as a systemd
/// service. It will return false if Rustty was launched any other way.
///
/// For other platforms and app runtimes, this returns false.
#[allow(non_snake_case)]
pub fn launchedBySystemd() -> bool {
    #[cfg(target_os = "linux")]
    {
        // On Linux, systemd sets the `INVOCATION_ID` (v232+) and the
        // `JOURNAL_STREAM` (v231+) environment variables. If these
        // environment variables are not present we were not launched by
        // systemd.
        if std::env::var_os("INVOCATION_ID").is_none() {
            return false;
        }
        if std::env::var_os("JOURNAL_STREAM").is_none() {
            return false;
        }

        // If `INVOCATION_ID` and `JOURNAL_STREAM` are present, check to make sure
        // that our parent process is actually `systemd`, not some other terminal
        // emulator that doesn't clean up those environment variables.
        let ppid = unsafe { libc::getppid() };
        if ppid == 1 {
            return true;
        }

        // If the parent PID is not 1 we need to check to see if we were launched by
        // a user systemd daemon. Do that by checking the `/proc/<ppid>/comm`
        // to see if it ends with `systemd`.
        let comm_path = format!("/proc/{ppid}/comm");
        let comm_data_buf = match std::fs::read(&comm_path) {
            Ok(d) => d,
            Err(_) => {
                log_err(&format!("unable to open '{comm_path}' for reading"));
                return false;
            }
        };

        // The maximum length of the command name is defined by
        // `TASK_COMM_LEN` in the Linux kernel. This is usually 16
        // bytes at the time of writing (Jun 2025) so its set to that.
        // Also, since we only care to compare to "systemd", anything
        // longer can be assumed to not be systemd.
        const TASK_COMM_LEN: usize = 16;
        let comm_size = std::cmp::min(comm_data_buf.len(), TASK_COMM_LEN);
        let comm_data = &comm_data_buf[0..comm_size];

        trim_right(comm_data, b"\n") == b"systemd"
    }

    // No other system supports systemd so always return false.
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// systemd notifications. Used by Rustty to inform systemd of the state of the
/// process. Currently only used to notify systemd that we are ready and that
/// configuration reloading has started.
///
/// See: https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html
///
/// These functions were re-implemented in Zig instead of using the `libsystemd`
/// library to avoid the complexity of another external dependency, as well as
/// to take advantage of Zig features like `comptime` to ensure minimal impact
/// on non-Linux systems (like FreeBSD) that will never support `systemd`.
///
/// Linux systems that do not use `systemd` should not be impacted as they
/// should never start Rustty with the `NOTIFY_SOCKET` environment variable set
/// and these functions essentially become a no-op.
///
/// See `systemd`'s [Interface Portability and Stability Promise](https://systemd.io/PORTABILITY_AND_STABILITY/)
/// for assurances that the interfaces used here will be supported and stable for
/// the long term.
pub mod notify {
    #[allow(unused_imports)]
    use super::{log_err, log_warn};

    /// Send the given message to the UNIX socket specified in the NOTIFY_SOCKET
    /// environment variable. If there NOTIFY_SOCKET environment variable does
    /// not exist then no message is sent.
    #[allow(unused_variables)]
    #[allow(dead_code)]
    fn send(message: &[u8]) {
        // systemd is Linux-only so this is a no-op anywhere else
        // PORT NOTE: `if (comptime builtin.os.tag != .linux) return;` — on non-Linux
        // the body below is `cfg`-excluded, which is the parity image of the
        // early `return` (the function does nothing).
        #[cfg(target_os = "linux")]
        {
            // Get the socket address that should receive notifications.
            let socket_path = match std::env::var_os("NOTIFY_SOCKET") {
                Some(v) => v,
                None => return,
            };
            let socket_path = socket_path.as_encoded_bytes();

            // If the socket address is an empty string return.
            if socket_path.is_empty() {
                return;
            }

            // The socket address must be a path or an abstract socket.
            if socket_path[0] != b'/' && socket_path[0] != b'@' {
                log_warn(
                    "only AF_UNIX sockets with path or abstract namespace addresses are supported!",
                );
                return;
            }

            let mut socket_address: libc::sockaddr_un = unsafe { std::mem::zeroed() };

            // Error out if the supplied socket path is too long.
            if socket_address.sun_path.len() < socket_path.len() {
                log_warn("NOTIFY_SOCKET path is too long!");
                return;
            }

            socket_address.sun_family = libc::AF_UNIX as libc::sa_family_t;

            // PORT NOTE: `@memcpy(socket_address.path[0..socket_path.len], socket_path);`
            // then `socket_address.path[socket_path.len] = 0;`. `sun_path` is
            // `c_char` (i8); copy the bytes preserving the trailing NUL.
            for (i, &b) in socket_path.iter().enumerate() {
                socket_address.sun_path[i] = b as libc::c_char;
            }
            socket_address.sun_path[socket_path.len()] = 0;

            let socket: libc::c_int = {
                let rc = unsafe {
                    libc::socket(libc::AF_UNIX, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0)
                };
                if rc < 0 {
                    log_warn(&format!(
                        "creating socket failed: {}",
                        std::io::Error::last_os_error()
                    ));
                    return;
                }
                rc
            };

            // PORT NOTE: `defer _ = std.os.linux.close(socket);` — close at scope exit.
            let _guard = SocketGuard(socket);

            // connect
            {
                let path_off = std::mem::offset_of!(libc::sockaddr_un, sun_path);
                let addr_len = (path_off + socket_path.len()) as libc::socklen_t;
                let rc = unsafe {
                    libc::connect(
                        socket,
                        &socket_address as *const libc::sockaddr_un as *const libc::sockaddr,
                        addr_len,
                    )
                };
                if rc != 0 {
                    log_warn(&format!(
                        "unable to connect to notify socket: {}",
                        std::io::Error::last_os_error()
                    ));
                    return;
                }
            }

            // write
            {
                let rc = unsafe {
                    libc::write(
                        socket,
                        message.as_ptr() as *const libc::c_void,
                        message.len(),
                    )
                };
                if rc < 0 {
                    log_warn(&format!(
                        "unable to write to notify socket: {}",
                        std::io::Error::last_os_error()
                    ));
                    return;
                }
                let written = rc as usize;
                if written < message.len() {
                    log_warn(&format!(
                        "short write to notify socket: {} < {}",
                        rc,
                        message.len()
                    ));
                }
            }
        }
    }

    /// Tell systemd that we are ready or that we are finished reloading.
    /// See: https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html#READY=1
    pub fn ready() {
        // PORT NOTE: `if (comptime builtin.os.tag != .linux) return;` — gated below.
        #[cfg(target_os = "linux")]
        send(b"READY=1");
    }

    /// Tell systemd that we have started reloading our configuration.
    /// See: https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html#RELOADING=1
    /// and: https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html#MONOTONIC_USEC=%E2%80%A6
    pub fn reloading() {
        // PORT NOTE: `if (comptime builtin.os.tag != .linux) return;` — gated below.
        #[cfg(target_os = "linux")]
        {
            let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
            let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
            if rc != 0 {
                log_err(&format!(
                    "unable to get MONOTONIC clock: {}",
                    std::io::Error::last_os_error()
                ));
                return;
            }

            // PORT NOTE: `ts.sec * std.time.us_per_s + @divFloor(ts.nsec, std.time.ns_per_us)`.
            const US_PER_S: i64 = 1_000_000;
            const NS_PER_US: i64 = 1_000;
            let now = ts.tv_sec as i64 * US_PER_S + (ts.tv_nsec as i64).div_euclid(NS_PER_US);

            let message = format!("RELOADING=1\nMONOTONIC_USEC={now}");
            send(message.as_bytes());
        }
    }

    #[cfg(target_os = "linux")]
    struct SocketGuard(libc::c_int);
    #[cfg(target_os = "linux")]
    impl Drop for SocketGuard {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.0);
            }
        }
    }
}

// PORT NOTE: `std.mem.trimRight(u8, slice, "\n")`.
#[cfg(target_os = "linux")]
fn trim_right<'a>(slice: &'a [u8], strip: &[u8]) -> &'a [u8] {
    let mut end = slice.len();
    while end > 0 && strip.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[0..end]
}

#[allow(dead_code)]
fn log_err(msg: &str) {
    eprintln!("[err] [{LOG_SCOPE}] {msg}");
}

#[allow(dead_code)]
fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}
