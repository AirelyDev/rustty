# rustty Phase 2 — Batch 3 Report: `rustty-os` integration to verified parity

**Verdict: GREEN.** `rustty-os` is integrated and at verified parity. All 58
ported `test {}` blocks pass natively; every per-crate gate is green; the full
`.port/tools/ci.sh` returns **CI RESULT: PASS** (including G4-uni at 0 diffs).

## Scope

`rustty-os` ports `ghostty/src/os/*.zig` (34 files incl. `wasm/`,
`string_encoding`, `shell`, `xdg`, `path`, `i18n`, ...) plus `pty.zig` (with the
vendored `pty.c` built via `cc`; `os/open` relocation per break #14 is expressed
as a local `Kind` seam, not a `rustty-apprt-api` dep). File-groups A-pure /
B-fs / C-platform / D-rest were already ported in prior commits; this batch is
the **integration step**.

## What this batch did

1. **`cargo build -p rustty-os` GREEN** — no cross-file integration breakage.
2. **`lib.rs` reconstructed the `os/main.zig` public surface.** The committed
   `lib.rs` was a module-skeleton stub (group D explicitly deferred "the
   os/main.zig re-export reconstruction" to integration). Reconstructed every
   `pub const` / `pub fn` re-export from `os/main.zig` lines 22-68
   (`GetEnvResult`, `getEnvMap`, `append/appendAlways/prependEnv`, `getenv`,
   `setenv`, `unsetenv`, `launchedFromDesktop`, `launchedByDbusActivation`,
   `launchedBySystemd`, `desktopEnvironment`, `rlimit`, `fixMaxFiles`,
   `restoreMaxFiles`, `allocTmpDir`, `freeTmpDir`, `randomTmpPath`, `isFlatpak`,
   `FlatpakHostCommand` [behind the off-by-default `flatpak` feature],
   `home`, `expandHome`, `ensureLocale`, `clickInterval`, `open`,
   `OpenType = open::Kind`, `pipe`, `resourcesDir`, `ResourcesDir`,
   `ShellEscapeWriter`, `getKernelInfo`, `CFReleaseThread`, `TempDir`). The
   `pub const args/cgroup/.../macos/windows` namespaces are already provided by
   the `pub mod` declarations; `windows`/`macos` carry their `#![cfg(...)]` os
   gates verbatim in their own files. Set `//! PORT STATUS: done`.
3. **Ported the `os/main.zig` `test {}` aggregator** as `tests::main_test_refs`
   (#[test]). In Zig it forces test analysis of named submodules and carries no
   assertions; in Rust submodules are compiled/tested unconditionally, so it is
   a no-op `#[test]` preserving the 1:1 `test{}` -> `#[test]` mapping (R1). This
   is the 58th test (group D had 57). The conditional `builtin.os.tag` refs are
   reproduced as `cfg!` checks.
4. **Name-set parity (G1 strict).** Renamed 7 ported test fns to forms the G1
   name-matcher accepts as the reference name: `shell::shell_command_builder`
   (`ShellCommandBuilder`), `homedir::expand_home_unix` (`expandHomeUnix`),
   `uri::is_valid_mac_address` (`isValidMacAddress`), `xdg::parse_terminal_exec`
   (`parseTerminalExec`), `hostname::is_valid` (`isValid`),
   `file::random_basename` (`randomBasename`); `mach::VMTag_make` (`VMTag.make`).
   snake_case was chosen where the PascalCase/camelCase form would collide with
   the glob-imported production identifier of the same name.
5. **`cargo fmt`.** The committed sources were not `cargo fmt --check`-clean
   (flatpak/windows/systemd/uri/pipe/locale/macos/...). Applied `cargo fmt`;
   only whitespace/wrapping changed — every comment is preserved verbatim and
   G3b remains `missing=0`.
6. **Fixed a parallel-test environment race.** `cargo test -p rustty-os` alone
   passed, but `cargo test --workspace` (and `RUST_TEST_THREADS>1`) failed in
   `xdg::fallback_when_xdg_env_empty_and_subdir`: the Zig reference runs `test {}`
   blocks **serially**, so env-touching tests never observed each other's
   `setenv`/`unsetenv`/`HOME` mutations, whereas Rust's `libtest` runs tests in
   parallel. Added a **test-only** crate-internal shared lock
   (`ENV_TEST_LOCK` / `env_test_lock()`) acquired by the 6 env-touching tests
   (`desktop::desktop_environment`; `xdg::{config_nonempty,
   fallback_when_xdg_env_empty, fallback_when_xdg_env_empty_and_subdir}`;
   `homedir::{expand_home_unix, home_returns_value}`). This is R1 no-loss glue:
   it alters no reference logic, comment, or assertion — it only restores the
   reference's serial-execution invariant. Verified stable across 8 consecutive
   `RUST_TEST_THREADS=8` runs.

## Gate results (real exit codes)

| Gate | Result |
| --- | --- |
| `cargo build -p rustty-os` | GREEN (exit 0) |
| `cargo test -p rustty-os` | GREEN — **58 passed**, 0 failed, 0 ignored |
| `cargo test --workspace` | GREEN (exit 0) after env-lock fix |
| `cargo clippy -p rustty-os --all-targets -- -D warnings` | GREEN |
| `cargo fmt -p rustty-os -- --check` (workspace `cargo fmt --check`) | GREEN |
| G1 `check_tests_ported.py --strict` | GREEN — 58/58, name-sets match |
| G2 `check_assertions.py --strict` | GREEN — rust=258 >= ref=177 |
| G3 `check_comments_fidelity.py --strict` | GREEN — rust=2140 >= ref=898 |
| G3b `check_comment_coverage.py` | GREEN — distinctive=688, **missing=0**, review=4 |
| G7 `check_acyclic.sh` | GREEN — 0 cross-crate SCCs |
| G6 `check_leaks.sh --crate rustty-os` (Miri) | GREEN — 40 passed, 18 ignored, 0 UB, 0 leaks |
| ASan (gettext FFI) | GREEN — `i18n::canonicalizeLocale_darwin` under `-Zsanitizer=address`, 0 ASan errors |
| **`.port/tools/ci.sh`** | **CI RESULT: PASS** (incl. **G4-uni 0 diffs**) |

## Miri exclusions (18; native `cargo test` still runs them)

Real-OS-I/O / syscall / FFI tests marked `#[cfg_attr(miri, ignore)]` (Miri can't
run real syscalls or foreign C; native run is the correctness oracle):

`args::args`, `desktop::desktop_environment`, `file::random_basename`,
`homedir::expand_home_unix`, `homedir::home_returns_value`,
`hostname::islocal_returns_false_when_hostname_is_not_local`,
`hostname::islocal_returns_true_when_hostname_is_local`,
`i18n::canonicalizeLocale_darwin` (gettext FFI — covered by ASan),
`kernel_info::read_proc_sys_kernel_osrelease`, `macos::cacheDir_paths`,
`passwd::passwd_entry`, `path::expand_does_not_exist`, `path::expand_hostname`,
`pty::pty_open_size_and_name`, `tempdir::tempdir_create_open_delete`,
`xdg::config_nonempty`, `xdg::fallback_when_xdg_env_empty`,
`xdg::fallback_when_xdg_env_empty_and_subdir`.

## Constitution / rules compliance

- **R1:** every line/comment/`test{}` ported; the 58th test (`main.zig`'s
  aggregator) restored; no comment dropped (G3b missing=0); all added code is
  no-loss glue (re-export surface, env-lock).
- **R2:** re-exports retargeted to `rustty` identity only (e.g.
  `OpenType = open::Kind` parity image; resources env var via `rustty_identity`).
- **R3:** no ghostty Cargo dependency; gettext bound via the vendored
  `crates-sys/gettext-sys` (same C lib).
- **R5:** no gate weakened — the env race was fixed by restoring serial
  semantics, not by ignoring/relaxing the test.
- **R6:** right-tool memory gate — Miri on pure-logic tests, ASan on the gettext
  C-FFI test.

**Blockers: none.**
