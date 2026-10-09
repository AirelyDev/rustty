# Phase 8 — upstream resync (pin `12752b2ac` → `a60e9e2a5`, 2026-10-07 to 2026-10-08)

Phase 8 absorbs `12752b2ac..a60e9e2a5`, advance-first, as in Phases 4–7. The rule is parity with
Ghostty, except that Rustty never copies a Ghostty bug.

- Plan: `docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md`.
- Spec: the four delta reviews in `delta-review/` (R1–R4) and the rulings in `DECISIONS.md`.
- Phase-start evidence (before/after the advance) and the finale evidence: `w0/README.md`, `w0/*-at-a60e9e2a5.txt`.
- Long-lane evidence (P8-W9 T1b, T2 and FX1-FX7): `w9/`.

## Range

```
from  12752b2ac1bb05ce53402ed8c853ed1f96eef0b1   2026-09-28
to    a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0   2026-10-07   (origin/main at the advance)
```

107 commits (69 non-merge), 85 files, +7,466 / −497 (`UPSTREAM_*`). On the Rustty side the
phase branch `phase-8-upstream-resync` carries 55 non-merge commits (71 with merges) over `main`
(`401b95b0e`) at the finale base `0d73a3075`, plus the finale's own commits on `phase-8/w9`. At
`26d23100b` (P8-W9 FX7 merged) it carries 101 non-merge commits (130 with merges) over `401b95b0e`. Local
`main` has since moved to `c17ca9c01` (the icon rebrand), so the merge into `main` is not a fast-forward.

## Finale table: workstreams

