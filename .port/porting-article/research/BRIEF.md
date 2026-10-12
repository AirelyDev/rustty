# Rustty journey article: research brief

Status: a synthesis of 8 round-1 dossiers (bun-article, timeline, strategy, bugs, perf,
usage-stats, codebase, website) and 6 round-2 dossiers (`r2-platforms`, `r2-human-story`,
`r2-codex-audit`, `r2-cost`, `r2-data-fixes`, `r2-glossary`, plus `r2-outline-plain.md`) in this
directory. Data lives in `data/` (round-2 files are `data/r2-*`). Snapshot: HEAD `1d22ba80d` on
`phase-8-upstream-resync`, 2026-10-08, oracle pin `a60e9e2a5`.

Conventions in this brief:
- **Provenance**: M = MEASURED, D = DOCUMENTED, E = ESTIMATED.
- **Check**: ✔ = confirmed, ✎ = corrected by the fact-checker (the corrected value is the one
  shown), ? = unverifiable. Round-2 values are the post-fact-check values from each dossier's
  "Fact-check" section, not the dossier's raw headline.
- Every number in section 2 is the post-check value. Never quote a dossier's raw headline without
  looking here first.
- **Internal ids stay in this brief only.** Published prose uses plain descriptions (rule names,
  gate ids, ledger ids, phase/wave labels and chart ids never appear). The mapping is in
  `r2-glossary.md` and `data/r2-glossary-ids.csv`.

---

## Round 2 changelog (2026-10-08)

Author answers now in force:
1. **Defects: publish fully.** The article is not live yet, so defect classes, counts, mechanisms and
   the fixes list can all be published.
2. **Repo will be public** at publication, so commit hashes can link.
3. **Cost: paid via subscription.** For the lost-history window, extrapolate assuming a maxed-out
   subscription and count half of it toward Rustty. API-equivalent list price is a reference only.
4. **Quotes OK, without internal ids** (no "R7", "R13", "UD", "G3b" and so on in prose).
5. **Linux:** check the commits made on the Linux VM, where GTK was really tested.
6. **Motivation:** "seeing what was possible". Quiet stretches were mostly token limits and other
   projects. The Codex audit ran in the Codex app. Lost transcripts were all Claude Code.
7. **Hardware:** a MacBook Pro, which `sysctl` and the prompt kit identify as an Apple M4 Pro (Mac16,7),
   14 cores, 48 GB. Daily-driving anecdotes are optional.

What changed in this brief:
- §1 outline rewritten without internal ids (from `r2-outline-plain.md`), with round-2 content: origin
  story, Linux/GTK hands-on testing, the Codex audit's real shape and stop reason, subscription cost
  framing, the silent-skip failure story, the per-platform limits.
- §2: corrected the gate counts (101 entries, 24 self-tests; "86 IDs" and "22 -SELF" were a regex
  artifact), the cost total ($23.8k API-equivalent, ~$420 paid), the zero-commit days (55, Pacific
  time), the discovery-method split, the libvt sizes and the sanitizer matrix (now with HWASan and Linux
  TSan). Added rows 64-95 (human story, Codex audit, cost, platforms, skips, build times, model
  timeline).
- §3: A2, N5, N15, N31, N42, N43, N49 moved to R; 10 new charts (C1-C10) added.
- §4: closed 8 contradictions, added 11 new ones.
- §5: Q2-Q8 and Q11 answered; Q1 answered with one residual check; Q9-Q10 still open.
- §7: items 3, 5, 8 and 10 partly or fully closed.

---

## 1. Proposed article outline (plain language)

The audience is technical. The arc follows the journey in time, and each section ends on a
measurable result. Working title: "Porting Ghostty to Rust", or "Rustty: 110 days of porting
Ghostty to Rust with agents". No internal ids in the prose; quote rule texts verbatim from
`r2-glossary.md` "Quotable rule texts" (3 of them come only from agent memory and need the author's
confirmation before printing).

