# Phase 7 decisions

Range `20abdb50a..12752b2ac` (Ghostty `origin/main`, 2026-09-28). This file settles every question
the five delta reviews raised (`.port/phase-7/delta-review/R1…R5`). It is part of the spec that
`docs/superpowers/plans/2026-09-28-phase7-upstream-resync.md` implements.

**Rule (maintainer, 2026-09-28):** Rustty matches Ghostty's latest upstream exactly, except it
never copies a Ghostty bug. Applied as follows:
- Upstream changed behaviour Rustty already had: adopt upstream's version, unless upstream's
  version is itself a defect.
- A suspected upstream defect: measure it first. If it is confirmed, fix it in Rustty and record
  it as a UD (next free id UD352). If it is not confirmed, port it as upstream has it.
- A Rustty-only CI leg, file or behaviour whose reference counterpart upstream removed: remove it.
- A mechanism with no Rust counterpart (Zig build internals): record it in `NON_PORTS`.

Only one decision needs the maintainer; it is marked **OPEN** at the end.

## Doctrine

| # | question | decision |
|---|---|---|
| D1 | Advance-first, as Phases 4–6? (R1 Q1, R3 Q4) | Yes. The pin moves in P7-W0, so every red gate afterwards is attributed to the advance. A5's registry arm already reads `origin/main` and is red since the fetch; record that as pre-existing. |
| D2 | Ship the macOS display-link deadlock (`72cf50855`) before the advance? (R2 Q10) | Yes, first task. It is a reachable deadlock that Rustty carries. |
| D3 | Windows-only commits: rwin before merge, or at the finale? (R1 Q11) | Before merge. Each Windows commit (`0b5446364`, `60b43068c`, and the fontconfig GNU re-measure) must pass on rwin before its workstream merges. |
| D4 | Re-anchor UD `reference:` lines (R5 Q5) | Re-anchor every UD whose cited code changed, plus the ones listed in R5 Appendix A. Leave the rest stamped `at 20abdb50a`. |

## Behaviour re-alignments (upstream changed behaviour Rustty already had)

| # | item | decision |
|---|---|---|
| B1 | Empty C outputs (`dd2edd760`; reverses N331/N361) | Adopt upstream: NULL for allocated empty outputs, real storage for borrowed empty strings. The Rust rendering of Zig's `""` is a crate-level `static EMPTY: [u8; 1]` in `rustty-lib`, because Rust does not guarantee that `"".as_ptr()` is real storage (R3 Q3). |
| B2 | Rustty-only guard tests that assert the old behaviour (`key_encode.rs:3040-3080`, `allocator.rs:165-180`, `formatter.rs:985-1022`) | Rewrite the assertions to the new behaviour, and keep each test's still-valid half ("never call the C allocator for zero bytes"; a single raw byte written verbatim) (R3 Q2). |
| B3 | OSC integers (`73768913b` and follow-ups; UD141) | Adopt `parse_int` at all 9 upstream sites: reject digit separators, and reject signs on unsigned fields. Narrow UD141 to the glyph APC and kitty graphics sites, which are still defective upstream. |
| B4 | `parseInt` home (`500040d3b`) | Move to `crates/rustty-lib/src/parse_int.rs`, mirroring `src/lib/parse_int.zig` (R1, R3 Q5). |
| B5 | tmux list-windows (`a41248013`, `9a4ba7d54`) | Faithful: append the action after `sync_layouts`, use the new errdefer, print `[N windows]`. The action borrows the viewer's windows list the way upstream does: a raw slice valid until the next viewer mutation, as in Zig. Port upstream's pointer-identity assertion, and cover the borrow with a Miri target (R1 Q4). Retires LEDGER S209-F3. |
| B6 | Kitty temp-dir path (`105d0a545`) | Faithful: one wrapper-owned copy from the caller's allocator, which every screen borrows (R1 Q3). |
| B7 | Dynamic palette (`d1cd56a56`) | Faithful: an owner enum (`&'static Palette` built-in, or a `VtAllocator`-owned copy). Losing `Copy` is accepted. The termio half moves in the same change (R1 Q10). |
| B8 | Wuffs for kitty zlib (`4b86e3594`) | Faithful: `rustty-wuffs` becomes a mandatory dependency of rustty-vt and rustty-vt-c whenever kitty graphics is on, including wasm32 as upstream builds it. `zlib-sys` leaves rustty-vt, and the wasm `Unsupported` stub is removed (R1 Q5, R4 Q6). |
| B9 | GL function loading after the GtkGLArea removal | Faithful: a per-render-thread function table (a struct-based `gl_generator` binding), loaded in `thread_enter` and dropped in `thread_exit`, as glad's thread-local context is (R2 Q1, option b). |
| B10 | EGL display teardown | Faithful: never call `eglTerminate`, as upstream. `EGL_SURFACE_TYPE` is `0` (R2 §3.2). |
| B11 | Windows OpenGL target now requiring EGL | Faithful: `OpenGL::init` loads EGL on every target as upstream does. First check whether upstream links `glad_egl` for Windows at all; mirror whatever upstream builds (R2 Q5). |
| B12 | UD150 (`5ca09acf6`, version string) | Upstream fixed it the same way. Port upstream's trim set (`std.ascii.whitespace`), set the status to fixed upstream, and retire the R13 PORT NOTE. |
| B13 | UD31 embedded half (`a1bf5b5e1`) | Port the new routing and log text. The GTK half stays faithful. |
| B14 | fontconfig oddities (stale `FC_VERSION_*` 2.14.2 defines; `fcconffile.c` omitted) | Faithful. Both are unobservable: nothing reads the defines, and nothing calls the omitted API. Record them in NEW-FINDINGS as latent notes, not UDs (R4 Q4). |
| B15 | fontconfig IA__ alias symbols on ELF | Faithful; the change in symbol shape is accepted (R4 Q5). |
| B16 | fontconfig override headers | Copy upstream's 13 headers verbatim (R4 Q3). |