"Commits" counts the non-merge commits each workstream's merges brought in (`git rev-list
--no-merges <merge>^1..<merge>^2`); a direct commit on the phase branch counts once. The id
columns list the ids each workstream opened; an id another workstream closed is noted.

| WS | scope | commits | merges / direct commits | UDs opened | Ns opened (closed elsewhere) | Vs |
|---|---|---|---|---|---|---|
| W0 | T1 wide-head cut by a non-reflow narrow (`e6db5b633`); T2 parity-surface generator by function; T3 the advance; T4 phase-start records and gate maps; T5 G9 structural CSI/ESC/C0 and exact OSC kinds, c-vt scripts; T6 freestanding wasm build; T7 gate repair at the pin (G3b self-test anchor, G4 leg B, two examples back to upstream) | 15 | `37f4ced8c` (T1), `5c160f7e7` (T2), `0ee78e36e` (T3+T4), `3552398fe` (T5), `9c42ba5e6` (T6), `334dec755` (T7); records `6235aa741` | — | N517, N518, N519 (record-only), N523 (closed W3), N524 (closed W4), N525, N526, N530 | — |
| W1 | pending wrap and saved cursor across resize; charsets above 0xFF and single shift (`afded91df`, `f9ab34f10`, `6220a3617`, `c706451fe`) | 3 | `8ec312003` | — | — | — |
| W2 | kitty placeholder flag, selectLine bound, RIS palette, tmux `max_bytes`, input encoders, prompt click row (`a573781c6`, `13b5ab204`, `bb20f8e45`, `2b1edded2`, `e8b858cc6`, `e55c687a4`, `157c44f64`, `9a4787d90`, `5170b06f6`) | 3 | `ecc1722a7` | — | — | — |
| W3 | DECRQCRA / XTCHECKSUM across vt, config, termio and the C API (`9272a2f7c`, `5f0d4febc`, `b2f557c6c`, `cc0276778`, `01b9f4958`, `00a19db51`, `7e2d1793d`, `25b04b410`) | 4 | `db71cee2d` | — | N520 (record-only) | — |
| W4 | OSC parser: CAN/SUB cancel, OSC 105, unknown-OSC capture, OSC 7501 program status (`520d8f55a`, `36953bca8`, `7b11f3dca`, `bae2c3cdb`) | 4 | `99d788e36` | — | — | — |
| W5 | memory usage, snapshot history compression, stream effects (reset, semantic prompt, program status, unknown OSC), libvt C API, examples, the Windows DLL NON-PORT proof; T5 the colliding cbindgen type names | 11 | `897fb3566`, `4b171279a` (T5); records `f6df38530`, `030e174c2` | — | N521, N528 (provisional N550), N529 (provisional N551) | — |
| W6 | WebGL stub removed; app-scoped render `Device`; EGL surfaceless diagnostics; render-thread exit unrealizes; GTK DPI warn-once (`05392dcc9`, `40d5b860d`, `2e36658c6`, `140feb86a`, `80456f057`, `0d82239b5`) | 6 | `56a2d7ef8` | — | N522 (PORT-ONLY record) | — |
| W7 | ssh-terminfo cache port key, macOS restoration/clipboard/compiler checks, iTerm2 themes (657), `build-rustty.sh` LDFLAGS (`6fbb560cd`, `dfd7bafcd`, `136029788`, `934ef8140`, `0e75d015f`, `e2721d093`, `be595d8f0`, `406f2e544`) | 4 | `83c2ac178` | — | N527 | — |
| W8 | esctest harness against librustty-vt, report-only CI job, G-ESCTEST differential gate (`438d584f0`, `edcaf6670`) | 3 | `9cc2640b7` | UD378 | — | — |
| W9 T1, T1a | finale: gates, tests checklist, targeted cross-host checks, records; T1a `rustty-extra` goldens and three carried comments | 5 | `3b6e760f2` | — | N531 (opened and closed in T1a) | — |
| W9 T1b | cross-host build check on rvm, rwin (msvc and gnu) and macOS | 2 | `1d22ba80d` | — | — | — |
| W9 T2 Mac | Mac long lanes at `1d22ba80d`: G6, G-ASAN, G-LSAN, G-TSAN, G-SAN-SELF, G-ESCTEST, `--webgl`, D1, G-LINT (full `ci.sh` BLOCKED on disk) | 9 | `d8ccc57c9` | — | N532, N533 (closed FX1), N534 (closed FX3), N535, N536 (closed FX2) | — |
| W9 T2 VM | rvm long lanes and rwin V26 at `1d22ba80d` | 6 | `c8a55c96c` | — | N538, N539, N540 (closed FX4), N541 (closed FX5); its D1 and G-LINT reds are N535 and N536 | — |
| W9 re-audit | controller workflow `p8-reaudit` over the 41 upstream-changed files under `src/terminal`, `src/terminal/c`, `src/renderer`, `include/ghostty/vt` | (evidence committed in FX3) | — | — | N537 (closed FX3) | — |
| W9 FX1 | G6 Miri: bench `black_box` sink under Miri; the semantic-prompt failure test reaches its failure in 5 prompts | 3 | `522640840` | — | closes N532, N533 | — |
| W9 FX2 | D1 slice-reinterpret ledger re-audited (line drift); `cvt-parity.sh` SC2010 and prettier on 3 docs | 2 | `562912c1f` | — | closes N535, N536 | — |
| W9 FX3 | shadertoy uniform-buffer count assertion; Mac LSan S19 (SkyLight one-time TCC preflight) | 5 | `b51588ca3` | — | closes N537, N534 | — |
| W9 FX4 | rvm sanitizers: `MSAN_SKIPS` M6, HWASan archive check scoped to the target, Oniguruma init before `Link::oni_regex`, Mesa T5/T6 | 4 | `c0adec970` | — | closes N538, N539, N540 | — |
| W9 FX5 | vt-c scoped-log test isolation: every `log_fn` emitter takes the sys guard, asserted under `cfg(test)` | 2 | `b865abda9` | — | closes N541 | — |
| W9 G6c/G6t | full G6c and G6t on the Mac at `c8a55c96c` | 3 | `614f6f417` | — | N542 (closed FX6), N543 (OPEN, pending) | — |
| W9 FX6 | page tests pass raw `Row`/`Cell` pointers (test-only Tree Borrows UB) | 2 | `ff2fee943` | — | closes N542 | — |
| W9 FX7 | final-review minors M1-M3, M5-M9: `Renderer::init` is an `unsafe fn` with `# Safety`; print tests M7/M8; records | 3 | `26d23100b` | — | — | — |
| W9 T3 | close-out records: id renumbering, these tables | 1 | on `phase-8/w9t3` | — | N543 recorded in NEW-FINDINGS | — |
| — | plan and phase-branch records | 2 | `5e969a4bb` (plan), `0d73a3075` (register) | — | — | — |

