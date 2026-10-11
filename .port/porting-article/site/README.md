# Porting article: source snapshot

A copy of the source of the "Porting Ghostty to Rust" article (`/porting`), taken from the
website repository's branch `journey-article` at commit `775a784f997a79e72166c4906318134215f6b104`.
It is kept here so that the article's code, data builders, generated data and plans sit next to
the research data they cite. It is a snapshot: the website branch stays the source of truth.

Nothing here is built in this repository. The paths are the website repository's own:

- `src/app/porting/`: the route, the article (`content.mdx`) and its generated data
- `src/components/porting/`: the widgets and their view models
- `scripts/porting/`: the data builders (`build_data.py`, `widgets/`), facts (`facts.csv`),
  the redacted-value scanner and their tests
- `scripts/verify-porting*.mjs`, `scripts/lib/porting-links-guard.mjs`: the article's checks
- `public/porting/`: the redacted prompt log the page fetches
- `docs/superpowers/`: the specs, plans and the author review checklist

To build it, check out `journey-article` in the website repository and run, from its root:

```sh
python3 scripts/porting/build_data.py   # regenerate the data (needs the untracked research dir)
npm run verify:porting
npm run lint && npm run build && npm run verify:porting:build
```
