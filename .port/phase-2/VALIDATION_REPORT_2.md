# rustty Validation Report #2 — Full-Port Audit, Sweep #2 (2026-07-12)

Scope: full-fidelity Zig→Rust port of Ghostty (rules in `.port/PORT_RULES.md`; parity contract in
`.port/phase-0/plan/DEFINITION_OF_PARITY.md`; oracle `/Users/gael/projects/ghostty` @ `4749c4e9`).
This is the second adversarial sweep, targeting the blind spots that Report #1 self-declared as
unexamined (§4 "What NO auditor examined") plus the two cfg-gated / provenance axes it could not
reach. Seven specialist auditors: the VT front-half print path, the selection+dump subsystem, the
bulk-unsafe bodies (terminal/screen/formatter), the rustty-inspector widget bodies + dcimgui FFI,
the unicode generated-tables provenance, and the cfg-gated platform-code type-check axis (Windows /
wasm / -sys build-script wall). All audits were **static** (read-only per constraints — a sibling
workflow holds the build locks): grep / read / Python-count only. **No cargo / Miri / ASan
execution informed any conclusion below.** Every finding cites exact `file:line`; every comparison
claim quotes both sides. Nothing in Report #1 §5 (refuted appendix) is re-raised.

---

## 1. Verdict

**Sweep #2 confirms and sharpens Report #1's headline: the ported code is faithful line-for-line;
the residual risks are (a) latent compile breaks in cfg-gated-off platform arms that the locked
macOS-first target never type-checks, and (b) a regression-detection hole in the one class of
artifact the port takes on faith (the unicode tables).** Two large previously-unwalked subsystems —
the VT print front-half (`Terminal.print`/`printCell`/`printWrap`/`printString`/`printRepeat`) and
the selection+dump subsystem — were walked line-by-line on both sides and found to have **zero
wrong-behavior drift**, extending Report #1's clean result for the VT back half. The bulk-unsafe
bodies of `terminal.rs`/`screen.rs`/`formatter.rs` were swept for the exact hazard class that
produced the real search-subsystem SB UB (a raw alias into inline `Screen` memory crossed by a
`&mut` retag) and found **systematically insulated** — `Screen` is heap-`Box`'d behind `*mut
Screen`, so `&mut Terminal`/`&mut Screen` retags never propagate into it, and every reallocation-
crossing raw is either a tracked pin or re-derived after a `continue`. No new live UB was found.

What did surface: two **latent, cfg-gated-off compile breaks** (a Windows `pipe()` arm missing its
`unsafe` block; a wasm `wasm.rs` referencing a nonexistent build-config item) that are invisible
today only because the locked target never compiles those arms; a **materially understated
dcimgui-sys FFI surface** (the internal DockBuilder API + `ImGuiDockNodeFlagsPrivate` + the `ext`
namespace are compiled into the archive but unreachable from Rust, which will block the Phase-3
inspector fill); and a **regression-detection hole** in the unicode tables — the exhaustive
differential gate silently downgrades to a WARNING when `zig` is absent, so on a zig-less runner
nothing under `cargo test` guards ~42k committed table entries beyond ~31 sampled codepoints. None
of these is live UB on the shipping macOS target; all are fidelity/latent-compile/coverage debts
that should be logged and (for the checksum test) cheaply closed.

The single most actionable item: **add the pure-Rust FNV-1a checksum test for the unicode arrays**
(Finding M3) — it is R1-clean, R3-clean, zero-value-change, and moves ~42k table entries under the
always-run `cargo test` umbrella independent of `zig`.

---

## 2. Confirmed findings (adversarially verified, ranked)

Severities are post-verification (corrected) severities. Rules: R1 = full fidelity
(lines/comments/tests), R3 = self-containment, R4 = acyclic layering, R5 = never weaken gates,
R6 = memory-safety proof (Miri/ASan).

### MEDIUM (confirmed)

**C1. `pipe.rs` Windows arm calls extern FFI (`CreatePipe`/`GetLastError`) with NO `unsafe` block —
will not compile on any Windows target (E0133).**
`crates/rustty-os/src/pipe.rs:72-83` — violates **R1** (faithful-compile fidelity; the arm is
`ported`, not deferred).
- `pipe() -> io::Result<[fd_t;2]>` (pipe.rs:23) is a **safe** fn. Its `#[cfg(windows)]` arm calls
  two `extern "system"` functions in bare expression position with no `unsafe { }`:
  `crate::windows::exp::kernel32::CreatePipe(...)` (pipe.rs:76, extern at windows.rs:207-208) and
  `crate::windows::kernel32::GetLastError()` (pipe.rs:79, extern at windows.rs:90-91). Calling an
  extern fn is an unsafe operation → E0133, will not compile on any Windows target.
  `unexpectedError` (windows.rs:120) is a **safe** fn, confirming those two are the only unsafe
  calls in the arm.
