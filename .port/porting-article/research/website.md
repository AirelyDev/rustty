# Website research: where and how the Rustty journey article should live

Repo: `/Users/gael/projects/ghostty-website`, branch `codex/rustty-website-rebrand` (HEAD `d052e941`, 2026-10-07). Read-only study; no branch switch.

Provenance tags: **MEASURED** = computed by a command (given), **DOCUMENTED** = read from a file (path given), **ESTIMATED** = judgement.

---

## 1. Framework and build

| Fact | Value | Provenance / evidence |
|---|---|---|
| Framework | Next.js **16.1.6**, App Router (`src/app/`), React **19.2.4** | MEASURED `grep version node_modules/next/package.json`; DOCUMENTED `package.json` |
| MDX | `@next/mdx` 16.1.6 + `@mdx-js/loader` 3.1.1; `pageExtensions: ts,tsx,js,jsx,md,mdx` | DOCUMENTED `next.config.mjs` |
| Remark/rehype | `remark-frontmatter`, `remark-gfm`, local `remark-gfm-alerts-as-callouts.mjs`, `remark-heading-ids.mjs`; `rehype-highlight-all.mjs` | DOCUMENTED `next.config.mjs`, `src/lib/docs/*.mjs` |
| Global MDX component map | `/Users/gael/projects/ghostty-website/mdx-components.tsx` (h1-h6 -> `JumplinkHeader`, `pre`/`code` -> `CodeBlock`/`Mermaid`, plus `Callout`, `Note`, `CardLinks`, `ButtonLinks`, `VTSequence`, `Video`, ...) | DOCUMENTED |
| TS | strict, `moduleResolution: bundler`, `resolveJsonModule: true`, alias `@/* -> ./src/*` | DOCUMENTED `tsconfig.json` |
| Build / verify | `npm run build` (required by AGENTS.md); `npm run lint` = `biome check .` (fix warnings too); `npm run verify:animation` (5 node scripts for the hero) | DOCUMENTED `package.json`, `AGENTS.md` |
| Dev | `make` -> `source .env.local && npm run dev` | DOCUMENTED `Makefile` |
| Lint scope | Biome 2.4.4 only lints `src/**/*.ts(x)` and `next.config.mjs`; double quotes, semicolons, 2-space; `noDangerouslySetInnerHtml` off; `useExhaustiveDependencies` off | DOCUMENTED `biome.json` |
| Code-style rule | "All functions and top-level constants should be documented with comments" | DOCUMENTED `AGENTS.md` |
| CI | GitHub Actions `test.yml`: `alejandra --check` (nix) + `npm run lint` via `nix develop` | DOCUMENTED `.github/workflows/test.yml` |
| Static constraint | "The site should be fully static, no server-side rendering, functions, etc." No `output: "export"` is set; every route is SSG via `generateStaticParams`/static server components. Deployed via Vercel env (`VERCEL_ENV`, `VERCEL_GIT_COMMIT_REF`) or Docker `next start` | DOCUMENTED `AGENTS.md`, `next.config.mjs`, `Dockerfile` |
| Prerendered routes in last local build | 106 | MEASURED `python3 -c "json.load(open('.next/prerender-manifest.json'))['routes']"` |
| CSP | None configured. Only header added is `X-Robots-Tag: noindex` on non-production | DOCUMENTED `next.config.mjs` `headers()` |
| Page-data budget | `experimental.largePageDataBytes = 2 MiB` (raised for the hero animation + config reference) | DOCUMENTED `next.config.mjs` |

**Implication:** an article route must be a server component (or MDX page) with no `cookies()/headers()/fetch` at request time; all data must be imported at build time (JSON imports or `fs` reads in a server component, as `terminal-data.tsx` does). Keep serialized chart data passed to client components well under 2 MiB.

## 2. Routes and how content is added

Existing routes (MEASURED `find src/app -type f`):

- `/` -> `src/app/page.tsx` (server; loads `terminals/home/**` text frames) -> `src/app/HomeContent.tsx` (client).
- `/docs/[[...path]]` -> `src/app/docs/[[...path]]/page.tsx`, catch-all reading `./docs/**/*.mdx` via `src/lib/docs/page.ts` (gray-matter frontmatter: `title`, `description`, `hideSidecar`, `editOnGithubLink`), nav from `docs/nav.json`. Rendered by `src/app/docs/DocsPageContent.tsx` (left NavTree + breadcrumbs + MDX + right `Sidecar` TOC). 102 `.mdx` pages (MEASURED `find docs -name '*.mdx' | wc -l`). Only `.mdx` become pages; `docs/superpowers/**/*.md` (Ferris animation specs/plans) are not routed.
- `/download` -> `src/app/download/page.tsx` (simple `SectionWrapper` + `H1` + content; good template for a standalone page).
- No blog/news route exists. There is no existing long-form article layout outside `/docs`.

