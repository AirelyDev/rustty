# Porting article data pipeline

Generates the committed data behind `/porting` ("Porting Ghostty to Rust"): `src/app/porting/data/facts.json` and one `<widget>.json` per widget.

## Inputs

| Input                        | Where it lives                                                                                 | What it holds                                                                                              |
| ---------------------------- | ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Research (`--research`)      | `$PORTING_RESEARCH`, default `/Users/gael/projects/rustty/.port/journey-article/research`      | Dossiers (`*.md`, with `## Fact-check` sections), `BRIEF.md` §2 fact-checked values, `data/*.csv`/`*.json` |
| Mockup build (`--mockbuild`) | `$PORTING_MOCKBUILD`, default `/Users/gael/projects/rustty/.port/journey-article/mockup-build` | Embedded mockup data some builders read (for example the prompt minutes in `w02-data.json`)                |
| Fact table                   | `scripts/porting/facts.csv` (this repository)                                                  | Every number the prose and the stats card print: `id,display,value,provenance,source,as_of`                |
| Hand inputs                  | `scripts/porting/inputs/` (this repository)                                                    | Typed-in tables, such as the verification-matrix lanes                                                     |
| Widget builders              | `scripts/porting/widgets/*.py`                                                                 | One `build(research, mockbuild) -> dict` per widget, registered in `widgets/__init__.py`                   |

The research directory is committed to the rustty repository on the local branch `journey-article-research` (commit `882e3df21`, at `.port/porting-article/research/`; not pushed yet), and the pipeline still reads the untracked source folder by default. The mockup build is not committed. To run the pipeline elsewhere, point `PORTING_RESEARCH` and `PORTING_MOCKBUILD` at a copy.

## Regenerate

```sh
# everything (facts and every widget) into src/app/porting/data
python3 scripts/porting/build_data.py

# one output only: "facts" or a widget slug
python3 scripts/porting/build_data.py --only facts
python3 scripts/porting/build_data.py --only agents-vs-human

# explicit paths (override the environment and the defaults)
python3 scripts/porting/build_data.py --research <dir> --mockbuild <dir> --out src/app/porting/data
```

Builders that rewrite visible text append each rewrite to `scripts/porting/jargon-review.txt` for hand review.

## Test

```sh
python3 -m unittest scripts/porting/test_build_data.py   # pipeline and data invariants (reads the same env vars)
npm run verify:porting         # registry, facts, prose and link guard; permalink paths and quotes; pure TS helpers
npm run lint && npm run build
npm run verify:porting:build   # built HTML holds every widget's SSR shell and the anchors
```

`npm run verify:porting` needs local clones of both repositories, because two of its checks read them with git: `scripts/verify-porting-paths.mjs` checks that every GitHub file or directory permalink on the page exists at its commit (`git cat-file`), and `scripts/verify-porting-quotes.mjs` checks that every quote linked to a committed Rustty source appears verbatim in the anchored lines (`git show`). Set the clones with:

| Variable       | Default                          | Must contain                                                                                   |
| -------------- | -------------------------------- | ---------------------------------------------------------------------------------------------- |
| `GHOSTTY_REPO` | `/Users/gael/projects/ghostty`   | the Ghostty reference commits the page links (`4749c4e93`, `a60e9e2a5`, `246f70287`)            |
| `RUSTTY_REPO`  | `/Users/gael/projects/rustty`    | Rustty `main` at `58aa62a81` and the research branch `journey-article-research` (`882e3df21`) |

Without them the two checks fail.

The author's pre-publication checklist is `docs/superpowers/plans/journey-review-checklist.md`.
