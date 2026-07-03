# CLI + bin milestone — implementation blueprint

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-02) — folded in below** (marked
`[ADV-FIX n]`). Leaves-first chunking survives; three backing gaps are now honestly sequenced:
- **[ADV-FIX 1] `version.zig` is PARTIAL, not pure.** It unconditionally reads `renderer.Renderer`
  (a Phase-3 leaf — commented out in rustty-renderer/lib.rs:68) and `xev.backend` (unimplemented
  in rustty-xev). Those two lines get documented STUB values (renderer type-name + libxev backend
  string) until R-d/D6 land; the `+version` KAT must NOT assert the renderer/libxev line content.
  0-test file → no contract breakage.
- **[ADV-FIX 2] Action detection moves to the BIN, not apprt (fixes an `apprt→cli` CYCLE).** §5/§6
  proposed extending `rustty_apprt::init()` to call `cli::action::detect_iter` — but cli deps
  apprt, so that back-edge cycles (check_acyclic RED), and core's `GlobalState`/`STATE` is bound
  `Action=()` and cannot hold a cli action. FIX: detection lives in the `rustty` bin (member 27,
  legitimately deps cli): the bin calls `cli::action::detect_iter::<cli::ghostty::Action>` over
  `os::args` and dispatches locally; if reference `state.action` storage is wanted, the bin
  instantiates its OWN `GlobalState<cli::ghostty::Action>` (NOT `rustty_core::STATE`).
  `rustty_apprt::init` stays cli-free.
- **[ADV-FIX 3] `explain_config`'s 1 test DEFERS on the unbuilt `help_strings` generator.** Its
  test needs `help_strings.Config."font-size"` / `.KeybindAction.copy_to_clipboard` — the
  build-generated doc tables, which are a deferred stub in rustty (config formatter_file.rs:142,
  input helpgen_actions NoHelpStrings no-op). So the cli contract is **44 tests now + 1 deferred**
  (explain_config, SkipZigTest-style, verbatim body preserved, DEFERRALS.md-tracked, un-deferred
  when a `help_strings` codegen follow-up lands — which also enables +help / show_config --docs /
  list_keybinds --docs, all 0-test). Do NOT claim explain_config lands now.
- **[ADV-FIX 4] cli dep set** also needs `rustty-build-config` (version/show_config/help read it);
  version's renderer/xev lines are STUBBED (no renderer/xev dep needed).
- **[ADV-FIX 5] R2 rename touches ssh/DiskCache TEST FIXTURES** (`xterm-ghostty`→`xterm-rustty` in
  DiskCache.zig:105/579/610-612, ssh.zig:255/271, Entry default; DiskCache program dir
  `"ghostty"`→`"rustty"`) — rename consistently via rustty-identity or the ssh-cache/terminfo tests
  diverge.
- **[ADV-FIX 6] bin test accounting:** re-home `main_c.zig` (3t)→apprt-c and `lib_vt.zig`
  (1t)→vt-c in the crate_map (option b) — keeps "bin does NOT dep apprt-c" true; bin = main 1 +
  main_ghostty 1 = 2t.

Scope: port `src/cli/` (dispatcher + action impls + ssh subsystem) into
`rustty-cli`, and `src/main.zig` + `main_ghostty.zig` (+ the main_c.zig test
tail) into the `rustty` bin. Grounded in the reference read of action.zig,
ghostty.zig, every action impl, the ssh subsystem, main.zig/main_ghostty.zig,
global.zig, and the landed crates (rustty-args, rustty-apprt::none,
rustty-apprt-api::ipc, rustty-config, rustty-core::STATE/GlobalState).
`args.zig`/`CommaSplitter.zig`/`diagnostics.zig` are already the separate
`rustty-args` crate (64t, DONE) and are NOT in this milestone's contract.

Authoritative test contract (crate_map, phase-0/inventory/tests-summary.json):
- **`cli` crate = 45 test blocks across 25 files.**
- **`bin` crate = 6 test blocks across 7 files** (lib_vt 1, main 1, main_c 3,
  main_ghostty 1, main_build_data/main_gen/main_wasm 0).

---

## 1. HEADLESS-NOW vs RUNTIME-DEFERRED split

