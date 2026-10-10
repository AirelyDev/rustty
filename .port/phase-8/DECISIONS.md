# Phase 8 decisions

Range `12752b2ac..a60e9e2a5` (Ghostty `origin/main`, 2026-10-07): 107 commits, 69 non-merge, 85
files, +7,466 / −497. This file settles every question the four delta reviews raised
(`.port/phase-8/delta-review/R1…R4`). It is part of the spec that
`docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md` implements.

**Rule (maintainer):** Rustty matches Ghostty's latest upstream exactly, except it never copies a
Ghostty bug. Applied as in Phase 7:
- Upstream changed behaviour Rustty already had: adopt upstream's version, unless upstream's
  version is itself a defect.
- A suspected upstream defect: measure it first. Confirmed → fix in Rustty and record a UD (next
  free UD378). Not confirmed → port it as upstream has it.
- A Rustty-only file or behaviour whose reference counterpart upstream removed: remove it.
- A mechanism with no Rust counterpart (Zig toolchain internals): record it in `NON_PORTS`.

No question needs the maintainer. The reviews found **no new UD candidate**; UD378 stays free
unless a measurement during the port confirms one.

## Id reservations

| id | what | opened by | closed by |
|---|---|---|---|
| N517 | the parity-surface generator (`.port/phase-0/inventory/parse_csi.py`, `parse_esc.py`) reads phase-0 line windows of `stream.zig`: the frozen surface misses 13 CSI finals and records CSI arms as ESC (R4 §5.3) | P8-W0 T2 | P8-W0 T2 |
| N518 | Rustty carries the wide-head-on-cut crash `e6db5b633` fixes (`pagelist/resize.rs:708-712`; debug panic, release stray write) (R1 §2.1) | P8-W0 T1 | P8-W0 T1 |
| N519 | G-TESTS' four-token matcher masks 17 of the 90 added tests (19%); the checklist is the meter (R4 §5.1) | P8-W0 T4 | record-only |
| N520 | `flags.xt_checksum` is not carried by the snapshot TERMINAL record, so a restored terminal answers DECRQCRA with the default variant. Opt-in, not a crash; the wire format stays byte-identical with Ghostty (R1 Q2) | P8-W3 | record-only |
| N521 | `rustty_terminal_paste` kept going after a refused write; `2b0ceff7d` fixes it upstream and the port closes it (R2 §2.6) | P8-W5 T3 | P8-W5 T3 |
| N522 | PORT-ONLY: the app-scoped render `Device` lives in its own allocation (`DeviceBox`), not by value in `App` (R3 Q1) | P8-W6 T2 | record-only |

Workers that run concurrently take disjoint provisional ranges (W1 N530+, W3 N540+, W5 N550+, W6
N560+, others N570+; UDs likewise UD380+/UD385+/UD390+/UD395+) and the coordinator renumbers at
merge to the next free ids.

## Doctrine

| # | question | decision |
|---|---|---|
| D1 | Advance-first, as Phases 4–7? | Yes. The pin moves in P8-W0 T3. A5 reads `origin/main` and has been RED since the fetch: record it as pre-existing in the "before" set (R4 §5.4). |
| D2 | Fix the carried wide-char crash (`e6db5b633`) before the advance? | Yes, P8-W0 T1, the Phase 7 D2 precedent: a reachable panic Rustty carries ships first. It is a faithful port of upstream's fix plus its test, recorded as N518 (no UD: upstream fixed it). |
| D3 | Regenerate `parity-surface.json` at the advance with the current generator? | No. Fix the generator first (P8-W0 T2, N517: locate `fn csiDispatch`/`fn escDispatch` by header and brace-match, self-test fails when a known final such as `'m'` is missing), then regenerate in T3. G9 may go RED on finals Rustty never had to declare; each is mapped or ported in the owning workstream, never waived silently (R5). |
| D4 | UD re-anchoring | Re-stamp only the 47 moved entries plus UD6/UD60/UD160 at `a60e9e2a5` (R4 Appendix A); leave the rest at their current pins. |
| D5 | G-TESTS matcher | Keep it; tick `R4-upstream-tests.tsv` by exact name at the finale (N519). |
| D6 | Long runs | Deferred to the finale (P8-W9 T2), as in Phase 7. Inside tasks: targeted `cargo test -p`, clippy/fmt, gate scripts, Miri runs bounded at 30 min. |

## Records

