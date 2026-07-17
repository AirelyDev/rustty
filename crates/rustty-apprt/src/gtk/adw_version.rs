//! rustty-apprt::gtk::adw_version
//!
//! PORTS: ghostty `src/apprt/gtk/adw_version.zig` (122 loc, 1 `test{}` block
//! @ `:87` "versionAtLeast" — ported below).
//!
//! Structurally this is `gtk_version`'s twin (see that module's doc for the
//! full comptime-fold derivation); everything there about `atLeast` being a
//! header fold rustty does not have applies verbatim here (`adw_version.zig:47`
//! mirrors `gtk_version.zig:49` line for line).
//!
//! # AD-3: the five `supports_*` helpers are DELETED
//!
//! `adw_version.zig:104-122` defines `supportsDialogs()` (`atLeast(1, 5, 0)`),
//! `supportsTabOverview()`, `supportsSwitchRow()`, `supportsToolbarView()`
//! (all `atLeast(1, 4, 0)`) and `supportsBanner()` (`atLeast(1, 3, 0)`). Every
//! one of them is at or below AD-3's pinned `adw >= 1.5` floor, so all five are
//! unconditionally true across the entire supported envelope. Per AD-3 they are
//! **not ported at all** — not even as `const fn { true }` — and the modern arm
//! is inlined at each of their call sites by the tasks that port those files:
//!
//! * `dialog.zig:21` and `surface_child_exited.zig:15` — `pub const Parent` /
//!   type alias. A Rust TYPE cannot be chosen at runtime.
//! * `debug_warning.zig:14` — same (`adw.Bin` vs `gtk.Box`).
//! * `clipboard_confirmation_dialog.zig:17` — file-level `const can_remember`.
//! * `debug_warning.zig:42` — inside `comptime gresource.blueprint(...)`, i.e.
//!   the build-time resource manifest. This is why the blueprint manifest is
//!   19 entries, not 21.
//! * `window.zig:1799` — AD-3's one extension BEYOND that rule: a plain runtime
//!   `if` around two `showAboutDialog` arms that the rule alone would keep
//!   live. Ported as the `adw` arm unconditionally, dropping the
//!   `gtk.showAboutDialog` legacy arm. Observable only on an adw < 1.5 runtime,
//!   which AD-3 declares unsupported. Recorded in `.port/phase-2/DEFERRALS.md`
//!   as a flagged deviation from ghostty-on-this-host, not as silent narrowing.
//!
//! `supportsTabOverview` and `supportsToolbarView` have ZERO call sites
//! repo-wide, so deleting them costs nothing at all.

/// `adw_version.zig:15` `comptime_version` — the version rustty COMPILES
/// against, i.e. the API surface the `libadwaita/v1_5` Cargo feature selects.
/// AD-3's pinned COMPATIBILITY floor, not this dev box's libadwaita (1.9.2).
/// See [`crate::gtk::gtk_version::BASELINE`] for why this is not a predicate
/// input.
pub const BASELINE: (u16, u16, u16) = (1, 5, 0);

/// AD-3's ratified floor; see [`crate::gtk::gtk_version::BASELINE`]'s tripwire.
const AD3_FLOOR: (u16, u16, u16) = (1, 5, 0);
const _: () = assert!(
    !matches!(order(BASELINE, AD3_FLOOR), std::cmp::Ordering::Less),
    "AD-3 pins the libadwaita baseline at >= 1.5; BASELINE was lowered below \
     it. Lower `Cargo.toml`'s `libadwaita` feature and this floor together, \
     via the plan."
);

