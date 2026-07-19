# GTK Quick Terminal (wlr-layer-shell) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or a dynamic Workflow). Steps use checkbox (`- [ ]`) syntax.
>
> **v2 (2026-07-19)** — rewritten after a full oracle re-map corrected v1's structural errors (see "Alignment corrections" below).

**Goal:** Implement the drop-down "quick terminal" on Wayland via `wlr-layer-shell` (`gtk4-layer-shell`), so `toggle_quick_terminal` shows an edge-anchored overlay terminal — faithful GTK parity with ghostty. Self-disabling on compositors without layer-shell (GNOME/mutter), safe on-by-default.

**Architecture (corrected to match ghostty):** The quick terminal is a regular **`Window` with `quick-terminal=true`**, not a separate class. The **window-side is already ported** in rustty (property, `on_notify_quick_terminal`, `is_quick_terminal`, autohide-on-focus-loss, header-bar suppression, `toggle_visibility`), and **all config keys + the KDE-slide scaffolding** (the `slide`/`slide_manager` fields, proxy imports, `deinit` release path) are already staged. The remaining work is entirely in the **Wayland winproto arm + the application toggle**: the layer-shell dependency, un-stubbing `supports/init_quick_terminal`, and adding `sync_quick_terminal` / `resolve_quick_terminal_monitor` / `entered_monitor` / `get_quick_terminal_window`. X11 is correctly unsupported (matches oracle). Runtime `layer_shell::is_supported()` gates it (false on mutter → graceful).

**Tech Stack:** Rust, `gtk4-layer-shell` 0.8.0 (binds libgtk4-layer-shell 1.3.0; matches our gtk4-rs 0.11), the existing `wayland`-feature winproto arm + its staged KDE-slide scaffold. Oracle: `/home/gael/projects/ghostty` @ `4749c4e93`, `src/apprt/gtk/winproto/wayland.zig` + `class/{application,window}.zig`.

## Alignment corrections vs the v1 plan (why v1 was wrong)

- **No GTK slide animation exists.** The only enter/exit animation is the **KDE `org_kde_kwin_slide`** compositor protocol, created inside `sync_quick_terminal`. `quick-terminal-animation-duration` is **macOS-only, never read in GTK**. On sway (no `kde_slide_manager`) the quick terminal simply **appears anchored — no animation** (correct/faithful). v1's "build a slide/animation" task was wrong.
- **`quick-terminal-space-behavior` is macOS-only** — never read on GTK. v1 listed it as GTK work; drop it.
- **`init` vs `sync` split.** `init_quick_terminal` is minimal (`initForWindow` + `setMonitor`); the layer/namespace/keyboard/anchor/margin/slide all live in **`sync_quick_terminal`**, invoked from `sync_appearance` on every appearance sync — not once.
- **Size lives in the winproto `entered_monitor`** (GDK `enter_monitor` signal), not window.zig — `quick-terminal-size.calculate(...)` → `set_default_size`.
- **The window-side + config are already done** — not part of this work.

## Global Constraints

- **R1 fidelity** — port the `wayland.zig` quick-terminal paths faithfully; preserve reference comments.
- **Feature-gated** — all `gtk4-layer-shell` use behind the existing `wayland` cargo feature. Default `--features gtk` build unchanged (mutter users get graceful `supports_quick_terminal() == false`).
- **Runtime gate** — `supports_quick_terminal()` returns `layer_shell::is_supported()`.
- **System dep** — `gtk4-layer-shell` + `gtk4-layer-shell-devel` (1.3.0) at build/runtime (pkg-config `gtk4-layer-shell`).
- **Build gate** — `cargo build -p rustty-apprt --features gtk,wayland` green after every task; `--features gtk` (no wayland) stays green.
- **Git** — explicit paths; retry on `index.lock`; never push/reset/stash.

## File Structure

