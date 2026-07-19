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

## V1. macOS build + test of `rustty-xev` (blocked: no macOS toolchain on this VM)

**What changed that macOS consumes.** The `xev-libxev-completion` branch (12 commits,
`eb655ba..9de16be`) edited SHARED code that the macOS build compiles:

- `crates/rustty-xev/src/thread_pool.rs` — the whole ThreadPool port is new and
  un-gated for both OSes. The macOS futex shim (`__ulock_wait`/`__ulock_wake`,
  `thread_pool.rs` bottom) **parses but has never been compiled** (cfg-gated, no
  toolchain here). Note: the Linux futex_wake i32-clamp bug class does NOT apply to
  the macOS shim (`n > 1` selects `ULF_WAKE_ALL` — a flag, not a count), but this
  reasoning is unverified by execution. The Task-1 `Event::wake` deliberate deviation
  (unconditional SHUTDOWN wake + sticky SHUTDOWN; see the DELIBERATE DEVIATION note
  in-file and the `DEFERRALS.md` entry) is shared code and runs on macOS too.
- `crates/rustty-xev/src/lib.rs` — export-seam and header-comment changes around the
  per-target `pub use` blocks.
- `crates/rustty-xev/tests/surface_parity.rs` — new compile-time export pins
  (Linux-gated; must stay inert on macOS).

**Recipe (on any Mac with the repo's pinned Rust 1.93.1):**
```
cargo build -p rustty-xev
cargo test  -p rustty-xev            # kqueue suite + ThreadPool smoke/lifecycle tests
cargo clippy -p rustty-xev --all-targets
```
**Expected:** builds clean; full suite green — specifically the `thread_pool` tests
(`schedule_runs_all_tasks_and_deinit_joins`, `multi_worker_quiesce_then_join_50x`,
`init_shutdown_deinit_without_tasks_joins_cleanly`) now exercise the `__ulock_*` shim
for the first time. `multi_worker_quiesce_then_join_50x` has a 60 s watchdog: a hang
there means the macOS shim's wake semantics diverge — capture per-thread backtraces
(`sample <pid>` / lldb `thread backtrace all`) before killing; do not paper over with
`max_threads = 1`.

Also worthwhile while there: `cargo build -p rustty-apprt` (macOS apprt untouched this
session — expected no-op confirmation).

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

**Out of scope by decision (not debt):** the WebGL/wasm apprt (owner decision,
2026-07); macOS `File`/`TCP`/`UDP` watcher instantiations (tracked in `DEFERRALS.md`
as the C7 follow-up chunk — an implementation deferral, not verification debt).
