# Rustty Linux/GTK — Manual Test Plan

A top-to-bottom checklist to validate the GTK terminal against **Ghostty** parity. Run each,
compare to Ghostty on the same machine, and note any divergence.

> **Scope:** this covers the Linux/GTK apprt (the native GTK4/libadwaita backend). It does **not**
> apply to the macOS build (different apprt). Items marked **[deferred]** are not wired yet — don't
> test them as bugs.

## 0. Setup (dev build)

A dev/cargo build needs a `share/`-style resources layout (a packaged install resolves this
automatically). One-time:

```sh
SHARE=/home/gael/rustty-dev-resources
mkdir -p "$SHARE/rustty" "$SHARE/terminfo"
ln -sfn <repo>/crates/rustty-extra/shell-integration "$SHARE/rustty/shell-integration"
sed 's/xterm-ghostty|ghostty|Ghostty/xterm-rustty|rustty|Rustty/' \
  <repo>/.port/phase-0/corpora/terminfo/ghostty.terminfo > /tmp/xterm-rustty.terminfo
tic -x -o "$SHARE/terminfo" /tmp/xterm-rustty.terminfo
```

Launch:
```sh
export TMPDIR=/home/gael/tmp RUSTTY_RESOURCES_DIR=$SHARE/rustty
<repo>/target/debug/rustty
```

- [ ] **0.1** A window opens with a libadwaita header bar (title, `+`, tab-overview, hamburger, close).
- [ ] **0.2** A shell prompt renders as crisp glyphs (not a blank/black area).

## 1. Rendering & fonts
- [ ] **1.1** Type `printf '%s\n' 'the quick brown fox 0123' 'ABC <>{}[]()#@&*'` — all glyphs render correctly.
- [ ] **1.2** Resize the window — the grid reflows; a **"cols × rows" overlay** briefly appears (parity: Ghostty).
- [ ] **1.3** Run a full-screen TUI (`vim`, `htop`, `less /etc/services`) — renders + scrolls correctly (needs terminfo, see 0).

## 2. Keyboard input
- [ ] **2.1** A freshly-opened terminal accepts typing **without clicking first**.
- [ ] **2.2** `ctrl+c` interrupts a running `sleep 100`; `ctrl+d` exits the shell.
- [ ] **2.3** `ctrl+l` (or `clear`) clears the screen. **(Was broken pre-terminfo-fix — confirm it clears.)**
- [ ] **2.4** Unicode / dead keys / IME compose (if configured) produce the right characters.

## 3. Clipboard
- [ ] **3.1 Copy:** select text, `ctrl+shift+c`, paste into another app — your selection appears.
- [ ] **3.2 Paste:** copy from another app, `ctrl+shift+v` in the terminal — it pastes.
- [ ] **3.3 Round-trip:** `echo hello`, select `hello`, copy, paste.
- [ ] **3.4 Unsafe-paste confirm:** copy **multi-line** text, `ctrl+shift+v` → a **"Warning: Potentially
      Unsafe Paste"** dialog with the content + **Deny/Allow**. Allow pastes; Deny doesn't. (Single-line pastes directly, no dialog.)

## 4. Close-confirmation dialogs
Run `sleep 300` first (so the surface "needs confirm"), then:
- [ ] **4.1** Close the **window** (X) → **"Close Window?"** dialog. Cancel keeps it; Close closes it.
- [ ] **4.2** New tab (`+`), `sleep 300`, close the **tab** → **"Close Tab?"**.
- [ ] **4.3** Split (if bound), `sleep 300`, close the **split** → **"Close Split?"**.
- [ ] **4.4** At a bare prompt (no running command) closing happens **without** a dialog.

## 5. Tab title
- [ ] **5.1** Right-click the **tab** → **"Change Tab Title…"** → dialog with an entry → type → OK → tab renames.
- [ ] **5.2** The hamburger/main-menu **"Change Tab Title…"** does the same.
- [ ] **5.3** OK with an empty field restores the default title; Cancel makes no change.

## 6. Menus & keyboard shortcuts
- [ ] **6.1** Open the **hamburger menu** — items show their **keyboard shortcuts** on the right (parity: Ghostty).
- [ ] **6.2** Right-click the terminal → **context menu** appears; **Escape/click-away dismisses it without crashing**.