- **Modify:** `crates/rustty-apprt/Cargo.toml` — `gtk4-layer-shell = { version = "0.8", optional = true }` in the `wayland` feature.
- **Modify:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs` — un-stub `supports_quick_terminal` (:221) + `init_quick_terminal` (:231); add `sync_quick_terminal`, `resolve_quick_terminal_monitor`, `entered_monitor`; wire the `enter_monitor` signal in `Window::init` (PORT NOTE at :352-353); un-comment the `sync_quick_terminal` call in `sync_appearance` (:408).
- **Modify:** `crates/rustty-apprt/src/gtk/class/application.rs` — implement `toggle_quick_terminal` (:3325 stub) + add `get_quick_terminal_window`.
- **Reused as-is (DONE):** the `quick-terminal` property + autohide + header suppression + `toggle_visibility` (window.rs); all `quick-terminal-*` config keys; the staged `Window.slide`/`slide_manager` scaffold + `deinit` release path; the `toggle-quick-terminal` action wiring; the X11 unsupported arms.

---

## Task 1: Add the `gtk4-layer-shell` dependency

**Files:** `crates/rustty-apprt/Cargo.toml`
- [ ] **Step 1:** Add `gtk4-layer-shell = { version = "0.8", optional = true }`; add `dep:gtk4-layer-shell` to the `wayland` feature. (0.8.0 → gtk4 ^0.11, matches ours.)
- [ ] **Step 2:** Add a `#[cfg(feature="wayland")]` link reference (`let _ = gtk4_layer_shell::is_supported;`). Build `cargo build -p rustty-apprt --features gtk,wayland` → green (needs `gtk4-layer-shell-devel`). Confirm `--features gtk` (no wayland) still green.
- [ ] **Step 3:** Commit: `gtk/winproto(wayland): add gtk4-layer-shell dependency (Task 1)`.

## Task 2: Un-stub `supports_quick_terminal` + `init_quick_terminal` + `resolve_quick_terminal_monitor`

**Files:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs`
**Oracle:** `wayland.zig:70-78` (supports), `:80-88` (init), `:449-483` (resolveQuickTerminalMonitor).
- [ ] **Step 1:** `supports_quick_terminal` (:221) → `if !gtk4_layer_shell::is_supported() { warn; return false } true`. Ported comment.
- [ ] **Step 2:** Add `resolve_quick_terminal_monitor(globals, apprt_window) -> Option<gdk::Monitor>` per `:449-483`: map `quick-terminal-screen` (`.mouse` → None; `.main`/`.macos-menu-bar` → match `globals.state.primary_output_name` against monitor connectors, fall back to first). Caller unrefs.
- [ ] **Step 3:** `init_quick_terminal` (:231) → `layer_shell.init_for_window(window)` + `set_monitor(window, resolve_quick_terminal_monitor(...))`, replacing the `Err(QuickTerminalUnsupported)`. Ported comments.
- [ ] **Step 4:** Build `--features gtk,wayland` green. Commit: `gtk/winproto(wayland): un-stub supports/init_quick_terminal + resolve monitor (Task 2)`.

## Task 3: Add `sync_quick_terminal` (layer/namespace/keyboard/anchor/margin + KDE slide)

**Files:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs`
**Oracle:** `wayland.zig:310-389` (syncQuickTerminal), invoked from `syncAppearance:209-213`.
- [ ] **Step 1:** Add `fn sync_quick_terminal(&mut self)` on the winproto `Window`: `set_layer` (from `gtk_quick_terminal_layer`), `set_namespace` (`gtk_quick_terminal_namespace`), re-resolve + `set_monitor`, `set_keyboard_mode` (from `quick_terminal_keyboard_interactivity`, with the `get_protocol_version() < 4` → exclusive fallback), the `anchored_edge` from `quick_terminal_position` (center → None), the per-`ShellEdge` loop (anchored edge: margin 0 + anchor true; others: margin 20 + anchor false). Then the KDE slide: `if let Some(s) = self.slide.take() { s.release() }`; if anchored, `self.slide_manager` → `create(self.surface)` → `set_location(edge)` → `commit()` → store; else `None`. Uses the ALREADY-STAGED `slide`/`slide_manager` fields.
- [ ] **Step 2:** Un-comment / add the `sync_quick_terminal()` call in `sync_appearance` (:408) guarded by `is_quick_terminal()` (mirror `wayland.zig:209-213`).
- [ ] **Step 3:** Build `--features gtk,wayland` green. Commit: `gtk/winproto(wayland): sync_quick_terminal — layer/anchor/margin + KDE slide (Task 3)`.

