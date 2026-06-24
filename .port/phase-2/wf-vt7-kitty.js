export const meta = {
  name: 'vt7-kitty',
  description: 'rustty-vt sub-batch vt-7: kitty graphics cluster minus the Screen/Terminal-dependent files. Ports kitty/key, graphics_command, graphics_unicode, graphics_render, graphics_image (sys.decode_png seam + os TempDir/file which exist), and the graphics/kitty aggregators (graphics_storage->Screen + graphics_exec->Terminal stubbed/deferred to vt-12). ~61 tests.',
  phases: [
    { title: 'Port', detail: 'key+graphics_command -> unicode+render -> graphics_image -> aggregators (defer storage/exec)' },
    { title: 'Verify', detail: 'all kitty tests + Miri + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const K = G + '/src/terminal/kitty'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: EVERY line; EVERY comment VERBATIM (retarget only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION, INDEPENDENT expected values (no tautology); nothing dropped/weakened. Reference READ-ONLY. Deps ready: PageList/sys/page/lib ported; rustty-os HAS tempdir.rs + file.rs (for os/TempDir + os/file); fastmem/quirks in util. kitty/color already ported (vt-6). PNG decode is an OPTIONAL sys hook (sys.decode_png) — port the `decode_png == null` path faithfully + seam the codec (do NOT pull a heavy image dep; mirror Ghostty which tolerates null). kitty graphics is gated by build_options.kitty_graphics (vt-4 wired the flag). The rust kitty/ files (key.rs, graphics_*.rs) are stubs. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, deferred: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, kitty_tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, deferred_documented: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const GROUPS = [
  { label: 'key+command', task: 'Port ' + K + '/key.zig (169 loc, 5t — kitty keyboard protocol flags) into kitty/key.rs AND ' + K + '/graphics_command.zig (1333 loc, 22t — the kitty graphics APC command parser) into kitty/graphics_command.rs. Both are leaves within kitty (deps lib/simd/quirks).' },
  { label: 'unicode+render', task: 'Port ' + K + '/graphics_unicode.zig (1347 loc, 16t — unicode placeholder/diacritic mapping) into kitty/graphics_unicode.rs AND ' + K + '/graphics_render.zig (27 loc, 0t — trivial) into kitty/graphics_render.rs.' },
  { label: 'image', task: 'Port ' + K + '/graphics_image.zig (1044 loc, 16t — kitty image load/decode/temp-file) into kitty/graphics_image.rs. Deps: graphics_command (this batch), PageList/sys (done), os/TempDir->rustty_os tempdir, os/file->rustty_os file, fastmem (util). PNG: port the sys.decode_png hook + the `decode_png == null` save-effort path faithfully; seam the actual PNG codec (no heavy dep). Reproduce temp-file image loading via rustty-os.' },
  { label: 'aggregators', task: 'Port ' + K + '/graphics.zig (37 loc, 1t — aggregates the 6 graphics_*) into kitty/graphics.rs AND ' + G + '/src/terminal/kitty.zig (15 loc, 1t — aggregates key/color/graphics) into kitty/mod.rs. graphics_storage (->Screen) + graphics_exec (->Terminal) are DEFERRED to vt-12: keep them as compiling stubs with a clear "DEFERRED to vt-12 (needs Screen/Terminal)" PORT NOTE; wire the available 4 graphics_* (command/image/unicode/render) into graphics.rs. kitty graphics gated by build_options.kitty_graphics.' },
]
const results = []
for (let i = 0; i < GROUPS.length; i++) {
  const g = GROUPS[i]
  const r = await robust(RULES +
    'TASK (kitty group "' + g.label + '"): ' + g.task + ' FULLY PORT (faithful; comments verbatim; tests assertion-for-assertion). Wire into kitty/mod.rs. GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt kitty (this group\'s tests pass + no regression); cargo +nightly miri test -p rustty-vt on 1-2 of this group\'s tests (0 UB/leak); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; comment coverage for this group\'s ref file(s) missing=0. Commit "rustty: vt kitty ' + g.label + ' (vt-7)". Return ok+tests_added+builds+commit+deferred+blockers. If a non-deferred file genuinely needs an un-ported dep beyond the known seams, surface it.',
    { schema: STEP, effort: 'high', label: 'kitty:' + g.label, phase: 'Port' })
  results.push({ group: g.label, r })
  if (!r || (r.ok === false && r.blockers && r.blockers.length)) break
}

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (all kitty tests pass: key 5 + graphics_command 22 + graphics_unicode 16 + graphics_image 16 + graphics 1 + kitty 1 = ~61; full suite no regression — 734 + kitty); cargo +nightly miri test -p rustty-vt on ~5 kitty tests (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt kitty address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. G3b across the ported kitty files (key/graphics_command/graphics_unicode/graphics_render/graphics_image/graphics + kitty.zig) missing=0. Confirm graphics_storage + graphics_exec are cleanly DEFERRED (compiling stubs + PORT NOTE naming vt-12) — NOT silently dropped (deferred_documented). Spot-check 3 ported tests faithful, not tautological. Verdict GREEN only if all green AND no NON-deferred file left stub/todo AND g3b_missing=0 AND the 2 deferrals are documented. Report verdict + kitty_tests_passing + miri_clean + asan_clean + g3b_missing + deferred_documented + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { results, verify }
