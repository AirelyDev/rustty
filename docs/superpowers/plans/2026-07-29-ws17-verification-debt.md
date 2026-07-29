# WS17 — Verification Debt Discharge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn every open entry in `.port/phase-2/VERIFICATION_DEBT.md` (V2 real-GPU sign-off, V3 KDE/KWin, V4 mutter `set_urgent`, and WS6's V1b cross-DPI residual) into either a **confirmed discharge backed by committed evidence** or an **honest, precisely-worded residual** — with no entry claiming more than was actually executed.

**Architecture:** This workstream produces almost no product code. It produces (a) a turnkey, self-testing hardware-GL probe script so V2 becomes a one-command check on any future GPU box, (b) one new ignored KAT that asserts the KDE Wayland globals actually bind under a real KWin, (c) runtime evidence captured from a **nested/headless** compositor rather than the machine's primary session (this is the de-risking hinge for V4, whose earlier attempt correlated with two hard host crashes), (d) a macOS lldb-instrumented run that proves the cross-DPI content-scale re-stamp path fires, and (e) a rewritten `VERIFICATION_DEBT.md`. Every check in this plan carries an explicit **non-vacuity control**: a positive control that proves the PASS arm is reachable, or a negative control that proves the check can fail.

**Tech Stack:** bash + `glxinfo` (Mesa `GLX_MESA_query_renderer`), Rust 1.93.1 / `cargo test` (ignored KATs), GTK4 + `wayland-client` (`rustty-apprt` winproto), KWin 6.7 `kwin_wayland` (nested), mutter 50 (`--headless --virtual-monitor`), lldb (macOS breakpoint instrumentation), UTM 4.7.5 / Apple Virtualization backend for the VM.

## Global Constraints

Inherited verbatim from `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`:

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: when a WS lands, strike its entries in `.port/phase-2/DEFERRALS.md` / `VERIFICATION_DEBT.md` in the same PR — this pass must leave them truthful.
- Commit style: follow existing `subsystem: summary (chunk-ref)` convention; branch per WS; merge to main only with both platforms green.
- Both-platform build gate before merge: `cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`.

WS17-specific constraints:

- **NEVER run** `.port/tools/gates/no_homebrew_gettext_gate.sh` or anything that renames/moves Homebrew's `libintl` — it mutates shared host state and is out of scope here.
- **Honesty rule:** an item is "discharged" only if a command was actually executed and its output captured to `.port/phase-2/evidence/ws17/`. Anything reasoned-but-not-run is a residual, worded as such. Never write "verified" for "read the code and it looks right".
- **Non-vacuity rule:** every PASS criterion in this plan is paired with a control proving the check can distinguish pass from fail on this hardware. A check with no reachable FAIL arm is a plan defect, not a pass.
- **Session-isolation pattern (SIP)** — any step that mutates shared/global host or VM state (package installs, display-resolution changes, long-lived compositors, lldb-attached app runs) MUST be started detached, in its own session, so a tool-call timeout cannot SIGKILL it mid-mutation:
  - macOS: `setsid nohup caffeinate -di bash /tmp/ws17/<script>.sh > /tmp/ws17/<script>.log 2>&1 < /dev/null &` then poll the log.
  - VM: `ssh rvm 'setsid nohup bash ~/ws17/<script>.sh > ~/ws17/<script>.log 2>&1 < /dev/null & echo started'` then poll with a second `ssh rvm 'tail -20 ~/ws17/<script>.log'`.
- **Rollback trap rule:** any such script installs `trap ... EXIT INT TERM HUP` restoring the pre-state, and begins with a **leftover-detection pre-flight** that aborts if a previous crashed run left state behind (stale pidfile, stale wayland socket, half-installed package set).
- **Non-destructive rule:** WS17 must not repartition, convert, or re-create the VM, and must not change the VM's UTM backend or its display manager default. The macOS data volume has **5.5 GiB free** — no step may write more than ~1 GiB.

### Environment facts established during planning (2026-07-29, do not re-derive)

| Fact | Value | How it was established |
|---|---|---|
| VM hypervisor | UTM 4.7.5, **Apple Virtualization backend** (`config.plist` → `Backend = Apple`) | `plutil -p ~/Library/Containers/com.utmapp.UTM/Data/Documents/'Fedora Linux.utm'/config.plist`; `ssh rvm systemd-detect-virt` → `apple` |
| Guest GPU | virtio-gpu, **3D disabled**: `[drm] features: -virgl -edid -resource_blob -host_visible` | `ssh rvm 'journalctl -k -b \| grep virtio_gpu'` |
| Guest GL driver | `llvmpipe (LLVM 22.1.8, 128 bits)`, Mesa 26.1.4, **`Accelerated: no`**, max core profile 4.6 | `glxinfo -B` on the live GNOME session (see Task 1) |
| Historical virgl measurement | When this host DID run virgl (2026-07-17, recorded in `crates/rustty-renderer/src/opengl/gl/sys.rs:611-628`): `GL_RENDERER = virgl (ANGLE (Apple, Apple M4 Pro, OpenGL 4.1 Metal - 90.5))`, `GL_VERSION = 2.1`, **max core profile 0.0** — every core-profile request 3.2..4.6 failed `EGL_BAD_MATCH` | in-repo doc comment |
| Rustty GL floor | `MIN_VERSION_MAJOR.MIN_VERSION_MINOR = 4.3` core (`crates/rustty-renderer/src/opengl/gl/sys.rs`) | source |
| VM sessions | GNOME/mutter 50 Wayland session live on tty2 (`wayland-0`), GDM active, sway 1.11 installed, `kwin` 6.7.3 available in `updates` (provides `/usr/bin/kwin_wayland`), no KDE installed | `loginctl`, `rpm -qa`, `dnf provides */kwin_wayland` |
| VM disk | 61 GiB free on `/` | `df -h /` |
| macOS host disk | **5.5 GiB free** (`/System/Volumes/Data`, 99% full); VM disk image is 37 GiB | `df -h` |
| Oracle blur protocol | `ext_background_effect` (NOT `org_kde_kwin_blur`) — `ghostty/src/apprt/gtk/winproto/wayland.zig:156`; rustty matches (`Tag::ExtBackgroundEffect`) | source |
| dnf repo hazard | the `cursor` repo prompts for a GPG key import and breaks non-interactive dnf — always pass `--disablerepo=cursor` | observed during planning |

---

## File Structure

| File | Created/Modified | Responsibility |
|---|---|---|
| `.port/tools/gates/hw_gl_probe.sh` | Create | Turnkey V2 check: exits 0 only on a genuinely accelerated GL ≥ 4.3-core driver. Fixture mode (`RUSTTY_HW_GL_PROBE_FIXTURE`) makes the PASS arm testable on a software-only box. |
| `.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt` | Create | Positive-control fixture (synthetic accelerated `glxinfo -B` output). |
| `.port/tools/gates/fixtures/glxinfo-llvmpipe.txt` | Create | Negative-control fixture (software `glxinfo -B` output). |
| `crates/rustty-apprt/tests/gtk_wayland_winproto.rs` | Modify (append one test) | New ignored KAT `kde_globals_bind_under_kwin` — asserts the KDE globals really bind under a live KWin, and prints the full globals dump for the ledger. |
| `.port/phase-2/evidence/ws17/*.txt` / `*.png` | Create | Committed raw evidence (probe outputs, KAT stdout, kwin/mutter logs, lldb transcript, screenshots). |
| `.port/phase-2/VERIFICATION_DEBT.md` | Modify | Final truthful state: V2/V3/V4/V1b each rewritten as discharged-with-evidence or residual-with-exact-reason. |
| `docs/manual-test-plan-gtk.md` | Modify (§W rows) | Record the KDE row outcomes with the date and the evidence path. |
| `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` | Modify (checklist + wave status) | Mark WS17 authored/executed. |

---

## Task 1: Branch, evidence scaffolding, and the V2 environment-ceiling proof

**Files:**
- Create: `.port/phase-2/evidence/ws17/README.md`
- Create: `.port/phase-2/evidence/ws17/v2-vm-gl-ceiling.txt`

**Interfaces:**
- Produces: the branch `ws17-verification-debt`; the evidence directory `.port/phase-2/evidence/ws17/` used by every later task; the file `v2-vm-gl-ceiling.txt` quoted verbatim by Task 12's ledger rewrite.

- [ ] **Step 1: Create the branch**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull --ff-only
git checkout -b ws17-verification-debt
git push -u origin ws17-verification-debt
mkdir -p .port/phase-2/evidence/ws17 /tmp/ws17
# Put the VM on the same branch immediately — every VM task below builds from it.
ssh rvm 'cd ~/projects/rustty && git fetch -q origin && git checkout ws17-verification-debt && git pull --ff-only -q && git log --oneline -1'
```

Expected: the VM prints the same HEAD commit as `git log --oneline -1` on macOS.

**Branch-sync rule for every later task:** after any commit on macOS that the VM must see, run
`git push` then `ssh rvm 'cd ~/projects/rustty && git pull --ff-only -q && git log --oneline -1'`
and confirm the hashes match before running anything on the VM.

- [ ] **Step 2: Write the evidence README**

Create `.port/phase-2/evidence/ws17/README.md`:

```markdown
# WS17 evidence

Raw, unedited command output captured while discharging `.port/phase-2/VERIFICATION_DEBT.md`.
Every claim in that ledger dated 2026-07-29 points at a file here. Nothing in this
directory is hand-written prose about what "would" happen — it is captured output only.

| File | Item | What it is |
|---|---|---|
| `v2-vm-gl-ceiling.txt` | V2 | Proof the VM's GL stack is software-only and why virgl cannot lift it |
| `v2-hw-gl-probe-controls.txt` | V2 | `hw_gl_probe.sh` run on the real box (FAIL) + both fixtures (PASS/FAIL) |
| `v2-llvmpipe-vs-ghostty.txt` | V2 | Same-box rustty vs distro ghostty throughput/resize smoke (narrowing only) |
| `v2-virgl-feasibility.md` | V2 | Why enabling virgl on this host is impossible AND pointless |
| `v3-kwin-install.txt` | V3 | KWin RPM transaction log + `kwin_wayland --help` flags + host-sanity checks |
| `v3-kwin-globals-kat.txt` | V3 | `kde_globals_bind_under_kwin` stdout under nested KWin |
| `v3-kwin-control-gnome.txt` | V3 | Same KAT under GNOME/mutter — the negative control |
| `v3-kwin-visual-*.png` | V3 | Quick-terminal slide / blur / SSD screenshots under KWin |
| `v3-kwin-visual-notes.txt` | V3 | What each screenshot does and does not prove; fallback outcome if any |
| `v4-mutter-headless-kat.txt` | V4 | `wayland_set_urgent_dispatches_activation_token` under headless mutter |
| `v4-mutter-control-nocompositor.txt` | V4 | Same KAT with no compositor — the negative control |
| `v4-primary-session-decision.md` | V4 | The reasoned decision NOT to run on the primary GNOME session |
| `v4-crash-postmortem.txt` | V4 | Only present if the VM died during Task 9; `journalctl -b -1` tail |
| `v1b-restamp-lldb.txt` | V1b | lldb transcript of `rustty_surface_set_content_scale` hits across a live DPI change |
| `v1b-display-modes.txt` | V1b | Pre/post `system_profiler` display mode — proof the host was restored |
```

- [ ] **Step 3: Capture the VM GL ceiling (read-only, no session isolation needed)**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 DISPLAY=:0 \
   XAUTHORITY=$(ls /run/user/1000/.mutter-Xwaylandauth.* | head -1);
 echo "== date =="; date -u;
 echo "== virt =="; systemd-detect-virt;
 echo "== kernel virtio_gpu features =="; journalctl -k -b | grep -i "virtio_gpu\|virgl";
 echo "== glxinfo -B =="; glxinfo -B;
 echo "== mesa =="; rpm -q mesa-dri-drivers virglrenderer' \
 > /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v2-vm-gl-ceiling.txt 2>&1
grep -E "Device:|Accelerated:|Max core|features:" /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v2-vm-gl-ceiling.txt
```

