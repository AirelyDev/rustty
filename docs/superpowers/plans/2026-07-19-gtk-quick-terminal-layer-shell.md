# GTK Quick Terminal (wlr-layer-shell) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or a dynamic Workflow). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Implement the drop-down "quick terminal" on Wayland via `wlr-layer-shell` (the `gtk4-layer-shell` binding), so `toggle_quick_terminal` shows an edge-anchored overlay terminal — GTK parity with ghostty. Self-disabling on compositors without layer-shell (GNOME/mutter), so it's safe on-by-default.

**Architecture (settled in design discussion):** The shared config surface is already ported (all `quick-terminal-*` / `gtk-quick-terminal-*` keys exist). The only gap is the Wayland winproto arm: `supports_quick_terminal()` / `init_quick_terminal()` are stubs. Add the `gtk4-layer-shell` crate (0.8.0, matches our gtk4-rs 0.11) behind the `wayland` feature, and port ghostty's `winproto/wayland.zig` layer-shell setup (`:70-88` init/isSupported/setMonitor + `:310-389` the quick-terminal window config) plus the slide/autohide behavior. Runtime `layer_shell::is_supported()` gates it (false on mutter → graceful).

**Tech Stack:** Rust, `gtk4-layer-shell` 0.8.0 (binds libgtk4-layer-shell 1.3.0), gtk4-rs 0.11, the existing `wayland`-feature winproto arm. Oracle: `/home/gael/projects/ghostty` @ `4749c4e93`, `src/apprt/gtk/winproto/wayland.zig`.

## Global Constraints

- **R1 fidelity** — port `wayland.zig`'s layer-shell paths faithfully; preserve reference comments.
- **Feature-gated** — all `gtk4-layer-shell` use is behind the existing `wayland` cargo feature (the winproto wayland arm). The default `--features gtk` build (no `wayland`) is unchanged; mutter users get the graceful `supports_quick_terminal() == false`.
- **Runtime gate** — `supports_quick_terminal()` returns `layer_shell::is_supported()` (false on mutter/GNOME, true on wlroots/KWin). Never assume support.
- **System dep** — requires `gtk4-layer-shell` + `gtk4-layer-shell-devel` (1.3.0, Fedora) at build/runtime (pkg-config `gtk4-layer-shell`).
- **Build gate** — `cargo build -p rustty-apprt --features gtk,wayland` green after every task; `cargo build -p rustty-apprt --features gtk` (no wayland) stays green too.
- **Git** — explicit paths; retry on `index.lock` (sleep 3, ≤10×); never push/reset/stash.

## File Structure

- **Modify:** `crates/rustty-apprt/Cargo.toml` — add `gtk4-layer-shell = { version = "0.8", optional = true }` to the `wayland` feature's deps.
- **Modify:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs` — real `supports_quick_terminal` + `init_quick_terminal` (currently stubs at :221/:231); the `initForWindow`/`setMonitor` bring-up.
- **Modify:** the GTK `Window`/quick-terminal-window construction site (where `init_quick_terminal` is called + the slide/autohide lives — find via the `slide_manager`/`Window.slide` scaffold the DEFERRALS mention).
- **Reused as-is:** all `quick-terminal-*` config keys (`rustty-config`), the `toggle_quick_terminal` action + IPC (already wired), the `QuickTerminalUnsupported` error.

---

## Task 1: Add the `gtk4-layer-shell` dependency + link-verify

**Files:** `crates/rustty-apprt/Cargo.toml`

- [ ] **Step 1:** Add `gtk4-layer-shell = { version = "0.8", optional = true }` to `[dependencies]` and add `dep:gtk4-layer-shell` to the `wayland` feature's enable list (alongside the other wayland deps). Confirm 0.8.0 resolves `gtk4 ^0.11` (matches our gtk4 0.11) via `cargo update -p gtk4-layer-shell --dry-run` or a plain build.
- [ ] **Step 2:** Add a trivial `#[cfg(feature = "wayland")]` reference (e.g. in wayland.rs, `let _ = gtk4_layer_shell::is_supported;`) so the crate links. Build: `cargo build -p rustty-apprt --features gtk,wayland` → green (pkg-config finds `gtk4-layer-shell` 1.3.0). If the `-sys` crate can't find the system lib, that's the `gtk4-layer-shell-devel` prereq — report it.
- [ ] **Step 3:** Confirm `cargo build -p rustty-apprt --features gtk` (WITHOUT wayland) still green (the dep is optional, not pulled). Commit: `gtk/winproto(wayland): add gtk4-layer-shell dependency (Task 1)`.

## Task 2: Port the layer-shell quick-terminal setup + wire supports/init

