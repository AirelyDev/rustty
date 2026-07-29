# WS15 — GTK GlobalShortcuts (XDG desktop-portal round-trip) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `global:` keybinds actually work in the Linux/GTK app by porting the deferred `org.freedesktop.portal.GlobalShortcuts` D-Bus round-trip (`CreateSession` → `BindShortcuts` → `Activated`) into `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`, and by wiring the resulting `trigger` signal through to the core app's `performAllAction`.

**Architecture:** The port is a 1:1 fill of the existing INHABITED NO-OP class (`refresh`/`close`/`request` are currently empty bodies, deliberately not `unimplemented!()`). All portal protocol logic goes into the SAME file as the oracle keeps it (`global_shortcuts.zig`), exposed as small free functions (`build_shortcut_map`, `generate_token`, `request_object_path`, `create_session_payload`, `bind_shortcuts_payload`, `session_handle_from_results`) so the protocol is unit-testable **headlessly** — no display, no bus. The live D-Bus half (`request`, `Method::on_response`, `shortcut_activated`) uses the SAME D-Bus stack the GTK apprt already has in hand: **`gio::DBusConnection`** (gtk4-rs / gio 0.22.8), obtained from `gio::Application::dbus_connection()` in `startup_global_shortcuts` and already stored in the class's `dbus-connection` property. No new D-Bus crate (no `zbus`, no `libportal`) is introduced. Finally `Application::global_shortcut_trigger` stops being a placeholder-leaf: a new `perform_all_action_thunk` (identical in shape to the existing `tick_thunk`/`update_config_thunk`/`color_scheme_thunk`) carries the monomorphized `core_app.performAllAction(rt_app, action)` call across the type-erasure seam.

**Tech Stack:** Rust 1.93.1 (pinned), gtk4-rs 0.11-era stack with `glib`/`gio` 0.22.8 (`gio::DBusConnection`, `glib::Variant`/`VariantDict`/`VariantTy`/`variant::ObjectPath`), `rustty-cfg-fmt` (`AllocatingWriter`, for `Action::format` → the portal `description` field), `rustty-os::file::crypto_random_bytes` (token entropy). Execution host: the Fedora 44 aarch64 VM `rvm` (GNOME Shell 50.0, Wayland, `xdg-desktop-portal` 1.21.1 + `xdg-desktop-portal-gnome` 50.0, `org.freedesktop.portal.GlobalShortcuts` **version 2** confirmed present). Oracle: `/Users/gael/projects/ghostty` at pin `4749c4e`, plus the installed reference binary `ghostty 1.3.1-2.fc44` on the VM (used as a live protocol oracle).

## Global Constraints (inherited from 2026-07-23-rustty-completion-ROADMAP.md)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay. **Applies to the portal `handle_token`/`session_handle_token` prefix: the oracle emits `ghostty_<hex>`; rustty emits `rustty_<hex>`.** The GType name `GhosttyGlobalShortcuts` is a registered type name and stays verbatim (existing precedent in this file).
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI. (WS15 touches no C-ABI surface.)
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike the DEFERRALS.md L6 GlobalShortcuts entry (lines 3982–4022) in the same PR.
- Commit style: `subsystem: summary (WS15 T<n>)`. Branch per WS; merge to main only with both platforms green.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes). The portal payload fields are D-Bus `s` (UTF-8 by protocol), so `String` is correct there; the one lossy conversion (`Action::format` output → `String`) is explicitly a UTF-8-by-construction formatter output.
- When porting oracle doc prose, strip snark/gag editorializing (memory: strip-ghostty-gag-prose). **This applies hard here:** `global_shortcuts.zig:340-360` contains a long editorializing rant about XDG portals ("really, really poorly-designed pieces of hot garbage", "even _TCP_ had figured out decades ago", "When in Rome…"). Port the *technical content* (why the request object path must be constructed and subscribed to BEFORE the method call — a server may emit `Response` before the client subscribes) and drop the editorializing.

## Platform scope

This is a **Linux-only** workstream. Every file touched is inside `crates/rustty-apprt/src/gtk/`, which is gated at `crates/rustty-apprt/src/lib.rs:46` by `#[cfg(all(target_os = "linux", feature = "gtk"))]`. **macOS is unaffected**: on macOS the `gtk` feature compiles no gtk4-rs and this module is not built at all. The only cross-platform edit in the whole plan is one line in `crates/rustty-apprt/Cargo.toml` (a new `rustty-cfg-fmt` dependency, a pure-Rust crate already in the workspace graph via `rustty-input`), which is why the final gate still builds BOTH platforms.

## Execution host (READ THIS FIRST)

All build/test/run commands in this plan run **on the VM**, via `ssh rvm 'cd ~/projects/rustty && …'`. The VM repo tracks `origin` = `git@github.com:AirelyDev/rustty.git`; the working copy on macOS is where edits are authored and committed, then `git push` / `ssh rvm 'git pull'` moves them across. Verified VM facts (probed 2026-07-29):

| Fact | Value |
|---|---|
| OS | Fedora release 44 (Forty Four), aarch64 |
| Desktop | GNOME Shell 50.0, session type **wayland**, seat0/tty2, `Active=yes` |
| Portal frontend | `xdg-desktop-portal` 1.21.1-1.fc44 (`org.freedesktop.portal.Desktop` running) |
| Portal backend | `xdg-desktop-portal-gnome` 50.0-1.fc44, exposes `org.freedesktop.impl.portal.GlobalShortcuts` |
| Portal interface | `org.freedesktop.portal.GlobalShortcuts` **version = 2**, methods `CreateSession`/`BindShortcuts`/`ListShortcuts`/`ConfigureShortcuts`, signals `Activated`/`Deactivated`/`ShortcutsChanged` |
| Reference app | `/usr/bin/ghostty` — **Ghostty 1.3.1-2.fc44** (a live oracle for the portal traffic) |
| D-Bus tooling | `dbus-monitor`, `gdbus`, `busctl` all present |
| Config path | `~/.config/rustty/config.rustty` exists (template); `XDG_CONFIG_HOME` is honored, so tests use an isolated config dir |
| App id | debug builds use `com.mitchellh.ghostty-debug` — distinct from the installed ghostty's `com.mitchellh.ghostty`, so both can run without single-instance collision |
| sudo | **requires a password** — no task in this plan may need root. Nothing here does. |

**No portal install/enable step is needed** — the backend is present, activated and at version 2. (If a future re-run finds it missing: `sudo dnf install -y xdg-desktop-portal xdg-desktop-portal-gnome && systemctl --user restart xdg-desktop-portal xdg-desktop-portal-gnome`. Requires an interactive sudo password on this VM.)

The environment prefix used by every GUI/portal command in this plan (an SSH session has no desktop env vars):

```bash
export XDG_RUNTIME_DIR=/run/user/1000
export WAYLAND_DISPLAY=wayland-0
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
export GDK_BACKEND=wayland
```

---

## File Structure

- `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` — **modify (the bulk of WS15).** Currently 219 lines: the GObject class with `config`/`dbus-connection` properties, the `trigger` signal, and no-op `refresh`/`close`. Gains: the private portal-session state (`map`, `handle`, `response_subscription`, `activate_subscription`), the real `refresh`/`close`, the `Method` enum with payload builders and response handling, `request`, `shortcut_activated`, six headless-testable free functions, and a `#[cfg(test)] mod tests`. Everything stays in ONE file because the oracle keeps it in one file (`global_shortcuts.zig`, 633 lines) and this tree mirrors oracle file layout 1:1.
- `crates/rustty-apprt/src/gtk/class/application.rs` — **modify (2 small edits + 1 test).** Add `perform_all_action_thunk` to `imp::Application`, install it in `Application::bind_core_app`, and make `global_shortcut_trigger` call it (it is currently `fn global_shortcut_trigger(&self, _action: &ActionBoxed) {}` at line 778).
- `crates/rustty-apprt/Cargo.toml` — **modify (1 line).** Add `rustty-cfg-fmt = { workspace = true }` so `Action::format` (whose `W: Writer` bound is `rustty_cfg_fmt::Writer`) can be driven through `rustty_cfg_fmt::AllocatingWriter`.
- `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` — **create.** The live VM gate: launches the app with a `global:` keybind under an isolated `XDG_CONFIG_HOME`, asserts the `CreateSession`→`BindShortcuts` round-trip completed against the real GNOME portal, then injects an `Activated` signal and asserts the action dispatched. Exits 0 = GREEN.
- `.port/phase-2/DEFERRALS.md` — **modify.** Strike the L6 GlobalShortcuts entry (lines 3982–4022).
- `docs/superpowers/plans/2026-07-29-ws15-gtk-globalshortcuts.md` — **modify.** This file; check off boxes as tasks land.

---

### Task 1: Branch, sync the VM, and record the oracle's live portal traffic

**Files:**
- Create (on the VM, scratch only): `/tmp/ws15/oracle-traffic.log`, `/tmp/ws15/oracle-cfg/rustty-oracle/config`
- Modify: none

**Interfaces:**
- Produces: the empirically-confirmed answers three later tasks depend on — (a) does GNOME 50 pop a permission dialog for `BindShortcuts`, (b) what the exact `CreateSession`/`BindShortcuts` payloads look like on this box, (c) whether `Activated` fires from a real key press. Recorded in the Task 1 commit message so no later task has to guess.

- [x] **Step 1: Create the branch on macOS and sync the VM**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git checkout -b ws15-gtk-globalshortcuts
git push -u origin ws15-gtk-globalshortcuts
ssh rvm 'cd ~/projects/rustty && git fetch origin && git checkout ws15-gtk-globalshortcuts && git status --short'
```

Expected: branch created on both sides; `git status --short` prints nothing.

- [x] **Step 2: Re-confirm the portal is present and at version 2**

```bash
ssh rvm 'export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus; \
  gdbus introspect --session --dest org.freedesktop.portal.Desktop \
    --object-path /org/freedesktop/portal/desktop \
  | grep -A40 "interface org.freedesktop.portal.GlobalShortcuts"'