- Contrast (same file, correct): the POSIX arm wraps every libc call in `unsafe` —
  pipe.rs:41 `let rc = unsafe { libc::pipe2(...) };`, likewise :56/:61/:65. The same two symbols
  are used **correctly** inside an `unsafe {` block at pty.rs:562/:614.
- Failure scenario: the moment anyone runs `cargo check --target x86_64-pc-windows-{gnu,msvc}`, the
  TU fails; the Windows `pipe()` path is dead until wrapped. Latent only because macOS-first is
  locked and the arm is `cfg(windows)`, never type-checked. Present impact on the shipping target:
  zero — hence medium, not high.
- Fix: wrap the arm's FFI in `unsafe` exactly as pty.rs does:
  `if unsafe { crate::windows::exp::kernel32::CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 { return crate::windows::unexpectedError(unsafe { crate::windows::kernel32::GetLastError() }); }`.

**C2. dcimgui-sys FFI surface omits the internal DockBuilder API, `ImGuiDockNodeFlagsPrivate`, and
the `ext` namespace that `main.zig` declares and every Phase-3 inspector widget calls — symbols are
compiled into the archive but unreachable from Rust.**
`crates-sys/dcimgui-sys/wrapper.h` (+ `lib.rs:11-12`, `build.rs:117-125`) — violates **R1** (the
whole of `main.zig` lines 34-73 + its `test { _ = c; }` are unrepresented) and understates the
deferral scope.
- The port reproduces only the `@cImport("dcimgui.h")` portion of `pkg/dcimgui/main.zig`
  (wrapper.h's entire content is `#include <dcimgui.h>`). But main.zig also manually declares
  (because they live in `dcimgui_internal.h`, whose bitfields `cImport` can't translate): the 17
  `ImGui_DockBuilder*` functions and the 13 `ImGuiDockNodeFlagsPrivate` constants (main.zig:37-52),
  plus the `ext` namespace `ImFontConfig_ImFontConfig` / `ImGuiStyle_ImGuiStyle` (main.zig:70-73,
  from ext.cpp).
- The symbols **exist in the archive** — build.rs compiles `dcimgui_internal.cpp` (build.rs:118)
  and `ext.cpp` (build.rs:125) into `libdcimgui.a` — but no Rust declaration can call them.
  Absence proof: `grep -n DockBuilder vendor/dcimgui/dcimgui.h` → only a FIXME comment (line 1616);
  `grep -c DockBuilder vendor/dcimgui/dcimgui_internal.h` → 22;
  `grep -c 'ImFontConfig_ImFontConfig\|ImGuiStyle_ImGuiStyle' vendor/dcimgui/dcimgui.h` → 0.
- Failure scenario: the Phase-3 inspector fill cannot compile — Inspector.zig:45 calls
  `cimgui.ext.ImFontConfig_ImFontConfig`; surface.zig:162-164 build docked layouts via
  `ImGui_DockBuilderAddNodeEx(... ImGuiDockNodeFlagsPrivate.DockSpace ...)`; none are in the
  generated bindings. The lib.rs/build.rs docstrings frame the omission narrowly as "platform
  rendering backends deferred", which materially understates it: DockBuilder + DockNodeFlagsPrivate
  + ext are neither platform backends nor deferred-from-compilation.
- Fix: extend wrapper.h to also `#include <dcimgui_internal.h>` (add its dir to bindgen clang args)
  so DockBuilder + `ImGuiDockNodeFlagsPrivate_` are emitted, and declare the two `ext` entry points
  (a small `extern "C"` header for ext.cpp), then hand-mirror the `ImGuiDockNodeFlagsPrivate`
  constants + the `ext` module in lib.rs matching main.zig:34-73. **Alternatively**, if intentionally
  deferred to Phase-3, log it explicitly in DEFERRALS.md as an OPEN dcimgui-sys item (it is not
  tracked today) and correct the docstrings to say the internal docking API + ext namespace are
  deferred, not merely "platform rendering backends".

**C3. Embedded unicode tables have NO always-run guard: the only in-crate test checks ~31
codepoints, and the exhaustive external gate silently downgrades to a WARNING when `zig` is absent —
a corrupted interior `u16` among ~42k table entries escapes `cargo test`.**
`crates/rustty-unicode/src/generated/props_data.rs` (+ symbols_data.rs, grapheme_data.rs;
`.port/tools/ci.sh:160-162`; `run_unicode_diff.sh:47-51`) — regression-detection hole against **R1**
(the tables are the one comptime-artifact class taken on faith, Report #1 §3g).
- Two verification layers exist, both with a hole for regression detection: (1) the in-crate tests
  `props_uucode.rs::unicode_props_tables_match_uucode` (~31 hand-curated codepoints) +
  `symbols_uucode.rs`, plus a generator `main()` that round-trips `get()->generate()` and is a
  **tautology** (`get()` reads the very embedded table it "verifies", props_uucode.rs:30). (2) The
  full-range differential G4-uni (`run_unicode_diff.sh`) genuinely diffs vs live uucode — but it
  **requires zig 0.15.2**, and when absent returns exit 3, which ci.sh classifies as
  BLOCKED→WARNING (ci.sh:160-162), never a failure.
- Net: on any machine/CI without zig, nothing run by `cargo test` guards the ~8192+31744+30 props +
  8192+1792+2 symbols + 8192 grapheme = ~42k committed entries beyond the ~31 sampled points.
  Failure scenario: a stray edit or a bad regeneration flips e.g. `props_data.rs STAGE2[17000]` (a
  CJK block index) or `grapheme_data.rs DATA[2888]`; every in-crate test still passes; ci.sh reports
  GREEN-with-warning on a zig-less runner; the defect surfaces only as a specific codepoint
  misrendering at runtime. Contrast: ghostty's equivalent `test "unicode props: tables match uucode"`
  (props_uucode.zig:66) iterating `min..maxInt(u21)` compiles into `zig build test`
  unconditionally.
- Fix (the audit brief's checksum test): add `crates/rustty-unicode/src/generated/checksum.rs` with
  a const-fn FNV-1a-64 and a `#[test]` asserting fixed golden digests over each array + const-assert
  the shapes. This is R3-clean (pure Rust, no uucode/ghostty dep) and R1-clean (adds a lock, changes
  no value). Ready-to-paste goldens computed over the CURRENT committed arrays:
  `PROPS_STAGE1=0xcb3ce2d26c5bc0a7`, `PROPS_STAGE2=0xb9c699472c3856bf`,
  `PROPS_STAGE3=0x6d849efcaf8fa676`, `SYM_STAGE1=0xdc8141a919b148df`,
  `SYM_STAGE2=0x786f80950ce6380d`, `SYM_STAGE3=0x08328707b4eb6e3a`,
  `GRAPHEME_DATA=0x29e1968ee59165fd`. Encoding: STAGE1/STAGE2 as u16-LE; props STAGE3 as 4
  bytes/entry `[width, wzg as 0/1, grapheme_break as u8, evb as 0/1]`; symbols STAGE3 as 0/1 bytes;
  grapheme DATA raw bytes. Shapes: STAGE1 8192; props STAGE2 31744 / STAGE3 30; sym STAGE2 1792 /
  STAGE3 2; DATA 8192. Anchor the digests to the one-time external validation (M4) so they are not
  self-referential.

### LOW (confirmed)

**C4. `wasm.rs` references nonexistent path `rustty_build_config::wasm_shared` — will not compile on
wasm32 (E0425/E0433).**
`crates/rustty-os/src/wasm.rs:32` — violates **R1** (the file is `PORT STATUS: ported`, wasm.rs:3,
not deferred).
- Inside `#[cfg(target_arch = "wasm32")] mod wasm_impl`, line 32 does
  `pub const shared_mem: bool = rustty_build_config::wasm_shared;`. There is no crate-root value
  item `wasm_shared`: it exists ONLY as a field of the `Config` struct (build.rs:46
  `pub wasm_shared: bool`, defaults build.rs:71/119), never re-exported at the crate root
  (lib.rs:45-48 re-exports only Config/ExeEntrypoint/ReleaseChannel/Runtime/WasmTarget/Arch/Os/Target,
  no glob). The crate's scalar build flags are exposed as `pub const fn` accessors (lib.rs:204
  `flatpak()`, :209 `snap()`, :101 `release_channel()`), so the parity-correct spelling is
  `wasm_shared()` — which also does not exist. Oracle `os/wasm.zig` does
  `pub const shared_mem = options.wasm_shared;` (a `build_options` field read), which the port must
  realize as an accessor call.
- Compounding fact: rustty-os is not cleanly buildable for wasm32 anyway — libc is
  `[target.'cfg(target_family = "unix")'.dependencies]` (Cargo.toml:48), absent on wasm32, yet
  PosixPty & friends are gated `#[cfg(not(any(windows, target_os="ios")))]` (pty.rs:63/167/188) and
  reference `libc::c_int` (pty.rs:121,189) — so the wasm surface isn't isolable by a naive
  `cargo check --target wasm32-unknown-unknown -p rustty-os`; the wasm modules would need to be
  isolated first.
- Failure scenario: any wasm32 build fails E0425/E0433. Dead code on the locked macOS target with
  zero current build/runtime impact and unreachable even by a naive wasm32 check → low.
- Fix: add `pub const fn wasm_shared() -> bool { ... }` to rustty-build-config/src/lib.rs (mirroring
  flatpak()/snap()) and change wasm.rs:32 to `... = rustty_build_config::wasm_shared();`. Separately,
  to make the wasm gate type-checkable at all, isolate the wasm modules from the PosixPty (libc)
  tree, or mark wasm out of scope.

---

## 3. Medium/low ledger (unverified as reported by auditors; grouped, compact)

### 3a. Cross-toolchain build-matrix gap (feeds C1/C4)

**M1 (medium). Every vendored `-sys` crate needs a TARGET C/C++ toolchain (+ libclang for bindgen) —
a naive cross `cargo check` of the workspace fails at build-script time before any gated Rust is
type-checked.** `crates-sys/freetype-sys/build.rs` et al. 13 of 14 `-sys` crates invoke `cc::Build`;
11 of those also run `bindgen` (needs libclang + target sysroot). `cargo check` runs build scripts,
so cross-checking anything transitively depending on a `-sys` crate (rustty-font, rustty-renderer,
rustty-crash, rustty-input, rustty-os) fails unless a cross C/C++ compiler + libc++/libstdc++ +
`BINDGEN_EXTRA_CLANG_ARGS --target` are configured. This is precisely why the cfg-gated Rust (C1,
C4) sits un-type-checked: the standard `cargo check` matrix can't reach it through the `-sys` wall.
Special cases: `fontconfig-sys` is a STUB (empty build.rs, cross-checks trivially, links zero real
fontconfig symbols); `gettext-sys` is `#[cfg(target_os="macos")]`-gated (no-op elsewhere);
`sentry-sys` selects per-target unwinder/symbolizer C files.
**CARGO-CHECK MATRIX RECOMMENDATION:** (1) add a cheap tier that cross-checks only the `-sys`-free,
cc-free platform crates — `cargo check -p rustty-xev -p rustty-vaxis -p rustty-termio --target
x86_64-pc-windows-gnu` and `... --target x86_64-unknown-linux-gnu` (run linux on a linux runner);
add `-p rustty-os` to the WINDOWS target only (its build.rs self-skips pty.c on windows and libc is
unix-gated, so it needs no C toolchain there) — this single check would have caught both C1 and C4's
class and specifically pipe.rs. (2) Keep the full-workspace cross-build (font/renderer/crash/input)
on a heavier, opt-in tier that provisions mingw/musl + libclang. (3) wasm: isolate the os/wasm
modules into a wasm-buildable unit before any wasm check can bind (see C4).

### 3b. Unicode provenance (companion to C3)

**M2 → renumbered M4 (medium). The "bit-for-bit from the reference generator" provenance claim is
only TRANSITIVELY established** (semantic per-cp equality vs live uucode + matching LUT construction
code); it is never directly confirmed by diffing ghostty's actual props-unigen/symbols-unigen array
output. `crates/rustty-unicode/src/generated/symbols_data.rs:3`. The existing G4-uni harness proves
per-codepoint VALUE equality (diff_unicode.py compares width/is_symbol/flags/grapheme_break per cp +
grapheme.bin); bit-for-bit RAW-ARRAY identity then follows only because lut.rs reproduces lut.zig's
deterministic construction (verified by inspection: linear first-seen stage3 dedup lut.rs:81-97 ==
lut.zig; block-keyed insertion-ordered stage2 map lut.rs:114-131 == lut.zig getOrPut; identical
tail memset-to-0; HashMap iteration order irrelevant to output). Ghostty does NOT commit its
generated arrays (`props_table.zig:11 @import("unicode_tables")` is build-produced), which is why
the direct diff was skipped. Fix (one-time, build-enabled on an authorized machine, still read-only
on ghostty): run ghostty's `props-unigen`/`symbols-unigen` (installed via
`zig build -Demit-unicode-table-gen`, SharedDeps.zig:46) to capture the emitted Zig arrays; parse
the three `stageN` arrays from each and element-wise diff against props_data.rs/symbols_data.rs;
also run ghostty's own tables-match-uucode tests to confirm currency; record the result + the M3
FNV-1a goldens in the harness README as the permanent anchor so the cheap in-crate checksum test
stands in for the expensive one-time diff. Optionally add an opt-in `--ghostty-tree <path>` raw-array
mode to run_unicode_diff.sh.

*(Note: the confirmed C3 checksum test is referenced by the auditor as "Finding 1" / M3; M4 above is
its provenance anchor. Both come from the same unicode-provenance auditor.)*

### 3c. Soundness / fragility watch items (static; none demonstrated as live UB)

**M5 (low, informational). `prev_cell` raw pointer held across `print_wrap()`/`move_grapheme()` in
the mode-2027 grapheme wide-at-line-edge path.** `crates/rustty-vt/src/terminal.rs:862`. The raw
`prev_cell` is written (set_wide/set_content_codepoint, 868-874) then reused after `self.print_wrap()`
(which calls `index()`, possibly scrolling/growing pages) at `move_grapheme(prev_cell,..)`/
`set_content_tag` (896-901). Behaviorally sound because Ghostty pages are stable linked-list nodes
(index/grow allocate NEW pages, never move existing ones) and the code re-derives via `up(1)`/
`new_rac`; Rust mirrors Zig `*Cell` exactly (Terminal.zig:456/464/476-477). Flagged only as a
Miri/SB watch item. No behavior change needed; if TB/SB flags the alias under Miri, re-derive from
`old_pin.row_and_cell().cell` after print_wrap.

**M6 (low). `active_mut()` materializes `&mut Screen` from raw `self.screens.active` — the exact
shape that broke the search subsystem; sound today ONLY because all 110 call sites keep the
`&mut Screen` transient (convention, not typed).** `crates/rustty-vt/src/terminal.rs:4885`
`unsafe { &mut *self.screens.active }` retags the whole heap Screen as Unique. Clean HERE because
(1) all 110 `self.active_mut()` uses are transient single-call expressions (the `&mut Screen` dies
at end-of-statement), and (2) cursor/cell/row/pin raws point into HEAP pool memory (separate
allocation) and Screen is `Box`'d behind `*mut Screen`, so no `&mut Terminal`/`&mut Screen` retag
pops them. Latent failure: a future edit that let-binds `active_mut()` across an inline-Screen raw
alias reintroduces search-class UB with nothing in the type system to catch it. Ledger action: add
`active_mut()` to GHOSTTY_LIFETIMES.tsv / UNSOUND_SAFE Class B with the invariant "the returned
`&mut Screen` must never be bound to a local held across a second self reborrow or an inline-Screen
raw alias." Faithful to oracle (ScreenSet.zig:28 `active: *Screen`; Terminal.zig reaches it inline).

**M7 (low). `insertLines`/`deleteLines` hold UNtracked heap-Page raws
(`off_p`/`off_row`/`cur_row`/`off_page`) across `active_mut().increase_capacity()` in the cross-page
reallocating branch — SB/UAF-safe only via the `continue`; this branch is in the Miri-timeout
exception set (multi-page), so it is NOT Miri-proven.** `crates/rustty-vt/src/terminal.rs:2924-3040`
(mirrored deleteLines 3218-3320). Real-UAF safety rests on: (a) only `cur_p` is a TRACKED pin
(track_pin at :2894, survives realloc — comment :2887 "so that if we need to adjust capacity it will
be properly tracked"), and (b) the code does `continue` (:3047) and re-derives cur_row/off_p/off_row
FRESH next iteration — no stale heap-Page raw is re-read after `increase_capacity` (:3025). Faithful
to oracle (Terminal.zig:1887 same comment; only cur_p trackPin'd there too, off_p is a plain local).
Latent failure: a future edit removing the `continue` or reusing off_p/off_row after increase_capacity
would dangle. Coverage action: add a Miri-tractable small-page-size cross-page-with-increaseCapacity
insertLines/deleteLines test (force IncreaseCapacity via a tiny styles/grapheme budget on a 2-page
list, staying under the Miri timeout) so this branch gets a distinct Miri-passing witness instead of
relying on the timed-out large-scrollback tests; add a LIFETIMES.tsv note that off_p/off_row must
stay confined to a single loop iteration.

**M8 (attestation, record so it is not re-questioned). The three bulk-unsafe target files
(terminal.rs / screen.rs / formatter.rs) contain NO instance of the search-subsystem hazard class**
— a raw alias into INLINE Screen memory crossed by a `&mut Screen`/`&mut Terminal` retag — because
`Screen` is heap-`Box`'d behind `*mut Screen` and all cell/row/pin raws are heap-pool. A grep for
the inline-alias shape (raw from `&self.pages`/`addr_of_mut!(self.pages)`/`&self.cursor` held across
a self reborrow) returned ZERO matches. Every raw is either (a) a copy of the raw field value
`self.screens.active` (Box'd Screen — `&mut Terminal` never retags through it), or (b) a pointer
into heap pool Page/Pin memory. Raws held across an actual reallocation are TRACKED pins
(cursor.page_pin, cur_p, saved_cursor_pin screen.rs:2412) or re-derived after via `continue`/explicit
index reload (append_grapheme reloads by INDEX not pointer, screen.rs:2996-3010, comment "The cell
pointer is now invalid"). The port authors consistently annotate the hazard (screen.rs:2205 "stash
the raw cursor pin/row pointers ... so clear_cells doesn't reborrow self"; screen.rs:1372
"referencing this pin after cursorChangePin is unsafe"). Record in UNSOUND_SAFE "Verified-clean in
this pass" so a future sweep does not re-open these bodies for this hazard class; the residual watch
items are M5/M6/M7. formatter.rs is read-only (`&*self.screen`/`&*terminal.screens.active`) — no
`&mut` materialization, nothing to add.

---

## 4. Coverage & blind spots

### Per-auditor coverage (honest self-reports, condensed)

- **VT print front-half** (`Terminal.print`/`printCell`/`printWrap`/`printString`/`printRepeat`):
  FULLY WALKED line-by-line, both sides. Every branch of print+print_inner (Terminal.zig:324-701 vs
  terminal.rs:687-1190) — status early-out, right_limit, the full grapheme-cluster subtree (left
  derivation, spacer_tail redirect, grapheme_break loop with DG4 unconditional side-effect,
  desired_wide Wide/Narrow/NoChange, wide-at-edge spacer_head/row_wrap with hasGrapheme transfer,
  zero-width width==0 attach incl. VS16/VS15, width==1, width==2 space check + edge spacer_head +
  printWrap + wide+spacer_tail, trailing pending_wrap-vs-cursorRight). printCell (charset
  single_shift/gl, wide-mismatch clearing incl. cursorCellEndOfPrev stale-head cleanup, style
  release/use ref-counting, hyperlink, kitty placeholder). printWrap (mark_wrap, semantic prompt
  save/restore/continuation, index+cursorHorizontalAbsolute). Helpers verified faithful
  (cursorCellLeft/Right/EndOfPrev, Cell.codepoint/hasText, unicode props, u21=u32 alias page.rs:73
  lossless for valid codepoints). RESULT: zero correctness drift — this was Report #1 blind spot #2
  (VALIDATION_REPORT.md:404,415); this sweep closes it. NOT re-verified (out of scope):
  append_grapheme/move_grapheme/clear_grapheme internals, index() scroll/grow, insert_blanks body,
  assert_integrity body.
- **Selection + dump** (rustty-vt): FULLY EXAMINED line-by-line — Selection.zig (1604) vs
  selection.rs (1367) [order 4-way branch, topLeft/bottomRight x-mirroring, contains rectangle
  short-circuit, containedRowCached splits, adjust 10 arms, raw `*mut Pin` end_ptr]; Screen
  selectLine (semantic-prompt scan, reverse per-cell wrap scan, whitespace trim, the
  start_offset==0 underflow-preemption), selectAll/selectWordBetween/selectWord (the forward-vs-
  backward CHECK-ORDER ASYMMETRY), selectOutput, promptClickMove/promptClickLine both branches;
  dumpString family + selectionString (formatter Options, tripwire copy_map, map-dupe-BEFORE-check
  ordering); StringMap SearchIterator (oniguruma `#[cfg]`-gated off == Zig SkipZigTest);
  selection_codepoints tables (byte-identical); formatter Options defaults; ALL non-test selectLine
  callers checked for SelectLine-default preservation. Test-name parity via grep counts: selectLine
  13/13, selectWord 3/3, selectAll 1/1, selectOutput 1/1, promptClickMove 17/17, selectionString
  14/14. RESULT: zero confirmed drift — every off-by-one candidate (copy.x=x-1, ends()[0]-1,
  cells.len()-1, size.cols-1) faithfully preserved. NOT deep-read: PageList cellIterator/rowIterator/
  promptIterator internals (only call-sites verified).
- **Bulk-unsafe bodies** (terminal.rs / screen.rs / formatter.rs): FULLY EXAMINED all raw-pointer
  materialization sites reachable by the hazard grep (screen.rs `&mut *` 2 sites both comments;
  terminal.rs `&mut *` 4 sites 4113/4139/4885/4886; formatter.rs `&mut *` 2 sites 516/760 — but
  formatter is read-only); every `let <name>: *mut/*const` in terminal.rs (~80) + the
  cursor-pointer family in screen.rs (~60). Deep-read the reflow/multi-page slow paths NOT cleared
  by the recent Miri sweep (the ~70 multi-page timeout exceptions): insertLines/deleteLines,
  cursor_down_scroll + cursor_scroll_above(_rotate), resize/resize_body/resize_finish,
  split_cell_boundary, append_grapheme OOM-fallback, reset, clone. Verified the guard structs
  (ShiftLinesEndGuard/TrackedPinGuard Drop order vs Zig defer LIFO). CONFIRMED the structural
  insulation (ScreenSet.zig:28 == screen_set.rs:48; Screen Box::into_raw'd screen_set.rs:80).
  SAMPLED not exhaustively re-walked: the print/erase/SGR hot paths (already Miri-cleared). NET:
  zero CONFIRMED new UB; M5/M6/M7 are low-confidence fragility/coverage candidates, each assessed
  clean today with the insulation reasoning.
- **rustty-inspector widget bodies + dcimgui FFI**: FULLY EXAMINED lib.rs/widgets.rs/widgets/key.rs
  vs ghostty inspector main/widgets/key.zig; dcimgui-sys src/lib.rs/build.rs/wrapper.h/ext.cpp vs
  pkg/dcimgui; the one live FFI seam rustty-vt/src/input/key.rs:783-797 (imgui_key). KEY REALITY
  CHECK: the "ported Dear ImGui widget render sections" DO NOT EXIST as live code — Event.render/
  renderUtf8/renderPty, Stream, modsTooltip, actionColor/keyColor/hsvToRgba are VERBATIM COMMENTS
  (key.rs:214-683), deferred to Phase-3; no imgui_impl backend FFI exists; Inspector is an
  uninhabited enum. So the audit's panic-across-FFI / draw-data-lifetime / render-loop-aliasing
  concerns are N/A (no live FFI/render code). The 3 inspector-tier `unimplemented!()` seams
  (rustty-vt key.rs:796 imgui_key; rustty-renderer generic.rs:4566 overlayFeatures — statically
  unreachable None-only arm; rustty-apprt embedded.rs:1648 queueInspectorRender) are ALL tracked in
  DEFERRALS and none is a live panic path. LIVE ported code (Event init/deinit/label, EventRing,
  Event/ImguiState structs) audited faithful (label pos-1 arithmetic, write_all overflow guard,
  disjoint &self + &mut buf). The one finding is C2. NOT examined: deferred widget bodies beyond
  key.rs; vendored imgui/dcimgui C++ internals.
- **rustty-unicode generated-tables provenance**: fully examined the three generated data files
  (headers + array shapes), props/symbols/grapheme table + lut.rs construction, the in-crate
  spot-check tests, ghostty oracle unicode + build/UnicodeTables + uucode_config, the differential
  harness + its ci.sh wiring. Directly spot-checked the EMBEDDED tables by parsing the committed
  arrays in Python and evaluating the 3-stage LUT: 14 props + 13 symbols + 4 grapheme keys, ALL
  correct. Confirmed harness uucode_config.zig byte-identical to ghostty's (cmp). Computed the 7
  FNV-1a goldens (C3). NOT done (needs a build): running ghostty's unigen and byte-diffing raw arrays
  (M4); running G4-uni; exhaustive per-cp diff (only ~31 sampled).
- **cfg-gated platform-code type-check axis**: FULLY read line-by-line rustty-os platform files
  (windows/pty/pipe/env/homedir/hostname/path/file/systemd/dbus/wasm + wasm subdir + windows arms of
  resourcesdir/tempdir/i18n/xdg/desktop); rustty-xev backend.rs all arms; rustty-termio command.rs
  Windows spawn+wait; flatpak.rs head + FFI block. ENUMERATION (grep, workspace excl. /target): 1272
  total cfg; 837 platform/feature after excluding cfg(test); non-macos target_os gates: macos 338,
  ios 79, linux 73, freebsd 40, windows 24 (+106 bare cfg(windows)), wasm32 15; feature gates:
  oniguruma 35, identity-oracle 10, flatpak 7, wasm 2. All 14 -sys build scripts inspected for
  cc/bindgen/stub. SAMPLED not exhaustively verified: flatpak host_command body; rustty-vaxis
  tty/loop_/vaxis Windows arms; rustty-vt-c Windows arms; rustty-cli ssh_cache/disk_cache Windows
  arms; small linux/freebsd runtime branches; FreeBSD-specific libc constant availability
  (e.g. libc::IUTF8 at pty.rs:264). NOT DONE (read-only mandate): no cargo/check/build/miri — every
  "would not compile" verdict is by static reading.

### What THIS sweep still did not reach (targets for sweep #3)

- **The heavy GUI/render tiers as running code**: rustty-renderer live-windowing `Renderer::init`,
  GPU backends, the font 3-tier shaper crate, and apprt embedded runtime paths were not
  execution-audited (build-locked); only their inspector/deferral call-sites were checked.
- **PageList reflow internals** (cellIterator/rowIterator/promptIterator, index() scroll/grow,
  increase_capacity body, assert_integrity body) — repeatedly relied upon by the print + selection +
  insertLines walks but not independently re-verified this sweep.
- **The append_grapheme/move_grapheme/clear_grapheme internals** — assumed faithful by the print
  walk, not walked.
- **Non-macOS runtime behavior** (not just type-check): the many linux/freebsd runtime branches in
  pty/systemd/dbus/xdg were read for compile-shape, not behavior-diffed against the oracle.
- **FreeBSD libc-constant availability** — could not confirm the `libc` crate defines all constants
  the freebsd arms use (e.g. `libc::IUTF8` pty.rs:264) without a build.
- **A real cross `cargo check`** on any Windows/wasm target — would decisively confirm/refute C1/C4
  and M1; the build lock prevented it. Both findings are static-read verdicts.
- **The vendored `-sys` C/C++ TU internals** (dcimgui, freetype, harfbuzz, etc.) — treated as
  vendored-identical, not re-audited byte-for-byte.

---

## 5. Refuted appendix (do NOT re-raise without new evidence)

*(No candidate raised in this sweep was refuted on verification — every finding that reached
verification survived, with two severity down-corrections: C1 high→medium and C4 high→low, both
because the affected arm is cfg-gated off under the locked macOS-first target and is never compiled
in the current build, so present impact is zero and only a Windows/wasm check would fire them.)*

For the standing refuted list, see Report #1 §5 — none of its entries was re-examined here and none
should be re-raised. This sweep adds no new refutations, but records **M8** as a positive
attestation (the search-subsystem inline-alias hazard class is verified ABSENT from
terminal.rs/screen.rs/formatter.rs) so a future sweep does not re-open those bulk-unsafe bodies for
that specific hazard.
