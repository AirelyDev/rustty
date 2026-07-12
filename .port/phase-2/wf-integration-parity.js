export const meta = {
  name: 'integration-parity',
  description: 'Parity review of this session\'s macOS-integration code against the ghostty oracle: W-ATTACH renderer wiring, ghostty_* aliases + main_c ports, xcframework packaging, resource generation, the host-view seam — flag every fidelity divergence (R1), distinguishing sanctioned (R2/R4/Branch-A) from real bugs',
  whenToUse: 'After the macOS terminal renders — verify the integration glue faithfully mirrors ghostty',
  phases: [
    { title: 'Review', detail: '5 parallel parity auditors, each an integration area vs the oracle' },
    { title: 'Verify', detail: 'adversarially confirm real divergences' },
    { title: 'Synthesize', detail: 'parity report + the fix list' },
  ],
}

const FINDINGS = {
  type: 'object',
  required: ['area', 'coverage', 'findings'],
  properties: {
    area: { type: 'string' },
    coverage: { type: 'string', description: 'what rustty code + ghostty reference you compared, and how (line-by-line vs sampled)' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['kind', 'severity', 'title', 'rustty', 'ghostty', 'detail'],
        properties: {
          kind: { type: 'string', enum: ['fidelity-bug', 'sanctioned-divergence', 'missing-piece', 'attestation'], description: 'fidelity-bug = behavior differs from the reference (R1 breach); sanctioned-divergence = deliberate R2/R4/Branch-A change, verify it is behavior-preserving + documented; missing-piece = reference does X, rustty omits it; attestation = confirmed faithful' },
          severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
          title: { type: 'string' },
          rustty: { type: 'string', description: 'rustty file:line + what it does' },
          ghostty: { type: 'string', description: 'ghostty file:line + what the reference does' },
          detail: { type: 'string', description: 'the exact divergence + its behavioral consequence; for sanctioned, why it is behavior-preserving' },
          fix_sketch: { type: 'string' },
        },
      },
    },
  },
}
const VERDICT = { type: 'object', required: ['confirmed', 'reason'], properties: { confirmed: { type: 'boolean' }, reason: { type: 'string' }, corrected_severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] } } }

const COMMON = `
CONTEXT: rustty is a full-fidelity Zig→Rust port of Ghostty (rules .port/PORT_RULES.md — R1 every line/behavior faithful; R2 rustty identity + ghostty_* C-ABI aliases; R4 acyclic crates via renderer-defined accessor traits; Branch A = ghostty_* symbols alias canonical rustty_*). This session built the macOS Swift-app integration: the app now renders a live terminal. Your job: PARITY-REVIEW your integration area against the READ-ONLY oracle /Users/gael/projects/ghostty @ 4749c4e9, flagging every divergence from the reference's behavior.
CRITICAL — distinguish three cases: (1) fidelity-bug = rustty's behavior differs from ghostty (dropped/changed/reordered logic that alters what happens) → R1 breach, must fix; (2) sanctioned-divergence = a deliberate R2 (identity)/R4 (acyclicity)/Branch-A (alias) restructure — verify it is BEHAVIOR-PRESERVING and documented (if it silently changes behavior it becomes a fidelity-bug); (3) missing-piece = the reference does something rustty omits. Also emit attestations for pieces confirmed faithful.
READ-ONLY: grep/read only, do NOT edit/build/commit. Cite BOTH sides file:line for every finding. Be rigorous — this is the integration seam where subtle ordering/lifetime/behavior drift hides. Return via StructuredOutput.`