| # | item | decision |
|---|---|---|
| M1 | UD20 (`80456f057`) and UD312 (`51d3ca168`): FIXED-SAME upstream | Keep both entries, set `- upstream status: fixed upstream in <sha>`, remove the "reference defect" PORT NOTEs, follow the UD150 precedent for the `docs/ghostty-bug-fixes.md` rows in the same commit (G-UDDOCS GREEN). Add the upstream-named tests (`custom shader selection uniform layout matches compiled GLSL`; rename Rustty's to `command_data_null`) and keep Rustty's broader ones. |
| M2 | UD6, UD60, UD160 (code moved, defects live) | Re-anchor only; Rustty's fixes stay. UD6's citations move to `tmux/control.zig:137-142` and `stream_handler.zig:426`. |
| M3 | UD113 | Add `c/osc.zig set` (`rustty_osc_set`) to its site list; keep Rustty's unconditional `c_int` narrowing (R2 Q6). |
| M4 | UD356 | Still valid (no `setError(true)` caller at `a60e9e2a5`). Its trigger (no EGL driver) now fails at App creation upstream, so its test moves to a per-surface context failure, re-measured on rvm; note the App-creation behaviour in the entry (R3 Q6). |
| M5 | `aca9bf031` (decode_png zeroing) | Rustty already zero-initialises; port the upstream-named test, no record beyond a PORT NOTE. |
| M6 | LEDGER S133-F1 (CLOSED) | Not reopened. Port `a573781c6` in P8-W2 and add a note to the CLOSED row (R4 Q5). |
| M7 | V25 (hosted CI never run) | Extend V25 with the new `test-esctest` job and the Windows DLL steps (R4 Q8). No new V. |

## Ports and non-ports

| # | item | decision |
|---|---|---|
| P1 | `src/lib/windows_dll.zig` (`a141f9bdb`) | NON-PORT (Zig toolchain workaround; rustc's CRT entry runs C++ initializers), **after** an rwin run proves `rustty_vt_c.dll` handles a multi-byte UTF-8 write through `c-vt-effects`. If rwin shows the crash (e.g. on `-gnu`), port the constructor walk instead. Port the CI step from `a7cf951eb` either way (R2 Q2, R4 Q2). |
| P2 | WebGL stub (`05392dcc9`) | Remove every Rustty `webgl`/`Backend::Webgl` site (R3 §2.1, 10 places) and `check_leaks.sh`'s `rustty-renderer:webgl` entry. The wasm module and its consumer gate stay (R3 Q7). |
| P3 | `test/esctest` (`438d584f0`, `edcaf6670`) | Port it at `test/esctest/` as a **separate Cargo package outside the workspace** (as upstream's separate Zig package), with a hash-pinned esctest2 fetch script and the report-only `test-esctest` CI job. Add a local **G-ESCTEST** differential gate: Rustty's failing set must equal the oracle harness's failing set at the pin (the oracle side is built in the oracle tree, never in Rustty's). After P8-W3 (R2 Q1, R4 Q1). |
| P4 | Unknown-OSC capture limit (R2 Q3) | Compare `len < cap.max_bytes` in the per-byte fast path; measure the osc-parser and terminal-stream benches at the finale. Only if they regress, give the unknown capture its own `CaptureBacking` variant. Never call `State::tag_name()` on that path: the prefix comes from a `const` table. |
| P5 | Core halves of `8834e161c`, `88e66cbc6`, `9272a2f7c` | Terminal-core owners land them before the C halves (R2 Q5). |
| P6 | `rustty_terminal_reset()` and the new `reset`/program-status `clear` effects | Not fired by the C reset; upstream fires them only for RIS from the stream (R2 Q7). |
| P7 | `Handler` trait additions | `request_xt_checksum`, `xt_checksum_extension`, `osc_unknown`, `mouse_shape_reset`, `program_status` as trait methods with no-op defaults; `UnknownSequence::Osc` added without `#[non_exhaustive]`, internal matches fixed in the same change (R1 Q3). |
| P8 | DECRQCRA/XTCHECKSUM ownership | One workstream (P8-W3) owns vt core, `stream_terminal`, config keys, termio and the C option, so `default_xt_checksum`/`set_default_xt_checksum` have one caller shape. All defaults stay **off** (R1 Q4). `Flags` is a `#[repr(transparent)] u8` with const accessors; no `HashMap`/`OnceLock` on the CSI path (R1 Q5). |
| P9 | `SemanticPrompt` naming | Crate-root `SemanticPrompt` is the event (`main.zig:64`); `stream::SemanticPrompt` stays where it is (R1 Q6). |
| P10 | Render `Device` | Out of line: `App.device: DeviceBox` owning a `Box::into_raw` allocation, created at the reference's point in `App::init`, freed in `App::deinit` after the surfaces; `Options.device: *const Device`. No `Arc` (N522, R3 Q1). |
| P11 | Core `App::init` tests without a GPU | A Rustty-only `#[cfg(test)]` constructor that builds the App with a null `DeviceBox` (deinit skips it), used by the tests that create no surface. Never `#[cfg_attr(miri, ignore)]` them. One non-Miri test creates and destroys a real device (Mac; rvm llvmpipe) (R3 Q2). |
| P12 | EGL extension pre-check | Drop it; port upstream's order (try the platform display, diagnose on `BadParameter`) so log lines and errors match. `OffscreenContext`'s app-scoped half moves into `opengl::Device`; no `eglTerminate` anywhere (Phase 7 B10) (R3 Q3, Q4). |
| P13 | ssh cache `host` vs `host:22` | Port faithfully; not a defect (R3 Q8). |
| P14 | `logWarnOnce` | A per-site `AtomicBool` with `Relaxed` (R3 Q10). |
| P15 | Displaced `DerivedConfig` doc comment in `Termio.zig` | Carry comments in upstream order (G3b); not a UD (R3 Q9). |
| P16 | iTerm2 themes | Re-vendor 657 files at `20260928-151043-99d9701`; update the four hard-coded 617s (R4 §4.1). |
