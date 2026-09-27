# Phase 3 — deliberate NON-PORTS (area E: OS / termio / platform)

**Status: CLOSED record, not a backlog.** Every row below is a decision that has
already been taken and executed. None is open work, none needs an owner, and
none is a deferral. Referenced from `.port/phase-2/DEFERRALS.md`'s
`# Justified R1 Divergences` section so a future mechanical sweep of the
upstream diff cannot "discover" these hunks and port them by reflex.

Produced by **`P3-W7` Task 8** (`docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md`).
Source of the verdicts: `.port/phase-3/assessments/E-os-termio-platform.md`.

## The 4-criterion basis

Every row states all four, because a non-port recorded without them is
indistinguishable from an oversight:

1. **What upstream did** (commit + file).
2. **Why the Rust port does not do it** — the mechanism, not a preference.
3. **What would break if we DID port it** — the cost of the wrong decision.
4. **What holds the line** — the test, gate, or structural fact that keeps this
   true, or an explicit statement that nothing does and why that is acceptable.

---

## E34 — `src/quirks_memset.zig` (NEW FILE, +338) — do NOT port

1. **Upstream.** `20c3eae04` "memset should match the C ABI" + `e8525c0fd`.
   A new 338-line file exporting a strong, hidden-visibility `memset` that
   overrides compiler_rt's, manually `@Vector`-ized with an aarch64 `dc zva`
   fast path (≥16 KiB, zero fill, 64-byte block) and `asm volatile ("")`
   barriers to defeat `LoopIdiomRecognize` recursion. Wired by
   `comptime { _ = @import("quirks_memset.zig"); }` in four artifact roots
   (`main.zig`, `main_c.zig`, `main_bench.zig`, `main_gen.zig`). Ships 3 tests.
2. **Why not.** The motivation is a **Zig-toolchain artifact with no Rust
   counterpart**: Zig 0.16.0 globally disabled LLVM loop auto-vectorization
   (LLVM 21 miscompile llvm/llvm-project#186922), which turned compiler_rt's
   byte-loop `memset` into a ~24× slower scalar loop — 63 % of executed
   instructions in `+terminal-stream`, a 2.8× total slowdown. Rust does not link
   compiler_rt's `memset`: `rustc` lowers `@llvm.memset` to the **platform
   libc** `memset`, which on macOS is Apple's hand-tuned `libSystem`
   implementation (already SIMD + `dc zva`) and on Linux glibc's IFUNC-dispatched
   one. Rust also does not disable LLVM loop vectorization, so the workaround has
   no analogue.
3. **What porting it would break.** Exporting a strong `memset` from a Rust
   dylib would *pessimize* rustty relative to libc and risk ODR/interposition
   breakage across the C API boundary. There is nothing to gain and a real
   hazard.
4. **What holds the line.** The perf baseline is the guard, not a test:
   `perf-baseline-rust-vs-zig` (14/14 wins, suite −21.7 %) was measured with
   rustty using libc `memset`.
   **MEASUREMENT HAZARD, recorded here because it will otherwise be misread:**
   this commit makes the **Zig oracle faster** than it was at the pin. Any
   future rustty-vs-Zig benchmark against `origin/main` will show a *smaller*
   rustty margin than the recorded baseline, and that is the oracle getting
   faster, **not** rustty regressing. The oracle build also requires
   `-Doptimize=ReleaseFast` (already recorded in the perf memory).

## E17 — `os/hostname.zig`'s `isValid` deletion — KEEP rustty's; do NOT mirror

1. **Upstream.** `e663d5405` "os/hostname: switch to `std.Io.net.HostName.validate`"
   deletes the hand-rolled RFC 1123 `isValid` **and its 25-assertion test**; the
   one caller (`cli/ssh-cache/DiskCache.zig`) uses the stdlib validator instead.
2. **Why not.** **Rust's std has no RFC 1123 hostname validator.** Mirroring the
   deletion would not move the validation somewhere else — it would silently
   *drop* it.
3. **What porting it would break.** `crates/rustty-cli/src/ssh_cache/disk_cache.rs`
   would accept malformed hostnames into the on-disk ssh cache, a file the user
   later sources. rustty's guard is now **stronger** than upstream's, which after
   this commit has none of its own.
4. **What holds the line.** `crates/rustty-os/src/hostname.rs` `isValid` and its
   test block, plus this record — the whole point of which is that a future
   mechanical sweep of the deleted-symbol list does not delete it.

## E25 — `os/windows.zig`'s rewrite (+344) — ALREADY AT THE DESTINATION SHAPE

1. **Upstream.** `e8525c0fd` rewrites `os/windows.zig`: stdlib re-exports are
   replaced by locally-declared constants and `extern` decls (Zig is removing
   `std.os.windows`); `exp.kernel32` gains `SetHandleInformation`, `CreateFileW`,
   `CreateNamedPipeW`, `CloseHandle`, `VirtualAlloc/Free`,
   `WaitForSingleObject`, `GetExitCodeProcess`, `TerminateProcess`,
   `CancelIoEx`, `ReadFile`; new `exp.ntdll.NtCreateFile`; `FALSE`/`TRUE` become
   `BOOL` values; `HPCON`/`STARTUPINFOEX`/`PROC_THREAD_ATTRIBUTE_*` are hoisted
   out of `exp`.
2. **Why not.** The change is "stop leaning on `std.os.windows`, declare your
   own". `crates/rustty-os/src/windows.rs` (308 lines) **already** declares its
   own `extern` set, because Rust has no `std.os.windows` to lean on in the first
   place. There is no delta to port — this is a convergence, not a change.
3. **What porting it would break.** Nothing, but transcribing the Zig hunk
   would add unused `extern` declarations for symbols no rustty code calls,
   which is dead surface a `dead_code` sweep would then remove again.
4. **What holds the line.** The `#[cfg(windows)]` gating: these arms are not
   built on this host, which is **verification debt, not unported code**, and is
   already tracked as such. **This row is NOT out-of-scope-by-platform** — the
   Windows code IS ported; only its execution is unverified here.

## E24 — `os/mach.zig`'s `next_mmap_addr_hint` — N/A

1. **Upstream.** `e8525c0fd`: `std.heap.next_mmap_addr_hint` was removed from
   the Zig stdlib, so `TaggedPageAllocator` keeps its own
   `var next_mmap_addr_hint`; `mmap` prot flags become a struct literal.
2. **Why not.** This is a Zig-stdlib-removal accommodation. Rust has no
   `std.heap` address-hint global at all, and
   `crates/rustty-os/src/mach.rs` (276 lines) + `page_allocator.rs` (494)
   already own their allocator state.
3. **What porting it would break.** Nothing meaningful; there is no global to
   replace.
4. **What holds the line.** The existing `mach`/`page_allocator` tests.

## E26 — `os/flatpak.zig`'s `@cImport` → `gio_c` — N/A (build system)

1. **Upstream.** `e8525c0fd`: `@cImport("gio/gio.h")` becomes
   `@import("gio_c")`, a build-graph translate-C module; `std.Thread.Mutex`/
   `Condition` become `std.Io.Mutex`/`Condition`. No logic change.
2. **Why not.** Both halves are Zig build-system / `std.Io` plumbing.
   `crates/rustty-os/src/flatpak.rs` (798 lines) already uses its own bindings
   and `std::sync`.
3. **What porting it would break.** Nothing — there is no logic in the hunk.
4. **What holds the line.** The existing flatpak tests (build-config-gated off
   on this host).

## E36 — `src/main_ghostty.zig`'s iOS `allow_stack_tracing = false` — N/A

1. **Upstream.** `e8525c0fd` adds `std_options.allow_stack_tracing = false` for
   **non-macOS Darwin (iOS)**, because Zig switched to
   `_dyld_get_image_header_containing_address`, which is unavailable there.
2. **Why not.** It is a **Zig unwinder** issue. Rust backtraces do not use the
   Zig unwinder, so the switch has no counterpart.
3. **What porting it would break.** Nothing; there is no knob to set.
4. **What holds the line.** Nothing needs to — but the fact is worth
   remembering **if rustty ever targets iOS**, which is why it is written down
   rather than merely omitted.
5. **Phase-6 follow-up (`caf48a41e` "main: fix inverted allow_stack_tracing
   condition").** The non-iOS arm was `builtin.strip_debug_info` (tracing
   allowed only in *stripped* builds — inverted) and is now
   `!builtin.strip_debug_info`. Same verdict, same reason: it is the Zig
   unwinder's switch and Rust has no build-time strip toggle to project it
   onto. The reference block is carried at its end state in
   `crates/rustty/src/main_rustty.rs` (`std_options` PORT NOTE) so the
   projection stays complete; see `.port/phase-6/assessments/W4-core-tier.md`.

## E35 — `src/main.zig`'s `std.testing.log_level = .err` — N/A

1. **Upstream.** `e8525c0fd` sets `std.testing.log_level = .err` in the root
   `test` block, because "Zig 0.16.0 has made test logging more strict — anything
   printed to stderr results in a failed-command message".
2. **Why not.** Rust's `cargo test` **captures** stdout/stderr per test by
   default and only prints them for failing tests; there is no equivalent
   strictness to accommodate.
3. **What porting it would break.** Nothing; suppressing log output would only
   make failing tests harder to diagnose.
4. **What holds the line.** `cargo test`'s default capture behaviour.

## E38 — `src/tripwire.zig` — DOC COMMENT ONLY; the convention needs NO rebase

1. **Upstream.** `e8525c0fd` changes exactly one line of `src/tripwire.zig`, and
   it is inside a `//!` block: the usage example's
   `std.fs.cwd().openFile` becomes `std.Io.Dir.cwd().openFile(io, …)`.
2. **Why not.** There is no semantic change to port. Crucially, `enabled()` is
   still `builtin.is_test`.
3. **What porting it would break.** Nothing — but the *reason this row exists*
   is the inverse risk: a reviewer seeing `tripwire.zig` in the diff might
   conclude the `rustty_util::tripwire` convention (`enabled()` =
   `cfg!(debug_assertions)`) needs re-derivation. It does not.
4. **What holds the line.** The existing tripwire tests, plus the newly added
   `locale::tests::ensure_locale_refuses_to_run_under_test`, which is the first
   consumer of the convention added by this workstream (`E18`).

---

# Two upstream bugs that must NOT be reproduced

The prose-fidelity carve-out (`strip-ghostty-gag-prose`, and R1's
comment-fidelity rule generally) covers obvious upstream mistakes. Both of these
are in the range diff and both would be faithfully wrong if transcribed.

## E28 — the `os/main.zig` doc typo

`e8525c0fd` leaves `src/os/main.zig`'s module doc reading
*"...low-level operations, butos/main.zig"* — an editing accident (a missing
space and a stray path fragment). `crates/rustty-os/src/lib.rs` keeps the
readable sentence. **Do not "fix" the port to match.**

## E32 — `src/pty.zig`'s `@tagName(posix.errno(-1))`

`e8525c0fd` rewrites `src/pty.zig` against raw syscalls and, at **two** sites,
logs `@tagName(posix.errno(-1))` — the errno of the *literal* `-1` rather than
the real `errno` from the failed call. Cosmetic (log text only), but wrong.
`crates/rustty-os/src/pty.rs` logs the real errno. **Do not reproduce the bug.**

> **REFUTED (annotated 2026-09-22, task R13-D; the text above is kept as written).** There is no
> bug to reproduce. With libc linked, Zig 0.16's `std.posix.errno` is `std.c.errno`, which returns
> `@enumFromInt(_errno().*)` when its argument is `-1` (`std/c.zig:75-78`): `errno(-1)` is the idiom for
> "read this thread's errno", and Ghostty always links libc (`src/build/SharedDeps.zig:470`). The two
> sites (`src/pty.zig:158` and `src/termio/Exec.zig:1759`, not two in `pty.zig`) therefore log the real
> errno, and the port's rendering is equivalent, not a correction. Refuted by the R13 record drafts
> (`r13-record-drafts.md`, M68); inventory M68 is DROP.

---

# Rows that are NOT non-ports (recorded so the boundary is unambiguous)

* **`E22` `os/systemd.zig`, `E23` `os/{kernel_info,cgroup,passwd}.zig`,
  `E27` `os/{TempDir,cf_release_thread,pipe,desktop}.zig`, `E31` `Command.cwd`,
  `E37` `App.zig`** — mechanical `std.Io` / stdlib-rename churn with rustty
  equivalents already in place. Examined, nothing to port, no decision to
  record; see the area-E assessment's own rows.
* **`E19` `os/resourcesdir.zig`'s `*const`** — rustty was *ahead*; the oracle
  caught up. **Do not "fix" it back** to `&mut`.
* **`E2` `bed47168c`** — added and **reverted** (`60121a039`) inside the range.
  Not a non-port: a **trap** for anyone porting commit-by-commit instead of from
  the range diff. Confirmed handled by WS10 T20's own commit message.

---

# `check_tests_ported.py` delta from the two file deletions (P3-W7 T4)

Mirroring upstream's `src/os/args.zig` + `src/os/env.zig` deletions moves five
tests to a different CRATE, which the per-crate name matcher in
`.port/tools/gates/check_tests_ported.py` reports as five new `os::` MISSING
rows. **No test was deleted.** The delta, recorded rather than silently dropped:

| reference name (still present at the PIN) | now lives in |
|---|---|
| `os::appendEnv empty` | `crates/rustty-termio/src/exec.rs::tests::appendEnv_empty` |
| `os::appendEnv existing` | `crates/rustty-termio/src/exec.rs::tests::appendEnv_existing` |
| `os::prependEnv empty` | `crates/rustty-termio/src/shell_integration.rs::tests::prependEnv_empty` |
| `os::prependEnv existing` | `crates/rustty-termio/src/shell_integration.rs::tests::prependEnv_existing` |
| `os::args` | `crates/rustty-args/tests/os_args_seam.rs::args` |

The oracle names are still matchable only because the **pin has not advanced
yet**: upstream deleted all five along with their files. This is the same
mechanism `README.md`'s re-vet assertion **P3-RV2** describes for G3b — the pin
advance (`P3-W10`) resolves it **by construction**, because the reference names
cease to exist. Until then the five rows are expected, and a per-file
`GHOSTTY_SRC` override to paper over them is **forbidden** (it is the spoof hole
`.port/tools/ci.sh` closed for G1b).

Three other `check_tests_ported.py` MISSING rows (`config::fromHex`,
`vt::PageList compact insufficient savings returns null`,
`vt::PageList compact std_size page returns null`) are **pre-existing** and not
this workstream's.

---

# Area F addendum (`P3-W8`)

## F1 — `crates/rustty-font/res/U+2500...U+25FF-9x17+1.png` diverges from the oracle golden by **exactly one pixel**

1. **Upstream.** `d0e72a3ab` "font/sprite: update to z2d 0.12.1, use native path
   insetting" regolds 6 sprite goldens. `P3-W8 T3` back-ported z2d 0.12.1's
   `Path.offset` (`crates/rustty-z2d/src/internal/path_fx/`) and rewrote
   `Canvas::innerStrokePath` on top of it.
2. **Result.** rustty now reproduces **5 of the 6** regolded goldens
   **pixel-for-pixel** against `origin/main`'s own files (they were copied in
   verbatim, not regenerated). The sixth,
   `U+2500...U+25FF-9x17+1.png`, differs at **1 pixel of 83,200** —
   `(x=133, y=381)`, oracle `79`, rustty `95`, delta `16/255` — inside the
   corner-triangle-outline glyph cell `(10,15)`. That file is therefore the one
   golden regenerated from rustty's own render rather than copied from the
   oracle.