Expected (matches the planning measurement):
```
    Device: llvmpipe (LLVM 22.1.8, 128 bits) (0xffffffff)
    Accelerated: no
    Max core profile version: 4.6
[drm] features: -virgl -edid -resource_blob -host_visible
```

PASS for this step = the file exists and contains a `Device:` line, an `Accelerated:` line and the kernel `features:` line. FAIL = any of those three missing (then the probe is vacuous — fix the command before continuing, do not proceed on partial output).

- [ ] **Step 4: Append the host-side hypervisor proof to the same file**

```bash
{ echo "== macOS host: UTM backend =="
  plutil -p ~/Library/Containers/com.utmapp.UTM/Data/Documents/"Fedora Linux.utm"/config.plist \
    | grep -E '"Backend"|"Architecture"|"CPUCount"|"MemorySize"'
  echo "== macOS host free space =="
  df -h /System/Volumes/Data
  echo "== VM disk image size =="
  ls -l ~/Library/Containers/com.utmapp.UTM/Data/Documents/"Fedora Linux.utm"/Data/*.img
} >> /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v2-vm-gl-ceiling.txt
```

Expected: `"Backend" => "Apple"`, and `Avail` on `/System/Volumes/Data` in the single-digit GiB range.

- [ ] **Step 5: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/README.md .port/phase-2/evidence/ws17/v2-vm-gl-ceiling.txt
git commit -m "ws17: evidence scaffolding + VD:V2 environment GL-ceiling capture (llvmpipe, -virgl, Apple VZ backend)"
```

---

## Task 2: The turnkey hardware-GL probe (`hw_gl_probe.sh`) with both controls

**Files:**
- Create: `.port/tools/gates/hw_gl_probe.sh`
- Create: `.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt`
- Create: `.port/tools/gates/fixtures/glxinfo-llvmpipe.txt`
- Create: `.port/phase-2/evidence/ws17/v2-hw-gl-probe-controls.txt`

**Interfaces:**
- Consumes: nothing from Task 1 except the evidence directory.
- Produces: `.port/tools/gates/hw_gl_probe.sh` — exit 0 = hardware GL ≥ 4.3 core; exit 1 = software / unaccelerated / below the renderer's version floor. Honors `RUSTTY_HW_GL_PROBE_FIXTURE=<path>` to read a canned `glxinfo -B` dump instead of calling `glxinfo`. Task 12's ledger text cites this script as the V2 re-check recipe.

- [ ] **Step 1: Write the negative-control fixture**

Create `.port/tools/gates/fixtures/glxinfo-llvmpipe.txt` (a trimmed copy of this VM's real output — the FAIL arm's ground truth):

```
name of display: :0
display: :0  screen: 0
direct rendering: Yes
Extended renderer info (GLX_MESA_query_renderer):
    Vendor: Mesa (0xffffffff)
    Device: llvmpipe (LLVM 22.1.8, 128 bits) (0xffffffff)
    Version: 26.1.4
    Accelerated: no
    Video memory: 9906MB
    Unified memory: yes
    Preferred profile: core (0x1)
    Max core profile version: 4.6
    Max compat profile version: 4.6
    Max GLES1 profile version: 1.1
    Max GLES[23] profile version: 3.2
OpenGL vendor string: Mesa
OpenGL renderer string: llvmpipe (LLVM 22.1.8, 128 bits)
OpenGL core profile version string: 4.6 (Core Profile) Mesa 26.1.4
```

- [ ] **Step 2: Write the positive-control fixture**

Create `.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt` (synthetic but format-exact — this is a *control*, not evidence of any real run; the header says so):

```
# SYNTHETIC CONTROL FIXTURE — not a real measurement. It exists solely to prove
# hw_gl_probe.sh's PASS arm is reachable on a box that has no hardware GL.
name of display: :0
display: :0  screen: 0
direct rendering: Yes
Extended renderer info (GLX_MESA_query_renderer):
    Vendor: AMD (0x1002)
    Device: AMD Radeon RX 7600 (radeonsi, navi33, LLVM 22.1.8, DRM 3.57) (0x7480)
    Version: 26.1.4
    Accelerated: yes
    Video memory: 8192MB
    Unified memory: no
    Preferred profile: core (0x1)
    Max core profile version: 4.6
    Max compat profile version: 4.6
    Max GLES1 profile version: 1.1
    Max GLES[23] profile version: 3.2
OpenGL vendor string: AMD
OpenGL renderer string: AMD Radeon RX 7600 (radeonsi, navi33, LLVM 22.1.8, DRM 3.57)
OpenGL core profile version string: 4.6 (Core Profile) Mesa 26.1.4
```

- [ ] **Step 3: Write the probe script**

Create `.port/tools/gates/hw_gl_probe.sh`:

```bash
#!/usr/bin/env bash
# WS17 / VD:V2 — hardware-GL probe.
#
# Exit 0 ONLY when the GL driver serving this display is a real hardware driver
# that also clears rustty's OpenGL renderer floor (4.3 core, see
# crates/rustty-renderer/src/opengl/gl/sys.rs MIN_VERSION_MAJOR/MINOR).
#
# This is the ONE command that closes VD:V2's "is the GPU path genuinely active,
# or did we silently fall back to llvmpipe?" question. Run it in the SAME session
# and with the SAME DISPLAY/WAYLAND_DISPLAY that the rustty run under test uses.
#
#   .port/tools/gates/hw_gl_probe.sh
#   RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt \
#     .port/tools/gates/hw_gl_probe.sh     # positive control (must PASS)
#   RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-llvmpipe.txt \
#     .port/tools/gates/hw_gl_probe.sh     # negative control (must FAIL)
set -uo pipefail

fixture="${RUSTTY_HW_GL_PROBE_FIXTURE:-}"
if [ -n "$fixture" ]; then
  info="$(cat "$fixture")" || { echo "FAIL: cannot read fixture $fixture" >&2; exit 1; }
else
  command -v glxinfo >/dev/null 2>&1 || {
    echo "FAIL: glxinfo not installed (dnf install glx-utils) — probe cannot run" >&2; exit 1; }
  info="$(glxinfo -B 2>&1)"
fi
printf '%s\n' "$info"
echo "--- hw_gl_probe verdict ---"

fail() { echo "FAIL: $*" >&2; exit 1; }
get()  { printf '%s\n' "$info" | sed -n "s/^ *$1: *//p" | head -1; }

renderer="$(get 'Device')"
[ -n "$renderer" ] || renderer="$(printf '%s\n' "$info" | sed -n 's/^OpenGL renderer string: *//p' | head -1)"
accel="$(get 'Accelerated')"
core="$(get 'Max core profile version')"
direct="$(printf '%s\n' "$info" | sed -n 's/^direct rendering: *//p' | head -1)"

# Vacuity guards: a probe that cannot see the fields it judges must FAIL LOUDLY,
# never pass by default.
[ -n "$renderer" ] || fail "no renderer string in glxinfo output — probe is vacuous"
[ -n "$core" ]     || fail "no 'Max core profile version' line — probe is vacuous"

case "$renderer" in
  *llvmpipe*|*softpipe*|*swrast*|*"Software Rasterizer"*|*"lavapipe"*)
    fail "software renderer: $renderer" ;;
esac

if [ -n "$accel" ]; then
  [ "$accel" = "yes" ] || fail "GLX_MESA_query_renderer reports Accelerated: $accel (renderer=$renderer)"
else
  # Non-Mesa stacks (NVIDIA proprietary) omit the extension; fall back to direct rendering.
  [ "$direct" = "Yes" ] || fail "no Accelerated: line and direct rendering=${direct:-<absent>}"
fi

major="${core%%.*}"; minor="${core#*.}"; minor="${minor%%.*}"
case "$major$minor" in ''|*[!0-9]*) fail "unparseable core profile version '$core'" ;; esac
if [ "$major" -lt 4 ] || { [ "$major" -eq 4 ] && [ "$minor" -lt 3 ]; }; then
  fail "max core profile $core < 4.3 — rustty's GL renderer floor cannot be met on this driver"
fi

echo "PASS: hardware GL — renderer='$renderer' accelerated='${accel:-n/a}' direct='${direct:-n/a}' max-core='$core'"
```

- [ ] **Step 4: Make it executable and run the negative control (must FAIL, exit 1)**

```bash
cd /Users/gael/projects/rustty
chmod +x .port/tools/gates/hw_gl_probe.sh
RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-llvmpipe.txt \
  .port/tools/gates/hw_gl_probe.sh; echo "exit=$?"
```

Expected: `FAIL: software renderer: llvmpipe (LLVM 22.1.8, 128 bits) (0xffffffff)` and `exit=1`.

- [ ] **Step 5: Run the positive control (must PASS, exit 0) — this is the non-vacuity proof**

```bash
cd /Users/gael/projects/rustty
RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt \
  .port/tools/gates/hw_gl_probe.sh; echo "exit=$?"
```

Expected: `PASS: hardware GL — renderer='AMD Radeon RX 7600 (radeonsi, navi33, LLVM 22.1.8, DRM 3.57) (0x7480)' accelerated='yes' direct='Yes' max-core='4.6'` and `exit=0`.

If this does not print PASS, the probe's pass arm is unreachable and the whole V2 recipe is worthless — fix the script before continuing.

- [ ] **Step 6: Run it for real on the VM (expected FAIL — that is the V2 finding)**

```bash
cd /Users/gael/projects/rustty
ssh rvm 'cd ~/projects/rustty && git fetch -q origin && true'  # no-op connectivity check
scp .port/tools/gates/hw_gl_probe.sh rvm:/tmp/hw_gl_probe.sh
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 DISPLAY=:0 \
   XAUTHORITY=$(ls /run/user/1000/.mutter-Xwaylandauth.* | head -1);
 chmod +x /tmp/hw_gl_probe.sh; /tmp/hw_gl_probe.sh; echo "exit=$?"'
```

Expected: the glxinfo dump, then `FAIL: software renderer: llvmpipe (LLVM 22.1.8, 128 bits) (0xffffffff)` and `exit=1`.

- [ ] **Step 7: Capture all three runs into one evidence file**

```bash
cd /Users/gael/projects/rustty
{ echo "== date =="; date -u
  echo "== negative control (llvmpipe fixture) =="
  RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-llvmpipe.txt \
    .port/tools/gates/hw_gl_probe.sh 2>&1; echo "exit=$?"
  echo "== positive control (synthetic radeonsi fixture) =="
  RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt \
    .port/tools/gates/hw_gl_probe.sh 2>&1; echo "exit=$?"
  echo "== real run on rvm =="
  ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 DISPLAY=:0 \
     XAUTHORITY=$(ls /run/user/1000/.mutter-Xwaylandauth.* | head -1); /tmp/hw_gl_probe.sh 2>&1; echo "exit=$?"'
} > .port/phase-2/evidence/ws17/v2-hw-gl-probe-controls.txt 2>&1
tail -5 .port/phase-2/evidence/ws17/v2-hw-gl-probe-controls.txt
```

- [ ] **Step 8: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/tools/gates/hw_gl_probe.sh .port/tools/gates/fixtures \
        .port/phase-2/evidence/ws17/v2-hw-gl-probe-controls.txt
git commit -m "ws17: hw_gl_probe.sh — turnkey non-vacuous hardware-GL gate for VD:V2 (+ pass/fail control fixtures)"
```

