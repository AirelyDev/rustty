export const meta = {
  name: 'vt6-osc',
  description: 'rustty-vt sub-batch vt-6: the OSC cluster (osc.zig parent + 16 osc/parsers/* children + encoding + kitty/color leaf), ~243 tests. Parent↔child cycle handled struct-first: osc.rs Command enum + Parser state machine + full dispatch with parser stubs first, then fill each parser group (faithful + tests + gates + per-group commit), then wire aggregator + osc.zig tests + verify. Unblocks Parser (vt-5) + stream (vt-9).',
  phases: [
    { title: 'Parent', detail: 'kitty/color + osc/encoding + osc.rs (Command+Parser+dispatch) with parser stubs -> builds' },
    { title: 'Parsers', detail: 'fill the 16 osc parsers in groups: faithful + tests + gates + commit' },
    { title: 'Verify', detail: 'aggregator + osc.zig tests + all osc tests + Miri + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const G3B = (refs, glob) => 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + refs + ' --port-glob "' + glob + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: EVERY line; EVERY comment VERBATIM (retarget only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION with INDEPENDENT expected values (no tautology); nothing dropped/weakened. Reference READ-ONLY. The rust osc/ + kitty/ module skeletons already exist (osc/mod.rs, osc/encoding.rs, osc/parsers/, kitty/*.rs — all stubs). Deps color/lib/sys/charsets are ported. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, osc_tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Parent')
const parent = await robust(RULES +
  'TASK (struct-first parent skeleton for the OSC cluster). (1) Port ' + T + '/kitty/color.zig (77 loc, 1 test) into crates/rustty-vt/src/kitty/color.rs (near-leaf; deps main re-export + std). (2) Port ' + T + '/osc/encoding.zig (38 loc, 1 test) into crates/rustty-vt/src/osc/encoding.rs (leaf). (3) Port ' + T + '/osc.zig (826 loc) into crates/rustty-vt/src/osc/mod.rs: the FULL Command enum (every variant), the Parser struct (state fields), init/deinit/reset, next(c), and the end()/dispatch state machine. The dispatch calls into the per-command parsers (osc/parsers/*). To break the parent↔child cycle: create/keep each osc/parsers/*.rs as a COMPILING STUB with the correct faithful function SIGNATURE (matching the Zig parser\'s parse entry + return Command variant) but a `todo!()`/placeholder body + NO tests yet — so osc/mod.rs\'s dispatch wires to real signatures and the crate BUILDS. List the 16 parser modules you stubbed (mouse_shape, change_window_icon, rxvt_extension, report_pwd, clipboard_operation, change_window_title, kitty_color, kitty_text_sizing, hyperlink, color, context_signal, iterm2, kitty_clipboard_protocol, osc9, semantic_prompt, + parsers aggregator). Defer osc.zig\'s own 2 test{} blocks to the Verify phase (they exercise filled parsers). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt (green, parsers stubbed); clippy -p rustty-vt -- -D warnings (allow todo! in stubs via #[allow] or expect); fmt --check; ' + G3B(T + '/osc.zig,' + T + '/kitty/color.zig,' + T + '/osc/encoding.zig', R + '/crates/rustty-vt/src/osc/mod.rs ' + R + '/crates/rustty-vt/src/kitty/color.rs ' + R + '/crates/rustty-vt/src/osc/encoding.rs') + ' => missing=0 for osc.zig+color+encoding. Commit "rustty: vt osc parent skeleton — Command+Parser+dispatch, parser stubs (vt-6)". Return ok+builds+commit+blockers+notes(the exact parser module signatures so the fill agents match them).',
  { schema: STEP, effort: 'high', label: 'osc-parent', phase: 'Parent' })

phase('Parsers')
const GROUPS = [
  { label: 'tiny', files: 'mouse_shape(1t), change_window_icon(1t), rxvt_extension(1t), report_pwd(2t), clipboard_operation(4t), change_window_title(7t)' },
  { label: 'kitty-small', files: 'kitty_color(5t), kitty_text_sizing(8t), hyperlink(8t)' },
  { label: 'color', files: 'color(12t, 780 loc — uses ported color.zig)' },
  { label: 'context_signal', files: 'context_signal(18t, 541 loc)' },
  { label: 'iterm2', files: 'iterm2(19t, 435 loc)' },
  { label: 'kitty_clipboard_protocol', files: 'kitty_clipboard_protocol(27t, 702 loc — uses osc/encoding)' },
  { label: 'osc9', files: 'osc9(61t, 1141 loc)' },
  { label: 'semantic_prompt', files: 'semantic_prompt(64t, 1280 loc)' },
]
const presults = []
for (let i = 0; i < GROUPS.length; i++) {
  const g = GROUPS[i]
  const r = await robust(RULES +
    'Parent skeleton landed: ' + JSON.stringify(parent && parent.notes ? parent.notes.slice(0, 400) : '') + '\n'
    + 'TASK (fill OSC parser group "' + g.label + '"): replace the stub(s) for ' + g.files + ' in crates/rustty-vt/src/osc/parsers/<name>.rs with the FAITHFUL port of ' + T + '/osc/parsers/<name>.zig — full parse logic + EVERY comment verbatim + ALL its test{} as #[test] assertion-for-assertion (independent expected values). Match the function signature the parent\'s osc dispatch expects (do not change the dispatch unless a signature was wrong — if so, fix minimally + note it). GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt osc (the new parser tests pass + no regression); cargo +nightly miri test -p rustty-vt on 1-2 of this group\'s tests (pure parsing = Miri-fast; 0 UB/leak); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; ' + G3B(T + '/osc/parsers/<each>.zig', R + '/crates/rustty-vt/src/osc/parsers/<each>.rs') + ' => missing=0 for this group\'s files. Commit "rustty: vt osc parsers ' + g.label + ' (vt-6)". Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label: 'osc:' + g.label, phase: 'Parsers' })
  presults.push({ group: g.label, r })
  if (!r || (r.ok === false && r.blockers && r.blockers.length)) break
}

phase('Verify')
const verify = await robust(RULES +
  'TASK (wire aggregator + osc.zig tests, then independent verify — default RED). (1) Port ' + T + '/osc/parsers.zig (21 loc, 1 test) aggregator into crates/rustty-vt/src/osc/parsers/mod.rs (re-export all 16 parsers). (2) Port osc.zig\'s own 2 test{} blocks (deferred from Parent) into osc/mod.rs now that parsers are filled. (3) Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt osc (ALL osc cluster tests pass — ~243: the 16 parsers + osc.zig 2 + encoding 1 + kitty/color 1) + the full suite has no regression (PageList 210 + vt-4 still green); cargo +nightly miri test -p rustty-vt on ~5 osc tests across parsers (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt osc address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. G3b across ALL osc files (refs ' + T + '/osc.zig + ' + T + '/osc/**/*.zig + kitty/color.zig) missing=0. Spot-check 3 ported tests (one big like osc9/semantic_prompt) are faithful, not tautological. Verdict GREEN only if all green AND no parser left stub/todo AND g3b_missing=0. Commit the aggregator+osc tests "rustty: vt osc aggregator + osc.zig tests + verify (vt-6)". Return verdict+osc_tests_passing+miri_clean+asan_clean+g3b_missing+tautological_tests+blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { parent, presults, verify }