3. **Why the offset port is not the cause — proven, not assumed.**
   * z2d 0.12.1's own `test "offset, e2e"` and `test "offset, zero value"` are
     ported verbatim (`internal/path_fx/offset.rs::tests::offset_e2e_inset` /
     `_outset` / `offset_zero_value_is_a_noop`) with the reference's **recorded
     expected node sets**, and pass exactly.
   * The specific failing shape was isolated: the `9x17+1` corner triangle
     `(0,0)→(0,17)→(9,0)` closed, inset by `-0.5`. rustty's `Path::offset`
     returns
     `(0.5, 0.5)`, `(0.5, 14.986923107684923)`, `(8.169547527597903, 0.5)` —
     **bit-identical** to an independent re-implementation of the reference
     algorithm in Python IEEE-754 doubles.
   * Substituting `sqrt(dx²+dy²)` for `hypot` in `Slope::init` did **not** move
     the pixel, so it is not a libm `hypot` ULP difference either.
   * rustty's **pre-change** golden for this file was pixel-identical to the
     oracle's **pre-change** golden (0 differing pixels), so nothing was already
     drifting.
   The residual is therefore in rustty-z2d's **stroke rasterizer** (ported at
   z2d 0.10.0), on an input — the inset path — that only became reachable with
   this change. It is a sub-quantum coverage difference on one antialiased edge
   sample at the smallest of the four metric sets; the same glyph at
   `11x21+2`, `12x24+3` and `18x36+4` matches the oracle exactly.
4. **What would break if we "fixed" it by keeping the oracle's file.** The
   `sprite_face_render_all_sprites` gate would be permanently red for a
   1/83,200-pixel, 6%-alpha difference, and R5 forbids weakening it with a
   tolerance. Chasing the rasterizer ULP is a z2d-0.10.0-rasterizer
   investigation with no upstream counterpart to port.
5. **What holds the line.** `sprite_face_render_all_sprites`
   (`crates/rustty-font/src/sprite/face.rs`) still compares all 36 goldens
   byte-for-byte, 35 of which are the oracle's own files; and the three
   verbatim z2d `offset` e2e tests pin the geometry independently of any
   golden. If the offset algorithm regresses, those fail first. The exact
   divergent pixel is recorded above so a future reader can re-measure it in one
   command rather than rediscovering it.
6. **RESOLVED (task T-F, 2026-09-23, LEDGER S97-F1). CORRECTION of point 3's
   conclusion ("The residual is therefore in rustty-z2d's stroke rasterizer").**
   The cause was `hypot` after all, in `Slope::normalize`, not `Slope::init`:
   z2d calls Zig's `std.math.hypot` and the port called libm's `f64::hypot`,
   which differ in the last bit for the offset hypotenuse's slope
   (`(30.678190110391604, 57.9476924307397)`: Zig `0x4050645096e433c0`, macOS
   libm `...c1`, both measured). That bit moved the outer miter points of the
   stroke (`(8, 16.000000000000004)` and `(44, 84)` in Zig against `(8, 16)` and
   `(44.00000000000001, 84.00000000000001)`), found by dumping both plotters'
   contours, and flipped one sample of pixel `(133, 381)`. Point 3's substitute,
   `sqrt(dx²+dy²)`, is a third algorithm, so it could not show this.
   `crates/rustty-z2d/src/internal/zig_math.rs` ports `std.math.hypot`, used at
   all three of z2d's `hypot` sites; the render now matches the oracle at that
   pixel, and the golden was regenerated from the oracle's own file through
   `RUSTTY_REGEN_FONT_KAT_PNGS` (which rewrites only when the decoded pixels
   are identical), so all 36 goldens hold the oracle's pixels.