---

## Task 3: V2 narrowing run — rustty vs distro ghostty on the same (software) box

**Files:**
- Create: `.port/phase-2/evidence/ws17/v2-llvmpipe-vs-ghostty.txt`

**Interfaces:**
- Consumes: `.port/phase-2/evidence/ws17/` from Task 1.
- Produces: measured same-box throughput/resize numbers for rustty vs `ghostty 1.3.1-2.fc44`. This NARROWS V2 (it discharges the "compare side-by-side with ghostty on the same box" half of the recipe for the software path) — it does NOT discharge V2, which is specifically about real GL. Task 12 must word it exactly that way.

**Why this is worth doing:** V2's recipe has four sub-checks; three of them are "compare to ghostty on the same machine". Running that comparison on llvmpipe cannot prove hardware behaviour, but it does prove rustty is not *relatively* worse than the reference on identical hardware — which is the part of the concern that is measurable here. Keep the claim that narrow.

- [ ] **Step 1: Build the GTK release binary on the VM (SIP — long-running, own session)**

```bash
ssh rvm 'mkdir -p ~/ws17 && cat > ~/ws17/build.sh <<"EOF"
set -euo pipefail
cd ~/projects/rustty
cargo build --release --features gtk
echo "BUILD_OK $(date -u)"
EOF
setsid nohup bash ~/ws17/build.sh > ~/ws17/build.log 2>&1 < /dev/null & echo started'
```

Poll until done:

```bash
ssh rvm 'tail -3 ~/ws17/build.log'
```

Expected eventually: `BUILD_OK <timestamp>`. FAIL: a compiler error — fix it before continuing (a WS17 task must never merge with a red build).

- [ ] **Step 2: Prepare a 64 MiB throughput corpus on the VM**

```bash
ssh rvm 'cd ~/ws17 && [ -f corpus.txt ] || (base64 /dev/urandom | head -c 67108864 > corpus.txt); ls -l ~/ws17/corpus.txt'
```

Expected: `-rw-r--r-- ... 67108864 ... corpus.txt`.

- [ ] **Step 3: Time `cat corpus.txt` in rustty and in ghostty on the primary GNOME session**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0;
 cd ~/projects/rustty;
 for i in 1 2 3; do
   /usr/bin/time -f "rustty run$i %e s" env RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra \
     target/release/rustty -e bash -lc "time cat ~/ws17/corpus.txt >/dev/null; sleep 0.2" 2>&1 | tail -2
 done;
 for i in 1 2 3; do
   /usr/bin/time -f "ghostty run$i %e s" ghostty -e bash -lc "time cat ~/ws17/corpus.txt >/dev/null; sleep 0.2" 2>&1 | tail -2
 done'
```

PASS: rustty's wall time is within 1.5× of ghostty's on the same box, three runs each.
FAIL: rustty > 1.5× ghostty — that is a real regression finding; record it in the ledger and open it as a follow-up rather than hiding it.

- [ ] **Step 4: Record the comparison and its explicit limitation**

```bash
cd /Users/gael/projects/rustty
{ echo "== date =="; date -u
  echo "== LIMITATION =="
  echo "Both apps ran on llvmpipe (Accelerated: no). This comparison narrows VD:V2's"
  echo "same-box comparison sub-checks for the SOFTWARE path only. It says NOTHING"
  echo "about hardware-GL behaviour and does not discharge V2."
  echo "== measurements =="
  ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0; cd ~/projects/rustty;
   for i in 1 2 3; do /usr/bin/time -f "rustty run$i %e s" env RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra \
     target/release/rustty -e bash -lc "cat ~/ws17/corpus.txt >/dev/null; sleep 0.2" 2>&1 | tail -1; done;
   for i in 1 2 3; do /usr/bin/time -f "ghostty run$i %e s" ghostty -e bash -lc \
     "cat ~/ws17/corpus.txt >/dev/null; sleep 0.2" 2>&1 | tail -1; done'
} > .port/phase-2/evidence/ws17/v2-llvmpipe-vs-ghostty.txt 2>&1
cat .port/phase-2/evidence/ws17/v2-llvmpipe-vs-ghostty.txt
```

- [ ] **Step 5: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v2-llvmpipe-vs-ghostty.txt
git commit -m "ws17: VD:V2 narrowing — same-box rustty vs ghostty 1.3.1 throughput on llvmpipe (does NOT close V2)"
```

---

## Task 4: V2 — the virgl feasibility verdict (analysis + a DO-NOT-RUN appendix)

**Files:**
- Create: `.port/phase-2/evidence/ws17/v2-virgl-feasibility.md`

**Interfaces:**
- Consumes: `v2-vm-gl-ceiling.txt` (Task 1), `v2-hw-gl-probe-controls.txt` (Task 2).
- Produces: the reasoned, evidence-cited verdict that Task 12 copies into the ledger.

**The verdict, stated up front so the executing engineer does not "try it anyway":** enabling virgl on this VM is (a) impossible without re-creating the VM under UTM's QEMU backend, (b) impossible to do non-destructively given 5.5 GiB free host disk vs a 37 GiB image, and (c) **pointless even if done**, because when this exact host DID run virgl (2026-07-17) it exposed `Max core profile version: 0.0` and every core-profile request from 3.2 to 4.6 failed `EGL_BAD_MATCH` — while rustty's OpenGL renderer requires 4.3 **core**. virgl-on-ANGLE-on-Metal has no desktop core profile at all. So virgl would not give V2 a hardware path; it would give a driver on which the renderer cannot even initialize.

- [ ] **Step 1: Write the feasibility note**

Create `.port/phase-2/evidence/ws17/v2-virgl-feasibility.md`:

```markdown
# VD:V2 — why virgl does not discharge this item on this host (2026-07-29)

## Measured state
- `systemd-detect-virt` → `apple`; UTM `config.plist` → `Backend = Apple`.
  UTM's Apple Virtualization backend gives Linux guests a plain virtio-gpu with no
  3D: `[drm] features: -virgl -edid -resource_blob -host_visible`
  (`v2-vm-gl-ceiling.txt`). Nothing inside the guest can turn that on.
- Current driver: `llvmpipe (LLVM 22.1.8, 128 bits)`, `Accelerated: no`
  (`v2-hw-gl-probe-controls.txt`, real-run section).

## Why switching to UTM's QEMU backend (the only way to get virgl here) is rejected
1. **It would not help.** This host has run virgl before — recorded in
   `crates/rustty-renderer/src/opengl/gl/sys.rs:611-628` (measured 2026-07-17):
   `GL_RENDERER = virgl (ANGLE (Apple, Apple M4 Pro, OpenGL 4.1 Metal - 90.5))`,
   `GL_VERSION = 2.1`, `Max core profile version: 0.0`. Every core-profile request
   from 3.2 through 4.6 failed `EGL_BAD_MATCH` across surfaceless, GBM and X11
   platforms. rustty's GL renderer floor is 4.3 core
   (`MIN_VERSION_MAJOR`/`MIN_VERSION_MINOR`). virgl on an Apple-Silicon host
   therefore CANNOT run the renderer at all, let alone sign it off.
2. **It is not non-destructive here.** The Apple-backend VM cannot be converted
   in place; a QEMU-backend VM must be created and the 37 GiB disk image imported.
   The macOS data volume has 5.5 GiB free (99% used). Copying is impossible and
   sharing one image between two VM definitions risks corrupting the only
   development VM.

## What WOULD discharge V2
Any Linux box with an amdgpu/intel/nvidia GPU (bare metal, or a QEMU/KVM guest on
an x86_64 host with virtio-gpu-gl where virgl exposes a real core profile):

    .port/tools/gates/hw_gl_probe.sh          # must print PASS (exit 0) FIRST
    cargo build --release --features gtk
    RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/release/rustty

then VD:V2's four sub-checks. The probe must be run in the SAME session as the
app; without a PASS from it, any rendering observation is unattributable and the
sign-off is vacuous.
```

- [ ] **Step 2: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v2-virgl-feasibility.md
git commit -m "ws17: VD:V2 virgl feasibility verdict — Apple VZ has no virgl, and virgl exposes no 4.3 core anyway"
```

---

## Task 5: V3 — install KWin on the VM (session-isolated, trapped, leftover-detecting)

**Files:**
- Create (on the VM, not committed): `~/ws17/install-kwin.sh`
- Create: `.port/phase-2/evidence/ws17/v3-kwin-install.txt`

**Interfaces:**
- Produces: `/usr/bin/kwin_wayland` (KWin 6.7.3) on the VM, with GDM and the default target UNCHANGED. Tasks 6 and 7 consume it.

**Shared-state mutation:** yes — an RPM transaction. Therefore SIP + rollback trap + leftover pre-flight are mandatory.

**Deliberate scope limit:** install `kwin` (+ its dependencies) ONLY. Do **not** install `@kde-desktop`, `sddm`, or `plasma-workspace` in this task — a display-manager swap is exactly the kind of shared-state change that can leave the VM unbootable to a graphical session. `plasma-workspace` is the Task 8 fallback and is installed only if Task 6/7 fail.

- [ ] **Step 1: Leftover-detection pre-flight**

```bash
ssh rvm 'echo "kwin installed: $(rpm -q kwin 2>&1)";
 echo "sddm installed: $(rpm -q sddm 2>&1)";
 echo "default target: $(systemctl get-default)";
 echo "display-manager: $(systemctl status display-manager --no-pager 2>/dev/null | head -1)";
 echo "stale ws17 state: $(ls ~/ws17/*.pid ~/ws17/*.stamp 2>/dev/null || echo none)";
 echo "stale sockets: $(ls /run/user/1000/wayland-kwinkat* /run/user/1000/wayland-kat* 2>/dev/null || echo none)"'
```

PASS to continue: `kwin` reports "not installed", default target is `graphical.target`, display-manager line mentions `gdm`, and both "stale" lines say `none`.
FAIL: any stale socket/pidfile → a previous crashed run left state; clean it explicitly (`ssh rvm 'rm -f /run/user/1000/wayland-kwinkat* ~/ws17/*.pid ~/ws17/*.stamp'`) and re-run this step. **Never** treat leftover state as a clean start.

- [ ] **Step 2: Write the install script with a rollback trap**

```bash
ssh rvm 'mkdir -p ~/ws17 && cat > ~/ws17/install-kwin.sh <<"EOF"
set -euo pipefail
STAMP=~/ws17/kwin-install.stamp
# Leftover detection inside the script too: a half-finished previous run must not
# be mistaken for a clean state.
if [ -f "$STAMP" ]; then
  echo "ABORT: $STAMP exists — a previous install run did not complete cleanly."
  exit 2