## Task 4: Quick-terminal sizing — `entered_monitor` + enter_monitor signal

**Files:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs`
**Oracle:** `wayland.zig:392-421` (enteredMonitor) + `Window.init:169-177` (signal connect).
- [ ] **Step 1:** Add `entered_monitor(surface, monitor, apprt_window)`: resolve the sizing monitor (configured, or the entered one in `.mouse` mode), read geometry, `config.quick_terminal_size.calculate(position, {w,h})` → `window.set_default_size(w, h)`.
- [ ] **Step 2:** In `Window::init`, when `apprt_window.is_quick_terminal()`, connect the GDK surface `enter-monitor` signal → `entered_monitor` (replacing the PORT NOTE at :352-353).
- [ ] **Step 3:** Build `--features gtk,wayland` green. Commit: `gtk/winproto(wayland): quick-terminal sizing via enter_monitor (Task 4)`.

## Task 5: `Application::toggle_quick_terminal` + `get_quick_terminal_window`

**Files:** `crates/rustty-apprt/src/gtk/class/application.rs`
**Oracle:** `application.zig:2649-2669` (toggle) + `:2713-2731` (getQuickTerminalWindow).
- [ ] **Step 1:** Add `get_quick_terminal_window() -> Option<Window>`: scan `gtk::Window::toplevels()`, return the first that downcasts to `Window` with `is_quick_terminal()`.
- [ ] **Step 2:** Implement `toggle_quick_terminal` (:3325): if `get_quick_terminal_window()` → `win.toggle_visibility()`, return true; else if `!winproto().supports_quick_terminal()` return false; else create a `Window` with the `quick-terminal` property = true (mirror `newInstance(Window, .{application, "quick-terminal"=true})`), assert `is_quick_terminal()`, and show it via the existing init-and-show path.
- [ ] **Step 3:** Build `--features gtk,wayland` green + `cargo build --workspace` green. Commit: `gtk: implement toggle_quick_terminal + get_quick_terminal_window (Task 5)`.

## Task 6: Nested-sway validation (HUMAN / main-agent — not a subagent)

- [ ] **Step 1:** Build the dev binary with the `wayland` feature reaching it.
- [ ] **Step 2:** Launch **nested sway** (`sway` 1.11, wlr-layer-shell) inside the session; run rustty inside it with a keybind bound to `toggle_quick_terminal`.
- [ ] **Step 3:** **Screenshot-verify:** toggle → an edge-anchored drop-down appears (test `quick-terminal-position` top/bottom/left/right, a layer, sizing). **Expect NO slide animation on sway** (KDE-only — the window just appears; this is faithful). Toggle again → hides; autohide-on-focus-loss works. Confirm `is_supported()` gates it **OFF on the GNOME/mutter host** (no crash, toggle no-ops).
- [ ] **Step 4:** Update `.port/phase-2/DEFERRALS.md` (quick-terminal sub-deferral → RESOLVED, noting KDE-slide-on-KDE-only), `docs/manual-test-plan-gtk.md`, and the parity memory. Commit.

---

## Self-Review

- Dependency + compat → Task 1. ✅
- supports/init + resolve monitor (`:70-88,449-483`) → Task 2. ✅
- sync_quick_terminal layer/anchor/margin + KDE slide (`:310-389`) → Task 3. ✅
- sizing via enter_monitor (`:392-421`) → Task 4. ✅
- toggle + get_quick_terminal_window (`:2649-2731`) → Task 5. ✅
- Config keys, window-side (property/autohide/header/toggle_visibility), KDE-slide scaffold, X11 unsupported — already DONE (no task). ✅
- **Corrected:** no GTK animation task; `space-behavior`/`animation-duration` are macOS-only (excluded); slide is KDE-protocol-only (staged). ✅
- **Sequencing:** T1 → T2 → T3 → T4 → T5 sequential; T6 human, last.
