# PHASE 2 FINAL AUDIT — item-level deferral inventory

**Date:** 2026-07-30 · **Base:** `main` @ `c43cf52` (fresh worktree) · **Oracle:**
`/Users/gael/projects/ghostty` @ pin `4749c4e93`, beyond-pin read via
`git -C ../ghostty show origin/main:<path>` (working tree never moved) ·
**Vendored Zig corpus:** `~/.cache/zig/p/`

**Binding rule applied throughout (user, verbatim):**

> "Everything is in scope if ghostty supports it. We may not be able to build/verify it
> yet, but I want everything ported, verification later if needed." — "NO MORE DEFERRALS.
> Finish everything before Phase 3."

Therefore the only acceptable dispositions are **PORTED**, **VERIFICATION DEBT** (ported,
unexercisable here, runnable recipe + live owner), or **GENUINELY N/A** (proved with an
oracle citation). "Out of scope by platform", "deferred to a later tier", "comptime false",
and "owned by Phase 3" are **findings**, not dispositions.

This is an audit. It changes no code and edits no Phase-2 ledger.

---

## 1. Headline counts

| Measure | Count |
|---|---|
| Distinct items examined | **121** |
| — non-`cfg(test)` `unimplemented!`/`todo!` sites (exhaustive) | 25 |
| — `# Justified R1 Divergences` entries (exhaustive) | 12 (+1 out-of-section) |
| — `VERIFICATION_DEBT.md` rows (exhaustive) | 8 |
| — `DEFERRALS.md` gate-visible open-claim entries (all listed; ~22 verified to code) | 64 |
| — oracle-vs-port file-level gaps in `src/apprt/gtk/**` (exhaustive at file granularity) | 12 |
| **Genuinely PORTED** (incl. rows whose "open" claim is stale) | **74** |
| **Verification debt — legitimate** | **6** (V1b-residual, V2, V3-residual, V5, V6, V7) |
| **NOT PORTED and must be** | **17 work items** (≈ 6,000–6,500 LOC of reference) |
| **Genuinely N/A (oracle-proved)** | **7** |
| **Mis-classified as divergence / scope / placeholder** | **11** |
| **Ledger rows whose CLAIMED STATUS IS WRONG** | **17** |

Two structural results dominate everything below:

1. **The closure gate `G-P2` encodes the exact failure the user rejected.** Its
   `CLOSED_RE` accepts the literal token `OUT-OF-SCOPE-BY-PLATFORM` as a valid closing
   disposition (`.port/tools/gates/check_phase2_closure.py:120`), and it accepts
   `Phase 3` as a legitimate **owner** for unported implementation work
   (`:184-186`). Twelve `DEFERRALS.md` rows are green solely on that second basis. Under
   "finish everything before Phase 3", both are wrong by construction, so a GREEN G-P2
   would not mean what the ROADMAP says it means.
2. **The port is much further along than its own ledgers say.** 17 rows claim open work
   that has shipped. The stale-in-the-optimistic-direction risk is now smaller than the
   stale-in-the-pessimistic-direction risk: this audit found roughly three times as many
   falsely-open rows as falsely-closed ones.

---

## 2. The prioritized WORK LIST — everything that must still be ported

Ordered by dependency, then by size. "Cost" is S ≤ ~150 LOC, M ≤ ~800, L > ~800.

### Tier 0 — already claimed by a live lane (do not duplicate)

| # | Item | Evidence | Cost |
|---|---|---|---|
| P0.1 | libxev **`iocp`** backend | `~/.cache/zig/p/libxev-*/src/backend/iocp.zig` = **2,376 lines**; port = 10 `unimplemented!` at `crates/rustty-xev/src/lib.rs:545-620` | **L** |
| P0.2 | libxev **`wasi_poll`** backend | same file, `wasi_poll.zig` = **1,648 lines**; same 10 stubs | **L** |
| P0.3 | `termio/Exec.zig` Windows arms | `crates/rustty-termio/src/exec.rs:2508` (`Subprocess.start` windows spawn) and `:3729` (`ReadThread.threadMainWindows`); oracle `src/termio/Exec.zig` has **44** windows references with real bodies | **M** |

### Tier 1 — the same archetype, **no lane owns them**

| # | Item | Evidence | Cost |
|---|---|---|---|
| **P1.1** ✅ **DONE — WS20 (2026-07-30)** | **`rustty-vaxis` `WindowsTty`** — 6 stubbed methods + the `Loop` windows arm | `crates/rustty-vaxis/src/tty.rs:417-448` (struct shell present, every body `unimplemented!("WindowsTty: R5 #[cfg(windows)] seam")`), `loop_.rs:341`. Reference: `~/.cache/zig/p/vaxis-*/src/tty.zig:189-691` = **503 lines** (console-mode, `ReadConsoleInputW`, `INPUT_RECORD` key/mouse/resize/focus decode) plus `Loop.zig:116-180`. This is P0.1's twin and it is **not** in the lane that is fixing xev/termio. | **M/L** |
| **P1.2** ✅ **DONE — WS20 (2026-07-30)** | **Windows font backend `freetype_windows`** | (a) `DeferredFace.Windows` payload left as a verbatim comment — `crates/rustty-font-shaper/src/deferred_face.rs:95-130`; (b) the `Windows` **discoverer** commented out wholesale — `discovery.rs:1911-2073` (~160 LOC), reference `src/font/discovery.zig:900+`; (c) `discovery.rs:2459/2471` test bodies deferred with it. `Backend::FreetypeWindows` already exists in `backend.rs:25`, so only the payloads are missing. | **M** |
| **P1.3** | **Flatpak spawn tier in `rustty-termio`** | `exec.rs:109` declares a **local empty placeholder** `pub struct FlatpakHostCommand {}` instead of using the **fully ported** `rustty_os::FlatpakHostCommand` (`crates/rustty-os/src/flatpak.rs:202-700`, zero `unimplemented!`); `exec.rs:3296` `killCommandFlatpak` is `unimplemented!` although `flatpak.rs:402 pub fn signal(&mut self, sig: u8, pg: bool)` exists; `flatpakExit` + the `waitXev` flatpak arm stay comments (`exec.rs:288-297`, `:906-910`). The excuse is "`build_config.flatpak` is const false" — but `-Dflatpak` is a **real ghostty build option** (`oracle:src/build/Config.zig:170`), which is exactly the "comptime false" version of "out of scope by platform". | **S/M** |

