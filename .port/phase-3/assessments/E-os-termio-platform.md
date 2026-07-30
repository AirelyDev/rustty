# Phase 3 assessment — area E: OS / termio / process platform layer

Range: `4749c4e93..6ad1fe7d8` (365 commits, 250 non-merge).
Scope: `src/os/**`, `src/termio/**`, `src/Command.zig`, `src/Surface.zig`,
`src/global.zig`, `src/pty.zig`, `src/quirks_memset.zig`, plus the top-level
`src/*.zig` not claimed by another area (`App.zig`, `main.zig`,
`main_ghostty.zig`, `main_c.zig`, `main_bench.zig`, `main_gen.zig`,
`tripwire.zig`).

Area diffstat: **34 files, +2,561 / −1,734** (os +636/−927 over 22 files with
2 whole-file deletions; termio +795/−387; Command +215/−92; Surface +174/−144;
global +357/−151; pty +46/−33; quirks_memset +338/−0).

---

## 0. Coverage statement — read this before trusting any row below

**Exhaustively read, line by line (100% of the hunks):**

- `src/termio/Exec.zig` (all 696 changed lines)
- `src/termio/{Termio,Thread,mailbox,message,shell_integration,stream_handler}.zig`
- `src/os/**` — every one of the 22 changed files, including the two full
  deletions (`args.zig` −130, `env.zig` −178)
- `src/Command.zig`, `src/pty.zig`, `src/global.zig`
- `src/quirks_memset.zig` (the whole new 338-line file) and its four build-wiring
  reference sites (`main.zig`, `main_c.zig`, `main_bench.zig`, `main_gen.zig`)
- `src/App.zig`, `src/main.zig`, `src/main_ghostty.zig`, `src/tripwire.zig`

**Sampled, not exhaustive:**

