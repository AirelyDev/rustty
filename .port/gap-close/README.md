# Gap close — 2026-08-26

Branch `upstream-gap-close`, based on `main` (`29da15fe`, the phase-5 finale).

## Where this came from

A parity audit of Mitchell Hashimoto's public posts for the month 2026-07-26 →
2026-08-26, checking whether every ghostty/libghostty feature he described has a
rustty equivalent. The bulk of it was already green — Kitty Graphics (all nine
`graphics_*` files at test parity, real `shm_open` transport, relative
placements, scroll-region adjust), the Kitty clipboard protocol, the SIMD OSC
rewrite (`8c5bc3d29`, including both differential tests), the occluded-surface
GPU release (`c4e16970a`), the OSC8 untrusted-URL classifier (`77537c806`, Swift
line-for-line), scrollback compression, and the wasm page pool. G1b was GREEN,
159/159 macOS Swift files, and 0 of 209 config keys missing.

Four things were not. This branch closes them.

## The four

### 1. `a177ba90a` display-link refactor — MISSED PORT (real)

"macos: tolerate display link creation failures" (2026-08-05), inside the
audited window. The phase-4 assessment had already caught it —
`assessments/E-os-termio-cli-app.md` row **P4-E28**, ABSENT, MEDIUM — but no
wave claimed it and no deferral record was written, so it fell through the
phase-4 finale's "re-greened" claim. Its `pkg/macos` half (`Error.CreationFailed`
replacing the `OutOfMemory` mis-mapping) HAD landed, which made the gap easy to
miss: the renderer call site was still pre-refactor.

Impact, both real: (a) `Renderer::init` created the display link eagerly and
propagated the error, so a surface created while the macOS session is locked —
no active displays — aborted renderer init, where upstream now falls back to
event-driven rendering without vsync; (b) no `visible` field, so `set_focus`
gated vsync on focus alone and focusing an occluded surface spun the display
link back up.

Closed in `3c518479`. Three port-side regression tests were added for the parts
observable without a real CoreVideo display (both guard clauses of the new
`sync_display_link`, and the visibility state machine); the reference added no
`test {}` because every success path needs a display.

### 2. `-Dvt-features` — HALF-LANDED (real)

`build_options.rs` had the `Features` struct and its parse, but every
`options::*` const was hardcoded `true` behind a PORT NOTE deferring the cargo
plumbing to "W5 Bundle M / W7 T6" — which never ran. rustty had no answer to the
reference's lib-artifact trimming, so nothing corresponded to the read-only
viewer subset.

Closed in `8feeb8e4`: nine cargo features on `rustty-vt` and `rustty-vt-c`, all
in `default` (that is what makes an additive feature set equivalent to the
reference's subtract-from-all model), gating the `options::*` consts and the 146
of 195 C exports that `lib_vt.zig` guards. Measured on wasm32:
1,352,416 → 870,067 raw and 412,694 → 234,900 gzip for `-all,+render-state`.
New gate **G-VTFEAT** (`tools/gates/check_vt_features.py`, wired into `ci.sh`)
derives its expectations from the oracle and was negative-controlled at landing.

### 3. renderer `Thread.zig` — FALSE FINDING (audit error)

The audit reported the 857-line renderer run loop as unported, from
`rustty-renderer/src/thread.rs` being 135 lines and its header saying the loop
"is NOT ported here — deferred with the D6 xev decision". **It is fully ported**,
in `rustty-core/src/renderer_seam.rs`, where R4 forces it to live
(renderer/Thread.zig imports `configpkg` AND core's `App.Mailbox`, and both edges
cycle from `rustty-renderer`).

Re-verified function-for-function before concluding: all 17 reference top-level
`fn`s plus `Compression`'s six, with zero production `unimplemented!()` (all 19
occurrences are header prose or inside `#[cfg(test)] mod compression_tests`).

What was real here is the documentation. Both `thread.rs`'s header and the
`renderer.Thread` row in `phase-2/DEFERRALS.md` still declared the loop DEFERRED
with `unimplemented!()` placeholders long after D6 Milestones A/B and the R-d GPU
tier W0–W5 landed. That is the usual bookkeeping hazard running backwards — not a
green gate resting on nothing, but a landed port documented as missing — and it
cost this audit a false finding. Both corrected in `bb4ca341`, each quoting the
superseded text so the change is legible.

### 4. Upstream drift — 2 commits (real)

`0f35043c9` (execute C0 0x10-0x1F in the ground state) and `40a40f848` (ignore
UTF-8-decoded C1 controls in the ground state), both `src/terminal/stream.zig`,
both landed after the phase-5 pin. Ported in `41442695` with both reference tests
assertion-for-assertion. The oracle pin advanced `683d8db64` → `88f57ee66`
advance-first, per the doctrine phase-4 and phase-5 used.

## Verification

| check | result |
|---|---|
| `rustty-vt` (scalar + `--features simd`) | 2,968 passed / 0 failed, both arms |
| `rustty-renderer` | 91 passed / 0 failed |
| `rustty-core` | 34 passed / 0 failed |
| `rustty-vt-c` | 469 passed / 0 failed |
| G3b comment coverage (vs the ADVANCED oracle) | GREEN, 13,820 distinctive vt comments, 0 missing |
| G1b test counts | GREEN |
| G-VTFEAT (new) | GREEN, 11 configurations, export sets exact |
| rustfmt on every file this branch touches | clean |

## Pre-existing conditions found, NOT fixed here

Both verified to reproduce on `main` at `29da15fe`, so neither is fallout from
this branch. Left alone as out of scope:

* **`check_tests_ported.py` is RED on main** — four crates (`datastruct`, `lib`,
  `vt`, `vt-c`) are missing named reference tests (e.g. `datastruct::SegmentedPool`,
  `lib::cutPrefix`, `vt::size report csi_21_t title`, `vt-c::mode_get null`).
  Base measured 4843 ported / 3227 contract with the same four RED rows; this
  branch is 4848 with the same four.
* **`cargo fmt --all --check` is RED on main** — drift across ~180 files. The
  `build-system-gaps` lane already carries the fix, so a sweep from here would
  only collide with it.
