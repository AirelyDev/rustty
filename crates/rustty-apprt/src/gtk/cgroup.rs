//! rustty-apprt::gtk::cgroup
//!
//! PORTS: ghostty `src/apprt/gtk/cgroup.zig`.
//!
//! "Contains all the logic for putting individual surfaces into transient
//! systemd scopes." (`cgroup.zig:1-2`)
//!
//! This is DISTINCT from `rustty_os::cgroup` (the port of `src/os/cgroup.zig`,
//! which only *reads* `/proc/<pid>/cgroup`). This module *creates* the transient
//! scope over D-Bus.
//!
//! R5 / platform gating: the whole `gtk` module is
//! `#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]`
//! (`lib.rs`), and every gtk4-rs dep is
//! `[target.'cfg(any(target_os = "linux", target_os = "freebsd"))'.dependencies]`
//! **and** `optional`. So this file is not compiled by a macOS build nor by a
//! default-feature Linux/FreeBSD build — the same structural gate every other
//! file under `src/gtk/` relies on.
//!
//! PORT NOTE (W2D/F-002): this file stays COMPILED on FreeBSD, exactly as the
//! reference compiles `apprt/gtk/cgroup.zig` for it (the file carries no os.tag
//! guard). Its body is pure gio/glib D-Bus — no Linux-only API is named — and
//! the systemd path is reached only when the `linux-cgroup` config says so
//! (`gtk/post_fork.zig:40`), which is a runtime decision, not a target one.
//!
//! PORT NOTE: the reference's `log = std.log.scoped(.gtk_systemd_cgroup)`; our
//! diagnostics carry the same scope in their prefix.

use glib::prelude::ToVariant;
use glib::VariantTy;

/// The scoped log name — `cgroup.zig:12` `std.log.scoped(.gtk_systemd_cgroup)`.
const LOG_SCOPE: &str = "gtk_systemd_cgroup";

/// PORTS: `cgroup.zig:14-17` `pub const Options`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// `memory_high: ?u64 = null` (`cgroup.zig:15`).
    pub memory_high: Option<u64>,
    /// `tasks_max: ?u64 = null` (`cgroup.zig:16`).
    pub tasks_max: Option<u64>,
}

/// The error the reference's `createScope` returns: `error{DbusCallFailed}`
/// (`cgroup.zig:29`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbusCallFailed;

impl core::fmt::Display for DbusCallFailed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "error.DbusCallFailed")
    }
}

impl std::error::Error for DbusCallFailed {}

/// The scope-name prefix. PORTS the `"app-ghostty-surface-transient-"` head of
/// `cgroup.zig:20`'s format string, retargeted to the rustty product identity
/// (R2) from the single source of truth (`rustty_identity::APP_NAME`).
const SCOPE_HEAD_A: &str = "app-";
const SCOPE_HEAD_B: &str = "-surface-transient-";
/// The `".scope"` tail of `cgroup.zig:20`'s format string.
const SCOPE_TAIL: &str = ".scope";

/// The maximum length of a [`fmt_scope`] result, EXCLUDING the NUL sentinel.
///
/// PORTS the reference's assert operand (`cgroup.zig:22`): `fmt.len - 2 +
/// log10_int(maxInt(u32)) + 1` — the format string minus the `{}` placeholder,
/// plus the widest `u32` decimal rendering (10 digits).
pub const SCOPE_BUF_LEN: usize = SCOPE_HEAD_A.len()
    + rustty_identity::APP_NAME.len()
    + SCOPE_HEAD_B.len()
    + 10
    + SCOPE_TAIL.len();