## 3. Home page and navigation

- `src/app/layout.tsx`: `NO_CHROME_PATHS = ["/"]` — the home page renders **without** navbar and footer. Navbar/footer links are `navLinks = [Docs, GitHub]` + CTA `Download`.
- `src/app/HomeContent.tsx`: full-viewport animated terminal (200x82 cells, Ferris + composed shell from `hero-shell.ts`, clip graph from `home-animation-graph.ts`), then a tagline `P`, then `GridContainer className={s.buttonsList}` with two `ButtonLink`s: `Download` (brand, large) and `Documentation` (neutral, large). These two buttons are the only navigation on `/`.
- Mobile: `home-content.module.css` makes buttons `min-width: min(100%, 300px)` under 560px with 12px gap; flex-wrap already handles a third button.

## 4. Design system

- **Dark-only.** `html { color-scheme: dark }`; no light theme. `@media print` inverts the gray scale and atom-one palette (DOCUMENTED `src/styles/globals.css`). Charts that use CSS variables (not hard-coded hex) get print inversion for free.
- **Tokens** (full list in `data/website-design-tokens.csv`): `--brand-color`/`--rust-copper #d97934`, `--rust-gold #ffbc7a`, `--rust-teal #64d8c6`, `--rust-ember #7a321d`, grays `--gray-0 #0f1111` (bg) ... `--gray-9 #f4f1ed` (headings), atom-one code palette (blue/green/yellow/purple/red/orange/teal) which also backs callout colours.
- Home background: radial copper glow `rgba(217,121,52,0.14)` over `--gray-0`.
- **Fonts:** Pretendard Std variable (local woff2, `--pretendard-std-variable`) for body/display; JetBrains Mono 400 (`next/font/google`, `--jetbrains-mono`) for code. Text scale in `src/components/text/Text.module.css` (h1 35px, h2 25px, body 16px/1.6, body colour `--gray-5`).
- **Styling:** CSS Modules with native nesting (`& .x`), `classnames`. No Tailwind. Tables are globally styled in `src/app/layout.module.css` (borders `--gray-2/3`, block overflow-auto) — markdown tables in MDX look right without extra work.
- Layout widths: `StandardGridConfig` max 1300px, padding 48/24px (`src/components/grid-container/index.tsx`).
- Motion: `prefers-reduced-motion` is honoured in `HomeContent.tsx`, `animated-terminal/index.tsx`, Navbar and ScrollToTop CSS. Article animations must do the same.

## 5. Existing visual / animation code (reusable)

Full table: `data/website-reusable-components.csv`. Highlights:

