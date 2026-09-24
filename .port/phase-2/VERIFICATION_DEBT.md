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

## V2. Bare-metal GPU rendering/perf sign-off (blocked: VM is llvmpipe-only) — **ENVIRONMENT-BLOCKED**

**Blocked on:** a Linux box with a real GPU. The project's only Linux machine is a
VM whose GL stack is llvmpipe (software) over virtio-gpu, with no hardware path.

```
PORTED: crates/rustty-renderer/src/opengl.rs
PORTED: .port/tools/gates/hw_gl_probe.sh
```

**Re-owned 2026-07-30 (was `OWNER: WS17`).** WS17 is finished. Correctness on this
renderer IS validated here (see the Context note below — llvmpipe is a legitimate
correctness environment and the glyph output was checked against upstream Ghostty
1.3.1); what is blocked is specifically the perf/behaviour-under-real-GL axis.
`hw_gl_probe.sh` makes it a one-command answer on any real box.


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
audit rows O13a/O14/O15/O16 — so it is scheduled work, not an out-of-scope decision]**
**[WS14 sharpening, 2026-07-30: what remains out of scope is the browser *apprt
runtime* — a full `apprt` App/Surface over the browser. `apprt/browser.zig` ITSELF is
4 loc and is ported at `crates/rustty-apprt/src/browser.rs`. This exclusion does NOT
cover WS14, which is DONE: it ports the `renderer/WebGL.zig` placeholder, the
`main_wasm.zig` module as a real freestanding `rustty_wasm.wasm` cdylib, `App.Wasm`
and `Atlas.Wasm` (all 4 reference asserts live), and gates browser rendering with a
falsifiable pixel assertion whose two negative controls must FAIL on every run
(`.port/tools/gates/ws14_webgl_browser_gate.sh`, `ci.sh --webgl`). WS14 adds NO
verification debt — the artifact builds, the gate runs headlessly on this machine,
and it is CI-wireable.]**;
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

## V7. RTL (Hebrew) UI layout sign-off (blocked: no RTL session; human visual judgment) — **ENVIRONMENT-BLOCKED**

**Blocked on:** a desktop session with `he_IL.UTF-8` generated, plus a human who
reads Hebrew looking at the mirrored layout. This is the one row here blocked on a
PERSON rather than a machine; it is filed under the same marker because the
requirement is identical — the code is ported, the recipe is written, and no
amount of work in this repo can discharge it.

```
PORTED: po/he.po
PORTED: crates/rustty-apprt/tests/i18n_msgid_coverage.rs
PORTED: crates/rustty-apprt-c/tests/i18n_translate.rs
```

**Re-owned 2026-07-30 (was `OWNER: WS20`).** WS20 is finished. Note what is NOT
debt here: every mechanical property is already asserted, including a non-Latin
(Japanese) catalog hit through the same code path `he` takes — so the lookup
machinery is proven script-generic. What remains is visual judgment alone.


> **Why WS20 and not WS13 or Phase 3.** WS13 is CLOSED, so filing here orphaned the
> row (that is how 21 items were lost before G-P2 existed). And `Phase 3` is NOT a
> valid owner for this: the closure gate now accepts Phase 3 only for work whose
> SOURCE is the beyond-pin upstream range (marked `BEYOND-PIN`), which this is not —
> it is Phase-2 verification debt that merely needs an RTL session. It is owned by
> the open WS20.
>
> **Original rationale (WS13 implementation is complete).** WS13's IMPLEMENTATION work is complete and merged —
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

---

## V8. libxev `wasi_poll` + `iocp` backends: cross-compile-verified only, never EXECUTED — **ENVIRONMENT-BLOCKED**

**Blocked on:** a Windows host (for `iocp`) and a WASI runtime host (for
`wasi_poll`). Neither exists in this project; the porting host is macOS aarch64.

```
PORTED: crates/rustty-xev/src/iocp.rs
PORTED: crates/rustty-xev/src/iocp_watcher.rs
PORTED: crates/rustty-xev/src/wasi.rs
PORTED: crates/rustty-xev/src/wasi_poll.rs
PORTED: crates/rustty-xev/src/wasi_watcher.rs
```

**Re-owned 2026-07-30 (was `OWNER: Phase 3`).** Nothing about this row is
beyond-pin: the code is ported and the tests are written. Parking it on a phase
made Phase-2 closure depend on that phase staying open, which is bookkeeping
rather than verification. The honest disposition is that the work is done and
the machine to run it on does not exist here.

**Filed WS20, 2026-07-30.** Both backends are PORTED (see the RETRACTED
"OUT OF SCOPE BY PLATFORM" entry in `DEFERRALS.md` — that classification was
wrong and is struck). This row is the honest remainder: on the porting host
(macOS aarch64) nothing in either backend can be RUN.

**What WAS run here, and passed:**
```
cargo check -p rustty-xev --target wasm32-wasip1          --all-targets   # GREEN
cargo test  -p rustty-xev --target wasm32-wasip1 --no-run                 # GREEN (both binaries link)
cargo check -p rustty-xev --target x86_64-pc-windows-msvc --all-targets   # GREEN
cargo check -p rustty-xev --target wasm32-unknown-unknown                 # GREEN
```
(`rustup target add wasm32-wasip1 x86_64-pc-windows-msvc` first.)

**What is NOT shown.** That the loops actually tick: `poll_oneoff` timing/wakeup
on a WASI host, and `GetQueuedCompletionStatusEx`/`OVERLAPPED` completion
recovery + `CancelIoEx` cancellation on Windows. The reference's own tests are
ported (`xev_wasi_poll` zig=8/rust=9, `xev_iocp` zig=14/rust=14, both gated by
`check_test_counts.py`) — they compile, they have simply never executed.

**[WS20 UPDATE] the WATCHER layer is now in the same boat, and is the larger
half.** `iocp_watcher.rs` and `wasi_watcher.rs` (the
`watcher/{async,timer,process,stream}.zig` monomorphizations — see the struck
DEFERRALS row) carry the reference's `AsyncTests` (3), `TimerTests` (5),
`ProcessTests` (2, iocp only) and `GenericStreamTests` (4) as documented
skip-stub `#[test]`s: **14 on iocp, 12 on wasi_poll**. They compile for their
targets and hold the count; none has run. The `GenericStreamTests` pty pair/trio
are skipped by the REFERENCE ITSELF outside `.linux`/`.macos`
(`else => return error.SkipZigTest`), so only `Stream decls` (a comptime
assertion, discharged by compiling) is genuinely inert there. Filling the
remaining stubs is the same Windows/WASI-host job as the loop stubs below —
do them in one pass on the same runner.
Additional commands run here and GREEN:
```
cargo clippy -p rustty-xev --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo clippy -p rustty-xev --target wasm32-wasip1          --all-targets -- -D warnings
```


**Recipe — wasi_poll (any host with a wasm runtime):**
```
rustup target add wasm32-wasip1
cargo install wasmtime-cli          # or use an existing wasmtime/wasmer
cargo test -p rustty-xev --target wasm32-wasip1 \
  --config 'target.wasm32-wasip1.runner=["wasmtime","--dir=."]'
```
Expect the 8 `wasi_*` loop/timer tests plus `batch_get_put` to pass. The `--dir=.`
preopen is also what unblocks the ONE documented skip-stub,
`wasi_file_skip_stub` (the reference's `test "wasi: file"`): with a preopened cwd
its fixture can be written against `std::fs`, at which point the stub should be
replaced by the real read/write/close round-trip. **Negative control:** drop
`--dir=.` and confirm the file test's fixture is what fails, not the loop.

**Recipe — iocp (a Windows host, or a Windows CI runner):**
```
cargo test -p rustty-xev                 # on Windows: runs the 9 live iocp tests
```
Expect `iocp_loop_time`, `iocp_stop`, `iocp_timer`, the four `timer_reset*`
variants, `iocp_timer_cancellation`, `iocp_canceling_a_completed_operation` and
`iocp_noop` to pass. The five skip-stubs
(`iocp_file_io*`, `iocp_socket_*`, `iocp_recv_cancellation`,
`iocp_accept_cancellation`) are skipped for their FIXTURES, not their subjects:
the file pair needs a `sliceToPrefixedFileW`-equivalent NT path normalizer, the
socket trio needs Winsock startup plus the `std.net` server/client fixture. On a
Windows host those fixtures are cheap; write them and delete the stubs.
**Negative control (this is what makes the run non-vacuous):** temporarily make
`Loop::tick` return `Ok(())` immediately and confirm `iocp_timer` FAILS. A green
suite with a no-op tick would mean the tests never drove the loop.

---

## V9. `rustty-termio` Windows paths: cannot even be CROSS-CHECKED on this host — **ENVIRONMENT-BLOCKED**

**Blocked on:** a Windows host with the MSVC toolchain. Cross-checking from
macOS is additionally blocked by `highway-sys`'s `cc`-driven C++ build (see
below), so unlike V8 this row has no compile-level evidence either.

```
PORTED: crates/rustty-termio/src/exec.rs
PORTED: crates/rustty-termio/src/command.rs
```

**Re-owned 2026-07-30 (was `OWNER: Phase 3`).** Same reasoning as V8: the code
is ported, only the machine is missing.

**Filed WS20, 2026-07-30.** `ReadThread::threadMainWindows` and
`Subprocess::start`'s Windows spawn half are PORTED (they previously carried
`unimplemented!("... OUT OF SCOPE BY PLATFORM ...")`; that classification was
wrong and is struck in `DEFERRALS.md`). This row is stronger debt than V8: the
code is not merely unrun, it is **uncompiled**.

**Why.** `rustty-termio` transitively depends on `highway-sys`, whose `cc`-driven
C++ build script cannot cross-compile to MSVC from macOS:
```
$ cargo check -p rustty-termio --target x86_64-pc-windows-msvc
  ...
  cargo:warning=.../clang/21/include/mm_malloc.h:13:10: fatal error: 'stdlib.h' file not found
  error occurred in cc-rs: command did not execute successfully (status code exit status: 1):
    LC_ALL="C" "c++" ... "--target=x86_64-pc-windows-msvc" ... crates-sys/highway-sys/cpp/per_target.cc
```
This is a toolchain gap (no MSVC headers/libs on this machine), not a code
problem: `cargo check -p rustty-xev --target x86_64-pc-windows-msvc` — a crate
with no C++ dependency — is GREEN, which proves the Windows target itself works.

**Recipe (a Windows host, or a `windows-latest` CI runner with MSVC):**
```
cargo check   -p rustty-termio --all-targets
cargo clippy  -p rustty-termio --all-targets -- -D warnings
cargo test    -p rustty-termio
```
Expect compile errors on the first run to be *mechanical* (field/API spellings in
`rustty-os::windows`, `rustty-os::pty::WindowsPty`, `rustty-termio::command`'s
Windows arm) — fix them in place; do NOT reintroduce a scope exclusion. Then
drive the end-to-end path: launch a surface, confirm `cmd.exe` spawns through the
pseudo console, that output reaches the terminal via `threadMainWindows`, and
that closing the surface makes `Exec.threadExit`'s `CancelIoEx` break the read
loop instead of hanging. **Negative control:** stub `threadMainWindows` to return
immediately and confirm the surface shows NO output — a passing run with a dead
read thread would prove nothing.

---

## V10. `rustty-vaxis` Windows console backend: PORTED, but uncompilable for a Windows target on this host — **ENVIRONMENT-BLOCKED**

**Blocked on:** a Windows host with the MSVC toolchain (`zlib-sys`'s vendored C
cannot cross-compile to MSVC from macOS — detail below).

```
PORTED: crates/rustty-vaxis/src/tty.rs
PORTED: crates/rustty-vaxis/src/loop_.rs
```

**Re-owned 2026-07-30 (was `OWNER: WS20`).** WS20 is finished; leaving the row
attached to it would have orphaned it the moment that box was ticked.

**Filed WS20, 2026-07-30.** The whole `#[cfg(windows)]` family in `rustty-vaxis` is
now LIVE, not a seam: `WindowsTty` (`vaxis/src/tty.zig:189-691` — console-mode +
codepage setup, `deinit` restore, the buffered `WriteFile` writer, the
`ReadConsoleInputW` pump and the full `INPUT_RECORD` key/mouse/resize/focus decode
including UTF-16 surrogate pairing), the `.windows` arm of `Loop.ttyRun`, and the
`.windows` arm of `handleEventGeneric` (`Loop.zig:167-234`, which is **not** a subset
of the POSIX arm). Win32 is bound by hand-rolled `extern "system"` against `kernel32`,
per the `rustty-os::windows` / `rustty-xev::windows` convention — no new crate.