> **P1.1 / P1.2 CLOSED by WS20 (2026-07-30).** Both are ported in full — the
> `WindowsTty` console backend + the two windows `Loop` arms, and the
> `freetype_windows` `Windows` discoverer + the `DeferredFace.win` payload with its
> `deinit`/`familyName`/`name`/`load`/`hasCodepoint` arms. The reference's own
> `test "windows"` ships as a documented skip-stub. Neither can be BUILT for a
> Windows target on this host (no MSVC/mingw C toolchain — see
> `.port/phase-2/VERIFICATION_DEBT.md` rows V10/V11, owner WS20); both are
> type-checked and clippy-clean via `.port/phase-2/wincheck.sh`. Nothing was
> reclassified as out of scope.

### Tier 2 — GTK subsystem files with **zero** rustty counterpart

Established exhaustively by diffing `git -C ../ghostty ls-tree -r HEAD src/apprt/gtk`
against `crates/rustty-apprt/src/gtk/**`. Sizes are oracle blob bytes.

| # | Oracle file | Size | Port state | Cost |
|---|---|---|---|---|
| **P2.1** | `gtk/ipc/DBus.zig` | 6,223 B | ~~absent~~ **FALSE POSITIVE — already ported** (WS20) | — |
| **P2.2** | `gtk/ipc/new_window.zig` | 2,213 B | ~~absent~~ **FALSE POSITIVE — already ported** (WS20) | — |
| **P2.3** | `gtk/ipc/toggle_quick_terminal.zig` | 803 B | ~~absent~~ **FALSE POSITIVE — already ported** (WS20) | — |
| **P2.4** | `gtk/post_fork.zig` | 4,403 B | **PORTED** (WS20) → `src/gtk/post_fork.rs` | done |
| **P2.5** | `gtk/pre_exec.zig` | 2,716 B | **PORTED** (WS20) → `src/gtk/pre_exec.rs` | done |
| **P2.6** | `gtk/cgroup.zig` | 3,354 B | **PORTED** (WS20) → `src/gtk/cgroup.rs` | done |
| **P2.7** | `gtk/flatpak.zig` (`resourcesDir`) | 1,127 B | **PORTED** (WS20) → `src/gtk/flatpak.rs` | done |
| **P2.8** | `gtk/class/config_errors_dialog.zig` | 4,477 B | **PORTED** (WS20) → `src/gtk/class/config_errors_dialog.rs`; the Task-24 placeholder in `class/application.rs` `show_config_errors_dialog` is discharged and the `.blp` is now live | done |
| **P2.9** | `gtk/build/blueprint.zig` | 5,643 B | ~~absent~~ **FALSE POSITIVE — already ported** (WS20) | — |

**WS20 RESOLUTION [2026-07-30] — 5 of the 9 needed porting; 4 were false
positives.** The file-level oracle diff that produced this table can only see
1:1 filename correspondence, and four of these nine were ported under a
different shape:

* **P2.1 / P2.2 / P2.3** (`gtk/ipc/*`) were FLATTENED into
  `crates/rustty-apprt/src/gtk/app.rs`. `DBus.zig`'s `init` target switch is
  `ipc_bus_target` (`app.rs:150`), its `send` is `ipc_send` (`app.rs:177`), and
  the two action files are `ipc_new_window` (`app.rs:233`) /
  `ipc_toggle_quick_terminal` (`app.rs:244`), reached from `App::perform_ipc`
  (`app.rs:135`) — the same dispatch the oracle's `App.zig:80-90` does.
* **P2.9** (`build/blueprint.zig`) was ported into
  `crates/rustty-apprt/build.rs`: the adw-version gate (`build.rs:210`, oracle
  `:58-67`), the `blueprint-compiler >= 0.16` gate
  (`blueprint_compiler_version_gate`, oracle `:69-121`), and the compile step
  (`compile_blueprint`, oracle `:123-173`), all citing the oracle lines. A
  standalone Zig build *binary* has no Rust analogue; a `build.rs` is where that
  logic must live.

The five real gaps (P2.4–P2.8) are ported. `pre_exec.rs` preserves the
reference's async-signal-safe discipline exactly — it does NOT call the
allocating `rustty_os::cgroup::current`, and hand-rolls an allocation-free
equivalent (a unit test proves the two agree byte-for-byte).

**Not a gap — `class/surface.rs:2029` `cgroup_path`.** M10 below cites this as
unported cgroup support. It is not: the oracle's `apprt/gtk/Surface.zig:40-42`
`cgroup()` returns `self.surface.cgroupPath()`, and `cgroupPath` **has no
definition anywhere in the oracle** (`git grep cgroupPath origin/main` → that
one call site only), nor does anything call `apprt.Surface.cgroup()`. The oracle
site is itself dead; the rustty `-> None` placeholder is behaviour-equivalent to
it and needs nothing from `gtk/cgroup.zig`.

