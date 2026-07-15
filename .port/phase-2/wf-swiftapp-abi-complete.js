export const meta = {
  name: 'swiftapp-abi-complete',
  description: 'Close the Phase-A ABI gap: config(13)+benchmark(1) ghostty_* aliases + port the 3 unported main_c exports (info/translate/cli_try_action) so all 90 app-C-ABI ghostty_* symbols resolve for the Swift link',
  whenToUse: 'Between Phase A and Phase B of the macOS Swift-app milestone — the 17 missing aliases found by nm',
  phases: [
    { title: 'Fill', detail: 'config aliases, bench alias, main_c port — parallel' },
    { title: 'Verify', detail: 'all Swift-referenced ghostty_* resolve; manifest gate green' },
  ],
}

const PRE = '.port/phase-2/PREPORT_SWIFT_APP.md'
const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' },
    blocker: { type: 'string' }, notes: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. READ /Users/gael/projects/ghostty (oracle @ 4749c4e9) but NEVER modify it. Context: the macOS Swift-app milestone needs all 90 app-C-ABI ghostty_* symbols to resolve when the reused Swift app links. Phase A added ghostty_* aliases for apprt-c's own exports; this closes the 17 that live in other crates. Precedent: crates/rustty-apprt-c/src/ghostty_alias.rs (thin #[no_mangle] pub unsafe extern "C" fn ghostty_x(args) { crate::rustty_x(args) } forwarders, gated by the identity-oracle cargo feature; ${PRE} §3 Branch A). rustty_config_* live in crates/rustty-config/src/c_api.rs; rustty_benchmark_cli in crates/rustty-bench/src/capi.rs; rustty_init/rustty_string_free in crates/rustty-apprt-c/src/lib.rs.
