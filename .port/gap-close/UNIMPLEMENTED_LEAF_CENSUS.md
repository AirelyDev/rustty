# The `unimplemented!()` leaf census — 2026-08-26

The question put to this audit: **does any `unimplemented!()` in the workspace
stand where the reference has real behaviour?** Not "how many are there" — a
count is not an answer, because most of these leaves are load-bearing and
correct. The audit had to sort the deliberate from the accidental.

Oracle: ghostty `88f57ee66` (the `.port/ORACLE_SYNC.md` pin), plus the pinned
`libxev` package under `zig-pkg/`.

## Method

`grep` for `unimplemented!` / `todo!(` across every `*.rs` in the workspace
returns 128 hits. Filtering out lines that are comments or doc text leaves **64
lines**, and one of those is a string literal inside a test assertion
(`rustty/tests/gui_leaf.rs:111`), so **63 live leaves**.

Every comment-only hit was eyeballed rather than assumed — several of them are
prose *about* leaves that no longer exist, which is the failure mode
`stale-deferral-docs-fake-gaps` warns about, and which cost the previous
`renderer.Thread` audit a false finding.

The 63 were then sorted by asking, per site, what the reference does on that
path.

## Result: 62 legitimate, 1 real gap

| category | count | where |
|---|---|---|
| Test doubles inside `#[cfg(test)]` or `tests/` | 46 | `rustty-core` `renderer_seam.rs` (12, in `#[cfg(test)] mod` at :2151), `lib.rs` (10, at :89), `app.rs` (8, at :1164), `tests/surface_visibility_kat.rs` (8); `rustty-apprt` `gtk/class/application.rs` (8, at :5041) |
| Targets libxev itself declares no backend for | 10 | `rustty-xev` `no_backend_seam` — freestanding wasm |
| Platform-unreachable leaves | 3 | `embedded.rs` `renderMetal` (non-Darwin stub; sole caller is `cfg(macos)`), `rustty-vaxis/tty.rs` (`TestTty::next_event`, Windows-API compat; Zig body is `error.SkipZigTest`), `rustty-termio/backend.rs` (`Backend.childExitedAbnormally`) |
| GObject property fallthrough | 2 | `gtk/class/inspector_widget.rs` — the `_ =>` arm for an unknown pspec, idiomatic gtk4-rs |
| Deliberate leaf with a test proving it unreachable | 1 | `main_rustty.rs:308` — a build with no compiled GUI backend; `rustty/tests/gui_leaf.rs` asserts the Gtk arm never reaches it |
| **Real gap — FIXED** | **1** | **`rustty-xev` on iOS/visionOS** |

Two of the "legitimate" rows were checked against the oracle rather than taken
on trust, because their PORT NOTEs made claims that would be easy to get wrong:

* **`Backend.childExitedAbnormally`** claims to be uninstantiated in the
  reference. Confirmed: `termio/backend.zig:88` declares it and calls
  `exec.childExitedAbnormally`, but `termio/Exec.zig` declares no such method,
  and the only call to a `childExitedAbnormally` anywhere is `Surface.zig:1277`
  → `Surface.zig:1330`, a *different, private* function. Zig's lazy compilation
  therefore never type-checks the Backend one. The leaf is correct, and the
  marker fires if a caller ever appears.
* **`renderMetal`'s** non-Darwin arm — sole caller `rustty_inspector_metal_render`
  is itself `#[cfg(target_os = "macos")]`. Unreachable as claimed.

## The one real gap

`libxev`'s `backend.zig:20,38` maps `.ios` and `.visionos` onto `.kqueue`
exactly as it maps `.macos`, and the mach paths in `kqueue.zig` are gated
`isDarwin()`, which covers all three (`kqueue.zig:965, 1081, 1443, 1674`;
`Kevent` at `:1861-1862` switches on `.ios, .macos, .visionos`).

rustty gated its kqueue family on `macos + freebsd` only — while `backend.rs`
already answered `Backend::Kqueue` for iOS. **The crate contradicted itself.** On
iOS, `Backend::default()` named a real backend while `lib.rs` re-exported
`no_backend_seam`, so `Loop`, `Async`, `Timer`, `Process` and `Stream` were every
one an `unimplemented!()` panic — on a target the reference ships for real
(ghostty builds libghostty for iOS).

