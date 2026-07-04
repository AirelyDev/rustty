export const meta = {
  name: 'cli-design',
  description: 'Design blueprint for the CLI + bin milestone (rustty-cli action dispatcher + action impls + ssh subsystem + the main dispatch bin) plus adversarial verification.',
  phases: [ { title: 'Design' }, { title: 'Verify' } ],
}

const CONTEXT = [
  'Project: rustty, a faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test-count contract; R3 byte-faithful every line and comment, byte-slices become and-u8 slices or str never lossy String; R2 identity rename GHOSTTY/ghostty to rustty via rustty-identity; R5 never weaken). Gates: check_acyclic.sh, check_test_counts.py, scoped-G3b.',
  '',
  'MILESTONE STATE (all committed, gate-green): the ENTIRE terminal and app tier is ported: vt (2019t), vt-c (308t), config (190t incl the rustty_config C-ABI), font stack, input, os, renderer headless, crash (12t), termio (44t), apprt-api (20t), rustty-core (15t, the generic App/Surface backend seam), AND the libghostty C-ABI (rustty-apprt-c: 71 rustty_ exports plus the embedded backend, C-smoke-KAT green). none.zig (the headless/IPC backend) is ported in rustty-apprt. rustty-args is DONE (65t: args.zig 36t plus CommaSplitter 28t plus diagnostics). rustty-cli is a STUB. rustty (the bin crate) is a STUB. D6 (the xev event loop), GPU R-d, GTK, and inspector-UI are Phase-3 deferred placeholder leaves.',
  '',
  'REFERENCE SCOPE (src/cli/, 9096 loc, ~102 test-blocks; args/CommaSplitter/diagnostics already ported):',
  '- Dispatcher and entry: cli/action.zig (277 loc, 7t, the +action registry and dispatch), cli/ghostty.zig (320 loc, 4t, the CLI main, R2-rename to rustty.zig), cli/help.zig (78, 0t), cli/version.zig (76, 0t), cli/Pager.zig (93, 2t), cli/explain_config.zig (151, 1t).',
  '- Action impls (mostly 0-test, G3b-gated): list_fonts (169), list_themes (1741, big), list_keybinds (435), list_colors (209), list_actions, show_config (92), show_face (239), validate_config (78), edit_config (180), crash_report (93), boo (236), new_window (231), toggle_quick_terminal (62).',
  '- SSH subsystem: ssh.zig (635, 9t), ssh_cache.zig (488, 1t), ssh-cache/DiskCache.zig (747, 9t), ssh-cache/Entry.zig (101, 3t).',
  '- The bin: main.zig (26), main_ghostty.zig (213), which dispatch CLI-vs-app; main_c.zig (247) is the libghostty entry (already partly covered by rustty-apprt-c rustty_init).',
].join('\n')

phase('Design')
const blueprint = await agent(CONTEXT + '\n\n' + [
  'You are the ARCHITECT. Produce a CONCISE implementation blueprint (write it to /Users/gael/projects/rustty/.port/phase-2/CLI_BLUEPRINT.md, do NOT commit; also return it as your final text) for the CLI + bin milestone. Ground EVERY claim by READING the reference (cli/action.zig dispatch, the action impls, the ssh subsystem, main.zig and main_ghostty.zig) and the landed crates (rustty-args, the rustty-apprt none backend, rustty-config, rustty-identity). Cover:',
  '1. HEADLESS-NOW vs RUNTIME-DEFERRED split. Which cli actions are FULLY portable and testable NOW over the none backend plus already-ported libs (list_fonts/list_themes/list_colors/list_keybinds/list_actions/show_config/show_face/validate_config/explain_config/version/help, i.e. pure config/font/vt introspection) vs which need a LIVE app/GUI/IPC (new_window and toggle_quick_terminal send IPC to a running instance; edit_config spawns an editor; boo is an animation) and therefore become documented deferrals or none/IPC-backed. Give the honest split and which land now.',
  '2. Crate placement + acyclicity. rustty-cli deps (config, font, vt, os, args, apprt/apprt-api for the none backend + IPC, terminfo, input for keybind listing). Confirm acyclic. Does cli need rustty-apprt (the backends) or just apprt-api? The bin (rustty crate) deps cli + apprt + apprt-c.',
  '3. The action dispatcher (action.zig): how the +action registry/enum + runAction dispatch ports (the comptime action enum becomes a Rust enum; arg parsing via the rustty-args Target trait).',
  '4. The SSH subsystem (ssh + ssh_cache + DiskCache + Entry, ~22 tests): self-contained? deps (os, disk cache). Port as a cli submodule.',
  '5. The bin (rustty crate): main.zig + main_ghostty.zig, the CLI-vs-app dispatch (argv[0] and +action detection). What can the bin DO now: headless CLI actions run for real; launching the GUI app hits the Phase-3 embedded/GTK + xev (D6) leaves. Give the honest framing (rustty --version and rustty +list-fonts WORK; rustty launching a window is Phase-3).',
  '6. Test accounting: the ~36 test-bearing files (action 7, ghostty 4, Pager 2, explain_config 1, ssh 9, ssh_cache 1, DiskCache 9, Entry 3) plus the 0-test G3b-gated action impls. MAP entries. Any test needing a live app defers.',
  '7. Chunked execution plan + a real "it runs" KAT (an integration test that runs the built rustty binary as a subprocess for +version / +validate-config / +list-actions and checks output, the headless deliverables proof).',
  '8. Risks (list_themes 1741 loc; the R2 rename surface; the IPC/GUI boundary honesty; ssh disk-cache file IO in tests).',
  'Be decisive and tight.',
].join('\n'), { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(CONTEXT + '\n\n' + [
  'You are the ADVERSARIAL REVIEWER. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/CLI_BLUEPRINT.md (read it). Try to BREAK it against the actual reference and repo; verify independently:',
  '1. Headless-now honesty: does each action the blueprint calls "portable now" REALLY avoid the live app/GUI/xev? grep each action impl for apprt App/Surface/IPC/renderer/xev usage. Any "portable now" that actually needs a running instance?',
  '2. Cycle traps: does the proposed rustty-cli / rustty-bin dep set create a cycle? (cli to apprt to core to apprt-api; does anything cli deps also dep cli?)',
  '3. Test count: grep each cli file for test blocks yourself; verify the blueprint per-file counts and that args/CommaSplitter/diagnostics are genuinely already ported (not double-counted). Any test needing a live app that the blueprint claims lands now?',
  '4. The dispatcher + bin: does the main dispatch actually reduce to something runnable now, or does the primary path launch the deferred app? Is the "it runs" KAT genuinely executable without GPU/xev?',
  '5. SSH subsystem: self-contained as claimed? any hidden network/GUI dep? DiskCache file-IO tests portable?',
  '6. Anything else (list_themes size, R2 rename correctness vs rustty-identity, help/usage string exactness).',
  'Return numbered findings (CONFIRMED-ISSUE with file:line evidence / FALSE-ALARM / RECOMMENDATION) plus an overall verdict: ACCEPT / ACCEPT-WITH-FIXES (list fixes) / REDESIGN. Do NOT edit the blueprint.',
].join('\n'), { label: 'adversary', phase: 'Verify' })

return { blueprint_len: (blueprint || '').length, verdict }
