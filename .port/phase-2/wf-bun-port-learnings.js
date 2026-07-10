export const meta = {
  name: 'bun-port-learnings',
  description: 'Deep nested review of the Bun Zig→Rust rewrite article, PORTING.md history, PRs, regression issues, and community critique — mapped onto the rustty port rules',
  whenToUse: 'One-shot research sweep feeding the rustty realignment plan',
  phases: [
    { title: 'Fetch', detail: 'parallel deep-reads of each source cluster' },
    { title: 'Synthesize', detail: 'map learnings onto rustty PORT_RULES + gaps' },
  ],
}

const FINDINGS = {
  type: 'object',
  required: ['source', 'summary', 'learnings'],
  properties: {
    source: { type: 'string' },
    summary: { type: 'string', description: '2-4 sentence summary of what the source is' },
    learnings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['title', 'detail', 'rustty_applicability'],
        properties: {
          title: { type: 'string' },
          detail: { type: 'string', description: 'the concrete rule/technique/failure, with enough detail to act on (quote exact rule text where possible)' },
          rustty_applicability: { type: 'string', description: 'ADOPT / ADAPT / ALREADY-COVERED / REJECT + one sentence why, relative to a full-fidelity Zig→Rust port with PORT_RULES R1-R6' },
          evidence_url: { type: 'string' },
        },
      },
    },
  },
}

const COMMON = `
You are researching the Bun Zig→Rust rewrite to extract porting methodology for "rustty", a full-fidelity Zig→Rust port of the Ghostty terminal emulator. rustty's binding rules (PORT_RULES R1-R6, already locked): R1 100% fidelity (every line/comment/test/assertion ported, no loss); R2 rustty identity w/ ghostty parity build profile; R3 self-contained, same C libs, reference used only as external oracle; R4 acyclic layered crates gated by cycle_check; R5 gates never weakened; R6 Miri primary memory-safety gate for pure-Rust crates, ASan for FFI crates, LSan on Linux CI.

Tools: for GitHub content ALWAYS prefer the gh CLI via Bash (gh api, gh pr view, gh issue view --comments, gh api repos/oven-sh/bun/contents/<path>?ref=<sha>, gh api "repos/oven-sh/bun/commits?path=<path>"). For non-GitHub web pages use WebFetch (if it errors as deferred, load it first via ToolSearch "select:WebFetch"). Be persistent: if a file is not at an expected path, search the repo tree and its git history — the user explicitly warned PORTING.md may only exist at OLD revisions.

Return via StructuredOutput. Extract EVERY actionable learning, not just highlights: exact rules, exact failure modes, exact review-workflow shapes. rustty_applicability must be a real judgment against R1-R6, not boilerplate.`

phase('Fetch')
const sources = [
  {
    key: 'porting-md',
    prompt: `${COMMON}
TASK: Deep review of Bun's PORTING.md (the Zig→Rust pattern-mapping guide, ~3h of authored rules).
1. Locate it: try gh api repos/oven-sh/bun/contents/PORTING.md, then search the tree (gh api repos/oven-sh/bun/git/trees/HEAD?recursive=1 piped through grep -i porting), then history: gh api "repos/oven-sh/bun/commits?path=PORTING.md&per_page=100" (also try src/PORTING.md, docs/ variants). If it only exists at old revisions, fetch it at the LAST revision before deletion AND the FIRST revision, and note how it evolved.
2. Read the ENTIRE document (it may be long — fetch raw via contents API with ?ref=). Extract every Zig→Rust mapping rule verbatim-ish: allocator patterns, defer→Drop, error unions→Result, comptime→generics/macros, slices/pointers, optionals, tagged unions, bitfields/packed structs, assert semantics, integer overflow semantics, ReleaseFast vs release bounds checks, format strings, threading, etc.
3. Also look for LIFETIMES.tsv or the lifetime-analysis artifact in the repo/history (search tree + commits for LIFETIMES). Describe its schema and the 2-reviewer-consensus process if findable.
Report every rule as a separate learning with rustty_applicability.`,
  },
  {
    key: 'port-pr-and-crates',
    prompt: `${COMMON}
TASK: Review PR https://github.com/oven-sh/bun/pull/30412 (the Rust port PR) and PR https://github.com/oven-sh/bun/pull/30224 (pre-rewrite crate dependency refactoring).
Use gh pr view 30412 --repo oven-sh/bun --json title,body,files,commits,comments and gh pr diff where feasible (the diff is huge — get file list + body, don't pull 1M lines). Extract: how they structured crates, how they broke cyclic dependencies (compare to rustty's cycle_check + R4 break-list approach), the described merge/validation gates, branch strategy, how they kept the identical TypeScript test suite as the oracle, worktree parallelization mechanics (16 loops x 4 worktrees), commit discipline (no stash/reset), and anything in PR comments about post-merge fallout. Every distinct technique = one learning with rustty_applicability.`,
  },
  {
    key: 'regressions',
    prompt: `${COMMON}
TASK: Build a porting-mistake taxonomy from Bun's post-rewrite regressions. Deep-read with gh issue view N --repo oven-sh/bun --comments for issues 30678 (React Fast Refresh HMR), 31188 (UTF-16 BOM odd-length slice-cast panic), 31503 (BSS overflow block bounds panic), 30693 (color marker in OSC8 hyperlink / comptime format strings), and gh pr view 30875 (LeakSanitizer integration). For each: root cause in Zig-vs-Rust semantic terms (debug_assert side effects, slice cast truncate-vs-panic, ReleaseFast no-bounds-checks vs Rust release bounds checks, comptime format strings vs runtime), how it escaped their gates, and the fix. Then generalize: what CLASS of Zig→Rust semantic mismatch does each represent, and what automated check would have caught it (grep/lint/test pattern). rustty ports Zig code with the same semantics — rate each class's likelihood in a terminal emulator codebase (parser tables, cell buffers, SGR/OSC handling) and propose a concrete detector for rustty. Each class = one learning.`,
  },
  {
    key: 'hn-critique',
    prompt: `${COMMON}
TASK: Read the Hacker News thread https://news.ycombinator.com/item?id=48016880 (PORTING.md discussion) — use WebFetch on the thread (and page 2 if ?p=2 exists). Extract: (a) substantive technical critiques of Bun's porting approach (what experts said is wrong/risky/missing — especially about lifetimes, unsafe density, mechanical vs idiomatic porting, test-suite blind spots); (b) any additional detail Jarred or Bun team members added in comments beyond the article; (c) counter-arguments worth stealing as risk-mitigations for rustty. Ignore flamewar noise. Each substantive point = one learning with rustty_applicability.`,
  },
  {
    key: 'unsafe-and-style',
    prompt: `${COMMON}
TASK: Two related sources. (1) https://bun.com/bun-unsafe-audit — Bun's unsafe-code audit resource: what they count, how they categorize unsafe (single-line C-pointer vs structural), targets for reduction. Also inspect the smart-pointer heritage: gh api to fetch src/ptr/shared.zig at ref 3a79bd746b11601c9db970b608c73f0b9f96ac81 (repo oven-sh/bun) and see what pattern it embodied and what replaced it in Rust (search the current repo for the Rust equivalent, e.g. shared ptr / RefCounted in their rust crates). (2) https://tigerstyle.dev/ (TigerBeetle style) — which of its rules Bun credited, and which transfer to Rust ports. For rustty: propose a concrete unsafe-accounting scheme (metrics, categories, audit cadence) compatible with R6 (Miri/ASan gates) and rustty's known SB/TB hazards around raw-pointer aliasing in Page/Row/Cell code. Each rule/scheme = one learning.`,
  },
]

