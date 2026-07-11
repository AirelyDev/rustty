# MARKERS.md — R10 marker-taxonomy census + migration (T20)

Date: 2026-07-10. Task 20 of `docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md`.
Scope: `crates/**/*.rs` (588 files). Oracle checks: ghostty @ `4749c4e9` (`src/`, `pkg/`),
Zig std 0.15.2 (`/opt/homebrew/Cellar/zig@0.15/0.15.2/lib/zig/std`), and the zig package
cache upstreams (`libxev-0.0.0-86vtc4…`, `vaxis-0.5.1`, `z2d-0.10.0`).

## Ratified R10 taxonomy (PORT_RULES.md §R10)

Machine-greppable markers only:
- `TODO(port): <reason>` — unresolvable now; flagging beats wrong code.
- `PERF(port): <zig idiom>` — perf idiom downgraded to the plain form; perf intent is debt.
- `PORT NOTE: <reshape reason>` (incl. the house qualifier form `PORT NOTE (<qualifier>): …`) — sanctioned structural divergence.
- `PORT STATUS: …` — per-file confidence trailer.
- `unimplemented!()` / `todo!()` placeholder leaves stay governed by the placeholder-leaf
  doctrine and its dated C-F endgame audit (NOT rewritten here).

## Headline result

The port's free-form comment-marker debt was **not** in TODO/FIXME space — every one of the
121 free-form `TODO`/`FIXME`/`PERF` comment lines is a **ported upstream comment**
(R1-protected, verified line-by-line against the oracles above; see "Free-form
TODO/FIXME/PERF classification"). The real free-form surface was a bare-`PORT` marker
dialect (`// PORT: …`, `// PORT (qualifier): …`, `PORT-NOTE`, `PORT helpers/support/glue`,
`PORT SCOPE/AUDIT`, `PORT/MIRI NOTE`, `PORT/GLUE`) invisible to `rg "PORT NOTE"`.
**1,015 marker lines in 137 files** were mechanically normalized to `PORT NOTE` forms.
Zero markers were invented, deleted, or reworded beyond the token; zero ported reference
comments were touched.

## Totals (before → after)

| Category | Before | After | Delta |
|---|---|---|---|
| `PORT NOTE` | 4,029 | 5,042 | +1,013 (+2 more inside changed lines: `PORT/MIRI NOTE`, `PORT/GLUE` → `PORT NOTE (…)`) |
| `TODO(port):` | 0 | 0 | 0 — no rustty-authored free-form TODOs exist to migrate |
| `PERF(port):` | 0 | 0 | 0 — see "PERF(port) population" below |
| `PORT STATUS` | 302 | 302 | 0 |
| bare-`PORT` marker dialect | 1,015 lines | 0 | fully migrated |
| free-form `TODO` (all ported, R1) | 107 | 107 | 0 (untouchable) |
| free-form `FIXME` (all ported, R1) | 13 | 13 | 0 (untouchable) |
| free-form `PERF` (ported `PERF(mitchellh)`) | 1 | 1 | 0 (untouchable) |
| `unimplemented!` | 129 | 129 | 0 (placeholder-leaf doctrine; C-F audit owns them) |
| `todo!(` | 10 | 10 | 0 (same doctrine) |

## Per-crate census

PN = `PORT NOTE` lines; migrated = bare-`PORT` lines normalized in this pass;
TODO/FIXME/PERF-free = free-form (all verified ported, unchanged); unimpl/todo! = code
placeholders (unchanged).

| Crate | PN before | migrated | PN after | TODO-free | FIXME | PERF-free | unimpl | todo! |
|---|---|---|---|---|---|---|---|---|
| rustty | 12 | 0 | 12 | 0 | 0 | 0 | 3 | 0 |
| rustty-apprt | 66 | 19 | 85 | 0 | 0 | 0 | 19 | 0 |
| rustty-apprt-api | 76 | 0 | 76 | 4 | 0 | 0 | 0 | 0 |
| rustty-apprt-c | 74 | 0 | 74 | 0 | 0 | 0 | 12 | 0 |
| rustty-args | 55 | 0 | 55 | 1 | 0 | 0 | 0 | 0 |
| rustty-bench | 27 | 0 | 27 | 0 | 0 | 0 | 0 | 0 |
| rustty-cfg-fmt | 16 | 2 | 18 | 0 | 0 | 0 | 0 | 0 |
| rustty-cli | 127 | 7 | 134 | 3 | 0 | 0 | 2 | 0 |
| rustty-config | 191 | 2 | 193 | 1 | 0 | 0 | 0 | 1 |
| rustty-core | 251 | 26 | 277 | 3 | 0 | 0 | 25 | 0 |
| rustty-crash | 0 | 71 | 71 | 0 | 0 | 0 | 0 | 0 |
| rustty-datastruct | 21 | 9 | 30 | 0 | 0 | 0 | 0 | 0 |
| rustty-extra | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 |
| rustty-font | 246 | 10 | 256 | 4 | 0 | 0 | 0 | 0 |
| rustty-font-shaper | 219 | 108 | 327 | 5 | 1 | 0 | 4 | 0 |
| rustty-helpgen | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 |
| rustty-input | 176 | 1 | 177 | 6 | 0 | 0 | 0 | 0 |
| rustty-inspector | 11 | 1 | 12 | 0 | 0 | 0 | 0 | 0 |
| rustty-lib | 50 | 0 | 50 | 0 | 0 | 0 | 0 | 0 |
| rustty-oniguruma | 29 | 0 | 29 | 2 | 0 | 0 | 0 | 0 |
| rustty-os | 3 | 240 | 243 | 6 | 0 | 0 | 1 | 0 |
| rustty-render-types | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 |
| rustty-renderer | 246 | 29 | 275 | 6 | 0 | 0 | 6 | 0 |
| rustty-simd | 2 | 3 | 5 | 2 | 0 | 0 | 0 | 0 |
| rustty-synthetic | 0 | 98 | 98 | 1 | 0 | 0 | 4 | 0 |
| rustty-termio | 140 | 153 | 291 | 5 | 0 | 0 | 4 | 0 |
| rustty-unicode | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 |
| rustty-util | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 |
| rustty-vaxis | 3 | 94 | 97 | 7 | 0 | 0 | 15 | 0 |
| rustty-vt | 1,478 | 2 | 1,480 | 39 | 1 | 1 | 1 | 6 |
| rustty-vt-c | 380 | 0 | 380 | 0 | 0 | 0 | 0 | 3 |
| rustty-wuffs | 20 | 0 | 20 | 0 | 0 | 0 | 6 | 0 |
| rustty-xev | 75 | 140 | 215 | 8 | 0 | 0 | 14 | 0 |
| rustty-z2d | 10 | 0 | 10 | 2 | 11 | 0 | 13 | 0 |
| rustty-zigstd | 4 | 0 | 4 | 1 | 0 | 0 | 0 | 0 |
| (zero-marker crates) | build-config, crash*, global, identity, terminfo, zf | | | | | | | |

(*rustty-crash had 0 `PORT NOTE` but 71 bare-`PORT` markers — the largest dialect pocket
along with rustty-os (240), rustty-termio (153), rustty-xev (140), rustty-font-shaper (108),
rustty-synthetic (98), rustty-vaxis (94, hyphenated `PORT-NOTE`).)

## Migration rules applied (mechanical, uppercase-guarded)

Perl rewrites, per line, guard `(?<![A-Z_])` protects `EXPORT/IMPORT/SUPPORT/REPORT/
SO_REUSEPORT/COMPLETION_PORT/VIEWPORT/…`:

1. `PORT: ` → `PORT NOTE: ` (also covers `(PORT: …)` → `(PORT NOTE: …)`)
2. `PORT (` → `PORT NOTE (` (qualifier form, e.g. `PORT (Zig-std shim):`, `PORT (R2 identity):`, `PORT (DEFERRED — …)`)
3. `PORT §` → `PORT NOTE §`
4. `PORT helper|helpers|support|glue|AUDIT|SCOPE` → `PORT NOTE: <same word>` (section headers)
5. `PORT note` (prose cross-refs whose target marker was renamed) → `PORT NOTE`
6. `PORT-NOTE` → `PORT NOTE` (rustty-vaxis hyphenated dialect, 94 lines)
7. Two one-off slash forms by hand: `PORT/MIRI NOTE:` → `PORT NOTE (MIRI):`
   (rustty-datastruct/src/intrusive_linked_list.rs), `PORT/GLUE:` → `PORT NOTE (glue):`
   (rustty-datastruct/src/f16.rs)

Safety evidence: staged diff = 1,015 −/+ line pairs, every changed line contains a PORT
token, every changed line is comment content (one is a trailing comment on an unchanged
code line, rustty-vaxis/src/queue.rs:31); upstream oracles contain no bare uppercase
`PORT: `/`PORT (` tokens, so no ported comment could match the rules.

## Deliberate residuals (9 lines — prose, not markers)

Line-wrapped or incidental prose mentions of "PORT" that are cross-references to real
`PORT NOTE` markers (or plain prose), left untouched:

- crates/rustty-termio/tests/live_shell.rs:3 ("THE PORT RUNS A REAL TERMINAL" — prose)
- crates/rustty-vt/src/kitty/graphics_exec.rs:409, crates/rustty-vt/src/tmux/viewer.rs:1797,
  crates/rustty-vt/src/dcs.rs:141, crates/rustty-core/src/surface.rs:78 + :3125,
  crates/rustty-renderer/src/shaders.rs:19 + :250 (all "… PORT\n// NOTE …" line-wrapped
  cross-references)
- crates/rustty-crash/src/lib.rs:27 ("PORT-hosted" — prose adjective)

## Free-form TODO/FIXME/PERF classification (all 121 = ported, R1-protected)

Every free-form line was traced to its upstream source text:

- **ghostty `src/`** (majority): e.g. `Terminal.zig` ("spacers should use a bgcolor only
  cell", "TODO(qwerasd): `insertLines`…"), `Screen.zig` ("FIXME: increaseCapacity should
  not do this."), `stream.zig` (7× "TODO: test", "Before shipping an ABI-compatible
  libghostty…"), `termio/Termio.zig:588` ("TODO: fix this"), `Command.zig`, `Exec.zig`
  ("support on windows"), `cli/list_themes.zig` ("Unicode-aware comparison"),
  `cli/args.zig:14`, `input/keycodes.zig` (5× `TODO(garykac)` — Chromium provenance),
  `input/Binding.zig`, `os/{shell,mach,desktop}.zig`, `pty.zig`, `config/Config.zig:1417`,
  `Surface.zig:2096`, `surface_mouse.zig`, `renderer/{shadertoy,generic}.zig`,
  `renderer/metal/RenderPass.zig`, `font/{Atlas,Collection,discovery,main}.zig`,
  `font/sprite/draw/symbols_for_legacy_computing{,_supplement}.zig`,
  `unicode/grapheme.zig`, `simd/vt.zig`, `synthetic/cli.zig`, `terminal/tmux/*`,
  `terminal/search/Thread.zig`, `terminal/PageList.zig` ("TODO: docs", 2× "Unit tests."),
  `terminal/kitty/graphics_storage.zig`, `terminal/Tabstops.zig`, `apprt/action.zig`
  (2× "check non-exhaustive"), `apprt/surface.zig` ("WriteReq"), and the single free-form
  PERF: `Terminal.zig:1503` `PERF(mitchellh)`.
- **ghostty `pkg/oniguruma/region.zig:9`** — "TODO: convert to Zig", faithfully adapted as
  "TODO: convert to Rust" (rustty-oniguruma/src/region.rs:23; the adaptation is itself
  documented in the adjacent PORT NOTE).
- **Zig std 0.15.2** — `heap/memory_pool.zig` (rustty-zigstd, framed "TODO (from Zig):"),
  `hash_map.zig` ("pointer subtraction once supported by zig", rustty-vt/src/hash_map.rs:916).
- **libxev** — kqueue/async/ThreadPool/queue_mpsc TODOs incl. 2× `TODO(mitchellh)`
  (rustty-xev, 8 lines).
- **libvaxis 0.5.1** — `Vaxis.zig`/`Key.zig` TODOs (rustty-vaxis, 7 lines).
- **z2d 0.10.0** — `Transformation.zig`/`internal/{point,arc}.zig`/`internal/tess/*` FIXMEs
  + TODOs (rustty-z2d, 13 lines).
- **Incidental prose** naming an upstream TODO (not markers): rustty-renderer/src/row.rs:4,
  rustty-apprt-api/src/action_tests.rs:7, rustty-cli/src/list_themes.rs:193,
  rustty-font-shaper/src/shaper/coretext.rs:1233 + face/coretext.rs:91 (objc2's TODO).

None may be rewritten to `TODO(port)` — they are ported reference comments under R1, and
R10's greppable-marker rule governs rustty-AUTHORED markers only.

## PERF(port) population — deliberately 0 in this pass

R10 makes `PERF(port)` mandatory where a Zig perf idiom (appendAssumeCapacity, arena
bulk-free, stack-fallback alloc, comptime monomorphization, packed tagged pointer) was
ported as the plain form, resolved by the bench-driven pass before R2 perf parity.
Retro-tagging existing sites is a semantic perf judgment, not a mechanical token rewrite,
so T20 does not tag any (would violate "mechanical rewrites ONLY"). The candidate pool is
now fully greppable for that pass:

- `rg -n "PORT NOTE" crates/ | rg -i "arena"` — ~45 arena-collapse notes (cli/core/termio/config/crash).
- `rg -in "stackFallback|stack.fallback" crates/` — ~10 sites (termio shell_integration/stream_handler, font glyf, vt apc, os shell).
- rustty-z2d keeps `*_assume_capacity` semantics faithfully (no downgrade → no tag needed).

## unimplemented!()/todo!() (139 total) — census only

Governed by the placeholder-leaf doctrine (memory: placeholder-leaf-doctrine) and R10's
endgame C-F audit; concentrated in rustty-core (25), rustty-apprt (19), rustty-vaxis (15),
rustty-xev (14), rustty-z2d (13), rustty-apprt-c (12). The endgame leverage-ranked backlog
(`unsafe_removed × affected_count / effort`) consumes this census.

## Reproduce

```
rg -c "TODO|FIXME|PORT NOTE|PERF|unimplemented!|todo!" crates/ -g '*.rs'   # raw census
rg -n "PORT[ :(]" crates/ -g '*.rs' -P | rg -v "PORT NOTE|PORT STATUS|PORTS|PORTED|[A-Z_]PORT"  # bare-PORT residual check (expect the 9 prose lines)
rg -c "TODO\(port\)|PERF\(port\)" crates/ -g '*.rs'   # R10 forms
```
