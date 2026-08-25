# Phase 5 — Coverage reconciliation (critic pass)

Range: `12967b68f..683d8db64` (169 non-merge commits — recounted, matches the
assessments). Oracle working tree at the new pin `683d8db64`; rustty at
`8120dd10` (main). Inputs: the five area assessments
(`A-kitty-subsystems.md`, `B-terminal-core.md`, `C-capi-lib.md`,
`D-ui-tiers.md`, `E-platform-tail.md`) and
`.port/phase-5/UPSTREAM_DIFF_STAT.txt` (155 files). No code and no other
assessment was modified.

---

## 1. File → area map (all 155 files)

Every file in `UPSTREAM_DIFF_STAT.txt`, mapped to the area whose stated scope
AND verdict rows cover it. `⚠ DOUBLE` = claimed by two areas (§3);
`⚠ ORPHAN` = claimed by none (assessed inline in §2).

| # | file | area |
|---|---|---|
| 1 | `.github/VOUCHED.td` | E (N-A ×8) |
| 2 | `.github/workflows/release-tip.yml` | E (PORTED) |
| 3 | `build.zig` | **⚠ ORPHAN** (`c5f4f00ed` slice) |
| 4 | `build.zig.zon` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 5 | `build.zig.zon.json` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 6 | `build.zig.zon.nix` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 7 | `build.zig.zon.txt` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 8–10 | `example/c-vt-effects/src/main.c`, `example/c-vt-paste/{README.md,src/main.c}` | C |
| 11 | `flake.lock` | **⚠ ORPHAN** (`48a9e29f7` — orphan COMMIT, see §2) |
| 12 | `flake.nix` | **⚠ ORPHAN** (`48a9e29f7`) |
| 13 | `flatpak/zig-packages.json` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 14–23 | `include/ghostty.h`, `include/ghostty/vt.h`, `include/ghostty/vt/{io,kitty_graphics,modes,osc,paste,sys,terminal,types}.h` | C |
| 24–53 | `macos/**` (30 files: Sources 24–47, Tests 48–53) | D |
| 54 | `nix/build-support/update-mirror.nu` | **⚠ ORPHAN** (`ffad4c6ec`) |
| 55 | `pkg/afl++/build.zig` | D |
| 56–61 | `pkg/wuffs/**` (build.zig, build.zig.zon, include/{stdlib,string}.h, src/{main,swizzle}.zig) | D |
| 62–65 | `po/{bg,nl,pt_BR,zh_TW}.po` | E |
| 66 | `src/Surface.zig` | B |
| 67 | `src/apprt.zig` | D |
| 68–74 | `src/apprt/**` (embedded.zig, gtk/Surface.zig, gtk/class/{clipboard_confirmation_dialog,surface}.zig, gtk/key.zig, structs.zig, surface.zig) | D |
| 75 | `src/build/GhosttyZig.zig` | C |
| 76–79 | `src/cli/{help,list_themes,ssh,ssh_cache}.zig` | E |
| 80–81 | `src/config/{Config,formatter_file}.zig` | E |
| 82 | `src/font/glyf_rasterize.zig` | D |
| 83 | `src/input/paste.zig` | **⚠ DOUBLE: B + D** |
| 84 | `src/lib/TinyIo.zig` | **⚠ ORPHAN** (`dda8e6f31` slice — three-way disclaim, §2) |
| 85 | `src/lib/types.zig` | C |
| 86 | `src/lib/union.zig` | C |
| 87 | `src/lib_vt.zig` | C |
| 88–89 | `src/os/{TempDir,macos}.zig` | E |
| 90–93 | `src/renderer/{OpenGL,Thread,generic,image}.zig` | D |
| 94–95 | `src/simd/{base64,base64_scalar}.zig` | B |
| 96–101 | `src/terminal/{PageList,Screen,StringMap,Tabstops,Terminal,build_options}.zig` | B |
| 102–109 | `src/terminal/c/{io,kitty_graphics,main,paste,result,sys,terminal,types}.zig` | **⚠ DOUBLE: B + C** |
| 110 | `src/terminal/clipboard.zig` | B |
| 111 | `src/terminal/compress/Page.zig` | B |
| 112 | `src/terminal/kitty.zig` | B |
| 113–130 | `src/terminal/kitty/**` (18 files: clipboard{,_command,_grants,_response,_write}.zig, dnd{,_command,_drop,_response,_test}.zig, graphics{,_animation,_command,_exec,_image,_pixel,_storage,_unicode}.zig) | A |
| 131 | `src/terminal/main.zig` | B (scope-covered; see §5 note — never cited by name) |
| 132 | `src/terminal/modes.zig` | B |
| 133 | `src/terminal/osc.zig` | B |
| 134 | `src/terminal/osc/kitty_metadata.zig` | B |
| 135 | `src/terminal/osc/parsers.zig` | B |
| 136–139 | `src/terminal/osc/parsers/{clipboard_operation,kitty_clipboard_protocol,kitty_desktop_notification,kitty_dnd_protocol}.zig` | B |
| 140–142 | `src/terminal/{page,parse_table,paste}.zig` | B |
| 143–145 | `src/terminal/snapshot/{snapshot.ksy,terminal.zig,testdata/complete-v1.hex}` | B |
| 146–148 | `src/terminal/{stream,stream_terminal,sys}.zig` | B |
| 149–153 | `src/termio/{Termio,Thread,message,shell_integration,stream_handler}.zig` | E |
| 154 | `typos.toml` | E |
| 155 | `valgrind.supp` | E |