---

# Area G — `P3-W9` (config / CLI / build / i18n / synthetic)

**READ FIRST — `G1` and `G2` are both DISCHARGED (parity tail, 2026-09-23); this
area is now a CLOSED record like the rest of the file.** The paragraph below is
kept as written in `P3-W9`. `G1` (uucode) and `G2` (vaxis/zf) are `P3-W9 T6` work that was
**measured and decided but NOT completed**. They are written up with full
evidence instead of being quietly dropped, but they have **NO OWNER**: `P3-W9`
closes with this commit and no other workstream claims them. Whoever sequences
the remaining workstreams must assign them, or `.port/phase-2/DEFERRALS.md` /
`VERIFICATION_DEBT.md` must carry them with an owner in an open workstream so
the `G-P2` closure gate can see them. Do not let this paragraph be the only
thing holding them.

**Update 2026-09-23 (parity tail S-10a): `G1` is DISCHARGED** — see its item 5
below and `NEW-FINDINGS.md` N220. **Update 2026-09-23 (parity tail S-10b): `G2` is
DISCHARGED** — see its item 5 below and `NEW-FINDINGS.md` N240.

Appended by **`P3-W9` Task 6 and Task 7**. Same 4-criterion basis as above.
Source of the verdicts: `.port/phase-3/assessments/G-config-build-i18n.md`.

## G1 — `uucode` revision bump (`e8525c0fd` / `f2a7652ab`) — DISCHARGED 2026-09-23 (was DEFERRED, never non-ported)

**DISCHARGED by parity tail S-10a; items 1–4 are the deferral as written,
kept for the record, and item 5 is the discharge with its evidence.** It was
recorded here rather than
silently dropped because `P3-W9 T6 step 2` explicitly sanctions "defer the
uucode bump with a written reason" as an outcome, and because deferring a
dependency bump without the reason is how a table goes stale unnoticed.

1. **Upstream.** `build.zig.zon`'s `uucode` moved from
   `uucode-0.2.0-ZZjBPqZVVABQepOqZHR7vV_NcaN-wats0IB6o-Exj6m9` to
   `uucode-2826a37a4562284fdacd8fa029d49509cc9bffcd` (hash `ZZjBPlK5…`),
   **still labelled `0.2.0`**. The API moved `uucode.x.types` -> `uucode.types`
   and `uucode.x.grapheme` -> `uucode.grapheme`, and
   `src/build/uucode_config.zig` was rewritten (+203).
2. **Why not now — measured, not assumed.**
   * **The UCD source data did not change.** Both tarballs were fetched and
     diffed (`.port/phase-3/w9-fetch-deps.sh`; `diff -rq <old>/ucd <new>/ucd`):
     **every `ucd/*.txt` file is byte-identical.** The only difference is a NEW
     file, `ucd/BidiMirroring.txt` (+636) — a bidi-mirroring property that
     `crates/rustty-unicode`'s three generated tables do not consume — plus a
     one-line `ucd/.gitignore` change. So the INPUT to rustty's tables is
     unchanged: "the version label did not change" is not the evidence here,
     the data byte-comparison is.
   * **The DERIVATION logic did move, and that is why this is a deferral and not
     a non-port.** `src/x/config_x/wcwidth.zig` (−228) was restructured into
     `src/components.zig` (+2717) and `src/x/grapheme.zig` (−842) into
     `src/grapheme.zig` (+771). A line-level comparison shows the old wcwidth
     `compute` body is **not** a verbatim move (60 of its 72 code lines do not
     appear verbatim in the new file), so byte-identical input does **not** by
     itself prove byte-identical output.
   * **The CI-toolchain coupling makes this a two-lane change.**
     `.github/workflows/ci.yml:69` installs **Zig 0.15.2** specifically as the
     G4-uni uucode oracle. Verified on this host: with Zig **0.16.0** installed,
     `bash .port/phase-0/harness/unicode/run_unicode_diff.sh` fails to build the
     OLD uucode at all —
     `zig-pkg/uucode-0.2.0-…/build.zig:211:18: error: member function expected 3
     argument(s), found 2` — while the NEW uucode requires 0.16. The two
     revisions are **mutually exclusive** under one Zig pin: bumping uucode
     forces the CI Zig pin to move in the same commit, and the G4-uni lane must
     be re-proven green under 0.16 before the tables are regenerated.
3. **What would break if we did it half-way.** Regenerating the three
   `crates/rustty-unicode/src/generated/*_data.rs` without moving the CI pin
   leaves CI unable to build the oracle it diffs against, i.e. G4-uni goes RED
   or (worse) BLOCKED-and-ignored. Moving the pin without regenerating leaves
   the pin describing a toolchain that cannot build the committed tables'
   generator. Either half alone is a false green.
