# Article outline, plain-language version (rewrite of BRIEF.md §1)

Same structure and numbers as BRIEF.md §1 (post-fact-check values), with every internal id
replaced by a description. Glossary and sources: `r2-glossary.md`. Where the author's round-1
answers change framing (cost, disclosure, hardware), the change is noted in *italics*.

The audience is technical. The arc follows the journey in time, and each section ends on a
measurable result. Working title: "Porting Ghostty to Rust", or "Rustty: 110 days of porting
Ghostty to Rust with agents".

1. **TL;DR / stats card.** 110 days, ~2,970 commits, 302k lines of Zig ported to 508k lines of
   Rust code, 7,270 Rust tests against 4,423 Zig tests, faster than Ghostty on all 14
   benchmarks, 205 Ghostty defects recorded (135 reachable ones handled safely in the port), and
   the Miri, sanitizer (ASan, LSan, MSan, TSan) and Valgrind lanes green. Mirrors Bun's opening stats table.

2. **Why, and what "port" means here.** *Motivation: seeing what was possible (author).* Parity
   with one pinned Ghostty commit, with every comment carried over and the C ABI kept
   byte-compatible. The Swift macOS app is reused unchanged rather than ported. Parity is the
   only scope: nothing Ghostty does not have. The one exception: never copy a Ghostty bug.

3. **Starting point: the reference as a contract.** Before any Rust, the 3,234 Zig tests were
   inventoried as a contract, Ghostty's benchmarks and binary sizes were recorded as a
   baseline, and the idea of a pinned reference commit was fixed.

4. **Cutting a 345-file cycle into crates.** Ghostty's `src/` is one import cycle of 345
   files. It was cut at 14 points into 61 Cargo packages with no cycles, 18 levels deep. Key
   layering choices: the font crate sits below the terminal crate and never depends on it; the
   renderer reads configuration through a trait it owns, so it never depends on the config
   crate; process spawning lives in the terminal-I/O crate because placing it in core created a
   cycle. 29 crates were scaffolded on day 1 and ported leaves first.

5. **The greenfield sprint (06-21 → 07-12).** The terminal engine went from 7k to 135k lines
   and from 113 to 2,032 tests in 4 days. Font, renderer and app-runtime tiers followed, using
   stub types whose methods panic until their subsystem (GPU, event loop, inspector) arrived, so
   the surrounding code compiled and was reviewed for real. First Metal pixels on day 19; a
   runnable macOS terminal on day 22, driving Ghostty's unchanged Swift app. Work was fanned
   out to agents by ~155 orchestration scripts on a linear history.

6. **Reading Bun's post: the 07-10 realignment.** Bun published "Bun in Rust" on 07-08. On
   07-10 Rustty adopted six new porting rules (Zig-to-Rust primitive semantics, keeping
   compile-time work compile-time, unsafe accounting, porting markers, split implementer and
   reviewer agents, upstream tracking), a pointer-lifetime ledger (216 pointer fields
   classified retroactively), an unsafe census, and eight automated checks, each aimed at one
   regression class Bun reported. Show which Bun ideas were adopted, adapted or rejected. The
   immediate payoff: the first full Miri run over the terminal engine (2,037 tests) found
   aliasing undefined behaviour in search, fixed about 7 hours later; the lifetime audit found
   20 issues, including latent aliasing UB in the event loop's lock-free queue.

7. **Translating Zig pointers into Rust's aliasing model.** The technical core. A Rust `&mut`
   argument is protected for the whole call, so code that Zig writes with plain pointers became
   raw-pointer arguments. Also: intrusive lists through a per-type "address of the link field"
   trait, recovering a parent struct from a field pointer (Zig's `@fieldParentPtr`, C's
   `container_of`), and a long-lived alias into a field used inside a loop. The method: prove
   each pattern on a small isolated Miri harness (33 of them) rather than by argument. Include
   the timeline of memory-safety bugs the port itself introduced (38 events, 81 defects, 62
   found by Miri). Code: Zig and Rust side by side for 2-3 patterns.

8. **Performance: from 0/14 to 14/14 in one day.** On 07-15 Rust was slower on all 14
   benchmarks (+42% on the suite). On 07-16, 29 performance commits over about 10.5 hours made
   it faster on all 14 (suite −21.7%, parser 2.85×, screen clone 2.42×). Cover why link-time
   optimisation is banned, the parity bug profiling exposed (`continue` vs `break :hyperlink`),
   and the August class of gaps where a Zig compile-time parameter had been ported as a
   runtime value (23 of 33 configurations faster against Ghostty main). The wall-clock trap:
   Rust's process floor is 3 ms lower, so compare per-operation slopes. Memory, binary size, and
   the October re-measure (14/14). *Hardware: Apple M4 Pro, 48 GB.*

