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

## V2. Bare-metal GPU rendering/perf sign-off (blocked: VM is llvmpipe-only) — **OWNER: WS17**

> **OWNER [WS19 T19, 2026-07-29]: WS17 (verification debt).** V2 had no owner written
> anywhere in its entry — it was implicitly WS17's (WS17's Goal names "V2 real-GPU
> sign-off"), but nothing in THIS file said so, and an implicit owner is how V5 became an
> orphan. Gate **G-P2** now requires every open row here to name an owner in an OPEN
> workstream AND to carry a runnable recipe (a fenced code block), because this file's own
> header promises "the exact recipe so whoever has the environment can run it without
> archaeology". WS17 T1 additionally lands `hw_gl_probe.sh` so V2 becomes a one-command
> check with a fixture-testable PASS arm.

**Context.** ALL Linux/GTK rendering validation to date ran on llvmpipe (software GL
4.6 core; virtio-gpu, no hardware acceleration). Per the 2026-07-18 decision this is a
legitimate correctness-validation environment (upstream Ghostty 1.3.1 runs here and
rustty's glyph output was validated against it — see `docs/manual-test-plan-gtk.md`
and the `gtk_glyph_dump` harness), so ONLY perf/behavior-under-real-GL remains open.

**STATUS [WS17, 2026-07-29]: STILL OPEN, and NOT dischargeable on this hardware — with a
turnkey one-command check now shipped so it costs nothing to answer on a real box.**

**Why it cannot be discharged here (established, not assumed).**
* The VM's hypervisor is UTM's **Apple Virtualization** backend, which gives Linux guests
  no 3D at all: `journalctl -k` reports
  `virtio_gpu … features: -virgl -edid -resource_blob -host_visible`.
* Converting the VM to the QEMU/virgl backend needs roughly 40 GB of host free space for
  the disk conversion. The host does not have it — and during this very workstream the
  macOS host hit **0 bytes free** twice, which is the same constraint from the other side.
* Even when this host DID run virgl (2026-07-17, recorded in
  `crates/rustty-renderer/src/opengl/gl/sys.rs`) it exposed
  **`Max core profile version: 0.0`** against rustty's **4.3-core floor** — every
  core-profile request from 3.2 to 4.6 failed `EGL_BAD_MATCH`. So virgl would not have
  discharged V2 even if it were free to enable.

**What WS17 shipped instead.** `.port/tools/gates/hw_gl_probe.sh` — a turnkey,
**non-vacuous** check that exits 0 only on a genuinely accelerated GL ≥ 4.3-core stack.
It fails on llvmpipe / `Accelerated: no` / a core profile below 4.3, and it is
self-testing:

```
# Both arms, on any box, no GPU required — proves the PASS arm is reachable:
.port/tools/gates/hw_gl_probe.sh --self-test
# On the box you actually want to sign off:
.port/tools/gates/hw_gl_probe.sh          # exit 0 => V2 can be executed here
```

Measured 2026-07-29:
* `--self-test` → GREEN. `fixtures/glxinfo-hw-radeonsi.txt` (**synthetic**, labelled as
  such in the file) exits **0**; `fixtures/glxinfo-llvmpipe.txt` (a **real** capture from
  this VM) exits **1**. A gate whose PASS arm has never been observed is
  indistinguishable from one that always fails; this is why the synthetic fixture exists,
  and it proves the script's logic only — never that any hardware was tested.
* Live on the VM → **FAIL**, for the right two reasons: `Accelerated: no` and
  `renderer is a software rasterizer: llvmpipe`. Note the third check *passed*
  (`core profile 4.6 >= 4.3`), which is exactly the trap: a probe that only looked at the
  GL version would have called this box adequate.
* The self-test earned its keep immediately — it caught two real parse bugs in the first
  cut of the script (an unanchored `Accelerated:` match that read the fixture's own
  explanatory header, and a `^Max core profile version:` anchor that missed glxinfo's
  indentation). Both arms reported INCONCLUSIVE rather than passing, which is the
  designed behavior for an unparseable input.

