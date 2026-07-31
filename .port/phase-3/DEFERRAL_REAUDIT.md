# DEFERRAL RE-AUDIT — every tracked deferral re-verified against code [2026-07-31]

**Scope.** `.port/phase-2/DEFERRALS.md` (7420 lines, ~90 sections) plus every other
`.port/**/*.md` carrying deferral / open-item / "not ported" / "out of scope"
language (`GAP_ANALYSIS.md`, `VERIFICATION_DEBT.md`, `MIRI_EXCEPTIONS.md`,
`INTEGRATION_PARITY.md`, `NON_PORTS.md`, `PHASE2_FINAL_AUDIT.md`,
`assessments/*.md`, the `*_BLUEPRINT.md` / `*_DESIGN.md` set, `REACHABILITY.md`,
`TEST_AUDIT.md`, `SYMBOL_AUDIT.md`, `P3-W5-REVET.md`, `VT_MIRI_SWEEP.md`,
`MAIN_PARITY.md`, `DEFERRALS.swiftapp-misc.md`, `specs/completeness-wasm.md`).

**Binding method rules** (both are prior failures of this program, re-applied here):

1. **Never verify a row with the search scope the row itself supplies.** Repo-wide
   first, then narrow. (`O17`/stb was wrong because the row's own grep was scoped to
   a crate the code had moved out of.)
