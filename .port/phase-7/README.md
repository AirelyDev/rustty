# Phase 7 — upstream resync (pin `20abdb50a` → `12752b2ac`, 2026-09-28 to 2026-09-30)

Phase 7 absorbs `20abdb50a..12752b2ac`, advance-first, as in Phases 4–6. The rule is parity with
Ghostty, except that Rustty never copies a Ghostty bug.

- Plan: `docs/superpowers/plans/2026-09-28-phase7-upstream-resync.md`.
- Spec: the five delta reviews in `delta-review/` and the rulings in `DECISIONS.md`.
- Phase-start evidence (before/after the advance): `w0/README.md`.

## Range

```
from  20abdb50a6216c450d6d4d010c41c7edf5ab15b2   2026-08-31
to    12752b2ac1bb05ce53402ed8c853ed1f96eef0b1   2026-09-28   (origin/main at the advance)
```

333 commits (234 non-merge), 233 files, +35,723 / −9,235 (`UPSTREAM_*`). On the Rustty side the
phase branch `phase-7-upstream-resync` carries 128 non-merge commits (175 with merges) over
`main` at the finale, plus the finale's own commits on `phase-7/w12f`.

## Finale table: workstreams

"Commits" counts the non-merge commits each workstream's merges brought in (a workstream's
direct commits on the phase branch count once each). The id columns list the ids each
workstream opened; an id another workstream closed is noted.

| WS | scope | commits | merges / direct commits | UDs opened | Ns opened (closed elsewhere) | Vs |
|---|---|---|---|---|---|---|
| W0 | display-link deadlock fix, UD-docs gate, the advance, phase-start records | 8 | `89d327ee4`, `d8f53fd76`, `c5000e050`, `5200d7aff`, `9f007b10e`, `941b18c99`, `ec0dd16ff`, `4a04be299` | UD352 | N435 (closed W5 T4), N436 (closed W2 T1), N437 (closed W7 T2) | — |
| W1 | terminal memory model: bitmap polarity, page layout, UntouchedPool, PageList pooling, Windows reclaim | 12 | `400dc73cc`, `ed4103688`, `22090553b` | UD361, UD364 | N458 | V27, V28 |
| W2 | libvt C API: search tick, empty output, kitty temp dir, mouse shape, pull scrollback, render hold, dynamic palette, raw-self stream drivers, paste re-entry | 11 | `07904557a`, `ef7b2ed94`, `bbd0c439c`, `12e90f232`, `86b7ac631`, `40559be6f`, `114c9e2f2`, `c51c8d443`; records `7b34d78bf` | — | N451, N452, N454 | — |
| W3 | RenderState overscan: Zig-API form, C API, bench modes | 4 | `21eedffc0` | UD369, UD370 | N453 | — |
| W4 | modifyOtherKeys 2 encoder, DECRQM + ComptimeIntSet, print/cursor/selection fixes | 3 | `e15865435`, `2c28402ab`, `3a4fafca5` | — | — | — |
| W5 | parse_int at the OSC sites, kitty Windows paths + wuffs zlib, tmux list-windows, search after prepend | 8 | `9cd56869a`, `d183a5f27`, `bd722e059`; records `3a827824b` | — | — | V29 |
| W6 | CSI 8 t across vt/termio/config/core/apprt-api/embed header, raw `Target::Surface`, GTK half | 7 | `366c820b6`, `513f603bc`, `467918b92`, `5da45a868` | UD371, UD372 | N456, N459 | — |
| W7 | GL leaves + EGL/DMA-BUF, renderer lifecycle + GTK RenderSurface, Metal soundness, core raw-self entries, Overrides, live GTK harness, termio raw-self | 35 | `fa60dd9e6`, `99f23b5ee`, `53020220b`, `c32a4930c`, `60b995159`, `f00301f28`, `df350e613`, `5af770b30`, `013006535` | UD353–UD357, UD362, UD363, UD365–UD368 | N440, N441, N443, N444, N448 (now UD366), N449, N450 | — |
| W8 | font keys + nerd-font tables, core small, systemd ready ordering | 8 | `de1845319`, `671fe6a12`, `de6043c0e` | — | N442 | — |
| W9 | macOS Swift fixes, CSI 8 t Swift half | 2 | `72b734a27`, `6d25db70b` | — | — | — |
| W10 | Unicode 18, fontconfig 2.18.3, translations/themes/version, CI and release workflows | 9 | `3bc688b41`, `dfdcd81eb`, `9c829206c`, `4a93a2889` | UD358–UD360 | N438 | — |
| W11 | shell integration, config/os/termio records | 4 | `f75df27f6` | — | — | — |
| W12 | T0 phase-branch gate repair; T1 comment carry; T1b cross-platform build check; T1 finale | 13 + finale | `4458e973e`, `39d788001`, `9076dfefd`; finale on `phase-7/w12f` | — | N455, N461–N464, N465 | — |
| — | plan and phase-branch records | 2 | `1eb0a9c2a`, `2d6f3e47e` | — | — | — |