fi
date -u > "$STAMP"
PRE_DEFAULT="$(systemctl get-default)"
echo "PRE default target: $PRE_DEFAULT"
rollback() {
  rc=$?
  # Restore only what we could have changed: the default target. The RPM set is
  # left in place deliberately (removing it mid-failure is riskier than keeping it);
  # Task 5 Step 6 records exactly what was installed so it can be removed later.
  cur="$(systemctl get-default)"
  if [ "$cur" != "$PRE_DEFAULT" ]; then
    echo "ROLLBACK: restoring default target $cur -> $PRE_DEFAULT"
    sudo systemctl set-default "$PRE_DEFAULT" || true
  fi
  rm -f "$STAMP"
  echo "EXIT rc=$rc"
}
trap rollback EXIT INT TERM HUP

sudo dnf install -y --disablerepo=cursor kwin glx-utils
echo "POST default target: $(systemctl get-default)"
echo "POST display-manager: $(systemctl status display-manager --no-pager 2>/dev/null | head -1)"
rpm -q kwin
command -v kwin_wayland
echo "INSTALL_OK $(date -u)"
EOF
echo written'
```

- [ ] **Step 3: Run it detached (SIP)**

```bash
ssh rvm 'setsid nohup bash ~/ws17/install-kwin.sh > ~/ws17/install-kwin.log 2>&1 < /dev/null & echo started'
```

Poll:

```bash
ssh rvm 'tail -20 ~/ws17/install-kwin.log'
```

PASS: the log ends with `/usr/bin/kwin_wayland`, `INSTALL_OK <timestamp>`, `EXIT rc=0`, and both `POST default target: graphical.target` and a `POST display-manager:` line still naming gdm.
FAIL: `ABORT:` (stale stamp — go back to Step 1), a dnf error, or any change in the default target / display manager. If the default target changed, the trap already restored it; verify with `ssh rvm 'systemctl get-default'` before proceeding.

- [ ] **Step 4: Confirm GDM and the primary session are untouched**

```bash
ssh rvm 'systemctl get-default; systemctl is-active gdm; systemctl is-active sddm; loginctl list-sessions; pgrep -a gnome-shell | head -1'
```

Expected: `graphical.target`, `active`, `inactive`, the same three sessions, gnome-shell still running. This is the "we did not break the machine" check.

- [ ] **Step 5: Discover the actual kwin_wayland flag surface (version-dependent — do NOT guess)**

```bash
ssh rvm 'kwin_wayland --help 2>&1 | head -60'
```

Record which of `--socket`, `--width`, `--height`, `--xwayland`, `--virtual`, `--output-count` exist. Task 6 picks its launch line from this output; if `--socket` is absent, use `--wayland-display` if offered, and if neither exists, go to Task 8's fallback.

- [ ] **Step 6: Record install evidence**

```bash
cd /Users/gael/projects/rustty
{ echo "== date =="; date -u
  ssh rvm 'cat ~/ws17/install-kwin.log; echo "== rpm set =="; rpm -q kwin kwin-common 2>&1;
   echo "== kwin_wayland flags =="; kwin_wayland --help 2>&1 | head -60;
   echo "== host still sane =="; systemctl get-default; systemctl is-active gdm; loginctl list-sessions'
} > .port/phase-2/evidence/ws17/v3-kwin-install.txt 2>&1
tail -20 .port/phase-2/evidence/ws17/v3-kwin-install.txt
```

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v3-kwin-install.txt
git commit -m "ws17: VD:V3 — install KWin 6.7.3 on rvm (nested-only; GDM/default target untouched)"
```

---

## Task 6: V3 — the KDE-globals KAT under a nested KWin (with a GNOME negative control)

**Files:**
- Modify: `crates/rustty-apprt/tests/gtk_wayland_winproto.rs` (append one `#[ignore]` test at end of file)
- Create: `.port/phase-2/evidence/ws17/v3-kwin-globals-kat.txt`
- Create: `.port/phase-2/evidence/ws17/v3-kwin-control-gnome.txt`

**Interfaces:**
- Consumes: `/usr/bin/kwin_wayland` from Task 5; `rustty_apprt::gtk::winproto::{App, Protocol}` and `rustty_config::Config as CoreConfig`, both already imported at the top of `gtk_wayland_winproto.rs`.
- Produces: the ignored test `kde_globals_bind_under_kwin` — run with
  `cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --ignored --exact kde_globals_bind_under_kwin --nocapture`.

**Ledger correction discovered while planning:** VERIFICATION_DEBT V3 names `org_kde_kwin_blur`. The oracle (`ghostty/src/apprt/gtk/winproto/wayland.zig:156`) and rustty both use **`ext_background_effect`** (`Tag::ExtBackgroundEffect`) for blur — `org_kde_kwin_blur` is not bound anywhere. Task 12 must fix that wording. The KAT below asserts on the field that actually exists (`has_ext_background_effect`) and records whatever KWin 6.7.3 advertises.

- [ ] **Step 1: Write the failing test**

Append to `crates/rustty-apprt/tests/gtk_wayland_winproto.rs`:

```rust
/// VD:V3 — RUNTIME EVIDENCE THAT THE KDE-ONLY GLOBALS ACTUALLY BIND UNDER KWin.
///
/// The wayland winproto arm binds three KDE globals that no other compositor in
/// this environment advertises: `org_kde_kwin_server_decoration_manager`,
/// `org_kde_kwin_slide_manager`, and `kde_output_order_v1` — plus
/// `ext_background_effect` (the blur protocol the oracle actually uses;
/// `wayland.zig:156`). On sway and GNOME every one of them is absent, and the
/// arm's graceful-absence behaviour is already verified. This KAT is the other
/// half: on a REAL KWin the registry binder must return `Some` for them.
///
/// It asserts on `App`'s Debug dump, which forwards to `Globals`'s Debug impl
/// (`has_kde_decoration_manager` / `has_kde_slide_manager` / `has_kde_output_order`
/// / `has_ext_background_effect` / `state.bg_blur_supported`) — the only
/// externally observable view of the bound globals.
///
/// NEGATIVE CONTROL: running this same test on the GNOME/mutter session must FAIL
/// on the first assertion. That is what proves the assertions are not vacuous.
#[test]
#[ignore = "headed GTK + a live KDE/KWin Wayland compositor; run (do NOT set GDK_BACKEND=x11): \
            cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto \
            -- --ignored --exact kde_globals_bind_under_kwin --nocapture"]
fn kde_globals_bind_under_kwin() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");

    assert_eq!(
        app.protocol(),
        Protocol::Wayland,
        "this KAT must run on a Wayland session (KWin), not X11/noop"
    );

    let dump = format!("{app:?}");
    println!("winproto globals dump: {dump}");

    assert!(
        dump.contains("has_kde_decoration_manager: true"),
        "KWin advertises org_kde_kwin_server_decoration_manager; the registry \
         binder must have bound it. dump={dump}"
    );
    assert!(
        dump.contains("has_kde_slide_manager: true"),
        "KWin advertises org_kde_kwin_slide_manager; the registry binder must \
         have bound it (this is what gives the quick terminal its compositor-side \
         slide). dump={dump}"
    );
    // The remaining two are RECORDED, not asserted: `kde_output_order_v1` is a
    // Plasma-shell-adjacent global and `ext_background_effect` is a young ext
    // protocol, so a bare KWin may legitimately advertise neither. Their true
    // values go into the ledger from this printed line.
    println!(
        "RECORDED(no assertion): has_kde_output_order / has_ext_background_effect / \
         bg_blur_supported are in the dump above"
    );
}
```

- [ ] **Step 2: Verify it compiles and is correctly ignored by default**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/tests/gtk_wayland_winproto.rs
git commit -m "ws17: VD:V3 KAT — kde_globals_bind_under_kwin (ignored; asserts the KDE globals bind)"
git push
ssh rvm 'cd ~/projects/rustty && git pull --ff-only -q && git log --oneline -1 &&
 cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --list 2>&1 | grep kde_globals'
```

Expected: `kde_globals_bind_under_kwin: test` appears in the list.
FAIL: a compile error (fix), or the test missing (the append landed outside the `#![cfg(...)]`-gated file body).

- [ ] **Step 3: Run the NEGATIVE CONTROL first — the KAT on GNOME/mutter must FAIL**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland;
 cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk,wayland \
   --test gtk_wayland_winproto -- --ignored --exact kde_globals_bind_under_kwin --nocapture 2>&1 | tail -25' \
 | tee /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-control-gnome.txt
```

Expected: `has_kde_decoration_manager: false` in the printed dump and a panic on the first assertion → `test result: FAILED`.
If this **passes** on GNOME, the assertions are vacuous (or mutter grew the protocols) — stop and re-derive the check before claiming anything about KWin.

- [ ] **Step 4: Launch a nested KWin (SIP — long-lived compositor, own session, trapped)**

```bash
ssh rvm 'cat > ~/ws17/kwin-nested.sh <<"EOF"
set -uo pipefail
export XDG_RUNTIME_DIR=/run/user/1000
SOCK=wayland-kwinkat
PID=~/ws17/kwin.pid
if [ -e "$XDG_RUNTIME_DIR/$SOCK" ] || [ -f "$PID" ]; then
  echo "ABORT: leftover nested-kwin state ($SOCK socket or pidfile) — clean it first"; exit 2
fi
cleanup() {
  rc=$?
  if [ -f "$PID" ]; then kill "$(cat "$PID")" 2>/dev/null || true; fi
  rm -f "$PID" "$XDG_RUNTIME_DIR/$SOCK" "$XDG_RUNTIME_DIR/$SOCK.lock"
  echo "CLEANUP rc=$rc"
}
trap cleanup EXIT INT TERM HUP
# Nested on the existing GNOME session: KWin uses its wayland backend because
# WAYLAND_DISPLAY is set, and serves its own socket so nothing touches wayland-0.
WAYLAND_DISPLAY=wayland-0 kwin_wayland --socket "$SOCK" --width 1600 --height 1000 --xwayland &
echo $! > "$PID"
sleep 5
if [ ! -e "$XDG_RUNTIME_DIR/$SOCK" ]; then echo "FAIL: kwin socket never appeared"; exit 1; fi
echo "KWIN_UP pid=$(cat $PID) socket=$SOCK"
wait "$(cat "$PID")"
EOF
setsid nohup bash ~/ws17/kwin-nested.sh > ~/ws17/kwin-nested.log 2>&1 < /dev/null & echo started'
sleep 8
ssh rvm 'tail -20 ~/ws17/kwin-nested.log; ls /run/user/1000/wayland-kwinkat*'
```

PASS: `KWIN_UP pid=<n> socket=wayland-kwinkat` and the socket file exists.
FAIL modes and what to do:
- `ABORT: leftover ...` → `ssh rvm 'rm -f ~/ws17/kwin.pid /run/user/1000/wayland-kwinkat*'`, re-run.
- `--socket` unrecognized → re-read Task 5 Step 5's flag dump and substitute the offered flag (e.g. `--wayland-display wayland-kwinkat`).
- `FAIL: kwin socket never appeared` → try the virtual backend instead: replace the launch line with `kwin_wayland --virtual --socket "$SOCK" --width 1600 --height 1000` and re-run. If that also fails, go to Task 8.

- [ ] **Step 5: Run the KAT against the nested KWin**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat GDK_BACKEND=wayland;
 cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk,wayland \
   --test gtk_wayland_winproto -- --ignored --exact kde_globals_bind_under_kwin --nocapture 2>&1 | tail -25' \
 | tee /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-globals-kat.txt
```

PASS: `has_kde_decoration_manager: true`, `has_kde_slide_manager: true`, `test result: ok. 1 passed`.
FAIL: any `false` on those two → a real gap in the registry binder (not an environment problem, since KWin does advertise them) — investigate with systematic-debugging before touching the ledger.

