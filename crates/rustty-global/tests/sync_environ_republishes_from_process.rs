//! Runtime coverage for the `sync_environ()` primitive itself, which
//! `crates/rustty-apprt/src/lib.rs`'s `init_with_action` fix (S43-F1) relies
//! on: after a process-environment mutation, `sync_environ()` must make
//! `environ()`/`environ_map()` observe it.
//!
//! `environ()`/`environ_map()` read a process-global, lazily-captured
//! snapshot (`ENVIRON`, `crates/rustty-global/src/environ.rs`) — once
//! captured, later mutations to the real process environment are invisible
//! through them until something calls `sync_environ()` to re-capture. This is
//! the exact mechanism `ensureLocale` (which mutates `LANG` via
//! `setenv`/`unsetenv`) needs re-run afterwards; see
//! `crates/rustty-apprt/tests/global_init_resyncs_environ.rs` for why the
//! wiring is checked separately (a structural check, because the end-to-end
//! behavior is masked by an unrelated call elsewhere).
//!
//! WHY ITS OWN TEST BINARY: `std::env::set_var` is process-global and
//! `unsafe`, and races every other test's `getenv` in a shared binary
//! (`parallel-cargo-test-exposes-c-init-races`). A file containing exactly
//! one `#[test]` is this workspace's established pattern for making that
//! sound (`rustty-vt/tests/kitty_temp_dir_security.rs`,
//! `rustty-global/tests/env_non_utf8.rs`).

#[test]
fn sync_environ_makes_a_post_capture_mutation_visible() {
    const KEY: &str = "RUSTTY_SYNC_ENVIRON_PROBE";

    // SAFETY: this test file contains exactly one `#[test]`, so it is the
    // only thread in this process that can observe/mutate the environment.
    unsafe { std::env::set_var(KEY, "before") };

    // Force the lazy capture now, at the "before" value — mirroring
    // `init_with_action`'s own early lazy capture (`tmp_dir_path` discovery /
    // `RUSTTY_LOG` read), both of which run before `ensureLocale`.
    let pre = rustty_global::environ_map();
    assert_eq!(
        pre.get(KEY.as_bytes()).map(Vec::as_slice),
        Some(b"before".as_slice()),
        "setup: the lazy capture must see the pre-mutation value, or the \
         control below is vacuous"
    );

    // Mutate the LIVE process environment without going through
    // `rustty_global` at all — exactly what `ensureLocale`'s
    // `setenv`/`unsetenv` calls do underneath rustty-os.
    unsafe { std::env::set_var(KEY, "after") };

    #[cfg(not(windows))]
    {
        // Without a re-sync, the captured snapshot is stale.
        let stale = rustty_global::environ_map();
        assert_eq!(
            stale.get(KEY.as_bytes()).map(Vec::as_slice),
            Some(b"before".as_slice()),
            "environ_map() must NOT see a live mutation until sync_environ() \
             re-captures — if this fails the snapshot semantics changed and the \
             test below is no longer a meaningful check of sync_environ()"
        );

        rustty_global::sync_environ();

        let synced = rustty_global::environ_map();
        assert_eq!(
            synced.get(KEY.as_bytes()).map(Vec::as_slice),
            Some(b"after".as_slice()),
            "sync_environ() must re-capture the process environment"
        );
    }

    // N352 (task W-5): on Windows the reference's environment is the global
    // block (`.use_global = true`), which `createMap`/`getWindows` read from the
    // PEB on every call, so a mutation is visible at once and `syncEnviron` is a
    // no-op there (`global.zig:307-321`). Names are case-insensitive (N340).
    #[cfg(windows)]
    {
        let live = rustty_global::environ_map();
        assert_eq!(
            live.get(KEY.as_bytes()).map(Vec::as_slice),
            Some(b"after".as_slice()),
            "environ_map() must read the live environment on Windows (N352)"
        );
        let lower = KEY.to_ascii_lowercase();
        assert_eq!(
            rustty_global::environ().get_alloc(lower.as_bytes()),
            Some(b"after".to_vec()),
            "getAlloc must read the live environment, case-insensitively"
        );
        assert!(rustty_global::environ().contains_unempty_constant(lower.as_bytes()));

        // SAFETY: as above, the only thread touching the environment.
        unsafe { std::env::remove_var(KEY) };
        assert!(
            !rustty_global::environ().contains_constant(KEY.as_bytes()),
            "a removal must be visible at once too"
        );
        assert_eq!(rustty_global::environ_map().get(KEY.as_bytes()), None);

        // `syncEnviron` is `.windows => {}`: it changes nothing.
        rustty_global::sync_environ();
        assert_eq!(rustty_global::environ().get_alloc(KEY.as_bytes()), None);
    }
}