4. **What holds the line.** Nothing currently detects table staleness against
   the NEW uucode — `crates/rustty-unicode/src/generated/checksum.rs` validates
   internal consistency, not upstream data, and G4-uni pins the tables to the
   **old** revision. Stated plainly rather than implied. The discharge recipe,
   in order:
   1. bump `.github/workflows/ci.yml:69` to Zig 0.16.0 (matching
      `build.zig.zon`'s `minimum_zig_version = "0.16.0"` at oracle head);
   2. update `.port/phase-0/harness/unicode/` to fetch the new uucode tarball
      and to use `uucode.types` / `uucode.grapheme` (dropping the `x.` prefix);
   3. run `run_unicode_diff.sh` and **byte-compare** the regenerated
      `props_data.rs` / `symbols_data.rs` / `grapheme_data.rs` against the
      committed ones. A non-empty diff is a REAL data change and must be
      reviewed codepoint-by-codepoint, not blessed.
5. **Discharge (parity tail S-10a, 2026-09-23; `NEW-FINDINGS.md` N220).** The
   recipe, step by step:
   1. `.github/workflows/ci.yml`'s G4-uni step now installs **Zig 0.16.0**.
      Nothing else in that job needed 0.15.2: every other gate that runs zig
      (`check_lz4_byte_exact.sh`, `check_ref_item_geometry.py`,
      `stb-resize-oracle/run.sh`, …) also needs the ghostty oracle checkout,
      which CI does not have, so they report BLOCKED there whatever the zig;
      locally they already run on 0.16.0. Before this, the CI pin could not
      build the harness at all (its `build.zig.zon` already said
      `minimum_zig_version = "0.16.0"`), so G4-uni was RED on CI.
   2. The harness had already been moved to uucode 2826a37a, the pin's
      `uucode_config.zig` (byte-identical to ghostty@20abdb50a's, `diff`
      empty) and `uucode.types` / `uucode.grapheme` by `986d90c8`
      (ci-triage); this row was not updated then. S-10a corrected the
      harness's stale version text (the BLOCKED message still said 0.15.2).
   3. **Tables regenerated and compared.** The reference's own generators,
      `src/unicode/props_uucode.zig` / `symbols_uucode.zig` at 20abdb50a
      (with `lut.zig`, `props.zig`), were built exactly as
      `src/build/UnicodeTables.zig` + `SharedDeps.zig` build them (uucode
      2826a37a, `build_config_path` = the pin's `uucode_config.zig`,
      `tables_path` = that dependency's `tables.zig`) and run. Their
      `stage1`/`stage2`/`stage3` arrays were compared element for element
      with `props_data.rs` (8192 / 31744 / 30) and `symbols_data.rs` (8192 /
      1792 / 2): **0 differences in every array**. `grapheme_data.rs`'s 8192
      entries are regenerated independently by the harness through
      `uucode.grapheme.computeGraphemeBreakNoControl`: 0 differences. So the
      restructured wcwidth derivation (`src/x/config_x/wcwidth.zig` ->
      `src/components.zig`) and grapheme logic (`src/x/grapheme.zig` ->
      `src/grapheme.zig`) produce the same values over the byte-identical UCD
      input: **no code point changed in any class**, and the checksums in
      `checksum.rs` stand unchanged. `run_unicode_diff.sh` (G4-uni) is GREEN
      under Zig 0.16.0: 0 diffs for width, is_symbol, props flags,
      grapheme_break_no_control over 0..=0x10FFFF and the 8192 precompute
      entries. Negative controls: one flipped `STAGE3` field in a copy of
      `props_data.rs` and one flipped `STAGE2` entry in a copy of
      `symbols_data.rs` each reported 1 difference.
   4. **The other uucode-derived data in the workspace**, probed the same way
      against uucode 2826a37a + the pin's config: `case_folding_full`
      (`rustty-unicode` `case_folding_data.rs`, 1585 code points) and
      `is_emoji_presentation` (`rustty-font-shaper` `uucode.rs`, 1219 code
      points) — 0 differences each (negative controls: one dropped and one
      altered entry each reported). `ascii.zig`, `utf8.zig` and
      `code_point.zig` are byte-identical between the two revisions, and
      `BreakState` / `GraphemeBreakNoControl` keep their order. The one
      uucode consumer this does NOT settle is `rustty-vaxis`'s vendored
      uucode 0.1.0 subset: at the pin, libvaxis 0.6.0 imports Ghostty's
      uucode 2826a37a (`external_uucode`, the `libvaxis_only` table), so
      moving it is part of `G2`, below, not of this row.

## G2 — `vaxis` 0.5.1 -> 0.6.0 and `zf` 0.10.3 -> 0.11.0 — DISCHARGED 2026-09-23 (was DEFERRED with a measured delta)

**DISCHARGED by parity tail S-10b; items 1–4 are the deferral as written, kept for
the record, and item 5 is the discharge with its evidence.**


Also open, also recorded rather than dropped. `P3-W9 T6 step 1` asks for the
tarball diff and the port of the deltas; the diff was produced and read, the
port was **not** completed, and the size is the reason.

1. **Upstream.** Both are ported Zig libraries, not Cargo dependencies:
   `crates/rustty-vaxis` and `crates/rustty-zf`. Consumers are `rustty-cli`'s
   `list-themes` / `list-keybinds` / `list-colors`.
2. **Measured delta** (`.port/phase-3/w9-fetch-deps.sh` +
   `w9-dep-delta.sh`; per-file numbers reproducible in one command):
   * **vaxis `src/` = 4,402 diff lines.** Largest: `widgets/TextInput.zig`
     +316/−19, `vxfw/TextField.zig` +301/−19, `tty.zig` +205/−107,
     `Vaxis.zig` +205/−91, `queue.zig` +160/−133, `vxfw/App.zig` +110/−44,
     `widgets/terminal/Command.zig` +86/−42, `Loop.zig` +76/−30.
   * **zf ported files = 847 diff lines** over `src/zf/{filter,zf,clib}.zig`.
3. **The four SEMANTIC changes found while reading it** — recorded so the port,
   when it happens, does not treat this as pure Zig-0.16 churn:
   * `ctlseqs.zig` +10: three NEW multi-cursor sequences
     (`secondary_cursors_rgb`, `reset_secondary_cursors`,
     `show_secondary_cursor`).
   * `Screen.zig`: `cursor_row`/`cursor_col` collapse into a new
     `pub const Cursor = struct { row, col }` field plus
     `cursor_secondary: []Cursor`; `Window.zig` follows at its two write sites.
   * `Mouse.zig`: `col`/`row` change type **`u16` -> `i16`** (negative
     coordinates now representable). A silent-truncation hazard if half-ported.
     The mouse fields are now half on 0.6.0: see the `Parser.zig` bullet below
     and `parse_param`'s PORT NOTE in `crates/rustty-vaxis/src/parser.rs`.
   * `Parser.zig`: `parseParam` reads CSI parameters with `parseInt` instead of
     `parseUnsigned` (accepting a leading `+` and `-0`). **Absorbed by N206**
     (tail S-9b): the port's `parse_param` already follows 0.6.0's grammar; only
     the mouse `i16` type change above is still deferred.
   * `zf`: `token` -> `needle` throughout, `rankToken` -> `rankNeedle`,
     `highlightToken` -> `highlightNeedle`, and — the one that is NOT a rename —
     `RankOptions.to_lower: bool = true` becomes
     `RankOptions.case_sensitive: bool = true`. The old code passed
     `!opts.to_lower` as `case_sensitive`, so the **default polarity flips**
     from case-INsensitive to case-SENSITIVE. Porting the rename without
     re-checking `rustty-cli`'s `list-themes` call site would silently change
     the fuzzy picker's matching behaviour.
   * `unicode.zig` / `gwidth.zig`: `GraphemeIterator.next()` ->
     `nextCodePoint()` and `res.cp` -> `res.code_point` — this is the **uucode
     API rename surfacing through vaxis**, so G2 and G1 are coupled: vaxis 0.6.0
     expects the new uucode.
4. **What holds the line.** `crates/rustty-vaxis` and `crates/rustty-zf` keep
   their own ported test suites, which pin the 0.5.1/0.10.3 behaviour. **Stated
   plainly: for `list-themes` there is effectively no runtime coverage** —
   `crates/rustty-cli/src/list_themes.rs`'s interactive `Loop` path is
   `#[ignore]`-on-device (it needs a real `/dev/tty`), so it is exercised by NO
   CI test and the guard there is compile-time only. Do not read the green
   `rustty-vaxis`/`rustty-zf` suites as coverage of the CLI integration.
5. **Discharge (parity tail S-10b, 2026-09-23; `NEW-FINDINGS.md` N240).**
   * **What was ported.** Every 0.5.1 -> 0.6.0 delta of the vaxis files the crate
     ports (the file list is in N240) and every 0.10.3 -> 0.11.0 delta of
     `zf/{zf,filter,clib}.zig`, with the tests 0.6.0 / 0.11.0 add (Parser 2,
     InternalScreen 1, Vaxis 1, TextInput 13, App 1, zf `deduplicateMatches`).
     The `std.Io` / allocator churn is not reproduced (the port has no `std.Io`;
     `error.Canceled` returns are PORT NOTEs). The four SEMANTIC changes of item 3:
     multi-cursor ctlseqs + `Screen.Cursor` / `cursor_secondary` + the render and
     API around them (`vaxis.rs`); `Mouse.col`/`row` `i16` together with
     `parseParam(i16, ..)` and every consumer (`translateMouse`, `Window.hasMouse`,
     vxfw `App`, the Windows decode, `list_themes`'s clamps from `d320cd7df`), so
     no half-applied type change remains (the 36 N206 mouse results now match);
     zf's rename with the polarity check below; the uucode API
     (`nextCodePoint` / `.code_point`).
   * **uucode for vaxis.** At the pin vaxis uses ghostty's uucode 2826a37a with
     ghostty's config (`SharedDeps.zig:24-78, 512-519`). `rustty-unicode`'s tables
     are not the same fields (they hold ghostty's derived width / is_symbol /
     grapheme_break_no_control, not `east_asian_width` / `general_category` /
     the full `grapheme_break`), so reusing them was not an option and no new
     crate edge was added; `rustty-vaxis`'s own `build.rs` now follows 2826a37a's
     `components.zig` over the vendored UCD 17.0.0. Zig-oracle differential over
     0..=0x10FFFF of the four fields: 0 differences (negative control: 1).
   * **zf polarity.** The pin's `list_themes.zig:377-386` passes
     `.case_sensitive = false, .plain = true`; `rustty-cli` passes the same, and
     714,870 ranked rows are bit-identical to zf 0.11.0.
   * **Reference defects**, not ported (R13): UD260 (also in 0.5.1) and UD261–UD266.
   * **Residual, recorded in N240:** the zigimg revision libvaxis 0.6.0 pins
     (`d695acd9`, was `d7b7ab0b`) was not diffed (the old tarball is not cached);
     it reaches only vaxis's image tier, which Ghostty never calls.

## G3 — `libxev` bump (`e8525c0fd`) — nothing semantic for rustty; the 3 QUIRKs RE-AFFIRMED

1. **Upstream.** `libxev-34fa5087…` -> `libxev-9ce8e8e6…`. `src/` delta is
   5,353 diff lines, concentrated in `windows.zig` (+677/−113), a NEW
   `posix.zig` (+694), `backend/kqueue.zig` (+295/−100),
   `backend/epoll.zig` (+149/−76) — i.e. the Zig-0.16 `std.posix`/`std.Io`
   migration and the Windows backend, none of which rustty ports differently.
2. **Why no port action.** `crates/rustty-xev` is a **D6 Zig-port-defer**
   (memory `integration-tier-decisions`); it is not tracking libxev revisions.
3. **The `P3-W0 T5` re-check, executed against the NEW revision** (the point of
   this row — `P3-W0 T5` deferred exactly this to `P3-W9 T6 step 3`):
   * `crates/rustty-xev/src/epoll.rs:1158` and `:1823`, and
     `io_uring.rs:1645`/`:2359` — the "sendmsg with buffer is unimplemented and
     panics" quirk. **Still present upstream:**
     `grep -rn 'TODO: sendmsg with buffer'` in the new tarball hits
     `backend/io_uring.zig:494` and `backend/epoll.zig:750` (was `:489`/`:706`).
     Re-affirmed, no change.
   * `crates/rustty-xev/src/dynamic.rs:142` — the doc says "defaults to the
     FIRST candidate backend", the code takes the LAST. **Still present
     upstream:** the new `dynamic.zig:28` still reads "defaults to the first
     candidate backend" while `:91` is
     `pub var backend: Backend = subset(bes[bes.len - 1]);`. Re-affirmed.
   * `crates/rustty-xev/src/io_uring.rs:7393` — the reference test's redundant
     second `loop.add(&c_read_cancel)` after `loop.cancel` already added it.
     **Still present upstream:** new `backend/io_uring.zig:1786`. Re-affirmed.
   * `COMPLETION_SIZE` tripwires (codex `W13A-03`): the new revision re-pins
     its own size contracts — epoll 208 -> **184** (`backend/epoll.zig:1458`),
     io_uring 152 -> **128** (`backend/io_uring.zig:1130`), and kqueue's
     `== 256` equality became a `<= 256` **ceiling**
     (`backend/kqueue.zig:1949`). Unabsorbed per point 2; the Rust tripwires
     (epoll 224 / io_uring 152 / kqueue 264) stay pinned to Rust-local layout
     per the `WS19 T10` `[R1-DIVERGENCE]` entry in
     `.port/phase-2/DEFERRALS.md`. A future bump absorption re-measures
     against 184/128/`<=256`, not 208/152/`==256`; each backend's PORT NOTE
     now carries a matching `UPSTREAM DRIFT (unabsorbed)` paragraph.
4. **What holds the line.** The three PORT NOTEs themselves, now carrying the
   re-check; and `crates/rustty-xev`'s ported test suite, which encodes the
   quirky behaviour (the double-cancel test asserts success twice).
5. **Partial absorption (codex `W13A-02`, 2026-08-10).** Point 1's delta
   enumeration missed one semantic item: the new revision's
   `watcher/file.zig:318` `test "queuePWrite"` final-content KAT (two
   `queueWrite` "1234"/"5678", then `queuePWrite` "000"@3 **reusing**
   `&write_req[1]`; asserts `"123000"`). Now ported as
   `epoll_file_queue_pwrite_kat` / `io_uring_file_queue_pwrite_kat`
   (`epoll.rs`/`io_uring.rs`, beside the `*_file_queued_writes` twins); the
   `queue_pwrite` request init in both backends now matches the new
   `file.zig:171` `.{ .full_write_buffer = buf }` body (behavior-neutral — the
   trampoline never reads `full_write_buffer`); and `check_test_counts.py`
   gained an `xev_file` row bound against the CURRENT 9ce8e8e6 checkout
   (`GHXEV_CUR`) plus corrected "exact commit ghostty pins" comments. The rest
   of the bump (point 3's tripwires included) remains unabsorbed per point 2.

## G4 — `z2d` 0.10.0 -> 0.12.1 — owned elsewhere, cross-reference only

`P3-W9 T6 step 4` is explicit that this is **`P3-W8 T3`**'s, and it landed
there: `2035be3` "z2d/font: native `Path::offset` replaces the two-surface
inner-stroke mask; drawE0B5 forces perpendicular butt caps; 6 goldens regolded
(oracle d0e72a3ab)". Recorded here only so a sweep of `build.zig.zon` does not
re-open it.

## G5 — `f2a7652ab` `Config.omitFramePointer()` — PROVEN N/A

1. **Upstream.** A new `src/build/Config.zig` helper that forces frame pointers
   on Darwin, because the Apple arm64 ABI requires `x29` to be a valid frame
   pointer and Zig would otherwise omit it in release modes.
2. **Why not — executed, not assumed.**
   `rustc +nightly -Z unstable-options --print target-spec-json --target
   aarch64-apple-darwin` -> `"frame-pointer": "non-leaf"`. rustc keeps frame
   pointers on Apple ARM **unconditionally**, independent of opt-level or
   strip. The requirement is already satisfied by the toolchain; there is
   nothing for a port to add.
3. **What would break if we ported it.** Nothing would break, but a
   `-Cforce-frame-pointers` flag in a build script would imply rustty is
   working around a hazard it does not have, and would have to be maintained
   against a target spec that already guarantees the property.
4. **What holds the line.** The target spec itself. If a future Rust release
   changed `frame-pointer` for Apple ARM it would be a rustc-wide breaking
   change, not a rustty regression.

## G6 — `f2a7652ab` `libsystem_override.sh` + `LibsystemOverrideStep.zig` — PROVEN N/A

1. **Upstream.** A Darwin-only build step (+123/+51) that runs `nmedit` over the
   static archive to LOCALIZE `_memcpy`/`_memmove`/`_memset`/`_strlen` and the
   libm symbols, so an app linking `libghostty` binds libSystem's
   implementations instead of Zig's compiler-rt copies.
2. **Why not — executed, not assumed.** The premise is Zig-specific: Zig's
   compiler-rt exports these as **C** symbols, so they participate in final
   link resolution. Rust's does not. Measured on this host
   (rustc 1.93.1, `aarch64-apple-darwin`):
   ```
   nm -g "$(rustc --print sysroot)"/lib/rustlib/aarch64-apple-darwin/lib/libcompiler_builtins-*.rlib
   ```
   -> **144** `T` (external text) symbols, of which the count matching
   `^[0-9a-f]* T _(memcpy|memmove|memset|memcmp|bcmp|strlen|cos|sin|exp|pow|ceil)$`
   is **0**. The mem functions exist only Rust-mangled
   (`__ZN17compiler_builtins3mem6memcpy…`), so they cannot shadow libSystem.
   (The area-G assessment reported 422 `T` symbols; that was a different
   toolchain build. The count that matters — C-symbol hits — is 0 in both.)
3. **Why this is not a theoretical concern.** `crates/rustty-vt-c` and
   `crates/rustty-apprt-c` both emit `staticlib`, i.e. rustty **is** in exactly
   the archive-consumer position the upstream step exists to protect, and is
   still safe. Recording the measurement rather than the intuition is the point.
4. **What holds the line.** Nothing automated, and that is acceptable: the
   property is a rustc/`compiler_builtins` invariant, not rustty code. The
   reproduction command above is one line if it ever needs re-checking.

## G7 — `d65cb5128` native-linker link for lib-vt on Apple + `pkg/apple-sdk/native_link.zig` — N/A, WITH A PRECONDITION

1. **Upstream.** Link `libghostty-vt` on Apple hosts with the **native** linker
   (`native_link.zig` +85, `GhosttyLibVt.zig` +131) because Zig's self-hosted
   Mach-O linker omits `LC_ENCRYPTION_INFO_64` on physical-iOS dylibs, which
   fails App Store validation. `.github/scripts/check-apple-libghostty-vt.nu`
   (+131) asserts it, plus single-architecture, install name, and exactly one
   `LC_BUILD_VERSION`.
2. **Why not.** The defect is in Zig's linker; rustc invokes `cc` -> `ld64`,
   which emits the load command. And rustty ships **no iOS dylib**:
   `xtask xcframework` packages a **static** library (`xtask/src/main.rs:395`,
   `:442` — `-create-xcframework -library` over `libRusttyKit.a`), macOS only.
   None of the four nushell assertions has a subject.
3. **What would break if we ported it.** A nushell validation script with no
   artifact to validate is a gate that passes vacuously — the exact shape this
   program treats as worse than absent.
4. **PRECONDITION TO CARRY FORWARD (this is why the row exists).** If rustty
   ever ships an iOS framework containing `rustty-vt-c`'s `cdylib`, those four
   assertions become **mandatory** — and they will pass, because rustc links
   Apple targets through `ld64`. Record it as a precondition on the
   (unscheduled) iOS tier, not as a dismissal.
5. **P-1 NOTE (2026-09-27) — §2 is partly stale.** The iOS precondition still
   holds: `xtask xcframework-vt` now packs `ios-arm64` and `ios-arm64-simulator`
   slices too, but all of them are STATIC archives, so there is still no iOS
   dylib. Two statements above are no longer true: the xcframework is not
   "macOS only", and the `xtask/src/main.rs:395`/`:442` citations have moved.
   More importantly, the reference's `test.yml` job `build-libghostty-vt-macos`
   runs the script over the per-target macOS **dylibs** (single architecture,
   install name, one `LC_BUILD_VERSION`), and rustty DOES ship
   a macOS dylib — since S64-F5 the CMake install names it
   `librustty-vt.<version>.dylib` with the `librustty-vt.<major>.dylib` soname
   and `librustty-vt.dylib` link-name symlinks (G-CCONSUMER) — so the macOS rows now
   have a subject. That job is listed as a remaining CI gap in NEW-FINDINGS N376,
   not dismissed here.

## G8 — `1c861e3c4` `pkg/apple-sdk/include/math.h` and `dac134d25` libc++ availability — N/A

1. **Upstream.** `1c861e3c4` adds a `#include_next <math.h>` shim defining
   `INFINITY`/`NAN` because Xcode 27's `math.h` uses the `__need_infinity_nan`
   protocol that **Zig 0.16's bundled `float.h`** predates. `dac134d25` enables
   the Apple vendor availability table for libc++ because Zig's **bundled**
   libc++ headers assume every LLVM-21 ABI symbol is present in the system
   dylib (crashing on macOS without `std::__hash_memory`).
2. **Why not.** Both premises are Zig **bundling** its own Clang resource
   headers / libc++. rustty compiles C and C++ through the `cc` crate ->
   Apple `clang` with its own matching resource headers, and `bindgen`/libclang
   likewise; there is no bundled `float.h` or bundled libc++ anywhere in the
   rustty toolchain.
3. **What would break if we ported it.** Injecting a `math.h` shim ahead of the
   real SDK header would shadow a correct header with a workaround for a
   compiler rustty does not use.
4. **What holds the line.** The `-sys` crates' own builds: if the SDK/compiler
   pairing were broken, `simdutf-sys`, `highway-sys` and `harfbuzz-sys` would
   fail to compile, which the workspace build already exercises every run.

## G9 — `84254a9d8` no-libcxx MSVC runtime — N/A, and the portable half ALREADY EXISTS

1. **Upstream.** Avoid dragging in the MSVC C++ runtime in no-libcxx builds
   (`pkg/highway`, `pkg/simdutf`, `GhosttyLibVt`, `SharedDeps`, `src/lib_vt.zig`).
2. **Why not.** The Windows/MSVC half is macOS-first N/A (locked fork decision,
   memory `rustty-completion-plan`). The portable half — "do not drag in a C++
   runtime" — is **already implemented**: `crates-sys/simdutf-sys/build.rs:50`
   and `crates-sys/highway-sys/build.rs:68` both call `cpp_link_stdlib(None)`,
   with the reasoning spelled out at `highway-sys/build.rs:39,64`.
3. **What would break.** Nothing; there is simply nothing left to port.
4. **What holds the line.** Those two `-sys` crate builds, which are part of
   every workspace build.

**`src/lib_vt.zig`'s MSVC `_fltused` marker [added 2026-09-24, parity tail S-8d].**
The `lib_vt.zig` part of this range is `var msvc_fltused: c_int = 1;` plus the
root-guarded `@export(&msvc_fltused, .{ .name = "_fltused" })` for static MSVC
builds — Windows/MSVC-only, so N/A on the same terms as the rest of this row.
Its reference comment lines (`:166`, `:173-174`) are quoted at the Rust site
that records the block, `crates/rustty-vt-c/src/lib.rs` (the root export-block
note), with the `quirks_memset` force-reference comment beside them.

## G10 — `1fe1b2d23`, `patch_rpath` -> `patchelf`, `terminalOptions(.., optimize)`, `10e6ace6b` — N/A (with two things recorded, not dismissed)

1. **Upstream.** `1fe1b2d23` fixes static `libghostty-vt` linking on Windows.
   `f2a7652ab` turns `patch_rpath` into `patchelf { interp, rpath }` (on NixOS
   also `--set-interpreter` from `ld.so`, not just `--set-rpath`), and makes
   `terminalOptions(artifact, optimize)` derive `slow_runtime_safety` from the
   artifact's optimize mode. `10e6ace6b` changes `GhosttyI18n.zig`'s
   `trimRight(locale, ".UTF-8")` to `trimEnd(...)` for the FreeBSD locale dir.
2. **Why not, item by item.**
   * Windows static lib-vt: macOS-first locked fork; rustty's Windows path is
     not stood up.
   * `patchelf`: `grep -rn 'patchelf\|IN_NIX_SHELL' . --exclude-dir=.git
     --exclude-dir=target` -> 0 hits. rustty has no Nix packaging at all
     (`PACKAGING.md:143` defers a reproducible Nix/crane build to release-infra
     Batch 4). **RECORDED AS A REAL REQUIREMENT, NOT DISMISSED:** if a Nix lane
     is ever stood up, the `--set-interpreter` half is load-bearing and a naive
     `--set-rpath`-only port would silently miss it.
     **[P-1 NOTE, 2026-09-27: stale. The Nix lane exists (`nix/`, `flake.nix`)
     and `xtask patchelf` ports patch-interp/patch-rpath with BOTH halves —
     the interpreter defaults to the `ld.so` on `PATH` inside a Nix shell
     (S63-F4). The snap now uses its `--patch-rpath` arm (N370).]**
   * `slow_runtime_safety`: `grep -rn 'slow_runtime_safety' crates/ xtask/
     Cargo.toml` -> 0 hits. rustty has no such build option; safety checks are
     Cargo profiles, which are per-profile by construction — the bug upstream
     fixed (global rather than per-artifact optimize mode) cannot occur.
   * FreeBSD `trimEnd`: a semantically identical Zig 0.16 rename.
3. **D-1 — rustty's `gen_locale` is deliberately MORE robust; do NOT "fix" it
   back.** The oracle derives the locale directory from a hardcoded list in
   `src/os/i18n_locales.zig` plus a `.UTF-8` trim (`GhosttyI18n.zig:26-31`).
   rustty derives it from the `po/*.po` **basename**
   (`xtask/src/res_misc.rs:375` `po.file_stem()`), so it cannot drift out of
   sync with `po/` and there is no `.UTF-8` suffix to trim. That is what makes
   `10e6ace6b` N/A. A future reviewer diffing against the oracle will see a
   "missing" hardcoded list; this row is why it is missing.
4. **What holds the line.** For D-1: `gen_locale`'s per-catalog `msgfmt`
   exit-status check (`res_misc.rs:398`) — a catalog that stopped being
   discovered would stop being compiled, and the check is per-catalog.
5. **P-1 NOTE (2026-09-27) — D-1 is stale.** `gen_locale` no longer derives the
   locales from the `po/*.po` basenames: it now compiles the DECLARED `LOCALES`
   set (`xtask/src/res_misc.rs`, the reference's `i18n_locales.zig` list), and
   `po_dir_matches_declared_locales` / `catalogs_error_on_missing_declared_po`
   hold the two in step. There is still no `.UTF-8` suffix to trim, so
   `10e6ace6b` stays N/A.

## G11 — `pkg/harfbuzz/c.zig` and `pkg/wuffs/src/c.zig` DELETIONS — N/A; upstream converged on rustty's design

1. **Upstream.** Zig 0.16 removed `@cImport`. `pkg/harfbuzz/c.zig` (−8) was
   replaced by a lazy `translate_c`-backport dependency whose `Translator`
   emits an `hb_c` module, imported at `pkg/harfbuzz/main.zig:11` as
   `pub const c = @import("hb_c")`. `pkg/wuffs/src/c.zig` (−18) was deleted and
   its `defines` array moved into `pkg/wuffs/build.zig:4-15`.
2. **Why nothing to follow.** rustty never had a `c.zig`:
   `crates-sys/harfbuzz-sys/build.rs:116` runs `bindgen` over
   `crates-sys/harfbuzz-sys/wrapper.h`, which **is** the structural equivalent
   of upstream's new `hb_c` module. Upstream arrived at rustty's design.
3. **The one real action, and it is done.** `crates-sys/wuffs-sys/build.rs`
   cited the deleted `pkg/wuffs/src/c.zig` at two places as the provenance of
   `MODULE_DEFINES`. `P3-W9 T7` step 1 retargeted both to
   `pkg/wuffs/build.zig:4-15` after re-verifying the ten-entry list is
   byte-identical there. The list was never wrong; only the citation rotted.
4. **What holds the line.** Nothing diffs the define list against the oracle
   automatically — stated plainly. The `wuffs-sys` build would fail loudly if a
   module gate were dropped (the decoder symbols would vanish), which covers
   removals but not additions.

## G12 — `po/` reference renumbering, and the 3 vicinae msgids — deliberate NON-ports

1. **Upstream.** `9c6f287aa` renumbers `#:` source references across all 32
   catalogs (**2,142 of the 2,516 changed `po/` lines**) and adds 3 msgids —
   `"The keybind was revoked by the system."`,
   `"The keybind was denied by the system."`, `"Global keybind unavailable"` —
   whose `msgstr` is empty in **all 32** upstream files.
2. **Why not.**
   * The `#:` renumbering cites `src/apprt/gtk/**` at **upstream's** line
     numbers. rustty's catalogs already carry **rustty's own** paths
     (`crates/rustty-apprt/assets/ui/1.2/surface.blp:323`), regenerated by
     WS13 T4 (`485d65b`). Copying upstream's renumbering would replace correct
     rustty references with references to files that exist in no rustty tree —
     making the metadata *differently* stale, not less stale. gettext ignores
     `#:` at lookup time, so the functional impact is zero either way.
   * The 3 vicinae msgids' only call sites are
     `src/apprt/gtk/class/application.zig:1708,1710,1721`, which is the
     `vicinae-hotkey-v1` work and is **`P3-W8 T7 step 3`**'s. Landing catalog
     entries before their call sites exist creates 96 lines that translate
     nothing and are reachable from nothing — and WS13 T1's
     `code_msgids_all_wrapped_and_in_pot` gate is written to FAIL on exactly
     that shape.
3. **What would break.** See above: a redder gate and worse metadata,
   respectively.
4. **What holds the line.** WS13 T1's msgid-coverage gate (it fails on a `.pot`
   msgid with no wrapped call site) and WS13 T4's `xtask update-translations`,
   which owns regeneration against rustty paths.

**STALE PREMISE FOUND (`P3-W9 T7` step 3).** The plan and the area-G assessment
both state that the bare `"Close Split"` msgid is missing from catalogs that
lack it, naming `po/be.po` as having only `"Close Split?"`. **That is no longer
true:** all **31** rustty catalogs already carry the bare msgid — verified with
`python3 .port/phase-3/w9-close-split-msgid.py` -> `LACK (0): []`, and by
reading `po/be.po:150-152` directly. WS13 T4 (`485d65b`, "extract gettext_/tr
msgids + scope the walk; regenerate the pot/catalog refs") added them when it
regenerated the catalogs. Only the zh_CN **translation** was outstanding, and
that is ported.

## G13 — `.github` (13 files), `nix/` (4), `flatpak/` (1), `build.zig.zon*` (4) — N/A wholesale [SUPERSEDED by §6 below, parity tail P-1, 2026-09-27: most of this is now ported]

1. **Upstream.** `VOUCHED.td` + 6 `vouch-*.yml` workflows (contributor-vouching
   governance for the upstream GitHub org); ~11 dependabot action-SHA bumps;
   `release-tip.yml` (+156), `release-tag.yml` (+16), `snap.yml`, `nix.yml`;
   `test.yml`'s Xcode 26.3 -> 26.6 plus a new `build-macos-xcode-27` job;
   `update-colorschemes.yml` (+6); all of `nix/`; `flatpak/zig-packages.json`;
   `build.zig`, `build.zig.zon` and its `.json`/`.nix`/`.txt` lock renderings.
2. **Why not.** None of these files exists in rustty, and each encodes
   infrastructure with no Cargo analogue: `ls .github/workflows` is `ci.yml`,
   `clean-artifacts.yml`, `milestone.yml`; there is no `nix/` and no
   `flatpak/`; rustty pins its toolchain in `rust-toolchain.toml`, not an Xcode
   matrix; `Cargo.lock` is the committed dependency lock.
3. **What would break if we ported them.** Workflows referencing jobs, secrets
   and artifacts that do not exist — CI noise that teaches reviewers to ignore
   red.
4. **What holds the line, and the two carve-outs.**
   * `update-colorschemes.yml`'s **payload** (the theme corpus) IS real work and
     is NOT dismissed: it is `P3-W9 T5`, landed, with the new `G-THEMES` digest
     gate.
   * `check-apple-libghostty-vt.nu` is N/A **with a precondition**, recorded in
     G7 above rather than here.
5. **AMENDED (wave 2, W2-05) — the release lane is no longer N/A.** The premise
   of §2 above ("`ls .github/workflows` is `ci.yml`, `clean-artifacts.yml`,
   `milestone.yml`") no longer holds for the release workflows: `release-tag.yml`,
   `release-tip.yml` and `publish-tag.yml` are now PORTED (Cargo/xtask shape:
   `xtask distcheck` for `zig build distcheck`, `macos/build-rustty{,-signed}.sh`
   for the inline xcodebuild/codesign blocks, the already-ported
   `dist/macos/update_appcast_{tag,tip}.py`), together with
   `.github/scripts/rustty-tip` (bash port of the nushell `ghostty-tip` probe).
   §3's objection is answered by construction: every job is gated on the
   `RUSTTY_RELEASE_ENABLED` repository variable, so on this repo as shipped —
   and on any fork — they are inert and can produce no red. `xtask/tests/
   release_workflows.rs` proves that gating on every CI run.
   **Still N/A here:** `snap.yml`, `flatpak.yml`, `nix.yml`, all of `nix/` and
   `flatpak/`, and `build.zig.zon*` — those remain Nix/flake- and GTK-app-coupled
   with no rustty counterpart. [STALE — see §6.]
6. **REWRITTEN (parity tail P-1, 2026-09-27) — what is ported, what is N/A, what
   is deferred.** §2's premise ("there is no `nix/` and no `flatpak/`") stopped
   being true when the build-system gap work (2026-08-25/26,
   `docs/superpowers/specs/2026-08-25-build-system-gap-audit.md`) ported the
   Nix flake, the Flatpak and the Snap. The record as of the pin `20abdb50a`:

   | Reference file(s) | Status | Rustty counterpart / reason |
   |---|---|---|
   | `flake.nix`, `flake.lock`, `default.nix`, `shell.nix`, `.envrc` | PORTED (Cargo shape) | crane + rust-overlay replace zig + zon2nix; differences recorded in `flake.nix`'s comments |
   | `nix/devShell.nix`, `nix/package.nix`, `nix/libghostty-vt.nix`, `nix/tests.nix`, `nix/build-support/{build-inputs,gi-typelib-path,ld-library-path}.nix`, `nix/build-support/check-blueprints.sh`, `nix/pkgs/*` (4), `nix/test-src/*`, `nix/vm/*` (16) | PORTED | `nix/`, same paths (`librustty-vt.nix`, `test_librustty_vt.c`); each file's header records its differences |
   | `nix/zigCacheHash.nix`, `nix/build-support/check-zig-cache.sh` | N/A — no Cargo role | they keep the generated Zig dependency renderings (`build.zig.zon.{nix,txt,json}`, `flatpak/zig-packages.json`) in sync; crane vendors from `Cargo.lock` directly, so there is no rendering to keep in sync. The one lock rustty commits is `Cargo.lock`; the Cargo shape of the check (`cargo metadata --locked` in the dev shell) is `nix.yml`'s job |
   | `nix/build-support/fetch-zig-cache.sh` | N/A — no Cargo role | a packager helper that pre-fetches the Zig package cache for offline builds; the Cargo equivalent is `cargo fetch --locked` / `cargo vendor`, and `xtask dist` already ships `vendor/` + `.cargo/config.toml` in the source tarball (PACKAGING.md "Source Tarballs") |
   | `nix/build-support/update-mirror.nu` | N/A — no Cargo role | mirrors the Zig package tarballs named in `build.zig.zon` to the reference's own file host; crates resolve from crates.io or the tarball's `vendor/` |
   | `build.zig`, `build.zig.zon`, `build.zig.zon.{json,nix,txt}` | N/A — replaced | `Cargo.toml` + `Cargo.lock` + `xtask` (the build graph itself is audited step by step in NEW-FINDINGS N377) |
   | `flatpak/com.mitchellh.ghostty{,-debug}.yml`, `flatpak/dependencies.yml`, `flatpak/exceptions.json` | PORTED | `flatpak/io.airely.rustty{,-debug}.yml`, `dependencies.yml`, `exceptions.json` |
   | `flatpak/zig-packages.json` | N/A — replaced by the vendored tarball | the Zig-dependency source list flatpak-builder needs because Flathub builds are offline. Rustty's manifests take the `xtask dist` tarball as their only source; it carries `vendor/` and a `.cargo/config.toml` redirecting crates.io to it, and the manifests set `CARGO_NET_OFFLINE`. P-1 built both manifests offline from that tarball on aarch64 Fedora (N370), so no `cargo-sources.json` counterpart is needed |
   | `snap/snapcraft.yaml`, `snap/local/launcher` | PORTED, UNVERIFIED | `snap/`; never built (needs snapd, i.e. root) — VERIFICATION_DEBT V24 |
   | `.github/workflows/flatpak.yml` | PORTED | merged with `test.yml`'s `build-flatpak`; differences recorded in its header |
   | `.github/workflows/nix.yml` | PORTED (P-1), UNVERIFIED, inert behind `RUSTTY_NIX_ENABLED` | `check-zig-cache-hash` → `check-cargo-lock`; VERIFICATION_DEBT V24 |
   | `.github/workflows/snap.yml` | PORTED (P-1), UNVERIFIED, inert behind `RUSTTY_SNAP_ENABLED` | VERIFICATION_DEBT V24 |
   | `.github/workflows/{release-tag,release-tip,publish-tag}.yml`, `.github/scripts/ghostty-tip` | PORTED (§5) | residual differences: NEW-FINDINGS N378 |
   | `.github/workflows/{update-colorschemes,milestone,clean-artifacts}.yml`, `.github/{dependabot,pinact}.yml`, `.github/DISCUSSION_TEMPLATE/issue-triage.yml` | PORTED | same paths |
   | `.github/workflows/test.yml` | TRANSLATED job by job | `ci.yml` jobs + `.port/tools/ci.sh` gates; the per-job table and the jobs still without a counterpart are NEW-FINDINGS N376 |
   | `.github/scripts/check-translations.sh` | TRANSLATED | G-I18N (`check_translations.sh`) |
   | `.github/scripts/check-apple-libghostty-vt.nu` | N/A with the §G7 precondition, **which has partly changed** — see G7's P-1 note |
   | `CLAUDE.md` (a symlink to `AGENTS.md`, reference `c09ade225`) | PORTED (P-1b, user decision 2026-09-27) | the same symlink, mode 120000 |
   | `.github/VOUCHED.td`, the **5** `vouch-*.yml` workflows (§1 said 6), `.github/DISCUSSION_TEMPLATE/vouch-request.yml`, `.github/issue-unvouched-message` | NOT PORTED — governance (user ruling) | contributor-vouching governance of the reference's own GitHub organisation; nothing for a port to reproduce |

   **Verification debt, not done:** Nix (`nix build`, `nix flake check`, the VM
   checks, `nix.yml`) and the Snap (`snapcraft`, `snap.yml`) are deferred by
   the user (2026-09-27): no Nix install, and snapd needs root. Recorded with
   the exact commands as VERIFICATION_DEBT V24. The Nix side was last run on
   aarch64-linux by the 2026-08-26 gap work; P-1's Nix edits are unverified.

---

## G14 — `P3-W9`'s G3b beyond-pin attribution (gate artifact, not a non-port)

`.port/tools/gates/README.md`'s G3b section requires each lane to record its own
beyond-pin `missing` delta — "which file, which sentence, and the upstream
commit that deleted it" — so the residual is attributable rather than merely
tolerated, and so that a rise with NO attribution can be treated as a real
regression. This is `P3-W9`'s record.

```
G3b [config]:    distinctive ref comments=2777  missing=28  review=13   (was 9)
G3b [cli]:       591 / 2 / 2      (unchanged)
G3b [synthetic]: 95 / 0 / 0       (unchanged)
G3b [vt]:        9609 / 81 / 48   (unchanged by this lane)
G3b [vt-c]:      334 / 0 / 0      (unchanged)
```

All **28** config entries are pin artifacts. Every one is a sentence the ORACLE
ITSELF DELETED beyond the pin, which rustty correctly no longer has; they exist
on the reference side only because `GHOSTTY_SRC` is the working tree at
`4749c4e93`. Attribution, all 28:

| # | Deleted by | Sentences (lowercased as the gate prints them) |
|---|---|---|
| 15 | **`2da02f4d2`** (background-blur rewrite, `P3-W9 T2` step 1) | `* kde plasma (wayland and x11)` · `supported on macos and on some linux desktop environments, including:` · `this setting to either \`true\` or any positive blur intensity value would` · `achieve the same effect. the reason is that kwin, the window compositor` · `powering plasma, only has one global blur setting and does not allow` · `applications to specify individual blur settings.` · `to configure kwin's global blur setting, open system settings and go to` · `"apps & windows" > "window management" > "desktop effects" and select the` · `"blur" plugin. if disabled, enable it by ticking the checkbox to the left.` · `then click on the "configure" button and there will be two sliders that` · `allow you to set background blur and noise intensities for all apps,` · `all other linux desktop environments are as of now unsupported. users may` · `need to set environment-specific settings and/or install third-party plugins` · `in order to support background blur, as there isn't a unified interface for` |
| 5 | **`d71695550`** (keybind `global:` rewrite, `P3-W9 T2` step 2) | `on linux, you need a desktop environment that implements the` · `protocol as a part of its xdg desktop protocol implementation.` · `(important: [\`xdg-desktop-portal-hyprland\`](…))` · `- notably, global shortcuts have not been implemented on wlroots-based` |
| 4 | `65c48213b` (scrollback-limit doc rewrite) — **pre-existing**, not this lane | `allocated lazily up to this limit, so if you set this to a very large` · `larger this value, the larger potential memory usage. scrollback is` · `scrollback currently exists completely in memory. this means that the` · `value, it will not immediately consume a lot of memory.` |
| 5 | `2970e9a2a` (`Color.fromHex` + `Palette.parseCLI` deletions) — **pre-existing** | `fromhex parses a color from a hex value such as #rrggbb. the "#"` · `expand short hex values to full hex values` · `parse the color part (color.parsecli will handle whitespace)` · `parse the key part (trim whitespace)` · `trim any whitespace before processing` |

So `P3-W9` added **+19** (15 background-blur + 4 keybind); the other 9 were
already there before this lane and belong to `P3-W3 T4` and to the already-landed
`2970e9a2a` port. The plan predicted ~+27 (~15 and ~12) for a total of ~39; the
measured total is **28**, because the gate counts *distinctive* comments (>= 5
word-tokens after normalization), which is fewer than the raw deleted lines.

**`P3-W10 T3` must bring the config row to 0.** If any of these 28 survives the
pin advance, it is a REAL drop and must be triaged individually — that is the
entire value of listing them here rather than reporting a number.

# `check_tests_ported.py` delta from the `max_scrollback` rename (P3-W3 T2)

`86f81fb5b` renames `Terminal.Options.max_scrollback` to `max_scrollback_bytes`
**and renames the three tests whose names embed it**. The port follows upstream,
so three reference names that still exist at the PIN no longer match:

| reference name (still present at the PIN) | now named |
|---|---|
| `vt::Terminal: scrollUp with max_scrollback zero` | `terminal_scroll_up_with_max_scrollback_bytes_zero` |
| `vt::Terminal: scrollUp with max_scrollback zero and top margin` | `terminal_scroll_up_with_max_scrollback_bytes_zero_and_top_margin` |
| `vt::Terminal: scrollUp with max_scrollback zero and left/right margin` | `terminal_scroll_up_with_max_scrollback_bytes_zero_and_left_right_margin` |

**No test was deleted or weakened** — only the three names changed, following
`86f81fb5b`'s own rename (`git show 86f81fb5b -- src/terminal/Terminal.zig` shows
the three `test "..."` headers renamed in the same hunk as the field). This is the
same pin-lag mechanism as the P3-W7 rows above and as `P3-RV2`: the pin advance
(`P3-W10`) resolves it **by construction**, because the reference names become the
`_bytes` forms the port already uses. A per-file `GHOSTTY_SRC` override to paper
over them is **forbidden**.

`vt::HashMap ensureUnusedCapacity with tombstones` is **not** this workstream's
(P3-W3 touches no HashMap code).

---

# Area A — file-level coverage audit (2026-07-31)

Added by the **per-file oracle→port coverage audit**
(`.port/tools/audit_file_coverage.py`), which asks the blunt question no existing
gate asks: *is there a Rust counterpart for this oracle file at all?* G1/G2/G3
read a **frozen phase-0 inventory**, so a file upstream added after the pin is in
none of them — these three were invisible to every gate.

All three arrive with `e8525c0fd` "Update to Zig 0.16.0", the same commit behind
config's 4 remaining G3b misses and global's 31.

## A1 — `src/datastruct/segmented_list.zig` (NEW, 468 lines) — do NOT port as a file

1. **Upstream.** Zig 0.16.0 removed `std.SegmentedList`, so upstream vendored it.
   Its one load-bearing property is **pointer stability across appends**.
2. **Why not.** Rust has no `SegmentedList`, and porting a growable-array
   container as a container would be porting a *Zig-std gap*, not a behaviour.
   What must survive is the guarantee, and it does — at both use sites, checked
   individually rather than assumed:
   - `src/terminal/osc/parsers/color.zig`'s `List` → `Vec<Request>`
     (`crates/rustty-vt/src/osc/parsers/color.rs:417`). Nothing holds a pointer
     into it across a push, so plain `Vec` is sufficient and the PORT NOTE says so.
   - `src/font/Collection.zig`'s `StyleArray` → `Vec<Box<EntryOrAlias>>`
     (`crates/rustty-font-shaper/src/collection.rs:885`). Here the guarantee IS
     load-bearing — `alias: *mut Entry` pointers must survive growth — so each
     element is heap-boxed. `clippy::vec_box` is `allow`ed at that site with the
     reason recorded, because "already on the heap" is exactly the point.
3. **What porting it would break.** Nothing, but it would add an unused container
   and a second way to express a guarantee already expressed at each site.
4. **What holds the line.** `collection.rs`'s alias tests: a bare
   `Vec<EntryOrAlias>` moves elements on realloc and invalidates the alias
   pointers, which is UB in Rust rather than merely wrong — so the `Box` is
   guarded by the type, not by a comment.
5. **CORRECTION (parity tail S7-7, N320).** Point 2's first leg no longer
   holds. Once the OSC color `List` grows through the C caller's allocator
   (S174-F3), `SegmentedList`'s shape is observable to that allocator: two
   preallocated slots mean an OSC with one or two requests allocates nothing,
   and the shelves are the calls it sees for more; a `Vec` allocates on the
   first push and doubles. The file is now ported as
   `crates/rustty-datastruct/src/segmented_list.rs` (its two reference tests
   included), and `color::List` is `SegmentedList<Request, 2, VtAllocator>`.
   `collection.rs` keeps its `Vec<Box<_>>` (its allocations are not
   caller-routed).

## A2 — `src/lib/compat/fd.zig` (NEW, 74 lines) — N/A

1. **Upstream.** `close`/`pipe`/`pipe2` shims over `std.posix`, whose signatures
   Zig 0.16.0 changed.
2. **Why not.** A shim that exists to bridge two versions of Zig's std has no
   Rust counterpart by construction.
3. **Equivalent.** `crates/rustty-os/src/pipe.rs:23` `pub fn pipe()`, hand-rolled
   over `extern "C"` per the standing rule that this port never adds the `libc`
   crate. `passwd.rs`'s uses resolve the same way.

## A3 — `src/lib/compat/reader.zig` (NEW, 55 lines) — N/A

1. **Upstream.** `readByte`/`readerInt`/`readStructEndian`/`readSkipBytes`/… over
   `std.Io.Reader`, the 0.16.0 I/O migration.
2. **Why not.** Same class as A2: a bridge between Zig std versions.
3. **Equivalent.** rustty's opentype readers use a slice cursor —
   `read_be_i16_init(data, &mut pos)` (`crates/rustty-font/src/opentype/glyf.rs:411`),
   with `svg.rs` and `feature.rs` on the same idiom. This is not a shim but a
   different shape, and it is the shape that made `P3-W8 T4`'s bounds-guard
   control **vacuous** — `slice::get(pos..pos+n)` already yields `EndOfStream`
   for any out-of-range `pos`, so the oracle's explicit guard is R1-faithful but
   observably redundant here.

# G3b NOT-CARRIED — stripped gag prose in code comments (`strip-ghostty-gag-prose`) [parity tail S-8d, 2026-09-24; revised S-8e]

**RATIFIED BY THE USER (2026-09-24).** The user's rule (memory `strip-ghostty-gag-prose`)
originally covered prose/prompt files, "not code". Shown these four lines, the user chose
"drop jokes, keep facts" for code comments too (following the WS16 precedent,
`docs/superpowers/plans/2026-07-29-ws16-x11-winproto.md:21`): a joke or insult is
dropped, and any fact on the same line is carried.

Each dropped line is listed below and is a `gag-prose` row of
`.port/parity-closure/COMMENT-NOT-CARRIED.tsv`, which G3b validates (the text is
the oracle line at the pin; no digit, identifier, ALL-CAPS token or technical term
unless the row proves, `MIXED-CARRIED`, that the port carries its fact).

- `apprt/gtk/class/global_shortcuts.zig:380`: opinion only; the XDG-portal note's
  facts are carried verbatim in `global_shortcuts.rs` `request`.
- `apprt/gtk/class/global_shortcuts.zig:396`: MIXED. The joke is dropped; its fact
  (clients must construct the request path themselves) is carried, reworded, on
  the next line of `request`.
- `apprt/gtk/class/global_shortcuts.zig:406`: a closing quip, no fact.
- `apprt/gtk/winproto/BlurRegion.zig:58`: an insult, no fact; `:59-61` (the
  ~30% CSD fact) are carried verbatim in `blur_region.rs`.
- `apprt/gtk/class/application.zig:3486`: MIXED (parity tail L-2). The jab is
  dropped; `:3484-3485` (`isActive` returns 1, 0 means equality) are carried verbatim.
- `apprt/gtk/winproto/x11.zig:486`: a closing quip, no fact (controller, 2026-09-26,
  after the gate learned `X11` as a proper-noun token); `:482-484` (the alignment
  note) are carried verbatim in `x11.rs`.
