# Handoff — Finish GTK on Linux and validate under real 3D acceleration

**Date:** 2026-07-18
**For:** a fresh session on a NEW VM that HAS working 3D acceleration (hardware GL 4.3+ core profile).
**Author of this handoff:** the session that built L0–L5 on a VM where 3D acceleration was disabled.
**Reference (parity oracle):** `/home/gael/projects/ghostty` @ `4749c4e9`, `src/apprt/gtk/**`.

---

## 0. TL;DR — do these in order

1. **Read §3, set up the new VM** (deps + `TMPDIR`). Confirm `glxinfo | grep "OpenGL core profile version"` shows **≥ 4.3 core on real hardware** (not llvmpipe).
2. **Do §4 first: real-hardware validation.** Run `target/debug/rustty` (NO `LIBGL_ALWAYS_SOFTWARE`). Confirm a window opens **with glyphs actually visible** and the shell is interactive. This is the one thing the previous VM could not verify. Everything else is downstream of this passing.
3. **Then §5: finish the remaining implementation** (T23 dialogs → T24 overlays → interactive surface handlers → T25 CommandPalette → T26 GlobalShortcuts → T27/T28 Wayland → L6 X11+inspector), each via the subagent implement→review→fix loop, orchestrated with **dynamic Workflows** (the user has repeatedly asked for this).
4. **Then §7 hygiene** and push.

---

## 1. Where things stand (all verified by RUNNING, not by agent reports)

**Branch state:** `main` == `linux-port/gtk-apprt` == commit `9bc62e5` (fast-forward merged 2026-07-18).
`main` is **~76 commits ahead of `origin/main`** and **NOT pushed** (this VM had no SSH key — see §7).

The full Linux stack L0–L5 is on `main`. Milestones, each controller-verified by running the binary/tests:

| Tier   | Scope                                                                                       | Status                                         |
| ------ | ------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| **L0** | Compile-gating (apprt-c objc2)                                                              | ✅                                             |
| **L1** | `rustty-xev` epoll + io_uring + hand-authored `Dynamic` selector                            | ✅ live shell on io_uring (58 tests)           |
| **L2** | `rustty-os` Linux                                                                           | ✅                                             |
| **L3** | Fontconfig discovery (`fontconfig-sys` 2.14.2 + `libxml2-sys` 2.11.5, both PINNED not host) | ✅ real font resolution (80 tests)             |
| **L4** | OpenGL renderer (`generic/linux_impl.rs`, copy-adapted from the Metal `macos_impl`)         | ✅ golden test + GL-ATTACH (60 tests)          |
| **L5** | Native GTK4/libadwaita apprt backend                                                        | ✅ **through "a live terminal in the window"** |

**What actually works today (verified on the OLD VM under `LIBGL_ALWAYS_SOFTWARE=1` llvmpipe):**

- `rustty` opens a real 850×650 GTK window (`io.airely.rustty-debug`, R2 identity), stays alive.
- The window **hosts a live terminal**: `TabView → TabPage → Tab → Surface`, `Surface::core()` becomes `Some`, a frame draws into GDK's `GLArea` framebuffer (GL-ATTACH), and the shell round-trips (`echo rustty-window-live` reaches the core `Screen`). Proven by the headed KATs `gtk_first_light.rs` and `gtk_window_live.rs` (both PASS).
- Teardown is memory-sound: self-referential `Surface` is heap-boxed-never-moved with an `AtomicPtr<CoreSurface>` (faithful to the Zig `?*CoreSurface`); render thread is joined before the core is cleared; the `Subprocess` argv double-free was fixed on both halves.

**What is NOT yet verified (the whole reason for this handoff):** that glyphs are **visibly rendered on real GPU hardware**. The old VM's virgl has no desktop-GL core profile, so the only GL path there was llvmpipe (software). first_light proves the _pipeline_ runs and the FBO readback is non-zero; it does **not** prove a human sees a correct terminal on a hardware GLArea. **That is §4.**

---

## 2. The 3D-acceleration constraint (context — should NOT apply on the new VM)