Totals: UD378 (1 reference defect, fixed and documented), N517–N543 (27 entries), no V. Next free
ids: **UD379, N544, V34**.

States at `26d23100b`:

- **UD378** has one `- upstream status:` line and a `docs/ghostty-bug-fixes.md` row (G-UDDOCS PASS);
  not reported upstream. UD20 and UD312 became "fixed upstream" at the advance (P8-W0 T4).
- **Ns.** CLOSED: N517, N518, N521, N523–N542. RECORD-ONLY: N519 (G-TESTS' four-token matcher; the
  checklist is the meter), N520 (the snapshot TERMINAL record omits `flags.xt_checksum`, for byte parity),
  N522 (PORT-ONLY `DeviceBox`). OPEN: N543 (G6t `rustty-core` never ran to completion; PENDING the final
  delta-Miri run).
- **Vs.** None opened this phase.

### Provisional id renumbering (P8-W9 T3)

DECISIONS ("the coordinator renumbers at merge to the next free ids") applies to the W9 runner and fix ids
too. The highest real ids before the renumbering were UD378, N531 and V33; no provisional UD or V id was in
use, and N532–N599 held no record (N540 appears only as the start of W3's provisional range in DECISIONS, and
N550/N551 only as W5's former provisional names of N528/N529).
Every reference in `.port/`, `docs/` and `crates/` (comments, PORT NOTEs, one test assertion message in
`rustty-vt-c/src/sys.rs`, gate skip reasons in `sanitizers.sh`, `tsan.supp`, the slice-reinterpret ledger and
the `w9/findings-*.md` records) now uses the new id. Captured tool output in `w9/` (`*.txt`, `*.log`) stays
verbatim with the provisional ids, under a one-line note at its top; commit subjects and the SDD ledger keep
the old ids too. This table maps them.

| provisional | final | finding |
|---|---|---|
| N600 | N532 | G6: rustty-bench `reg_sink` inline asm under Miri |
| N601 | N533 | G6: semantic-prompt failure test did not finish under Miri |
| N602 | N534 | G-LSAN (Mac): SkyLight one-time TCC preflight, S19 |
| N603, N623 | N535 | D1 RED: slice-reinterpret ledger drift (Mac and rvm found the same; one id) |
| N604, N624 | N536 | G-LINT RED: SC2010 and prettier (Mac and rvm found the same; one id) |
| N605 | N537 | re-audit: shadertoy uniform-buffer count assertion |
| N620 | N538 | G-MSAN: rustty-core render-device tests reach uninstrumented libEGL, M6 |
| N621 | N539 | G-HWASAN: archive check saw the host build-dependency copy |
| N622 | N540 | G-TSAN: Oniguruma first-use init; Mesa T5/T6 |
| N625 | N541 | V26: flaky vt-c scoped-log test |
| N650, N640 | N542 | G6t: page tests' Tree Borrows UB (N650 was FX6's first name, never committed) |
| N641 | N543 | G6t `rustty-core` not run to completion |

## Finale gates (at `a60e9e2a5`, on the finale head)

Evidence: `w0/*-finale-at-a60e9e2a5.txt`; `w0/gates-finale-summary-at-a60e9e2a5.txt` is the one-line
verdict of all 42 gate runs, plus the `cargo test -p rustty-extra` line (the re-runs after the finale's own fixes replace the first runs; the first
runs of the gates that were RED are kept as `*-finale-before-*-at-a60e9e2a5.txt`).

