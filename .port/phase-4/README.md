# Phase 4 — upstream resync (pin → oracle head, 2026-08-17)

Phase 4 closes the gap between the Phase-3 sync point and upstream ghostty's `main`,
absorbing the whole range with **no deferrals** (explicit user direction, 2026-08-17).

## Range

```
from  6ad1fe7d8cbda36c77b337a96c9bea8a77883699   2026-07-29  url: exclude trailing spaces from path matches (#13505)
to    12967b68f7d46bdbfb2cfffb6768332fb9db68c0   2026-08-17  libghostty: option to retain continuations on snapshot decode (#13878)
```

543 commits (359 non-merge). **381 files, +72,554 / −8,106.**

**Unlike Phase 3, the oracle working tree was advanced FIRST** (explicit user direction),
so during Phase-4 porting the tree-reading gates measure against the TARGET:

- G1b arrived **RED**: 10 modules, ~36 dropped tests (`w0/g1b-after-at-12967b68f.txt`) —
  formatter 2, apc 4, stream 8, kitty 11, modes 1, input_key_encode 3,
  cli_ssh_cache_diskcache 1, font_shaper_harfbuzz 1, renderer_cell 1, apprt_api_surface 4.
- G3b's missing counts grew by the range's new reference text (`w0/g3b-after-at-12967b68f.txt`).

Both were GREEN at `6ad1fe7d8` immediately before the advance
(`w0/*-before-at-6ad1fe7d8.txt`). **They must be GREEN again at the finale.**

## Artifacts (generated, do not hand-edit)

| File | What |
|---|---|
| `UPSTREAM_DIFF.patch` | the complete diff, 101,411 lines |
| `UPSTREAM_DIFF_STAT.txt` | per-file `--stat` (382 lines) |
| `UPSTREAM_COMMITS.txt` | 359 non-merge commits, oldest-last |
| `UPSTREAM_COMMITS_WITH_MERGES.txt` | all 543, for PR-boundary context |
| `w0/` | before/after gate states for the pin advance |
| `assessments/*.md` | per-area re-vetting verdicts |
| `wf-*.js` | Workflow orchestration scripts for this phase |

Regenerate with:

```sh
cd ../ghostty   # working tree now AT the pin = origin/main = 12967b68f
git diff  6ad1fe7d8..12967b68f            > ../rustty/.port/phase-4/UPSTREAM_DIFF.patch
git diff  --stat 6ad1fe7d8..12967b68f     > ../rustty/.port/phase-4/UPSTREAM_DIFF_STAT.txt
git log   --oneline --no-merges 6ad1fe7d8..12967b68f > ../rustty/.port/phase-4/UPSTREAM_COMMITS.txt
git log   --oneline            6ad1fe7d8..12967b68f > ../rustty/.port/phase-4/UPSTREAM_COMMITS_WITH_MERGES.txt
```

## Binding rules (inherited from Phase 3 — see `.port/phase-3/README.md` and the plan)

- **Port from the RANGE DIFF, never commit-by-commit** (reverted/superseded commits in a
  range are a proven trap). Where several commits rewrite the same function, port the END
  STATE and carry every commit's regression tests.
- **Re-vet, do not assume.** "Already ported" verdicts need evidence: rustty symbol,
  file:line, and a test/gate that fails on regression. Hash citations are not coverage
  in either direction (measured in Phase 3).
- **Every task states its NEGATIVE CONTROL** — the mutation that must make its check fail.
- R1 full fidelity (comments, tests assertion-for-assertion, error paths); R5 gates are
  never weakened; R7 arithmetic/narrowing discipline; R9 unsafe accounting; DG5 abort-
  at-C-ABI; raw-pointer args for Page/Row/Cell/Screen re-read seams (SB UB otherwise).
- Oracle reads: the working tree IS the target now — read files directly; `git show` is
  only needed for intermediate states of superseded commits.

## Area breakdown (drives the assessment fan-out)

Largest new subsystem: `src/terminal/snapshot/` (~16k lines incl. `.ksy` spec and hex
golden testdata) + `src/terminal/c/{snapshot,io}.zig` + `include/ghostty/vt/{snapshot,io}.h`
+ `src/lib/TinyIo.zig` + `src/crc32c.zig` — a full terminal-state serialization codec with
streaming C API. Other majors: wasm allocator infra (`src/lib/allocator/wasm.zig`,
`src/datastruct/wasm_page_pool.zig`, `src/datastruct/limited_allocator.zig`), packed cell
layout / ABI metadata (`src/lib/packed.zig`, `src/lib/c_abi.zig`, types.schema.json),
`src/fastprint.zig`, `src/terminal/fraction.zig`, `src/terminal/stream_continuation.zig`,
kitty graphics_storage churn, formatter pending-wrap, `-Dvt-features`, cli new_tab +
SSH terminfo, os/{edit,stderr}.zig, macOS wave, po/ i18n wave.

Files upstream DELETED (port must delete too, or justify keeping):
`src/datastruct/{array_list_collection,lru,segmented_pool}.zig`,
`src/lib/allocator/convenience.zig`, `src/lib/string.zig`, macOS helpers
(`AnySortKey.swift`, `CrossKit.swift`, `HostingWindow.swift`, iOS files),
`.github/ISSUE_TEMPLATE/{config.yml,preapproved.md}`.