9. **Keeping a port in sync: 7 moves of the reference commit.** 1,157 upstream non-merge
   commits (+185k/−34k lines) absorbed. The approach changed to "move the reference first, then
   port until the checks are green again", with the number of failing checks as the progress
   meter; turnaround shrank to the same day. Ghostty's Zig code grew 34% during the port.

10. **Checks as the specification.** The CI check list grew from 11 to 101 entries (75
    blocking, 26 reporting progress); 24 of them test the checkers themselves by injecting a
    violation and requiring a failure. Lesson: a check that derives "green" from a checklist is
    only as true as the checklist, so audit what it rests on. Results: comment fidelity (34,345
    comments, 0 missing), test counts per module (211 modules, all at or above Zig), C ABI
    parity (35/35 headers, 208/208 functions, 91/91 embedding entry points), and the struct-size
    check added after a by-value struct size mismatch crashed the macOS menu shortcuts.

11. **Never port a bug: the defect ledger.** On 09-11 (extended 09-23) the rule changed from
    "port faithfully, report upstream" to "fix in the port, prove it with a test that fails when
    the bug is put back, and record it". 205 entries (135 live: 60 crashes, 19 undefined
    behaviour, 14 leaks, 9 hangs, 33 broken intent), 172 in Ghostty and 33 in dependencies. 38
    of the 60 crashes are undefined behaviour in Ghostty's shipped ReleaseFast build. How
    porting finds bugs: checked integer conversions, compiling code Zig never analyses,
    bringing up other platforms, and sanitizers. Ghostty has since fixed 3 independently and
    Google Highway 1. *Disclosure: author approved publishing categories, counts and
    mechanisms; still no trigger recipe for the two privately reportable crashes.*

12. **Second opinions: the Codex audit and adversarial review.** A Codex audit (run in the
    Codex app) produced 659 rows, grouped into 147: 144 confirmed, 3 refuted, 140 fixed. The
    implementer/reviewer loop shown as a replay of 3 real bugs, and ~1,073 automated
    security-review sessions.

13. **Verification matrix (late September to early October).** Miri over 38 crates (13,354 s),
    Tree Borrows green after 8 test-side fixes, a 6.9× Miri speed-up from running one process
    per test with nextest. ASan, LSan, MSan, TSan and Valgrind test counts. Three Ghostty-side
    defects found by sanitizers (Valgrind in font loading, LSan in the ImGui Metal backend, TSan
    in Google Highway). Cross-host builds green on macOS, Linux (Fedora aarch64 VM), Windows MSVC
    and Windows GNU.

14. **The orchestration evolved.** Orchestration scripts gave way to subagents working in git
    worktrees, each reviewed before merge. In September: 1,142 subagents, 214 worktrees, 88.6% of
    model calls from subagents. Lessons: no more than three worktrees building at once (the disk
    fills), context per call grew (median 77k → 204k tokens), commits per human prompt by period
    (3.9 before the realignment, 1.7 in July-August, 5.8 in September-October), and the model
    timeline. *Author prompted through Claude Code on the Mac and on the Linux VM.*

15. **What it cost.** *Paid by subscription, not API.* Present API-equivalent list price as a
    reference: $11,032 recovered (a lower bound); ~$17.8k estimated total. *For the lost-history
    window, extrapolate assuming the subscription was maxed out and count half toward Rustty
    (time was shared with other projects).* Cache reads are 96.5% of tokens but 55% of cost;
    delegated agents 75.6% of spend; $5.67 per commit. Be explicit about the lost window. Compare
    with Bun's ~$165k only with a scale caveat.

16. **Unsafe, honestly.** 22,310 `unsafe` sites, 26.66 per thousand physical lines, roughly flat
    while the code doubled. About 58% are in tests; density concentrates in the FFI crates.
    Compare with Bun's ~4% with a basis caveat.

17. **What's left / limitations.** One low-severity difference is open (about 35 log lines
    format values differently). The GTK app is not yet live (*check the Linux-VM commits for
    what was tested*), there is no benchmark run at the current reference commit, Windows is
    build-only, and the z2d graphics port is a subset at a different version.

18. **Appendix: methodology.** How each number was measured (git revisions, tokei code lines vs
    physical lines, commit trailer parsing, LiteLLM list prices) and the hardware (Apple M4 Pro,
    48 GB, 14 cores).
