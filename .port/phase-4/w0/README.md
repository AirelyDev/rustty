# Phase-4 pin advance — gate evidence (2026-08-17)

The oracle working tree was advanced `6ad1fe7d8` → `12967b68f` (= `origin/main`) at the
**start** of Phase 4 (explicit user direction; contrast `P3-W10 T3`, which advanced at the
finale). Preconditions verified: no worktrees/lanes active in either repo, oracle
`status --porcelain` empty, rustty `main` clean, `cargo build --workspace` green (41s).

## Files

| file | what |
|---|---|
| `g3b-before-at-6ad1fe7d8.txt` | G3b at the old pin — GREEN, missing=0 in every examined crate |
| `g1b-before-at-6ad1fe7d8.txt` | G1b at the old pin — GREEN, every module rust ≥ zig |
| `g3b-after-at-12967b68f.txt` | G3b at the new pin — RED, 1,230 missing (per-crate below) |
| `g1b-after-at-12967b68f.txt` | G1b at the new pin — RED, 10 modules / 36 dropped tests |

## G3b per-crate delta (missing sentences, before → after)

All crates were 0 before. After (17 non-zero of 22 examined; 7 crates remain SKIPPED
pre-existing: apprt, apprt-api, bin, cfg-fmt, font, input, renderer):

| crate | missing | | crate | missing |
|---|---:|---|---|---:|
| vt | 921 | | crash | 13 |
| vt-c | 187 | | simd | 11 |
| config | 24 | | cli | 8 |
| lib | 15 | | core | 8 |
| os | 14 | | termio | 8 |
| synthetic | 6 | | terminfo | 5 |
| inspector | 4 | | bench | 2 |
| datastruct | 2 | | args | 1 |
| global | 1 | | | |

The advance is bidirectional, but this range deleted little reference text the port
carried (before-state had missing=0 and the after-state's `datastruct` row is 2 despite
upstream deleting three whole files there — deletions REMOVE tracked sentences; the
port-side deletions are Phase-4 work, see the area-G assessment).

## G1b dropped-test targets (module: drop)

formatter 2, apc 4, stream 8, kitty 11, modes 1, input_key_encode 3,
cli_ssh_cache_diskcache 1, font_shaper_harfbuzz 1, renderer_cell 1, apprt_api_surface 4.

**Both gates MUST be GREEN again at the Phase-4 finale.** These files are the frozen
before/after record required by `.port/ORACLE_SYNC.md` "Advancing the pin".
