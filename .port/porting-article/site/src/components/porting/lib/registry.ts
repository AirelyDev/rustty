import type { CardMinHeight } from "../ui/InstrumentCard";

/** Static description of one porting widget: chrome text, sizing and data contract. */
export type WidgetEntry = {
  slug: string;
  title: string;
  label: string;
  source: string;
  /** Permalink the source pill opens: the widget's research dataset or primary record. */
  sourceHref: string;
  caption: string;
  /**
   * Visible line under the title: what the widget adds beyond the prose and
   * how to read it. It never repeats a number its section's prose states.
   */
  deck?: string;
  /** Text alternative of the widget, rendered for screen readers (and as the visible line when deck is absent). */
  summary: string;
  minHeight: CardMinHeight;
  flagship: boolean;
  frame: "card" | "terminal";
  dataFile: string | null;
  requiredKeys: string[];
};

/**
 * All widgets used by the porting article, in article order.
 *
 * minHeight values are rendered card heights at 1100 px and 390 px,
 * re-measured on the finished page (2026-10-09) with the reservation
 * disabled, at mount time; `reduced` holds the heights under
 * prefers-reduced-motion where a replay mounts at a taller final state.
 */
export const WIDGETS: readonly WidgetEntry[] = [
  {
    slug: "stats-card",
    title: "The port in numbers",
    label: "AT A GLANCE",
    source: "fact table · git · tokei · test census · perf records",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/refresh-2026-10-10.md#L18",
    caption:
      "Tiles summarise the whole port, 2026-06-21 to 2026-10-10. Line counts are tokei code lines; test counts are Rust #[test] functions in every crate (including the ported libxev, libvaxis and z2d and the -sys wrappers) and Zig test blocks in Ghostty's src/ at the current reference commit; matched module by module with the dependencies, as in the translation lens, it is 7,221 against 4,911. Every value comes from the article's fact table; hover, tap or focus a tile for each figure's provenance and as-of commit. Cost and usage figures stop at 2026-10-08. The API-equivalent figure is list price for the same usage, a reference value, not money spent.",
    summary:
      "In 112 days, 302,922 lines of Zig became 508,388 lines of Rust across 3,085 commits. The Rust side has 7,287 tests across all crates, the ported dependencies included, against 4,433 Zig test blocks in Ghostty's src/. It beat Ghostty's release build on 14 of 14 benchmarks at the current reference commit and recorded 206 defects in Ghostty and its dependencies along the way.",
    deck: "Each tile is one headline figure of the port. Hover, tap or focus a tile for its source, its provenance and the commit or date it was measured at.",
    minHeight: { desktop: 503, phone: 962 },
    flagship: false,
    frame: "card",
    dataFile: "stats-card.json",
    requiredKeys: ["tiles"],
  },
  {
    slug: "test-contract",
    title: "The test contract",
    label: "ZIG TESTS AS THE SPEC",
    source: "git log · per-module #[test] counts",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-test-contract-rust-per-module-daily-2026-10-10.csv",
    caption:
      "One square per Zig test, coloured by the day its module's Rust test count reached it. Counts from 110 daily snapshots of the Rustty repo against the Ghostty reference commit of the day.",
    summary:
      "86% of the first-day contract was covered within a week. Each reference move grew the contract; in total the Zig suite grew from 3,189 to 4,433 string-named and anonymous test blocks.",
    deck: "Each square is one Zig test, turning copper on the day its module's Rust test count reached it; play or scrub to watch the contract fill and grow. Below: Rust against Zig tests per module, the contract at each reference move and the census of skipped tests.",
    minHeight: { desktop: 2920, phone: 5863 },
    flagship: true,
    frame: "card",
    dataFile: "test-contract.json",
    requiredKeys: ["dates", "revs", "rust", "pins", "mods", "skip"],
  },
  {
    slug: "cutting-the-cycle",
    title: "Cutting the cycle",
    label: "FROM ONE IMPORT CYCLE TO A CRATE GRAPH",
    source: "Ghostty src/ at 4749c4e93 · cargo metadata",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-cutting-the-cycle-data.json",
    caption:
      "Stages 1 and 2: runtime @import edges among the 512 Zig files of Ghostty's src/ at the first reference commit (2026-06-20), excluding imports inside test blocks; the cycle is found with Tarjan's algorithm. Files are grouped by directory and coloured by the crate tier they became. Stage 3: the Cargo workspace graph, node area by code lines. Stage 4: daily Rust lines per crate from each crate's first commit.",
    summary:
      "At the first reference commit, 345 of Ghostty's 512 source files formed a single import cycle with 1,410 edges inside it. The port broke it with 14 planned cuts, and every one is load-bearing: skip any one and Cargo sees a cycle. The result is 61 packages with a longest dependency chain of 18 hops.",
    deck: "The tabs step from the cycle among Ghostty's files, coloured by the crate tier each file became, through the planned cuts to the crate graph and a replay of each crate's growth. Click or press Enter on a file or crate for its details, or trace a real cycle file by file.",
    minHeight: { desktop: 1115, phone: 2296 },
    flagship: true,
    frame: "card",
    dataFile: "cutting-the-cycle.json",
    requiredKeys: ["zig", "crates", "replay", "text"],
  },
  {
    slug: "agents-vs-human",
    title: "Agents vs human: 111 days, hour by hour",
    label: "COMMITS AND PROMPTS BY PACIFIC HOUR",
    source: "git log --all · prompt history · usage transcripts",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-agents-vs-human-grid-2026-10-10.csv",
    caption:
      "Each row is one Pacific day (06-20 to 10-08 14:00), each column one hour. Cells show non-merge commits on all branches (bots excluded), lines changed (vendored code excluded) or prompts, on a log scale; teal dots mark the author's prompts to the port, and hollow rings the 11 prompts of the founding sessions typed before the repository existed. Model is the Co-Authored-By trailer of each commit. API-equivalent cost is recovered per-call cost at list price for the port's sessions, a reference value, not money spent; hatched rows fall where Mac Claude transcripts were lost (06-21 to 09-07), so a blank there means not recorded, not zero. Threads are distinct agent threads in recovered transcripts. Lag is the time from the author's last port prompt to each commit; the 41 first-day commits followed an unrecorded launch prompt. Commit subjects in tooltips have internal task labels removed.",
    summary:
      "52% of 2,718 non-merge commits landed more than an hour after the author's last prompt, and 9% more than eight hours after. Agents committed most at 20:00; the author prompted most at 08:00. About 79 hours of hands-on prompting drove the port, roughly 34 commits per hands-on hour, and the longest unattended run was 180 commits overnight.",
    deck: "Days run down and Pacific hours across; switch the cells between commits, lines changed and prompts, brush to zoom, or replay day by day. Agents committed most at 20:00, while the author's prompts peaked at 08:00.",
    minHeight: { desktop: 1675, phone: 2650 },
    flagship: true,
    frame: "card",
    dataFile: "agents-vs-human.json",
    requiredKeys: [
      "grid",
      "commits",
      "prompts",
      "daily",
      "humanEras",
      "events",
    ],
  },
  {
    slug: "strategy-timeline",
    title: "How the method changed",
    label: "METHOD CHANGES OVER TIME",
    source: "git log · method records · pointer-lifetime ledger",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/strategy-timeline-2026-10-10.csv",
    caption:
      "Each marker is a dated change to how the port was run, placed at the commit that introduced it; hover, tap or focus it for the change and its commit. Era bands give commits per day. The ledger strip counts data rows in the pointer-lifetime ledger at each first-parent commit that changed it (last value per day); the registry strip counts check-registry entries at weekly snapshots.",
    summary:
      "On day 20 (07-10) the port adopted six new working rules and a pointer-lifetime ledger, which grew from 216 rows at creation to 314. An unsafe census started the same day and eight regression detectors two days later. On 09-11 the port added the rule to never carry a Ghostty bug into Rust; from then to 10-10 it averaged 45.9 commits a day, against 20.0 in the era before.",
    deck: "Each marker sits at the commit that changed how the port was run; hover, tap or focus one for the change. The era bands give the commit rate, and the strips beneath show the pointer-lifetime ledger and the check registry filling in.",
    minHeight: { desktop: 743, phone: 3064 },
    flagship: false,
    frame: "card",
    dataFile: "strategy-timeline.json",
    requiredKeys: ["lanes", "eras", "events", "trigger", "ledger", "registry"],
  },
  {
    slug: "translation-lens",
    title: "Translation lens: Zig ↔ Rust",
    label: "TRANSLATION LENS · ZIG ↔ RUST",
    source: "git grep · Ghostty a60e9e2a5 · Rustty 1d22ba80d",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-translation-lens.json",
    caption:
      "Excerpts are real code at Ghostty's reference commit and Rustty's head, side by side, with hand-drawn line mappings. Carried comments are found by exact text match. Occurrence counts are git grep over Ghostty's src/ (plus libxev where marked) and Rustty's crates/, tests included. The unsafe classification is a same-line heuristic and labelled as such.",
    summary:
      "Six Zig patterns and what they became in Rust: aliased pointer arguments, intrusive links, comptime parameters, packed structs, errdefer and byte slices. Overall the Rust code has 1.68× the Zig code lines, 2.56× the comment lines and 1.47× the tests. In the excerpts, 62 of 73 Zig comment lines were carried word for word.",
    deck: "Pick a pattern to see Zig beside its Rust port, with lines mapped across, carried comments marked and the evidence for the Rust shape. Across the whole code base, the port has 2.56× Ghostty's comment lines for 1.68× its code lines.",
    minHeight: { desktop: 2391, phone: 5254 },
    flagship: true,
    frame: "card",
    dataFile: "translation-lens.json",
    requiredKeys: [
      "commits",
      "patterns",
      "ledgerClasses",
      "modules",
      "unsafeCrates",
      "unsafeHeur",
    ],
  },
  {
    slug: "three-reviewers",
    title: "Three reviewers: implementer, adversarial reviewer, Miri",
    label: "PORT MEMORY-SAFETY BUGS, REPLAYED",
    source: "git · Miri runs · review and audit records",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-three-reviewers-bugs.json",
    caption:
      "Rail: the 37 curated events (80 defects) recorded up to 2026-10-08, test-isolation races excluded, laned by the tool that found each one. Code is shown at the commits named in each stage; where a first pass was never committed, the Miri proof-harness model is shown and labelled. Miri output was regenerated on 2026-10-08 by running the command in each terminal title against the parent commit of the fix (bugs A and C) or on the in-repo proof harness's first-pass model (bugs B and D), with nightly Miri of 2026-07-22, so tag numbers come from those runs. Reviewer text is quoted verbatim, with internal identifiers cut as […].",
    summary:
      "Four real memory-safety bugs from the port, each a different aliasing mechanism, replayed through the code, the reviewer, Miri's borrow stack and the fix. Miri, or an audit that Miri then proved, found 62 of the 80 port memory-safety defects recorded up to 8 October. In one case the reviewer saw the hazard but proposed a cure that was still undefined behaviour; only the proof harness showed it.",
    deck: "Choose a bug and step through its life: the ported code, the reviewer's verdict, Miri's borrow stack at the bad access under either aliasing model, and the fix. The rail above places each bug among every recorded port memory-safety event, by the tool that found it.",
    minHeight: { desktop: 1521, phone: 2982 },
    flagship: true,
    frame: "terminal",
    dataFile: "three-reviewers.json",
    requiredKeys: ["head", "lanes", "timeline", "bugs"],
  },
  {
    slug: "memory-safety",
    title: "Memory-safety bugs the port introduced, and what caught them",
    label: "PORT-INTRODUCED MEMORY-SAFETY BUGS",
    source: "memory-safety timeline · git · port-findings ledger",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-09-memory-safety-events-2026-10-10.csv",
    caption:
      "Plotted: 39 hand-curated events from commits and the port-findings ledger. Before September, port bugs were tracked only in commit messages and squashed merges, so this is a floor. Rows group the detecting tool: Miri over each crate's tests, isolated Miri proof harnesses, the Tree Borrows lane, sanitizers, review and symptoms. Bubble area is defects per event; the cumulative curve counts defects.",
    summary:
      "The port introduced at least 83 memory-safety defects in 39 events, and Miri caught 64 of them. 62 were aliasing or provenance undefined behaviour, a bug class that exists because Rust has an aliasing model and Zig does not. Sanitizers found 7: leaks, a dropped Zig field default and a C library initialisation race, none of them aliasing.",
    deck: "Each bubble is one event, sized by its defects and laned by what caught it, with the working rules the bugs produced flagged along the timeline. Regroup the rows by crate area, filter from the crate × bug-class table, or select an event for its detail.",
    minHeight: { desktop: 1284, phone: 2097 },
    flagship: false,
    frame: "card",
    dataFile: "memory-safety.json",
    requiredKeys: ["events", "rules"],
  },
  {
    slug: "perf-replay",
    title: "From 0 of 14 to 14 of 14 in one day",
    label: "RUSTTY ÷ GHOSTTY, SAME SESSION",
    source: "perf log · raw benchmark sweeps · git log",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-05-perf-replay-data.json",
    caption:
      "Plotted: per-benchmark median of 7 timed runs (1 warm-up discarded) for each Rustty build, divided by the median of Ghostty's benchmark tool (Zig ReleaseFast) measured in the same session. Log ratio axis; bars span the run-to-run range. Win, tie or loss uses a ±1.5% tie band. Step times are commit times of the code measured. Hardware: MacBook Pro, Apple M4 Pro, 14 cores, 48 GB.",
    summary:
      "On 07-15 Rustty lost all 14 benchmarks against Ghostty's release build in the same session, with the suite 42.0% slower. By the morning of 07-16, after 29 performance commits, it won 14 of 14: 2,269 ms against Ghostty's 2,897 ms, 21.7% faster. The terminal parser ended 2.85× faster and screen cloning 2.42× faster. At the current reference commit, on 10-10, it won 14 of 14 again, with the suite 27.4% faster.",
    deck: "Each dot is one benchmark's Rustty time over Ghostty's on a log scale, with its run-to-run range; step through the night one measured build at a time. Further down are the link-time-optimisation detour, the optimisations that measured worse, and memory and binary size.",
    minHeight: {
      desktop: 3644,
      phone: 6736,
      reduced: { desktop: 3819, phone: 6939 },
    },
    flagship: true,
    frame: "terminal",
    dataFile: "perf-replay.json",
    requiredKeys: ["bench", "stages", "commits", "waterfall", "pin"],
  },
  {
    slug: "moving-target",
    title: "Chasing a moving target",
    label: "UPSTREAM ABSORBED PER REFERENCE MOVE",
    source: "Ghostty git · upstream catch-up records",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-06-moving-target-areas-2026-10-10.csv",
    caption:
      "Plotted: Ghostty .zig line counts at the first-parent commit of each week and at each reference commit; Rustty .rs line counts per day (raw lines, not code lines, so the two are only roughly comparable). Matrix: end-to-end diff of each reference range grouped by top-level directory, and non-merge commits touching each directory. Wait is the upstream commit time of the new reference to the time of Rustty's commit that adopted it.",
    summary:
      "The port moved its Ghostty reference commit 8 times, absorbing 1,169 upstream non-merge commits while Ghostty's Zig code grew 34.0%. The wait between an upstream commit and the port adopting it was 47.6 hours at the first move, as little as 1.7 hours, and 7.7 hours at the last; the median was 7.55 hours. 43% of the upstream churn landed in the terminal engine.",
    deck: "The size chart sets Ghostty's growth against the port's, with each reference move marked, and the matrix shows where each move's upstream range landed, as lines changed or commits. Select a move to link the chart, the matrix and its detail panel.",
    minHeight: { desktop: 2014, phone: 2963 },
    flagship: false,
    frame: "card",
    dataFile: "moving-target.json",
    requiredKeys: ["adv", "gh", "rs", "rows"],
  },
  {
    slug: "checks-growth",
    title: "How the checks grew",
    label: "CHECK REGISTRY GROWTH",
    source: "check registry · comment and C API census",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/strategy-gate-registry-growth-2026-10-10.csv",
    caption:
      "Step lines count entries in the port's check registry at weekly snapshots, with the self-tests among them, and the check scripts that can test themselves. The waffle shows today's registry by kind: filled entries must pass, outlined ones are tracked toward a target. A distinctive comment is a phrase or sentence of five or more words in Ghostty's source; the C interface pills compare Rustty's headers and exports with Ghostty's at the reference commit.",
    summary:
      "The check registry grew from 11 entries to 105: 79 hard and 26 progress checks, including 27 self-tests. At the end, 34,371 Zig comments were carried into the Rust code with none missing. The C API matched 35 of 35 headers and 208 of 208 functions, and the embedding API 91 of 91 entry points.",
    deck: "Step lines trace the registry week by week, with its self-tests and the check scripts that can test themselves. In the waffle, filled squares must pass and outlined ones track progress toward a target.",
    minHeight: { desktop: 668, phone: 1272 },
    flagship: false,
    frame: "card",
    dataFile: "checks-growth.json",
    requiredKeys: [
      "registry",
      "scripts",
      "final",
      "composition",
      "comments",
      "capi",
    ],
  },
  {
    slug: "defect-map",
    title: "Defects on the map of Ghostty",
    label: "UPSTREAM DEFECTS FOUND BY PORTING",
    source: "upstream-defect ledger · Ghostty 246f70287",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/58aa62a81e7db5f8762b0158acc661dafc31f230/.port/parity-closure/UPSTREAM-DEFECTS.md",
    caption:
      "Plotted: every entry of the port's upstream-defect ledger (206), placed at the first file path in its reference. Tile area is Zig lines at Ghostty's reference commit; children under 3% of their directory fold into other files, and tiles holding a defect get a minimum area so their marks fit. Dependency defects and non-Zig build files sit in the strip below the map, not to scale. Class comes from each entry's kind (a mixed leak/crash entry counts as a crash). Colour and the class chips give the class (hollow: latent; dashed: test weakness; centre dot: hang); mark shape gives the discovery method. Discovery method (a hand pass over the ledger) and the ReleaseFast consequence (read from each entry's ReleaseFast field) are estimated classifications. Commit = the Rustty commit that recorded the entry, normally the one that shipped the fix.",
    summary:
      "Porting surfaced 206 defects: 173 in Ghostty and 33 in its dependencies. Of the 60 crash-class entries, 37 are undefined behaviour (estimated) in Ghostty's ReleaseFast build. Checked arithmetic found 29 crashes, and reading and review found the most defects overall (93). Upstream had already fixed 4 independently, confirming they are real: 3 in Ghostty and 1 in Highway, not yet in Ghostty's pinned version.",
    deck: "Ghostty's source tree as a map sized by Zig lines, with one mark per defect at its file: colour gives the class and shape how it was found. Filter by either, and click a mark for the defect, its ReleaseFast consequence and the Rustty commit.",
    minHeight: { desktop: 1879, phone: 2865 },
    flagship: false,
    frame: "card",
    dataFile: "defect-map.json",
    requiredKeys: ["tree", "defs", "reported"],
  },
  {
    slug: "audit-funnel",
    title: "A second opinion from Codex",
    label: "CODEX AUDIT FUNNEL",
    source: "audit registry · Codex rate-limit log · git",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/r2-codex-audit-funnel.csv",
    caption:
      "Funnel stages count audit findings from the registry rows Codex wrote to the fixes verified in the remediation commit; the first count is as Codex reported it, the rest are counted from the vetting state. The meter is the median of the weekly-limit readings Codex logged in each hour (UTC) of its two audit runs, 07-31 to 08-02, with the band spanning every reading in the hour and the audit's events marked; the readings of 08-01 00:00 to 03:59 are unexplained and annotated.",
    summary:
      "Codex audited the port read-only over about 43 hours in two runs. Its 659 registry rows reduced to 147 distinct findings: 144 confirmed and 3 refuted, and 140 were remediated and verified. The weekly usage meter went from 8% to 99% during the second run.",
    deck: "The funnel narrows Codex's raw rows to findings that were confirmed, fixed and verified. The meter beneath tracks its weekly usage limit hour by hour across both runs, with the audit's events marked; hover, tap or focus a reading for its value.",
    minHeight: { desktop: 603, phone: 1055 },
    flagship: false,
    frame: "card",
    dataFile: "audit-funnel.json",
    requiredKeys: ["stages", "severity", "meter", "events", "runs"],
  },
  {
    slug: "verification-matrix",
    title: "Every lane at every reference commit",
    label: "HOSTS × TOOLS × REFERENCE COMMITS",
    source: "port records · verification logs · Linux VM runs",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/verification-lanes-2026-10-10.csv",
    caption:
      'Each cell is a recorded run: test count, wall time and date as written in the port\'s records, or the commit that recorded it. A blank cell means no run is recorded for that reference commit, not that the lane failed. "Not available" means the tool does not exist for that host (MemorySanitizer on macOS, HWAddressSanitizer outside aarch64 Linux).',
    summary:
      "Taking each lane at the latest reference commit it completed at, 58,767 test executions ran under a sanitizer or Valgrind on macOS and a Linux VM: the macOS lanes and Valgrind at the current reference commit, the Linux sanitizers at the one before. Miri was the matrix's bug finder: Stacked Borrows lanes found 18 port bugs and Tree Borrows 15, against 10 for all sanitizer and test lanes combined. The matrix was first complete at the sixth of eight reference moves.",
    deck: "Pick a reference commit to see every host and tool lane at it, then a cell for its run; a blank cell means no recorded run, not a failure. The lane history below follows each lane across reference commits.",
    minHeight: { desktop: 1862, phone: 3271 },
    flagship: false,
    frame: "card",
    dataFile: "verification-matrix.json",
    requiredKeys: ["pins", "hosts", "tools", "runs", "hero"],
  },
  {
    slug: "platform-table",
    title: "What runs where",
    label: "PLATFORM STATUS",
    source: "build and test records · Linux, Windows and FreeBSD VMs",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/f153b70146edeaf792db0b65469dedbc792e9816/.port/porting-article/research/data/r2-platforms-matrix-2026-10-11b.csv",
    caption:
      "One row per platform: what builds, what runs, what was tested by hand, which verification lanes ran, and the date of the latest evidence, then the claim the article makes. Hover, tap or focus a cell for its evidence. Linux was tested in a Fedora 44 aarch64 virtual machine with software rendering, never on a real GPU or x86_64.",
    summary:
      "macOS is the primary platform: the app builds and runs, with the full Miri and most sanitizer evidence. Linux GTK builds and runs in a virtual machine and passes 18 of 18 live GTK tests. Windows builds and passes the full workspace test suite but, as in Ghostty, has no app. FreeBSD builds and runs the GTK app in an arm64 virtual machine, where its tests pass after the fixes from that run, except three DMA-BUF tests that need a GPU driver the VM lacks (as of a later commit, 110adad44). The WebAssembly build runs in browsers, Node and wasmtime, with three example pages matching Ghostty's output (as of a later commit, 3a59e7edc).",
    deck: "Read across a row for what builds, runs and was tested on that platform, and how recent its evidence is. Hover, tap or focus a cell for the record behind it.",
    minHeight: { desktop: 770, phone: 2338 },
    flagship: false,
    frame: "card",
    dataFile: "platform-table.json",
    requiredKeys: ["columns", "rows"],
  },
  {
    slug: "orchestration",
    title: "From workflow scripts to subagents",
    label: "MODELS, CONTEXT AND AGENTS",
    source: "Claude Code and Codex transcripts · git trailers",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/usage-stats-agent-share-weekly-2026-10-10.csv",
    caption:
      "Top: non-merge commits per week, to 2026-10-10, by the model named in each commit's Co-Authored-By trailer, stacked around a centre line. Middle: median context per call by week with the 90th-percentile band, from recovered Claude Code and Codex transcripts to 2026-10-08; Mac transcripts before 09-08 were mostly lost, so earlier weeks are partial. Bottom: workflow scripts by the day they were first committed, and agent threads per day in recovered transcripts (square-root scale).",
    summary:
      "The port first ran through 211 committed hand-written workflow scripts, none after 08-25. In September an orchestrator dispatched 1,142 subagents across 214 worktrees, and subagents made 88.6% of calls. Median context per call grew from 77k to 204k tokens.",
    deck: "Read the three bands as one timeline: which model wrote each week's commits, how much context a call carried, and when hand-written workflow scripts gave way to subagent threads.",
    minHeight: { desktop: 754, phone: 975 },
    flagship: false,
    frame: "card",
    dataFile: "orchestration.json",
    requiredKeys: ["models", "weeks", "ctx", "scripts", "threads", "totals"],
  },
  {
    slug: "every-prompt",
    title: "Sessions and prompts, word for word",
    label: "EVERY HUMAN PROMPT AND SESSION, 20 JUNE TO 11 OCTOBER",
    source: "prompt history · Codex sessions · usage transcripts",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/f153b70146edeaf792db0b65469dedbc792e9816/.port/porting-article/research/data/prompts-redacted-2026-10-11f.json",
    caption:
      "Top: each bar is one session (one Claude Code or Codex session on one machine) from its first to its last prompt, rows packed so overlapping sessions stack, coloured and named by its area and thicker for more prompts; the 44 sessions that hold only Claude Code commands or shell escapes have no area and are hidden until turned on (then thin grey ticks), and diamonds mark the 3 founding sessions. Session titles are Claude Code's own; where it gave none or the author changed one, Claude wrote it (marked title by Claude). Below, each tick is one human prompt to the port or its website (typed prompt, slash command or shell escape), 2026-06-20 to 2026-10-11 in Pacific time, in three lanes: Claude Code on the Mac, Claude Code on the Linux VM and the Codex app; tick height is length on a log scale, and the bars beneath are non-merge commits per day. By default the ticks, the log and the length figures show instructions and decisions; the 187 Claude Code commands and shell escapes are one click away, and the prompt counts always include them. Mac Claude Code transcripts from before 2026-09-08 were purged, so model and effort are unknown there (hatched) unless the Co-Authored-By trailer of a commit linked to the prompt names the model (marked inferred from commits). Text is verbatim except 24 redactions (secrets, logins, email and IP addresses, names, one unrelated project), each shown as [redacted: reason]; pasted text is not published, and for 10 prompts it no longer exists. The 60 prompts of the October sessions about this article are included and tagged article work; the 163 prompts typed in the website folders (114 of them in the Codex app), most of them the late-June and early-July build of the site, carry a website folder badge, and their commits, in the website repo, are not linked. Commits are linked to a prompt only where the session or branch evidence ties them to it unambiguously. The 13 founding prompts (typed outside the repository, most of them before it existed) are included; prompts sent to subagents, automated Codex review sessions and other prompts typed outside the repository and the website folders are not.",
    summary:
      "1,229 human prompts to the port and its website (60 of them about this article) over 114 days: 928 in Claude Code on the Mac, 165 in Claude Code on the Linux VM and 136 in the Codex app. They came in 112 sessions, 44 of them only Claude Code commands or shell escapes; on the busiest days, 2026-07-03 and 2026-07-31, 7 sessions each had an instruction or decision, and the longest session held 168 prompts over 14 days. The median prompt is 50 characters long (all prompts), and 49 prompts only answer a choice or acknowledge. Model and effort are unknown for 505 prompts, most of them typed on the Mac before 2026-09-08, whose transcripts were purged; for 96 more the model is inferred from commits. The text carries 24 redactions in 19 prompts, and 130 prompts are linked to the commits they led to.",
    deck: "Bars are sessions and ticks single prompts, taller for longer text; brush the days, filter by area, lane or kind, or search, then read the matching prompts word for word in the log. The median prompt, counting every kind, is 50 characters long.",
    minHeight: { desktop: 2730, phone: 4740 },
    flagship: true,
    frame: "card",
    dataFile: "every-prompt.json",
    requiredKeys: ["ticks", "weeks", "totals", "lanes"],
  },
  {
    slug: "cost",
    title: "Usage, tokens and the subscription",
    label: "SUBSCRIPTION PAID VS LIST PRICE",
    source: "usage transcripts · prompt history · git trailers",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/mock-cost-data-2026-10-10.json",
    caption:
      "Plotted: API-equivalent list-price cost and billable tokens per call from recovered Claude Code and Codex transcripts (snapshot 2026-10-08). Mac Claude transcripts before 09-08 were purged; for those weeks the port's cost is modeled as a maxed subscription week × 50% port share. Paid figures use public plan prices. The API-equivalent cost is a reference value at list price, not money spent.",
    summary:
      "The port was paid by subscription: about $420 attributable (range $420–$600, plus unknown extra-usage credits), from Claude Max 20x for the whole port and one month of ChatGPT Pro. At API list prices the same usage would be about $23.8k ($19.8k–$31.4k), a reference value, not money spent. Cache reads were 96.5% of tokens and 55% of the list-price cost.",
    deck: "Switch between list-price dollars and tokens, and between Rustty alone and all projects; hollow bars are modeled weeks whose transcripts were lost. Further down, the list price is split by token type, by main thread against subagents and workflow runs, and by era.",
    minHeight: { desktop: 2447, phone: 4473 },
    flagship: true,
    frame: "card",
    dataFile: "cost.json",
    requiredKeys: ["qw", "cov", "ev", "paid", "api", "ceil"],
  },
  {
    slug: "unsafe-density",
    title: "Unsafe code per crate, over time",
    label: "UNSAFE SITES PER 1,000 LINES",
    source: "unsafe census · tokei",
    sourceHref:
      "https://github.com/AirelyDev/rustty/blob/882e3df21ee8c93d9055ffac9abce8385b6e5db6/.port/porting-article/research/data/codebase-unsafe-census-timeseries-2026-10-10.csv",
    caption:
      "Unsafe sites counted by the port's census pattern (unsafe blocks, functions, extern blocks, impls and traits; vendored code excluded) per 1,000 physical lines of tracked Rust at each census commit, ending at head. Crate bars are labelled per 1,000 physical lines of the crate, engine-area bars per 1,000 code lines. The test/non-test split is a heuristic: everything after a file's first test module counts as test code.",
    summary:
      "The port has 22,240 unsafe sites in 838,127 physical lines, 26.54 per thousand. The density stayed nearly flat: 27.02 on 07-10, a peak of 28.15, then 27.87 on 08-25. By a heuristic count, about 58% of the sites are in test code.",
    deck: "The line follows unsafe sites per thousand physical lines at each census; hover, tap or focus a point for its counts. The bars rank crates by density and split the terminal engine by area.",
    minHeight: { desktop: 525, phone: 1339 },
    flagship: false,
    frame: "card",
    dataFile: "unsafe-density.json",
    requiredKeys: ["series", "crates", "vtAreas", "testShare"],
  },
];

/** Returns the registry entry for slug, throwing if it does not exist. */
export function widget(slug: string): WidgetEntry {
  const entry = WIDGETS.find((w) => w.slug === slug);
  if (!entry) throw new Error(`unknown porting widget: ${slug}`);
  return entry;
}
