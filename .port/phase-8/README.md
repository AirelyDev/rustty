# Phase 8 — upstream resync (pin `12752b2ac` → `a60e9e2a5`, 2026-10-07 to 2026-10-08)

Phase 8 absorbs `12752b2ac..a60e9e2a5`, advance-first, as in Phases 4–7. The rule is parity with
Ghostty, except that Rustty never copies a Ghostty bug.

- Plan: `docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md`.
- Spec: the four delta reviews in `delta-review/` (R1–R4) and the rulings in `DECISIONS.md`.
- Phase-start evidence (before/after the advance) and the finale evidence: `w0/README.md`, `w0/*-at-a60e9e2a5.txt`.

## Range

```
from  12752b2ac1bb05ce53402ed8c853ed1f96eef0b1   2026-09-28
to    a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0   2026-10-07   (origin/main at the advance)
```

107 commits (69 non-merge), 85 files, +7,466 / −497 (`UPSTREAM_*`). On the Rustty side the
phase branch `phase-8-upstream-resync` carries 55 non-merge commits (71 with merges) over `main`
(`401b95b0e`) at the finale base `0d73a3075`, plus the finale's own commits on `phase-8/w9`.

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
| W9 | T1 finale: gates, tests checklist, targeted cross-host checks, records | finale | on `phase-8/w9` | — | — | — |
| — | plan and phase-branch records | 2 | `5e969a4bb` (plan), `0d73a3075` (register) | — | — | — |

Totals: UD378 (1 reference defect, fixed and documented), N517–N530 (14 entries), no V. Next free
ids at the finale: **UD379, N531, V34**.

States at the finale:

- **UD378** has one `- upstream status:` line and a `docs/ghostty-bug-fixes.md` row (G-UDDOCS PASS);
  not reported upstream. UD20 and UD312 became "fixed upstream" at the advance (P8-W0 T4).
- **Ns.** CLOSED: N517, N518, N521, N523–N530. RECORD-ONLY: N519 (G-TESTS' four-token matcher; the
  checklist is the meter), N520 (the snapshot TERMINAL record omits `flags.xt_checksum`, for byte parity),
  N522 (PORT-ONLY `DeviceBox`). None is open.
- **Vs.** None opened this phase.

## Finale gates (at `a60e9e2a5`, on the finale head)

Evidence: `w0/*-finale-at-a60e9e2a5.txt`; `w0/gates-finale-summary-at-a60e9e2a5.txt` is the one-line
verdict of all 43 gate runs (the re-runs after the finale's own fixes replace the first runs; the first
runs of the gates that were RED are kept as `*-finale-before-*-at-a60e9e2a5.txt`).

| gate | result |
|---|---|
| R4 tests checklist (`w0/tests-checklist.txt`) | 90 added tests present by exact name, 30 changed bodies re-aligned, 0 removals; 0 gaps. The 19 names N519's matcher masks are marked and ticked by hand |
| G-TESTS and its self-test | GREEN: 4,277 live-oracle test blocks, 0 unexplained, 1 accepted |
| G-FILES | GREEN: 562 oracle files, 0 missing, 20 non-port |
| G-SYMS | GREEN: 2,908 `pub fn`, 0 unexplained, 7 accepted |
| G1b (+self), G1 (+self) | GREEN (every module rust ≥ zig, the `xt_checksum` row included; G1 6,229 / 3,227) |
| G3b default scope, `--crate vt`, `--strict` | **RED: 2 vt comments (default and `--crate vt`), + 1 apprt (`--strict`)** — see "Open at the finale". Self-test GREEN; `--not-carried-check` GREEN (79 rows) |
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
| rvm | `cargo test -p` for the 18 touched packages (vt 3,373: 3,372 in the first run, plus the G-REFGEOM golden test re-run once `.port` was on the VM; vt-c 604; apprt `--features gtk,x11,wayland` 173; renderer and core with `RUSTTY_REQUIRE_GL=1` 158 / 87; config 273; termio 112; cli 124; font-shaper 133; datastruct 100; bench 52; apprt-c 32; inspector 23; crash 20; rustty 15; build-config 5; xtask 83, the two `dist::` tests re-run in a git checkout) | PASS except **rustty-extra: 6 failed** (see "Open at the finale"); the same 6 fail on the Mac |
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

## Open at the finale

Both are code (test data and comments), not records, so the finale did not change them:

1. **`rustty-extra` goldens were not regenerated for P8-W3 T2's two config keys.** `cargo test -p
   rustty-extra` fails 6 tests on the Mac and on rvm: `bash`, `fish`, `zsh` `completions_match_golden`,
   `sublime` and `vim` `syntax_matches_golden` (the generated files now carry `vt-xt-checksum-report` and
   `vt-xt-checksum-extension`; the committed `crates/rustty-extra/testdata/*` do not), and
   `mdgen::tests::gen_config_covers_every_field_and_honors_the_cli_flag` (field count 212, test expects 210).
   Upstream generates these files from `Config` at build time, so its output carries the keys. Fix:
   regenerate the testdata from the port (checked against the reference's generated output) and bump the count,
   as P7-W6 T1 did for `vt-window-resize-allowed`.
2. **G3b: three reference comments in translated doc examples are not quoted.** Default scope and `--crate vt`:
   `osc/parsers/program_status.zig:141` (`if (report.readOption(.progress)) |percent| showProgress(percent);`,
   Rust `osc/parsers/program_status.rs:172` paraphrases it in Rust) and `stream_terminal.zig:126`
   (`if (v.truncated or !std.mem.startsWith(u8, v.content, prefix)) return;`, Rust `stream_terminal.rs:195`).
   `--strict` adds `apprt/gtk/class/surface.zig:3952` (the `Static.key` doc of `logWarnOnce`, P8-W6 T4;
   Rust `gtk/class/surface/core_callbacks.rs:17-26` paraphrases it). The `COMMENT-NOT-CARRIED.tsv` header
   rules out a row where a Rust site exists; the fix is to quote each line at its Rust site with a PORT NOTE.

The deferred review minors of the phase are listed in the SDD ledger (`minor (deferred)` lines) for the
final whole-branch review. Everything else is P8-W9 T2: the benches against a zigbench at `a60e9e2a5`,
full `ci.sh` on both hosts, the Miri lanes (including the `prompt_click_relative_row` run P8-W2 T3 capped at
30 minutes), the sanitizer matrix and the WS14 `--webgl` lane.