The decisive finding: **every one of the 45 cli test blocks lives in a file that
is fully portable NOW.** All vaxis/zf-blocked code sits in 0-test files or 0-test
branches, so the crate reaches 45/45 without a live app, GUI, or TUI library.

### LANDS NOW — full logic + all tests (pure config/font/vt/os/crash introspection)
Backed only by already-ported crates. All parse via the `rustty_args::Target`
trait + `args_iterator()`.

| file | t | backing (ported) |
|---|---|---|
| action.zig | 7 | self (detect/dispatch) |
| ghostty.zig | 4 | self (Action enum + detectSpecialCase) |
| Pager.zig | 2 | rustty-os (getenv, Child) — tests are non-tty/default-writer |
| explain_config.zig | 1→**DEFER** | needs `help_strings` generator (unbuilt) — [ADV-FIX 3], test defers |
| version.zig | 0 (**PARTIAL**) | rustty-build-config; renderer/xev lines STUBBED — [ADV-FIX 1/4] |
| help.zig | 0 | self (enum field iteration) |
| show_config.zig | 0 | rustty-config (Config.load/default + FileFormatter) + Pager |
| validate_config.zig | 0 | rustty-config (load/loadFile/finalize + diagnostics) |
| show_face.zig | 0 | rustty-font |
| list_fonts.zig | 0 | rustty-font (discovery) |
| list_actions.zig | 0 | rustty-input (helpgen_actions.generate → plaintext) |
| crash_report.zig | 0 | rustty-crash (defaultDir, Report iterator) — ported (12t) |
| edit_config.zig | 0 | rustty-config + rustty-os (getenv, execvpeZ) — see §1 note |
| ssh.zig | 10 | rustty-os + rustty-terminfo (see §4) |
| ssh_cache.zig | 5 | rustty-config (Duration) + DiskCache |
| ssh-cache/DiskCache.zig | 12 | rustty-os (xdg, hostname) |
| ssh-cache/Entry.zig | 3 | self (parse/format) |

`edit_config` and `ssh`/`ssh-cache` **spawn/exec real processes at runtime** but
that is I/O the harness invokes, not a dependency on a running Ghostty; their
parsing + cache + resolution logic is pure and the tests never spawn a terminal.

### LANDS NOW — but none-backed (IPC delivery deferred, honest exit-1)
`new_window` + `toggle_quick_terminal` call `apprt.App.performIpc(...)`. The
headless runtime is `none`, whose `perform_ipc` (already ported,
`rustty_apprt::none::App::perform_ipc`) **returns `Ok(false)`**. Faithful
behavior over `none`: both actions print `"+<action> is not supported on this
platform."` to stderr and exit 1. **The full arg-parsing surface ports now** —
`new_window`'s `parseManuallyHook` (`--class`, `--working-directory` realpath +
`~` expansion via rustty-os homedir, `-e` command capture, cwd injection) and
`toggle_quick_terminal`'s `--class`. Only the *delivery* is deferred (real IPC
arrives with a live GTK/embedded backend). 0 tests each → no contract risk.

### RUNTIME-DEFERRED — needs an unported Zig TUI library (vaxis / zf)
`vaxis` (TTY UI) and `zf` (fuzzy match) are **Zig-source dependencies, not
ghostty src and not C libs** — the exact class as libxev (D6) and z2d. They are
not yet ported. The code that needs them (all 0-test):
- **boo.zig** (0t) — a pure vaxis animation. 100% TUI. Defer as a placeholder
  leaf (like GTK/xev); no test contract impact.
- **list_themes.zig** (1741 loc, 0t) — default output is an interactive
  vaxis+zf fuzzy finder. Its *theme enumeration* rests on `config/theme.zig`
  (ported); port the plain/non-interactive listing now and defer the interactive
  picker behind the vaxis leaf.
- **pretty-print branches of list_colors.zig / list_keybinds.zig** (0t) — each
  already gates the vaxis path behind `tui.can_pretty_print && !plain &&
  isatty`. Port the `--plain`/non-tty path now (fully functional, and the only
  path the KAT and any redirected pipe ever hit); the `prettyPrint` fn is the
  deferred vaxis leaf.