- `src/components/animated-terminal/{index.tsx,playback.ts}` — frame-clip player with a pure playback state machine (`playback.ts` is importable from node scripts; type-only imports). Could drive "replay" style figures (Bun's "replay phase D" / "replay 11 days in 30s").
- `src/app/hero-shell.ts` (425 lines) — composes a typed shell session over frames (CHAR_TICKS, ENTER_PAUSE_TICKS, cursor blink). Pattern reusable for an animated "cargo test / bench" terminal figure.
- `src/app/home-animation-graph.ts` (117 lines) — weighted clip-transition graph; homepage-specific.
- `src/components/terminal/index.tsx` — static terminal window chrome (macOS / Adwaita by UA).
- `src/components/mermaid/index.tsx` — lazy-loads mermaid 11.12.3 client-side; **forces every stroke/fill to white and fills to brand via CSS overrides** in `docs-mdx.module.css`. Fine for monochrome structure diagrams (crate graph, phase flow), unusable for multi-series colour charts.
- `src/components/sidecar` + `jumplink-header` (uses `react-intersection-observer` + `zustand` store `src/lib/docs/store.ts`) — the "On this page" TOC, equivalent to the Bun post's TOC.
- Verification scripts `scripts/verify-*.mjs` — precedent for node-side gates over animation data (could add a data-integrity check for article JSON).

**No chart library is a direct dependency.** d3 7.9.0 is present only transitively via mermaid (MEASURED `npm ls d3`). Do not import it without declaring it.

## 6. Branding / attribution already present

- `src/lib/brand.ts`: `RUSTTY_SITE_URL = https://rustty.sh`, `RUSTTY_REPO_URL = https://github.com/AirelyDev/rustty`, `RUSTTY_WEBSITE_REPO_URL = https://github.com/AirelyDev/rustty-website`, `SITE_DESCRIPTION`.
- Footer copyright: "© {year} Rustty. A port of Ghostty by Mitchell Hashimoto and the Ghostty contributors." (`src/app/layout.tsx:111`).
- `docs/about.mdx` opens with a `<Note>` crediting Ghostty as the source of the port and linking ghostty.org/docs/about; `docs/index.mdx` description: "It is a Rust port of Ghostty."
- 20 docs files mention Ghostty; 19 files under docs/src link ghostty.org / ghostty-org (MEASURED `grep -rlI`).
- Mascot: Ferris the Rustacean (CC0, rustacean.net; README). SVGs in `public/ferris-rustacean*.svg`, logo `public/rustty-logo.svg`.
- Branch history (MEASURED `git log --author=Gael main..HEAD`, `data/website-branch-commits-by-day.csv`): 50 Gael commits — 42 on 2026-06-28..07-04 (rebrand `a5ee3721`, Ferris hero animation revamp), 8 on 2026-10-07 (restore upstream docs structure, license notice, Ghostty attribution, version parity). The rest of `main..HEAD` (26 commits) are upstream website commits merged in. Diff `main..HEAD`: 918 files, +41,403/-11,645 (mostly `terminals/` frame data, 568 files / 13 MB).

## 7. Reference: Bun article visual vocabulary (to mirror)

From https://bun.com/blog/bun-in-rust (WebFetch, 2026-10-08): single-column long-form with TOC; interactive figures with **replay buttons** (compiler-error countdown dashboard, CI "race to green" lane grid, commit-activity replay), a commits-per-hour histogram with hover, code before/after cards for review-caught bugs, and plain tables for perf/memory/binary size. All of these are achievable with inline SVG + small React state.

## 8. Recommendations

### 8.1 Route and file layout (standalone page, not under /docs)

Rationale: `/docs` wraps content in a left NavTree column, which narrows wide charts; the Bun-style article wants full single-column width with a right TOC. A standalone route like `/download` gives that, and can still reuse `Sidecar`/`JumplinkHeader`.

```
src/app/journey/page.tsx                 server: metadata + layout shell (SectionWrapper, H1, Sidecar)
src/app/journey/JourneyPage.module.css   article column (~760-820px prose, charts can break out to 1100px)
src/app/journey/content.mdx              article prose; imports chart components; h2/h3 via mdx-components.tsx
src/app/journey/data/*.json              build-time chart data (copied from rustty/.port/journey-article/research/data)
src/components/journey-charts/           one folder per chart, "use client", inline SVG + CSS module
  chart-frame/        shared figure chrome: title, caption, source/provenance line, replay button, reduced-motion
  scales.ts           tiny linear/time/band scale helpers (documented per AGENTS.md)
  commit-timeline/    commits per day/hour histogram (Bun #4/#5)
  phase-timeline/     phase/gantt lanes (Bun #3 style)
  ...
```

Notes:
- `page.tsx` imports `content.mdx` as a component (`import Content from "./content.mdx"`); `mdx-components.tsx` styling then applies automatically. Alternatively `page.mdx` directly with `export const metadata`.
- Page headers for the Sidecar: either hand-maintain the list in `page.tsx` or reuse the remark heading-id logic from `src/lib/docs/page.ts` by reading the MDX source with `fs` at build time.
- Wrap prose in `customMdxStyles.customMDX` (`src/lib/docs/docs-mdx.module.css`) to match docs typography.

### 8.2 Home-page link

Add a third button in `src/app/HomeContent.tsx` inside `s.buttonsList`, after Documentation:

```tsx
<ButtonLink href="/journey" text="How we ported it" size="large" theme="neutral" />
```

(exact wording to be decided; keep neutral theme so Download stays the single brand CTA). Optionally also add `{ text: "Journey", href: "/journey" }` to `navLinks` in `src/app/layout.tsx` (navbar + footer on all non-home pages) and a sentence/link in `docs/about.mdx` under the attribution `<Note>`.

### 8.3 Visualization approach

**Inline SVG React client components, no chart library.** Reasons:
- Zero new dependencies; site already favours hand-rolled visual code (hero animation, playback state machine, verify scripts).
- Full control over the dark palette via CSS variables (copper/gold/teal/ember series; atom-one green/red for pass/fail), automatic print inversion.
- Mermaid's forced-white overrides rule it out for colour charts; keep Mermaid only for monochrome structure diagrams (crate dependency DAG, phase flow).
- Replay animations: `requestAnimationFrame` + React state, gated on `prefers-reduced-motion` (show final state), triggered by a button and/or `useInView` from the already-installed `react-intersection-observer`.
- If a chart needs real layout math (sankey, force), add `d3-scale`/`d3-shape`/`d3-sankey` as **explicit** dependencies rather than relying on mermaid's transitive copy.

Colour assignment proposal (from tokens): Rust/rustty = `--rust-copper`, Zig/Ghostty upstream = `--rust-teal`, secondary series `--rust-gold`, pass `--atom-one-green`, fail `--atom-one-red`, gridlines `--gray-2`, axis text `--gray-3/4`, panel `--gray-1`.

Every figure should carry a caption line stating data source and provenance (MEASURED/DOCUMENTED), matching the research rule.

### 8.4 Verification for the article PR

`npm run lint` (zero warnings), `npm run build` (confirm `/journey` appears as static in the route table), `npm run verify:animation` (home page untouched behaviour), manual check at 375px width for horizontal overflow on charts (wrap SVGs in `overflow-x: auto` or use `viewBox` + `width:100%`).

## Fact-check

Independent re-derivation (2026-10-08, branch `codex/rustty-website-rebrand`).

| # | Fact | Status | Evidence |
|---|------|--------|----------|
| 1 | Next 16.1.6 / React 19.2.4 / @next/mdx 16.1.6, md/mdx pageExtensions | confirmed | installed `node_modules/*/package.json` versions; `next.config.mjs` `pageExtensions` includes md, mdx |
| 2 | Fully static, no `output: export`, 106 prerendered routes | confirmed | AGENTS.md line 7; no `output` key in next.config.mjs; `.next/prerender-manifest.json` has 106 `routes` (last local build) |
| 3 | No CSP; X-Robots-Tag noindex off-production; largePageDataBytes 2 MiB | confirmed | `headers()` only pushes X-Robots-Tag when `VERCEL_ENV !== "production"`; no Content-Security-Policy anywhere in src/config |
| 4 | Home has no chrome; only Download + Documentation ButtonLinks | confirmed | layout.tsx:32 `NO_CHROME_PATHS = ["/"]`; HomeContent.tsx:156-161 buttonsList |
| 5 | No direct chart lib; d3 7.9.0 transitive via mermaid 11.12.3 | confirmed | `npm ls d3` -> mermaid@11.12.3 > d3@7.9.0 (and dagre-d3-es deduped); d3 not in package.json |
| 6 | Mermaid monochrome (white strokes, brand fill via CSS) | confirmed | mermaid/index.tsx themeVariables all `#ffffff`; docs-mdx.module.css:53-63 `fill: var(--brand-color)` / `fill: white` |
| 7 | Dark-only CSS vars, tokens, print inverts grays, Pretendard + JetBrains Mono | confirmed | globals.css:33-65 tokens; `@media print` (line 152) redefines `--gray-0: #ededf1`; no `prefers-color-scheme`; text/index.tsx localFont pretendard-std-variable.woff2 + JetBrains_Mono |
| 8 | Lint/build gates, doc-comment rule, CI alejandra + biome | corrected (minor) | `lint` = `biome check .`; biome.json `files.includes` covers `src/**/*.ts(x)` **and `next.config.mjs`** (not src only). Rest confirmed: AGENTS.md "fix all issues including warnings", doc-comment rule line 11, test.yml alejandra + biome jobs, `verify:animation` script exists |
| 9 | 50 Gael commits (42 Jun 28-Jul 4, 8 Oct 7); diff 918 files +41,403/-11,645 | confirmed | per-date counts 14+26+1+1 = 42 and 8 on 2026-10-07; `git diff --shortstat main..HEAD` identical to merge-base diff. Note: main..HEAD has 76 commits total; the other 26 are upstream authors (trag1c 11, Mitchell Hashimoto 2, ...), so the 918-file diff is not Gael-only |
| 10 | Attribution: footer line, about.mdx credit, rustty.sh / AirelyDev/rustty, Ferris CC0 | confirmed | layout.tsx:111; docs/about.mdx:8-13; brand.ts:2,5; README.md:34 |
| 11 | Recommendation `/journey` route + third ButtonLink | unverifiable (proposal) | Design proposal, not a fact; consistent with existing structure (src/app has docs/, download/ siblings; buttonsList at HomeContent.tsx:156) |
