//! S43-F1 — `global.init` must re-sync the captured environment after
//! `ensureLocale`.
//!
//! ORACLE: `src/global.zig:204-209` / `:307-322` —
//! `try internal_os.ensureLocale(); syncEnviron();`. `ensureLocale` mutates
//! the process environment (via `setenv`/`unsetenv`, or on macOS via
//! `setLangFromCocoa`'s `setenv("LANG", ...)`), and `syncEnviron` re-captures
//! the process-global `Environ` immediately afterwards so every later
//! `environ()`/`environMap()` consumer (termio child-process environments,
//! the crash env snapshot) sees the corrected value.
//!
//! `crates/rustty-apprt/src/lib.rs`'s `init_with_action` called
//! `rustty_os::ensureLocale()` but — before this fix — never called
//! `rustty_global::sync_environ()` afterwards.
//!
//! # Why this is a source-level structural check, not an end-to-end behavioral one
//!
//! An end-to-end test (mutate `LANG`, call `init_with_action`, assert
//! `environ_map()` reflects the fix) was tried first and rejected as
//! TAUTOLOGICAL — it passes identically with the fix present OR absent.
//! Reason, verified by instrumenting both call sites:
//!
//! `init_with_action`'s OWN body (a few lines after the `ensureLocale()`
//! call site, at the `rustty_core::GlobalState::new((), resources_dir)`
//! construction) already performs an UNRELATED fresh
//! `environ::Environ::from_process()` capture + `set_environ()` publish, as a
//! side effect of `GlobalState::new`/`new_with_environ` needing an environ
//! block to derive `tmp_dir_path` from (`rustty-global/src/lib.rs:437-450`).
//! That happens to run strictly after `ensureLocale()` on the ONE reachable
//! success path, so it accidentally republishes the corrected environment
//! regardless of whether `sync_environ()` is called explicitly. Confirmed by
//! running the described end-to-end scenario against the pre-fix source: the
//! assertion `environ_map().get("LANG")` is non-empty even with no
//! `sync_environ()` call anywhere in `init_with_action`.
//!
//! This masking is NOT the fix, and is NOT robust:
//!   * it never fires on the (real, if rare) error path where `ensureLocale`
//!     succeeds but a LATER fallible step (`glslang::init`, `oni::init`,
//!     `resourcesDir`) fails before reaching the `GlobalState::new` call —
//!     `ENVIRON` is left holding the pre-locale-fix snapshot in that case,
//!     with no fix in place, until a later successful retry;
//!   * `GlobalState::new`'s own doc comment (`new_with_environ`,
//!     `rustty-global/src/lib.rs:415-424`) says the reference `.main` arm
//!     should reuse the ALREADY-captured environ, not grab a fresh one — so a
//!     future refactor that makes `GlobalState::new` reuse the existing
//!     capture (fixing that redundancy) would silently remove the masking and
//!     make this defect symptomatic again;
//!   * `rustty-os/src/locale.rs:171-173` already carries a comment promising
//!     "the app initializer calls [sync_environ] right after ensure_locale()"
//!     — before this fix that comment was simply false.
//!
//! So the actually-reliable, order-independent, non-tautological check is
//! textual: does `init_with_action`'s source call `sync_environ()` strictly
//! between the `ensureLocale()` call and the (masking) `GlobalState::new`
//! call? This is deterministic (no process env, no test ordering, no shared
//! process-global state to race), and it fails for exactly the right reason
//! when the fix is reverted: the call is simply absent from that span.
//!
//! See `crates/rustty-global/tests/sync_environ_republishes_from_process.rs`
//! for a runtime test of the underlying `sync_environ()` primitive itself
//! (that mutating the process env and calling it changes what `environ_map()`
//! returns) — this file only checks that `init_with_action` actually calls
//! that primitive at the right point.

const LIB_RS: &str = include_str!("../src/lib.rs");

#[test]
fn init_with_action_calls_sync_environ_right_after_ensure_locale() {
    let ensure_locale_at = LIB_RS
        .find("rustty_os::ensureLocale()")
        .expect("init_with_action must call rustty_os::ensureLocale()");

    // The masking construction identified above: `sync_environ()` is only a
    // genuine fix (rather than redundant with that side effect) if it runs
    // BEFORE this point too.
    let global_state_new_at = LIB_RS[ensure_locale_at..]
        .find("rustty_core::GlobalState::new(")
        .map(|off| ensure_locale_at + off)
        .expect("init_with_action must construct rustty_core::GlobalState after ensureLocale");

    let span = &LIB_RS[ensure_locale_at..global_state_new_at];
    assert!(
        span.contains("rustty_global::sync_environ()"),
        "init_with_action does not call rustty_global::sync_environ() between \
         ensureLocale() and the GlobalState::new() construction — the \
         LANG/LC_* mutations ensureLocale performs are not re-synced into the \
         process-global ENVIRON on the path where it matters (S43-F1). \
         (oracle: src/global.zig:204-209, :307-322)"
    );
}