**Seam:** port `cli/tui.zig` (`can_pretty_print`, which depends only on
`builtin.os.tag`) faithfully; keep the vaxis-calling functions behind a
`#[cfg]`/feature or a `vaxis`-deferred module so the plain paths compile and run
today. This is the same "deferred placeholder leaf" pattern the plan already
uses for GPU/GTK/xev (accept no unit gate until the dep lands).

---

## 2. Crate placement + acyclicity

`rustty-cli` is member 26; deps already declared in its Cargo.toml:
`rustty-config, rustty-vt, rustty-font, rustty-input, rustty-os,
rustty-terminfo, rustty-args, rustty-apprt, rustty-core, rustty-crash`. All are
strictly-earlier members → **acyclic** (cli depends on nothing that depends on
cli; apprt=25, core=24, config=21 all precede it). `cargo_cycle_check.py` /
`check_acyclic.sh` stay green.

- **cli needs `rustty-apprt` (the backend), not only apprt-api.** `new_window`/
  `toggle_quick_terminal` call `apprt.App.performIpc` — the concrete backend
  entry — which for headless resolves to `rustty_apprt::none::App::perform_ipc`.
  apprt-api only carries the `ipc::{Target, Action, NewWindow}` *value types*.
- **Add `rustty-apprt-api` as a direct cli dep.** cli must name
  `ipc::Target::Detect`, `ipc::Action::NewWindow(NewWindow{..})`,
  `ipc::ToggleQuickTerminal` to build the performIpc args; `rustty-apprt` does
  not re-export apprt-api. One-line Cargo.toml addition (still acyclic; apprt-api
  is member 16).
- **The `rustty` bin** (member 27) deps: `rustty-cli, rustty-apprt,
  rustty-core` (as already declared). **It does NOT need `rustty-apprt-c`.**
  apprt-c is the libghostty cdylib/staticlib top-sink that *nothing* depends on;
  main_c.zig compiles into the *lib* artifact, not the `rustty` exe. (See §5/§6
  for the main_c.zig test-accounting nuance.)

---

## 3. The action dispatcher (action.zig + ghostty.zig)

`action.zig` is generic over `comptime E` (the action enum). Port as:
- `DetectError { MultipleActions, InvalidAction }` → a Rust enum.
- `SpecialCase(E)` union → `enum SpecialCase<E> { Action(E), Fallback(E),
  AbortIfNoAction }`.
- `detectIter` is generic over "an enum with an optional `detectSpecialCase`
  decl." Port as a **trait** `Action: Sized + Copy` with
  `fn from_plus_name(&[u8]) -> Option<Self>` (the `stringToEnum(arg[1..])`) and
  an optional `fn detect_special_case(&[u8]) -> Option<SpecialCase<Self>>`
  (default `None`). `detect_iter<A: Action, I: Iterator<Item=Vec<u8>>>` walks
  args: special-case first, then `+`-prefixed → `from_plus_name` (None →
  `InvalidAction`), `pending != None` → `MultipleActions`; return
  pending ?? fallback ?? None. All 7 action.zig tests port verbatim against a
  local test enum implementing the trait.
- `ghostty.zig`'s `Action` enum (18 variants; keep exact tag order for parity)
  implements the trait. `detect_special_case`: `-e` → `AbortIfNoAction`,
  `--version` → `Action(Version)`, `--help`/`-h` → `Fallback(Help)`.
  `from_plus_name` maps kebab tag names (`list-fonts` etc.) — a hand-written
  `match` (Zig's `stringToEnum` over `@tagName`; the R2 rename keeps the same
  kebab tags, they are *user-facing CLI verbs*, unchanged by GHOSTTY→rustty).
- `run`/`runMain`: `runMain` is the `match self { Version => version::run(),
  ... }` dispatch returning `Result<u8, _>`. `run` wraps it, catching the
  `help_error` sentinel (`error.ActionHelpRequested`) to print the action's
  help string. In Zig this uses comptime `help_strings.Action.<field>`;
  port as a `help_text(self) -> &'static str` match populated from the
  ported help-strings (or a `helpgen` build step) — mechanical, 0 new tests.
- The 4 ghostty.zig tests (parse action none/version/plus/plus-ignores-`-e`)
  port against `detect_iter::<Action>`.

Each action impl's `Options` struct becomes a `#[derive]`-free struct
implementing `rustty_args::Target` (as landed action impls already do), parsed
via `args::parse(&mut opts, &mut iter)`.