phase('Review')
const auditors = [
  { key: 'w-attach-renderer', prompt: `${COMMON}
AREA: W-ATTACH — the renderer host-attach wiring (commit 96b0116). Compare rustty's split (offscreen Metal::init_offscreen at generic.rs, then Metal::attach_view at finalizeSurfaceInit) against ghostty's Metal.init(alloc, opts) which builds the layer WITH the view in one step (src/renderer/Metal.zig ~110-180 init + surfaceInit + generic.zig finalizeSurfaceInit + Surface.zig init ordering). VERIFY LINE-BY-LINE: (a) does rustty's attach_view (metal_impl.rs) do EVERYTHING ghostty's Metal.init view-attach half does — setLayer:/setWantsLayer:/setClipsToBounds:/setContentsScale:/needsDisplayOnBoundsChange, in the SAME order, with the same values? Anything ghostty's init does that the offscreen+attach split misses (e.g. surfaceSize/bounds read during init, device/queue setup that depends on the view, max_texture_size, blending, autoreleasepool)? (b) ORDERING: ghostty attaches the view inside init (before the render thread); rustty attaches at finalizeSurfaceInit before the thread spawn — is the observable sequence identical (does anything between init and finalize read layer, expecting Some)? (c) content_scale SOURCE: ghostty uses opts.rt_surface's content_scale — is rustty reading the same value at the same time (surface.rs host_content_scale = content_scale.x)? (d) does ghostty's init do a first surfaceSize/resize that rustty's split defers? Flag any behavioral drift.` },
  { key: 'cabi-aliases-mainc', prompt: `${COMMON}
AREA: the ghostty_* C-ABI alias layer + the main_c ports (commits 698523d, 454e996, 33da59b, d6a9af3, 2a79346). (1) Alias forwarders (crates/rustty-apprt-c/src/ghostty_alias.rs + rustty-config/src/ghostty_alias.rs): each ghostty_<name> forwards to rustty_<name> — verify the SIGNATURE of each forwarder byte-matches the ghostty export in include/ghostty.h (arg count/types/return), and that it is a pure pass-through (no logic). Sample ~15 across app/config. (2) The main_c ports rustty_info/rustty_translate/rustty_cli_try_action vs src/main_c.zig ghostty_info/translate/cli_try_action — FULL parity: ghostty_info returns Info{mode, version.ptr, version.len} (build mode enum + build_config version) — does rustty match the mode mapping + version source + the Info struct layout? ghostty_translate returns i18n._(msgid) — rustty's identity-return-on-deferred: is the i18n deferral faithful (does ghostty's _ do more that rustty drops)? ghostty_cli_try_action runs state.action orelse return + posix.exit — rustty's GUI-path port: is the null-action return faithful, and is the deferred non-null path a sanctioned divergence (documented) or a silent behavior loss? Flag anything.` },
  { key: 'xcframework', prompt: `${COMMON}
AREA: the xcframework packaging (xtask/src/main.rs xcframework(), commits d4184c3, 9b2890a) vs ghostty's recipe (src/build/GhosttyXCFramework.zig, XCFrameworkStep.zig, LibtoolStep.zig, CombineArchivesStep.zig, GhosttyLib.zig). VERIFY: (a) the libtool combine — does rustty combine the SAME set of archives ghostty's CombineArchivesStep does (the Rust staticlib + which C deps)? ghostty combines libghostty + its C libs; is rustty missing any archive ghostty includes, or including extras? (b) the header — ghostty stages ghostty.h + a module.modulemap; does rustty's match (umbrella header, module name GhosttyKit)? (c) the xcodebuild -create-xcframework invocation flags vs XCFrameworkStep.zig. (d) the slice layout (macos-arm64 vs ghostty's macos-arm64_x86_64 universal) — rustty is arm64-only; is that a sanctioned scope reduction or a parity gap? (e) the panic=abort profile vs ghostty's ReleaseSafe/ReleaseFast — faithful to the no-unwind semantics? Flag divergences from ghostty's build.` },
  { key: 'resources', prompt: `${COMMON}
AREA: the resource generation (xtask/src/res_{terminfo,shell,editor,misc}.rs, commits 269cdb8, 3a3096c, 731f024, 5db4721) vs ghostty's build resource steps (grep ghostty build.zig + src/build/ for terminfo compile, shell-integration bundling, vim/nvim syntax, bash/zsh/fish completions, man/locale install; src/terminfo, src/shell-integration, the extra/ generators). VERIFY the OUTPUT parity: (a) terminfo — does gen_terminfo compile the SAME terminfo source ghostty ships (xterm-ghostty capabilities), to the same share/terminfo/<hash>/ layout? (b) shell completions — do gen_shell's bash/zsh/fish outputs match ghostty's generated completions (paths + content generators)? (c) editor syntax — gen_editor vim/nvim vs ghostty's? (d) share/ghostty root — ghostty puts shell-integration scripts + themes there; does rustty? What's stubbed/empty vs ghostty's real content, and is each stub a documented deferral or a silent gap? This is about whether the app gets the SAME resources ghostty's app gets.` },
  { key: 'host-view-seam', prompt: `${COMMON}
AREA: the host-view seam + the embedded Surface platform handling (apprt-api seam.rs host_view/host_content_scale, apprt/embedded.rs impl, the R4 accessor-trait pattern) vs ghostty's embedded.zig Surface + how it hands the view to the renderer. VERIFY: (a) ghostty's embedded Surface stores platform.macos.nsview (embedded.zig ~343-609) and the renderer reads opts.rt_surface's info.view — does rustty's host_view() return the SAME view (platform.macos.nsview) the reference passes? (b) content_scale — ghostty's rt_surface.content_scale.x; rustty's host_content_scale = content_scale.x — same field, same units? (c) the R4 accessor-trait indirection (host_view on the ApprtSurface trait, default null) — is it behavior-preserving vs ghostty's direct field access, and correct for the macOS backend? (d) does ghostty do anything with the view at surface init that rustty's seam omits (e.g. iOS uiview arm, null-view rejection error.NSViewMustBeSet — does rustty preserve the null-check)? Flag any lifetime/ownership/behavior drift in how the view crosses into the renderer.` },
]
const reports = (await parallel(auditors.map(a => () => agent(a.prompt, { label: `parity:${a.key}`, phase: 'Review', schema: FINDINGS })))).filter(Boolean)
const all = reports.flatMap(r => r.findings.map(f => ({ ...f, area: r.area })))
const real = all.filter(f => (f.kind === 'fidelity-bug' || f.kind === 'missing-piece') && (f.severity === 'blocker' || f.severity === 'high' || f.severity === 'medium'))
log(`Review: ${reports.length}/${auditors.length} areas, ${all.length} findings (${real.length} real high+ to verify; ${all.filter(f=>f.kind==='attestation').length} attestations)`)

