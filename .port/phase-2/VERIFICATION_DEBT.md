# VERIFICATION_DEBT — environment-gated checks this VM cannot run

Status snapshot 2026-07-19 @ `9597128` (post xev-libxev-completion merge). This file
tracks **verification debt only**: work that is implemented and believed correct, whose
*confirmation* needs hardware/environments the development VM (Fedora 44 aarch64,
llvmpipe, GNOME/mutter + nested sway, no macOS toolchain) does not have. It is NOT a
deferral ledger — implementation deferrals live in `DEFERRALS.md`. Each item carries the
exact recipe so whoever has the environment can run it without archaeology.

When an item is verified, move its outcome into `DEFERRALS.md`/the relevant test plan
and delete it here.

---

## ~~V1. macOS build + test of `rustty-xev`~~ — **DISCHARGED 2026-07-23; ROW DELETED [WS19 T14, 2026-07-29]**

This row's body has been **deleted**, per this file's own header rule ("When an item is
verified … delete it here"). It was a claimed-open item that had shipped weeks earlier —
audit row **W11**: `ROADMAP:41` records "VD:V1 (rustty-xev never compiled on macOS) —
DISCHARGED 2026-07-23", and every workstream since gates on `cargo build --workspace` +
`cargo test` on macOS as a merge condition (ROADMAP sequencing rule 2), which compiles
and runs `rustty-xev` including the `__ulock_wait`/`__ulock_wake` shim and the
`thread_pool` lifecycle tests.

The audit trail lives in the `# PHASE 2 CLOSURE AUDIT (WS19)` section of
`.port/phase-2/DEFERRALS.md` (row W11), which is why deleting the body here loses
nothing. This one-paragraph stub is kept ONLY so a reader who arrives from an old
reference to "V1" is not left wondering whether the row was dropped by accident; it
carries no open work and no recipe.

---

## V1b. macOS terminal inspector (cmd+opt+i) — SIGNED OFF [WS6, 2026-07-28]

**Status: CLOSED.** This entry records the macOS counterpart of the Linux
inspector debt in V2 item 3 (which stays OPEN for its frame-time-plot check —
that is a VM-track item).

The macOS Dear ImGui / Metal inspector is runtime-verified. On the WS6 branch
HEAD, `bash macos/build-rustty.sh` → launch → **cmd+opt+i** opens a live,
rendering inspector docked under the terminal, with the
Terminal / Surface / Keyboard / Terminal IO / Renderer tab bar and live values:

- **Terminal** — Screen Status table (primary `active`, alternate
  `(not initialized)`) + Misc/Layout/Mouse/Color/Modes sections.
- **Screen** — Info/Cell/PageList sub-tabs with a real cursor position.
- **Keyboard** — per-key press/release rows with Mods / UTF-8 / PTY-encoding /
  Binding columns, populated by real typing.
- **Terminal IO** — the live VT event stream (`csi`/`esc`/`osc` records).

Evidence (screenshots): `/tmp/ws6/inspector-open.png`,
`/tmp/ws6/inspector-keyboard.png`, `/tmp/ws6/inspector-after-input.png`,
`/tmp/ws6/inspector-screen.png`, `/tmp/ws6/v2-inspector-open.png`.

**HiDPI style rescale** (the WS6 T3 deliverable) was verified by an A/B rebuild
rather than a monitor swap (only one built-in Retina display was available):
with T3 reverted the ImGui chrome is frozen at the unscaled 1.0 defaults
(inspector row pitch ~21 px, `/tmp/ws6/ab-NO-T3.png`); with T3 it is the default
style run through `ScaleAllSizes` at the Retina backing scale (row pitch ~27 px,
i.e. `FramePadding.y` 3 → 6, exactly `ImTrunc(v * 2.0)`).

Backend rasterization is additionally gated in CI-able form by the offscreen
real-`MTLDevice` readback KAT
`dcimgui-sys::metal_offscreen_render_writes_pixels`, and the style rescale by
`update_content_scale_rescales_active_imgui_style` /
`update_content_scale_is_idempotent_not_compounding`.

**Residual (minor):** a genuine cross-DPI drag-between-monitors was not possible
on this host (only one built-in Retina display was available). The WS6 plan's
T5 Step 6 offered a single-display fallback — System Settings → Displays →
toggle the built-in display between a HiDPI and a non-HiDPI (1x) resolution —
and that fallback was **not attempted**. The A/B rebuild (above) proves T3
*changes rendering* (unscaled vs. Retina-scaled style), but nothing exercised
the *runtime* re-stamp path on a live density change: `updateSize()` calling
`convertToBacking` is the resize path, while the density-change path is
`viewDidChangeBackingProperties` → `setContentScale` → `updateContentScale`,
which was read and confirmed present but never triggered end-to-end.
Re-verify opportunistically on a multi-display machine, or by exercising the
1x/HiDPI resolution-toggle fallback on a single-display machine.

## V2. Bare-metal GPU rendering/perf sign-off (blocked: VM is llvmpipe-only)

**Context.** ALL Linux/GTK rendering validation to date ran on llvmpipe (software GL
4.6 core; virtio-gpu, no hardware acceleration). Per the 2026-07-18 decision this is a
legitimate correctness-validation environment (upstream Ghostty 1.3.1 runs here and
rustty's glyph output was validated against it — see `docs/manual-test-plan-gtk.md`
and the `gtk_glyph_dump` harness), so ONLY perf/behavior-under-real-GL remains open.

**Recipe (any Linux box with a real GPU — amdgpu/intel/nvidia):**
```
cargo build --release --features gtk
RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/release/rustty
```
1. Glyphs crisp at multiple font sizes + HiDPI scale (compare side-by-side with
   distro ghostty on the same box).
2. Sustained-throughput smoke: `cat` a large file; window resize while streaming —
   no tearing/stutter beyond what ghostty shows on the same machine.
3. Inspector (`Ctrl+Shift+I`, build with `--features gtk,imgui`): frame-time plot
   sane under load, comparable to ghostty's inspector on the same hardware.
4. Confirm the GL context is the hardware driver (`GDK_DEBUG=opengl` startup lines),
   not llvmpipe.

## V3. KDE Plasma (KWin) quick-terminal + decorations polish (blocked: no KDE here)

**Context.** The wayland winproto arm is implemented + reviewed FAITHFUL, and the
wlr-layer-shell quick terminal is verified end-to-end on real sway (keybind + D-Bus
IPC both render). The three KDE-only globals are wired but no KWin was available:
`org_kde_kwin_slide` (compositor-side QT slide animation), `org_kde_kwin_blur`, and
`org_kde_kwin_server_decoration`. On sway these are gracefully absent (verified); on
GNOME the whole arm is a verified non-gap.

**Recipe (KDE Plasma Wayland session):**
```
cargo build --features gtk,wayland
RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/debug/rustty
```
1. Quick terminal (`toggle_quick_terminal` keybind + `rustty +toggle-quick-terminal`):
   drops with KWin's slide animation (the slide is compositor-side — its *presence*
   is the KDE-only assertion; position/size were already validated on sway).