```

Expected: the interface block listing `CreateSession`, `BindShortcuts`, `ListShortcuts`, `ConfigureShortcuts`, signals `Activated`/`Deactivated`/`ShortcutsChanged`, and `readonly u version = 2;`.

If the interface is absent, STOP and run `sudo dnf install -y xdg-desktop-portal-gnome && systemctl --user restart xdg-desktop-portal xdg-desktop-portal-gnome` (interactive password required), then re-run this step.

- [x] **Step 3: Capture the ORACLE (`ghostty 1.3.1`) portal round-trip**

This is the ground truth the rustty implementation must reproduce. It runs the installed reference binary with one `global:` keybind under a throwaway config dir.

Write the capture script on the VM with a quoted heredoc (no shell-escaping traps), then run it:

```bash
ssh rvm 'mkdir -p /tmp/ws15 && cat > /tmp/ws15/capture-oracle.sh' <<'EOF'
#!/usr/bin/env bash
set -u
export XDG_RUNTIME_DIR=/run/user/1000
export WAYLAND_DISPLAY=wayland-0
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
export GDK_BACKEND=wayland

rm -rf /tmp/ws15/oracle-cfg
mkdir -p /tmp/ws15/oracle-cfg/ghostty
printf 'keybind = global:ctrl+alt+shift+f12=new_window\n' > /tmp/ws15/oracle-cfg/ghostty/config

dbus-monitor --session "path_namespace=/org/freedesktop/portal/desktop" \
  > /tmp/ws15/oracle-traffic.log 2>&1 &
MON=$!
sleep 1
XDG_CONFIG_HOME=/tmp/ws15/oracle-cfg ghostty > /tmp/ws15/oracle-app.log 2>&1 &
APP=$!
sleep 10
kill "$APP" "$MON" 2>/dev/null

echo "GlobalShortcuts lines: $(grep -c GlobalShortcuts /tmp/ws15/oracle-traffic.log)"
grep -E "member=(CreateSession|BindShortcuts|Response|Activated)" \
  /tmp/ws15/oracle-traffic.log | head -40
EOF
ssh rvm 'chmod +x /tmp/ws15/capture-oracle.sh && /tmp/ws15/capture-oracle.sh'
```

Expected: a non-zero `GlobalShortcuts` line count and `member=CreateSession` followed by `member=Response` and `member=BindShortcuts`. **Record in the commit message** whether a `Response` with `uint32 0` follows `BindShortcuts` immediately or only after a user dialog is dismissed (GNOME 50 may present an "allow shortcuts" dialog on the VM's physical screen — if the log shows `CreateSession`+`Response 0` but `BindShortcuts` with no `Response`, a dialog IS waiting and Task 8 must include the manual dismissal step).

- [x] **Step 4: Record the exact request-path convention the oracle used**

```bash
ssh rvm 'grep -B4 -A12 "member=CreateSession" /tmp/ws15/oracle-traffic.log | head -60'
```

Expected: the `handle_token`/`session_handle_token` string values (shape `ghostty_<7 hex chars>`), and later a `Response` signal on a path of the form `/org/freedesktop/portal/desktop/request/<unique_name with : stripped and . → _>/<handle_token>`. This confirms the `request_object_path` construction Task 3 implements.

- [x] **Step 5: Commit the recorded findings**

```bash
cd /Users/gael/projects/rustty
git commit --allow-empty -m "gtk/global-shortcuts: record VM portal environment + ghostty 1.3.1 oracle traffic baseline (WS15 T1)

Portal: org.freedesktop.portal.GlobalShortcuts version 2 via
xdg-desktop-portal-gnome 50.0 on GNOME Shell 50.0 (wayland, seat0/tty2).
Oracle traffic captured at /tmp/ws15/oracle-traffic.log on rvm.
Dialog behavior observed: <FILL IN from Step 3>."
git push
```

---

### Task 2: The trigger→action map (headless, pure)

**Files:**
- Modify: `crates/rustty-apprt/Cargo.toml` (add `rustty-cfg-fmt`)
- Modify: `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` (add free functions + test module)

**Interfaces:**
- Consumes: `rustty_config::Config::keybind.set` → `rustty_input::binding::Set::iter_bindings() -> impl Iterator<Item = (&Trigger, &Value)>`; `Value::{Leader, Leaf, LeafChained}`; `Leaf::generic()/LeafChained::generic() -> GenericLeaf { flags: Flags, actions }`; `GenericLeaf::actions_slice() -> &[Action]`; `Flags::global: bool`; `crate::gtk::key::xdg_shortcut_from_trigger(Trigger) -> Option<String>`; `rustty_input::binding::Action::format<W: rustty_cfg_fmt::Writer>(&self, &mut W)`.
- Produces:
  - `pub(crate) fn build_shortcut_map(config: &rustty_config::Config) -> Vec<(String, rustty_input::binding::Action)>` — insertion-ordered, key = XDG shortcut id.
  - `pub(crate) fn action_description(action: &rustty_input::binding::Action) -> String`.

- [x] **Step 1: Add the `rustty-cfg-fmt` dependency**

In `crates/rustty-apprt/Cargo.toml`, in the main `[dependencies]` block, immediately after the `rustty-input = { workspace = true }` line, add:

```toml
# WS15 (GTK GlobalShortcuts): `Binding.Action::format` is generic over
# `rustty_cfg_fmt::Writer`; the portal `BindShortcuts` payload needs each action
# rendered in its parse/format spelling for the shortcut `description` field.
# Already in the graph transitively (rustty-input deps it); named directly here
# because Rust needs a direct edge to `use` the crate. Acyclic: rustty-cfg-fmt is
# a strictly-earlier workspace member with no apprt edge.
rustty-cfg-fmt = { workspace = true }
```

- [x] **Step 2: Write the failing tests**

Append to `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` (the file has no `mod tests` yet — create it at the end of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustty_input::binding::Action;

    /// Build a config carrying the given keybind lines.
    fn config_with(binds: &[&str]) -> rustty_config::Config {
        let mut config = rustty_config::Config::default_config();
        for b in binds {
            config
                .keybind
                .set
                .parse_and_put(b.as_bytes())
                .expect("keybind parses");
        }
        config
    }

    /// PORTS: `global_shortcuts.zig:168-206` — only `global:` leaves with exactly
    /// one action reach the portal map, keyed by the XDG shortcut spelling.
    #[test]
    fn global_shortcuts_map_only_global_single_action_binds() {
        let config = config_with(&[
            "global:super+q=new_window",
            "ctrl+shift+t=new_tab",
            "global:ctrl+alt+shift+f12=toggle_quick_terminal",
        ]);

        let map = build_shortcut_map(&config);

        assert_eq!(
            map,
            vec![
                ("LOGO+q".to_owned(), Action::NewWindow),
                (
                    "SHIFT+CTRL+ALT+F12".to_owned(),
                    Action::ToggleQuickTerminal
                ),
            ],
            "non-global binds are skipped; global binds keep insertion order"
        );
    }

    /// PORTS: `global_shortcuts.zig:179-181` — multi-action (chained) leaves are
    /// skipped for now ("We only allow global keybinds that map to exactly a
    /// single action"). `chain=<action>` appends to the immediately-preceding
    /// binding, turning its leaf into a `LeafChained` with two actions
    /// (`binding.rs:3645`, `parse_and_put(b"chain=…")`).
    #[test]
    fn global_shortcuts_map_skips_chained_actions() {
        let config = config_with(&["global:super+q=new_window", "chain=new_tab"]);
        let map = build_shortcut_map(&config);
        assert!(
            map.is_empty(),
            "a chained (2-action) global leaf is skipped, got {map:?}"
        );
    }

    /// A default config has no `global:` binds at all, so the map is empty and
    /// (per `refresh`) no portal session is ever created.
    #[test]
    fn global_shortcuts_map_empty_by_default() {
        let config = rustty_config::Config::default_config();
        assert!(build_shortcut_map(&config).is_empty());
    }

    /// PORTS: `global_shortcuts.zig:259-263` — the shortcut `description` is the
    /// action rendered in its parse/format spelling.
    #[test]
    fn global_shortcuts_action_description_is_format_spelling() {
        assert_eq!(action_description(&Action::NewWindow), "new_window");
        assert_eq!(
            action_description(&Action::ToggleQuickTerminal),
            "toggle_quick_terminal"
        );
    }
}
```

- [x] **Step 3: Run the tests to verify they fail**

```bash
ssh rvm 'cd ~/projects/rustty && git pull && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -20'
```

Expected: FAIL — `cannot find function build_shortcut_map in this scope` / `cannot find function action_description in this scope`.

- [x] **Step 4: Write the implementation**

In `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`, extend the `use` block at the top of the file (after the existing `use super::config::Config;`) with:

```rust
use rustty_input::binding::{Action, Value};

use crate::gtk::key;
```

and add, immediately after the `impl Default for GlobalShortcuts { … }` block at the end of the file (before the new `mod tests`):

```rust
//---------------------------------------------------------------
// Portal protocol helpers (free functions so they are unit-testable with no
// display and no bus — the oracle keeps them as private methods/`Method` arms
// on the class, which Zig can test in-file and Rust cannot).

/// PORTS: `global_shortcuts.zig:168-206` — the map-build half of `refresh`.
///
/// Walks the config's keybind set and returns the portal shortcut map: an
/// insertion-ordered list of `(xdg shortcut id, action)`. Only `global:` leaves
/// that carry exactly one action are registered; leader (sequence) entries and
/// chained leaves are skipped, matching the reference's `TODO: remove this
/// restriction`. The key is the trigger's XDG spelling because triggers are
/// unique in the keymap (`:64-67`).
///
/// PORT NOTE: the reference formats the trigger into a 1024-byte stack buffer and
/// has a `error.WriteFailed` arm that warns and skips; `xdg_shortcut_from_trigger`
/// returns a growable `String` here so only the `null` arm (an untranslatable
/// trigger — `catch_all`, or a key with no GDK keysym name) can skip a bind.
pub(crate) fn build_shortcut_map(config: &rustty_config::Config) -> Vec<(String, Action)> {
    let mut map: Vec<(String, Action)> = Vec::new();

    for (trigger, value) in config.keybind.set.iter_bindings() {
        // `.leader => continue`, `inline .leaf, .leaf_chained => |l| l.generic()`.
        let leaf = match value {
            Value::Leader(_) => continue,
            Value::Leaf(l) => l.generic(),
            Value::LeafChained(l) => l.generic(),
        };

        // `if (!leaf.flags.global) continue;`
        if !leaf.flags.global {
            continue;
        }

        // "We only allow global keybinds that map to exactly a single action
        // for now." (`:179-181`)
        let actions = leaf.actions_slice();
        if actions.len() != 1 {
            continue;
        }

        let Some(shortcut) = key::xdg_shortcut_from_trigger(*trigger) else {
            continue;
        };

        // `priv.map.put(alloc, trigger, actions[0])` — last write wins.
        match map.iter_mut().find(|(k, _)| *k == shortcut) {
            Some(entry) => entry.1 = actions[0].clone(),
            None => map.push((shortcut, actions[0].clone())),
        }
    }

    map
}

