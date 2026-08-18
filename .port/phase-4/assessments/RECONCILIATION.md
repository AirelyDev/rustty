# Phase 4 — coverage reconciliation (critic pass)

Range: `6ad1fe7d8..12967b68f` (oracle at the new pin). Inputs: the 7 area assessments
in this directory + `UPSTREAM_DIFF_STAT.txt` (381 changed files). rustty `main`
@ `8120dd10`. Critic: coverage-reconciliation agent, 2026-08-17. No code or other
assessment was modified.

## 1. File → area coverage map

Every one of the 381 files in `UPSTREAM_DIFF_STAT.txt` was mapped to the assessment
that verdicts it (by the areas' declared file-sets and per-commit tables). Result:
**366 files are covered by exactly the expected area(s); 15 files are orphans**
(§2). Directory-family map (counts from the diff stat):

| paths | files | area |
|---|---:|---|
| `src/terminal/{PageList,Terminal,Screen,page,style,Tabstops,ScreenSet,StringMap,ref_counted_set,bitmap_allocator,hash_map,fraction,stream,stream_continuation,stream_terminal,main,render,Parser,point,cursor}.zig`, `src/fastprint.zig` | 21 | A |
| `src/terminal/snapshot/**` (40), `src/crc32c.zig`, `src/lib/TinyIo.zig`, `src/benchmark/**` (7), `src/synthetic/**` (3) | 52 | B (benchmark/main.zig shared with G) |
| `src/lib/**` (rest, 8), `src/lib_vt.zig`, `include/**` (15), `src/terminal/c/**` (15), `example/**` (10), `test/wasm-alloc.mjs`, `src/global.zig` (shared w/ G), `src/main_ghostty.zig` | 52 | C |
| `src/terminal/kitty/**` (5), `src/terminal/{apc.zig,apc/**}` (4), `src/terminal/osc/parsers/**` (3 incl. osc/ dir files), `formatter.zig`, `modes.zig`, `color.zig`, `dcs.zig`, `hyperlink.zig`, `src/input/**` (3) | 20 | D |
| `src/os/**` (10), `src/termio/**` (7), `src/cli/**` (10), `src/{App,Surface}.zig`, `src/crash/**` (2), `src/terminfo/**` (3), `src/renderer/**` (6) | 40 | E |
| `macos/**` (70), `src/apprt/**` (23), `src/font/**` (6), `src/inspector/**` (3), `pkg/**` (7), `src/build/**` (6), `flatpak/**` (2), `snap/snapcraft.yaml` | 118 | F |
| `src/config/**` (6), `src/datastruct/**` (8, two new files shared w/ C), `po/**` (35), `.github/**` (9), `typos.toml`, `PACKAGING.md`, `src/simd/vt.zig` | 61 | G |
| orphans (§2) | 15 | — |

(Sub-counts include the shared files once per primary owner; shared files are
enumerated in §3.)

## 2. Orphans — files no assessment verdicts, each assigned + assessed here

Assessed by this critic against the oracle hunks (`git show`) and the rustty tree
(greps/reads cited). None of these change any area's headline numbers materially,
but two are **real porting debt that fell between areas** (osc.zig cap, themes pin).

| # | file | range commit(s) | assigned to | critic verdict + evidence |
|---|---|---|---|---|
| O1 | `src/terminal/osc.zig` (±83) | `727b8a02f` | **A** (bundle with A's page-cap half of the same commit; D's 2 parser call sites follow) | **ABSENT — the 8 MiB OSC `Capture.max_allocating_bytes` cap, the substantive half of a §1.2-severity DoS fix.** A's set omits osc.zig; D explicitly excludes it ("osc.zig owner" named as if a third party — nobody was). Verified: `grep max_allocating crates/rustty-vt/src/osc/mod.rs` → 0 (matches D's own side-evidence). The capture-cap tests ride with it. |
| O2 | `src/terminal/ansi.zig` (13) | `b867a0f59` | A | **N-A.** `ProtectedMode` plain enum → `lib.Enum(lib.target,…)`. Same doctrine as A/D's b867a0f59 N-A rows: rustty's plain enum is behavior-identical (verified `ansi.rs:204` `Off/Iso/Dec`); the C-ABI registry arm is area C's bundle T. |
| O3 | `src/terminal/sgr.zig` (13) | A call-site / C machinery | **ABSENT, blocked on C bundle T.** `Attribute.C = lib.CUnion(…)` second arg becomes an options struct with 7 `field_renames` (`@"256_fg"` → `"fg_256"` etc.) — the exact shape A verdicted for point.zig (`9673a22b0`); this hunk was simply not in A's declared set. No standalone action until C ports `lib.c_abi` options. |
| O4 | `src/terminal/build_options.zig` (+254) | **C** (with its #42 `-Dvt-features` row; F B10 owns build plumbing) | **ABSENT.** A explicitly disclaimed ("flag machinery is another area"); C's #42 covers lib_vt.zig + c-file guards but never claims this file. Verified: `crates/rustty-vt/src/build_options.rs` has `simd`/`kitty_graphics` but no `glyph_protocol`, no `Features` parse machinery. |
| O5 | `src/terminal/lib.zig` (1) | B/C (bundle S) | **ABSENT (rides).** One line: `pub const TinyIo = …` Zig-module export from `82df79ec8`. Lands with B's TinyIo + C's bundle S; no standalone action. |
| O6 | `build.zig` (22) | C bundle T / C#42+F B10 / F | **N-A as a file.** All three owning commits (`c75559589`, `1fdbb8c91`, `49fd1ae65`) are verdicted by C/F; the build.zig hunks are Zig build-graph whose Rust equivalents (cargo features, CI schema-verify step) land inside those bundles. |
| O7–O10 | `build.zig.zon`, `build.zig.zon.{json,nix,txt}` | **G** (themes half) / F (wuffs half, already PARTIAL) | **ABSENT (themes pin) — real finding.** F's rows deferred the iTerm2-themes pin verification to G ("area G"); G's file-set never included build.zig.zon and never did it. Verified: oracle now pins `ghostty-themes-release-20260810-152212-0173c3c.tgz`; rustty's vendored corpus (`crates/rustty-extra/themes/README.md` provenance) is at `20260720-153658-97e244c` = the range-START pin, i.e. one release behind. Action: re-vendor the corpus from the new release + re-run the G-THEMES count gate (592 may change). Wuffs `alpha.9→alpha.10` is F's existing PARTIAL row (confirmed §4). |
| O11 | `nix/devShell.nix` (28) | — | **N-A.** rustty has no `nix/` directory (verified `ls`). All 5 owning commits' substantive halves are verdicted elsewhere. |
| O12 | `Doxyfile` (3) | **C** (bundle T rider) | **ABSENT, blocked.** Hunk ships `src/terminal/c/types.schema.json` as an HTML_EXTRA_FILE. rustty HAS a Doxyfile but no schema file exists yet (verified grep → 0); ride bundle T. |
| O13 | `HACKING.md` (2) | — | **N-A.** Upstream drops "the iOS SDK" from the macOS build prereqs; rustty's HACKING.md is a deliberate fork (xtask/cargo pipeline) with no iOS-SDK sentence (verified: zero iOS mentions). |
| O14 | `CODEOWNERS` (1) | — | **N-A.** Upstream adds `/po/da.po @ghostty-org/da_DK`; rustty's CODEOWNERS is a deliberately forked single-owner file with no per-locale po rows (verified read). No action even when G ports `da.po`. |
| O15 | `.agents/skills/writing-commit-messages/SKILL.md` (2) | **G** (repo-meta) | **ABSENT.** Commit `fde9e281c` ("agents: remove double negative") appears in **no** area's commit enumeration — the only orphan *commit* found. rustty still carries the double negative "without no filler phrases" (verified, SKILL.md ~line 50). One-word fix. |

## 3. Double-claims — files verdicted by two areas

All shared files were checked for verdict conflicts. **No conflicting verdicts
exist.** Every double-claim is either (a) both-ABSENT with consistent ownership
notes, or (b) a per-file-slice split of one commit (different hunks, different
verdicts — by design, not a conflict).

| file | claimants | verdicts | conflict? |
|---|---|---|---|
| `src/global.zig` | C (#4, #27) + G (a82637b53 hunk) | ABSENT / ABSENT | no |
| `src/datastruct/limited_allocator.zig` | C (#9) + G (owner "C/D") + D (dependency) | ABSENT everywhere | no |
| `src/datastruct/wasm_page_pool.zig` | C (#49) + G + A (blocked consumer) | ABSENT everywhere | no |
| `src/benchmark/main.zig` | B (C8 registrations) + G (3 hunks) | ABSENT / ABSENT | no |
| `src/apprt/surface.zig` (53c6fdbe7) | E (P4-E7) + F (§2b, owns the 4 G1b tests) | ABSENT / ABSENT | no |
| `macos …/SurfaceView_AppKit.swift` (f4309055f) | D (Swift-half flag) + F | ABSENT / ABSENT | no |
| `src/os/i18n.zig` `N_` | E (P4-E43) + G (1125fa26d README pairing) | ABSENT / ABSENT | no |

Same-commit, different-file verdict splits verified as file-disjoint (not
conflicts): `727b8a02f` (A ABSENT page-cap / B ABSENT grid / D N-A parser lines /
O1 osc.zig), `b867a0f59` (A N-A / D N-A / B ABSENT snapshot-screen / O2 N-A),
`74a233b54` (A ABSENT render / C ABSENT headers / D N-A color-loop),
`2ed67cadd` (A/B/D ABSENT, disjoint files), `9673a22b0` (A/C ABSENT + O3),
`1fdbb8c91` (A/C/F ABSENT-or-N-A + O4), `82df79ec8` (B ABSENT / C N-A-this-area + O5).

## 4. Spot-checks (R11 default-refute)

**All seven assessments report 0 PORTED verdicts**, so there are no PORTED rows to
refute. The closest checkable claims of the same kind — rows positively asserting
"rustty already has this behavior/state at file:line" (PARTIAL already-equivalent
halves, N-A-by-equivalence rows, and load-bearing baseline claims) — were pooled
(17 candidates) and 10 selected by random shuffle (`sort -R`). Each was verified by
reading the upstream hunk and the cited rustty code. **Result: 10/10 CONFIRMED,
0 refuted.**

| # | claim (area, row) | verified | result |
|---|---|---|---|
| 1 | C §1 baseline: Phase-3 backlog remediated — `Option_` ends `ProgressReport = 30`, `TerminalData` ends `ScrollbackMaxLines = 35` | read `crates/rustty-vt-c/src/terminal.rs` ~1267 and ~2255: exactly those tags, exactly those values | **CONFIRMED** |
| 2 | G `02f34835e`: lru has zero rustty callers; deletable now | `pub mod lru` at `rustty-datastruct/src/lib.rs:21`; every non-datastruct "lru" hit is `FillRule` (z2d) or `-lrustty_apprt_c` (smoke.rs:151) — false positives as claimed | **CONFIRMED** |
| 3 | F `7c4c7adad` PARTIAL: wuffs-sys vendors the release-C artifact at alpha.9 | `crates-sys/wuffs-sys/build.rs:1-12` states v0.4.0-alpha.9, vendored `vendor/release/c/`; oracle moved to alpha.10 | **CONFIRMED** |
| 4 | F `7a171895d` PARTIAL: xcframework already iOS-free | `ls dist/RusttyKit.xcframework/` → `Info.plist`, `macos-arm64` only | **CONFIRMED** |
| 5 | E P4-E17(c) `0060d89b5`: `sequence_set` assigned BEFORE fallible `encodeKey` — live state-corruption path | `rustty-core/src/surface.rs` ~6743: `self.keyboard.sequence_set = Some(…)` precedes `self.encodeKey(…)?` at ~6751 | **CONFIRMED** |
| 6 | G `a00d155e9`/`df23bef0e` PARTIAL: rustty appends precomputed `defaults()`/`defaults_c()` (non-GTK arm of end state), no `translated()` loop | `rustty-config/src/value_types.rs` ~5971-5982: `extend_from_slice(&rustty_input::command::defaults{,_c}())` | **CONFIRMED** |
| 7 | C #22 `f36854345` N-A: C-ABI panic policy proven by live gate | `crates/rustty-apprt-c/tests/panic_policy.rs:35` `exported_c_abi_panic_aborts_host`, asserts signal 6 (SIGABRT) | **CONFIRMED** |
| 8 | E P4-E19 `f17b425aa` PARTIAL: `Surface.id` exists; the two log sites are comments still citing `@intFromPtr` | `surface.rs:293` `pub id: u64`; `surface.rs:~2801` + `app.rs:~736-739` commented-out logs with `@intFromPtr` text | **CONFIRMED** |
| 9 | C #34 `c80c37362` N-A: `is_at_prompt` never added; end state already holds | grep `is_at_prompt` → 0 in `include/rustty-embed.h` AND 0 in oracle `include/ghostty.h` at head | **CONFIRMED** |
| 10 | A `e20564791` N-A: fixup and integrity check gated on the SAME predicate by construction | upstream hunk = 1-line gate swap in printCell; rustty `terminal.rs` ~2450 `cfg!(debug_assertions)` spacer-tail fixup + `page_impl.rs:545` `assert_integrity` on the same `cfg!(debug_assertions)` | **CONFIRMED** |

## 5. Global verdict table

Per-area rows as published (a commit touching several areas' files is verdicted in
each; the range has **359 unique non-merge commits**, so area rows intentionally
overlap and do not sum to 359):

| area | commits verdicted | PORTED | PARTIAL | ABSENT | N-A |
|---|---:|---:|---:|---:|---:|
| A terminal-core | 53 | 0 | 0 | 49 | 4 |
| B snapshot-io | 73 | 0 | 0 | 73 | 0 |
| C capi-lib-wasm | 52 | 0 | 0 | 47 | 5 |
| D terminal-subsystems | 47 | 0 | 0 | 44 | 3 |
| E os-termio-cli-app | 53 | 0 | 6 | 46 | 1 |
| F ui-tiers | 93 | 0 | 2 | 82 | 9 |
| G config-i18n-datastruct | 102 | 0 | 2 | 62 | 38 |
| **area-row totals** | **473** | **0** | **10** | **403** | **60** |
| orphan commit (`fde9e281c`, §2 O15) | 1 | 0 | 0 | 1 | 0 |

Internal-consistency check: every area's published verdict counts sum to its
commit total (53=49+4, 73, 52=47+5, 47=44+3, 53=46+6+1, 93=82+2+9, 102=62+2+38). ✓

## 6. Critic's bottom line

1. **Coverage is near-complete and honest.** 366/381 files cleanly owned; the 15
   orphans are assessed above. Two orphans are real porting debt that fell in the
   seams: **O1 `osc.zig` 8 MiB capture cap** (severity: the DoS half of
   `727b8a02f` — both A and D believed the other owned it) and **O7 the
   iTerm2-themes pin** (rustty corpus one release behind). Both now have owners.
2. **No double-claim conflicts.** All shared files carry consistent verdicts.
3. **The uniform 0-PORTED result is credible.** 10/10 randomized presence-claims
   survived default-refutation, including the load-bearing Phase-3-baseline claim
   (C) and both "coincidental equivalence" PARTIALs (F). Nothing suggests the
   assessments over- or under-credited rustty's current state.
4. One orphan **commit** (`fde9e281c`, `.agents` doc fix) was outside every area's
   enumeration; assessed ABSENT here, assigned to G.