**Same-box narrowing run (NOT a sign-off).** `evidence/ws17/v2-llvmpipe-vs-ghostty.txt`:
rustty RELEASE vs `ghostty-1.3.1-2.fc44` on the SAME llvmpipe stack, wall clock for
`-e /bin/sh -c "cat <4.9 MB file>"`, 3 reps each — rustty **0.45 / 0.40 / 0.40 s**,
ghostty **0.38 / 0.41 / 0.39 s**: parity within noise, no same-environment regression. This
narrows V2 (it rules out a gross regression against the oracle build) and does not discharge
it, because both sides are software-rendered. **A trap worth knowing:** the first pass
compared rustty's DEBUG build against the packaged ghostty and produced 54 s vs 0.4 s — a
~135x gap that looks like a catastrophic regression and is purely the build profile.

**Recipe to actually discharge V2 (any Linux box with a real GPU — amdgpu/intel/nvidia):**
```
.port/tools/gates/hw_gl_probe.sh            # must exit 0 FIRST; if it does not, stop
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
   not llvmpipe — and re-run `hw_gl_probe.sh` to record it.

**Honest residual wording (do not upgrade this).** V2 is **verification debt with a
runnable recipe**, never a deferral: the renderer code is implemented and its correctness
was validated on llvmpipe against upstream ghostty 1.3.1 on the same box. What is
unmeasured is **behaviour and performance under a real GL driver**. No claim of GPU
sign-off may be made until `hw_gl_probe.sh` exits 0 somewhere and steps 1–4 are captured.

## V3. KDE Plasma (KWin) — decoration globals DISCHARGED under a live KWin [WS17, 2026-07-29]; slide + visual blur remain

**LEDGER CORRECTION FIRST (this entry was factually wrong).** V3's original text listed
the KDE-only globals as `org_kde_kwin_slide`, **`org_kde_kwin_blur`** and
`org_kde_kwin_server_decoration`. **`org_kde_kwin_blur` is not used by either tree.** Blur
rides **`ext_background_effect_v1`**: the oracle at
`ghostty/src/apprt/gtk/winproto/wayland.zig:156`, and rustty via
`Tag::ExtBackgroundEffect` in `winproto/wayland/globals.rs`. A code-level grep for
`org_kde_kwin_blur` under `crates/*/src/` returns **0** sites, and
`.port/tools/gates/ws17_kde_globals_gate.sh` now asserts that statically so the claim
cannot silently rot back.

**DISCHARGED: the KDE decoration globals really do bind under a real KWin.**
`.port/tools/gates/ws17_kde_globals_gate.sh` +
`crates/rustty-apprt/tests/gtk_wayland_kde_globals.rs`, GREEN 2026-07-29, evidence in
`.port/phase-2/evidence/ws17/v3-kde-globals-under-kwin.txt`. KWin **6.7.3** runs NESTED on
its own socket with a throwaway `XDG_CONFIG_HOME`, so GDM, the default systemd target and
the primary tty2 GNOME session are untouched (uptime unchanged across the run; no
leftover compositor or socket). Measured bind set:

| | nested KWin 6.7.3 | GNOME/mutter 50 (negative control) |
|---|---|---|
| `org_kde_kwin_server_decoration_manager` | **true** | false |
| `default_deco_mode` (raw) | **Some(2)** = Server | None |
| `org_kde_kwin_slide_manager` | false | false |
| `ext_background_effect` | **true** | false |
| blur capability advertised | false | false |
| `xdg_activation_v1` | true | true |

**NEGATIVE CONTROL, run by the gate and required to fail:** the SAME KAT on GNOME/mutter
FAILS at `org_kde_kwin_server_decoration_manager did not bind`. If it passed there the
positive arm would prove nothing. The KAT additionally asserts `xdg_activation_v1` bound
(so a KDE-global failure cannot be blamed on a wholly broken registry walk) and that
`default_mode` actually arrived (so a bound manager whose event never dispatched is
caught).

**Two residuals, both narrowed by measurement rather than assumed:**
1. **`org_kde_kwin_slide_manager` (quick-terminal slide animation) is NOT advertised by a
   bare nested `kwin_wayland`.** The global is registered by KWin's `slidingpopups`
   builtin effect, which this configuration cannot load —
   `org.kde.kwin.Effects.loadEffect("slidingpopups")` returns **`false`** (measured).
   `org_kde_kwin_slide` IS present in `libkwin.so.6.7.3`, so this is a compositor
   *configuration* limit, not an absence of support. Needs a full Plasma session.
2. **Visible blur is still unverified, and now for a sharper reason than "no KDE":** KWin
   6.7.3 DOES advertise `ext_background_effect`, but with the blur capability **absent**
   (`blur_capability=false`), so `Window::sync_blur` correctly early-returns. A compositor
   that advertises the blur capability is required — not merely any KDE.

**Recipe for the remainder (a full KDE Plasma Wayland session):**
```
.port/tools/gates/ws17_kde_globals_gate.sh   # re-run: kde_slide_manager should now be true
cargo build --features gtk,wayland
RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/debug/rustty
```
1. Quick terminal (`toggle_quick_terminal` keybind + `rustty +toggle-quick-terminal`):
   drops with KWin's slide animation (the slide is compositor-side — its *presence*
   is the KDE-only assertion; position/size were already validated on sway). The KAT's
   `kde_slide_manager=true` line is the machine-checkable half.
2. `background-opacity < 1.0` → blur behind the window, on a compositor whose
   `ext_background_effect` advertises the blur capability (KWin 6.7.3 does not).
3. `window-decoration = server` → KWin server-side decorations, and CSD fallback when set
   to `client`. The globals half of this is already DISCHARGED above; what remains is the
   visual confirmation.
4. Repeat `docs/manual-test-plan-gtk.md` §W (the KDE/wlr-flagged rows).

**Rootless KWin (no `sudo` password available on this VM — a blocker the WS17 plan did not
anticipate; `sudo -n` fails and the user is not in a passwordless sudoers rule):**
```
mkdir -p /tmp/kwinrpm && cd /tmp/kwinrpm
dnf download --disablerepo=cursor --resolve kwin        # 99 packages, ~94 MB, NO root
mkdir root && cd root && for f in ../*.rpm; do rpm2cpio "$f" | cpio -idmu --quiet; done
# kwin_wayland then runs from the prefix; see the gate for the env it needs.
```
Two traps the gate encodes: `kwin_wayland` needs the **parent** `WAYLAND_DISPLAY` or it
falls through to the DRM backend and exits 1 with an EMPTY log; and its socket file appears
BEFORE it accepts clients, so a client that connects too early fails with a bare
`Failed to initialize GTK`. The gate waits for `org.kde.KWin` on the session bus instead.

---

**Out of scope by decision (not debt):** ~~the WebGL/wasm apprt (owner decision,
2026-07)~~ **[WS19 T14 correction: the WebGL/wasm surface is now OWNED — routed to WS14,
audit rows O13a/O14/O15/O16 — so it is scheduled work, not an out-of-scope decision]**;
~~macOS `File`/`TCP`/`UDP` watcher instantiations (tracked in `DEFERRALS.md`
as the C7 follow-up chunk — an implementation deferral, not verification debt)~~
**[WS19 T11 correction: reclassified as a permanent Justified R1 Divergence, with a
4-criterion basis, in `DEFERRALS.md`'s divergence section — it is neither debt nor a
deferral, and `crates/rustty-xev/tests/surface_parity.rs` pins it]**.

## V4. `set_urgent` KAT on the primary GNOME/mutter session (avoided: crash correlation) — **OWNER: WS17**

> **OWNER [WS19 T19, 2026-07-29]: WS17 (verification debt).** Gate G-P2 flagged this row
> as owned only by CLOSED workstreams (WS16, WS19 appear in its text as the lanes that
> touched it, which is provenance, not ownership). WS17's Goal names "V4 mutter
> `set_urgent`", and WS17 T9 runs it under a NESTED/headless compositor rather than the
> primary session — which is the whole point, since two hard host crashes correlated with
> primary-session KAT activity. The recipe below was retargeted by WS19 T4 to the new
> `gtk_wayland_set_urgent` binary; verify that before relying on it.

**Context.** The set_urgent activation-token dispatch seam is RESOLVED and
runtime-verified on an ISOLATED headless sway (see `DEFERRALS.md` 2026-07-19 entry;
`wayland_set_urgent_dispatches_activation_token`). Two hard machine crashes on this VM
coincided with live-compositor KAT activity on the PRIMARY GNOME session (abrupt
journal end, no guest OOM/panic — hypervisor-style death; root cause not attributed),
so the mutter-session run was deliberately not repeated. mutter implements the same
`xdg_activation_v1` protocol; this is belt-and-suspenders, not a correctness gap.

**DISCHARGED against REAL mutter [WS17, 2026-07-29] — without going near the primary
session.** `.port/tools/gates/ws17_set_urgent_mutter_gate.sh`, GREEN; evidence in
`.port/phase-2/evidence/ws17/v4-set-urgent-real-mutter.txt`.

The crash risk is removed by running a **SECOND, HEADLESS instance of real mutter** —
`mutter 50.0 --headless --virtual-monitor 1280x720 --wayland-display wayland-mutterkat` —
on its own socket. `--headless` needs no DRM device and no seat, so it cannot contend with
the primary tty2 GNOME session, and GDM and the default systemd target are untouched. This
is mutter itself, not a stand-in for it, so the "never verified on mutter" gap is closed.
`wayland_set_urgent_dispatches_activation_token` PASSES there: mutter's real
`xdg_activation_token_v1.done` dispatched through `WlQueueSource` and bumped
`App::activation_done_count`. **VM uptime 14:43 before and 14:43 after — no crash, and the
gate records uptime on both sides of every run.**

**NEGATIVE CONTROL, run by the gate and required to fail:** the same KAT pointed at a
socket no compositor serves. It fails. **Be precise about what that control proves** — it
fails at `gtk4::init()` ("Failed to initialize GTK", i.e. no display), so it proves the KAT
genuinely requires a live compositor; it does NOT independently prove the counter is driven
by a *token* rather than by connecting. The stronger control — a compositor that serves
clients but does not implement `xdg_activation_v1` — is not available here (mutter, KWin and
sway all implement it). The counter's provenance is instead pinned by construction: nothing
but the `Globals` token handler writes it.

**EXPLICIT REASONED RESIDUAL (not an oversight):** re-running this KAT on the **primary**
GNOME session is deliberately NOT done. Two hard host crashes correlated with
primary-session live-compositor KAT activity (abrupt journal end, no guest OOM/panic —
hypervisor-style death; root cause never attributed). This run exercises the same mutter
codebase and the same `xdg_activation_v1` implementation, so the marginal information from
repeating it on the crash-correlated session does not justify the risk to a shared
development host. If someone does want it, the recipe below is the one to use, on a machine
that is not shared.

**Recovery procedure, if a host ever does wedge** (from the macOS host, over SSH or
Terminal — UTM's AppleScript interface survives an unresponsive guest):
```
osascript -e 'tell application "UTM" to stop virtual machine "Fedora Linux"'
osascript -e 'tell application "UTM" to start virtual machine "Fedora Linux"'
```

**Recipe (re-run the discharge, or run it on any stable GNOME Wayland session):**
```
# Preferred — headless mutter on its own socket, primary session untouched:
.port/tools/gates/ws17_set_urgent_mutter_gate.sh

# Or directly, against whatever compositor WAYLAND_DISPLAY names:
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

## V6. Wayland `default_mode` post-bind TRANSPORT — no compositor on this host re-advertises it [WS19 T9, 2026-07-29]

**What IS verified, and by what.** WS19 T9 made `winproto::wayland::Window` re-read
`Globals` at every `decoration_mode`/`client_side_decoration_enabled`/`sync_blur` call
instead of snapshotting at init (the reference's live `globals: *Globals` dereference,
`wayland.zig:222`/`:252`/`:303`; audit row O7). The RE-READ is proven end-to-end on a real
compositor by `.port/tools/gates/ws19_wayland_deco_live_gate.sh` +
`crates/rustty-apprt/tests/gtk_wayland_deco_live.rs`: nested KWin 6.7.3 on its own socket,
a real realized GTK window and a real `winproto` `Window`; the INITIAL mode comes off the
wire (KWin advertises `org_kde_kwin_server_decoration_manager` `default_mode = Server`, and
the port reports server-side decorations as a result — an assertion that FAILS on mutter,
which has no such manager, so it cannot pass vacuously); the pre-fix snapshot code FAILS
the post-change assertion; and a latch check (mode back to `Server` → `csd` back to
`false`) rules out a stuck value.

**What is NOT verified: the transport.** The mode CHANGE in that KAT is injected by writing
the exact field the `default_mode` dispatch handler writes
(`App::test_inject_default_deco_mode`, taking the raw protocol uint and running it through
the same `Mode::try_from(mode).ok()`), because **no compositor available on this host can
re-advertise `default_mode` after bind.** MEASURED, not assumed:

* **KWin 6.7.3** sends it exactly once per bound resource. The KAT still performs the real
  provocation — rewrite `kwinrc` to name a missing decoration plugin, then
  `org.kde.KWin.reconfigure` over D-Bus — and reports the outcome. Under
  `WAYLAND_DEBUG=1` the client trace shows exactly two
  `org_kde_kwin_server_decoration_manager…default_mode(2)` events (one on GDK's default
  queue, one on rustty's private queue), BOTH at bind time, and none after the reconfigure.
* **sway 1.11 / wlroots** calls `wlr_server_decoration_manager_set_default_mode` once at
  startup; there is no runtime lever.
* **mutter 50** advertises no KDE decoration manager at all, so the value is permanently
  `None` there.

So the residual is narrow and precisely: *a compositor-originated post-bind `default_mode`
event travelling over the wire, through `WlQueueSource`, into `Globals`.* The two halves on
either side of it ARE covered — `WlQueueSource`'s post-init dispatch is proven independently
by `gtk_wayland_set_urgent.rs` (a real `xdg_activation_token_v1.done` cranked through the
same GSource), and the `Globals`→`Window` re-read is proven by the KAT above. This item
exists so nobody later reads the KAT as wire-level proof.

**A finding worth keeping:** the WS19 plan (and the WS17 plan's V3 assumptions) treated a
live KWin decoration-mode change as available. It is not, for the reason measured above.

**Recipe (any of these discharges it):**
1. A compositor that calls `set_default_mode` after startup — e.g. a wlroots build patched
   to re-call `wlr_server_decoration_manager_set_default_mode` on a signal, or a future KWin
   that re-broadcasts on `reconfigure`. Then delete the injection from
   `gtk_wayland_deco_live.rs` and assert on the provocation directly:
   ```
   .port/tools/gates/ws19_wayland_deco_live_gate.sh     # already performs and reports it
   ```
   The gate prints `compositor provocation: … csd=<v>`; when `csd=true` appears there, the
   wire half is covered and this row can be deleted.
2. A wire-level injector (a `wl_display` proxy that relays the real compositor and emits an
   extra `default_mode` on the manager object id it sees pass through). Confirm with
   `WAYLAND_DEBUG=1` that a THIRD `default_mode(N)` reaches the client after bind.

---

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
>
> **WS17 OUTCOME [2026-07-29]: all three items REMAIN OPEN. The "items 2–3 come free with
> V3's box" premise did not hold, because V3 itself was discharged WITHOUT a KDE box.**
> WS17 got a real KWin only by running `kwin_wayland` NESTED from a rootless
> RPM-extracted prefix (`sudo` on this VM requires a password, so no KDE could be
> *installed*). That yields KWin on **Wayland**, not a KDE-Plasma-**on-X11** session, so:
> * **item 2** (a native X11 login session) is untouched — still needs
>   `/usr/share/xsessions` to offer a GNOME/Plasma X11 session and a human at the GDM
>   greeter, or a box that already has one;
> * **item 3** (KWin blur) is *worse* than "no KWin available": WS17 measured that KWin
>   6.7.3 advertises `ext_background_effect` with the blur capability **absent**
>   (`blur_capability=false`), so the `_KDE_NET_WM_BLUR_BEHIND_REGION` X11 write and its
>   Wayland sibling still cannot be visually confirmed even with KWin present. See V3.
> * **item 1** stays exactly as written below — proven unverifiable on this VM, physical
>   keyboard required. WS17 did not re-litigate it and neither should anyone else.

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


---

## V7. RTL (Hebrew) UI layout sign-off (blocked: no RTL session; human visual judgment) — **OWNER: Phase 3**

> **Why Phase 3 and not WS13.** WS13's IMPLEMENTATION work is complete and merged —
> nothing here is unported, and every mechanical property that CAN be asserted is
> asserted (see below). What remains needs a desktop session with `he_IL.UTF-8`
> generated and a human looking at a mirrored layout, which is exactly the class of
> environment-gated GUI verification the Phase-3 windowing/integration tier carries
> (alongside V2's bare-metal GPU sign-off and V5's native-X11 residuals). Routing it
> to a workstream that has no such environment would be a fiction.

**What IS shown.** The Hebrew catalog `po/he.po` ships, compiles
(`msgfmt --check` clean, enforced at build time by
`crates/rustty-apprt/build.rs`'s `check_translations`), and is installed by
`xtask resources` into `share/locale/he/LC_MESSAGES/io.airely.rustty.mo`. Every
runtime msgid is proven to EXIST in the catalogs in both directions by
`crates/rustty-apprt/tests/i18n_msgid_coverage.rs`, and the lookup mechanism
itself is proven locale-generic and non-vacuous by
`crates/rustty-apprt-c/tests/i18n_translate.rs`, which spawns a child with
`LANG`/`LC_ALL` set and asserts a German hit, a JAPANESE hit that differs from
it (a non-Latin catalog — the same code path `he` takes), an `LANG=C`
passthrough, and a stale-`Ghostty`-msgid passthrough. Nothing about the
translation path is Latin-script-specific.

**What is NOT shown.** That GTK's right-to-left MIRRORING of the ported widget
tree reads correctly: header bar, tab strip and split-pane order flipping, while
the terminal grid itself stays LTR-ordered (the VT layer is direction-agnostic).

**Why it is blocked here.** `he_IL.UTF-8` is not in the VM's generated locale
set as a SESSION locale; the VM's only graphical session is GNOME/Wayland driven
over SSH; and judging whether a mirrored layout reads correctly is a human
visual task, not an assertable one. This is scope-of-VERIFICATION only — no code
path is unported and no gate is weakened.

**Recipe (a machine with `he_IL.UTF-8` generated and a graphical session):**
```
cargo run -p xtask -- resources
LANG=he_IL.UTF-8 LC_ALL=he_IL.UTF-8 \
  RUSTTY_RESOURCES_DIR=$PWD/zig-out/share/rustty target/debug/rustty
```
1. The window chrome renders in Hebrew (the `po/he.po` `msgstr`s) — e.g. the
   About menu item, and with `--confirm-close-surface=true` the close dialog
   heading.
2. The header bar, tab strip and split panes mirror right-to-left.
3. Terminal CONTENT stays LTR-ordered.
4. **Negative control (this is what makes it non-vacuous):** repeat with
   `LANG=C LC_ALL=C` and confirm the same chrome renders in English and
   left-to-right. Hebrew in both runs would mean the text is baked in; English
   in both would mean the lookup never fired.