/// PORTS: `global_shortcuts.zig:259-263` — the `description` field of a
/// `BindShortcuts` entry: the action in its parse/format spelling (`"{f}"` of
/// `Binding.Action`), e.g. `new_window`, `goto_tab:3`.
///
/// PORT NOTE: the reference formats into a 256-byte stack buffer and `catch
/// continue`s a binding whose action does not fit. `AllocatingWriter` grows, so
/// no binding is ever dropped for length here. `Action::format`'s writer error is
/// `Infallible` for this writer.
pub(crate) fn action_description(action: &Action) -> String {
    let mut w = rustty_cfg_fmt::AllocatingWriter::init();
    let _ = action.format(&mut w);
    String::from_utf8_lossy(w.written()).into_owned()
}
```

- [x] **Step 5: Run the tests to verify they pass**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -10'
```

Expected: `test result: ok. 4 passed`.

If `global_shortcuts_map_only_global_single_action_binds` fails on the expected key strings, do NOT edit the expectation blindly: print the actual map and confirm it against `crates/rustty-apprt/src/gtk/key.rs:72-96` (`xdg_shortcut_from_trigger` emits `SHIFT+`, `CTRL+`, `ALT+`, `LOGO+` in that fixed order, then the GDK keysym name).

- [x] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/Cargo.toml crates/rustty-apprt/src/gtk/class/global_shortcuts.rs
git commit -m "gtk/global-shortcuts: port the trigger->action map build + action description (WS15 T2)"
git push
```

---

### Task 3: Request tokens and the request object path (headless, pure)

**Files:**
- Modify: `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`

**Interfaces:**
- Consumes: `rustty_os::file::crypto_random_bytes(&mut [u8])`.
- Produces:
  - `pub(crate) fn generate_token() -> String` — `rustty_<7 hex chars>`.
  - `pub(crate) fn request_object_path(unique_name: &str, token: &str) -> String`.
  - `pub(crate) const PORTAL_BUS_NAME/PORTAL_OBJECT_PATH/GLOBAL_SHORTCUTS_IFACE/REQUEST_IFACE/SESSION_IFACE: &str`.

- [x] **Step 1: Write the failing tests**

Add inside the existing `mod tests` in `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`:

```rust
    /// PORTS: `global_shortcuts.zig:625-633` `generateToken`. R2: the reference's
    /// `ghostty_` prefix becomes `rustty_`; the value is a 28-bit random hex,
    /// right-padded with `0` to 7 columns (Zig `{x:0<7}` == Rust `{:0<7x}`).
    #[test]
    fn global_shortcuts_token_shape() {
        for _ in 0..64 {
            let t = generate_token();
            assert_eq!(t.len(), "rustty_".len() + 7, "token={t}");
            assert!(t.starts_with("rustty_"), "token={t}");
            assert!(
                t["rustty_".len()..].bytes().all(|b| b.is_ascii_hexdigit()),
                "token tail must be hex: {t}"
            );
        }
        assert_ne!(
            generate_token(),
            generate_token(),
            "tokens must not be constant"
        );
    }

    /// PORTS: `global_shortcuts.zig:468-497` `getRequestPath`. The unique name
    /// loses its leading `:` and every `.` becomes `_`, e.g. `:1.192` → `1_192`.
    #[test]
    fn global_shortcuts_request_object_path() {
        assert_eq!(
            request_object_path(":1.192", "rustty_abc1230"),
            "/org/freedesktop/portal/desktop/request/1_192/rustty_abc1230"
        );
        assert_eq!(
            request_object_path(":1.2.3", "rustty_0000000"),
            "/org/freedesktop/portal/desktop/request/1_2_3/rustty_0000000"
        );
    }

    /// The generated path must be a valid D-Bus object path (the portal will
    /// reject a malformed one, and `signal_subscribe` would silently never fire).
    #[test]
    fn global_shortcuts_request_object_path_is_valid() {
        let path = request_object_path(":1.192", &generate_token());
        assert!(
            gtk4::glib::Variant::is_object_path(&path),
            "not a valid object path: {path}"
        );
    }
```

- [x] **Step 2: Run the tests to verify they fail**

```bash
ssh rvm 'cd ~/projects/rustty && git pull && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -20'
```

Expected: FAIL — `cannot find function generate_token` / `cannot find function request_object_path`.

- [x] **Step 3: Write the implementation**

Add to `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`, just above `build_shortcut_map`:

```rust
/// The XDG desktop portal's well-known bus name.
pub(crate) const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
/// The XDG desktop portal's root object path.
pub(crate) const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
/// `global_shortcuts.zig:450` — the interface every method call targets.
pub(crate) const GLOBAL_SHORTCUTS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// `global_shortcuts.zig:437` — the interface carrying the `Response` signal.
pub(crate) const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// `global_shortcuts.zig:128` — the interface carrying `Close`.
pub(crate) const SESSION_IFACE: &str = "org.freedesktop.portal.Session";

/// PORTS: `global_shortcuts.zig:625-633` `generateToken`. A random token suitable
/// for a portal `handle_token` / `session_handle_token`.
///
/// R2: the reference prefixes `ghostty_`; rustty prefixes `rustty_`. The reference
/// draws a `u28` and formats `{x:0<7}` (hex, fill `0`, LEFT-aligned to width 7 —
/// i.e. zeros are appended, not prepended); Rust's `{:0<7x}` is the same spec.
pub(crate) fn generate_token() -> String {
    let mut buf = [0u8; 4];
    rustty_os::file::crypto_random_bytes(&mut buf);
    // `std.crypto.random.int(u28)`
    let v = u32::from_ne_bytes(buf) & 0x0fff_ffff;
    format!("rustty_{v:0<7x}")
}

/// PORTS: `global_shortcuts.zig:468-497` `getRequestPath`.
///
/// The portal's `Request` object path must be computed by the CLIENT and
/// subscribed to BEFORE the method call is made: an eager portal implementation
/// can emit `Response` before a client that waited for the method's return value
/// has subscribed, and the signal is delivered exactly once. The path is
/// `/org/freedesktop/portal/desktop/request/<SENDER>/<TOKEN>`, where `<SENDER>` is
/// the connection's unique name with the leading `:` removed and every `.`
/// replaced by `_` (`:1.192` → `1_192`); D-Bus object-path components may not
/// contain `.`.
pub(crate) fn request_object_path(unique_name: &str, token: &str) -> String {
    let sender = unique_name
        .strip_prefix(':')
        .unwrap_or(unique_name)
        .replace('.', "_");
    format!("{PORTAL_OBJECT_PATH}/request/{sender}/{token}")
}
```

- [x] **Step 4: Run the tests to verify they pass**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -10'
```

Expected: `test result: ok. 7 passed`.

