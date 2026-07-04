export const meta = {
  name: 'cli-port',
  description: 'Port the CLI + bin milestone per the reviewed CLI_BLUEPRINT: C1 dispatcher, C2 pure config actions, C3 ssh subsystem, C4 apprt-backed+misc, C5 vaxis-deferred, C6 bin + the it-runs KAT. Sequential, per-chunk commits, halt on block.',
  phases: [
    { title: 'C1-dispatcher' }, { title: 'C2-config-actions' }, { title: 'C3-ssh' },
    { title: 'C4-apprt-misc' }, { title: 'C5-vaxis-deferred' }, { title: 'C6-bin' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['chunk', 'ok', 'commit', 'blockers', 'notes'],
  properties: {
    chunk: { type: 'string' }, ok: { type: 'boolean' }, commit: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' },
  },
}

const COMMON = [
  'OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R1 (every reference test ported; test COUNT is a contract), R3 (byte-faithful, every line and comment; byte-slices become and-u8 slices or str never lossy String), R2 (GHOSTTY/ghostty identity renames route through rustty-identity; user-facing CLI verb tags like list-fonts stay unchanged), R5 (never weaken/skip).',
  'FIRST read /Users/gael/projects/rustty/.port/phase-2/CLI_BLUEPRINT.md — the adversarially-reviewed design (ACCEPT-WITH-FIXES; honor every ADV-FIX n). You implement one chunk of its section-7 plan. Reference READ-ONLY at /Users/gael/projects/ghostty/src/cli/ + main.zig/main_ghostty.zig.',
  'LANDED BASE (all gate-green): rustty-args DONE (65t), rustty-apprt::none backend (perform_ipc returns Ok(false)), rustty-apprt-api::ipc value types, rustty-config (incl help_strings DEFERRED stub), rustty-font, rustty-vt, rustty-os, rustty-terminfo, rustty-crash, rustty-core (GlobalState is generic over Action; core STATE is Action=()). rustty-apprt-c ships the libghostty C-ABI.',
  'KEY ADV-FIXES: (1) version.zig is PARTIAL — renderer.Renderer + xev.backend lines get documented STUB strings until Phase-3; (2) action DETECTION lives in the rustty BIN (deps cli), NOT rustty_apprt::init (would cycle apprt->cli); the bin builds its OWN GlobalState<Action>; (3) explain_config.zig 1 test DEFERS on the unbuilt help_strings generator (SkipZigTest-style, body preserved, DEFERRALS.md-tracked); (4) cli also deps rustty-build-config; (5) R2-rename ssh/DiskCache TEST FIXTURES xterm-ghostty->xterm-rustty + DiskCache dir ghostty->rustty via rustty-identity; (6) re-home main_c(3t)->apprt-c, lib_vt(1t)->vt-c in crate_map. Vaxis/zf TUI paths are deferred placeholder leaves (plain/enumeration paths land now).',
  'GATES (FOREGROUND, ALL green before commit — never commit red): cargo build --workspace; cargo test on touched crates all pass; NO regression (spot-check rustty-vt 2019, rustty-core 15, rustty-apprt-api 20, rustty-apprt-c smoke, rustty-termio 44); cargo clippy --all-targets -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh GREEN (NO apprt->cli edge — detection is in the bin); python3 .port/tools/gates/check_test_counts.py GREEN (add/adjust MAP entries; the explain_config deferral + main_c/lib_vt re-home must keep it exact); scoped-G3b on your ported .zig files (missing=0). Update DEFERRALS.md for vaxis leaves + explain_config/help_strings + version stub lines.',
  'COMMIT only your files + gate/manifest edits. Trailer line: Co-Authored-By: Claude Opus 4.8 (1M context). If hard-blocked: commit what is cleanly green, report precisely. Add faithful sequencing stubs (exact reference signature + allow(dead_code)) for later-chunk helpers and say so.',
].join('\n')

const CHUNKS = [
  { phase: 'C1-dispatcher', prompt: 'CHUNK C1 (blueprint section 3 + 7): the dispatcher into crates/rustty-cli. Port cli/action.zig (7t — DetectError, SpecialCase, the Action trait + detect_iter over an args iterator; all 7 tests vs a local test enum) + cli/ghostty.zig Action enum (18 variants, EXACT tag order — ADV Risk 7) + detectSpecialCase + run/runMain dispatch + the 4 ghostty.zig tests + cli.zig re-export (1t) + help.zig (0t) + version.zig (0t, PARTIAL per ADV-FIX 1: renderer type-name + libxev backend become documented STUB strings; add rustty-build-config dep). Wire rustty-cli lib.rs. MAP entries cli_action/cli_ghostty/cli_root/cli_help/cli_version. Commit: "rustty: cli C1 — action dispatcher + ghostty Action enum + version(partial)/help"' },
  { phase: 'C2-config-actions', prompt: 'CHUNK C2 (blueprint section 7): the pure config/font/introspection actions into rustty-cli: show_config.zig (0t), validate_config.zig (0t), show_face.zig (0t), list_fonts.zig (0t), list_actions.zig (0t), Pager.zig (2t), and explain_config.zig — [ADV-FIX 3] port the file FULLY but DEFER its 1 test on the help_strings generator (SkipZigTest-style: keep the test body verbatim behind a documented gate that skips until help_strings lands; DEFERRALS.md entry; do NOT let it fail or fabricate help_strings). Each Options struct impls rustty_args::Target. MAP entries; scoped-G3b each. Commit: "rustty: cli C2 — pure config actions (show/validate/list) + Pager + explain_config(test deferred)"' },
  { phase: 'C3-ssh', prompt: 'CHUNK C3 (blueprint section 4): the ssh subsystem into rustty-cli as an ssh submodule tree — Entry.zig (3t) -> ssh-cache/DiskCache.zig (12t) -> ssh_cache.zig (5t) -> ssh.zig (10t) = 30 tests. Deps rustty-os (xdg/hostname/Child/ShellEscapeWriter) + rustty-terminfo (encode) + rustty-config (Duration). [ADV-FIX 5]: rename the xterm-ghostty TERM token -> xterm-rustty (consistent with rustty-terminfo entry name) and the DiskCache program dir ghostty -> rustty, ROUTED THROUGH rustty-identity, in BOTH runtime strings AND the ported test fixtures (DiskCache.zig:105/579/610-612, ssh.zig:255/271, Entry default) so the tests stay self-consistent. The disk/OOM tests use per-test tmp dirs (recall the parallel-init isolation lesson) + the rustty OOM-injection harness. MAP entries; scoped-G3b. Commit: "rustty: cli C3 — ssh subsystem (Entry/DiskCache/ssh_cache/ssh, 30 tests, xterm-rustty)"' },
  { phase: 'C4-apprt-misc', prompt: 'CHUNK C4 (blueprint section 1/7): the apprt-backed + misc actions into rustty-cli: crash_report.zig (0t, rustty-crash), edit_config.zig (0t, rustty-os exec), new_window.zig (0t) + toggle_quick_terminal.zig (0t) — the none-backed IPC actions: full arg-parsing (new_window parseManuallyHook: --class/--working-directory realpath+~ expansion/-e capture/cwd; toggle --class) LANDS; delivery over rustty_apprt::none::App::perform_ipc returns Ok(false) so both print "+<action> is not supported on this platform." to stderr + exit 1 (faithful, NOT a stub — ADV-FIX 4/Risk4) + cli/tui.zig seam (can_pretty_print, builtin.os.tag only). Add the rustty-apprt-api direct dep here (for ipc::{Target,Action,NewWindow}). Verify check_acyclic stays GREEN (cli->apprt/apprt-api legal; NO apprt->cli). MAP entries; scoped-G3b. Commit: "rustty: cli C4 — crash_report/edit_config + none-backed IPC actions + tui seam"' },
  { phase: 'C5-vaxis-deferred', prompt: 'CHUNK C5 (blueprint section 1/8): the vaxis/zf-touching actions into rustty-cli, plain paths LIVE + TUI paths deferred leaves. list_colors.zig (0t) + list_keybinds.zig (0t): port the --plain/non-tty paths fully (the only path the KAT/pipes hit); the prettyPrint vaxis fn is a documented deferred placeholder leaf. list_themes.zig (1741 loc, 0t — its own sub-batch, ADV Risk 2): port the theme ENUMERATION (on ported config/theme.zig) + ALL comments faithfully; the interactive vaxis+zf picker is a deferred leaf. boo.zig (0t): a pure-vaxis animation -> register as a vaxis-deferred placeholder leaf (comment-preserved). vaxis + zf are unported Zig deps (like xev/z2d) — DEFERRALS.md entry. Full scoped-G3b on each (missing=0 incl list_themes 1741 loc). Commit: "rustty: cli C5 — list_colors/keybinds/themes plain paths + vaxis-deferred TUI leaves"' },
  { phase: 'C6-bin', prompt: 'CHUNK C6 (blueprint section 5/6): the rustty BIN crate (crates/rustty). Port main.zig + main_ghostty.zig into the bin. [ADV-FIX 2] action detection lives HERE (bin deps cli): the bin runs cli::action::detect_iter::<cli::ghostty::Action> over os args, constructing the bin OWN rustty_global::GlobalState<Action> (NOT core STATE which is Action=()); on detected action -> exit(action.run(alloc)); for app_runtime none + no action -> print the "Usage: rustty +<action>..." banner + exit 0; the GUI else-branch (App init/run) is a documented Phase-3 unimplemented!() leaf (embedded/GTK + xev D6). apprt::init stays cli-free. logFn/std_options log setup; action path disables stderr logging. [ADV-FIX 6] re-home main_c.zig(3t)->apprt-c + lib_vt.zig(1t)->vt-c in check_test_counts.py crate_map (do NOT make bin dep apprt-c); bin owes main.zig 1t + main_ghostty.zig 1t. THE "IT RUNS" KAT (blueprint section 7): crates/rustty/tests/cli_smoke.rs spawning env CARGO_BIN_EXE_rustty via std::process::Command: +version exit0 stdout starts "rustty " (ADV-FIX 1: do NOT assert renderer/libxev line content); --version exit0; +list-actions exit0 non-empty; +validate-config exit0 on valid tmp config / exit1+diagnostic on broken; +ssh-cache with XDG_STATE_HOME tmp -> exit0 + --add shows entry; +new-window exit1 + stderr "not supported on this platform"; no-args exit0 + usage banner; +bogus exit1 + unknown-action msg. Update NEXT_PHASE_PLAN.md with the CLI+bin DONE checkpoint (headless product runs; GUI launch = Phase-3). Commit: "rustty: cli C6 — bin (action dispatch + usage banner + Phase-3 GUI leaf) + it-runs smoke KAT"' },
]

const results = []
for (const c of CHUNKS) {
  phase(c.phase)
  let r
  try {
    r = await agent(COMMON + '\n\n' + c.prompt + '\n\nReturn the structured result (chunk, ok, commit, blockers, notes).',
      { label: c.phase, phase: c.phase, schema: RESULT_SCHEMA })
  } catch (e) {
    r = { chunk: c.phase, ok: false, commit: '', blockers: ['threw: ' + String(e)], notes: '' }
  }
  if (!r) r = { chunk: c.phase, ok: false, commit: '', blockers: ['agent returned null'], notes: '' }
  results.push(r)
  log(c.phase + ': ' + (r.ok ? 'OK ' + r.commit : 'BLOCKED — halting'))
  if (!r.ok) break
}
return { results }
