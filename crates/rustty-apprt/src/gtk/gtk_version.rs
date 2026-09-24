//! rustty-apprt::gtk::gtk_version
//!
//! PORTS: ghostty `src/apprt/gtk/gtk_version.zig` (140 loc, 2 `test{}` blocks
//! @ `:103` "atLeast" and `:122` "runtimeUntil" — both ported below).
//!
//! # AD-3: the comptime fold is DELETED, the runtime predicates stay live
//!
//! The oracle carries TWO version numbers:
//!
//! * `comptime_version` (`gtk_version.zig:14-18`) — read from the BUILD BOX's
//!   headers via `@cInclude("gtk/gtk.h")` -> `c.GTK_MAJOR_VERSION`. It answers
//!   "which symbols may I name?".
//! * `getRuntimeVersion()` (`gtk_version.zig:20-26`) — `gtk_get_major_version()`
//!   et al, a read of the actually-linked shared library. It answers "what am I
//!   talking to right now?".
//!
//! `atLeast` (`gtk_version.zig:49`) folds the first over the second: it opens
//! with `if (comptime comptime_version.order(threshold) == .lt) return false;`
//! (`:58-62`), a compile-time-CONSTANT condition inside an `inline fn` with
//! `comptime` params — so it collapses at EVERY call site, runtime ones
//! included. Its own doc says so (`:47-48`): "This is inlined so that the
//! comptime checks will disable the runtime checks if the comptime checks
//! fail." `runtimeAtLeast`/`runtimeUntil` (`:75`, `:90`) skip the fold and are
//! unconditionally live.
//!
//! **rustty has no `comptime_version`.** There is no header include:
//! [`gtk4::minor_version`] is ALWAYS a runtime read of the real shared library,
//! and the `v4_14` Cargo feature is an API-surface CEILING (which symbols
//! exist), not a version oracle. [`BASELINE`] below is the honest analogue of
//! `comptime_version` — the version rustty COMPILES against — but it is a
//! documentation/logging constant, NOT a predicate input. See [`at_least`].
//!
//! Per AD-3 the comptime branches collapse to the modern arm and are deleted;
//! the runtime predicates stay live. The full derivation, the enumerated
//! deletion set, and the two `v4_14`-ceiling losses live in `Cargo.toml` and
//! `.port/phase-2/DEFERRALS.md`.

/// `gtk_version.zig:14` `comptime_version` — the version rustty COMPILES
/// against, i.e. the API surface the `gtk4/v4_14` Cargo feature selects.
///
/// This is AD-3's pinned COMPATIBILITY floor, not this dev box's GTK (4.22.4).
/// It is deliberately **not** a predicate input: unlike the oracle's
/// header-derived `comptime_version`, it cannot tell you anything about the
/// library you are linked to at runtime. Its two honest uses are
/// [`log_version`]'s `build=` field (exactly the oracle's use of
/// `comptime_version` at `gtk_version.zig:29-32`) and the tripwire below.
pub const BASELINE: (u16, u16, u16) = (4, 14, 0);

/// AD-3's ratified floor. Kept separate from [`BASELINE`] so that lowering the
/// baseline below what the plan ratified fails the BUILD rather than silently
/// widening the supported envelope. Raising `BASELINE` is a plan decision (it
/// must move in lockstep with the `gtk4` feature in `Cargo.toml`); this only
/// catches a DOWNGRADE.
const AD3_FLOOR: (u16, u16, u16) = (4, 14, 0);
const _: () = assert!(
    !matches!(order(BASELINE, AD3_FLOOR), std::cmp::Ordering::Less),
    "AD-3 pins the GTK baseline at >= 4.14; BASELINE was lowered below it. \
     Lower `Cargo.toml`'s `gtk4` feature and this floor together, via the plan."
);

/// `std.SemanticVersion.order` (`gtk_version.zig:58`, `:85`, `:96`).
///
/// Rust's std has no `SemanticVersion`, so the oracle's shared stdlib call
/// becomes a local const fn. `adw_version` carries its own copy rather than
/// importing this one: the oracle's two version modules are independent
/// parallel files, and keeping each side-by-side-reviewable against its own
/// `.zig` counterpart is worth nine duplicated lines.
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