- [x] **Step 5: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/gtk/class/global_shortcuts.rs
git commit -m "gtk/global-shortcuts: port generateToken + getRequestPath (WS15 T3)"
git push
```

---

### Task 4: The `CreateSession` / `BindShortcuts` payloads and response parsing (headless, pure)

**Files:**
- Modify: `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`

**Interfaces:**
- Consumes: `build_shortcut_map`, `action_description` (Task 2); `glib::{Variant, VariantDict, VariantTy}`, `glib::variant::ObjectPath`.
- Produces:
  - `pub(crate) fn create_session_payload(request_token: &str, session_token: &str) -> glib::Variant` — type `(a{sv})`.
  - `pub(crate) fn bind_shortcuts_payload(handle: &str, map: &[(String, Action)], request_token: &str) -> Option<glib::Variant>` — type `(oa(sa{sv})sa{sv})`, `None` if `handle` is not a valid object path.
  - `pub(crate) fn session_handle_from_results(results: &glib::Variant) -> Option<String>`.

- [x] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
    /// PORTS: `global_shortcuts.zig:234-241` — the `CreateSession` payload is a
    /// 1-tuple holding the options vardict with both tokens.
    #[test]
    fn global_shortcuts_create_session_payload() {
        let v = create_session_payload("rustty_1111111", "rustty_2222222");
        assert_eq!(v.type_().as_str(), "(a{sv})");

        let dict = gtk4::glib::VariantDict::new(Some(&v.child_value(0)));
        assert_eq!(
            dict.lookup::<String>("handle_token").unwrap().as_deref(),
            Some("rustty_1111111")
        );
        assert_eq!(
            dict.lookup::<String>("session_handle_token")
                .unwrap()
                .as_deref(),
            Some("rustty_2222222")
        );
    }

    /// PORTS: `global_shortcuts.zig:244-279` — the `BindShortcuts` payload is
    /// `(session_handle, [(id, {description, preferred_trigger})], parent_window,
    /// {handle_token})`.
    #[test]
    fn global_shortcuts_bind_shortcuts_payload() {
        let map = vec![
            ("LOGO+q".to_owned(), Action::NewWindow),
            (
                "SHIFT+CTRL+ALT+F12".to_owned(),
                Action::ToggleQuickTerminal,
            ),
        ];
        let v = bind_shortcuts_payload(
            "/org/freedesktop/portal/desktop/session/1_192/rustty_2222222",
            &map,
            "rustty_1111111",
        )
        .expect("valid session handle builds a payload");

        assert_eq!(v.type_().as_str(), "(oa(sa{sv})sa{sv})");
        assert_eq!(
            v.child_value(0).str(),
            Some("/org/freedesktop/portal/desktop/session/1_192/rustty_2222222")
        );

        let shortcuts = v.child_value(1);
        assert_eq!(shortcuts.n_children(), 2);

        let first = shortcuts.child_value(0);
        assert_eq!(first.child_value(0).str(), Some("LOGO+q"));
        let opts = gtk4::glib::VariantDict::new(Some(&first.child_value(1)));
        assert_eq!(
            opts.lookup::<String>("description").unwrap().as_deref(),
            Some("new_window")
        );
        assert_eq!(
            opts.lookup::<String>("preferred_trigger")
                .unwrap()
                .as_deref(),
            Some("LOGO+q")
        );

        // parent_window is always empty (`:274` `''`).
        assert_eq!(v.child_value(2).str(), Some(""));

        let tail = gtk4::glib::VariantDict::new(Some(&v.child_value(3)));
        assert_eq!(
            tail.lookup::<String>("handle_token").unwrap().as_deref(),
            Some("rustty_1111111")
        );
    }

    /// A malformed session handle can never produce a `(o…)` payload — the
    /// reference would have already asserted; here we return `None` and skip.
    #[test]
    fn global_shortcuts_bind_shortcuts_payload_rejects_bad_handle() {
        assert!(bind_shortcuts_payload("not a path", &[], "rustty_1111111").is_none());
    }

    /// PORTS: `global_shortcuts.zig:286-293` — pull `session_handle` out of the
    /// `Response` results vardict. Portals in the wild type this field as either
    /// `s` or `o`; both must be accepted.
    #[test]
    fn global_shortcuts_session_handle_from_results() {
        use gtk4::glib::ToVariant;

        let as_string = gtk4::glib::VariantDict::new(None);
        as_string.insert_value("session_handle", &"/session/a".to_variant());
        assert_eq!(
            session_handle_from_results(&as_string.end()).as_deref(),
            Some("/session/a")
        );

        let as_path = gtk4::glib::VariantDict::new(None);
        as_path.insert_value(
            "session_handle",
            &gtk4::glib::variant::ObjectPath::try_from("/session/b")
                .unwrap()
                .to_variant(),
        );
        assert_eq!(
            session_handle_from_results(&as_path.end()).as_deref(),
            Some("/session/b")
        );

        let empty = gtk4::glib::VariantDict::new(None);
        assert_eq!(session_handle_from_results(&empty.end()), None);
    }
```

- [x] **Step 2: Run the tests to verify they fail**

```bash
ssh rvm 'cd ~/projects/rustty && git pull && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -20'
```

Expected: FAIL — `cannot find function create_session_payload` (and the other two).

- [x] **Step 3: Write the implementation**

Add to `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`, after `request_object_path`:

```rust
/// PORTS: `global_shortcuts.zig:234-241` — the `CreateSession` payload.
///
/// D-Bus signature `(a{sv})`: one options vardict carrying the request's
/// `handle_token` and the session's `session_handle_token`.
/// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>
pub(crate) fn create_session_payload(request_token: &str, session_token: &str) -> glib::Variant {
    use glib::ToVariant as _;

    let options = glib::VariantDict::new(None);
    options.insert_value("handle_token", &request_token.to_variant());
    options.insert_value("session_handle_token", &session_token.to_variant());
    glib::Variant::tuple_from_iter([options.end()])
}

/// PORTS: `global_shortcuts.zig:244-279` — the `BindShortcuts` payload.
///
/// D-Bus signature `(oa(sa{sv})sa{sv})`: the session handle, the shortcut list
/// (each `(id, {description, preferred_trigger})`), an empty parent-window
/// identifier, and the request options carrying `handle_token`.
///
/// Returns `None` when `handle` is not a valid D-Bus object path — the reference
/// reaches this point only with a handle the portal itself just gave us
/// (`:246` `priv.handle orelse return null`), so `None` is the same "skip the
/// request" outcome without a panic.
pub(crate) fn bind_shortcuts_payload(
    handle: &str,
    map: &[(String, Action)],
    request_token: &str,
) -> Option<glib::Variant> {
    use glib::ToVariant as _;

    let session = glib::variant::ObjectPath::try_from(handle).ok()?;
    let entry_ty = glib::VariantTy::new("(sa{sv})").ok()?;

    let entries: Vec<glib::Variant> = map
        .iter()
        .map(|(trigger, action)| {
            let opts = glib::VariantDict::new(None);
            opts.insert_value("description", &action_description(action).to_variant());
            opts.insert_value("preferred_trigger", &trigger.to_variant());
            glib::Variant::tuple_from_iter([trigger.to_variant(), opts.end()])
        })
        .collect();
    let shortcuts = glib::Variant::array_from_iter_with_type(entry_ty, entries);

    let options = glib::VariantDict::new(None);
    options.insert_value("handle_token", &request_token.to_variant());

    Some(glib::Variant::tuple_from_iter([
        session.to_variant(),
        shortcuts,
        "".to_variant(),
        options.end(),
    ]))
}

/// PORTS: `global_shortcuts.zig:286-293` — read `session_handle` out of a
/// `Response` results vardict.
///
/// PORT NOTE: the reference looks the key up as `&s`. Portal implementations
/// disagree on whether this field is `s` or `o`, so we accept either
/// (`glib::Variant::str` covers `s`/`o`/`g`).
pub(crate) fn session_handle_from_results(results: &glib::Variant) -> Option<String> {
    let dict = glib::VariantDict::new(Some(results));
    let value = dict.lookup_value("session_handle", None)?;
    value.str().map(str::to_owned)
}
```

- [x] **Step 4: Run the tests to verify they pass**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -10'
```

Expected: `test result: ok. 11 passed`.

- [x] **Step 5: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/gtk/class/global_shortcuts.rs
git commit -m "gtk/global-shortcuts: port the CreateSession/BindShortcuts payloads + response parsing (WS15 T4)"
git push
```

---

### Task 5: Portal session state, the real `refresh`/`close`, and activation dispatch

**Files:**
- Modify: `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`

**Interfaces:**
- Consumes: everything from Tasks 2–4; `gio::{DBusConnection, SignalSubscriptionId, DBusCallFlags, Cancellable}`.
- Produces:
  - `imp::GlobalShortcuts` fields `map: RefCell<Vec<(String, Action)>>`, `handle: RefCell<Option<String>>`, `response_subscription: RefCell<Option<gio::SignalSubscriptionId>>`, `activate_subscription: RefCell<Option<gio::SignalSubscriptionId>>`.
  - `GlobalShortcuts::refresh(&self)` — real body (map build + `request(Method::CreateSession)`).
  - `GlobalShortcuts::close(&self)` — real body (unsubscribe ×2, `Session.Close`, clear map/handle).
  - `pub(crate) fn GlobalShortcuts::dispatch_activation(&self, shortcut_id: &str)` — map lookup + `trigger` emission; the display-free half of `shortcutActivated`, callable from tests.
  - `pub(crate) fn GlobalShortcuts::shortcut_map(&self) -> Vec<(String, Action)>` — test/inspection accessor.

Task 6 fills `request` / `Method`; until then `refresh` calls a `request` that exists but is stubbed to `return` if no connection — which is exactly its production behavior when the app has no bus.

- [x] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
    /// `refresh` rebuilds the shortcut map from the bound config. With no D-Bus
    /// connection there is no portal session to create, so the map is built and
    /// the round-trip is simply not attempted (this is the headless path).
    #[test]
    fn global_shortcuts_refresh_builds_map_without_dbus() {
        let core = config_with(&["global:super+q=new_window"]);
        let cfg = Config::new(&core);
        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&cfg));

        assert_eq!(
            gs.shortcut_map(),
            vec![("LOGO+q".to_owned(), Action::NewWindow)],
            "notify::config must drive refresh and populate the map"
        );
    }

    /// `close` (which `refresh` always runs first, and `dispose` runs) clears the
    /// portal session state. Re-setting a config with no global binds empties it.
    #[test]
    fn global_shortcuts_refresh_clears_prior_map() {
        let with_global = Config::new(&config_with(&["global:super+q=new_window"]));
        let without_global = Config::new(&config_with(&["ctrl+shift+t=new_tab"]));

        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&with_global));
        assert_eq!(gs.shortcut_map().len(), 1);

        gs.set_config(Some(&without_global));
        assert!(
            gs.shortcut_map().is_empty(),
            "a config with no global binds leaves an empty map"
        );
    }

    /// PORTS: `global_shortcuts.zig:541-565` `shortcutActivated` — the portal's
    /// shortcut id is looked up in the map and the matching action is re-emitted
    /// on the `trigger` signal. This is the whole point of the class.
    #[test]
    fn global_shortcuts_activation_emits_trigger() {
        use std::cell::RefCell as StdRefCell;
        use std::rc::Rc;

        let cfg = Config::new(&config_with(&["global:super+q=new_window"]));
        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&cfg));

        let seen: Rc<StdRefCell<Vec<Action>>> = Rc::new(StdRefCell::new(Vec::new()));
        let seen_cb = seen.clone();
        gs.connect_local("trigger", false, move |args| {
            let boxed: ActionBoxed = args[1].get().expect("trigger carries an ActionBoxed");
            seen_cb.borrow_mut().push(boxed.0);
            None
        });

        gs.dispatch_activation("LOGO+q");
        gs.dispatch_activation("LOGO+never-bound");

        assert_eq!(
            *seen.borrow(),
            vec![Action::NewWindow],
            "only a mapped shortcut id emits trigger"
        );
    }
```

- [x] **Step 2: Run the tests to verify they fail**

```bash
ssh rvm 'cd ~/projects/rustty && git pull && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -20'
```

Expected: FAIL — `no method named shortcut_map` / `no method named dispatch_activation`.

Note on the `set_config` call: gtk4-rs's `#[property(get, set, nullable)]` derive
on `RefCell<Option<Config>>` generates `set_config(&self, value: Option<&Config>)`.
If the compiler reports a different expected argument type, use the property
system directly instead — `gs.set_property("config", &cfg);` — which is the same
write path (`notify::config` still fires, so `refresh` still runs).

- [x] **Step 3: Add the private portal-session state**

In `mod imp`, replace the `pub struct GlobalShortcuts { … }` body's trailing brace region by adding these four fields after the existing `dbus_connection` field (keep both existing property fields and their doc comments exactly as they are):

```rust
        /// PORTS: `global_shortcuts.zig:64-68` `map`. A mapping from the portal
        /// shortcut id to the action it performs. The id is the serialized XDG
        /// spelling of the trigger, which is unique in the keymap. Insertion
        /// order is preserved (the reference uses a `StringArrayHashMapUnmanaged`),
        /// because that is the order the shortcuts are presented to the portal.
        ///
        /// PORT NOTE: the reference allocates the keys in a per-refresh arena
        /// (`:61-62`) and drops the whole arena in `close`; Rust owns the strings
        /// in the `Vec`, so `close` just clears it and the arena field has no
        /// counterpart.
        pub map: RefCell<Vec<(String, rustty_input::binding::Action)>>,

        /// PORTS: `global_shortcuts.zig:70-72` `handle` — the current portal
        /// session's D-Bus object path, or `None` when no session exists.
        pub handle: RefCell<Option<String>>,

        /// PORTS: `global_shortcuts.zig:74-76` `response_subscription` — the
        /// subscription to the in-flight request's `Response` signal. The
        /// reference uses `0` as the null id; gio's `SignalSubscriptionId` is a
        /// non-zero newtype, so `Option` is the faithful spelling.
        pub response_subscription: RefCell<Option<gio::SignalSubscriptionId>>,

        /// PORTS: `global_shortcuts.zig:78-80` `activate_subscription` — the
        /// subscription to the session's `Activated` signal.
        pub activate_subscription: RefCell<Option<gio::SignalSubscriptionId>>,
```

These four fields are plain `RefCell`s with `Default` impls, so the struct's
`#[derive(Default, Properties)]` still applies unchanged (the `Properties` derive
ignores fields with no `#[property]` attribute).

- [x] **Step 4: Replace `refresh` and `close`, and add the two new methods**

In `impl GlobalShortcuts` (the wrapper impl at the bottom of the file), replace the whole existing `fn refresh(&self)` and `fn close(&self)` (currently the two DEFERRAL no-ops) with:

```rust
    /// PORTS: `global_shortcuts.zig:148-213` `refresh`. Re-derive the global
    /// shortcut registration from the current config: close the previous session,
    /// rebuild the trigger→action map, and — if anything is registered — start the
    /// portal round-trip with `CreateSession`.
    fn refresh(&self) {
        // Always close our previous state first (`:150`).
        self.close();

        let imp = self.imp();

        // We need configuration to proceed (`:155`).
        let map = {
            let config = imp.config.borrow();
            let Some(config) = config.as_ref() else {
                return;
            };
            build_shortcut_map(config.get())
        };

        let count = map.len();
        imp.map.replace(map);

        // Create our session if we have global shortcuts (`:212`).
        if count > 0 {
            self.request(Method::CreateSession);
        }
    }

    /// PORTS: `global_shortcuts.zig:109-146` `close`. Tear down the current portal
    /// session: drop both signal subscriptions, ask the portal to close the
    /// session, and clear the map.
    fn close(&self) {
        let imp = self.imp();

        // `const dbus = priv.dbus_connection orelse return;` (`:111`)
        let dbus = { imp.dbus_connection.borrow().clone() };
        let Some(dbus) = dbus else {
            // Without a connection there are no subscriptions and no session, but
            // the map may still hold state from a headless refresh.
            imp.map.replace(Vec::new());
            imp.handle.replace(None);
            return;
        };

        // `:113-121`
        if let Some(id) = imp.response_subscription.borrow_mut().take() {
            dbus.signal_unsubscribe(id);
        }
        if let Some(id) = imp.activate_subscription.borrow_mut().take() {
            dbus.signal_unsubscribe(id);
        }

        // Close the existing session (`:123-139`).
        let handle = imp.handle.borrow_mut().take();
        if let Some(handle) = handle {
            dbus.call(
                Some(PORTAL_BUS_NAME),
                &handle,
                SESSION_IFACE,
                "Close",
                None,
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
                |_res| {},
            );
        }

        // `arena.deinit(); priv.map = .{}` (`:141-145`) — Rust owns the map's
        // strings, so clearing the vec is the whole teardown.
        imp.map.replace(Vec::new());
    }

    /// PORTS: `global_shortcuts.zig:558-564` — the display-free half of
    /// `shortcutActivated`: look the portal's shortcut id up in the map and
    /// re-emit the bound action on the `trigger` signal. Unknown ids are ignored.
    ///
    /// Split out from the D-Bus callback so it can be exercised without a bus.
    pub(crate) fn dispatch_activation(&self, shortcut_id: &str) {
        let action = {
            let map = self.imp().map.borrow();
            map.iter()
                .find(|(k, _)| k == shortcut_id)
                .map(|(_, a)| a.clone())
        };
        let Some(action) = action else {
            return;
        };

        self.emit_by_name::<()>("trigger", &[&ActionBoxed(action)]);
    }

    /// The current portal shortcut map (id → action), in registration order.
    /// rustty-only accessor: the reference reads `priv.map` directly in-file.
    pub(crate) fn shortcut_map(&self) -> Vec<(String, Action)> {
        self.imp().map.borrow().clone()
    }
```

- [x] **Step 5: Add the temporary `request` seam so the file compiles**

Task 6 replaces this with the real body. Add to the same `impl GlobalShortcuts`:

```rust
    /// PORTS: `global_shortcuts.zig:329-460` `request`. Filled in WS15 T6.
    fn request(&self, _method: Method) {}
```

and, above `build_shortcut_map`, the enum it takes:

```rust
/// PORTS: `global_shortcuts.zig:215-326` `Method` — the two portal methods this
/// class drives, in the order it drives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    /// `CreateSession`
    CreateSession,
    /// `BindShortcuts`
    BindShortcuts,
}

impl Method {
    /// PORTS: `global_shortcuts.zig:219-224` `Method.name`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Method::CreateSession => "CreateSession",
            Method::BindShortcuts => "BindShortcuts",
        }
    }
}
```

- [x] **Step 6: Run the tests to verify they pass**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -10'
```

Expected: `test result: ok. 14 passed`.

- [x] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/gtk/class/global_shortcuts.rs
git commit -m "gtk/global-shortcuts: real refresh/close + portal session state + activation dispatch (WS15 T5)"
git push
```

---

### Task 6: The live D-Bus round-trip — `request` and `Method::on_response`

**Files:**
- Modify: `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`

**Interfaces:**
- Consumes: `Method`, `create_session_payload`, `bind_shortcuts_payload`, `session_handle_from_results`, `request_object_path`, `generate_token`, `dispatch_activation` (Tasks 3–5); `gio::DBusConnection::{signal_subscribe, signal_unsubscribe, call, unique_name}`, `gio::DBusSignalFlags::{NONE, MATCH_ARG0_PATH}`.
- Produces: a complete `GlobalShortcuts::request(&self, method: Method)` and `GlobalShortcuts::on_response(&self, method: Method, results: &glib::Variant)`. No new public surface; this task is verified live in Task 8 (a bus round-trip cannot be unit-tested headlessly, which is stated explicitly there).

- [x] **Step 1: Replace the `request` stub with the real implementation**

In `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs`, replace `fn request(&self, _method: Method) {}` with:

```rust
    /// PORTS: `global_shortcuts.zig:329-460` `request`. Submit one request to the
    /// GlobalShortcuts portal.
    ///
    /// PROTOCOL NOTE (`:340-360`, editorializing stripped): the documented flow is
    /// "call the method, it returns a `Request` object path, then listen for that
    /// object's `Response` signal". That flow is racy — a portal implementation may
    /// emit `Response` before the caller has subscribed, and the signal is
    /// delivered exactly once, so the response would be lost. The portal
    /// documentation's own remedy (under the `Request` interface) is for the client
    /// to CONSTRUCT the expected request object path from its own unique bus name
    /// plus the `handle_token` it is about to send, subscribe to `Response` on that
    /// path FIRST, and only then issue the method call — which is what this does,
    /// making the method's return value informational.
    fn request(&self, method: Method) {
        let imp = self.imp();

        let dbus = { imp.dbus_connection.borrow().clone() };
        let Some(dbus) = dbus else {
            // The reference reaches `priv.dbus_connection.?` here; `refresh` is
            // driven by `notify::config` too, which can fire before the connection
            // is set. Returning is the LATENT-CRASH-safe equivalent of a
            // round-trip that cannot be started.
            return;
        };

        let request_token = generate_token();

        // `method.makePayload(self, request_token) orelse return` (`:428`).
        let payload = match method {
            Method::CreateSession => create_session_payload(&request_token, &generate_token()),
            Method::BindShortcuts => {
                let handle = { imp.handle.borrow().clone() };
                let Some(handle) = handle else {
                    return;
                };
                let map = imp.map.borrow();
                let Some(payload) = bind_shortcuts_payload(&handle, map.as_slice(), &request_token)
                else {
                    return;
                };
                payload
            }
        };

        // `getRequestPath` (`:429`). `getUniqueName` never returns null on an
        // ordinary message-bus connection (`:476-478`).
        let Some(unique_name) = dbus.unique_name() else {
            eprintln!("warn(gtk): global shortcuts: dbus connection has no unique name");
            return;
        };
        let request_path = request_object_path(unique_name.as_str(), &request_token);

        // `assert(priv.response_subscription == 0)` (`:434`).
        debug_assert!(
            imp.response_subscription.borrow().is_none(),
            "a global shortcuts request is already in flight"
        );

        // `dbus.signalSubscribe(.., Request, Response, request_path, ..)`
        // (`:435-445`) → `callbacks.responded` (`:390-422`).
        let weak = self.downgrade();
        let subscription = dbus.signal_subscribe(
            None,
            Some(REQUEST_IFACE),
            Some("Response"),
            Some(&request_path),
            None,
            gio::DBusSignalFlags::NONE,
            move |dbus, _sender, _path, _iface, _signal, params| {
                let Some(this) = weak.upgrade() else {
                    return;
                };

                // Unsubscribe from the response signal (`:402-406`).
                if let Some(id) = this.imp().response_subscription.borrow_mut().take() {
                    dbus.signal_unsubscribe(id);
                }

                // `params.get("(u@a{sv})", &response, &vardict)` (`:411`).
                let response: u32 = params.child_value(0).get().unwrap_or(2);
                let results = params.child_value(1);

                match response {
                    // `.{0 => method.onResponse(..)}` (`:414-417`)
                    0 => {
                        eprintln!(
                            "debug(gtk): global shortcuts request successful method={}",
                            method.name()
                        );
                        this.on_response(method, &results);
                    }
                    1 => eprintln!("debug(gtk): global shortcuts request was cancelled by user"),
                    2 => eprintln!("warn(gtk): global shortcuts request ended unexpectedly"),
                    other => {
                        eprintln!("warn(gtk): global shortcuts unrecognized response code={other}")
                    }
                }
            },
        );
        imp.response_subscription.replace(Some(subscription));

        // The method call itself (`:447-459`). Its reply is informational (see the
        // PROTOCOL NOTE); we only log a failure, as `gotResponseHandle` does
        // (`:363-387`).
        let method_name = method.name();
        dbus.call(
            Some(PORTAL_BUS_NAME),
            PORTAL_OBJECT_PATH,
            GLOBAL_SHORTCUTS_IFACE,
            method_name,
            Some(&payload),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            move |res| {
                // `callbacks.gotResponseHandle` (`:363-387`): the reply carries
                // the portal's ACTUAL request object path. The reference carries a
                // TODO to re-point the subscription when it differs from the
                // computed one (for older portals) and does not implement it —
                // neither do we, for the same reason: doing so reintroduces the
                // very race the pre-subscription avoids.
                if let Err(err) = res {
                    eprintln!("warn(gtk): global shortcuts {method_name} request failed={err}");
                }
            },
        );
    }

    /// PORTS: `global_shortcuts.zig:283-325` `Method.onResponse` — what to do once
    /// a request's `Response` arrives with code 0.
    fn on_response(&self, method: Method, results: &glib::Variant) {
        match method {
            Method::CreateSession => {
                // `vardict.lookup("session_handle", "&s", &handle)` (`:287-293`).
                let Some(handle) = session_handle_from_results(results) else {
                    eprintln!(
                        "warn(gtk): global shortcuts session handle not found in response={}",
                        results.print(true)
                    );
                    return;
                };
                eprintln!("debug(gtk): global shortcuts session_handle={handle}");
                self.imp().handle.replace(Some(handle.clone()));

                let dbus = { self.imp().dbus_connection.borrow().clone() };
                let Some(dbus) = dbus else {
                    return;
                };

                // Subscribe to keybind activations (`:304-316`). `arg0` is the
                // session handle, matched as an object path.
                debug_assert!(self.imp().activate_subscription.borrow().is_none());
                let weak = self.downgrade();
                let subscription = dbus.signal_subscribe(
                    None,
                    Some(GLOBAL_SHORTCUTS_IFACE),
                    Some("Activated"),
                    Some(PORTAL_OBJECT_PATH),
                    Some(&handle),
                    gio::DBusSignalFlags::MATCH_ARG0_PATH,
                    move |_dbus, _sender, _path, _iface, _signal, params| {
                        let Some(this) = weak.upgrade() else {
                            return;
                        };
                        // 2nd value in the tuple is the activated shortcut id
                        // (`:552-555`).
                        let Some(id) = params.child_value(1).str().map(str::to_owned) else {
                            return;
                        };
                        eprintln!("debug(gtk): global shortcut activated={id}");
                        this.dispatch_activation(&id);
                    },
                );
                self.imp().activate_subscription.replace(Some(subscription));

                // `shortcuts.request(.bind_shortcuts)` (`:318-321`).
                self.request(Method::BindShortcuts);
            }

            // `.bind_shortcuts => {}` (`:323`).
            //
            // PORT NOTE (rustty-only instrumentation): the reference's arm is
            // empty. We log the accepted binding set, because this is the single
            // observable that proves the compositor took the shortcuts — it is
            // what `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` asserts.
            Method::BindShortcuts => {
                eprintln!(
                    "debug(gtk): global shortcuts bound results={}",
                    results.print(true)
                );
            }
        }
    }
```

- [x] **Step 2: Rewrite the module doc so it no longer advertises a deferral**

Replace the module header's `# DEFERRAL — the XDG portal round-trip is L6 (portals)` section (currently lines 27–42 of the file) with:

```rust
//! # The XDG portal round-trip
//!
//! [`GlobalShortcuts::refresh`] rebuilds the trigger→action map from the config's
//! `global:` keybinds and, when the map is non-empty, drives the portal
//! handshake: `CreateSession` → (on `Response` code 0) subscribe to `Activated`
//! and issue `BindShortcuts`. Each request pre-computes its own `Request` object
//! path from the connection's unique name plus the `handle_token` it is about to
//! send, and subscribes to `Response` on that path BEFORE issuing the call,
//! because a portal may emit `Response` before a naive client has subscribed and
//! the signal is delivered exactly once (`global_shortcuts.zig:340-360`).
//!
//! [`GlobalShortcuts::close`] is the exact inverse: unsubscribe both signals, ask
//! the portal to `Session.Close` the handle, and drop the map. It runs at the head
//! of every `refresh`, on a `dbus-connection` swap, and in `dispose`.
//!
//! An `Activated` signal carries the shortcut id; it is looked up in the map and
//! the bound action is re-emitted on the [`trigger`] signal, which
//! [`super::application`] connects to `core.performAllAction`.
```

- [x] **Step 3: Build and re-run the headless suite**

```bash
ssh rvm 'cd ~/projects/rustty && git pull && cargo build -p rustty-apprt --features gtk 2>&1 | tail -5 && cargo test -p rustty-apprt --features gtk global_shortcuts_ 2>&1 | tail -10'
```

Expected: `Finished` and `test result: ok. 14 passed`. (No new headless test here — the bus round-trip is verified live in Task 8.)

- [x] **Step 4: Clippy must stay clean**

```bash
ssh rvm 'cd ~/projects/rustty && cargo clippy -p rustty-apprt --features gtk 2>&1 | tail -15'
```

Expected: no warnings from `global_shortcuts.rs`.

- [x] **Step 5: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/gtk/class/global_shortcuts.rs
git commit -m "gtk/global-shortcuts: port the portal request/onResponse round-trip over gio::DBusConnection (WS15 T6)"
git push
```

---

### Task 7: Dispatch the triggered action — `perform_all_action_thunk`

**Files:**
- Modify: `crates/rustty-apprt/src/gtk/class/application.rs` (imp field ~line 274, `bind_core_app` ~line 2358, `global_shortcut_trigger` line 778, test module)

**Interfaces:**
- Consumes: `rustty_core::app::App::<A>::performAllAction(&mut self, rt_app: &A, action: rustty_input::binding::Action) -> Result<(), rustty_core::app::Error>`; `ActionBoxed(pub rustty_input::binding::Action)`.
- Produces: `imp::Application::perform_all_action_thunk: RefCell<Option<Box<dyn Fn(rustty_input::binding::Action)>>>`, installed by `Application::bind_core_app::<A>()`; `imp::Application::global_shortcut_trigger` now invokes it. Test-only counter `global_shortcut_trigger_count: Cell<u64>` (same precedent as the existing `tick_count`).

- [x] **Step 1: Write the failing test**

Add to the `mod tests` at the bottom of `crates/rustty-apprt/src/gtk/class/application.rs`, next to `application_activate_pushes_new_window`:

```rust
    /// PORTS: `application.zig:1655-1663` `globalShortcutTrigger`. A `trigger`
    /// from the GlobalShortcuts portal performs the action on the core app via
    /// `performAllAction`. Requires a display only because `build_test_app`
    /// constructs a real `GtkApplication`.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_global_shortcut_trigger_performs_action() {
        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        let mut core_app: Box<CoreApp<Double>> =
            CoreApp::<Double>::create(Global).expect("core app");
        let core_ptr: *mut CoreApp<Double> = &mut *core_app;

        let app = build_test_app(core_ptr.cast());
        assert!(
            app.imp().perform_all_action_thunk.borrow().is_none(),
            "unbound core app installs no dispatch thunk"
        );

        app.bind_core_app::<Double>();
        assert!(
            app.imp().perform_all_action_thunk.borrow().is_some(),
            "bind_core_app must install the performAllAction thunk"
        );

        app.imp()
            .global_shortcut_trigger(&ActionBoxed(rustty_input::binding::Action::NewWindow));

        assert_eq!(
            app.imp().global_shortcut_trigger_count.get(),
            1,
            "the trigger handler must reach the core dispatch exactly once"
        );
    }
