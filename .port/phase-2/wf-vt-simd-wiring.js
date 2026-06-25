export const meta = {
  name: 'vt-simd-wiring',
  description: 'Stage A.1: wire the already-ported rustty-simd vt primitives into stream.rs (the deferred SIMD nextSlice fast path). Fill stream.rs:651 todo!() + nextSliceCapped + consumeAllEscapes + consumeUntilGround + the nextSlice SIMD arm (ref stream.zig:471-577). Closes the stream SIMD deferral + ~23 G3b missing. Scalar fallback stays default; SIMD arm under the `simd` feature.',
  phases: [
    { title: 'Fill', detail: 'port the SIMD next_slice path + helpers using rustty-simd' },
    { title: 'Verify', detail: 'stream G3b->0 + tests (default + --features simd) + Miri/ASan + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SG3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/stream.zig --port-glob "' + R + '/crates/rustty-vt/src/stream.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = every line/comment verbatim. Reference READ-ONLY ' + T + '/stream.zig. rustty-vt complete (1948 tests); rustty-simd is FULLY ported (crate rustty_simd::vt::utf8_decode_until_control_seq etc. — simd/vt.zig:19 analogue). The stream SIMD fast path was deferred (DEFERRALS.md "Deferred to Dispatch/SIMD phase"): stream.rs has a todo!() SIMD arm (~stream.rs:651) gated on options::SIMD; the scalar fallback runs by default + passes all 38 stream tests. '
  + 'KEY: port nextSliceCapped + consumeAllEscapes + consumeUntilGround + the nextSlice SIMD branch (ref stream.zig:471-577) using rustty_simd::vt (utf8DecodeUntilControlSeq) — faithful, every comment verbatim incl the cp_buf chunk-size consts + @setEvalBranchQuota notes. The SIMD path is the `comptime debug or !build_options.simd` ELSE arm; keep the scalar arm as the default (SIMD feature off). [:0]const u8->Vec<u8>. SOUNDNESS RULE (memory): raw-ptr discipline; the SIMD path slices input via raw ptrs. '
  + 'MIRI: per MIRI_EXCEPTIONS.md — note the SIMD path calls rustty-simd C FFI (base64/utf8 SIMD) which Miri cannot execute, so SIMD-arm tests are ASan-covered (R6 FFI->ASan), scalar tests stay Miri-clean. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, builds: { type: 'boolean' }, stream_g3b_missing: { type: 'integer' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, stream_g3b_missing: { type: 'integer' }, default_tests_pass: { type: 'boolean' }, simd_tests_pass: { type: 'boolean' }, asan_clean: { type: 'boolean' }, residual_deferred: { type: 'boolean' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (wire the SIMD next_slice path). In crates/rustty-vt/src/stream.rs, replace the SIMD todo!() (~line 651) + port nextSliceCapped + consumeAllEscapes + consumeUntilGround + the nextSlice SIMD branch from ' + T + '/stream.zig:471-577, using rustty_simd::vt for the SIMD utf8/control-seq scan. Keep the scalar fallback as the default (SIMD under `cfg!(feature="simd")`). Restore the deferred SIMD-region comments verbatim (cp_buf consts, @setEvalBranchQuota, the chunk/arbitrary-number notes). Update DEFERRALS.md (mark the stream SIMD seam RESOLVED). GATES FOREGROUND: cargo build -p rustty-vt; cargo build -p rustty-vt --features simd; cargo test -p rustty-vt stream (38 pass, default); cargo test -p rustty-vt --features simd stream (the SIMD arm runs + matches); clippy --all-targets -D (default + --features simd); fmt; ' + SG3B + ' => the SIMD-region missing now resolved (only residual = c_union C-ABI -> vt-c). Commit "rustty: wire SIMD next_slice fast path (Stage A.1 — stream SIMD deferral resolved)". Return ok+builds+stream_g3b_missing+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES + 'Fill: ' + JSON.stringify(fill && { commit: fill.commit, stream_g3b_missing: fill.stream_g3b_missing, blockers: fill.blockers }) + '\n'
  + 'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt + --features simd; cargo test -p rustty-vt stream (default 38 pass, scalar) + cargo test -p rustty-vt --features simd (SIMD arm exercised, same results); `bash ' + GATES + '/sanitizers.sh rustty-vt stream address` + with --features simd if the script supports it (ASan covers the SIMD C-FFI path Miri cannot); cargo +nightly miri test -p rustty-vt on 2 scalar stream tests (0 UB/leak); clippy --all-targets -D (both feature sets) + fmt --check + ' + GATES + '/check_acyclic.sh; ' + SG3B + ' => residual missing maps ONLY to the c_union C-ABI (-> vt-c, Stage B) — the SIMD seam comments now present (residual_deferred). Spot-check the SIMD path is faithful to stream.zig (nextSliceCapped chunking + consumeUntilGround) + observationally identical to the scalar arm. Verdict GREEN only if builds(both) + default+simd tests pass + ASan clean + stream G3b residual = c_union-only + clippy/fmt/acyclic green. Report verdict + stream_g3b_missing + default_tests_pass + simd_tests_pass + asan_clean + residual_deferred + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