## 7. Window / tab title tracking
- [ ] **7.1** The window title reflects the shell (e.g. `~`), not a static "Rustty"; `cd` into a dir updates it.
      (Exact string depends on shell integration; parity is "tracks the surface, like Ghostty".)

## 8. Bell
- [ ] **8.1** `printf '\a'` (or a command that beeps) → **border flash** + system beep / urgency hint.

## 9. Mouse
- [ ] **9.1** Cursor shape changes to an **I-beam** over text (and a **hand** over a URL).
- [ ] **9.2** The pointer **auto-hides while typing** and reappears on mouse move.
- [ ] **9.3** Hover a URL (e.g. `echo https://example.com`) → it **highlights**; **click/ctrl-click opens it** in the browser.

## 10. Drag & drop
- [ ] **10.1** Drag a **file** from the file manager onto the terminal → its **path is pasted** (shell-escaped). **(Was a crash pre-fix — confirm no crash.)**
- [ ] **10.2** Drag **text** onto the terminal → the text is pasted.

## 11. Config
- [ ] **11.1** `theme = catppuccin-mocha` in `~/.config/rustty/config.rustty` → colors change (bundled themes now resolve).
- [ ] **11.2** Reload config (menu / keybind / `kill -USR2 <pid>`) → changes apply to **live** windows (colors, font size).
- [ ] **11.3** A config with a bad key → a **config-errors dialog** lists the diagnostics.

## 12. Tab / window management
- [ ] **12.1** `ctrl+shift+t` new tab; next/prev/`alt+N` **switch tabs**; reorder (move-tab) keybind moves the tab.
- [ ] **12.2** Fullscreen keybind toggles fullscreen; maximize toggles maximize.

## 13. Overlays
- [ ] **13.1** Resize → the size overlay (13.: see 1.2).
- [ ] **13.2** Search keybind (`ctrl+shift+f` or configured) opens a **search bar**; typing highlights matches; next/prev navigate; Esc closes. **[verify — new in T24]**
- [ ] **13.3** A **leader-key** sequence (if configured) shows a **key-state overlay** of the pending keys.
- [ ] **13.4** When the child process **exits** (type `exit`), a **child-exited banner** appears with a close button.
- [ ] **13.5** A shell emitting **OSC 9;4** progress shows a **progress bar**.

## 14. Command palette
- [ ] **14.1** The command-palette keybind (`ctrl+shift+p` / configured) opens a **searchable command list**; picking one runs it. **[verify — new in T25]**

## 15. Notifications
- [ ] **15.1** OSC 777 / notify-on-command-finish → a **desktop notification** appears.

## 16. CLI IPC (single-instance)
Launch a primary: `rustty --gtk-single-instance=true &`, then:
- [ ] **16.1** `rustty +new-window` → a **new window** opens in the running instance.
- [ ] **16.2** `rustty +new-window -e echo hi` → new window running that command (argv override implemented 2026-07-19).
- [ ] **16.3** `rustty +toggle-quick-terminal` (Wayland build on wlroots/KDE) → toggles the quick terminal. NOTE: the **keybind** path is verified working; the **D-Bus IPC** path has an unconfirmed render asymmetry (toggle/IPC code is verified faithful to ghostty — see DEFERRALS).

---

## Window-protocol integration (implemented behind feature flags)

The Wayland (`--features gtk,wayland`) and X11 (`--features gtk,x11`) winproto arms are implemented +
runtime-verified (App::init selects the right protocol — `cargo test -p rustty-apprt --features
gtk,wayland --test gtk_wayland_winproto -- --ignored` and the `gtk,x11` counterpart). The default
`--features gtk` build uses the noop arm (correct on GNOME).
- [ ] **W.1 (KDE/wlr only):** with a KDE or wlroots session, build `--features gtk,wayland`, set
      `background-blur = true` → the window background blurs; `window-decoration = server` → server-side
      decorations. On GNOME/mutter these are inert by design (mutter lacks the protocols — matches Ghostty).
- [ ] **W.2:** a backgrounded terminal's bell requests attention (taskbar flash). Works on X11; on
      **Wayland** now via a GSource that pumps the registry queue so the `xdg-activation` token `done`
      completes (RESOLVED 2026-07-19; machine-verified on a live wlroots compositor (headless sway) by the
      `wayland_set_urgent_dispatches_activation_token` KAT — real token request → `done` → `activate`:
      `cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --ignored
      --exact wayland_set_urgent_dispatches_activation_token`).
