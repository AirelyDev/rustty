# R4 — build, packaging, CI and i18n: upstream delta review

Range `20abdb50a..12752b2ac` (oracle `origin/main`, read without checkout; the oracle tree stays at
the pin). Read-only review: no Rustty file other than this record was changed, nothing was built.
Scope: `build.zig`, `build.zig.zon*`, `src/build/**`, `src/apprt/gtk/build/**` (GTK build-cache
commit), `pkg/{fontconfig,translate-c,wuffs,oniguruma,sentry}/**`, `pkg/opengl/build.zig.zon`, the
uucode dependency bump (Unicode 18), the iTerm2 themes bump, `flatpak/**`, `nix/**`, `flake.*`,
`.github/**`, `po/**`, `vendor/**` (glad EGL is R2's), `CLAUDE.md`, VOUCHED/CODEOWNERS.

Method: `git log --no-merges 20abdb50a..origin/main -- <paths>` gives **124 commits** in scope:
**70 po-only**, **20 governance** (17 `Update VOUCHED list`, 3 `Sync CODEOWNERS vouch list`, all of
which touch only `.github/VOUCHED.td`; `CODEOWNERS` itself is unchanged in the range), **34 other**.
Out-of-tree evidence was fetched to `/tmp` only: the fontconfig 2.18.3 tarball (the URL in the new
`pkg/fontconfig/build.zig.zon`), uucode `9d55524` (the URL in the new `build.zig.zon`), and the
themes tarball `ghostty-themes-release-20260921-150923-0b55a9e.tgz`. The old uucode `2826a37` was
read from `~/.cache/zig/p/uucode-0.2.0-ZZjBPlK5…`.

## Summary

| theme | commits | Rustty status | size | top risk |
|---|---|---|---|---|
| A. fontconfig 2.14.2 → 2.18.3 | `501e7b5c1` (+ build-side of `fe9cf6a26`) | **absent** | **L** | new vendored C + 16k lines of generated headers; ELF alias headers now active on Linux; `ENABLE_FREETYPE=1` required; KAT and Windows-GNU (N286) evidence must be redone |
| B. uucode → Unicode 18 | `12542b392`, `aef7aaeb8`, `7644e6d62`, `7adbb5160` | **absent** | **L** | new `GraphemeBreakNoControl` variant and a restructured `BreakState`; every generated table and FNV checksum changes; two independent uucode ports (rustty-unicode, rustty-vaxis) |
| C. wuffs zlib decoder | `4b86e3594` (pkg half), `42049f1c4` | **absent** | **M** | must land before R1 switches kitty `decompressZlib`; G1b `wuffs` row is an explicit file list that will not see `zlib.zig` |
| D. translations | 70 po commits (10 catalogs) | **absent** (mechanical re-mirror) | S | none; `.pot` unchanged, no new locale |
| E. iTerm2 themes | `11d1cc4fc`, `75166e9eb` | **absent** | S | 606 → 617 files; counts hard-coded in three places |
| F. version string | `5ca09acf6` | **present** (already fixed as R13 UD150) | S | record bookkeeping: UD150 is now fixed upstream; the port's trim set differs slightly |
| G. CI workflows | `380778e3c`, 7 action-bump commits | **partial** | S | upstream dropped `test-windows` and `build-libghostty-windows-gnu`, which Rustty's `windows` job ports |
| H. EGL link + devShell | `758100916`, `c488a41b0` (`fa2ec3a6a`, `cc140d478` are R2's) | **absent**, coupled to R2 | S–M | depends on how R2 binds EGL |
| I. translate-c / cImport transition | `c0c5473da`, `fe9cf6a26`, `2b3b89391` | **N/A** (Zig build mechanics; bindgen) | S (record) | verified: no C compile flag changed |
| J. Zig-package / Nix renderings | `aef7aaeb8`, `57286723d`, `f5efbaee5`, `13a21fc04`, `1a174c063`, `659ca22c0`, `1f3d869b4` | **N/A** (NON_PORTS G13 §6) | S (record) | none |
| K. GTK build cache | `ef7edc4a2` (+ graphene line of `9dc7408be`) | **N/A** as mechanics; doc citations stale | S | none |
| L. `CLAUDE.md` deletion | `9d2d9acac` | **present as symlink by user decision** | — | maintainer question |
| M. governance | 20 VOUCHED/CODEOWNERS-sync commits | **N/A** (NON_PORTS G13 §6) | — | none |

**Total effort: about 6 engineer-days** (A ≈ 2, B ≈ 2.5, C ≈ 0.75, D+E+F+G+records ≈ 0.75),
excluding H, which is sized by R2's EGL decision.

## Per-commit table (all 124, newest first within each group)

### Non-po, non-governance (34)

| commit | subject | theme | Rustty status | size | risk |
|---|---|---|---|---|---|
| `659ca22c0` | update to zon2nix 0.9.0 | J | N/A: Rustty has no zon2nix input (`flake.nix:25`, `nix/devShell.nix:3,10`) | — | none |
| `1f3d869b4` | nix: remove no-longer-necessary linkFarm override | J | N/A: `nix/librustty-vt.nix` never carried the override (crane, `nix/librustty-vt.nix:9-13`) | — | none |
| `1a174c063` | nix: update zon2nix to 0.8.1 | J | N/A | — | none |
| `75166e9eb` | deps: Update iTerm2 color schemes | E | absent (end state `20260921-150923-0b55a9e`) | S | low |
| `42049f1c4` | pkg/wuffs: align zlib decoder storage | C | absent | S | low |
| `4b86e3594` | terminal/kitty: use wuffs for zlib image decompression | C (pkg half; `graphics_image.zig` half is R1's) | absent | M | ordering |
| `5ca09acf6` | build: trim branch name before sanitizing it for the version | F | present (R13 UD150) | S | bookkeeping |
| `9dc7408be` | gtk: snap fractional texture origin to device pixels | K (the `graphene` import line only; the GTK change is the apprt reviewer's) | N/A as build wiring (gtk-rs crates) | — | none |
| `9d2d9acac` | agents: drop CLAUDE.md | L | present as symlink (user decision) | — | question |
| `cc140d478` | opengl: explicitly initialize surfaceless display | H (vendor/glad is R2's) | R2 | — | — |
| `aef7aaeb8` | check-zig-hash --update | J (renders the uucode/libvaxis entries) | N/A | — | none |
| `12542b392` | deps: Update uucode for Unicode 18 | B | absent | L | high |
| `98f2e3e6d` | bump nscloud-cache-action 1.6.1 → 1.7.0 | G | N/A: Rustty uses `actions/cache` on GitHub runners, no nscloud (grep: 0 uses) | — | none |
| `cd8daf9eb` | bump docker/build-push-action 7.3.0 → 7.4.0 | G | N/A: Rustty's `test-debian-13` runs `docker build` directly (`ci.yml:1456-1458`) | — | none |
| `fe9cf6a26` | build: fully transition away from cImport/addTranslateC | I (+ A's `c.zig`, oniguruma, sentry) | N/A (bindgen) | S (record) | none; flags verified |
| `ef7edc4a2` | gtk/build: speed up repeated builds by letting the Zig build cache work | K | N/A as mechanics; `rustty-apprt/build.rs` citations stale | S | none |
| `c0c5473da` | build: refactoring our use of translate-c | I (+ wuffs build) | N/A | S (record) | none |
| `13a21fc04` | nix: use --reflink=auto instead of --link | J | N/A | — | none |
| `f5efbaee5` | nix: use hardlinks | J | N/A | — | none |
| `57286723d` | nix: update zon2nix to 0.7.0 | J | N/A | — | none |
| `7644e6d62` | build/linghostty-vt: fix libvaxis access to uucode tables | B | N/A (Zig comptime workaround; no value change) | S (record) | none |
| `380778e3c` | ci: drop libghostty-internal builds on Windows | G | partial: Rustty still runs both dropped jobs | S | question |
| `758100916` | build: link against libEGL | H | absent, coupled to R2 | S–M | coupling |
| `fa2ec3a6a` | pkg/opengl: add EGL headers and bindings | H (R2 owns `vendor/glad` and `pkg/opengl` source) | R2 | — | — |
| `c488a41b0` | nix: add libglvnd to devShell for EGL | H | absent, follows R2 | S | coupling |
| `7adbb5160` | build/libghostty-vt: export uucode so that it can be re-used | B | N/A (Zig package-graph export) | S (record) | none |
| `2b3b89391` | deps: update translate-c backport | I | N/A | — | none |
| `e9283ba87` | bump c-hive/gha-remove-artifacts 1.4.0 → 1.8.0 | G | absent (`clean-artifacts.yml:13`) | S | none |
| `258c057c0` | bump ryand56/r2-upload-action 1.4 → 1.5 | G | absent (7 sites: `publish-tag.yml:94`, `release-tag.yml:512`, `release-tip.yml:333,413,542,770,796`) | S | none |
| `11d1cc4fc` | deps: Update iTerm2 color schemes | E (intermediate hop `20260831-151010-752a9c0`) | superseded by `75166e9eb` | — | — |
| `7358067d1` | bump cachix/cachix-action | G | N/A: Rustty uses `cachix/install-nix-action` only (3 sites), no cachix-action | — | none |
| `2ba75764f` | bump flatpak-github-actions/flatpak-builder 6.7 → 6.8 | G | N/A: Rustty runs the `flatpak-builder` CLI in the flathub-infra image (`flatpak.yml:24,78,96`) | — | none |
| `501e7b5c1` | pkg/fontconfig: update to 2.18.3 | A | absent | L | high |
| `38c984e67` | bump softprops/action-gh-release 3.0.2 → 3.0.3 | G | absent (5 sites: `release-tip.yml:248,315,395,521,697`) | S | none |

### po-only (70) and governance (20)

Every po row: Rustty status **absent (re-mirror of the END STATE)**, size S, risk low; the port is one
re-mirror per catalog (theme D), not one per commit. Every governance row is N/A under NON_PORTS G13 §6
(`.port/phase-3/NON_PORTS.md:876`).

| commit | subject | theme | Rustty status | size | risk |
|---|---|---|---|---|---|
| `12752b2ac` | Update VOUCHED list (#14445) | M | N/A | — | none |
| `6301810a4` | Update VOUCHED list (#14409) | M | N/A | — | none |
| `c273d6ff6` | Update VOUCHED list (#14387) | M | N/A | — | none |
| `7c40388b2` | Update VOUCHED list (#14373) | M | N/A | — | none |
| `622b4eecd` | Update VOUCHED list (#14367) | M | N/A | — | none |
| `7fb75b3c5` | Update VOUCHED list (#14363) | M | N/A | — | none |
| `bd1c82bc5` | Update VOUCHED list (#14341) | M | N/A | — | none |
| `3c47ca159` | Sync CODEOWNERS vouch list (#14327) | M | N/A | — | none |
| `27e8b3fa8` | Update VOUCHED list (#14320) | M | N/A | — | none |
| `ca9b0384f` | Update VOUCHED list (#14308) | M | N/A | — | none |
| `86f449013` | Update VOUCHED list (#14292) | M | N/A | — | none |
| `87b60655b` | Update the Norwegian translation | D `po/nb.po` | absent | S | low |
| `e842d763c` | Update VOUCHED list (#14290) | M | N/A | — | none |
| `57bdb8c44` | i18n(ru): small fixes | D `po/ru.po` | absent | S | low |
| `494e41374` | i18n(ru): refine Russian translation | D `po/ru.po` | absent | S | low |
| `f9a3f24a5` | Update VOUCHED list (#14256) | M | N/A | — | none |
| `d4c88d806` | Update VOUCHED list (#14241) | M | N/A | — | none |
| `45058d768` | Review feedback | D `po/it.po` | absent | S | low |
| `0c2a290d3` | Sync CODEOWNERS vouch list (#14229) | M | N/A | — | none |
| `55c3e8cc0` | First review feedback | D `po/it.po` | absent | S | low |
| `09a2724c2` | Update VOUCHED list (#14223) | M | N/A | — | none |
| `6a64b1c86` | po/zh_CN: add missing translations | D `po/zh_CN.po` | absent | S | low |
| `4dac203c3` | Finish italian translation for 1.4 | D `po/it.po` | absent | S | low |
| `9bbb9b246` | Update VOUCHED list (#14215) | M | N/A | — | none |
| `6d8f8b9aa` | chore: make event reporting explicit | D `po/hr.po` | absent | S | low |
| `466439bdf` | Update VOUCHED list (#14205) | M | N/A | — | none |
| `69f42c1a3` | Use 'riquadro' instead of 'divisione' for 'Split' | D `po/it.po` | absent | S | low |
| `d08ebcc86` | Keep working on italian translation on 1.4 | D `po/it.po` | absent | S | low |
| `8c17235f8` | Update VOUCHED list (#14195) | M | N/A | — | none |
| `8a3dbc8ce` | i18n(hr): spiffy up the translation | D `po/hr.po` | absent | S | low |
| `68852fc00` | i18n(hr): unify translation | D `po/hr.po` | absent | S | low |
| `d1ed60d56` | Update hr.po | D `po/hr.po` | absent | S | low |
| `c01c683eb` | i18n(hr): "toggle" string translations | D `po/hr.po` | absent | S | low |
| `edf3aa01b` | i18n(hr): added more translations and fixes | D `po/hr.po` | absent | S | low |
| `437dce21b` | Update hr.po | D `po/hr.po` | absent | S | low |
| `1ff027c33` | i18n(hr): started updating hr translation | D `po/hr.po` | absent | S | low |
| `607d59b0e` | i18n: fix Lithuanian grammar and terminology consistency in lt.po | D `po/lt.po` | absent | S | low |
| `ff5b87e4d` | i18n: re-apply remaining lt.po review suggestions | D `po/lt.po` | absent | S | low |
| `97f2ddb06` | Sync CODEOWNERS vouch list (#14163) | M | N/A | — | none |
| `50f757dc8` | i18n(de): adjust floating state description | D `po/de.po` | absent | S | low |
| `752017502` | more fixes | D `po/eu.po` | absent | S | low |
| `9801423d0` | update | D `po/eu.po` | absent | S | low |
| `4da902b3c` | update | D `po/eu.po` | absent | S | low |
| `f184d3ceb` | update | D `po/eu.po` | absent | S | low |
| `63039a688` | update | D `po/eu.po` | absent | S | low |
| `f407316c8` | Update po/eu.po | D `po/eu.po` | absent | S | low |
| `4ffec4bda` | Update po/eu.po | D `po/eu.po` | absent | S | low |
| `4c731b5d8` | Update po/eu.po | D `po/eu.po` | absent | S | low |
| `6a5152651` | Update po/eu.po | D `po/eu.po` | absent | S | low |
| `58aa59d0e` | Update po/eu.po | D `po/eu.po` | absent | S | low |
| `59141ad21` | add eu translation | D `po/eu.po` | absent | S | low |
| `ee10453a2` | fix unclosed quote | D `po/mk.po` | absent | S | low |
| `36015c99d` | updated revision date | D `po/mk.po` | absent | S | low |
| `807a51e3e` | updated localization file | D `po/mk.po` | absent | S | low |
| `e3849b7a6` | i18n(de): update German translation | D `po/de.po` | absent | S | low |
| `5e6167072` | i18n(de): update German translation | D `po/de.po` | absent | S | low |
| `654228866` | Start Italian translation for 1.4 | D `po/it.po` | absent | S | low |
| `4cc8fa335` | i18n(de): Fix accidental deletion | D `po/de.po` | absent | S | low |
| `fee8cb523` | i18n(de): Wording plain text | D `po/de.po` | absent | S | low |
| `159be4d3c` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `05dd3c53a` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `85508ca2c` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `56b739cc3` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `7d6c090a7` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `7a8221113` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `18d323d24` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `6633869d8` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `d405da445` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `7d355b8a0` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `ab26bb4b2` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `4df91b3fc` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `a44c47477` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `58dd6d777` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `1779cabf4` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `eba2ec988` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `dc2d6f1b3` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `57b312293` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `b3f4dbb37` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `0f03d156c` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `e9693a4a2` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `f8304c01f` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `bd4226741` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `167ed5788` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `3e0865422` | Update po/lt.po | D `po/lt.po` | absent | S | low |
| `d13d43995` | update | D `po/vi.po` | absent | S | low |
| `53ae1dd8b` | update translation | D `po/vi.po` | absent | S | low |
| `4c7496c03` | i18n: update Lithuanian translation for v1.4 | D `po/lt.po` | absent | S | low |
| `8b5678bd1` | i18n(de): German translation improvements | D `po/de.po` | absent | S | low |
| `ba8690c42` | i18n(de): Typo fix | D `po/de.po` | absent | S | low |
| `1266c0a32` | i18n: update `de_DE` translations | D `po/de.po` | absent | S | low |

## A. fontconfig 2.14.2 → 2.18.3 (`501e7b5c1`, build half of `fe9cf6a26`)

**Upstream end state.**
- Source: `pkg/fontconfig/build.zig.zon` now fetches
  `https://gitlab.freedesktop.org/api/v4/projects/890/packages/generic/fontconfig/2.18.3/fontconfig-2.18.3.tar.xz`
  (was the `deps.files.ghostty.org` 2.14.2 mirror). The zon `.version` field still reads `"2.14.2"`.
- The 2.18.3 tarball no longer ships the generated headers the 2.14.2 one did (`fc-case/` has only
  `fccase.tmpl.h`, `fc-lang/` only `fclang.tmpl.h`, `src/` only `fcstdint.h.in`,
  `fontconfig/` only `fontconfig.h.in`). Upstream hand-extracted them from an autogen run into
  `pkg/fontconfig/override/`: `fc-case/fccase.h` (354), `fc-const/fcconst.h` (700, new file
  `fcconst.h` included by `fcname.c:139`), `fc-genericfamily/fcgenericfamily.h` (6,963, included by
  the new `fcgenericalias.c:22`), `fc-lang/fclang.h` (5,586), `fontconfig/fontconfig.h` (1,245,
  `FC_MAJOR 2 / FC_MINOR 18 / FC_REVISION 3`, `FC_CACHE_VERSION_NUMBER 12`), and `src/`:
  `fcalias.h` (472), `fcaliastail.h` (528), `fcftalias.h` (16), `fcftaliastail.h` (24), `fcint.h`
  (1,752), `fcobjshash.h` (moved from `include/`, regenerated), `fcstdint.h` (9). The four EMPTY
  `override/include/*alias*.h` stubs are deleted. Build notes for regenerating these:
  `pkg/fontconfig/build.zig:6-46`.
- `override/src/fcint.h` is byte-identical to the tarball's `src/fcint.h` (`cmp` on the fetched
  tarball), so it is redundant rather than generated.
- Include path (`build.zig:131-144`): `override`, `override/fc-const`, `override/fc-genericfamily`,
  `override/fc-lang`, `override/src`, then the upstream root. `override/fc-case` is deliberately not
  listed: `fcstr.c:186` does `#include "../fc-case/fccase.h"`, which misses in the tarball's own
  `fc-case/` and resolves through `override/src/../fc-case/fccase.h`. A port must keep an
  `override/<subdir>` on the path for that `..` lookup to work.
- New defines: `-DFC_VERSION_{MAJOR,MINOR,MICRO}` from the zon version (`build.zig:148-153`),
  `-DHAVE_VSNPRINTF` (`:169`, selects the `FcStrDupVapFormat` arm at `fcint.h:1031`),
  `-DHAVE_USELOCALE` off-Windows (`:237`, the only arm of `FcLocaleSetCurrent`, `fccompat.c:375-385`,
  that does not `#error`), `-DENABLE_FREETYPE=1` when FreeType is enabled (`:271`; "value-tested
  (#if/#elif) by upstream so it must be =1": `fcdir.c:35,91`, `fclang.c:27,1062` compile the FreeType
  paths only under it).
- New TU `src/fcgenericalias.c` (`build.zig:355`); `lib.installHeader(override/fontconfig/fontconfig.h)`
  (`:333`); `c.zig` goes through translate-c (N/A for Rust).
- Windows arm unchanged (`HAVE_UNISTD_H` is still unconditional, so the MSVC failure N286 measured
  still applies by construction).

**Facts found while reviewing (not in upstream's commit message).**
1. The `FC_VERSION_*` defines carry `2.14.2` (stale zon `.version`), but no 2.18.3 source reads them:
   they appear only in `config.h.in:50` and `configure.ac:75-77` / `meson.build:45-47`, and every
   `#include <config.h>` is `HAVE_CONFIG_H`-guarded. `FcGetVersion()` returns `FC_VERSION` from
   `fontconfig.h` (`fcinit.c:92-95`), i.e. 21803. Dead, so a latent record at most.
2. The reference's `srcs` list omits `fcconffile.c`, which meson compiles (`src/meson.build:25`);
   it defines the public `FcConfigFileGenerate` and `FcConfigFileGenerateGenericAlias`, which no other
   TU calls. So the reference's static archive leaves that public API undefined. Faithful to port;
   a candidate latent UD.
3. With real `fcalias.h`/`fcaliastail.h`, `fcint.h:131-134` now activates the IA__ hidden-alias
   scheme on ELF targets (`__ELF__` and GCC ≥ 3.3): Linux and FreeBSD static archives change symbol
   shape (hidden `IA__Fc*` plus `alias(...)` exports). Darwin and Windows are unaffected (not ELF). At
   the pin the stubs were empty, so no aliasing happened anywhere.

**Rustty counterpart.** `crates-sys/fontconfig-sys`: vendored 2.14.2 under `vendor/` (tarball files
plus `vendor/override/include/` with the 4 empty stubs and `fcobjshash.h`); `build.rs:55-84`
`SOURCES` (28 TUs), `:90` `override/include` first on the path (`:122`), common defines `:128-148`,
Windows arm `:163-176` (N286), off-Windows `:177-213`, libxml2 `:230-243`, `-std=gnu17` `:250`,
bindgen `-I` list `:275-279`; KAT `src/lib.rs:77-85` asserts `FcGetVersion() == 21402`. Version text
also in `Cargo.toml:4,14`, `wrapper.h:10`, `src/lib.rs:1,6,58-85`, `crates/rustty-font-shaper/Cargo.toml:105,197`,
`crates-sys/libxml2-sys/build.rs:140`.

**Status: absent. Size L.** Work items:
1. Replace `vendor/` with the 2.18.3 tarball subset the 29 TUs need (`src/*.c|h`,
   `fontconfig/*.h`) plus `COPYING`; copy the 13 reference override headers verbatim into
   `vendor/override/` preserving the reference's directory structure; delete
   `vendor/override/include/`.
2. `build.rs`: the new include order (and a comment on the `../fc-case` lookup), `fcgenericalias.c`,
   `HAVE_VSNPRINTF`, `HAVE_USELOCALE` (off-Windows), `ENABLE_FREETYPE=1`, the three `FC_VERSION_*`
   with the reference's literal `2`/`14`/`2` (R1; see question Q4). Bindgen must parse
   `override/fontconfig/fontconfig.h`, not the missing tarball header.
3. KAT: 21402 → 21803, rename `kat_fontconfig_2_14_2_*`, update every "2.14.2" doc line listed above.
4. Verify on all three hosts: macOS (`cargo test -p fontconfig-sys`, the G-FONTBACKEND
   `fontconfig_freetype` arm), Linux `rvm` (the ELF alias change only compiles there; check `nm`
   of the archive and the font-shaper `fc` tests), Windows GNU on `rwin` (redo N286's W-6 evidence:
   2.18.3 adds `FcLocaleSetCurrent`/`_create_locale` paths and the reference keeps
   `-Werror=implicit-function-declaration` on Windows, `build.zig:291-297`). Re-measure the
   reference on GNU Windows before deciding parity, as W-6 did.
5. Behaviour: 2.18 adds built-in generic-family aliasing (`fcgenericalias.c`,
   `fc-genericfamily/*.txt`) and cache version 8 → 12. Font-matching test expectations in
   `rustty-font-shaper` (`fc::tests`, `discovery::tests::fontconfig`, `fontconfig_codepoint`,
   `deferred_face::tests::fontconfig`) may move on Linux; the reference's own `pkg/fontconfig` tests
   only assert `version() > 0` (`pkg/fontconfig/init.zig:25-27`).

**Risks.** Largest vendored-C change in the range (+18k lines); the `ENABLE_FREETYPE=1` define is
easy to miss and its absence compiles cleanly while silently dropping FreeType font scanning;
Linux-only symbol-shape change (run the Linux lane early); Windows-GNU parity must be re-proven on
`rwin` (disk-tight VM, one clone at a time).

## B. uucode 2826a37 → 9d55524 (Unicode 17 → 18)

**Upstream end state.** `build.zig.zon` uucode URL/hash (`12542b392`); `src/unicode/grapheme.zig`
precompute loop now iterates `BreakState.table_len` and builds states with `.fromTableIndex`
(`grapheme.zig:171-176`, quota 10k → 20k); `src/build/uucode_config.zig:51-74` sets the `buildtime`
table `.packing = .unpacked` (`7644e6d62`, a Zig 0.16 comptime workaround; values unchanged);
`SharedDeps.zig:84` exports the uucode module for Zig embedders (`7adbb5160`).

**What changes in uucode itself** (diff of the two trees):
- Types: `GraphemeBreakNoControl` (and `GraphemeBreak`) rename `indic_conjunct_break_linker` →
  `indic_conjunct_break_linker_extend` and gain `indic_conjunct_break_linker_other` (InCB=Linker with
  GCB=Other: U+1CF5, U+1CF6, U+11A3A). `components.zig` derives the split.
- Algorithm: `BreakState` goes from `enum(u3)` {default, regional_indicator, extended_pictographic,
  indic_conjunct_break_consonant, indic_conjunct_break_linker} to `packed struct(u3)
  { after_linker: bool, base: enum(u2){default, extended_pictographic, regional_indicator} }`,
  `table_len = 5`; GB9c is now tracked as a flag alongside the base so it can overlap GB11. `.default`
  survives as a deprecated alias. The 8,192-entry precompute (`Key` = state u3 + gb1 u5 + gb2 u5)
  keeps its size but its state encoding and contents change.
- Data (UCD 17.0.0 → 18.0.0): `UnicodeData.txt` 40,575 → 41,341 lines; `DerivedEastAsianWidth`
  103 changed lines, new W ranges including Jurchen `18E00..19191`, `191A0..191D2`, Small Seal
  `3D000..3FC3F`, emoji `1F6D9`, `1FA8B..1FA8D`, `1FACC`, `1FADD`, `1FAEB`, `1FAF9..1FAFA`;
  `GraphemeBreakProperty` 24 changed lines (Extend growth: `05C8..05C9`, `0B53..0B54`, `1AEC..1AF0`,
  `10ECB..10ECF`, `10EF0..10EF9`, `11DF0`, musical-symbol combiners); `emoji-data.txt` 9 new
  `Emoji_Presentation` points (E18.0) and matching `Extended_Pictographic` reshapes;
  `CaseFolding.txt` 22 changed lines (e.g. `A7DD`, `A7E2`, `AB6C`, `AB6D`, `1DF40`);
  `emoji-variation-sequences.txt` no data change.

**Rustty counterparts** (grep of the whole workspace):
- `crates/rustty-unicode`: `props.rs:15-36` (`GraphemeBreakNoControl`, `IndicConjunctBreakLinker = 15`),
  `grapheme.rs:12-28` (`BreakState` enum), `generated/{props,symbols,grapheme,case_folding}_data.rs`
  (headers cite uucode 2826a37 at pin 20abdb50a), `generated/checksum.rs` (FNV pins, e.g. `:44`,
  `:58`), `props_uucode.rs:268-282` spot-checks naming `IndicConjunctBreakLinker`, `lib.rs:8-13`.
- G4-uni harness `.port/phase-0/harness/unicode/`: `build.zig.zon:24-26` pins the old URL/hash,
  `uucode_config.zig` is a byte copy of the pin's (verified `cmp` equal), `zig-pkg/` holds the old
  package; zig 0.16.0 is installed on the Mac.
- `crates/rustty-vaxis`: a second, Zig-free port of uucode — `build.rs:1-20` (follows 2826a37's
  `components.zig` over the vendored `ucd/` 17.0.0 corpus, `ucd/PROVENANCE.md`),
  `src/uucode/grapheme.rs:124-141,165-290` (old `BreakState` and GB9c state machine),
  `tests/ucd_provenance.rs`.
- `crates/rustty-font-shaper/src/uucode.rs:17-30,338,376,399` (a hand-reproduced Unicode 17
  `Emoji_Presentation` table with a point-count test).
- `BreakState` users outside the two ports: `rustty-vt/src/terminal.rs`,
  `rustty-font-shaper/src/shaper/web_canvas.rs`, `rustty-bench/src/grapheme_break.rs` (5 uses).
- `rustty-input/src/binding.rs:3390-3410` (case folding, S65-F1) and
  `generated/case_folding_data.rs` (CaseFolding changes).
- Not affected: `rustty-simd/src/codepoint_width.rs` mirrors `src/simd/codepoint_width.cpp`, whose
  hard-coded tables upstream did not touch.

**Status: absent. Size L.** Work items, in order:
1. Harness first: bump `build.zig.zon`, re-copy `uucode_config.zig` from the new pin, refresh
   `zig-pkg/`, and update `uucode_oracle.zig` for the new `BreakState` API.
2. rustty-unicode: add the variant (discriminants must follow the new Zig enum order), port the new
   `BreakState` as a 3-bit packed value with `table_index`/`from_table_index`, port the new
   `isBreak` state machine, regenerate the four data files from the harness, re-pin the FNV
   checksums in the same commit (`checksum.rs:1-15` says so), update the spot-checks.
3. rustty-vaxis: re-vendor `ucd/` from 9d55524, port the `components.zig` split and the new
   grapheme algorithm, update `PROVENANCE.md` and `tests/ucd_provenance.rs`.
4. font-shaper emoji table (+9 points) and its two count/addition tests; case-folding table.
5. G4-uni GREEN (0 diffs full range), rustty-vaxis S-10b-style differential, `cargo test -p
   rustty-unicode -p rustty-vaxis -p rustty-vt -p rustty-font-shaper`, then a grapheme-break
   perf check (`rustty-bench grapheme_break`; the hot path in `terminal.rs` changes shape — see
   memory `comptime-flattening-perf-hazard`).
6. `7adbb5160` and `7644e6d62`: records only (Zig package export; Zig comptime layout).

**Risks.** Every generated table changes (expected golden churn: 4 data files, 3–4 checksums,
vaxis tables, the emoji table); two independent algorithm ports must move in lockstep; the
per-codepoint differential is the only proof and needs zig. Upstream changed no test expectation in
`src/unicode` beyond the precompute loop, so any R1 terminal-width tests that move are Rustty-side.

## C. wuffs zlib decoder (`4b86e3594` pkg half, `42049f1c4`)

**Upstream.** New `pkg/wuffs/src/zlib.zig`: `decode(alloc, data, limit, size_hint)` over
`wuffs_zlib__decoder` with a growing output buffer, `error.Overflow` at `limit`, `WuffsError` on
trailing input or checksum failure; decoder storage `alignedAlloc(u8, .@"16", …)` (`zlib.zig:22`);
5 tests (`:73,83,91,102,124`). `main.zig` exports it. `pkg/wuffs/build.zig` (c0c5473da) only moves to
the shared translate-c helper: defines and C flags unchanged (verified: identical flag literals at
pin and head); `WUFFS_CONFIG__MODULE__ZLIB` was already enabled at the pin.

**Rustty.** `crates/rustty-wuffs/src/lib.rs` has `png`, `jpeg`, `swizzle`, `error` (no `zlib`);
`crates-sys/wuffs-sys/build.rs:60-71` already enables the zlib module and bindgen allowlists
`wuffs_.*`/`sizeof__wuffs_.*` (`:175-183`), so the zlib entry points should already be bound; inline
helpers (`wuffs_base__empty_slice_u8`, `wuffs_base__status__is_ok`) may need shim functions like the
existing `rustty_wuffs_*` ones (`shim.h:27-57`). `png.rs:49-51` already backs the opaque decoder with
8-aligned storage. The kitty consumer today is zlib-sys (`rustty-vt/Cargo.toml:259-269`,
`graphics_image.rs:28-31,1195,1209`), plus a WASM `Unsupported` stub.

**Status: absent. Size M.** Add `rustty-wuffs/src/zlib.rs` (16-aligned decoder storage, the 5 tests
by name), extend the G1b `wuffs` row (`.port/tools/gates/check_test_counts.py:244`, an explicit file
list that will not see `zlib.zig`, the same trap Phase 6 hit with `search/terminal.zig`).

**Ordering.** Before R1 ports `graphics_image.zig`'s `decompressZlib` switch. That switch also
decides whether `zlib-sys` leaves `rustty-vt`, and it removes the premise of the WASM divergence
(the reference's zlib path is now wuffs, which the reference builds for wasm32 with minimal libc
headers; `wuffs-sys/build.rs:37-40` says Rustty never compiled wuffs for wasm). Question Q6.

**Upstream note.** Upstream aligned only the zlib decoder; `png.zig:21` and `jpeg.zig:21` still use
`alloc.alloc(u8, …)`. Rustty already over-aligns png (`png.rs:49-51`). A latent UD candidate.

## D. Translations (70 commits, 10 catalogs)

Catalogs: `de` (8 commits), `eu` (11), `hr` (8), `it` (6), `lt` (28), `mk` (3), `nb` (1), `ru` (2),
`vi` (2), `zh_CN` (1). `eu.po` already existed at the pin; "add eu translation" (`59141ad21`) rewrites
it. The `.pot` is unchanged in the range (`git diff --stat` empty) and has 253 `msgid` lines; no
new locale, so `xtask/src/translations.rs:66` `LOCALES` and `rustty-os` `i18n_locales.rs` need no
change.

Measured with the Phase-6 identity transform (`.port/phase-6/assessments/W6-platform-tail.md` §1):
all 10 Rustty catalogs are exactly `xf(oracle@pin)` today (0 diff lines each); against
`xf(oracle@head)` they differ by de 388, eu 358, hr 366, it 378, lt 369, mk 383, nb 22, ru 14,
vi 371, zh_CN 15 lines. `msgfmt --statistics` at head: all ten report `252 translated messages`;
seven of them (de, eu, hr, it, lt, mk, vi) were `71 translated, 181 untranslated` at the pin. New
`msgstr` lines containing "Ghostty" (e.g. `de`: `msgstr "Ghostty"`, `"Einen kleinen Ghostty in dein
Terminal setzen."`) are rebranded by the transform, as before.

**Status: absent. Size S.** Re-mirror the ten END-STATE catalogs through the transform, prove 0 diff
lines against `xf(oracle@head)`, G-I18N GREEN (34 catalogues), `msgfmt --check` on each.

## E. iTerm2 themes (`11d1cc4fc`, `75166e9eb`)

End state `ghostty-themes-release-20260921-150923-0b55a9e.tgz` (`build.zig.zon`; the intermediate
`20260831-151010-752a9c0` is superseded). Fetched and compared with `crates/rustty-extra/themes/`:
617 theme files (was 606); **11 added** — Catthode, Earl Grey, Isocon Dark, Isocon Light, Lemon
Graphite, Nachtschicht, SilkCircuit Dawn/Glow/Neon/Soft/Vibrant; 0 removed; 0 existing files changed
(byte compare, `Ghostty Default Style Dark` ↔ `Rustty Default Style Dark` aside); none of the new
files contains "ghostty".

**Status: absent. Size S.** Re-vendor via `xtask/src/res_themes.rs`, regenerate
`themes.manifest` (`check_theme_corpus.py --write`), update the hard-coded 606 in
`res_themes.rs:110-115`, `.port/tools/ci.sh:129,1044`, and `crates/rustty-extra/themes/README.md:12,80`.

## F. Version string (`5ca09acf6`)

Upstream now trims before sanitizing (`GitVersion.zig:34-43`), and trims the short hash with
`std.mem.trim(…, std.ascii.whitespace)` (`:56`). Rustty fixed the same defect as R13 **UD150**
(`crates/rustty-version/src/git_version.rs:88-99`, `docs/ghostty-bug-fixes.md:26`,
`.port/parity-closure/UPSTREAM-DEFECTS.md:1752`). Remaining differences: Rustty trims only
`['\r', '\n', ' ']` from the end (`git_version.rs:65-67`, used for branch and short hash), upstream
trims all ASCII whitespace (adds `\t`, VT, FF) from both ends for the hash, and for the branch
keeps `tmp[0..trim(tmp).len]` (end-trim in effect; git output has no leading whitespace).

**Status: present (behaviour), records stale. Size S.** Align the trim set to `std.ascii.whitespace`,
retire the `PORT NOTE: reference defect (R13)` in favour of a plain port note, set UD150's
`upstream status` to `fixed upstream 5ca09acf6` (field format at `UPSTREAM-DEFECTS.md:39`), and
remove or annotate the `docs/ghostty-bug-fixes.md:26` row, since Rustty no longer differs.

## G. CI workflows

- `380778e3c` removes `test-windows` (`zig build -Dapp-runtime=none test`) and
  `build-libghostty-windows-gnu` from `test.yml` and from `required`. Rustty ports both inside its
  `windows` job (`.github/workflows/ci.yml:517-531` header; "Test the workspace" `:622`, "Build the
  apprt-c library for the GNU ABI" `:683`, the latter never run, "UNVERIFIED (review W-2 F8)"), and
  the `windows` job is in `required.needs`. N376 lists both as covered. The other Windows jobs
  (`build-libghostty-vt-windows`, `build-examples-cmake-windows`) are unchanged upstream.
- Action bumps that apply (pinact-style SHA + comment): `ryand56/r2-upload-action` v1.4 → v1.5
  `33ebb7a4…` (7 sites), `softprops/action-gh-release` v3.0.2 → v3.0.3 `efb35369…` (5 sites),
  `c-hive/gha-remove-artifacts` v1.4.0 → v1.8.0 `62c2fbea…` (1 site). The release lanes stay inert
  behind `RUSTTY_RELEASE_ENABLED` (N378), so these are unverified by running (V25).
- Bumps with no Rustty counterpart: `nscloud-cache-action` (56 sites upstream), `cachix-action`
  (55), `docker/build-push-action`, `flatpak-github-actions/flatpak-builder` (see table).
- The `!CLAUDE.md` tip-paths filter is still in upstream `release-tip.yml:84` after the file's
  deletion; Rustty mirrors it (`release-tip.yml:121`).

**Status: partial. Size S.** Apply the three bumps; record the four N/A bumps; decide Q2; update
N376's "Covered" list either way.

## H. EGL link (`758100916`) and devShell `libglvnd` (`c488a41b0`)

Upstream compiles `vendor/glad/src/glad_egl.c` and links system `egl` for the GTK runtime
(`SharedDeps.zig:664-672`), and adds `libglvnd` to `nix/devShell.nix`. Rustty's OpenGL layer uses
the `gl` crate instead of vendored glad (`crates/rustty-renderer/src/opengl/gl.rs:2-8,22-23`); no
Rustty build script links EGL or compiles glad (grep: none). Whatever R2 chooses for `egl.zig`
(vendor glad EGL via `cc`, or a binding crate) decides the `rustty-apprt`/`rustty-renderer`
`build.rs` link line and whether `nix/devShell.nix` gains `libglvnd`. **Absent, coupled to R2.**

## I. translate-c / cImport transition (`c0c5473da`, `fe9cf6a26`, `2b3b89391`)

Replaces `@cImport`/`b.addTranslateC` with a local `pkg/translate-c` helper everywhere (50 files in
`fe9cf6a26`). Rust uses `bindgen` per `-sys` crate, so the mechanism is N/A (precedent NON_PORTS
§E26, `.port/phase-3/NON_PORTS.md:109`). Checked that no C compile flag moved: the set of
`-D/-W/-f/-I/-s` literals in `pkg/{harfbuzz,freetype,glslang,spirv-cross,dcimgui,gtk4-layer-shell,
macos,oniguruma,sentry,wuffs,opengl}/build.zig` is identical at pin and head except `-fblocks`, which
at the pin was a translate-c argument only (`pkg/harfbuzz/build.zig:303` in the translator's
`extra_args`; `pkg/macos/build.zig`); harfbuzz's library compile flags are unchanged
(head `build.zig:179-220`). `default_init = true` (dcimgui, freetype, glslang) affects Zig's
translated struct defaults only. `pkg/oniguruma`, `pkg/sentry`: translate-c only. **N/A; record.**

## J. Zig-package and Nix renderings

`build.zig.zon.{json,nix,txt}`, `flatpak/zig-packages.json`, zon2nix bumps (0.7.0 → 0.9.0, pinned by
tag in `flake.nix`), the `linkFarm` removal in `nix/libghostty-vt.nix`: N/A under NON_PORTS G13 §6
(`.port/phase-3/NON_PORTS.md:857-862`; crane vendors from `Cargo.lock`). The only payloads carried by
these files are the uucode, fontconfig, translate-c and themes URLs, covered by A, B, E.

## K. GTK build cache (`ef7edc4a2`)

Zig build-cache mechanics: relative `.ui` paths under `--sourcedir`, blueprint-compiler invoked
directly with a one-shot version check against the highest `{major}.{minor}` across blueprints
(equivalent to the per-blueprint check), memoised gresource pipeline. Rustty's `rustty-apprt/build.rs`
compiles blueprints itself with per-blueprint adw checks (`:285-316`) and cites the old
`blueprint.zig`/`gresource.zig` line ranges and module docs (`:6-40`, `:199-206`, `:293-296`); the
generated bundle aliases are unchanged. **N/A as behaviour; refresh the citations and the quoted
module docs (G3b will see the new `genUi` doc comment).** The `graphene` import added by `9dc7408be`
is the GTK reviewer's.

## L. `CLAUDE.md` (`9d2d9acac`)

Upstream deleted the `CLAUDE.md -> AGENTS.md` symlink. Rustty carries the same symlink by user
decision (`.port/phase-3/NON_PORTS.md:875`, P-1b 2026-09-27), and this repository's agent tooling
loads it. Maintainer question Q1.

## M. Governance

20 commits touching only `.github/VOUCHED.td`; not carried (NON_PORTS G13 §6, `:876`). `CODEOWNERS`
has no diff in the range.

## Proposed workstreams

| WS | content | depends on | size |
|---|---|---|---|
| R4-0 | pin advance bookkeeping for this area: extend G1b `wuffs` row with `zlib.zig` (before porting, so it reads RED honestly) | pin advance | S |
| R4-B | Unicode 18: harness → rustty-unicode → rustty-vaxis → font-shaper/case folding → G4-uni + perf | R4-0; **before** R1's terminal width/grapheme tests and any `terminal.rs` print-path work | L (~2.5 d) |
| R4-A | fontconfig 2.18.3 vendoring + build.rs + KAT; macOS, then Linux `rvm` early, then Windows GNU on `rwin` | none (independent) | L (~2 d) |
| R4-C | `rustty-wuffs` zlib + tests | R4-0; **before** R1's `decompressZlib` | M (~0.75 d) |
| R4-D/E | po re-mirror; themes re-vendor + manifest + counts | none | S (~0.25 d) |
| R4-F | GitVersion alignment + UD150 retirement | none | S |
| R4-G | CI action bumps, N/A records, Q2 outcome, N376/N378 refresh | Q2 | S |
| R4-H | EGL link + `libglvnd` | R2's EGL binding decision | S–M |
| R4-R | records: NON_PORTS entries for I, J, K, M and `7adbb5160`/`7644e6d62`; UD candidates (fontconfig stale version, omitted `fcconffile.c`, wuffs png/jpeg alignment) | none | S |

Parallelism: R4-A, R4-B and R4-C touch disjoint crates and can run as three worktrees (the
disk-budget memory caps building worktrees at three). R4-A's Linux and Windows lanes and R4-B's
G4-uni are the long poles.

## Open questions for the maintainer

- **Q1.** `CLAUDE.md`: upstream deleted the symlink (`9d2d9acac`). Keep Rustty's (it is how this
  repository's agent instructions load; P-1b user decision), or delete it for parity?
- **Q2.** Upstream stopped testing the internal library on Windows (`380778e3c`). Rustty's `windows`
  job still runs the full workspace test and an apprt-c GNU build (never run). Drop both for parity,
  or keep them as surplus coverage and record the divergence in N376?
- **Q3.** fontconfig overrides: copy the reference's 13 hand-extracted override headers verbatim
  (R3-compatible; they are fontconfig-generated MIT files), or regenerate them with fontconfig's own
  autogen/meson as `pkg/fontconfig/build.zig:6-46` describes? Copying keeps parity by construction.
- **Q4.** The reference passes `FC_VERSION_*` = 2.14.2 (stale zon version) and omits `fcconffile.c`.
  Port both verbatim (R1; both are dead or unreferenced) and file latent UDs, or correct them?
- **Q5.** The IA__ alias scheme turns on for Linux/FreeBSD with 2.18.3. Faithful, but it changes the
  static archive's symbol shape on those targets; confirm no Linux packaging check (`nm` strip checks,
  N422) is expected to hold unchanged.
- **Q6.** Kitty zlib on wasm: with the reference's decoder now wuffs, should Rustty build `wuffs-sys`
  for wasm32 and drop the `decompressZlib` `Unsupported` stub (and `zlib-sys` from `rustty-vt`)? Shared
  with R1.
- **Q7.** `7adbb5160` exports uucode for Zig embedders of libghostty-vt. Rustty has no uucode crate
  (R3); `rustty-unicode` is already an ordinary crate. Record as N/A?
- **Q8.** `passthrough.glsl` appeared at the repository root in `0fca3d34a` (a GTK imgui commit),
  apparently by accident. Out of this area; which reviewer records it?