/// PORTS: `cgroup.zig:19-26` `fmtScope`. Renders the transient scope unit name
/// for `pid` into `buf` and returns the rendered bytes; a NUL sentinel is
/// written to `buf` immediately past them, exactly as `std.fmt.bufPrintZ`'s
/// `[:0]const u8` result does.
///
/// PORT NOTE (byte fidelity): the reference returns a Zig byte slice; we return
/// `&[u8]`, never a `String` — this value is compared byte-for-byte against a
/// slice of `/proc/<pid>/cgroup` in [`super::pre_exec`] / [`super::post_fork`].
///
/// ASYNC-SIGNAL SAFETY: this function performs NO allocation, NO locking and no
/// syscalls — it is a pure fill of a caller-provided buffer, which is what makes
/// it legal on the post-`fork` side of `pre_exec` (see [`super::pre_exec`]).
///
/// # Panics
/// If `buf` is smaller than `SCOPE_BUF_LEN + 1`. The reference asserts the same
/// bound (`cgroup.zig:22`, `quirks.inlineAssert`) and then `catch unreachable`s
/// the format.
#[must_use]
pub fn fmt_scope(buf: &mut [u8], pid: u32) -> &[u8] {
    // `assert(buf.len >= fmt.len - 2 + log10_int(maxInt(u32)) + 1)`
    // (`cgroup.zig:22`); `+ 1` beyond it for the `bufPrintZ` sentinel.
    assert!(
        buf.len() > SCOPE_BUF_LEN,
        "cgroup.fmt_scope: buffer too small"
    );

    let mut n = 0usize;

    // `"app-" ++ <app name> ++ "-surface-transient-"`.
    for part in [SCOPE_HEAD_A, rustty_identity::APP_NAME, SCOPE_HEAD_B] {
        buf[n..n + part.len()].copy_from_slice(part.as_bytes());
        n += part.len();
    }

    // `{}` — the pid, decimal. A `u32` renders in at most 10 digits.
    let mut digits = [0u8; 10];
    let mut d = digits.len();
    let mut v = pid;
    loop {
        d -= 1;
        digits[d] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    buf[n..n + (digits.len() - d)].copy_from_slice(&digits[d..]);
    n += digits.len() - d;

    // `".scope"`.
    buf[n..n + SCOPE_TAIL.len()].copy_from_slice(SCOPE_TAIL.as_bytes());
    n += SCOPE_TAIL.len();

    // `bufPrintZ`'s sentinel.
    buf[n] = 0;
    &buf[..n]
}

/// Create a transient systemd scope unit for the given process and
/// move the process into it.
///
/// PORTS: `cgroup.zig:28-115` `createScope`.
///
/// # Errors
/// [`DbusCallFailed`] if the `StartTransientUnit` call fails
/// (`cgroup.zig:106-114`).
///
/// # Panics
/// Never in practice: the `VariantTy::new` calls parse static, valid type
/// strings, and the rendered scope name is ASCII by construction.
pub fn create_scope(
    dbus: &gio::DBusConnection,
    pid: u32,
    options: Options,
) -> Result<(), DbusCallFailed> {
    use gio::prelude::*;

    // "The unit name needs to be unique. We use the PID for this."
    // (`cgroup.zig:33-35`) — the reference's `var name_buf: [256]u8`.
    let mut name_buf = [0u8; 256];
    let name = fmt_scope(&mut name_buf, pid);
    // `builder.add("s", name.ptr)` (`cgroup.zig:43`). The bytes are ASCII by
    // construction (two ASCII literals, the ASCII app name, and decimal
    // digits), so this is lossless — NOT a lossy `String` conversion.
    let name = std::str::from_utf8(name).expect("scope name is ASCII by construction");

    // Properties — the opened `a(sv)` (`cgroup.zig:46-73`), built in the
    // reference's exact order.
    let mut properties: Vec<glib::Variant> = Vec::new();

    // `if (options.memory_high) |value| builder.add("(sv)", "MemoryHigh",
    // glib.Variant.newUint64(value));` (`cgroup.zig:55-57`).
    if let Some(value) = options.memory_high {
        properties.push(sv("MemoryHigh", &value.to_variant()));
    }

    // `if (options.tasks_max) |value| ... "TasksMax" ...` (`cgroup.zig:59-61`).
    if let Some(value) = options.tasks_max {
        properties.push(sv("TasksMax", &value.to_variant()));
    }

    // https://www.freedesktop.org/software/systemd/man/latest/systemd-oomd.service.html
    // `builder.add("(sv)", "ManagedOOMMemoryPressure",
    // glib.Variant.newString("kill"))` (`cgroup.zig:63-64`).
    properties.push(sv("ManagedOOMMemoryPressure", &"kill".to_variant()));

    // "PID to move into the unit" — `glib.Variant.newFixedArray(<"u">, &pid, 1,
    // @sizeOf(u32))` is a one-element `au` (`cgroup.zig:66-72`).
    properties.push(sv("PIDs", &vec![pid].to_variant()));

    let properties = glib::Variant::array_from_iter_with_type(
        VariantTy::new("(sv)").expect("static type string"),
        properties,
    );

    // "Aux - unused but must be present" — an EMPTY `a(sa(sv))`
    // (`cgroup.zig:75-82`).
    let aux = glib::Variant::array_from_iter_with_type(
        VariantTy::new("(sa(sv))").expect("static type string"),
        std::iter::empty::<glib::Variant>(),
    );

    // Initialize our builder to build up our parameters
    //
    // `builder_type = "(ssa(sv)a(sa(sv)))"` (`cgroup.zig:37`), filled with
    // `.add("s", name)` / `.add("s", "fail")` (`cgroup.zig:43-44`) and the two
    // opened sub-arrays; `builder.end()` (`cgroup.zig:92`) is this tuple.
    let value =
        glib::Variant::tuple_from_iter([name.to_variant(), "fail".to_variant(), properties, aux]);

    // `reply_type = glib.VariantType.new("(o)")` (`cgroup.zig:89`).
    let reply_type = VariantTy::new("(o)").expect("static type string");

    // `dbus.callSync(...)` (`cgroup.zig:94-105`): `.{}` flags is `NONE`, `-1` is
    // the default timeout, `null` is "not cancellable".
    match dbus.call_sync(
        Some("org.freedesktop.systemd1"),
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
        "StartTransientUnit",
        Some(&value),
        Some(reply_type),
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    ) {
        // `defer reply.unref();` (`cgroup.zig:115`) — the returned `Variant`'s
        // scope-end drop.
        Ok(_reply) => Ok(()),
        Err(err) => {
            // `log.err("creating transient cgroup scope failed code={} err={s}",
            // .{ e.f_code, e.f_message orelse "(no message)" })`
            // (`cgroup.zig:106-113`).
            //
            // PORT NOTE: gtk-rs wraps `*glib.Error` in `glib::Error`, whose
            // `Debug` renders the domain, the `f_code` and the `f_message` — the
            // same two fields the reference names, and it cannot be null-messaged
            // (the "(no message)" fallback is unreachable through this binding).
            eprintln!("error({LOG_SCOPE}): creating transient cgroup scope failed {err:?}");
            Err(DbusCallFailed)
        }
    }
}

/// Build one `(sv)` property entry — the reference's `builder.add("(sv)", key,
/// value)` (`cgroup.zig:56,60,64,72`).
fn sv(key: &str, value: &glib::Variant) -> glib::Variant {
    glib::Variant::tuple_from_iter([key.to_variant(), glib::Variant::from_variant(value)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scope name must render exactly the reference's format, with the
    /// rustty identity substituted for `ghostty` (R2).
    #[test]
    fn fmt_scope_renders_reference_format() {
        let mut buf = [0u8; 256];
        let got = fmt_scope(&mut buf, 1234);
        assert_eq!(
            got,
            format!(
                "app-{}-surface-transient-1234.scope",
                rustty_identity::APP_NAME
            )
            .as_bytes()
        );
        // `bufPrintZ`'s sentinel sits just past the returned slice.
        assert_eq!(buf[got.len()], 0);
    }

    /// The widest `u32` still fits the asserted buffer bound (`cgroup.zig:22`).
    #[test]
    fn fmt_scope_fits_max_u32() {
        let mut buf = [0u8; 256];
        let got = fmt_scope(&mut buf, u32::MAX);
        assert!(got.ends_with(b"-4294967295.scope"));
        assert_eq!(got.len(), SCOPE_BUF_LEN);
    }

    /// Single-digit pids must not be zero-padded.
    #[test]
    fn fmt_scope_single_digit() {
        let mut buf = [0u8; 256];
        let got = fmt_scope(&mut buf, 7);
        assert!(
            got.ends_with(b"-7.scope"),
            "{:?}",
            String::from_utf8_lossy(got)
        );
    }

    /// The reference's fixed `[256]u8` call-site buffers (`cgroup.zig:34`,
    /// `pre_exec.zig:46`, `post_fork.zig:57`) must be big enough for us too —
    /// exercised, not just asserted: a 256-byte buffer must accept the widest
    /// possible scope name.
    #[test]
    fn scope_buf_len_fits_reference_256_byte_buffer() {
        let mut buf = [0u8; 256];
        let got = fmt_scope(&mut buf, u32::MAX);
        assert_eq!(got.len(), SCOPE_BUF_LEN);
        assert!(got.len() < buf.len(), "the NUL sentinel must also fit");
    }
}
