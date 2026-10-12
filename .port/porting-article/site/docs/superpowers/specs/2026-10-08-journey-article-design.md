# "Porting Ghostty to Rust" — journey article design

Date: 2026-10-08 · Status: approved in brainstorming, pending spec review

## Goal

A single long-form, technical article on this site, linked from the home page, describing how
Ghostty was ported from Zig to Rust (Rustty): the engineering, the method and how it evolved, the
verification, the bugs found, performance, and cost. Audience: engineers. Technical content only.

The visualizations are the centrepiece: dense, multi-view, interactive instruments built from real
process data (git history, test/benchmark records, defect ledgers, agent usage stats), each with
computed insights the prose alone would not give.

## Non-goals

- Not a comparison with Bun's port. Bun's "Rewriting Bun in Rust" post is credited once, briefly,
  where it triggered process changes (§6). No Bun reference bars, bands or "vs Bun" framing in any
  chart or caption.
- No marketing copy, no features beyond what Rustty has, no claims beyond what the data supports.

## Source material (rustty repo, `~/projects/rustty`)

- `.port/journey-article/research/BRIEF.md` — fact-checked numbers (§2; only post-check values,
  respect its "do not quote" list), author answers, contradictions resolved.
- `.port/journey-article/research/*.md`, `r2-*.md`, `data/` — dossiers and chart-ready data.
- `.port/journey-article/research/r2-glossary.md` — plain-language mapping for every internal id,
  plus quotable rule texts (author-approved, including the three from agent memory).
- `.port/journey-article/mockups/NN-*.html|md` — the 11 approved rich mockups (vanilla HTML/SVG/JS
  fragments) and their specs (views, data series + provenance, insights, gaps).
  `mockups/gallery.html` shows all of them together. Build scripts: `.port/journey-article/mockup-build/`.

## Author decisions (binding)

- Title: **Porting Ghostty to Rust**. Route: `/journey`.
- One long page with a side table of contents.
- Defects may be published fully (classes, counts, mechanisms, trigger details). The repo will be
  public at publication: commit hashes link to it.
- Cost framing: paid by subscription (Claude Max 20x for the whole port; ChatGPT Pro for one month,
  Plus otherwise). The lost-transcript window (Mac Claude before 2026-09-08) is extrapolated at a
  maxed-out subscription × 50% Rustty share. API-equivalent list price is shown as a reference value,
  never as money spent.
- No internal ID jargon in published text (rule/gate/ledger/phase/wave/chart ids). Describe what
  the thing is; quote rule texts verbatim where useful.
- Never describe Codex as "less capable" (that was a prompting tactic, not an assessment).
- No snark about Ghostty; credit Ghostty and its authors clearly.
- Hardware: MacBook Pro, Apple M4 Pro, 14 cores, 48 GB.
- Data freshness: re-run benchmarks and finish the sanitizer lanes at the current reference commit
  before publication (see "Data refresh").

## Article structure

★ = flagship widget (full interactivity: replay/scrub/brush/click-detail). Other widgets are
hover/click-rich static charts. Mockup numbers refer to `.port/journey-article/mockups/`.

| # | Section | Visual |
|---|---|---|
| 1 | Opening stats card — duration, commits, Zig→Rust lines, tests vs Zig tests, benchmark result, defects recorded, verification lanes | stat tiles |
| 2 | Why, and what "port" means — "seeing what was possible"; parity at a pinned Ghostty commit, comments carried, C ABI byte-compatible, Swift app reused; parity minus bugs | none |
| 3 | The reference as a contract — Zig tests as the spec; the contract grows with every resync; silent-skip census | ★ 03 Test contract |
| 4 | Cutting a 345-file cycle into crates — the 14 cuts, the crate DAG, layering decisions | ★ 04 Cutting the cycle |
| 5 | The first three weeks — engine in 4 days, first pixels day 19, runnable macOS app day 22 | ★ 02 Agents vs human |
| 6 | The method matures — lifetimes ledger, unsafe census, regression detectors, gates that self-test; one-sentence credit to Bun's post as the trigger on 07-10 | static: strategy timeline |
| 7 | Zig pointers in Rust's aliasing model — the technical core | ★ 07 Translation lens, ★ 01 Three reviewers, 09 Memory-safety bugs |
| 8 | Performance — 0/14 → 14/14 in a day, LTO, comptime flattening, later pins | ★ 05 Perf replay |
| 9 | Keeping up with upstream — 7 moves of the reference commit | 06 Moving target |
| 10 | Checks as the specification — check-list growth, comment fidelity, C-ABI parity, "a derived green can be bookkeeping" | static: check growth + fidelity/ABI pills |
| 11 | Never port a bug — the defect ledger | 08 Defect map |
| 12 | Second opinions — the Codex audit and adversarial review | static: audit funnel + Codex weekly meter |
| 13 | Verification matrix — Miri (Stacked/Tree Borrows), sanitizers, Valgrind, cross-host | 10 Verification matrix |
| 14 | How the orchestration changed — workflow scripts → worktree subagents; models, context size, concurrency | static: model timeline, context, concurrency |
| 15 | What it cost | ★ 11 Cost |
| 16 | Unsafe, measured — density over time and per crate (no external comparison) | static: unsafe density |
| 17 | Limitations and what's left — per-platform status table, open differences | table |
| 18 | Appendix: methodology and hardware — how every number was measured | none |