---

## 4. The SSH subsystem (ssh + ssh_cache + DiskCache + Entry = 30 tests)

Self-contained; port as a `ssh` submodule tree under rustty-cli. Corrected
counts (task under-counted): **ssh.zig 10, ssh_cache.zig 5, DiskCache.zig 12,
Entry.zig 3 = 30 tests, 137 assertions.**

Deps (all ported): `rustty-os` (xdg.state, hostname.isValid, randomTmpPath,
ShellEscapeWriter, std process Child/run), `rustty-terminfo`
(`ghostty.encode` for the remote install payload), `rustty-config`
(`Config.Duration` for `--prune` + relative-age formatting).

- **Entry.zig** — `parse` (`host|ts|terminfo`, default `xterm-ghostty`) +
  `format`. Byte-faithful `[]const u8` → `&[u8]`/`Vec<u8>` (R3, never lossy
  String). 3 tests port directly.
- **DiskCache.zig** — the file store: `defaultPath`
  (`$XDG_STATE_HOME/ghostty/ssh_cache`), `add/remove/prune/contains/clear/list`,
  atomic-write + `0o600` + `tryLock`, `isValidCacheKey/isValidHost/isValidUser`.
  12 tests; note **3 use `std.testing.tmpDir` + `FailingAllocator`** — port over
  a tempdir (`tempfile` crate or a `TMPDIR` scratch) and rustty's OOM-injection
  test harness. The R2 rename: the on-disk program dir stays `ghostty`? **No —
  the cache subdir/program name is a rustty identity string; rename to `rustty`
  via rustty-identity** (it is a product path, not a wire/user token). Flag for
  the identity gate.
- **ssh_cache.zig** — the `+ssh-cache` action (list/add/remove/prune/query,
  `--host=` deprecation, `matchesQuery`, `formatTimestamp`, `relativeAge`).
  `runInner` is split out and unit-tested (2 of the 5) with in-memory writers —
  **no disk** — so they port cleanly. 5 tests.
- **ssh.zig** — the `+ssh` wrapper action: `Options.parseManuallyHook`
  (`--`/bare-arg → pass-through to ssh argv), `resolveDestination`
  (`ssh -G` parse), `installRemoteTerminfo` (ControlMaster + terminfo encode),
  `childExec` (128+signum). 10 tests are all pure parse tests (parseManuallyHook
  ×3, parseDestination ×6, the `Joined` formatter ×1) — no network. Runtime
  ssh/terminfo install is spawn-based, invoked only in real use.

---

## 5. The bin (`rustty` crate): main.zig + main_ghostty.zig

Dispatch flow (faithful to main.zig → main_ghostty.zig → global.zig):
1. `global.zig state.init()` detects the action **first** via
   `cli.action.detectArgs(Action)`. **[ADV-FIX 2] This lives in the `rustty` BIN,
   NOT `rustty_apprt::init` (which deps below cli — calling cli would cycle
   `apprt→cli`, and `rustty_core::STATE` is bound `Action=()`).** The bin calls
   `cli::action::detect_iter::<cli::ghostty::Action>` over `os::args`; if the
   reference `state.action` storage is wanted, the bin constructs its OWN
   `rustty_global::GlobalState<cli::ghostty::Action>` (generic over Action) rather
   than reusing core's `Action=()` STATE. `rustty_apprt::init` stays cli-free.
   The init error mapping (MultipleActions / InvalidAction → the specific stderr
   messages, else generic) ports from main.zig's `state.init() catch`.
2. If `state.action` is set → `exit(action.run(alloc))`. **This is the whole
   headless product and it works for real now.**
3. Else, for `app_runtime == .none` (the headless build) → print the
   "Usage: rustty +<action>…" banner and `exit(0)` (main_ghostty.zig lines
   74–94). **This is the honest terminal behavior of the headless bin.**
4. The `else` branch (create `App`, `apprt.App.init/run` the GUI event loop) is
   the **Phase-3 leaf**: it hits the embedded/GTK backend + xev (D6). Port as a
   documented `unimplemented!()`/deferred call, exactly as the plan defers GPU/
   GTK/xev.