2. **Re-derive against the ORACLE, not only the port.** ("WebGL backend missing,
   cost L" was wrong: the oracle's `src/renderer/WebGL.zig` is 2 lines.)

**Standing scope rule** (user, verbatim): *"Everything is in scope if ghostty
supports it. We may not be able to build/verify it yet, but I want everything
ported, just verification later if needed."* Every "out of scope" disposition was
re-tested against it.

---

## A. Counts

| | |
|---|---|
| Deferral rows examined (DEFERRALS.md + all sibling ledgers) | **231** |
| **DISCHARGED** — verified done against code | **196** |
| **STILL OPEN** — verified genuinely unimplemented | **19** |
| **NON-PORT** — argued reason that survives the scope rule | **16** |
| Newly implemented in this pass | **5** |
| Rows whose **claimed status was WRONG** | **14** |
| of those, claimed OPEN but actually **DONE** | **11** |
| of those, claimed RESOLVED/permanent but actually **OPEN** | **3** |

The dominant error direction has **inverted** since the WS19 audit. WS19's problem
was orphaned open rows; today's problem is **stale-open rows** — work that later
workstreams (P3-W3, P3-W5, P3-W10, WS13, WS14, WS20) landed without ticking the
ledger. Eleven rows assert missing code that exists.

---

## B. THE HEADLINE — the oracle pin advanced *today*, invalidating the two
## "BEYOND-PIN / owner Phase 3" dispositions

`.port/ORACLE_SYNC.md:36` records the pin moving `4749c4e93` → `6ad1fe7d8` on
**2026-07-31** (P3-W10 T3), a span of **365 commits**. Two rows were parked as
"beyond-pin, therefore Phase 3's problem". Both are now not merely portable —
**both are already ported**, and the ledger never noticed.

| Row | Ledger claim | Verified reality |
|---|---|---|
| **O3** `Screen::cursor_scroll_region_up` (`D:6073`) | "unported; blocking commit `77190bd02` is BEYOND-PIN; re-owned to Phase 3" | **DISCHARGED.** `crates/rustty-vt/src/screen.rs:1885` (`_inner` `:1903`, `_slow` `:2054`), all 6 oracle tests ported (`screen.rs:12828`–`13097`) **plus** a fast/slow differential the oracle does not have. Landed by `a8855af` (P3-W5 T3). `77190bd02` is now an ancestor of the pin. |
| **O5** `PageList::Limits` / `Limits::enforce` (`D:5998`) | "no rustty counterpart; Phase 3 re-vet **P3-RV1**" | **DISCHARGED.** `crates/rustty-vt/src/pagelist/mod.rs:1295` `Limits`, `:1370` `exceeded`, `:2388` `enforce_limit`. Landed by `abc2b3d` (P3-W3 T3) — whose commit message *explicitly says* "mark_activity on enforce", i.e. it satisfied P3-RV1's assertion at the moment it was written. |

**Why the gate did not catch this.** `G-P2`
(`.port/tools/gates/check_phase2_closure.py`) is **green** — 93 open-claim entries,
0 violations. But re-read what it asserts: every row is *"struck, reclassified, or
**owned by an OPEN workstream**"*. That is an **ownership** predicate, not a
completion predicate. A row saying "OPEN, owner: Phase 3" satisfies it forever,
whether the work is untouched or shipped months ago. G-P2 is well-built for what it
does (it has a real `--self-test` and real FAIL arms) but it **structurally cannot
detect an item that is done, nor one that is undone** — only one that is unowned.
Nothing in the gate set closes that loop. This is the single most important
structural finding of this audit.

---

## C. STILL OPEN — the complete list (19)

Ordered by size. Nothing here is half-done; each is recorded precisely.

### Small (XS/S) — 8

| # | Item | Evidence / what is missing | Size |
|---|---|---|---|
| 1 | **`freetype-load-flags` config is silently ignored** | `crates/rustty-core/src/surface.rs:1201` hard-codes `FREETYPE_LOAD_FLAGS_DEFAULT` instead of `config.freetype_load_flags`. The consuming machinery is fully live (`crates/rustty-font-shaper/src/face/freetype.rs:255,521`). Three more stubbed sites: `rustty-renderer/src/generic.rs:6204,6332`, `generic/linux_impl.rs:3899`. Oracle: `src/font/SharedGridSet.zig:488,528,696`. **User-visible: the setting does nothing on Linux/freetype builds.** | S |
| 2 | **termio `dcsCommand` tmux arm is a silent no-op** | `crates/rustty-termio/src/stream_handler.rs:507` is `Command::Tmux(_) => {}`, body commented out at `:429-505`. **Every stated blocker is gone**: `rustty_vt::dcs::TmuxCommand` is real, `crates/rustty-vt/src/tmux/viewer.rs:441` `next` is ported, the handler already owns `tmux_viewer` (`:158`) and its teardown (`:186`). Not comptime-dead — `TMUX_ENABLED` (`:75`) is **true** in the shipped binary (`rustty-core/Cargo.toml:28` enables `oniguruma`). Oracle: `src/termio/stream_handler.zig:384-460`. | S–M |
| 3 | **wayland `bind_global` map shape diverges** | rustty keys `names: HashMap<u32, Tag>` name→tag (`crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:149`). The oracle's map is **Tag-keyed with the name in the value** (`Globals.zig:165-175`) and on re-advertisement calls `onGlobalRemoved(tag)` + `old.proxy.destroy()`. rustty does neither → stale `name→tag` entry **and** a leaked superseded proxy. The "pathological input" divergence basis is false: criterion 2 misdescribes the oracle. | S |
| 4 | **`os_signpost` Instruments interval still a no-op** | `crates/rustty-bench/src/benchmark.rs:115-118` `#[cfg(target_os="macos")] { /* no-op */ }`, reference body preserved as comments `:93-114`. Its permanence basis was "the log-facade tier is deferred tree-wide" — **that is no longer true**: `macos.os.Log` is ported (`crates/rustty-os/src/macos.rs:220-262`). Sole hold-out. Oracle: `src/benchmark/Benchmark.zig:45-66` + `pkg/macos/os/signpost.zig`. | S |
| 5 | **`setNonblock(fd)` failure arm skips the drain** | `crates/rustty-termio/src/exec.rs:4129-4132` returns without `drain_pty_master_once` (helper exists at `:3395`), after `threadEnter` returned success and before the gather thread exists — the same wedge the 6 `threadEnter` arms were fixed for. **Deliberately not fixed here**: the oracle also just returns (`Exec.zig:1428`), so this extends an existing approved R1-DIVERGENCE rather than porting anything, and I have no test for the path. See §E. | XS |
| 6 | **iOS Darwin export gating** | `grep target_os = "ios"` in `crates/rustty-apprt-c/src/lib.rs` → 0 hits. The oracle gates on `isDarwin()` (macOS **or** iOS) and ships `macos/Sources/.../iOSApp.swift` + iOS arms in `apprt/embedded.zig`. `INTEGRATION_PARITY.md:394` calls the attach path "a separate deferred tier" with no owner. | S–M |
| 7 | **`renderer/Thread.zig` G3b comment drift** | Scoped G3b now reports **11 dropped oracle comments** vs `crates/rustty-core/src/renderer_seam.rs` (oracle `Thread.zig:27,560,617,641,647,680`, …). Newly *visible* only because this audit fixed the gate — see §D. | XS |
| 8 | **`MIRI_EXCEPTIONS.md` inventory incomplete** | 64 plain `cfg_attr(miri, ignore)` repo-wide; **12 in `rustty-cli`** (`list_themes.rs:2548`, `ssh_cache/disk_cache.rs:910,922,950`, …) and **3 in `rustty-extra`** (`mdgen.rs:228,299`, `webgen.rs:249`) have inline reasons but **no ledger entry**, which that file's own four-criterion rule (`:17`) requires. | XS |

### Medium (M) — 6

| # | Item | Evidence | Size |
|---|---|---|---|
| 9 | **`stream.zig` `Action` C-ABI union unported** | The only one of four `lib.TaggedUnion` projections not hand-authored (`Point.C` `rustty-vt-c/src/point.rs:103`, sgr `Attribute.C` `sgr.rs:168`, `ScrollViewport.C` `terminal.rs:1845` are all done). Commented out at `crates/rustty-vt/src/stream.rs:94-105`; **0 hits** for `ActionC` in `crates/`. The row's premise ("nothing references `stream.Action`") is false — oracle `src/terminal/c/terminal.zig:8` imports it, and `stream.zig:2783` has a live `test Action { _ = Action.C; }` the port excluded. ~95 variants; mechanical. | M |
| 10 | **`vt → xev` is a hand-rolled fake** | `crates/rustty-vt/src/search/thread.rs:1740` is a 200+-line in-crate `pub mod xev` faking `Loop`/`Async`/`Timer`/`Completion` over `Condvar`+`Instant` (`:1745-1790`), bound at `:140,146,152,158`. `crates/rustty-vt/Cargo.toml` has **zero** xev entries. Oracle `src/terminal/search.zig:10` deps the real libxev. | M |
| 11 | **macOS `xev.File`/`TCP`/`UDP` watchers** | Linux-only (`crates/rustty-xev/tests/surface_parity.rs:59-62,169-172`; `lib.rs:379,426-430` says "still deferred on macOS"). The oracle **has** the kqueue arms: libxev `watcher/file.zig:134,305`, `tcp.zig`, `udp.zig`, plus test `backend/kqueue.zig:2486`. Its permanence basis is a *reachability* argument — the exact argument the ledger already retracted for `iocp`/`wasi_poll`. | M |
| 12 | **libxev `c_api.zig`** | Zero `c_api` references in `crates/rustty-xev/src/`; oracle file is ~360 lines. Excused as "outside the consumption surface / ungateable" — the same retracted reachability argument, and criterion 3 (ungateable) is answered by its own criterion 4 (cbindgen + smoke KAT, exactly as `rustty-vt-c` does). | M |
| 13 | **uucode revision bump (`NON_PORTS.md:296` G1)** | `crates/rustty-unicode/src/generated/*_data.rs` pinned to the old uucode; oracle `build.zig.zon` is newer. Needs CI Zig pin 0.15.2→0.16 + regenerate + byte-compare. **No owner.** | M |
| 14 | **Windows termio spawn arms** | `crates/rustty-termio/src/exec.rs:278,497,691-694,746` keep the Windows `Subprocess.start` / `CancelIoEx` arms as comments only. (`ReadThread::threadMainWindows` **is** ported at `:4752`.) Oracle `src/termio/Exec.zig`. | M |

### Large (L) — 2

| # | Item | Evidence | Size |
|---|---|---|---|
| 15 | **vaxis 0.5.1→0.6.0 + zf 0.10.3→0.11.0 upstream delta** (`NON_PORTS.md:355` G2) | 4,402 + 847 diff lines of real upstream code: multi-cursor ctlseqs, `Mouse` u16→i16, zf `to_lower`→`case_sensitive` polarity flip. Filed as a "non-port"; under the scope rule it is **unported upstream code**. **No owner.** | L |
| 16 | **13 unported vxfw widgets** | `crates/rustty-vaxis/src/lib.rs:105-109` — "unused by ghostty (YAGNI) and out of scope". A reachability argument over vendored reference source, identical in form to the one voided at `D:6552`. Needs re-derivation against libvaxis `vxfw/`. | L |

### Verification-only (code present, host absent) — 3 groups

| # | Item | Note |
|---|---|---|
| 17 | **V2–V7, V19, V21(b)** | Real-GPU Linux box, KDE/KWin session, mutter, native X11, RTL sign-off, macOS 27. Code present in every case. |
| 18 | **V9–V12** (Windows termio / vaxis console / font discovery / flatpak) | **All four are DISCHARGED as ports** — the residual is execution only. Correctly no longer "out of scope by platform". |
| 19 | **V15–V18** perf / binary-size campaign | Needs a quiet box + Zig 0.16. `P3-W5-REVET.md`'s "finale perf re-measure not run" routes here. |

---

## D. Rows whose CLAIMED STATUS WAS WRONG (14)

### Claimed OPEN, actually DONE (11)

| Row | Claim | Reality |
|---|---|---|
| `D:6073` **O3** | `cursor_scroll_region_up` unported, BEYOND-PIN → Phase 3 | Ported `screen.rs:1885` + 6 tests + differential (`a8855af`) |
| `D:5998` **O5** | `PageList::Limits` has no counterpart → P3-RV1 | Ported `pagelist/mod.rs:1295` (`abc2b3d`) |
| `D:4459` | gtk `apprt-gtk`: "Rustty counterpart: **NONE** (zero counterpart — the single largest unported chunk)" | **53 oracle files ↔ 53 rustty files.** The only 4 without same-named counterparts are restructured, not missing: `build/blueprint.zig`→`crates/rustty-apprt/build.rs:173-242`; `ipc/{DBus,new_window,toggle_quick_terminal}.zig`→`gtk/app.rs:172,250,261` |
| `D:4790` | `main_gen`/`main_wasm`/`main_build_data` "NOT ported — DEFERRED" | All three ported: `rustty-synthetic/src/main_gen.rs`, `rustty-wasm` cdylib, `xtask/src/main_build_data.rs:49` |
| `D:3362` | `logFn` macOS unified-logging "preserved verbatim as comments" | **LIVE** — `crates/rustty-core/src/logging.rs:55-77` over real FFI (`rustty-os/src/macos.rs:233-239` + `cbits/log_shim.c`) |
| `D:3393`/`:3400` | `Action::help_text` "still returns \"\""; explain_config test deferred | `rustty-cli/src/rustty.rs:402` returns the real table; test live at `explain_config.rs:283`. C1/C2 copies were struck; the C6 copies never were |
| `D:3781` | xev LT8 DIVERGENCE #1 `Loop::init` arity | Un-done — `kqueue.rs:366 init(Options)` on both targets; pinned `surface_parity.rs:197` |
| `D:6996` | terminfo SOURCE + termcap "never emitted" | **Emitted unconditionally** — `xtask/src/res_terminfo.rs:123`→`:165`, asserted `:260` |
| `D:256` | `Key::imgui_key` is an `unimplemented!()` placeholder | Full mapping at `crates/rustty-vt/src/input/key.rs:839`, consumed at `gtk/class/imgui_widget.rs:375` |
| `D:477` | shadertoy `loadFromFiles`/`loadFromFile` DEFERRED | Live `shadertoy.rs:141,188`, called at `generic.rs:5273`; port even **added** 3 tests |
| `NON_PORTS.md:594` **G12** | 3 vicinae msgids are "deliberate non-ports, call sites don't exist" | Call sites landed (`gtk/class/application.rs:1595,1597,1604`, tests `:4631-4652`); **32 catalogs** carry the msgids |

### Claimed RESOLVED / permanent, actually OPEN (3)

| Row | Claim | Reality |
|---|---|---|
| `D:6829`/`:6864` | macOS `xev.File`/`TCP`/`UDP` + libxev `c_api.zig` = "permanent Justified R1 Divergence" | Both bases are *reachability* arguments — the identical argument the ledger **retracted** at `D:6552` for `iocp`/`wasi_poll`. The oracle has all four. Reclassified STILL OPEN (§C 11, 12) |
| `D:6692` | wayland duplicate-global = "permanent divergence, pathological input" | **Criterion 2 is factually false** about the oracle's map shape (`Globals.zig:165-175`); rustty also leaks the superseded proxy. STILL OPEN (§C 3) |
| `D:6937` | `os_signpost` = "permanent, the log-facade tier is deferred tree-wide" | The log-facade tier **landed** (`rustty-os/src/macos.rs:220-262`). Basis expired. STILL OPEN (§C 4) |

---

## E. NON-PORT — argued reasons that survive the scope rule (16, sampled)

Confirmed honest, each re-derived against the oracle:

* **`Backend::childExitedAbnormally`** (`crates/rustty-termio/src/backend.rs:180`) —
  oracle `backend.zig:88` dispatches to `exec.childExitedAbnormally`, which **does
  not exist** in `src/termio/Exec.zig`, and the method has no callers. Zig lazy
  compilation never type-checks it. Nothing to port.
* **xev `no_backend_seam`** (`crates/rustty-xev/src/lib.rs:625-699`, 10 leaves) —
  cfg-gated to targets that are none of macos/linux/windows/wasi. libxev's own
  `backend.zig` declares no backend there either. **PARITY.**
* **`TestTty::next_event`** (`crates/rustty-vaxis/src/tty.rs:1524`) — upstream's body
  is literally `return error.SkipZigTest;`. The panic **is** the port.
* **WebGL** — oracle `src/renderer/WebGL.zig` is **2 lines**; `crates/rustty-renderer/src/webgl.rs`
  (37 lines) already exceeds it.
* **`main_rustty.rs:299`** — statically unreachable on both supported hosts (macOS →
  banner + `exit(0)` at `:257`; Linux+gtk → `run_gtk -> !` at `:292/:333`). A
  configuration guard with no oracle counterpart.
* **`ghostty_*` alias layer** — disposition correct, **citation bogus**: `e4cc852`
  is not a valid object in the oracle, which still has **90** `export fn ghostty_`.
  The real reason is that rustty *rebranded* the Swift app
  (`macos/Sources/RusttyKit/.../SurfaceView_AppKit.swift:371` calls `rustty_surface_new`),
  so no unmodified-app compatibility surface is needed. **Row corrected.**
* Also confirmed: bounded `killPid`, ReadThread POLLHUP quirk (re-verified against
  oracle `Exec.zig:1720-1722`), xev ThreadPool shutdown wake, `Completion.task_wakeup`,
  local `detectArgs` replicas (G7 acyclicity), stb ±1 LSB, `include/rustty/vt.h`
  doc-only additions, `segmented_list.zig`, `quirks_memset.zig`.

**One non-port left deliberately unimplemented** (§C 5, `setNonblock`): the fix is
one line and the helper exists, but the oracle behaves identically, so landing it
*creates* a divergence rather than discharging one, on a path with no test. Recorded,
not half-done.

---

## F. Newly implemented in this pass (5)

Each mutation-proved where a behavioural assertion existed (inject fault → RED →
revert in place → GREEN; never `mv .bak`).

1. **`sgr::SepList` de-duplicated onto `parser::SepList`**
   (`crates/rustty-vt/src/sgr.rs`). The oracle has exactly **one** `SepList`
   (`sgr.zig:8` imports `Parser.Action.CSI.SepList`); rustty had two structurally
   identical copies, and `parser.rs:307-313`'s own PORT NOTE falsely claimed sgr
   already referenced the shared type. **Mutation proof:** faulting
   `parser::SepList::is_set` turned **12 sgr tests RED**; reverted → 31 GREEN.
   Full suite `rustty-vt --lib`: **2302 passed, 0 failed**.

2. **`check_comment_coverage.py` `--ref-files` vacuity hole closed** (G3b).
   The scope override passed paths to `comment_bodies()` verbatim, so a path
   relative to the oracle root — the natural spelling — never resolved;
   `comment_bodies` swallows the error, and the gate printed
   `distinctive ref comments=0 missing=0 … GREEN`, **exit 0**. A *nonexistent* file
   did the same. This is precisely the "full coverage against nothing" spoof hole
   the module's own `GHOSTTY_SRC` note (`:30-38`) says it refuses to have,
   reintroduced through the override. Now: relative entries resolve against
   `GHOSTTY_SRC`; a missing scope file or an empty `--port-glob` is **BLOCKED (2)**.
   **Proof:** bogus ref → exit 2; bogus glob → exit 2; real scoped run → exit **1
   (RED, 11 real missing comments)** where it previously returned a vacuous 0.
   Unscoped behaviour is untouched (G3b is a PROGRESS gate, already RED).
   *This gate fix is what exposed open item §C 7.*

3. **`THREAD_POOL_BODY_PORTED` → `MACOS_FILE_WATCHER_PORTED`**
   (`crates/rustty-xev/src/kqueue.rs`). The honest-skip marker named a blocker that
   no longer exists — the Zap work-stealing pool shipped in full
   (`thread_pool.rs`, 0 leaves) — so it read as "pool unported" to any grep. The
   sole real blocker is the macOS `xev.File` watcher. Const, prose and ledger-guard
   anchor all retargeted. **Mutation proof:** breaking the ledger anchor → RED;
   reverted → GREEN.

4. **zf path separator made target-native** (`crates/rustty-zf/src/filter.rs`).
   `SEP` was hardcoded `b'/'` with "Windows `'\\'` is out of scope". Upstream zf uses
   `const sep = std.fs.path.sep` (`zf/filter.zig:4`), which feeds
   `hasSeparator`/`PathIterator`/`isStartOfWord` — a Windows build scored path
   segments wrongly. Now `#[cfg(windows)] b'\\'`. 9 lib tests green.

5. **Four stale "still `todo!()`" claims corrected**
   (`crates/rustty-vt/src/terminal.rs`, `tests/d5_paired_invariants.rs`,
   `tests/d2_capacity_saturation.rs`, and the `xev/backend.rs` retracted-claim
   comment). `Colors::default()` is fully implemented at `terminal.rs:400`; two of
   the four comments were being used to *justify* duplicated test helpers. Also
   verified the deferrals those notes pointed at were genuinely delivered:
   `terminal_linefeed_mode_automatic_carriage_return` exists (`terminal.rs:11444`),
   and the erase cohort is **56 rustty tests vs 55 oracle**.

---

## G. Doc-rot inventory (no code owed, but actively misleading)

Every one of these reads as open work to a grep:

* `crates/rustty-apprt/src/embedded.rs:1305-1312,1327-1331` — asserts
  `Inspector::init` "is a Phase-3 dcimgui leaf (`unimplemented!()`), so this line
  panics". It is live at `:1947`; dcimgui is fully wired (13 real `ImGui_*` calls).
* `crates/rustty-renderer/src/thread.rs:4-12` — asserts the renderer event loop "is
  NOT ported here … deferred with D6". It was ported into
  `crates/rustty-core/src/renderer_seam.rs` (`threadMain` `:956`, `drainMailbox` `:1172`).
* `crates/rustty-font-shaper/` — 6 doc comments describing *former* `unimplemented!()`
  states (`deferred_face.rs:505`, `discovery.rs:87,2440`, `collection.rs:283`, …).
* `D:2808` — "runtime-live subset ≈15/71; GPU/xev/objc/coretext deferred to Phase-3".
  With the renderer/windowing tier landed the live subset is effectively **71/71** on
  macOS. The single most misleading line in the E-section.
* `VT_MIRI_SWEEP.md:14,47,53` still reads "9 real SB UB — fix pending; G6 lane RED"
  (fixed in `9e3fe1c`). `MAIN_PARITY.md:144-169` still carries ~15 "missing BL-1"
  rows against `crates/rustty-vt/src/compress/lz4.rs`, which exists.
* `VERIFICATION_DEBT.md` — **6 discharged rows (V1b, V13, V14, V20, V20a, V22)**
  still occupy 700+ lines against that file's own "delete when verified" rule (`:10`).
* `PHASE2_FINAL_AUDIT.md:284` **M9** — `check_phase2_closure.py`'s `LEDGERS` list
  still omits `VT_MIRI_SWEEP.md` and `DEFERRALS.swiftapp-misc.md`, so neither is
  gated at all.

---

## H. Verified-no-action (sampled, for auditability)

apprt-c exports **76 = 71 (`embedded.zig` parity) + 5 (`main_c.zig`)**, zero missing,
zero live leaves. Oracle CLI actions **18/18** present. `src/benchmark/**` **16/16**
(the ledger undercounted the oracle at 12). `src/extra/**` **5/5**. mdgen/webgen emit
for real (`xtask/src/res_misc.rs:149,172,193`). xcframework xtask real
(`xtask/src/main.rs:66,108,290`). W-ATTACH live (`renderer_seam.rs:338` →
`metal_impl.rs:448`, KAT un-ignored). wuffs swizzle 6/6, z2d 9/9 arms filled. Core
`surface.rs`: **0** `unimplemented!()`. Assessment docs A/B/C/D/G: every sampled
"ABSENT" row now present.

---

## I. Recommendations

1. **Add a completion predicate to the gate set.** G-P2 proves ownership; nothing
   proves *doneness*. A row asserting "X is unported" should carry a machine-checkable
   probe (the grep that made the claim), re-run by the gate, so a row that goes stale
   in the DONE direction turns RED. Both §B rows would have been caught the day
   they landed.
2. **Re-run every "beyond-pin" disposition after each pin advance.** The pin moved 365
   commits today; two rows were parked behind it and both were already ported. Pin
   advance must trigger a ledger sweep, not just a resync.
3. **Retire "reachability" as a permanence basis.** It has now produced four wrong
   dispositions (`iocp`/`wasi_poll` — retracted; macOS `xev.File`; libxev `c_api.zig`;
   the vxfw widgets). Under the standing scope rule "nothing consumes it" is not a
   reason not to port it.
4. **Own G1/G2 (`NON_PORTS.md`).** The vaxis/zf/uucode upstream deltas are the
   largest genuinely-unported bodies of oracle-supported code left, and both rows
   say "NO OWNER".
