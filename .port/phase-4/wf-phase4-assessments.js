export const meta = {
  name: 'phase4-assessments',
  description: 'Phase-4 upstream resync: per-area re-vetting assessments of 6ad1fe7d8..12967b68f',
  phases: [
    { title: 'Assess', detail: 'seven parallel area agents, one assessment doc each' },
    { title: 'Reconcile', detail: 'coverage critic: every changed file owned by exactly one area' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const RANGE = '6ad1fe7d8..12967b68f'

const COMMON = `
You are a Phase-4 re-vetting assessment agent for the rustty port (full-fidelity Zig->Rust port
of ghostty). Repo: ${REPO}. Read-only oracle: ${REF} — its working tree is NOW CHECKED OUT AT
THE NEW PIN 12967b68f (= origin/main), so you can read target-state files directly. NEVER write
to the oracle repo, never run git checkout/pull/reset there. Intermediate states of superseded
commits: git -C ${REF} show <hash>:<path>.

READ FIRST (methodology + format precedent):
- ${REPO}/.port/phase-4/README.md   (the range, binding rules)
- ${REPO}/.port/PORT_RULES.md       (R1/R2/R5/R7/R9/R11/R12 — binding)
- The Phase-3 assessment for your area in ${REPO}/.port/phase-3/assessments/ (format precedent)
- ${REPO}/.port/phase-4/UPSTREAM_DIFF_STAT.txt and UPSTREAM_COMMITS.txt

YOUR JOB — for YOUR AREA's file set only:
1. Enumerate the range's commits touching your files:
   git -C ${REF} log --oneline --no-merges ${RANGE} -- <your paths>
2. For EVERY commit: read its hunks (git -C ${REF} show <hash> -- <paths>), locate the
   corresponding rustty code, and give a verdict: PORTED / PARTIAL / ABSENT / N-A.
   - PORTED requires evidence: rustty symbol + file:line + the test/gate that would fail on
     regression. Hash citations in comments are NOT evidence in either direction (measured).
   - N-A requires a reason (no Rust analogue, pkg/CI-only, reverted within range, etc.).
     A commit REVERTED inside the range: verdict N-A with the reverting hash.
   - Where several commits rewrite the same function, assess the END STATE but list every
     commit and note which tests each contributes.
3. Identify ATOMIC BUNDLES (commits that must land together or be worse than absent),
   cross-area COUPLINGS (name the other area), and any UB/SB hazards for rustty's
   raw-pointer structures (Page/Row/Cell/Screen aliasing — see PORT_RULES).
4. State COVERAGE HONESTY explicitly: what you read line-by-line vs sampled. Prefer
   exhaustive; if you sample a mechanical class, say exactly what filter you used and on
   which files.
5. Note dropped-test targets from ${REPO}/.port/phase-4/w0/g1b-after-at-12967b68f.txt that
   belong to your area.
6. Write your assessment to ${REPO}/.port/phase-4/assessments/<YOUR-FILE> in the Phase-3
   format (summary table with per-verdict counts, per-commit verdict rows with evidence,
   clusters, couplings, coverage honesty section). This file is the deliverable.

Casing/renames: Ghostty->Rustty, ghostty->rustty per token (R2); published C symbols are
rustty_*; the parity map is ${REPO}/.port/tools/parity_names.env if you need it.

Do NOT modify any code. Assessment only.
`

const SCHEMA = {
  type: 'object',
  required: ['area', 'file', 'commits_total', 'ported', 'partial', 'absent', 'na', 'clusters', 'couplings', 'risks'],
  properties: {
    area: { type: 'string' },
    file: { type: 'string', description: 'repo-relative path of the written assessment' },
    commits_total: { type: 'integer' },
    ported: { type: 'integer' },
    partial: { type: 'integer' },
    absent: { type: 'integer' },
    na: { type: 'integer' },
    clusters: {
      type: 'array',
      items: {
        type: 'object',
        required: ['name', 'verdict_mix', 'approx_lines', 'commits'],
        properties: {
          name: { type: 'string' },
          verdict_mix: { type: 'string' },
          approx_lines: { type: 'integer' },
          commits: { type: 'array', items: { type: 'string' } },
          notes: { type: 'string' },
        },
      },
    },
    couplings: { type: 'array', items: { type: 'string' } },
    risks: { type: 'array', items: { type: 'string' } },
  },
}

const AREAS = [
  {
    key: 'A-terminal-core',
    file: 'A-terminal-core.md',
    paths: 'src/terminal/PageList.zig src/terminal/Terminal.zig src/terminal/Screen.zig src/terminal/page.zig src/terminal/style.zig src/terminal/Selection.zig src/terminal/Tabstops.zig src/terminal/ScreenSet.zig src/terminal/StringMap.zig src/terminal/ref_counted_set.zig src/terminal/bitmap_allocator.zig src/terminal/size.zig src/terminal/mem.zig src/terminal/fastmem.zig src/terminal/hash_map.zig src/fastprint.zig src/terminal/fraction.zig src/terminal/stream.zig src/terminal/stream_continuation.zig src/terminal/stream_terminal.zig src/terminal/main.zig src/terminal/render.zig src/terminal/Parser.zig src/terminal/parse_table.zig src/terminal/sanitize.zig src/terminal/point.zig src/terminal/cursor.zig',
    extra: `Your area is terminal core. Known majors in range: PageList.zig +~2,100 lines churn;
stream.zig ~1,036; NEW files fastprint.zig, src/terminal/fraction.zig (custom fraction parsing,
946422dbc), stream_continuation.zig; formatter pending-wrap (997a2aff2) belongs to area D but its
Terminal/Screen touchpoints are yours — note the coupling. 88ed6bebf (much faster wide-character
reflow on resize) and 3d9b2b483 (grapheme-heavy IO) are perf-critical PageList/page work — check
whether rustty's ported PageList already has these shapes. e523cf810 (cursor home after formatting
tabstops). G1b says module 'stream' dropped 8 tests — find which upstream tests those are.
Rustty side: crates/rustty-vt/src/.`,
  },
  {
    key: 'B-snapshot-io',
    file: 'B-snapshot-io.md',
    paths: 'src/terminal/snapshot src/crc32c.zig src/lib/TinyIo.zig src/terminal/compress src/benchmark src/synthetic',
    extra: `Your area is the NEW snapshot serialization subsystem (~16k lines incl. .ksy Kaitai spec
and hex golden testdata under src/terminal/snapshot/testdata/), src/crc32c.zig (slicing-by-16
software CRC32C, 36d8e3f77 + eb09bf829 interleaved streams), src/lib/TinyIo.zig, any compress/
churn, and the benchmark + synthetic additions (TerminalSnapshot/TerminalFormatter/TerminalResize
benchmarks, synthetic Styled). This is almost certainly wholly ABSENT from rustty — your job is to
map its internal structure (module DAG, public API, the golden testdata format, the AGENTS.md
policy file), its dependencies on terminal core, and the perf-tuned commits that REWROTE parts of
it inside the range (single-pass style codec 1359973ea, skip remap tables 47a518262, wide pairs
per-row 7c1014ef6, vectorized decode 2aaad3ca9 / encode 973f619a2, grapheme suffix batch 593762cfa,
borrow buffered payloads c1a61fddd, checkpoint/continuation retain options 9be6c2ea2) so the port
targets the END STATE. Note every hex golden must be carried bit-for-bit (R1 comptime artifacts).
Check whether snapshot depends on upstream types rustty already has and where they diverge.`,
  },
  {
    key: 'C-capi-lib-wasm',
    file: 'C-capi-lib-wasm.md',
    paths: 'src/lib src/lib_vt.zig include src/terminal/c example test/wasm-alloc.mjs src/datastruct/wasm_page_pool.zig src/datastruct/limited_allocator.zig src/global.zig src/main_ghostty.zig',
    extra: `Your area is the libghostty C-API / lib/ / wasm tier: src/lib/* (NEW: TinyIo is area B's,
but c_abi.zig, packed.zig, compat/*, allocator/wasm.zig are yours; DELETED upstream:
lib/allocator/convenience.zig, lib/string.zig — rustty must delete its ports too or justify),
src/terminal/c/* (NEW: snapshot.zig 1,533 + io.zig; types.zig +1,252; types.schema.json +
types-schema-verify.py — an ABI manifest schema c75559589/9673a22b0), include/ghostty/vt/*
(NEW: snapshot.h, io.h; changed: others incl. packed cell layout 0e8b7bea6, sized init valid C++
d930c74c4, self-contained device/point headers 51992ab01, enum underlying type detection
e4ec4f0f9, all public structs in type JSON 309440e07), lib_vt.zig (writer adapter buffering
846d24e12, streaming formatter C API 924c8a90d, -Dvt-features 1fdbb8c91, wasm logging d61920d80),
the wasm allocator infrastructure (custom memory pool 492c26067, reduced stack 5364e5158,
simplified alloc API a8e9b413f, wasm_page_pool.zig, limited_allocator.zig, test/wasm-alloc.mjs),
dirty-row iteration 0d37f2d34, render-state cursor read b4079f00c, semantic prompt C APIs
bdb566068, c80c37362 (remove internal surface prompt query), and the new example/c-vt-snapshot.
Rustty side: crates/rustty-vt-c, crates/rustty-lib*, include/rustty*, the A5 header gate
.port/tools/gates/check_rustty_vt_headers.sh (its export-parity arm will flip RED for every new
export — enumerate the delta explicitly: oracle exports at the new pin vs rustty's). G1b says
apprt_api_surface dropped 4 tests. Check the wasm gate ws14_webgl_browser_gate.sh coupling.`,
  },
  {
    key: 'D-terminal-subsystems',
    file: 'D-terminal-subsystems.md',
    paths: 'src/terminal/kitty src/terminal/apc src/terminal/osc src/terminal/formatter.zig src/terminal/modes.zig src/terminal/color.zig src/terminal/device_status.zig src/terminal/dcs.zig src/terminal/csi.zig src/terminal/charsets.zig src/terminal/hyperlink.zig src/terminal/search src/terminal/tmux.zig src/terminal/x11_color.zig src/input',
    extra: `Your area: terminal subsystems + input. Majors: kitty/graphics_storage.zig ~1,193 churn
(G1b: kitty dropped 11 tests — find them); formatter.zig ~1,187 (pending wrap preservation
997a2aff2; G1b: formatter dropped 2); apc glyf decode allocation limits 433b16bcb (G1b: apc
dropped 4); modes (G1b: modes dropped 1); input: don't emit fallback text on key release
bd647035e, f4309055f macos 0x7F key event — input_key_encode dropped 3 tests. Also osc parsers
churn, color scheme work (37174c73b light/dark expansion helper). Rustty side:
crates/rustty-vt/src/{kitty,apc,osc,formatter*,modes,...}, crates/rustty-input.`,
  },
  {
    key: 'E-os-termio-cli-app',
    file: 'E-os-termio-cli-app.md',
    paths: 'src/os src/termio src/cli src/Command.zig src/Surface.zig src/App.zig src/crash src/terminfo src/shell-integration src/renderer',
    extra: `Your area: os / termio / cli / app-level / terminfo / renderer-zig-side. Majors: NEW
src/os/edit.zig, src/os/stderr.zig; termio UTF-8 prefix rescan avoidance ae6d97ea7 + UTF-8
preserving notification truncation 6cadad06f + apprt-owned truncation 53fea5d91/53c6fdbe7
(coupling with F); cli: NEW new_tab.zig, SSH terminfo versioning 69b9abf09 + always-install
fff9f62f3 (G1b: cli_ssh_cache_diskcache dropped 1); renderer: cell row storage simplification
cde7f9343 (G1b: renderer_cell dropped 1); 3650bef4e GTK shell-integration inheritance fix
(coupling with F). Rustty side: crates/rustty-{os,termio,cli,core,crash,renderer}. Check the
deleted-os-modules gate .port/tools/gates/check_deleted_os_modules.sh for os deletions.`,
  },
  {
    key: 'F-ui-tiers',
    file: 'F-ui-tiers.md',
    paths: 'macos src/apprt src/font src/inspector pkg/macos pkg/gtk4-layer-shell pkg/android-ndk pkg/wuffs src/build dist flatpak snap',
    extra: `Your area: UI tiers (macOS Swift app, apprt incl. gtk ipc new_tab.zig, font, inspector),
pkg/ churn, build system, packaging. macOS wave is large (33 files: command palette sort keys
5c952ac97 + tests, UntrustedURL{,Alert}.swift NEW + tests, CommandLineOpenFileFilter NEW + tests,
insertText commits as key events ecbeb60ca, notification fixes, Sendable/actor warning fixes,
DELETED helpers AnySortKey/CrossKit/HostingWindow/iOS files — rustty's macos/ Swift tree must
track these). font: shaper harfbuzz (G1b: dropped 1). apprt: desktop notification truncation
ownership 53c6fdbe7, gtk ipc new_tab (paired with cli new_tab — coupling with E). build:
-Dvt-features plumbing (coupling with C), libsystem override, 7c4c7adad pkg/wuffs C-only mirror.
Rustty side: macos/ (Swift), crates/rustty-{apprt,font,inspector}, crates-sys/, CMakeLists/dist.
Check PORT STATUS of apprt/font/inspector crates first (G3b skips crates whose PORT STATUS is
not done — those get assessed for RANGE work anyway, the placeholder-leaf doctrine governs).`,
  },
  {
    key: 'G-config-i18n-datastruct',
    file: 'G-config-i18n-datastruct.md',
    paths: 'src/config src/datastruct po .github typos.toml PACKAGING.md src/global.zig src/simd src/unicode src/benchmark/main.zig',
    extra: `Your area: config, datastruct, i18n, repo meta. config: hex escape byte preservation
29b82dd80 (zig-byteslice rule!), theme tilde/HOME expansion f748b17e2 + 37174c73b + shell-expand
move c12173429, autogenerated template flush 98b828a02. datastruct: DELETED upstream
array_list_collection.zig, lru.zig, segmented_pool.zig; NEW limited_allocator.zig +
wasm_page_pool.zig are area C's — verify rustty deletes its ports of the deleted files or
justify (check callers!). po/: ~35 files of translations — the real content is translations
(da.po is NEW — Danish); verify rustty's po/ mirrors (mostly mechanical; measure, do not read
every line — but state the filter). .github issue template deletions: mirror if rustty carries
them. simd/unicode churn if any. Rustty side: crates/rustty-{config,datastruct,simd,unicode}, po/.`,
  },
]

phase('Assess')
const results = await parallel(AREAS.map(a => () =>
  agent(
    COMMON +
    `\nYOUR AREA: ${a.key}. Your assessment file: .port/phase-4/assessments/${a.file}\n` +
    `Your file-set (paths relative to the oracle root; own every range commit touching them):\n${a.paths}\n\n` +
    a.extra +
    `\nReturn the structured summary; the .md file is the real deliverable.`,
    { label: `assess:${a.key}`, phase: 'Assess', schema: SCHEMA },
  )
))

phase('Reconcile')
const summary = JSON.stringify(
  results.filter(Boolean).map(r => ({ area: r.area, file: r.file, absent: r.absent, partial: r.partial })),
)
const critic = await agent(
  `You are the coverage-reconciliation critic for the rustty Phase-4 resync (range ${RANGE}).
Repo ${REPO}, oracle ${REF} (read-only, at the new pin).
The seven area assessments just written are in ${REPO}/.port/phase-4/assessments/ : ` +
  AREAS.map(a => a.file).join(', ') +
  `. Area summaries: ${summary}

1. Read ${REPO}/.port/phase-4/UPSTREAM_DIFF_STAT.txt (382 lines, one per changed file).
2. Map EVERY file to the area that assessed it (read the assessments' file lists / commit
   tables). Produce the orphan list: files no assessment covers, and files claimed by two
   areas with conflicting verdicts.
3. Spot-check 10 randomized PORTED verdicts across the assessments (R11: default-refute) —
   read the upstream hunk and the cited rustty file:line yourself; report any that do not
   survive.
4. Write ${REPO}/.port/phase-4/assessments/RECONCILIATION.md with: the orphan list (must be
   empty or each orphan assigned + assessed inline by you), double-claims, spot-check
   results, and a global verdict table (total commits, ported/partial/absent/na by area).
Do not modify code or other assessments.`,
  { label: 'reconcile', phase: 'Reconcile', schema: {
    type: 'object',
    required: ['orphans', 'double_claims', 'spot_check_failures', 'file'],
    properties: {
      orphans: { type: 'array', items: { type: 'string' } },
      double_claims: { type: 'array', items: { type: 'string' } },
      spot_check_failures: { type: 'array', items: { type: 'string' } },
      file: { type: 'string' },
    },
  } },
)

return { areas: results.filter(Boolean), reconciliation: critic }
