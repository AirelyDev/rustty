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
        match comm_is_systemd(comm_path.as_bytes()) {
            Ok(v) => v,
            Err(CommError::Open) => {
                log_err(&format!("unable to open '{comm_path}' for reading"));
                false
            }
            Err(CommError::Read) => {
                log_err(&format!("problems reading from '{comm_path}'"));
                false
            }
        }
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

            // The socket address must be a path or an abstract socket, and fit
            // `sun_path` (see `notify_address`, UD202/UD203).
            let (socket_address, addr_len) = match super::notify_address(socket_path) {
                Ok(v) => v,
                Err(super::NotifyAddressError::NotPathOrAbstract) => {
                    log_warn(
                        "only AF_UNIX sockets with path or abstract namespace addresses are supported!",
                    );
                    return;
                }
                Err(super::NotifyAddressError::TooLong) => {
                    log_warn("NOTIFY_SOCKET path is too long!");
                    return;
                }
            };

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
            // PORT NOTE: no conversion on either field, exactly like the
            // reference expression above — `timespec.tv_sec`/`tv_nsec` are
            // already `i64` on the 64-bit targets this builds for, and both a
            // cast and a `From` are rejected here as no-ops.
            let now = ts.tv_sec * US_PER_S + ts.tv_nsec.div_euclid(NS_PER_US);

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

/// Which step of the `/proc/<ppid>/comm` check failed.
#[cfg(any(target_os = "linux", all(test, unix)))]
#[derive(Debug, PartialEq, Eq)]
enum CommError {
    /// `openFileAbsolute` failed: "unable to open '{s}' for reading".
    Open,
    /// `readAll` failed: "problems reading from '{s}'".
    Read,
}

/// The `/proc/<ppid>/comm` half of `launchedBySystemd`: open the file, read at
/// most `TASK_COMM_LEN` bytes, and compare them, less trailing newlines, to
/// `systemd`.
//
// PORT NOTE (S112-F8): the reference has two failure arms with two messages
// (open, then read) and reads into a 16-byte buffer. The port read the whole
// file with `std::fs::read` and logged every failure as the open message.
#[cfg(any(target_os = "linux", all(test, unix)))]
fn comm_is_systemd(comm_path: &[u8]) -> Result<bool, CommError> {
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;

    // const comm_file = std.Io.Dir.openFileAbsolute(global.io(), comm_path, .{ .mode = .read_only }) catch { ... };
    let mut comm_file =
        std::fs::File::open(std::ffi::OsStr::from_bytes(comm_path)).map_err(|_| CommError::Open)?;

    // The maximum length of the command name is defined by
    // `TASK_COMM_LEN` in the Linux kernel. This is usually 16
    // bytes at the time of writing (Jun 2025) so its set to that.
    // Also, since we only care to compare to "systemd", anything
    // longer can be assumed to not be systemd.
    const TASK_COMM_LEN: usize = 16;
    let mut comm_data_buf = [0u8; TASK_COMM_LEN];
    // const comm_size = comm_file.readAll(&comm_data_buf) catch { ... };
    // (`readAll`: read until the buffer is full or the file ends.)
    let mut comm_size = 0;
    while comm_size < TASK_COMM_LEN {
        match comm_file.read(&mut comm_data_buf[comm_size..]) {
            Ok(0) => break,
            Ok(n) => comm_size += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(CommError::Read),
        }
    }
    let comm_data = &comm_data_buf[0..comm_size];

    Ok(trim_right(comm_data, b"\n") == b"systemd")
}

/// Why a `NOTIFY_SOCKET` value cannot be used; each is the reference's warning.
#[cfg(any(target_os = "linux", all(test, unix)))]
#[derive(Debug, PartialEq, Eq)]
enum NotifyAddressError {
    /// "only AF_UNIX sockets with path or abstract namespace addresses are supported!"
    NotPathOrAbstract,
    /// "NOTIFY_SOCKET path is too long!"
    TooLong,
}