```

- [x] **Step 2: Run the test to verify it fails**

```bash
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && git pull && cargo test -p rustty-apprt --features gtk application_global_shortcut -- --ignored 2>&1 | tail -20"'
```

Expected: FAIL — `no field perform_all_action_thunk on type &imp::Application`.

- [x] **Step 3: Add the thunk field and the test counter**

In `crates/rustty-apprt/src/gtk/class/application.rs`, inside `mod imp`'s `pub struct Application`, immediately after the `update_config_thunk` field, add:

```rust
        /// rustty-only seam carrying the monomorphized global-shortcut dispatch
        /// (`self.core().performAllAction(self.rt(), action)`,
        /// `application.zig:1660`, `globalShortcutTrigger`). Same type-erasure
        /// rationale as [`Self::tick_thunk`]: the non-generic `Application` cannot
        /// name the concrete `rustty_core::app::App<A>` its type-erased
        /// `core_app`/`rt_app` pointers refer to, so
        /// [`super::Application::bind_core_app`] captures both here. `None` until
        /// bound — a global shortcut then performs nothing (faithful to a
        /// not-yet-wired core).
        #[allow(clippy::type_complexity)]
        pub perform_all_action_thunk:
            RefCell<Option<Box<dyn Fn(rustty_input::binding::Action)>>>,