Total Tier 2 ≈ **31 KB** of reference ≈ **900–1,000 LOC**, of which ~15 KB was
already ported under other names.

### Tier 3 — named, real, single-item gaps

| # | Item | Evidence | Cost |
|---|---|---|---|
| **P3.1** | **WS19 T6** — the `-C debug-assertions=off` vt Miri lane | The **only** true `G-P2` violation (`DEFERRALS.md:6928`, audit row O2). Its blocker T5 is **DONE** (see §5 W11), so T6 is unblocked today. `grep -rn 'debug-assertions' .port/tools/` → still no second lane. | M |
| **P3.2** | **BL-5 residual** — `rustty_render_state_begin_update` / `_end_update` | `.port/phase-3/README.md:73` (P3-RV5): the oracle exports 185 `ghostty_*` from `src/lib_vt.zig`; rustty ships 183. Upstream split `RenderState.update` into `beginUpdate`/`endUpdate` with a new `pending_styles` field (`origin/main:src/terminal/render.zig:356`/`:399`); rustty has **0** `pending_styles` hits. A feature port, not a signature split. | M |
| **P3.3** | **`Screen::cursor_scroll_region_up`** (BL-4) | `grep -rn 'cursor_scroll_region_up\|cursorScrollRegionUp' crates/` → **0 hits**, re-confirmed this audit. Defined by beyond-pin `77190bd02`. Owner **WS10**, which is **NOT AUTHORED**. | S |
| **P3.4** | **WS10 / BL-4 perf series** as a whole | ROADMAP:192 — "NOT authored, not executed". The only Phase-2 workstream with no plan at all. | L |
| **P3.5** | **GTK AD-3 ceiling losses** (2 forced-legacy arms) | `gtk.rs:66-70`: `application.zig:1261` (`CssSection::bytes()`, `v4_16`) and `application.zig:1565` (`gtk::InterfaceColorScheme`, `v4_20`) are ported to their LEGACY arms because the crate pins the `v4_14` feature ceiling. Real reference behaviour is not reachable in any build. Raising the ceiling is the fix. | S |
| **P3.6** | **Linux `GL-ATTACH`** — `attach_host_view` is a no-op | `crates/rustty-renderer/src/generic/linux_impl.rs:1547-1559` returns `Ok(())` with no hookup, "the windowed `GtkGLArea` context hookup is L5's job". `opengl_impl.rs:365/428` *do* port `present`/`presentLastTarget` verbatim, so the residual is only the attach seam. **Confidence: medium — sampled, not proved.** GTK windows do render live on the VM (WS15/WS16 evidence), so the GLArea may drive `draw_frame` directly and this stub may be vestigial. **Confirm before scheduling.** | S–M |

### Dependency order

```
P3.1 (T6, unblocked now)        ──┐
P1.3 flatpak (os tier ready)     │  independent, can run in parallel
P2.1–P2.9 GTK files              │
P1.2 windows font backend        │
P1.1 vaxis WindowsTty          ──┘
P3.5 GTK ceiling  ── needs a gtk4-rs feature bump first
P3.6 GL-ATTACH    ── CONFIRM the premise before doing anything
P3.2 BL-5 render_state  ── cheapest AFTER Phase 3 advances the pin (beyond-pin API)
P3.3 cursor_scroll_region_up ── pin-sequenced, same reason
P3.4 WS10 perf series   ── needs P3.3 + a plan; largest remaining single unit
```

Only **P3.2** and **P3.3** have a legitimate reason to sequence after the pin advances
(their reference code does not exist at the pin). Everything else is portable today.

---

## 3. Item-level table

### 3a. Every non-`cfg(test)` `unimplemented!`/`todo!` in `crates/**` (exhaustive, 25 sites)

Method: a brace-tracking classifier over every `.rs` file that resolves each `#[cfg(test)]`
/ `#[cfg(windows)]` **item region** by matching braces, rather than assuming everything
after the first `#[cfg(test)]` is test code. Totals: 767 macro hits; **0** `todo!` anywhere;
414 production `unreachable!` (spot-checked — faithful ports of Zig `unreachable`, not
stubs); 55 `unimplemented!` of which 39 are inside `cfg(test)`.

| Site | Class | Verified status | Disposition |
|---|---|---|---|
| `rustty-xev/src/lib.rs:545,555,562,572,579,586,596,603,613,620` (10) | production | iocp/wasi_poll stubs | **NOT PORTED** → P0.1/P0.2 |
| `rustty-vaxis/src/tty.rs:419,422,425,439,442,445` (6) | `cfg(windows)` | 503-LOC reference absent | **NOT PORTED** → P1.1 |
| `rustty-vaxis/src/loop_.rs:341` | `cfg(windows)` | Loop windows arm absent | **NOT PORTED** → P1.1 |
| `rustty-termio/src/exec.rs:2508` | `cfg(windows)` | windows spawn | **NOT PORTED** → P0.3 |
| `rustty-termio/src/exec.rs:3729` | `cfg(windows)` | `threadMainWindows` | **NOT PORTED** → P0.3 |
| `rustty-termio/src/exec.rs:3297` `killCommandFlatpak` | production | `rustty_os::…::signal` exists at `flatpak.rs:402` | **NOT PORTED** → P1.3 |
| `rustty-termio/src/backend.rs:180` `childExitedAbnormally` | production | **N/A, oracle-proved.** `git -C ../ghostty grep childExitedAbnormally -- src/` → only `Surface.zig` + `termio/backend.zig:88,96`; `termio/Exec.zig` declares no such method at the pin **and** at `origin/main`, so Zig never instantiates the switch arm. | **GENUINELY N/A** |
| `rustty-apprt/src/embedded.rs:2275` `renderMetal` (non-Darwin) | production | sole caller `rustty_inspector_metal_render` is itself `cfg(target_os="macos")` | **GENUINELY N/A** |
| `rustty-apprt/src/gtk/class/inspector_widget.rs:109,120` (2) | production | gtk4-rs `property`/`set_property` catch-all arms over a 1-property class | **GENUINELY N/A** |
| `rustty/src/main_rustty.rs:299` | production | reached only when `app_runtime()` names a backend not compiled into this build; macOS is `.none` | **GENUINELY N/A** |

