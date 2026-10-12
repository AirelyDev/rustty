# "Porting Ghostty to Rust" Journey Article Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a static `/journey` page titled "Porting Ghostty to Rust", linked from the home page, with ~9k words of technical prose and 11 data-rich interactive widgets plus 7 static charts, every number traceable to a fact-checked source.

**Architecture:** A Next.js App Router route renders one MDX file. Numbers in prose come from a generated `facts.json` via a `<Fact>` server component; each widget is a server-rendered `JourneyWidget` shell (card chrome, caption, text summary) that lazy-mounts a client "body" component when scrolled near. Widget data is generated JSON produced by a stdlib-Python pipeline (`scripts/journey/`) from the research data in the rustty repo, and imported only by the widget body (so it ships in the lazily loaded chunk, not page data).

**Tech Stack:** Next.js 16 (App Router, static), React 19, @next/mdx, CSS Modules, inline SVG, TypeScript run directly by Node 25 for pure-helper tests (`node scripts/verify-journey-*.mjs`, existing repo convention), Python 3 stdlib + `unittest` for the data pipeline, Biome for lint. No chart library unless a task says otherwise.

**Spec:** `docs/superpowers/specs/2026-10-08-journey-article-design.md` (read it first; this plan argues from it).

**Source material (rustty repo):** `RESEARCH=/Users/gael/projects/rustty/.port/journey-article/research` (BRIEF.md, dossiers, `data/`), `MOCKUPS=/Users/gael/projects/rustty/.port/journey-article/mockups` (`NN-*.html` behavioural/visual reference + `NN-*.md` spec per widget, `gallery.html`), `MOCKBUILD=/Users/gael/projects/rustty/.port/journey-article/mockup-build` (scripts that produced mockup data).

## Global Constraints

- Title "Porting Ghostty to Rust"; route `/journey`; one long page with the site Sidecar TOC.
- Fully static site: no server functions, no runtime fetches; `npm run build` must succeed.
- `npm run lint` (Biome) clean including warnings; **every function and top-level constant has a doc comment** (repo AGENTS.md rule).
- No internal ID jargon in any visible text: no `R7`, `R13`, `D1`, `G3b`, `G6`, `UD123`, `N11`, `U-…`, `A4`, `P8-W9`, `T1b`, phase/wave labels, mockup/chart ids. Use plain descriptions (`$RESEARCH/r2-glossary.md`).
- Bun's port is credited once, concisely, in §6 only. No Bun reference values, bands, bars, or "vs Bun" wording in any widget, caption or other section.
- Never describe Codex as "less capable". No snark about Ghostty; credit Ghostty and its authors.
- Cost framing: subscription paid (Claude Max 20x whole port; ChatGPT Pro one month, Plus otherwise); lost window (Mac Claude before 2026-09-08) modeled at maxed-out subscription × 50% Rustty share; API-equivalent list price is labelled a reference value, never money spent.
- Use only post-fact-check values from `$RESEARCH/BRIEF.md` §2 and the `## Fact-check` sections of dossiers; never quote anything on BRIEF's "do not quote" list.
- Defects may be published fully; commit hashes link to `https://github.com/AirelyDev/rustty/commit/<hash>` (public at publication).
- Palette: copper `#d97934` Rust, teal `#64d8c6` Zig/upstream, gold `#ffbc7a`, green `#98c379` pass, red `#e06c75` fail, blue `#61aeee`, purple `#c678dd`, yellow `#e6c07b`, orange `#d19a66`; grays `#0f1111 #171a19 #262927 #4a4f4b #858a84 #9aa09a #c1c8c0 #d9ded6 #f4f1ed`; card `#0b0c0c`. Colour is never the only encoding.
- Instrument card: background `#0b0c0c`, 1px border `#262927`, radius 16px; header = outlined source pill (copper, mono 10px) + uppercase tracked label (`#858a84`, 11px, letter-spacing 2px); optional mono headline (30px/700/`#f4f1ed`); footer caption under a 1px `#262927` rule (12px sans `#858a84`) stating what is plotted and its source. Terminal-window frame only for widgets 01 and 05.
- Hardware wording: "MacBook Pro, Apple M4 Pro, 14 cores, 48 GB".
- Measured sections carry an "as of <commit>" label sourced from `facts.json`.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_0137GzaML9GbVQDyvvXuuVN1
  ```

## Review Focus

1. **Phone width (390 px):** no page-level horizontal scroll; every widget switches to its stacked layout; very wide plots scroll inside their own container only. → pinned by the per-widget visual check step and Task 20's sweep.
2. **Reduced motion:** with `prefers-reduced-motion: reduce`, replays start at their final state and never auto-animate. → `replayStart()` unit test in Task 4; each replay widget uses it.
3. **Touch / keyboard:** information shown on hover is reachable by tap (tap-to-pin) and by keyboard focus. → `Tooltip` behaviour test in Task 4 (pin/unpin state machine) + Task 20 manual check.
4. **No-JS / slow JS:** the built HTML contains every widget's title, caption and text summary (SSR shell), so the page is readable before or without hydration. → `verify-journey-build.mjs` in Task 2 asserts it against `.next` output.
5. **Deep links:** `/journey#<heading-id>` lands on the right heading without layout shift from lazy widgets (skeleton height equals rendered height). → `minHeight` per widget declared in the registry and asserted in Task 2's registry test; Task 20 checks anchors.

---

## File Structure

```
scripts/journey/
  build_data.py           # CLI: --research <dir> --mockbuild <dir> --out src/app/journey/data
  facts.csv               # hand-curated facts (id,display,value,provenance,source,as_of)
  jargon.py               # strip_internal_ids(text) -> (clean, changed: bool)
  widgets/                # one module per widget: build(research, mockbuild) -> dict
    __init__.py
    w01_three_reviewers.py … w11_cost.py, static_charts.py
  test_build_data.py      # unittest for pipeline + invariants
scripts/verify-journey-registry.mjs   # node test: registry/facts/data consistency (pre-build)
scripts/verify-journey-build.mjs      # node test: built HTML contains SSR shells + anchors
scripts/verify-journey-helpers.mjs    # node test: pure TS helpers (scales, format, replay, tooltip)
src/app/journey/
  page.tsx                # route: metadata, layout, Sidecar, MDX content
  content.mdx             # the article
  JourneyPage.module.css
  data/facts.json, data/<widget>.json   # generated, committed
src/components/journey/
  ui/  InstrumentCard.tsx, TerminalFrame.tsx, Tooltip.tsx, ReplayControls.tsx,
       JourneyWidget.tsx, LazyMount.tsx, Fact.tsx, CommitLink.tsx, *.module.css
  lib/ scales.ts, format.ts, replay.ts, tooltip-state.ts, registry.ts, palette.ts
  three-reviewers/ agents-vs-human/ test-contract/ cutting-the-cycle/ perf-replay/
  moving-target/ translation-lens/ defect-map/ memory-safety/ verification-matrix/ cost/
  static-charts/          # stats card, strategy timeline, checks growth, audit funnel,
                          # orchestration, unsafe density, platform table
src/lib/docs/page.ts      # modify: export extractPageHeaders
src/app/HomeContent.tsx   # modify: third ButtonLink
package.json              # modify: add verify:journey script
```

Widget folder convention (`src/components/journey/<slug>/`): `index.ts` (re-exports `Body`), `Body.tsx` (`"use client"`, default export, imports its JSON), `*.module.css`, `model.ts` (pure data → view-model functions, unit-tested), sub-view components as needed (`<View>.tsx`, each < ~300 lines).

---

### Task 0: Preserve the research in the rustty repo (needs author OK before running)

The research, mockups and mockup build scripts are untracked in `~/projects/rustty` (branch `phase-8-upstream-resync` has in-flight work, so do not commit there).

**Files:** `/Users/gael/projects/rustty/.port/journey-article/**` (commit only), `.port/phase-2/wf-journey-article-research*.js`

- [ ] **Step 1: Create a worktree branch off main and copy the material**

