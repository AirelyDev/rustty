export const meta = {
  name: 'batch6-i18n-pipeline',
  description: 'Batch 6 i18n reasoning work in parallel: port xtask update-translations (GhosttyI18n.zig pipeline), un-defer the res_misc .mo compilation, and port the two translator READMEs. The 31 .po + .pot are already ported deterministically. NO commits.',
  phases: [{ title: 'i18n', detail: 'update-translations | .mo compile | translator READMEs (parallel)' }],
}

const PREAMBLE = `Batch 6 = full i18n for rustty. CONTEXT you can rely on:
- REPO: /Users/gael/projects/rustty (edit in place). ORACLE (READ-ONLY): /Users/gael/projects/ghostty.
- The 31 locale catalogs + template ALREADY EXIST and are msgfmt-clean: po/*.po + po/io.airely.rustty.pot (domain rebranded com.mitchellh.ghostty -> io.airely.rustty; app-name Ghostty -> Rustty; translator emails preserved). Do NOT re-port or modify them.
- The RUNTIME i18n is ALREADY PORTED: crates/rustty-os/src/{i18n.rs,i18n_locales.rs,locale.rs}. \`_()\` -> \`dgettext(BUNDLE_ID, msgid)\`; \`init\` -> \`bindtextdomain(BUNDLE_ID, <share>/locale)\`. BUNDLE_ID = io.airely.rustty (via rustty-build-config). Do NOT modify the runtime unless your task explicitly says to.
- gettext tools are at /opt/homebrew/opt/gettext/bin (msgfmt, msgmerge, xgettext). Prepend to PATH.
- The compiled .mo domain filename is \`io.airely.rustty.mo\` (matches BUNDLE_ID), installed to share/locale/<lang>/LC_MESSAGES/io.airely.rustty.mo.
- rustty's GTK apprt is NOT live yet, so xgettext over rustty source finds almost no translatable strings (the UI .blp/.zig call sites don't exist). This means REGENERATING the .pot now would obsolete the ported catalogs — your code must be IMPLEMENTED and its invocation validated, but you must NOT run it against po/*.po in a way that guts them.
- RULES: NO git commit. Rebrand map: Ghostty->Rustty, ghostty->rustty, com.mitchellh.ghostty->io.airely.rustty, ghostty.org->rustty.sh, repo github.com/AirelyDev/rustty. Faithfully port from the oracle.
- Rust style: match the surrounding xtask code (Result<PathBuf,String> subcommands, workspace_root(), std::process::Command). Run cargo fmt + cargo build -p xtask to verify your Rust compiles.

YOUR TASK:
`

const SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['task', 'files', 'status', 'verification', 'notes'],
  properties: {
    task: { type: 'string' },
    files: { type: 'array', items: { type: 'string' } },
    status: { type: 'string', enum: ['DONE', 'BLOCKED'] },
    verification: { type: 'string', description: 'the exact command(s) you ran to verify + their result (build OK, .mo files produced, msgfmt clean, etc.)' },
    notes: { type: 'string', description: 'judgment calls, the deferred-regeneration guard, anything the integrator should know' },
  },
}