```

and, next to the existing `#[cfg(test)] pub tick_count: Cell<u64>` field, add:

```rust
        /// Test-only instrumentation: how many times a portal `trigger` reached
        /// the core dispatch (`application.zig:1660` leaves no observable, so the
        /// port adds one — the `tick_count` precedent). `#[cfg(test)]` so
        /// production carries no extra field.
        #[cfg(test)]
        pub global_shortcut_trigger_count: Cell<u64>,
```

- [x] **Step 4: Install the thunk in `bind_core_app`**

In `Application::bind_core_app`, immediately after the `color_scheme_thunk` installation block (which ends with the per-surface `colorSchemeCallback` loop), add:

```rust
        // The global-shortcut dispatch (`globalShortcutTrigger`,
        // `application.zig:1655-1663`): `core_app.performAllAction(rt_app,
        // action)`. `performAllAction` takes `&mut App<A>` and `rt_app: &A`; both
        // are BACKREFs owned by libghostty-core / the apprt `App`, never freed
        // here. It runs on the app thread (the GTK main loop delivers the portal
        // `Activated` signal there).
        self.imp()
            .perform_all_action_thunk
            .replace(Some(Box::new(move |action: rustty_input::binding::Action| {
                let Some(rt_app) = rt_app else {
                    return;
                };
                // SAFETY: as above — app-thread-only access to the two backrefs.
                if let Err(_err) =
                    unsafe { &mut *core_app }.performAllAction(unsafe { &*rt_app }, action)
                {
                    // log.warn("failed to perform action={}", .{err});
                    eprintln!("warn(gtk): failed to perform global shortcut action");
                }
            })));
```

- [x] **Step 5: Fill `global_shortcut_trigger`**

Replace the placeholder at `crates/rustty-apprt/src/gtk/class/application.rs:778` — the whole doc comment plus `fn global_shortcut_trigger(&self, _action: &ActionBoxed) {}` — with:

```rust
        /// PORTS: `application.zig:1655-1663` `globalShortcutTrigger`. Perform the
        /// activated global shortcut's action across all surfaces, via the
        /// monomorphized [`Self::perform_all_action_thunk`]
        /// ([`super::Application::bind_core_app`] installs it). An unbound core
        /// performs nothing.
        ///
        /// `pub(super)` (the reference's method is file-private) so the KAT in the
        /// outer module's `mod tests` can drive it directly; `mod imp`'s own call
        /// site in `startup_global_shortcuts` is unaffected.
        pub(super) fn global_shortcut_trigger(&self, action: &ActionBoxed) {
            #[cfg(test)]
            self.global_shortcut_trigger_count
                .set(self.global_shortcut_trigger_count.get() + 1);

            if let Some(thunk) = self.perform_all_action_thunk.borrow().as_ref() {
                thunk(action.0.clone());
            }
        }