- [ ] **Step 6: Also run the existing winproto KATs under KWin (free coverage)**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat GDK_BACKEND=wayland;
 cd ~/projects/rustty && cargo test -p rustty-apprt --features gtk,wayland \
   --test gtk_wayland_winproto -- --ignored --nocapture 2>&1 | tail -30' \
 >> /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-globals-kat.txt
tail -30 /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-globals-kat.txt
```

Expected: `wayland_app_init_selects_wayland_arm` ok and `wayland_set_urgent_dispatches_activation_token` ok (KWin implements `xdg_activation_v1`) — a bonus data point for V4.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v3-kwin-globals-kat.txt \
        .port/phase-2/evidence/ws17/v3-kwin-control-gnome.txt
git commit -m "ws17: VD:V3 — KWin/GNOME control runs for kde_globals_bind_under_kwin (slide + SSD managers bind)"
git push
```

---

## Task 7: V3 — the three visual KDE checks under nested KWin

**Files:**
- Create: `.port/phase-2/evidence/ws17/v3-kwin-visual-quickterm.png`, `v3-kwin-visual-blur.png`, `v3-kwin-visual-ssd.png`, `v3-kwin-visual-notes.txt`

**Interfaces:**
- Consumes: the nested KWin from Task 6 Step 4 (still running on `wayland-kwinkat`), the release binary from Task 3 Step 1.
- Produces: the three screenshots + the notes file cited by Task 12.

**Screenshot mechanism:** KWin's own D-Bus screenshot API is Plasma-coupled; with a bare KWin use `grim` if present, else take the screenshot from the OUTER GNOME session (the nested KWin renders into a window on `wayland-0`, so a full-screen GNOME screenshot captures it).

- [ ] **Step 1: Confirm KWin is still up and pick a screenshot tool**

```bash
ssh rvm 'ls /run/user/1000/wayland-kwinkat; command -v grim gnome-screenshot spectacle 2>/dev/null; echo "---";
 rpm -q grim 2>&1'
```

If none of the tools exist: `ssh rvm 'sudo dnf install -y --disablerepo=cursor grim'` (small, no shared-state risk beyond the RPM set; record it in `v3-kwin-visual-notes.txt`).

- [ ] **Step 2: Quick terminal with the KWin slide**

```bash
ssh rvm 'mkdir -p ~/ws17/cfg && cat > ~/ws17/cfg/config <<EOF
keybind = super+grave_accent=toggle_quick_terminal
EOF
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat GDK_BACKEND=wayland;
cd ~/projects/rustty;
setsid nohup env XDG_CONFIG_HOME=~/ws17/cfg RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra \
  target/release/rustty > ~/ws17/rustty-kwin.log 2>&1 < /dev/null & echo started'
sleep 6
ssh rvm 'tail -5 ~/ws17/rustty-kwin.log'
```

Then press `Super+\`` inside the nested KWin window (the nested compositor has keyboard focus when its window is focused on the GNOME session) and capture:

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat; grim ~/ws17/qt.png && echo captured'
scp rvm:~/ws17/qt.png /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-visual-quickterm.png
```

PASS: the drop-down terminal is anchored full-width at the top of the KWin output, and the earlier KAT already proved `has_kde_slide_manager: true` (the slide itself is compositor-side animation — its *presence* is what V3 asserts, and the bound manager plus a working drop-down is that evidence; a still screenshot cannot show an animation and must not be claimed to).
FAIL: nothing drops down → capture `~/ws17/rustty-kwin.log` and record V3.1 as NOT discharged.

- [ ] **Step 3: Blur (`background-opacity` + `background-blur`)**

```bash
ssh rvm 'cat > ~/ws17/cfg/config <<EOF
background-opacity = 0.8
background-blur = true
EOF
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat GDK_BACKEND=wayland;
cd ~/projects/rustty; pkill -f "target/release/rustty" || true; sleep 1;
setsid nohup env XDG_CONFIG_HOME=~/ws17/cfg RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra \
  target/release/rustty > ~/ws17/rustty-blur.log 2>&1 < /dev/null & echo started'
sleep 6
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat; grim ~/ws17/blur.png && echo captured;
 grep -i "blur\|background_effect" ~/ws17/rustty-blur.log | head'
scp rvm:~/ws17/blur.png /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-visual-blur.png
```

PASS: the window is translucent AND the content behind it is blurred rather than merely see-through.
CONDITIONAL FAIL (expected-possible, record honestly): if Task 6's dump showed `has_ext_background_effect: false`, blur cannot engage — rustty's blur rides `ext-background-effect-v1`, not `org_kde_kwin_blur`, and KWin 6.7.3 may not advertise it. In that case the correct ledger wording is "translucency only; `ext_background_effect` not advertised by KWin 6.7.3, so the blur path is untestable here" — NOT "blur failed" and NOT "blur verified".

- [ ] **Step 4: Server-side decorations**

```bash
ssh rvm 'cat > ~/ws17/cfg/config <<EOF
window-decoration = server
EOF
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat GDK_BACKEND=wayland;
cd ~/projects/rustty; pkill -f "target/release/rustty" || true; sleep 1;
setsid nohup env XDG_CONFIG_HOME=~/ws17/cfg RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra \
  target/release/rustty > ~/ws17/rustty-ssd.log 2>&1 < /dev/null & echo started'
sleep 6
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-kwinkat; grim ~/ws17/ssd.png && echo captured'
scp rvm:~/ws17/ssd.png /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v3-kwin-visual-ssd.png
```

PASS: a KWin (Breeze) titlebar frames the window — visually distinct from the GTK client-side headerbar.
Then flip to `window-decoration = client` and re-capture; the KWin titlebar must disappear. Both states are required: one screenshot alone does not distinguish "SSD works" from "KWin decorates everything".

- [ ] **Step 5: Write the notes file and tear the nested KWin down**

```bash
cd /Users/gael/projects/rustty
{ echo "== date =="; date -u
  echo "V3 visual checks under nested KWin 6.7.3 on rvm (socket wayland-kwinkat)."
  echo "Screenshots: v3-kwin-visual-{quickterm,blur,ssd}.png"
  echo "Slide: a still image cannot show an animation; the assertion V3 makes is the"
  echo "PRESENCE of org_kde_kwin_slide_manager, proven by v3-kwin-globals-kat.txt."
  ssh rvm 'echo "== rustty logs =="; tail -5 ~/ws17/rustty-kwin.log ~/ws17/rustty-blur.log ~/ws17/rustty-ssd.log'
} > .port/phase-2/evidence/ws17/v3-kwin-visual-notes.txt 2>&1

ssh rvm 'pkill -f "target/release/rustty" || true;
 if [ -f ~/ws17/kwin.pid ]; then kill "$(cat ~/ws17/kwin.pid)" 2>/dev/null || true; fi;
 sleep 2; rm -f ~/ws17/kwin.pid /run/user/1000/wayland-kwinkat*;
 echo "leftover after teardown: $(ls /run/user/1000/wayland-kwinkat* 2>/dev/null || echo none)"'
```

Expected: `leftover after teardown: none` — the teardown must leave the VM exactly as Task 5 Step 1 found it (minus the RPMs).

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v3-kwin-visual-*
git commit -m "ws17: VD:V3 — KWin quick-terminal / blur / SSD visual evidence under nested kwin_wayland"
```

---

## Task 8: V3 fallback — a real Plasma session on a spare VT (only if Tasks 6–7 failed)

**Files:**
- Create (VM, not committed): `~/ws17/plasma-session.sh`
- Modify: `.port/phase-2/evidence/ws17/v3-kwin-visual-notes.txt` (append the fallback outcome)

**Interfaces:**
- Consumes: Task 5's KWin install.
- Produces: either the same three visual outcomes on a full Plasma session, or a recorded reason V3 stays residual.

**Run this task ONLY if** Task 6 Step 4 could not bring up a nested KWin under any flag combination, or Task 7's checks were impossible in the nested window. A full Plasma session is a bigger shared-state change (extra ~700 MB of RPMs and a second live graphical session), so it is the fallback, not the default.

- [ ] **Step 1: Install plasma-workspace, session-isolated and trapped**

```bash
ssh rvm 'cat > ~/ws17/install-plasma.sh <<"EOF"
set -euo pipefail
STAMP=~/ws17/plasma-install.stamp
[ -f "$STAMP" ] && { echo "ABORT: $STAMP exists — previous run unfinished"; exit 2; }
date -u > "$STAMP"
PRE_DM="$(systemctl get-default)"
rollback() { rc=$?; cur="$(systemctl get-default)";
  [ "$cur" != "$PRE_DM" ] && sudo systemctl set-default "$PRE_DM" || true;
  rm -f "$STAMP"; echo "EXIT rc=$rc"; }
trap rollback EXIT INT TERM HUP
sudo dnf install -y --disablerepo=cursor plasma-workspace plasma-desktop
# Explicitly do NOT install or enable sddm; GDM stays the display manager.
rpm -q sddm 2>&1 || true
echo "default target still: $(systemctl get-default)"
command -v startplasma-wayland
echo "PLASMA_INSTALL_OK $(date -u)"
EOF
setsid nohup bash ~/ws17/install-plasma.sh > ~/ws17/install-plasma.log 2>&1 < /dev/null & echo started'
```

Poll `ssh rvm 'tail -20 ~/ws17/install-plasma.log'`. PASS: `PLASMA_INSTALL_OK`, `sddm ... not installed`, default target unchanged.

- [ ] **Step 2: Start Plasma on its own VT (isolated from the GNOME session)**

```bash
ssh rvm 'setsid nohup bash -c "export XDG_RUNTIME_DIR=/run/user/1000; \
  dbus-run-session -- startplasma-wayland" > ~/ws17/plasma.log 2>&1 < /dev/null & echo started'
sleep 15
ssh rvm 'ls /run/user/1000/wayland-*; tail -20 ~/ws17/plasma.log'
```

PASS: a new `wayland-N` socket (N ≠ 0) appears and the log shows KWin starting.
FAIL: no socket → Plasma cannot start from an SSH context on this VM; record V3 as residual with this log and stop.

- [ ] **Step 3: Re-run Task 6 Step 5 and Task 7 Steps 2–4 against the new socket**

Substitute `WAYLAND_DISPLAY=wayland-N` (the socket found in Step 2) in every command from Task 6 Step 5 and Task 7 Steps 2–4, and write outputs to the same evidence filenames.

- [ ] **Step 4: Tear down and record**

```bash
ssh rvm 'pkill -f startplasma-wayland || true; sleep 3; ls /run/user/1000/wayland-*;
 systemctl get-default; systemctl is-active gdm'
cd /Users/gael/projects/rustty
{ echo "== fallback (Task 8) outcome $(date -u) =="; ssh rvm 'tail -30 ~/ws17/plasma.log'; } \
  >> .port/phase-2/evidence/ws17/v3-kwin-visual-notes.txt
git add .port/phase-2/evidence/ws17/v3-kwin-visual-notes.txt
git commit -m "ws17: VD:V3 fallback — full Plasma session attempt on rvm, outcome recorded"
```

**Recovery if the VM's graphical stack breaks during this task:** `ssh rvm 'systemctl get-default'` must still say `graphical.target` and `systemctl is-active gdm` must say `active`. If GDM died: `ssh rvm 'sudo systemctl restart gdm'`. If SSH itself is gone, follow the recovery procedure in Task 9 Step 6.