const fetched = await parallel(sources.map(s => () =>
  agent(s.prompt, { label: `fetch:${s.key}`, phase: 'Fetch', schema: FINDINGS })
))

const good = fetched.filter(Boolean)
log(`Fetched ${good.length}/${sources.length} source clusters, ${good.reduce((n, r) => n + r.learnings.length, 0)} raw learnings`)

phase('Synthesize')
const synthesis = await agent(`You are synthesizing research on the Bun Zig→Rust rewrite into an actionable learnings document for rustty (full-fidelity Zig→Rust port of Ghostty; rules R1-R6 as described below).

rustty PORT_RULES summary: R1 100% fidelity (every line/comment/test/assertion, no loss; completeness critic vs Phase-0 inventory); R2 rustty identity + identity-ghostty parity profile; R3 self-contained, same C libs, ghostty used only as external oracle; R4 acyclic layered crates (39 crates), cycle_check gate; R5 never weaken gates; R6 Miri primary for pure-Rust, ASan for FFI, LSan deferred to Linux CI. Known rustty-specific hazards from prior work: Stacked-Borrows UB from &mut args re-read across assert_integrity (fixed via raw-ptr args), long-lived raw aliases + interleaved &mut (fixed via raw-self helpers, proven on isolated Miri harnesses).

Here are the raw findings from ${good.length} research agents (JSON):
${JSON.stringify(good, null, 1)}

Write /Users/gael/projects/rustty/.port/phase-2/BUN_PORT_LEARNINGS.md with:
1. **TL;DR** — the 10 highest-leverage adoptions for rustty, ranked.
2. **Source map** — one paragraph per source cluster.
3. **Learnings catalog** — every learning, grouped by theme (porting rules / semantic-mismatch taxonomy / review & validation workflow / memory & unsafe strategy / crate structure & scale mechanics / risks & critiques), each tagged ADOPT / ADAPT / ALREADY-COVERED / REJECT with a one-line justification against R1-R6. Deduplicate across sources; keep evidence URLs.
4. **Semantic-mismatch detector spec** — the concrete automated checks rustty should add (each: what it greps/lints/tests, which Bun regression class it kills, where it plugs into ci.sh).
5. **Proposed PORT_RULES amendments** — draft R7+ text for anything worth making binding (e.g. debug_assert side-effect ban, bounds-check/overflow-semantics policy, unsafe accounting, adversarial-review split-context protocol) — clearly marked PROPOSED, not yet agreed.
Be precise and quote exact rule text where the research provides it. Then return a <=400-word executive summary of the document.`,
  { label: 'synthesize', phase: 'Synthesize' })

return { summary: synthesis, learnings_doc: '.port/phase-2/BUN_PORT_LEARNINGS.md', clusters: good.map(g => ({ source: g.source, n: g.learnings.length })) }
