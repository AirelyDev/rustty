# Phase 0 — Reference Oracle Build & Test Summary

**Date:** 2026-06-21
**Reference tree:** `/Users/gael/projects/ghostty` (READ-ONLY — confirmed untouched)
**Zig:** `0.15.2` (confirmed)
**HEAD:** `4749c4e93731067049bfbf2e4572061cef2bdd17` (confirmed == pinned `4749c4e`)

All zig/cmake builds were serialized (no concurrent zig invocations; shared
`.zig-cache`). Long commands were run in background and polled.

---

## 1. Full unit test suite — `zig build test`

- **Log:** `.port/phase-0/test-run.log`
- **Result:** PASS. Runner exit code = **0** (recorded as `EXIT_CODE=0` in log).
- The Zig test runner prints NO numeric pass/fail summary line when every test
  passes (it only emits per-failure diagnostics); with all tests passing the
  only stdout/stderr captured is expected `(warn)` log lines from tests that
  deliberately exercise error/warning paths (e.g. `[stream] invalid ... command`,
  `[terminal_apc] kitty graphics protocol error`, `[io_exec] failed to read
  passwd`). **No `FAIL`, no `panic`, no leaked-allocation lines appear.**
- On macOS with no `-Dtest-filter`, the `test` step also pulls in the native
  macOS app (Swift/Xcode) test dependencies; an `xcodebuild` ran as part of the
  step (its banner is at the top of the log) and the step completed cleanly.
- **Authoritative test-block count (the R1 contract source):** counted from
  source `test {}` / `test "..."` blocks under `src/`:
  - **Total: 3234** (3113 string-named `test "..."` + 45 identifier-named
    `test name {}` + 76 anonymous `test {}`; 3158 named total), 11975 assertions,
    279 files. NOTE: an earlier draft of this line said 3189 — that undercounted
    the 45 identifier-named blocks. Authoritative contract = **3234**
    (see inventory/tests-summary.json; corrected per critic blocker B1).
  - This count — not a runner summary — is the parity inventory contract. The
    runner provides the PASS/FAIL oracle only.

**Blocker status: NONE.** No failing tests.

## 2. libghostty-vt tests — `zig build test-lib-vt`

- **Log:** `.port/phase-0/test-lib-vt.log`
- **Result:** PASS. Exit code = **0** (`EXIT_CODE=0`). Only expected `(warn)`
  lines captured; no failures.
- Covers `mod.vt` (root `src/lib_vt.zig`) + `mod.vt_c` test modules.
- Source test-block counts in the lib-vt surface area:
  - `src/terminal/`: **2171** test blocks
  - `src/terminal/c/` (vt-c): **330** test blocks

## 3. libghostty-vt library — `zig build -Demit-lib-vt=true`

- **Log:** `.port/phase-0/build-lib-vt.log` (exit 0)
- Produced under `/Users/gael/projects/ghostty/zig-out/lib/`:
  - `libghostty-vt.a` (12,065,920 bytes, static)
  - `libghostty-vt.0.1.0.dylib` (5,742,200 bytes)
  - `libghostty-vt.0.dylib` -> `libghostty-vt.0.1.0.dylib` (symlink)
  - `libghostty-vt.dylib` -> `libghostty-vt.0.dylib` (symlink)
  - `ghostty-vt.xcframework/` with slices `macos-arm64_x86_64`, `ios-arm64`,
    `ios-arm64-simulator` (each with `Headers/`)
- C headers emitted under `/Users/gael/projects/ghostty/zig-out/include/ghostty/`:
  `vt.h` plus `vt/` tree (terminal.h, screen.h, style.h, color.h, sgr.h,
  selection.h, point.h, types.h, key.h + key/{event,encoder}.h,
  mouse.h + mouse/{event,encoder}.h, focus.h, paste.h, osc.h, modes.h,
  formatter.h, render.h, grid_ref.h, grid_ref_tracked.h, kitty_graphics.h,
  size_report.h, device.h, allocator.h, sys.h, build_info.h, wasm.h, ...).

## 4. Benchmarks — `zig build -Demit-bench=true`

- **Log:** `.port/phase-0/build-bench.log` (exit 0)
- IMPORTANT: this revision does **NOT** produce separate `ghostty-bench-*`
  binaries. `src/build/GhosttyBench.zig` emits exactly **two** binaries into
  `/Users/gael/projects/ghostty/zig-out/bin/`:
  - `ghostty-bench` (1,628,128 bytes) — single multiplexed runner; dispatches by
    a subcommand `Action` (see `src/benchmark/cli.zig`).
  - `ghostty-gen` — synthetic data generator (`src/main_gen.zig`).
- (`zig-out/bin/` also contains `ghostty-gen`, `props-unigen`, `symbols-unigen`
  from earlier emit steps in the shared `zig-out`.)
- Benchmark actions (run as `ghostty-bench <action> [opts]`):
  `codepoint-width`, `grapheme-break`, `screen-clone`, `terminal-parser`,
  `terminal-stream`, `is-symbol`, `osc-parser`.

## 5. Example c-vt-* programs (libghostty-vt consumers)

Built (NOT run — corpora agent will run). To keep the reference tree READ-ONLY,
each example was built with an isolated `--prefix` and `--cache-dir` under
`.port/phase-0/examples-out/<name>/`. Verified the ghostty tree stayed clean
(`git status example/` empty; no `zig-out`/`.zig-cache` leaked into `example/`).