### 3b. `# Justified R1 Divergences` — all 12 entries re-audited

The brief expected **18**. There are **12** in the section (`DEFERRALS.md:6327-6890`) plus
one out-of-section permanent divergence at `:4251`. The "18" premise is itself stale — see
§5 W17.

| # | Entry | 4-criterion basis holds? | Verdict |
|---|---|---|---|
| D1 | libxev `iocp` + `wasi_poll` — "OUT OF SCOPE BY PLATFORM" (`:6327`) | **NO.** Criterion 1 asserts "Windows is out of scope at the ROADMAP level" — that is a project convenience, not a property of the reference. 4,024 lines of real libxev code. | **MIS-CLASSIFIED** → M1 |
| D2 | wayland `bind_global` stale `name→tag` (`:6388`) | **YES.** Criterion 2 is the decisive one and it is verifiable: the oracle's `Globals.zig` has the same shape, so "fixing" it would diverge *from* the reference. | **LEGITIMATE** |
| D3 | termio ReadThread Linux `POLLHUP` quirk (`:6415`) | **YES.** Pure fidelity: the port reproduces `threadMainPosix`'s HUP-before-drain ordering; the un-diverge trigger is an upstream fix, tracked as `P3-RV4`. | **LEGITIMATE** |
| D4 | xev ThreadPool `Event` shutdown wake (`:6487`) | **YES.** Liveness > R1, controller-approved, regression-gated (`multi_worker_quiesce_then_join_50x`). | **LEGITIMATE** |
| D5 | macOS `xev.File`/`TCP`/`UDP` + kqueue skip-stub (`:6525`) | **PARTIAL.** Criteria 1–2 hold and `tests/surface_parity.rs` pins it. But it is an **export decision on an unexercised surface**, and the entry admits the Linux side is ported — i.e. the capability exists and only the macOS instantiation is missing. Under the binding rule this is small unported work wearing a divergence label. | **WEAK — reclassify as S-cost work** |
| D6 | libxev `c_api.zig` (`:6560`) | **NO.** Criterion 3 ("porting it would create an ungateable surface") argues *against porting because we could not test it* — precisely the reasoning the binding rule forbids. The reference file exists and is 1 test. | **MIS-CLASSIFIED** → M6a |
| D7 | xev `Completion.task_wakeup` (`:6584`) | **YES, and it is the model entry.** The naive faithful shape is *proved* UB under both Stacked and Tree Borrows on a committed harness (`mirihar-xev-mpsc`), the cheaper fix is proved insufficient, and the divergence is bit-invisible to every observer. | **LEGITIMATE** |
| D8 | `rustty-bench` macOS `os_signpost` (`:6633`) | **PARTIAL.** Criteria 1–3 hold (pure Instruments annotation). Criterion 4 is "the log-facade tier is deferred tree-wide" — a deferral used as a justification. Small, but it is unported reference code. | **WEAK — S-cost work** |
| D9 | `rustty-synthetic`/`bench` local `detectArgs` (`:6656`) | **YES.** Forced by the acyclic constraint (R4/G7), replication is faithful, behaviour identical. | **LEGITIMATE** |
| D10 | terminfo SOURCE + termcap never emitted (`:6692`) | **YES.** The reference's own `emit_terminfo_source`/`emit_termcap` default **false for Release** (`oracle:src/build/Config.zig:410-429`) and the shipping app is Release, so the shipped configuration matches exactly. | **LEGITIMATE** |
| D11 | `killPid` bounded wait (`:6717`) | **YES — the strongest entry in the file.** Cures an unrecoverable `?Es` wedge, states its precise guarantee, documents the *rejected* SIGKILL escalation and why, and is honest that no in-suite guard exists. | **LEGITIMATE** |
| D12 | `threadEnter` error arms drain pty master (`:6834`) | **YES.** Strictly weaker than D11 (no observable consequence outside the failing arm), bounded by `DRAIN_BUDGET`, guard KAT mutation-verified. Upstream defect re-confirmed at head. | **LEGITIMATE** |
| D13 | `stbir_resize_uint8` ±1 LSB (`:4251`, outside the section) | **YES.** ghostty's own Debug and ReleaseFast builds of the identical C disagree (9/512 bytes, maxdelta 1); rustty sits on the consensus side across 5 toolchain/opt combos on 2 OSes. Changing flags would *create* divergence. | **LEGITIMATE** |

**Score: 8 legitimate, 2 weak, 2 mis-classified** (D1 is the archetype; D6 is the same
reasoning in miniature).

### 3c. `VERIFICATION_DEBT.md` V1–V7 (exhaustive)