const TASKS = [
  {
    label: 'update-translations',
    text: `Implement the \`update-translations\` xtask subcommand — a faithful port of ghostty's \`src/build/GhosttyI18n.zig\` \`createUpdateStep\` (READ IT: /Users/gael/projects/ghostty/src/build/GhosttyI18n.zig).

Ghostty's pipeline: (1) xgettext --language=C --from-code=UTF-8 --keyword=_ --keyword=C_:1c,2 over the GTK Blueprint (.blp) files + every .zig under src/apprt/gtk -> gtk.pot; (2) xgettext --language=Python over dist/linux/ghostty_nautilus.py -> py.pot; (3) merge py.pot + gtk.pot with --add-comments=Translators --package-name=com.mitchellh.ghostty --msgid-bugs-address=m@mitchellh.com --copyright-holder=... -> po/com.mitchellh.ghostty.pot; (4) msgmerge --quiet --no-fuzzy-matching each po/<locale>.po against the merged pot.

Port it to rustty as a new xtask module (e.g. xtask/src/translations.rs) + a \`update-translations\` dispatch arm in xtask/src/main.rs + a USAGE line, adapting:
- package-name -> io.airely.rustty ; msgid-bugs-address -> gael@airely.io ; output -> po/io.airely.rustty.pot ; msgmerge over po/*.po.
- The Python input is our dist/linux/rustty_nautilus.py (exists).
- The GTK Blueprint/.rs sources: rustty's GTK apprt is a placeholder, so scan whatever GTK source exists under crates/rustty-apprt (glob *.blp and the relevant .rs; if none exist yet, the gtk.pot leg is effectively empty — that is EXPECTED and fine). Use the gettext keyword \`_\` (and \`C_:1c,2\` for context) matching rustty-os's \`_()\`/\`C_()\` wrappers.
- Resolve tools from PATH (msgfmt/msgmerge/xgettext); document that /opt/homebrew/opt/gettext/bin may need to be on PATH on macOS.

CRITICAL GUARD: because rustty's GTK UI is not live, running this now would regenerate a near-empty pot and msgmerge would OBSOLETE the ported catalogs. So: IMPLEMENT the subcommand, and VERIFY it BUILDS (cargo build -p xtask) and that the xgettext/msgmerge invocations are correct — but DO NOT run \`update-translations\` against the real po/ (do not overwrite po/*.po or po/io.airely.rustty.pot). If you want to prove the pipeline runs, do it in a throwaway temp dir on copies, never on the tracked po/. Add a doc comment explaining this deferred-regeneration reality (regeneration is meaningful once the GTK apprt UI is live). Report the files + your build verification.`,
  },
  {
    label: 'mo-compile',
    text: `Un-defer the \`.mo\` compilation in xtask. Currently xtask/src/res_misc.rs treats \`share/locale\` as a deferred BREAK (emits only a .placeholder, no .mo) — READ its doc comment (the "locale" deferral, ~lines 48-60). The un-defer trigger is now met: po/*.po exist.

Change \`gen_misc\` (res_misc.rs) so that instead of the placeholder, it compiles every po/<locale>.po into share/locale/<locale>/LC_MESSAGES/io.airely.rustty.mo using \`msgfmt\`. Mirror ghostty's install step (GhosttyI18n.zig \`init\`: \`msgfmt -o - po/<locale>.po\` captured to share/locale/<locale>/LC_MESSAGES/<domain>.mo). Domain filename = io.airely.rustty.mo (matches BUNDLE_ID). The <locale> is the .po basename (de, ja, zh_CN, ...). Iterate po/*.po (skip the .pot and the README*.md). Use \`Command::new("msgfmt")\` (resolve from PATH; note /opt/homebrew/opt/gettext/bin on macOS). Keep it self-contained (std only), matching the existing res_misc style. Update the module doc comment (remove the "deferred BREAK", describe the real compile).

VERIFY: run \`cargo run -p xtask -- resources\` (with gettext on PATH) and confirm real .mo files appear, e.g. \`ls zig-out/share/locale/de/LC_MESSAGES/io.airely.rustty.mo\` and that \`msgunfmt\`/\`file\` shows it's a valid GNU message catalog. Report the exact ls + a couple of the produced .mo paths.`,
  },
  {
    label: 'translator-readmes',
    text: `Port the two translator guide docs into po/: README_TRANSLATORS.md and README_CONTRIBUTORS.md, from /Users/gael/projects/ghostty/po/{README_TRANSLATORS.md,README_CONTRIBUTORS.md}.

Rebrand faithfully (Ghostty->Rustty, ghostty->rustty, com.mitchellh.ghostty->io.airely.rustty, ghostty.org->rustty.sh, github.com/ghostty-org/ghostty->github.com/AirelyDev/rustty, the .pot filename com.mitchellh.ghostty.pot->io.airely.rustty.pot, the update-translations command \`zig build update-translations\`->\`cargo run -p xtask -- update-translations\`). Keep the Style Guide + the how-to-contribute-a-translation structure. Per rustty's MINIMAL-governance decision, trim any vouch-system / Discord references to a short pointer at upstream Ghostty governance (do not invent a rustty Discord/vouch list). Add a short NOTE that the catalogs currently hold the upstream UI-string translations and are regenerated (via update-translations) once rustty's GTK UI is live.

VERIFY: \`grep -niE 'ghostty|zig build|com\\.mitchellh' po/README_TRANSLATORS.md po/README_CONTRIBUTORS.md\` and confirm every remaining hit is legitimate upstream provenance. Report the files + the audit result.`,
  },
]

phase('i18n')
const results = await parallel(
  TASKS.map((t) => () =>
    agent(PREAMBLE + t.text, { label: t.label, phase: 'i18n', schema: SCHEMA, effort: 'high' })
      .then((r) => (r ? { ...r, label: t.label } : { label: t.label, task: t.label, files: [], status: 'BLOCKED', verification: 'n/a', notes: 'agent returned null' })),
  ),
)

const done = results.filter((r) => r.status === 'DONE')
const blocked = results.filter((r) => r.status === 'BLOCKED')
return {
  summary: `${done.length}/${TASKS.length} DONE, ${blocked.length} BLOCKED`,
  done: done.map((r) => ({ label: r.label, files: r.files, verification: r.verification, notes: r.notes })),
  blocked,
}