| gate | result |
|---|---|
| R4 tests checklist (`w0/tests-checklist.txt`) | 90 added tests present by exact name, 30 changed bodies re-aligned, 0 removals; 0 gaps. The 19 names N519's matcher masks are marked and ticked by hand |
| G-TESTS and its self-test | GREEN: 4,277 live-oracle test blocks, 0 unexplained, 1 accepted |
| G-FILES | GREEN: 562 oracle files, 0 missing, 20 non-port |
| G-SYMS | GREEN: 2,908 `pub fn`, 0 unexplained, 7 accepted |
| G1b (+self), G1 (+self) | GREEN (every module rust ≥ zig, the `xt_checksum` row included; G1 6,229 / 3,227) |
| G3b default scope, `--crate vt`, `--strict` | GREEN after P8-W9 T1a (RED before: 2 vt comments in default and `--crate vt`, plus 1 apprt under `--strict`; kept as `w0/g3b*-finale-before-t1a-at-a60e9e2a5.txt`). Self-test GREEN; `--not-carried-check` GREEN (79 rows) |
| G-LOGFILE and self-test | GREEN after the finale re-anchored 19 `LOG-ABSENT-SITES.tsv` rows (RED before: 18 off-line rows, 3 files short) |
| G-CENSUS and self-test | GREEN: ledger 4,487 / 18,258 / 360 since P8-W0 T3 |
| A3, A4 | GREEN |
| A5 (+ masking self-test) | GREEN: 719 discriminants across 77 enums. Run 3× at the finale head: byte-identical GREEN output each time (P8-W2 T2's one-off arm (b) RED did not recur). Arm (b)'s floor raised 203 → 204 with a RED negative control at 205 |
| G4 (drivers rebuilt at the pin) | GREEN: 12 fixtures byte-exact, 23 c-vt golden cases; leg B (`cvt-parity.sh --all`) GREEN, 25 identical, 3 CMake examples not covered |
| G-REFGEOM and self-test | GREEN: 36 lines, 3 targets |
| G-EOL | GREEN (7 arms) |
| G-I18N | GREEN (34 catalogues) |
| G-THEMES | GREEN (657 files); `cargo test -p xtask emits_full_theme_corpus` passes |
| G-UDDOCS and self-test | GREEN |
| G-PARITY and self-test | GREEN: 497 findings, 404 closed, 93 waived, 0 open |
| G7 | GREEN |
| G9 (+self) | GREEN: 777 / 777 (regenerated surface; CSI/ESC/C0 structural) |
| G-P2 and self-test | GREEN (97 open-claim entries) |
| G-ORACLE-SH-SELF | GREEN after the finale named the pin symbolically in `cvt-parity.sh` (RED before: a 7-hex piece of the pin in its header, added by P8-W0 T7) |
| G-SKIP (+self) | GREEN: 0 silent skips, 265 guarded returns |
| G-ESCTEST (+self) | GREEN: 567 tests, identical outcomes and logs (264 pass, 42 known bugs, 261 fail on both) |
| `postmerge_gates.sh` (build, clippy, fmt, vt-headers, embed-abi, rustty-h-abi, g-skip, g-logfile(+self), oracle-self, test-counts, acyclic, vtalloc, phase2, g3b-self, ud-docs, g-parity) | every line GREEN (`w0/postmerge-finale-at-a60e9e2a5.txt`) |

## Finale cross-host checks (targeted; the full lanes are P8-W9 T2)

| host | check | result |
|---|---|---|
| rvm (Linux aarch64) | `cargo clippy --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland -- -D warnings` | PASS |
| rvm | `cargo test -p` for the 18 touched packages (vt 3,373: 3,372 in the first run, plus the G-REFGEOM golden test re-run once `.port` was on the VM; vt-c 604; apprt `--features gtk,x11,wayland` 173; renderer and core with `RUSTTY_REQUIRE_GL=1` 158 / 87; config 273; termio 112; cli 124; font-shaper 133; datastruct 100; bench 52; apprt-c 32; inspector 23; crash 20; rustty 15; build-config 5; xtask 83, the two `dist::` tests re-run in a git checkout) | PASS. At the T1 head `rustty-extra` failed 6 tests (on the Mac too); P8-W9 T1a fixed them (N531), and `cargo test -p rustty-extra` passes at the finale head on the Mac (`w0/rustty-extra-tests-finale-at-a60e9e2a5.txt`) |
| rvm | P8-W6 T2's env-gated `app_init_without_an_egl_driver_fails_with_device_failed_and_says_why` (`__EGL_VENDOR_LIBRARY_FILENAMES=/nonexistent.json`) | PASS |
| rvm | live GTK KATs through `.port/tools/gtk_live_headless_sway.sh`: `gtk_first_light`, `gtk_input_focus`, `gtk_resize_window_live`, `gtk_inspector_dump`, `gtk_glyph_dump`, `gtk_window_live`, `gtk_close_confirmation`, `gtk_split_drop`; the five `gtk_perform_action` KATs (`--exact`); the five `gtk_render_surface` KATs with their recipes (CPU fallback under broadway with `render-surface-fault-inject`; DMA-BUF and the build-failure destroy-notify under `RUSTTY_REQUIRE_DMABUF=1`; the UD356 error page with `MESA_GL_VERSION_OVERRIDE=4.1`; 1.25x via `swaymsg`) | PASS, 18 of 18 |
| rwin (Windows 11 ARM64, x64 MSVC) | `cargo build -p rustty-vt-c --release --target x86_64-pc-windows-msvc`; `example/c-vt-effects` built with clang against `rustty_vt_c.dll.lib` and run | PASS: exit 0, output byte-identical to `.port/phase-0/corpora/cvt/c-vt-effects.golden` after CRLF → LF |
| macOS | `bash macos/build-rustty.sh`; `xcodebuild test -only-testing:RusttyTests` (Debug, arm64) | BUILD SUCCEEDED; TEST SUCCEEDED (404 passed, 0 failed) |
| macOS | menu-shortcut parity harness | 51 actions byte-identical |

## Finale fixes (records and gate maps only)

- **G-LOGFILE**: 19 `LOG-ABSENT-SITES.tsv` rows still named their `12752b2ac` lines (`renderer/generic.zig`
  +8, `Surface.zig` +1, `config/Config.zig` +53, `terminal/stream.zig` +35). P8-W0 T3 recorded them at the
  arrival and P8-W0 T4 left them out of its scope; no later task owned them. Re-anchored by mapping each old line
  through the `12752b2ac..a60e9e2a5` diff of its file (row 58, `generic.zig:1245`, still sat on a log line, the
  one row 57 excuses, so it moved too). The three "short" files were the same rows: G-LOGFILE GREEN with no
  allowance change.
- **G-ORACLE-SH-SELF**: `cvt-parity.sh`'s header held a 7-hex piece of the pin (P8-W0 T7, merged after the
  last oracle-self run); it now names `gates/ORACLE_PIN`.