**Honest framing:** `rustty --version`, `rustty +version`, `rustty +list-fonts`,
`rustty +validate-config`, `rustty +show-config`, `rustty +list-actions`,
`rustty +ssh-cache`, `rustty +ssh …` **all run for real.** `rustty` with no
action **prints the headless usage banner and exits** (it does not, and is not
meant to, open a window in the headless build). `rustty` launching a GUI window
is Phase-3 (needs a live apprt backend + xev). This mirrors upstream: the CLI
helper binary is fully functional headless; the windowed app is the GUI build.

`logFn`/`std_options` (macOS unified logging + stderr) port as the bin's log
setup; the action path disables stderr logging (global.zig line 107) — keep that
so action stdout stays clean.

---

## 6. Test accounting

**`cli` crate → 44/45 now + 1 deferred [ADV-FIX 3].** Test-bearing files:
action 7, ghostty 4, Pager 2, **explain_config 1 (DEFERRED on help_strings)**,
cli.zig 1, ssh 10, ssh_cache 5, DiskCache 12, Entry 3 = **45** (44 land now; the
explain_config test is a documented SkipZigTest-style deferral with its body
preserved verbatim, un-deferred by the help_strings codegen follow-up). Every 0-test action impl (boo, crash_report,
edit_config, help, list_*, new_window, show_config, show_face,
toggle_quick_terminal, tui, validate_config, version) is **G3b-gated on
comments**, not tests — port each with full comment fidelity per the
per-sub-batch scoped-G3b rule.

- The vaxis-deferred files (boo, list_themes interactive picker) carry **0
  tests** → deferring their render path costs nothing against the 45. But **G3b
  comment coverage still applies**: either port list_themes' 1741 loc faithfully
  now (enumeration real, picker stubbed with comments intact) or register boo/
  list_themes-picker as **documented placeholder leaves** exempt from the gate
  until vaxis lands (GPU/GTK/xev precedent). Recommend: port list_themes'
  enumeration + comments now; register `boo` as a vaxis-deferred leaf.

**`bin` crate → 6 test blocks, 7 files.** Nuance: `main_c.zig` (3t) is
**crate_map-assigned to `bin`**, but its libghostty entry is functionally
already covered by `rustty-apprt-c::rustty_init`. Resolve one of two ways
(decide with the gate owner): (a) port main_c.zig's 3 tests into the `rustty`
bin as a thin libghostty-entry module that re-exports apprt-c, or (b) amend
crate_map to re-home main_c.zig → `apprt-c`. `lib_vt.zig` (1t) similarly is the
libghostty-vt entry (vt-c precedent). `main.zig` (1t, `test { _ = entrypoint; }`)
and `main_ghostty.zig` (1t, the big `refAllDecls`-style `test {…}`) port as
module-existence tests over the bin's real imports. main_build_data/main_gen/
main_wasm are 0t (gen/wasm entrypoints — deferred stubs with comments).

No cli test needs a live app: the two IPC actions and edit_config are 0-test.

---

## 7. Chunked execution plan + the "it runs" KAT

**Chunks (leaves-first inside cli, each gate-green + scoped-G3b before next):**
1. **C1 dispatcher** — action.zig (7t) + ghostty.zig Action enum/detect (4t) +
   help.zig + version.zig + cli.zig re-export. Unblocks everything.
2. **C2 pure config actions** — show_config, validate_config, explain_config
   (1t), show_face, list_fonts, list_actions + Pager (2t). Backing all ported.
3. **C3 ssh subsystem** — Entry (3t) → DiskCache (12t) → ssh_cache (5t) → ssh
   (10t). Self-contained sub-tree; the disk/OOM tests need the tempdir harness.
4. **C4 apprt-backed + misc** — crash_report, edit_config, new_window,
   toggle_quick_terminal (none-backed, exit-1), tui.zig seam. Add the
   `rustty-apprt-api` direct dep here.
5. **C5 vaxis-deferred** — list_colors/list_keybinds plain paths (land) + defer
   pretty; list_themes enumeration (land) + defer picker; boo → deferred leaf.
6. **C6 bin** — **[ADV-FIX 2]** action detection lives IN THE BIN (bin deps cli;
   apprt stays cli-free), constructing the bin's own `GlobalState<Action>`; wire
   main.rs dispatch (action.run → exit; none → usage banner; GUI → Phase-3
   leaf); port the bin test tail (§6). **[ADV-FIX 6]** re-home main_c(3t)→apprt-c,
   lib_vt(1t)→vt-c in crate_map; bin owes main 1 + main_ghostty 1 = 2t.