| Row | Really unexercisable here? | Runnable recipe? | Live owner? | Verdict |
|---|---|---|---|---|
| **V1** macOS xev build | n/a — discharged, body deleted per the file's own rule | — | — | correctly closed |
| **V1b** macOS inspector | **Closed**, with a residual: cross-DPI drag never triggered `viewDidChangeBackingProperties`. The single-display fallback the plan offered was **not attempted** — so the residual is *assumed* unexercisable, not proved. | yes (resolution toggle) | WS17 (open) via the ROADMAP scope table | **legit debt, but re-test is cheap — try the fallback** |
| **V2** bare-metal GPU sign-off | **YES, established not assumed.** UTM Apple-Virtualization gives no 3D (`-virgl` in `journalctl -k`); the earlier virgl attempt exposed `Max core profile version: 0.0` against a 4.3-core floor; disk conversion needs ~40 GB the host lacks. | yes — `hw_gl_probe.sh` with a self-test proving both arms | WS17 (open) | **legitimate** |
| **V3** KDE/KWin | **Partly.** Globals DISCHARGED under a real nested KWin 6.7.3 with a mutter negative control. Two residuals remain (`slidingpopups` not loadable ⇒ no `org_kde_kwin_slide_manager`; KWin advertises `ext_background_effect` with `blur_capability=false`). | yes | **NONE — the entry names no owner.** G-P2 scores the whole row "closed" on the word DISCHARGED, so its two live residuals are invisible to the gate. | **legit debt, ORPHANED** |
| **V4** mutter `set_urgent` | **DISCHARGED** against real headless mutter 50.0 on its own socket; uptime recorded both sides. Residual (re-run on the primary session) is an explicit, reasoned refusal. | yes | header still says "OWNER: WS17" though the row is closed | **correctly closed; header is stale** |
| **V5** X11 residuals | **YES for item 1** — no form of synthetic keyboard input reaches any client on this session, proved three ways (XSendEvent ignored by GTK; XTEST with confirmed focus delivered nothing; an independent `zenity` client captured empty). Items 2–3 need a box that does not exist here. | yes | WS17 (open) | **legitimate — best-evidenced row in the file** |
| **V6** wayland `default_mode` post-bind transport | **YES, measured on three compositors** (KWin sends it once per bound resource — confirmed under `WAYLAND_DEBUG=1`; sway sets it once at startup; mutter has no KDE manager). | yes (two routes) | **NONE written.** G-P2 credits `['WS17','WS19']` by *grepping WS ids anywhere in the entry* — both are provenance mentions. | **legit debt, ORPHANED** |
| **V7** RTL Hebrew layout | **YES** — `he_IL.UTF-8` not generated as a session locale; judgement is human-visual. Mechanically-assertable parts are all asserted (`i18n_msgid_coverage.rs`, `i18n_translate.rs` with German + Japanese + `LANG=C` + stale-msgid controls). | yes, with a negative control | Phase 3 | **legitimate** |

On the brief's WS17 concern: the `[x]` in the ROADMAP means **plan authored**, not
executed, and G-P2 parses it correctly (`check_phase2_closure.py:172-180` gives
"NOT executed" precedence over the tick), so WS17 *is* treated as an OPEN owner. But the
row also under-states reality — `.port/phase-2/evidence/ws17/` contains
`v2-hw-gl-probe-controls.txt`, `v2-llvmpipe-vs-ghostty.txt`, `v3-kde-globals-under-kwin.txt`
and `v4-set-urgent-real-mutter.txt`, i.e. **WS17 partially executed** (V3 and V4 landed).
V2/V4 are therefore not orphaned; **V3's and V6's residuals are.**

### 3d. `MIRI_EXCEPTIONS.md`

| Claim | Verified | Verdict |
|---|---|---|
| The "**66+ min** crate-scale Miri compile wall" (`:248-253`) | The file itself corrects it at `:274-287` — an isolated run "compiled AND ran 15 clone tests in **44 s** total"; the 66-min figure was cold-cache + workflow contention. | **corrected in place; the false figure is retained above its own correction, which is how it keeps getting re-quoted** |
| "`roundtrip` + `leak` … ~45 min" (`:1126`) | Corrected at `:1268` and `:1323`: those attempts were **host-killed at ~45 and ~65 min without finishing** and were explicitly **not** used as acceptance. | **now honest** |
| The `style::Color` OPEN FINDING is "**NOT yet executed**" and "`style.rs:143` is a plain Rust `enum Color` with no `#[repr]`" (`:1126-1134`) | **FALSE on both counts.** `crates/rustty-vt/src/style.rs:175` is `#[repr(C)] pub struct Color { tag: u8, data: [u8;3] }` with `const _` asserts pinning `size_of == 4`, `align_of == 1`. The same file says RESOLVED at `:1290-1325`. | **the file CONTRADICTS ITSELF** → W11 |
| The 10 `cfg_attr(miri, ignore)` compression annotations (`:1382-1404`) | Method is sound — every candidate re-measured in **single-worker isolation** before being excepted, and one tractable test (`…leaves_a_compressed_node_compressed`, 77 s) deliberately left un-annotated. The documented harness trap (an *ignored* test is scored `res=FAIL`) is real and is called out. | **still true** |
| Category-2 raw-syscall exceptions | unchanged, complement `rss_probe` still cited | **still true** |

### 3e. `DEFERRALS.md` — the 64 gate-visible open-claim entries

The gate's own enumeration (`check_phase2_closure.py -v`) is the index; 22 of the 64 were
verified against code and oracle for this audit, the remainder were read but not
code-verified (see §6). The material results:

