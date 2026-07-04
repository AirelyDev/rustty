export const meta = {
  name: 'help-strings',
  description: 'Build the deferred help_strings doc-table generator (extract Config-field + keybind-Action doc-comments), un-defer explain_config test + wire FileFormatter --docs / list_keybinds --docs / +help. Architect -> builder -> adversary.',
  phases: [ { title: 'Design' }, { title: 'Build' }, { title: 'Verify' } ],
}

const CONTEXT = [
  'Project: rustty, a faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test COUNT is a contract; R3 byte-faithful every line and comment; R2 identity via rustty-identity; R5 never weaken). Gates: check_acyclic.sh, check_test_counts.py, scoped-G3b.',
  '',
  'TASK CONTEXT: ghostty generates a compile-time doc table `help_strings` (src/build/HelpStrings.zig, 56 loc, + its helpgen step) by extracting the /// doc-comments above each Config field (config/Config.zig) and each keybind Action variant (input/Binding.zig) into `help_strings.Config.<field>` and `help_strings.KeybindAction.<action>`. In rustty this is currently DEFERRED: crates/rustty-config/src/formatter_file.rs:142 has an unimplemented!("FileFormatter docs mode requires help_strings (deferred)"); crates/rustty-input/src/helpgen_actions.rs has a `NoHelpStrings` no-op seam trait; and crates/rustty-cli/src/explain_config.rs has 1 test DEFERRED (SkipZigTest-style, body preserved) because explainOption("font-size")/explainKeybind("copy_to_clipboard") need those tables.',
  '',
  'GOAL: implement help_strings faithfully so those tables exist, then un-defer: (a) the explain_config test (the R1 deferral to close); (b) FileFormatter --docs mode (config); (c) list_keybinds --docs + the +help action text (cli, 0-test consumers). The doc-comments the reference extracts MUST already be present in the ported rustty source (R3 requires every comment ported; the scoped-G3b gate enforced it) — verify and extract them.',
  '',
  'LANDED BASE (all gate-green): config (190t, 203 Config fields), input (113t, the keybind Action enum), cli (44t + the deferred explain_config test), all committed.',
].join('\n')

phase('Design')
const design = await agent(CONTEXT + '\n\n' + [
  'You are the ARCHITECT. Produce a CONCISE blueprint (write to /Users/gael/projects/rustty/.port/phase-2/HELP_STRINGS_BLUEPRINT.md; also return as final text) for implementing help_strings. Ground by READING: ghostty src/build/HelpStrings.zig + how it is generated (the helpgen AST-extraction step; grep for helpgen), how config/CApi/FileFormatter and cli/explain_config/list_keybinds/help consume help_strings.Config/KeybindAction; and the rustty source to CONFIRM the /// doc-comments above Config fields + keybind Action variants were actually ported (grep config_struct.rs / the Config field declarations and input binding.rs Action variants for the doc-comments). Cover:',
  '1. WHERE the doc-comments live in the rustty source and whether they are present (they must be, per R3/G3b) — cite examples (font-size field comment, copy_to_clipboard action comment).',
  '2. The GENERATION approach decision: (a) a build.rs codegen that parses the Rust source doc-comments into a generated table in OUT_DIR (closest to ghostty build-time helpgen), (b) a proc-macro, or (c) a maintained data module. Recommend one with rationale (faithfulness to the reference build-time extraction, maintainability, the R3 requirement that the strings equal the ported comments). Note: the reference extracts EXACTLY the doc-comment text; the rustty table must equal the ported doc-comment text (self-consistent).',
  '3. Placement: which crate owns the generator + the tables (config for Config table, input for KeybindAction table? or a shared helpgen). How the config `NoHelpStrings`/formatter_file seam + the input `HelpStrings` trait get their real impl. Acyclicity.',
  '4. The un-defers enabled: explain_config test (the contract one), FileFormatter --docs, list_keybinds --docs, +help — which land in this milestone.',
  '5. Test/verification: the explain_config test is the R1 proof (must pass, un-deferred); any rustty-only generator tests; how the generated table stays consistent with the source comments (a check).',
  '6. Risks (doc-comment format edge cases; the config field vs Key-registry mapping; keybind Action doc-comment presence; build.rs vs committed-generated).',
  'Be decisive and tight.',
].join('\n'), { label: 'architect', phase: 'Design' })

phase('Build')
const build = await agent(CONTEXT + '\n\n' + 'DESIGN (follow it):\n' + design + '\n\n' + [
  'You are the BUILDER. Implement the help_strings blueprint above end to end. Faithfully port src/build/HelpStrings.zig + its extraction, build the help_strings.Config + help_strings.KeybindAction tables per the chosen approach, replace the config formatter_file.rs unimplemented!() docs path + the input NoHelpStrings seam with the real tables, and UN-DEFER the cli explain_config test (restore it as a live #[test] — remove the SkipZigTest-style gate; its body asserts explainOption("font-size")!=null and explainKeybind("copy_to_clipboard")!=null). Also wire the 0-test consumers (FileFormatter --docs, list_keybinds --docs, +help) so they use the real tables.',
  'GATES (FOREGROUND, all green before commit): cargo build --workspace; cargo test -p rustty-cli (now 45 — explain_config un-deferred; verify it passes for real); cargo test -p rustty-config + -p rustty-input (no regression); NO workspace regression (vt 2019); clippy --all-targets -D warnings on touched crates; fmt --check; check_acyclic.sh GREEN; check_test_counts.py GREEN (flip explain_config apc_deferred 1->0 and confirm cli=45); scoped-G3b on HelpStrings.zig + touched files missing=0; update DEFERRALS.md (help_strings RESOLVED; explain_config un-deferred).',
  'Commit "rustty: help_strings doc-table generator + un-defer explain_config + --docs/+help" with trailer line: Co-Authored-By: Claude Opus 4.8 (1M context). Stage only your files + gate edits. If a genuine blocker appears (e.g. doc-comments NOT present in the ported source), STOP and report precisely rather than fabricating help text. Return the structured outcome (what you built, explain_config passing?, gates).',
].join('\n'), { label: 'builder', phase: 'Build' })

phase('Verify')
const verdict = await agent(CONTEXT + '\n\n' + [
  'You are the ADVERSARIAL REVIEWER. The help_strings implementation just landed (read the latest commit + the touched files: config formatter_file.rs, input helpgen_actions.rs, the generator, cli explain_config.rs). Verify independently:',
  '1. Is the explain_config test genuinely UN-DEFERRED and passing for REAL (not stubbed/weakened)? Run cargo test -p rustty-cli and confirm cli=45 with explain_config among them; read the test to confirm its assertions are the reference ones (explainOption/explainKeybind non-null).',
  '2. FAITHFULNESS: does the generated help text EQUAL the ported source doc-comments (R3)? Spot-check 3 config fields + 2 keybind actions: the help_strings entry must match the /// comment text in the rustty source, which must match the ghostty reference. Any fabricated/paraphrased/empty help text is a FAIL.',
  '3. Coverage: does the table cover ALL Config fields + ALL keybind actions the reference help_strings covers (not a subset)? Any silently-missing entries?',
  '4. No regression + gates: vt 2019, config/input unchanged, acyclic + count-gate GREEN, no new unimplemented!() gaps.',
  '5. Any cycle / build.rs determinism / R2 issue.',
  'Return numbered findings (CONFIRMED-ISSUE with evidence / FALSE-ALARM) + verdict: ACCEPT / ACCEPT-WITH-FIXES (list) / REJECT. Do NOT edit code.',
].join('\n'), { label: 'adversary', phase: 'Verify' })

return { design_len: (design || '').length, build, verdict }
