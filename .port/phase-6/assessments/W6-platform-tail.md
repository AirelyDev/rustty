# P6-W6 — platform tail: assess + port record

Range `88f57ee66..20abdb50a`, oracle read-only at `/Users/gael/projects/ghostty` (pin
`20abdb50a`). Branch `phase-6/w6-platform-tail`, base `62c1690d`. Port target: range END STATE.

## Coverage statement

Every range file in the W6 row of `.port/phase-6/README.md` was read and vetted:

| oracle path (range) | verdict | rustty home |
|---|---|---|
| `po/fr.po` `po/ru.po` `po/uk.po` (405/405/382 diff lines) | **ABSENT → PORTED** | `po/{fr,ru,uk}.po` |
| `po/sr.po` `po/sr@latin.po` (NEW, 1054/1135) | **ABSENT → PORTED** | `po/sr.po`, `po/sr@latin.po` + `xtask/src/translations.rs` `LOCALES` |
| `src/font/res/NotoColorEmoji.ttf`, `NotoEmoji-Regular.ttf` (binary) | **ABSENT → PORTED** | `crates/rustty-font/res/` + KAT regold (rustty-only golden) |
| `build.zig.zon` + `.json/.nix/.txt`, `flatpak/zig-packages.json` (themes hash only) | **N-A as manifests; the themes hop PORTED** | `crates/rustty-extra/themes/` (+4), `themes.manifest`, `xtask/src/res_themes.rs` |
| `.github/workflows/milestone.yml` | **ABSENT → PORTED** | same path |
| `.github/workflows/test.yml` (freestanding job, END STATE of `10b7158e8`/`fc7bfa60b`/`5e02d0014`) | **N-A today, finding recorded** (§4) | — |
| `.github/workflows/release-tag.yml`, `release-tip.yml` (Sparkle 2.9.6 `SPARKLE_VERSION`) | **W5's** (plan: `c18198325` "release workflows SPARKLE_VERSION" is in the W5 row) | — |
| `.github/VOUCHED.td` (7 commits) | **N-A** | rustty has no `.github/VOUCHED.td` (`ls .github` → `DISCUSSION_TEMPLATE dependabot.yml pinact.yml scripts workflows`); Phase-4/5 precedent (P5-E20) |
| `CODEOWNERS` (`74e0bfdf2`, `ec3e384d2`: `+/po/sr.po @ghostty-org/sr_RS`, `+/po/sr@latin.po @ghostty-org/sr_RS`) | **N-A** | rustty's `CODEOWNERS` is a deliberately forked single-owner file (`* @AirelyDev`) with NO per-locale rows for ANY catalog (verified: 45 lines, no `/po/` entries); Phase-4 reconciliation O14 set this precedent for `da.po`. Adding two `@AirelyDev` rows already covered by `*` would be vacuous. |
| `src/os/i18n_locales.zig` (`7aa476714`: `+"sr", +"sr@latin"`) | **W4's** (`crates/rustty-os/src/i18n_locales.rs`) — see §1 coupling | — |

## 1. po catalogs

**Method** (Phase-4/5 "po re-mirror"): byte-mirror of each END-STATE oracle catalog through the
identity transform rustty's already-mirrored catalogs use. The transform was DERIVED, not
assumed, by diffing `de.po`, then PROVEN on four already-mirrored catalogs before use:

```
xf = sed -E  com.mitchellh.ghostty→io.airely.rustty ; m@mitchellh.com→gael@airely.io ;
             src/apprt/gtk/ui/→crates/rustty-apprt/assets/ui/ ;
             src/apprt/gtk/class/X.zig→crates/rustty-apprt/src/gtk/class/X.rs ;
             src/input/X.zig→crates/rustty-input/src/X.rs ; Ghostty→Rustty ; ghostty→rustty
diff <(xf oracle/po/L.po) po/L.po  →  de: 0  bg: 0  nl: 0  zh_TW: 0 diff lines
```