---

## Task 9: V4 — `set_urgent` KAT on a real mutter, via a HEADLESS mutter instance

**Files:**
- Create: `.port/phase-2/evidence/ws17/v4-mutter-headless-kat.txt`, `v4-mutter-control-nocompositor.txt`

**Interfaces:**
- Consumes: the existing ignored test `wayland_set_urgent_dispatches_activation_token` in `crates/rustty-apprt/tests/gtk_wayland_winproto.rs`.
- Produces: runtime evidence that mutter's `xdg_activation_v1` completes the token round-trip.

**CRASH RISK — read before running.** Two hard machine crashes on this VM coincided with live-compositor KAT activity on the **primary GNOME session** (abrupt journal end, no guest OOM/panic — hypervisor-level death, root cause never attributed). This task therefore does **not** touch the primary session. It starts a **separate, headless mutter** (`--headless --virtual-monitor`) on its own Wayland socket, which is real mutter code driving the real `xdg_activation_v1` implementation without any interaction with the running gnome-shell, GDM, or the virtio-gpu scanout path that the crashes correlated with. If the VM dies anyway, follow Step 6.

- [ ] **Step 1: Leftover-detection pre-flight**

```bash
ssh rvm 'echo "sockets: $(ls /run/user/1000/wayland-* 2>/dev/null)";
 echo "stale kat socket: $(ls /run/user/1000/wayland-mutterkat* 2>/dev/null || echo none)";
 echo "stale pid: $(ls ~/ws17/mutter.pid 2>/dev/null || echo none)";
 echo "mutter procs: $(pgrep -a mutter | grep -v gnome-shell || echo none)";
 echo "uptime: $(uptime)"'
```

PASS to continue: `stale kat socket: none`, `stale pid: none`, no stray `mutter` process. Otherwise clean (`ssh rvm 'rm -f ~/ws17/mutter.pid /run/user/1000/wayland-mutterkat*'`) and re-run — a crashed previous run must never be read as a clean start.

- [ ] **Step 2: Run the NEGATIVE CONTROL first — the KAT with no compositor must not pass**

```bash
ssh rvm 'cd ~/projects/rustty; env -u WAYLAND_DISPLAY XDG_RUNTIME_DIR=/run/user/1000 GDK_BACKEND=wayland \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto \
  -- --ignored --exact wayland_set_urgent_dispatches_activation_token --nocapture 2>&1 | tail -15' \
 | tee /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v4-mutter-control-nocompositor.txt
```

Expected: a failure/abort at `gtk4::init()` or "a default GDK display" — i.e. the test cannot silently "pass" without a compositor. If it PASSES here, the KAT is vacuous and no compositor result from it means anything; stop and fix the KAT first.

- [ ] **Step 3: Start headless mutter (SIP — own session, trapped)**

```bash
ssh rvm 'cat > ~/ws17/mutter-headless.sh <<"EOF"
set -uo pipefail
export XDG_RUNTIME_DIR=/run/user/1000
SOCK=wayland-mutterkat
PID=~/ws17/mutter.pid
if [ -e "$XDG_RUNTIME_DIR/$SOCK" ] || [ -f "$PID" ]; then
  echo "ABORT: leftover headless-mutter state — clean it first"; exit 2
fi
cleanup() {
  rc=$?
  if [ -f "$PID" ]; then kill "$(cat "$PID")" 2>/dev/null || true; fi
  rm -f "$PID" "$XDG_RUNTIME_DIR/$SOCK" "$XDG_RUNTIME_DIR/$SOCK.lock"
  echo "CLEANUP rc=$rc"
}
trap cleanup EXIT INT TERM HUP
# Headless: no DRM, no scanout, no contact with the primary GNOME session.
dbus-run-session -- mutter --headless --virtual-monitor 1920x1080 --wayland \
  --no-x11 --wayland-display "$SOCK" &
echo $! > "$PID"
sleep 5
[ -e "$XDG_RUNTIME_DIR/$SOCK" ] || { echo "FAIL: mutter socket never appeared"; exit 1; }
echo "MUTTER_UP pid=$(cat $PID) socket=$SOCK"
wait "$(cat "$PID")"
EOF
setsid nohup bash ~/ws17/mutter-headless.sh > ~/ws17/mutter-headless.log 2>&1 < /dev/null & echo started'
sleep 8
ssh rvm 'tail -20 ~/ws17/mutter-headless.log; ls /run/user/1000/wayland-mutterkat*'
```

PASS: `MUTTER_UP pid=<n> socket=wayland-mutterkat` and the socket exists.
FAIL: `FAIL: mutter socket never appeared` → check the log for a missing `--wayland-display` flag (mutter 50 lists it; confirm with `ssh rvm 'mutter --help | grep wayland-display'`) and retry once. If it still will not start headless, do NOT fall back to the primary session — record V4 as residual per Task 12's wording.

- [ ] **Step 4: Run the KAT against headless mutter**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-mutterkat GDK_BACKEND=wayland;
 cd ~/projects/rustty && timeout 120 cargo test -p rustty-apprt --features gtk,wayland \
   --test gtk_wayland_winproto -- --ignored --exact wayland_set_urgent_dispatches_activation_token \
   --nocapture 2>&1 | tail -25' \
 | tee /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v4-mutter-headless-kat.txt
```

PASS: `test result: ok. 1 passed`, in ~1 s (the KAT is internally bounded at ~10 s; a broken seam fails, never hangs).
FAIL: `activation_done_count` stays 0 → the GSource is not cranking under mutter; that is a genuine bug, investigate with systematic-debugging.
Note the run also proves protocol selection (`Protocol::Wayland`) on mutter.

- [ ] **Step 5: Also run the full ignored winproto suite under headless mutter, then tear down**

```bash
ssh rvm 'export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-mutterkat GDK_BACKEND=wayland;
 cd ~/projects/rustty && timeout 300 cargo test -p rustty-apprt --features gtk,wayland \
   --test gtk_wayland_winproto -- --ignored --nocapture 2>&1 | tail -30' \
 >> /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v4-mutter-headless-kat.txt

ssh rvm 'if [ -f ~/ws17/mutter.pid ]; then kill "$(cat ~/ws17/mutter.pid)" 2>/dev/null || true; fi;
 sleep 2; rm -f ~/ws17/mutter.pid /run/user/1000/wayland-mutterkat*;
 echo "leftover: $(ls /run/user/1000/wayland-mutterkat* 2>/dev/null || echo none)"; uptime'
```

Expected: `leftover: none`, and `uptime` showing the VM did **not** reboot during the task (compare with Step 1's uptime).

- [ ] **Step 6: RECOVERY PROCEDURE (only if the VM/session dies during Steps 3–5)**

Symptom: `ssh rvm` hangs or refuses; or `uptime` shows a fresh boot.

```bash
# 1. Is the VM process still alive on the host?
ps aux | grep -i "[U]TM" | head -3
# 2. Restart the VM through UTM (AppleScript; no GUI clicking required):
osascript -e 'tell application "UTM" to start virtual machine named "Fedora Linux"'
# 3. Wait for SSH to come back (poll; do not block a tool call for minutes):
ssh -o ConnectTimeout=5 rvm 'uptime' || echo "not up yet — re-run"
# 4. Post-mortem: did the guest log anything, or did it die abruptly (hypervisor-style)?
ssh rvm 'journalctl -b -1 -e --no-pager | tail -60' \
  > /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v4-crash-postmortem.txt
# 5. Clean leftovers before any retry:
ssh rvm 'rm -f ~/ws17/*.pid ~/ws17/*.stamp /run/user/1000/wayland-mutterkat* /run/user/1000/wayland-kwinkat*'
```

If a crash occurs: **do not retry the same step more than once.** Record V4 as residual with the post-mortem attached, exactly as Task 12 specifies. Two prior crashes already correlate with this activity class; a third is a reason to stop, not to iterate.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v4-*
git commit -m "ws17: VD:V4 — set_urgent KAT on real mutter 50 (headless instance, primary session untouched)"
```

---

## Task 10: V4 — the primary-GNOME-session run stays deliberately unrun

**Files:**
- Create: `.port/phase-2/evidence/ws17/v4-primary-session-decision.md`

**Interfaces:**
- Consumes: Task 9's result.
- Produces: the written decision Task 12 cites.

**Decision:** after Task 9 discharges V4 on a real mutter instance, the marginal information from repeating it on the *primary* GNOME session is near-zero (same mutter binary, same `xdg_activation_v1` implementation), while the risk is a third unattributed hard crash of the development VM. WS17 does not run it. This is recorded as an explicit, reasoned residual — not an oversight, and not silently folded into the discharge.

- [ ] **Step 1: Write the decision note**

Create `.port/phase-2/evidence/ws17/v4-primary-session-decision.md`:

```markdown
# VD:V4 — primary-session run: deliberately NOT executed (2026-07-29)

WS17 ran `wayland_set_urgent_dispatches_activation_token` against a real mutter 50
started as `mutter --headless --virtual-monitor 1920x1080 --wayland --no-x11
--wayland-display wayland-mutterkat` (evidence: `v4-mutter-headless-kat.txt`),
plus the same KAT under KWin 6.7.3 (`v3-kwin-globals-kat.txt`).

The remaining untested configuration is the *primary* GNOME session on tty2 —
the one whose live-compositor KAT activity coincided with two hard, unattributed
VM deaths (abrupt journal end, no guest OOM or panic). Difference in what it would
prove: none in protocol terms — same mutter build, same `xdg_activation_v1`
implementation, same GTK client path; the only delta is the DRM/scanout path,
which `set_urgent` does not touch. Risk: a third crash of the sole development VM.

Decision: not run. This is a permanent, reasoned residual, re-openable only if a
future failure implicates the scanout path.
```

- [ ] **Step 2: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v4-primary-session-decision.md
git commit -m "ws17: VD:V4 — record the deliberate non-execution of the primary-session run"
```

---

## Task 11: V1b — prove the macOS cross-DPI runtime re-stamp path fires

**Files:**
- Create: `/tmp/ws17/restamp.lldb`, `/tmp/ws17/restamp-run.sh` (scratch, not committed)
- Create: `.port/phase-2/evidence/ws17/v1b-restamp-lldb.txt`, `.port/phase-2/evidence/ws17/v1b-display-modes.txt`

**Interfaces:**
- Consumes: the Swift host path `SurfaceView_AppKit.swift:837-871` (`viewDidChangeBackingProperties` → `rustty_surface_set_content_scale` → `Surface::updateContentScale`).
- Produces: an lldb transcript showing `rustty_surface_set_content_scale` called with `d0/d1 = 2.0` at startup and again with `d0/d1 = 1.0` after a live display-density change — the exact evidence WS6's residual says was never obtained.

**Why lldb and not a log line:** the oracle's `SurfaceView` has no logging here, so adding one permanently would be an R1 deviation. A breakpoint on the C-ABI symbol `rustty_surface_set_content_scale` reads the two `double` arguments straight out of `d0`/`d1` (AAPCS64) with zero source change.

**Host-state mutation:** changing the built-in display's resolution. SIP + a written restore step + a recorded pre-state are mandatory.

- [ ] **Step 1: Record the pre-state display mode (leftover detection + restore reference)**

```bash
mkdir -p /tmp/ws17
{ echo "== date =="; date -u
  echo "== pre-state display mode =="
  system_profiler SPDisplaysDataType | sed -n '1,40p'
} | tee /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v1b-display-modes.txt
```

Note the `Resolution:` and `UI Looks like:` lines — this is what Step 8 must restore. If `UI Looks like` is already a non-HiDPI mode, a previous run left the display changed: restore it first, then restart this task.

- [ ] **Step 2: Build the app**

```bash
cd /Users/gael/projects/rustty
setsid nohup caffeinate -di bash macos/build-rustty.sh > /tmp/ws17/build.log 2>&1 < /dev/null &
sleep 5; tail -5 /tmp/ws17/build.log
```

Poll `tail -5 /tmp/ws17/build.log` until `** BUILD SUCCEEDED **`.

```bash
APP=$(ls -td ~/Library/Developer/Xcode/DerivedData/Rustty-*/Build/Products/Debug/Rustty.app | head -1); echo "$APP"
```

Expected: a path ending in `Debug/Rustty.app`. FAIL: no such path → the build did not produce a bundle; fix before continuing.

- [ ] **Step 3: Write the lldb script**

Create `/tmp/ws17/restamp.lldb` (substitute the real `$APP` path on the `target create` line):

```
settings set auto-confirm true
target create "/Users/gael/Library/Developer/Xcode/DerivedData/Rustty-REPLACE/Build/Products/Debug/Rustty.app/Contents/MacOS/Rustty"
breakpoint set --name rustty_surface_set_content_scale --auto-continue true \
  --command "script print('WS17-RESTAMP-HIT')" \
  --command "register read d0 d1" \
  --command "thread backtrace 4"
