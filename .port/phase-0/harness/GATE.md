# Differential Harness as a Workspace Gate — `gate_empty_divergence.sh`

**Date:** 2026-06-21
**Reference oracle (READ-ONLY):** `/Users/gael/projects/ghostty` @ `4749c4e`
(Zig 0.15.2) → `zig-out/lib/libghostty-vt.a`. NEVER modified; NEVER a Cargo dep
(R3) — it is linked only by the differential **harness** as an external oracle.
**Port under test:** `/Users/gael/projects/rustty` (Cargo workspace; crates are
scaffold stubs in Phase 0/1).
**Governing docs:** `.port/PORT_RULES.md` (R1–R5),
`docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md` §8–§9,
`.port/phase-0/plan/PORT_PLAN.md`, `.port/phase-0/plan/DEFINITION_OF_PARITY.md`,
and `DESIGN.md` (the comparator architecture) next to this file.

---

## 0. What this gate is (and is NOT)

`gate_empty_divergence.sh` is a **harness self-test / soundness control**, not a
parity gate. It proves the differential harness can **detect total divergence**
*before* any port code exists, so that later — when the port emits real output —
a reading of "0% divergence" is trustworthy rather than a metric stuck at zero.

Two legs run on every invocation, over the curated visible-content VT corpus in
`fixtures/*.vt` (the `empty.vt` fixture is deliberately excluded from the
headline metric — both sides serialize to nothing there, which is correct but
uninformative for a "total divergence" proof):

| Leg | What runs | MUST report | Why |
|---|---|---|---|
| **CONTROL** | reference **vs reference** | **0.0000%** on every case | If ref==ref were not 0%, the divergence metric would be broken. Proves the metric is not trivially stuck at 100%. |
| **EMPTY GATE** | reference **vs empty port** | **≥ 99.9%** mean, and **100%** on every case | The empty port emits no canonical output, so divergence is total. Proves the harness *detects* divergence. |

**Pass = both legs hold.** This gate is *expected to PASS now* (port empty) and
to *flip* (start failing → then the real parity gates take over) as crates land.

```
$ ./gate_empty_divergence.sh        # current result, verbatim:
  CONTROL mean divergence = 0.0%   (bad cases: 0)
  MEASURED EMPTY-DIVERGENCE = 100.0%   (over 11 fixtures)   cases below 100%: 0
  GATE PASS: harness correctly detects total divergence (100.0%).
  exit 0
```

---

## 1. How to run

```bash
# Prereq: reference oracle driver built once (reads ghostty zig-out, never writes):
bash .port/phase-0/harness/build_ref_driver.sh

# The gate itself:
.port/phase-0/harness/gate_empty_divergence.sh
# exit 0 = harness sound (control 0%, empty divergence ~100%); non-zero = broken.
```

Tunables (env): `EMPTY_DIVERGENCE_THRESHOLD` (default `99.9`), `PYTHON`
(default `python3`). Machine-readable summary lands in
`results/empty_gate_summary.json` + `results/empty_gate.csv`.

---

## 2. The port hook — wired for the future rustty driver

The comparator feeds **identical** VT bytes + grid/format flags to two drivers
honoring one CLI contract (`DESIGN.md` §2):

- **reference:** `bin/vt_driver_ref` — `ref_driver/vt_driver.c` statically
  linked to ghostty's `libghostty-vt.a`.
- **port:** `port_hook/vt_driver_port.sh` — resolves a real rustty driver in
  this order, falling back to the **empty stub** (emits nothing, exit 70):
  1. `$RUSTTY_VT_DRIVER` (explicit path override);
  2. `bin/vt_driver_port` (a built artifact, if present);
  3. opt-in auto-build via `build_port_driver.sh` when `RUSTTY_AUTOBUILD_PORT=1`.