**KAT — the headless "it runs" proof.** Add `crates/rustty/tests/cli_smoke.rs`,
an integration test that spawns the **built binary** via
`env!("CARGO_BIN_EXE_rustty")` (Cargo sets this for `tests/`) with
`std::process::Command`, asserting real end-to-end behavior:
- `rustty +version` → exit 0, stdout starts `rustty `. **[ADV-FIX 1]** do NOT
  assert the renderer/libxev line CONTENT (those are stubbed until R-d/D6); assert
  only exit 0 + the version/build lines that are real.
- `rustty --version` → exit 0 (special-case, same output as `+version`).
- `rustty +list-actions` → exit 0, non-empty stdout (keybind action list).
- `rustty +validate-config` → exit 0 on a valid tmp `--config-file=…`;
  exit 1 + a diagnostic line on a deliberately-broken config file.
- `rustty +ssh-cache` (with `XDG_STATE_HOME` pointed at a tmp dir) → exit 0,
  and after `--add=user@example.com` the listing shows the entry.
- `rustty +new-window` → exit 1 + stderr `not supported on this platform`
  (faithful none-backend behavior — proves the IPC seam is wired, not stubbed).
- `rustty` (no args) → exit 0 + the headless usage banner.
- `rustty +bogus` → exit 1 + the `unknown CLI action` message.

This is a bin-crate integration KAT (not a unit test in the 45/6 contract); it
is the deliverables proof that the headless product actually executes.

---

## 8. Risks

1. **vaxis/zf are unported Zig deps.** boo, list_themes' picker, and the
   list_colors/list_keybinds pretty branches need them. Mitigation: port the
   plain/enumeration paths now (all the KAT and pipe use hits these); register
   the TUI render paths as vaxis-deferred placeholder leaves (GPU/GTK/xev
   precedent). No test-contract impact (those code paths are 0-test).
2. **list_themes.zig = 1741 loc, 0 tests.** Biggest single file, heavy TUI, but
   G3b comment coverage still owed. Split: enumeration (real, on ported
   config/theme.zig) vs interactive picker (deferred). Budget it as its own
   sub-batch with scoped-G3b; do not fold into a finale.
3. **R2 identity rename surface.** version.zig/help.zig user-facing strings
   (`Ghostty`→`rustty`, the commit-URL host), the crash_report ghost emoji line,
   and — subtly — the **ssh terminfo term name `xterm-ghostty`** and the
   **DiskCache program dir `ghostty`**. The `xterm-ghostty` TERM value and
   `SetEnv=TERM=…` and the remote `infocmp xterm-ghostty`/`tic` script are a
   **wire/interop token tied to the installed terminfo entry**; renaming it must
   stay consistent with rustty-terminfo's actual entry name (whatever
   rustty-identity renamed it to). The cache *dir* is a local product path →
   rename to `rustty`. Get both right or ssh-cache tests and terminfo install
   diverge. Run through rustty-identity, do not hand-edit.
4. **IPC/GUI boundary honesty.** new_window/toggle_quick_terminal exit 1
   "not supported" over `none` — this is *correct*, not a stub, and must be
   documented as such so it is not mistaken for incomplete work. Real IPC is
   gated on a live backend (Phase-3).
5. **ssh disk-cache file I/O in tests.** 3 DiskCache tests + the "cleans up temp
   files"/prune tests need real tmp dirs; 2 use `FailingAllocator` OOM injection
   (double-free/leak detection). Requires the rustty tempdir + OOM-injection
   test harness to be available; parallel `cargo test` must not share a fixed
   cache path (use per-test tmp dirs; recall the onig_initialize parallel-init
   lesson — isolate global/file state).
6. **main_c.zig test-accounting** (§6): 3 tests contract-assigned to `bin` but
   functionally in apprt-c. Reconcile with the gate owner (re-home vs
   crate_map amendment) so check_test_counts stays exact — do not silently drop.
7. **Action enum tag order** must match the reference exactly (help/completions/
   C-enum parity, same discipline as apprt-api's ipc ActionKey). Keep the 18
   variants in source order.