- **G3b `--strict`, crate `bin`**: three `COMMENT-NOT-CARRIED.tsv` rows for `lib_vt.zig:510-512`, the doc of
  `pub const DllMain` that exports `lib/windows_dll.zig`'s workaround (NON-PORT proven by P8-W5 T4; evidence:
  the `.port/phase-3/NON_PORTS.md` row, which names `DllMain` in `lib_vt.zig`). W5 T4 added the 39
  `windows_dll.zig` rows but not these, because `bin` is outside G3b's default scope. Cap 76 → 79.
- **A5 arm (b)** floor 203 → 204 (the P8-W5 T3 review's deferred minor), negative control RED at 205.

## Finale fixes (P8-W9 T1a)

Both items T1 left open (code, not records) are fixed, recorded as N531 (CLOSED):

1. **`rustty-extra` goldens.** P8-W3 T2's two config keys (`vt-xt-checksum-report`,
   `vt-xt-checksum-extension`) were missing from `crates/rustty-extra/testdata/*` (bash, fish, zsh,
   sublime, vim) and the mdgen field count (210 → 212). The goldens are regenerated; `cargo test -p
   rustty-extra` passes.
2. **G3b: three reference comments in translated doc examples are now quoted verbatim** at their Rust sites:
   `osc/parsers/program_status.zig:141` (`program_status.rs`), `stream_terminal.zig:126`
   (`stream_terminal.rs`) and, under `--strict`, `apprt/gtk/class/surface.zig:3952` (the `Static.key` doc of
   `logWarnOnce`, `gtk/class/surface/core_callbacks.rs`). G3b default, `--crate vt` and `--strict` are GREEN.