On the old VM:

- virgl (the QEMU/virtio-gpu Mesa driver) exposes **no desktop GL core profile** — only GLES + a GL compat profile too old for the renderer's GL 4.3 core requirement.
- The only way to get GL 4.3+ core was `LIBGL_ALWAYS_SOFTWARE=1` → llvmpipe → GL 4.5 core (CPU rasterization, slow, but correct API surface).
- Even **upstream ghostty would not run** there: it errored with <https://ghostty.org/docs/help/gtk-opengl-context>. So the old VM could never be the place final GTK rendering was signed off.

**On the new VM:** you should have a real GPU with a hardware GL ≥ 4.3 core profile. Verify with `glxinfo | grep -i "core profile version"` and `eglinfo`. If it still says llvmpipe, 3D accel isn't actually active — stop and fix the VM's GPU passthrough before proceeding, otherwise §4 is meaningless.

---

## 3. New-VM environment setup

**Repo layout:**

- Main checkout: `/home/gael/projects/rustty` (branch `main` @ `9bc62e5`).
- The port branch `linux-port/gtk-apprt` points at the same commit (now vestigial — `main` is the source of truth; you can delete it once pushed). A worktree may still exist at `/home/gael/projects/rustty-gtk`; on a fresh VM it likely won't — just work in the main checkout.
- Reference: clone/copy ghostty to `/home/gael/projects/ghostty` and check out `4749c4e9`. The port cites `src/apprt/gtk/**:line` throughout; you need it present for parity work.

**Critical env:**

- **`export TMPDIR=/home/gael/tmp`** (make the dir). The system `/tmp` is a small tmpfs; builds overflow it and GCC prints `Disk quota exceeded`, which _masquerades as a build break_. This bit the old VM repeatedly. Set `TMPDIR` before any `cargo build`.
- Do NOT set a fresh `CARGO_TARGET_DIR` (breaks incremental caching the workflows rely on).