- **Log dir:** `.port/phase-0/examples-out/<name>.log`; wrapper:
  `.port/phase-0/build-examples.sh`, top-level `.port/phase-0/build-examples.log`.

### Built successfully (21/21 build.zig-based examples, all exit 0)

All use a local path dependency `.ghostty = .{ .path = "../../" }` (no network).
Binaries at `.port/phase-0/examples-out/<dir>/zig-out/bin/<exe>`:

| dir | binary | run command |
|---|---|---|
| c-vt | c_vt | `.../c-vt/zig-out/bin/c_vt` |
| c-vt-build-info | c_vt_build_info | `.../c-vt-build-info/zig-out/bin/c_vt_build_info` |
| c-vt-colors | c_vt_colors | `.../c-vt-colors/zig-out/bin/c_vt_colors` |
| c-vt-effects | c_vt_effects | `.../c-vt-effects/zig-out/bin/c_vt_effects` |
| c-vt-encode-focus | c_vt_encode_focus | `.../c-vt-encode-focus/zig-out/bin/c_vt_encode_focus` |
| c-vt-encode-key | c_vt_encode_key | `.../c-vt-encode-key/zig-out/bin/c_vt_encode_key` |
| c-vt-encode-mouse | c_vt_encode_mouse | `.../c-vt-encode-mouse/zig-out/bin/c_vt_encode_mouse` |
| c-vt-formatter | c_vt_formatter | `.../c-vt-formatter/zig-out/bin/c_vt_formatter` |
| c-vt-grid-ref-tracked | c_vt_grid_ref_tracked | `.../c-vt-grid-ref-tracked/zig-out/bin/c_vt_grid_ref_tracked` |
| c-vt-grid-traverse | c_vt_grid_traverse | `.../c-vt-grid-traverse/zig-out/bin/c_vt_grid_traverse` |
| c-vt-kitty-graphics | c_vt_kitty_graphics | `.../c-vt-kitty-graphics/zig-out/bin/c_vt_kitty_graphics` |
| c-vt-modes | c_vt_modes | `.../c-vt-modes/zig-out/bin/c_vt_modes` |
| c-vt-paste | c_vt_paste | `.../c-vt-paste/zig-out/bin/c_vt_paste` |
| c-vt-render | c_vt_render | `.../c-vt-render/zig-out/bin/c_vt_render` |
| c-vt-selection | c_vt_selection | `.../c-vt-selection/zig-out/bin/c_vt_selection` |
| c-vt-selection-gesture | c_vt_selection_gesture | `.../c-vt-selection-gesture/zig-out/bin/c_vt_selection_gesture` |
| c-vt-sgr | c_vt_sgr | `.../c-vt-sgr/zig-out/bin/c_vt_sgr` |
| c-vt-size-report | c_vt_size_report | `.../c-vt-size-report/zig-out/bin/c_vt_size_report` |
| c-vt-static | c_vt_static | `.../c-vt-static/zig-out/bin/c_vt_static` |
| c-vt-stream | c_vt_stream | `.../c-vt-stream/zig-out/bin/c_vt_stream` |
| cpp-vt-stream | cpp_vt_stream | `.../cpp-vt-stream/zig-out/bin/cpp_vt_stream` |

(Paths above are under `/Users/gael/projects/rustty/.port/phase-0/examples-out/`.)

Alternatively each can be (re)built/run in place via `cd example/<dir> &&
zig build run`, but that writes into the ghostty tree, so it was NOT used here.

### NOT built — cmake examples (tooling unavailable)

- `c-vt-cmake`, `c-vt-cmake-static`, `c-vt-cmake-cross` — use CMake `FetchContent`
  from `https://github.com/ghostty-org/ghostty.git` (GIT_TAG main). **`cmake` is
  not installed on this machine** (`cmake not found`), so these cannot be built
  here regardless of network. (Network IS available in this environment, but the
  blocker is the missing `cmake` toolchain; these examples also fetch from
  GitHub `main`, not the pinned tree.)

### Not attempted (out of scope: wasm / swift xcframework)

- `wasm-vt`, `wasm-sgr`, `wasm-key-encode`, `swift-vt-xcframework`,
  `zig-vt`, `zig-vt-stream`, `zig-formatter` — these are Zig/wasm/Swift consumers,
  not the C `c-vt-*` programs targeted by this task. (The xcframework slices are
  already produced via `-Demit-lib-vt=true`, step 3.)

---

## Artifacts index

- `test-run.log`, `test-lib-vt.log` — test runner output (both exit 0)
- `build-lib-vt.log`, `build-bench.log` — library/bench build logs (exit 0)
- `build-examples.sh`, `build-examples.log`, `examples-out/<name>.log` — examples
- Reference library artifacts under `/Users/gael/projects/ghostty/zig-out/{lib,include,bin}`
- Example binaries under `/Users/gael/projects/rustty/.port/phase-0/examples-out/*/zig-out/bin/`

## Blockers

- **No test/parity blockers.** Both test suites pass with exit 0; no failing
  tests. (The kickoff condition "if `zig build test` reports ANY failing test"
  is NOT triggered.)
- **Tooling gap (non-parity):** `cmake` not installed -> 3 `c-vt-cmake*` examples
  not built. They are not parity oracles for VT behavior (they merely demonstrate
  CMake consumption of the same library) and fetch from GitHub `main` rather than
  the pinned tree.