Count check: A 18 + B 30 + C 17 + D 50 + E 21 + orphans 10 + double-claimed 9
= **155** ✓.

---

## 2. Orphans — 10 files, assessed inline here

No area's scope or verdict rows cover these. Verdicts below are mine
(critic), same vocabulary as the areas.

### 2.1 `build.zig` (+2) — commit `c5f4f00ed` — **N-A**

Adds `--error-exitcode=1` to both valgrind runner invocations (the flag makes
valgrind findings fail the build). Rustty has **no** `build.zig` and **no
valgrind runner anywhere** (`grep valgrind` over ci.sh / xtask / Makefile /
workflows → only the `valgrind.supp` path filter in `release-tip.yml:129`);
its memory gate is Miri/ASan (R6). No seam. **Reopen trigger:** if rustty
ever adds a valgrind lane, carry `--error-exitcode=1` from day one.

### 2.2 `build.zig.zon` (±2), `build.zig.zon.json` (±6), `build.zig.zon.nix` (±8), `build.zig.zon.txt` (±4), `flatpak/zig-packages.json` (±10), `nix/build-support/update-mirror.nu` (+52/−6) — commit `ffad4c6ec` — **N-A ×6**

One coherent dependency-mirror migration: gobject and wuffs URLs move from
GitHub to `deps.files.ghostty.org` tarballs (wuffs rev UNCHANGED,
`7411f488` = v0.4.0-alpha.10; only the transport changed, hash re-derived for
the tarball form), and `update-mirror.nu` gains git-dependency support via
`zig fetch` normalized cache archives. Rustty vendors dependencies in-tree
(R3) and has no zon/nix/flatpak/mirror pipeline — nothing to port. Consistent
with area D's `ffad4c6ec` N-A verdict for the `pkg/wuffs/build.zig.zon`
slice, including its (pre-existing, Phase-4 `7c4c7adad`) note that rustty
vendors alpha.9 while upstream pins alpha.10 — this commit does not move that
needle.

### 2.3 `flake.lock` (±26), `flake.nix` (±30) — commit `48a9e29f7` "nix: update nixpkgs-unstable" — **N-A**, and an **orphan COMMIT**

`48a9e29f7` appears in **no** area's commit enumeration (every area's path
filter excludes flake files) — with it counted, the phase covers 169/169
non-merge commits; without it the union of area logs reaches only 168.
Content: nixpkgs-unstable bump (comment: Zig 0.16 / Gnome 50 / Gtk 4.22 /
fontconfig 2.18), channel URL `.tar.xz`→`.tar.zst`, and removal of the
libfyaml/appstream/libadwaita devShell override workaround. Rustty has no
flake, no nix directory. No seam, nothing transfers. **N-A.**

### 2.4 `src/lib/TinyIo.zig` (+53) — commit `dda8e6f31` slice — **ABSENT (behavior) / N-A (vehicle)** — three-way ownership disclaim

The pointer cycle: area A says the TinyIo half is "area C/os" (§2.6), area C
says it "is **area B's** (explicit fan-out split)" (§2 row 18, §5 Bundle SR),
and area B **never mentions TinyIo at all** (its `dda8e6f31` row covers only
`src/terminal/sys.zig` + `c/sys.zig`). Net: the hunk was read by C (coupling
note) but verdicted by nobody.