```bash
git -C /Users/gael/projects/rustty worktree add /Users/gael/projects/rustty-journey -b journey-article-research main
rsync -a /Users/gael/projects/rustty/.port/journey-article/ /Users/gael/projects/rustty-journey/.port/journey-article/
cp /Users/gael/projects/rustty/.port/phase-2/wf-journey-article-research.js /Users/gael/projects/rustty/.port/phase-2/wf-journey-article-research-2.js /Users/gael/projects/rustty-journey/.port/phase-2/
du -sh /Users/gael/projects/rustty-journey/.port/journey-article   # expect ~60 MB or less
```

- [ ] **Step 2: Commit**

```bash
git -C /Users/gael/projects/rustty-journey add .port/journey-article .port/phase-2/wf-journey-article-research.js .port/phase-2/wf-journey-article-research-2.js
git -C /Users/gael/projects/rustty-journey commit -m "records: journey-article research, mockups and mockup build scripts"
```

- [ ] **Step 3: Remove the worktree (keep the branch)**

```bash
git -C /Users/gael/projects/rustty worktree remove /Users/gael/projects/rustty-journey
```

---

### Task 1: Data pipeline skeleton, facts, and jargon stripping

**Files:**
- Create: `scripts/journey/build_data.py`, `scripts/journey/jargon.py`, `scripts/journey/facts.csv`, `scripts/journey/widgets/__init__.py`, `scripts/journey/test_build_data.py`
- Generated: `src/app/journey/data/facts.json`

**Interfaces:**
- Produces: `build_data.py --research DIR --mockbuild DIR --out DIR [--only SLUG]`; `widgets.REGISTRY: dict[str, Callable[[Path, Path], dict]]` (slug → builder); `jargon.strip_internal_ids(text: str) -> tuple[str, bool]`; `facts.json` = `{ "<id>": {"display": str, "value": number|str, "provenance": "MEASURED"|"DOCUMENTED"|"ESTIMATED", "source": str, "asOf": str} }`. Every widget JSON gets a top-level `"_meta": {"slug", "generatedFrom": [paths], "asOf"}`.

- [ ] **Step 1: Write the failing tests**

```python
# scripts/journey/test_build_data.py
"""Unit tests for the journey-article data pipeline."""
import csv, json, subprocess, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import jargon  # noqa: E402

RESEARCH = Path("/Users/gael/projects/rustty/.port/journey-article/research")
MOCKBUILD = Path("/Users/gael/projects/rustty/.port/journey-article/mockup-build")


class JargonTest(unittest.TestCase):
    """strip_internal_ids removes internal identifiers and reports changes."""

    def test_strips_known_id_families(self):
        for raw in ["fix UD123 crash", "gate G3b green", "rule R13 says", "N511 apc",
                    "P8-W9 T1b check", "D1 layering", "A4 sret gate", "G6c lane"]:
            clean, changed = jargon.strip_internal_ids(raw)
            self.assertTrue(changed, raw)
            for tok in ["UD123", "G3b", "R13", "N511", "P8-W9", "T1b", "D1", "A4", "G6c"]:
                self.assertNotIn(tok, clean)

    def test_leaves_plain_text_and_hashes_alone(self):
        for raw in ["vt: fix search aliasing (9e3fe1c25)", "M4 Pro", "UTF-8", "x86_64", "AArch64"]:
            self.assertEqual(jargon.strip_internal_ids(raw), (raw, False))


class FactsTest(unittest.TestCase):
    """facts.csv is well formed and the pipeline emits facts.json."""

    def test_facts_csv_rows_valid(self):
        with open(HERE / "facts.csv", newline="") as f:
            rows = list(csv.DictReader(f))
        self.assertGreaterEqual(len(rows), 30)
        ids = [r["id"] for r in rows]
        self.assertEqual(len(ids), len(set(ids)), "duplicate fact ids")
        for r in rows:
            self.assertRegex(r["id"], r"^[a-z0-9_]+$")
            self.assertIn(r["provenance"], {"MEASURED", "DOCUMENTED", "ESTIMATED"})
            self.assertTrue(r["display"] and r["source"] and r["as_of"], r["id"])
            self.assertFalse(jargon.strip_internal_ids(r["display"])[1], r["id"])

    def test_cli_writes_facts_json(self):
        with tempfile.TemporaryDirectory() as out:
            subprocess.run([sys.executable, str(HERE / "build_data.py"), "--research", str(RESEARCH),
                            "--mockbuild", str(MOCKBUILD), "--out", out, "--only", "facts"], check=True)
            facts = json.loads((Path(out) / "facts.json").read_text())
            self.assertEqual(facts["commits_head"]["display"], "2,968")
            self.assertEqual(facts["perf_suite_delta_july"]["display"], "−21.7%")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run to verify failure**

Run: `python3 -m unittest scripts/journey/test_build_data.py -v`
Expected: FAIL / ERROR — `ModuleNotFoundError: No module named 'jargon'`.

- [ ] **Step 3: Implement `jargon.py`**

```python
# scripts/journey/jargon.py
"""Strip internal Rustty port identifiers from text destined for the published article."""
import re

# ID_PATTERN matches the internal id families listed in research/r2-glossary.md.
ID_PATTERN = re.compile(
    r"\b(?:UD\d+|N\d{1,3}|U-\w+|R\d{1,2}[a-z]?|D\d[a-z]?|G\d+[a-z]?(?:-SELF)?|G-[A-Z]+|"
    r"A\d|P\d+-W\d+[a-z]?|W\d+\.T\d+[a-z]?|T\d+[a-z])\b"
)


def strip_internal_ids(text: str) -> tuple[str, bool]:
    """Return (text without internal ids, whether anything was removed)."""
    clean = ID_PATTERN.sub("", text)
    clean = re.sub(r"\(\s*\)|\[\s*\]", "", clean)
    clean = re.sub(r"\s{2,}", " ", clean).strip(" :;,-")
    return (clean, True) if clean != text else (text, False)
