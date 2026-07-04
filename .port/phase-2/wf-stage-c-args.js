export const meta = {
  name: 'stage-c-args',
  description: 'Stage C leaf: rustty-args — the CLI arg parser (cli/args.zig 36t + cli/CommaSplitter.zig 28t + diagnostics if needed = ~64 tests). Deps ready (os/util/build-config). Mechanical; breaks the config<->cli knot (args is BELOW config). No config dep (verified).',
  phases: [
    { title: 'Port', detail: 'CommaSplitter + args.zig (+diagnostics if needed) into rustty-args' },
    { title: 'Verify', detail: 'count-gate + tests + Miri + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const C = G + '/src/cli'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. Reference READ-ONLY ' + C + '/. This fills rustty-args (crates/rustty-args/, currently a lib.rs stub; Cargo.toml deps rustty-os/util/build-config — all PORTED). Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md (Stage C). '
  + 'KEY: args.zig (1627 loc, 36t — the generic CLI arg parser: parseIntoField/Diagnostics/ArgsIterator) + CommaSplitter.zig (483 loc, 28t) into rustty-args. args.zig deps: os/main->rustty-os, quirks->rustty-util, CommaSplitter (sibling), diagnostics.zig (port it too if args needs it — it is a cli/ sibling, 0 tests). NO config dep (args is below config — verified). [:0]const u8->Vec<u8>; byte-slices &[u8]/Vec<u8>. comptime field-reflection parseIntoField -> faithful Rust (a trait or per-type, matching how rustty-vt handled comptime reflection e.g. tmux/output FormatStruct). SOUNDNESS RULE (memory): raw-ptr discipline. '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset in ISOLATION. PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (port the arg parser). Port FULLY (every test) into rustty-args: ' + C + '/CommaSplitter.zig (28t) -> src/comma_splitter.rs; ' + C + '/args.zig (36t) -> src/args.rs; + ' + C + '/diagnostics.zig if args.zig requires it -> src/diagnostics.rs. Wire src/lib.rs. GATES FOREGROUND: cargo build -p rustty-args; cargo test -p rustty-args (64 pass); cargo +nightly miri test -p rustty-args on 2 tests ISOLATED (0 UB/leak); clippy -p rustty-args --all-targets -D; fmt; check_comment_coverage.py --crate args (or scoped) for args.zig + CommaSplitter.zig missing=0. Commit "rustty: port rustty-args (args + CommaSplitter, ~64 tests) — Stage C". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'port', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES + 'Port: ' + JSON.stringify(port && port.commit) + '\n'
  + 'TASK (independent verify — default RED). Add rustty-args mappings to ' + GATES + '/check_test_counts.py (note GH base = src/cli; use a separate GHC const or inline paths) — args.zig vs args.rs, CommaSplitter.zig vs comma_splitter.rs. Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => args modules rust>=zig (count_ok) + ALL still GREEN; cargo test -p rustty-args (CommaSplitter 28 + args 36 pass); cargo +nightly miri test -p rustty-args on ~3 tests ISOLATED (0 UB/leak); clippy -p rustty-args --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh (no cycle); G3b args.zig + CommaSplitter.zig missing=0. Spot-check 3 tests (a parseIntoField, a CommaSplitter split, a diagnostics case) faithful, not tautological. Verdict GREEN only if count_ok + build + tests + Miri-subset + g3b=0 + clippy + fmt + acyclic green. Report verdict + tests_passing + count_ok + miri_subset_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