* **12 rows are owned by "Phase 3"** and are therefore, under the binding rule, deferrals
  past the line the user drew: `D:172` (stream `c_union`), `D:466` (shadertoy — **already
  ported**, W1), `D:658` (termio stage 2), `D:1858`/`D:1923` (core C-F roll-up — both
  struck RESOLVED), `D:2117`/`D:2127` (inspector E2→E4), `D:2631` (`rustty_inspector_metal_*`),
  `D:2751` (cli C1), `D:3202` (rustty bin C6), `D:3550` (xev LT8), `D:4339` (`ghostty_*`
  alias — struck SUPERSEDED). Of these, five are struck/resolved in place; the rest need a
  real owner or a port.
* **`D:4882`/`D:4897`** (vaxis `WindowsTty`, `TestTty::next_event`) pass the gate with owner
  `WS19` — but WS19's plan contains no task to port them. **Owned in name only.**
* **`D:4388`** (the GTK subsystem) is not gate-visible at all (its marker is "Why deferred",
  not "What's deferred") and is grossly stale — see W3.
* Core (`D:1858`) is genuinely closed: `surface.rs` has **0** production `unimplemented!`,
  `app.rs`'s 8 and `renderer_seam.rs`'s 15 are all inside `cfg(test)` fakes, independently
  reproduced by this audit's classifier.

---

## 4. Every mis-classification found, with evidence

| # | Mis-classification | Evidence |
|---|---|---|
| **M1** | libxev `iocp`/`wasi_poll` filed as a permanent **"OUT OF SCOPE BY PLATFORM"** Justified R1 Divergence | `DEFERRALS.md:6327-6386`; 4,024 lines of real reference code in `~/.cache/zig/p/libxev-*/src/backend/`. Criterion 1 reduces to "the ROADMAP says Windows is out of scope", which is a project decision, not a property of the reference. **Still on `main` at `c43cf52`.** |
| **M2** | `termio` `cfg(windows)` seams re-worded as an explicit **platform scope boundary** | commit `7781af8` "termio: reword the cfg(windows) spawn/read seams as an explicit platform scope boundary"; panic strings at `exec.rs:2509` and `:3730` literally read `"OUT OF SCOPE BY PLATFORM"`. Oracle `src/termio/Exec.zig` carries **44** windows references with real bodies. **Still on `main`.** |
| **M3** | `rustty-vaxis` Windows seams labelled **"R5 `#[cfg(windows)]` seam"** | 7 sites: `tty.rs:419,422,425,439,442,445`, `loop_.rs:341`. Identical class to M1/M2 and **not covered by the lane fixing them.** 503 LOC of `vaxis/src/tty.zig`. |
| **M4** | The font tier cites a divergence entry **that does not exist** | `crates/rustty-font-shaper/src/deferred_face.rs:29` and `:74` both say Windows is excluded per `DEFERRALS.md` "Justified R1 Divergences" — there is **no** entry in that section for the Windows font backend. A dangling citation is worse than an honest deferral: it reads as adjudicated when nothing adjudicated it. |
| **M5** | **The closure gate blesses the rejected disposition** | `.port/tools/gates/check_phase2_closure.py:120` — `CLOSED_RE` includes `\bOUT-OF-SCOPE-BY-PLATFORM\b`. Any row that writes that phrase is scored disposed. The mis-classification is encoded in the machine that is supposed to prevent it. |
| **M6** | **"Phase 3" accepted as an owner for implementation work** | `check_phase2_closure.py:184-186` unconditionally adds `Phase 3`/`Phase-3` to `open_ws`. Phase 3 is the *upstream-resync* phase (`.port/phase-3/README.md:1-5`), not an implementation phase. 12 `DEFERRALS.md` rows are green only because of this. |
| **M6a** | libxev `c_api.zig` excused because **it could not be gated** | `DEFERRALS.md:6572-6576` — "porting it would create an UNGATEABLE surface … strictly worse than its absence". Under "verification later if needed", untestability is not a reason not to port. |
| **M7** | Gate owner-detection is a **bare WS-id grep**, so provenance passes as ownership | `OWNER_RE` at `:105` matches `WS\d` anywhere in an entry. `VERIFICATION_DEBT.md` V6 has **no** `OWNER:` line yet is credited `['WS17','WS19']` from mentions of "WS19 T9 made…" and "the WS17 plan's V3 assumptions". |
| **M8** | Gate scores a whole entry **closed on any disposition token**, hiding live residuals | V3 is scored `closed` on the word DISCHARGED while its own text lists **two** open residuals with a recipe and names no owner. |
| **M9** | Two ledgers the gate **never reads** | `LEDGERS` at `:75-80` omits `.port/phase-2/VT_MIRI_SWEEP.md` — which carried **9 real Stacked-Borrows UB tests** marked "fix pending" and "the vt Miri G6 lane stays correctly RED" — and `.port/phase-2/DEFERRALS.swiftapp-misc.md`. An unported UB fix could sit there indefinitely and G-P2 would stay green. |
| **M10** | Unported GTK cgroup support labelled a **"Placeholder-leaf"** | `crates/rustty-apprt/src/gtk/class/surface.rs:2029`. `gtk/cgroup.zig` (3,354 B) has no counterpart anywhere in `crates/`. "Placeholder-leaf" is doing the same work as "scope boundary". **RESOLVED [2026-07-30, WS20], and the row was half wrong.** `gtk/cgroup.zig` IS now ported (`src/gtk/cgroup.rs`, with `pre_exec.rs`/`post_fork.rs`), so the label no longer stands in for missing work. The `cgroup_path` site itself was never the gap: the oracle's `apprt/gtk/Surface.zig:41` calls `self.surface.cgroupPath()`, a method with **no definition anywhere in the oracle**, and nothing calls `apprt.Surface.cgroup()` either — the oracle site is dead, so `-> None` is behaviour-equivalent to it. The mis-labelling critique stands; the "unported cgroup support" premise attached to *this* site did not. |
| **M11** | Flatpak excused as a **"build-config branch (const false)"** | `exec.rs:3297`. `-Dflatpak` is a real, user-selectable ghostty build option (`oracle:src/build/Config.zig:170`, wired at `:529`/`:621`) and is what the official Flatpak package uses. "comptime false" is the Zig-flavoured spelling of "out of scope". |