```

Note: `M4` (chip) is not matched because `M` is not an id family; `A4` is. If a legitimate token is hit (e.g. a benchmark name), add a test for it and narrow the pattern.

- [ ] **Step 4: Create `facts.csv` from BRIEF §2 (post-check values only)**

Header and the starter rows below are required; add more as prose tasks need them (each prose task appends its facts). Source column = dossier/file path; `as_of` = commit or date.

```csv
id,display,value,provenance,source,as_of
days_total,110 days,110,MEASURED,research/timeline.md,2026-10-08
commits_head,"2,968",2968,MEASURED,research/timeline.md,1d22ba80d
commits_nonmerge,"2,728",2728,MEASURED,research/timeline.md,1d22ba80d
merges,266,266,MEASURED,research/timeline.md,1d22ba80d
busiest_day,"249 commits on 2026-09-23",249,MEASURED,research/timeline.md,1d22ba80d
zig_code_lines,"302,669",302669,MEASURED,research/codebase.md,a60e9e2a5
rust_code_lines,"507,628",507628,MEASURED,research/codebase.md,1d22ba80d
rust_zig_ratio,1.68×,1.68,MEASURED,research/codebase.md,1d22ba80d
vt_lines_4days,"6,996 → 135,431",135431,MEASURED,research/timeline.md,2026-06-25
vt_tests_4days,"113 → 2,032",2032,MEASURED,research/timeline.md,2026-06-25
rust_tests,"7,270",7270,MEASURED,research/codebase.md,1d22ba80d
zig_tests_pin,"4,423",4423,MEASURED,research/codebase.md,a60e9e2a5
zig_tests_contract,"3,234",3234,DOCUMENTED,research/timeline.md,4749c4e93
crate_packages,61,61,MEASURED,research/codebase.md,1d22ba80d
crate_depth,18,18,MEASURED,research/codebase.md,1d22ba80d
scc_files,345,345,DOCUMENTED,research/codebase.md,4749c4e93
scc_cuts,14,14,DOCUMENTED,research/codebase.md,4749c4e93
first_pixels_day,day 19,19,MEASURED,research/timeline.md,2026-07-09
runnable_macos_day,day 22,22,MEASURED,research/timeline.md,2026-07-12
lifetimes_rows,314,314,MEASURED,research/r2-data-fixes.md,1d22ba80d
perf_wins_july,14/14,14,MEASURED,research/perf.md,4749c4e93
perf_suite_delta_july,−21.7%,-21.7,MEASURED,research/perf.md,4749c4e93
perf_tparser_speedup,2.85×,2.85,MEASURED,research/perf.md,4749c4e93
perf_clone_speedup,2.42×,2.42,MEASURED,research/perf.md,4749c4e93
lto_tparser,322 → 531 ms,531,MEASURED,research/perf.md,4749c4e93
pin_advances,7,7,MEASURED,research/timeline.md,a60e9e2a5
upstream_commits_absorbed,"1,157",1157,MEASURED,research/timeline.md,a60e9e2a5
ghostty_growth,+33.9%,33.9,MEASURED,research/timeline.md,a60e9e2a5
defects_total,205,205,MEASURED,research/bugs.md,1d22ba80d
defects_live,135,135,MEASURED,research/bugs.md,1d22ba80d
defects_ghostty,172,172,MEASURED,research/bugs.md,1d22ba80d
defects_deps,33,33,MEASURED,research/bugs.md,1d22ba80d
port_memsafety_defects,81,81,MEASURED,research/bugs.md,1d22ba80d
port_memsafety_miri,62,62,MEASURED,research/bugs.md,1d22ba80d
comments_carried,"34,345",34345,DOCUMENTED,research/codebase.md,1d22ba80d
unsafe_sites,"22,310",22310,MEASURED,research/codebase.md,1d22ba80d
cost_paid,~$420–$600,422,ESTIMATED,research/r2-cost.md,2026-10-08
cost_api_equiv,~$23.8k,23791,ESTIMATED,research/r2-cost.md,2026-10-08
cost_recovered,"$11,032",11032.43,MEASURED,research/r2-cost.md,2026-10-08
hardware,"MacBook Pro, Apple M4 Pro, 14 cores, 48 GB",M4 Pro,DOCUMENTED,research/BRIEF.md,2026-10-08
```

Verify each starter value against BRIEF §2 while typing it; if BRIEF shows a corrected (✎) value, use it.

- [ ] **Step 5: Implement `build_data.py` and `widgets/__init__.py`**

```python
# scripts/journey/build_data.py
"""Generate src/app/journey/data/*.json for the journey article from rustty research data."""
import argparse, csv, json, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from widgets import REGISTRY  # noqa: E402