phase('Verify')
const verified = await parallel(real.map(f => () =>
  agent(`${COMMON}
ADVERSARIAL VERIFIER. Confirm or refute this claimed parity divergence: re-read BOTH sides, and decide if rustty's behavior REALLY differs from ghostty in a way that matters (or if it's a behavior-preserving sanctioned divergence / the reviewer misread). Default confirmed=false unless the divergence is real and behavior-altering.
FINDING [${f.kind}/${f.severity}] ${f.title}\nRUSTTY: ${f.rustty}\nGHOSTTY: ${f.ghostty}\nDETAIL: ${f.detail}`,
    { label: `verify:${f.title.slice(0,34)}`, phase: 'Verify', schema: VERDICT }).then(v => ({ ...f, verdict: v }))))
const confirmed = verified.filter(Boolean).filter(v => v.verdict && v.verdict.confirmed).map(v => ({ ...v, severity: v.verdict.corrected_severity || v.severity }))
log(`Verify: ${confirmed.length} confirmed real divergences`)

phase('Synthesize')
const synth = await agent(`${COMMON.split('Your job')[0]}
Write /Users/gael/projects/rustty/.port/phase-2/INTEGRATION_PARITY.md — a parity report for the macOS-integration code:
1. **Verdict** — one paragraph: is the integration faithful to ghostty? Any real fidelity bugs?
2. **Confirmed divergences** (verified fidelity-bugs/missing-pieces) — ranked, each with rustty vs ghostty file:line, behavioral consequence, fix sketch.
3. **Sanctioned divergences** — the deliberate R2/R4/Branch-A restructures, each attested behavior-preserving (or flagged if not).
4. **Attestations** — integration pieces confirmed faithful.
5. **Fix list** — the ordered real fixes (if any).

CONFIRMED: ${JSON.stringify(confirmed, null, 1)}
ALL FINDINGS (incl. sanctioned + attestations + unverified low): ${JSON.stringify(reports.map(r => ({ area: r.area, coverage: r.coverage, findings: r.findings })), null, 1)}

Return a <=350-word executive summary: parity verdict, the confirmed real fixes (if any), and confirmation the sanctioned divergences are behavior-preserving.`,
  { label: 'synthesize', phase: 'Synthesize' })

return { areas: reports.length, findings: all.length, confirmed: confirmed.length, summary: synth, doc: '.port/phase-2/INTEGRATION_PARITY.md' }