**Why it cannot be cross-checked here.** `rustty-vaxis → rustty-zigimg → zlib-sys`,
and `zlib-sys`'s `cc`-driven build cannot cross-compile to MSVC from macOS:
```
$ cargo check -p rustty-vaxis --target x86_64-pc-windows-msvc --all-targets
error: failed to run custom build command for `zlib-sys v0.1.0 (crates-sys/zlib-sys)`
  cargo:warning=crates-sys/zlib-sys/vendor/zconf.h:446:14: fatal error: 'sys/types.h' file not found
  error occurred in cc-rs: command did not execute successfully (status code exit status: 1):
  LC_ALL="C" "cc" "-O0" ... "--target=x86_64-pc-windows-msvc" ... "-c" ".../vendor/adler32.c"
```
Same class as V9's `highway-sys`. `CC_FORCE_DISABLE=1` does not rescue it — cc-rs
hard-errors instead of no-op'ing (`the `cc` crate's functionality has been disabled by
the CC_FORCE_DISABLE environment variable`). No cross toolchain exists on this machine
either (`x86_64-w64-mingw32-gcc`, `xwin`, `lld-link` all absent), so
`x86_64-pc-windows-gnu` is equally blocked. That this is a **C-toolchain** limit and not
a Rust-code limit is proven by a C-free crate cross-checking green:
```
$ cargo check -p rustty-xev --target x86_64-pc-windows-msvc --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.58s
```

**What WAS verified here.** `cargo check`/`cargo clippy` never link, so the Windows Rust
code is fully type-checked and linted on this host by forcing the `cfg` arms — see
`.port/phase-2/wincheck.sh`, which copies `crates/<crate>/src` to a scratch package and
rewrites `#[cfg(windows)]` → `#[cfg(unix)]` (true here) and `#[cfg(not(windows))]` →
`#[cfg(target_os = "wasi")]` (false here):
```
bash .port/phase-2/wincheck.sh "$PWD" rustty-vaxis
cargo clippy --manifest-path /tmp/wincheck-rustty-vaxis/Cargo.toml --all-targets -- -D warnings
```
GREEN with zero diagnostics at WS20. It does NOT prove the Win32 struct layouts
(`INPUT_RECORD` & co. are `#[repr(C)]` declarations checked by no linker and no run) and
resolves no `kernel32` symbol.

**Recipe (a Windows host, or a `windows-latest` CI runner with MSVC):**
```
cargo check  -p rustty-vaxis --target x86_64-pc-windows-msvc --all-targets
cargo clippy -p rustty-vaxis --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test   -p rustty-vaxis --target x86_64-pc-windows-msvc
```
Then drive the real console: run a vaxis consumer in `conhost`/Windows Terminal and
confirm (a) typing produces `key_press`/`key_release` with correct `mods` and text,
including an astral-plane character (which exercises the two-record UTF-16 surrogate
path), (b) mouse press/drag/release/wheel produce the right `Mouse.Button`/`Type` — the
`last_mouse_button_press` XOR is the part most likely to be wrong, (c) resizing the
window delivers a `winsize` whose size comes from `GetConsoleScreenBufferInfo`, not from
the (inaccurate) record, and (d) alt-tab produces `focus_out`/`focus_in`.
**Negative control:** force `event_from_record` to return `Ok(None)` for `EventType ==
0x0002` and confirm mouse input goes dead while the keyboard still works — a green run
with a dead mouse arm would prove nothing.

---

## V11. `rustty-font-shaper` Windows font discovery: PORTED, needs a stock Windows install to exercise — **ENVIRONMENT-BLOCKED**