/// `std.SemanticVersion.order` (`adw_version.zig:55`, `:80`). Deliberately a
/// local copy of `gtk_version`'s — see that module's `order` doc.
const fn order(a: (u16, u16, u16), b: (u16, u16, u16)) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if a.0 != b.0 {
        return if a.0 < b.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    if a.1 != b.1 {
        return if a.1 < b.1 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    if a.2 != b.2 {
        return if a.2 < b.2 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    Ordering::Equal
}

/// `adw_version.zig:22` `getRuntimeVersion` — the version of the libadwaita we
/// are actually linked against.
///
/// # Why the raw `ffi` calls rather than `libadwaita::major_version()`
///
/// `adw_get_major_version()` is a pure constant-returning C function, exactly
/// like its `gtk_get_major_version()` counterpart, and the oracle calls it with
/// no initialization precondition (`adw_version.zig:23-25`, reached from
/// `logVersion`). But libadwaita-0.9.2 wraps all three in
/// `assert_initialized_main_thread!` (`src/auto/functions.rs:147,154,161`),
/// which **panics** with "Gtk has to be initialized before using libadwaita"
/// unless `gtk::init()` has run on this thread (`src/lib.rs:98-108`). That is a
/// binding artifact — a `.gir` annotation accident — and not a real constraint:
/// gtk4-rs marks the identical GTK functions `skip_assert_initialized!`
/// (gtk4-0.11.4 `src/auto/functions.rs:163,170,177`).
///
/// Going through the safe wrapper would therefore inject a panic the oracle
/// does not have, on a function the oracle treats as callable anywhere. The
/// `ffi` module is re-exported for exactly this (libadwaita-0.9.2
/// `src/lib.rs:91`), and it keeps this module's test out of the display-gated
/// lane.
///
/// SAFETY: `adw_get_{major,minor,micro}_version` return a compiled-in constant
/// and touch no library state, so they have no initialization, threading, or
/// aliasing precondition. Symbols resolve against the same `libadwaita-1.so`
/// the safe bindings link (libadwaita-sys-0.9.2 `src/lib.rs:7619-7621`).
#[inline]
pub fn runtime_version() -> (u16, u16, u16) {
    unsafe {
        (
            libadwaita::ffi::adw_get_major_version() as u16,
            libadwaita::ffi::adw_get_minor_version() as u16,
            libadwaita::ffi::adw_get_micro_version() as u16,
        )
    }
}

/// `adw_version.zig:30` `logVersion`.
pub fn log_version() {
    let (bma, bmi, bmc) = BASELINE;
    let (rma, rmi, rmc) = runtime_version();
    log_info(format!(
        "libadwaita version build={bma}.{bmi}.{bmc} runtime={rma}.{rmi}.{rmc}"
    ));
}

/// `adw_version.zig:47` `atLeast` — verifies the running libadwaita version is
/// at least the given version.
///
/// A live delegate to [`runtime_at_least`], for the reasons spelled out at
/// [`crate::gtk::gtk_version::at_least`]: rustty has no header fold to apply.
#[inline]
pub fn at_least(major: u16, minor: u16, micro: u16) -> bool {
    runtime_at_least(major, minor, micro)
}

/// `adw_version.zig:73` `runtimeAtLeast` — verifies the libadwaita version AT
/// RUNTIME is at least the given version.
///
/// Oracle: `runtime_version.order(threshold) != .lt` (`:80-84`).
#[inline]
pub fn runtime_at_least(major: u16, minor: u16, micro: u16) -> bool {
    !matches!(
        order(runtime_version(), (major, minor, micro)),
        std::cmp::Ordering::Less
    )
}

/// `std.log.info` scoped `.gtk` (`adw_version.zig:13` — the adw module logs
/// under the `.gtk` scope too, not a `.adw` one).
fn log_info(msg: String) {
    eprintln!("info(gtk): {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `adw_version.zig:87` `test "versionAtLeast"` — the same table
    /// `gtk_version`'s "atLeast" runs; see that test for why the pivot is the
    /// runtime version rather than a header constant.
    ///
    /// Non-display test: `adw_get_*_version` need no `gtk_init` (see
    /// [`runtime_version`]), so this runs in the normal gate.
    #[test]
    fn adw_version_at_least() {
        let (ma, mi, mc) = runtime_version();

        // The `mi - 1` row below underflows at minor == 0; AD-3's 1.5 floor
        // guarantees minor >= 5 on every supported runtime.
        assert!(
            runtime_at_least(BASELINE.0, BASELINE.1, BASELINE.2),
            "AD-3 floor: adw >= {BASELINE:?} required, found {:?}",
            (ma, mi, mc)
        );

        // Oracle: `const funs = &.{ atLeast, runtimeAtLeast };`
        let funs: [fn(u16, u16, u16) -> bool; 2] = [at_least, runtime_at_least];
        for fun in funs {
            assert!(fun(ma, mi, mc));
            assert!(!fun(ma, mi, mc + 1));
            assert!(!fun(ma, mi + 1, mc));
            assert!(!fun(ma + 1, mi, mc));
            assert!(fun(ma - 1, mi, mc));
            assert!(fun(ma - 1, mi + 1, mc));
            assert!(fun(ma - 1, mi, mc + 1));
            assert!(fun(ma, mi - 1, mc + 1));
        }
    }
}