Inline assessment: the hunk gives TinyIo a real `randomSecure` vtable impl —
`arc4random_buf` where libc provides it, `getrandom` loop on Linux
(EINTR-retry), `error.EntropyUnavailable` otherwise; zero-length is a no-op —
plus test `"randomSecure fills with fresh entropy"` (non-zero, non-repeating,
zero-len no-op; comment: a zero fill would make every one-time password the
same).

- **Vehicle:** TinyIo itself remains a CONFIRMED NON-PORT
  (`.port/phase-4/NON_PORTS.md` row: rustty has no `std.Io` abstraction; W5
  Bundle S re-read 2026-08-25). That row's **reopen trigger is now HIT**:
  this range creates exactly the foreseen "future upstream consumer" —
  `kitty/clipboard_grants.generateOtp(io)` draws OTP entropy through the Io.
  The row must be re-read and its rationale updated in the same change that
  ports Bundle SR / area A's `clipboard_grants`.
- **Behavior (ABSENT):** the platform-entropy default has no rustty
  counterpart yet. The Rust shape: the OTP path gets entropy from the OS
  directly (std/`getrandom` — the honest analogue of TinyIo's default), with
  the C-API `sys.random_secure` override (area C Bundle SR) taking
  precedence, and **no-entropy must be an error, never a weak password**
  (the upstream test's load-bearing assertion). The 4 assertions of the
  TinyIo test transfer to whatever Rust helper hosts this — **gate-invisible
  test debt** (G1b maps no `src/lib/*.zig`), on top of the three blind spots
  area B already lists for `dda8e6f31`'s other halves.
- **Owner assignment (to unblock planning):** with area C's Bundle SR, which
  already owns the `sys` C surface and names the coordination; C's "TinyIo is
  area B's" pointer should be treated as superseded by this row.

---

## 3. Double-claims — 9 files

Verdicts do NOT conflict in either case (all ABSENT on both sides); the risk
is task-assignment duplication or a dropped half, so the porting workflow
must pick one owner per file.

1. **`src/input/paste.zig` — B and D.** B pulls it in explicitly ("one file
   OUTSIDE the literal path filter... the brief assigns its G1b drop to this
   area", §scope; verdicts `a5bb22e23` in §2.1 and claims `input_paste
   drop=7` in §5). D lists it in its owned paths (§heading), verdicts
   `a5bb22e23` in §2a, and claims the same `input_paste drop=7` as "this
   area's ONLY red G1b row" (§5). Both agree: ABSENT, 7 tests
   (`isSafeWith`, 5× `encodeWriter…`, `max_frame_size`), oracle 17 vs rustty
   10. **Suggested resolution:** D's enumeration of the 7 test names is more
   precise; B's placement inside atomic bundle B-1 is the binding ordering
   constraint. One task, filed under the B-1/Bundle-P landing, satisfies
   both.
2. **`src/terminal/c/{io,kitty_graphics,main,paste,result,sys,terminal,types}.zig`
   (8 files) — B and C.** B's scope statement includes `src/terminal/c/`
   ("the `c/` zig files themselves are MINE (rustty-vt-c)", §7) and its §2.1
   rows verdict the c/ hunks of `60a1ae2df`, `da27e6c90`, `e03475c0c`,
   `dda8e6f31` etc.; C's title scope is `src/terminal/c/**` and its §0 marks
   the same files' range diffs EXHAUSTIVE, line-by-line, with the deeper
   evidence (export comm, A5(d) RED on `terminal_paste`, layout-test gaps).
   Verdicts agree everywhere the rows overlap (ABSENT throughout; both flag
   the same G1b vt_c masking — 9 new zig tests under a +45 rust surplus).
   **Suggested resolution:** C owns these files (its coverage is exhaustive
   and gate-anchored); B's rows stand as cross-area couplings only.

---

## 4. Spot-checks — 8 randomized non-ABSENT verdicts, R11 default-refute

Method: pooled every non-ABSENT verdict across the five assessments
(A: 2 N-A; B: 4 N-A; C: 1 PORTED + 1 N-A; D: 3 N-A; E: 4 PARTIAL + 1 PORTED
+ 8 N-A collapsed to one candidate since the 8 VOUCHED rows are a single
classification) → 17 candidates; shuffled with `awk srand(683)`, took the
first 8. Each was checked against the upstream commit in the oracle AND the
cited rustty code, attempting to refute.

| # | verdict checked | result | evidence |
|---|---|---|---|
| 1 | **A / `f3e98fb72` N-A** (supersede: removes in-range C-key acceptance) | **CONFIRMED** | `git show f3e98fb72`: deletes the `if (kv.get('C')) \|v\|` block + the 0.44/0.45 rationale comment, leaves "Tested only against 1 like Kitty."; `4b915e3bc` is an ancestor-verified in-range commit and its diff ADDS that exact block — the code lived and died inside the range. Rustty `graphics_command.rs` AnimationFrameLoading parse (~:1137–1142) handles only `X` (and hard-errors on X∉{0,1}, the pin shape, exactly as A's row says); the `kv.get(b'C')` hits at :1038/:1203 are different structs (AnimationFrameComposition, where C is correct upstream too). Rustty never had the removed code. |
| 2 | **B / `b7ee3ab6b` N-A** (the revert itself) | **CONFIRMED** | Commit message: `This reverts commit b56c6d88f81bd…`; stat is the exact mirror (−10/+2 vs +10/−2). `git diff b56c6d88f~1 b7ee3ab6b -- src/terminal/stream_terminal.zig` is EMPTY — the file at the revert equals its pre-`b56c6d88f` state (which already includes `eeab85b96`, separately verdicted ABSENT by B as the surviving end state). Clean revert; nothing to port. |
| 3 | **B / `b56c6d88f` N-A** (reverted in-range) | **CONFIRMED** | Same evidence as #2, from the other side: its +10/−2 is fully annulled inside the range; porting it would build code the range itself deletes. B's rule citation (end state = `eeab85b96` alone) is exactly right. |
| 4 | **E / `2303bcf08` PORTED** (release-tip pinned-tag comment) | **CONFIRMED** | Oracle hunk: `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` comment `# v6.0` → `# v7.0.1`, SHA unchanged. Rustty `.github/workflows/release-tip.yml:461`: identical SHA, comment already `# v7.0.1`. End state byte-equivalent (rustty's own pinact lane is the plausible independent source, as E says — and E correctly warns not to count it as porting-lane evidence). |
| 5 | **C / `8c5bc3d29` N-A** (lib/union.zig slice) | **CONFIRMED** | The union.zig hunk is purely comptime: `Value()`'s `@setEvalBranchQuota` + inline-for body → `return @FieldType(Union, @tagName(tag));`. Zero ABI/behavior delta. Rustty `union.rs` (~:120–145) uses a descriptor model storing already-resolved per-field size/alignment (PORT NOTE present) — no transcription of the old body exists to update. Note the verdict is scoped correctly: the commit's stream/osc bulk is verdicted ABSENT by B (§2.3), so N-A here creates no coverage hole. |
| 6 | **E / VOUCHED ×8 N-A** (`bb00a5c98` `a36dc245b` `5834a0e3d` `da0093671` `d03cd1f53` `2021d2add` `74a133ea1` `311d38376`) | **CONFIRMED** | All 8 commits touch ONLY `.github/VOUCHED.td` (7×+1, 1×+4 = 11 lines, matching the diff stat). Rustty `.github/` has no `VOUCHED.td` (ls verified) and `AI_POLICY.md` exists at the repo root — the Phase-4 G precedent (roster superseded by rustty's own AI policy) applies unchanged. |
| 7 | **E / `9566a1a87` PARTIAL** (termio release initial input resources) | **CONFIRMED** | Upstream diff verified: ArrayList + `errdefer for (inputs.items) \|item\| item.deinit()`; `.raw` stops duping (`.{ .string = v }`); `Input.deinit` closes the file; threadEnter `defer` deinits all inputs; `.file` arm gains `defer self.alloc.free(contents)`. Rustty citations all check out: `termio_impl.rs` prepareInput (~:225–247) — an open failure `return Err(…)` drops the partially-built `Vec`, closing already-opened `Input::File`s via Drop; threadEnter (~:882–921) consumes `inputs.into_iter().flatten()`, and the error path (`threadExit` then `return Err`) drops the remainder; `read_input_file` bytes drop at arm end. So the leak-fix behavior is indeed N-A-by-ownership. The ABSENT remainder is real too: the mirror comments still quote the OLD reference (`// .raw => \|v\| .{ .string = try alloc.dupe(u8, v) }` at ~:228; the old `.file => \|f\| self.queueWrite(data, f.readToEndAlloc(…)` shape quoted in both the ~:758–796 block and the ~:889–899 in-body mirror), and `Input::String(v.clone())` (~:230) still clones where upstream now borrows. PARTIAL is the right call. |
| 8 | **C / `f4f9991d2` PORTED** (pre-existing equivalence: snapshot module surface) | **CONFIRMED** | Upstream: one line, `pub const snapshot = terminal.snapshot;` in lib_vt.zig. Rustty: `crates/rustty-vt/src/lib.rs` has `pub mod snapshot;` (public, in the :120–130 window — C's ":125" cite is within drift tolerance), and `crates/rustty-vt-c/src/snapshot.rs:22–23` consumes `rustty_vt::snapshot{,::io}` — the claimed cargo-build regression guard is real (privatizing the module breaks the workspace). |

**Spot-check failures: 0 of 8.** No verdict was refuted; every citation
resolved to the claimed content (one line-number cite off by a few lines,
noted in row 8, content correct).

---

## 5. Global verdict table

Commit-level rows as reported by each area, plus this pass's inline rows.
(Cross-area commits appear in multiple areas' counts by design; the range
itself has 169 unique non-merge commits.)

| Area | rows | PORTED | PARTIAL | ABSENT | N-A |
|---|---:|---:|---:|---:|---:|
| A kitty-subsystems | 39 | 0 | 0 | 37 | 2 |
| B terminal-core | 63 | 0 | 0 | 59 | 4 |
| C capi-lib | 31 | 1 | 0 | 29 | 1 |
| D ui-tiers | 38 | 0 | 0 | 35 | 3 |
| E platform-tail | 65 | 1 | 4 | 52 | 8 |
| Reconciliation inline: `48a9e29f7` (flake) | 1 | 0 | 0 | 0 | 1 |
| Reconciliation inline: orphan file slices (`c5f4f00ed` build.zig; `ffad4c6ec` zon/flatpak/mirror ×6; `dda8e6f31` TinyIo) | 8 | 0 | 0 | 1 (TinyIo behavior) | 7 |

Matches the task-brief summaries exactly (A 37/0, B 59/0, C 29/0, D 35/0,
E 52/4 absent/partial). The phase's headline stands and is now
reconciliation-verified: **essentially nothing from this range is ported**;
the 2 PORTED rows are independent-path equivalences (pinact comment, module
visibility), the 4 PARTIALs are Rust-ownership equivalences with comment/
idiom residue, and every N-A is either an in-range supersession/revert, a
Zig/nix/flatpak-toolchain artifact with no rustty seam, or the VOUCHED
roster precedent.

## 6. Notes for the porting workflow (defects found by this pass)

1. **Orphan commit `48a9e29f7`** (flake bump): harmless (N-A), but the
   phase's commit-coverage claim should read 169/169 only WITH this
   reconciliation counted. Future phase briefs should include `flake.*`,
   `build.zig*`, `flatpak/`, `nix/` in some area's path set so the
   enumeration is closed by construction.
2. **TinyIo three-way disclaim** (§2.4) is the only place the areas'
   cross-referencing actually dropped a hunk. Assign to C/Bundle SR; re-read
   the NON_PORTS TinyIo row there (its reopen trigger is hit); carry the 4
   entropy-test assertions as explicit acceptance criteria.
3. **Contradictory ownership pointers, no coverage hole:** for
   `src/terminal/sys.zig`, A says "area C/os owns" and C says "area D" — in
   fact B verdicts it (correctly; it is in B's path filter). Cosmetic, but
   task-routing should follow B's row, not the pointers.
4. **`src/terminal/main.zig` never cited by name** in B despite being in
   B's scope; its +6 lines (module exports `paste`/`Paste*`/`MimeReader`)
   belong to `a5bb22e23`/`876032316`/`da27e6c90`, all verdicted ABSENT in
   B §2.1, so it is commit-covered — but the B-1 port tasks must remember
   the main.zig export rider explicitly or the new modules land private.
5. **Double-claim resolutions** (§3): file `src/terminal/c/**` under C,
   `src/input/paste.zig` as one task under the B-1/Bundle-P landing.
