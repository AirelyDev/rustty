export const meta = {
  name: 'p1-resources',
  description: 'Plan P1: ship all macOS app resources as rustty data — shell-integration, themes, man pages, bat syntax, terminfo source/termcap — functional like ghostty',
  whenToUse: 'Executes docs/superpowers/plans/2026-07-12-macos-app-full-functionality.md Phase 1',
  phases: [
    { title: 'Resources', detail: 'P1.1 shell-integration, P1.2 themes, P1.3 man+bat+terminfo-src — parallel vendor+emit' },
  ],
}

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' },
    residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle (READ-ONLY, never modify): /Users/gael/projects/ghostty @ 4749c4e9. Plan: docs/superpowers/plans/2026-07-12-macos-app-full-functionality.md (read your task's section). The macOS app renders; these resource stubs ship dark. The xtask "resources" subcommand (xtask/src/main.rs resources() + res_terminfo.rs/res_shell.rs/res_editor.rs/res_misc.rs) generates zig-out/share/. The Xcode Copy-Resources phase copies zig-out/share/{terminfo,ghostty,vim,nvim,fish,zsh,bash-completion,man,locale,bat}.
RULES: R1 fidelity (vendored data byte-identical to ghostty's); R2 (product is "Rustty" but PROTOCOL/DATA identity ghostty keeps its names — TERM=xterm-ghostty, share/ghostty/, OSC ids — because the reused Swift app + emitted escape sequences expect them; document each kept name in a header comment); R3 self-contained (vendor ghostty's data into rustty's tree, NO build/run dep on the ghostty checkout); R5 never fake content. Each agent edits ONLY its own files (disjoint) — do NOT edit the shared xtask resources() fn dispatcher; add your generation as a new pub fn in your own res module + note in your report which line resources() should call (a follow-up wires it, OR wire it yourself if you can do so without racing — prefer: append your call at the end of resources() with index.lock retry). Git: explicit paths, index.lock retry (sleep 3, ≤10x), no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Resources')
const tasks = [
  {
    key: 'p1.1-shell', label: 'P1.1 shell-integration',
    prompt: `${COMMON}
TASK P1.1: vendor + emit shell-integration. (1) Vendor ghostty's src/shell-integration/{bash,zsh,fish,elvish,nushell} tree (8 top files + subtrees) VERBATIM into a rustty data dir — check how rustty already vendors static assets (crates-sys vendor/ dirs, or crates/rustty-extra) and place it consistently, e.g. crates/rustty-extra/shell-integration/ or a dedicated assets dir; add a header/README noting "vendored ghostty shell-integration; protocol/OSC identifiers + xterm-ghostty kept (R2 data identity); product is rustty". (2) In xtask (a new xtask/src/res_shell_integration.rs OR extend res_terminfo.rs gen_ghostty_root), copy the vendored tree into zig-out/share/ghostty/shell-integration/ (exclude .md). Wire it into resources(). (3) Mark zig-out/share/ghostty/DEFERRALS.md shell-integration RESOLVED. Verify: cargo run -p xtask -- resources then ls -R zig-out/share/ghostty/shell-integration shows the real non-empty scripts matching ghostty's file set. Commit ("resources: ship ghostty shell-integration (bash/zsh/fish/elvish/nushell) — R3 vendored (P1.1)").`,
  },
  {
    key: 'p1.2-themes', label: 'P1.2 themes corpus',
    prompt: `${COMMON}
TASK P1.2: vendor + emit the themes corpus. (1) Locate ghostty's shipped themes — its build copies the iTerm2-Color-Schemes-derived theme .conf files to share/ghostty/themes (grep ghostty build.zig + src for "themes"; the corpus may live in a ghostty submodule/vendor dir or be generated). Vendor them VERBATIM into a rustty data dir (crates/rustty-extra/themes/ or an assets dir), with a header noting the vendored-ghostty-themes data identity (R2/R3). (2) In xtask (new res module or extend), copy them into zig-out/share/ghostty/themes/ + wire into resources(). Verify the file count matches ghostty's shipped set. Verify: cargo run -p xtask -- resources then ls zig-out/share/ghostty/themes | wc -l equals ghostty's count. Commit ("resources: ship the ghostty themes corpus — list_themes + theme setting functional (P1.2)"). If the corpus is a git submodule not present in the ghostty checkout, report blocked with exactly where ghostty sources it.`,
  },
  {
    key: 'p1.3-misc', label: 'P1.3 man/bat/terminfo-src',
    prompt: `${COMMON}
TASK P1.3: man pages + bat syntax + terminfo source/termcap. (1) MAN: ghostty ships generated man pages (share/man/man{1,5}/ghostty.*). Find how ghostty generates them (build.zig man step, likely from src docs via a generator or vendored roff); either port rustty's man generation (rustty-helpgen may cover help→man) or vendor the man sources + generate with mandoc/nroff. Emit into zig-out/share/man/. Verify a page renders: man -M zig-out/share/man ghostty. (2) BAT: ghostty ships a bat syntax under share/bat/ (rustty-extra sublime generator may produce the .sublime-syntax). Emit it; verify non-empty. (3) TERMINFO SOURCE (INTEGRATION_PARITY §2.8): emit ghostty.terminfo (source) + termcap alongside the compiled DB in res_terminfo.rs so Debug builds + infocmp match ghostty. Edit xtask/src/res_misc.rs + res_terminfo.rs. Wire into resources() if not already. Commit ("resources: man pages + bat syntax + terminfo source/termcap — resource parity (P1.3)"). Report what you ported vs vendored vs deferred (with DEFERRALS entry for any genuine deferral, R5).`,
  },
]
const res = (await parallel(tasks.map(t => () => agent(t.prompt, { label: t.label, phase: 'Resources', schema: OUT })))).filter(Boolean)
log(`P1: ${res.filter(r => r.status === 'done').length}/${tasks.length} done`)
return { items: res.map(r => ({ t: r.task, s: r.status, ref: r.commit_ref, residual: r.residual })), summary: res.map(r => `${(r.task||'').slice(0,20)}:${r.status}`).join(' | ') }
