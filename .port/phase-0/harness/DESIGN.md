# Differential Harness — Comparator Architecture (Phase 0)

**Date:** 2026-06-21
**Reference:** `/Users/gael/projects/ghostty` @ `4749c4e` (Zig 0.15.2), **READ-ONLY**.
**Port target:** `/Users/gael/projects/rustty` (currently EMPTY).
**Governing docs:** `.port/PORT_RULES.md` (R1–R5),
`docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md` §8–§9.

The harness is the *differential-testing oracle* of §8: feed **identical input**
to the reference and the port, serialize each side to a **canonical form**, and
diff. This document describes the comparator; the working skeleton lives beside
it (`comparator.py`, `diff_vt.sh`, `ref_driver/`, `port_hook/`,
`gate_empty_divergence.sh`).

---

## 0. Principles (from the spec / rules)

- **No canonicalization of identity tokens (R2).** The port is built with the
  `identity-ghostty` feature so its identity-bearing output (XTVERSION, Primary/
  Secondary DA, version strings, `TERM`, terminfo name, config dir) equals the
  reference **byte-for-byte**. The comparator therefore performs **raw byte
  diffs** — it never normalizes `ghostty`↔`rustty`. If a divergence is purely an
  identity token, that means the port was built WITHOUT `identity-ghostty`, which
  is itself the bug to surface, not to paper over.
- **Reference is an external oracle only (R3).** The comparator links the
  reference's prebuilt `zig-out/libghostty-vt.a`; it never makes rustty depend on
  the ghostty tree. The ghostty tree is read, never written.
- **Gates never weaken (R5).** Divergence == 0 is the bar. The metric is proven
  sound by a control (reference-vs-reference == 0%) on every run.

---

## 1. The four comparison legs

The spec (§8.2) names four canonical-output channels. This phase delivers leg (A)
end-to-end and specifies (B)–(D) with their exact port hooks.

### (A) VT core — Screen/PageList serialization *(implemented)*

The crown jewel (`rustty-vt`). Canonical form = the **libghostty-vt formatter**
output over the resulting Screen/PageList, exactly as
`example/c-vt-formatter` and `example/c-vt-stream` do:

```
ghostty_terminal_new(opts{cols,rows,max_scrollback})
  -> ghostty_terminal_vt_write(buf, len)          # feed identical VT bytes
  -> ghostty_formatter_terminal_new(emit, trim, unwrap)
  -> ghostty_formatter_format_alloc(&buf, &len)   # canonical bytes
```

`emit` ∈ {`PLAIN`, `VT`, `HTML`}:
- **PLAIN** — visible glyphs only (whitespace-trimmed). Stable, minimal.
- **VT** — glyphs **plus** SGR/color/URL escape sequences, i.e. style-bearing
  canonical form. Used to diff styling, not just text.
- **HTML** — inline-styled HTML; a third independent canonicalization.

Reference impl: `ref_driver/vt_driver.c` (statically linked to the reference
`libghostty-vt.a`). It is a parameterized generalization of the read-only
examples: input comes from a **file or stdin** (not hard-coded), and grid/format
options are CLI flags so BOTH sides receive identical configuration. Output is
the formatter's bytes **verbatim** (no banner, no extra newline) so it is
byte-diffable.

**Known limitation (documented honestly, see §4):** the formatter serializes the
*visible grid* only. Non-printing sequences (OSC title/pwd, DECSET/DECRST modes,
scroll-region, clipboard, DCS/APC queries) leave the visible screen empty and
thus produce empty formatter output. Full VT-core coverage additionally captures
**terminal state** (`ghostty_terminal_get`/`get_multi`: cursor row/col/style,
active screen primary/alt, fg/bg/cursor colors, palette, title, pwd) and **pty
write-back** (the `GHOSTTY_TERMINAL_OPT_*` callbacks: XTVERSION, DA, ENQ, size,
color-scheme query responses). These extend the canonical record beyond glyphs;
they are specified here and are the next increment to `vt_driver.c`. The pty
write-back channel is also where **identity tokens** surface for byte-exact
matching under `identity-ghostty`.

### (B) Config parse-then-serialize diff *(specified)*

Canonical form = parse a config source, then serialize it back via the config
formatter (`config/formatter.zig` → `rustty-cfg-fmt`). Diff the round-tripped
text. The reference oracle is a small driver over libghostty's config API (or the
`ghostty +show-config`-style path); the port hook is the analogous `rustty`
config crate once it exists. Identity-bearing defaults (config dir, `TERM`) match
byte-exact under `identity-ghostty`. Corpus seed dir: `.port/phase-0/corpora/config`.

### (C) Input encode diff *(specified)*

Canonical form = the byte sequence produced by encoding a key/mouse/focus/paste
event (the `c-vt-encode-{key,mouse,focus}` example programs are the reference
oracles; libghostty exposes `ghostty/vt/key`, `mouse`, `focus`, `paste`). Feed an
identical event descriptor to both sides; diff emitted bytes. Port hook: the
`rustty-input` encoders. CSI-u / kitty-keyboard / xterm-mods all covered.

### (D) Render perceptual diff *(specified, explicit tolerance)*

Canonical form = a rendered raster. Because GPU/Metal/OpenGL rasterization is not
bit-exact across drivers, this leg uses a **perceptual** comparison with an
**explicit tolerance**, not byte equality:

- Metric: **per-pixel max channel delta** + **fraction of differing pixels**.
- Default tolerance: a pixel "matches" if all of R,G,B,A differ by ≤ **2/255**;
  an image "matches" if **≥ 99.5%** of pixels match (≤ 0.5% differing) AND no
  pixel exceeds a hard ceiling of **16/255** on any channel.
