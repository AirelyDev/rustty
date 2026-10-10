# rustty crate-graph cycle analysis (pre-Phase-0)

Reference: ghostty @ `246f70287` (this analysis was originally taken at `4749c4e`;
the oracle sync point was advanced by `P3-W10 T3` to `6ad1fe7d8` on 2026-07-31,
by the Phase-4 resync to `12967b68f` on 2026-08-17, by the Phase-5 resync to `683d8db64` on 2026-08-25,
by the gap-close to `88f57ee66` on 2026-08-26, by the Phase-6 resync to `20abdb50a` on 2026-09-01,
by the Phase-7 resync to `12752b2ac` on 2026-09-28,
by the Phase-8 resync to `a60e9e2a5` on 2026-10-07,
then by the Phase-8 second advance to `246f70287` on 2026-10-09 — see
`.port/ORACLE_SYNC.md`), `src/` (512 .zig files at the original analysis). Tooling:
`.port/tools/cycle_check.py` (crate-level SCC) and `.port/tools/reach_scc.py`
(file-level reachability + SCC condensation). Both are kept as **parity gates**:
re-run continuously; fail on any unexpected cross-crate SCC.

## Method

Resolve every `@import("…zig")` to a file, exclude `test {}`-block imports
(Cargo permits cyclic **dev-dependencies**, so test-only edges don't constrain
crate boundaries), build the file graph, run Tarjan SCC. A file-level SCC > 1
that spans multiple proposed crates = an illegal Cargo cycle.

## Headline result

Runtime-only, with **zero** refactoring, `src/` is **one 345-file SCC** spanning
terminal, apprt, font, renderer, cli, config, input, os, inspector, termio,
datastruct, simd, crash. The directory structure is NOT an acyclic crate graph.
Even "obvious leaves" (datastruct, simd) are pulled in by bottom-up hub edges.

## The cycle drivers (ranked)

1. **`global.zig` — the master hub.** Imported bottom-up from `terminal/`,
   `os/`, `crash/`, `renderer/`, `termio/`, `config/`, `cli/`; itself imports
   `apprt`, `cli`, `renderer`, `crash`, `font`, `build_config` top-down. It alone
   glues most of the graph. Nearly every `X↔core` cycle is *only* via `global.zig`.
2. **`quirks.zig → font/main.zig`** (1 edge) — makes the util/assert leaf non-leaf.
3. **`build_config.zig → {apprt, font, renderer}`** — pulls enum types up; imported
   by almost everything.
4. **Upper-half mesh:** `font↔config`, `font→renderer`, `renderer→apprt`,
   `renderer↔inspector`, `inspector→core`, `termio→renderer/apprt`, `config→cli`.

## What breaks cleanly (proven)

**Round 1** — inject `global` (DI/OnceLock), fix `quirks→font`, move
`build_config` enums down, `font/SharedGrid→render-types`, `os/open→apprt`,
split `terminal/c` (vt-c):  345-SCC → **shatters**. `datastruct`, `unicode`,
`simd`, `lib`, `os`, `vt` (55-file terminal core), `vt-c` all become clean.
Residual: a **167-file upper-half SCC** (apprt43, font35, renderer22, cli19,
inspector12, config12, termio9, …).

**Round 2** — add `apprt-api` trait (core→apprt), `font↔config` extraction,
renderer/inspector/termio surface-traits, `args` extraction (config→cli):
167-SCC → **decomposes** into separate crates:
`font`(35), `apprt`(43), `cli`(18), `config`(9) clean; `renderer+inspector+termio`
(45) remain glued pending more trait work.

## Honest cost revision

Original estimate "~5 shared leaf files" was **wrong**. True break-list is **one
big hub (`global.zig`, dozens of call sites) + ~13 architectural seams**:

| # | Break | Mechanism | Frees |
|---|-------|-----------|-------|
| 1 | `global.zig` | split → `rustty-global` leaf (state) + wiring in core | the whole bottom-up mesh |
| 2 | `quirks.zig→font` | relocate the one font-quirk | util leaf |
| 3 | `build_config→{apprt,font,renderer}` | define enums low / generate | build-config leaf |
| 4 | `terminal/c` split | separate `rustty-vt-c` crate | vt / vt-c |
| 5 | `renderer/size.zig` | extract `rustty-render-types` | font↔renderer |
| 6 | `config/formatter.zig`(+`config/key.zig`) | extract `rustty-cfg-fmt` | config↔input |
| 7 | `cli/args.zig`(+`CommaSplitter`,`diagnostics`) | extract `rustty-args` | config↔cli |
| 8 | apprt iface | extract `rustty-apprt-api` trait | apprt↔core |
| 9 | `font↔config` | extract font-config types / DI | font / config |
| 10 | `renderer→apprt` | renderer-surface trait | renderer / apprt |
| 11 | `renderer↔inspector` | inspector hook in render State | renderer / inspector |
| 12 | `inspector→core` | inspector takes refs/trait | inspector / core |
| 13 | `termio→renderer/apprt` | termio-surface trait | termio / renderer |
| 14 | `os/open→apprt` | relocate / inject | os leaf |

Every break is behavior-preserving and individually parity-gated.

## Crown jewel is clean

`rustty-vt` = the lib_vt-reachable terminal core is a self-contained 55-file
internal SCC (one crate, fine). Its only leaks (`search/Thread→global`,
`apc/glyph/Glossary→font`) are app-mode paths the reference itself gates off in
the libghostty-vt build; rustty keeps them behind the existing `sys`
runtime-swappable hooks (no hard font/global dep). High confidence.

## Recommendation: two-tier commitment under the gate

- **FIRM now (proven cheap & clean):** `rustty-util, -datastruct, -unicode,
  -simd, -lib, -global, -build-config, -os, -render-types, -vt, -vt-c, -crash`.
- **TARGET upper tier, carved incrementally under the acyclicity gate:**
  `-cfg-fmt, -args, -apprt-api, -font, -config, -input, -inspector, -renderer,
  -termio, -apprt, -core, -cli`. Each carved when its break (table above) lands;
  the cycle checker + the Rust compiler make a regression impossible to miss.
  Documented fallback: if a split proves too costly, group (e.g.
  `renderer+inspector+termio` → one crate) rather than force it.