/// `gtk_version.zig:20` `getRuntimeVersion` — the version of the GTK we are
/// actually linked against.
///
/// `gtk_get_major_version()` and friends are pure constant-returning C
/// functions; gtk4-rs marks all three `skip_assert_initialized!`
/// (gtk4-0.11.4 `src/auto/functions.rs:163,170,177`), so this is safe to call
/// before `gtk_init` and off the main thread — matching the oracle, which calls
/// them from `logVersion` with no initialization precondition.
#[inline]
pub fn runtime_version() -> (u16, u16, u16) {
    (
        gtk4::major_version() as u16,
        gtk4::minor_version() as u16,
        gtk4::micro_version() as u16,
    )
}

/// `gtk_version.zig:28` `logVersion`.
pub fn log_version() {
    let (bma, bmi, bmc) = BASELINE;
    let (rma, rmi, rmc) = runtime_version();
    log_info(format!(
        "GTK version build={bma}.{bmi}.{bmc} runtime={rma}.{rmi}.{rmc}"
    ));
}

/// `gtk_version.zig:49` `atLeast`.
///
/// Verifies that the GTK version is at least the given version.
///
/// This can be run in both a comptime and runtime context. If it is run in a
/// comptime context, it will only check the version in the headers. If it is
/// run in a runtime context, it will check the actual version of the library we
/// are linked against.
///
/// This function should be used in cases where the version check would affect
/// code generation, such as using symbols that are only available beyond a
/// certain version. For checks which only depend on GTK's runtime behavior,
/// use `runtimeAtLeast`.
///
/// This is inlined so that the comptime checks will disable the runtime checks
/// if the comptime checks fail.
///
/// # This is a live runtime check, NOT a comptime fold
///
/// The oracle's `atLeast` is `comptime` fold + runtime check (see the module
/// doc). rustty has only the second half, so **`at_least` is a live delegate to
/// [`runtime_at_least`]** and the two are behaviorally identical. The name is
/// kept so the oracle's `atLeast` call sites port 1:1.
///
/// The plan (`docs/superpowers/plans/2026-07-17-L5-gtk-apprt.md:96`) specs this
/// as a "const fn over `BASELINE`". That was **not** taken, because it would
/// manufacture a header fold rustty does not have and would const-`false`
/// `window.zig:695` (`!gtk_version.atLeast(4, 16, 0)`, gating the
/// `window-theme-ghostty` CSS class) — a site `.port/phase-2/DEFERRALS.md`
/// ratifies as LIVE and explicitly warns must "NOT be deleted as [a] comptime
/// [branch]". Upstream ghostty on a >= 4.16 host takes the live arm there;
/// a const-`BASELINE` fold would silently take the other one on every host
/// forever. Note also that a const-`BASELINE` `at_least` would have no
/// legitimate call site at all: the two sites where the `v4_14` ceiling really
/// does force the legacy arm (`application.zig:1261`, `:1565`) cannot even name
/// their modern arm's symbols, so they port as the legacy arm unconditionally,
/// with no predicate. DEFERRALS assigns this call to T2 ("T2 must implement it
/// as a live delegate or get plan:96 amended; T1 flags, does not decide") and
/// amending the plan is the plan owner's authority, not a task's — so: live
/// delegate. Flagged in the T2 report for the plan owner to ratify or re-spec.
#[inline]
pub fn at_least(major: u16, minor: u16, micro: u16) -> bool {
    runtime_at_least(major, minor, micro)
}

/// `gtk_version.zig:75` `runtimeAtLeast`.
///
/// Verifies that the GTK version at runtime is at least the given version.
///
/// This function should be used in cases where the only the runtime behavior
/// is affected by the version check. For checks which would affect code
/// generation, use `atLeast`.
///
/// Oracle: `runtime_version.order(threshold) != .lt` (`:85-89`).
#[inline]
pub fn runtime_at_least(major: u16, minor: u16, micro: u16) -> bool {
    // We use the functions instead of the constants such as c.GTK_MINOR_VERSION
    // because the function gets the actual runtime version.
    !matches!(
        order(runtime_version(), (major, minor, micro)),
        std::cmp::Ordering::Less
    )
}

