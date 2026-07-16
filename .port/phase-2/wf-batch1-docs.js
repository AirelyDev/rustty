export const meta = {
  name: 'batch1-prose-docs-dotfiles',
  description: 'Port ghostty top-level prose/dev docs, dotfiles, nested AGENTS.md + mapped READMEs onto rustty (structure-faithful, reality-accurate, rebranded, Zig->Cargo). One agent per doc-unit; each ports + rebrand-audits its files. NO commits.',
  phases: [{ title: 'Port', detail: 'one agent per doc-unit: read oracle, write rustty file(s), rebrand-audit' }],
}

// Shared preamble injected into every agent prompt. Mirrors the plan's Global
// Constraints (docs/superpowers/plans/2026-07-15-batch1-prose-docs-dotfiles.md).
const PREAMBLE = `You port ONE documentation/config unit from ghostty to rustty. This is Batch 1 of the top-level-files port.

REPO: /Users/gael/projects/rustty (work in the MAIN tree, edit files in place).
ORACLE (READ-ONLY, never modify): /Users/gael/projects/ghostty
FULL PLAN (read your task section for detail): /Users/gael/projects/rustty/docs/superpowers/plans/2026-07-15-batch1-prose-docs-dotfiles.md

RULES:
- DO NOT git add / git commit / git stage anything. Leave changes in the working tree.
- Fidelity contract: STRUCTURE-FAITHFUL, REALITY-ACCURATE (not byte-parity). Keep the same section skeleton/headings as the oracle counterpart; rebrand per-token; translate Zig build commands to Cargo/xtask; adapt sections for infra that differs; NEVER fabricate external values (use clearly-labeled maintainer-provided placeholders).
- REBRAND MAP (product side only; genuine upstream/oracle-provenance refs stay 'ghostty'):
  Ghostty->Rustty, ghostty->rustty, GHOSTTY_->RUSTTY_
  bundle id / gettext domain com.mitchellh.ghostty -> io.airely.rustty
  TERM xterm-ghostty -> xterm-rustty ; env prefix GHOSTTY_ -> RUSTTY_
  config dir ~/.config/ghostty -> ~/.config/rustty ; state ~/.local/state/ghostty -> ~/.local/state/rustty
  C lib libghostty / ghostty-vt -> librustty / rustty-vt ; headers include/ghostty/ -> include/rustty/
  GhosttyKit -> RusttyKit ; xcframework macos/GhosttyKit.xcframework -> dist/RusttyKit.xcframework
  URLs ghostty.org -> rustty.sh ; upstream repo citations MAY remain github.com/ghostty-org/ghostty (provenance)
  crash ext .ghosttycrash -> .rusttycrash
- COMMAND TRANSLATION (Zig->rustty):
  zig build -> cargo build ; zig build run -> cargo run -p rustty ; zig build test -> cargo test (note -p <crate> and 'cargo test <name>' filters)
  zig fmt . -> cargo fmt ; add cargo clippy
  libghostty-vt build -> cargo build -p rustty-vt-c --release [--features simd] ; wasm -> cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown
  zig build dist -> cargo run -p xtask -- dist ; update-translations -> cargo run -p xtask -- update-translations
  macOS lib: zig build -Demit-macos-app=false -> cargo run -p xtask -- xcframework
- ALLOWED residual 'ghostty': legitimate upstream/provenance only ("full-fidelity port of Ghostty", "upstream ghostty", github.com/ghostty-org, oracle paths). Everything product-facing rebrands.

AFTER writing each file, RUN THE REBRAND AUDIT and include its result in your report:
  grep -niE 'ghostty|com\\.mitchellh|ghostty\\.org|zig build|GhosttyKit' <file>
Confirm every remaining hit is legitimate provenance. A stray product ref or stale 'zig build' is a failure to fix before you finish.

YOUR TASK:
`

const SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['task', 'files_written', 'status', 'audit', 'decisions'],
  properties: {
    task: { type: 'string' },
    files_written: { type: 'array', items: { type: 'string' } },
    status: { type: 'string', enum: ['DONE', 'NEEDS_INPUT', 'BLOCKED'] },
    audit: { type: 'string', description: 'per-file audit result: "clean" or the residual grep hits with a one-line justification each' },
    decisions: { type: 'string', description: 'judgment calls, placeholders used (e.g. maintainer-provided DSN/owner-handle), sections trimmed/dropped, or files skipped because target dir absent' },
  },
}

