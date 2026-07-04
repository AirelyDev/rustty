export const meta = {
  name: 'config-c1-leaves',
  description: 'Config cluster C1: the config leaf value-types (Config-independent, portable before the giant Config struct). RepeatableStringMap(4)+string(9)+io(4)+path(8)+command(10)+conditional(1)+ClipboardCodepointMap/ErrorList/edit/file_load(0)+url(1, oniguruma-gated -> defer the test) = ~37t into rustty-config. Deps ready: cfg-fmt/args/os/util/string. C2 (the 10934-loc Config struct) + C3 (CApi) follow.',
  phases: [
    { title: 'Leaves', detail: 'RepeatableStringMap/string/io/path/command/conditional/ClipboardCodepointMap/ErrorList/edit/file_load/url -> rustty-config' },
    { title: 'Verify', detail: 'count-gate + tests + Miri + G3b + faithfulness (url test oniguruma-deferred)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const CF = G + '/src/config'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R2 = rustty identity. Reference READ-ONLY ' + CF + '/. Fills rustty-config (crates/rustty-config/, lib.rs stub; deps cfg-fmt/args/os/util/vt/input/render-types — all PORTED; the rustty-renderer dep is a harmless stub, do NOT use it). Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md + the config-scoping panel (C1 = leaf value-types, Config-independent). '
  + 'KEY: these are the config VALUE-TYPES the giant Config struct (C2, later) composes. Deps: formatter->rustty-cfg-fmt (done), cli->rustty-args (done), os->rustty-os, quirks->rustty-util. [:0]const u8->Vec<u8>; byte-slices &[u8]/Vec<u8> (config values embed raw bytes). comptime reflection -> the rustty-args Target-trait / FormatEntry pattern (NO proc-macro). url.zig uses oniguruma (1 test) — gate that ONE test behind #[cfg(feature="oniguruma")] (off; oniguruma-sys not built yet — SkipZigTest-style defer, documented in DEFERRALS.md), port url.zig`s non-regex logic normally. SOUNDNESS (memory): raw-ptr discipline. '
  + 'MIRI per MIRI_EXCEPTIONS.md ISOLATED. PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, url_deferred_documented: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Leaves')
const leaves = await robust(RULES +
  'TASK (config C1 leaf value-types). Port FULLY (every test{}) into rustty-config: ' + CF + '/{string.zig 9t, RepeatableStringMap.zig 4t, io.zig 4t, path.zig 8t (510loc), command.zig 10t (330loc — the config Command type, distinct from input/command), conditional.zig 1t, ClipboardCodepointMap.zig 0t, ErrorList.zig 0t, edit.zig 0t, file_load.zig 0t, url.zig 1t (523loc — port the URL-detection logic; gate its ONE oniguruma regex test #[cfg(feature="oniguruma")] OFF + document in DEFERRALS.md)}. Wire src/lib.rs. GATES FOREGROUND: cargo build -p rustty-config; cargo test -p rustty-config (the ~36 non-deferred pass); cargo +nightly miri test on 3 ISOLATED (0 UB/leak); clippy -p rustty-config --all-targets -D; fmt; check_comment_coverage.py scoped to the ported files missing=0. Commit "rustty: config C1 leaf value-types (string/path/io/command/url/... ~36 tests)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'leaves', phase: 'Leaves' })

phase('Verify')
const verify = await robust(RULES + 'Leaves: ' + JSON.stringify(leaves && leaves.commit) + '\n'
  + 'TASK (independent verify — default RED). Add the rustty-config C1 module mappings to ' + GATES + '/check_test_counts.py (GH base = src/config; url.zig deferred=1 for the oniguruma test). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => config C1 modules rust>=zig (count_ok, url deferred=1) + ALL GREEN; cargo test -p rustty-config (all non-deferred pass); cargo +nightly miri test on ~3 ISOLATED (0 UB/leak); clippy -p rustty-config --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh; G3b on the ported .zig files missing=0; confirm url.zig`s oniguruma test is cfg-gated + documented in DEFERRALS.md (url_deferred_documented). Spot-check 3 tests (a path expansion, a string parse, a command parse) faithful, not tautological. Verdict GREEN only if count_ok + build + tests + Miri-subset + g3b=0 + clippy + fmt + acyclic green. Report verdict + tests_passing + count_ok + miri_subset_clean + g3b_missing + url_deferred_documented + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { leaves, verify }