/// `gtk_version.zig:90` `runtimeUntil` — the GTK version at runtime is BELOW
/// the given version.
///
/// Oracle: `runtime_version.order(threshold) == .lt` (`:96-100`), i.e. the
/// exact negation of [`runtime_at_least`]'s `!= .lt`.
#[inline]
pub fn runtime_until(major: u16, minor: u16, micro: u16) -> bool {
    matches!(
        order(runtime_version(), (major, minor, micro)),
        std::cmp::Ordering::Less
    )
}

/// `const log = std.log.scoped(.gtk)` (`gtk_version.zig:11`). No brand token, so
/// nothing to rebrand (cf. `gtk/class/config.rs`'s `LOG_SCOPE` naming call).
const LOG_SCOPE: &str = "gtk";

/// `std.log.info` scoped `.gtk` (`gtk_version.zig:11`). The parity harness does
/// not assert log output; this goes to stderr like the reference's scoped
/// logger (the `rustty-config` convention, `config_struct.rs:5237`).
fn log_info(msg: String) {
    log::info!(target: LOG_SCOPE, "{msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `gtk_version.zig:103` `test "atLeast"`.
    ///
    /// The oracle pivots its table on `c.GTK_{MAJOR,MINOR,MICRO}_VERSION` — the
    /// BUILD BOX's headers — and applies the SAME table to both `atLeast` and
    /// `runtimeAtLeast`. Note that the table only holds for `runtimeAtLeast`
    /// because on the build box the header version and the linked runtime
    /// version are the same number: `!fun(MA, MI, MC + 1)` asserts
    /// `runtime < header + 1`, which pins `runtime == header`. rustty has no
    /// header, so the faithful pivot is the RUNTIME version, read exactly as
    /// `getRuntimeVersion` reads it (`gtk_version.zig:20`).
    ///
    /// What the table actually exercises is the semver total order — that major
    /// dominates minor dominates micro (`fun(MA - 1, MI + 1, MC)` is true even
    /// though the minor went UP) — and that ports across exactly.
    ///
    /// Non-display test: `gtk_get_*_version` need no `gtk_init` (see
    /// [`runtime_version`]), so this runs in the normal gate.
    #[test]
    fn gtk_version_at_least() {
        let (ma, mi, mc) = runtime_version();

        // The `mi - 1` row below underflows at minor == 0. AD-3 pins the
        // supported floor at 4.14, so every runtime this suite is valid on has
        // minor >= 14; assert that rather than let a subtraction wrap.
        assert!(
            runtime_at_least(BASELINE.0, BASELINE.1, BASELINE.2),
            "AD-3 floor: gtk >= {BASELINE:?} required, found {:?}",
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

    /// PORTS: `gtk_version.zig:122` `test "runtimeUntil"` — the exact negation
    /// of the table above.
    #[test]
    fn gtk_version_runtime_until() {
        let (ma, mi, mc) = runtime_version();

        assert!(
            runtime_at_least(BASELINE.0, BASELINE.1, BASELINE.2),
            "AD-3 floor: gtk >= {BASELINE:?} required, found {:?}",
            (ma, mi, mc)
        );

        // Oracle: "This is an array in case we add a comptime variant." (`:125`)
        let funs: [fn(u16, u16, u16) -> bool; 1] = [runtime_until];
        for fun in funs {
            assert!(!fun(ma, mi, mc));

            assert!(fun(ma, mi, mc + 1));
            assert!(fun(ma, mi + 1, mc));
            assert!(fun(ma + 1, mi, mc));

            assert!(!fun(ma - 1, mi, mc));
            assert!(!fun(ma - 1, mi + 1, mc));
            assert!(!fun(ma - 1, mi, mc + 1));

            assert!(!fun(ma, mi - 1, mc + 1));
        }
    }
}