const TASKS = [
  { label: 'T1-AGENTS', text: `Task 1 — top-level AGENTS.md. Oracle: /Users/gael/projects/ghostty/AGENTS.md. Create /Users/gael/projects/rustty/AGENTS.md. Translate the Commands table to cargo/xtask (drop the -Demit-macos-app tip; replace with 'cargo build -p <crate>'). Rename '## libghostty-vt' -> '## librustty-vt' with cargo build/test/wasm commands and the RUSTTY_ENUM_MAX_VALUE sentinel rule + include/rustty/vt/ path. Directory Structure -> the Cargo workspace crates/ (name rustty-vt, rustty-core, rustty-renderer), macos/, and GTK app under crates/rustty-apprt (note: not yet live). Keep the Issue/PR guidelines rebranded. See plan Task 1.` },
  { label: 'T2-README', text: `Task 2 — README.md. Oracle: /Users/gael/projects/ghostty/README.md. Create /Users/gael/projects/rustty/README.md. Reframe as a full-fidelity Rust port of Ghostty (credit upstream). Do NOT hotlink ghostty's logo asset (use a text title or placeholder). Nav/links -> rustty.sh (mark 'coming soon' if not live), CONTRIBUTING.md, HACKING.md. Keep About/Roadmap-table/sub-sections/Crash-Reports structure, rebranded; libghostty->librustty. Crash Reports: .rusttycrash, ~/.local/state/rustty/crash, 'rustty +crash-report'; the Sentry DSN is ghostty's — replace with a maintainer-provided-DSN placeholder (do NOT reuse ghostty's DSN). See plan Task 2.` },
  { label: 'T3-HACKING', text: `Task 3 — HACKING.md. Oracle: /Users/gael/projects/ghostty/HACKING.md. Create /Users/gael/projects/rustty/HACKING.md. Build-steps table -> cargo/xtask. Logging -> RUSTTY_LOG, subsystem io.airely.rustty. Linting (prettier/swiftlint/shellcheck/alejandra) via the Nix dev shell (Batch 4 — label as a forward reference 'once the Nix flake lands') + non-Nix install; add cargo fmt/clippy. DROP the 'Updating the Zig Cache Fixed-Output Derivation Hash' section entirely (crane eliminates it). Memory-leak checking -> rustty Miri/ASan gates (.port/tools/gates/check_leaks.sh) + a Valgrind note for FFI/C paths. Keep Xcode/SDK, IME/input-stack test matrix, and Nix VM sections rebranded (point Nix at Batch 4). See plan Task 3.` },
  { label: 'T4-CONTRIB', text: `Task 4 — CONTRIBUTING.md + AI_POLICY.md. Oracles: /Users/gael/projects/ghostty/{CONTRIBUTING.md,AI_POLICY.md}. Create both at rustty root. AI_POLICY.md: near-verbatim rebrand (drop/replace m@mitchellh.com upstream contact). CONTRIBUTING.md: MINIMAL GOVERNANCE — keep 'understand your code', the AI_POLICY pointer, 'Issues are Actionable', 'PRs Implement an Issue' (rebranded); TRIM the vouch/denounce/Discord/discussion-category machinery to 2-3 sentences pointing at upstream Ghostty governance (rustty is a solo full-fidelity port). Do NOT invent a rustty Discord/vouch list/discussion categories. See plan Task 4.` },
  { label: 'T5-PACKAGING', text: `Task 5 — PACKAGING.md. Oracle: /Users/gael/projects/ghostty/PACKAGING.md. Create /Users/gael/projects/rustty/PACKAGING.md. Source-tarball URLs + minisign key -> rustty release-infra placeholders (do NOT reuse release.files.ghostty.org or ghostty's minisign pubkey). 'Zig Version' -> 'Rust Toolchain' (rust-toolchain.toml). Replace the fetch-zig-cache + 'zig build --prefix --system' recipe with cargo: cargo build -p rustty --release (+ --offline w/ cargo vendor for the no-network step), install via xtask/DESTDIR; mention the crane/Nix path (Batch 4) as the reproducible option. Map --prefix/DESTDIR/-Doptimize/-Dcpu/-Dtarget to cargo profiles + --target + RUSTFLAGS baseline. See plan Task 5.` },
  { label: 'T6-CODEOWNERS', text: `Task 6 — CODEOWNERS (minimal rewrite). Oracle: /Users/gael/projects/ghostty/CODEOWNERS. Create /Users/gael/projects/rustty/CODEOWNERS. Trim the BDFL/maintainer preamble to a few lines (rustty is a solo full-fidelity port; this file documents crate ownership). Map subsystems to rustty CRATE paths (verify each exists with ls): Font->/crates/rustty-font/ /crates/rustty-font-shaper/ ; Renderer->/crates/rustty-renderer/ /crates/rustty-render-types/ ; Terminal->/crates/rustty-vt/ /crates/rustty-vt-c/ /crates/rustty-input/ /crates/rustty-terminfo/ /crates/rustty-termio/ /crates/rustty-unicode/ /crates/rustty-simd/ ; Shell->/crates/rustty-extra/shell-integration/ ; macOS->/crates/rustty-apprt-c/ /crates/rustty-os/ /macos/ /include/rustty/ . Owner = single maintainer; if the handle is unknown use a '# TODO: owner handle' comment (do NOT invent a @rustty-org team). DROP all @ghostty-org/* handles and the per-locale /po/*.po rows. See plan Task 6.` },
  { label: 'T7-DOTFILES', text: `Task 7 — dotfiles. Oracles under /Users/gael/projects/ghostty/. Create at rustty root. Copy VERBATIM: .clang-format, .editorconfig, .shellcheckrc, valgrind.supp. Copy+rebrand .prettierignore (rustty image paths; keep generic ignores, target/). Copy+adjust .swiftlint.yml (must target macos/). Copy+adjust typos.toml (excludes -> crates-sys/*/vendor/, target/, .port/phase-0/** goldens, fonts/images — NOT src/ or *.zon). .mailmap: write a MINIMAL rustty one (port authorship; upstream provenance is in NOTICE). Do NOT touch .gitignore/.gitattributes (already correct). See plan Task 7.` },
  { label: 'T8-NESTED-AGENTS', text: `Task 8 — nested AGENTS.md (6) + fix macos/AGENTS.md. Create: example/AGENTS.md (from ghostty example/AGENTS.md — adapt new-example flow to rustty's c-vt-* build.sh pattern, not build.zig.zon; ghostty-vt->rustty-vt). crates/rustty-bench/AGENTS.md (from src/benchmark/AGENTS.md — methodology verbatim; cargo run -p rustty-bench / rustty-synthetic). crates/rustty-inspector/AGENTS.md (from src/inspector/AGENTS.md — dcimgui-sys crate ref; 'no unit tests' kept; cargo build -p rustty-inspector). crates/rustty-vt/src/apc/glyph/AGENTS.md (from src/terminal/apc/glyph/AGENTS.md — near-verbatim; adjust summary path to the rust module). crates/rustty-vt-c/AGENTS.md (from src/terminal/c/AGENTS.md — rewrite the 4-step @export chain to the cbindgen reality: module -> re-export -> cargo run -p xtask -- header-gen emits include/rustty/vt/ ; keep ABI patterns + RUSTTY_INIT_SIZED). MODIFY macos/AGENTS.md: replace the stale 'zig build -Demit-macos-app=false' line with 'cargo run -p xtask -- xcframework' (produces dist/RusttyKit.xcframework), then macos/build.nu; remove any other zig/ghostty residue. See plan Task 8.` },
  { label: 'T9-READMES', text: `Task 9 — mapped nested READMEs. crates/rustty-cli/README.md (from src/cli/README.md; rebrand + cargo commands). crates-sys/README.md (from pkg/README.md; adapt: the vendored-C pkg/ packages -> the crates-sys/* FFI crates, each vendoring C under vendor/; rebrand). Font READMEs: from src/font/res/README.md and src/font/sprite/draw/README.md -> crates/rustty-font/res/README.md and the sprite-draw location IF it exists under crates/rustty-font/src/ (verify with ls FIRST; skip + report any whose target dir is absent). Do NOT overwrite the existing crates/rustty-extra/shell-integration/README.md or themes/README.md. See plan Task 9.` },
]

phase('Port')
const results = await parallel(
  TASKS.map((t) => () =>
    agent(PREAMBLE + t.text, {
      label: t.label,
      phase: 'Port',
      schema: SCHEMA,
      agentType: 'general-purpose',
      effort: 'medium',
    }).then((r) => (r ? { ...r, label: t.label } : { label: t.label, task: t.label, files_written: [], status: 'BLOCKED', audit: 'n/a', decisions: 'agent returned null' })),
  ),
)

const done = results.filter((r) => r.status === 'DONE')
const needs = results.filter((r) => r.status === 'NEEDS_INPUT')
const blocked = results.filter((r) => r.status === 'BLOCKED')
return {
  summary: `${done.length}/${TASKS.length} DONE, ${needs.length} NEEDS_INPUT, ${blocked.length} BLOCKED`,
  done: done.map((r) => ({ label: r.label, files: r.files_written, audit: r.audit, decisions: r.decisions })),
  needs_input: needs,
  blocked,
}
