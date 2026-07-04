export const meta = {
  name: 'integration-tier',
  description: 'Port the integration tier per the committed blueprint: crash (9t) -> termio dep wiring + renderer mailbox -> Command relocation (10t) -> termio testable logic (Exec/shell_integration/message/root). Sequential, per-stage commits.',
  phases: [
    { title: 'crash' },
    { title: 'termio-wiring' },
    { title: 'command' },
    { title: 'termio-logic' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['stage', 'ok', 'tests_passing', 'commit', 'blockers', 'notes'],
  properties: {
    stage: { type: 'string' },
    ok: { type: 'boolean' },
    tests_passing: { type: 'number', description: 'tests passing in the touched crate(s) after this stage' },
    commit: { type: 'string', description: 'commit sha, empty if not committed' },
    blockers: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
}

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R1 (every reference \`test{}\` ported assertion-for-assertion — the test COUNT is a contract), R3 (byte-faithful to ghostty, every line + comment preserved; byte-slices \`[]const u8\`/\`[:0]const u8\` -> \`&[u8]\`/\`&str\`, NEVER lossy String), R5 (never weaken/skip/xfail a gate or test). Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2).

FIRST read the authoritative reviewed design: /Users/gael/projects/rustty/.port/phase-2/INTEGRATION_TIER_BLUEPRINT.md — your stage below implements one step of it. Follow its placements, seams, and corrections exactly; where the reference reveals detail the blueprint did not pin, use faithful judgment.

GATES (FOREGROUND, ALL green before commit — never commit red):
- cargo build --workspace clean
- cargo test on every touched crate: all pass, no regression elsewhere (default rustty-vt stays green)
- cargo clippy --all-targets -D warnings on touched crates; cargo fmt --check
- bash .port/tools/gates/check_acyclic.sh GREEN (the binding layering gate)
- python3 .port/tools/gates/check_test_counts.py GREEN — add/adjust the MAP entries your stage owes (per blueprint section 2); never fabricate counts
- scoped-G3b: distinctive reference comments preserved for each ported .zig file (missing=0)
- If the touched crate gains a C-FFI path (e.g. sentry-sys), ensure it is in .port/tools/gates/check_leaks.sh FFI_CRATES and run its tests under ASan the way check_leaks.sh does (G6)
- Update .port/phase-2/DEFERRALS.md for any documented deferral your stage creates or resolves

COMMIT: stage only YOUR files plus shared registration/gate edits (root Cargo.toml, Cargo.lock, gate files, DEFERRALS.md, PORT_PLAN corrections). Leave unrelated working-tree changes untouched. End the commit message with the trailer line:
Co-Authored-By: Claude Opus 4.8 (1M context)

If a genuine hard blocker stops you, do NOT fabricate or weaken — commit whatever is cleanly green, and report the blocker precisely in the structured result.`

const STAGES = [
  {
    phase: 'crash', label: 'crash',
    prompt: `STAGE 1 — port the crash tier into crates/rustty-crash (currently a ~9-loc stub).

Reference: /Users/gael/projects/ghostty/src/crash/*.zig (main.zig, dir.zig, sentry.zig, sentry_envelope.zig — enumerate the dir; port every file). Owed tests: 9 (sentry_envelope 8 + main.zig 1 reify).

Blueprint specifics to follow:
- dir.zig + sentry_envelope.zig port as-is (deps only rustty-os).
- sentry.zig/main.zig port now EXCEPT the beforeSend ThreadState.surface metadata block, which is seamed via a NEW local trait: pub trait ThreadStateSurface { fn screen_size(&self)->(u32,u32); fn grid_size(&self)->(u32,u32); fn cell_size(&self)->(u32,u32); } — ThreadState<S: ThreadStateSurface> holds a raw *const S, preserving the reference's live (possibly-torn) read semantics (the Zig comment says the read is "likely unsafe... but most likely fine"; do NOT snapshot). rustty-core's Surface implements it later. Do NOT reuse apprt-api's CoreSurfaceHandle. Plain (u32,u32) tuples are acyclicity-REQUIRED: rustty-renderer already deps rustty-crash, so crash must NOT dep renderer.
- New Cargo deps: rustty-build-config (the setTag build-mode/app-runtime/font-backend/renderer values), sentry-sys (vendored at crates-sys/sentry-sys — study its src/lib.rs KAT for the value-API idiom and note bindgen emits enum consts as sentry_value_type_t_SENTRY_VALUE_TYPE_*). rustty-global stays INJECTED (allocator passed as an explicit parameter), not a hard dep — mirror rustty-os's DI pattern.
- Source the app-name segment of the "ghostty/crash"/"ghostty/sentry" XDG subdirs from rustty-identity (reachable via rustty-os) rather than hardcoding.
- sentry_init starts transports/threads — the reference gates init on build config; port faithfully, but any test touching real sentry_init must follow the reference's own gating (do not spawn network transports in tests; transport is .none per the vendored build).
- Add check_test_counts.py MAP entries: crash_dir, crash_sentry, crash_sentry_envelope, crash_main (per blueprint section 2). Add rustty-crash to check_leaks.sh FFI_CRATES (sentry-sys C path) and run its tests under ASan.

Commit message: "rustty: port crash tier (dir/sentry/sentry_envelope, ThreadStateSurface seam) — 9 tests"`,
  },
  {
    phase: 'termio-wiring', label: 'termio-wiring',
    prompt: `STAGE 2 — termio dependency wiring + struct shapes (the 0-test files) + the minimal renderer mailbox surface.

Blueprint specifics:
- Add to crates/rustty-termio/Cargo.toml: rustty-config, rustty-renderer, rustty-apprt-api, rustty-global (all verified downward-safe; acyclic gate is the backstop).
- In crates/rustty-renderer, land the minimal cross-thread surface termio needs (NO xev, NO GPU): renderer message.zig -> a message.rs Message enum (faithful port of src/renderer/message.zig — read it; it has 0 tests) + Thread::Mailbox = BlockingQueue<Message, 64> (rustty-datastruct's already-ported BlockingQueue) + thin DerivedConfig placeholder types where Termio/Options reference renderer.Renderer.DerivedConfig / renderer.Thread.DerivedConfig, documented deferred-seam style (like ApprtError/Xev). Do NOT port renderer/Thread.zig's event loop.
- Port the struct SHAPES of src/termio/{Options.zig, Termio.zig, backend.zig, mailbox.zig, message.zig} into crates/rustty-termio (options.rs, termio_impl.rs, backend.rs, mailbox.rs, message.rs) against the existing seams: apprt.surface.Mailbox -> rustty_apprt_api::surface::{AppMailbox, Message, SurfaceMessageEnvelope} (termio is a generic consumer, M: AppMailbox); renderer::Size/State from rustty-renderer/rustty-render-types; Config from rustty-config. xev.Async/Loop/Completion/Timer/Process field types -> a termio-local documented placeholder seam (D6 deferred; see blueprint "Deferred decision points" — rustty-global::seam::Xev precedent). message.zig has 2 tests — port them now if they compile against the shapes (they are simple layout/size tests); otherwise they land in stage 4.
- Function bodies that require the live event loop (Thread.zig's run loop, Termio.zig's xev-driven callbacks) are ported as faithful signatures + verbatim-comment bodies with documented D6 deferral markers (0 reference tests — R1-safe), NOT fabricated logic.
- Add MAP entries per blueprint (termio_options, termio_termio, termio_backend, termio_mailbox, termio_message, termio_thread as applicable; 0 zig tests each except message=2).
- termio.zig root (1 reify test) may land here or stage 4 — your call; account for it exactly once.

Commit message: "rustty: termio dep wiring + struct shapes (Options/Termio/backend/mailbox/message) + renderer Thread::Mailbox"`,
  },
  {
    phase: 'command', label: 'command',
    prompt: `STAGE 3 — relocate + port src/Command.zig (911 loc, 10 tests) into crates/rustty-termio/src/command.rs (break #13b).

Blueprint specifics:
- Command goes in rustty-termio, NOT rustty-core (core placement would cycle: termio/Exec constructs+starts Command; core already deps termio). Re-export rustty_termio::Command at crate root (core's Surface reaches it later as rustty_termio::Command, matching the reference's flat Command symbol).
- Command's apprt import is ONLY the comptime @hasDecl(apprt.runtime, "pre_exec"/"post_fork") check — realized only by GTK (last). Port RtPreExecInfo/RtPostForkInfo as the trivial fallback (fn init(_: &Config) -> Self on unit-like structs) needing only rustty-config's Config TYPE, with a code comment marking the seam GTK's backend specializes later. Do NOT add anything to apprt-api.
- Deps: rustty-config, rustty-global (global_state.rlimits.restore()), rustty-os — all already wired in stage 2.
- The 10 tests are pure POSIX process-spawn tests (fork/exec, env, cwd, stdout redirect) — port assertion-for-assertion. The Windows path (startWindows) ports structurally (every line/comment) with the reference's own windows SkipZigTest gating mirrored (cfg-gated, like the F3 DirectWrite precedent).
- CORRECTIONS (blueprint section 1): remove "Command.zig" from crates/rustty-core/src/lib.rs's PORTS docstring line (keep App.zig/Surface.zig/surface_mouse.zig); update crates/rustty-termio/src/lib.rs docstring to add Command.zig citing break #13b; correct .port/phase-0/plan/PORT_PLAN.md crate 24 (core: Tests 1, Command's 10 move out) and crate 23 (termio: 44) + the section-5 edge table (termio gains config/renderer/apprt-api/global).
- MAP entry: termio_command with the zig glob pointing at src/Command.zig (the physical reference location) and the rust file at crates/rustty-termio/src/command.rs.

Commit message: "rustty: port Command.zig -> rustty-termio::command (break #13b) — 10 tests"`,
  },
  {
    phase: 'termio-logic', label: 'termio-logic',
    prompt: `STAGE 4 — port termio's testable logic: src/termio/Exec.zig (10 tests), src/termio/shell_integration.zig (21 tests), src/termio/stream_handler.zig (0 tests, shapes+logic), src/termio.zig root (1 reify test), and src/termio/message.zig's 2 tests if stage 2 left them.

Blueprint specifics:
- Exec.zig: the testable core is the execCommand/subprocess argv+env assembly logic (10 tests — port assertion-for-assertion). Subprocess.start constructs rustty_termio::Command (landed stage 3). xev.Process / the exec backend's event-loop wiring follows the same D6 documented-deferral seam as stage 2 (0 tests on those paths).
- shell_integration.zig: 21 tests, byte-exact per-shell (bash/zsh/fish/elvish) string/env generation — the reference resources dir handling ports against rustty-os resourcesdir. Be exact: these tests compare full strings.
- stream_handler.zig: the big vt-Stream handler impl — port faithfully against rustty-vt's Stream/handler surface and the apprt-api surface Mailbox seam; 0 reference tests, so any genuinely core-Surface-dependent callback body gets a documented deferral marker with verbatim reference body preserved (R5: never a silent drop).
- Final accounting: cargo test -p rustty-termio must show ALL owed termio tests passing — the R1 contract for the crate is 44 total (Exec 10 + shell_integration 21 + message 2 + root 1 + Command 10). Ensure the MAP entries (termio_exec, termio_shell_integration, termio_root + earlier ones) make check_test_counts.py GREEN with these counts. Update DEFERRALS.md (D6/xev markers consolidated; stream_handler core-dependent bodies if any).
- Also verify cargo test -p rustty-crash and -p rustty-renderer still green (no regression from this tier).

Commit message: "rustty: termio testable logic — Exec/shell_integration/stream_handler/root — termio 44 tests"`,
  },
]

const results = []
for (const s of STAGES) {
  phase(s.phase)
  let r
  try {
    r = await agent(`${COMMON}\n\n${s.prompt}\n\nReturn the structured result (stage, ok, tests_passing, commit, blockers, notes).`,
      { label: s.label, phase: s.phase, schema: RESULT_SCHEMA })
  } catch (e) {
    r = { stage: s.phase, ok: false, tests_passing: 0, commit: '', blockers: ['threw: ' + String(e)], notes: '' }
  }
  if (!r) r = { stage: s.phase, ok: false, tests_passing: 0, commit: '', blockers: ['agent returned null (skipped/died)'], notes: '' }
  results.push(r)
  log(`${s.phase}: ${r.ok ? 'OK ' + r.commit + ' (' + r.tests_passing + 't)' : 'BLOCKED — stopping tier'}`)
  if (!r.ok) break
}
return { results }