2. `background-opacity < 1.0` → KWin blur behind the window (`org_kde_kwin_blur`).
3. `window-decoration = server` → KWin server-side decorations
   (`org_kde_kwin_server_decoration`), and CSD fallback when set to `client`.
4. Repeat `docs/manual-test-plan-gtk.md` §W (the KDE/wlr-flagged rows).

---

**Out of scope by decision (not debt):** ~~the WebGL/wasm apprt (owner decision,
2026-07)~~ **[WS19 T14 correction: the WebGL/wasm surface is now OWNED — routed to WS14,
audit rows O13a/O14/O15/O16 — so it is scheduled work, not an out-of-scope decision]**;
~~macOS `File`/`TCP`/`UDP` watcher instantiations (tracked in `DEFERRALS.md`
as the C7 follow-up chunk — an implementation deferral, not verification debt)~~
**[WS19 T11 correction: reclassified as a permanent Justified R1 Divergence, with a
4-criterion basis, in `DEFERRALS.md`'s divergence section — it is neither debt nor a
deferral, and `crates/rustty-xev/tests/surface_parity.rs` pins it]**.

## V4. `set_urgent` KAT on the primary GNOME/mutter session (avoided: crash correlation)

**Context.** The set_urgent activation-token dispatch seam is RESOLVED and
runtime-verified on an ISOLATED headless sway (see `DEFERRALS.md` 2026-07-19 entry;
`wayland_set_urgent_dispatches_activation_token`). Two hard machine crashes on this VM
coincided with live-compositor KAT activity on the PRIMARY GNOME session (abrupt
journal end, no guest OOM/panic — hypervisor-style death; root cause not attributed),
so the mutter-session run was deliberately not repeated. mutter implements the same
`xdg_activation_v1` protocol; this is belt-and-suspenders, not a correctness gap.

**Recipe (any stable GNOME Wayland session):**
```
cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_set_urgent \
  -- --ignored --exact wayland_set_urgent_dispatches_activation_token
```
> **RECIPE RETARGETED [WS19 T4, 2026-07-29 — audit row N3].** The binary was
> ~~`--test gtk_wayland_winproto`~~ and would have run the WRONG binary. That file
> carried TWO `gtk4::init()` calls, which can never both pass (GTK is thread-pinned;
> libtest gives every `#[test]` its own thread even at `--test-threads=1`), so this
> test was split into its own binary `tests/gtk_wayland_set_urgent.rs` following the
> WS16 precedent `a7fa3b7`. Gate **G-GTKINIT**
> (`.port/tools/gates/check_gtk_one_init.sh`) now fails if any test binary regains a
> second `gtk4::init()`.
**Expected:** `ok` in ~1 s (bounded at ~10 s — a broken seam fails, never hangs).