run
```

- [ ] **Step 4: Launch lldb detached with stdin held open (SIP)**

```bash
mkdir -p /tmp/ws17 && rm -f /tmp/ws17/lldb.in && mkfifo /tmp/ws17/lldb.in
setsid nohup caffeinate -di bash -c 'exec lldb --source /tmp/ws17/restamp.lldb < /tmp/ws17/lldb.in' \
  > /tmp/ws17/restamp.log 2>&1 &
setsid nohup bash -c 'sleep 100000 > /tmp/ws17/lldb.in' > /dev/null 2>&1 &
sleep 12
grep -c "WS17-RESTAMP-HIT" /tmp/ws17/restamp.log; tail -30 /tmp/ws17/restamp.log
```

**NON-VACUITY CONTROL (this step is the whole point):** the app's first frame must already produce at least one hit with `d0 = 2` (the Retina backing scale). If `WS17-RESTAMP-HIT` count is 0 here, the instrumentation is not live and nothing later in this task means anything — fix the breakpoint (check the symbol is exported: `nm -gU "$APP/Contents/MacOS/Rustty" | grep rustty_surface_set_content_scale`) before touching System Settings.

- [ ] **Step 5: Second control — prove an ordinary window move does NOT produce a hit**

Move/resize the Rustty window with the mouse for ~5 seconds (resize DOES legitimately hit `set_size`, not `set_content_scale`; a plain *move* on one display must produce no new scale hit), then:

```bash
grep -c "WS17-RESTAMP-HIT" /tmp/ws17/restamp.log
```

Expected: the same count as Step 4. If moving the window alone bumps the counter, the signal is noisy and Step 7's evidence would be ambiguous — note that in the evidence file and rely on the `d0` value (2.0 → 1.0) rather than the count.

- [ ] **Step 6: Trigger the live density change (manual, macOS System Settings)**

Exact steps on the M4 Pro's single built-in Retina display:

1. Keep the Rustty window visible on screen (do not quit it — the re-stamp path only fires on a live window).
2. Open **System Settings → Displays**.
3. Click **Scaled**, then **hold Option and click** the resolution list — this reveals the full mode list including the non-HiDPI (1x) modes, which are labelled `(low resolution)`.
4. Select a **`(low resolution)`** mode (e.g. `1512 × 982 (low resolution)`). Confirm the change when prompted.
5. Wait ~5 seconds for the window server to settle.

What this does: `backingScaleFactor` goes 2.0 → 1.0 on the live window, which is precisely the `viewDidChangeBackingProperties` → `setContentScale` → `updateContentScale` path WS6 left unexercised.

- [ ] **Step 7: Capture the hit**

```bash
grep -A4 "WS17-RESTAMP-HIT" /tmp/ws17/restamp.log | tail -40
cp /tmp/ws17/restamp.log /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v1b-restamp-lldb.txt
```

PASS (all three required):
1. a new `WS17-RESTAMP-HIT` after the resolution change,
2. its `register read d0 d1` shows `d0 = 1` and `d1 = 1` (was `2`/`2` at startup),
3. its `thread backtrace 4` contains `viewDidChangeBackingProperties`.

FAIL: no new hit → the density-change path genuinely does not reach the C ABI; that is a real bug, not a verification gap — investigate before writing anything in the ledger.

- [ ] **Step 8: RESTORE the display mode and tear down (mandatory)**

1. System Settings → Displays → select the original mode recorded in Step 1 (the default HiDPI one). Confirm.
2. Verify:

```bash
system_profiler SPDisplaysDataType | sed -n '1,40p' \
  >> /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v1b-display-modes.txt
diff <(sed -n '/pre-state/,/^$/p' /Users/gael/projects/rustty/.port/phase-2/evidence/ws17/v1b-display-modes.txt | grep Resolution) \
     <(system_profiler SPDisplaysDataType | grep Resolution) && echo "DISPLAY RESTORED"
```

3. Kill the instrumented app and the fifo holder:

```bash
pkill -f 'lldb --source /tmp/ws17/restamp.lldb' || true
pkill -f 'sleep 100000' || true
rm -f /tmp/ws17/lldb.in
pgrep -fl 'Rustty.app/Contents/MacOS/Rustty' || echo "app stopped"
```

**Recovery if the screen becomes unusable after the resolution change:** the 1x modes are *larger*, never smaller, so the UI stays readable; if the display goes black, wait 15 s (macOS reverts an unconfirmed mode automatically), else press Escape, else `sudo pkill -HUP WindowServer` from an SSH session into the Mac. Do not leave the display in the 1x mode — Step 8.1 is not optional.

- [ ] **Step 9: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/evidence/ws17/v1b-restamp-lldb.txt .port/phase-2/evidence/ws17/v1b-display-modes.txt
git commit -m "ws17: V1b residual — lldb evidence that the cross-DPI runtime re-stamp fires (2.0 -> 1.0)"
```

---

## Task 12: Rewrite `VERIFICATION_DEBT.md` truthfully

**Files:**
- Modify: `.port/phase-2/VERIFICATION_DEBT.md`
- Modify: `docs/manual-test-plan-gtk.md` (§W rows)

**Interfaces:**
- Consumes: every file in `.port/phase-2/evidence/ws17/`.
- Produces: the final ledger state. Each item is written from the evidence file, not from this plan's expectations. **If an outcome differed from what this plan predicted, the ledger records the actual outcome.**

- [ ] **Step 1: Rewrite the file header and V1b**

Replace the header status line with:

```markdown
Status snapshot 2026-07-29 @ WS17 (`ws17-verification-debt`). This file tracks
**verification debt only**: work that is implemented and believed correct, whose
*confirmation* needs hardware/environments this development environment (macOS M4 Pro
single Retina display + Fedora 44 aarch64 UTM/Apple-Virtualization VM, llvmpipe-only,
GNOME/mutter + sway + KWin) does not have. Implementation deferrals live in
`DEFERRALS.md`. Raw evidence for every 2026-07-29 claim below is committed under
`.port/phase-2/evidence/ws17/`.
```

Then replace V1b's "Residual (minor)" paragraph with the outcome from `v1b-restamp-lldb.txt`. If Task 11 passed:

```markdown
**Residual — CLOSED [WS17, 2026-07-29].** The runtime re-stamp path
(`viewDidChangeBackingProperties` → `setContentScale` → `updateContentScale`) is now
runtime-verified on the single built-in Retina display using the System Settings
HiDPI ↔ 1x resolution-toggle fallback that WS6's T5 Step 6 offered but never
attempted. Method: lldb breakpoint on the C-ABI symbol
`rustty_surface_set_content_scale` (`--auto-continue true`, dumping `d0`/`d1` and a
4-frame backtrace) — no source change, so no R1 deviation. Evidence
(`evidence/ws17/v1b-restamp-lldb.txt`): a startup hit at `d0=2, d1=2`, then after
switching the built-in display to a `(low resolution)` mode a further hit at
`d0=1, d1=1` whose backtrace goes through `viewDidChangeBackingProperties`. The
display was restored to its original mode (`evidence/ws17/v1b-display-modes.txt`).
A genuine two-monitor drag remains untried (no second display exists on this host)
and is not required: the density-change code path is the same one either event
drives.
```

If Task 11 did NOT produce a post-change hit, write instead:

```markdown
**Residual — OPEN.** The 1x/HiDPI resolution-toggle fallback WAS attempted on
2026-07-29 (`evidence/ws17/v1b-restamp-lldb.txt`). The lldb breakpoint on
`rustty_surface_set_content_scale` was proven live (startup hit at `d0=2,d1=2`) but
NO hit followed the density change. That is a suspected real defect in the
density-change path, not an environment gap — see the WS17 follow-up.
```

- [ ] **Step 2: Rewrite V2**

Replace the whole `## V2.` section with:

```markdown
## V2. Bare-metal GPU rendering/perf sign-off — OPEN (honest residual; this
environment cannot host a hardware GL driver)

**Status 2026-07-29 (WS17).** NOT discharged, and provably not dischargeable here.
Evidence: `evidence/ws17/v2-vm-gl-ceiling.txt`,
`evidence/ws17/v2-hw-gl-probe-controls.txt`, `evidence/ws17/v2-virgl-feasibility.md`.

1. The VM runs under UTM's **Apple Virtualization backend** (`config.plist`
   `Backend = Apple`; `systemd-detect-virt` → `apple`), which gives Linux guests a
   virtio-gpu with 3D disabled: `[drm] features: -virgl -edid -resource_blob
   -host_visible`. Measured driver: `llvmpipe (LLVM 22.1.8, 128 bits)`,
   `Accelerated: no`, Mesa 26.1.4.
2. Enabling virgl would require re-creating the VM under UTM's QEMU backend. That is
   rejected on two independent grounds: (a) the macOS data volume has ~5.5 GiB free
   against a 37 GiB disk image, so no non-destructive import is possible; and (b) it
   would not help — when this same host ran virgl (2026-07-17, recorded in
   `crates/rustty-renderer/src/opengl/gl/sys.rs:611-628`) it reported
   `GL_RENDERER = virgl (ANGLE (Apple, Apple M4 Pro, OpenGL 4.1 Metal - 90.5))`,
   `Max core profile version: 0.0`, and every core-profile request from 3.2 to 4.6
   failed `EGL_BAD_MATCH`. rustty's GL renderer floor is **4.3 core**, so virgl
   on Apple Silicon cannot run the renderer at all.
3. NARROWED, not closed: a same-box comparison of rustty vs distro
   `ghostty 1.3.1-2.fc44` sustained-throughput on llvmpipe is recorded in
   `evidence/ws17/v2-llvmpipe-vs-ghostty.txt`. It says nothing about hardware GL.

**Turnkey re-check (run on any Linux box with an amdgpu/intel/nvidia GPU):**
```
.port/tools/gates/hw_gl_probe.sh        # MUST print PASS (exit 0) FIRST, in the
                                        # same session/DISPLAY as the app run
