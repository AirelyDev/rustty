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