## V5. X11 winproto residuals: no native X11 session, no KWin, and NO synthetic keyboard input on this VM [WS16, 2026-07-29]

> **OWNER [WS19 audit, 2026-07-29]: WS17.** V5 was filed by WS16 on 2026-07-29, six
> days AFTER the WS17 plan was authored (2026-07-23), so WS17's stated scope
> ("V2 real-GPU sign-off, V3 KDE/KWin, V4 mutter `set_urgent`, and WS6's V1b cross-DPI
> residual") does **not** name it — V5 was ORPHANED. It is verification debt, not an
> implementation deferral, and its recipe below is already runnable, so it belongs to
> WS17: **items 2 and 3 are discharged by the very KDE-Plasma-on-X11 box V3 already
> requires** (a native X11 session *and* KWin), and item 1 (a physical keypress) is the
> one part that needs real hardware input. WS19 T14 adds V5 to WS17's scope table; WS17
> must either discharge it on that box or restate it as a precisely-worded residual.

**Context.** The x11 winproto arm is complete and verified live (WS16) but ONLY under
`GDK_BACKEND=x11` against the VM's rootless `Xwayland :0` inside a GNOME/mutter
Wayland session. That environment provides a real X server, real XIDs, real
properties, and mutter as the X window manager — so the arm selection
(`windowing protocol=X11` in the app log), `WM_CLASS`, `_MOTIF_WM_HINTS` (CSD `0x0`
vs SSD `0x1`), `WINDOWID`, XKB extension init (`base_event_code = 85`), the
blur-property write/delete, and the `WM_HINTS` urgency bit are all genuinely
exercised, each against a negative control. THREE things it cannot show:

1. **`App::event_mods` returning `Some(mods)` — the pending-`XkbStateNotify` half.**
   This needs a real keypress still queued when `event_mods` runs.
   **NO form of synthetic keyboard input reaches ANY application on this session.**
   Proven, not assumed: `xdotool type --window` (XSendEvent) is ignored by GTK as
   synthetic; plain `xdotool type` (XTEST) with the target window holding confirmed X
   focus (`xdotool getwindowfocus` == the window's XID) also delivered nothing — and
   the same injection into an INDEPENDENT X client (`zenity --entry`, likewise
   focused) captured an empty string. So the observed `0` occurrences of
   `debug(gtk-x11): found extra XkbStateNotify event w/lookup_mods: …` says nothing
   about the port: no keystroke ever arrived. (mutter routes XTEST through
   libei/RemoteDesktop, which an SSH session cannot obtain.) The code path itself IS
   live, not dead: `gtk/key.rs:206` `event_mods` calls
   `app_winproto.event_mods(device, gtk_mods)`, reached from
   `class/surface.rs:2602`, matching the reference's `key.zig:153` /
   `surface.zig:1382`. The idle-queue half IS automated
   (`tests/gtk_x11_event_mods.rs`), and it also guards the load-bearing
   `XEventsQueued` check — without it `XPeekEvent` blocks forever and that KAT hangs.
2. **A native X11 login session.** The VM has no GNOME X11 session installed
   (`/usr/share/xsessions/` holds only `xfce.desktop`; `gnome-session-xsession` is
   absent) and `Xvfb` is not installed; selecting the Xfce-on-Xorg session requires a
   human at the GDM greeter. Anything that differs between "GTK app forced onto the
   X11 backend inside a Wayland session" and "GTK app in a real X11 session"
   (XSETTINGS/xrdb scale factors, session-type-dependent portal behavior) is
   untested. Nothing in `winproto/x11.zig` reads those, so this is
   belt-and-suspenders.
3. **KWin blur.** `_KDE_NET_WM_BLUR_BEHIND_REGION` is a KDE protocol; under mutter
   only the property write is observable, never the visual effect. Same limitation as
   V3 for the Wayland arm.

**Recipe (a machine with a real X11 session and a console keyboard; KDE Plasma X11
covers all three):**
```
cargo build -p rustty --features gtk,x11
RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/debug/rustty
```
1. `xprop -id $(xdotool search --class rustty | tail -1) WM_CLASS` →
   `"rustty-debug", "io.airely.rustty-debug"` (release build: `"rustty"`,
   `"io.airely.rustty"`).
2. `--window-decoration=server` → `_MOTIF_WM_HINTS` decorations word (3rd) `0x1` AND
   a WM-drawn title bar; default → `0x0` AND the app's CSD header bar.
3. On KDE/KWin: `--background-blur=true --background-opacity=0.8` → VISIBLE blur
   behind the window.
4. **Press Shift, then Ctrl, on a PHYSICAL keyboard in the focused window** and
   confirm the ported XKB peek fires — stderr shows
   `debug(gtk-x11): found extra XkbStateNotify event w/lookup_mods: 1` (bit 0 =
   Shift) / `... : 100` (bit 2 = Control). This is item 1 above and is the only one
   of the four that needs real hardware input rather than just a different session.