Totals: UD352–UD372 (21 reference defects, all fixed and documented), N435–N465 (25 entries; ids
N439, N445–N447, N457 and N460 were reserved and never used), V27–V29 (3).

States at the finale:

- **UDs.** Every UD352–UD372 has one `- upstream status:` line and a `docs/ghostty-bug-fixes.md`
  row (G-UDDOCS GREEN). None has been reported upstream.
- **Ns.** All are CLOSED except N438, which is record-only (not a UD, unreachable). N448 is
  superseded by UD366, and N453 is the record of UD369/UD370.
- **Vs.** V27–V29 stayed ENVIRONMENT-BLOCKED (time) at the finale. They are Miri runs over the 30-minute task
  bound, deferred to P7-W12 T2 with runnable recipes. P7-W15 discharged all three on 2026-10-06 (G6, G6c and G6t
  GREEN at `04f0a894c`).

## Finale gates (at `12752b2ac`, on the finale head)

Evidence: `w0/*-finale-at-12752b2ac.txt`; `w0/gates-finale-summary-at-12752b2ac.txt` is the one-line verdict of all 35 gate runs on the finale head (the post-merge set ran at `d36ddf38`, before the comment-only commit).

| gate | result |
|---|---|
| G-TESTS (`audit_test_coverage.py`) and its self-test | GREEN: 4,188 live-oracle test blocks, 0 unexplained, 1 accepted. Self-test GREEN after its accepted-arm anchor moved off a row that W10 T2's `codepoint_width_unicode18_wide_ranges` now name-matches |
| R5 tests checklist (`w0/tests-checklist.txt`) | 112 added tests present by name, 40 changed bodies re-aligned, 2 removals dropped; 0 gaps |
| G-FILES | GREEN: 557 oracle files, 0 missing, 18 non-port |
| G-SYMS | GREEN: 2,890 `pub fn`, 0 unexplained, 7 accepted |
| G1b, G1 | GREEN (every module rust ≥ zig; G1 6,109 / 3,227) |
| G3b | GREEN (default scope), and **`--strict` GREEN** for the first time since the advance (the finale carried the last 11 comments); self-test GREEN |
| G-LOGFILE and self-test | GREEN |
| G-CENSUS and self-test | GREEN: the ledger moved to 4,397 / 17,717 / 357 at P7-W0 T4, so not red-by-design at the finale |
| A3, A4, A5 (+ masking self-test) | GREEN (A5: 683 discriminants across 71 enums) |
| G4 (drivers rebuilt at the pin) | GREEN: 12 fixtures byte-exact, 23 c-vt golden cases |
| G-REFGEOM and self-test | GREEN: 36 lines, 3 targets |
| G-EOL | GREEN (7 arms) |
| G-I18N | GREEN (34 catalogues) |
| G-THEMES | GREEN (617 files) |
| G-UDDOCS and self-test | GREEN |
| G-PARITY and self-test | GREEN: 497 findings, 0 open |
| G7 | GREEN |
| G9 | GREEN: 693 / 693 |
| G-P2 and self-test, oracle-sh self-test | GREEN (oracle-self after the finale named the pin symbolically in `exe-differential.ps1`) |
| `postmerge_gates.sh` (build, clippy, fmt, vt-headers, embed-abi, rustty-h-abi, g-skip, g-logfile(+self), oracle-self, test-counts, acyclic, vtalloc, phase2, g3b-self, ud-docs, g-parity) | every line GREEN |