1. **TL;DR / stats card.** 110 days, ~2,970 commits, 302k lines of Zig ported to 508k lines of Rust
   code, 7,270 Rust tests against 4,423 Zig tests (none dropped), faster than Ghostty on all 14
   benchmarks, 205 Ghostty defects recorded (135 reachable ones handled in the port; do not say "205
   fixed"), Miri and the sanitizer lanes green at the previous reference commit. Paid by two flat
   subscriptions (~$420 attributable). Mirrors Bun's opening stats table.

2. **Why, and what "port" means here.** Motivation: seeing what was possible. Origin storyboard: on
   06-20 at 21:42 PT the author asked Claude to clone Ghostty; at 22:40 asked about the benefits of a
   Zig-to-Rust port; at 23:08 asked how to prompt "a full fidelity port, 100% functional parity"; at
   00:27 named it Rustty and had Claude write the prompt kit (a parity constitution, a master prompt and
   per-phase prompts). The author launched the master prompt; agents wrote the Phase 0 plan. About 18
   hours from clone to first commit. No earlier interest in porting is on record; the author was a
   Ghostty user. Scope: parity with one pinned Ghostty commit, every comment carried over, the C ABI
   byte-compatible, the Swift macOS app reused unchanged, nothing Ghostty does not have. The one
   exception: never copy a Ghostty bug.

3. **Starting point: the reference as a contract.** Before any Rust, the 3,234 Zig tests were
   inventoried as a contract, Ghostty's benchmarks and binary sizes recorded as a baseline, and the
   idea of a pinned reference commit fixed. The sanitizer matrix and a counting allocator on the page
   pool were first requested on day 3; the first usage-limit interruption came on day 2.

4. **Cutting a 345-file cycle into crates.** Ghostty's `src/` is one import cycle of 345 files. It was
   cut at 14 points into 61 Cargo packages with no cycles, 18 levels deep. Key layering choices: the
   font crate sits below the terminal crate and never depends on it (a 06-25 design decision); the
   renderer reads configuration through a trait it owns, so it never depends on the config crate;
   process spawning lives in the terminal-I/O crate because placing it in core created a cycle. 43 of
   the 60 crate directories (29 crates, 14 C-library wrappers) were scaffolded on day 1 and ported
   leaves first; none was ever deleted.

5. **The greenfield sprint (06-21 → 07-12).** The terminal engine went from 7k to 135k lines and from
   113 to 2,032 tests in 4 days. Font, renderer and app-runtime tiers followed, using stub types whose
   methods panic until their subsystem (GPU, event loop, inspector) arrived. First Metal pixels on day
   19; a runnable macOS terminal on day 22, driving Ghostty's unchanged Swift app. Work was fanned out
   by ~155 orchestration scripts launched with prompts like "ultracode" and "continue".

6. **Reading Bun's post: the 07-10 realignment.** Bun published "Bun in Rust" on 07-08. Quote the
   author's 07-10 02:43 prompt, especially "I haven't validated my port at all, so I'd like you do help
   do that validation, and possibly catch bugs I haven't uncovered yet", and the 03:04 request to adopt
   the LIFETIMES.tsv approach. Rustty then adopted six new porting rules (Zig-to-Rust primitive
   semantics, keeping compile-time work compile-time, unsafe accounting, porting markers, split
   implementer and reviewer agents, upstream tracking), a pointer-lifetime ledger (216 pointer fields
   classified retroactively, 314 by October), an unsafe census and eight automated checks, each aimed
   at a regression class Bun reported. Show what was adopted, adapted or rejected. Payoff: the first
   full Miri run over the terminal engine (2,037 tests) found aliasing UB in search, fixed ~7 hours
   later; the lifetime audit found 20 issues, including latent aliasing UB in the event loop's
   lock-free queue.

7. **Translating Zig pointers into Rust's aliasing model.** The technical core. A Rust `&mut` argument
   is protected for the whole call, so code Zig writes with plain pointers became raw-pointer
   arguments. Also intrusive lists via a per-type "address of the link field" trait, recovering a
   parent struct from a field pointer (`@fieldParentPtr` / `container_of`), and a long-lived alias
   into a field used in a loop. Method: prove each pattern on a small isolated Miri harness (33) rather
   than by argument. Timeline of memory-safety bugs the port itself introduced (38 events, 81 defects,
   62 found by Miri). Zig and Rust side by side for 2-3 patterns.

8. **Performance: from 0/14 to 14/14 in one day.** On 07-15 Rust was slower on all 14 benchmarks (+42%
   suite). On 07-16, 29 performance commits over ~10.5 hours made it faster on all 14 (suite −21.7%,
   parser 2.85×, screen clone 2.42×). Why link-time optimisation is banned, the parity bug profiling
   exposed (`continue` vs `break :hyperlink`), the August class of gaps where a compile-time parameter
   had been ported as a runtime value (23 of 33 configurations faster against Ghostty main), the 3 ms
   lower process floor (compare per-operation slopes). Memory, binary size (native libvt 1.1× Ghostty's),
   the October re-measure (14/14). Hardware: MacBook Pro, Apple M4 Pro, 48 GB.

9. **Keeping a port in sync: 7 moves of the reference commit.** 1,157 upstream non-merge commits
   (+185k/−34k lines) absorbed. "Move the reference first, then port until the checks are green again",
   with the number of failing checks as the progress meter; turnaround shrank to the same day. Ghostty's
   Zig code grew 34% during the port.

10. **Checks as the specification.** The CI check list grew from 11 to 101 entries (75 blocking, 26
    reporting progress); 24 of them test the checkers themselves by injecting a violation. Lesson: a
    check that derives "green" from a checklist is only as true as the checklist. Results: comment
    fidelity (34,345 comments, 0 missing), test counts per module (211 modules, all at or above Zig),
    C ABI parity (35/35 headers, 208/208 functions, 91/91 embedding entry points), and the struct-size
    check added after a by-value struct size mismatch crashed macOS menu shortcuts. **Failure story:**
    on 09-23 a new lint found 185 tests that passed while checking nothing whenever a resource was
    missing; now 0, and skips must be compile-time, strict-mode pivots or loudly ignored. 6,325 of 7,270
    tests have no skip mechanism at all.

11. **Never port a bug: the defect ledger.** The rule came from one human prompt (09-11 10:43): "Wait,
    that seems odd. We shouldn't port the crash, please don't. Fix it here, update the plan/rules
    accordingly, and document all issues like this so we can propose upstream fixes later. Do this for
    past work too..." It was extended on 09-23 to "the port implements the intended behaviour, not the
    bug". 205 entries (135 live: 60 crashes, 19 UB, 14 leaks, 9 hangs, 33 broken intent; 59 latent, 11
    test weaknesses), 172 in Ghostty and 33 in dependencies. 38 of the 60 crashes are UB in Ghostty's
    shipped ReleaseFast build. How they were found (hand-classified): reading 92 (parity audit, defect
    sweep, translation, reviewers), checked arithmetic/casts/bounds 30 (29 crashes), cross-platform
    bring-up 25, compiling code Zig never analyses 17 (no crashes), fault injection 12, test port 11,
    the type checker forcing the question 5, sanitizers 3; Miri and fuzzing found none first. Ghostty has
    since fixed 3 independently and Google Highway 1. Publish fully, with a link to the fixes list.

12. **Second opinions: the Codex audit.** Two Codex app runs, 07-31 18:51 → 08-02 14:05 UTC (~43 h):
    run 1 on gpt-5.6-sol (high) audited non-macOS parity and returned 20 verified gaps; run 2 on gpt-5.5
    (high, then medium) tried to audit "every single line" with a /goal loop: 203 automatic
    continuations vs 10 human messages, 667 workers (449 range reviews + 248 independent replays), at
    most 6 at once. It stopped when the Codex weekly limit hit 99%, with 0.35% of the 3.1M-line universe
    strictly credited. Funnel: 659 registry rows → 147 groups → 144 confirmed, 3 refuted, 140 fixed and
    verified (3 deferred, 1 no change) in one Claude remediation commit on 08-11 (532 files, +87k, 4,660
    tests). Indirect win: the fuzz target the audit asked for found a reachable crash in OSC color
    parsing. Then the implementer/reviewer loop shown as a replay of 3 real bugs, and ~1,073 automated
    security-review sessions.

13. **Verification matrix and platforms.** Miri over 38 crates (13,354 s), Tree Borrows green after 8
    test-side fixes, a 6.9× Miri speed-up from nextest. ASan, LSan, MSan, TSan, HWASan and Valgrind
    counts with the host named per lane (macOS or the Linux VM), dated to late September / early
    October. Three Ghostty-side defects found by sanitizers. Cross-host builds green on macOS, Linux
    (Fedora aarch64 VM), Windows MSVC and Windows GNU. **Linux story:** the GTK port was written on two
    Linux VMs by Claude Code sessions running there (77 commits on the first VM, 07-16 → 07-18; ~75 on
    the second, 07-18 → 07-22); first light 07-17, a live terminal in a GTK window 07-18. The author
    tested it by hand next to Ghostty on GNOME/Wayland, X11 and sway (155 prompts over 4 days) and found
    real bugs: input swallowed, a crash on right-click, missing menu accelerators, title-format drift,
    an unknown terminfo name, a striped title bar. Quote: "Is works exactly as Ghostty to me" (07-19,
    sway quick terminal). Show the platform table (§2 rows 86-91).

14. **The orchestration evolved, and the human in the loop.** Orchestration scripts gave way to
    subagents working in git worktrees, each reviewed before merge. September: 1,142 subagents, 214
    worktrees, 88.6% of model calls from subagents. 898 Rustty prompts on 62 days (median 46
    characters); ~79 engaged hours (estimate); over half of non-merge commits landed more than an hour
    after the last human prompt; the longest unattended run was 180 commits overnight on 09-23/24.
    Prompting shifted from "workflow"/"continue" to manual bug reports (VM era) to status checks and
    Miri talk (October). Model switches followed usage allotments. Lessons: no more than three
    worktrees building at once (disk), context growth (median 77k → 204k tokens). Model timeline from
    commit trailers. Optional aside: Rustty was embedded as a library in another of the author's apps on
    09-17. All prompting was via Claude Code (Mac, and on the Linux VM).

15. **What it cost.** Paid by subscription: Claude Max 20x for the whole port (assumed) and one ChatGPT
    Pro month for Codex. Attributable outlay ~$420-$600 (Claude at half its share, Codex at its measured
    30.5%). API-equivalent at list price as a reference: ~$23.8k central ($19.8k-$31.4k), of which
    $11.0k is recovered and measured; the lost Mac window (06-21 → 09-07) is extrapolated as a maxed
    weekly quota × 50%, which the measured 54-56% Rustty share in the recovered window supports. About
    56× leverage. Codex needs no halving (99% Rustty in the audit week). Cache reads are 96.5% of tokens
    but 55% of cost; delegated agents 75.6%. Show the limit timeline with the author's own words.
    Compare with Bun's ~$165k only API-equivalent to API-equivalent, with a scale caveat.

16. **Unsafe, honestly.** 22,310 `unsafe` sites, 26.66 per thousand physical lines, roughly flat while
    the code doubled. About 58% are in tests; density concentrates in the FFI crates. Compare with Bun's
    ~4% with a basis caveat.

17. **What's left / limitations.** One low-severity difference is open (about 35 log lines format
    values differently). The GTK app runs in a Linux VM (GNOME/Wayland, X11, sway) on software GL, but
    was never run on a real GPU or on x86_64. FreeBSD is type-checked only. Windows has libraries but
    no app, as in Ghostty at the pin; the remaining Windows gap is a full test run. No benchmark run at
    the current reference commit. The z2d graphics port is a subset at a different version. The
    latest sanitizer lanes at the newest reference commit are mid-run (see §4).

18. **Appendix: methodology.** How each number was measured (git revisions, tokei code lines vs
    physical lines, commit trailer parsing, LiteLLM list prices, the subscription extrapolation in
    `r2-cost.md` §5(b)) and the hardware (MacBook Pro, Apple M4 Pro, 48 GB, 14 cores; one Miri
    re-measure ran on a slower host). Linux: Fedora 44 aarch64 VM, 8 vCPU, ~10 GB, software GL.
    Windows: Windows 11 ARM64 VM targeting x86_64 under emulation.

---

## 2. Headline numbers (fact-checked values only)