```

- [x] **Step 6: Run the test to verify it passes**

```bash
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk application_global_shortcut -- --ignored 2>&1 | tail -10"'
```

Expected: `test result: ok. 1 passed`.

- [x] **Step 7: Run the whole gtk suite (headless + display)**

```bash
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk 2>&1 | tail -5 && cargo test -p rustty-apprt --features gtk -- --ignored 2>&1 | tail -5"'
```

Expected: both `test result: ok`, 0 failed. Record the two counts in the commit message.

- [x] **Step 8: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/gtk/class/application.rs
git commit -m "gtk/application: dispatch global shortcut triggers through core performAllAction (WS15 T7)"
git push
```

---

### Task 8: Live portal round-trip on the VM's real GNOME session

**Files:**
- Create: `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`

**Interfaces:**
- Consumes: the built `target/debug/rustty` binary on the VM; the `debug(gtk):` log lines emitted by Tasks 5–6 (`global shortcuts session_handle=`, `global shortcuts request successful method=`, `global shortcuts bound results=`, `global shortcut activated=`).
- Produces: an executable gate script that exits 0 only when the full round-trip AND an activation dispatch are observed.

**This is the real verification, not a compile check.** It runs against the live `xdg-desktop-portal-gnome` on the VM's active GNOME session.

**Automation boundary (stated explicitly):** the portal handshake (`CreateSession` → `BindShortcuts` → `Response 0`) IS fully automatable and is asserted by this script. Synthesizing a real, physical key press is NOT automatable on this VM: mutter ignores `wtype`'s virtual-keyboard protocol, and the kernel-level alternative (`ydotool`, `/dev/uinput`) needs `sudo`, which requires an interactive password here. Therefore activation is verified in two parts: **(a)** automated — an `Activated` signal is injected on the session bus with `gdbus emit` and must produce a dispatch (this exercises the exact subscription, the arg0 match, the map lookup, the `trigger` emission and the `performAllAction` call — every line of code the real signal takes, differing only in who put the signal on the bus); **(b)** manual — the physical key press, in Step 5, with the exact steps and the exact expected output.

- [x] **Step 1: Write the gate script**

Create `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`:

```bash
#!/usr/bin/env bash
# ws15_globalshortcuts_portal_gate.sh — WS15 live gate (LINUX / VM ONLY)
#
# Proves the XDG GlobalShortcuts portal round-trip actually completes against
# the running desktop, and that an `Activated` signal dispatches the bound
# action.
#
# Run ON THE VM inside the GNOME session's environment:
#   ssh rvm 'cd ~/projects/rustty && \
#     XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 \
#     DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus \
#     .port/tools/gates/ws15_globalshortcuts_portal_gate.sh'
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORK="${TMPDIR:-/tmp}/ws15-gate"
APP_LOG="$WORK/app.log"
BUS_LOG="$WORK/bus.log"
SHORTCUT="SHIFT+CTRL+ALT+F12"
BIN="$RUSTTY_ROOT/target/debug/rustty"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

# --- preconditions -----------------------------------------------------------
[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ] || fail "DBUS_SESSION_BUS_ADDRESS unset"
[ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] || fail "no WAYLAND_DISPLAY/DISPLAY"
[ -x "$BIN" ] || fail "missing $BIN (run: cargo build -p rustty --features gtk)"

version="$(gdbus introspect --session --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop 2>/dev/null \
  | awk '/interface org.freedesktop.portal.GlobalShortcuts/,/^  };/' \
  | sed -n 's/.*readonly u version = \([0-9]*\);.*/\1/p')"
[ -n "$version" ] || fail "org.freedesktop.portal.GlobalShortcuts not present on the session bus"
echo "portal GlobalShortcuts version=$version"

# --- isolated config with one global keybind ---------------------------------
rm -rf "$WORK"; mkdir -p "$WORK/cfg/rustty"
printf 'keybind = global:ctrl+alt+shift+f12=new_window\n' > "$WORK/cfg/rustty/config.rustty"

dbus-monitor --session "path_namespace=/org/freedesktop/portal/desktop" > "$BUS_LOG" 2>&1 &
MON_PID=$!
sleep 1

XDG_CONFIG_HOME="$WORK/cfg" "$BIN" > "$APP_LOG" 2>&1 &
APP_PID=$!
cleanup() { kill "$APP_PID" "$MON_PID" 2>/dev/null; wait "$APP_PID" 2>/dev/null; }
trap cleanup EXIT

# --- wait for the round-trip -------------------------------------------------
handle=""
for _ in $(seq 1 40); do
  sleep 0.5
  handle="$(sed -n 's/.*global shortcuts session_handle=\(.*\)$/\1/p' "$APP_LOG" | head -1)"
  [ -n "$handle" ] && break
done
[ -n "$handle" ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  echo "--- bus log (portal) ---"; grep -E "member=(CreateSession|BindShortcuts|Response)" "$BUS_LOG" | head -20
  fail "no portal session handle: CreateSession never got Response 0"
}
echo "session_handle=$handle"

bound=0
for _ in $(seq 1 60); do
  if grep -q "global shortcuts bound results=" "$APP_LOG"; then bound=1; break; fi
  sleep 0.5
done
[ "$bound" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "BindShortcuts never completed (a GNOME permission dialog may be waiting on the VM's screen — see the plan's Task 8 Step 3)"
}
grep -q "$SHORTCUT" "$APP_LOG" \
  || fail "BindShortcuts results do not mention $SHORTCUT"
echo "BindShortcuts accepted $SHORTCUT"

# --- activation dispatch -----------------------------------------------------
gdbus emit --session \
  --object-path /org/freedesktop/portal/desktop \
  --signal org.freedesktop.portal.GlobalShortcuts.Activated \
  "objectpath '$handle'" "'$SHORTCUT'" "uint64 0" "@a{sv} {}" \
  || fail "gdbus emit failed"

activated=0
for _ in $(seq 1 20); do
  if grep -q "global shortcut activated=$SHORTCUT" "$APP_LOG"; then activated=1; break; fi
  sleep 0.5
done
[ "$activated" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "Activated signal was not received/dispatched"
}

echo "GATE PASS: portal round-trip + activation dispatch confirmed"
```

- [x] **Step 2: Make it executable, build the app on the VM, and run the gate**

```bash
cd /Users/gael/projects/rustty && chmod +x .port/tools/gates/ws15_globalshortcuts_portal_gate.sh
git add .port/tools/gates/ws15_globalshortcuts_portal_gate.sh && git commit -m "gates: WS15 live GlobalShortcuts portal gate (WS15 T8)" && git push
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && git pull && cargo build -p rustty --features gtk 2>&1 | tail -3 && \
.port/tools/gates/ws15_globalshortcuts_portal_gate.sh"'
```

Expected final line: `GATE PASS: portal round-trip + activation dispatch confirmed`.

- [x] **Step 3: If the gate stops at "BindShortcuts never completed"**

GNOME 50 presents a confirmation dialog for `BindShortcuts` (Task 1 Step 3 recorded whether it does). It appears on the VM's physical screen, so it must be dismissed there. Re-run the gate and, at the VM console, click **Allow** on the "…wants to register global shortcuts" dialog while the gate is waiting (it waits 30s). GNOME remembers the grant per app id (`com.mitchellh.ghostty-debug`), so subsequent runs complete unattended — confirm by re-running the gate and seeing it pass with no interaction. Record that in the commit message.

- [x] **Step 4: Cross-check against the oracle's traffic**

```bash
ssh rvm 'grep -E "member=(CreateSession|BindShortcuts)" /tmp/ws15-gate/bus.log | head -10; echo "=== oracle ==="; grep -E "member=(CreateSession|BindShortcuts)" /tmp/ws15/oracle-traffic.log | head -10'
```

Expected: rustty issues the same two method calls, on the same interface and object path, in the same order as ghostty 1.3.1 did in Task 1. Any difference in the method sequence is a parity bug — fix it before proceeding.

- [x] **Step 5: MANUAL verification — a real key press (cannot be automated here; see the Automation boundary note above)**

Perform these at the VM's physical console/display (not over SSH):

1. Over SSH, write a launcher and run it in the foreground so its log streams back:
   ```bash
   ssh rvm 'cat > /tmp/ws15-manual.sh' <<'EOF'
   #!/usr/bin/env bash
   set -u
   export XDG_RUNTIME_DIR=/run/user/1000
   export WAYLAND_DISPLAY=wayland-0
   export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
   export GDK_BACKEND=wayland
   mkdir -p /tmp/ws15-manual/cfg/rustty
   printf 'keybind = global:ctrl+alt+shift+f12=new_window\n' \
     > /tmp/ws15-manual/cfg/rustty/config.rustty
   cd "$HOME/projects/rustty" || exit 1
   XDG_CONFIG_HOME=/tmp/ws15-manual/cfg ./target/debug/rustty 2>&1 \
     | tee /tmp/ws15-manual/app.log
   EOF
   ssh rvm 'chmod +x /tmp/ws15-manual.sh && /tmp/ws15-manual.sh'
   ```
2. At the VM console, give focus to **any other application** (e.g. Files or the GNOME overview) — the point is that Rustty is NOT focused.
3. Press **Ctrl+Alt+Shift+F12**.
4. Back in the SSH tail, the log must show, in order:
   ```
   debug(gtk): global shortcut activated=SHIFT+CTRL+ALT+F12
   ```
   and a NEW Rustty window must appear on the VM's screen (the bound action is `new_window`), while the previously focused app was in the foreground.

**Success criteria:** the `global shortcut activated=` line appears with no Rustty window focused, AND a new window opens. That is the end-to-end proof that `global:` keybinds work. If the line does not appear, check GNOME Settings → Keyboard → Shortcuts for the registered entry (GNOME may have assigned a different trigger than `preferred_trigger`; the portal is allowed to, and the `Activated` id is still our shortcut id, so the log line is the authority, not the key combination).

- [x] **Step 6: Commit the verification result**

```bash
cd /Users/gael/projects/rustty
git commit --allow-empty -m "verify: WS15 live GlobalShortcuts round-trip + activation on rvm GNOME 50 (WS15 T8)

Gate output: GATE PASS (session_handle=<FILL IN>, BindShortcuts accepted
SHIFT+CTRL+ALT+F12, injected Activated dispatched).
Manual key press (Ctrl+Alt+Shift+F12, Rustty unfocused): <FILL IN — logged
'global shortcut activated=SHIFT+CTRL+ALT+F12' and opened a new window>.
Permission dialog required on first bind: <FILL IN yes/no>."
git push
```

