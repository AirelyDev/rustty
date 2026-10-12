# Live Rustty Terminal on /porting (wterm + rustty-wterm) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the wterm.dev demo (a just-bash shell in a DOM terminal) on `/porting`, with the terminal engine being Rustty's Rust VT core running as WebAssembly, switchable live against Ghostty's Zig core.

**Architecture:** A new standalone Rust project, `rustty-wterm`, builds a `wasm32-unknown-unknown` module that exports the exact flat ABI of wterm's Zig export layer (`packages/@wterm/ghostty/zig/src/wasm_api.zig`, `GhosttyExports` in `@wterm/ghostty`), implemented on top of the `rustty-vt` crate. The unmodified `@wterm/ghostty` `GhosttyCore` then loads it with `GhosttyCore.load({ wasmPath })`, so wterm's DOM renderer, `BashShell` (just-bash) and React bindings need no changes. The website gets one new `/porting` widget that lazy-loads wterm, just-bash and either wasm binary.

**Tech Stack:** Rust 1.93.1 (`rustty-vt` by path dependency), `wasm32-unknown-unknown`, Node 22 for the differential harness, `@wterm/{core,dom,ghostty,just-bash,react}` 0.5.4 and `just-bash` (exact pins), Next.js 16 App Router, React 19, CSS Modules.

**Spec:** None separate; the design is this section plus *Design decisions* below (it was settled in conversation on 2026-10-11 after reading `wasm_api.zig` at wterm commit `488e9a7344e812e35a951f803618d7767cc4be5b` and `rustty-vt` at Rustty `main`). Executors read both.

## Design decisions (already made — do not re-litigate)