---

## 5. Ledger rows whose claimed status is WRONG (17)

All verified against **code on `main`** and, where relevant, the oracle.

| # | Row | Claim | Truth |
|---|---|---|---|
| **W1** | `DEFERRALS.md:466` | "`loadFromFiles`/`loadFromFile` **DEFERRED**" | **PORTED.** `crates/rustty-renderer/src/shadertoy.rs:141 load_from_files`, `:188 load_from_file`, with a test at `:877`. The module doc at `:11` already says so. |
| **W2** | `crates/rustty-renderer/src/generic.rs:45` | "DEFERRED … **still an `unimplemented!()` seam**" for inspector overlay-features | **PORTED, and the seam does not exist.** `grep -rn 'unimplemented!\|todo!' crates/rustty-renderer/src` → one **comment** at `:5222`, zero call sites. `InspectorOverlay::overlay_features` is implemented at `rustty-inspector/src/widgets/renderer.rs:294-295` and consumed live at `rustty-core/src/surface.rs:13896`. Matches `widgets.rs:39` ("LIVE"). |
| **W3** | `DEFERRALS.md:4392-4393` | GTK: "**Rustty counterpart: NONE** (zero counterpart — the single largest unported chunk)" | **46 `.rs` files, 26,673 lines** under `crates/rustty-apprt/src/gtk/` vs 52 oracle `.zig`. Only the 9 files in §2 Tier 2 are actually absent. |
| **W4** | `crates/rustty-apprt/src/gtk.rs:10-13` | "PORT STATUS: **scaffold** … Nothing is re-exported yet because **nothing is ported yet**" | Same as W3. The module doc is ~26k lines out of date. |
| **W5** | `crates/rustty-font-shaper/src/discovery.rs:5,32,60-64,69,705` | Fontconfig discoverer "**DEFERRED**", `Discover` is a "**typed stub** … present only so `CodepointResolver`/`Collection` type-check" | **FULLY IMPLEMENTED** at `discovery.rs:88-370` (`fontconfig_impl`): real `to_fc_pattern` / `substitute_with_pat` / `font_sort` against the `fc` binding. Confirmed by `DEFERRALS.md:5348` ("RESOLVED"), which the file's own prose contradicts. |
| **W6** | `discovery.rs:2114`, `deferred_face.rs:384,496-497` | bodies are "**honest `unimplemented!()` leaves**" | `grep -rn 'unimplemented!\|todo!' crates/rustty-font-shaper/src` → **6 hits, all inside comments**. Zero call sites in the crate. |
| **W7** | `crates/rustty-renderer/src/metal/metal_impl.rs:292` | "**DEFERRED** (windowing-host seam M-2): the real `Metal.zig` `init(alloc, opts)`" | `pub fn init` is live at `metal_impl.rs:379`; `generic.rs:42` states W-ATTACH is RESOLVED at `96b0116` with "NO remaining windowed-render seam". |
| **W8** | `crates/rustty-termio/src/exec.rs` — **13** "DEFERRED (D6 …)" comments (`:205,:630,:750,:864,:1229,:1439,:3342,:3491,:3671`, …) | D6 xev wiring deferred | **D6 LANDED.** `threadEnter` live at `exec.rs:331`, `focusGained` at `:785`, `queueWrite` at `:1298`. `DEFERRALS.md:658`'s own header already says "D6 RESOLVED — xev Milestone B DONE, live shell runs". The comments sit above preserved verbatim Zig, immediately followed by the real implementation. |
| **W9** | `.port/phase-2/VT_MIRI_SWEEP.md:14,49-52` | 9 real Stacked-Borrows UB tests, "**fix pending**", "the vt Miri G6 lane stays correctly **RED** until this lands" | **THE FIX LANDED.** The prescribed raw-pointer restoration is in the tree: `crates/rustty-vt/src/highlight.rs:54,87,118` and `search/screen.rs:97,143,167,267` all take `*mut Screen`, not `&mut Screen`. (Whether the 9 tests now run Miri-clean was **not** re-measured here — Miri is not cheap on this box. The *code* change is done; the *proof* is unrecorded.) |
| **W10** | `MIRI_EXCEPTIONS.md:1126-1134` | the `style::Color` finding is "**NOT yet executed**"; `style.rs:143` is "a plain Rust `enum Color` … with no `#[repr]`" | **DONE.** `crates/rustty-vt/src/style.rs:175` is the byte-complete `#[repr(C)]` struct with `const _` size/align asserts. The same file records the resolution at `:1290-1325`. Two sections of one file disagree. |
| **W11** | ROADMAP:235-241 | "REMAINING: **T5, T6, T8, T9, T10, T12, T17**, and T20" | **Six of eight have landed.** T5 → `style.rs:175` (W10). T8 → `DEFERRALS.md:4650` "RESOLVED — WS19 T8". T9 → `VERIFICATION_DEBT.md:349` (the re-read shipped). T10 → the `Completion.task_wakeup` divergence `:6584` is stamped "WS19 T10". T12 → audit row O12 "DONE — WS19 T12, IMPLEMENTED not diverged". T17 → `DEFERRALS.md:6172`/`:6253` both "✅ RESOLVED … WS19 T17a/T17b". **Only T6 and the T20 finale remain.** |
| **W12** | ROADMAP:268 | "The open set is **exactly WS10, WS11, WS12, WS13, WS17**" | Wrong on four of five, as the brief suspected. WS11 CLOSED (row 194), WS13 DONE (row 208), WS14 DONE and not listed, WS19 open and not listed. The gate derives the true set: **WS10, WS12, WS17, WS19**. |
| **W13** | ROADMAP:259-261 | WS12/WS13/WS14 executed "on branch `…`, **not yet merged**" | **The content is on `main`.** `crates/rustty-zigimg/` exists (WS12); `crates/rustty-extra/{mdgen,webgen}.rs` + `xtask/src/res_misc.rs` exist (WS13); `example/wasm-webgl/` + `crates/rustty-wasm/` exist (WS14). Neither `ws12-*` nor `ws14-*` appears in `git branch -a`. Exactly the "branch merged ≠ work on main" trap, inverted. |
| **W14** | ROADMAP:228 | WS17 "**NOT executed**" | Partially executed: `.port/phase-2/evidence/ws17/` holds V2 probe controls, the V2 llvmpipe-vs-ghostty comparison, the V3 KDE-globals discharge and the V4 real-mutter discharge. |
| **W15** | `crates/rustty-apprt-c/tests/abi_layout.rs:242` | "DEFERRED: `ghostty_info()` … NOT yet ported" | Superseded — `crates/rustty-apprt-c/src/lib.rs:911` says "now landed — supersedes `abi_layout.rs`'s stale 'DEFERRED' note". The stale note is still in the test file. |
| **W16** | `DEFERRALS.md:1943-1955` core item 4 (D6 xev leaves in `surface.rs`) | live calls into the `rustty-xev` **placeholder** that "panic only at runtime" | Struck RESOLVED at `:1957-1963`, and independently confirmed: `rustty-xev` is a real crate (`thread_pool.rs` = 1,630 lines, 0 `unimplemented!`). Listed here because the pre-strike text is still the first thing a reader hits. |
| **W17** | The audit brief itself | "the **18** entries under `# Justified R1 Divergences`" | There are **12** (`:6327,:6388,:6415,:6487,:6525,:6560,:6584,:6633,:6656,:6692,:6717,:6834`), plus one permanent divergence filed outside the section at `:4251`. |