---

### Task 9: Both-platform build gate

**Files:** none (verification only)

**Interfaces:**
- Consumes: the complete WS15 tree.
- Produces: the roadmap's standing both-platform green result.

- [x] **Step 1: Linux (VM) full workspace build + tests**

```bash
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && git pull && cargo build --workspace 2>&1 | tail -3 && \
cargo test -p rustty-apprt --features gtk 2>&1 | tail -3 && \
cargo test -p rustty-apprt --features gtk -- --ignored 2>&1 | tail -3"'
```

Expected: `Finished`, and both `test result: ok` lines with `0 failed`.

- [x] **Step 2: macOS full workspace build + the apprt suite (run by the controller on macOS)**

```bash
cd /Users/gael/projects/rustty && git pull && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-apprt 2>&1 | tail -3
```

Expected: `Finished` and `test result: ok`. Note WHY this must be green despite WS15 being Linux-only: the one macOS-visible change is the new `rustty-cfg-fmt` dependency line in `crates/rustty-apprt/Cargo.toml`.

- [x] **Step 3: The acyclic gate (the new crate edge must not create a cycle)**

```bash
cd /Users/gael/projects/rustty && .port/tools/gates/check_acyclic.sh
```

Expected: GREEN (exit 0).

- [x] **Step 4: Commit the gate evidence**

```bash
cd /Users/gael/projects/rustty
git commit --allow-empty -m "gate: WS15 both-platform build green (macOS + rvm) + G7 acyclic (WS15 T9)

Linux: cargo build --workspace Finished; rustty-apprt gtk suite <N> passed,
ignored suite <M> passed.
macOS: cargo build --workspace Finished; rustty-apprt <K> passed.
check_acyclic.sh: GREEN (new rustty-apprt -> rustty-cfg-fmt edge)."
git push
```

---

### Task 10: Strike the DEFERRALS L6 entry

**Files:**
- Modify: `.port/phase-2/DEFERRALS.md:3982-4022`

**Interfaces:**
- Consumes: the landed WS15 implementation.
- Produces: a truthful ledger — the entry is struck in the established style (`~~heading~~` + a `**RESOLVED (WS15)**` paragraph), not deleted.

- [x] **Step 1: Rewrite the entry**

Replace the heading at line 3982 and the whole body through line 4022 with:

```markdown
## ~~GlobalShortcuts XDG-portal round-trip (L6 portals)~~ [added 2026-07-18, T26 GlobalShortcuts]

**RESOLVED (WS15, 2026-07-29).** The XDG portal round-trip is PORTED and
verified live. `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` now
implements the reference's full body (`global_shortcuts.zig:109-460,541-565`):
`refresh` builds the trigger→action map from `config.keybind.set` (`global:`
leaves with exactly one action, keyed by `key::xdg_shortcut_from_trigger`) and
drives `CreateSession` → `BindShortcuts`; `request` pre-computes the `Request`
object path from the connection's unique name + `handle_token` and subscribes to
`Response` BEFORE issuing the call; `on_response` stores the session handle,
subscribes to `Activated` (arg0-path-matched on the session handle) and chains
`BindShortcuts`; `close` unsubscribes both signals, calls `Session.Close` and
clears the map. The D-Bus stack is `gio::DBusConnection` (the connection the
`Application` already hands the class) — no new D-Bus dependency.

`Application::global_shortcut_trigger` is no longer a placeholder-leaf either: a
`perform_all_action_thunk` (installed by `bind_core_app`, same shape as
`tick_thunk`/`update_config_thunk`) carries `core_app.performAllAction(rt_app,
action)` across the type-erasure seam, so an activated global shortcut performs
its action. `global:` keybinds are FUNCTIONAL on the GTK apprt.

**Verification:** 14 headless unit tests (map build, token shape, request path,
both payload shapes, response parsing, activation → `trigger` emission) plus one
display test (`application_global_shortcut_trigger_performs_action`), plus the
live gate `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` run against
`xdg-desktop-portal-gnome` 50.0 on the VM's GNOME Shell 50 Wayland session
(`org.freedesktop.portal.GlobalShortcuts` version 2): CreateSession/BindShortcuts
both answered `Response 0`, and an `Activated` signal dispatched the bound
action. A physical key press with Rustty unfocused was verified manually (WS15
T8 Step 5) — key injection cannot be automated on this VM (mutter ignores
`wtype`; `ydotool`/uinput needs root).

**Still deferred (SEPARATE, unchanged):** the sibling portal `OpenURI` call site
in `application.zig` (the INHABITED placeholder at
`crates/rustty-apprt/src/gtk/class/application.rs`'s `startup` — the D-Bus
connection is fetched but has no OpenURI consumer). WS15 does not cover it.
```

- [x] **Step 2: Check off this plan's completed tasks**

Tick every `- [ ]` box in `docs/superpowers/plans/2026-07-29-ws15-gtk-globalshortcuts.md` that has been executed.

- [x] **Step 3: Verify no other ledger row still claims the deferral**

```bash
cd /Users/gael/projects/rustty
grep -rn "GlobalShortcuts" .port/phase-2/DEFERRALS.md .port/phase-2/VERIFICATION_DEBT.md .port/phase-2/MAIN_PARITY.md | grep -vi "resolved\|WS15"
```

Expected: no output. If a row appears (e.g. an L6 roll-up listing GlobalShortcuts among open items), amend it in the same commit to point at the RESOLVED entry.

- [x] **Step 4: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/DEFERRALS.md docs/superpowers/plans/2026-07-29-ws15-gtk-globalshortcuts.md
git commit -m "parity: strike the L6 GlobalShortcuts portal deferral — round-trip landed (WS15 T10)"
git push
```

---

### Task 11: Merge to main and clean up

**Files:** none (git flow, per the roadmap: "branch per WS; merge to main only with both platforms green").

- [ ] **Step 1: Re-confirm both platforms are green on the final tree**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3
ssh rvm 'cd ~/projects/rustty && git pull && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-apprt --features gtk 2>&1 | tail -3'
```

Expected: `Finished` on both; `test result: ok` with 0 failed on the VM.

- [ ] **Step 2: Merge**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git merge --no-ff ws15-gtk-globalshortcuts -m "merge: WS15 GTK GlobalShortcuts — XDG portal round-trip, global: keybinds functional"
git push
```

- [ ] **Step 3: Verify the merge result builds on both platforms**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull && cargo build --workspace 2>&1 | tail -3'
```

Expected: `Finished` on both.

- [ ] **Step 4: Re-run the live gate on merged main (regression guard)**

```bash
ssh rvm 'bash -lc "
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
cd ~/projects/rustty && cargo build -p rustty --features gtk 2>&1 | tail -2 && \
.port/tools/gates/ws15_globalshortcuts_portal_gate.sh"'
```

Expected: `GATE PASS: portal round-trip + activation dispatch confirmed`.

- [ ] **Step 5: Delete the branch and update the roadmap checklist**

```bash
cd /Users/gael/projects/rustty
git branch -d ws15-gtk-globalshortcuts && git push origin --delete ws15-gtk-globalshortcuts
ssh rvm 'cd ~/projects/rustty && git branch -D ws15-gtk-globalshortcuts 2>/dev/null; true'
```

Then in `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`, update the VM-track row to point at this plan file (`2026-07-29-ws15-gtk-globalshortcuts.md`, not the placeholder `2026-07-23-…` name) and mark WS15 AUTHORED + EXECUTED + MERGED in the plan-authoring checklist. Commit:

```bash
cd /Users/gael/projects/rustty
git add docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "docs: WS15 merged — GTK GlobalShortcuts portal round-trip (WS15 T11)"
git push
```

- [ ] **Step 6: Confirm the working tree is clean**

```bash
cd /Users/gael/projects/rustty && git status --short
```

Expected: no output.

---

## Execution notes (WS15, 2026-07-29)

Tasks 1–10 executed. **Task 11 (merge to main + branch deletion) was NOT
executed** — the executing controller withheld merge authority; the branch
`ws15-gtk-globalshortcuts` is pushed and intact.

Deltas from the plan as written, all forced by facts the plan could not know:

1. **Task 1 needed two extra environment steps to get any oracle traffic at
   all.** `ghostty` launched from an ssh shell (a) is absorbed by the session's
   already-running single instance, and (b) once past that, gets
   `org.freedesktop.portal.Error.NotAllowed: "An app id is required"` from
   `CreateSession`. The capture therefore runs
   `ghostty --gtk-single-instance=false` inside
   `systemd-run --user --scope --slice=app.slice --unit=app-<appid>-N.scope`.
   The WS15 gate does the same for `rustty` and additionally installs a
   user-level `io.airely.rustty-debug.desktop`.

2. **The GNOME 50 `BindShortcuts` dialog IS automatable** — the plan assumed it
   was not. Its "Add" button exposes an AT-SPI `click` action and `pyatspi` is
   present on the VM, so the gate answers it with no root and no uinput. GNOME
   then remembers the grant per app id and later runs are unattended (verified:
   a second gate run reports "no permission dialog appeared").

3. **Task 6 uses `gio::DBusConnection::subscribe_to_signal`, not the plan's
   `signal_subscribe`.** gio 0.22.8 deprecates `signal_subscribe` /
   `signal_unsubscribe`; using them makes clippy warn, and the gates are
   invariant (R4/R5). The replacement returns an RAII `SignalSubscription`, so
   the private fields are `RefCell<Option<gio::SignalSubscription>>` and
   `close`'s explicit unsubscribe is a `.take()` + drop.

4. **Task 8 Step 5 (physical key press) remains genuinely manual.** AT-SPI key
   synthesis was tried: `Atspi.generate_keyboard_event` returns success but
   mutter ignores it on Wayland, so no activation results. What is proven
   automatically instead is that gnome-settings-daemon's own store holds the
   accepted binding, i.e. the compositor grabbed the accelerator.

5. **The display-gated `--ignored` suite cannot be run as one process.** It
   fails wholesale with `gtk4::init(): Attempted to initialize GTK from two
   different threads`, and `--test-threads=1` does not help. This is
   PRE-EXISTING — verified identical on `origin/main` (2 passed / 19 failed).
   Display KATs must be run one `--filter` at a time; done that way the new
   `application_global_shortcut_trigger_performs_action` passes.
