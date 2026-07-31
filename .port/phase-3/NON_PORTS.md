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

---

# Area G — `P3-W9` (config / CLI / build / i18n / synthetic)

**READ FIRST — two rows here are OPEN, and this file is otherwise a CLOSED
record.** `G1` (uucode) and `G2` (vaxis/zf) are `P3-W9 T6` work that was
**measured and decided but NOT completed**. They are written up with full
evidence instead of being quietly dropped, but they have **NO OWNER**: `P3-W9`
closes with this commit and no other workstream claims them. Whoever sequences
the remaining workstreams must assign them, or `.port/phase-2/DEFERRALS.md` /
`VERIFICATION_DEBT.md` must carry them with an owner in an open workstream so
the `G-P2` closure gate can see them. Do not let this paragraph be the only
thing holding them.

Appended by **`P3-W9` Task 6 and Task 7**. Same 4-criterion basis as above.
Source of the verdicts: `.port/phase-3/assessments/G-config-build-i18n.md`.

## G1 — `uucode` revision bump (`e8525c0fd` / `f2a7652ab`) — DEFERRED, NOT non-ported

**This row is the ONE open item in this file.** It is recorded here rather than
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

## G2 — `vaxis` 0.5.1 -> 0.6.0 and `zf` 0.10.3 -> 0.11.0 — DEFERRED with a measured delta

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
4. **What holds the line.** The three PORT NOTEs themselves, now carrying the
   re-check; and `crates/rustty-xev`'s ported test suite, which encodes the
   quirky behaviour (the double-cancel test asserts success twice).

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

## G13 — `.github` (13 files), `nix/` (4), `flatpak/` (1), `build.zig.zon*` (4) — N/A wholesale

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