def build_facts(_research: Path, _mockbuild: Path) -> dict:
    """Load facts.csv into the facts.json mapping keyed by fact id."""
    with open(HERE / "facts.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    def num(v: str):
        try:
            return float(v) if "." in v else int(v)
        except ValueError:
            return v

    return {r["id"]: {"display": r["display"], "value": num(r["value"]),
                      "provenance": r["provenance"], "source": r["source"], "asOf": r["as_of"]}
            for r in rows}


def main() -> None:
    """Parse CLI args and write every (or one) generated JSON file."""
    ap = argparse.ArgumentParser()
    ap.add_argument("--research", type=Path, required=True)
    ap.add_argument("--mockbuild", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--only")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    builders = {"facts": build_facts, **REGISTRY}
    for slug, fn in builders.items():
        if a.only and slug != a.only:
            continue
        data = fn(a.research, a.mockbuild)
        (a.out / f"{slug}.json").write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")))
        print(f"wrote {slug}.json ({(a.out / f'{slug}.json').stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
```

```python
# scripts/journey/widgets/__init__.py
"""Registry of widget data builders: slug -> build(research_dir, mockbuild_dir) -> dict."""
REGISTRY: dict = {}
```

- [ ] **Step 6: Run tests to verify they pass, then generate facts.json**

Run: `python3 -m unittest scripts/journey/test_build_data.py -v` → all PASS.
Run: `python3 scripts/journey/build_data.py --research /Users/gael/projects/rustty/.port/journey-article/research --mockbuild /Users/gael/projects/rustty/.port/journey-article/mockup-build --out src/app/journey/data --only facts` → `wrote facts.json`.

- [ ] **Step 7: Commit**

```bash
git add scripts/journey src/app/journey/data/facts.json
git commit -m "journey: data pipeline skeleton, curated facts and jargon stripping"
```

---

### Task 2: Widget registry and verification scripts

**Files:**
- Create: `src/components/journey/lib/registry.ts`, `scripts/verify-journey-registry.mjs`, `scripts/verify-journey-build.mjs`
- Modify: `package.json` (scripts)

**Interfaces:**
- Produces: `export type WidgetEntry = { slug: string; title: string; label: string; source: string; caption: string; summary: string; minHeight: { desktop: number; phone: number }; flagship: boolean; frame: "card" | "terminal"; dataFile: string | null; requiredKeys: string[] }` and `export const WIDGETS: readonly WidgetEntry[]` and `export function widget(slug: string): WidgetEntry` (throws on unknown slug). `npm run verify:journey` runs the registry check; `npm run verify:journey:build` runs the built-HTML check (after `npm run build`).

- [ ] **Step 1: Write the failing registry test**

```js
// scripts/verify-journey-registry.mjs
/** Verifies the journey widget registry, generated data and fact references are consistent. */
import { readFileSync, existsSync } from "node:fs";
import { WIDGETS } from "../src/components/journey/lib/registry.ts";

/** Collected failures; the script exits non-zero if any are recorded. */
const failures = [];
/** Records a failure message when condition is false. */
function check(condition, message) {
  if (!condition) failures.push(message);
}

/** ID_RE flags internal identifiers that must not appear in visible text. */
const ID_RE = /\b(UD\d+|N\d{1,3}|R\d{1,2}[a-z]?|G\d+[a-z]?|G-[A-Z]+|P\d+-W\d+|T\d+[a-z]|A\d)\b/;
const facts = JSON.parse(readFileSync("src/app/journey/data/facts.json", "utf8"));
const slugs = new Set();
for (const w of WIDGETS) {
  check(!slugs.has(w.slug), `duplicate slug ${w.slug}`);
  slugs.add(w.slug);
  for (const field of ["title", "label", "caption", "summary"]) {
    check(!ID_RE.test(w[field]), `${w.slug}.${field} contains an internal id`);
    check(!/\bBun\b/.test(w[field]), `${w.slug}.${field} mentions Bun`);
  }
  check(w.minHeight.desktop >= 200 && w.minHeight.phone >= 200, `${w.slug} minHeight too small`);
  if (w.dataFile) {
    const path = `src/app/journey/data/${w.dataFile}`;
    check(existsSync(path), `${w.slug}: missing ${path}`);
    if (existsSync(path)) {
      const data = JSON.parse(readFileSync(path, "utf8"));
      for (const k of [...w.requiredKeys, "_meta"]) check(k in data, `${w.slug}: data missing key ${k}`);
      check(readFileSync(path).length < 200 * 1024, `${w.slug}: data over 200 KB`);
    }
  }
}
const mdx = existsSync("src/app/journey/content.mdx") ? readFileSync("src/app/journey/content.mdx", "utf8") : "";
for (const [, id] of mdx.matchAll(/<Fact id="([a-z0-9_]+)"/g)) check(id in facts, `unknown fact id ${id}`);
for (const [, slug] of mdx.matchAll(/<JourneyWidget slug="([a-z0-9-]+)"/g)) check(slugs.has(slug), `unknown widget ${slug}`);
check(!ID_RE.test(mdx.replace(/<[^>]+>/g, "")), "content.mdx prose contains an internal id");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log(`journey registry OK (${WIDGETS.length} widgets, ${Object.keys(facts).length} facts)`);
```

- [ ] **Step 2: Run it to verify failure**

Run: `node scripts/verify-journey-registry.mjs`
Expected: FAIL — cannot find module `registry.ts`.

- [ ] **Step 3: Implement `registry.ts` with all 18 entries**

Write one entry per widget (11 interactive: `three-reviewers`, `agents-vs-human`, `test-contract`, `cutting-the-cycle`, `perf-replay`, `moving-target`, `translation-lens`, `defect-map`, `memory-safety`, `verification-matrix`, `cost`; 7 static: `stats-card`, `strategy-timeline`, `checks-growth`, `audit-funnel`, `orchestration`, `unsafe-density`, `platform-table`). Take `title`, `label`, `source` (pill text = real data source), `caption` and a 2–4 sentence plain-text `summary` of the key insight from each mockup's `.md` spec (`$MOCKUPS/NN-*.md`), rewritten without ids and without Bun. Set `dataFile` to `"<slug>.json"`, `requiredKeys` to the top-level keys the Body will read (fill in when the widget task defines them; start with `[]`), `frame: "terminal"` only for `three-reviewers` and `perf-replay`, `minHeight` from the mockup's rendered height at 1100 px and 390 px (measure in the gallery). Example entry:

```ts
// src/components/journey/lib/registry.ts
/** Static description of one journey widget: chrome text, sizing and data contract. */
export type WidgetEntry = {
  slug: string;
  title: string;
  label: string;
  source: string;
  caption: string;
  summary: string;
  minHeight: { desktop: number; phone: number };
  flagship: boolean;
  frame: "card" | "terminal";
  dataFile: string | null;
  requiredKeys: string[];
};

/** All widgets used by the journey article, in article order. */
export const WIDGETS: readonly WidgetEntry[] = [
  {
    slug: "test-contract",
    title: "The test contract",
    label: "ZIG TESTS AS THE SPEC",
    source: "git log · per-module #[test] counts",
    caption:
      "One square per Zig test, coloured by the day its module's Rust test count reached it. Counts from 110 daily snapshots of the Rustty repo against the Ghostty reference commit of the day.",
    summary:
      "86% of the first-day contract was covered within a week. Each resync grew the contract; in total the Zig suite grew from 3,189 to 4,423 test blocks.",
    minHeight: { desktop: 1400, phone: 2200 },
    flagship: true,
    frame: "card",
    dataFile: "test-contract.json",
    requiredKeys: [],
  },
  // … the other 17 entries
];

/** Returns the registry entry for slug, throwing if it does not exist. */
export function widget(slug: string): WidgetEntry {
  const entry = WIDGETS.find((w) => w.slug === slug);
  if (!entry) throw new Error(`unknown journey widget: ${slug}`);
  return entry;
}
```

Until a widget's data exists, set its `dataFile` to `null` (the widget task flips it).

- [ ] **Step 4: Write the built-HTML check**

```js
// scripts/verify-journey-build.mjs
/** Verifies the statically built /journey page contains every widget's SSR shell and the TOC anchors. */
import { readFileSync, existsSync } from "node:fs";
import { WIDGETS } from "../src/components/journey/lib/registry.ts";

/** HTML_PATH is where `next build` writes the prerendered /journey document. */
const HTML_PATH = ".next/server/app/journey.html";
if (!existsSync(HTML_PATH)) {
  console.error(`${HTML_PATH} missing — run npm run build first`);
  process.exit(1);
}
const html = readFileSync(HTML_PATH, "utf8");
const failures = [];
const mdx = readFileSync("src/app/journey/content.mdx", "utf8");
for (const [, slug] of mdx.matchAll(/<JourneyWidget slug="([a-z0-9-]+)"/g)) {
  const w = WIDGETS.find((x) => x.slug === slug);
  if (!html.includes(`data-journey-widget="${slug}"`)) failures.push(`${slug}: shell missing`);
  if (w && !html.includes(w.summary.slice(0, 40).replaceAll("&", "&amp;"))) failures.push(`${slug}: summary not server-rendered`);
}
for (const [, heading] of mdx.matchAll(/^## (.+)$/gm)) {
  const id = heading.toLowerCase().replace(/[^a-z0-9\s-]/g, "").trim().replace(/\s+/g, "-");
  if (!html.includes(`id="${id}`)) failures.push(`anchor missing for "${heading}" (#${id})`);
}
if (/\bUD\d+|\bG3b\b|\bR13\b/.test(html.replace(/<script[\s\S]*?<\/script>/g, ""))) failures.push("internal id in rendered HTML");
if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("journey build OK");
```

Note: the anchor slug rule must mirror `remark-heading-ids.mjs` (slugify lower-case). If the check disagrees with the site's ids, import `slugify` and use it exactly like `remark-heading-ids.mjs` does.

- [ ] **Step 5: Add scripts to `package.json`**

```json
"verify:journey": "node scripts/verify-journey-registry.mjs && node scripts/verify-journey-helpers.mjs",
"verify:journey:build": "node scripts/verify-journey-build.mjs"
```

(`verify-journey-helpers.mjs` is created in Task 4; until then run the registry script directly.)

- [ ] **Step 6: Run and commit**

Run: `node scripts/verify-journey-registry.mjs` → `journey registry OK (18 widgets, N facts)`.
Run: `npm run lint` → clean.

```bash
git add src/components/journey/lib/registry.ts scripts/verify-journey-registry.mjs scripts/verify-journey-build.mjs package.json
git commit -m "journey: widget registry and verification scripts"
```

---

### Task 3: Route shell, TOC, Fact component, home link

**Files:**
- Create: `src/app/journey/page.tsx`, `src/app/journey/content.mdx`, `src/app/journey/JourneyPage.module.css`, `src/components/journey/ui/Fact.tsx`, `src/components/journey/ui/CommitLink.tsx`
- Modify: `src/lib/docs/page.ts:105-116` (export `extractPageHeaders`), `src/app/HomeContent.tsx:153-161` (third button)

**Interfaces:**
- Consumes: `extractPageHeaders(source: string): Promise<PageHeader[]>` from `@/lib/docs/page`; `Sidecar` (`items: {id,title,depth}[]`); `facts.json`.
- Produces: `<Fact id="…" />` (server component; renders `display` inside `<span class=fact title="source · provenance · as of">`; throws at build on unknown id); `<CommitLink hash="9e3fe1c25" />` (link to `https://github.com/AirelyDev/rustty/commit/<hash>`, mono short hash).

- [ ] **Step 1: Export `extractPageHeaders`**

In `src/lib/docs/page.ts` change `async function extractPageHeaders(` to `export async function extractPageHeaders(` (keep the doc comment).

- [ ] **Step 2: Create `Fact.tsx` and `CommitLink.tsx`**

```tsx
// src/components/journey/ui/Fact.tsx
import facts from "@/app/journey/data/facts.json";

/** One generated fact as stored in facts.json. */
type FactRecord = { display: string; value: number | string; provenance: string; source: string; asOf: string };

/** FACTS indexes generated facts by id. */
const FACTS = facts as Record<string, FactRecord>;

/** Renders a fact-checked number from facts.json; unknown ids fail the static build. */
export default function Fact({ id }: { id: string }) {
  const f = FACTS[id];
  if (!f) throw new Error(`unknown fact id: ${id}`);
  return (
    <span className="journey-fact" title={`${f.source} · ${f.provenance.toLowerCase()} · as of ${f.asOf}`}>
      {f.display}
    </span>
  );
}
```

```tsx
// src/components/journey/ui/CommitLink.tsx
/** REPO_COMMIT_URL is the public Rustty repository commit URL prefix. */
const REPO_COMMIT_URL = "https://github.com/AirelyDev/rustty/commit/";

/** Renders a short commit hash linking to the public Rustty repository. */
export default function CommitLink({ hash }: { hash: string }) {
  return (
    <a href={`${REPO_COMMIT_URL}${hash}`} className="journey-commit" style={{ fontFamily: "var(--font-jetbrains-mono, ui-monospace)" }}>
      {hash.slice(0, 9)}
    </a>
  );
}
```

If biome rejects the JSON import typing, add `"resolveJsonModule": true` check in `tsconfig.json` (already true in Next defaults) — do not disable lint rules.

- [ ] **Step 3: Create `content.mdx` with the 18 section headings**

```mdx
import Fact from "@/components/journey/ui/Fact";
import CommitLink from "@/components/journey/ui/CommitLink";

Rustty is a port of Ghostty from Zig to Rust, done over <Fact id="days_total" /> with Claude Code agents.

## At a glance
## Why port, and what "port" means here
## The reference as a contract
## Cutting a 345-file cycle into crates
## The first three weeks
## The method matures
## Zig pointers in Rust's aliasing model
## Performance
## Keeping up with upstream
## Checks as the specification
## Never port a bug
## Second opinions
## The verification matrix
## How the orchestration changed
## What it cost
## Unsafe, measured
## Limitations and what's left
## Appendix: methodology
```

- [ ] **Step 4: Create `page.tsx` and CSS**

```tsx
// src/app/journey/page.tsx
import { promises as fs } from "node:fs";
import * as nodePath from "node:path";
import type { Metadata } from "next";
import Sidecar from "@/components/sidecar";
import { H1, P } from "@/components/text";
import customMdxStyles from "@/lib/docs/docs-mdx.module.css";
import { extractPageHeaders } from "@/lib/docs/page";
import Content from "./content.mdx";
import s from "./JourneyPage.module.css";

/** Page metadata for the journey article. */
export const metadata: Metadata = {
  title: "Porting Ghostty to Rust",
  description:
    "How Ghostty was ported from Zig to Rust: architecture, aliasing, verification, defects found, performance and cost, with the data behind every number.",
};

/** CONTENT_PATH is the article source, read at build time for TOC extraction. */
const CONTENT_PATH = nodePath.join(process.cwd(), "src/app/journey/content.mdx");

/** Renders the journey article with its table of contents. */
export default async function JourneyPage() {
  const headers = await extractPageHeaders(await fs.readFile(CONTENT_PATH, "utf8"));
  return (
    <div className={s.page}>
      <main className={s.main}>
        <header className={s.heading}>
          <H1>Porting Ghostty to Rust</H1>
          <P weight="regular" className={s.dek}>
            A technical account of porting the Ghostty terminal emulator from Zig to Rust with agents.
          </P>
        </header>
        <article className={`${customMdxStyles.customMDX} ${s.article}`}>
          <Content />
        </article>
      </main>
      <Sidecar className={s.sidecar} items={headers} />
    </div>
  );
}
```

`JourneyPage.module.css`: grid with a 736 px prose column plus sidecar (copy column widths/breakpoints from `src/app/docs/DocsPage.module.css` `.contentWrapper`/`.docsContentWrapper`/`.sidecar`), and a `.article :global(.journey-wide)` class giving `width: min(1000px, 100vw - 32px); margin-inline: calc((736px - min(1000px, 100vw - 32px)) / 2)` with a phone fallback of `width: 100%; margin-inline: 0` under 800 px. Every rule documented with a comment.

- [ ] **Step 5: Add the home link**

In `src/app/HomeContent.tsx`, inside `<GridContainer className={s.buttonsList}>` after the Documentation button:

```tsx
              <ButtonLink
                href="/journey"
                text="Porting Ghostty to Rust"
                size="large"
                theme="neutral"
              />
```

Check `home-content.module.css` `.buttonsList` wraps three buttons at 390 px; if not, add `flex-wrap: wrap` with a comment.

- [ ] **Step 6: Verify**

Run: `npm run lint && npm run build` → success; `.next/server/app/journey.html` exists.
Run: `node scripts/verify-journey-registry.mjs` → OK.
Run `npm run dev`, open `/journey` and `/` with the Playwright MCP at 1100 px and 390 px: TOC lists the 18 headings, home shows the third button, no console errors.

- [ ] **Step 7: Commit**

```bash
git add src/app/journey src/components/journey/ui src/lib/docs/page.ts src/app/HomeContent.tsx src/app/home-content.module.css
git commit -m "journey: route shell, TOC, Fact and CommitLink, home link"
```

---

### Task 4: UI primitives and pure helpers

**Files:**
- Create: `src/components/journey/lib/{scales.ts,format.ts,replay.ts,tooltip-state.ts,palette.ts}`, `src/components/journey/ui/{InstrumentCard.tsx,TerminalFrame.tsx,Tooltip.tsx,ReplayControls.tsx,LazyMount.tsx,JourneyWidget.tsx,ui.module.css}`, `scripts/verify-journey-helpers.mjs`

**Interfaces:**
- Produces (lib, all pure):
  - `linear(domain: [number, number], range: [number, number]): (v: number) => number` (+ `.invert`)
  - `log(domain, range)`, `time(domain: [Date, Date], range)`, `band(keys: string[], range, padding = 0.1): { (k: string): number; bandwidth: number }`
  - `ticksLinear(domain, count): number[]`, `ticksLog(domain): number[]` (1-2-5 per decade)
  - `fmtInt(n) -> "2,968"`, `fmtPct(x, digits=1, signed=true) -> "−21.7%"` (U+2212 minus), `fmtMs(ms)`, `fmtUsd(n, compact=false) -> "$11,032" | "$23.8k"`, `fmtRatio(x) -> "2.85×"`, `fmtDatePT(iso) -> "Jul 16"`
  - `replayStart(reducedMotion: boolean, lastIndex: number): { index: number; playing: boolean }` → reduced: `{index: lastIndex, playing: false}`, else `{index: 0, playing: false}`
  - `replayStep(state, lastIndex) -> state` (advances, stops at end)
  - `tooltipReducer(state: {pinned: string | null; hover: string | null}, action: {type: "enter"|"leave"|"tap"|"focus"|"blur"|"escape"; id?: string}) -> state`; visible id = `pinned ?? hover`
  - `PALETTE` (role → hex, from Global Constraints), `SERIES` (ordered categorical list: copper, teal, gold, blue, purple, yellow, orange, green, red) and `SHAPES` (`circle, square, triangle, diamond, cross`) for redundant encoding.
- Produces (ui):
  - `InstrumentCard({ label, source, headline?, caption, wide = true, children })`
  - `TerminalFrame({ title, children })` — three dots + title (real command/path)
  - `Tooltip({ id, content, children })` using `tooltipReducer`; hover/focus shows, tap toggles pin, Escape clears; `role="tooltip"`, `aria-describedby`
  - `ReplayControls({ index, lastIndex, playing, onPlay, onPause, onSeek, label })` — buttons ▶/❚❚/↻, range input scrubber, live label (`aria-live="polite"`)
  - `LazyMount({ minHeight, load })` — renders a skeleton `div` with `minHeight` until within 600 px of the viewport (react-intersection-observer `useInView`, already a dependency), then renders `load()` result via `React.lazy`/`Suspense`
  - `JourneyWidget({ slug, asOf? })` — `asOf` is a fact id whose `asOf` value is appended to the caption as "as of <commit>" (required for measured widgets: perf-replay, verification-matrix, test-contract, cutting-the-cycle, defect-map, cost). **Server component**: reads `widget(slug)`, renders `<section data-journey-widget={slug} className="journey-wide">` with `InstrumentCard` (or `TerminalFrame` when `frame === "terminal"`) chrome, the SSR `summary` paragraph (visually small, also the text alternative), and a client `WidgetMount` that maps slug → dynamic import of `@/components/journey/<slug>` Body.

- [ ] **Step 1: Write failing helper tests**

```js
// scripts/verify-journey-helpers.mjs
/** Unit tests for the journey widgets' pure helpers (scales, format, replay, tooltip state). */
import assert from "node:assert/strict";
import { linear, log, band, ticksLog } from "../src/components/journey/lib/scales.ts";
import { fmtInt, fmtPct, fmtUsd, fmtRatio } from "../src/components/journey/lib/format.ts";
import { replayStart, replayStep } from "../src/components/journey/lib/replay.ts";
import { tooltipReducer } from "../src/components/journey/lib/tooltip-state.ts";

const x = linear([0, 10], [0, 100]);
assert.equal(x(5), 50);
assert.equal(x.invert(25), 2.5);
const l = log([0.3, 3.5], [150, 500]);
assert.ok(Math.abs(l(1) - (150 + (Math.log(1 / 0.3) / Math.log(3.5 / 0.3)) * 350)) < 1e-9);
const b = band(["a", "b", "c"], [0, 300], 0);
assert.equal(b("b"), 100);
assert.equal(b.bandwidth, 100);
assert.deepEqual(ticksLog([0.3, 3.5]), [0.5, 1, 2]);

assert.equal(fmtInt(2968), "2,968");
assert.equal(fmtPct(-21.7), "−21.7%");
assert.equal(fmtPct(4.2), "+4.2%");
assert.equal(fmtUsd(11032.43), "$11,032");
assert.equal(fmtUsd(23791, true), "$23.8k");
assert.equal(fmtRatio(2.849), "2.85×");

assert.deepEqual(replayStart(true, 13), { index: 13, playing: false });
assert.deepEqual(replayStart(false, 13), { index: 0, playing: false });
assert.deepEqual(replayStep({ index: 12, playing: true }, 13), { index: 13, playing: false });

let t = { pinned: null, hover: null };
t = tooltipReducer(t, { type: "enter", id: "a" });
assert.equal(t.pinned ?? t.hover, "a");
t = tooltipReducer(t, { type: "tap", id: "b" });
t = tooltipReducer(t, { type: "leave" });
assert.equal(t.pinned ?? t.hover, "b", "tap pins through mouse leave");
t = tooltipReducer(t, { type: "tap", id: "b" });
assert.equal(t.pinned, null, "second tap unpins");
t = tooltipReducer(tooltipReducer(t, { type: "focus", id: "c" }), { type: "escape" });
assert.equal(t.pinned ?? t.hover, null);
console.log("journey helpers OK");
```

- [ ] **Step 2: Run to verify failure**

Run: `node scripts/verify-journey-helpers.mjs` → FAIL (modules missing).

- [ ] **Step 3: Implement the lib modules** to satisfy exactly the interfaces above (doc comment on every export; `ticksLog` returns 1-2-5 multiples within the domain; `fmtPct` uses `"−"` U+2212 for negatives and `"+"` for positives when `signed`).

- [ ] **Step 4: Run tests to pass**

Run: `node scripts/verify-journey-helpers.mjs` → `journey helpers OK`.

- [ ] **Step 5: Implement the UI components** per the interfaces. `JourneyWidget` imports `WidgetMount` (client) which holds a `const LOADERS: Record<string, () => Promise<{ default: ComponentType }>>` map; until a widget exists, its loader renders a "data pending" placeholder so the page builds. Card styles per Global Constraints in `ui.module.css`.

- [ ] **Step 6: Smoke-test in the page**

Add `<JourneyWidget slug="stats-card" />` under "At a glance" in `content.mdx` (placeholder body). Run `npm run lint && npm run build && npm run verify:journey && npm run verify:journey:build` → all OK (the build check passes for the one shell).

- [ ] **Step 7: Commit**

```bash
git add src/components/journey scripts/verify-journey-helpers.mjs src/app/journey/content.mdx package.json
git commit -m "journey: instrument card, terminal frame, tooltip, replay, lazy mount and helpers"
```

---

## Widget tasks (5–15)

Every widget task follows the same seven steps; each task below gives the specifics (data contract, invariants for the tests, mockup reference, deviations from the mockup). **The mockup HTML (`$MOCKUPS/NN-<slug>.html`) is the behavioural and visual reference: reproduce every view, encoding, interaction and insight tile it has, as React + inline SVG, inside the shared primitives.** Its `.md` spec lists data provenance and gaps. Do not add features the mockup lacks; do not drop ones it has unless the task says so.

Common steps for a widget task `<slug>` (module `wNN_<name>.py`):

1. **Data builder test.** Add a `class <Slug>DataTest(unittest.TestCase)` to `test_build_data.py` that calls `REGISTRY["<slug>"](RESEARCH, MOCKBUILD)` and asserts the task's **Invariants** plus: `"_meta" in data`; `json.dumps(data)` < 200 KB; every string field passed through `jargon.strip_internal_ids` is unchanged (walk the structure). Run → FAIL (KeyError).
2. **Data builder.** Implement `widgets/wNN_<name>.py` `build(research, mockbuild) -> dict` reproducing the mockup's embedded data object from the persisted files listed under **Data sources** (port the data step of the listed mockup build script; do not re-derive numbers differently). Apply `strip_internal_ids` to all embedded commit subjects/ledger text and append each changed string to `scripts/journey/jargon-review.txt` (`<slug>\t<before>\t<after>`) for the hand review. Register in `widgets/__init__.py`. Run tests → PASS. Generate: `python3 scripts/journey/build_data.py ... --only <slug>`.
3. **View-model tests.** Write `src/components/journey/<slug>/model.ts` (pure functions from data → plotted series, e.g. aggregation for the current brush, replay frames) and add assertions for it to `scripts/verify-journey-helpers.mjs` using the task's **Model checks**. Run → FAIL, implement, → PASS.
4. **Body.** Implement `Body.tsx` (+ sub-views) reproducing the mockup. Use `Tooltip`, `ReplayControls` + `replayStart(useReducedMotion())`, `PALETTE`/`SERIES`/`SHAPES`. Phone (< 800 px) layout = the mockup's stacked layout; wide plots get an `overflow-x: auto` container with a visible scroll hint.
5. **Wire up.** Add the loader to `WidgetMount`'s `LOADERS`, set `dataFile`/`requiredKeys` in the registry entry, place `<JourneyWidget slug="<slug>" />` in its section of `content.mdx`.
6. **Verify.** `npm run lint && npm run build && npm run verify:journey && npm run verify:journey:build` all pass. With the Playwright MCP on `npm run dev` `/journey#<section-id>`: screenshot at 1100 px and 390 px, compare against the mockup in `$MOCKUPS/gallery.html`, exercise every interaction (hover, tap, keyboard Tab/Enter/Escape, replay, scrub, toggles, brush), check `document.documentElement.scrollWidth <= innerWidth`, zero console errors; emulate `prefers-reduced-motion: reduce` and confirm replays show the final state. Delete any screenshots written into the repo.
7. **Commit** `git add scripts/journey src/app/journey/data/<slug>.json src/components/journey/<slug> src/components/journey/lib/registry.ts src/components/journey/ui/WidgetMount.tsx src/app/journey/content.mdx scripts/verify-journey-helpers.mjs` and `git commit -m "journey: <title> widget"`.

### Task 5: Agents vs human (`agents-vs-human`, flagship, section "The first three weeks")
- Mockup: `$MOCKUPS/02-agents-vs-human.html|md`. Build script: `$MOCKBUILD/build_w02.py`, `embed_w02.py`, `w02-data.json`.
- Data sources: `$RESEARCH/data/mock-agents-vs-human-{grid,daily,commit-lag}.csv`, `-summary.json`, `-annotations.json`, `r2-cost-model-*.csv`, `usage-stats-activity-daily.csv`.
- Invariants: grid has 111 days × 24 hours; total non-merge commits in the grid = `summary.n_commits` (2,718 in the mockup); share of commits landing > 1 h after the last prompt (conservative) = 0.52 ± 0.005; longest unattended run = 180 commits.
- Model checks: `brushStats(data, startDay, endDay)` over the full range reproduces the six whole-period insight tiles; over the 09-23→09-24 range reproduces the 180-commit run.
- Deviations: the lost-window cost ribbon is hatched "not recorded", never zero; model colours Fable 5 vs Opus 5 get a texture or label (colour-blind).

### Task 6: The test contract (`test-contract`, flagship, section "The reference as a contract")
- Mockup: `$MOCKUPS/03-test-contract.html|md`. Build scripts: `$MOCKBUILD/w03_build.py`, `w03_assemble.py`, `w03_names.py`, `w03_crates.py`; data `w03_data.json`, `w03_embed.json`.
- Data sources: `$RESEARCH/data/mock-test-contract-*.csv`.
- Invariants: 4,172 squares at the current reference commit; 211 modules in the parity scatter, none below the diagonal; silent skips total 185 with 134 build-time-as-runtime; 7 resync markers.
- Model checks: `coverageOn(data, "2026-06-28")` mapped coverage ≈ 2,684 / 3,134; first-contract fully met on 2026-07-30.

### Task 7: Cutting the cycle (`cutting-the-cycle`, flagship, section "Cutting a 345-file cycle into crates")
- Mockup: `$MOCKUPS/04-cutting-the-cycle.html|md`. Build scripts: `$RESEARCH/data/mock-cutting-the-cycle-compute.py`, `-relax.py`, `$MOCKBUILD/w04/build.py`.
- Data sources: `$RESEARCH/data/mock-cutting-the-cycle-embed.json` (layout precomputed), `-data.json`.
- Invariants: SCC file count 345 (332 with entry points as pure sources, as the mockup states); 14 cuts; 61 packages; max depth 18; 313 edges.
- Model checks: `cycleSizeAfter(data, k)` for k = 13 → 219 files and k = 14 → 45 (as computed in the mockup); `longestChain(data).length` = 19 nodes (18 steps).
- Deviations: crate names stay (they are real crate names, not internal ids); the unplanned seam (crash reporter → surface) is described in plain words.

### Task 8: Translation lens (`translation-lens`, flagship, section "Zig pointers in Rust's aliasing model")
- Mockup: `$MOCKUPS/07-translation-lens.html|md`. Build scripts: `$MOCKBUILD/w07/build.py`, `assemble.py`.
- Data sources: `$RESEARCH/data/mock-translation-lens.json`, `-counts.csv`, `-unsafe-heuristic.csv`.
- Invariants: 6 patterns, each with Zig and Rust excerpts and ≥ 1 line mapping; ledger rows = 314 across 18 classes; Rust/Zig code ratio 1.68.
- Model checks: `mappedLines(pattern, zigLine)` returns the Rust line set for each mapping; mapping is symmetric (`mappedLines` from Rust side returns the original Zig line).
- Deviations: lines whose comments had ids stripped keep the `•` marker in a footnote explaining "identifier removed"; flag all hand-drawn mappings in `jargon-review.txt`-style list `scripts/journey/mapping-review.txt` for author review.

### Task 9: Three reviewers (`three-reviewers`, flagship, terminal frame, section "Zig pointers in Rust's aliasing model")
- Mockup: `$MOCKUPS/01-three-reviewers.html|md`. Build material: `$MOCKBUILD/w01/` (logs `*.log`, diffs `fix-*.diff`, pre-fix sources).
- Data sources: `$RESEARCH/data/mock-three-reviewers-bugs.json`, `-pattern-counts.csv`.
- Invariants: 4 bugs with fix commits `3662e56bf`, `f046d41a6`, `9e3fe1c25`, `dcae8d61e`; timeline has 38 events totalling 81 defects; each bug has borrow-stack frames for both Stacked and Tree Borrows modes and a Miri output excerpt.
- Model checks: `stackAt(bug, step)` replays push/pop so the frame flagged invalid equals the mockup's flagged frame for each bug.
- Deviations: caption states Miri output was regenerated on 2026-10-08 against the pre-fix commits; terminal title = the actual command from the bug's data; remove `mirihar-*` paths from visible headers (show the crate/test name instead).

### Task 10: Port memory-safety bugs (`memory-safety`, section "Zig pointers in Rust's aliasing model")
- Mockup: `$MOCKUPS/09-memory-safety.html|md`. Build: `$MOCKBUILD/mock/build09.py`, `09data.json`.
- Data sources: `$RESEARCH/data/mock-09-memory-safety-events.csv`.
- Invariants: 38 events, 81 defects, 62 Miri-found, 60 aliasing/provenance, 8 rule flags.
- Model checks: cumulative curve endpoints (all 81, Miri 62).

### Task 11: Perf replay (`perf-replay`, flagship, terminal frame, section "Performance")
- Mockup: `$MOCKUPS/05-perf-replay.html|md`. Build: `$MOCKBUILD/w05/build.py`.
- Data sources: `$RESEARCH/data/mock-05-perf-replay-data.json` and the `perf-*.csv` files it lists.
- Invariants: 14 benchmarks; 14 recorded sweeps on 07-15/07-16; final July suite total 2,269 ms vs Zig 2,897 ms (−21.7%); LTO t-parser 531 ms; 11 negative results; `pin` slot for `a60e9e2a5` present with `status: "pending"` until Task 21.
- Model checks: `scoreboard(stageIndex)` W/T/L at last stage = 14/0/0; geomean at last July stage = 0.758.
- Deviations: no Bun references; the pending slot renders "measurement pending" text (Task 21 fills it).

### Task 12: Moving target (`moving-target`, section "Keeping up with upstream")
- Mockup: `$MOCKUPS/06-moving-target.html|md`. Build: `$MOCKBUILD/mock/m06.py`.
- Data sources: `$RESEARCH/data/mock-06-moving-target-{areas,ghostty-size}.csv`, `timeline-upstream_pins.csv`, `strategy-oracle-pin-advances.csv`.
- Invariants: 7 advances; upstream non-merge commits sum 1,157; wait from upstream commit to pin move first 47.6 h, last 1.7 h.

### Task 13: Defect map (`defect-map`, section "Never port a bug")
- Mockup: `$MOCKUPS/08-defect-map.html|md`. Build: `$MOCKBUILD/mock/build08.py`, `m08build.py`, `m08parse.py`, `m08dirs.py`, `ud.json`.
- Data sources: `$RESEARCH/data/mock-08-defect-map.json`, `bugs-*.csv`, `r2-bugs-*.csv`.
- Invariants: 205 defects (172 Ghostty, 33 dependencies), 135 live; crash-class ReleaseFast split 38/13/9; upstream status: 0 reported, 3 fixed independently by Ghostty, 1 by Highway.
- Deviations: defect ids hidden (author may later decide to show them); the two formerly-private entries may now include file and trigger details (author approved full publication) — take them from `UPSTREAM-DEFECTS.md`.
- Data size: if over 200 KB, move long mechanism texts into a second lazily fetched JSON chunk (`defect-map-details.json`, imported via dynamic `import()` on first click).

### Task 14: Verification matrix (`verification-matrix`, section "The verification matrix")
- Mockup: `$MOCKUPS/10-verification-matrix.html|md`. The mockup's data is embedded in the HTML: write `scripts/journey/widgets/extract_embed.py` (find the JSON literal assigned in the mockup's `<script>`; parse with `json.loads`) and commit the extracted object as the builder's input `scripts/journey/inputs/verification-matrix.json`.
- Invariants: 11 tools × 4 hosts; 8 reference commits; at `a60e9e2a5` the sanitizer, Valgrind, Miri and full-Linux cells are `pending`; at `12752b2ac` total sanitizer/Valgrind test runs = 61,403.
- Deviations: Task 21 flips pending cells to their real status.

### Task 15: What it cost (`cost`, flagship, section "What it cost")
- Mockup: `$MOCKUPS/11-cost.html|md`. Build: `$MOCKBUILD/cost_mock.py`, `embed.py`, `w11.tpl.html`.
- Data sources: `$RESEARCH/data/mock-cost-{data,embed}.json`, `mock-cost-quota-weeks.csv`, `r2-cost-*.csv`, `usage-stats-*.csv`.
- Invariants: recovered Rustty API-equivalent = 11,032.43; modeled lost window low/central/high = 8,725 / 12,759 / 20,331; 17 quota weeks; paid central 422 (range to 600); delegated share 0.756.
- Deviations: **remove the Bun ~$165k reference bar** and any Bun text; the "paid" and "API-equivalent" labels follow the cost framing in Global Constraints.

---

### Task 16: Static charts

**Files:** `src/components/journey/static-charts/{StatsCard,StrategyTimeline,ChecksGrowth,AuditFunnel,Orchestration,UnsafeDensity,PlatformTable}.tsx` (+ one `model.ts`, one CSS module), `scripts/journey/widgets/static_charts.py` (one builder per slug, each writing its own JSON).

These are server-rendered SVG/HTML (no client JS except `Tooltip`), each in an `InstrumentCard`, hover-rich and data-dense in the same style as the widgets. Data and invariants:

- `stats-card` — tiles from `facts.json` only (days, commits, Zig→Rust lines, tests vs Zig tests, July perf result, defects recorded, verification lanes). No Bun row. Invariant: every tile value is a fact id.
- `strategy-timeline` — `$RESEARCH/data/strategy-timeline.csv`, `strategy-bun-adoptions.csv`, `r2-lifetimes-rows.csv`: annotated timeline of method changes (lifetimes ledger 216 → 314 rows, unsafe census, regression detectors, never-port-a-bug change, check-list growth). The single Bun mention is a small "trigger" marker on 07-10 with no comparison. Invariant: ledger series ends at 314.
- `checks-growth` — `strategy-gate-registry-growth.csv`, `strategy-gate-selftest-growth.csv`, `r2-gate-registry.csv`, `codebase-comment-coverage-g3b.csv`, `codebase-c-api.csv`: step lines 11 → 101 entries (75 hard / 26 progress), self-test count 24, pills for comments carried (34,345, 0 missing), C-ABI (35/35 headers, 208/208 functions, 91/91 embedding entry points). Check names are described, never shown as ids.
- `audit-funnel` — `r2-codex-audit-funnel.csv`, `r2-codex-weekly-limit.csv`, `r2-codex-audit-timeline.csv`: 659 rows → 147 groups → 144 confirmed → 140 remediated; Codex weekly meter 2% → 99% with audit waves. No "less capable" wording.
- `orchestration` — `r2-cost-model-weekly.csv`, `usage-stats-context-size-weekly.csv`, `usage-stats-activity-daily.csv`, `strategy-workflow-scripts-by-date.csv`: model streamgraph by week, context median 77k → 204k with p90 band, workflow scripts → worktree subagents.
- `unsafe-density` — `codebase-unsafe-census-timeseries.csv` (+ HEAD point 26.66 per kLOC), `codebase-unsafe-per-crate.csv`, `codebase-vt-unsafe-by-area.csv`. No external comparison band.
- `platform-table` — `r2-platforms-matrix.csv`: per platform what builds, runs, was tested by hand, which lanes ran, last evidence date, honest one-line claim.

Steps: (1) builder tests with the invariants above → FAIL; (2) builders → PASS, generate; (3) model tests in `verify-journey-helpers.mjs` (e.g. step-line series monotonic, funnel stages non-increasing) → FAIL → implement → PASS; (4) components; (5) registry + MDX placement; (6) verify as in the common step 6; (7) commit `journey: static charts`.

---

## Prose tasks (17–19)

Each prose task writes its sections of `content.mdx`. Rules: technical register, no marketing; every number is a `<Fact id>` (append new rows to `facts.csv` with source + provenance and regenerate); commit hashes via `<CommitLink>`; Zig/Rust code blocks are real excerpts (from the oracle checkout / repo at the stated commit); verbatim rule quotes from `$RESEARCH/r2-glossary.md` "Quotable rule texts"; no internal ids; Bun only in §6; each section ends on a measurable result; widgets placed as in the spec table. Sources per section: `$RESEARCH/r2-outline-plain.md` (section bullets) + the dossiers it cites + the widget's `.md` spec (insights).

Steps per task: (1) draft sections; (2) `python3 scripts/journey/build_data.py … --only facts` after adding facts; (3) `npm run verify:journey` (catches unknown facts, ids, Bun outside §6 — extend the registry script with a check that "Bun" appears only between the "## The method matures" heading and the next `## `); (4) `npm run lint && npm run build && npm run verify:journey:build`; (5) read the rendered page section in the browser; (6) commit `journey: prose <sections>`.

- [ ] **Task 17:** §1–§5 (At a glance → The first three weeks), ~2.5k words. Include the origin prompts (quoted from `$RESEARCH/r2-human-story.md`), the reference-as-contract method, the 14 cuts and layering decisions in plain words.
- [ ] **Task 18:** §6–§10 (The method matures → Checks as the specification), ~3.5k words. §6 has the single concise Bun credit; §7 is the technical core with 2–3 Zig/Rust side-by-side blocks beyond the lens widget; §8 includes methodology (ns/op slopes, same-session Zig, ReleaseFast, LTO).
- [ ] **Task 19:** §11–§18 (Never port a bug → Appendix), ~3k words. §15 states the cost framing exactly as in Global Constraints; §17 uses the platform table claims verbatim; §18 lists how every headline number was measured and the hardware.

---

### Task 20: Accessibility, mobile, print and hand-review pass

**Files:** any `src/components/journey/**`, `src/app/journey/**`; create `docs/superpowers/plans/journey-review-checklist.md` (author-facing list).

- [ ] **Step 1:** Playwright MCP sweep of the full page at 1100 px and 390 px: no page-level horizontal scroll (`scrollWidth <= innerWidth`), no console errors, every widget mounts when scrolled to, skeleton→widget causes no visible jump (measure the heading's `getBoundingClientRect().top` before/after mount for `/journey#performance` — difference ≤ 4 px).
- [ ] **Step 2:** Keyboard-only pass: Tab reaches every control and data mark with a tooltip; Escape closes; focus ring visible. Touch emulation (`hasTouch`): tap pins tooltips on every widget.
- [ ] **Step 3:** `prefers-reduced-motion: reduce`: every replay renders its final state, nothing auto-animates.
- [ ] **Step 4:** Colour: run the dataviz palette validator on every categorical set used (or check manually against a deuteranopia simulation in DevTools); add shape/texture/labels wherever two series are confusable.
- [ ] **Step 5:** Print preview (DevTools emulate print): text legible, cards not cut mid-chart (`break-inside: avoid` on cards).
- [ ] **Step 6:** Write `journey-review-checklist.md` for the author: contents of `scripts/journey/jargon-review.txt` and `mapping-review.txt`, the rule quotes used, the regenerated Miri outputs, the estimated classifications (defect discovery method, ReleaseFast consequence, era classifier), and every `ESTIMATED` fact.
- [ ] **Step 7:** `npm run lint && npm run build && npm run verify:journey && npm run verify:journey:build && npm run verify:animation` → all pass. Commit `journey: accessibility, mobile and print pass`.

### Task 21: Data refresh at the current reference commit (rustty repo + regenerate)

Coordinate with the author: they pause in-flight validation on `phase-8-upstream-resync` first.

- [ ] **Step 1: Benchmarks.** In `~/projects/rustty` on `phase-8-upstream-resync` (current reference commit `a60e9e2a5`): build Zig with `-Doptimize=ReleaseFast` and Rustty's bench binaries per the recorded methodology in `$RESEARCH/perf.md` (ns/op loops-sweep, same-session Zig, no LTO), run all 14 benchmarks, and save raw results under `.port/journey-article/research/data/perf-pin-a60e9e2a5-{rust,zig}.jsonl` plus a ratio CSV with the same columns as `perf-ratio-pin-12752b2ac-idle.csv`. Record the exact commands in `research/perf-a60e9e2a5.md`.
- [ ] **Step 2: Verification lanes.** Determine the state of the sanitizer/Valgrind/Miri lanes at `a60e9e2a5` from the phase-8 records. If they are still running or failing, the article keeps "pending" (do **not** fix rustty code as part of this plan; that is phase-8 work). If green, record per-lane test counts/durations in `research/verification-a60e9e2a5.csv`.
- [ ] **Step 3: Regenerate.** Update the perf-replay builder to fill the `pin` slot from Step 1 (status `measured`), the verification-matrix input from Step 2, and the affected facts (`as_of` = `a60e9e2a5`). Add builder tests: pin slot has 14 benchmarks with positive ms values. Run `python3 -m unittest scripts/journey/test_build_data.py`, regenerate all JSON, run every verify script and the build.
- [ ] **Step 4:** Update prose that cites the new numbers (Task 18/19 sections) and commit `journey: refresh measurements at a60e9e2a5`.