Target length 8–10k words. Every section ends on a measurable result.

## Site architecture

- **Route:** `src/app/journey/page.tsx` (metadata, layout, TOC) + `src/app/journey/content.mdx`
  (prose). Static (SSG), consistent with the site's "fully static" rule.
- **Home link:** a third neutral `ButtonLink` in `buttonsList` in `src/app/HomeContent.tsx`.
- **TOC:** reuse the site's Sidecar + JumplinkHeader; add a small heading extractor for this
  non-docs MDX page if the docs one cannot be reused.
- **Widgets:** one React client component per widget under `src/components/journey/<Widget>/`
  (`index.tsx`, `*.module.css`, pure helper modules for layout/scales). Each is a faithful rewrite
  of its mockup's views, encodings and interactions in React + inline SVG. Mockup layout math that
  was computed offline (e.g. graph coordinates) stays precomputed in data.
- **Shared primitives** (`src/components/journey/ui/`): `InstrumentCard` (approved Bun-style card:
  `#0b0c0c`, 1px `#262927`, radius 16, source pill, tracked label, mono headline, caption strip),
  `TerminalFrame` (used only by replay widgets whose producing command is real: 01, 05, and where
  the mockup used it), `Tooltip` (hover + tap-to-pin + keyboard focus), `ReplayControls` (play/
  replay/scrub/step, live timestamp), `useReducedMotion`, `useInView`, scale helpers (linear/log/
  time/band in plain TS; add `d3-scale`/`d3-shape` at pinned versions only if clearly needed).
- **Palette/typography:** site tokens — copper `#d97934` Rust, teal `#64d8c6` Zig/upstream, gold
  `#ffbc7a`, atom-one green/red pass/fail, plus blue/purple/yellow/orange for extra categories;
  JetBrains Mono tabular numerals. Wherever two hues are hard to tell apart for colour-blind readers
  (blue/purple), add shape, texture or direct labels.
- **Loading:** widgets are lazy-mounted when scrolled near (dynamic import + IntersectionObserver),
  with a skeleton card of the final size to avoid layout shift. Per-widget data budget ~150 KB;
  page data stays well under Next's `largePageDataBytes`.
- **Width:** cards break out of the 736 px prose column to ~1000 px on wide screens; at phone width
  each widget has an explicit stacked layout (no page-level horizontal scroll; very wide plots may
  scroll inside their own container).

## Data pipeline

- `~/projects/rustty/.port/journey-article/build-data.py` (stdlib Python) reads the research data
  and mockup data series, and writes trimmed JSON to `src/app/journey/data/<widget>.json` plus
  `src/app/journey/data/facts.json` (`{id: {value, display, provenance, source, asOf}}`).
- Prose cites numbers through `<Fact id="…"/>` (renders `display`; title attribute shows source).
  A check script (`npm run verify:journey`) fails if a referenced fact id is missing or if a widget
  JSON is missing a declared field.
- Commit hashes render as links to the public repo.
- Strip internal ids from any embedded commit subjects/ledger text in the pipeline (the mockups
  used a regex; the pipeline makes it deterministic and lists every rewritten string for review).

## Data refresh (before publication)

1. Benchmarks at the current reference commit `a60e9e2a5`, on `phase-8-upstream-resync` (the author
   pauses in-flight validation to free CPU/memory). Zig built with `-Doptimize=ReleaseFast`; same
   methodology as the recorded runs (ns/op slopes, same-session Zig). Fills the reserved slot in
   the perf widget.
2. Finish the sanitizer/Valgrind/Miri lanes at the current reference commit (currently pending on an
   unmerged branch). The verification matrix shows "pending" until then — never provisional reds
   or greens.
3. Re-run `build-data.py`; measured sections carry an "as of <commit>" label.

Writing proceeds in parallel against current numbers.

## Quality bar

- `npm run lint` (biome; doc comments on all functions and top-level constants), `npm run build`,
  `npm run verify:animation`, `npm run verify:journey` all pass with no warnings.
- Playwright screenshots of every widget at 1100 px and 390 px; no console errors; no page-level
  horizontal scroll.
- Accessibility: keyboard reachable controls and tooltips, tap-to-pin on touch, reduced motion
  honoured (replays jump to final state), colour never the only encoding, text alternatives
  (caption + summary) for every widget. Print styles checked.
- Hand review before publication: Zig↔Rust line mappings (07), residual internal ids in commit
  subjects, rule quotes, the Miri outputs regenerated on 2026-10-08 (01; caption says so), the
  estimated classifications (defect discovery method, ReleaseFast consequence, era classifier).

## Known data caveats to carry into captions

- Claude totals before 2026-09-08 are modeled (lost transcripts); shown hatched with ranges.
- Test-contract squares turn copper when a module's Rust test count reaches the Zig count, not on a
  per-test pass history (none exists).
- The verification matrix is reconstructed from records (no gate-run log).
- The port memory-safety timeline is a floor (hand-curated events).
- The crate-graph replay uses today's edges.
