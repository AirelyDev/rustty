export const meta = {
  name: 'swiftapp-resources',
  description: 'Generate the macOS app resource bundle (zig-out/share/{terminfo,ghostty,shell,editor,man,locale,...}) via an xtask `resources` subcommand, then re-attempt the xcodebuild link',
  whenToUse: 'macOS Swift-app milestone Phase B3: the app links but Copy-Resources needs zig-out/share',
  phases: [
    { title: 'Generate', detail: 'terminfo, shell-completions, editor-syntax, man/locale — parallel generators' },
    { title: 'Land+Build', detail: 'wire xtask resources subcommand, run it, re-attempt xcodebuild' },
  ],
}

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'blocked', 'partial'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' },
    produced_dirs: { type: 'array', items: { type: 'string' } }, xtask_fn: { type: 'string' },
    blocker: { type: 'string' }, notes: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. READ /Users/gael/projects/ghostty (oracle @ 4749c4e9) but NEVER modify it. Context: the macOS Swift app (macos/) LINKS against rustty's GhosttyKit.xcframework; the last gate is the Xcode Copy-Resources phase which needs folders at rustty/zig-out/share/{terminfo,ghostty,vim,nvim,fish,zsh,bash-completion,man,locale,bat} (referenced as ../zig-out/share/<name> from macos/Ghostty.xcodeproj). ghostty's zig build produces these; rustty has generators: rustty-terminfo (entry::rustty()/ghostty() -> Source, Source::encode(w) writes tic-compilable text; identity-ghostty feature emits xterm-ghostty), rustty-extra (bash.rs/zsh.rs write_*_completions, vim.rs/sublime.rs syntax generators), rustty-helpgen (man/help). Each resource is a folder Xcode copies; terminfo is runtime-critical, the rest are build-phase folders (empty-but-present is acceptable for MVP where a generator isn't ported).
HARD: R1/R2 (identity — TERM=xterm-rustty is canonical, but the reused Swift app hard-codes xterm-ghostty in its Ghostty.app usage; use identity-ghostty terminfo names so the app's TERM matches — document the R2 note); R5 (if a generator isn't ported, create a minimal VALID folder + a DEFERRALS entry, never fake content). Git: explicit paths, index.lock retry, no push/reset/stash. Each agent writes its generation into a NEW disjoint xtask module file (xtask/src/res_<name>.rs) exporting one pub fn that writes its share/ subtree; the land agent wires the subcommand. Return via StructuredOutput; COMMIT own files.`

phase('Generate')
const gens = [
  {
    key: 'terminfo', label: 'terminfo + share/ghostty root',
    prompt: `${COMMON}
TASK: generate the terminfo resource (runtime-critical) + the share/ghostty root. Create xtask/src/res_terminfo.rs with pub fn gen_terminfo(share_dir: &Path) -> Result<(), String>: (1) get the terminfo Source via rustty_terminfo::entry (use the identity-ghostty spelling xterm-ghostty so the reused app's TERM resolves — cite the R2 note that xterm-rustty is canonical); Source::encode into a String; write it to a temp .terminfo file; run tic (/usr/bin/tic) with -x -o <share>/terminfo to compile it into share/terminfo/x/xterm-ghostty (and 78/... layout tic produces). (2) Create share/ghostty/ and populate what belongs there — at minimum the shell-integration scripts if rustty ships them (search rustty-extra / crates for shell integration bash/zsh/fish/elvish scripts; ghostty puts them under share/ghostty/shell-integration/). If shell-integration scripts aren't ported as data, create share/ghostty/ with a minimal valid structure + a DEFERRALS note. Verify tic produces the compiled terminfo (ls share/terminfo). Add xtask-dep on rustty-terminfo if needed (Cargo.toml). Commit res_terminfo.rs (+ Cargo.toml). Report produced_dirs + xtask_fn=gen_terminfo.`,
  },
  {
    key: 'shell', label: 'shell completions bash/zsh/fish',
    prompt: `${COMMON}
TASK: generate shell completions. Create xtask/src/res_shell.rs with pub fn gen_shell(share_dir: &Path) -> Result<(), String> that writes share/{bash-completion,zsh,fish} using rustty-extra's generators (bash.rs write_bash_completions, zsh.rs write_zsh_completions, and fish if present). Match ghostty's layout: share/bash-completion/completions/ghostty, share/zsh/site-functions/_ghostty, share/fish/vendor_completions.d/ghostty.fish (verify the exact paths against ghostty's build — grep its build for bash-completion/zsh/fish install paths). If a shell's generator isn't ported, create the folder minimally + DEFERRALS note (R5). Add xtask dep on rustty-extra if needed. Verify the files are non-empty where the generator exists. Commit. Report produced_dirs + xtask_fn=gen_shell.`,
  },
  {
    key: 'editor', label: 'editor syntax vim/nvim',
    prompt: `${COMMON}
TASK: generate editor syntax. Create xtask/src/res_editor.rs with pub fn gen_editor(share_dir: &Path) -> Result<(), String> that writes share/{vim,nvim} using rustty-extra's vim/nvim syntax generators (vim.rs). Match ghostty's install layout (grep ghostty build for vim/nvim paths — typically share/vim/vimfiles/syntax/ghostty.vim + share/nvim/site/syntax/ghostty.vim etc.). If nvim reuses vim output, mirror faithfully. Missing generator → minimal folder + DEFERRALS. Add rustty-extra dep if needed. Commit. Report produced_dirs + xtask_fn=gen_editor.`,
  },
  {
    key: 'misc', label: 'man/locale/bat',
    prompt: `${COMMON}
TASK: generate the remaining build-phase resources: share/{man,locale,bat}. Create xtask/src/res_misc.rs with pub fn gen_misc(share_dir: &Path) -> Result<(), String>: (1) man — if rustty-helpgen produces man pages, write them to share/man/man1/ etc.; else minimal valid share/man/ + DEFERRALS. (2) locale — i18n .mo files; rustty's i18n is likely deferred (BREAK) — create share/locale/ minimally + DEFERRALS note. (3) bat — bat syntax/theme; if rustty-extra/sublime produces it, write share/bat/; else minimal + DEFERRALS. These are NOT runtime-critical for a basic terminal window; the goal is the folders EXIST so Xcode Copy-Resources succeeds (R5: minimal-valid + ledgered, never faked content). Commit. Report produced_dirs + xtask_fn=gen_misc.`,
  },
]
const res = await parallel(gens.map(g => () => agent(g.prompt, { label: g.label, phase: 'Generate', schema: OUT })))
const good = res.filter(Boolean)
log(`Generate: ${good.filter(r => r.status === 'done').length}/${gens.length} done; fns: ${good.map(r => r.xtask_fn).filter(Boolean).join(',')}`)

phase('Land+Build')
const land = await agent(`${COMMON}
TASK: wire the xtask "resources" subcommand and re-attempt the app build.
1. In xtask/src/main.rs: add a "resources" subcommand + a fn resources() that creates zig-out/share/ (at workspace root) and calls each generator: ${good.map(r => r.xtask_fn).filter(Boolean).join(', ')} (declared as mod res_terminfo; etc.). Each was committed as its own xtask/src/res_*.rs by the generator agents.
2. cargo build -p xtask green; cargo run -p xtask -- resources → creates rustty/zig-out/share/{terminfo,ghostty,vim,nvim,fish,zsh,bash-completion,man,locale,bat}. ls each to confirm present.
3. Re-attempt the app build: cd macos && xcodebuild -project Ghostty.xcodeproj -scheme Ghostty -configuration Debug -destination 'platform=macOS,arch=arm64' CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO build 2>&1 | grep -E "error:|Undefined symbol|BUILD SUCCEEDED|BUILD FAILED" | head -30.
4. Report the outcome precisely: BUILD SUCCEEDED, or the exact remaining errors (which resource still missing / which new error class — link vs resource vs Swift). Gitignore zig-out/ (regenerable). Commit the xtask wiring + gitignore. If BUILD SUCCEEDED, note it prominently. Report produced state + remaining errors verbatim.`,
  { label: 'land+build', phase: 'Land+Build', schema: OUT })

return {
  generate: good.map(r => ({ t: r.task, s: r.status, dirs: r.produced_dirs })),
  land: land ? { status: land.status, notes: land.notes } : null,
  summary: `resources ${good.filter(r => r.status === 'done').length}/${gens.length}; build: ${land ? (land.notes || land.evidence || '').slice(0, 400) : 'land failed'}`,
}