HARD: R1 fidelity (port the Zig faithfully, comments preserved); R2 (canonical rustty_*, ghostty_* alias); R5. Git: explicit paths, index.lock retry, no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Fill')
const tasks = [
  {
    key: 'config-alias', label: 'config ghostty_* aliases (13)',
    prompt: `${COMMON}
TASK: add the ghostty_config_* alias layer. The 13 rustty_config_* exports already exist in crates/rustty-config/src/c_api.rs. Add crates/rustty-config/src/ghostty_alias.rs (model on apprt-c's ghostty_alias.rs): one #[no_mangle] pub unsafe extern "C" fn ghostty_config_<name>(<exact args from the rustty_config_<name> decl>) -> <Ret> { crate::c_api::rustty_config_<name>(<args>) } forwarder for all 13 (config_new, config_free, config_clone, config_load_cli_args, config_load_default_files, config_load_file, config_load_recursive_files, config_finalize, config_get, config_diagnostics_count, config_get_diagnostic, config_open_path, config_trigger). Add an identity-oracle feature to crates/rustty-config/Cargo.toml and #[cfg(feature = "identity-oracle")] mod ghostty_alias; to its lib.rs. IMPORTANT: apprt-c must enable rustty-config's identity-oracle feature when its own identity-oracle is on — add rustty-config = { ..., features/optional } wiring so cargo build -p rustty-apprt-c --features identity-oracle pulls the config aliases into the staticlib. Verify: cargo build -p rustty-apprt-c --features identity-oracle green; nm -gU target/debug/librustty_apprt_c.a | grep -c '_ghostty_config_' == 13. Commit.`,
  },
  {
    key: 'maincapi-port', label: 'port main_c info/translate + bench alias',
    prompt: `${COMMON}
TASK: port the 2 small unported main_c exports + the benchmark alias. In crates/rustty-apprt-c (where rustty_init lives):
1. Port ghostty_info -> rustty_info faithfully (reference src/main_c.zig ghostty_info: returns Info{ mode: build mode enum, version: build_config.version_string.ptr, version_len: .len }). The Info struct + BuildMode enum ordinals are in ${PRE} §6 (debug=0/release_safe=1/release_fast=2/release_small=3). Use rustty-build-config for version + mode. Add the ghostty_info alias.
2. Port ghostty_translate -> rustty_translate (reference: 1 line, returns internal_os.i18n._(msgid) — the gettext wrapper). Find rustty's i18n (rustty-os or rustty-global); if the gettext binding exists, forward to it; if i18n is deferred, return msgid unchanged (identity translation) with a PORT NOTE citing the deferral — faithful degradation, document it. Add the alias.
3. Add the ghostty_benchmark_cli alias: rustty_benchmark_cli exists in crates/rustty-bench/src/capi.rs — add a ghostty_alias in rustty-bench (identity-oracle gated) + wire the feature through apprt-c, OR forward from apprt-c via an extern "C" decl — whichever matches the config-alias agent's approach (coordinate: prefer per-crate alias files).
Preserve the Zig source as ported comments (R1). Verify each: cargo build -p rustty-apprt-c --features identity-oracle green; nm shows _ghostty_info, _ghostty_translate, _ghostty_benchmark_cli. Commit.`,
  },
  {
    key: 'cli-try-action', label: 'port ghostty_cli_try_action',
    prompt: `${COMMON}
TASK: port ghostty_cli_try_action -> rustty_cli_try_action (reference src/main_c.zig: 'const action = state.action orelse return; log; posix.exit(action.run(alloc) catch exit(1)); posix.exit(0);'). This runs a CLI action stored in the global C-API state during ghostty_init, or returns immediately if none (the GUI launch path — state.action is null → return, no exit). Find rustty's equivalent global C-API state (where rustty_init stashes parsed CLI action; likely rustty-apprt-c or rustty-global). Port faithfully: if the state holds an action, run it and posix-exit with its code; else return. The action.run dispatch is the CLI action machinery (rustty-cli). If the full action.run wiring is not yet ported, port the CONTROL FLOW faithfully (the null-action GUI path returns cleanly — that is what a windowed launch hits) and, for the non-null path, either wire to the ported CLI action runner if it exists or leave a documented TODO(port) with a DEFERRALS entry for the CLI-action-from-C path (R5: don't fake it, but the GUI-launch path — the milestone's actual need — must work). Add the ghostty_cli_try_action alias. Verify: cargo build -p rustty-apprt-c --features identity-oracle green; nm shows _ghostty_cli_try_action; if you can, a quick test that the null-action path returns without exiting. Commit. Report exactly what the non-null path does (ported vs deferred).`,
  },
]
const res = await parallel(tasks.map(t => () => agent(t.prompt, { label: t.label, phase: 'Fill', schema: OUT })))
const good = res.filter(Boolean)
log(`Fill: ${good.filter(r => r.status === 'done').length}/${tasks.length} done`)

phase('Verify')
const verify = await agent(`${COMMON}
TASK: verify the ABI surface is complete for the Swift link. (1) cargo build -p rustty-apprt-c --features identity-oracle. (2) nm -gU target/debug/librustty_apprt_c.a | grep '_ghostty_' | sed 's/.* _//' | sort -u  -> the exported ghostty_* set. (3) The link-critical set = the ghostty_* names the reused Swift app actually references: grep -rho 'ghostty_[a-z_]*' /Users/gael/projects/ghostty/macos/Sources | sort -u. (4) Assert EVERY Swift-referenced ghostty_* symbol is in the exported set (that is what makes the app link). Report any still-missing referenced symbol as a blocker with its source_file from .port/audits/CABI_SYMBOLS.tsv. (5) Run .port/tools/gates/check_symbol_manifest.sh against the built lib -> report GREEN or the drift. Also update the alias-file docstrings/manifest if any crate's coverage note is now stale. Commit any doc fixes. Report: exported ghostty_* count, Swift-referenced count, and the intersection gap (should be empty).`,
  { label: 'verify-abi', phase: 'Verify', schema: OUT })

return {
  fill: good.map(r => ({ t: r.task, s: r.status })),
  verify: verify ? verify.status : 'failed',
  summary: `fill ${good.filter(r => r.status === 'done').length}/${tasks.length}; verify ${verify ? verify.status : 'fail'} — ${(verify && (verify.notes || verify.evidence) || '').slice(0, 300)}`,
}
