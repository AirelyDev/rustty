# Tracked Deferrals — swiftapp-resources / misc (`share/{man,locale,bat}`)

Ledger for the `gen_misc` build-phase resource generator
(`xtask/src/res_misc.rs`, wired into the `resources` subcommand). Disjoint,
per-agent file (avoids clobbering the concurrent generator agents' ledgers).
The macOS `Ghostty.xcodeproj` Copy-Resources phase references
`../zig-out/share/{man,locale,bat}` and only needs the folders to EXIST; each is
emitted minimal-but-VALID (R5) — never faked content.

## 1. man — real man-page generation deferred (mdgen not ported)

ghostty generates man pages from markdown via `src/build/mdgen/mdgen.zig`
(pandoc `--to man`) and installs them to `share/man/man{1,5}/ghostty.{1,5}`
(`GhosttyDocs.zig`). rustty's `rustty-helpgen` ports only the doc-comment
**extraction** primitives (`extractDocComments`/`findCommonPrefix`), NOT the
mdgen markdown→roff pipeline. On the macOS-app path ghostty ITSELF emits no man
pages — `GhosttyDocs.installDummy` (invoked from build.zig:88-96 when `emit-docs`
is unset) writes `share/man/.placeholder` with the exact bytes
`"emit-docs not true so no man pages"`. `gen_misc` replicates that byte-faithful
fallback.

**Un-defer trigger:** port `mdgen` (+ the `ghostty_1`/`ghostty_5` markdown
header/footer fixtures) → real `share/man/man{1,5}` pages.

## 2. locale — i18n .mo catalogs deferred (BREAK)

ghostty compiles gettext `.po` → `.mo` into
`share/locale/<lang>/LC_MESSAGES/com.mitchellh.ghostty.mo`
(`GhosttyI18n.zig`, path template `share/locale/{s}/LC_MESSAGES/{s}.mo`).
rustty i18n is a deferred BREAK: no `.po` corpus and no `msgfmt` compile step
are ported. `gen_misc` emits a minimal-valid `share/locale/.placeholder` only;
no `.mo` is fabricated (never fake content, R5).

**Un-defer trigger:** port the i18n `.po` corpus + `msgfmt` compile pipeline.

## 3. bat — REAL content (NOT deferred), self-contained embedding note

The sublime-syntax generator IS ported: `rustty_extra::sublime::syntax()`,
pinned byte-exact to ghostty's `ghostty.sublime-syntax` by its
`syntax_matches_golden` test. `gen_misc` writes REAL content to
`share/bat/syntaxes/ghostty.sublime-syntax` (ghostty's install layout,
`GhosttyResources.zig`), embedding the committed golden source of that generator
via `include_bytes!` — keeping `gen_misc` pure-`std` (no xtask→rustty-extra
Cargo dep) so it compiles regardless of the concurrent Cargo.toml edits by the
sibling resource-generator agents.

**Optional follow-up (content is already real & valid — this is NOT a content
deferral):** the land agent MAY switch to a live `rustty_extra::sublime::syntax()`
call once `rustty-extra` is a wired xtask dependency; the emitted bytes are
identical either way (guaranteed by the generator's golden test).