`build_port_driver.sh` compiles the **same** `ref_driver/vt_driver.c` source —
the only difference from the reference build is the linked library:
`librustty-vt.a` (rustty's own C ABI from `rustty-vt-c`) instead of ghostty's.

**Today this is correct-by-construction:** `rustty-vt-c` is a scaffold stub. Its
staticlib exports **zero** C ABI symbols, so `build_port_driver.sh` fails at link
and refuses to emit a binary (it surfaces a precise blocker, exit 13 — it never
fabricates a driver, R5). The hook therefore falls through to the empty stub, and
the gate keeps reading **100% divergence**. That is the intended Phase-0 state.

---

## 3. How Phase 2 flips this from 100% → 0% (crate by crate)

The flip is driven entirely by the **port slot** becoming real; the comparator,
fixtures, metric, threshold, and gate script **do not change** (R5: gates are
invariant — we never loosen them to pass).

### 3.1 The single trigger: a working `rustty-vt-c` C ABI

When break **#4** lands (PORT_PLAN crate 12: `terminal/c/` → `rustty-vt-c`),
`rustty-vt-c` emits, via cbindgen, the C ABI the driver calls:
`ghostty_terminal_new`, `ghostty_terminal_vt_write`,
`ghostty_formatter_terminal_new`, `ghostty_formatter_format_alloc`,
`ghostty_{terminal,formatter}_free`, `ghostty_free` (named `ghostty_*` under the
`identity-ghostty` profile — see §4). At that point:

```bash
# build_port_driver.sh links vt_driver.c against rustty's librustty-vt.a and
# produces bin/vt_driver_port — no edit to the gate or comparator required:
bash .port/phase-0/harness/build_port_driver.sh
.port/phase-0/harness/gate_empty_divergence.sh   # now measures REAL divergence
```

The port slot is no longer the empty stub; `gate_empty_divergence.sh` now
measures **real** ref-vs-port divergence. Its hard assertion "every case == 100%"
will **fail** the moment the port emits *any* correct output — which is the
**signal to retire this control** and switch to the real parity gate (§3.3).

### 3.2 The gradient: divergence shrinks as `rustty-vt` lands

`rustty-vt-c` is a thin C ABI over `rustty-vt` (PORT_PLAN crate 11, the crown
jewel: 95 files, ~2001 tests). The VT-core formatter output is produced by
`rustty-vt`, so divergence falls as `rustty-vt` is ported, in the same
leaves-first order its dependencies require (`datastruct, unicode, simd, os, lib,
util, render-types` first):

- **100%** — empty port (today): port emits nothing.
- **partial** — `rustty-vt` parses/serializes *some* sequences correctly: the
  per-fixture `byte_divergence_pct` drops below 100; the corpus mean falls. Each
  fixture is an independent dial — `basic_text.vt` may hit 0% while
  `fuzz_21-osc-hyperlink.vt` still diverges, localizing exactly which VT feature
  remains unported. `comparator.py --show-diff` prints the unified diff per case.
- **0%** — full VT parity: every fixture is byte-identical; `identical == true`;
  the comparator's exit code is 0 on every case.

The same divergence corpus is the differential-**fuzz** corpus
(`.port/phase-0/corpora/cvt/*`, and `synthetic/` generators per PORT_PLAN crate
29) — as fuzzing widens input coverage, any newly-found divergence is a blocker,
not a tolerance to widen.

### 3.3 From control to parity gate (the hand-off)

Once a real `bin/vt_driver_port` exists, the **parity bar is 0%, not ~100%**:

- The day-to-day gate becomes `diff_vt.sh` / `comparator.py` over the corpus with
  the bar **`byte_divergence_pct == 0` (and `identical == true`) on every case**
  — exit 0 iff byte-identical. This is leg (A) of `DESIGN.md`.
- `gate_empty_divergence.sh` (this control) is kept as a **regression guard on
  the metric itself**: pointing both `--ref` and `--port` at the reference must
  still read 0% (the CONTROL leg), proving the comparator never silently
  canonicalizes differences away. Its "100% vs empty" assertion is only
  meaningful while the port is empty; after the flip it is documentation of the
  baseline, not a passing condition.
- The other legs come online the same way, each just a new port driver in the
  same comparator (`DESIGN.md` §1): (B) config parse-then-serialize via
  `rustty-cfg-fmt`/`rustty-config`; (C) input-encode via `rustty-input`; (D)
  render **perceptual** diff (the only leg with an explicit, small tolerance).

No gate is ever weakened, xfailed, or loosened to pass (R5). A red gate that
cannot be made green honestly is a STOP-and-surface blocker.

---

## 4. Why outputs are byte-comparable: the `identity-ghostty` profile (R2)

The port's published identity is **`rustty`** (binary `rustty`, `rustty_*` C
symbols, `include/rustty.h`, `TERM=xterm-rustty`, terminfo `rustty`, version /
XTVERSION / DA reporting rustty). If the port emitted those tokens, its canonical
output would differ from the reference wherever an identity token appears
(XTVERSION, Primary/Secondary DA, version strings echoed into the stream, etc.) —
and a raw byte diff would report false divergence.