**Blocked on:** a stock Windows install (the discoverer scans `%SYSTEMROOT%\Fonts`
and `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, so a bare toolchain is not enough —
this row needs a real font-populated system).

```
PORTED: crates/rustty-font-shaper/src/discovery.rs
```

**Re-owned 2026-07-30 (was `OWNER: WS20`).** Same reasoning as V10.

**Filed WS20, 2026-07-30.** `discovery.zig`'s `Windows` discoverer (the system +
per-user font-directory scan, `%SYSTEMROOT%\Fonts` and
`%LOCALAPPDATA%\Microsoft\Windows\Fonts`, with the FreeType `family_name` match and its
SFNT-name-table fallback, `.ttc` multi-face probing, and the codepoint/CMap filter) and
`DeferredFace`'s `win` payload (`path`/`face_index`/`variations`/`peek`/`presentation`,
plus the `deinit`/`familyName`/`name`/`load`/`hasCodepoint` Windows arms) are LIVE.
`Discover`/`Face`/`DeferredFace` now resolve to the `freetype_windows` backend on
`target_os = "windows"` instead of falling through to the Fontconfig arm, matching
`Backend.default`'s `.windows => .freetype_windows`.

**Cross-check blocker:** identical to V10. `cargo check -p rustty-font-shaper --target
x86_64-pc-windows-msvc --all-targets` dies in the vendored C/C++ build scripts before
rustc sees any Rust — the first two to fail are `zlib-sys`
(`vendor/zconf.h:446: fatal error: 'sys/types.h' file not found`) and `simdutf-sys`
(`vendor/simdutf.h:5: fatal error: 'cstring' file not found`), both reached transitively
via `rustty-vt`; `freetype-sys`/`harfbuzz-sys` (this crate's own C deps) are behind
them in the queue and fail the same way.

**Verified here:** `.port/phase-2/wincheck.sh` Recipe B, GREEN under
`cargo clippy --all-targets -- -D warnings`; host `cargo test -p rustty-font-shaper`
unchanged.

**Runtime debt.** The reference's own `test "windows"` (`discovery.zig:1333-1352`) is
ported as a documented skip-stub (`discovery::tests::windows`), matching this crate's
existing `fontconfig`/`web_canvas` skip-stub precedent — it needs a stock Windows
install, because it asserts that `Arial` is discoverable and has a glyph for `'A'`.

**Recipe (Windows host with MSVC):**
```
cargo check  -p rustty-font-shaper --target x86_64-pc-windows-msvc --all-targets
cargo clippy -p rustty-font-shaper --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test   -p rustty-font-shaper --target x86_64-pc-windows-msvc
```
Un-skip `discovery::tests::windows` by removing its early `return` once the backend is
actually `freetype_windows`, then check the discovered `DeferredFace` loads and shapes.
**Negative control:** ask for a family that is definitely absent (e.g. `"Nonexistent
Family QQQ"`) and confirm the iterator yields `None` rather than the first font on disk —
a passing `Arial` lookup alone does not prove the family filter works.
## V12. `rustty-termio` + `rustty-os` `flatpak` feature: cross-compile-verified only, never EXECUTED — **ENVIRONMENT-BLOCKED**

**Blocked on:** a Linux host running inside a flatpak sandbox (or a container
exposing the `org.freedesktop.Flatpak.Development` portal on the session bus).
The project's Linux VM is not sandboxed, so it does not discharge this either.

```
PORTED: crates/rustty-os/src/flatpak.rs
PORTED: crates/rustty-termio/src/exec.rs
```

**Re-owned 2026-07-30 (was `OWNER: WS20`).** Same reasoning as V10.

**Filed WS20, 2026-07-30.** The whole flatpak host-command tier is PORTED and now
WIRED (see the struck "Flatpak comptime-false branches" row in `DEFERRALS.md` —
treating `build_config.flatpak` as a `const false` dead branch was wrong;
`-Dflatpak` is a real ghostty build option). This row is the honest remainder.

**What WAS run here, and passed:**
```
cargo check  -p rustty-os     --features flatpak
cargo clippy -p rustty-os     --features flatpak --all-targets -- -D warnings
cargo check  -p rustty-termio --features flatpak --all-targets
cargo clippy -p rustty-termio --features flatpak --all-targets -- -D warnings
```

**What is NOT shown.** Nothing in the path can execute on macOS: it links
GIO/GLib and talks to `org.freedesktop.Flatpak.Development` over the session bus.
Unproven: that `HostCommandSpawn` returns a pid, that the `HostCommandExited`
signal subscription fires `onExit`, that `waitXev`'s 0ms timer delivers the exit
code into `flatpakExit` -> `processExitCommon` -> the surface mailbox, and that
`killCommandFlatpak`'s `HostCommandSignal(SIGHUP)` reaches the child.

**Recipe (a Linux host inside a flatpak sandbox, or a container with the
`Development` portal):**
```
# on Linux, with libgio-2.0/libglib-2.0 dev packages present:
cargo check -p rustty-termio --features flatpak --all-targets
cargo build -p rustty-termio --features flatpak         # proves the GIO link
# then, INSIDE a flatpak sandbox (/.flatpak-info present):
cargo test  -p rustty-termio --features flatpak
```
Then drive the end-to-end path: launch a surface inside the sandbox, confirm the
log line `started subcommand on host via flatpak API path=... pid=...`, confirm
the shell is reachable, then exit it and confirm the surface receives
`child_exited` with the right code (that exercises `onExit`'s
`if (state.completion)` xev wake, which was MISSING before WS20 and is the single
most likely place for a regression). **Negative control:** make
`FlatpakHostCommand::signal` return `Ok(())` without the DBus call and confirm
closing the surface HANGS — a passing run with a no-op signal would prove
nothing.

## ~~V13. GTK `pre_exec`/`post_fork` are ported but NOT YET REACHED by `Command`~~ — **DISCHARGED 2026-07-30 [WS20]**

> **END-TO-END CHECK RUN AND PASSED, 2026-07-30 (Linux VM, GTK 4.22.4, Wayland).**
> Recipe step 4 is no longer debt. Both arms, real `/proc` evidence, config written
> to the REAL config path:
>
> ```
> ARM linux-cgroup=always -> scope-transition log lines: 2
>   debug(gtk_post_fork): beginning transition to transient systemd scope
>     app-rustty-surface-transient-362480.scope
>   child 362480 cgroup:
>     0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-rustty-surface-transient-362480.scope
>
> ARM linux-cgroup=never  -> scope-transition log lines: 0
>   (no transition line)
>   child 362565 cgroup: 0::/user.slice/user-1000.slice/session-545.scope
> ```
>
> **THE FIRST RUN OF THIS KAT WAS VACUOUS, and that is worth recording.** It set
> `RUSTTY_CONFIG_PATH=...` — an environment variable this project does not have.
> Config is discovered through the XDG path (`rustty_config::file_load::default_xdg_path`
> -> `~/.config/rustty/config.rustty`), so BOTH arms ran the DEFAULT config, whose
> Linux default is `LinuxCgroup::SingleInstance` — and both arms transitioned. The
> `never` arm "failing" is what exposed it; had the default been `never`, both arms
> would have agreed and the KAT would have "passed" while testing nothing. Rerun
> against the real config path (with backup/restore), the arms differ, which is the
> non-vacuity proof.
>
> Also verified on Linux, since macOS compiles NONE of the gtk half:
> `cargo check -p rustty-apprt --features gtk --all-targets` GREEN,
> `cargo test -p rustty-termio --no-fail-fast` 55 passed / 0 failed,
> `cargo clippy -p rustty-apprt --features gtk --all-targets -- -D warnings` clean.


**Filed WS20, 2026-07-30. Wiring discharged 2026-07-30 (branch
`ws20-v13-preexec-wiring`).** `src/apprt/gtk/pre_exec.zig` and
`src/apprt/gtk/post_fork.zig` were already ported in full
(`crates/rustty-apprt/src/gtk/{pre_exec,post_fork}.rs`, unit-tested and
mutation-proved). What was missing was the seam that makes
`rustty_termio::command::Command` CALL them.

**DISCHARGED (recipe steps 1-3).**
1. `rustty_termio::command::{RtPreExecInfo, RtPostForkInfo}` are no longer the
   empty growth points: they carry the reference's real GTK fields
   (`pre_exec.zig:11-23` / `post_fork.zig:15-31`) and their `init(&Config)`
   copies them (no new crate edge — termio already deps rustty-config).
   `rustty_apprt::gtk::{pre_exec::PreExecInfo, post_fork::PostForkInfo}` are now
   RE-EXPORTS of those types, which makes the reference's comptime identity
   `RtPreExecInfo == apprt.runtime.pre_exec.PreExecInfo` literally true — just
   established from the LOWER crate, because R4 forbids
   `rustty-termio -> rustty-apprt`.
2. The hook FUNCTIONS are installed from above: `gtk::App::init` calls
   `rustty_termio::command::install_rt_hooks({rt_pre_exec, rt_post_fork})` (thin
   `PreExecFn`/`PostForkFn` adapters in the two gtk modules, doing the
   `cmd.rt_pre_exec_info` / `cmd.rt_post_fork_info` + `cmd.pid` reads the
   reference does inline), and `termio::exec` reads `command::rt_hooks()` at
   exactly the site the reference resolves `@hasDecl` (`Exec.zig:1034-1037`).
   **This is the one divergence from the oracle's comptime branch**, documented
   in full at the seam (PORT NOTE above `command::RtPreExecInfo`): the
   reference's choice is a build-time constant, ours is a process-global set
   once during apprt init, and a second install of a DIFFERENT pair panics.
   `rustty-apprt` gained a direct (downward, previously transitive)
   `rustty-termio` edge; `check_acyclic.sh` is GREEN.
3. `rustty_apprt::gtk::post_fork::PostForkError` is collapsed — now a re-export
   of `rustty_termio::command::PostForkError` (the mirror existed only to dodge
   the crate edge step 2 legitimately adds).

Gates run on macOS at discharge: `cargo check --workspace --all-targets`,
`cargo test --workspace --no-fail-fast` (4250 passed / 0 failed), `cargo fmt
--all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`check_acyclic.sh`, `cargo check -p rustty-apprt --features gtk --all-targets`.
New mutation-proved KATs:
`exec::tests::rt_hooks_reach_the_spawned_command` (a real fork/exec KAT proving
the installed `postFork` IS called by `Command::start`; reverting
`rt_post_fork: rt_hooks.post_fork` to `None` fails it 0 != 1),
`command::tests::{rt_pre_exec_info_init_copies_the_config_fields,
rt_post_fork_info_init_copies_the_config_fields, rt_info_defaults_are_inert}`,
and `crates/rustty-termio/tests/rt_hooks_install.rs` (own process; its fail arm
dies if the double-install assert is weakened).

**STILL OPEN — HONEST HARDWARE DEBT (recipe step 4).** The GTK modules are
`#[cfg(all(target_os = "linux", feature = "gtk"))]`, so on the macOS dev host
NOTHING in `crates/rustty-apprt/src/gtk/{pre_exec,post_fork}.rs` — including the
new adapters — nor the `gtk::App::init` install call is COMPILED, let alone run.
Two things remain unverified, and neither was attempted:
- the gtk-side COMPILE (`cargo check -p rustty-apprt --features gtk` on Linux);
- the end-to-end cgroup behavior: with `linux-cgroup = always` the SHELL process
  (not the app) must land in `app-rustty-surface-transient-<pid>.scope`.

Runnable, on the Linux VM (`ssh gael@192.168.64.4`):
```sh
cd ~/projects/rustty
cargo test  -p rustty-termio --lib
cargo test  -p rustty-termio --test rt_hooks_install
cargo check -p rustty-apprt  --features gtk --all-targets
cargo test  -p rustty-apprt  --features gtk --lib
cargo build -p rustty --features gtk

# end-to-end: cgroup ON
printf 'linux-cgroup = always\n' > /tmp/rustty-cgroup-on.conf
RUSTTY_CONFIG_PATH=/tmp/rustty-cgroup-on.conf ./target/debug/rustty &
sleep 3
cat /proc/$(pgrep -n bash)/cgroup   # expect: .../app-rustty-surface-transient-<pid>.scope
kill %1

# NEGATIVE CONTROL: cgroup OFF — the shell must NOT be in a transient scope
printf 'linux-cgroup = never\n' > /tmp/rustty-cgroup-off.conf
RUSTTY_CONFIG_PATH=/tmp/rustty-cgroup-off.conf ./target/debug/rustty &
sleep 3
cat /proc/$(pgrep -n bash)/cgroup   # expect: NO app-rustty-surface-transient-* segment
kill %1
```

**Already discharged by WS20 (do not redo):** the D-Bus payload itself is
verified end to end against real systemd —
`crates/rustty-apprt/tests/gtk_cgroup_scope.rs` spawns a child, calls
`cgroup::create_scope`, and asserts via `/proc/<pid>/cgroup` that the child
landed in the scope `cgroup::fmt_scope` names (mutation-proved: renaming the
`PIDs` property → RED).

---

## ~~V14. The `gtk-v4_16` byte-dump is proved at the BINDING, not at the HANDLER~~ — **DISCHARGED 2026-07-30 [WS20]**

**Filed WS20, 2026-07-30.** `class/application.rs`'s
`signal_css_parsing_error` now carries the `application.zig:1261-1274` byte-dump
arm behind the opt-in `gtk-v4_16` feature. `tests/gtk_css_parse_error.rs` proves
the ceiling is lifted and that a real `CssProvider::parsing-error` hands back a
`CssSection` whose bytes are byte-for-byte the loaded source. It does NOT drive
`signal_css_parsing_error` itself, which is `pub(super)` and unreachable from an
integration test.

**Recipe.** Add a `#[cfg(test)]` unit test inside `class/application.rs`'s `imp`
module: build an `Application`, capture stderr, obtain a `CssSection` from a real
`parsing-error` emission, call `signal_css_parsing_error`, and assert the logged
line CONTAINS the offending source bytes under `--features gtk,gtk-v4_16` and
does NOT under `--features gtk`. **Negative control:** the same assertion run
without the feature must fail if it claims the bytes are present. Note the
G-GTKINIT constraint: the lib-test binary already hosts headed tests, so the new
test must reuse the existing init rather than calling `gtk4::init()` again.

Runnable, on the Linux VM (`ssh gael@192.168.64.4`), once the unit test exists:
```sh
cd ~/projects/rustty
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0

# the byte-dump must be PRESENT with the opt-in feature
cargo test -p rustty-apprt --features gtk,gtk-v4_16 --lib \
  css_parsing_error -- --ignored --nocapture

# NEGATIVE CONTROL: the same test must FAIL (or be cfg'd out) without it —
# at the v4_14 ceiling `CssSection::bytes` does not exist at all
cargo test -p rustty-apprt --features gtk --lib \
  css_parsing_error -- --ignored --nocapture

# the ceiling itself is A/B-provable at compile time: flip the arm's
# `#[cfg(feature = "gtk-v4_16")]` to `#[cfg(all())]` and the plain-gtk build
# must fail with `no method named 'bytes' found for reference '&CssSection'`
cargo check -p rustty-apprt --features gtk
```

**Already discharged by WS20 (do not redo):** the ceiling A/B above was run —
under `--features gtk` the unconditional arm fails to compile exactly as quoted,
and under `--features gtk,gtk-v4_16` it compiles and the headed KAT passes on
GTK 4.22.4.

**DISCHARGED 2026-07-30 [WS20, branch `ws20-v14-css-handler`].** The recipe's unit
test now exists: `crates/rustty-apprt/src/gtk/class/application.rs`,
`mod tests::signal_css_parsing_error_byte_dump_follows_the_feature_ceiling`. It
builds an `Application` (`build_test_app`), wires a real `CssProvider`'s
`parsing-error` straight into `imp::Application::signal_css_parsing_error` (the
production shape at `:1201-1207`), redirects fd 2 into a temp file across
`load_from_string`, and reads the handler's own bytes back. Being a LIB test it
reaches the `pub(super)` handler the integration KAT cannot.

The assertion is a DIFFERENTIAL — `assert_eq!(has_bytes, cfg!(feature =
"gtk-v4_16"))` — so it is not satisfiable by both arms for the same reason. Two
anti-vacuity guards: the handler-call count must be > 0, and an EMPTY fd-2
capture is a hard failure (libtest's output capture would otherwise swallow the
`eprintln!`, hence the mandatory `--nocapture`).

Run on the Linux VM, GTK 4.22.4, Wayland:

```
$ cargo test -p rustty-apprt --features gtk,gtk-v4_16 --lib css_parsing_error -- --ignored --nocapture
V14 EVIDENCE (gtk-v4_16 = true): warn(gtk): css parsing failed at <data>:1:17-18: gtk-css-parser-error-quark 1 Expected a valid color.
window { color: ###notacolor; }
test ...::signal_css_parsing_error_byte_dump_follows_the_feature_ceiling ... ok
test result: ok. 1 passed; 0 failed

$ cargo test -p rustty-apprt --features gtk --lib css_parsing_error -- --ignored --nocapture
V14 EVIDENCE (gtk-v4_16 = false): warn(gtk): css parsing failed at <data>:1:17-18: gtk-css-parser-error-quark 1 Expected a valid color.
test ...::signal_css_parsing_error_byte_dump_follows_the_feature_ceiling ... ok
test result: ok. 1 passed; 0 failed
```

The source line `window { color: ###notacolor; }` is PRESENT in the first output
and ABSENT in the second — the handler's `:1261-1274` arm, not the binding.

**NEGATIVE CONTROL (run, not assumed).** With the expected value temporarily
hard-coded to `true` (i.e. claiming the bytes are dumped at the 4.14 ceiling) and
built `--features gtk`, the same test FAILS:

```
assertion `left == right` failed: the `application.zig:1261-1274` byte dump must be
present IFF `gtk-v4_16` is on (feature on = false); logged: "warn(gtk): css parsing
failed at <data>:1:17-18: gtk-css-parser-error-quark 1 Expected a valid color.\n"
  left: false
 right: true
test result: FAILED. 0 passed; 1 failed
```

The patch was reverted (`git checkout`) immediately after; the committed test
carries `cfg!(feature = "gtk-v4_16")`, not `true`.

**G-GTKINIT: GREEN** (`9 gtk4::init() call site(s) across 29 test file(s); none
doubled`). The new test adds NO `gtk4::init()` to a `tests/` binary — it lives in
the lib-test binary and reuses that binary's single init with the established
soft-skip (`if gtk4::init().is_err() { println!("V14: SKIPPED …"); return; }`),
which is why the recipe's `--lib css_parsing_error` filter matters: it makes this
the only headed body in the process. `cargo fmt --check` clean; `cargo clippy
-p rustty-apprt --features gtk,gtk-v4_16 --lib --tests` clean for this code (one
PRE-EXISTING `unused_imports` warning on `tests/gtk_css_parse_error.rs:24`,
untouched here).

---

> **ID NOTE [WS10 T22, 2026-07-30].** The WS10 plan names its two new rows "V6"
> and "V7". Both ids were **already taken** in this file (V6 = Wayland
> `default_mode`, V7 = RTL layout). The plan's numbering is errata; the rows
> land here as **V15** and **V16**, plus **V17** for the drift bisection.

## V15. The `origin/main` oracle cannot be compared against, and the PINNED oracle can no longer be rebuilt — **OWNER: Phase 3 — `BEYOND-PIN`**

**Re-verified after pin advance:** 2026-07-31 — **PREMISE REFUTED.** This row's
blocker was "the pinned oracle needs Zig 0.15.2 and this host has 0.16.0, so it
cannot be rebuilt, and `origin/main` cannot be built at all". The pin advanced to
`6ad1fe7d8`, whose own `build.zig.zon` declares `minimum_zig_version = "0.16.0"`
— the version installed here. Tested rather than assumed:

* `zig build -Doptimize=ReleaseFast` at `6ad1fe7d8` reaches **394/397 steps**. The
  only failure is the macOS app-bundle `CpResource` step in `xcodebuild`, which
  copies shell-completion and terminfo resources into `Ghostty.app` and has
  nothing to do with any benchmark or library artifact.
* `zig build -Doptimize=ReleaseFast -Demit-bench=true -Dapp-runtime=none`
  **succeeds** and produces `ghostty-bench`
  (`version=1.3.2-HEAD-+6ad1fe7d8`, `optimize=ReleaseFast`).
* Preserved at `~/rustty-oracle-artifacts/ghostty-bench-6ad1fe7d8`, sha256
  `52606275dde534b032d8f021c3f82e281969e011306edb6ebfe8223a17edd3d0`.
* `src/benchmark/` at this pin carries **ApcParser.zig** and **HyperlinkMap.zig**
  — the two actions V18 said no obtainable oracle binary had — and its action set
  is 1:1 with `crates/rustty-bench/src/`.

So a comparable oracle exists again and these three rows are no longer blocked.
The measurements themselves are the remaining work, not the impossibility.

**Update (task S-4, 2026-09-23): the fence now uses an oracle at the current
pin.** `ws10_suite.py` Fence 1 reads
`~/rustty-oracle-artifacts/ghostty-bench-<first 9 hex of ORACLE_PIN>`, built
ReleaseFast at the pin from a `git archive` (oracle checkout untouched), sha256
`46b5adfb6cd4bfcde6a693a3a6d0cc8c94c6cd825ff66e98c6c6b79d1d95fa2a`, checked
before every oracle run (BLOCKED on mismatch). Provenance and the rebuild recipe:
`.port/tools/perf/README.md`. A provisional run (load 14.5) held 14/14; the
authoritative idle run is owed (task S-4 report). G8's BASELINE.json is still
the `4749c4e93` series and is not re-based here.

The 4749c4e93 binary and its sha256 stay recorded above: historical rustty
figures are anchored to it, and numbers from the new binary are a NEW baseline,
not a continuation of the old series. Comparing across the two would be invalid.


**Two halves, both about the reference oracle.**

**(a) The pinned oracle binary is now irreplaceable.** Every rustty perf figure
ever recorded is anchored on
`.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`, sha256
`0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb`. That binary
is oracle `4749c4e93`, which requires **Zig 0.15.2**. The installed Zig is now
**0.16.0**, and the pin does not build under it: `src/build/zig.zig` carries a
hard `@compileError` requiring 0.15.2, and there is real 0.15→0.16 std breakage
on top of it (`build.zig:27`, `readFileAlloc` arity). Verified 2026-07-30.

- **The binary is untracked** (`.gitignore` excludes reference-oracle build
  outputs as "regenerable" — it is no longer regenerable on this host).
- A verified byte-identical backup exists at
  `~/rustty-oracle-artifacts/ghostty-bench-4749c4e93`.
- **Do not delete, overwrite, or rebuild it.** If its sha256 ever differs from
  the above, **STOP**: every historical number becomes incomparable. That is a
  report-and-stop condition, not something to work around.

**(b) `origin/main` is not a substitute, and comparing against it naively reads
as a phantom rustty regression.** `origin/main` (`6ad1fe7d8`) *does* build (it
wants 0.16.0). But upstream added `src/quirks_memset.zig` in the pin→head range,
which makes the oracle **~2.8× faster on `+terminal-stream`**
(`.port/phase-3/assessments/E-os-termio-platform.md` row E34). A rustty-vs-head
run will therefore show a **much smaller rustty margin on terminal-stream, and
possibly a loss** — **that is the oracle getting faster, NOT rustty regressing.**

**`quirks_memset` must NOT be ported.** `rustc` lowers `@llvm.memset` to the
platform libc `memset` (Apple `libSystem`; glibc IFUNC on Linux), both already
SIMD + `dc zva`. Exporting a strong `memset` from a Rust dylib would pessimize
rustty and risk ODR/interposition breakage.

**Runnable recipe** (the oracle worktree must be created with `worktree add` —
**never** `git checkout` in `../ghostty`, four lanes depend on its working tree
staying at the pin):

```sh
git -C /Users/gael/projects/ghostty worktree add /tmp/ghostty-origin-main origin/main
cd /tmp/ghostty-origin-main && zig build -Doptimize=ReleaseFast   # needs Zig 0.16.0

# Compare, and read the result with the caveat above:
cd /Users/gael/projects/rustty
python3 .port/tools/perf/ws10_suite.py \
    --rust target/release-fast/rustty-bench \
    --oracle /tmp/ghostty-origin-main/zig-out/bin/ghostty-bench -n 9

git -C /Users/gael/projects/ghostty worktree remove /tmp/ghostty-origin-main
```

**Pre-stated expected outcome** (so nobody misreads it): `terminal-stream`
margin collapses by roughly 2.8× on the oracle side; the other 13 configs move
little. A terminal-stream LOSS against head is **expected and is not a rustty
regression**. The authoritative fence remains the **pinned** oracle.

### WS10 execution status (2026-07-30): half (b) WAS RUN — and the result is worse than "the oracle got faster"

`origin/main` (`6ad1fe7d8`) **does** build under Zig 0.16.0. Note the plan's
recipe is incomplete: plain `zig build` installs `Ghostty.app` and **no bench
binary**; the flag is **`-Demit-bench`** (`build.zig:105`, `Config.zig:391`).
Built and run (PROVISIONAL — load 6.24, `IDLE_MAX` 2.0):

| bench | PINNED zig ms | origin/main zig ms | oracle change | rust ms |
|---|--:|--:|--:|--:|
| terminal-stream | 543.97 | **51.59** | **10.5× FASTER** | 344.51 |
| codepoint-width.wcwidth | 261.09 | **802.40** | **3.1× SLOWER** | 238.17 |
| screen-clone.clone | 54.65 | **295.33** | **5.4× SLOWER** | 22.56 |
| screen-clone.render | 16.64 | **420.01** | **25.2× SLOWER** | 14.71 |
| screen-clone.noop | 6.87 | 9.91 | 1.4× slower | 5.38 |
| codepoint-width.noop | 4.55 | 6.72 | 1.5× slower | 3.31 |
| terminal-parser | 266.58 | 267.22 | flat | 106.40 |
| the other 7 | — | — | within ~2% | — |

**Verdict: `origin/main` is not a valid comparison target, and the reason is
bigger than `quirks_memset`.** The expected phantom regression did appear —
`LOST bench terminal-stream/ascii (STRICT): zig=51.59ms rust=344.51ms (+567.8%)`,
the single RED in an otherwise 13/14 run — and at **10.5×**, not the ~2.8×
E34 predicted. But four other configs moved the **opposite** way, up to 25×
slower. A pure codegen win cannot do that.

**The benchmarks themselves changed beyond the pin.** BL-4 contains the bench
corpus commits: `258de36d1`/`4c5b1d5d5` swap `TerminalStream`'s handler and chunk
size, and `446f80f4e` adds `--loops` plus three render modes to `ScreenClone`.
So `+terminal-stream` and `+screen-clone` on `origin/main` **measure different
work** than the same-named benches on the pin. Comparing rustty against them is
not like-for-like in either direction, and neither column can be read as a
speed-up or a slow-down.

**Consequence for the port.** A rustty-vs-`origin/main` fence only becomes
meaningful **after** rustty ports the BL-4 bench-corpus commits (plan T10-T12,
T17) so both sides measure the same work again. Until then this row stays open
and the **pinned** oracle remains the only valid fence. `quirks_memset` still
must not be ported, for the reasons above.

The oracle worktree was removed after the run (`git -C ../ghostty worktree
remove`); the pinned working tree was never touched — no `checkout` was run in
`../ghostty` at any point.

Owner is Phase 3 because `origin/main` **is** the upstream resync range:
`BEYOND-PIN`.

## V16. The binary-size axis of `BASELINE.json` is unmeasured, so G8 runs without `--strict` — **OWNER: Phase 3 — `BEYOND-PIN`**

**Re-verified after pin advance:** 2026-07-31 — **PREMISE REFUTED.** This row's
blocker was "the pinned oracle needs Zig 0.15.2 and this host has 0.16.0, so it
cannot be rebuilt, and `origin/main` cannot be built at all". The pin advanced to
`6ad1fe7d8`, whose own `build.zig.zon` declares `minimum_zig_version = "0.16.0"`
— the version installed here. Tested rather than assumed:

* `zig build -Doptimize=ReleaseFast` at `6ad1fe7d8` reaches **394/397 steps**. The
  only failure is the macOS app-bundle `CpResource` step in `xcodebuild`, which
  copies shell-completion and terminfo resources into `Ghostty.app` and has
  nothing to do with any benchmark or library artifact.
* `zig build -Doptimize=ReleaseFast -Demit-bench=true -Dapp-runtime=none`
  **succeeds** and produces `ghostty-bench`
  (`version=1.3.2-HEAD-+6ad1fe7d8`, `optimize=ReleaseFast`).
* Preserved at `~/rustty-oracle-artifacts/ghostty-bench-6ad1fe7d8`, sha256
  `52606275dde534b032d8f021c3f82e281969e011306edb6ebfe8223a17edd3d0`.
* `src/benchmark/` at this pin carries **ApcParser.zig** and **HyperlinkMap.zig**
  — the two actions V18 said no obtainable oracle binary had — and its action set
  is 1:1 with `crates/rustty-bench/src/`.

So a comparable oracle exists again and these three rows are no longer blocked.
The measurements themselves are the remaining work, not the impossibility.

The 4749c4e93 binary and its sha256 stay recorded above: historical rustty
figures are anchored to it, and numbers from the new binary are a NEW baseline,
not a continuation of the old series. Comparing across the two would be invalid.


**Re-owned 2026-07-30 (was `OWNER: WS20`).** WS20 is finished, and `Phase 3` here is
not a parking space — it is forced. The 9 labels are ORACLE artifacts (Zig static
libs and test executables), and per **V15** the pinned oracle can no longer be built
at all (Zig 0.16.0 hard-errors on it) while `origin/main` can. So the numbers this
row needs cannot be produced until the pin advances, which is `P3-W10`. That is the
`BEYOND-PIN` condition, not a deferral.


`BASELINE.json` carries 9 `binary_sizes_bytes` labels (`libghostty-vt.a`,
`ghostty-bench`, `ghostty-test`, …) at **0% tolerance** per DoP.
`ws10_suite.py` measures the 14 benchmarks and **nothing else**, so
`check_baseline_regression.py --metrics … --strict` reports:

```
compared 14 metrics; regressions 0; missing-from-candidate 9
    NOT-MEASURED size: libghostty-vt.a (static) …
RED: candidate is missing baseline metrics (strict)
```

G8 is therefore armed **without** `--strict` (WS10 T3). That is the honest
option — the alternative was to emit fabricated or mismapped size numbers to
turn the gate green, which R4/R5 forbids.

**Why it is not trivial to discharge:** the 9 labels are *oracle* artifacts
(Zig static libs / test executables). Comparing a Rust `.a` against a Zig `.a`
at 0% tolerance is not a meaningful fence; discharging this needs a **rustty
artifact→label mapping with its own justified tolerance**, which is a design
decision, not a measurement.

**Runnable recipe once that mapping exists:** have `ws10_suite.py --emit-metrics`
additionally emit `binary_sizes_bytes` for the mapped rustty artifacts, then
restore `--strict` on the G8 line in `.port/tools/ci.sh` and confirm GREEN.

## V17. The oracle-anchored perf measurement campaign — **OWNER: Phase 3 — `BEYOND-PIN`**

**Scope decision (user, 2026-07-30): perf *measurement/verification* may be
deferred until after Phase 3; perf *implementation* may not.** This row is the
single ledger entry for the deferred measurement. The gate/fence *machinery*,
its three FAIL arms, the G8 vacuity fix and all BL-4 code changes are **not**
covered here — they landed in WS10.

**Enabling condition — this is why the owner is Phase 3 and why the row carries
`BEYOND-PIN`.** Every oracle-anchored figure depends on one irreplaceable
prebuilt binary, because the pinned oracle `4749c4e93` requires **Zig 0.15.2**
and this host now has **0.16.0** (see V15). **Phase 3's final task advances the
pin to `origin/main`, which itself requires 0.16.0** — so once that lands the
oracle builds from source again and every oracle-anchored comparison becomes
cleanly reproducible instead of depending on a binary that cannot be regenerated.
The pin advance is therefore the literal precondition for doing this work well,
which is exactly the case `check_phase2_closure.py`'s `row_is_beyond_pin()`
admits `Phase 3` as an owner for.

### What is already MEASURED and must NOT be redone

Done on 2026-07-30 and recorded in `PERF_LOG.md` (PROVISIONAL — load 4.76-5.80
against `IDLE_MAX` 2.0, so not authoritative, but the first figures since the
four core-pinning `yes` processes were killed):

- **Fence 1 (ORACLE), 14 configs, N=9, paired vs the pinned binary — GREEN,
  14/14 holds**, thinnest margin `codepoint-width.wcwidth` −7.0%.
- **The suspected drift REPRODUCES**: `terminal-parser` **+17.8%**,
  `screen-clone.clone` **+20.0%** vs the r5-SHIP record, while the oracle column
  reproduces its historical values to ~2% **in the same session** — the control
  that proves the gap is rustty-side and not contention.
- **The `origin/main` comparison (V15) was RUN**, and found something larger than
  the expected `quirks_memset` effect: the *benchmarks themselves* diverged
  beyond the pin. See V15.
- Both gate self-tests and the live simd flip proof — see `PERF_LOG.md`.

### What remains UNMEASURED

1. **An authoritative IDLE 14-config baseline.** Everything above is provisional.
2. **The bisection of the confirmed drift** — the substantive open item.
3. Re-running V15's `origin/main` comparison once the benches are comparable.

### The drift, for whoever bisects it

| config | r5-SHIP | 2026-07-30 | drift | rust spread |
|---|--:|--:|--:|--:|
| `terminal-parser` | 92.5 ms | 109.01 ms | **+17.8%** | 1.097 |
| `screen-clone.clone` | 20.9 ms | 25.07 ms | **+20.0%** | 1.163 |

This is a **pre-existing rustty regression**, not a WS10 one — WS10 modified no
hot path. Prime suspects in order: **WS7 Phase A-E** (BL-1 compression — it
touched `Page`/`PageList`/`Screen` hard, which are exactly these two configs' hot
paths), then WS5, WS8, WS19.

**Not attempted at load ~5:** a 12 s rebuild competes with the measurement and
the per-step delta a bisection must resolve is the same order as these configs'
spreads (1.097 / 1.163). A contended bisection returns a confident wrong answer,
which is worse than none.

**`.perf-bins/rustty-bench-ws10-entry-RF` is a DEGRADED anchor.** It carries this
drift, so Fence 2 will happily hold the 20% regression in place. Re-stash the
entry binary after the drift is fixed.

### Runnable recipe (requires 1-min load average ≤ 2.0)

```sh
cd /Users/gael/projects/rustty
for ref in a3a2b23 74226dd <WS7 Phase A..E merges> <WS8 merge> <WS19 merge> main; do
  git checkout "$ref" -- crates/ && cargo build -p rustty-bench --profile release-fast
  cp target/release-fast/rustty-bench ".perf-bins/bisect-$ref"
done
# pair each against the entry binary on the two drifted configs only:
python3 .port/tools/perf/ws10_suite.py --rust .perf-bins/bisect-<ref> \
    --self .perf-bins/rustty-bench-ws10-entry-RF --no-oracle -n 9

# and, once the pin has advanced, the authoritative oracle baseline:
python3 .port/tools/perf/ws10_suite.py --rust target/release-fast/rustty-bench -n 9 \
    --emit-metrics /tmp/ws10-metrics.json
```

### ⚠ `quirks_memset` caveat — read before interpreting ANY post-pin oracle run

Upstream added `src/quirks_memset.zig` in the pin→head range, which makes the
oracle **~2.8× faster on `+terminal-stream`** (measured here at **10.5×**;
`.port/phase-3/assessments/E-os-termio-platform.md` row E34). **A shrunken rustty
margin — or an outright loss — on `terminal-stream` against a post-pin oracle is
the ORACLE GETTING FASTER, not rustty regressing.** Do not "fix" a phantom.

**`quirks_memset` must NOT be ported.** `rustc` lowers `@llvm.memset` to the
platform libc `memset` (Apple `libSystem`; glibc IFUNC on Linux), both already
SIMD + `dc zva`. Exporting a strong `memset` from a Rust dylib would pessimize
rustty and risk ODR/interposition breakage.

And per V15: after the pin advances, `+terminal-stream` and `+screen-clone` also
*measure different work* than they did at the pin, so the comparison is only
like-for-like once rustty has ported the BL-4 bench-corpus commits.

---

## ~~V17-old. The rustty-side perf drift since `a3a2b23` is CONFIRMED but NOT BISECTED — OWNER: WS10~~ — **SUPERSEDED by V17 above [2026-07-30]**

Re-owned from WS10 to **Phase 3 (`BEYOND-PIN`)** under the scope decision above.
Content merged into V17. Retained as a heading only so the id's history is
traceable.

---

## V18. `f2a7652ab`'s two BL-4 perf configs have no paired oracle number — **OWNER: Phase 3 — `BEYOND-PIN`** [WS10 T11/T16/T17, 2026-07-30]

**Re-verified after pin advance:** 2026-07-31 — **PREMISE REFUTED.** This row's
blocker was "the pinned oracle needs Zig 0.15.2 and this host has 0.16.0, so it
cannot be rebuilt, and `origin/main` cannot be built at all". The pin advanced to
`6ad1fe7d8`, whose own `build.zig.zon` declares `minimum_zig_version = "0.16.0"`
— the version installed here. Tested rather than assumed:

* `zig build -Doptimize=ReleaseFast` at `6ad1fe7d8` reaches **394/397 steps**. The
  only failure is the macOS app-bundle `CpResource` step in `xcodebuild`, which
  copies shell-completion and terminfo resources into `Ghostty.app` and has
  nothing to do with any benchmark or library artifact.
* `zig build -Doptimize=ReleaseFast -Demit-bench=true -Dapp-runtime=none`
  **succeeds** and produces `ghostty-bench`
  (`version=1.3.2-HEAD-+6ad1fe7d8`, `optimize=ReleaseFast`).
* Preserved at `~/rustty-oracle-artifacts/ghostty-bench-6ad1fe7d8`, sha256
  `52606275dde534b032d8f021c3f82e281969e011306edb6ebfe8223a17edd3d0`.
* `src/benchmark/` at this pin carries **ApcParser.zig** and **HyperlinkMap.zig**
  — the two actions V18 said no obtainable oracle binary had — and its action set
  is 1:1 with `crates/rustty-bench/src/`.

So a comparable oracle exists again and these three rows are no longer blocked.
The measurements themselves are the remaining work, not the impossibility.

The 4749c4e93 binary and its sha256 stay recorded above: historical rustty
figures are anchored to it, and numbers from the new binary are a NEW baseline,
not a continuation of the old series. Comparing across the two would be invalid.


WS10 T11 and T17 each add a rustty-bench config that the pinned oracle binary
does not have, because both were introduced upstream AFTER the pin:

| config | upstream commit | rustty commit |
|---|---|---|
| `hyperlink-map` (`--mode=lookup\|churn`) | `7e14347c1` | `b8d24a6` |
| `apc-parser` | `f6f79acce` | `f96957c` |

The only oracle bench binary on this host is
`.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`, built from the pin
`4749c4e93` and byte-stable since 2026-06-21 (see V15). It has neither action.
The pinned oracle needs Zig 0.15.2; this host has 0.16.0, so it cannot be
rebuilt and `origin/main` cannot be built at all.

**Both configs are therefore rustty-only, SELF-fenced, and neither counts toward
the 14/14.** They are still worth running as self-fences (a rustty-vs-rustty A/B
across a change) — that is what they are wired for today.

### Recipe (once the pin advances and a modern Zig is present)

```
# oracle at or after the commit that adds the action
git -C ../ghostty worktree add /tmp/oracle-bl4 7e14347c1   # or f6f79acce
zig build -Doptimize=ReleaseFast --prefix /tmp/oracle-bl4-out  # in that worktree

# hyperlink-map: no corpus needed
/tmp/oracle-bl4-out/bin/ghostty-bench +hyperlink-map --mode=churn --entries=4096
cargo run -p rustty-bench --profile release-fast -- +hyperlink-map --mode=churn --entries=4096

# apc-parser: generate the corpus with EACH side's own generator
/tmp/oracle-bl4-out/bin/ghostty-gen +kitty > /tmp/kitty-zig.bin
cargo run -p rustty-synthetic --profile release-fast -- +kitty > /tmp/kitty-rs.bin
# ... then +apc-parser --data=<corpus> on both, under
# .port/tools/perf/ws10_suite.py's paired A/B methodology (same session,
# back-to-back, N>=7, idle gate armed).
```

**Expected outcome, pre-stated so it is not misread:** these two configs measure
paths upstream rewrote for exactly these numbers (`7e14347c1` reports 100x on map
churn; `f6f79acce` reports ~25x on `apc-parser`, `8c523ed03` a further 1.69x).
rustty has now ported all of that, so the two sides should be COMPARABLE. A large
oracle win on either would mean a porting gap, not measurement noise; a large
rustty win would most likely mean the corpus differs (the two generators are
independently seeded — always compare each side against its own generator's
output AND against the other's, and say which).

**Not a deferral.** The implementation is complete and behaviourally tested (see
the two commits above). Only the oracle-anchored NUMBER is outstanding, and only
because of the toolchain gap V15 describes.

---

## V19. macOS 27 AppKit workarounds + the synchronous-`deinit` free assertion — **ENVIRONMENT-BLOCKED** [2026-07-30, re-owned 2026-07-31]

**Blocked on:** a macOS 27 host (beta or release) — **for `2104e0749` and
`632aa6756` only**. This box is macOS 26.5 (Darwin 25.5.0, Xcode 26.5).
`2104e0749`'s body sits inside `if #available(macOS 27, *)` and targets a
macOS-27-only `NSScrollPocket`, so it cannot execute here at all; `632aa6756`'s
added branch is the `?? firstSuperview(withClassName: "NSTitlebarAccessoryContainerView")`
fallback, which is dead code on a macOS 26 view hierarchy because the first lookup
(`NSTitlebarAccessoryClipView`) always succeeds. Neither can be exercised on any
amount of effort here. `fa3802a70` was **not** in that category: its six-case mapping
is now **verified on this host** and that portion of the debt is discharged — see V19a
below. (Written in lower case on purpose: `check_phase2_closure.py` treats the
all-caps disposition token anywhere in a row body as closing the WHOLE row, and this
row must keep its `ENVIRONMENT-BLOCKED` disposition for the two items above.)

The artifacts still carrying unverified debt in this row (`macos/Sources/Helpers/Backport.swift`
is no longer among them — its mapping is asserted by a test, see V19a):

```
PORTED: macos/Sources/Features/Terminal/Window Styles/HiddenTitlebarTerminalWindow.swift
PORTED: macos/Sources/Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift
PORTED: macos/Sources/RusttyKit/Rustty.Surface.swift
```

The first two are the macOS-27 pair named in **Blocked on:** above; the third is
V19b, which is effort-blocked rather than environment-blocked (see its scope note).

**Re-owned 2026-07-31 (was `OWNER: P3-W8 T2`).** `P3-W8` merged, which made this
row an orphan under G-P2 — correctly, because a closed workstream cannot do the
work. The right disposition is not a new owner: the residual is an ENVIRONMENT
limit, not unfinished porting. All four artifacts above are ported and compile,
and `xcodebuild test` is green on them. Reclassified with the recipe kept intact
so the day a macOS 27 host exists the work is a checklist, not a rediscovery.

**Scope note.** V19b (the synchronous-`deinit` free assertion) is folded into
this row rather than left open separately. It is *not* macOS-27-gated — see its
own recipe below, which is runnable on this host — but it is genuinely blocked on
nothing except effort, and the analysis of WHY the obvious test double would be
vacuous is the part worth preserving. If it is ever picked up it should be split
back out; it is recorded here so the reasoning is not lost.

Four items from `P3-W8 T2` were **ported and compiling, but NOT verified**, and were
listed here rather than being claimed green. One of the four (`fa3802a70`) has since
been verified on this host — see V19a. Three remain.

### V19a. macOS-27-only view-hierarchy / pointer-style workarounds — "ported, unverified on macOS 27"

| Oracle | rustty site | What it does |
|---|---|---|
| `2104e0749` | `macos/Sources/Features/Terminal/Window Styles/HiddenTitlebarTerminalWindow.swift` `reapplyHiddenStyle()` | `if #available(macOS 27, *)` hide the `NSScrollPocket` descendant of the theme frame |
| `632aa6756` | `macos/Sources/Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift` | accept `NSTitlebarAccessoryContainerView` as well as `…ClipView` when finding the clip view |
| ~~`fa3802a70`~~ | `macos/Sources/Helpers/Backport.swift` `BackportPointerStyle.official` | **mapping verified 2026-07-31** — `frameResize(position:directions:)` → `columnResize(directions:)` / `rowResize(directions:)` |

This host is **macOS 26.5 (Darwin 25.5.0, Xcode 26.5)**. `2104e0749` and `632aa6756`
are guarded by / only reachable on macOS 27 view hierarchies. `xcodebuild test`
compiles both and is green, which proves **compilation and
availability-annotation correctness only** — it proves nothing about their behaviour.

**CORRECTION, 2026-07-31 — `fa3802a70` is NOT macOS-27-gated.** The original text
said it was "a pointer-shape change that only the macOS 27 HIG cursor set provides".
That is wrong at the code level, and was tested rather than assumed on this host:
`BackportPointerStyle.official` is annotated `@available(macOS 15, *)`, and both the
old and the new spellings — `PointerStyle.frameResize(position:directions:)` and
`.columnResize(directions:)` / `.rowResize(directions:)` — are **macOS 15** SwiftUI
APIs. A probe run under `swift` on macOS 26.5.1 (25F80) executed all three and showed
them distinguishable:

```
columnResize: PointerStyle(value: columnResize(directions: Set(rawValue: 1))
frameResize : PointerStyle(value: frameResize(position: leading, directions: Set(rawValue: 3)))
rowResize   : PointerStyle(value: rowResize(directions: Set(rawValue: 1))
```

#### `fa3802a70` six-case mapping — **VERIFIED ON THIS HOST 2026-07-31; that portion of the debt is discharged** (branch `v19c-pointer-style`)

**Test:** `macos/Tests/Helpers/BackportPointerStyleTests.swift` — four `@Test` cases
over the real `BackportPointerStyle.official`, compared through `String(describing:)`
because `PointerStyle` has no `Equatable` and no introspection:

* `testResizeCasesMapToColumnAndRowResize` — all six resize cases against the
  `PointerStyle` each must produce (`.columnResize(directions: .leading/.trailing)`,
  `.rowResize(directions: .up/.down)`, bare `.rowResize`, bare `.columnResize`).
  The direction sets are covered, not just the case name: the descriptions carry
  `Set(rawValue: 1/2/3)`.
* `testNoResizeCaseStillUsesFrameResize` — no resize case's description contains
  `frameResize`, i.e. the pre-`fa3802a70` spelling is gone.
* `testResizeCaseDescriptionsArePairwiseDistinct` — proves `String(describing:)`
  actually discriminates the six, so the two assertions above cannot pass vacuously.
* `testNonResizeCasesAreUnchanged` — the six cases `fa3802a70` did not touch.

**Run:**

```sh
cargo run -q -p xtask -- xcframework && rm -rf macos/RusttyKit.xcframework \
  && cp -R dist/RusttyKit.xcframework macos/RusttyKit.xcframework
cargo run -q -p xtask -- resources     # zig-out/share, needed by the app's CpResource phase
cd macos && xcodebuild test -project Rustty.xcodeproj -scheme Rustty \
  -destination 'platform=macOS' -only-testing:RusttyTests
```

(The xcframework must be REBUILT, not reused: a pre-WS9 archive does not define
`_dgettext`/`_bindtextdomain`/`_textdomain` and the app fails to link. Do NOT pass
`OTHER_LDFLAGS=` on the command line — it overrides the per-target value and breaks
the `DockTilePlugin` link; the project already carries `-Wl,-u,_rustty_benchmark_cli`
on the targets that need it.)

**Result — `** TEST SUCCEEDED **`**, whole `RusttyTests` suite, macOS 26.5.1 (25F80):

```
Test case 'BackportPointerStyleTests/testResizeCasesMapToColumnAndRowResize()' passed
Test case 'BackportPointerStyleTests/testNoResizeCaseStillUsesFrameResize()' passed
Test case 'BackportPointerStyleTests/testResizeCaseDescriptionsArePairwiseDistinct()' passed
Test case 'BackportPointerStyleTests/testNonResizeCasesAreUnchanged()' passed
RusttyTests: 228 tests, 227 passed, 1 skipped, 0 failed
```

**Negative controls — BOTH RUN, both observed FAILING, then reverted in place**
(edited in place, never `mv`d back from a `.bak`, so no stale mtime could make the
rebuild skip):

1. Swap `.resizeLeft` and `.resizeUp` (`columnResize` ↔ `rowResize`) in
   `Backport.swift`:

   ```
   Test case 'BackportPointerStyleTests/testResizeCasesMapToColumnAndRowResize()' failed
   Expectation failed: (actual → "PointerStyle(value: rowResize(directions: Set(rawValue: 1))")
     == (String(describing: expected) → "PointerStyle(value: columnResize(directions: Set(rawValue: 1))"):
     resizeLeft: expected PointerStyle(value: columnResize(directions: Set(rawValue: 1)),
     got PointerStyle(value: rowResize(directions: Set(rawValue: 1))
   ```

2. Revert `.resizeUpDown` to the pre-`fa3802a70` `.frameResize(position: .top)`:

   ```
   Test case 'BackportPointerStyleTests/testNoResizeCaseStillUsesFrameResize()' failed
   Test case 'BackportPointerStyleTests/testResizeCasesMapToColumnAndRowResize()' failed
   Expectation failed: !((actual → "PointerStyle(value: frameResize(position: top, directions: Set(rawValue: 3)))")
     .contains("frameResize") → true): resizeUpDown: still uses frameResize —
     PointerStyle(value: frameResize(position: top, directions: Set(rawValue: 3)))
   ```

**Explicitly NOT discharged:** the *rendered cursor artwork*. What the six cases map
to is now asserted; whether macOS 27 draws the HIG double-headed bar for
`columnResize`/`rowResize` is a human visual judgment in the same family as row V7,
and remains step 4 of the recipe below.

**Recipe (needs a macOS 27 beta host or VM):**

```sh
bash macos/build-rustty.sh
open macos/build/Release/Rustty.app   # then work through the 4 steps below
```

1. `bash macos/build-rustty.sh`, launch `Rustty.app`.
2. `2104e0749`: set `macos-titlebar-style = hidden`, open a window, screenshot. The
   terminal content must reach the top of the window with no translucent "pocket"
   strip overlaying it (ghostty issue #13390).
3. `632aa6756`: set `macos-titlebar-style = tabs`, open ≥2 tabs, screenshot. The tab
   bar must be vertically centred in the titlebar and not stretched — on macOS 27
   beta 2 without this change `firstSuperview(withClassName:)` returns nil and the
   whole layout pass early-returns.
4. `fa3802a70` **artwork only** (the mapping is already discharged above): hover a
   split divider; the pointer must be the HIG column/row-resize cursor (double-headed
   bar), not the frame-resize cursor.

### V19b. `a6edca2d7` synchronous surface free on the main thread — no counting double

**Ported:** `macos/Sources/RusttyKit/Rustty.Surface.swift` `deinit` now has the
`guard !Thread.isMainThread else { rustty_surface_free(surface); return }` fast path.

**Not verified, and deliberately NOT faked.** The plan asked for "a test double
counting `rustty_surface_free` calls, asserting the free happens before `deinit`
returns". There is no non-divergent way to do that from `RusttyTests`:

* `rustty_surface_free` is a C symbol in `rustty.debug.dylib`; Swift cannot interpose
  it without `DYLD_INSERT_LIBRARIES` and a purpose-built shim dylib.
* `crates/rustty-apprt-c/src/lib.rs:1178` dereferences `(*ptr).app` immediately, so a
  synthetic/`nil` `rustty_surface_t` cannot be passed — the double must be a *real*
  surface.
* Injecting a `freeHook` closure into `deinit` would be an R1 divergence in the
  production teardown path, invented purely for the test.

**A mirror class in the test file that reproduces the `deinit` shape would be a
VACUOUS control** — it would pass whether or not `Rustty.Surface.deinit` is correct —
so none was written. Recorded here instead.

**Recipe (runnable, ~half a day):** `crates/rustty-apprt-c/tests/macos_window_host.rs`
already stands up a real `rustty_app_t` + `rustty_surface_t` headlessly from Rust.
Port that bring-up into a Swift test helper in `macos/Tests/`, wrap the resulting
`rustty_surface_t` in `Rustty.Surface`, drop the last reference **on the main thread**
inside an `autoreleasepool`, and assert — immediately after the pool exits, with no
run-loop turn — that the app's surface registry no longer contains the pointer
(`rustty_app_surface_count` or equivalent). Under the pre-`a6edca2d7` detached-`Task`
path that assertion fails, because the free is still queued.


---

## V20. `P3-W8 T5` apprt/GTK: the monitor clamp's geometry effect — **DISCHARGED 2026-07-31**

**Both halves are now RUN and PASSED on the Fedora VM.** V20a (the suspended-state
half) passed 2026-07-31; V20b (the monitor clamp's geometry effect) — the residual
this row was previously filed as environment-limited on — passed the same day
against a real compositor with **two heads**, once the "needs two monitors" premise
was tested instead of assumed. The environment-limit marker and its `Blocked on:`
line are gone because the block was not real: the VM has one PHYSICAL head, but a headless
compositor supplies as many VIRTUAL heads as the test needs, on its own socket,
without touching the primary session.

```
PORTED: crates/rustty-apprt/src/gtk/class/window.rs
PORTED: crates/rustty-apprt/src/gtk/class/surface.rs
```

**What the old disposition got wrong.** It read "a second monitor" as "hardware".
`.port/tools/gates/ws17_set_urgent_mutter_gate.sh` had already established the
pattern that dissolves that — a SECOND, HEADLESS compositor instance with a virtual
monitor, needing no DRM device and no seat. Extending it from one virtual head to
two is a configuration change, not a hardware one.

**Re-owned 2026-07-31 (was `OWNER: P3-W8` follow-up).** `P3-W8` merged, so G-P2
correctly reported this row as an orphan. `P3-W6` executed the runnable part (V20a)
and deferred the remainder; that remainder is now executed too.

### V20b. `004c88e41` clamp the max window size to the current monitor — **DISCHARGED 2026-07-31: the geometry effect was RUN on two real heads**

**Ported:** `crates/rustty-apprt/src/gtk/class/window.rs`
`imp::compute_size_decision` (the pure predicate) + `imp::toplevel_compute_size`
(the GDK wrapper) + a `G_CONNECT_AFTER` `compute-size` connection on the
realized `gdk::Toplevel`.

**Binding question closed (assessment F §6 open question 4):** `gdk4` **0.11.4**
does have `ToplevelExtManual::connect_compute_size`, `ToplevelSize::bounds()` and
`ToplevelSize::set_size()` — verified by reading
`~/.cargo/registry/src/index.crates.io-*/gdk4-0.11.4/src/toplevel.rs` and
`src/toplevel_size.rs` on the VM. The cost therefore stays **M**, no manual
`gdk_toplevel_size_*` FFI needed for the accessors. One divergence: gtk4-rs's
`connect_compute_size` hard-codes `after = false`, and the reference needs
`.{ .after = true }`, so the trampoline is connected directly with
`g_signal_connect_data(..., G_CONNECT_AFTER)`.

**Unit-level verification (unchanged, 2026-07-30).** `compute_size_decision_guards`
covers the zero-size, zero-bounds, within-bounds and exceeds-bounds arms. Two
independent negative controls were run on the VM and observed to fail: deleting
`if w <= bounds_w && h <= bounds_h { return false; }` →
`assertion failed: !imp::compute_size_decision(800, 600, 800, 600)`; deleting
`if bounds_w <= 0 || bounds_h <= 0 { return false; }` →
`assertion failed: !imp::compute_size_decision(1000, 800, 0, 600)`.

#### The geometry effect — RUN 2026-07-31

**Gate:** `.port/tools/gates/v20b_compute_size_clamp_gate.sh`.
**KAT:** `crates/rustty-apprt/tests/gtk_wayland_compute_size.rs`
(`compute_size_reasserts_size_on_a_smaller_monitor`).
**Evidence:** `.port/phase-2/evidence/v20b/v20b-compute-size-two-heads.txt`.

**Environment.** Fedora VM (`ssh gael@192.168.64.4`, one PHYSICAL head), GNOME
Shell 50.0 / mutter on Wayland as the primary session — **not touched**. The gate
runs a SECOND compositor: a headless **sway 1.11** (`WLR_BACKENDS=headless`,
`WLR_HEADLESS_OUTPUTS=2`) on its own `WAYLAND_DISPLAY` and its own IPC socket, the
same isolation discipline as the WS17 mutter gate and for the same reason (row V4's
two hard host crashes correlated with primary-session live-compositor activity).
Two heads, deliberately different sizes: `HEADLESS-2` 800x600 at the origin and
`HEADLESS-1` 1920x1200 beside it.

```sh
ssh gael@192.168.64.4
export XDG_RUNTIME_DIR=/run/user/1000     # NOT WAYLAND_DISPLAY — the gate makes its own
cd ~/projects/rustty
bash .port/tools/gates/v20b_compute_size_clamp_gate.sh
```

**Why sway and not mutter.** mutter's `--virtual-monitor` also gives extra heads, but
nothing on this host moves a window BETWEEN mutter's heads without injecting input
into a live session. sway exposes `swaymsg move workspace to output` /
`swaymsg output … disable`, i.e. an external, scriptable, compositor-driven move.
Both are real compositors; the seam under test is GDK's, not sway's.

**Two provisioning traps the gate now asserts around, because both silently
produced a green-looking run first:**

1. `output … mode/resolution` in sway's config file is dropped — the headless outputs
   do not exist yet when the config is parsed — so both heads stayed at wlroots'
   1280x720 default. The modes are set over IPC after startup instead.
2. With the default GLES renderer the headless backend still allocates through GBM,
   which fails `Permission denied` on this VM (`DRM_IOCTL_MODE_CREATE_DUMB failed`).
   `swaymsg output … resolution` still answers `success: true` while nothing changes.
   `WLR_RENDERER=pixman` fixes it. The gate re-reads `swaymsg -t get_outputs` and
   FAILS unless the two heads really are 1920x1200 and 800x600.

**How the bounds are made to shrink.** The predicate only fires when the
compositor's bounds drop BELOW the window's live size. A probe temporarily added to
`toplevel_compute_size` showed what GDK actually delivers on this stack:
`bounds=2720x1200` with both heads attached — the bounding box of the ATTACHED
monitors (1920+800 wide), i.e. sway does not send `xdg_toplevel.configure_bounds`
and GDK falls back. So moving the window alone does not shrink the bounds; the
larger head has to LEAVE. The gate therefore moves the window to the 800x600 head
and then switches the 1920x1200 head off, at which point the probe reads
`bounds=800x600`. The window has still travelled from a large head to a small one
under compositor control, which is exactly the stale-bounds condition the commit
addresses.

**Arm 1 `exceeds` — the clamp is defeated (PASS).** Window realized at 1400x1000 on
the big head, moved to the 800x600 head, big head disabled, then asked for its size
again (1401x1001 — the reference's "resize past the smaller head's dimensions"; the
`+1` stops GTK short-circuiting an unchanged request):

```
V20B arm=exceeds requested=1400x1000 small_head=800x600 (HEADLESS-2)
V20B heads advertised by the compositor: [("HEADLESS-2", 800, 600), ("HEADLESS-1", 1920, 1200)]
V20B before move: surface=1400x1000
V20B outputs now: HEADLESS-2:True:800x600 HEADLESS-1:False:0x0
V20B after move to HEADLESS-2 (800x600) + re-request 1401x1001: surface=1400x1000
V20B arm `exceeds` OK
test result: ok. 1 passed; 0 failed
```

The surface stays **1400x1000 on an 800x600 head** — and stays at its CURRENT size,
not the 1401x1001 that was requested, which is precisely `size.setSize(w, h)`'s
semantics rather than some generic "requests are honoured".

**Arm 2 `fits` — the contrasting control (PASS).** Identical run at 640x480, which
fits the small head, so the predicate must return false and the handler must be a
no-op. GTK's default sizing wins and the window is NOT inflated to the head:

```
V20B arm=fits requested=640x480 small_head=800x600 (HEADLESS-2)
V20B before move: surface=640x480
V20B outputs now: HEADLESS-2:True:800x600 HEADLESS-1:False:0x0
V20B after move to HEADLESS-2 (800x600) + re-request 641x481: surface=641x481
V20B arm `fits` OK
test result: ok. 1 passed; 0 failed
```

Without this arm, "the window is not the monitor's size" would not distinguish a
working handler from one that never runs at all.

**Arm 3 — SOURCE-MUTATION NEGATIVE CONTROL, run and observed to FAIL.** The
re-assertion was disabled in place (`if false && compute_size_decision(…)` in
`toplevel_compute_size`), rebuilt, and arm 1 re-run against the same two heads. The
probe caught the bounds shrinking and GTK's clamp winning:

```
V20BPROBE surface=1x1       bounds=2720x1200
V20BPROBE surface=1400x1000 bounds=2720x1200   (x3)
V20BPROBE surface=1400x1000 bounds=800x600     <- the big head left
V20BPROBE surface=800x600   bounds=800x600     (x2)
V20B after move to HEADLESS-2 (800x600) + re-request 1401x1001: surface=800x600
panicked at gtk_wayland_compute_size.rs (the `exceeds` assertion):
  CLAMP NOT DEFEATED: after moving to the 800x600 head the surface is 800x600,
  i.e. GTK's clamp to the stale monitor bounds won.
test result: FAILED. 0 passed; 1 failed
GATE FAIL: arm `exceeds`: the clamp was NOT defeated
```

That is the pre-`004c88e41` behaviour, reproduced: 1400x1000 → 800x600. The mutation
and the probe were then reverted with `git checkout --` (which rewrites the mtime, so
cargo really did rebuild — a `mv file.bak file` restore would have preserved the older
mtime and silently skipped it) and the gate re-run green.

**STILL NOT exercised, and honestly so:** the maximized / fullscreen /
quick-terminal early returns and the "no GdkSurface yet" return. They are reviewed
and unit-adjacent only; none is reachable from this KAT, which never maximizes,
never goes fullscreen and never builds a quick terminal. That residual is small
enough not to warrant its own blocked row, but it is not claimed green.

## V20a. `2f3814ca5` honor suspended window state (`P3-W8 T5`) — **DISCHARGED 2026-07-31 (P3-W6)**

**The headed control was RUN on the Fedora VM and PASSED.** Full evidence below;
the original "not run" text is kept underneath for provenance.

**Environment.** `ssh gael@192.168.64.4`, Fedora 44, GNOME Shell 50.0 on Wayland
(mutter), one head, `XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0`.
Repo at `276ee821c` ("wip: T5 steps 1-2"), which carries the ported
`update_occlusion`/`window_suspended`/`prop_suspended`. Built to completion
FIRST, then run headed — never concurrently, per this row's own warning.

**How the minimise was driven.** `wtype` is refused by mutter ("Compositor does
not support the virtual keyboard protocol") and `org.gnome.Shell.Eval` returns
`(false, '')` (unsafe mode off). Input was injected from OUTSIDE the app via
mutter's own `org.gnome.Mutter.RemoteDesktop` API — `CreateSession` + `Start` +
`NotifyKeyboardKeycode`, holding ONE D-Bus connection for the session's lifetime
(a `gdbus call` per keystroke destroys the session as the connection closes).
Super+H is GNOME's `minimize` binding here (`gsettings get
org.gnome.desktop.wm.keybindings minimize` → `['<Super>h']`); Alt+Tab restores.
Driving it externally is the point: the claim under test is that a LIVE
COMPOSITOR's decision reaches the core surface.

**Observation probes** (temporary, reverted after the run — see "clean-up"):
an `eprintln!` in `Surface::update_occlusion` printing `mapped`/`suspended`/
`visible`, and one on each side of `drawFrame`'s `if !visible { return; }`.

**RESULT — PASS, both halves of the assertion.**

| Step | Observed |
|---|---|
| steady state | `drawFrame DRAWN` repeatedly |
| **Super+H (minimise)** | `V20A-PROBE occlusion mapped=true suspended=true visible=false` |
| 12 s while minimised | **0** `drawFrame DRAWN`, 0 `drawFrame SKIPPED`, 0 new log lines at all |
| **Alt+Tab (restore)** | `V20A-PROBE occlusion mapped=true suspended=false visible=true` |
| after restore | 13 `drawFrame DRAWN` |

So the occlusion callback fires with `visible=false` on minimise, frame
production stops completely while suspended, and both reverse on restore.

**NEGATIVE CONTROL — executed and observed to FAIL, then reverted.** Reverting
just the conjunction to its pre-`2f3814ca5` form —
`let visible = self.imp().mapped.get();` (dropping `&& !self.window_suspended()`)
— rebuilt and re-run through the identical Super+H injection produced:

```
V20A-PROBE occlusion mapped=true suspended=true visible=true
```

i.e. mutter still reports the toplevel suspended, but the derived visibility
stays `true`, so the core surface is never told it is occluded, and 1 frame was
still drawn in the following 10 s (against 0 for the ported code). That isolates
the ported conjunction as the cause of the pass: without it the observation does
not occur. **HONEST CAVEAT:** the frame-count contrast alone is weak (1 vs 0),
because mutter also stops delivering frame callbacks to a minimised window, so
the render loop stalls somewhat either way. The load-bearing, unambiguous
difference is the `visible=false` delivery, which is present in one arm and
absent in the other.

**Clean-up.** Both probe files were restored from backups **and `touch`ed**, then
rebuilt to completion (the `mv`-preserves-mtime trap: a restore with an older
mtime makes cargo skip the rebuild and the "reverted" state is never actually
compiled). `git status --short` and `git diff --stat` on the VM are both empty.

**Original text, kept for provenance:**

**Ported:** `crates/rustty-apprt/src/gtk/class/surface.rs`
`Surface::update_occlusion()` (now no-argument, `pub`, deriving
`mapped && !window_suspended()`) + `window_suspended()` via
`gtk::Window::is_suspended`; `crates/rustty-apprt/src/gtk/class/window.rs`
`realize()` connects `notify::suspended` → `prop_suspended()` → every surface of
the selected tab's tree.

**Not run:** the plan's `P3-W8 T5 Step 4` control — run headed on the VM,
minimise (or fully cover) the window, and assert the occlusion callback fires
**and frame production stops**. It was not executed in the P3-W8 lane; there is
no unit-level substitute, because the whole content of the change is that a live
compositor's `xdg_toplevel.suspended` reaches the core surface, and a test of the
`mapped && !suspended` conjunction on its own would be vacuous.

**Recipe (VM, `ssh gael@192.168.64.4`):**

```sh
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
cd ~/projects/rustty
# BUILD FIRST, TO COMPLETION — never build concurrently with headed compositor
# work on this VM (that combination preceded three crashes).
cargo build -p rustty --features gtk,wayland
RUSTTY_LOG=debug ./target/debug/rustty 2>&1 | tee /tmp/occl.log &
# then, in the GNOME session: minimise the window, wait ~2s, restore it.
```

Assert in `/tmp/occl.log`: the occlusion path runs on minimise with
`visible=false` and again with `visible=true` on restore, and that no frames are
produced in between (the renderer thread's frame counter / draw log line).
mutter does advertise `xdg_toplevel.suspended` for minimised windows, so this is
reproducible on the one head the VM has — **unlike V20b it needs no second
monitor**, only a headed session.


---

## V21. `P3-W4 T7` Surface-level visibility KAT (dedup + report reaches the pty) — **(a) DISCHARGED by `P3-W11` Task 6; (b) PARTIAL, OWNER: `P3-W11` Task 6** [2026-07-31]

> **Re-owned at the `P3-W4` merge, 2026-07-31.** The row was filed naming
> `P3-W4 T7`, and the filing lane flagged in its own report that this would
> become a closed-workstream orphan the moment `merge: P3-W4` landed — the exact
> shape of the two V19/V20 violations `G-P2` had just caught. It was re-owned in
> the merge commit itself rather than after, so the orphan never existed.

`6c8c07981`'s `Surface.occlusionCallback` change is **ported and compiling, but the
Surface-level half is NOT behaviourally verified.**

**Ported** (`crates/rustty-core/src/surface.rs` `occlusionCallback`): the
`Surface.visible` field, the dedup early-return (`if self.visible == visible
return`), the `terminal.flags.visible` write under the renderer lock, and the
unforced `queueIo(.visibility_report)`.

**Verified elsewhere** (both executed, both with an observed-failing mutation):
- vt: `rustty-vt` `stream_terminal::tests::visibility_reports` — mode 2033
  exists, an explicit `CSI ? 998 n` answers without enabling it, ENABLING always
  emits (count assertion), DISABLING does not, and RIS PRESERVES
  `flags.visible`. Removing the RIS exemption makes it fail.
- termio: `rustty-termio`
  `stream_handler::tests::stream_handler_visibility_report_arms_push_forced_messages`
  — the mode-2033 enable arm and the `CSI ? 998 n` DSR arm each push a FORCED
  `Message::VisibilityReport` carrying `terminal.flags.visible`; disabling
  pushes nothing. Dropping the enable arm makes it fail.

**NOT verified:** the two behaviours that live only in `occlusionCallback` —
(a) the dedup (a repeat same-state callback must push NEITHER a renderer
`Visible` message NOR a `VisibilityReport`; note this is an OBSERVABLE CHANGE to
the renderer path, not an additive feature), and (b) the end-to-end path
"enable mode 2033 -> `occlusionCallback(false)` -> the encoded
`\x1B[?999;2n` reaches the pty write queue".

**Why:** `rustty-core` has no `Surface` test fixture. A `Surface` owns a live
`App`, a renderer thread, an io thread and a pty; there is no existing harness to
extend and building one was out of scope for this workstream.

**Recipe:** extend the `rustty-termio` `exec.rs` `TestTermio` fixture (a
heap-pinned `Termio` with re-seated self-pointers, `crates/rustty-termio/src/exec.rs`)
into a `rustty-core` `TestSurface`: heap-pin a `Surface` with a stub `App`, the
same renderer `State` + guard-less mutex, a real termio mailbox and a real
renderer mailbox. Then:

```rust
// (a) dedup
surface.io.terminal.modes.set(Mode::ReportVisibility, true);
surface.occlusionCallback(false).unwrap();
assert_eq!(1, renderer_mailbox_len());          // one Visible message
assert!(matches!(termio_mailbox_pop(), Some(Message::VisibilityReport { visible: false, force: false })));
surface.occlusionCallback(false).unwrap();      // REPEAT, same state
assert_eq!(1, renderer_mailbox_len());          // still one — nothing pushed
assert!(termio_mailbox_pop().is_none());

// (b) end to end: drain the termio mailbox through Thread::drainMailbox and
//     assert the pty write queue contains "\x1B[?999;2n".
```

**Mutation that must fail once the KAT exists:** delete the
`if self.visible == visible { return Ok(()); }` early-return — (a) must then see
two renderer messages and two reports.

**OWNERSHIP WARNING:** this row is owned by `P3-W4 T7`, which is OPEN only until
its `merge: P3-W4` commit lands. G-P2 derives P3 workstream closure from those
merge commits, so the moment `P3-W4` merges this row becomes an owner-in-a-closed-
workstream violation — exactly the shape of the existing V19/V20 violations.
**Re-own it to an open workstream (or close it) as part of the P3-W4 merge.**

---

### DISPOSITION, `P3-W11` Task 6, 2026-07-31

**(a) the dedup: DISCHARGED.** `crates/rustty-core/tests/surface_visibility_kat.rs`
builds the `TestSurface` this row's recipe describes — a heap-pinned
`MaybeUninit<Surface<TestApp>>` with only the fields `occlusionCallback` reads
written (the `app.rs::fabricateSurface` precedent), carrying a real `Termio`
assembled the way `rustty-termio`'s `exec.rs` `TestTermio` assembles one, with
its self-referential pointers re-seated at the final in-`Surface` address. It
asserts:

* a state CHANGE pushes exactly **one** renderer `Visible(false)` and exactly
  one **unforced** `VisibilityReport { visible: false }`, and updates
  `io.terminal.flags.visible` under the renderer lock;
* a REPEAT same-state callback pushes **neither**;
* a transition back is not deduped;
* with mode 2033 disabled the renderer message still goes out but no report is
  queued.

**Negative control executed and observed RED.** Deleting the
`if self.visible == visible { return Ok(()); }` early-return in
`crates/rustty-core/src/surface.rs`:

```
thread 'surface_occlusion_callback_dedups_and_reports' panicked at
crates/rustty-core/tests/surface_visibility_kat.rs:403:5:
assertion `left == right` failed: a repeat same-state occlusion callback must not reach the renderer
  left: 0
 right: 1
```

Reverted; the KAT is green. A `REBUILD SENTINEL` assert in the test was
separately tripped and observed to fire, so the mutation-restore path is known
to rebuild.

**(b) end to end: PARTIAL, still open.** The KAT pins the encoded payload the
report carries (`encode_visibility_report(NotVisible) == b"\x1b[?999;2n"`), but
the last hop — `termio::Thread::drainMailbox` -> `Termio::visibilityReport` ->
`queueWrite` -> the pty write queue — needs a live `ThreadData` with a real pty
and io thread, which the fabricated `Surface` deliberately does not have
(`queueWrite` takes `&mut ThreadData<M>`, which only `Thread::threadMain`
produces). **Remaining recipe:** extend the fixture with a
`termio::Thread` + `ThreadData` over an `Exec` backend whose pty is a
`rustty_os::pty::Pty` pair, drive one `drainMailbox` tick, and assert the
subprocess-side read end receives `\x1b[?999;2n`. That is a pty-owning fixture,
i.e. its own task, and it is the only part of V21 that remains.

---

## V22. `P3-W11 T4` — the `vicinae_hotkey_v1` WIRE path (F-W4) — **DISCHARGED by `P3-W12`, 2026-07-31**

**The wire half of F-W4 is now verified end to end against a fake compositor.**
`vicinae_hotkey_v1` is unratified and no mainstream compositor (mutter, KWin,
wlroots) implements it, so there is no host that can drive it — but that was never
an environment limit in the sense this file reserves the term (something no work in
this repo can fix). It was a missing harness, and the harness now exists:

`crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs`,
`#[test] hotkeys_wire_path_bound_pressed_denied_revoked` — no longer `#[ignore]`d,
because it needs **no display and no compositor** and runs in the ordinary
`--features gtk,wayland,x11` unit lane in ~10 ms.

### What it stands up

* A minimal fake compositor on `wayland-server` 0.31 — the **sibling** of the
  vendored `wayland-client` 0.31 — whose bindings `wayland-scanner` generates from
  the **same** vendored `crates/rustty-apprt/protocols/vicinae-hotkey-v1.xml` the
  client bindings come from, so both halves speak the same wire by construction
  rather than by a hand-written encoder that could agree with a mis-generated
  client. `wayland-server` is a **dev-dependency only** (`check_acyclic.sh` is the
  enforcement) and the module is `#[cfg(test)]`.
* A `UnixStream::pair` instead of a filesystem socket + `WAYLAND_DISPLAY`: mutating
  the process environment from one test of a multi-threaded test binary is a data
  race. The wire, the XML and both generated halves are identical either way.
* **Two client connections, deliberately.** `host` owns the `wl_display` and is the
  only reader of its socket — it stands in for GDK. rustty's registry connection is
  a guest-mode `Backend::from_foreign_display` over that same display with its own
  private queue, which is literally what `wayland::App::init` builds. This is what
  makes the harness exercise `queue_source::WlQueueSource`: every event below is
  dispatched by the GSource's trampoline in a real `MainContext` turn, which is also
  what drains the DEFERRED `trigger` / `bind-failed` emissions (the second of the two
  forced divergences recorded in `hotkeys.rs`). A harness that called `read()` on
  rustty's own queue would dispatch it as a side effect, leave `prepare_read()`
  reporting "not pending", and the GSource — the component under test — would never
  fire at all.

### The four assertions, and the observed output

```sh
cd ~/projects/rustty      # the Linux VM; this is GTK/Wayland code
cargo test -p rustty-apprt --features gtk,wayland,x11 --lib \
  hotkeys_wire_path_bound_pressed_denied_revoked -- --nocapture
```

```
running 1 test
debug(gtk): hotkey bound action=new_window
debug(gtk): hotkey bound action=close_surface
debug(gtk): hotkey bound action=toggle_quick_terminal
warn(gtk): hotkey denied action=close_surface reason=Ok(1) message=policy forbids this combination
warn(gtk): hotkey revoked action=new_window reason=Ok(0) message=the user removed this shortcut
test gtk::winproto::wayland::hotkeys::tests::hotkeys_wire_path_bound_pressed_denied_revoked ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 123 filtered out; finished in 0.01s
```

1. **`bind`** — one `vicinae_hotkey_manager_v1.bind` per selected entry reaches the
   compositor, compared as a whole struct against the keysym
   `key::keysym_from_trigger` produced, the modifier bits (`shift=1 ctrl=2 alt=4
   super=8`), the null `seat`, the `app_id` and the action description.
2. **`bound`** — three `bound` events dispatch and `bound_count() == entries`. The
   `bound_count` half of this is NOT sufficient on its own: it is already true the
   instant `Entry::bind` issued the request, so it would pass against a compositor
   that never answered. A rustty-only `Hotkeys::bound_event_count()` counter (sibling
   of `Globals::activation_done_count`, same rationale) is what makes the assertion
   real, and it was controlled — deleting the compositor's `bound()` sends makes it
   time out.
3. **`pressed`** — re-emits exactly the bound action on the `trigger` signal, drained
   by the queue GSource.
4. **`denied` / `revoked`** — reach `Entry::fail` -> `emit_bind_failed` and the
   `bind-failed` signal, carrying the compositor's message **BYTES** verbatim and the
   right `revoked` flag, and drop `bound_count()` by one each. The two failed entries
   also release their objects on the wire (`Entry::fail`'s `hotkey.destroy()` is
   observed as two `vicinae_hotkey_v1.destroy` requests server-side), and no further
   `trigger` fires.

### Negative control (mandatory, and watched go red)

The control is the one the row demanded: make `Entry::fail` stop calling
`emit_bind_failed` — it returns `None` instead of the `Emission::BindFailed`, i.e.
a denied binding silently no-ops, which is the exact defect this whole series
exists to stop. Edited **in place** (never `mv file.bak file`, whose older mtime
would make cargo skip the rebuild and invalidate the control) and restored with
`git checkout --`, which writes a fresh mtime.

`denied` arm:

```
warn(gtk): hotkey denied action=close_surface reason=Ok(1) message=policy forbids this combination

thread '...::hotkeys_wire_path_bound_pressed_denied_revoked' panicked at
crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:676:9:
timed out waiting for a bind-failed emission for the denied binding
test result: FAILED. 0 passed; 1 failed; ...
```

`revoked` arm, isolated by temporarily removing the (now-failing, and therefore
arm-masking) `denied` block so the later assertion is actually reached:

```
warn(gtk): hotkey revoked action=new_window reason=Ok(0) message=the user removed this shortcut

thread '...::hotkeys_wire_path_bound_pressed_denied_revoked' panicked at
crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:676:9:
timed out waiting for a bind-failed emission for the revoked binding
test result: FAILED. 0 passed; 1 failed; ...
```

Note what stays GREEN under the control in both runs: the compositor still logs all
three `bound` lines and the `denied`/`revoked` handlers still run. Only the
user-visible notification channel is dead — which is precisely the silent no-op a
"bound works" wire test would have missed.

### Rebuild-sensitivity, proven once

Before trusting any of the above, the same in-place edit flow was proven to actually
reach the compiler: an `assert!(false, "REBUILD SENTINEL")` inserted into the test
body was observed to FAIL (`panicked at ...:719:9: REBUILD SENTINEL`), then removed
in place, after which the test was observed to pass again. No control in this row
rests on an artifact cargo might have skipped rebuilding.

### What is still NOT verified (honest residue)

* `released` is not driven. The port's handler for it is `{}` (`Hotkeys.zig:166`), so
  there is nothing to observe; the generated event `match` in `Hotkeys::on_event` is
  exhaustive, which is what would catch a protocol bump that gave it meaning.
* No REAL compositor has ever answered these requests, because none implements the
  protocol. The harness proves rustty's half of the wire — request encoding, event
  decoding, dispatch routing, the deferred-emission drain and the failure channel —
  not that any specific compositor will behave as the fake one does.
* The graceful-ABSENCE half was already verified separately on a live GNOME/mutter
  session (`tests/gtk_wayland_vicinae_hotkeys.rs`) and by the full 6-step XDG-portal
  chain (`.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`); that is unchanged.

**Protocol-churn note (unchanged).** The XML's own description says the protocol
*"is currently in the testing phase. Backward incompatible changes may be added
together with the corresponding interface version bump."* A future incompatible bump
is EXPECTED: re-vendor `crates/rustty-apprt/protocols/vicinae-hotkey-v1.xml` verbatim
from the oracle and re-run the gates. Both the client bindings AND this harness's
server bindings are generated from that one file, so they cannot drift apart.