## Suspected upstream defects (measure, then fix or port)

| # | candidate | decision |
|---|---|---|
| M1 | UD200: the opener's last stderr line is lost at EOF (`3b8141fbd`) | R5 measured it with zig 0.16.0: upstream drops an unterminated last line. That breaks the loop's evident intent, which is to log the opener's stderr. Keep Rustty's handling. UD200 becomes "partly fixed upstream in 3b8141fbd (hang, reap); remaining: EOF tail lost". |
| M2 | GTK error page unreachable after `2f0b65346` (R2 Q2) | Measure: confirm at `origin/main` that no path calls `setError(true)`. If confirmed, restore the error page on surface-init failure (R13 evident intent: the page exists for this failure) and open a UD. |
| M3 | Y-axis snap inverted under the OpenGL flip (R2 Q3) | Measure on rvm at a fractional scale. If confirmed, fix and open a UD; otherwise port as is. |
| M4 | DMA-BUF fd leaks in `Planes.validate` and on `alloc.create(Planes)` failure (R2 Q4) | A leak is never ported. Fix both, with fail-first tests using an injected invalid plane and a failing allocator, and open a UD. |
| M5 | Render target always sRGB with `alpha-blending = native` (R2 Q9) | Measure with the offscreen golden set to `native`. If output changes, treat it as a defect (fix and UD); otherwise port as is. |
| M6 | CSI 8 t size arithmetic (`714fe9b90` core, `219cca932` GTK) | Port with the checked helpers (`u32_from_ceil`, checked sums). Measure reachability with `adjust-cell-width` and a 65535-column request; if it traps upstream, open a UD (core) and a UD (GTK `c_int`). |
| M7 | Wuffs png/jpeg decoder storage left unaligned upstream | Rustty already over-aligns png. Measure whether jpeg needs it; if it does, fix and open a UD. |

## Records and gates

| # | item | decision |
|---|---|---|
| R1 | The search tick use-after-free (`b1c264163`) and the search `unreachable!()` (`06178eeaa`), which Rustty carried unrecorded | New NEW-FINDINGS entries, closed by the port, citing the upstream fix (R1, R3 Q6). |
| R2 | UDs that upstream fixed and Rustty now matches exactly (UD150; UD140's zlib half) | Keep the UD entry with `upstream status: fixed upstream in <sha>`. Remove the fully fixed row from `docs/ghostty-bug-fixes.md`, and reword partial rows to the remaining difference. The UD-docs check moves from the gitignored `.superpowers/` directory into `.port/tools/gates/check_ud_docs.py`, a tracked `ci.sh` gate that exempts fully fixed-upstream entries (R5 Q2). |
| R3 | UD131 / UD351 private advisory | Unchanged: the maintainer decides when to report. Re-verify UD131's recipe at the new pin first (the page layout change may move the line count). |
| R4 | S119-F3 (waiver resting on the removed GtkGLArea path) | Re-adjudicate in the same task that lands the redesign. G-PARITY stays green in between (R5 Q6). |
| R5 | TinyIo | Stays a non-port; the reopen trigger does not fire. Extend the NON_PORTS row to `src/lib/tinyio/{posix,windows,test}.zig` (R3, R5 Q7). |
| R6 | Windows CI legs upstream dropped (`test-windows`, `build-libghostty-windows-gnu`) | Remove them from Rustty's `windows` job, for parity (R4 Q2, R5 Q9). |
| R7 | Retroactive docs note for macOS Find Previous (`e347482fb`) | None; upstream and Rustty agree after the port (R2 Q6). |
| R8 | `passthrough.glsl` at the upstream root | NON_PORTS: an unreferenced file (R2 Q8, R4 Q8). |
| R9 | `7adbb5160` (uucode export), `7644e6d62`, translate-c/cImport, Zig package and Nix renderings, GTK build cache, VOUCHED | NON_PORTS rows (R4). |
| R10 | Gates that read `origin/main` (A5, G-BENCH, G-LZ4) or frozen inventories (G9, G1, D3) | No redesign. Regenerate the frozen inventories (G9 `parity-surface.json`) at the new pin during the advance. `R5-upstream-tests.tsv` is the checklist for the 40 changed test bodies no gate sees (R5 Q10). |
| R11 | G4 canonical serialization and page capacity (R5 Q11) | The P7-W0 advance task checks it. If the format includes capacity, G4 is expected red until the memory model lands; record that in the "after" snapshot. |
| R12 | `modeFromInt` exhaustive test under Miri (131,072 iterations) | Accept an R14-declared `cfg_attr(miri, …)` iteration cap, printed; full run outside Miri (R1 Q7). |
| R13 | `UntouchedPool` test injection | Make the page allocator a generic parameter defaulting to `PageBackingAlloc`, so the new PageList tests port assertion for assertion (R1 Q8). |
| R14 | N88 geometry re-probe | Run after the advance, from the oracle at the new pin (R1 Q9). |
| R15 | CSI 8 t ownership | One cross-tier workstream owns the whole feature: vt, termio, config, core, apprt-api, embed header, GTK and Swift (R1 Q6). |

## OPEN (maintainer)

- **`CLAUDE.md`** (`9d2d9acac`). Upstream deleted its `CLAUDE.md -> AGENTS.md` symlink. The
  parity rule says to delete Rustty's too, and `AGENTS.md` stays. Side effect: Claude Code loads
  `CLAUDE.md` automatically, so agents in this repository would no longer pick up the project
  instructions unless they are loaded some other way.
