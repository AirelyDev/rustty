//! S43-F1 — `global.init` must re-sync the captured environment after
//! `ensureLocale`. See `crates/rustty-apprt/src/lib.rs`'s `init_with_action`,
//! at the `ensureLocale()` call site, for the full rationale (oracle
//! reference, why this is a single-`#[test]` file, and the platform gate).
//! History: an earlier version of this test called `init_with_action`
//! end-to-end and found it vacuous — a few lines after `ensureLocale()`,
//! `init_with_action` constructed `GlobalState::new(...)`, and that
//! constructor performed an unrelated fresh environ capture that accidentally
//! republished the corrected environment regardless of this fix. That was
//! itself an oracle divergence, recorded as finding N1
//! (`.port/parity-closure/NEW-FINDINGS.md`) and fixed by switching that call
//! site to `new_with_environ` in the same commit as this test — which is what
//! makes the assertion below load-bearing.

#![cfg(target_os = "macos")]

#[test]
fn init_resyncs_environ_after_ensure_locale() {
    // Force `ensureLocale`'s macOS arm: an empty `LANG` unconditionally
    // routes through `setLangFromCocoa`, which `setenv`s `LANG` to a
    // non-empty `"<lang>_<country>.UTF-8"` value.
    //
    // SAFETY: this test file contains exactly one `#[test]`, so it is the
    // only thread in this process that can observe/mutate the environment.
    unsafe { std::env::set_var("LANG", "") };

    // Capture the PRE-locale-fix block, exactly as `init_with_action` does
    // internally (it lazily captures `ENVIRON` at the `tmp_dir_path`
    // discovery / `RUSTTY_LOG` read, both of which run before
    // `ensureLocale`). Calling it here too just makes that ordering
    // independent of this test's assertion: whichever call wins, `ENVIRON` is
    // guaranteed to hold the empty `LANG` at this point, before the fix runs.
    let pre = rustty_global::environ_map();
    assert_eq!(
        pre.get(&b"LANG"[..]).map(Vec::as_slice),
        Some(&b""[..]),
        "setup: LANG must read back empty before init runs, or the trigger \
         below is vacuous"
    );

    rustty_apprt::init_with_action(false).expect("full init");

    // With the fix, the corrected LANG (set by setLangFromCocoa via setenv,
    // then re-captured by sync_environ()) must be visible through the
    // process-global environ. Without the fix, `environ_map()` still returns
    // the empty value captured above.
    let lang = rustty_global::environ_map()
        .get(&b"LANG"[..])
        .map(|v| String::from_utf8_lossy(v).into_owned())
        .unwrap_or_default();
    assert!(
        !lang.is_empty(),
        "environ still holds the pre-ensureLocale (empty) LANG — \
         init_with_action did not re-sync the captured environment"
    );

    // And it must be visible through the live process environment too (the
    // thing `sync_environ()` re-captures FROM) — a stronger check that the
    // mutation actually happened, not just that some other value leaked in.
    let live = std::env::var("LANG").unwrap_or_default();
    assert!(!live.is_empty(), "setLangFromCocoa did not set LANG");
    assert_eq!(
        lang, live,
        "captured environ's LANG must match the live process LANG"
    );
}