This was *documented as a deliberate deferral*, in a PORT NOTE at `kqueue.rs`'s
`available()` saying widening "would compile the mach paths on untested
targets". That note was wrong on its own terms, and the ledger already said so:
`DEFERRALS.md`'s 2026-07-31 re-audit had corrected the **identical**
`target_os = "macos"`-vs-`isDarwin()` narrowing in the apprt-c Darwin exports,
ruling that iOS is a real ghostty target and that codegen-without-a-device is
acceptable verification debt. The xev gate was simply missed in that sweep.

### Proven, not argued

The bug was not inferred from reading cfgs. A `compile_error!` was planted
inside `no_backend_seam` and the crate checked for `aarch64-apple-ios`: it
fired, so the seam demonstrably *was* what iOS compiled. After the fix, the same
probe stopped firing.

`aarch64-apple-ios` turns out to be a **tier-2 rustup target**, so this is
build-verifiable rather than paper — which is what made the fix worth doing here
rather than ledgering it as more debt.

### The fix

Every Darwin gate in `rustty-xev` now names the reference's own set
(`macos + ios + visionos`, plus `freebsd` for the kqueue family). That includes
`thread_pool.rs`'s port of Zig's `std.Thread.Futex` `DarwinImpl`, whose
no-threads fallback arm had to stop claiming iOS — caught by the build, not by
inspection.

The two host-test arms in `backend.rs` genuinely mean "the macOS host", and are
marked `GATE-OK(darwin-set):` rather than widened.

### Two defences against re-rot

* **An in-crate `compile_error!` fence inside `no_backend_seam`**, gated
  `not(target_family = "wasm")`. It lives *inside* the module so it inherits the
  module's own cfg — there is no second copy of the target list to drift, which
  was a flaw in the first draft of this fence and is why it moved. Any future
  narrowing is a build error at the first `cargo check` of the affected target,
  instead of a panic on a device.
* **`G-XEV-IOS`** (`.port/tools/gates/xev_ios_kqueue_gate.sh`, wired HARD into
  `ci.sh`): type-checks the crate for iOS, runs
  `check_xev_darwin_set.py` over every cfg attribute in the backend-selection
  files to catch a `macos`-without-`ios`/`visionos` narrowing, and re-runs the
  host suite. Exits 3 BLOCKED — never green — when the iOS rust-std is absent.

Both new checks have their **FAIL arms proven reachable**, per
`derived-gate-green-can-be-bookkeeping`:

* `xev_ios_kqueue_gate.sh --self-test` reintroduces the exact bug (drops
  `target_os = "ios"` from the seam's two cfg blocks), requires the build to go
  red *specifically on the seam fence* — a red build for any other reason is
  reported as a broken self-test, not a pass — then restores and verifies the
  tree is clean.
* `check_xev_darwin_set.py` was confirmed red against a hand-narrowed cfg in
  `kqueue.rs`, and carries a vacuity floor so that a rename or path typo which
  stops it seeing any cfg attributes reports failure rather than a clean bill.

## Verification

| check | result |
|---|---|
| `cargo check -p rustty-xev` × 7 installed targets (macos, ios, linux, freebsd, windows-msvc, wasm32-unknown-unknown, wasm32-wasip1) | all green |
| iOS selects the REAL kqueue backend (`compile_error!` probe in the seam) | fires before the fix, silent after |
| `cargo test -p rustty-xev` | 52 passed / 0 failed |
| `cargo clippy -p rustty-xev --all-targets -- -D warnings` | clean |
| `cargo check --workspace --all-targets` | green |
| `cargo test --workspace` | green, 0 failures |
| `G-XEV-IOS` | GREEN |
| `G-XEV-IOS --self-test` | fence fired as designed; tree restored |
| `check_xev_darwin_set.py` | 127 cfg attributes scanned, 0 narrowings |
| `cargo fmt -p rustty-xev` | clean |

## Limits, stated rather than left implicit

* This **type-checks** iOS. It never links and never runs there; no iOS device
  or simulator lane exists in this repo. The widened mach/kqueue arms are proven
  for iOS up to codegen and no further.
* **`visionos` is tier-3** with no rustup `rust-std`, so that arm is not even
  type-checked. It remains codegen debt, mirroring the reference's own gating.
* A real iOS-simulator test lane is the honest follow-up.

## Note on the workspace test run

`rustty-apprt-c`'s `c_abi_smoke_compile_link_run` fails on a cold `cargo test
--workspace` with "missing cdylib" — it needs `cargo build --workspace` to have
published `librustty_apprt_c.dylib` first. This is a test-ordering artifact, not
a regression, and is unrelated to this branch: it passes on the same tree once
the workspace has been built.