| # | Metric | Value | Prov | Check | Source |
|---|---|---|---|---|---|
| 1 | Duration | 110 days, 2026-06-21 → 2026-10-08; **55 zero-commit days (Pacific time, all refs)** in 15 runs | M | ✎ (52 is unexplained; UTC gives 60) | timeline / r2-human-story |
| 2 | Commits | 2,968 on HEAD (2,994 all refs, 2,728 non-merge); 266 merges; first merge 2026-07-16 | M | ✔ | timeline/strategy |
| 3 | Busiest day | 2026-09-23, 249 commits | M | ✔ | timeline |
| 4 | Ported Zig scope vs Rust (tokei code lines) | 302,669 Zig → 507,628 Rust (1.68×), over 44 crates + xtask | M | ✔ | codebase |
| 5 | VT engine | 116,461 Zig → 174,265 Rust (1.50×) | M | ✔ | codebase |
| 6 | Rust physical lines | 33,040 (06-21) → 836,905 tracked non-vendor (10-08); 857,867 counting all .rs incl. .port harnesses | M | ✔ | timeline/codebase |
| 7 | vt in 4 days | 6,996 → 135,431 LOC; 113 → 2,032 tests (06-21 → 06-25) | M | ✔ | timeline |
| 8 | Tests | 7,270 `#[test]` vs 4,423 Zig string/anonymous test blocks (4,487 incl. 64 identifier-named decltests); per-module gate 211 modules, 6,042 ≥ 4,172, 0 below | M | ✔ / ✎ (Zig scope) | codebase / r2-data-fixes |
| 9 | Reference contract | 3,234 Zig tests (Phase 0), passed within week 1 (3,005 by 06-28) | D | ✔ | timeline |
| 10 | Crate graph | 61 packages (44 crates + xtask + 16 -sys), 313 edges, depth 18, 138 crates.io deps; 43 of 60 crate dirs born day 1 (29 + 14 -sys); none deleted | M | ✔ | codebase / r2-data-fixes |
| 11 | Ghostty SCC | one 345-file cycle, 14 breaks | D | ✔ | codebase |
| 12 | Swift app | reused byte-identical (35,733 physical lines = Ghostty's at 4749c4e93); now 26,843 vs 26,742 tokei code | M | ✔ | timeline/codebase |
| 13 | Milestones | Metal pixels day 19 (07-09), runnable macOS day 22 (07-12), 14/14 perf day 26 (07-16), GTK first light day 27 (07-17, 3dad6d7a2), **live terminal in a GTK window 07-18 (9bc62e574)**, first pin advance day 41, parity-closure merged day 100 (09-28, one low-severity item open) | M | ✔ | timeline / r2-platforms |
| 14 | Bun post → realignment | Post 07-08; six new rules + LIFETIMES on 07-10 (day 20); 452 of 2,968 commits predate it (15.2%) | M | ✔ | strategy/bun |
| 15 | LIFETIMES ledger | 216 rows at creation → **314** data rows (225 / 234 / 253 intermediate, not 226 / 235 / 254); 35 first-parent commits; 18 classes (BORROW_FIELD 68, BORROW_PARAM 49, OWNED 34, FFI 31, SHARED 27, INTRUSIVE 25…) | M | ✎ | r2-data-fixes |
| 16 | LIFETIMES audit | 20 findings (0 high, 2 medium, 18 low); 2 medium = latent aliasing UB in xev MPSC (dead branch), fixed dcae8d61e, Miri-proven | D | ✔ | bun-article |
| 17 | First full vt Miri sweep | 2,037 tests; 1,948 clean; 9-test aliasing UB in search; fixed same day (~7 h) | D | ✔ | bugs/strategy |
| 18 | Regression detectors | 8 Bun-derived on 07-12 (11 detector entries by October); the 7th slot intentionally absent (covered by the unsafe census) | M | ✔ | strategy / r2-glossary |
| 19 | Gate registry | 11 → 12 → 69 → **101 entries, all distinct ids (75 HARD / 26 PROGRESS); 24 self-tests (23 HARD, 1 PROGRESS); the other 77 = 63 gates + 11 detectors + 3 ABI checks** | M | ✎ | r2-data-fixes / r2-glossary |
| 20 | Self-testing gate scripts | 10 of 12 → 57 of 90 scripts | M | ✔ | strategy |
| 21 | Workflow scripts | 212 `wf-*.js`, 15,576 lines, ~691 agent call sites; 155 before 07-10, 56 after; none committed after 08-25 | M | ✔ | strategy |
| 22 | Pin advances | 7 (4749c4e93 → a60e9e2a5); 1,157 upstream non-merge commits; +185,221/−33,790 | M/D | ✔ | timeline/strategy |
| 23 | Ghostty growth during port | 273,827 → 366,687 .zig lines (+33.9%) | M | ✔ | timeline |
| 24 | Perf, July | 0/14 (+42.0%) on 07-15 → 14/14, suite 2,897 → 2,269 ms (−21.7%), geomean 0.758 on 07-16, 29 perf commits in 01:18-10:41 | M | ✔ | perf |
| 25 | Perf highlights | t-parser 2.85×, screen-clone 2.42× faster than Zig ReleaseFast | M | ✔ | perf |
| 26 | Perf breadth | 23/33 vs Ghostty main 4540d499a, geomean 0.750 (1.33×), suite −15.2% | M | ✔ | perf |
| 27 | Perf at pin 12752b2ac | 14/14 twice, −27.9%; thinnest terminal-stream 0.984 | D | ✔ | perf |
| 28 | LTO | t-parser 322 → 531 ms (+65%), so it is banned | M | ✔ | perf |
| 29 | Memory | page allocs 6 = 6; RSS lower in 7 of 8 configs; −30% peak page bytes only at the old pin | D | ✔ + caveat | perf |
| 30 | Binary size | bench 0.73×, gen 1.09×; **native libvt dylib 2,108,032 B at HEAD = 1.095× Ghostty's 1,924,784 B (oracle measured at older pin 12752b2ac)**; was 2,498,112 before size work, 2,071,760 (1.08×) at the 10-07 re-base | M | ✎ | r2-data-fixes |
| 31 | Defect ledger | 205 entries; 135 live (60 crash, 19 UB, 14 leak, 9 hang, 33 intent); 59 latent, 11 test-weakness | M | ✔ | bugs |
| 32 | Whose defects | 172 Ghostty, 33 deps (libvaxis 17, libxev 8, Zig std 3, Highway 2, GStreamer 1, GTK 1, zf 1) | M | ✔ | bugs |
| 33 | Shipped fixes list | 141 entries in docs/ghostty-bug-fixes.md (publishable, author-approved) | M | ✔ | bugs |
| 34 | Upstream status | 0 reported; 4 fixed independently upstream (3 by Ghostty, 1 by Highway 1.4.0), 5 partly; 1 obsolete; 2 private | M | ✔ | bugs |
| 35 | ReleaseFast impact | 38 of 60 crash-class are UB in ReleaseFast; ~13 wrong value; ~9 still crash | E | ✔ | bugs |
| 36 | Discovery method (hand pass, 96 hand-read) | reading/review 92 (code reading 25, defect sweep 24, parity audit 23, while translating 11, adversarial reviewer 6, re-audit 2, security review 1); checked arithmetic/casts/bounds 30 (29 crash); cross-platform bring-up 25 (6 crash); compiling unanalysed code 17 (no crashes); fault injection 12; test port 11; type checker "forced the question" 5 (4 UB); build-system port 4; live testing 4; sanitizers 3; withheld 2; **Miri 0, fuzz 0** as first finder | E | ✎ (±1 noise in buckets not hand-read) | r2-data-fixes |
| 37 | Sanitizer-found defects | 3 (Valgrind in font loading, LSan in ImGui Metal, TSan in Highway) | D | ✔ | bugs |
| 38 | Port findings ledger | 335 entries; 20 "port soundness" | M | ✔ | bugs |
| 39 | Port memory-safety events | 38 events, 81 defects, 62 by Miri, 29 in production (a floor) | M | ✔ | bugs |
| 40 | Miri harness crates | 33 | M | ✔ | bugs/codebase |
| 41 | Miri lanes (Phase 7) | 38 crates 13,354 s (vt 3,104 tests, 9,525 s); debug-assertions-off 124/124 (1,110 s); Tree Borrows green after 8 test-side UB; nextest 6.9× faster (11,722 → 1,687 s) | D | ✔ | bugs / r2-data-fixes |
| 42 | Sanitizer matrix (late Sept-10-07, host named) | ASan 9,850 (macOS); LSan 9,850 (macOS) / 10,111 (Linux VM, 10-02); MSan 6,183 (Linux VM, 10-02); **HWASan 9,987 (Linux VM only, aarch64, 10-02)**; TSan 4,380 (macOS) / **4,428 (Linux VM, 10-07, 20.75 min)**; Valgrind 6,614 in 914 processes (Linux VM, 10-07); LSan on a live GTK surface (Linux VM, 10-07) | D | ✎ (host split added) | bugs / r2-platforms |
| 43 | unsafe census | 22,310 sites / 836,905 physical lines = 26.66 per kLOC; 27.02 (07-10) → 28.15 → 27.87 (08-25) → 26.66 | M | ✔ | codebase/bugs |
| 44 | unsafe in tests | ~58% (heuristic; 52% without the file-name rule) | E | ✔ | codebase |
| 45 | Comment fidelity | 34,345 comments, 29 crates, 0 missing; 79 not carried (0.23%) | D | ✔ | codebase |
| 46 | C ABI | 35/35 headers, 208/208 fns, 79/79 sentinels; embedding 91/91 | M | ✔ | codebase |
| 47 | Vendored C/C++ | 1,058,459 lines over 16 -sys crates vs 4,100 lines of Rust glue | M | ✔ | codebase |
| 48 | Dependency ports | libxev 17,239 → 30,372; libvaxis 14,206 → 13,862; z2d 19,812 → 9,907 (subset, 0 unsafe, version drift); zf 664 → 1,423 | M | ✔ | codebase |
| 49 | Tooling | 33,072 lines in .port/tools | M | ✔ | codebase |
| 50 | Codex audit funnel | 659 registry rows (D); 190 severity-bearing (P1 12 / P2 27 / P3 64 / P4 87, D) plus the 20 run-1 findings, parsed to 179 rows + 19 folded duplicates → 147 groups (P1 7 / P2 20 / P3 51 / P4 69) → 144 confirmed, 3 refuted → 140 remediated and verified (wave 1: 131, wave 2: 9), 3 deferred, 1 no change; Claude agreed with Codex's severity on 144 of 147 | M/D | ✎ (190 does not "become" 179+19 arithmetically) | r2-codex-audit |
| 51 | Cost recovered (API-equivalent) | $11,032 (Mac Claude $7,957, Codex $2,351, Linux VM Claude $724), lower bound | M | ✔ | usage / r2-cost |
| 52 | Cost, API-equivalent total | **~$23.8k central ($19.8k-$31.4k)**; lost Mac Claude window (06-21 → 09-07) ~$12.8k ($8.7k-$20.3k) by maxed-subscription × 50%; cross-checks $6.8k (commit-based) and $17.3k (511 prompts × $33.9) | E | ✎ (method replaced) | r2-cost |
| 53 | Token mix | cache reads 96.5% of 20.43B tokens = 55.0% of cost; output 0.3% of tokens = 12.7% of cost | M | ✔ | usage |
| 54 | Delegated spend | 75.6% by subagents and workflows | M | ✔ | usage |
| 55 | Unit costs (09-08 → 10-08) | $5.67 per commit; $33.9 per prompt; $18.1 per 1k code lines | M | ✔ | usage |
| 56 | Human prompts | **898 Rustty prompts (Mac 733, Linux VM 165) + 13 origin prompts; 740 typed, 158 bare slash commands; 62 active days; median 46 chars (typed 60, p90 197)** vs 112,324 recovered API calls | M | ✎ (890 was a 12:50 snapshot) | r2-human-story |
| 57 | Commits per prompt by era | round 1: 3.9 / 1.7 / 5.8. Finer eras: A 3.6, B 1.6, C (VM) 0.7, D 3.0, E 9.1, F 2.4; prompts per active day 8.9 / 29.4 / 35.8 / 10.2 / 8.2 / 17.0 | M | ✔ | usage / r2-human-story |
| 58 | Concurrency | 391 agent-thread hours vs 196 busy hours (~2.0×); meaningful peak 7.4× on 09-08 (452 threads) | M | ✔ | usage |
| 59 | Context per call | median 77k → 204k; p90 0.4-0.5M; max ~999k | M | ✔ | usage |
| 60 | Sept orchestration | 1,142 subagents, 214 worktrees; 88.6% of calls by subagents | M | ✔ | strategy |
| 61 | Security-review sessions | 1,073 (977 Opus 4.7 xhigh + 96 Opus 5.5 medium), $528 | M | ✔ | usage |
| 62 | Defect ledger growth | 66 entries on 09-11 (retroactive sweep) → 205 | M | ✔ | strategy |
| 63 | Bun reference | 11 days; ~64 Claudes peak; 6,502 non-merge commits (6,778 in their table); ~$165k; 19 regressions | D/M | ✔ | bun-article |
| 64 | Origin | 06-20 21:42 PT clone prompt → 06-21 00:27 "Lets call our port rustty, write these prompts to a md file" → first commit 15c1a9bd7 06-21 15:36 PT (17 h 54 min); prompt kit 31 KB, written by Claude; master prompt at kit line 144 | M | ✔ | r2-human-story |
| 65 | Prior interest | 0 relevant prompts about porting Ghostty / Zig→Rust / Bun before 06-20; a Ghostty theme question on 04-12 shows the author was a user | M | ✎ ("0 relevant", one false positive) | r2-human-story |
| 66 | Quiet stretches | 15 zero-commit runs (55 PT days); 7 bracketed by explicit limit language (+1 "focus tokens"), 1 is the Codex audit; limit language in 20 Rustty prompts on 15 days | M | ✔ | r2-human-story |
| 67 | 08-01 → 08-09 | Codex audit (read-only by instruction) then other projects: 3 Rustty vs 152 other-project prompts; ISO W32 0/152, W33 17/181, W34 16/189 Rustty/other prompts | M | ✔ | r2-human-story |
| 68 | Human engaged time | ~79 h (prompt clusters, 30-min gap; ~59 h at 15 min) | E | ✔ | r2-human-story |
| 69 | Autonomy | of 2,718 non-merge commits, **52% landed >1 h after the last Rustty prompt, 22% >4 h, 9% >8 h** (conservative; 53.7 / 23.1 / 10.7% if the 06-21 origin prompt counts); longest unattended run 180 commits (09-23 19:00 → 09-24 06:33) | M | ✎ (use conservative) | r2-human-story |
| 70 | Time of day | prompts 00-06 h 10% vs commits 18%; weekend prompts 23.3% vs commits 11.7%; peak prompt cell Thu 08 h, peak commit cell Tue 20 h | M | ✔ | r2-human-story |
| 71 | Slash commands | /resume 55, /model 42, /btw 33, /effort 21, /clear 13, /compact 9, /goal 6; "ultracode" 9× (last 09-01) | M | ✔ | r2-human-story |
| 72 | Codex audit runs | 2 runs, 07-31 18:51 → 08-02 14:05 UTC (~43 h). Run 1 gpt-5.6-sol high (4,533 calls) + gpt-5.6-terra (42); run 2 gpt-5.6-sol kickoff (391) then gpt-5.5, ~9.8k calls at high then ~13.2k at medium after the author's "use medium effort" prompt; plus 382 auto-review calls at low | M | ✎ (kickoff model; effort split) | r2-codex-audit |
| 73 | Codex orchestration | /goal loop: 203 automatic continuations vs **10** human messages; 745 spawns, 78 refused at the thread limit, 667 workers (449 primary + 248 replays + 23 admission + 25 other), peak 6 concurrent; waves up to W100 | M | ✎ (10, not 7) | r2-codex-audit |
| 74 | Codex audit universe and stop | 11,150 files / 3,103,589 lines (Rustty 2.59M, Ghostty 0.52M); weekly limit 8% → 99% during run 2; strict ledger credit 10,929 lines (0.35%, Codex's own figure) | M/D | ✔ (0.35% unverifiable) | r2-codex-audit |
| 75 | Codex audit cost | 28,377 calls, 902 threads, 2.97B tokens (2.82B cache read), $2,347 API-equivalent; ~$22 per weekly-meter percentage point | M/E | ✔ | r2-codex-audit |
| 76 | Codex remediation | 8120dd10d (08-11 22:03 PDT): 532 files, +87,270/−5,513, 4,660 tests pass, 336 proving tests, Windows type-check restored (was 25 errors); fuzz target for an audit finding found a reachable P1 RGB::parse panic; the remediation's own verifier caught a signing flag its change activated | M/D | ✎ (signing is not an audit miss) | r2-codex-audit |
| 77 | Other Codex use | Website rebrand 06-28 → 07-05 ($240, 2,161 calls); Linux VM setup 07-18 ($3.00); build questions 07-15 ($1.12) | M | ✔ (12 roots, not 10) | r2-codex-audit |
| 78 | Plans | Claude Max 20x (snapshot; assumed whole port); ChatGPT Plus, Pro 07-18 → 08-18 (covers 99.8% of Rustty Codex usage) | D/M | ✔ | r2-cost |
| 79 | Rustty share of maxed Claude weeks | 53.9% ($6,539 of $12,132, complete quota weeks 09-08 → 10-06); 55.5% from 09-07 by day | M | ✔ | r2-cost / r2-human-story |
| 80 | Ceiling of a maxed week (API-equivalent, all projects) | $1,974 (floor, limit + 4 idle days) / $2,769 / $4,262; mean of 4 weeks $3,033; weekly reset Tue 3 pm PT | M | ✔ | r2-cost |
| 81 | Subscription paid, attributable | **~$420 central** (Claude $200 × 3.61 mo × 50% = $361 + Codex Pro $200 × 30.5% = $61); range ~$420-$600 (billing-cycle and Codex-50% variants); extra-usage credits unknown | E | ✎ ($400 cycle variant wrong) | r2-cost |
| 82 | Leverage | ~56× API-equivalent per dollar paid (≈40-74× with consistent bounds); a maxed week = 43-92× the plan's weekly price | E | ✎ (range) | r2-cost |
| 83 | Commit model timeline (trailers) | Opus 4.8 740 (+7 co-signed with Fable 5), Fable 5 124, Opus 5 615, Opus 5.5 993, Sonnet 5.5 25, no trailer 198; 2,702 non-merge on HEAD; no Codex/GPT trailers | M | ✔ | r2-cost |
| 84 | Linux VM commits | VM #1: 77 commits 07-16 20:59 → 07-18 02:13 PDT (unpushed, no 3D); second VM: ~75 commits 07-18 → 07-22 PDT, 37 hash-confirmed; none after 07-23; later Linux work driven from the Mac over ssh | M | ✔ | r2-platforms |
| 85 | Linux VM spec | Fedora 44 Workstation aarch64, kernel 6.19, 8 vCPU, ~10 GB, GTK 4.22.4, libadwaita 1.9.2, virtio-gpu with software GL (llvmpipe) | M | ✔ | r2-platforms |
| 86 | Platform: macOS | builds; app runs; XCTest 404/0; menu parity 51 actions; full Miri + most sanitizer evidence (10-08) | D | ✔ | r2-platforms |
| 87 | Platform: Linux GTK | builds + runs in a VM; 6,748 / 0 workspace tests with GTK/X11/Wayland in 431 s (10-08, unmerged record); 18/18 live GTK tests; 232 GTK-gated tests; Flatpak built and run (08-26); GlobalShortcuts portal, KWin, mutter checks; never on real GPU or x86_64 | M/D | ✔ | r2-platforms |
| 88 | Platform: Windows | workspace build + clippy (MSVC, x64 emulation on ARM64; 3 -sys crates excluded); targeted tests (xev 42-45, termio 65, vaxis 157, vt kitty 393 + mem 4, font-shaper 133 on GNU); C example byte-identical; no app, as in Ghostty at the pin; no full test run | D | ✔ | r2-platforms |
| 89 | Platform: FreeBSD | ported; type-checked from macOS only; never built natively or run | D | ✔ | r2-platforms |
| 90 | Platform: WASM | freestanding build in default CI; browser gate byte-identical (headless SwiftShader and Chrome on ANGLE Metal); run in Node; `rustty_vt_c.wasm` 1,385,595 B (no Ghostty wasm measured); wasi_poll never executed | M/D | ✔ | r2-platforms / r2-data-fixes |
| 91 | Linux-only defects | Phase 6: 3 failures macOS cannot see (all 25 C/C++ examples unbuildable on Linux, a Linux-only lint failure, a false release-check RED), fixed 09-08 | D | ✔ | r2-platforms |
| 92 | Test skips | 6,325 of 7,270 have no skip mechanism; 133 plain `#[ignore]` (101 headed GTK/display/D-Bus); 475 Miri-only ignores (128 Miri + debug-assertions); 136 ignored only by a platform/feature cfg_attr (152 incl. 16 that also have a Miri ignore); 259 cfg-gated; 0 silent skips (185 removed 09-23) | M | ✎ (152; 101) | r2-data-fixes |
| 93 | Zig skips / dropped tests | 199 of 4,423 Zig test blocks can SkipZigTest (201 of 4,487 incl. decltests; a floor); 4,277 of 4,277 live oracle tests have a counterpart or written reason, 0 unexplained | M | ✔ | r2-data-fixes |
| 94 | Build times | cold libvt release ~30 s (27-32 s over two runs) on the M4 Pro, wasm ~11-14 s; fresh-clone cross-host: Linux VM workspace + GTK 3m11s, Windows MSVC workspace 2m37s, macOS workspace 49.6 s (cache state unknown); full CI never timed | M/D | ✎ (range) | r2-data-fixes |
| 95 | Verification lane durations | Miri 13,354 s; ASan ~15 min; LSan 33 min Mac / 20 min Linux; MSan 50 min; HWASan 56-77 min; TSan 38 / 21 min; Valgrind 13,451 s | D | ✔ | r2-data-fixes |

**Do not quote:** "0 open" parity, "LIFETIMES 315", "226/235/254", "unsafe 22,311", "27 harnesses",
"90/90", "fixed next day", "955 call sites", "17 -sys crates", "8.5× peak", "$17k", "$17.8k",
"4 fixed by Ghostty", "2 sanitizer-found", "86 gate IDs", "22 -SELF", "136 cfg_attr(miri, ignore)",
"52 zero-commit days", "7 human prompts" (Codex), "13 limit signals", "$400 by 4 billing cycles",
"GTK not yet live", "Codex (GPT-5.5)" alone, "audit landed ~08-11", "205 fixed", or "from 2.9×
slower" without the same-session caveat (§4). No internal ids in prose.

**Privacy rule.** Never publish raw prompt history. Four prompts contain VM credentials; the 07-16
20:11 and Codex VM-setup prompts must not be quoted past the IP. Data files redact passwords, IPs and
emails. Do not quote the 07-15 18:42 prompt about Ghostty's AGENTS.md or the "joke comments" asides
(tone rule). Decide whether to quote "less capable" about Codex (the 144/147 figure makes the point
without it).

---

## 3. Visualization inventory

Readiness: **R** = data ready; **W** = needs work (stated); **B** = blocked (needs author input or new
data collection). Chart ids below are internal and collide with other id families (A3-A5 ABI checks,
N### port findings, V## verification debt); never print them, and consider renaming to C1..Cn.

### 3a. Analogues of Bun's five widgets and tables

| # | Title | Section | Chart type | Data file(s) | Ready |
|---|---|---|---|---|---|
| A1 | Stats card (Bun's opening table) | §1 | stat tiles plus a comparison row against Bun | §2 table; `bun-article-metrics.csv` | R |
| A2 | Commit heatmap, days × 24 h, with phase annotations, the 07-10 pivot, prompt dots and quiet-run bands | §5 / §14 | calendar heatmap, log scale | `r2-human-story-day-hour-commits-prompts.csv` (111 days × 24 h), `r2-human-story-quiet-stretches.csv`; Bun: `bun-article-commits-per-hour.csv` | **R** |
| A3 | Adversarial review replay: implementer → reviewer critique → fix commit for 3 real bugs | §12 | animated chat/diff stepper | candidates 10f2091, 40727a2cb, 9e3fe1c25 | **W**: hand-curate; reviewer text in gitignored ledgers |
| A4 | Gate convergence swimlanes per host | §13 | swimlane strip | `bun-article-ci-race.csv` (shape only); `r2-platforms-rvm-lanes-2026-10-08.csv` (one Linux snapshot) | **B/W**: no gate-run log; reconstruct or drop |
| A5 | Per-crate commit replay | §5 | animated stacked bars | `timeline-loc_by_day_long.csv`, `codebase-growth-weekly-per-crate.csv` | R |
| A6 | Outcome tables: perf, memory, binary size, tests | §8 / §1 | paired bars / tables | `perf-alloc-memory.csv`, `perf-binary-size.csv`, `r2-libvt-sizes.csv`, `bun-article-tables.csv` | R (RSS has only 4 of 8 configs) |
| A7 | Unsafe density vs Bun | §16 | dot/line with Bun band | `bun-article-unsafe-compare.csv`, `codebase-unsafe-census-timeseries.csv` + HEAD point | **W**: append HEAD point; label basis |
| A8 | LIFETIMES class distribution | §6 / §7 | horizontal bar | `codebase-lifetimes-classes.csv` | R (sums to 314) |

### 3b. New Rustty-specific charts

| # | Title | Section | Chart type | Data file | Ready |
|---|---|---|---|---|---|
| N1 | Journey gantt with key-commit ticks | §2 / §5 | gantt | `timeline-eras.json` | R (era counts are E) |
| N2 | Commit cadence with era bands | §5 | daily bars + cumulative line | `timeline-commits_by_day.csv` | R |
| N3 | Milestone timeline | §5 | annotated timeline | `timeline-firsts.csv` + `r2-platforms-gtk-linux-timeline.csv` | **W**: reword "0 open"; add GTK live window 07-18 and the two-VM span |
| N4 | Rust LOC by crate + flat Swift line | §5 | stacked area | `timeline-loc_by_day_long.csv` | R |
| N5 | Crate birth swimlane | §4 | dot + bar | `r2-crate-birth.csv` (60 rows) | **R** |
| N6 | 345-file SCC → acyclic crate DAG | §4 | layered node-link (custom SVG) | `codebase-crate-graph.json` | R |
| N7 | Zig vs Rust per module | §4 / §7 | dumbbell | `codebase-zig-vs-rust-per-module.csv` | R |
| N8 | Where the Rust lives | §4 | treemap | `codebase-treemap.json` | R |
| N9 | Every Zig test has a counterpart | §10 | log-log scatter | `codebase-test-counts-per-module.csv` | R |
| N10 | Test growth vs 3,234 contract | §3 / §5 | step line | `timeline-tests_by_week.csv` | R |
| N11 | Strategy timeline pre-Bun / post-Bun / post-never-port-a-bug | §6 | annotated timeline | `strategy-timeline.csv`, `bun-article-adoption-map.csv` | R |
| N12 | Bun idea → Rustty artifact | §6 | mapping table / Sankey | `strategy-bun-adoptions.csv` | R |
| N13 | Workflow-script anatomy by era | §14 | grouped bar | `strategy-workflow-patterns-by-era.csv` | R (E) |
| N14 | Orchestration modes | §14 | multi-strip | `strategy-workflow-scripts-by-date.csv`, `timeline-commits_by_week.csv` | R |
| N15 | LIFETIMES ledger growth 216 → 314 | §6 | step line | `r2-lifetimes-rows.csv` (first-parent, blank lines excluded) | **R** |
| N16 | Gates over time + self-test share | §10 | step lines | `strategy-gate-registry-growth.csv`, `strategy-gate-selftest-growth.csv`, `r2-gate-registry.csv` | R |
| N17 | Gate scripts added by era × category | §10 | stacked bar | `strategy-gate-scripts-added.csv` | R (E) |
| N18 | Port memory-safety bugs on the timeline | §7 | dot/strip | `bugs-port-memory-safety-timeline.csv` | R (floor) |
| N19 | Speed-up per benchmark (14/14) | §8 | sorted horizontal bar | `perf-ratio-r5-vs-zig-4749c4e93.csv` | R |
| N20 | 0/14 → 14/14 dumbbell | §8 | dumbbell, log axis | `perf-history-by-stage.csv` | R |
| N21 | Scoreboard by round | §8 | stacked W/T/L + suite line | `perf-scoreboard-by-stage.csv` | R |
| N22 | Optimization waterfalls | §8 | small multiples | `perf-waterfall-key-benches.csv` | R |
| N23 | Why LTO is banned | §8 | grouped bars | `perf-lto-profile-experiment.csv` | R |
| N24 | Comptime flattening gap closure | §8 | slope chart | `perf-comptime-flattening-steps.csv`, `perf-new7-comptime-fixes.csv` | R |
| N25 | 33-config matrix | §8 | log-log scatter | `perf-full33-4540d499a.csv` | R |
| N26 | Perf at advanced pin | §8 | bar + step | `perf-ratio-pin-12752b2ac-idle.csv`, `perf-n511-apc-hyperlink.csv` | R |
| N27 | Perf-gap hazard classes / falsified optimizations | §8 | tables | `perf-gap-classes.csv`, `perf-negative-results.csv` | R |
| N28 | Upstream absorbed per pin advance | §9 | paired bars + lines | `timeline-upstream_pins.csv`, `strategy-oracle-pin-advances.csv` | R |
| N29 | Ghostty defects recorded over time | §11 | cumulative step | `bugs-upstream-cumulative.csv` | R (recording dates) |
| N30 | Where defects live (area × class) | §11 | heatmap | `bugs-upstream-area-x-class.csv` | R |
| N31 | How porting finds bugs (method × class) | §11 | stacked bar | `r2-bugs-upstream-method-x-class.csv` (+ `-detail-x-class`) | **R** (hand pass; 23 medium confidence; mark Miri 0 and fuzz 0) |
| N32 | ReleaseFast consequence of crash-class bugs | §11 | waffle | `bugs-upstream-crash-releasefast.csv` | R (E) |
| N33 | Whose code: Ghostty vs deps | §11 | bar | `bugs-upstream-project.csv` | R |
| N34 | Upstream status | §11 | 100% bar | `bugs-upstream-status.csv` | **W**: split the 4 fixes 3 Ghostty / 1 Highway (disclosure no longer blocks) |
| N35 | Reachability × class | §11 | stacked bar | `bugs-upstream-reachability-x-class.csv` | R (E) |
| N36 | Verification matrix | §13 | table/bar | `bugs-verification-gates.csv` + `r2-platforms-matrix.csv` | **W**: add HWASan and Linux TSan rows, host column (CSV's TSan row is macOS) |
| N37 | Unsafe per crate + inside vt | §16 | stacked bars | `codebase-unsafe-per-crate.csv`, `codebase-vt-unsafe-by-area.csv` | R (per code kLOC: label) |
| N38 | Comment fidelity / C-ABI parity pills | §10 | bar + pills | `codebase-comment-coverage-g3b.csv`, `codebase-c-api.csv` | **W**: embedding 90 → 91 |
| N39 | Same C libs, thin Rust glue | §4 | proportional bars | `codebase-third-party-sys.csv` | R |
| N40 | Language mix before/after | §4 | stacked bars | `codebase-language-mix.csv` | R |
| N41 | Data coverage + prompts per day | §15 | stacked bar + lost-window band | `usage-stats-coverage-daily.csv` | R |
| N42 | Weekly cost: recovered vs extrapolated, low-high whiskers, lost-window shading | §15 | stacked + hatched | `r2-cost-weekly-chart.csv` (VM double-count already removed) | **R** |
| N43 | Model timeline from trailers | §14 | streamgraph | `r2-cost-model-weekly.csv`, `r2-cost-model-commits.csv` | **R** |
| N44 | Tokens vs dollars | §15 | two 100% bars | `usage-stats-token-mix.csv` | R |
| N45 | Main vs subagent vs workflow spend | §14 / §15 | 100% bar | `usage-stats-agent-share*.csv` | R |
| N46 | Agent concurrency | §14 | dumbbell + ratio | `usage-stats-activity-daily.csv` | **W**: suppress small-denominator days |
| N47 | Context size per call | §14 | median + p90 band | `usage-stats-context-size-weekly.csv` | R |
| N48 | Eras compared | §14 | small multiples | `usage-stats-eras.csv` | **W**: records ratio with/without patches |
| N49 | Human-in-loop heatmaps (prompts and commits twins) | §14 | weekday × hour | `r2-human-story-heatmap-prompts-weekday-hour.csv` (898), `r2-human-story-heatmap-commits-weekday-hour.csv` | **R** (author approved) |
| N50 | Line churn per day | §9 / appendix | diverging bars | `timeline-churn_by_day.csv` | R |
| N51 | Commit-keyword heat strip | §14 | heatmap | `strategy-commit-keywords-by-week.csv` | R |
| C1 | Autonomy histogram (commit lag since last prompt) | §14 | histogram | `r2-human-story-commit-lag-since-prompt.csv` | R (use the conservative variant) |
| C2 | Prompt style by era | §14 | small multiples | `r2-human-story-prompts-eras.csv`, `r2-human-story-prompt-phrasing-by-era.csv` | R (regex E; hand-check a sample) |
| C3 | Rustty share of the author's prompts and spend | §15 | line / 100% bar | `r2-human-story-prompts-daily.csv`, `r2-cost-quota-weeks.csv` | R |
| C4 | Subscription ceiling per quota week | §15 | bars + ceiling band | `r2-cost-quota-weeks.csv` | R |
| C5 | Usage-limit timeline with the author's words | §15 | annotated timeline | `r2-cost-limit-events.csv`, `r2-human-story-limit-mentions.csv` | R (quote events, not a count) |
| C6 | Codex audit funnel | §12 | funnel | `r2-codex-audit-funnel.csv` | R (659 and 190 are D) |
| C7 | Codex weekly-limit burn-down with wave markers | §12 | line | `r2-codex-weekly-limit.csv`, `r2-codex-audit-timeline.csv` | R |
| C8 | Platform matrix | §13 / §17 | table | `r2-platforms-matrix.csv` | R |
| C9 | GTK/Linux timeline (two VMs, hands-on window, later ssh verification) | §13 | annotated timeline | `r2-platforms-gtk-linux-timeline.csv`, `r2-platforms-rvm-commit-attribution.csv` | R |
| C10 | Test-skip census vs Zig | §10 | stacked bar | `r2-skip-summary.csv`, `r2-zig-skip-tests.csv` | R |

Suggested cut for publication: about 20 to 25 charts. Keep A1-A3, A5, A7, A8, N1, N4, N6, N9, N12,
N15, N19, N20, N22, N28, N29, N31, N36, N42, N45, C1, C6, C8. Push the rest into expandable panels.

---

## 4. Contradictions between dossiers and how to resolve them

Closed in round 2 (kept for the record): LIFETIMES 315 vs 314 (314; intermediate points 225/234/253);
gates "-SELF 24 vs 22" and "86 IDs" (101 entries, 24 self-tests; 22/86 were a regex that dropped
A5-MASK-SELF and the detector/ABI ids); cost $17k vs $17.8k (both superseded by the subscription
method, §2 row 52); model share week 07-06 (trailer re-parse: Opus 4.8 leads); N42 double-count (only
~75 VM commits 07-18 → 07-22 overlap recovered cost; VM #1's 77 commits stay in the lost-window
estimate); GTK "first light" vs AGENTS.md "not yet live" (the AGENTS.md line is a stale 07-15 note
written before any Linux work); Codex "landed ~08-11" (audit 07-31 → 08-02; remediation 08-10 → 08-11
PDT); "136 cfg_attr(miri, ignore)" (475; round 1 counted single-line attributes).

| Topic | Values seen | Resolution |
|---|---|---|
| Commit total | 2,968 (HEAD) vs 2,994 (all refs) vs 2,728 / 2,718 / 2,702 (non-merge, by ref set and bot filter) | Quote HEAD 2,968 with its date; footnote non-merge and its scope. Recompute on publish day. |
| Zero-commit days | 52 (timeline) vs 55 (PT, all refs) vs 60 (UTC) | **55, Pacific time**. The 52 is not explained by timezone; drop it. |
| First merge | "0 before 07-13" vs 07-16 | **07-16** (d211a84a6). |
| Rust size | 857,867 / 836,905 / 818,802 physical vs 507,628 tokei code | Tokei code lines for Zig/Rust comparisons; physical tracked non-vendor for growth and unsafe density. Label axes. |
| Swift size | 35,733 / 38,180 physical vs 26,843 code | Physical for "byte-identical"; code for comparison. |
| Tests | 7,284 vs 7,270 vs 7,204; Zig 4,423 vs 4,487 | Quote 7,270 vs 4,423 (string/anonymous blocks); footnote 4,487 incl. decltests. |
| unsafe | 22,310 vs 22,311; units; trend; Bun basis | **22,310**; label units; "about 27 ± 1"; quote Bun's ~4% without a ratio. |
| Upstream commits | 1,716 vs 1,718 | Quote **1,157 non-merge**. |
| Miri harnesses / search fix / -sys / embedding / sanitizer UDs / upstream fixes / NEW-FINDINGS / parity closure | 27 vs 33; next day vs same; 17 vs 16; 90 vs 91; 2 vs 3; 4 Ghostty; 334 vs 335; 0 open | 33; same day; 16; 91; 3; 3 Ghostty + 1 Highway; 335; "merged with one item open". |
| $/prompt, concurrency, context max, call sites | 34.2/33.9; 8.5×/7.4×; 967k/999k; 955/691 | $33.9; 7.4×; ~999k; ~691. |
| Perf "2.9× slower" | first head-to-head vs same-session | "+42% suite / 0 of 14" for the start; same-session per bench. |
| Alloc −30% | old pin only | Caveat or quote "6 = 6 allocs, RSS lower 7 of 8". |
| Records/docs ratio | with vs without patches | Show patch-free, or both. |
| Bun commits | 6,502 vs 6,778 | Quote the table figure; note heatmap sums 6,502 non-merge. |
| Human prompts | 890 (usage snapshot) vs 898 (full history) | **898**; 890 was a 12:50 PT snapshot. |
| Autonomy shares | 53.7/23.1/10.7% vs 52.2/21.6/9.2% | Use the conservative **52 / 22 / 9%** (the day-1 commits followed an unrecorded launch prompt). |
| Codex human prompts | 7 vs 10 | **10** typed messages in the three root rollouts. |
| Codex effort split | 8,209 high / 14,820 medium (CSV) vs ~9.8k / ~13.2k (rollout turn_context) | Say "about 9.8k at high, then about 13.2k at medium" or give CSV numbers with the caveat. |
| Codex funnel | "190 become 179 + 19" | 179 + 19 = 198 > 190; say "190 severity rows plus 20 run-1 findings, parsed to 179 rows + 19 duplicates". |
| Subscription paid | $361 / $400 / $500 / $600 | Central ~$420; range ~$420-$600; the "4 cycles" variant is wrong (5 cycles touched). |
| Leverage range | 47-63× vs 40-74× | Central **~56×**; range ≈40-74× with consistent bounds. |
| Hardware | "Apple M4 Pro" (records, sysctl) vs "MacBook Pro M4" (author) | Consistent: a MacBook Pro with an **Apple M4 Pro**. Print "MacBook Pro (Apple M4 Pro, 48 GB)". |
| Sanitizers at newest pin | Phase 7 lanes GREEN vs the newest-pin run (10-08) with MSan, TSan, HWASan provisionally RED (uninstrumented libEGL; Oniguruma init + Mesa races, cause not established; host libz.a in an archive check after 10,182 passing tests) | Date the matrix to Phase 7 (late Sept-10-07). Re-check the newest-pin run before publishing; it is on an unmerged branch and still running. |
| Commit attribution to VMs | 37 hash-confirmed vs ~75 by activity window vs 77 VM #1 by time window | Quote "77 on the first VM, about 75 on the second (37 confirmed by hash)". |

---

## 5. Open questions for the author, and remaining research gaps

### Author questions: answered

1. **Disclosure.** Answered: publish fully (classes, counts, mechanisms, the fixes list); not live
   yet. *Residual check:* the agent-memory rule says no trigger recipe for the 2 privately
   reportable entries. Confirm at publish time whether that still holds, since 0 of 205 are reported
   upstream.
2. **Origin.** Answered: "seeing what was possible". Exploration in `~/projects` 06-20 21:42 → 06-21
   00:27 PT; Claude wrote the prompt kit from the author's 23:08 spec; the author launched the master
   prompt; agents wrote the Phase 0 plan; ~18 h clone to first commit (§2 row 64).
3. **Gaps.** Answered: 7 of 15 quiet runs are bracketed by token, usage or session-limit prompts; the
   rest are other projects (`r2-human-story-quiet-stretches.csv`). 08-01 → 08-09 was the Codex app audit
   (read-only by instruction; 99% of the Codex weekly limit on 08-02), then other projects. Codex
   models, prompts and cost are answered (§2 rows 72-75). Only the Claude-side vetting/remediation cost
   (08-10 → 08-12) is lost.
4. **Lost history.** Answered in part: prompt *text* survives for the whole lost window (676 Rustty
   prompts before 09-08); only telemetry is lost. Style: workflow scripts via "ultracode" or "dynamic
   workflows", "continue"/"status" steering, frequent /model and /effort switches (arguments not
   logged). Model names appear in prompts (Opus, Fable, Opus 4.8, Opus 5). All via Claude Code.
5. **Cost framing.** Answered: subscription; lost window extrapolated at maxed-out × 50%;
   API-equivalent as a reference (`r2-cost.md` §5(b)). Compare with Bun API-equivalent to
   API-equivalent only.
6. **Personal data.** Answered: prompt heatmap, counts and costs OK; rule and prompt quotes OK without
   internal ids. Privacy rule in §2 still applies.
7. **Public repo.** Answered: public at publication; link hashes.
8. **Platform claims.** Answered by `r2-platforms.md` §2 and §7 (§2 rows 84-91, chart C8).
11. **Hardware.** Answered: MacBook Pro, Apple M4 Pro (Mac16,7), 14 cores, 48 GB, macOS 26.5.1 today.

### Author questions: still open

9. **Daily driving.** Optional per the author. The Linux VM shows no interactive launches after 07-31;
   the 09-17 embedding in another app is a usable anecdote. Is Rustty the author's daily terminal?
10. **Title, byline and tone toward Ghostty.** Needs attribution and credit for Mitchell Hashimoto.
    Should a Ghostty maintainer see the draft first (relevant now that all defects are published)?
12. **New:** confirm the Claude tier history (Max 20x for the whole port, or upgraded from 5x?), the
    extra-usage credits bought, and the prices as billed (assumed $200 Max 20x, $200 Pro).
13. **New:** confirm the 3 quotable rule texts that exist only in agent memory (the Rust `&mut`
    over-constraint note, the never-port-a-bug trigger quote, the parity-only ruling).
14. **New:** quote "less capable" about Codex, or let 144/147 make the point?

### Remaining research gaps

- Newest-pin sanitizer run (MSan, TSan, HWASan provisionally RED; full CI still running on an unmerged
  branch). Final status at the current reference commit unknown.
- No perf run at the current reference commit. Either run it or date the perf section.
- Gate-run history for the A4 swimlanes (reconstruct from records or drop).
- Reviewer-critique text for A3 (gitignored ledgers).
- Full 8-config RSS table missing.
- No Ghostty wasm size to compare with Rustty's 1,385,595 B (building it would write into the
  read-only Ghostty checkout); the dylib ratio uses an oracle size from an older pin.
- No clean full-workspace Mac build timing; full CI never timed; no macOS runtime ignored count.
- Regenerate `bugs-upstream-by-week-method.csv` with the round-2 labels; 109 defect entries keep
  round-1 keyword labels.
- Miri-ignore scope: 475 (scan) vs 468 (exceptions ledger) not reconciled.
- Linux never run on a real GPU or x86_64 (rests on absence of records).
- VM #1 has no transcript; its 77 commits are attributed by time window and the handoff document.
  38 of the ~75 second-VM commits rest on a 15-minute activity window.
- Pre-09-08 /model and /effort arguments are not logged; large pasted prompts survive only as hashes;
  the 06-21 master-prompt launch is not in history.
- Spend-share evidence for "count half" covers only 09-07 → 10-08; 198 untrailered commits
  (07-10 → 09-01) remain unattributed (not Codex by timing).
- Claude-side cost of the Codex vet and remediation is lost (~400 agent runs, ESTIMATED).
- Engaged hours and phrase-family shares are heuristics; hand-check before quoting precise values.
- Fuzzing contribution beyond the RGB::parse find is unquantified.
- Pre-never-port-a-bug U-ids were not reconciled against the defect ledger.

---

## 6. Website integration summary

- **Stack.** Next.js 16.1.6 App Router, React 19.2.4, @next/mdx; fully static (SSG, 106 routes);
  no CSP; `largePageDataBytes` is 2 MiB, so keep chart JSON small. It is dark-only and uses CSS
  Modules tokens: copper `#d97934` = Rust, teal `#64d8c6` = Zig/upstream, gold `#ffbc7a` secondary,
  atom-one green and red for pass and fail. Fonts are Pretendard and JetBrains Mono, the latter for
  numerals (the analogue of Bun's Martian Mono).
- **Route.** `src/app/journey/{page.tsx, content.mdx, JourneyPage.module.css, data/*.json}`, with
  charts in `src/components/journey-charts/*`.
- **Home link.** Add a third neutral `ButtonLink` to the `buttonsList` in
  `src/app/HomeContent.tsx:156`. The home page has no navbar or footer. Optionally add a
  `navLinks` entry in `layout.tsx`. Keep the footer attribution.
- **TOC.** Reuse Sidecar + JumplinkHeader. A small build-time heading extractor is needed for a
  non-docs MDX page.
- **Visualizations.** Hand-rolled inline-SVG React client components in Bun's style: dark cards,
  tabular numerals, hover tooltips, and an optional rAF replay that honours
  `prefers-reduced-motion`. Reuse `animated-terminal/playback.ts` for the steppers (A3, A5). Do not
  rely on transitive d3. If scales are wanted, add `d3-scale` and `d3-shape` explicitly at pinned
  versions. Otherwise use plain math. Mermaid is monochrome and only suits simple structure
  figures; N6 needs custom SVG.
- **Data pipeline.** A script (e.g. `scripts/journey-data.mjs` or Python in the rustty repo)
  converts the chosen `data/*.csv` into trimmed JSON committed under `src/app/journey/data/`.
  Numbers in the prose should come from one `facts.json`, so corrections propagate. Commit hashes
  link to the public repo.
- **Gates.** `npm run lint` (biome, with doc comments required on all functions and top-level
  constants), `npm run build` and `npm run verify:animation`. CI runs alejandra + biome. Print CSS
  inverts the grays, so check charts in print.
- **Codex did the website rebrand** (06-28 → 07-05, branch `codex/rustty-website-rebrand`); worth a
  line if the website is mentioned.

---

## 7. Completeness critic: what a technical reader will expect that is still missing

1. **Side-by-side code.** Zig → Rust pairs for pointer aliasing, comptime → const generics,
   `break :label` vs `continue`, @intCast → checked conversion, container_of. Not curated yet.
2. **The actual rules and prompts.** Partly closed: 12 curated verbatim rule quotes + 5 reserves in
   `r2-glossary.md`, and a prompt quote bank in `data/r2-human-story-quotable-prompts.csv`. Still to
   pick: a LIFETIMES.tsv row example and a trimmed orchestration script.
3. **"0 tests skipped" analogue.** **Closed** (§2 rows 92-93, `r2-skip-summary.csv`).
4. **End-user performance.** Startup time, input latency, throughput, app RSS, renderer FPS. Still
   missing; all perf data is library micro-benchmarks.
5. **Build experience.** Mostly closed (§2 row 94, `r2-build-times.csv`); clean full Mac build and
   full CI timing still unrecorded.
6. **Correctness beyond unit tests.** esctest runs on Linux, the WASM browser gate and the fuzz find
   exist; vttest/golden-render conformance and real-world usage are not summarised.
7. **Failure stories.** Data exists: the disk filling at ~7 worktrees, squashed eras, reverted
   optimizations, the gate keying bug that fenced terminal-stream 57× loose, "derived green was
   bookkeeping", the 185 silent skips, the Codex "every line" goal stopping at 0.35%, VM #1's unpushed
   77 commits. §10 and §17 own them now.
8. **Human effort.** **Partly closed**: 898 prompts, ~79 engaged hours (E), autonomy shares, time of
   day, and user rulings over time (`r2-human-story.md` §3c). Keyboard telemetry does not exist.
9. **Methodology and reproducibility.** Hardware closed; benchmark harness flags, oracle build and
   per-number commands still to be written up.
10. **Platform matrix.** **Closed** (`r2-platforms-matrix.csv`, chart C8).
11. **Licensing and attribution.** License of the port vs Ghostty (MIT), vendored libs, the CC0
    Ferris icon. Not drafted.
12. **Upstream relationship.** Whether Ghostty maintainers know about the port, and plans to report
    the 205 defects. Ties to open question 10.
13. **What it would take to repeat.** Lessons list. Not drafted.
14. **Per-pin perf continuity.** Perf at the current pin is missing (§5).