**Files:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs`
**Oracle:** `winproto/wayland.zig:70-88` (initForWindow/isSupported/setMonitor) + `:310-389` (the quick-terminal window config).

**Interfaces:**
- Produces: `App::supports_quick_terminal()` → `gtk4_layer_shell::is_supported()`; `App::init_quick_terminal(window)` → initializes the window as a layer surface and applies layer/namespace/keyboard-mode/anchors/margins/size from config.

- [ ] **Step 1:** `supports_quick_terminal()` (wayland.rs:221) → return `gtk4_layer_shell::LayerShell`/`is_supported()` (the crate's global `is_supported()` fn), replacing `false`. Ported comment from `wayland.zig:72`.
- [ ] **Step 2:** `init_quick_terminal(window)` (wayland.rs:231) → port `wayland.zig:82-88` + `:310-389`: `layer_shell.init_for_window(window)`; `set_namespace(config.gtk_quick_terminal_namespace)`; `set_layer(...)` from `gtk_quick_terminal_layer` (Background/Bottom/Top/Overlay); `set_monitor(...)` from `quick_terminal_screen`; `set_keyboard_mode(...)` (with the `get_protocol_version() < 4` guard → OnDemand fallback per `:336`); then per `ShellEdge`, `set_anchor`/`set_margin` for the `quick_terminal_position` (the anchored-edge + the two perpendicular edges get anchored, the opposite edge free — faithful to `:346-365`). Replace the `Err(QuickTerminalUnsupported)` return. Ported comments.
- [ ] **Step 3:** Build `cargo build -p rustty-apprt --features gtk,wayland` → green. Commit: `gtk/winproto(wayland): port layer-shell quick-terminal setup + wire supports/init (Task 2)`.

## Task 3: Slide / autohide / focus-out behavior

**Files:** the quick-terminal `Window` construction + toggle site (locate via the `slide`/`slide_manager` scaffold).
**Oracle:** `winproto/wayland.zig` slide/`slide_manager` + the reference's quick-terminal show/hide (animation duration, autohide on focus-out, space-behavior).

- [ ] **Step 1:** Port the slide-in/out animation (`quick_terminal_animation_duration`) and the autohide-on-focus-loss (`quick_terminal_autohide`) + `quick_terminal_space_behavior`, using the staged `Window.slide`/`slide_manager` fields. Faithful to the reference show/hide.
- [ ] **Step 2:** Build `--features gtk,wayland` green. Commit: `gtk/winproto(wayland): quick-terminal slide + autohide + space-behavior (Task 3)`.

## Task 4: Nested-sway validation (HUMAN/main-agent — not a subagent)

**Files:** none (verification); may fix wiring the live run surfaces.

- [ ] **Step 1:** Build `cargo build -p rustty --features` (the bin; ensure the `wayland` feature reaches it) → the dev binary with layer-shell.
- [ ] **Step 2:** Launch a **nested sway** inside the current session (`sway` with the wayland/x11 backend → a compositor in a window; sway 1.11 implements wlr-layer-shell). Run rustty inside nested sway with a keybind bound to `toggle_quick_terminal`.
- [ ] **Step 3:** **Screenshot-verify:** toggle → an edge-anchored drop-down terminal slides in over the sway workspace; toggle again → it hides. Test `quick-terminal-position` top/bottom/left/right and a layer. Confirm `is_supported()` gates it OFF on the GNOME/mutter host (no crash, toggle is a no-op there). Commit any wiring fixes: `gtk/winproto(wayland): quick-terminal nested-sway shakedown (Task 4)`.
- [ ] **Step 4:** Update `.port/phase-2/DEFERRALS.md` (the quick-terminal sub-deferral → RESOLVED), `docs/manual-test-plan-gtk.md`, and the parity memory.

---

## Self-Review

- **Dependency + compat** (gtk4-layer-shell 0.8 ↔ gtk4 0.11) → Task 1. ✅
- **isSupported/init/setMonitor + the quick-terminal window config** (`wayland.zig:70-88,310-389`) → Task 2. ✅
- **Slide/autohide/space-behavior** → Task 3. ✅
- **Config keys** — already ported (no task needed). ✅
- **Runtime gate + feature gate + graceful-on-mutter** → Global Constraints + Task 2 Step 1. ✅
- **Validation** (nested sway; graceful-off on GNOME) → Task 4. ✅
- **Placeholder scan:** the port tasks cite exact oracle spans; the novel bits (Cargo dep, the `is_supported()` wiring) are concrete. Acceptable for a port plan.
- **Type consistency:** `supports_quick_terminal`/`init_quick_terminal` (winproto.rs dispatch), `gtk4_layer_shell::is_supported`, `QuickTerminalUnsupported`. Consistent.

**Sequencing:** T1 → T2 → T3 strictly sequential (each builds on the dep + prior wiring). T4 (nested-sway) is main-agent/human, after T1-T3.
