# Research dossier: bugs

Area: defects found during the Rustty port. Three parts:

- (a) defects in Ghostty (the upstream) that porting surfaced
- (b) bugs the port introduced, and the gates that caught them
- (c) how the unsafe code was verified (Miri lanes and the sanitizer matrix)

Repo head at time of research: `1d22ba80d` (branch `phase-8-upstream-resync`, 2026-10-08).

Provenance tags:

- MEASURED: computed here; the command or script is given.
- DOCUMENTED: read from a repo record; the path is given.
- ESTIMATED: a heuristic classification. It is labelled wherever it appears.

Scripts that regenerate every CSV live in `.port/journey-article/research/tmp/`:

- `parse_ud.py` and `agg_ud.py` parse the upstream-defect ledger.
- `parse_n.py` parses the port-findings ledger.
- `mine_commits.py` mines commit subjects.

Run each with `python3 -I <script>`.

---

## 0. Headline numbers

| Fact | Value | Provenance / evidence |
|---|---|---|
| Upstream defect entries (UD ids) in the ledger | **205** (UD1..UD378, sparse numbering) | MEASURED: `parse_ud.py` over `.port/parity-closure/UPSTREAM-DEFECTS.md` (the `### UD<n>` headings minus the format template) |
| ...of which live (crash, UB, leak, deadlock/hang, intent-breaking) | **135**. Breakdown: 60 crash, 19 UB, 14 leak, 9 deadlock/hang, 33 intent-breaking | MEASURED: `bugs-upstream-defects.csv`, column `live` |
| ...of which latent (unreachable at the pin, dead code, unbuilt code) | 59 | MEASURED, same file |
| ...of which weaknesses in Ghostty's own tests | 11 | MEASURED, same file |
| Entries in Ghostty's own code | 172. The other 33 are in pinned dependencies: libvaxis 17, libxev 8, Zig std 3, google-highway 2, GStreamer 1, GTK 1, zf 1 | MEASURED: `bugs-upstream-project.csv` |
| Upstream fixes Rustty ships and lists for users | **141** UD ids in `docs/ghostty-bug-fixes.md` (138 table rows) | MEASURED: `in_user_docs` column |
| Severity (the ledger's own rating, weighted by reachability) | 7 high, 28 medium, 170 low | MEASURED: `bugs-upstream-severity-x-class.csv` |
| Crash-class entries that are UB in ReleaseFast (Ghostty's shipped build mode) | 38 of 60. Another 13 give a wrong value silently and 9 still crash | ESTIMATED: classified from each entry's `ReleaseFast:` field (`bugs-upstream-crash-releasefast.csv`) |
| Reported upstream | **0** filed so far. Before Rustty reported anything, 4 had been fixed upstream independently and 5 partly. 1 is obsolete upstream. 2 wait for private disclosure | MEASURED: `bugs-upstream-status.csv` |
| Port-findings ledger (bugs in the port itself, plus gate and records defects) | **334** entries (N1..N530) | MEASURED: `parse_n.py` over `.port/parity-closure/NEW-FINDINGS.md` |
| Findings explicitly tagged `kind: port soundness` (aliasing-model UB in Rust that Ghostty does not have) | 20. Ids: N270 N273 N385 N391 N393 N394 N430 N431 N434 N437 N440 N441 N443 N444 N451 N452 N454 N456 N459 N465 | MEASURED: awk over NEW-FINDINGS.md |
| Curated timeline of memory-safety, concurrency and ABI bugs the port introduced | 38 events covering 81 individual defects. 29 events are in production code, 8 in tests, 1 in a benchmark | MEASURED by hand from commits and N entries: `bugs-port-memory-safety-timeline.csv` |
| ...of those 81 defects, found by Miri (or a Miri-proven LIFETIMES audit) | 62 | same file |
| First full Miri sweep of `rustty-vt` (2026-07-11) | 2,037 tests: 1,948 clean, **9 real Stacked Borrows UB** (one class), about 70 intractable | DOCUMENTED: `.port/phase-2/VT_MIRI_SWEEP.md`, commit `3277f2f78`; fix `9e3fe1c25` |
| Last full Miri lanes (2026-10-06, head `04f0a894c`) | G6 GREEN over 38 crates in 13,354 s (`rustty-vt` 3,104 tests in 9,525 s). G6c 124/124 in 1,110 s. G6t (Tree Borrows) GREEN after fixing N500 and N505 | DOCUMENTED: `.port/parity-closure/README.md` |
| Sanitizer matrix (2026-10-02 to 2026-10-07) | ASan 9,850 tests; LSan 10,111 (Linux) and 9,850 (macOS); MSan 6,183; TSan 4,380; Valgrind 6,614 tests in 914 memcheck processes. All GREEN after their first-run findings | DOCUMENTED: `.port/parity-closure/README.md` |
| Count of `unsafe` sites at HEAD | **22,310** sites in 836,905 lines of Rust: 26.66 per kLOC, 3,028 `SAFETY` comment lines | MEASURED with the census command in `.port/audits/count-unsafe.sh` (`rg --no-config -o 'unsafe \{\|unsafe fn\|unsafe extern\|unsafe impl\|unsafe trait' --glob '*.rs' --glob '!**/vendor/**' --glob '!**/cbits/**' crates crates-sys xtask`) |
| Dedicated Miri proof harness crates | 27 (`.port/phase-2/mirihar-*`) | MEASURED: `ls -d .port/phase-2/mirihar-* \| wc -l` |

---

## (a) Upstream (Ghostty) defects found by porting

### a.1 How the policy changed: faithful porting, then R13

- **Until 2026-09-11, reference bugs were ported faithfully and recorded as `U` ids.** Examples:
  - commit `49edf86a4` (2026-07-31): "record 2 faithfully-ported upstream bugs"
  - `13ec8ffec`: "U3 tmux overflow upstream bug"
  - `445239ad0` (2026-08-25): "fc-match double-destroy (UPSTREAM BUG P4-U3)"

  DOCUMENTED: `git log` subjects.
- **2026-09-11: R13, "Reference defects are not ported".** It started as a user ruling. One task ported a null-unwrap panic in GTK `global_shortcuts.zig` to keep parity (`dae5118b`). The user overruled that, and the port reversed it (`d9de0339`).

  Under R13, a reachable crash, UB, leak or deadlock is handled the way the reference evidently intends. Each such fix gets three things: a test with a negative control, a `PORT NOTE: reference defect (R13)` at the site, and a UD entry. Assertions of real invariants are still ported.

  DOCUMENTED: `.port/PORT_RULES.md:346`; memory note `reference-defects-not-ported.md`. The ledger file was created the same day (`7dd991582`, 2026-09-11).
- **2026-09-23: R13 was extended to non-crash bugs that break the code's evident intent.** Examples: output lost to a missing flush, an inverted XDG check. Every such divergence also gets a line in `docs/ghostty-bug-fixes.md`.
- **2026-09-24: disclosure rule.** A UD marked "report PRIVATELY" never has its trigger in public docs.
- Effect on the curve: 66 entries were recorded on 2026-09-11 alone, by a retroactive sweep of earlier work. So **ledger dates are recording dates, not discovery dates**, for that first batch. Every later batch was recorded by the task that found the defect, so the later dates are close to discovery dates.

  MEASURED: `bugs-upstream-cumulative.csv`.

### a.2 Counts by class and area

Class is MEASURED from each entry's `kind:` field. Raw kinds are normalized into 7 classes; the mapping is in `parse_ud.py` `klass()`. The area grouping is a path-prefix rule on `reference:`, plus 16 manual overrides where that field gives only a commit hash (`OVR` in `parse_ud.py`).

| area | crash | UB | leak | deadlock/hang | intent-breaking | latent | test-weakness | total |
|---|---|---|---|---|---|---|---|---|
| app runtimes (GTK/macOS/embed) | 15 | 8 | 4 | 1 | 4 | 10 | 2 | 44 |
| VT core + libghostty-vt (incl. tmux, C API, render state) | 19 | 1 | 3 | 1 | 5 | 7 | 3 | 39 |
| CLI + TUIs (incl. libvaxis) | 8 | 2 | 0 | 2 | 7 | 9 | 1 | 29 |
| termio/pty/os | 4 | 2 | 1 | 4 | 6 | 6 | 0 | 23 |
| renderer | 7 | 3 | 3 | 1 | 1 | 7 | 0 | 22 |
| font | 5 | 1 | 3 | 0 | 2 | 6 | 3 | 20 |
| build/test infra | 0 | 0 | 0 | 0 | 6 | 4 | 0 | 10 |
| event loop (libxev) | 0 | 0 | 0 | 0 | 0 | 6 | 2 | 8 |
| dependencies (Zig std / highway / GStreamer / zf) | 2 | 2 | 0 | 0 | 2 | 1 | 0 | 7 |
| misc (datastruct / docs / IPC) | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 3 |
| **total** | **60** | **19** | **14** | **9** | **33** | **59** | **11** | **205** |

Data: `data/bugs-upstream-area-x-class.csv`. A finer subsystem breakdown is in `data/bugs-upstream-subsystem-x-class.csv`.

Notes:

- VT core has the most crashes (19). A large share of them sit behind tmux control mode (UD6-UD12) and libghostty-vt's C API. These are the inputs a byte stream or an embedder controls.
- Over a third of the crash class traps only in safety-checked builds. Of the 60 crash-class entries, 38 are undefined behaviour in a ReleaseFast build, typically:
  - a wrapped integer used as a size
  - a refcount corrupted past `u16` (UD80, UD361)
  - an out-of-bounds read

  ESTIMATED: keyword classification of the `ReleaseFast:` field (`bugs-upstream-crash-releasefast.csv`).
- Reachability (ESTIMATED, keyword classification of `reachability:`, `bugs-upstream-reachability-x-class.csv`):

  | Reachable through | Entries |
  |---|---|
  | library C API or embedder | 50 |
  | none at the pin | 45 |
  | config file | 26 |
  | VT stream (terminal output) | 22 |
  | file or font input | 16 |
  | OS or driver behaviour | 10 |
  | peer process (D-Bus/X11/Wayland) | 7 |
  | other | 22 |

### a.3 How the defects were found (ESTIMATED)

The ledger has no structured "found by" field: only about 40 entries name a discoverer in prose. Method is therefore a keyword heuristic over each entry, with this order of precedence:

1. sanitizer gate
2. Miri
3. fuzz
4. test-weakness
5. latent-compile
6. allocation failure
7. Windows
8. "by reading"
9. checked cast or overflow
10. default

| method (heuristic) | entries | what it means |
|---|---|---|
| line-by-line port review | 96 | found while translating a function and its tests line by line; the default bucket |
| checked arithmetic and casts | 29 | a Zig `@intCast`, `@truncate` or overflow on input-controlled data. Rust forces a choice at the conversion (`try_from`, checked or saturating ops), which forces a range analysis. All 29 are crash class |
| code reading (port review) | 25 | the entry says "found by reading" or "not reproduced". This bucket holds 9 of the 19 UB entries: data races and use-after-free on macOS and GTK threads |
| compiler/build (porting an unbuilt path) | 16 | Zig analyses lazily, so code no build reaches is never type-checked. Rust compiles everything, so porting it turned up 10 `latent (compile)` entries, e.g. `@typeInfo` tags removed in newer Zig (UD40) |
| cross-platform bring-up (Windows/Linux VM) | 16 | found by running the port on `rwin` (Windows 11 ARM64) and `rvm` (Fedora aarch64), e.g. UD325, UD341-UD346, UD358, UD359 |
| reference test review (test port) | 11 | Ghostty tests that cannot fail, or that double-free in test code (UD109) |
| fault injection (allocation failure) | 7 | failing-allocator tests reaching `errdefer` paths (UD305, UD315, UD364, UD369) |
| sanitizer gate | 3 | UD376 (G-LSAN's first macOS run: Dear ImGui's Metal backend compiled without ARC leaks every frame) and UD377 (G-TSAN: a data race in Highway's chosen-target mask); 1 more by keyword |
| withheld | 2 | the private-disclosure entries |

Data: `data/bugs-upstream-method-x-class.csv` and `data/bugs-upstream-by-week-method.csv`.

**Important caveat for the article.** Miri appears in 6 upstream entries, but as a *confirmation* tool, not the discoverer. UD352, UD363 and UD368 were found by reading; then a Miri model harness (`.port/phase-2/mirihar-n437`, `mirihar-n444`) showed the reference's order is UB under Stacked and Tree Borrows and the fixed shape is clean over 16 seeds. Zig has no aliasing model, so Miri cannot see upstream bugs directly. It can only check a Rust model of the same access pattern.

### a.4 Status upstream

MEASURED: `bugs-upstream-status.csv`.

- 193 not reported. The ledger is written to be filed later: every entry has a `proposed fix:` field.
- 2 waiting for private disclosure (UD131, UD351).
- 4 fixed upstream independently: UD20, UD55, UD150, UD312. In each case Ghostty later fixed the same defect, which independently confirms it was real.
- 5 partly fixed upstream: UD31, UD40, UD140, UD141, UD200.
- 1 obsolete upstream: UD32.

### a.5 The privately reported entries (category and impact only)

- **UD131**: crash class, VT core. Certain terminal output crashes Ghostty in every build mode. Impact: a remotely reachable denial of service from ordinary terminal output.
- **UD351**: crash class, VT core, a different trigger. Certain terminal output crashes Debug and ReleaseSafe builds and hangs ReleaseFast builds. Impact: also a remotely reachable denial of service.

Both are withheld until Ghostty has the private reports. Rustty does not crash or hang on either. The public docs give no details (`docs/ghostty-bug-fixes.md` rows UD131 and UD351). **The internal ledger holds full recipes: do not quote it.**

### a.6 Most technically interesting upstream cases (mechanism level)

1. **UD291: Highway never sees CPU features on non-x86 Linux.**
   - `pkg/highway`'s Linux detection calls `std.os.linux.getauxval`. That reads `elf_aux_maybe`, which only Zig's own `_start` sets.
   - Ghostty always links libc on Linux, and libghostty-vt has no Zig `_start` at all. So `getauxval` returns 0 for every key, and SIMD dispatch stays at the baseline target (`neon_without_aes` on aarch64).
   - Measured on the aarch64 VM.
2. **UD80 / UD361: the `u16` style refcount at about 47,900 columns.**
   - When the width no longer fits the standard page budget, `initialCapacity` keeps 215 rows and widens the page. One page can then hold more than 10 million cells sharing a style, against a `u16` refcount.
   - The first failing width was measured at **47,932** columns (macOS arm64 ReleaseSafe, 16 KiB pages) and **46,908** on Linux aarch64.
   - Result: a trap in safe builds, refcount corruption in ReleaseFast.
3. **UD362: an app-wide deadlock.**
   - The OpenGL render thread pushes `.redraw` with `.forever` while holding `draw_mutex`.
   - If the 64-entry app mailbox is full and the GTK main thread realizes or unrealizes that surface (moving a tab, closing a split), the main thread blocks on `draw_mutex`. It is also the only thread that drains the mailbox, so neither thread can proceed. macOS has a variant through the Metal completion thread.
   - Rated high when reached.
4. **UD10: tmux layout recursion.**
   - `parseNext` recurses once per `{` or `[`, and a layout may be up to 1 MiB (about 116,000 levels).
   - Measured stack overflow at about 9,200 levels in Debug and about 39,000 in ReleaseSafe and ReleaseFast, on both macOS and Linux.
   - Rustty caps the depth at 1,000 levels, the limit tmux itself adopted.
5. **UD19: final output lost at exit (Linux).**
   - The pty read thread returns on `POLLHUP` without checking `POLLIN`, although the same function's bridge poll handles that case correctly.
   - The port running Ghostty's decision lost the output of a command that prints and exits in 23 of 40 runs. With the fix, 40 of 40 were correct.
6. **UD205: OSC 4/10-19 color queries.**
   - Replies go into an allocating writer over a 1024-byte fixed buffer. The growth policy (`n + n/2 + init`) fails well before 1024 bytes: 25 palette reports fit.
   - On failure, `error.WriteFailed` drops *every* reply, plus any set or reset later in the same sequence.
7. **UD377: Highway target-mask data race (found by G-TSAN).**
   - When two threads make their first SIMD call at the same time, both read and write Highway's chosen-target mask with no synchronization. That is a C++ data race, i.e. UB.
   - The fix: relaxed `__atomic` builtins in the vendored `targets.h`.

---

## (b) Bugs the port introduced, and the gates that caught them

### b.1 The recurring mechanism: Zig pointers have no aliasing model; Rust references do

The dominant class of port bug is **Stacked or Tree Borrows UB that does not exist in Zig**.

- **Zig `*T` and `[]T` arguments are unprotected raw pointers.** A natural translation to `&mut T` adds a *protector* for the duration of the call, and a Unique retag on every reborrow. Several situations then become UB:
  - a reference re-read across `assert_integrity`
  - a stored raw alias into a struct that is later reborrowed as a whole
  - a host callback that re-enters an object while a `&mut self` is live
- The doctrine came from the first such bug (`3662e56bf`, 2026-06-24, `clear_cells`): port `*Page`, `*Row` and `[]Cell` as raw-pointer arguments (memory note `rust-mut-args-strongly-protected`).
- Data: `data/bugs-port-memory-safety-timeline.csv` (38 events, 81 defects).
  - 62 of the 81 were found by Miri or Miri-proven audits.
  - 6 by sanitizers.
  - 13 by symptom, review or stress.

### b.2 Most technically interesting port bugs

1. **Search subsystem: 9 Stacked Borrows UB tests** (found 2026-07-11 by the first full `rustty-vt` Miri sweep; fixed in `9e3fe1c25`).
   - Search objects stored `PageListSearch.list: *mut PageList`, a pointer *into* the `Screen` allocation at the `.pages` offset.
   - The port had turned the reference's `*Screen` parameters into `&mut Screen`. Each whole-`Screen` Unique retag popped the stored alias, so the later `untrack_pin` through it was UB.
   - Fix: restore `*mut Screen` across `highlight.rs`, `search/screen.rs` and `search/pagelist.rs`.
   - A memory note kept reporting this as open for 18 days after the fix: a stale-record failure the project later turned into a rule ("re-measure before inheriting a known-red lane").
2. **ABI: a by-value struct size mismatch caused an AAPCS64 sret crash** (2026-07-15, `40727a2cb`).
   - `TriggerC` embedded a non-`repr(C)` `Mods`, which made it 20 bytes in Rust against 12 in `rustty-embed.h`.
   - On AArch64, results over 16 bytes come back through a hidden `x8` sret pointer, and smaller ones in registers. Swift and Rust therefore disagreed on the calling convention: SIGBUS on the first call.
   - Visible symptom: the View menu shortcuts were missing. An `#[allow(improper_ctypes_definitions)]` had been masking the lint.
   - Gate A4 (`.port/tools/gates/check_embed_h_abi.sh`) now clang-probes every by-value aggregate's size and field offsets.
3. **Parallel `cargo test` exposed a C-init race** (2026-07-01, `6cf99b4cd`; recurred as N102 on 2026-09-23).
   - Ghostty's `ensureInit` never sets its `initialized` flag, so `onig_initialize` runs on every call. That is harmless under Zig's serial `zig test`.
   - Under thread-per-test `cargo test` it corrupted the C heap. The crash surfaced in an unrelated arena (`ArenaAllocator::usable` underflow) and was found by bisecting test groups, not by reading the crash site.
   - N102 measured 27 crashes in 1,500 parallel runs (20 SIGTRAP, 6 SIGABRT, 1 SIGSEGV), and 0 single-threaded.
   - Fix: `OnceLock`. Miri could not see the race because the fault is in C.
4. **Uninitialized padding fed to LZ4, and an argument freed mid-call** (2026-07-30, `9d8fafb82`, the P3 vt Miri sweep).
   - The page compressor hashes the whole page mapping as `&[u8]`. `OffsetSlice<T>` (4 tail-padding bytes) and the `PageEntryId` enum (7 discriminant-padding bytes) left uninitialized bytes in it. Zig may read undefined bytes; Rust may not. Both became byte-complete `#[repr(C)]` types with named padding, at the same size.
   - Same commit: `Screen::start_hyperlink` frees the current hyperlink, which its own `&[u8]` argument may point into. That is UB under protectors. It moved to a raw-slice `unsafe fn`.
5. **xev ThreadPool: three findings in one module** (2026-07-31, `70a4b2909`):
   - a test-side `container_of` through a narrow field tag (wildcard provenance)
   - `deinit(&mut self)`'s protector against workers' `&self`
   - a real data race: `join()`'s `Relaxed` load raced a peer's frame teardown, so it became `Acquire`

   The same day, `5243303fb` found why ASan had been "crashing" on this module. Zig gives `stack_size` a field default of 16 MiB. A Rust struct literal requires every field, and two tests wrote `0`, which gave a 16 KiB thread stack and a BUS error. The misdiagnosis ("an ASan limitation") had removed the module from ASan coverage.
6. **A parity bug found by perf profiling** (2026-07-16, `10f2091`).
   - The same-page clone path used `continue` where Zig's `break :hyperlink` exits only the labelled block. A cell with both a hyperlink and a style lost its style.
   - It was found through an instruction-level diff against the Zig build.
7. **Cross-thread references to the whole renderer or surface** (Phase 7: N437, N440, N441, N443, N444, N456, N459, N465; 2026-09-28 to 2026-09-30).
   - On macOS, CoreAnimation draws on the main thread through a raw pointer while the render thread calls `&mut self` methods on the whole `Renderer`. Host callbacks also re-enter `Surface` and `App` under protected borrows.
   - Each case was modelled in an isolated harness (`mirihar-n437`, `n440`, `n441`, `n444`, `n456`, `n465`) and proven UB before the fix and clean after, under both SB and TB. Hand reasoning was not accepted, per memory note `shaper-hook-sb-clean-interleave`.
8. **Provenance: an 8-byte store through a 1-byte field pointer** (N393, 2026-09-27).
   - `Style::write` ported `@as(*align(1) u64, @ptrCast(&out.bold)).* = bytes` as `(&mut out.bold as *mut bool as *mut u64).write_unaligned(..)`, which has provenance for 1 byte.
   - Fix: derive the pointer from the whole struct, with an `offset_of!` offset.

### b.3 The port-findings ledger over time (ESTIMATED classes)

`data/bugs-port-findings.csv` has all 334 N entries, with severity and the first commit that added each. The class comes from keywords in the title, then the body (`parse_n.py`), and is approximate:

| class | entries |
|---|---|
| behaviour divergence from Ghostty | 192 |
| crash/panic | 37 |
| logging/diagnostic fidelity | 31 |
| leak | 25 |
| ABI/header | 17 |
| aliasing UB (Miri) | 14 |
| gate/instrument | 11 |
| race/deadlock | 4 |
| memory error (ASan/MSan) | 3 |

By severity: 9 high, 82 medium, 201 low, 12 informational, 8 none, 22 unrated. The ledger was opened during the parity closure: its first entry is in week 2026-09-07, and weekly counts by first appearance are 53, 178, 77 and 24. Before September, port bugs were tracked only in commit messages and phase records. That is why the curated timeline (b.1) mixes commit hashes with N ids.

### b.4 Fix-commit share per week (weak signal)

`data/bugs-port-fix-share-by-week.csv` counts non-merge commits whose subject matches fix, bug, UB, crash, leak, race and similar words.

- The share runs from 5% (week of 2026-06-22, initial bulk porting) to 28% (week of 2026-09-07, the parity closure).
- `data/bugs-port-fix-commits.csv` lists them with a regex category. 274 of the 478 matched commits fall into "other fix", which includes review fix rounds.
- Treat this as context, not a bug count.

---

## (c) Unsafe-code verification

### c.1 The tool for each tier

The tiers are set by rule R6, `.port/PORT_RULES.md`:

- Miri for pure-Rust crates (G6 Stacked Borrows, G6c with debug assertions off, G6t Tree Borrows)
- ASan for FFI crates
- the reference's Valgrind job, reproduced as G-VALGRIND

Each Miri exception has to meet four criteria, criterion 3 being "no unsafe path uncovered". Those criteria are in `.port/phase-2/MIRI_EXCEPTIONS.md`. G-ASAN also cross-checks every Miri-excepted test: all **468** must have run and passed under ASan, or be on a per-OS expected-absent list.

### c.2 Lane results

From `data/bugs-verification-gates.csv`:

| gate | date | scope | result |
|---|---|---|---|
| Miri G6, first `rustty-vt` sweep | 2026-07-11 | 2,037 tests | 9 SB UB (fixed the same day) |
| Miri G6 on nextest | 2026-10-06 | 38 crates, 13,354 s | GREEN |
| Miri G6c | 2026-10-06 | 124 tests, 1,110 s | GREEN |
| Miri G6t (Tree Borrows) | 2026-10-02 | vt 1,597+119, vt-c 570, core 67, xev 22, renderer 60 | GREEN after N500 and N505, 8 test-side TB UB in all |
| G-ASAN (macOS) | 2026-10-02 | 9,850 tests | GREEN |
| G-LSAN (Linux, macOS) | 2026-10-02 | 10,111 and 9,850 tests | GREEN after UD376, N498, N499, N502, N503 |
| G-MSAN (Linux) | 2026-10-02 | 6,183 tests | GREEN, 0 in Rustty code |
| G-TSAN (macOS) | 2026-10-07 | 4,380 tests, 38 min | GREEN after UD377 |
| G-VALGRIND (Linux) | 2026-10-07 | 914 processes, 6,614 tests, 3 h 44 min | 0 errors in Rustty code; Mesa EGL suppressions only |

Moving Miri to `cargo miri nextest` (one process per test) cut the debt groups from 11,722 s to 1,687 s, **6.9x** (DOCUMENTED, `MIRI_EXCEPTIONS.md` "P7-W15"). The `rustty-vt` Miri lane takes about 8 h under libtest.

Gates find bugs in their own instruments too. Several were RED, or GREEN, for the wrong reason:

- N31: the Miri runner fabricated failures by classifying on an abort *string*.
- N497: a Miri exception claimed ASan coverage that no run executed.
- `c25d02ced`: an "ASan control existed only on paper".
- N513: G-SAN-SELF's HWASan arm could go GREEN 1 time in 256.

That is why every gate now carries self-test arms that must turn RED.

### c.3 Unsafe census over time

`data/bugs-unsafe-census.csv`:

| snapshot | unsafe sites | Rust LOC | density per kLOC | provenance |
|---|---|---|---|---|
| 2026-07-10 | 11,145 | 412k | 27.0 | DOCUMENTED |
| 2026-08-25 | 18,826 | 676k | 27.9 | DOCUMENTED |
| 2026-10-08 HEAD | 22,310 | 837k | 26.7 | MEASURED |

The census method is Bun's (`count-unsafe.sh` header).

`.port/audits/UNSOUND_SAFE.md` tracks the class a grep for `unsafe` cannot find: *safe* functions whose soundness rests on invariants. Its classes:

- A: safe functions that dereference raw-pointer arguments
- B: `&self -> &mut` accessors such as `stream_terminal.rs` `fn term(&self) -> &mut Terminal`
- C: lifetime-less pointer-holding pub structs

Each entry is triaged VT-STREAM, INTERNAL or THEORETICAL.

---

## Proposed visualizations

| # | title | chart type | data file | shows / why it is interesting |
|---|---|---|---|---|
| V1 | Ghostty defects recorded over time | step line, cumulative total and cumulative live; annotated at 2026-09-11 (R13) and 2026-09-23 (R13 extended) | `data/bugs-upstream-cumulative.csv` | Shows the policy turn from "port bugs faithfully" to "fix and record", and the retroactive sweep: 66 on day 1. Note on the chart that the first-day spike is a recording date |
| V2 | Where Ghostty's defects live | heatmap, area x class (10 x 7) with row and column totals | `data/bugs-upstream-area-x-class.csv` (finer: `bugs-upstream-subsystem-x-class.csv`) | Crashes cluster in VT core (tmux, C API) and the app runtimes; latent and compile-only defects in libxev and libvaxis |
| V3 | How porting finds bugs | horizontal stacked bar, method x class (ESTIMATED) | `data/bugs-upstream-method-x-class.csv` | The point for a technical reader: Rust's checked conversions (29 crashes), compiling code Zig never analysed (16), and cross-platform bring-up (16) each find a distinct kind of bug. Label it heuristic |
| V4 | What a safety-checked trap becomes in ReleaseFast | bar or waffle over the 60 crash-class entries | `data/bugs-upstream-crash-releasefast.csv` | 38 of 60 are UB in the shipped build mode. Explains why "it panics in Debug" understates the impact (ESTIMATED) |
| V5 | Whose code | treemap or bar | `data/bugs-upstream-project.csv` | 172 in Ghostty, 33 in pinned dependencies (libvaxis 17, libxev 8, ...) |
| V6 | Upstream status | 100% bar | `data/bugs-upstream-status.csv` | 0 reported yet; 4 since fixed upstream independently, 5 partly, which cross-validates the findings |
| V7 | Port-introduced memory-safety bugs on the timeline | dot or strip plot: x = date, y = tool (Miri SB / Miri TB / harness / sanitizer / symptom), dot size = count, colour = production vs test | `data/bugs-port-memory-safety-timeline.csv` | The aliasing-model class runs end to end, from June's `clear_cells` to October's Tree Borrows lane; adding TB in October surfaced a new, test-only shape |
| V8 | The verification matrix | table, or bar of tests per gate with wall time as a second series | `data/bugs-verification-gates.csv` | Scale of the evidence: about 10k tests per sanitizer, 13,354 s of Miri over 38 crates, 3 h 44 min of Valgrind |
| V9 | Unsafe census vs code size | dual line, sites and LOC, with density as a flat line | `data/bugs-unsafe-census.csv` | Unsafe sites grew with the code (11k to 22k) while density stayed at about 27 per kLOC; comparable to Bun's census method |
| V10 | Reachability of Ghostty's defects | stacked bar, source x class (ESTIMATED) | `data/bugs-upstream-reachability-x-class.csv` | Separates "any program's output can trigger it" (VT stream) from C API, config and dead code |
| V11 | Port-findings ledger by week | stacked bar, week x class (ESTIMATED) | `data/bugs-port-findings-by-week-class.csv` | Activity during the parity closure and the Phase 7 soundness work |
| V12 (optional) | Fix-commit share per week | bar of commits with a line for the fix share | `data/bugs-port-fix-share-by-week.csv` | Context only; noisy |

Also produced, for drill-down tables in the article:

- `data/bugs-upstream-defects.csv`: one row per UD, with columns:
  - class, severity, project, subsystem, area
  - heuristic method
  - status, private flag, in-user-docs flag
  - first commit and date
- `data/bugs-port-findings.csv`
- `data/bugs-port-fix-commits.csv`
- `data/bugs-upstream-reachability.csv`

## Gaps

See the structured summary returned to the orchestrator. In brief:

- Discovery method and reachability are heuristic, because the ledger has no structured field for either.
- Ledger dates are recording dates; the 2026-09-11 batch was retroactive.
- Upstream bugs found before R13 were tracked as `U` ids in phase records and were not fully reconciled here.
- The curated port-bug timeline is hand-built. Pre-September port bugs exist only as commit messages and squash merges (e.g. `47b2dbbb7` bundles a whole phase), so it is a floor, not a census.
- No fuzz-found defects were identified in either ledger.
- Upstream reporting has not started.

## Fact-check

Independent re-derivation, 2026-10-08, at `1d22ba80d`. Own parsers in the session scratchpad (not `parse_ud.py`/`parse_n.py`), `git log -p` for dates, and `rg`/`git ls-files` for counts.

| # | Fact | Verdict | Evidence / correction |
|---|---|---|---|
| 1 | UD ledger: 205 entries, 135 live (60 crash, 19 UB, 14 leak, 9 deadlock/hang, 33 intent-breaking), 59 latent, 11 test-weakness | **confirmed** (with a classification note) | A separate split on `### UD<n> — ` after `## Entries` gives 205 entries. Bucketing each `kind:` line by its first word gives crash 59, UB 19 (ub 12 + UB 7), leak 15, deadlock/hang 9, intent-breaking 33 (functional 17 + non-crash 16), latent 59, test-weakness 11. The difference of 1 is UD115, whose kind is `leak / crash (safety-checked trap)`, which the dossier puts under crash. Both choices are valid, and the live total of 135 does not change. If the article shows 60/14, say that mixed-kind entries are counted under their crash half. |
| 2 | 172 Ghostty, 33 deps (libvaxis 17, libxev 8, zig 3, highway 2, GStreamer 1, GTK 1, zf 1) | **confirmed** | Direct parse of `upstream project:` gives the same counts. |
| 3 | 141 UD ids in 138 rows of docs/ghostty-bug-fixes.md | **confirmed** | `grep -oE '\bUD[0-9]+\b' \| sort -u` = 141. 138 of the 140 table lines name a UD; the other 2 are the header and separator. |
| 4 | 0 reported; fixed 4 independently, 5 partly; 1 obsolete; 2 private | **corrected (wording)** | The counts are right: 4 fixed, 5 partly, 1 obsolete, 2 private-disclosure, 193 not reported. But one of the 4 "fixed" is UD55, in google-highway, fixed in **Highway 1.4.0**, while Ghostty still pins 1.2.0. Write "upstream had already fixed 4 (3 in Ghostty, 1 in Highway, not yet picked up by Ghostty's pin) and partly fixed 5 in Ghostty". |
| 5 | R13 dates from 2026-09-11, extended 2026-09-23; 66 entries recorded on 2026-09-11 | **confirmed** | `PORT_RULES.md` "R13 ... user ruling 2026-09-11" and ":377 (user ruling 2026-09-23)". Commits `7dd991582` (2026-09-11, R13 rule) and `f94ab8e81` (2026-09-23, the non-crash extension; better evidence than the dossier's list). Using `git log -p` on the first commit that adds each `### UD` heading gives 66 on 2026-09-11, then 38/29/19 on 09-22/23/24. Note that `49edf86a4` is dated 2026-07-31: it records 2 bugs that were ported faithfully before the policy. |
| 6 | ReleaseFast: 38 of 60 crash-class are UB; 13 wrong value; 9 still crash | **confirmed (ESTIMATED)** | A separate regex classification of the 60 `ReleaseFast:` fields also gives exactly 38 UB. Hand-reading the other 22: about 13 silent wrong values (UD9, 58, 59, 63, 71, 76, 90, 95-100) and about 9 that still terminate (UD3, 10, 56, 61, 70, 170, 290, 325, plus UD115, which is a leak whose assert is compiled out). The 13/9 split may be off by about 1 depending on how UD115 and UD9 are read, so keep it labelled ESTIMATED. |
| 7 | 29 crash-class found by checked conversions; "another 16" by compiling unanalysed code and 16 by Windows/Linux bring-up | **corrected (framing)** | The 29 holds: it is the `checked arithmetic/casts` row and is crash-only. The two 16s are **totals across all classes**, not crash-class. The compile path is 14 latent + 2 intent-breaking with **0 crashes**. Bring-up is 5 crash + 2 deadlock + 5 intent + 4 latent. The bring-up heuristic also matches only the word `windows`, so "Windows and Linux" overstates Linux. Suggested text: "29 crash-class entries come from range-checking Zig casts and overflow sites. Separately, 16 entries (mostly latent) came from compiling code Zig never analyses, and 16 (5 of them crashes) from the Windows bring-up." All three numbers are keyword heuristics. |
| 8 | Sanitizer-found Ghostty-side defects: UD376 (LSan), UD377 (TSan); Miri only confirmed read-found races | **corrected** | There is a **third**: **UD350**, the font collection leak when `SharedGrid.init` fails. It was found by G-VALGRIND's first memcheck run on rustty-font-shaper (NEW-FINDINGS N404, commit `7d8a101a8`, 2026-09-27). The dossier's own `bugs-upstream-defects.csv` tags it `sanitizer gate`. Also, UD377 is in google-highway, not Ghostty. The Miri claim holds: UD352 by reading; UD363 and UD368 were found in review and proven with `mirihar-n437`/`mirihar-n444` SB/TB models. |
| 9 | 334 N entries (N1..N530); 20 'port soundness' | **corrected** | There are **335** `## N<n>` headings (unique, N1..N530). The dossier's parser drops **N35**, whose heading has no title (`## N35`, line 1964; the search_thread load-dependent flake). Corrected value: 335. The 20 `**kind**: ... port soundness` count is confirmed (grep finds 20). |
| 10 | 38 curated events, 81 defects, 62 Miri, 29 production | **confirmed (arithmetic)** | The CSV re-sums to 38 rows, 81 total in `count`, 62 where the tool contains "Miri", and 29 whose scope starts with `production` (30 if "test harness + latent production" counts). All 24 commit refs resolve. The 14 N-ref rows exist. Curation itself cannot be checked independently (hand-built floor). |
| 11 | First vt Miri sweep: 2,037 tests, 1,948 clean, 9 real SB UB in search, fixed same day | **confirmed** | VT_MIRI_SWEEP.md: "all 2037 `rustty-vt` lib tests", "Miri-clean \| 1948". `3277f2f78` 2026-07-11 01:14 and fix `9e3fe1c25` 2026-07-11 08:20 (9 tests). The remaining 80 are other buckets, for example 8 parallelism false-FAILs. |
| 12 | G6 38 crates 13,354 s (vt 3,104 tests); G6c 124/124 1,110 s; G6t green after 8 test-side UB (N500, N505) | **confirmed** | README.md:92-93 and :130-132 at `04f0a894c` (2026-10-06). N500 = 1 test (`snapshot::codec`); N505 = "the same shape in seven `rustty-vt-c` and `rustty-core` tests" → 8. |
| 13 | nextest speed-up 6.9x (11,722 s → 1,687 s, V27-V29) | **confirmed** | MIRI_EXCEPTIONS.md table "total \| 11,722 s \| 1,687 s \| 6.9x" (M4 Pro, 8 processes). Per group it varies from 2.1x to 9.9x, so cite it as the aggregate of three groups. |
| 14 | ASan 9,850; LSan 10,111 Linux / 9,850 macOS; MSan 6,183; TSan 4,380; Valgrind 6,614 tests / 914 processes; 0 Valgrind errors in Rustty code | **confirmed** | README.md:72 (ASan 9,850; MSan 6,183; LSan 10,111 on `05478107`; the first run was 10,087), :78 (LSan macOS 9,850), :65 (TSan 4,380), :108-116 (Valgrind). Caveat: the full Valgrind run was **RED on rustty-renderer** (N509 Mesa EGL leak, suppressed; N510 a test fix). It went GREEN after a renderer-only rerun, not a fresh full 914-process run. "No ... leak in Rustty code anywhere" is the README's own wording. |
| 15 | unsafe: 22,310 in 836,905 lines (26.66/kLOC), from 11,145 in 412k (27.0) on 2026-07-10 | **confirmed** | The committed `rg` pattern from count-unsafe.sh, run at `1d22ba80d`, gives 22,310. `git ls-files` for crates/ crates-sys/ xtask/ `*.rs` minus vendor/cbits, through `cat \| wc -l`, gives 836,905. 22,310/836.905 = 26.66. Snapshot `unsafe-census-b4845f4.json`: totals.sites 11,145, loc 412,464, density 27.02; `b4845f427` is dated 2026-07-10. |
| 16 | TriggerC 20 B vs 12 B header, SIGBUS on first call, gate A4 | **confirmed** | `40727a2cb` (2026-07-15) message: "size_of::<TriggerC>() == 20 ... SIGBUS'd on the first call", new `check_embed_h_abi.sh` "(CI id A4)". It also left every dynamic macOS menu shortcut blank and swept 8 fns / 6 types. |
| 17 | 27 crashes in 1,500 parallel runs (N102); first seen as heap corruption in 6cf99b4cd | **corrected (framing)** | "27 of 1,500 runs (SIGTRAP 20, SIGABRT 6, SIGSEGV 1)" is confirmed in N102, which was fixed in `249d174ab` on **2026-09-23** at the tmux control-mode parser. `6cf99b4cd` (2026-07-01) is an **earlier, separate instance of the same class**: the renderer `render_cell_map*` tests, about 1 crash in 25 runs, from a faithfully ported `ensureInit` that never set its flag. It is not N102's first sighting. Write: "the class first appeared on 2026-07-01 (`6cf99b4cd`) and recurred as N102 in the tmux parser (27/1,500, fixed 2026-09-23)". |
| 18 | 27 Miri proof-harness crates | **corrected** | 27 counts only `.port/phase-2/mirihar-*`. There are **33** tracked `mirihar-*` crates (`git ls-files '.port/*mirihar*/Cargo.toml'`): 27 in phase-2, 3 in phase-3 (fingerprint-update, hashmap-shift, scroll-region-up), 2 in phase-4 (reflow-recycle, termio-pool) and 1 in phase-5 (scroll-margins). Worktree copies under `.claude/worktrees/` are excluded. |

Summary: 12 confirmed, 6 corrected, 0 unverifiable. Two corrections are count errors: N entries are 335, not 334, and harness crates are 33, not 27. One adds a missed item: UD350 was found by Valgrind. Three fix framing: the Highway fix attribution, the scope of the two 16s in the method counts, and N102 versus 6cf99b4cd. Item #1 is confirmed, with a note on whether mixed leak/crash entries count as crash (60) or leak (59/15).