/// The `sockaddr_un` and address length `notify.send` connects to, from a
/// non-empty `NOTIFY_SOCKET` value.
//
// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD202. The reference
// rejects a path only when it is LONGER than `sun_path` (`path.len <
// socket_path.len`) and then writes the terminating NUL at
// `path[socket_path.len]`, so a value exactly `sun_path.len()` (108) bytes long
// indexes one past the array: a bounds-check panic in ReleaseSafe. The port had
// the same index and panicked. `NOTIFY_SOCKET` comes from the environment, so
// such a value is reachable. A value that leaves no room for the NUL now takes
// the reference's own "too long" warning.
//
// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD203. The reference
// accepts an `@`-prefixed value as an "abstract namespace address" but copies
// the `@` into `sun_path` as a literal byte and connects with the whole struct
// as the length, so it connects to a filesystem socket named `@...` relative
// to the working directory and never to the abstract socket systemd means
// (`sd_notify(3)`: a leading `@` stands for a NUL byte, and the address is as
// long as the name). The port does what the check says is supported: `@`
// becomes the leading NUL and the length is the name's. Path addresses keep
// the reference's length, `@offsetOf(sockaddr.un, "path") + path.len` (the
// whole struct; S112-F8).
#[cfg(any(target_os = "linux", all(test, unix)))]
fn notify_address(
    socket_path: &[u8],
) -> Result<(libc::sockaddr_un, libc::socklen_t), NotifyAddressError> {
    // The socket address must be a path or an abstract socket.
    if socket_path[0] != b'/' && socket_path[0] != b'@' {
        return Err(NotifyAddressError::NotPathOrAbstract);
    }

    let mut socket_address: libc::sockaddr_un = unsafe { std::mem::zeroed() };

    // Error out if the supplied socket path is too long.
    // (`<=`, not the reference's `<`: the NUL below needs one more byte. UD202.)
    if socket_address.sun_path.len() <= socket_path.len() {
        return Err(NotifyAddressError::TooLong);
    }

    socket_address.sun_family = libc::AF_UNIX as libc::sa_family_t;

    // @memcpy(socket_address.path[0..socket_path.len], socket_path);
    // socket_address.path[socket_path.len] = 0;
    for (i, &b) in socket_path.iter().enumerate() {
        socket_address.sun_path[i] = b as libc::c_char;
    }
    socket_address.sun_path[socket_path.len()] = 0;

    let path_off = std::mem::offset_of!(libc::sockaddr_un, sun_path);
    let addr_len = if socket_path[0] == b'@' {
        // UD203: the abstract name is the bytes after a leading NUL.
        socket_address.sun_path[0] = 0;
        path_off + socket_path.len()
    } else {
        // @offsetOf(std.os.linux.sockaddr.un, "path") + socket_address.path.len
        path_off + socket_address.sun_path.len()
    };
    Ok((socket_address, addr_len as libc::socklen_t))
}

// PORT NOTE: `std.mem.trimRight(u8, slice, "\n")`.
#[cfg(any(target_os = "linux", all(test, unix)))]
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

// (unix: the helpers these test are the Linux arm's, compiled for tests on
// every unix host; Windows has no `/proc` or `sockaddr_un`.)
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A fresh file or directory under the system temp dir, removed on drop.
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn path(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("rustty-tg-systemd-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            let _ = std::fs::remove_file(&p);
            Temp(p)
        }
        fn bytes(&self) -> Vec<u8> {
            self.0.as_os_str().as_encoded_bytes().to_vec()
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// S112-F8: the comm check compares at most 16 bytes, less trailing
    /// newlines, and reports an open failure and a read failure separately.
    #[test]
    fn comm_check_reads_16_bytes_and_has_two_failure_arms() {
        let f = Temp::path("comm");
        std::fs::write(&f.0, b"systemd\n").unwrap();
        assert_eq!(comm_is_systemd(&f.bytes()), Ok(true));
        std::fs::write(&f.0, b"systemd").unwrap();
        assert_eq!(comm_is_systemd(&f.bytes()), Ok(true));
        std::fs::write(&f.0, b"bash\n").unwrap();
        assert_eq!(comm_is_systemd(&f.bytes()), Ok(false));
        // 16 bytes are compared: "systemd" followed by 9 newlines is "systemd".
        std::fs::write(&f.0, b"systemd\n\n\n\n\n\n\n\n\nX").unwrap();
        assert_eq!(comm_is_systemd(&f.bytes()), Ok(true));

        // Missing file: the open arm.
        let missing = Temp::path("missing");
        assert_eq!(comm_is_systemd(&missing.bytes()), Err(CommError::Open));

        // A directory opens but cannot be read (EISDIR): the read arm.
        let d = Temp::path("dir");
        std::fs::create_dir_all(&d.0).unwrap();
        assert_eq!(comm_is_systemd(&d.bytes()), Err(CommError::Read));
    }

    fn sun_path_len() -> usize {
        let a: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        a.sun_path.len()
    }

    /// UD202: a `NOTIFY_SOCKET` exactly as long as `sun_path` leaves no room for
    /// the NUL; it is "too long" instead of an out-of-bounds write. One byte
    /// shorter still fits.
    #[test]
    fn notify_socket_as_long_as_sun_path_is_too_long() {
        let n = sun_path_len();
        let mut path = b"/".to_vec();
        path.resize(n, b'a');
        assert_eq!(
            notify_address(&path).err(),
            Some(NotifyAddressError::TooLong)
        );
        path.pop();
        let (addr, _) = notify_address(&path).expect("sun_path.len() - 1 bytes fit");
        assert_eq!(addr.sun_path[n - 1], 0);
    }

    /// S112-F8 and UD203: a path address is connected with the reference's
    /// length (the whole struct); an abstract `@name` becomes `\0name`, with
    /// the name's length.
    #[test]
    fn notify_address_lengths() {
        let off = std::mem::offset_of!(libc::sockaddr_un, sun_path);
        let (addr, len) = notify_address(b"/run/systemd/notify").unwrap();
        assert_eq!(len as usize, off + sun_path_len());
        assert_eq!(addr.sun_path[0], b'/' as libc::c_char);

        let (addr, len) = notify_address(b"@abstract").unwrap();
        assert_eq!(len as usize, off + b"@abstract".len());
        assert_eq!(addr.sun_path[0], 0);
        assert_eq!(addr.sun_path[1], b'a' as libc::c_char);

        assert_eq!(
            notify_address(b"relative").err(),
            Some(NotifyAddressError::NotPathOrAbstract)
        );
    }
}