**System deps (Fedora-family; adapt for the new VM's distro):**

- Toolchain: `clang-devel` (bindgen for the `-sys` crates), a C/C++ compiler, `pkg-config`.
- GTK stack: `gtk4-devel`, `libadwaita-devel` (need adw ≥ 1.5 / gtk ≥ 4.14 — the port pins that baseline and deleted all older version-conditional branches, AD-3), `gtk4-layer-shell-devel` (for the Wayland quick-terminal, T28).
- Blueprint: `blueprint-compiler` (the `.blp` → `.ui` asset pipeline in `build.rs`).
- Fonts/GL: `freetype-devel`, `mesa-libGL-devel`, `mesa-libEGL-devel`, Wayland + X11 client libs.
- fontconfig/libxml2 are **vendored and pinned** inside `crates-sys/` — do NOT rely on host versions (host 2.12/2.17 diverges; the port needs fontconfig 2.14.2 + libxml2 2.11.5). The two-error-atom gotcha (`FontConfigFailed` vs `FontconfigFailed` kept deliberately distinct) is load-bearing — don't "fix" it.

**Build & smoke-test:**

```bash
export TMPDIR=/home/gael/tmp && mkdir -p "$TMPDIR"
cd /home/gael/projects/rustty
cargo build --features gtk                    # workspace must be green
cargo build                                   # MUST also build WITHOUT gtk (R5 macOS/embedded path)
cargo build -p rustty-apprt --features gtk    # the GTK backend specifically
```

**Run (the payoff):**

```bash
# On real hardware, NO software fallback:
target/debug/rustty
# If it fails with the gtk-opengl-context error, 3D accel is not active — see §2.
```

**GTK display tests** need exactly one main thread — run them individually, never batched:

```bash
cargo test -p rustty-apprt --features gtk -- --ignored --exact gtk_first_light::<name>
# batching --include-ignored ABORTS (GTK asserts single main thread).
```

---

## 4. FIRST TASK on the new VM — real-hardware validation (do before any new code)

This is the deliverable the old VM could not produce. Structure it as a short **Workflow**: one agent runs each check, one agent adversarially confirms the evidence is real (not a false green).

1. **Hardware GL present:** `glxinfo | grep -i "core profile version"` ≥ 4.3; `eglinfo` shows the real GPU vendor, not `llvmpipe`/`Mesa ... softpipe`.
2. **Window renders glyphs visibly:** launch `target/debug/rustty` with NO `LIBGL_ALWAYS_SOFTWARE`. A window opens showing a **shell prompt rendered as actual glyphs** (not a blank/black GLArea). Type `ls`, see output. Resize the window; the grid reflows. Screenshot it (`grim` on Wayland, `import`/`scrot` on X11) and save under `.superpowers/sdd/` as evidence.
3. **first_light KAT on hardware GL:** run `gtk_first_light` and `gtk_window_live` WITHOUT the software flag. Both should still pass (they passed under llvmpipe; confirm no hardware-context regression).
4. **Compare against upstream ghostty** (now that it runs on this VM): open both side by side; the rustty window should be visually equivalent (font, colors, cursor, ligatures). Note any divergence as a parity bug with a `surface.zig`/`OpenGL.zig` citation.
5. **Interactive-handler reachability sweep (see §5.C):** while the terminal is live, mouse over a URL, open search (`Ctrl+Shift+F` or the configured binding), right-click for the context menu, let a child process exit. Any of these that **panics** points at an `unimplemented!()` surface handler that hardware use reaches — record which, they become priority work.

If step 2 shows a blank GLArea despite hardware GL, the likely suspects are the GL-ATTACH context borrow (`generic/linux_impl.rs::init_windowed`) or the `MUST_DRAW_FROM_APP_THREAD` redraw contract — debug there first (both are cited in the L4 plan doc).

---

## 5. Remaining implementation work (ordered; each via implement→review→fix, orchestrated with Workflows)

The L5 plan (`docs/superpowers/plans/2026-07-17-L5-gtk-apprt.md`) defines Tasks 23–28 in full detail (steps, reference line ranges, commit messages, invariants). Use it as the per-task brief. Below is the map + priorities.

**A. L5 T23 — the dialog family (off the normal render path, but reachable).**
Reference: `class/{dialog,close_confirmation,clipboard_confirmation,config_errors,title,debug_warning}.zig`.
Fills these still-`unimplemented!()` leaves:

- `window.rs:1395` + `window.rs:1420` — `on_close_request` / `tabViewClosePage` → `CloseConfirmationDialog`.
- `split_tree.rs:682` — `surfaceCloseRequest` → `CloseConfirmationDialog`.
- `tab.rs:242` — `promptTabTitle` → `TitleDialog`.
  Collapse version conditionals to the adw ≥ 1.5 baseline (AD-3).

**B. L5 T24 — overlays + media.** `ResizeOverlay`, `SearchOverlay`, `KeyStateOverlay`, `ChildExited`, `media` (AudioBell). These are the template subclasses the `Surface` template `ensure_type`s; today they're inhabited placeholders.

**C. Interactive surface handlers (completion of L5.2 T16/T17) — PRIORITIZE per §4 step 5.**
`surface.rs` still has ~11 `unimplemented!()` template-callbacks that fire on user interaction, NOT on the render path (which is why first_light is green):

- `url_mouse_enter`/`url_mouse_leave` (2506/2510), `notify_mouse_hover_url` (2558), `notify_mouse_hidden` (2562), `notify_mouse_shape` (2564) — mouse/URL hover + cursor shape.
- `context_menu_closed` (2520), `child_exited_close` (2516).
- `search_stop`/`search_changed`/`search_next_match`/`search_previous_match` (2612–2624) — search overlay (couples with T24's `SearchOverlay`).
- `Surface::drop` (2194) — teardown.
  Port from `surface.zig`'s corresponding signal handlers. **§4 step 5 tells you which actually panic under real interaction** — fill those first. (`application.rs:3124–3185` are `#[cfg(test)]` test doubles — NOT production, ignore.)

**D. L5 T25 — `CommandPalette` + `Command`.** Reference `class/command_palette.zig`, `class/command.zig`.

**E. L5 T26 — `GlobalShortcuts` (inhabited placeholder) + boxed-type registrations + the L6 DEFERRALS ledger.** The class/binding/signal are real; the XDG-portal DBus round-trip is a tracked deferral (`startupGlobalShortcuts` binds config + connects the `trigger` signal; `application.zig:1441-1456`).

**F. L5 T27/T28 — Wayland winproto.** `wayland::App` + `Globals` (T27); `wayland::Window` + `BlurRegion` + CSD/blur/slide/xdg-activation + quick-terminal via `gtk4-layer-shell` (T28). Reference `apprt/gtk/winproto/wayland.zig`. `winproto::Window::init` currently lands as a NO-OP under the noop arm; T27/T28 make it real. **Test both Wayland and X11 sessions on the new VM.**

**G. L6 — X11 winproto + the inspector.** X11 `winproto` counterpart; the dcimgui-based inspector (there's a `2026-07-14-inspector-port.md` plan). Plus the `ipc/` bodies (`ipcNewWindow`/`ipcToggleQuickTerminal`, currently inhabited placeholders returning `Ok(false)`).

**H. L3 T9 — gate bookkeeping.** Update the gate scripts / `DEFERRALS.md` / test-count checks (`.port/tools/gates/`, `.port/phase-2/DEFERRALS.md`) so CI accounting matches the landed Linux tiers.

### How to orchestrate (the user's standing request)

Use **dynamic Workflows** to parallelize. The proven pattern for this port (see `/home/gael/tmp/l5-window-chrome.js` for a real example that shipped T20–T22):

- `pipeline(tasks, implement, review, fix)` — each task flows implement → adversarial-review → bounded fix loop (max 2 rounds), no barrier between tasks.
- Per-agent schemas force structured `{status, base, commit, tests, concerns[]}` / `{approved, critical[], ...}` returns.
- **Model:** the user directed **Fable 5** for agent work "until usage runs out." In workflow scripts, **omit `opts.model`** so agents inherit the session model; do not hardcode `model:'opus'` in new scripts. If Fable usage is exhausted, fall back to the session default.
- **Git hygiene inside workflows (LOAD-BEARING — caused real incidents):** NEVER `git add -A`. Each agent stages ONLY its own files by explicit path, confirms `git diff --cached --name-only` lists only those, and commits via `git commit --only <path> -F <msgfile>` (msgfile in `.superpowers/sdd/`, **NO backticks** — heredoc backticks run as command substitution). Concurrent agents on a shared index race; `--only <path>` is the mitigation. Retry on `index.lock`.

---

## 6. Parity & process discipline (the rules that have held for the whole port)

- **R1 fidelity:** port faithfully from the reference; cite `file:line` in commits/comments. Behavior byte-identical where the reference is deterministic.
- **R2 identity:** all identity strings come from `rustty-identity` (`io.airely.rustty`), never hardcoded `com.mitchellh.ghostty`.
- **R3 self-contained:** no ghostty Cargo dependency; vendor pinned lib versions (not host).
- **R4:** acyclic crate graph.
- **R5 (the big one):** everything Linux/GTK is `#[cfg(all(target_os="linux", feature="gtk"))]`-gated. The crate MUST build **without** the `gtk` feature (macOS/embedded path stays byte-identical). **Never edit macOS-gated code.** Always verify both `cargo build --features gtk` AND `cargo build`.
- **The 12 PREPORT_GTK invariants are acceptance criteria** (see `PREPORT_GTK.md`), notably: self-referential Surface heap-boxed-never-moved (AtomicPtr core_surface); lazy `core_surface` on first RESIZE not realize (inv 3); deleteSurface-before-deinit + join-render-thread-before-clear (inv 5); template double-free → `set_child(null)` before `dispose_template` (inv 6); glib source-ids are `SourceId` RAII freed in `dispose` (inv 11); floating-ref sink (inv 12); WeakRef pairing (inv 4).
- **gtk4-rs idiom:** `#[glib::object_subclass]` + `CompositeTemplate` + `glib::Properties` + `*Impl` vfuncs. The GTK apprt is a **native** backend implementing the `apprt_api` seam (generic, not `dyn`) — it does NOT go through the C-ABI (that's the macOS/Swift path).
- **Verify by running, not by trusting agent reports.** Three real memory-safety bugs on this port were found only by running the binary — none by tests. Adversarial review + actually launching `rustty` is the gate.

---

## 7. Known issues / gotchas

- **NOT PUSHED.** `main` is ~76 commits ahead of `origin/main`. The old VM had no SSH key for `git@github.com:AirelyDev/rustty.git` (`Permission denied (publickey)`). On the new VM, configure the key/auth and `git push origin main`. Consider deleting the now-vestigial `linux-port/gtk-apprt` branch afterward.
- **rustfmt cannot parse `application.rs`** (chokes at ~line 1083, `found "GDK_DEBUG"`). This is a **rustfmt limitation, not a compile error** — `cargo build --features gtk` is green. Do NOT run package-wide `cargo fmt` expecting it to succeed on that file; format per-file and skip that one, or accept the rustfmt skip.
- **Pre-merge hygiene pass owed:** ~16 pre-existing clippy/fmt findings across the GTK files (crate-wide drift that was ruled out-of-scope during feature work). Do a dedicated clippy/fmt sweep before considering the port "clean" — but keep it a separate commit, and never let a hygiene sweep touch macOS-gated code.
- **A superseded font-shaper draft was discarded on merge.** Four `rustty-font-shaper` files had an uncommitted earlier-draft of the L3 compile fix on `main`; the branch's reviewed version (`981b57b` + `e35e574`) supersedes them, so they were `git restore`d. A recoverable backup patch was saved to the old VM's scratchpad (`main-fontshaper-draft-superseded.patch`) — you almost certainly don't need it; the branch version is strictly better.
- **GTK single-main-thread:** any test/tool touching a GTK display must run alone (`--exact`), never batched.
- **`TMPDIR` trap** (§3) — the #1 cause of phantom "build breaks" on the old VM.

---

## 8. Pointers

- **Committed plan docs** (`docs/superpowers/plans/`):
  - `2026-07-16-linux-gtk-port-plan.md` — the L0–L6 roadmap.
  - `2026-07-16-linux-xev-epoll-iouring-dynamic.md` — L1.
  - `2026-07-17-L3-fontconfig-discovery.md` — L3.
  - `2026-07-17-L4-opengl-renderer.md` — L4 (read for GL-ATTACH / draw-thread contract when debugging §4).
  - `2026-07-17-L5-gtk-apprt.md` — **L5, 28 tasks with full per-task briefs** (T23–T28 are the remaining ones).
- **The durable ledger** (gitignored): `.superpowers/sdd/progress.md` — full milestone history, resume recipes for the workflows, the git-hygiene incidents, the model directive. Carry this forward if you copy the repo; recreate it on the new VM if starting fresh.
- **Key source files:**
  - `crates/rustty-apprt/src/gtk/class/{application,window,tab,split_tree,surface}.rs` — the backend; the remaining leaves are here.
  - `crates/rustty-apprt/tests/{gtk_first_light,gtk_window_live}.rs` — the headed KATs (your validation anchors).
  - `crates/rustty-renderer/src/generic/linux_impl.rs` — the OpenGL renderer (`init_windowed`/`init_offscreen`, `display_realized/unrealized`).
  - `crates/rustty-core/src/renderer_seam.rs` — Linux arm forwards to `rustty_renderer::Renderer` + 13 thread-lifecycle methods.
- **Reference:** `/home/gael/projects/ghostty/src/apprt/gtk/**` @ `4749c4e9`.

---

**Bottom line for the new session:** the terminal already builds, opens a window, hosts a live shell, and draws frames — proven under software GL. Your job is (1) confirm it renders correctly on real hardware (§4), then (2) finish the interactive/dialog/overlay/palette/winproto surface (§5) with Workflow-orchestrated subagents, holding the parity + R5 + invariant discipline (§6), then (3) hygiene + push (§7).