The fix is **not** canonicalization in the comparator. `comparator.py` performs
**raw byte diffs** and never rewrites `ghostty`↔`rustty` (DESIGN.md §0). Instead,
the **single source-of-truth crate `rustty-identity`** carries a build feature
**`identity-ghostty`** that flips *every* identity value to the reference's:
`rustty`→`ghostty`, `xterm-rustty`→`xterm-ghostty`, the cbindgen header path and
`rustty_*`→`ghostty_*` symbol prefix, version/XTVERSION/DA payloads, config dir,
terminfo name. The port driver for the harness is built **with
`--features identity-ghostty`**, so:

- the C ABI it exports is named `ghostty_*` and lives at `ghostty/vt.h` — the
  **same** header the shared `vt_driver.c` `#include`s — so one C source compiles
  and links unchanged against either library (this is exactly why
  `build_port_driver.sh` can reuse `ref_driver/vt_driver.c` verbatim);
- any identity token the port emits into the canonical stream equals the
  reference **byte-for-byte**, so a `0%` reading means real behavioral parity,
  not a token that happened to be normalized.

Consequence (R2 + R5): if a divergence is *purely* an identity token, that means
the port was built **without** `identity-ghostty` — which is itself the bug to
surface, not to paper over. The harness only ever links the *parity profile*
build of the port; the `rustty`-identity build is what ships.

The identity-EQUIVALENCE gates from the original constitution (ghostty.h ABI
match, Swift-app-links-unchanged, terminfo==ghostty) are **dropped** under R2;
all BEHAVIORAL parity gates — including this harness and the 0% byte-divergence
bar — remain fully in force.

---

## 5. Files

| Path | Role |
|---|---|
| `gate_empty_divergence.sh` | THE control gate (this doc). Control 0% + empty 100% ⇒ harness sound. |
| `comparator.py` | Runs both drivers on identical input/flags; emits byte/line divergence % + JSON. |
| `diff_vt.sh` | Thin entry point around `comparator.py` (the day-to-day 0%-bar parity driver post-flip). |
| `ref_driver/vt_driver.c` | The ONE C source both drivers are built from. |
| `build_ref_driver.sh` | Builds `bin/vt_driver_ref` (links ghostty `libghostty-vt.a`, read-only). |
| `build_port_driver.sh` | Builds `bin/vt_driver_port` (links rustty `librustty-vt.a`, `identity-ghostty`). Blocks honestly while `rustty-vt-c` is a stub. |
| `port_hook/vt_driver_port.sh` | Port slot: resolves real driver → else empty stub (100% divergence). |
| `fixtures/*.vt` | Curated visible-content VT corpus for the divergence metric. |
| `results/empty_gate_summary.json`, `results/empty_gate.csv` | Last run's machine-readable verdict. |

---

## 6. Current status (this run, no fabrication)

- `gate_empty_divergence.sh` **runs and PASSES**, exit `0`.
- CONTROL (reference vs reference) mean divergence = **0.0%** (metric sound).
- EMPTY GATE (reference vs empty port) divergence = **100.0%** over 11 fixtures,
  **0** cases below 100% (total divergence correctly detected).
- `build_port_driver.sh` correctly **BLOCKS** (exit 13): `rustty-vt-c` is a
  scaffold stub exporting zero C ABI symbols, so no port driver is produced and
  the gate stays at 100% — the intended Phase-0 state.