---

## 6. What was verified exhaustively vs sampled

### Exhaustive (machine-enumerated, every hit inspected)

* **Every `unimplemented!`/`todo!`/`unreachable!` in `crates/**/*.rs`** — 767 hits,
  classified by a brace-matching `#[cfg(...)]` region resolver rather than the
  "everything after the first `#[cfg(test)]`" heuristic the brief warned about. All 25
  non-`cfg(test)` `unimplemented!` sites are individually dispositioned in §3a. `todo!`
  count is **zero**.
* **All 12 `# Justified R1 Divergences` entries** — each read in full and scored against
  its own 4-criterion basis (§3b).
* **All 8 `VERIFICATION_DEBT.md` rows** (§3c).
* **File-level oracle-vs-port diff of `src/apprt/gtk/**`** — every oracle `.zig` matched
  against `crates/rustty-apprt/src/gtk/**`; the 9 absent files are listed with sizes.
* **The G-P2 gate's full enumeration** of 64 + 4 + 1 + 18 open-claim entries, plus a read
  of the gate source to establish its blind spots (M5–M9).
* **The ROADMAP's WS checklist and "true remaining scope" table** — every row cross-checked
  against `main` content.

### Sampled (read, but not verified to full depth)

* **42 of the 64 `DEFERRALS.md` gate-visible entries** were read but **not** individually
  code-verified. 22 were. Given that 17 wrong-status rows surfaced from those 22, the
  expected number of further stale rows in the unverified 42 is **material** — treat §3e's
  per-row statuses as indicative, not final.
* **The 279 strong `DEFERRED`/`TODO`/`FIXME`/`later tier` comment markers** were enumerated
  in full and triaged by file, but only ~30 were opened. Most `TODO`s are faithful ports of
  ghostty's own `TODO` comments (e.g. `rustty-input/src/keycodes.rs` `TODO(garykac)`,
  `rustty-vt/src/terminal.rs` `TODO(qwerasd)`) and are correct as-is.
* **414 production `unreachable!`** were spot-checked (≈15 sites), not exhaustively
  reviewed. All checked were faithful ports of Zig `unreachable`.
* **P3.6 (Linux `GL-ATTACH`)** is the weakest item in the work list: the stub is real, but
  whether it is load-bearing was not proved. Confirm before scheduling.
* **W9 (the vt search Stacked-Borrows fix)** — the *code* change was verified present. The
  *Miri result* was not re-measured; running the 9 tests under Miri costs 8–23 s each plus
  a crate compile, which was outside this audit's budget on a shared host.
* **Windows coverage outside vaxis/xev/termio/font** was sampled: `rustty-os/src/windows.rs`
  (308 lines) and the ConPTY path (`pty.rs:630 CreatePseudoConsole`) are genuinely ported,
  and `rustty-os` has **zero** `unimplemented!`. So Windows is *not* uniformly excluded —
  which makes the four places it *is* excluded harder to defend, not easier.
* No build heavier than `grep` was run. `df -h /` reported **36 GiB free** at start.

### Not attempted

* Running Miri, ASan, the GTK/browser gates, or `cargo test`.
* Verifying the beyond-pin Phase-3 area assessments (`.port/phase-3/assessments/*.md`).
* `.port/phase-2/DEFERRALS.swiftapp-misc.md` (a second deferral ledger the gate does not
  read) was identified but not audited.