1. **Option B, not a TS adapter.** We reimplement wterm's Zig ABI in Rust rather than writing a new `TerminalCore` in TypeScript. Reason: wterm's 867-line `GhosttyCore` JS (graphemes, hyperlinks, scrollback, graphics, tracked positions, responses) is reused untouched, and the two engines can be A/B-switched on one page by changing only `wasmPath`.
2. **The shim sits on `rustty-vt`, not `rustty-vt-c`.** The Zig layer is built on libghostty *internals* (`Terminal`, `Stream`, `RenderState`, `Pin`, `PageList`, kitty `ImageStorage`), which the C API does not expose (per-row scrollback, per-cell hyperlinks, prompt iteration, tracked pins). `rustty-vt` has the Rust equivalents of each.
3. **Standalone repository at `~/projects/rustty-wterm`**, not a crate inside the Rustty workspace. Reasons: (a) it ports nothing from Ghostty, so Rustty's rules and gates (R1–R5, comment-fidelity, ABI-schema, acyclic) do not apply and must not be bent for it; (b) Rustty's gates enumerate crates by hand (`check_valgrind.sh`, `sanitizers.sh`, `xev_scope_boundary_gate.sh`, `check_runtime_artifact_setter.py`, `.port/tools/ci.sh` all name `rustty-web-canvas`/`rustty-wasm` explicitly), so a new member means editing several gate lists under R5, for a crate that is not part of the port (the article's own figures are as-of commit `58aa62a81`, so they are not at risk); (c) it is an adapter between two other projects' ABIs and may later go upstream to `vercel-labs/wterm` as `@wterm/rustty`. It depends on `rustty-vt` by path (`../rustty/crates/rustty-vt`); verified on 2026-10-11 that `cargo metadata` resolves this from outside the Rustty workspace.
4. **Mirror the Zig layer function for function.** Same exports, same byte layouts, same quirks (documented in `wasm_api.zig`), so the differential test can demand byte equality wherever the engines agree.
5. **Custom demo commands are shell scripts in the virtual FS**, not just-bash custom commands. `BashShell` constructs its own `Bash` and exposes no command-registration option, so `printf`-based scripts under `/home/user/demo/` are the supported route.
6. **Lazy loading everywhere.** The page must not ship wterm, just-bash or any wasm until the visitor clicks "Start the terminal".

## Global Constraints

- **Never push** either repository. The website's origin is `ghostty-org/website`; commit locally only.
- Website work happens in a new worktree `/Users/gael/projects/ghostty-website-wterm` on branch `wterm-live-terminal`, created from `codex/rustty-website-rebrand` at `b4e4d40d`. Rustty-wterm work happens in `/Users/gael/projects/rustty-wterm` on branch `main` (new repo, local only).
- Do not modify `/Users/gael/projects/rustty` (read-only dependency) or `/Users/gael/projects/ghostty` (read-only oracle).
- Pin exactly: `@wterm/core`, `@wterm/dom`, `@wterm/ghostty`, `@wterm/just-bash`, `@wterm/react` = `0.5.4` (no `^`). Record the wterm reference commit `488e9a7344e812e35a951f803618d7767cc4be5b` and the `rustty-vt` commit used in `rustty-wterm/README.md`.
- Rust: toolchain `1.93.1` (copy Rustty's `rust-toolchain.toml`); `#![deny(unsafe_op_in_unsafe_fn)]`; every `unsafe` block carries a `// SAFETY:` comment; wasm build has **zero imports** (the Zig layer imports `env.log`; ours must not, and the loader must tolerate an unused `env` import object).
- The wasm must export every symbol in `GhosttyExports` that the Zig build exports, **except** where a task states an explicit, documented omission (optional exports only).
- Website article rules (enforced by `npm run verify:porting`): every number in prose is a `<Fact>` with an `href`; no bare commit hashes (`<CommitLink>`); no bare source paths (`<SourceLink>`); no internal ids (`R4`, `G12`, …) in widget text; the registry `deck` never repeats a number its section's prose states; "pin" never means the reference commit; Bun appears only in "The method matures".
- Word choice in prose: "engine" for the terminal core, "Rust engine"/"Zig engine"; never "emulator core". Pacific time for dates.
- Commit trailers on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Quality bar. Rust: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Website: `npm run lint`, `npm run verify:porting`, `npm run build`, `npm run verify:porting:build`, `npm run verify:animation`; Playwright at 1100 px and 390 px against `next build` + `next start -p 3143` (3141 is the author's dev server — kill only PIDs you started, never by pattern); Playwright output under `/Users/gael/projects/rustty/.playwright-mcp/`, deleted after.

## Review Focus

1. **Zig and Rust engines disagree on a byte stream** (version skew between wterm's pinned libghostty and Rustty's oracle). Expected: every difference is either fixed or listed, with a reason, in `harness/known-diffs.json`; an unlisted difference fails the harness. Test: Task 6.
2. **A response query hangs the shell** (DA1, DSR 6, DECRQM, OSC 10/11, kitty keyboard query). Expected: the exact reply bytes the Zig layer produces, queued in order, dropped when the 256-entry queue is full. Test: Task 4.
3. **Hostile or huge input** (offset `u32::MAX` into scrollback, 64 KB OSC 52, a 48 MB+ APC, zero-size resize, `resize(0, 0)`). Expected: no panic/abort; out-of-range returns 0/-1 like the Zig layer. wasm panics abort the whole module, so each exported function must be total. Test: Task 3 and Task 5.
4. **Visitor never clicks start / is offline / wasm fails to instantiate.** Expected: the article renders fully without the demo; a failed load shows a static fallback and a link, never a blank card or a throw. Test: Task 9 and Task 11.
5. **Engine switch mid-session.** Expected: switching re-initialises the core and replays the session's recorded output so the screen matches, with scrollback; cursor and title survive; no leaked wasm instance (old core `dispose()`d). Test: Task 10.
6. **Phone width and reduced motion.** Expected: no page-level horizontal scroll at 390 px; on-screen keyboard can open; `prefers-reduced-motion` disables cursor blink animation and any auto-play. Test: Task 11.
7. **Wasm served with the wrong MIME type** (`application/octet-stream` locally or behind a proxy). Expected: still loads (wterm falls back from `compileStreaming` to `arrayBuffer`). Test: Task 9 build check serves with and without the header.

---

## File Structure

### `~/projects/rustty-wterm` (new repository)

| Path | Responsibility |
|---|---|
| `Cargo.toml`, `rust-toolchain.toml`, `Cargo.lock` | Package `rustty-wterm` (`cdylib` + `rlib`), `rustty-vt` path dep, lockfile seeded from Rustty's |
| `src/lib.rs` | Crate root; module wiring; `#[cfg(target_arch = "wasm32")] mod exports;` |
| `src/cell.rs` | Pure cell byte encoder (`CellFields` → 16/20-byte layout). No VT types — unit-testable alone |
| `src/sinks.rs` | `ResponseQueue`, `TitleState`, `DirectoryState`, `ClipboardState`, `ShellState`, `Counters` — ports of the Zig structs, pure |
| `src/core.rs` | `Core`: owns `Terminal`, `Stream`, `RenderState`, sinks; safe Rust API used by exports and tests |
| `src/effects.rs` | `Effects` callbacks that route stream effects into the sinks |
| `src/scrollback.rs` | Scrollback / viewport row readers over `PageList` pins, grapheme + hyperlink encoders |
| `src/graphics.rs` | Kitty image/placement record encoders, version + fingerprint logic (Task 5) |
| `src/positions.rs` | 64-slot tracked-position table |
| `src/exports.rs` | `#[no_mangle] extern "C"` wrappers; the `GhosttyExports` surface |
| `tests/*.rs` | Host integration tests (`cargo test`) |
| `tools/build-wasm.sh` | Release wasm build + export/import audit |
| `tools/audit-wasm.mjs` | Fails if the wasm has imports or lacks a required export |
| `harness/` | Node differential harness: `package.json` (pins wterm), `diff.mjs`, `corpus/*.bin` + `corpus/gen.mjs`, `known-diffs.json` |
| `README.md` | What it is, pins, build, test, ABI table |

### `~/projects/ghostty-website-wterm` (worktree of `ghostty-website`)

| Path | Responsibility |
|---|---|
| `package.json` | Add exact-pinned wterm packages and `just-bash` |
| `scripts/porting/sync-wterm-wasm.mjs` | Copies both wasm binaries into `public/porting/wterm/` and writes `manifest.json` (sizes, sha256, versions) |
| `public/porting/wterm/{rustty-wterm.wasm,ghostty-vt.wasm,manifest.json}` | Served binaries |
| `src/components/porting/live-terminal/index.ts` | Default export for the `WidgetMount` loader |
| `src/components/porting/live-terminal/Body.tsx` | Start gate, engine switch, status line, error fallback |
| `src/components/porting/live-terminal/TerminalView.tsx` | Client-only wterm mount (dynamic imports of `@wterm/dom`, `@wterm/ghostty`, `@wterm/just-bash`) |
| `src/components/porting/live-terminal/session.ts` | Pure: seeded files, env, prompt, greeting, recorded-output buffer for engine-switch replay |
| `src/components/porting/live-terminal/demo-scripts.ts` | Pure: the `printf` showcase scripts as strings |
| `src/components/porting/live-terminal/engines.ts` | Pure: engine ids → wasm URLs, labels |
| `src/components/porting/live-terminal/LiveTerminal.module.css` | Styles |
| `src/components/porting/ui/WidgetMount.tsx` | Add loader entry `"live-terminal"` |
| `src/components/porting/lib/registry.ts` | Add widget entry |
| `src/app/porting/content.mdx` | Add section + `<PortingWidget slug="live-terminal" />` |
| `src/app/porting/page.tsx` | Add TOC group entry if the section opens a group |
| `scripts/verify-porting-terminal.mjs` | Node checks: session/engines/scripts invariants, manifest vs files, lazy-load guard |
| `package.json` `verify:porting` | Append the new script |

---

## Task 1: Scaffold `rustty-wterm`, lifecycle exports, wasm audit

**Files:**
- Create: `~/projects/rustty-wterm/{Cargo.toml,rust-toolchain.toml,.gitignore,README.md}`, `src/{lib.rs,core.rs,exports.rs}`, `tests/lifecycle.rs`, `tools/build-wasm.sh`, `tools/audit-wasm.mjs`

**Interfaces:**
- Produces (used by every later task):
  - `pub struct Core` with `pub fn new(cols: u16, rows: u16, max_scrollback: u32, fg_rgb: u32, bg_rgb: u32, image_storage_limit: u32) -> Option<Box<Core>>`, `pub fn write(&mut self, data: &[u8])`, `pub fn resize(&mut self, cols: u16, rows: u16)`, `pub fn cols(&self) -> u32`, `pub fn rows(&self) -> u32`, `pub fn terminal(&self) -> &Terminal`, `pub fn terminal_mut(&mut self) -> &mut Terminal`.
  - Exports: `init(u16,u16,u32,u32,u32,u32)->usize`, `deinit(usize)`, `resize(usize,u16,u16)`, `write(usize,*const u8,u32)`, `get_cols(usize)->u32`, `get_rows(usize)->u32`, `alloc_buffer(u32)->usize`, `free_buffer(usize,u32)`.
  - `tools/audit-wasm.mjs <file.wasm>` exits non-zero on any import or any missing name from `REQUIRED` (a constant list at the top of the file, grown by later tasks).

- [ ] **Step 1: Create the repo and manifest**

```bash
mkdir -p ~/projects/rustty-wterm && cd ~/projects/rustty-wterm && git init -b main
cp ~/projects/rustty/rust-toolchain.toml .
cp ~/projects/rustty/Cargo.lock .
```

`Cargo.toml`:

```toml
[package]
name = "rustty-wterm"
version = "0.1.0"
edition = "2021"
rust-version = "1.93"
license = "MIT"
publish = false
description = "wterm's ghostty-vt wasm ABI, implemented on the Rustty VT engine"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# Path dependency on the Rustty workspace (read-only). Features mirror what the
# Zig layer needs: render state and kitty graphics. Everything else is off to
# keep the wasm small; Task 5 decides whether kitty-graphics survives
# wasm32-unknown-unknown.
rustty-vt = { path = "../rustty/crates/rustty-vt", default-features = false, features = ["render-state", "kitty-graphics"] }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

`.gitignore`: `target/` and `harness/node_modules/`.

- [ ] **Step 2: Write the failing lifecycle test**

`tests/lifecycle.rs`:

```rust
use rustty_wterm::Core;

#[test]
fn new_core_reports_requested_grid() {
    let core = Core::new(80, 24, 10_000, 0xd4d4d4, 0x1e1e1e, 32 * 1024 * 1024).expect("core");
    assert_eq!((core.cols(), core.rows()), (80, 24));
}

#[test]
fn write_then_resize_keeps_core_usable() {
    let mut core = Core::new(80, 24, 1_000, 0xffffff, 0x000000, 0).unwrap();
    core.write(b"hello\r\nworld");
    core.resize(100, 30);
    assert_eq!((core.cols(), core.rows()), (100, 30));
}

#[test]
fn zero_sized_grid_is_rejected_not_aborted() {
    // wasm panics abort the module; init must return None instead.
    assert!(Core::new(0, 0, 0, 0, 0, 0).is_none());
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cd ~/projects/rustty-wterm && cargo test --test lifecycle`
Expected: FAIL — `unresolved import rustty_wterm::Core` (crate has no source yet).

- [ ] **Step 4: Implement `Core` (minimal)**

`src/lib.rs`:

```rust
//! rustty-wterm — wterm's `ghostty-vt.wasm` ABI on the Rustty VT engine.
#![deny(unsafe_op_in_unsafe_fn)]

mod core;
pub use crate::core::Core;

#[cfg(target_arch = "wasm32")]
mod exports;
```

`src/core.rs` (field and type names follow `rustty-vt`; if `cargo check` reports a mismatch, the authoritative references are `crates/rustty-vt/src/terminal.rs` `Options` at line ~302 and the `Options` literal in `crates/rustty-vt/tests/d2_capacity_saturation.rs` line ~395):

```rust
use rustty_vt::alloc::Global;
use rustty_vt::color::{DynamicPalette, DynamicRGB};
use rustty_vt::modes::ModePacked;
use rustty_vt::render::RenderState;
use rustty_vt::stream_terminal::Handler;
use rustty_vt::terminal::{Colors, Options, Terminal};
use rustty_vt::Stream;

/// One terminal plus everything the wterm ABI hangs off it. Boxed so the
/// `Terminal` address is stable: the stream's handler holds a raw pointer to it.
pub struct Core {
    terminal: Box<Terminal>,
    stream: Stream<'static>,
    render: RenderState,
    cols: u16,
    rows: u16,
}

fn rgb(v: u32) -> DynamicRGB {
    DynamicRGB::init(rustty_vt::color::RGB {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    })
}

impl Core {
    pub fn new(
        cols: u16,
        rows: u16,
        max_scrollback: u32,
        fg_rgb: u32,
        bg_rgb: u32,
        image_storage_limit: u32,
    ) -> Option<Box<Core>> {
        if cols == 0 || rows == 0 {
            return None;
        }
        let mut modes = ModePacked::default();
        modes.grapheme_cluster = true; // `.default_modes = .{ .grapheme_cluster = true }` in wasm_api.zig
        let opts = Options {
            cols,
            rows,
            max_scrollback_bytes: Some(max_scrollback as usize),
            max_scrollback_lines: None,
            colors: Colors {
                background: rgb(bg_rgb),
                foreground: rgb(fg_rgb),
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modes,
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            default_xt_checksum: Default::default(),
            kitty_image_storage_limit: image_storage_limit as usize,
            kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
        };
        let mut terminal = Box::new(Terminal::init(Global, opts).ok()?);
        // SAFETY: the Box is never moved out of `Core`, and `Core` is dropped
        // (stream first) before the Terminal, so the handler's pointer stays valid.
        let stream: Stream<'static> =
            unsafe { core::mem::transmute(terminal.vt_stream()) };
        Some(Box::new(Core { terminal, stream, render: RenderState::empty(), cols, rows }))
    }

    pub fn write(&mut self, data: &[u8]) {
        self.stream.next_slice(data);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        let _ = self.terminal.resize(Global, cols, rows);
        self.cols = cols;
        self.rows = rows;
    }

    pub fn cols(&self) -> u32 {
        self.cols as u32
    }
    pub fn rows(&self) -> u32 {
        self.rows as u32
    }
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }
    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }
}
```

Field drop order matters: declare `stream` before `terminal` is **not** what Rust does (fields drop in declaration order), so reorder the struct as `stream, render, terminal, cols, rows` and keep the SAFETY comment accurate. Remove `Core { … }` literal ordering accordingly.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test lifecycle`
Expected: PASS (3 tests). If `RenderState::empty()` or `DynamicRGB::init` names differ, fix them against the source files named in Step 4; do not weaken the tests.

- [ ] **Step 6: Add the wasm exports and audit tooling**

`src/exports.rs`:

```rust
//! `#[no_mangle] extern "C"` surface. Handles are `usize` pointers to a boxed
//! `Core`, as in wasm_api.zig. Every function is total: bad input returns the
//! Zig layer's sentinel and never panics (a panic aborts the whole module).
use crate::Core;

fn core<'a>(ptr: usize) -> Option<&'a mut Core> {
    if ptr == 0 {
        return None;
    }
    // SAFETY: `ptr` came from `init` (Box::into_raw) and is live until `deinit`.
    Some(unsafe { &mut *(ptr as *mut Core) })
}

#[no_mangle]
pub extern "C" fn init(cols: u16, rows: u16, max_scrollback: u32, fg: u32, bg: u32, img: u32) -> usize {
    match Core::new(cols, rows, max_scrollback, fg, bg, img) {
        Some(b) => Box::into_raw(b) as usize,
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn deinit(ptr: usize) {
    if ptr != 0 {
        // SAFETY: matches the Box::into_raw in `init`; JS calls this once.
        drop(unsafe { Box::from_raw(ptr as *mut Core) });
    }
}

#[no_mangle]
pub extern "C" fn resize(ptr: usize, cols: u16, rows: u16) {
    if let Some(c) = core(ptr) {
        c.resize(cols, rows);
    }
}

#[no_mangle]
pub extern "C" fn write(ptr: usize, data: *const u8, len: u32) {
    let Some(c) = core(ptr) else { return };
    if data.is_null() {
        return;
    }
    // SAFETY: JS passes a buffer from `alloc_buffer` of at least `len` bytes.
    c.write(unsafe { core::slice::from_raw_parts(data, len as usize) });
}

#[no_mangle]
pub extern "C" fn get_cols(ptr: usize) -> u32 {
    core(ptr).map_or(0, |c| c.cols())
}
#[no_mangle]
pub extern "C" fn get_rows(ptr: usize) -> u32 {
    core(ptr).map_or(0, |c| c.rows())
}

#[no_mangle]
pub extern "C" fn alloc_buffer(len: u32) -> usize {
    let mut v: Vec<u8> = Vec::new();
    if v.try_reserve_exact(len as usize).is_err() {
        return 0;
    }
    v.resize(len as usize, 0);
    let b = v.into_boxed_slice();
    Box::into_raw(b) as *mut u8 as usize
}

#[no_mangle]
pub extern "C" fn free_buffer(ptr: usize, len: u32) {
    if ptr != 0 {
        // SAFETY: `ptr`/`len` are exactly what `alloc_buffer` returned/received.
        drop(unsafe { Box::from_raw(core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize)) });
    }
}
```

`tools/build-wasm.sh`:

```sh
#!/bin/sh
# Release wasm build, audited. Usage: tools/build-wasm.sh
set -e
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo rustc --release --target wasm32-unknown-unknown -- -C link-arg=-zstack-size=131072
out=target/wasm32-unknown-unknown/release/rustty_wterm.wasm
node tools/audit-wasm.mjs "$out"
ls -l "$out"
```

`tools/audit-wasm.mjs`:

```js
import { readFileSync } from "node:fs";

/** REQUIRED lists every export the wasm must carry; later tasks append to it. */
const REQUIRED = ["init", "deinit", "resize", "write", "get_cols", "get_rows", "alloc_buffer", "free_buffer", "memory"];

const module = new WebAssembly.Module(readFileSync(process.argv[2]));
const imports = WebAssembly.Module.imports(module);
const exports = new Set(WebAssembly.Module.exports(module).map((e) => e.name));
const missing = REQUIRED.filter((n) => !exports.has(n));
if (imports.length || missing.length) {
  console.error("imports:", imports.map((i) => `${i.module}.${i.name}`), "missing:", missing);
  process.exit(1);
}
console.log(`ok: ${exports.size} exports, 0 imports`);
```

- [ ] **Step 7: Build the wasm and run the audit**

Run: `chmod +x tools/build-wasm.sh && tools/build-wasm.sh`
Expected: `ok: … exports, 0 imports` and a file size line. If imports appear (e.g. `env.*`), find which `rustty-vt` dependency pulls them and gate it off with features — do not add a stub import.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat: scaffold rustty-wterm with lifecycle exports and wasm audit

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 2: Cell encoder, viewport, dirty tracking, cursor and mode getters

**Files:**
- Create: `src/cell.rs`, `tests/cell_layout.rs`, `tests/viewport.rs`
- Modify: `src/core.rs`, `src/lib.rs`, `src/exports.rs`, `tools/audit-wasm.mjs` (REQUIRED)

**Interfaces:**
- Consumes: `Core::{write,terminal,terminal_mut}` from Task 1.
- Produces:
  - `pub const CELL_BYTES: usize = 16; pub const CELL_BYTES_V2: usize = 20;`
  - `pub struct CellFields { pub codepoint: u32, pub fg: Option<[u8;3]>, pub bg: Option<[u8;3]>, pub flags: u8, pub width: u8, pub has_grapheme: bool, pub has_hyperlink: bool, pub spacer_head: bool, pub underline: u8, pub underline_rgb: Option<[u8;3]> }`
  - `pub fn encode_cell(fields: &CellFields, out: &mut [u8])` — `out.len()` selects v1 (16) or v2 (20).
  - `Core::update(&mut self)`, `Core::viewport(&mut self, out: &mut [u8], cell_bytes: usize) -> u32`, `Core::is_dirty(&self) -> u32` (0 false / 1 partial / 2 full), `Core::is_dirty_row(&self, row: u16) -> bool`, `Core::clear_dirty(&mut self)`, `Core::cursor(&self) -> CursorInfo { row: u32, col: u32, visible: bool, shape: u32, blinking: bool }`, mode getters `cursor_keys_app/bracketed_paste/using_alt_screen/mouse_tracking/mouse_sgr/mouse_encoding/focus_events/synchronized_output/kitty_keyboard_flags`.
  - Exports: `update, get_viewport, get_viewport_v2, is_dirty, is_dirty_row, clear_dirty, get_cursor_row, get_cursor_col, get_cursor_visible, get_cursor_shape, get_cursor_blinking, cursor_keys_app, bracketed_paste, using_alt_screen, mouse_tracking, mouse_sgr, mouse_encoding, focus_events, synchronized_output, kitty_keyboard_flags`.

The authoritative behaviour is `encodeCell`, `packFlags`, `cellWidth`, `getViewport`, the cursor and the mode exports in `wasm_api.zig` lines 866–1224 (wterm checkout: `/private/tmp/claude-501/-Users-gael/4a637fbe-2c34-4cf2-8dc7-93338a6861bd/scratchpad/wterm-repo/packages/@wterm/ghostty/zig/src/wasm_api.zig`; re-clone with `git clone https://github.com/vercel-labs/wterm` at `488e9a7344e812e35a951f803618d7767cc4be5b` if missing).

- [ ] **Step 1: Write the failing layout tests** (pure encoder, from the layout comment at the top of `wasm_api.zig`)

`tests/cell_layout.rs`:

```rust
use rustty_wterm::cell::{encode_cell, CellFields, CELL_BYTES, CELL_BYTES_V2};

fn base() -> CellFields {
    CellFields { codepoint: 'A' as u32, fg: None, bg: None, flags: 0, width: 1,
        has_grapheme: false, has_hyperlink: false, spacer_head: false,
        underline: 0, underline_rgb: None }
}

#[test]
fn v1_layout_matches_the_documented_byte_offsets() {
    let mut f = base();
    f.fg = Some([1, 2, 3]);
    f.bg = Some([4, 5, 6]);
    f.flags = 0x01 | 0x20;
    f.width = 2;
    f.has_grapheme = true;
    f.has_hyperlink = true;
    let mut out = [0xAAu8; CELL_BYTES];
    encode_cell(&f, &mut out);
    assert_eq!(&out[0..4], &65u32.to_le_bytes());
    assert_eq!(&out[4..10], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(out[10], 0x21);      // bold | inverse
    assert_eq!(out[11], 2);         // width
    assert_eq!(out[12], 0b11);      // explicit fg | explicit bg
    assert_eq!(out[13], 0b011);     // grapheme | hyperlink
    assert_eq!(&out[14..16], &[0, 0]);
}

#[test]
fn spacer_head_sets_content_flag_bit_two_and_keeps_width_one() {
    let mut f = base();
    f.width = 1;
    f.spacer_head = true;
    let mut out = [0u8; CELL_BYTES];
    encode_cell(&f, &mut out);
    assert_eq!(out[11], 1);
    assert_eq!(out[13] & 4, 4);
}

#[test]
fn v2_adds_underline_style_and_colour_and_color_flag_bit_two() {
    let mut f = base();
    f.underline = 3; // curly
    f.underline_rgb = Some([9, 8, 7]);
    let mut out = [0u8; CELL_BYTES_V2];
    encode_cell(&f, &mut out);
    assert_eq!(out[12] & 4, 4);
    assert_eq!(out[16], 3);
    assert_eq!(&out[17..20], &[9, 8, 7]);
}

#[test]
fn v2_without_underline_colour_leaves_bit_two_clear() {
    let mut out = [0u8; CELL_BYTES_V2];
    encode_cell(&base(), &mut out);
    assert_eq!(out[12] & 4, 0);
    assert_eq!(&out[16..20], &[0, 0, 0, 0]);
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test --test cell_layout` → FAIL (`unresolved import rustty_wterm::cell`).

- [ ] **Step 3: Implement `src/cell.rs`**

```rust
//! Byte layout of one cell as wterm's JS reads it (`parseCell` in
//! `@wterm/ghostty` `wasm-bindings.js`). Pure: no VT types, so it is tested
//! without a terminal.

pub const CELL_BYTES: usize = 16;
pub const CELL_BYTES_V2: usize = 20;

#[derive(Clone, Debug, Default)]
pub struct CellFields {
    pub codepoint: u32,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    /// bold=1 faint=2 italic=4 underline=8 blink=16 inverse=32 invisible=64 strike=128
    pub flags: u8,
    /// 0 = continuation of a wide cell, 1 = normal (also spacer head), 2 = wide
    pub width: u8,
    pub has_grapheme: bool,
    pub has_hyperlink: bool,
    pub spacer_head: bool,
    /// 0 none, 1 single, 2 double, 3 curly, 4 dotted, 5 dashed (v2 only)
    pub underline: u8,
    pub underline_rgb: Option<[u8; 3]>,
}

/// Writes `fields` into `out`. `out.len()` must be 16 (v1) or 20 (v2).
pub fn encode_cell(f: &CellFields, out: &mut [u8]) {
    debug_assert!(out.len() == CELL_BYTES || out.len() == CELL_BYTES_V2);
    let fg = f.fg.unwrap_or([0; 3]);
    let bg = f.bg.unwrap_or([0; 3]);
    out[0..4].copy_from_slice(&f.codepoint.to_le_bytes());
    out[4..7].copy_from_slice(&fg);
    out[7..10].copy_from_slice(&bg);
    out[10] = f.flags;
    out[11] = f.width;
    out[12] = u8::from(f.fg.is_some()) | (u8::from(f.bg.is_some()) << 1);
    out[13] = u8::from(f.has_grapheme) | (u8::from(f.has_hyperlink) << 1) | (u8::from(f.spacer_head) << 2);
    out[14] = 0;
    out[15] = 0;
    if out.len() == CELL_BYTES_V2 {
        out[16] = f.underline;
        let u = f.underline_rgb.unwrap_or([0; 3]);
        if f.underline_rgb.is_some() {
            out[12] |= 4;
        }
        out[17..20].copy_from_slice(&u);
    }
}
```

Add `pub mod cell;` to `src/lib.rs`.

- [ ] **Step 4: Run to verify pass** — `cargo test --test cell_layout` → PASS (4 tests).

- [ ] **Step 5: Write the failing viewport test (end to end through a real terminal)**

`tests/viewport.rs`:

```rust
use rustty_wterm::{cell::CELL_BYTES_V2, Core};

fn viewport(core: &mut Core) -> Vec<u8> {
    core.update();
    let mut buf = vec![0u8; (core.cols() * core.rows()) as usize * CELL_BYTES_V2];
    assert_eq!(core.viewport(&mut buf, CELL_BYTES_V2), core.cols() * core.rows());
    buf
}
fn cell(buf: &[u8], cols: u32, row: u32, col: u32) -> &[u8] {
    let i = (row * cols + col) as usize * CELL_BYTES_V2;
    &buf[i..i + CELL_BYTES_V2]
}

#[test]
fn red_bold_text_is_encoded_with_explicit_fg_and_bold() {
    let mut c = Core::new(10, 3, 100, 0xd4d4d4, 0x1e1e1e, 0).unwrap();
    c.write(b"\x1b[1;31mhi\x1b[0m");
    let v = viewport(&mut c);
    let h = cell(&v, 10, 0, 0);
    assert_eq!(&h[0..4], &('h' as u32).to_le_bytes());
    assert_eq!(h[10] & 1, 1);       // bold
    assert_eq!(h[12] & 1, 1);       // explicit fg
    assert_ne!(&h[4..7], &[0, 0, 0]);
}

#[test]
fn wide_glyph_has_width_two_then_a_zero_width_tail() {
    let mut c = Core::new(10, 3, 100, 0, 0, 0).unwrap();
    c.write("漢".as_bytes());
    let v = viewport(&mut c);
    assert_eq!(cell(&v, 10, 0, 0)[11], 2);
    assert_eq!(cell(&v, 10, 0, 1)[11], 0);
}

#[test]
fn dirty_rows_are_reported_then_cleared() {
    let mut c = Core::new(10, 3, 100, 0, 0, 0).unwrap();
    c.write(b"x");
    c.update();
    assert!(c.is_dirty() >= 1);
    assert!(c.is_dirty_row(0));
    c.clear_dirty();
    assert_eq!(c.is_dirty(), 0);
    assert!(!c.is_dirty_row(0));
}

#[test]
fn cursor_follows_output_and_modes_report() {
    let mut c = Core::new(10, 3, 100, 0, 0, 0).unwrap();
    c.write(b"ab\x1b[?1h\x1b[?2004h\x1b[?1049h");
    c.update();
    let cur = c.cursor();
    assert_eq!((cur.row, cur.col), (0, 2));
    assert!(cur.visible);
    assert!(c.cursor_keys_app());
    assert!(c.bracketed_paste());
    assert!(c.using_alt_screen());
}

#[test]
fn viewport_into_too_small_buffer_writes_nothing() {
    let mut c = Core::new(10, 3, 100, 0, 0, 0).unwrap();
    c.update();
    let mut tiny = [0u8; 4];
    assert_eq!(c.viewport(&mut tiny, CELL_BYTES_V2), 0);
}
```

- [ ] **Step 6: Run to verify failure** — `cargo test --test viewport` → FAIL (`no method named update`).

- [ ] **Step 7: Implement in `Core`**

Port `getViewport` / `encodeCell` / `packFlags` / `cellWidth` / `resolveRgb` from `wasm_api.zig` lines 866–1011 into `Core::viewport`, iterating `self.render.row_data` and each row's cells (`raw`, `style`). Required behaviours, each from the Zig source:
- Style is read **only when `raw.style_id != 0`**, else the default style (the Zig comment at lines 999–1003 explains the stale-style hazard).
- Background: cell-content background (`bg_color_palette` / `bg_color_rgb`) wins over the style background; both set `bg` and the "explicit bg" flag.
- `cp` is the codepoint only for `codepoint` / `codepoint_grapheme` content, else 0.
- Width: narrow→1, wide→2, spacer_tail→0, spacer_head→1 (+ `spacer_head` flag).
- Underline style index and colour come from the style (`underlineColor(palette)` equivalent) and only matter for v2.
- Pad missing rows/cells with zero bytes; return `rows * cols`; return 0 without writing if `out.len() < rows * cols * cell_bytes`.
- Dirty: `RenderState.dirty` (false/partial/full) and per-row `dirty`; `clear_dirty` clears both.
- Cursor: `render.cursor.active` x/y, `visible`, `visual_style` (block|block_hollow→0, underline→1, bar→2), `blinking`.
- Modes: `cursor_keys`, `bracketed_paste`, `focus_event`, `synchronized_output`; alt screen = active screen key ≠ primary; mouse tracking `normal→1000, button→1002, any→1003, else 0`; `mouse_sgr` = format is sgr; `mouse_encoding` = the format's integer discriminant (must match JS `MouseEncoding` ordering in `ghostty-core.js`; verify by reading how `mouseEncoding()` maps numbers); `kitty_keyboard_flags` = current flags int.

Then add the exports in `exports.rs`, for example:

```rust
#[no_mangle]
pub extern "C" fn get_viewport_v2(ptr: usize, buf: *mut u8) -> u32 {
    let Some(c) = core(ptr) else { return 0 };
    if buf.is_null() { return 0; }
    let n = (c.cols() * c.rows()) as usize * crate::cell::CELL_BYTES_V2;
    // SAFETY: JS allocated cols*rows*20 bytes via alloc_buffer for this call.
    c.viewport(unsafe { core::slice::from_raw_parts_mut(buf, n) }, crate::cell::CELL_BYTES_V2)
}
```

(`get_viewport` identical with `CELL_BYTES`; the other exports are one-liners over the `Core` getters, booleans as `u32` 0/1.) Append every new export name to `REQUIRED` in `tools/audit-wasm.mjs`.

- [ ] **Step 8: Run all tests and the wasm audit** — `cargo test && tools/build-wasm.sh` → all PASS, `ok: … 0 imports`.

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "feat: cell encoder, viewport, dirty tracking, cursor and mode exports

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 3: Graphemes, hyperlinks, scrollback and row-wrap metadata

**Files:**
- Create: `src/scrollback.rs`, `tests/scrollback.rs`
- Modify: `src/core.rs`, `src/lib.rs`, `src/exports.rs`, `tools/audit-wasm.mjs`

**Interfaces:**
- Consumes: `encode_cell`, `CellFields`, `Core::{write,update,viewport}`.
- Produces: `Core::scrollback_count() -> u32`, `Core::scrollback_discarded() -> u32`, `Core::scrollback_line(offset: u32, out: &mut [u8], max_cols: u32, cell_bytes: usize) -> u32`, `Core::grapheme_viewport(row: u32, col: u32, out: Option<&mut [u8]>) -> u32`, `Core::grapheme_scrollback(offset, col, out) -> u32`, `Core::hyperlink_viewport(row, col, out) -> u32`, `Core::hyperlink_scrollback(offset, col, out) -> u32`, `Core::row_wraps(row) -> i32`, `Core::scrollback_row_wraps(offset) -> i32`. Exports: `get_scrollback_count, get_scrollback_discarded_count, get_scrollback_line, get_scrollback_line_v2, get_scrollback_grapheme, get_scrollback_hyperlink, get_viewport_grapheme, get_viewport_hyperlink, get_row_wraps, get_scrollback_row_wraps`.

Reference: `wasm_api.zig` lines 1013–1113 (grapheme/hyperlink encoders) and 1410–1526 (scrollback). Key semantics to preserve:
- Scrollback **offset 0 is the newest retained row** (the row directly above the active area); `get_scrollback_count` = total page rows − visible rows (0 if not greater).
- A row is found by walking `rows_up = offset + 1` from the active top-left; the `+ 1` **must be checked** (`checked_add`) — `offset = u32::MAX` returns 0 (Review Focus 3).
- Grapheme encoding returns the required byte length when the buffer is too small or null, 0 when the cell has no extra codepoints.
- Hyperlink encoding is `uri \0 explicit-id \0 implicit-id` and returns the required length (`uri + id lengths + 2`); implicit ids are decimal text.
- Wrap flags: bit 0 = row wraps to next, bit 1 = row continues previous; `-1` for out of range.

- [ ] **Step 1: Write the failing tests**

`tests/scrollback.rs`:

```rust
use rustty_wterm::{cell::CELL_BYTES_V2, Core};

fn fill(core: &mut Core, lines: u32) {
    for i in 0..lines {
        core.write(format!("line {i}\r\n").as_bytes());
    }
}

#[test]
fn scrolled_off_rows_are_counted_and_offset_zero_is_the_newest() {
    let mut c = Core::new(20, 5, 100_000, 0, 0, 0).unwrap();
    fill(&mut c, 12);
    c.update();
    let n = c.scrollback_count();
    assert!(n >= 7, "expected at least 7 rows in history, got {n}");
    let mut row = vec![0u8; 20 * CELL_BYTES_V2];
    assert_eq!(c.scrollback_line(0, &mut row, 20, CELL_BYTES_V2), 20);
    let text: String = (0..6).map(|i| char::from_u32(u32::from_le_bytes(row[i*20..i*20+4].try_into().unwrap())).unwrap()).collect();
    // newest history row is the last one that scrolled off, not "line 0"
    assert_ne!(text, "line 0");
}

#[test]
fn absurd_offsets_return_zero_instead_of_wrapping_to_the_newest_row() {
    let mut c = Core::new(20, 5, 100_000, 0, 0, 0).unwrap();
    fill(&mut c, 12);
    let mut row = vec![0u8; 20 * CELL_BYTES_V2];
    assert_eq!(c.scrollback_line(u32::MAX, &mut row, 20, CELL_BYTES_V2), 0);
    assert_eq!(c.scrollback_line(c.scrollback_count(), &mut row, 20, CELL_BYTES_V2), 0);
    assert_eq!(c.grapheme_scrollback(u32::MAX, 0, None), 0);
    assert_eq!(c.scrollback_row_wraps(u32::MAX), -1);
}

#[test]
fn zwj_emoji_reports_full_grapheme_length_then_bytes() {
    let mut c = Core::new(20, 3, 100, 0, 0, 0).unwrap();
    let family = "👨‍👩‍👧";
    c.write(family.as_bytes());
    c.update();
    let need = c.grapheme_viewport(0, 0, None);
    assert_eq!(need as usize, family.len());
    let mut buf = vec![0u8; need as usize];
    assert_eq!(c.grapheme_viewport(0, 0, Some(&mut buf)), need);
    assert_eq!(std::str::from_utf8(&buf).unwrap(), family);
    // too-small buffer still reports the required size and writes nothing
    let mut small = [0u8; 2];
    assert_eq!(c.grapheme_viewport(0, 0, Some(&mut small)), need);
}

#[test]
fn osc8_hyperlink_encodes_uri_and_ids_nul_separated() {
    let mut c = Core::new(30, 3, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b]8;id=abc;https://example.org/x\x1b\\link\x1b]8;;\x1b\\");
    c.update();
    let need = c.hyperlink_viewport(0, 0, None);
    let mut buf = vec![0u8; need as usize];
    c.hyperlink_viewport(0, 0, Some(&mut buf));
    let parts: Vec<&[u8]> = buf.split(|b| *b == 0).collect();
    assert_eq!(parts[0], b"https://example.org/x");
    assert_eq!(parts[1], b"abc");
    // a cell with no link reports 0
    assert_eq!(c.hyperlink_viewport(0, 10, None), 0);
}

#[test]
fn soft_wrapped_rows_report_wrap_flags() {
    let mut c = Core::new(10, 4, 100, 0, 0, 0).unwrap();
    c.write(b"aaaaaaaaaabbbbb"); // 15 chars wrap at col 10
    c.update();
    assert_eq!(c.row_wraps(0) & 1, 1);
    assert_eq!(c.row_wraps(1) & 2, 2);
    assert_eq!(c.row_wraps(99), -1);
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test --test scrollback` → FAIL (`no method named scrollback_count`).

- [ ] **Step 3: Implement `src/scrollback.rs` and the `Core` methods**

Use `terminal.screens.active().pages` (the `PageList`), `get_top_left(point::Tag::Active)`, `Pin::up(n)`, `Pin::cells(...)`, `Pin::style(&cell)`, `Pin::grapheme(&cell)`, and the page `lookup_hyperlink` / `hyperlink_set` APIs. Discover exact names by reading `crates/rustty-vt/src/pagelist/pin.rs`, `pagelist/access.rs` and `page.rs`; the Zig originals are the line ranges above. `scrollback_line` builds a `CellFields` per cell and calls `encode_cell`; palette for history rows is `terminal.colors.palette.current` (note: viewport uses the render state's palette, history uses the terminal's — keep that asymmetry, it is in the Zig source at line 1478).

Add the ten exports; each takes `ptr: usize` first and an optional `buf_ptr: usize`/`buf_len: u32` pair (`buf_ptr == 0` ⇒ `None`, length query). Append names to `REQUIRED`.

- [ ] **Step 4: Run to verify pass** — `cargo test --test scrollback` → PASS (5 tests).

- [ ] **Step 5: Full check and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && tools/build-wasm.sh
git add -A && git commit -m "feat: grapheme, hyperlink, scrollback and wrap-metadata exports

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: all green; `ok: … 0 imports`.

---

## Task 4: Effects — responses, title, directory, clipboard, bell, shell state, colours, prompts, tracked positions

**Files:**
- Create: `src/sinks.rs`, `src/effects.rs`, `src/positions.rs`, `tests/sinks.rs`, `tests/responses.rs`, `tests/effects.rs`
- Modify: `src/core.rs`, `src/lib.rs`, `src/exports.rs`, `tools/audit-wasm.mjs`

**Interfaces:**
- Consumes: `Core::{new,write,update,terminal,terminal_mut}`.
- Produces (pure, `src/sinks.rs`): `ResponseQueue::{push(&mut self,&[u8]), pop(&mut self,&mut [u8])->u32}` (256 slots × 64 bytes, drop-newest when full, oversize ignored, `pop` into too-small buffer returns 0 and keeps the entry); `TitleState`, `DirectoryState`, `ClipboardState`, `ShellState` with `set`/`marker` and `changed` flags exactly as the Zig structs (lines 84–154); `Core` methods `read_response(&mut self, out:&mut [u8])->u32`, `title(&mut self)->Option<&[u8]>` (consume-on-read), `working_directory()`, `bell_count()` (read-and-clear), `clipboard_write()`, `shell_phase()->i32`, `shell_exit_code()->f64`, `synchronized_output_generation()->u32`, `color_override(target:u8)->i32`, `set_theme_colors(&mut self, &[u32;19])`, `find_prompt(row:u32, direction:i32)->f64`, `track_position(row,col)->u32`, `resolve_position(id)->f64`, `release_position(id)`, `get_row_wraps` already from Task 3.
- Exports: `read_response, get_title_len, get_title_ptr, get_working_directory_len, get_working_directory_ptr, get_bell_count, get_clipboard_write_len, get_clipboard_write_ptr, get_shell_phase, get_shell_exit_code, synchronized_output_generation, get_color_override, set_theme_colors, find_prompt, track_position, resolve_position, release_position`.

Mechanism: Rust's `stream_terminal::Effects` carries plain `fn` pointers with **no user-data parameter**, so callbacks reach the sinks through a single-threaded module-level pointer: `thread_local!` is unavailable-in-spirit on wasm, so use `static mut ACTIVE: *mut Sinks` set by `Core::write` around `stream.next_slice` and cleared after (a `struct ActiveGuard` whose `Drop` clears it). Callbacks read `ACTIVE`; if null they do nothing. Document that wasm32-unknown-unknown is single-threaded.

Behaviours to reproduce from `wasm_api.zig` (line numbers there):
- **DA1** → `"\x1b[?1;2c"` only for primary (lines 332–338). Do not widen.
- **DSR** operating status → `"\x1b[0n"`; cursor position → `"\x1b[{row+1};{col+1}R"` (339–352).
- **DECRQM** → `"\x1b[{?}{mode};{1|2}$y"` with `?` only for DEC private modes; unknown mode → `…;0$y` (353–382). Mode state read from the same `terminal.modes` the set/reset path writes.
- **Kitty keyboard query** → `"\x1b[?{flags}u"` (383–391).
- **OSC 10/11 query** → `"\x1b]{10|11};rgb:RRRR/GGGG/BBBB{terminator}"` with each channel `value * 257` as 4 hex digits, echoing the request's terminator (BEL or ST) (401–446). OSC 110/111/112 reset clears the override (408–418).
- **Title**: ≥ 256 bytes ignored (173–184). **PWD (OSC 7)**: > 2047 bytes or invalid UTF-8 ignored, identical value does not set `changed` (93–105). **Clipboard (OSC 52)**: only selector `c`, never `?`, base64 must decode, ≤ 65 536 decoded bytes, valid UTF-8; the latest valid write wins (135–154).
- **Bell** saturating counter, read-and-clear. **Synchronized output** generation increments only on the off→on transition of mode 2026, for both `set_mode` and `restore_mode` (311–330).
- **Shell state** from OSC 133 markers, primary screen only; full reset sets `changed` with phase 0 (110–131, 304–308).
- **Tracked positions**: 64 slots, ids start at 1 and never alias a released/invalidated slot, invalidated when the active screen changes or the pin is garbage; `resolve` returns `row * 65536 + col` as `f64`, `-1` when invalid (156–169, 1369–1406). Full reset and any screen switch clear all positions.
- **`find_prompt`**: primary screen only, direction ±1, uses the page list's prompt iterator and collapses continuation rows (1331–1367).

- [ ] **Step 1: Write the failing sink unit tests** (`tests/sinks.rs`)

```rust
use rustty_wterm::sinks::{ClipboardState, DirectoryState, ResponseQueue, TitleState};

#[test]
fn queue_is_fifo_and_drops_the_newest_when_full() {
    let mut q = ResponseQueue::default();
    for i in 0..256u32 {
        q.push(format!("r{i}").as_bytes());
    }
    q.push(b"overflow"); // dropped
    let mut out = [0u8; 64];
    let n = q.pop(&mut out);
    assert_eq!(&out[..n as usize], b"r0");
    for _ in 1..256 { assert!(q.pop(&mut out) > 0); }
    assert_eq!(q.pop(&mut out), 0); // "overflow" never entered
}

#[test]
fn oversize_response_is_ignored_and_small_output_buffer_keeps_the_entry() {
    let mut q = ResponseQueue::default();
    q.push(&[b'x'; 65]);
    let mut out = [0u8; 64];
    assert_eq!(q.pop(&mut out), 0);
    q.push(b"hello");
    let mut tiny = [0u8; 2];
    assert_eq!(q.pop(&mut tiny), 0);
    assert_eq!(q.pop(&mut out), 5);
}

#[test]
fn title_of_256_bytes_is_ignored_255_is_kept() {
    let mut t = TitleState::default();
    t.set(&vec![b'a'; 256]);
    assert!(!t.changed);
    t.set(&vec![b'a'; 255]);
    assert!(t.changed && t.len == 255);
}

#[test]
fn directory_ignores_invalid_utf8_and_unchanged_values() {
    let mut d = DirectoryState::default();
    d.set(&[0xff, 0xfe]);
    assert!(!d.changed);
    d.set(b"file:///home/user");
    assert!(d.changed);
    d.changed = false;
    d.set(b"file:///home/user");
    assert!(!d.changed);
}

#[test]
fn clipboard_accepts_only_c_selector_valid_base64_utf8() {
    let mut c = ClipboardState::default();
    c.set(b'c', b"?");               assert!(!c.changed);
    c.set(b'p', b"aGk=");            assert!(!c.changed);
    c.set(b'c', b"!!notbase64!!");   assert!(!c.changed);
    c.set(b'c', b"/w==");            assert!(!c.changed); // 0xff: not UTF-8
    c.set(b'c', b"aGk=");            assert!(c.changed && &c.bytes[..c.len as usize] == b"hi");
}
```

Run `cargo test --test sinks` → FAIL; implement `src/sinks.rs` as straight ports of the Zig structs (base64 via a small decoder written in-crate — no new dependencies); re-run → PASS.

- [ ] **Step 2: Write the failing response/effects tests through a real terminal** (`tests/responses.rs`, `tests/effects.rs`)

```rust
use rustty_wterm::Core;

fn reply(c: &mut Core) -> Vec<Vec<u8>> {
    let mut all = vec![];
    loop {
        let mut b = [0u8; 64];
        let n = c.read_response(&mut b);
        if n == 0 { break; }
        all.push(b[..n as usize].to_vec());
    }
    all
}

#[test]
fn da1_dsr_and_cpr_replies_are_the_exact_zig_bytes() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b[c\x1b[5n\x1b[3;7H\x1b[6n");
    assert_eq!(reply(&mut c), vec![b"\x1b[?1;2c".to_vec(), b"\x1b[0n".to_vec(), b"\x1b[3;7R".to_vec()]);
}

#[test]
fn decrqm_reports_set_reset_and_unknown() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b[?2004h\x1b[?2004$p\x1b[?25l\x1b[?25$p\x1b[?9999$p");
    assert_eq!(reply(&mut c), vec![b"\x1b[?2004;1$y".to_vec(), b"\x1b[?25;2$y".to_vec(), b"\x1b[?9999;0$y".to_vec()]);
}

#[test]
fn osc11_query_answers_with_the_configured_background_and_echoes_terminator() {
    let mut c = Core::new(80, 24, 100, 0xd4d4d4, 0x1e1e1e, 0).unwrap();
    c.write(b"\x1b]11;?\x07\x1b]10;?\x1b\\");
    assert_eq!(reply(&mut c), vec![
        b"\x1b]11;rgb:1e1e/1e1e/1e1e\x07".to_vec(),
        b"\x1b]10;rgb:d4d4/d4d4/d4d4\x1b\\".to_vec(),
    ]);
}

#[test]
fn kitty_keyboard_query_reports_flags() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b[>1u\x1b[?u");
    assert_eq!(reply(&mut c), vec![b"\x1b[?1u".to_vec()]);
}

#[test]
fn queries_split_across_writes_still_answer_once() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b[");
    c.write(b"c");
    assert_eq!(reply(&mut c).len(), 1);
}
```

`tests/effects.rs`:

```rust
use rustty_wterm::Core;

#[test]
fn title_is_consume_on_read() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x1b]2;hello\x07");
    assert_eq!(c.title().map(|t| t.to_vec()), Some(b"hello".to_vec()));
    assert!(c.title().is_none());
}

#[test]
fn bell_counts_and_clears_and_osc_terminating_bel_does_not_count() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"\x07\x07\x1b]2;t\x07");
    assert_eq!(c.bell_count(), 2);
    assert_eq!(c.bell_count(), 0);
}

#[test]
fn synchronized_output_generation_bumps_only_on_off_to_on() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    let g0 = c.synchronized_output_generation();
    c.write(b"\x1b[?2026h\x1b[?2026h");
    assert_eq!(c.synchronized_output_generation(), g0 + 1);
    c.write(b"\x1b[?2026l\x1b[?2026h");
    assert_eq!(c.synchronized_output_generation(), g0 + 2);
}

#[test]
fn shell_phase_follows_osc133_and_exit_code_is_nan_when_unset() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    assert_eq!(c.shell_phase(), -1);
    c.write(b"\x1b]133;A\x07");           assert_eq!(c.shell_phase(), 1); // prompt
    c.write(b"\x1b]133;B\x07");           assert_eq!(c.shell_phase(), 2); // input
    c.write(b"\x1b]133;C\x07");           assert_eq!(c.shell_phase(), 3); // running
    c.write(b"\x1b]133;D;7\x07");         assert_eq!(c.shell_phase(), 4); // complete
    assert_eq!(c.shell_exit_code(), 7.0);
}

#[test]
fn osc_colour_override_set_then_reset() {
    let mut c = Core::new(80, 24, 100, 0xd4d4d4, 0x1e1e1e, 0).unwrap();
    assert_eq!(c.color_override(1), -1);
    c.write(b"\x1b]11;rgb:ff/00/00\x07");
    assert_eq!(c.color_override(1), 0xff0000);
    c.write(b"\x1b]111\x07");
    assert_eq!(c.color_override(1), -1);
}

#[test]
fn tracked_position_follows_scroll_and_dies_on_screen_switch() {
    let mut c = Core::new(10, 3, 1000, 0, 0, 0).unwrap();
    c.write(b"abc");
    let id = c.track_position(0, 1);
    assert_ne!(id, 0);
    assert_eq!(c.resolve_position(id), 0.0 * 65536.0 + 1.0);
    c.write(b"\r\n\r\n\r\n\r\n");              // scroll: row 0 stays row 0 in screen coords
    assert_eq!(c.resolve_position(id), 1.0);
    c.write(b"\x1b[?1049h");
    assert_eq!(c.resolve_position(id), -1.0);
    c.release_position(id);
    assert_eq!(c.track_position(99_999, 0), 0);  // out of range
}

#[test]
fn position_table_holds_64_then_refuses() {
    let mut c = Core::new(10, 3, 1000, 0, 0, 0).unwrap();
    let ids: Vec<u32> = (0..64).map(|_| c.track_position(0, 0)).collect();
    assert!(ids.iter().all(|i| *i != 0));
    assert_eq!(c.track_position(0, 0), 0);
    c.release_position(ids[0]);
    let again = c.track_position(0, 0);
    assert!(again != 0 && !ids.contains(&again)); // ids never alias
}
```

- [ ] **Step 3: Run to verify failure** — `cargo test --test responses --test effects` → FAIL (`no method named read_response`).

- [ ] **Step 4: Implement `src/effects.rs`, `src/positions.rs` and wire into `Core`**

Set on the stream's handler (`self.stream.handler.effects`; the field is public in `stream_terminal.rs`): `write_pty` (queue push — covers DSR/CPR/DECRQM/kitty-keyboard replies if Rustty's handler already formats them; **if it does, do not also format them in our code**, or replies double), `bell`, `title_changed`, `pwd_changed`, `clipboard_write`, `device_attributes` (return the `Attributes` whose encoding is exactly `\x1b[?1;2c`), `semantic_prompt`, `render_hold` (sync-output generation), `color_scheme: None`, `xtversion: None`, `size: None`, `enquiry: None` (the Zig layer answers none of these; unanswered queries must stay unanswered or the differential test diverges). Where an action is not an effect in Rustty (OSC 10/11 queries, OSC 110/111/112 override resets), find where the handler dispatches it (`crates/rustty-vt/src/stream_terminal/dispatch.rs`, `reports.rs`) and use the closest hook; if none exists, **stop and record the gap in `README.md` under "Known gaps" and add the case to `harness/known-diffs.json` in Task 6** rather than patching `rustty-vt`.

Port `PositionTracker` with `PageList::track_pin`/`untrack_pin`/`point_from_pin` (Rust names in `pagelist/pin.rs` lines 60, 98, 134).

- [ ] **Step 5: Run to verify pass** — `cargo test` → all PASS.

- [ ] **Step 6: Add exports, extend `REQUIRED`, build the wasm**

`get_title_len` returns `i32` (`-1` = unchanged, consuming on read) and `get_title_ptr` returns the address of the title bytes inside `Core` (stable while the `Core` lives); same pair for working directory and clipboard. `get_shell_exit_code` returns `f64`. Run `tools/build-wasm.sh` → `ok`.

- [ ] **Step 7: Commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add -A && git commit -m "feat: terminal responses, title, cwd, clipboard, bell, shell state, colours, prompts, tracked positions

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 5: Kitty graphics exports (with a decision gate)

**Files:**
- Create: `src/graphics.rs`, `tests/graphics.rs`
- Modify: `src/core.rs`, `src/effects.rs`, `src/exports.rs`, `Cargo.toml`, `tools/audit-wasm.mjs`, `README.md`

**Interfaces:**
- Consumes: `Core`, the APC path in `Effects`/handler.
- Produces: `Core::graphics_generation()`, `graphics_image_count()`, `graphics_placement_count()`, `graphics_bytes_used()`, `graphics_capacity()`, `graphics_rejected()`, `graphics_evicted()`, `graphics_images(out:&mut [u8], max_records:u32)->u32` (16-byte records: id, version, width, height), `graphics_placements(out, max)->u32` (60-byte records: image_id, placement_id, version, row, col, x_off, y_off, src_x, src_y, src_w, src_h, columns, rows, z(i32), placement_tag), `graphics_image_size(id, version)->u32`, `graphics_image_copy(id, version, out)->u32` (RGBA). Exports of the same names, plus `GRAPHICS_*` constants.

Reference: `wasm_api.zig` lines 20–25 and 513–681 and 1226–1313. Rules: only **direct** transmission is accepted (file/shared-memory media are rejected and counted in `rejected`); an APC > 48 MiB is rejected and counted; image version = low 31 bits of `transmit_time` (never 0) OR'd with the active-screen key in bit 31; `generation` bumps only when the fingerprint of visible image state changes (cheap, order-independent, no pixel scan); `image_copy` converts RGB→RGBA with alpha 255 and refuses mismatched sizes.

- [ ] **Step 1: Decision gate — does kitty-graphics build on wasm?**

Run: `cargo build --release --target wasm32-unknown-unknown 2>&1 | tail -30`
Expected outcomes:
  - **A. Builds with `kitty-graphics` and zero imports** → continue with Step 2.
  - **B. Fails or adds imports (zlib/wuffs/libc)** → Rustty's own wasm build turns it off (`.port/tools/build-vt-wasm.sh`: "Kitty graphics off on this freestanding target"). Remove `kitty-graphics` from `Cargo.toml` features, **omit** the `graphics_*` exports (wterm treats every `graphics_*` export as optional — `GhosttyExports` marks them `?` — so images are simply unsupported), delete Steps 2–6, write "Kitty graphics: not supported by the Rust build (no PNG/zlib on wasm32-unknown-unknown); `printf` image demo is disabled for this engine" in `README.md`, add the string `"graphics"` to a `NOT_EXPORTED` list in `tools/audit-wasm.mjs`, and skip the image demo script for this engine in Task 8. Commit and go to Task 6.

Record which outcome happened in the commit message.

- [ ] **Step 2: Write the failing tests** (outcome A)

`tests/graphics.rs` — a 2×1 RGB image sent as a direct, uncompressed kitty command (`f=24,s=2,v=1`, payload `AQIDBAUG` = bytes `01 02 03 04 05 06`):

```rust
use rustty_wterm::Core;

const IMG: &[u8] = b"\x1b_Ga=T,f=24,s=2,v=1,i=7;AQIDBAUG\x1b\\";

#[test]
fn direct_rgb_image_is_stored_and_placed() {
    let mut c = Core::new(20, 5, 100, 0, 0, 32 * 1024 * 1024).unwrap();
    let g0 = c.graphics_generation();
    c.write(IMG);
    assert_eq!(c.graphics_image_count(), 1);
    assert_eq!(c.graphics_placement_count(), 1);
    assert_ne!(c.graphics_generation(), g0);
    let mut rec = [0u8; 16];
    assert_eq!(c.graphics_images(&mut rec, 1), 1);
    assert_eq!(u32::from_le_bytes(rec[0..4].try_into().unwrap()), 7);
    let (w, h) = (u32::from_le_bytes(rec[8..12].try_into().unwrap()), u32::from_le_bytes(rec[12..16].try_into().unwrap()));
    assert_eq!((w, h), (2, 1));
}

#[test]
fn image_copy_expands_rgb_to_rgba_and_checks_version() {
    let mut c = Core::new(20, 5, 100, 0, 0, 32 * 1024 * 1024).unwrap();
    c.write(IMG);
    let mut rec = [0u8; 16];
    c.graphics_images(&mut rec, 1);
    let ver = u32::from_le_bytes(rec[4..8].try_into().unwrap());
    assert_eq!(c.graphics_image_size(7, ver), 8);
    let mut px = [0u8; 8];
    assert_eq!(c.graphics_image_copy(7, ver, &mut px), 8);
    assert_eq!(px, [1, 2, 3, 255, 4, 5, 6, 255]);
    assert_eq!(c.graphics_image_size(7, ver ^ 1), 0);      // stale version
    assert_eq!(c.graphics_image_copy(7, ver, &mut [0u8; 4]), 0); // short buffer
}

#[test]
fn file_medium_is_rejected_and_counted_not_executed() {
    let mut c = Core::new(20, 5, 100, 0, 0, 32 * 1024 * 1024).unwrap();
    c.write(b"\x1b_Ga=T,t=f,f=24,s=1,v=1;L2V0Yy9wYXNzd2Q=\x1b\\"); // path "/etc/passwd"
    assert_eq!(c.graphics_image_count(), 0);
    assert_eq!(c.graphics_rejected(), 1);
}

#[test]
fn plain_text_does_not_bump_generation_and_zero_limit_disables_images() {
    let mut c = Core::new(20, 5, 100, 0, 0, 0).unwrap();
    let g = c.graphics_generation();
    c.write(b"just text\r\n");
    assert_eq!(c.graphics_generation(), g);
    c.write(IMG);
    assert_eq!(c.graphics_image_count(), 0);
}

#[test]
fn oversized_apc_is_rejected_without_growing_memory_unboundedly() {
    let mut c = Core::new(20, 5, 100, 0, 0, 32 * 1024 * 1024).unwrap();
    c.write(b"\x1b_Ga=T,f=24,s=1,v=1;");
    let chunk = vec![b'A'; 1024 * 1024];
    for _ in 0..50 { c.write(&chunk); }   // 50 MiB > 48 MiB cap
    c.write(b"\x1b\\");
    assert_eq!(c.graphics_image_count(), 0);
    assert!(c.graphics_rejected() >= 1);
}
```

- [ ] **Step 3: Run to verify failure** — `cargo test --test graphics` → FAIL (`no method named graphics_generation`).

- [ ] **Step 4: Implement `src/graphics.rs`** following the Zig lines above (storage lives on `terminal.screens.active().kitty_images`; iteration of `images` and `placements`; `point_from_pin(Tag::Screen, pin)` for placement coordinates). Track `graphics_generation`, `graphics_fingerprint`, `graphics_screen`, `rejected_images` in `Core` and call a `refresh_graphics_generation()` at the end of `write` and `resize`, gated on `storage.dirty || screen changed` (cheap path). Intercept APC start/put/end to enforce the direct-medium and 48 MiB rules **before** invoking the executor (Zig lines 265–281, 447–478); if Rustty's stream does not expose an APC interception point, enforce the medium rule via the `Limits::DIRECT` option already passed in `Core::new` (this is the Rust equivalent of rejecting non-direct media) and enforce the size cap in `Core::write` by tracking APC byte counts with a tiny state machine over the incoming bytes.

- [ ] **Step 5: Run to verify pass** — `cargo test` → PASS.

- [ ] **Step 6: Exports, audit list, build, commit**

```bash
tools/build-wasm.sh
git add -A && git commit -m "feat: kitty graphics exports (direct media only); decision gate outcome A

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 6: Differential harness against `ghostty-vt.wasm`

**Files:**
- Create: `harness/package.json`, `harness/diff.mjs`, `harness/corpus/gen.mjs`, `harness/known-diffs.json`, `harness/lib/load-core.mjs`, `harness/README.md`
- Modify: `README.md`, `tools/build-wasm.sh` (optional `--diff` flag)

**Interfaces:**
- Consumes: the built `rustty_wterm.wasm`; the reference `ghostty-vt.wasm` shipped in `@wterm/ghostty@0.5.4`.
- Produces: `node harness/diff.mjs` — exit 0 when every corpus entry produces identical observable state in both engines (modulo `known-diffs.json`); prints `corpus entries: N, identical: M, known: K`. `harness/lib/load-core.mjs` exports `async loadEngine(wasmPath, opts) -> GhosttyCore` (constructs `GhosttyCore` from raw bytes — Node 22's `fetch` cannot load `file:` URLs).

The harness drives **wterm's own unmodified `GhosttyCore`** for both engines, so it also proves the Rust build satisfies the JS adapter, not just the raw ABI.

- [ ] **Step 1: Package and loader**

`harness/package.json`:

```json
{
  "name": "rustty-wterm-harness",
  "private": true,
  "type": "module",
  "dependencies": { "@wterm/core": "0.5.4", "@wterm/ghostty": "0.5.4" }
}
```

`harness/lib/load-core.mjs`:

```js
import { readFileSync } from "node:fs";
import { GhosttyCore } from "@wterm/ghostty";

/** Instantiates a wasm file and wraps it in wterm's own GhosttyCore (public constructor). */
export async function loadEngine(path, options = {}) {
  const { instance } = await WebAssembly.instantiate(readFileSync(path), { env: { log() {} } });
  return new GhosttyCore({ exports: instance.exports, instance }, options);
}
```

Run: `cd harness && npm install` — expected: installs 2 packages, no errors.

- [ ] **Step 2: Write the corpus generator** (`harness/corpus/gen.mjs`) emitting `corpus/<name>.bin` files. Entries (each a byte string; keep them small and deterministic):

`plain` (text + CRLF), `sgr-basic` (all 8/16 colours, bold/faint/italic/blink/inverse/invisible/strike), `sgr-256-truecolor` (`38;5;n`, `38;2;r;g;b`, `48;…`), `underlines` (`4:1`…`4:5`, `58;2;…`, `59`), `wide-cjk`, `emoji-zwj` (family, flags, skin tone), `combining` (e + U+0301), `wrap-and-reflow` (long line then resize in the diff step), `scrollback-120` (120 lines), `alt-screen` (`?1049h` draw `?1049l`), `cursor-moves` (CUP/CUU/ECH/DECSC/DECRC), `scroll-region` (DECSTBM + IND/RI), `tabs`, `osc8` (links with and without `id=`), `osc-title-pwd` (OSC 0/2/7), `osc-colors` (OSC 4/10/11/12 set, query, reset 104/110/111/112), `queries` (DA1, DSR, CPR, DECRQM sweep over 40 modes, kitty keyboard query), `mouse-modes` (`?1000h`, `?1002h`, `?1003h`, `?1006h`, `?1016h`), `bracketed-paste-focus-sync` (`?2004h ?1004h ?2026h`), `osc52` (valid, invalid, `?`, oversize), `bell`, `osc133` (A/B/C/D;n cycle), `decscusr` (cursor shapes), `kitty-image-rgb` (the 2×1 image; skipped when Task 5 outcome is B), `chunked-split` (every entry also run split at every byte boundary — see Step 3), and `random-fuzz-<seed>` (1 000 seeded pseudo-random mixes of the above fragments, 20 seeds).

- [ ] **Step 3: Write `diff.mjs`**

For each entry and each of three chunkings (whole, byte-by-byte, 7-byte chunks), create both engines at 80×24 with scrollback 1000 and fg `#d4d4d4` / bg `#1e1e1e`, feed identical bytes, then compare:

```js
function snapshot(core) {
  core.writeString(""); // no-op, ensures update path
  const rows = core.getRows(), cols = core.getCols();
  const grid = [];
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) grid.push(core.getCell(r, c));
  const sb = [];
  for (let o = 0; o < core.getScrollbackCount(); o++) {
    const n = core.getScrollbackLineLen(o);
    for (let c = 0; c < n; c++) sb.push(core.getScrollbackCell(o, c));
  }
  const responses = []; for (let r; (r = core.getResponse()) !== null; ) responses.push(r);
  return {
    grid, sb, responses,
    cursor: core.getCursor(), title: core.getTitle(), cwd: core.getWorkingDirectory?.(),
    bells: core.getBellCount?.(), clipboard: core.getClipboardWrite?.(),
    shell: core.getShellIntegrationState?.(), overrides: core.getColorOverrides?.(),
    modes: [core.cursorKeysApp(), core.bracketedPaste(), core.usingAltScreen(),
            core.mouseTracking?.(), core.mouseEncoding?.(), core.focusEvents?.(),
            core.synchronizedOutput?.(), core.kittyKeyboardFlags?.()],
    wraps: Array.from({ length: rows }, (_, r) => core.getRowMetadata?.(r)),
    scrollbackCount: core.getScrollbackCount(),
  };
}
```

Compare with `assert.deepStrictEqual`-style structural diff; on mismatch print the entry name, chunking, and the first differing path (e.g. `grid[412].fgRgb: 0xff0000 != 0xcc0000`). Before comparing, call `core.isDirtyRow`-agnostic reads only (the snapshot above uses no dirty state). An entry in `known-diffs.json` has the shape `{ "entry": "osc-colors", "path": "responses[2]", "reason": "…", "owner": "rustty-vt|wterm|skew" }`; a known diff is skipped only if both `entry` and `path` match; an unused known-diff entry **fails** the run (so stale allowances cannot hide forever).

- [ ] **Step 4: Run it and triage**

Run: `tools/build-wasm.sh && node harness/diff.mjs --rust target/wasm32-unknown-unknown/release/rustty_wterm.wasm --zig harness/node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm`
Expected first run: failures. For each, classify:
  1. **Our bug** (Task 1–5 code wrong) → fix in Rust with a new regression test beside the originating task's tests, rerun.
  2. **Version skew** (wterm's libghostty differs from Rustty's oracle commit `246f70287`; check by reading the relevant `ghostty` source at each commit) → add to `known-diffs.json` with `"owner": "skew"` and the commit evidence in `reason`.
  3. **Rustty engine difference** (a real divergence in `rustty-vt`) → do **not** patch Rustty; add to `known-diffs.json` with `"owner": "rustty-vt"` and open a note in `README.md` "Findings" (this is publishable material for the article).
Never add a known diff without a reason that names evidence.

- [ ] **Step 5: Wire a one-command check**

Add to `README.md`: `tools/check.sh` running `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `tools/build-wasm.sh`, `node harness/diff.mjs …`. Create it, `chmod +x`, run it, expect exit 0.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "test: differential harness against ghostty-vt.wasm via wterm's own GhosttyCore

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 7: Real-program corpus and robustness

**Files:**
- Create: `harness/corpus/record.sh`, `harness/corpus/real-*.bin`, `harness/robustness.mjs`, `tests/hostile.rs`
- Modify: `harness/diff.mjs` (include `real-*`), `README.md`

**Interfaces:**
- Consumes: Task 6 harness. Produces: real-world recordings in the corpus and a robustness suite that proves no export aborts on hostile input.

- [ ] **Step 1: Record real programs** (macOS, deterministic where possible) with `script -q /dev/null` piped through a fixed 80×24 pty (`stty cols 80 rows 24`), writing to `harness/corpus/real-<name>.bin`: `ls -la --color=always` of a fixed fixture directory (`harness/corpus/fixture/`), `git log --oneline --decorate --color=always -n 30` of a fixed tiny repo created by `record.sh`, `vim -u NONE -c 'set nu' fixture/a.txt -c 'normal 3Gdd' -c ':q!'` driven by a here-doc of keystrokes, `htop`-like: `top -l 1 -n 5` (ANSI-light), `bat`/`less -R` if installed (skip silently if not). Commit the `.bin` files (they are small; refuse any > 64 KB).

- [ ] **Step 2: Write the failing hostile-input Rust tests** (`tests/hostile.rs`)

```rust
use rustty_wterm::Core;

#[test]
fn resize_to_zero_or_huge_never_aborts() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.resize(0, 0);
    c.resize(u16::MAX, u16::MAX);
    c.resize(1, 1);
    c.write(b"still alive");
    assert_eq!((c.cols(), c.rows()), (1, 1));
}

#[test]
fn garbage_bytes_never_abort() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    let mut x = 0x2545F4914F6CDD1Du64;
    let mut buf = vec![0u8; 1 << 16];
    for _ in 0..64 {
        for b in buf.iter_mut() { x ^= x << 13; x ^= x >> 7; x ^= x << 17; *b = x as u8; }
        c.write(&buf);
        c.update();
    }
}

#[test]
fn viewport_read_after_shrink_is_total() {
    let mut c = Core::new(80, 24, 100, 0, 0, 0).unwrap();
    c.write(b"x\r\n".repeat(100).as_slice());
    c.update();
    c.resize(5, 2);
    c.update();
    let mut buf = vec![0u8; 5 * 2 * 20];
    assert_eq!(c.viewport(&mut buf, 20), 10);
}
```

Run `cargo test --test hostile`. Expected: PASS if the implementation is total, otherwise FAIL — fix the panic site (never `catch_unwind`: `panic = "abort"` on wasm).

- [ ] **Step 3: Write `harness/robustness.mjs`** — against the wasm through the raw exports: call every export with `ptr = 0`, with a freed pointer **excluded** (undefined behaviour, documented), with `u32::MAX` offsets/lengths and null buffers; assert none traps (`WebAssembly.RuntimeError`). Run: `node harness/robustness.mjs target/wasm32-unknown-unknown/release/rustty_wterm.wasm` → exit 0.

- [ ] **Step 4: Run the full harness including real recordings** — `node harness/diff.mjs …` → exit 0 with `known-diffs.json` fully used. Triage new failures as in Task 6 Step 4.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "test: real-program recordings and hostile-input robustness

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 8: Website worktree, dependencies, wasm sync, session and demo scripts

**Files (website worktree):**
- Create: `scripts/porting/sync-wterm-wasm.mjs`, `public/porting/wterm/*`, `src/components/porting/live-terminal/{engines.ts,session.ts,demo-scripts.ts}`, `scripts/verify-porting-terminal.mjs`
- Modify: `package.json`, `package-lock.json`

**Interfaces:**
- Produces:
  - `engines.ts`: `export type EngineId = "rustty" | "ghostty"; export const ENGINES: readonly { id: EngineId; label: string; wasmPath: string }[]` with `wasmPath` `"/porting/wterm/rustty-wterm.wasm"` and `"/porting/wterm/ghostty-vt.wasm"`.
  - `session.ts`: `export function seedFiles(): Record<string, string>`, `export const ENV: Record<string, string>`, `export const GREETING: string[]`, `export function prompt(cwd: string): string`, `export class OutputLog { push(chunk: string): void; text(): string; clear(): void; readonly maxBytes: number }` (ring buffer, default 512 KiB, drops oldest whole chunks).
  - `demo-scripts.ts`: `export const DEMO_SCRIPTS: Record<string, string>` mapping `/home/user/demo/<name>.sh` to script text.

- [ ] **Step 1: Worktree and branch**

```bash
cd ~/projects/ghostty-website && git worktree add ../ghostty-website-wterm -b wterm-live-terminal b4e4d40d
cd ../ghostty-website-wterm && npm ci
```

Expected: worktree created; `npm ci` succeeds (the plan's later steps run in this directory).

- [ ] **Step 2: Add the dependencies, exact-pinned**

```bash
npm install --save-exact @wterm/core@0.5.4 @wterm/dom@0.5.4 @wterm/ghostty@0.5.4 @wterm/just-bash@0.5.4 @wterm/react@0.5.4 just-bash@3.6.0
git diff package.json
```

Expected: six new entries with no `^`/`~`. (Confirm `just-bash` 3.6.0 is what `@wterm/just-bash@0.5.4` peers on: `npm ls just-bash`; if it dedupes to a different version, pin that one.)

- [ ] **Step 3: Write the failing verify script (invariants first)**

`scripts/verify-porting-terminal.mjs`:

```js
/** Checks the live-terminal widget's pure modules, shipped wasm manifest and lazy-load guard. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { ENGINES } from "../src/components/porting/live-terminal/engines.ts";
import { DEMO_SCRIPTS } from "../src/components/porting/live-terminal/demo-scripts.ts";
import { ENV, OutputLog, prompt, seedFiles } from "../src/components/porting/live-terminal/session.ts";

// Engines: ids unique, each wasm exists on disk under public/ and matches the manifest.
const manifest = JSON.parse(readFileSync("public/porting/wterm/manifest.json", "utf8"));
assert.deepEqual(ENGINES.map((e) => e.id).sort(), ["ghostty", "rustty"]);
for (const e of ENGINES) {
  const file = `public${e.wasmPath}`;
  assert.ok(existsSync(file), `${file} missing`);
  const bytes = readFileSync(file);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), manifest[e.id].sha256, `${e.id} sha256`);
  assert.equal(statSync(file).size, manifest[e.id].bytes, `${e.id} size`);
  assert.ok(bytes.subarray(0, 4).equals(Buffer.from([0, 0x61, 0x73, 0x6d])), `${e.id} is not wasm`);
}

// Seeded files: absolute paths, text only, every demo script present and executable-looking.
const files = seedFiles();
for (const [path, text] of Object.entries(files)) {
  assert.ok(path.startsWith("/home/user/"), `seed outside home: ${path}`);
  assert.equal(typeof text === "string" ? text : text.content, typeof text === "string" ? text : text.content);
}
for (const path of Object.keys(DEMO_SCRIPTS)) {
  assert.ok(path in files, `demo script not seeded: ${path}`);
  assert.ok(String(files[path].content ?? files[path]).startsWith("#!/bin/bash\n"), `${path} lacks shebang`);
}

// Scripts must not reach the network, read outside the demo dir, or run unbounded loops.
for (const [path, text] of Object.entries(DEMO_SCRIPTS)) {
  assert.ok(!/\b(curl|wget|nc|ssh)\b/.test(text), `${path} uses the network`);
  assert.ok(!/while\s+true|for\s*\(\(\s*;;/.test(text), `${path} loops forever`);
  assert.ok(text.length < 4000, `${path} is large`);
}

// Environment and prompt.
assert.equal(ENV.TERM, "xterm-256color");
assert.match(prompt("/home/user"), /\x1b\[1;32m.*\x1b\[0m:.*~/);

// OutputLog: bounded, drops oldest whole chunks, never splits a chunk.
const log = new OutputLog(64);
for (let i = 0; i < 20; i++) log.push(`chunk-${i}-`.padEnd(10, "x"));
assert.ok(log.text().length <= 64);
assert.ok(log.text().endsWith("chunk-19-xx"), "keeps the newest chunk");
assert.ok(/^chunk-\d+-x+/.test(log.text()), "starts on a chunk boundary");
log.clear();
assert.equal(log.text(), "");

console.log("verify-porting-terminal: ok");
```

Run: `node --experimental-strip-types scripts/verify-porting-terminal.mjs` (check how `verify-porting-registry.mjs` imports `.ts` — it imports `../src/components/porting/lib/registry.ts` directly, so the same Node flags work; reuse them). Expected: FAIL — modules/manifest missing.

- [ ] **Step 4: Write `sync-wterm-wasm.mjs`**

```js
/** Copies the two engine binaries into public/porting/wterm and records sizes and hashes. */
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";

const RUSTY = process.argv[2];
if (!RUSTY) throw new Error("usage: sync-wterm-wasm.mjs <path to rustty_wterm.wasm>");
const ZIG = "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm";
const OUT = "public/porting/wterm";
mkdirSync(OUT, { recursive: true });
copyFileSync(RUSTY, `${OUT}/rustty-wterm.wasm`);
copyFileSync(ZIG, `${OUT}/ghostty-vt.wasm`);

const entry = (file, extra) => {
  const bytes = readFileSync(file);
  return { bytes: statSync(file).size, sha256: createHash("sha256").update(bytes).digest("hex"), ...extra };
};
const wterm = JSON.parse(readFileSync("node_modules/@wterm/ghostty/package.json", "utf8")).version;
writeFileSync(`${OUT}/manifest.json`, JSON.stringify({
  rustty: entry(`${OUT}/rustty-wterm.wasm`, { source: "rustty-wterm" }),
  ghostty: entry(`${OUT}/ghostty-vt.wasm`, { source: `@wterm/ghostty@${wterm}` }),
}, null, 2) + "\n");
console.log("synced");
```

Run: `node scripts/porting/sync-wterm-wasm.mjs ~/projects/rustty-wterm/target/wasm32-unknown-unknown/release/rustty_wterm.wasm` → `synced`.

- [ ] **Step 5: Implement the pure modules**

`engines.ts`:

```ts
/** EngineId names the two terminal engines the demo can run. */
export type EngineId = "rustty" | "ghostty";

/** ENGINES lists the selectable engines with the wasm each loads. */
export const ENGINES: readonly { id: EngineId; label: string; wasmPath: string }[] = [
  { id: "rustty", label: "Rust engine (Rustty)", wasmPath: "/porting/wterm/rustty-wterm.wasm" },
  { id: "ghostty", label: "Zig engine (Ghostty)", wasmPath: "/porting/wterm/ghostty-vt.wasm" },
];
```

`session.ts`:

```ts
import { DEMO_SCRIPTS } from "./demo-scripts";

/** ENV is the shell environment the demo starts with. */
export const ENV: Record<string, string> = {
  SHELL: "/bin/bash",
  TERM: "xterm-256color",
  COLORTERM: "truecolor",
  USER: "user",
  HOME: "/home/user",
};

/** GREETING is printed once when a session starts. */
export const GREETING: string[] = [
  "\x1b[1mRustty, in your browser.\x1b[0m This terminal is the Rust engine compiled to WebAssembly.",
  "Try: \x1b[36mls demo\x1b[0m, \x1b[36m./demo/truecolor.sh\x1b[0m, \x1b[36mcat README.md\x1b[0m. Switch engines above.",
  "",
];

/** Returns the colored shell prompt for cwd, with the home directory shown as "~". */
export function prompt(cwd: string): string {
  const shown = cwd.replace(/^\/home\/user/, "~") || "/";
  return `\x1b[1;32muser@rustty\x1b[0m:\x1b[1;34m${shown}\x1b[0m$ `;
}

/** Returns the virtual filesystem the session starts with (absolute paths). */
export function seedFiles(): Record<string, string> {
  const files: Record<string, string> = {
    "/home/user/README.md":
      "# Rustty demo\n\nA bash-like shell (just-bash) running in your browser,\ndrawn by the terminal engine you pick above.\n\nRun the scripts in ./demo to exercise the engine.\n",
  };
  for (const [path, text] of Object.entries(DEMO_SCRIPTS)) files[path] = text;
  return files;
}

/**
 * OutputLog keeps the bytes written to the terminal, bounded, so a switched
 * engine can be replayed to the same screen. It drops whole oldest chunks.
 */
export class OutputLog {
  private chunks: string[] = [];
  private size = 0;

  /** Creates a log that holds at most maxBytes UTF-16 code units. */
  constructor(readonly maxBytes = 512 * 1024) {}

  /** Appends a chunk, dropping the oldest chunks while over the limit. */
  push(chunk: string): void {
    this.chunks.push(chunk);
    this.size += chunk.length;
    while (this.size > this.maxBytes && this.chunks.length > 1) {
      this.size -= this.chunks.shift()!.length;
    }
  }

  /** Returns everything still held, in order. */
  text(): string {
    return this.chunks.join("");
  }

  /** Empties the log. */
  clear(): void {
    this.chunks = [];
    this.size = 0;
  }
}
```

`demo-scripts.ts` — `printf`-only scripts (bash features limited to what just-bash supports: `printf`, `for ((i=0;i<N;i++))`, `seq`, `echo -e`; keep each short):

```ts
/** DEMO_SCRIPTS are shell scripts seeded into /home/user/demo; each prints terminal sequences with printf. */
export const DEMO_SCRIPTS: Record<string, string> = {
  "/home/user/demo/truecolor.sh": `#!/bin/bash
# A 24-bit gradient: one SGR 48;2 per cell.
for ((i = 0; i < 64; i++)); do
  r=$((i * 4)); g=$((255 - i * 4)); b=128
  printf '\\033[48;2;%d;%d;%dm ' "$r" "$g" "$b"
done
printf '\\033[0m\\n'
`,
  "/home/user/demo/styles.sh": `#!/bin/bash
printf '\\033[1mbold\\033[0m \\033[2mfaint\\033[0m \\033[3mitalic\\033[0m \\033[5mblink\\033[0m \\033[7minverse\\033[0m \\033[9mstrike\\033[0m\\n'
printf '\\033[4:1msingle\\033[0m \\033[4:2mdouble\\033[0m \\033[4:3mcurly\\033[0m \\033[4:4mdotted\\033[0m \\033[4:5mdashed\\033[0m\\n'
printf '\\033[4:3m\\033[58;2;255;80;80mcurly red underline\\033[0m\\n'
`,
  "/home/user/demo/unicode.sh": `#!/bin/bash
printf '漢字 かな 한글 — wide cells stay aligned\\n'
printf 'e\\xcc\\x81 (combining)  \\xf0\\x9f\\x91\\xa8\\xe2\\x80\\x8d\\xf0\\x9f\\x91\\xa9\\xe2\\x80\\x8d\\xf0\\x9f\\x91\\xa7 (ZWJ family)\\n'
`,
  "/home/user/demo/links.sh": `#!/bin/bash
printf '\\033]8;;https://ghostty.org\\033\\\\Ghostty\\033]8;;\\033\\\\ and '
printf '\\033]8;;https://www.rust-lang.org\\033\\\\Rust\\033]8;;\\033\\\\ (OSC 8 hyperlinks)\\n'
`,
  "/home/user/demo/title.sh": `#!/bin/bash
printf '\\033]2;set by the shell\\007'
printf 'window title set via OSC 2\\n'
`,
  "/home/user/demo/progress.sh": `#!/bin/bash
# Cursor movement and line rewriting: 20 steps, no sleep needed.
for ((i = 0; i <= 20; i++)); do
  bar=$(printf '%*s' "$i" '' | tr ' ' '#')
  printf '\\r[%-20s] %3d%%' "$bar" "$((i * 5))"
done
printf '\\n'
`,
};
```

(The script bodies are TypeScript template strings: the doubled backslashes become single backslashes in the seeded file. The verify script's `startsWith("#!/bin/bash\n")` check holds.)

If Task 5 outcome was A, append `/home/user/demo/image.sh` printing the 2×1 kitty image from `tests/graphics.rs` (`printf '\033_Ga=T,f=24,s=2,v=1;AQIDBAUG\033\\'`) and add it to `README.md`'s demo list.

- [ ] **Step 6: Run the verify script** — Expected: `verify-porting-terminal: ok`. Add it to `package.json`'s `verify:porting` chain (` && node scripts/verify-porting-terminal.mjs`) using the same invocation style as its siblings.

- [ ] **Step 7: Run lint and commit**

```bash
npm run lint && npm run verify:porting
git add -A && git commit -m "feat(porting): live-terminal dependencies, engine wasm sync, session and demo scripts

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: lint and all `verify:porting` scripts pass.

---

## Task 9: The live terminal widget (start gate, wterm mount, error fallback)

**Files:**
- Create: `src/components/porting/live-terminal/{index.ts,Body.tsx,TerminalView.tsx,LiveTerminal.module.css}`
- Modify: `src/components/porting/ui/WidgetMount.tsx`, `src/components/porting/lib/registry.ts`, `scripts/verify-porting-terminal.mjs`, `next.config.mjs` (only if the build requires it)

**Interfaces:**
- Consumes: `ENGINES`, `EngineId`, `seedFiles`, `ENV`, `GREETING`, `prompt`, `OutputLog` (Task 8).
- Produces: `export default function LiveTerminal(): JSX.Element` from `index.ts`; `TerminalView` props `{ engine: EngineId; log: OutputLog; onReady(): void; onError(e: unknown): void }`; registry slug `"live-terminal"`.

- [ ] **Step 1: Extend the verify script with the lazy-load guard (failing first)**

Append to `scripts/verify-porting-terminal.mjs`:

```js
// Lazy-load guard: nothing wterm-related may be statically imported outside live-terminal/TerminalView.tsx,
// and TerminalView must only be reached through a dynamic import.
import { readdirSync } from "node:fs";
const root = "src";
const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((d) =>
  d.isDirectory() ? walk(`${dir}/${d.name}`) : [`${dir}/${d.name}`]);
for (const file of walk(root).filter((f) => /\.(ts|tsx|mdx)$/.test(f))) {
  const text = readFileSync(file, "utf8");
  const heavy = /from\s+["'](@wterm\/[^"']+|just-bash)["']/.test(text);
  if (heavy) assert.equal(file, "src/components/porting/live-terminal/TerminalView.tsx", `${file} statically imports wterm/just-bash`);
}
const body = readFileSync("src/components/porting/live-terminal/Body.tsx", "utf8");
assert.ok(/import\(\s*["']\.\/TerminalView["']\s*\)/.test(body), "Body must dynamic-import TerminalView");
assert.ok(!/from\s+["']\.\/TerminalView["']/.test(body), "Body must not static-import TerminalView");
```

Run the script → FAIL (`Body.tsx` unreadable).

- [ ] **Step 2: Implement `TerminalView.tsx`** (client; the only file importing wterm)

```tsx
"use client";

import { useEffect, useRef } from "react";
import { ENGINES, type EngineId } from "./engines";
import { ENV, GREETING, prompt, seedFiles, type OutputLog } from "./session";

/** Props for TerminalView. */
type Props = {
  engine: EngineId;
  log: OutputLog;
  /** Called once the terminal is interactive. */
  onReady: () => void;
  /** Called when loading or running the terminal fails. */
  onError: (error: unknown) => void;
};

/**
 * Mounts a wterm terminal driven by the chosen engine and a just-bash shell.
 * It is the only module that imports wterm or just-bash, and it is only loaded
 * through a dynamic import after the visitor starts the demo.
 */
export default function TerminalView({ engine, log, onReady, onError }: Props) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    let term: { destroy?: () => void } | null = null;

    (async () => {
      const [{ WTerm }, { GhosttyCore }, { BashShell }] = await Promise.all([
        import("@wterm/dom"),
        import("@wterm/ghostty"),
        import("@wterm/just-bash"),
      ]);
      const wasmPath = ENGINES.find((e) => e.id === engine)!.wasmPath;
      const core = await GhosttyCore.load({ wasmPath });
      if (disposed || !host.current) return;

      const shell = new BashShell({ files: seedFiles(), env: ENV, greeting: GREETING, prompt });
      const t = new WTerm(host.current, {
        core,
        onData: (data: string) => void shell.handleInput(data),
        onTitle: undefined,
      });
      term = t;
      await t.init();
      if (disposed) return;

      // Replay what the previous engine showed so a switch keeps the screen.
      const replay = log.text();
      if (replay) t.write(replay);
      await shell.attach((chunk: string) => {
        log.push(chunk);
        t.write(chunk);
      });
      onReady();
    })().catch((e) => !disposed && onError(e));

    return () => {
      disposed = true;
      term?.destroy?.();
    };
  }, [engine, log, onReady, onError]);

  return <div ref={host} />;
}
```

(Confirm the exact `WTerm` constructor options (`onData`, `cols`, `rows`, `autoResize`) and teardown method name from `@wterm/dom`'s `wterm.d.ts`: `grep -n "destroy\|dispose\|onData" node_modules/@wterm/dom/dist/wterm.d.ts`. Replace `destroy` with the real method; if none exists, remove the host element's children and drop the reference. Also note: **re-attaching a new `BashShell` on engine switch resets the shell**, so on switch call `shell.attach` only when `log` is empty; otherwise keep a module-level shell in `Body` and pass it down. Choose the second: hold the `BashShell` in a `useRef` in `Body` and pass it as a prop, so filesystem and history survive an engine switch. Update the props and this effect accordingly before running Step 5.)

- [ ] **Step 3: Implement `Body.tsx`**

```tsx
"use client";

import { type ComponentType, lazy, Suspense, useCallback, useMemo, useState } from "react";
import { ENGINES, type EngineId } from "./engines";
import s from "./LiveTerminal.module.css";
import { OutputLog } from "./session";

/** TerminalView is loaded on demand so wterm, just-bash and the wasm stay out of the page until the visitor starts. */
const TerminalView = lazy(() => import("./TerminalView"));

/** Phases of the demo: idle until started, then loading, ready, or failed. */
type Phase = "idle" | "loading" | "ready" | "failed";

/** LiveTerminal is the /porting demo: wterm + just-bash, drawn by a switchable engine. */
export default function LiveTerminal() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [engine, setEngine] = useState<EngineId>("rustty");
  const [error, setError] = useState<string>("");
  const log = useMemo(() => new OutputLog(), []);

  const start = useCallback(() => setPhase("loading"), []);
  const ready = useCallback(() => setPhase("ready"), []);
  const failed = useCallback((e: unknown) => {
    setError(e instanceof Error ? e.message : String(e));
    setPhase("failed");
  }, []);

  if (phase === "idle") {
    return (
      <div className={s.gate}>
        <button type="button" className={s.start} onClick={start}>
          Start the terminal
        </button>
        <p className={s.note}>Loads a shell and the Rust engine (about 2 MB). Nothing runs until you click.</p>
      </div>
    );
  }
  if (phase === "failed") {
    return (
      <div className={s.gate} role="alert">
        <p>The live terminal could not start{error ? `: ${error}` : ""}.</p>
        <button type="button" className={s.start} onClick={() => setPhase("loading")}>
          Try again
        </button>
      </div>
    );
  }
  return (
    <div className={s.frame}>
      <div className={s.bar} role="group" aria-label="Terminal engine">
        {ENGINES.map((e) => (
          <button
            key={e.id}
            type="button"
            aria-pressed={engine === e.id}
            className={engine === e.id ? s.engineOn : s.engine}
            onClick={() => {
              setEngine(e.id);
              setPhase("loading");
            }}
          >
            {e.label}
          </button>
        ))}
      </div>
      <Suspense fallback={<div className={s.loading}>Loading…</div>}>
        <TerminalView engine={engine} log={log} onReady={ready} onError={failed} />
      </Suspense>
    </div>
  );
}
```

The guard in Step 1 expects a literal `import("./TerminalView")` — `lazy(() => import("./TerminalView"))` satisfies it. `ComponentType` import can be dropped if unused (Biome will flag).

- [ ] **Step 4: Styles, `index.ts`, loader entry, registry entry**

`index.ts`: `export { default } from "./Body";`

`WidgetMount.tsx` `LOADERS`: add `"live-terminal": () => import("@/components/porting/live-terminal"),`

`LiveTerminal.module.css` (use the existing CSS custom properties used by `ui.module.css` — read its first 60 lines for token names; terminal area: monospace, `min-height: 22rem`, `max-width: 100%`, `overflow: auto` so a 390 px viewport scrolls inside the frame; `.start` is a large tap target ≥ 44 px; `@media (prefers-reduced-motion: reduce)` removes any transition; wterm's own CSS (`@wterm/react/css` / `terminal.css`) must be imported in `TerminalView` — check `node_modules/@wterm/dom` for the stylesheet entry and import it there, not globally).

`registry.ts` — append (field rules from `verify-porting-registry.mjs`: no internal ids, `deck` repeats no number from its section's prose, `minHeight` ≥ 200):

```ts
  {
    slug: "live-terminal",
    title: "Run Rustty in your browser",
    label: "LIVE TERMINAL",
    source: "rustty-wterm · wterm · just-bash",
    sourceHref: "https://github.com/vercel-labs/wterm",
    caption:
      "A shell running in your browser, drawn by Rustty's terminal engine compiled to WebAssembly. The shell is just-bash and the terminal front end is wterm; the Rust engine speaks the same interface wterm already uses for Ghostty's Zig engine, so the two can be swapped while you type. Nothing is sent to a server.",
    deck: "Press start, run the scripts in the demo folder, then flip the engine and compare. Try ls demo and ./demo/truecolor.sh.",
    summary:
      "An interactive terminal in the page. It runs a bash-like shell in your browser and draws it with either Rustty's Rust engine or Ghostty's Zig engine, both compiled to WebAssembly; you can switch between them without losing the screen.",
    minHeight: { desktop: 520, phone: 520 },
    flagship: false,
    frame: "terminal",
    dataFile: null,
    requiredKeys: [],
  },
```

(The `sourceHref` must be a permalink to a record, per the type comment; once `rustty-wterm` has a public home, change it to that commit permalink. Until then the wterm repo at the pinned commit is the honest primary record: `https://github.com/vercel-labs/wterm/tree/488e9a7344e812e35a951f803618d7767cc4be5b/packages/@wterm/ghostty`.)

- [ ] **Step 5: Build and run the checks**

```bash
node scripts/verify-porting-terminal.mjs   # lazy-load guard passes
npm run lint
npm run build
```

Expected: verify ok; lint clean; `next build` succeeds. If the build fails on `just-bash` (Node-only imports) or on `wterm` CSS, fix in `TerminalView` (dynamic import only) or add the minimal `next.config.mjs` `serverExternalPackages`/webpack `resolve.fallback` entry the error message names. Record any config change in the commit message.

- [ ] **Step 6: MIME-type check** (Review Focus 7)

```bash
npx next start -p 3143 &  PID=$!
curl -sI http://localhost:3143/porting/wterm/rustty-wterm.wasm | grep -i content-type
kill $PID
```

Expected: the file is served (200). If the header is not `application/wasm`, add a `headers()` entry in `next.config.mjs` for `/porting/wterm/:path*.wasm` with `Content-Type: application/wasm` and `Cache-Control: public, max-age=31536000, immutable` is **not** allowed (filenames are not content-hashed); use `max-age=3600`. Verify wterm still loads with the header removed (temporarily serve with `application/octet-stream`) — it must fall back to `arrayBuffer`, per wterm's loader.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(porting): live-terminal widget with start gate, engine switch and error fallback

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 10: Engine switch fidelity and shell persistence

**Files:**
- Modify: `src/components/porting/live-terminal/{Body.tsx,TerminalView.tsx,session.ts}`, `scripts/verify-porting-terminal.mjs`

**Interfaces:**
- Produces: `Body` owns one `BashShell` (via `useRef`) and one `OutputLog` for the whole session; `TerminalView` takes `shell: BashShell` as a prop; switching engines replays `log.text()` into the new core.

Open design issue to resolve here (flagged in Task 9 Step 2): `BashShell.attach(write)` prints the greeting and a fresh prompt and sets the write callback. After a switch the shell must keep its filesystem, cwd and history, but its output must go to the **new** terminal. Check `@wterm/just-bash`'s `attach` by reading `node_modules/@wterm/just-bash/dist/index.js`: if calling `attach` twice re-creates `Bash` (it does at line `this._bash = new Bash(...)` per the 0.5.4 source), then **do not call `attach` again**. Instead keep a mutable sink in `Body`: `const sink = { write: (_: string) => {} }`; call `shell.attach((c) => sink.write(c))` exactly once at first start, and on each `TerminalView` mount set `sink.write = (c) => { log.push(c); term.write(c) }`. Input is likewise routed through `sink`'s counterpart: `onData` of the current `WTerm` calls `shell.handleInput`.

- [ ] **Step 1: Add the failing pure test for the sink/replay contract**

Extract the sink into `session.ts` as a testable class and test it first:

```ts
/** OutputRouter forwards shell output to the current terminal and the replay log. */
export class OutputRouter {
  private target: ((chunk: string) => void) | null = null;
  constructor(private readonly log: OutputLog) {}
  /** Points output at a terminal's write function; returns what to replay into it first. */
  attach(write: (chunk: string) => void): string {
    this.target = write;
    return this.log.text();
  }
  /** Stops forwarding to the current terminal (it was destroyed). */
  detach(): void {
    this.target = null;
  }
  /** Records chunk and forwards it to the attached terminal, if any. */
  write(chunk: string): void {
    this.log.push(chunk);
    this.target?.(chunk);
  }
}
```

Append to `verify-porting-terminal.mjs`:

```js
import { OutputRouter } from "../src/components/porting/live-terminal/session.ts";
{
  const log = new OutputLog();
  const router = new OutputRouter(log);
  const a = []; const b = [];
  assert.equal(router.attach((c) => a.push(c)), "");
  router.write("one"); router.write("two");
  router.detach();
  router.write("while-detached");                       // recorded, not delivered
  assert.equal(router.attach((c) => b.push(c)), "onetwowhile-detached"); // replay for the new terminal
  router.write("three");
  assert.deepEqual(a, ["one", "two"]);
  assert.deepEqual(b, ["three"]);
}
```

Run → FAIL (`OutputRouter` not exported). Add the class → run → PASS.

- [ ] **Step 2: Rewire `Body.tsx` and `TerminalView.tsx`** to the router: `Body` creates `log`, `router = new OutputRouter(log)`, one `BashShell`, and calls `shell.attach((c) => router.write(c))` on first start only. `TerminalView` on mount: `const replay = router.attach((c) => t.write(c)); if (replay) t.write(replay);` and on cleanup `router.detach()` then destroy the terminal; `onData` → `shell.handleInput`.

- [ ] **Step 3: Verify in a real browser (Playwright, 1100 px)**

```bash
npm run build && npx next start -p 3143 &
```

With the Playwright MCP (output under `/Users/gael/projects/rustty/.playwright-mcp/`): open `/porting`, scroll to the widget, click Start; type `echo hello` + Enter, `./demo/truecolor.sh` + Enter; take a screenshot; click the Zig engine button; screenshot again. Expected: both screenshots show `hello` and the gradient with scrollback intact; the prompt is not duplicated; typing still works; `cd demo` before the switch is still the cwd after it (`pwd`). Kill only the PID started.

- [ ] **Step 4: Commit**

```bash
npm run lint && npm run verify:porting
git add -A && git commit -m "feat(porting): keep shell state and screen across engine switches

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 11: Article section, page integration, cross-browser and phone checks

**Files:**
- Modify: `src/app/porting/content.mdx`, `src/app/porting/page.tsx` (only if the new section opens a TOC group), `scripts/porting/facts.csv` + regenerated `src/app/porting/data/facts.json` if new facts are added, `scripts/verify-porting-terminal.mjs`

**Interfaces:**
- Consumes: registry slug `"live-terminal"`; fact ids to add: `wterm_wasm_bytes` (size of `rustty-wterm.wasm`, from `manifest.json`), `zig_wasm_bytes` (size of `ghostty-vt.wasm`).

Placement: a new level-2 section **"Try it in your browser"** directly after the section that describes the byte-compatible C ABI and the macOS app's reuse — read `content.mdx` lines 38–46 and the headings list (`grep -n "^## " src/app/porting/content.mdx`) and place it before "The reference as a contract" only if that reads naturally; otherwise place it just before "Limitations and what's left". Choose by reading both neighbours; prefer the position where the reader has already learned that Rustty keeps Ghostty's C ABI.

- [ ] **Step 1: Write the section** (prose rules: first person singular sparingly; every number a `<Fact>` with `href`; no internal ids; link names, not bare paths):

```mdx
## Try it in your browser

The terminal below is Rustty's engine compiled to WebAssembly. [wterm](https://wterm.dev/) is a terminal front end for the web from Vercel. It talks to its terminal engine through a small interface, and ships one engine built from Ghostty's Zig code. I wrote a second implementation of that interface on top of Rustty's engine, so wterm runs the Rust engine without changing a line of its own code. The Rust build is <Fact id="wterm_wasm_bytes" /> and the Zig one it replaces is <Fact id="zig_wasm_bytes" />.

The shell is [just-bash](https://github.com/vercel-labs/just-bash), a bash interpreter written in TypeScript with an in-memory filesystem, so nothing leaves your browser. Switch engines while you type: the screen, the history and the working directory carry over.

<PortingWidget slug="live-terminal" />

The scripts in the `demo` folder exercise the engine on purpose: 24-bit colour, five underline styles, wide and combining characters, hyperlinks and window titles. The two engines agree on the recorded sessions I compared them on; where they do not, the cause is listed in the project's known differences.
```

(The last sentence must only be kept if `harness/known-diffs.json` lists any entries; if it is empty write "The two engines agree on every recorded session I compared them on." If entries exist, link them with `<SourceLink at="…" path="harness/known-diffs.json" />` only once `rustty-wterm` has a public commit to link; until then drop the clause and the sentence about known differences — do not link to a private path.)

- [ ] **Step 2: Add the two facts**

Append rows to `scripts/porting/facts.csv` following the file's existing column order (read its header row first) with `href` pointing at the wterm package page for the Zig size and at the `rustty-wterm` repo (or, until public, the wterm `packages/@wterm/ghostty` tree at commit `488e9a7344e812e35a951f803618d7767cc4be5b`) for the Rust size; values computed from `public/porting/wterm/manifest.json` (display as `1.4 MB` / `592 KB` style — match how existing size facts are formatted by grepping `facts.csv` for "MB"). Regenerate with `python3 scripts/porting/build_data.py` and run `python3 -m unittest scripts/porting/test_build_data.py`. Expected: tests pass; `facts.json` contains both ids.

- [ ] **Step 3: Run the article guards**

```bash
npm run lint && npm run verify:porting && npm run build && npm run verify:porting:build
```

Expected: all pass. Typical failures and fixes: `deck repeats N` (reword the registry `deck`), `bare source path` (use `<SourceLink>`), `unknown fact id` (Step 2), TOC `group heading … not found` (only if you changed an existing group heading).

- [ ] **Step 4: Cross-viewport and reduced-motion checks (Playwright)**

At 1100 px and 390 px against `next build` + `next start -p 3143`:
1. Page has no horizontal scroll (`document.documentElement.scrollWidth <= innerWidth`).
2. Before clicking: network panel shows **no** request for `*.wasm` or a just-bash/wterm chunk (Review Focus 4 — verify with `browser_network_requests`).
3. After Start: `rustty-wterm.wasm` returns 200; typing `ls demo` + Enter lists six scripts; `./demo/styles.sh` shows five underline styles; `./demo/links.sh` yields an `<a>` element (OSC 8) — assert via `browser_evaluate`.
4. 390 px: tapping the terminal focuses the hidden input (soft keyboard can open); the frame scrolls internally.
5. Emulate `prefers-reduced-motion: reduce` (`browser_emulate_media`): cursor does not blink.
6. Block the wasm URL (`browser_run_code_unsafe` route abort) and click Start: the "could not start" panel with **Try again** appears; the rest of the article is unaffected.
7. Repeat 3 in Firefox and WebKit if the Playwright install provides them; note any failure in the commit message rather than skipping silently.

Delete screenshots under `.playwright-mcp/` afterwards.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(porting): add 'Try it in your browser' section with the live terminal

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 12: Release hygiene and handoff

**Files:**
- Modify: `~/projects/rustty-wterm/README.md`; website `docs/superpowers/plans/2026-10-11-wterm-live-terminal.md` (tick boxes only)

- [ ] **Step 1: `rustty-wterm/README.md` complete** — sections: what it is; the ABI table (every export, with the Zig line it mirrors); pins (`rustty-vt` commit from `git -C ~/projects/rustty rev-parse HEAD`, wterm commit `488e9a7344e812e35a951f803618d7767cc4be5b`, `@wterm/*` 0.5.4); build (`tools/build-wasm.sh`); test (`tools/check.sh`); Kitty graphics outcome (Task 5); Known gaps; Findings (differences in `known-diffs.json` with owners); licence note: wterm is Apache-2.0, credit it, and if the Zig binary is redistributed on the site keep its `LICENSE`/NOTICE (copy `node_modules/@wterm/ghostty/LICENSE` to `public/porting/wterm/LICENSE-wterm.txt`).

- [ ] **Step 2: Add the attribution file in the website** — `cp node_modules/@wterm/ghostty/LICENSE public/porting/wterm/LICENSE-wterm.txt`; mention wterm and just-bash with links in the section prose (done in Task 11) and in the widget caption (done in Task 9).

- [ ] **Step 3: Final full verification** (both repos)

```bash
cd ~/projects/rustty-wterm && tools/check.sh
cd ~/projects/ghostty-website-wterm && npm run lint && npm run verify:porting && npm run build && npm run verify:porting:build && npm run verify:animation
git -C ~/projects/ghostty-website-wterm status --short   # expect clean
```

Expected: everything green, both trees clean. Do **not** push either repository.

- [ ] **Step 4: Commit**

```bash
git -C ~/projects/rustty-wterm add -A && git -C ~/projects/rustty-wterm commit -m "docs: README, pins and findings

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
cd ~/projects/ghostty-website-wterm && git add -A && git commit -m "chore(porting): wterm licence notice for redistributed engine binary

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review notes (completed)

- **Spec coverage:** goal items → tasks: Rust engine behind wterm's ABI (1–5), proof of equivalence (6–7), page widget with lazy load and fallback (8–9), engine switch with state kept (10), article section and guards (11), licences and docs (12). Review Focus 1→T6, 2→T4, 3→T3/T5/T7, 4→T9/T11, 5→T10, 6→T11, 7→T9.
- **Open risks the executor must surface, not hide:** (a) Task 4 effects — OSC 10/11 queries and override resets may lack a Rustty hook; the plan says record a gap, not patch `rustty-vt`; (b) Task 5 gate B removes image support; (c) Task 6 triage may reveal real Rustty-vs-Ghostty differences — those are findings for the article, not failures to hide; (d) `WTerm` option and teardown names in Task 9 are given from the `.d.ts` read on 2026-10-11 and must be re-checked with the grep shown.
- **Type consistency:** `Core` method names used in tests (`read_response`, `title`, `bell_count`, `shell_phase`, `shell_exit_code`, `color_override`, `track_position`, `resolve_position`, `release_position`, `scrollback_*`, `grapheme_*`, `hyperlink_*`, `row_wraps`, `graphics_*`) match the Interfaces blocks; `OutputLog`/`OutputRouter` and `ENGINES` signatures match across Tasks 8–10.