## Finale cross-host checks (targeted; the full lanes are P7-W12 T2)

| host | check | result |
|---|---|---|
| rvm (Linux aarch64) | `cargo clippy --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland -- -D warnings` | PASS |
| rvm | `cargo test -p` for the 29 crates the phase touched (vt 3,332; vt-c 586; apprt `--features gtk,x11,wayland` 174; renderer 154; config 272; …) | PASS; `rustty-apprt-c --test smoke` needs the cdylib built first (the known ordering, W12 T1b item 5) and passes after `cargo build -p rustty-apprt-c` |
| rvm | the live GTK KATs through `.port/tools/gtk_live_headless_sway.sh`: `gtk_first_light`, `gtk_input_focus`, `gtk_resize_window_live`, `gtk_inspector_dump`, `gtk_split_drop`, `gtk_glyph_dump`, `gtk_window_live`, `gtk_close_confirmation`; the five `gtk_render_surface` KATs one by one with their module-doc recipes (DMA-BUF under `RUSTTY_REQUIRE_DMABUF=1`, CPU fallback and build-failure notify with `render-surface-fault-inject`, the UD356 error page, 1.25x fractional scale via `swaymsg`); the five `gtk_perform_action` KATs one by one (`--exact`) | PASS, 18 of 18 |
| rwin (Windows 11 ARM64, x64 MSVC) | `rustty-vt` `kitty::` (393, the Windows path and UNC tests included), `mem::` (4, `mem_windows_can_reclaim_reports_support` included), `rss_probe --release` (returns memory, PASS), `rustty-xev` (45) | PASS |
| rwin (x64 GNU, cross-built on the Mac with mingw-w64) | `rustty-font-shaper` with `fontconfig_freetype` (133) and `fontconfig-sys` (4), including the UD360 child-process test | PASS |
| macOS | `xcodebuild test -only-testing:RusttyTests` | TEST SUCCEEDED (403 passed, 0 failed) |
| macOS | menu-shortcut parity harness | 51 actions byte-identical |

## Finale fixes

- **N465** (final review, ruling 61): `Surface::close(&mut self)` held a protected `&mut Surface`
  across the runtime close callback, which both hosts re-enter. Now raw-self, with the seam's
  `close` taking `*const Self`; proof `.port/phase-2/mirihar-n465`, live test
  `close_callback_reenters_the_closing_surface`.
- G-TESTS-SELF and oracle-self had gone red after the last merges (a name-masked self-test
  anchor; a 7-hex pin piece in `exe-differential.ps1`). Both are fixed without weakening either
  gate.
- G3b `--strict`: the systemd and gresource comments (`4eb308831`, `ef7edc4a2`) and the refreshed
  TinyIo doc quote.
- Records: NEW-FINDINGS state lines show only the final state; N456's known limits no longer call
  N459 open; the unused ids are listed in `.port/parity-closure/README.md`.

## Open after Phase 7

All of these are P7-W12 T2 or later: the deferred long runs (benches against a zigbench at
`12752b2ac`, the full Linux lane, full `ci.sh` on both hosts, continuous G6/G6c, V24–V29), and
the parked review items in the phase's SDD log.