- These thresholds are config constants (`RENDER_PER_PIXEL_TOL`,
  `RENDER_FRACTION_TOL`, `RENDER_HARD_CEILING`) so they are reviewable and
  tightenable; a render diff above tolerance is a gate failure.
- The reference raster comes from the `c-vt-render` example / the renderer's
  software backend (`renderer-software`) for determinism; the port renders the
  same Screen + font + theme. Glyph/atlas inputs are pinned (same C font libs per
  R3) so only the rasterizer differs. Corpus: snapshot fixtures (§8.3).

Legs (A)–(C) are **byte-exact** (tolerance 0). Only leg (D) carries a tolerance,
and it is explicit and small.

---

## 2. The comparator (`comparator.py`)

```
input VT bytes ─┬─> [ref driver]  --cols/--rows/--scrollback/--format/--trim ─> canonical_ref
                └─> [port driver]  (same flags, same stdin)                    ─> canonical_port
                                                            │
                                                  diff(canonical_ref, canonical_port)
                                                            │
                                       byte_divergence% , line_divergence% , identical
```

- **Identical options to both sides.** `comparator.py build_driver_args()`
  constructs ONE flag vector and passes it unchanged to ref and port. There is no
  asymmetry the diff could falsely attribute.
- **Drivers are interchangeable.** `--ref` and `--port` each accept any executable
  honoring the driver CLI contract (below). The port slot currently points at the
  stub; pointing both at the reference is the **control** (must be 0%).

### Divergence metric (explicit & reproducible)

Headline = **byte divergence**:

```
byte_divergence = 1 - SequenceMatcher(ref_bytes, port_bytes).ratio()
                = 1 - 2*M / (len(ref) + len(port))     # M = matched bytes
```

Symmetric, in `[0,1]`. `0.0` ⇔ byte-identical. `1.0` (100%) ⇔ one side empty and
the other non-empty (the **empty-port** case) or zero common substructure.
A secondary **line divergence** (`1 - matched_lines / max(lines)`) localizes
differences for humans. `identical` is the strict `ref_bytes == port_bytes`
boolean that drives exit codes (0 iff identical). The metric's soundness is
asserted every run by the control leg in the gate (ref-vs-ref must read 0%).

### Driver CLI contract (ref and port MUST both honor)

```
DRIVER [--cols N] [--rows N] [--scrollback N]
       [--format plain|vt|html] [--trim|--no-trim] [--unwrap]
       [INPUT_FILE]            # else read VT bytes from stdin
  stdout = canonical bytes ONLY (no banner / trailing newline)
  stderr = diagnostics only
  exit   = 0 on success
```

`ref_driver/vt_driver.c` implements this against the reference. The port hook
(`port_hook/vt_driver_port.sh`) forwards this contract to a real rustty driver
when one exists (resolved via `$RUSTTY_VT_DRIVER` or `bin/vt_driver_port`),
falling back to the empty stub today.

---

## 3. How the port plugs in (no harness change needed)

When `rustty-vt-c` exists, build a `vt_driver_port` from the **same C source**
(`ref_driver/vt_driver.c`) but `#include <rustty/vt.h>` and link
`librustty-vt.a`, with `--features identity-ghostty`. Drop it at
`bin/vt_driver_port` (or export `RUSTTY_VT_DRIVER`). The wrapper detects it and
forwards argv+stdin unchanged. The comparator, fixtures, gate, and metric are
unchanged — only the port slot becomes real, and the empty-divergence gate flips
from "100% (proves detection)" to the real parity gate "0% (proves parity)".

Because identity is matched via the **build profile**, the comparator does **zero
token canonicalization** — fully consistent with R2.

---

## 4. Honest limitations (surfaced, not hidden — R5)

1. **Formatter sees only the visible grid.** Confirmed empirically: of the 24
   hand-written `stream-initial` fuzz seeds, 11 produce non-empty formatter
   output and 13 (OSC title, DECSET/DECRST, scroll-region, DCS/APC queries,
   clipboard, empty) produce *empty* output under both PLAIN and VT formats. The
   empty-divergence GATE therefore runs over **curated fixtures that produce
   visible content**, where empty-port divergence is unambiguously 100%. Full
   coverage of non-printing state requires the state-query + pty-write-back
   capture described in §1(A); that is the next driver increment.
2. **Render leg (D) is perceptual, not exact** — by necessity (GPU
   nondeterminism). Tolerance is explicit (§1D) and small.
3. **AFL++ not installed** in this environment (see FUZZ-INVENTORY.md): the
   differential-fuzz *wiring* and shared corpus are delivered; live fuzzing
   awaits `afl-cc`/`afl-fuzz` (a tooling gap, not a parity gap).

---

## 5. Files

| Path | Role |
|---|---|
| `ref_driver/vt_driver.c` | Reference VT oracle (libghostty-vt formatter) |
| `build_ref_driver.sh` | Builds `bin/vt_driver_ref` (static link, real clang) |
| `port_hook/vt_driver_port.sh` | Port-side hook (stub until rustty exists) |
| `comparator.py` | Runs both sides, diffs, emits divergence % / JSON |
| `diff_vt.sh` | Thin entry point around `comparator.py` |
| `gate_empty_divergence.sh` | EMPTY-WORKSPACE GATE (proves total-divergence detection) |
| `fixtures/*.vt` | Curated visible-content VT fixtures for the gate |
| `results/` | `empty_gate.csv`, `empty_gate_summary.json` |
| `../corpora/cvt/{stream,osc,parser}-initial/` | Shared differential-fuzz seed corpus |
