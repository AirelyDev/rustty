# Porting article: source snapshot

A copy of the source of the "Porting Ghostty to Rust" article (`/porting`), taken from the
website repository's `main` branch at commit `b08bece5510e1ffcf47bc70bb4d2c0d3a66ed59e`
(the repository is `AirelyDev/rustty-website`; the article's work moved to `main` from the `journey-article`
branch, which this snapshot was first taken from).
It is kept here so that the article's code, data builders, generated data and plans sit next to
the research data they cite. It is a snapshot: the website branch stays the source of truth.

Nothing here is built in this repository. The paths are the website repository's own:

- `src/app/porting/`: the route, the article (`content.mdx`) and its generated data
- `src/components/porting/`: the widgets and their view models
- `scripts/porting/`: the data builders (`build_data.py`, `widgets/`), facts (`facts.csv`),
  the redacted-value scanner and their tests
- `scripts/verify-porting*.mjs`, `scripts/lib/porting-links-guard.mjs`: the article's checks
- `public/porting/`: the redacted prompt log the page fetches (the live terminal's WebAssembly
  binaries, `public/porting/wterm/`, are left out)
- `docs/superpowers/`: the article's specs, plans and the author review checklist. The website
  branch's four July docs about the website itself (the Ferris animation revamp and the hero
  terminal cues, a spec and a plan each) are left out: they are not about the article.

To build it, check out `main` in the website repository and run, from its root:

```sh
python3 scripts/porting/build_data.py   # regenerate the data (needs the untracked research dir)
npm run verify:porting
npm run lint && npm run build && npm run verify:porting:build
```