- `src/Surface.zig` — I read the full 904-line diff through a filter that
  suppressed the mechanical `std.Io` mutex/timestamp/`global.io()` churn, then
  read every surviving hunk in full context. The mechanical residue (~85% of
  the file's diff) was verified as mechanical by pattern, not read individually.
  **Risk:** a semantic change disguised inside a `lockUncancelable` hunk would
  be missed. Cost to close: ~30 min of full-diff reading.
- **rustty side:** for each upstream change I grepped for the specific rustty
  symbol and read the matching region. I did **not** read all 40k lines of the
  rustty area files. Absence verdicts are grep-based (the exact greps are cited);
  presence verdicts are read-based with `file:line`.
- `src/main_c.zig` (+52) — read only the `ghostty_init` / `ghostty_cli_try_action`
  / `quirks_memset` regions, not all 52 lines.

**Not run:** no builds, no test runs, no Linux VM check. Justified in §7.

**Not in this area** (flagged for the owning lane): `madvise`/BL-2 lives in
`crates/rustty-vt/src/mem.rs:43-74` (`src/terminal/mem.zig`), area A–D. It is
already hand-rolled `extern "C"` per the convention. I touched nothing there.

---

## 1. Summary — the numbers

The dominant fact about this range in area E is that **~78% of the churn is the
Zig 0.16.0 toolchain migration** (`e8525c0fd` + the eight follow-up fixups), which
is a stdlib-API rewrite with no observable behavior change. Those hunks are `N/A`
for a Rust port and must not be "ported". Once they are set aside, the range
contains **13 genuinely semantic changes** in this area, and rustty is behind on
almost all of them.

| Verdict | Count | Notes |
|---|---|---|
| **ABSENT** | **9** | 1 large (read pipeline), 8 small/medium |
| **PARTIAL** | **3** | `yieldToDemand`, `killPid` rebase, `Command` PATH-exec |
| **PORTED** | **1** | `waitPid` EINTR retry (pre-existing, exact) |
| **N/A — Zig toolchain** | **~19 hunk groups** | Zig 0.16 stdlib migration, `quirks_memset`, `windows.zig`, `pty.zig` Windows arms, `flatpak` `gio_c`, `mach.zig` |
| **N/A — KEEP rustty's version** | **1** | `hostname.isValid` deletion (upstream leaned on Zig stdlib; Rust has no equivalent) |
| **DELETIONS rustty must mirror** | **2 files** | `src/os/args.zig`, `src/os/env.zig` |
| **Divergences to rebase** | **2** | bounded `killPid` (documented), `posix_waitpid` ECHILD (open item — *upstream has now inadvertently fixed this*) |

**Headline:** the single largest gap is the **two-stage pty read pipeline**
(`2f0e6659d` + `bb0ac4c72`), ~350 lines of new concurrent code in
`Exec.ReadThread`, entirely absent. Second is **visibility reporting / DEC mode
2033** (`6c8c07981`), absent across `message.zig` / `Termio.zig` /
`stream_handler.zig` / `Surface.zig`. Third is the **`env.zig` deletion**, which
rustty must mirror and which — because rustty's `env.rs` is `String`-typed on top
of `std::env::vars()` — is currently hiding a **live panic on non-UTF-8
environment variables** (§3.1).

---

## 2. Per-change verdict table

### 2.1 termio — semantic changes

| # | Upstream commit(s) | What changed | rustty status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|---|
| E1 | `2f0e6659d` "termio: pipeline pty reads to overlap parsing with draining"; `bb0ac4c72` "don't bridge pty reads while the parser is idle" | `Exec.ReadThread` split into a **gather stage** (`gatherMainPosix`, new thread `io-gather`) + a **parse stage** (`threadMainPosix`), coupled by a 4-slot × 64 KiB ring (`Pipeline` struct: mutex, `batch_ready`/`slot_free` conditions, `lens`/`head`/`tail`/`count`, `bridging` flag, idle self-pipe, `done`). Adds bridge-spin (`bridge_spin_max = 16`), bridge poll (`bridge_poll_timeout_ms = 1`), saturation threshold (`bridge_threshold = 1024`), latency budget (`gather_budget_ns = 3ms`), and a 3-fd poll set. Buffer grows 1 KiB → 64 KiB. Motivation: macOS kernel tty output queue caps master reads at ~1 KiB, so the old serial loop stalls producers. | **ABSENT** | rustty still has the pre-pipeline single loop: `crates/rustty-termio/src/exec.rs:3323` `ReadThread::threadMainPosix`, `let mut buf = [0u8; 1024];` at `:3411`, one `'tight:` read loop + one 2-fd `poll` at `:3413-3510`. No `Pipeline`, no `io-gather`, no `bridge_*`: `grep -rn "io-gather\|bridge_threshold\|batch_ready" crates/` → 0 hits. | **both** (kernel-queue cap is a macOS symptom; the code is POSIX and runs on Linux too) | Existing KAT `readthread_pty_bytes_land_in_screen` (`exec.rs:4925`) covers the current shape and would still pass after a pipeline port — so it is **not** a regression guard for the pipeline. **None — needs one** (a saturated-producer throughput assertion + a latency assertion for a sub-1 KiB interactive write). | **L** | **HIGH.** New cross-thread ownership protocol. In Rust the ring's "buffer contents need no locking, only ring metadata is guarded" invariant is exactly a Stacked-Borrows hazard (`&mut` to a slot while another thread holds an alias): this needs raw-pointer slot handoff, not `&mut`, per the `rust-mut-args-strongly-protected` rule. Miri harness strongly advised. |
| E2 | `bed47168c` "termio: bound POSIX read-ahead on non-Darwin" → **reverted by** `60121a039` | Non-Darwin read-ahead bound, added and reverted inside the range. | **N/A — net zero** | Net diff of the pair is empty; verified `git log --oneline` ordering (`bed47168c` older, `60121a039` newer). | Linux | n/a | — | **Trap:** anyone porting commit-by-commit rather than from the range diff will port the reverted version. Explicitly do not. |
| E3 | `d34b54e9b` "renderer: hand off state mutex to avoid starving frames" | `io.renderer_state.yieldToDemand()` at each batch boundary in both `threadMainPosix` (pipeline parse loop) and `threadMainWindows`. | **PARTIAL — ported at the correct equivalent site** | `crates/rustty-termio/src/exec.rs:3489-3505`: `unsafe { (*(*io).renderer_state).yield_to_demand(); }`, immediately after `processOutput` returns, with an explicit PORT NOTE stating the pipeline is not ported and that this is where the oracle's `threadMainWindows` hunk (same older loop shape) puts it. Reasoning verified correct: `processOutput` locks *and* unlocks internally, so the mutex is not held at the call. | both | Depends on `renderer::State::lock_demand` tests (area F). **None for the placement itself — needs one.** | **S** (already done) | LOW now; **must be re-sited when E1 lands** (it moves to after the tail advance, before the next `batch_ready` wait). Do not lose it in the E1 rewrite. |
| E4 | `2f0e6659d` (same commit, separable hunk) | `setQosClass()` → `internal_os.macos.setQosClass(.user_initiated)` on **both** pipeline threads (macOS only). Measured 15% throughput delta on M4 Max. | **ABSENT at this call site** (the primitive exists) | Primitive: `crates/rustty-os/src/macos.rs:85` `pub fn setQosClass(class: QosClass)`. Callers: only `crates/rustty-core/src/renderer_seam.rs:986`. `grep -rn "setQosClass" crates/rustty-termio/` → 0 hits. | **macOS only** | **None — needs one** (a `pthread_get_qos_class_np` assertion inside the spawned read thread). | **S** | LOW. Separable from E1 and worth landing independently — it applies to the *current* single read thread too. |
| E5 | `6c8c07981` "terminal: add visibility reports" | New DEC mode 2033 reporting end to end: `termio/message.zig` `visibility_report{visible,force}`; `Termio.visibilityReport()` using `device_status.encodeVisibilityReport` + `max_visibility_report_encode_size`; `Thread.drainMailbox` arm; `stream_handler` `.report_visibility` mode-set arm (`force = true`) and `.visibility` DSR arm; `Surface.visible: bool = true` field with dedup, `terminal.flags.visible` write under the renderer lock, and an unforced `visibility_report` from `occlusionCallback`. | **ABSENT (all five sites)** | `grep -rn "visibility_report\|report_visibility" crates/ --include="*.rs"` → **0 hits**. `crates/rustty-core/src/surface.rs:7303` `occlusionCallback` still only pushes `rendererpkg::Message::Visible` + `queueRender` — no `visible` field, no dedup, no terminal flag, no IO message. `crates/rustty-termio/src/message.rs` has no `VisibilityReport` variant. | both | **None — needs one** (a KAT: enable 2033, drive `occlusionCallback(false)`, assert the encoded report reaches the pty write queue; plus a dedup assertion that a repeat call emits nothing). | **M** | MEDIUM. Blocked on area A–D for `terminal.flags.visible`, `modes.report_visibility`, and `device_status::encodeVisibilityReport`. The `Surface` dedup (`if (self.visible == visible) return;`) also changes existing behavior: repeated same-state occlusion callbacks no longer push a renderer message. That is an **observable change to the renderer path**, not only new reporting — do not treat it as additive-only. |
| E6 | `f00e90694` "lib-vt: add color scheme report encoder" | `Termio.colorSchemeReportLocked` stops hand-writing `"\x1B[?997;1n"`/`;2n` and calls `terminalpkg.device_status.encodeColorSchemeReport(&writer, scheme)` into a `max_color_scheme_report_encode_size` buffer. | **ABSENT (termio call site)** | `grep -rn "encode_color_scheme_report\|encodeColorSchemeReport" crates/` → 0 hits. rustty still hand-writes the literals in `termio_impl.rs`. | both | Any existing color-scheme-report KAT will pass either way (same bytes emitted). **None — needs one at the encoder level** (area A–D). | **S** | LOW. Pure refactor, byte-identical output. Blocked on area A–D landing `device_status` encoders. Land with E5 (same encoder module, same `max_*_encode_size` convention). |
| E7 | `c594031d5` "terminal: move cursor defaults into terminal state" | `StreamHandler` loses `default_cursor: bool`, `default_cursor_style`, `default_cursor_blink` and its whole `setCursorStyle` function (−54 lines). Cursor defaults move into `Terminal` (`terminal.setDefaultCursorStyle/Blink`, `terminal.cursor.default_blink`, `terminal.setCursorStyle`). `Termio.init` passes `default_cursor_style`/`default_cursor_blink` into `Terminal.init` instead of poking `screens.active.cursor.cursor_style` afterward. `changeConfig` calls the setters. | **ABSENT** | rustty carries the old shape verbatim: `crates/rustty-termio/src/stream_handler.rs:123-127` (`default_cursor`, `default_cursor_style`, `default_cursor_blink` fields), `:1310` `pub fn setCursorStyle`, `:938` `self.default_cursor_blink.is_some()`. `termio_impl.rs:575` still does `(*term.screens.active).cursor.cursor_style = opts.config.cursor_style`. | both | Existing DECSCUSR / `cursor_style` KATs guard the *behavior*; nothing guards the *ownership*. **None needed for behavior; the move is guarded by area A–D's terminal-state tests.** | **M** | MEDIUM. Blocked on area A–D. The mode-2031 `cursor_blinking` guard at `stream_handler.rs:938` reads a field that moves — a mechanical port that forgets to re-point it silently breaks the "shell set an explicit blink, ignore mode changes" rule. |
| E8 | `89b103dd5` "terminal: more full featured resize (cell geo, sync rendering, etc.)" | `Termio.resize` now calls `terminal.resize(alloc, .{cols, rows, cell_size_px})` and **deletes** its own `width_px`/`height_px` assignment and its `modes.set(.synchronized_output, false)` — both move inside `Terminal.resize`. `stream_handler`'s mode-2048 resize likewise switches to the struct form (no cell size). | **ABSENT** | `crates/rustty-termio/src/termio_impl.rs:1061` `pub fn resize(&mut self, td, size)` still assigns `self.terminal.width_px` at `:1092` and `modes.set(synchronized_output, false)` at `:1098`. `grep -n "cell_size_px" crates/rustty-termio/` → 0 hits. | both | Existing resize KATs. **Needs one** asserting `width_px` and the synchronized-output clear happen exactly once (a naive port that keeps both call sites double-clears — harmless — but a port that removes both *without* the terminal-side move loses them entirely). | **M** | MEDIUM. Blocked on area A–D (`Terminal.resize` signature). Ordering: A–D must land first, or rustty loses `width_px` updates. |
| E9 | `86f81fb5b`, `65c48213b`, `1092204df` (scrollback limit series) | `Termio.init` replaces `max_scrollback = full_config."scrollback-limit"` with `max_scrollback_bytes = full_config."scrollback-limit-bytes".optional()` **and** `max_scrollback_lines = full_config."scrollback-limit-lines".optional()`; `null` means unlimited. | **ABSENT** | `crates/rustty-termio/src/termio_impl.rs:533` `max_scrollback: opts.full_config.scrollback_limit`. `grep -rn "scrollback_limit_bytes\|scrollback_limit_lines" crates/` → 0 hits. | both | **None — needs one** at the config→termio boundary. | **S** at this call site | LOW here; the work is in area G (`config/limit.zig`, new file, +115) and A–D (`PageList`). This row is the *call site* only. |
| E10 | `f6f79acce` "dispatch APC string bytes in bulk slices"; `47e26df60` "batch printed codepoint runs into direct row fills" | Two new `stream_handler` action arms: `.print_slice => self.terminal.printSlice(value.cps)` and `.apc_put_slice => self.apc.feedSlice(self.alloc, value.bytes)`. Both are `@branchHint(.likely)`-class hot paths. | **ABSENT** | `grep -rn "print_slice\|PrintSlice\|apc_put_slice\|feed_slice" crates/ --include="*.rs"` → 0 hits. | both | Existing per-codepoint print KATs stay valid (the slice arm must be *equivalent*). **Needs one** asserting a multi-codepoint run through `print_slice` matches the same bytes fed one at a time. | **S** at this call site | LOW here. Blocked on area A–D (`stream.zig` must emit the slice actions, `Terminal.printSlice`, `apc.feedSlice`). This is a pure-perf change; correctness risk is entirely on the A–D side. |
| E11 | `39ae85f04` "lib-vt: handle DECRQSS" | `stream_handler`'s 60-line inline DECRQSS response builder is deleted in favour of `decrqss.encode(self.terminal, &response)` into a `terminal.dcs.Command.DECRQSS.max_response_bytes` buffer. | **ABSENT (call site)** | `crates/rustty-termio/src/stream_handler.rs:540-660` still holds the full inline builder (`Decrqss::Sgr` / `Decscusr` / `Decstbm` / `Decslrm` arms with hand-rolled prefix-offset arithmetic). `grep -rn "max_response_bytes" crates/` → 0 hits. | both | Existing DECRQSS KATs guard the bytes; they will pass either way. | **S** at this call site | LOW. Pure refactor. Blocked on area A–D landing `dcs::Decrqss::encode`. |
| E12 | `e8525c0fd` (separable hunk) | `Termio.init`'s `kitty_image_loading_limits = .all` becomes `.allWithTempDir(global.tmpDirPath())`; `changeConfig` likewise. The kitty file-transmission temp-dir allowlist now derives from the discovered `TMPDIR` instead of a hardcoded set. | **ABSENT** | `crates/rustty-termio/src/termio_impl.rs:559` `kitty_image_loading_limits: Limits::ALL`. `grep -rn "all_with_temp_dir\|allWithTempDir" crates/` → 0 hits. | both | **None — needs one.** | **S** at this call site | **MEDIUM despite the size** — this is a *security-relevant* allowlist (which directories a remote-controlled kitty `f=` transmission may read from). Blocked on area A–D (`graphics_image.zig`) and on rustty gaining a `tmpDirPath()` global (E20). Do not stub it as "allow everything". |
| E13 | `e8525c0fd` (separable hunk) | `internal_os.{appendEnv,appendEnvAlways}` are **moved into `termio/Exec.zig`** as file-private fns; `internal_os.prependEnv` is **moved into `termio/shell_integration.zig`**. This is the migration path for the `os/env.zig` deletion. | **ABSENT (still in `rustty-os`)** | Definitions: `crates/rustty-os/src/env.rs:54,70,77`; re-exported at `crates/rustty-os/src/lib.rs:126,127,130`. Consumers: `crates/rustty-termio/src/exec.rs:1923,1968,1996` and `crates/rustty-termio/src/shell_integration.rs:609`. | both | rustty already has the `appendEnv`/`prependEnv` unit tests (ported from the deleted `env.zig`); they must **move with the functions**, not be deleted. | **S** | LOW, but see §4 — this is a prerequisite for deleting `env.rs`. |
| E14 | `e8525c0fd` (separable hunk) | `shell_integration.setupBash` now needs an env map to find `HOME`: `var environ_map = try global.environMap(); defer environ_map.deinit(); homedir.home(&environ_map, &home_buf)`. Also `env.remove` → `env.orderedRemove`/`env.swapRemove` (EnvMap API change, order-observable for the `swapRemove` case). | **ABSENT** | `crates/rustty-termio/src/shell_integration.rs` uses `homedir::home(&mut buf)` (no map). `crates/rustty-os/src/homedir.rs:25` `pub fn home(buf: &mut [u8])`. | both | Existing `bash: HISTFILE` KAT. | **S** | LOW on its own; part of the E15 env-map thread. Note `swapRemove` vs `orderedRemove`: upstream uses `swapRemove` for `GHOSTTY_BASH_ENV` and `orderedRemove` for `VTE_VERSION`/`LD_LIBRARY_PATH`. If rustty's EnvMap is order-preserving, using an unordered remove for the bash case is a fidelity nit worth matching but not a behavior change (the map is only iterated to build `envp`, where order is unspecified). |

### 2.2 `src/os` — semantic changes

| # | Upstream commit(s) | What changed | rustty status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|---|
| E15 | `e8525c0fd` + `f8e13f31e` "Fix desktop detection tests when running from Gnome" | **Environment is captured once at global init and threaded explicitly.** `desktopEnvironment(environ_map)`, `homedir.home(environ_map, buf)`, `homedir.expandHome(environ_map, path, buf)`, `xdg.{config,cache,state}(alloc, environ_map, opts)`, `path.expand(io, alloc, environ_map, cmd)`, `file.allocTmpDir(alloc, environ)` all gain an explicit env parameter. Every test that previously called `setenv`/`unsetenv` on the **process** environment now builds a local `Environ.Map`. `desktop.launchedFromDesktop` and `systemd.launchedBySystemd` read `global.environ().getPosix(...)`. | **ABSENT** | Signatures unchanged in rustty: `crates/rustty-os/src/desktop.rs:118` `desktopEnvironment()`, `homedir.rs:25` `home(buf)`, `homedir.rs:228` `expandHome(path, buf)`, `path.rs:27` `expand(cmd)`, `file.rs:108` `allocTmpDir()`, `xdg.rs` `config(opts)`. Tests still mutate the process env: `desktop.rs:211-234` (10 `setenv`/`unsetenv` calls), `xdg.rs:302,334,357,389,435,438`. | **both** (the reading code is cross-platform; the *test* races are worst on Linux where the `desktop` arms are compiled) | rustty's own `desktop`/`xdg`/`homedir` tests are the ones at risk — they are the guard **and** the hazard. **Needs a gate** once ported: a grep gate banning `setenv`/`unsetenv` in `#[cfg(test)]` code under `crates/rustty-os/`. | **M** (touches 6 signatures + ~8 call-site crates) | **MEDIUM-HIGH, and this is the highest-value non-pipeline port in the area.** Two independent reasons: (a) `crates/rustty-os/src/desktop.rs:220` and `crates/rustty-os/src/xdg.rs:302` call `setenv` from `#[cfg(test)]` code with **no serialization** — `setenv` is not thread-safe against a concurrent `getenv` in glibc or Apple libc, and `cargo test` runs these on parallel threads by default. This is precisely the failure class recorded in the `parallel-cargo-test-exposes-c-init-races` memory. (b) Rust 2024 makes `std::env::set_var` `unsafe` for exactly this reason; the upstream design removes the need entirely. Porting E15 deletes the hazard rather than papering over it with a mutex. |
| E16 | `e8525c0fd`, `a77c706a1`, `7121ab6c3` "global: state should default to null", `048619a6b` "global: take minimal instead of juicy main" | `global.zig` restructured: `pub var state: GlobalState = undefined` → **`var state: ?GlobalState = null`** (private) with free-function accessors `init/deinit/io/alloc/environ/environMap/syncEnviron/args/tmpDirPath/resourcesDir/rlimits/logging/action`, each of which returns a testing fallback when `builtin.is_test`. New `InitOpts` union with three arms: `main` / **`tool`** (new: `ghostty-bench`, `ghostty-gen` — skips app-action detection so `+terminal-stream` doesn't fail with `InvalidAction`) / `c` (argc/argv/environ from the C caller; **returns `error.UnsupportedOSForCApi` on Windows**). New fields `environ`, `args`, `tmp_dir_path`, `io_impl`. New comptime guard: importing `global.zig` from the `lib` (libghostty-vt) artifact is a **compile error**. | **PARTIAL** | rustty already has the *shape* upstream moved to: `crates/rustty-global/src/lib.rs:68` `pub static STATE: OnceLock<GlobalState>` (a closer match to `?GlobalState = null` than the old `undefined`), and `crates/rustty-apprt-c/src/lib.rs:239` `rustty_init(argc, argv)` already stores argv in an `ARGV` `OnceLock` (`:244-263`). **Missing:** the `environ` / `tmp_dir_path` fields (`crates/rustty-global/src/lib.rs:81-104` has only `gpa`, `alloc`, `action`, `logging`, `rlimits`, `resources_dir`), the `tool` arm, the `environMap()`/`environ()`/`tmpDirPath()` accessors, and the libghostty-vt import guard. `io_impl` is **N/A** (Zig `std.Io`). | both | `crates/rustty-global` has the `GlobalState`/`ResourceLimits` unit tests. **Needs one** for `environMap()`/`tmpDirPath()` once added. | **M** | MEDIUM. `environ` + `tmpDirPath` are prerequisites for E12 and E15. The **`tool` arm is N/A for rustty** — verified: `crates/rustty-bench/src/main_bench.rs` never initializes global state and `crates/rustty-bench/src/cli.rs:46` owns its own action namespace, so the `InvalidAction` failure upstream fixed cannot occur (see §7 Q4). The libghostty-vt import guard maps to a Cargo dependency-edge gate (`.port/tools/gates/check_acyclic.sh` territory), not a comptime block. |
| E17 | `e663d5405` "os/hostname: switch to std.Io.net.HostName.validate" | `os/hostname.zig`'s hand-rolled RFC 1123 `isValid` (+ its 25-assertion test) is **deleted**; the one caller (`cli/ssh-cache/DiskCache.zig`) switches to `std.Io.net.HostName.validate`. | **N/A — KEEP rustty's version; do NOT mirror this deletion** | `crates/rustty-os/src/hostname.rs:56` `pub fn isValid(hostname: &[u8]) -> bool` (correctly byte-typed), tests at `:207-240`. One caller: `crates/rustty-cli/src/ssh_cache/disk_cache.rs:637`. | both | rustty's `hostname::isValid` test block **is** the guard — and after this commit it is *stronger* than upstream's, which now has none. | **0** (do nothing) | LOW. Rust's std has no RFC 1123 hostname validator; deleting `isValid` would silently drop validation. Record the intentional keep so a future mechanical sweep does not delete it. |
| E18 | `e8525c0fd` | `os/locale.zig`: `ensureLocale(alloc)` → `ensureLocale()` (no allocator), **and two new invariants**: `assert(!builtin.is_test)` ("does in-process mutation of the environment and can't be run in a test") and a **new Windows early-return arm** that logs `"TODO: setlocale failed on Windows, implement better fallbacks"` and returns instead of falling through. `setenv`/`unsetenv` become file-local `extern "c"` decls (the `os/env.zig` versions are gone). | **PARTIAL** | `crates/rustty-os/src/locale.rs:27` `pub fn ensureLocale() -> std::io::Result<()>` — allocator already dropped (Rust-natural), so that part matches. **Missing:** the `!is_test` assertion (`grep -n "is_test\|cfg(test)" crates/rustty-os/src/locale.rs` → 0 hits) and the Windows arm (`grep -n "windows" crates/rustty-os/src/locale.rs` → 0 hits). `locale.rs:34` still calls `internal_os::getenv("LANG")` (the `env.rs` API that must die — §4). | Windows arm: **Windows only**. `!is_test` assert: **both**. | **None — needs one:** a `#[cfg(test)] #[should_panic]` test that `ensureLocale()` refuses to run under test. | **S** | LOW. The `!is_test` invariant is worth porting exactly (per the `tripwire` convention, `debug_assertions`-gated); it documents that this function mutates process-global state and prevents someone wiring it into a `#[test]`. The Windows arm is a stub that only matters if/when rustty targets Windows. |
| E19 | `e8525c0fd`, `f8e13f31e` | `os/resourcesdir.zig`: `app()`/`host()` take `*const ResourcesDir` (were `*ResourcesDir`); the debug `GHOSTTY_RESOURCES_DIR` fallback now also swallows `error.InvalidWtf8`; env reads go through `global.environ().getAlloc`. | **PORTED (the `*const` part) / ABSENT (the rest)** | `crates/rustty-os/src/resourcesdir.rs:39` `pub fn app(&self)`, `:49` `pub fn host(&self)` — already `&self`, which *is* upstream's new shape. Env reads still `std::env`-based. | both | Existing `resourcesDir` tests. | **S** | LOW. The `&self` change was already right; the env part folds into E15. |
| E20 | `e8525c0fd` | `os/file.zig`: `allocTmpDir(alloc, environ)` takes the captured environ; `randomTmpPath` **stops calling `allocTmpDir`** and uses the cached `global.tmpDirPath()`; `randomBasename` uses `global.io().random(&rand_buf)` instead of `std.crypto.random.bytes`. `os/main.zig` **stops re-exporting** `allocTmpDir`/`freeTmpDir` (only `global.zig` imports them now). | **ABSENT** | `crates/rustty-os/src/file.rs:108` `allocTmpDir()`, `:221` `randomTmpPath` calls `allocTmpDir()` + `freeTmpDir()` per call, `:206` `crypto_random_bytes(&mut rand_buf)`. Re-exports still present at `crates/rustty-os/src/lib.rs:135,139`. | both | **None — needs one** for the cached-tmpdir path. | **S** | LOW. `crypto_random_bytes` → an `io.random` equivalent is a **non-change for Rust**: keep the CSPRNG. Do not weaken it to a non-cryptographic RNG for "fidelity" — the Zig change is a plumbing artifact of `std.Io`, and the basename is used for temp-path unguessability. The removal of the `os/main.zig` re-exports is a mechanical `pub use` cleanup. |
| E21 | `e8525c0fd` | `os/open.zig`: `open(alloc, kind, url)` → `open(kind, url)`; `std.process.Child` → `std.process.SpawnOptions`; the snap `LD_LIBRARY_PATH` strip now uses `global.environMap()` + `orderedRemove`, with a new comment noting `spawn` copies the map so it may be freed immediately after. | **PORTED-equivalent** | `crates/rustty-os/src/open.rs:62` `pub fn open(kind: Kind, url: &[u8])` — already no allocator, already `&[u8]` (not `String`). The snap arm is build-config-gated. | Linux (snap arm), both (signature) | Existing `open` tests. | **0–S** | LOW. Only the snap env-strip detail remains, and it folds into E15. |
| E22 | `e8525c0fd` | `os/systemd.zig`: `std.os.linux.E.init(rc)` replaced by a local `linuxErrnoFromSyscall(r)` helper (identical semantics: `-4096 < signed_r < 0` → `-signed_r`, else 0); `notify.reloading` uses `Timestamp.now().toMicroseconds()` instead of `clock_gettime(.MONOTONIC)` + manual arithmetic, and **stops logging on clock failure** (it can no longer fail). Env reads via `global.environ().getPosix`. | **N/A (mechanical) + trivial ABSENT** | `crates/rustty-os/src/systemd.rs` — rustty uses `libc` errno directly, which is the same thing. The `reloading` timestamp is a `MONOTONIC` read either way. | **Linux only** | Existing systemd tests. | **0** | LOW. Nothing to do. Recorded so the row is not mistaken for an unexamined file. |
| E23 | `e8525c0fd` | `os/kernel_info.zig`: `file.readToEndAlloc(alloc, 128)` → a stack `[128]u8` + `readPositionalAll`, removing one heap round-trip. `os/cgroup.zig`: `trimRight` → `trimEnd` (rename only). `os/passwd.zig`: `trimRight` → `trimEnd`, `posix.close` → `compat_fd.close`. | **N/A** | Rust equivalents already stack/`Vec`-based; `crates/rustty-os/src/kernel_info.rs` (65 lines), `cgroup.rs` (48), `passwd.rs` (251). | Linux (kernel_info, cgroup), both (passwd) | Existing tests. | **0** | NONE. |
| E24 | `e8525c0fd` | `os/mach.zig`: `std.heap.next_mmap_addr_hint` was removed from the Zig stdlib, so `TaggedPageAllocator` keeps its own `var next_mmap_addr_hint`; `mmap` prot flags become a struct literal. | **N/A — Zig stdlib artifact** | `crates/rustty-os/src/mach.rs` (276 lines) + `page_allocator.rs` (494) already own their allocator state; Rust has no `std.heap` hint global. | macOS only | Existing `mach`/`page_allocator` tests. | **0** | NONE. |
| E25 | `e8525c0fd` | `os/windows.zig` **rewritten** (+344/−?): stdlib re-exports replaced by locally-declared constants and `extern` decls (Zig is removing `std.os.windows`); `exp.kernel32` gains `SetHandleInformation`, `CreateFileW`, `CreateNamedPipeW`, `CloseHandle`, `VirtualAlloc/Free`, `WaitForSingleObject`, `GetExitCodeProcess`, `TerminateProcess`, `CancelIoEx`, `ReadFile`; new `exp.ntdll.NtCreateFile`; `FALSE`/`TRUE` become `BOOL` values (so every `== 0` becomes `== windows.FALSE`); `HPCON`/`STARTUPINFOEX`/`PROC_THREAD_ATTRIBUTE_*` hoisted out of `exp`. | **N/A — Windows only, macOS-first port** | `crates/rustty-os/src/windows.rs` (308 lines) already declares its own `extern` set (Rust has no `std.os.windows` to lean on), so rustty is *already* at the destination shape. | **Windows only** | n/a | **0** | NONE, provided rustty's windows arms stay `cfg(windows)`-gated and unbuilt. |
| E26 | `e8525c0fd` | `os/flatpak.zig`: `@cImport("gio/gio.h")` → `@import("gio_c")` (a build-graph translate-C module); `std.Thread.Mutex/Condition` → `std.Io.Mutex/Condition`. No logic change. | **N/A — build-system** | `crates/rustty-os/src/flatpak.rs` (798 lines) uses its own bindings + `std::sync`. | Linux only | Existing flatpak tests (build-config-gated off). | **0** | NONE. |
| E27 | `e8525c0fd` | `os/TempDir.zig`, `os/cf_release_thread.zig`, `os/pipe.zig` (`compat_fd.pipe2`), `os/desktop.zig` (`@cImport unistd.h` kept) — all `std.Io` mechanical. | **N/A** | `crates/rustty-os/src/{tempdir,cf_release_thread,pipe}.rs`. | both | Existing tests. | **0** | NONE. |
| E28 | `e8525c0fd` typo | `src/os/main.zig` doc comment now reads `"...low-level operations, butos/main.zig"` — an upstream editing accident. | **N/A** | — | — | — | **0** | NONE. Do **not** reproduce the typo; the `strip-ghostty-gag-prose` / prose-fidelity carve-out covers obvious upstream mistakes. |

### 2.3 `src/Command.zig`, `src/pty.zig`

| # | Upstream commit(s) | What changed | rustty status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|---|
| E29 | `e8525c0fd` + `a77c706a1` "fix process and global error handling" | `posix.execvpeZ` was removed from Zig std, so `Command.startPosix` **inlines the PATH search**: direct `execve` if the path contains `/`; otherwise tokenize `PATH` on `':'`, build `dir/file`, `execve`, and continue on `NOENT`/`NOTDIR`, latch `seen_eacces` on `ACCES`, bail with `NAMETOOLONG` if the buffer is too small; final result is `ACCES` if any was seen, else the last errno. `PATH` default `"/usr/local/bin:/bin/:/usr/bin"`, read from `global.environ().getPosix("PATH")`. | **PARTIAL — already byte-equivalent, one detail off** | `crates/rustty-termio/src/command.rs:1245` `fn execvpeZ` is the same algorithm, same PATH default (`:1261` `b"/usr/local/bin:/bin/:/usr/bin"`), same error precedence, same `PATH_MAX` buffer, called from `:484`. **Difference:** rustty's `let err = ExecveError::FileNotFound;` (`:1270`) is never reassigned inside the loop, whereas upstream's `err` is updated each iteration — so if the final PATH entry yields `ENOTDIR`, upstream reports `NOTDIR` and rustty reports `NOENT`. | both (POSIX) | `crates/rustty-termio/src/command.rs` has `posix fork handles execveZ failure`-class tests; the `err`-latching nit is unguarded. | **S** | LOW. The nit is observable only in the child's stderr message text (`NOENT` prints the friendly "not on the PATH" message; anything else prints "unexpected error: E<name>"). Worth a 2-line fix for exactness. rustty reads `PATH` via `libc::getenv` rather than a captured environ — equivalent, and correct in a forked child. |
| E30 | `a77c706a1` | New `fn fork()` and `fn waitPid(pid, status, flags)` wrappers restoring the error handling the removed `std.posix` wrappers gave: `fork` maps `AGAIN`/`NOMEM` → `error.SystemResources`; `waitPid` **retries `INTR`** before reading status, and maps `CHILD` → `error.NoChildProcess`, `INVAL` → `error.InvalidWaitOptions`. `Command.wait` restructured around it. `setupFd` gains a `PosixCall.f` retry helper. | **PORTED (EINTR retry) / PARTIAL (error mapping)** | `crates/rustty-termio/src/command.rs:1133` `posix_waitpid` already loops on `EINTR` (`:1144`). **But** it maps every other errno to `unreachable!("waitpid: unexpected errno")` (`:1147`) — no `NoChildProcess`/`InvalidWaitOptions` channel. See §5.2: upstream's change to `killPid` means this now matters. | both (POSIX) | `subprocess_start_spawns_and_reaps` (`exec.rs:4387`). | **S–M** | **MEDIUM** — this is where the `posix_waitpid` ECHILD open item lives, and upstream just changed the calculus. See §5.2. |
| E31 | `e8525c0fd` | `Command.cwd` becomes `?[:0]const u8` and `posix.chdir(cwd)` becomes a raw `posix.system.chdir(cwd)` whose failure is ignored (the comment moves above the call). `pid` becomes `posix.system.pid_t`. | **N/A** | rustty already uses `libc::chdir` with a `CString` cwd. | POSIX | — | **0** | NONE. |
| E32 | `e8525c0fd` | `src/pty.zig`: every `posix.fcntl`/`posix.close`/`windows.*` call rewritten against raw syscalls / relocated Windows decls; `linux.E.init(rc)` → bare `switch (rc) { 0 => …}` in `tcgetpgrp`. No behavior change. **Upstream bug introduced:** `@tagName(posix.errno(-1))` at two sites reports the errno of the literal `-1` rather than the real `errno` (cosmetic, log text only). | **N/A** | `crates/rustty-os/src/pty.rs` (743 lines) already uses `libc` directly. | both (Windows arms Windows-only) | Existing pty tests. | **0** | NONE. Do not reproduce the `errno(-1)` bug. |

### 2.4 `src/Surface.zig`, `src/global.zig`, `src/quirks_memset.zig`, top-level

| # | Upstream commit(s) | What changed | rustty status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|---|
| E33 | `6c8c07981` | `Surface.visible: bool = true` + the `occlusionCallback` rewrite. **See E5** — same commit, counted once there. | **ABSENT** | `crates/rustty-core/src/surface.rs:7303`. | both | see E5 | see E5 | see E5 |
| E34 | `20c3eae04` "memset should match the C ABI" + `e8525c0fd` | **NEW FILE `src/quirks_memset.zig` (338 lines).** Exports a strong `memset` symbol (hidden visibility) that overrides compiler_rt's, because **Zig 0.16.0 globally disabled LLVM loop auto-vectorization** (LLVM 21 miscompile llvm/llvm-project#186922), turning compiler_rt's byte-loop `memset` into a ~24× slower scalar loop — 63% of executed instructions in `+terminal-stream`, 2.8× total slowdown. Manually `@Vector`-ized with an aarch64 `dc zva` fast path (≥16 KiB, zero fill, 64-byte block), `asm volatile ("")` barriers to defeat `LoopIdiomRecognize` recursion, and linkage selected by `builtin.output_mode`/`link_mode`. Wired by `comptime { _ = @import("quirks_memset.zig"); }` in **four artifact roots**: `main.zig` (and its `test` block), `main_c.zig`, `main_bench.zig`, `main_gen.zig`. Ships 3 tests. | **N/A — Zig-toolchain-specific, do NOT port** | Justification, not a grep: (a) Rust does not link compiler_rt's `memset` — `rustc` lowers `@llvm.memset` to the **platform libc** `memset`, which on macOS is Apple's hand-tuned `libSystem` implementation (already SIMD + `dc zva`) and on Linux glibc's IFUNC-dispatched one. (b) Rust does not disable LLVM loop vectorization; the 0.16 workaround has no analogue. (c) Exporting a strong `memset` from a Rust dylib would *pessimize* rustty relative to libc and risk ODR/interposition breakage. There is nothing to be gained and a real hazard in porting it. | both (the `dc zva` path is aarch64) | The perf baseline is the real guard: `perf-baseline-rust-vs-zig` (14/14 wins, suite −21.7%) was measured with rustty using libc `memset`. **Needs a note, not a test.** | **0** (deliberate non-port) | **NONE for rustty — but a MEASUREMENT hazard.** This commit makes the Zig oracle *faster* than it was at the pin. Any future rustty-vs-Zig benchmark run against `origin/main` will show a smaller rustty margin than the recorded 14/14 baseline, and that is the oracle getting faster, **not** rustty regressing. Record this in the perf baseline doc before anyone re-runs the comparison and panics. Also note the oracle's build now requires `-Doptimize=ReleaseFast` (already recorded in the perf memory). |
| E35 | `e8525c0fd` | `src/main.zig`: `std.testing.log_level = .err` in the root `test` block, because "Zig 0.16.0 has made test logging more strict — anything printed to stderr results in a failed-command message". | **N/A** | Rust's `cargo test` captures stdout/stderr per test by default. | both | — | **0** | NONE. |
| E36 | `e8525c0fd` | `src/main_ghostty.zig`: `main()` now takes `std.process.Init.Minimal`; init-failure reporting uses `std.Io.Threaded.global_single_threaded.io()` because "global I/O has already been torn down by that point" (`a77c706a1`); `posix.exit` → `std.process.exit`; new `std_options.allow_stack_tracing = false` for **non-macOS Darwin (iOS)** because Zig switched to `_dyld_get_image_header_containing_address`, unavailable there. | **N/A** (all four) | Rust has no `std.Io` provider to tear down; `std::process::exit`; Rust backtraces do not use the Zig unwinder. | iOS arm: iOS only | — | **0** | NONE. The iOS stack-tracing note is worth remembering **if** rustty ever targets iOS, but it is a Zig-unwinder issue with no Rust counterpart. |
| E37 | `e8525c0fd` | `src/App.zig`: `last_notification_time` → `std.Io.Timestamp`; `mailbox.pop/push(global.io(), …)`; `.{}` → `.empty`. | **N/A** | Mechanical. | both | — | **0** | NONE. |
| E38 | `e8525c0fd` | `src/tripwire.zig`: **doc comment only** (`std.fs.cwd().openFile` → `std.Io.Dir.cwd().openFile(io, …)` in the usage example). The `enabled()` = `builtin.is_test` semantics are **unchanged**. | **N/A — and this is load-bearing news** | Confirms the `rustty_util::tripwire` convention (fail-points, `enabled()` = `cfg!(debug_assertions)`) needs **no rebase** for this range. `grep -n` on the diff shows the only changed line is inside a `//!` block. | both | Existing tripwire tests. | **0** | NONE. Recorded because the assessment brief specifically asked me to judge upstream's error-injection tests against the tripwire convention: **there are none new in this range in area E**, and the convention itself did not move. |

---

## 3. Partials and pre-existing defects, with the exact missing hunks

### 3.1 `crates/rustty-os/src/env.rs` — a live non-UTF-8 panic (found while assessing the deletion)

This is not an upstream change; it is a defect this assessment surfaced, and the
`env.zig` deletion (§4) is the natural occasion to fix it.

`crates/rustty-os/src/env.rs:43`:

```rust
pub fn getEnvMap() -> HashMap<String, String> {
    if is_flatpak() { HashMap::new() } else { std::env::vars().collect() }
}
```

`std::env::vars()` **panics** if any variable is not valid UTF-8 (its docs say so
explicitly; `vars_os()` is the non-panicking form). On POSIX, environment
variables are arbitrary byte strings. `getEnvMap()` is called on the surface
creation path (`crates/rustty-core/src/surface.rs:2087`,
`crates/rustty-apprt/src/embedded.rs:3018`,
`crates/rustty-apprt/src/gtk/class/surface.rs:2361`), so a single non-UTF-8 env
var in the user's shell makes **every new surface panic**.

The oracle at the pin returned `std.process.EnvMap` over raw byte slices and had
no such failure mode. This is a `String`-translation bug of exactly the class the
`zig-byteslice-to-rust-rawbytes` rule exists to prevent.

Secondary, same file:

- `env.rs:117` `getenv` uses `std::env::var`, which maps a non-UTF-8 value to
  `None`. That matches Zig's **Windows** branch (`InvalidWtf8` → null) but **not**
  the POSIX branch, where `posix.getenv` returns the raw bytes. A non-UTF-8
  `$PAGER`/`$LANG`/`$GHOSTTY_LOG` is silently treated as unset.
- `env.rs:54,70,77` `appendEnv`/`appendEnvAlways`/`prependEnv` take and return
  `&str`/`String`. PATH-like variables are byte strings; a non-UTF-8 `PATH`
  component cannot round-trip. These are the functions upstream **moves into
  termio** (E13) — move them as `&[u8]`/`Vec<u8>`, don't move them as-is.

**Recommended fix, folded into E13/E15:** re-type the whole family on
`OsString`/`Vec<u8>` (`std::env::vars_os()`, `std::env::var_os()`), and on
POSIX use `OsStrExt::as_bytes` for the append/prepend arithmetic.
Cost **S–M**; risk of *not* doing it: a reproducible user-visible panic.

### 3.2 E3 `yieldToDemand` — placement will move under E1

`crates/rustty-termio/src/exec.rs:3489-3505` is correct for the current loop
shape and has a PORT NOTE saying so. When E1 lands, the call must move to the
parse loop's post-tail-advance point (upstream `Exec.zig`, right after
`slot_free.signal`). Missing hunk: nothing today; a **regression risk** tomorrow.

### 3.3 E29 `execvpeZ` — `err` not latched

`crates/rustty-termio/src/command.rs:1270`:

```rust
let err = ExecveError::FileNotFound;   // never reassigned
```

Upstream (`Command.zig`, new inline version):

```zig
var err: posix.system.E = .NOENT;
...
err = posix.errno(posix.system.execve(full_path, argsZ, envp));
switch (err) { .ACCES => seen_eacces = true, .NOENT, .NOTDIR => {}, else => break :execve err }
```

Missing hunk: make `err` `mut` and assign it from each `execveZ` result before the
`match`. 2 lines. Observable only in the child's stderr text.

### 3.4 E16 `global` — the three missing fields/accessors

Missing from `crates/rustty-global/src/lib.rs:81-104` (`GlobalState`):

- `environ` (the captured env block) + accessors `environ()` / `environMap()` —
  **prerequisite for E15**
- `tmp_dir_path: Option<...>` + accessor `tmpDirPath()` — **prerequisite for
  E12 and E20**
- the "cannot be used in libghostty-vt" import guard → a Cargo dependency-edge
  gate, not a comptime block

Deliberately **not** ported: the `InitOpts::tool` arm (N/A — see §7 Q4).

`io_impl` is N/A. rustty's `OnceLock<GlobalState>` is already the right shape for
`7121ab6c3`.

---

## 4. Deletions section

Upstream deleted two whole files in this area. **Both are still present and live
in rustty.**

### 4.1 `src/os/args.zig` (−130) → `crates/rustty-os/src/args.rs` (201 lines)

**What it was.** An `ArgIterator` that on macOS used `NSProcessInfo.arguments`
via objc instead of libc `argc`/`argv`, because under `NSApplicationMain` the
libc argv pointers Zig captures at startup are not valid.

**What replaced it upstream.** Nothing in `src/os`. Zig 0.16's `std.process.Args`
is handed to `main()` inside `std.process.Init.Minimal`, and the C API path takes
them from the caller: `global.init(.{ .c = .{ .argc, .argv, .environ } })` with
`.args = .{ .vector = c.argv[0..c.argc] }` (`main_c.zig:110-135`). The macOS objc
detour is simply no longer needed. Note `ghostty_init(argc, argv)`'s **signature
is unchanged** from the pin — only what it does with argv changed
(`std.os.argv = argv[0..argc]` → stored in `GlobalState.args`).

**Why rustty must delete too.** The Rust motivation for the objc iterator never
existed: `std::env::args_os()` on macOS reads the real process argv (via
`_NSGetArgv`) and is valid under `NSApplicationMain`. Keeping `IteratorMacOS`
means maintaining an `objc2`/`objc2-foundation` dependency and a
`getCString:`/`UTF8String` path for zero benefit — and that path is
**lossy**: `crates/rustty-os/src/args.rs:172` does
`String::from_utf8_lossy(bytes).into_owned()`, and `:66`/`:141` return
`Option<String>`, so a non-UTF-8 argv element is corrupted (a real concern for
`ghostty -e <path with invalid UTF-8>`).

**Migration path.**

1. Re-type the consumer seam on bytes first: `crates/rustty-args/src/args.rs:1626`
   `args_iterator()` and `:1638` `OsArgIterator`, plus
   `crates/rustty-cli/src/action.rs:62,74` `OsArgs`. Yield `OsString` (or
   `Vec<u8>` on POSIX) instead of `String`.
2. Repoint both to `std::env::args_os()`.
3. Delete `crates/rustty-os/src/args.rs`, `pub mod args;`
   (`crates/rustty-os/src/lib.rs:38`), and the `InitError` re-exports in
   `crates/rustty-cli/src/rustty.rs:43,98-99`.
4. Keep the `args` test (`iter.next().?.len > 0`) as a smoke test on the new seam
   — upstream deleted it with the file, but rustty's test-count contract
   (`.port/tools/gates/check_tests_ported.py`) needs the delta recorded, not
   silently dropped.
5. The C-API argv path (`crates/rustty-apprt-c/src/lib.rs:239-263` `ARGV`
   `OnceLock`) already matches upstream's new `.c` arm and needs no change,
   except that its `Vec<Vec<u8>>` storage is already correctly byte-typed —
   good, keep it.

**Cost M** (4 crates touched). **Risk MEDIUM** — the byte re-typing ripples
through the CLI arg parser, which is area G's territory; coordinate.

### 4.2 `src/os/env.zig` (−178) → `crates/rustty-os/src/env.rs` (232 lines)

**What it was.** `getEnvMap`, `appendEnv`, `appendEnvAlways`, `prependEnv`,
`GetEnvResult`, `getenv`, `getenvNotEmpty`, `setenv`, `unsetenv`, plus 4 tests.

**What replaced it upstream — the exact mapping:**

| Deleted symbol | Upstream replacement |
|---|---|
| `getEnvMap(alloc)` | `global.environMap()` (`global.zig:280`), which is `state.environ.createMap(state.alloc)`; the flatpak special case is gone |
| `appendEnv`, `appendEnvAlways` | **moved verbatim into `src/termio/Exec.zig`** as file-private fns (`Exec.zig`, new block after `execCommand`) |
| `prependEnv` | **moved verbatim into `src/termio/shell_integration.zig`** as a file-private fn |
| `getenv`, `getenvNotEmpty`, `GetEnvResult` | gone. Callers use `environ_map.get(key)` (a plain slice lookup, no Windows allocation dance) or `global.environ().getAlloc(alloc, key)` / `.getPosix(key)` |
| `setenv`, `unsetenv` (+ the `c` extern block) | gone from `os/`. The **only** remaining in-process mutation is `os/locale.zig`, which now declares its own file-local `extern "c" fn setenv/unsetenv`. `global.syncEnviron()` re-captures the block afterward |
| 4 tests (`appendEnv empty/existing`, `prependEnv empty/existing`) | move with the functions into termio / shell_integration |
| `os/main.zig` re-exports | 6 `pub const` lines deleted (`GetEnvResult`, `getEnvMap`, `appendEnv`, `appendEnvAlways`, `prependEnv`, `getenv`, `setenv`, `unsetenv`) plus `allocTmpDir`/`freeTmpDir` |

**What rustty must remove.** All of `crates/rustty-os/src/env.rs`,
`pub mod env;` (`lib.rs:45`), and the 8 re-exports at `lib.rs:126-133`.

**Consumers to migrate (exhaustive grep):**

| Site | Current call | Target |
|---|---|---|
| `crates/rustty-termio/src/exec.rs:1923,1968,1996` | `rustty_os::env::appendEnv/appendEnvAlways` | move the fns into `exec.rs` (E13) |
| `crates/rustty-termio/src/shell_integration.rs:609` | `rustty_os::env::prependEnv` | move the fn into `shell_integration.rs` (E13) |
| `crates/rustty-core/src/surface.rs:2087` | `internal_os::env::getEnvMap()` | `global::environMap()` (E16) — **and this is the panic site, §3.1** |
| `crates/rustty-apprt/src/embedded.rs:3018` | `rustty_os::env::getEnvMap()` | `global::environMap()` |
| `crates/rustty-apprt/src/gtk/class/surface.rs:2361` | `rustty_os::env::getEnvMap()` | `global::environMap()` |
| `crates/rustty-apprt/src/lib.rs:199` | `rustty_os::env::getenv("RUSTTY_LOG")` | `global::environ().get_alloc(...)` (this is the `GHOSTTY_LOG` read, which upstream moved *into* `global.init`) |
| `crates/rustty-cli/src/pager.rs:139,142` | `rustty_os::env::getenv` | env-map lookup (area G) |
| `crates/rustty-os/src/locale.rs:34` | `internal_os::getenv("LANG")` | file-local `libc::getenv` (E18) |
| `crates/rustty-apprt/src/gtk/class/application.rs:1826,1845,2080,2090` | `rustty_os::env::setenv/getenv` (`GDK_DEBUG`, `GDK_DISABLE`, `LANG`) | **these are genuine process-env mutations with no upstream replacement in `os/`.** Upstream's GTK application.zig is area F; check what it does now before deciding. Interim: keep local `extern "c"` decls in the GTK module, following the `locale.zig` precedent |
| `crates/rustty-os/src/desktop.rs:211-234`, `xdg.rs:302-438` (tests) | `setenv`/`unsetenv` | **delete entirely** — replaced by local env maps (E15) |

**Ordering:** E13 (move append/prepend) and E16 (`environ`/`environMap`) must land
before `env.rs` can be deleted. E15 (env-map threading) removes the test-side
`setenv` callers. The GTK `setenv` callers (area F) are the last blocker.

**Cost L** (10 sites across 6 crates). **Risk MEDIUM-HIGH** — this is the biggest
mechanical footprint in the area, and doing it *without* the byte re-typing in
§3.1 would be a wasted pass.

### 4.3 Deletions in the range that are NOT rustty's to make

- `src/os/hostname.zig`'s `isValid` + tests — **keep rustty's**, see E17.
- `src/os/main.zig`'s `allocTmpDir`/`freeTmpDir` re-exports — mirror (E20),
  trivial.
- `pkg/harfbuzz/c.zig`, `pkg/wuffs/src/c.zig` — area G, not mine.

---

## 5. Divergence-rebase section

### 5.1 Bounded `killPid` (`crates/rustty-termio/src/exec.rs:2978-3110`) — **KEEP, and it gets MORE valuable**

Recorded in `.port/phase-2/DEFERRALS.md:5128` ("Justified R1 Divergences").
Shape: `0 .. KILL_ABANDON` (10s) is byte-for-byte the oracle loop
(`killpg(pgid, SIGHUP)` + `waitpid(WNOHANG)` + 10 ms sleep); at the deadline
rustty returns `KillError::Abandoned` instead of looping forever. Rationale: a
macOS `login(1)` SIGHUP'd with output queued on its pty slave and no reader
wedges in state `?Es`, SIGKILL-immune and never reapable, so the oracle's
`while (true)` hangs teardown permanently.

**What upstream changed in this exact code (final state at `6ad1fe7d8`):**

```zig
// still while (true) — the loop is UNBOUNDED upstream
const res_pid = while (true) {
    const rc = posix.system.waitpid(pid, null, std.c.W.NOHANG);
    if (posix.errno(rc) == .INTR) continue;
    break rc;
};
log.debug("waitpid result={}", .{res_pid});
if (res_pid != 0) break;
try std.Io.sleep(global.io(), .fromMilliseconds(10), .awake);
```

and in `getpgid`: `std.Thread.sleep(...)` → `std.Io.sleep(...) catch {}`.

**How the divergence must be rebased — three changes, none of which touch the
deadline:**

1. **Keep `KILL_ABANDON` and the `elapsed >= KILL_ABANDON` check exactly where
   they are** (`exec.rs:3083-3095`). The oracle loop is still `while (true)`;
   the divergence is unchanged in kind and still the only thing standing between
   rustty and an unclosable window. Do **not** let a mechanical rebase drop it.
2. **Adopt the explicit `EINTR` retry.** rustty's `posix_waitpid`
   (`command.rs:1133`) already retries `EINTR`, so this is already satisfied —
   but the *reason* it's satisfied changes (see 3).
3. **Adopt upstream's status discard.** Upstream now passes `null` for the status
   pointer in `killPid` (it never read the status). rustty's shared
   `posix_waitpid` writes into a local. Cosmetic; no action required.

**The divergence's justification strengthens under E1.** The pipeline
(`2f0e6659d`) makes `threadMainPosix` return early in two new places — if
`setNonblock(fd)` fails, and if the `io-gather` thread fails to spawn. Both leave
a forked child with **no reader on the pty master** while `threadEnter` has
already succeeded, so `threadExit`'s `stop()` runs into precisely the wedging
condition. Upstream therefore has *more* paths into the hang than at the pin, and
the bounded `killPid` covers them. **Update the DEFERRALS.md entry's
"Reachability in PRODUCTION" paragraph when E1 lands** to add these two arms
alongside the existing `threadEnter` error arms.

### 5.2 `posix_waitpid` aborts on `ECHILD` (`DEFERRALS.md:5116`) — **upstream has now inadvertently fixed this; the open item's premise is stale**

The open item's key claim is that rustty is *faithful* here: ghostty's `killPid`
called Zig std's `posix.waitpid`, which has
`.CHILD => unreachable, // It would be a race condition to handle this error.`
So rustty's `unreachable!("waitpid: unexpected errno")` inherited the exposure
rather than introducing it, and the only difference was Zig's `unreachable` being
UB-in-ReleaseFast vs Rust's always-panic.

**That is no longer true at upstream head.** By dropping the std wrapper for a
raw syscall (`a77c706a1` / `e8525c0fd`), upstream's `killPid` now does:

```zig
const rc = posix.system.waitpid(pid, null, std.c.W.NOHANG);
if (posix.errno(rc) == .INTR) continue;
break rc;             // any other error → rc == -1
...
if (res_pid != 0) break;   // -1 != 0, so the loop BREAKS
```

An `ECHILD` (which rustty can genuinely hit, because
`crates/rustty-xev/src/process.rs`'s `.proc` completion is a **second reaper**
calling `waitpid(pid, &status, 0)`) now **ends the loop gracefully** upstream
instead of hitting `unreachable`. That is exactly the resolution the open item
proposed — "treat `ECHILD` there as 'reaped by someone else' → break the loop."

**Rebase recommendation:** implement the open item's suggested resolution now,
and **reclassify it from an R1 divergence to plain R1 fidelity against upstream
head**. Concretely: give `killPid` an `ECHILD`-aware `posix_waitpid` variant
(returning `Option<WaitPidResult>` or a dedicated `-1` sentinel) and break the
loop on it. This *removes* a divergence rather than adding one, and removes a
reachable Rust panic. Cost **S**. Risk **LOW**. This is the single best
cost/benefit item in the whole area.

### 5.3 `threadEnter` error-arm drain-before-`stop()` (`DEFERRALS.md:5044`) — **upstream churn does NOT touch it; still open, and E1 widens it**

I checked the full `Exec.zig` diff for the ordering. `errdefer self.subprocess.stop();`
(right after `Subprocess.start`) and the subsequent `errdefer`s for `pipe`,
`xev::Timer::init`, `xev::Process::init` and the read-thread spawn are **context
lines, unchanged** in this range. The only edits near them are
`posix.exit(1)` → `std.process.exit(1)` in the in-child arm, and
`errdefer posix.close(pipe[n])` → `errdefer _ = posix.system.close(pipe[n])`.
`process_start` changes type (`std.time.Instant` → `std.Io.Timestamp`).

So: the ordering defect is untouched upstream, confirming the DEFERRALS.md claim
that "upstream Ghostty has the same ordering and therefore the same latent
defect." **But E1 adds two new no-reader windows** (see §5.1): `setNonblock`
failure and `io-gather` spawn failure now abort the reader *from inside* the read
thread, after `threadEnter` has already returned success. Those are not
`threadEnter` error arms at all — they are a *new* class of the same bug, and
they are not covered by the proposed `threadEnter` restructuring.

**Recommendation:** when E1 is scheduled, fold the drain-before-`stop()` fix into
the same work item rather than leaving it deferred. The pipeline already needs a
"drain the master once" concept (the gather stage *is* that), so the restructuring
the deferral describes becomes cheap at that moment and expensive later. Update
the deferral entry to say so.

### 5.4 Other divergences checked, no rebase needed

- **`rustty_util::tripwire`** — E38 confirms the oracle's `tripwire.zig` changed
  only a doc comment in this range; `enabled()` = `builtin.is_test` is unchanged,
  so the `cfg!(debug_assertions)` equivalence still holds and no new
  error-injection tests in area E need judging against it.
- **`madvise` hand-rolled `extern "C"`** — out of area
  (`crates/rustty-vt/src/mem.rs:43-74`). No `src/os` or `src/termio` hunk in this
  range touches `madvise`. The prohibition on routing it through the `libc` crate
  is unaffected. (For the record: `crates/rustty-vt/Cargo.toml:101` *does* depend
  on `libc`, as do `rustty-os:56` and `rustty-termio:79` — so the hand-rolled
  `extern "C"` `madvise` is a **convention** requirement matching `page.rs`'s
  `Backing::alloc/free`, not a dependency-availability one. Don't let "but libc
  is already there" be read as permission.)
- **`rustty-os/src/resourcesdir.rs` `&self`** (E19) — rustty was *ahead* of the
  oracle here; the oracle caught up. Nothing to do, but do not "fix" it back.

---

## 6. Cross-area dependencies and ordering

**Blocked on areas A–D (`src/terminal`) — cannot land first:**

| Item | Needs from A–D |
|---|---|
| E5 (visibility) | `terminal.flags.visible`, `modes.report_visibility` (mode 2033), `device_status::encodeVisibilityReport` + `max_visibility_report_encode_size` |
| E6 (color scheme encoder) | `device_status::encodeColorSchemeReport` + `max_color_scheme_report_encode_size` |
| E7 (cursor defaults) | `Terminal::{setDefaultCursorStyle,setDefaultCursorBlink,setCursorStyle}`, `terminal.cursor.default_blink`, `Terminal::init` opts |
| E8 (resize) | `Terminal::resize(alloc, ResizeOpts{cols,rows,cell_size_px})` owning `width_px`/`height_px` and the synchronized-output clear |
| E9 (scrollback limits) | `PageList` byte/line limits; **also area G** for `config/limit.zig` (new, +115) and the two new config keys |
| E10 (print/apc slices) | `stream.zig` emitting `.print_slice`/`.apc_put_slice`, `Terminal::printSlice`, `apc::feedSlice` |
| E11 (DECRQSS) | `dcs::Command::Decrqss::encode` + `max_response_bytes` |
| E12 (kitty temp dir) | `graphics_image.zig` `Limits::allWithTempDir` |

**Blocked on area F (`src/apprt`, `src/renderer`):**

- E5's `Surface` half needs the apprt occlusion path (area F's
  `apprt/embedded.zig` +103 / `apprt/gtk/class/surface.zig` +111 hunks).
- E3's re-siting depends on `renderer/State.zig` (+76) `lockDemand`/`yieldToDemand`
  — verify area F's assessment says that landed, since rustty's
  `yield_to_demand` call at `exec.rs:3489` already depends on it.
- §4.2's GTK `setenv` callers (`gtk/class/application.rs:1826,1845,2090`) are the
  last blocker on deleting `env.rs`.

**Blocked on area G (`src/config`, `src/cli`):**

- E9's config keys; §4.1's byte re-typing of the CLI arg iterator seam.

**Nothing in area E blocks another area**, with one exception: **E16's `environ`
and `tmpDirPath` global accessors** are needed by E12 and E15 here, and possibly
by area F/G's env reads. Land E16 early.

**Recommended ordering within area E:**

1. **E16** (global: `environ` + `tmp_dir_path` + `tool` arm) — unblocks the rest.
2. **§5.2** (`posix_waitpid` ECHILD) — S, removes a divergence and a reachable panic.
3. **§3.1 + E13** (byte-type the env family; move append/prepend into termio) —
   fixes a live panic and is a prerequisite for the `env.rs` deletion.
4. **E15** (thread env maps; delete the test-side `setenv`) — kills the parallel-test
   `setenv` race. Add the grep gate.
5. **E4** (`setQosClass` on the read thread) — S, independent of E1, benefits the
   current single-threaded reader.
6. **§4.1** (delete `args.rs`) and **E20** (cached tmpdir) — S/M cleanups.
7. **E18/E19** small `os` items.
8. **§4.2** (delete `env.rs`) once F's GTK callers are migrated.
9. **A–D-blocked batch:** E6, E11, E9, E10, E12 (all S at the call site), then
   E7, E8 (M), then **E5** (M, needs A–D + F).
10. **E1** (read pipeline, L) **last**, folding in §5.3 (drain-before-`stop`) and
    re-siting E3. Requires a Miri harness for the ring handoff.

---

## 7. Open questions, with recommendations

1. **Should the read pipeline (E1) be ported at all, or is it a candidate for a
   deliberate divergence?** It is a pure-performance change (macOS 1 KiB
   tty-queue cap) that adds a second thread and a lock-protected ring — the
   highest-UB-risk shape in Rust in this whole area, and the one most likely to
   need raw-pointer slot handoff plus a Miri harness.
   **Recommendation: port it, but last and with a Miri harness.** Two reasons not
   to skip: (a) the `perf-baseline-rust-vs-zig` result (14/14 wins) was measured
   against a pre-pipeline oracle; skipping E1 while the oracle has it means
   rustty loses the throughput comparison on bulk output (`cat` of a large file)
   on the very platform rustty targets first. (b) The `Pipeline` struct is
   self-contained and its correctness is testable in isolation (feed a known byte
   stream through the ring, assert byte-exact delivery and ordering), so the risk
   is bounded. If it is deferred, it must be recorded as a **performance
   deferral with a measured gap**, not left silently absent.

2. **Should E15's env-map threading be ported, or is it "just a Zig stdlib
   accommodation"?** It looks mechanical (six signatures gain a parameter) and a
   reasonable reviewer could call it N/A.
   **Recommendation: port it.** It is not an accommodation — `f8e13f31e` is a real
   bug fix, and in Rust the payoff is larger than in Zig: it deletes
   `setenv`-from-`#[cfg(test)]` calls that are UB against concurrent `getenv`
   under `cargo test`'s default parallelism. That is the exact failure class the
   `parallel-cargo-test-exposes-c-init-races` memory records, and the alternative
   (a global test mutex) is strictly worse.

3. **Is rustty's `getEnvMap` non-UTF-8 panic (§3.1) actually reachable in
   practice, or is it theoretical?** I did not construct a reproducer.
   **Recommendation: construct one before scheduling** — `env FOO=$'\xff' cargo
   run` should be enough to confirm, and it is a 5-minute check that decides
   whether §3.1 is P1 or P3. My read of `std::env::vars`'s documented panic plus
   the three surface-creation call sites says P1, but I am reporting that as
   inference, not as a verified crash.

4. ~~**Does `rustty-bench` mis-parse `+`-actions (E16's `tool` arm)?**~~
   **RESOLVED while writing this: the `tool` arm is N/A for rustty.**
   `crates/rustty-bench/src/main_bench.rs` is a bare
   `rustty_bench::cli::main()` wrapper that never initializes global state and
   never calls app-action detection (`grep -n "STATE\|init_with_action\|global"`
   → 0 hits), and `crates/rustty-bench/src/cli.rs:46` `Action::from_name` owns its
   own action namespace. Upstream needed the `tool` arm only because
   `main_bench.zig` funnels through the shared `global.init`. **E16 shrinks
   accordingly** — port `environ` / `tmp_dir_path` / the accessors, skip the
   `tool` arm, and record it as N/A so nobody adds it later. (The same reasoning
   applies to `main_gen.zig` → `rustty-gen`.)

5. **Where do the GTK `setenv` calls go (§4.2)?** `gtk/class/application.rs`
   mutates `GDK_DEBUG`/`GDK_DISABLE`/`LANG` before GTK init, and upstream's
   `global.syncEnviron()` exists precisely to re-capture after such mutations.
   **Recommendation: ask area F's assessment** what
   `apprt/gtk/class/application.zig` (+128 in this range) does now, and follow the
   `locale.zig` precedent (file-local `extern "c"` decls + a `sync_environ()`
   call) rather than keeping `rustty_os::env::setenv` alive just for GTK.

6. **Perf-baseline invalidation from `quirks_memset` (E34).** The oracle got
   ~2.8× faster on `+terminal-stream` inside this range.
   **Recommendation:** append a note to the `perf-baseline-rust-vs-zig` record
   *now*, before anyone re-benchmarks against `origin/main` and reads the shrunken
   margin as a rustty regression. Also re-confirm the oracle build flag
   (`-Doptimize=ReleaseFast`) still applies.

7. **No Linux VM check was run.** I judged it unnecessary: every Linux-specific
   hunk in this area is either mechanical (`cgroup` `trimRight`→`trimEnd`,
   `systemd`'s `linuxErrnoFromSyscall` reimplementing `std.os.linux.E.init`
   verbatim, `flatpak`'s `@cImport`→`@import("gio_c")`) or is `desktop.zig`'s env
   threading, whose semantics I read in full and which needs no runtime
   confirmation. Nothing in area E's verdicts rests on Linux runtime behavior. If
   E15 is implemented, the `desktop`/`xdg` tests **do** need a Linux run to
   confirm the `setenv` removal, and that is the right moment to use the VM —
   with the standing caution about not running a heavy build concurrently with
   headed compositor work.

8. **`src/Surface.zig` sampling gap.** See §0. If any area-E verdict later looks
   wrong, `Surface.zig`'s mechanical residue is the first place to re-read.