The deferred review minors of the phase went to the final whole-branch review (at `c0adec970`:
READY_WITH_FIXES, Critical 0, Important 1, Minor 9). Its I1 (these records) is P8-W9 T3; M1-M3 and M5-M9
are fixed by FX7; M4 (the T5/T6 negative controls cannot show a Rustty-memory vs Mesa race) is accepted
as documented under T5/T6 in `.port/phase-2/SANITIZER_SUPPRESSIONS.md`.

## Validation: long lanes (P8-W9 T2 and FX1-FX7)

Evidence: `w9/`. Unless a row says otherwise, T2 ran at `1d22ba80d` (the code of the T1 finale), the FX
re-runs at their own heads. A RED row names the finding that closed it and the re-run that turned it GREEN.
Rows marked PENDING are lanes still running at `26d23100b`; the controller fills them at close-out.

| host | lane | result | evidence |
|---|---|---|---|
| Mac | G6 (Miri SB, `check_leaks.sh --all-done`, then `--crate` for the last 7 crates after a session restart) | RED → GREEN. RED on `rustty-bench` (N532, inline asm) and one `rustty-vt` test that did not finish (N533); every other crate GREEN, `rustty-core` 69 passed including the `prompt_click_relative_row` carry (about 2 h). FX1 fixed both; the bench crate re-ran GREEN (38 passed) and the N533 test passes under Miri in 86-89 s | `G6-at-a60e9e2a5.txt`, `g6-part1.log`, `g6-part2.log`, N532, N533 |
| Mac | G6c (`--debug-assertions-off`, at `c8a55c96c`) | GREEN: 126 passed, 2,403 s | `G6c-at-a60e9e2a5.txt` |
| Mac | G6t (`--tree-borrows`, at `c8a55c96c`) | RED → GREEN for `rustty-vt` (N542: two page tests, test-only TB UB; FX6: `page::` 86/86 under TB). `rustty-vt-c` 586/586, `rustty-renderer` 65/65, `rustty-xev` 22/22 GREEN. `rustty-core` not completed (N543) | `G6t-at-a60e9e2a5.txt`, `findings-g6ct.md`, N542, N543 |
| Mac | G-ASAN (`--per-crate-target`) | GREEN | `G-ASAN-mac-at-a60e9e2a5.txt` |
| Mac | G-LSAN (`--per-crate-target`) | RED → GREEN: `rustty-apprt-c` 328 B at exit, SkyLight's one-time TCC preflight (N534); S19 added (FX3); full pass GREEN, 10,045 tests, 0 findings | `G-LSAN-mac-at-a60e9e2a5.txt`, `G-LSAN-mac.rustty-apprt-c.log`, N534 |
| Mac | G-TSAN (`--per-crate-target`, RF4 test strict) | GREEN (`rustty-core` 86, termio 129, xev 54, renderer 145, vt 3,450) | `G-TSAN-mac-at-a60e9e2a5.txt`, `G-TSAN-mac.rustty-core.log` |
| Mac | G-SAN-SELF | GREEN | `G-SAN-SELF-mac-at-a60e9e2a5.txt` |
| Mac | G-ESCTEST | GREEN: 567 tests, identical outcomes and logs | `G-ESCTEST-mac-at-a60e9e2a5.txt` |
| Mac | `ci.sh --webgl` (WS14) | GREEN: WebGL readback matches, both negative controls fail | `ci-webgl-mac-at-a60e9e2a5.txt` |
| Mac | D1 (slice reinterpret) | RED → GREEN: 4 audited sites drifted (N535, RED since `401b95b0e`); FX2 re-keyed the rows | `D1-mac.full.log`, N535 |
| Mac | G-LINT | RED → GREEN: SC2010 in `cvt-parity.sh` and prettier on 3 docs (N536); FX2 | `G-LINT-mac.full.log`, N536 |
| Mac | full `ci.sh` at `1d22ba80d` | BLOCKED (disk below 12 GB after 3 min; fmt, clippy, build and wasm check passed before the stop); not a code verdict | `ci-full-mac-at-a60e9e2a5.txt` |
| Mac | re-audit of the 41 upstream-changed vt/C/renderer files (5 shards, adversarial verifier) | 4 shards clean; 1 low finding, N537 (FX3 fixed it, with a negative control) | `reaudit-at-a60e9e2a5.txt`, N537 |
| rvm | full Linux lane (`cargo test --workspace --features rustty/gtk,rustty/x11,rustty/wayland`) | GREEN (rc 0, 431 s) | `full-linux-lane-at-a60e9e2a5.txt` |
| rvm | G-GTKFEAT | GREEN | `g-gtkfeat-at-a60e9e2a5.txt` |
| rvm | G-REL (release lane) | GREEN: 6,726 passed, 167 ignored, 0 failed | `g-rel-at-a60e9e2a5.txt` |
| rvm | G-EXAMPLES | GREEN: 27 examples built | `g-examples-at-a60e9e2a5.txt` |
| rvm | G-ESCTEST | GREEN: 567 tests identical (the first attempt was BLOCKED on the oracle's Zig packages, then re-run) | `g-esctest-at-a60e9e2a5.txt`, `g-esctest-attempt1-blocked-at-a60e9e2a5.txt` |
| rvm | EGL diagnosis (`app_init_without_an_egl_driver_fails_with_device_failed_and_says_why`) | PASS | `egl-diagnosis-at-a60e9e2a5.txt` |
| rvm | G-ASAN | GREEN: 10,307 tests, 0 findings | `g-asan-at-a60e9e2a5.txt` |
| rvm | G-LSAN, G-LSAN-GTK | GREEN: 10,307 tests; GTK 1 test | `g-lsan-at-a60e9e2a5.txt`, `g-lsan-gtk-at-a60e9e2a5.txt` |
| rvm | G-SAN-SELF | GREEN (T2 and again after FX4) | `g-san-self-at-a60e9e2a5.txt`, `fx4-g-san-self.txt` |
| rvm | D6 (sanitizer canary) | GREEN | `d6-clean-clone-at-a60e9e2a5.txt` |
| rvm | G-VALGRIND (4 shards) | GREEN: 377 + 208 + 181 + 148 = 914 memcheck runs clean | `g-valgrind-shard{1..4}of4-at-a60e9e2a5.txt` |
| rvm | G-MSAN | RED → GREEN: rustty-core render-device tests reach uninstrumented libEGL (N538, `MSAN_SKIPS` M6); after FX4 GREEN, 6,375 tests, 0 findings | `g-msan-at-a60e9e2a5.txt`, `fx4-g-msan.txt`, N538 |
| rvm | G-HWASAN | RED → GREEN: the archive check saw the host build-dependency `libz.a` (N539); after FX4 GREEN, 10,183 tests, 0 findings | `g-hwasan-at-a60e9e2a5.txt`, `fx4-g-hwasan.txt`, N539 |
| rvm | G-TSAN | RED → GREEN: Oniguruma first-use init race (fixed) and two Mesa reports (T5/T6) (N540); after FX4 GREEN, 4,536 tests, 0 reports | `g-tsan-at-a60e9e2a5.txt`, `fx4-g-tsan.txt`, `fx4-diagnosis.txt`, N540 |
| rvm | D1, G-LINT | RED, the same findings as the Mac (N535, N536), fixed by FX2 | `d1-clean-clone-at-a60e9e2a5.txt`, `g-lint-clean-clone-at-a60e9e2a5.txt` |
| rvm | G4 | not runnable on Linux (the drivers are macOS-only); G4 is GREEN on the Mac at the finale | `g4-not-runnable-on-rvm-at-a60e9e2a5.txt` |
| rvm | full `ci.sh` at `1d22ba80d` (a clone with stray outputs) | RED only on G-MSAN, G-HWASAN, G-TSAN, D1 and G-LINT, all listed above and fixed | `ci-sh-rvm-polluted-clone-at-a60e9e2a5.txt` |
| rwin | V26: `cargo test --workspace --exclude libxml2-sys --exclude fontconfig-sys --exclude sentry-sys --target x86_64-pc-windows-msvc` | RED → GREEN: 6,398 passed, 3 failed: 2 `xtask dist` tests needed `.git` (pass in a git checkout), 1 flaky vt-c log-sink test (N541; FX5: 599/599 twice on rwin) | `v26-windows-full-workspace-at-a60e9e2a5.txt`, `v26-dll-step-and-reruns-at-a60e9e2a5.txt`, N541 |
| rwin | DLL step: `rustty_vt_c.dll` (x64 MSVC) with `c-vt-effects` | PASS | `v26-dll-step-and-reruns-at-a60e9e2a5.txt` |
| rvm, rwin, Mac | cross-host build (T1b): rvm GTK-feature build, clippy and release; rwin msvc and gnu; macOS xcframework | PASS, zero warnings | `cross-host-build-at-a60e9e2a5.txt` |
| Mac | `ci.sh --fast` plus delta G6/G6c/G6t (crates changed since their full evidence) and delta ASan/LSan/TSan at `26d23100b` | PENDING (controller fills at close-out) | — |
| Mac | G6t `rustty-core` in full at `26d23100b` (N543) | PENDING (controller fills at close-out) | — |
| rvm | full `ci.sh` (Valgrind inline) at `c0adec970` | PENDING (controller fills at close-out) | — |
| rvm | delta at `26d23100b` (compiles FX7's Linux `Renderer::init` twin) | PENDING (controller fills at close-out) | — |
| rwin | V26 plus the DLL step at `26d23100b` | PENDING (controller fills at close-out) | — |
| Mac (idle) | perf suite against the zigbench at `a60e9e2a5` (ReleaseFast; artifacts rebuilt by perf prep on `phase-8/w9perf`, `fdf81eba7`, not merged), loop-sweep ns/op; DECISIONS P4's osc-parser / terminal-stream check of the unknown-OSC fast path | PENDING (controller fills at close-out) | — |
| Mac (idle) | G8 rebase, G8 `--strict`, G8b (and the `ORACLE_SHA256` provenance comment, still "P7-W19") | PENDING (controller fills at close-out) | — |

## Owed before the merge to `main`

- N543: G6t over `rustty-core` in full, GREEN.
- The delta lanes at `26d23100b` on the Mac (`ci.sh --fast`, delta Miri, delta sanitizers) and on rvm, the
  rvm full `ci.sh` at `c0adec970`, and rwin V26 plus the DLL step at `26d23100b`: each GREEN or its finding
  recorded and closed.
- Perf: the suite and the P4 measurement against the zigbench at `a60e9e2a5`, then the G8 rebase,
  `--strict` and G8b; merge `phase-8/w9perf`.
- The PENDING rows above filled, and the README totals re-checked if any of them opens an id.

Hosted CI legs stay push-gated under V25 (the maintainer's call).