- [ ] **[implemented 2026-07-19] wlr layer-shell quick-terminal** (drop-down): bind a key to
      `toggle_quick_terminal` (e.g. `keybind = super+grave_accent=toggle_quick_terminal`), build
      `--features gtk,wayland`, and run on a **wlroots** compositor (sway/Hyprland) or KDE → a
      full-width drop-down anchors at the top (verified on sway 1.11). Inert on GNOME/mutter (no
      wlr-layer-shell — `supports_quick_terminal()` returns false, graceful).

### X11 arm (WS16, 2026-07-29) — run the binary with `--features gtk,x11`

Note the `rustty` BINARY needs its own pass-through feature: `cargo build -p rustty
--features gtk,x11` (a plain `--features gtk` binary is noop-only and none of the rows
below will show anything). Under a Wayland session, force the X11 backend with
`GDK_BACKEND=x11` plus `DISPLAY=:0` and an `XAUTHORITY` from the session; the app log
must print `debug(gtk): windowing protocol=X11`.
- [ ] **X.1:** a window appears and `xprop -id <wid> WM_CLASS` reads
      `"rustty-debug", "io.airely.rustty-debug"` (release: `"rustty", "io.airely.rustty"`).
- [ ] **X.2:** `--window-decoration=server` → `xprop -id <wid> _MOTIF_WM_HINTS` has
      decorations (3rd) word `0x1` and the WM draws the title bar; the default reads
      `0x0` with the app's own header bar.
- [ ] **X.3:** `--background-blur=true` → `_KDE_NET_WM_BLUR_BEHIND_REGION` is a
      CARDINAL rect list (4-tuples); without it the property is absent. (VISIBLE blur
      needs KWin — see `VERIFICATION_DEBT.md` V5.)
- [ ] **X.4:** `rustty -e /bin/sh -c 'printenv WINDOWID'` prints an XID that
      `xprop -id <that XID> WM_CLASS` resolves to the same window.
- [ ] **X.5:** ring the bell in an UNFOCUSED window (`printf '\a'`) → `xprop -id <wid>
      WM_HINTS` gains `The urgency hint bit is set`; it is absent before the bell.
- [ ] **X.6:** press Shift, then Ctrl, on a PHYSICAL keyboard in the focused window →
      stderr logs `debug(gtk-x11): found extra XkbStateNotify event w/lookup_mods: …`.
      SSH-injected input (xdotool/XTEST) does NOT work for this on a mutter session —
      see `VERIFICATION_DEBT.md` V5 item 1.

## Deferred / not-yet-wired (do NOT test as bugs)

- **[implemented 2026-07-19] Terminal inspector (Dear ImGui over OpenGL):** press **Ctrl+Shift+I** to
  toggle a separate "Terminal Inspector" window with the live panels (Terminal / Surface / Keyboard /
  Terminal IO / Renderer / Dear ImGui Demo). Type in the terminal → the Keyboard panel logs the events;
  run shell output → the Terminal IO panel logs the VT stream. Toggling again closes it. Regression
  anchor: `RUSTTY_DUMP_PPM=/tmp/i.ppm cargo test -p rustty-apprt --features gtk --test gtk_inspector_dump
  -- --ignored --exact inspector_dump` (needs a display). NOTE: a native-Xorg session is required to
  screenshot the GLArea content (`import` reads blank under XWayland; the KAT's framebuffer readback is
  the portable path).
- **[at-parity, not a gap] OSC color/palette runtime change, secure input, undo/redo:** unimplemented in
  Ghostty's GTK apprt too.

## Regression anchors (automated — `cargo test -p rustty-apprt --features gtk --test <name> -- --ignored`)
`gtk_first_light`, `gtk_window_live`, `gtk_glyph_dump` (rendering); `gtk_input_focus` (overlays don't
capture input); `gtk_close_confirmation`, `gtk_title_dialog`, `gtk_clipboard`, `gtk_clipboard_confirm`
(dialogs/clipboard); `gtk_parity_callbacks` (the newly-wired callbacks don't abort).