Applied to `fr`, `ru`, `uk`, `sr`, `sr@latin`; every output re-diffs against `xf(oracle)` at 0
lines; residual `ghostty` tokens in the five: 0. `sr.po`/`sr@latin.po` are the END STATE
(`cdef14a1f` X-Generator removal and `8bf7e517f` sr@latin regeneration are inside the mirrored
bytes; nothing intermediate was ported). The `.pot` is not in the range; `xf(oracle .pot)` vs
`po/io.airely.rustty.pot` differs only in the pre-existing copyright-holder header line
(rustty's xtask `COPYRIGHT_HOLDER`), untouched.

**Locale enumerations besides the catalogs** (`grep -rIl zh_TW` outside `po/`):
`xtask/src/translations.rs` `LOCALES` (mine — **PORTED**: `+"sr", "sr@latin"` after `"da"`,
the oracle's `i18n_locales.zig` order), `crates/rustty-os/src/i18n_locales.rs` (**W4's**
half of `7aa476714`; the two lists are documented as mirrors — W7 should confirm both landed),
and `crates-sys/gettext-sys/vendor/*` (upstream gettext sources, not an enumeration). No
`nix/`, `flatpak/`, `dist/`, Makefile or CMake list names catalogs (repo-wide grep).

**Gates / checks**

| check | before | after |
|---|---|---|
| G-I18N (`check_translations.sh`, msgcmp vs pot) | `GREEN: all 32 catalogues` | `GREEN: all 34 catalogues match po/io.airely.rustty.pot.` |
| `msgfmt --check --statistics` fr / ru / uk | `71 translated, 181 untranslated` ×3 | `252 translated messages.` ×3 |
| `msgfmt --check --statistics` sr / sr@latin | (absent) | `252 translated messages.` ×2 |
| `cargo test -p xtask` (incl. `po_dir_matches_declared_locales`) | — | `ok. 46 passed; 0 failed; 2 ignored` + `ok. 10 passed` |

**Negative controls (executed)**
- (a) truncated `sr.po` (first 20,000 bytes): `msgfmt --check` → `end-of-file within string`, exit 1.
- (c) `fr.po` with the `Quit Rustty?` entry removed: `msgcmp --use-untranslated` → `found 1 fatal error`, exit 1 (the gate's FAIL arm).
- (d) truncated catalog through `msgcmp`: exit 1.
- LOCALES tripwire: with `"sr", "sr@latin"` temporarily removed from `LOCALES`,
  `po_dir_matches_declared_locales` → `FAILED … po/*.po and translations::LOCALES are out of lockstep`; restored → passes.
- (b) **Finding, not fixed (scope):** a catalog carrying an EXTRA msgid not in the pot makes
  `msgcmp --use-untranslated` print `warning: this message is not used` but exit **0** — the
  G-I18N header's claim that a removed/renamed msgid "cannot leave a catalogue silently
  carrying a string that no longer exists" is only true for the MISSING direction. Left for W7
  (strengthening the gate = grepping the warning; not weakening anything). Comment counts in
  the gate header updated 32→34.

## 2. `28b5bf905` Noto emoji fonts

Byte-copied from the oracle; `shasum -a 256` equal on both:
`NotoColorEmoji.ttf` `72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b` (was `bf2a8506…`, 9,867,108 → 10,673,480 B),
`NotoEmoji-Regular.ttf` `cd07743a4c93d8da929afdd28c1d368f11da478a1093146a13610081c2a58440` (was `84a38f2f…`, 868,608 → 880,860 B).
The range diff changes no test expectation with the font (`git diff --stat -- src/font` = the two binaries only).

`cargo test -p rustty-font`: before `ok. 81 passed` → after `ok. 81 passed; 0 failed` (rustty-font recompiled; `Compiling rustty-font` seen).

Consumers of `embedded::EMOJI`/`EMOJI_TEXT` are all in `rustty-font-shaper` (grep). `cargo test -p rustty-font-shaper`
with the new font: `130 passed; 1 failed` — `face::freetype::tests::stb_resize_kat_bitmap_strike_bgra`, a
**rustty-only golden** (WS11 KAT; the oracle has no such test) pinning the resized bytes of the U+1F600
136×128 BGRA strike. Re-derived per its own README recipe, with evidence that the INPUT changed, not the resampler:

| measurement (`bindiff.py`) | 16×15 | 8×7 |
|---|---|---|
| pre-resize strike, old font vs new font | 5988/69632 differ, maxdelta=42 | (same strike) |
| OLD fixture vs OLD-font output | 0/960, maxdelta=0 | 0/224, maxdelta=0 |
| OLD fixture vs NEW-font output (the failure) | 67/960 (7.0 %), maxdelta=1 | 14/224 (6.3 %), maxdelta=1 |
| NEW fixture vs consensus `run.sh --expect` | EXACT at host cc -O0/-O1/-O2/-O3/-Os, ghostty Debug/ReleaseSafe/ReleaseSmall; ReleaseFast 0 differ → `run.sh: GREEN` | same, `run.sh: GREEN` |

Temporary env-gated dump hooks were used and removed (`git diff --stat freetype.rs` = 0 lines). After the regold:
`cargo test -p rustty-font-shaper` → `ok. 131 passed; 0 failed; 1 ignored` (incl. `stb_resize_kat_rejects_nearest_and_bilinear`, which re-measures the discrimination margins on the new fixture); D10 `check_stb_resize_parity.sh` → `D10: GREEN`. The grayscale (Terminus) fixtures are untouched. Documented in `crates/rustty-font-shaper/src/testdata/README.stb-resize.md`.
**Negative control:** the old fixture against the new font fails (row 3) — the KAT is not vacuous.

## 3. `094d175ef` iTerm2 themes `20260824-153547-75c93ee`

Fetched `https://deps.files.ghostty.org/ghostty-themes-release-20260824-153547-75c93ee.tgz` (the one permitted network fetch);
observed sha256 `b1fd1f31057f97c1274db03c6ce44960837a1a41f52275cbc64aee2e6e34eb9e` = the `sha256` the oracle records for it in
`flatpak/zig-packages.json` (range diff). The zig hash `N-V-__8AAGZkBACH0haGC9R-hfNPYWxu16hr-ydEsolad9GV` is cited from the oracle,
not reproduced (`zig fetch` not run). Re-vendored with the prescribed `.port/phase-3/w9-revendor-themes.py` (keeps the single R2
filename rename; identity survey of the extracted tarball: basenames match only `Ghostty Default Style Dark`, bodies match nothing):
`added=4 updated=0 removed=0 total_non_md=606` — `Grok Day`, `Grok Night`, `Warm Burnout Dark`, `Warm Burnout Light`.

| gate | before | after |
|---|---|---|
| G-THEMES `check_theme_corpus.py` | `byte-exact (602 files)` | `byte-exact (606 files)` |
| `xtask` `emits_full_theme_corpus` count | 602 | 606 (re-derived from the tarball's file count, not hand-picked) |

**Negative controls (executed):** old manifest vs new corpus → `RED … ADDED Grok Day / Grok Night / Warm Burnout Dark / Warm Burnout Light`, exit 1 (then `--write`, then GREEN);
`emits_full_theme_corpus` with the assertion still at 602 → `FAILED … left: 606 right: 602`, then updated → passes.
Doc counts updated in `res_themes.rs`, `themes/README.md` (provenance block for this hop), `ci.sh`/`check_theme_corpus.py` comments, `update-colorschemes.yml` (604→608 incl. `.md`s).

## 4. `.github`

- `522ebdd0a` milestone-action 3.1→3.2: **PORTED** — both `uses:` pinned to `2f38355153344ccaaa44b5b5fcff9f604dff1b45 # v3.2`, the SHA the oracle's pinact-pinned file carries (`.github/pinact.yml` has no ignore for this action, so the SHA form is the prescribed one). Remaining diff vs oracle is pre-range and deliberate: `runs-on: ubuntu-latest` and no `!contains(title, 'VOUCHED')` (rustty has no VOUCHED process). `prettier --check` clean.
- **Freestanding CI lane (END STATE: `build-libghostty-vt-freestanding` in `test.yml`, matrix `riscv32-freestanding-eabi`/`thumb-freestanding-eabi` cortex_m4, `zig build -Demit-lib-vt -Doptimize=ReleaseSafe`): N-A today — nothing added (a vacuous lane is worse than none).** Honest finding:
  - `rustup target list --installed` has no `*-none*` target. `cargo build -p rustty-vt --target x86_64-unknown-none` → first error `error[E0463]: can't find crate for `std`` in `rustty-util`/`rustty-identity`/`allocator-api2`/`rustty-unicode`/`rustty-lib`/`rustty-os` (7 errors, exit 101); `--target thumbv7em-none-eabi` → same plus `can't find crate for `core`` (12 errors). Full logs were captured (`/tmp/w6/freestanding-*.txt`, not committed).
  - Even with the targets installed the build cannot succeed: neither `rustty-vt` nor `rustty-vt-c` carries `#![no_std]`/`cfg_attr(no_std)`; 64 files in `crates/rustty-vt/src` use `std::`, and its dependency chain (`rustty-os`, `rustty-lib`, `zlib-sys`, `allocator-api2` default features) is std-only. The source-side half (`3376153a4`/`a5423592c`, W1) is a prerequisite; G-XTARGET stays the honest analog (its wasm32 arm is the closest existing freestanding-ish lane). Adding a `none` target to G-XTARGET would only be legitimate once W1's finding says the crate graph can build there.
- `VOUCHED.td` (`20abdb50a`, `e8aa09867`, `6d850fef7`, `98cd670c0`, `7b47213f9`, `ec3e384d2`-adjacent): **N-A**, rustty has none.
- `CODEOWNERS`: **N-A**, see coverage table.

## 5. Zig manifests

`build.zig.zon`, `build.zig.zon.{json,nix,txt}`: N-A (Zig package manifests; the only in-range content is the themes hop, ported as §3).
`flatpak/zig-packages.json`: no rustty analog — `flatpak/` holds `dependencies.yml exceptions.json io.airely.rustty{,-debug}.yml` only; rustty's build reads the checked-in corpus and fetches nothing (per `update-colorschemes.yml`'s header).

## Gate summary (verbatim tails)

```
G-I18N GREEN: all 34 catalogues match po/io.airely.rustty.pot.
G-THEMES: vendored theme corpus byte-exact (606 files)
cargo test -p rustty-font:        test result: ok. 81 passed; 0 failed; 0 ignored
cargo test -p rustty-font-shaper: test result: ok. 131 passed; 0 failed; 1 ignored
cargo test -p xtask:              test result: ok. 46 passed; 0 failed; 2 ignored / ok. 10 passed
D10: GREEN — fixtures exact at every opt level; ghostty Debug/ReleaseSafe/ReleaseSmall byte-exact
G-LINT: shellcheck ok, typos ok, swiftlint ok, prettier FAIL (pre-existing, not W6: docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md from 62c1690d)
```

## Open items (for W7)

1. G-LINT prettier is RED on the coordinator's plan file (`docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md`, committed in `62c1690d`) — not a W6 file; `prettier --write` it on the integration branch.
2. G-I18N's extra-msgid direction is warning-only (§1 (b)) — consider hardening.
3. `crates/rustty-os/src/i18n_locales.rs` needs `"sr", "sr@latin"` (W4, `7aa476714`); xtask `LOCALES` is already extended here and documents itself as that list's mirror.
4. Freestanding lane: revisit after W1's source-side finding; nothing vacuous was added.