cargo build --release --features gtk
RUSTTY_RESOURCES_DIR=$PWD/crates/rustty-extra target/release/rustty
```
The probe is the anti-vacuity guard: it fails on llvmpipe/softpipe/swrast, on
`Accelerated: no`, and on any driver whose max core profile is below 4.3. Its own
PASS arm is proven reachable by
`RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt`.
Then the four original sub-checks: (1) glyph crispness at multiple sizes + HiDPI vs
ghostty on the same box; (2) `cat` a large file while resizing — no tearing/stutter
beyond ghostty's; (3) inspector (`Ctrl+Shift+I`, `--features gtk,imgui`) frame-time
plot under load; (4) `GDK_DEBUG=opengl` startup lines naming the hardware driver.
```

- [ ] **Step 3: Rewrite V3 from `v3-kwin-globals-kat.txt` and `v3-kwin-visual-notes.txt`**

If Tasks 6–7 passed, replace the `## V3.` section with (filling the bracketed values from the actual dump — do not invent them):

```markdown
## V3. KDE Plasma (KWin) quick-terminal + decorations — DISCHARGED IN PART
[WS17, 2026-07-29]

KWin 6.7.3 was installed on the VM (`kwin` RPM only — GDM and the default target
were left untouched; `evidence/ws17/v3-kwin-install.txt`) and run **nested** on its
own socket (`wayland-kwinkat`) so the primary GNOME session was never involved.

**Globals binding — CONFIRMED.** New KAT `kde_globals_bind_under_kwin`
(`crates/rustty-apprt/tests/gtk_wayland_winproto.rs`) asserts on the winproto
`Globals` Debug dump under a live KWin:
`has_kde_decoration_manager: true`, `has_kde_slide_manager: true`
(`evidence/ws17/v3-kwin-globals-kat.txt`). NON-VACUOUS: the identical KAT run on
the GNOME/mutter session FAILS with `has_kde_decoration_manager: false`
(`evidence/ws17/v3-kwin-control-gnome.txt`).

**LEDGER CORRECTION.** The previous wording named `org_kde_kwin_blur`. Neither the
oracle (`ghostty/src/apprt/gtk/winproto/wayland.zig:156`) nor rustty binds that
protocol — blur rides **`ext-background-effect-v1`** (`Tag::ExtBackgroundEffect`).
Recorded value under KWin 6.7.3: `has_ext_background_effect: [VALUE FROM DUMP]`.

**Visual checks** (`evidence/ws17/v3-kwin-visual-*.png`, `-notes.txt`):
1. Quick terminal drops down full-width, anchored top, under KWin — CONFIRMED.
   The slide *animation* is compositor-side and a still capture cannot show it; the
   assertion V3 actually makes is the presence of `org_kde_kwin_slide_manager`,
   which the KAT proves.
2. `background-opacity < 1.0` + `background-blur = true` — [CONFIRMED blurred /
   translucency only because KWin 6.7.3 does not advertise
   `ext-background-effect-v1`; pick from the actual result].
3. `window-decoration = server` → KWin (Breeze) titlebar; `= client` → titlebar
   gone — CONFIRMED both directions.

**Remaining:** a full Plasma *shell* session (panels, `kde_output_order_v1`
populated by plasmashell) was not exercised — bare nested KWin advertises
`has_kde_output_order: [VALUE FROM DUMP]`. `docs/manual-test-plan-gtk.md` §W rows
updated accordingly.
```

If Tasks 6–8 failed to bring up any KWin, write instead:

```markdown
## V3. KDE Plasma (KWin) — OPEN (residual)

WS17 installed KWin 6.7.3 on the VM but could not bring up a usable compositor
(nested and, in the fallback, a full Plasma session both failed —
`evidence/ws17/v3-kwin-install.txt`, `evidence/ws17/v3-kwin-visual-notes.txt`).
The `kde_globals_bind_under_kwin` KAT is committed and ready
(`crates/rustty-apprt/tests/gtk_wayland_winproto.rs`); it needs one command on any
KDE Plasma Wayland session:
`cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --ignored --exact kde_globals_bind_under_kwin --nocapture`
plus the three visual checks (quick-terminal drop-down, blur via
`ext-background-effect-v1`, `window-decoration = server`/`client`).
```

- [ ] **Step 4: Rewrite V4**

```markdown
## V4. `set_urgent` KAT on real mutter — DISCHARGED [WS17, 2026-07-29]
(primary-session run deliberately still not run)

`wayland_set_urgent_dispatches_activation_token` passes against a real mutter 50
started headless and fully isolated from the primary GNOME session:
`dbus-run-session -- mutter --headless --virtual-monitor 1920x1080 --wayland
--no-x11 --wayland-display wayland-mutterkat`, then
`WAYLAND_DISPLAY=wayland-mutterkat cargo test -p rustty-apprt --features gtk,wayland
--test gtk_wayland_winproto -- --ignored --exact
wayland_set_urgent_dispatches_activation_token`.
Evidence: `evidence/ws17/v4-mutter-headless-kat.txt` (real token request → `done` →
`activate`). NON-VACUOUS: the same KAT with no compositor available fails at display
acquisition rather than passing (`evidence/ws17/v4-mutter-control-nocompositor.txt`).
The KAT also passed under KWin 6.7.3 (`evidence/ws17/v3-kwin-globals-kat.txt`).

**Residual (deliberate, reasoned):** the run on the *primary* GNOME session on tty2
was NOT repeated. Two prior hard VM deaths correlated with live-compositor KAT
activity on that session, and the delta versus the headless run is only the
DRM/scanout path, which `set_urgent` does not touch. See
`evidence/ws17/v4-primary-session-decision.md`.
```

- [ ] **Step 5: Update `docs/manual-test-plan-gtk.md` §W**

Change row **W.1** to record the outcome (example wording for the passing case; use the real result):

```markdown
- [x] **W.1 (KDE/wlr only):** VERIFIED 2026-07-29 (WS17) on nested KWin 6.7.3:
      `window-decoration = server` → KWin Breeze titlebar, `= client` → none;
      `background-opacity`/`background-blur` result and screenshots in
      `.port/phase-2/evidence/ws17/`. Blur rides `ext-background-effect-v1`
      (NOT `org_kde_kwin_blur`). On GNOME/mutter these stay inert by design.
```

And append to the quick-terminal row: `Also verified on KWin 6.7.3 (nested) 2026-07-29 — WS17.`

- [ ] **Step 6: Self-check the ledger against the evidence directory**

```bash
cd /Users/gael/projects/rustty
ls .port/phase-2/evidence/ws17/
grep -n "evidence/ws17/" .port/phase-2/VERIFICATION_DEBT.md
```

Every `evidence/ws17/...` path named in the ledger must exist; every file in the directory should be referenced by some ledger line or the evidence README. PASS: both directions hold. FAIL: a ledger line cites a file that was never produced → that claim is unbacked; delete or downgrade it.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/VERIFICATION_DEBT.md docs/manual-test-plan-gtk.md
git commit -m "ws17: rewrite VERIFICATION_DEBT — V3/V4/V1b discharged with evidence, V2 recorded as an honest residual"
```

---

## Task 13: Both-platform build gate, roadmap update, merge

**Files:**
- Modify: `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`

**Interfaces:**
- Consumes: everything above.
- Produces: `main` containing WS17.

- [ ] **Step 1: macOS build + the touched crate's tests**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -5
cargo test -p rustty-apprt 2>&1 | tail -5
```

Expected: `Finished` and no failures. (The new KAT is `#[ignore]`d and Linux-gated, so it must not run here.)

- [ ] **Step 2: Linux build + the winproto test target compiles**

```bash
ssh rvm 'cd ~/projects/rustty && git fetch -q && git checkout ws17-verification-debt && git pull --ff-only -q &&
 cargo build --workspace 2>&1 | tail -3 &&
 cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto 2>&1 | tail -5'
```

Expected: workspace `Finished`; the winproto target reports `0 passed; 0 failed; N ignored` (all its tests are ignored by default — that is the correct default-run behaviour).

- [ ] **Step 3: Shell-check the new gate script**

```bash
cd /Users/gael/projects/rustty
bash -n .port/tools/gates/hw_gl_probe.sh && echo "syntax ok"
RUSTTY_HW_GL_PROBE_FIXTURE=.port/tools/gates/fixtures/glxinfo-hw-radeonsi.txt .port/tools/gates/hw_gl_probe.sh >/dev/null && echo "pass arm still reachable"
```

- [ ] **Step 4: Update the roadmap**

In `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`:

- Change the WS17 row's scope cell to: `VD:V3 KDE/KWin (nested kwin_wayland) + VD:V4 mutter set_urgent (headless mutter) DISCHARGED 2026-07-29; VD:V1b macOS cross-DPI re-stamp CLOSED; VD:V2 real-GPU sign-off remains an honest residual — this host is UTM/Apple-VZ (no virgl) and virgl-on-Apple-Silicon exposes no 4.3 core profile. Turnkey re-check: .port/tools/gates/hw_gl_probe.sh`.
- In the plan-authoring checklist, replace the `WS10–WS17` line with two lines: `- [x] WS17 verification debt — AUTHORED + EXECUTED + MERGED (2026-07-29)` and `- [ ] WS10–WS16: authored at wave start`.

- [ ] **Step 5: Commit and merge**

```bash
cd /Users/gael/projects/rustty
git add docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "docs: roadmap — WS17 executed (V3/V4/V1b discharged, V2 honest residual)"
git checkout main && git pull --ff-only
git merge --no-ff ws17-verification-debt -m "merge: WS17 verification-debt discharge — V3 KWin, V4 mutter, V1b cross-DPI; V2 residual with turnkey probe"
cargo build --workspace 2>&1 | tail -3
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull --ff-only -q && cargo build --workspace 2>&1 | tail -3'
git push
git branch -d ws17-verification-debt
git push origin --delete ws17-verification-debt
ssh rvm 'cd ~/projects/rustty && git branch -D ws17-verification-debt 2>/dev/null; git log --oneline -1'
```

Expected: both `Finished`; branch deleted. FAIL on either platform → `git reset --hard HEAD~1` on main and fix on the branch before re-merging.

---

## Appendix A — Steps deliberately NOT taken (and why)

Recorded so a later reader does not mistake them for oversights:

1. **Converting the VM to UTM's QEMU backend to obtain virgl.** Rejected: 5.5 GiB free host disk vs a 37 GiB image (no non-destructive path), and virgl-on-Apple-Silicon exposes `Max core profile version: 0.0` while rustty's renderer floor is 4.3 core — it would produce a driver the renderer cannot initialize. See `evidence/ws17/v2-virgl-feasibility.md`.
2. **Running the `set_urgent` KAT on the primary GNOME session.** Rejected: two prior unattributed hard VM deaths correlate with that activity; the headless-mutter run covers the same protocol implementation. See `evidence/ws17/v4-primary-session-decision.md`.
3. **Installing SDDM / switching the display manager for V3.** Rejected: swapping the display manager on the sole development VM is a high-blast-radius change; nested `kwin_wayland` provides the same protocol surface.
4. **Running `.port/tools/gates/no_homebrew_gettext_gate.sh`.** Out of scope for WS17 and mutates shared host state (Homebrew `libintl`) — explicitly forbidden by this plan.
5. **Adding a permanent GL-renderer-string log line to the renderer.** Rejected as an R1 deviation: the oracle logs only `loaded OpenGL {major}.{minor}` (`ghostty/src/renderer/OpenGL.zig:138`). The external `hw_gl_probe.sh` gives the same information with no fidelity cost.
