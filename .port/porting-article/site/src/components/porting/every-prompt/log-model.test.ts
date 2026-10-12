/**
 * Unit tests of the "Every prompt" log view-model, run by
 * scripts/verify-porting-helpers.mjs (npm run verify:porting), which passes
 * the imported log-model and model modules in.
 */
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import type * as LogModel from "./log-model";
import type * as Model from "./model";

/** Builds a log row with defaults for the fields a case does not care about. */
function logRow(p: Partial<LogModel.LogRow> & { id: string; ts: string }) {
  return {
    tool: "claude",
    machine: "mac",
    model: null,
    effort: null,
    mode: null,
    settingsSource: "unknown",
    chars: (p.text ?? "x").length,
    kind: "prompt",
    pasted: "none",
    articleWork: false,
    decision: false,
    phase: "founding",
    commits: [],
    commitsBasis: "none",
    text: "x",
    ...p,
  } as LogModel.LogRow;
}

/** FIXTURE_LOG is a six-prompt log over the window 2026-06-21 … 2026-06-30. */
const FIXTURE_LOG = {
  glossary: {
    UD131: "an upstream defect",
    R7: "a porting rule",
    "P3-W0": "phase 3, wave 0",
  },
  rows: [
    // 06-22 05:30 UTC is 06-21 22:30 Pacific: day 0, not day 1.
    logRow({
      id: "claude-mac-a",
      ts: "2026-06-22T05:30:00Z",
      text: "ok",
      decision: true,
    }),
    logRow({
      id: "claude-mac-b",
      ts: "2026-06-23T18:00:00Z",
      text: "Continue with <script>alert(1)</script> and {x} **bold**",
      model: "claude-opus-4-8",
      settingsSource: "commit-trailer",
      pasted: "lost",
    }),
    logRow({
      id: "claude-vm-c",
      ts: "2026-06-25T18:00:00Z",
      machine: "vm",
      kind: "command",
      text: "/compact",
    }),
    logRow({
      id: "codex-mac-d",
      ts: "2026-06-28T18:00:00Z",
      tool: "codex",
      text: "fix UD131 then R7",
      model: "gpt-5.6-sol",
      effort: "medium",
      settingsSource: "transcript",
      phase: "tightened",
    }),
    logRow({
      id: "claude-mac-e",
      ts: "2026-06-30T18:00:00Z",
      text: "  continue\n",
      articleWork: true,
      model: "gpt-5.6-sol",
      effort: "high",
      settingsSource: "transcript",
      phase: "tightened",
    }),
    logRow({
      id: "claude-mac-f",
      ts: "2026-06-30T19:00:00Z",
      text: "continue [redacted: secret]",
      phase: "tightened",
    }),
  ],
};

/** The matching summary's lanes and window start. */
const LANES = ["claude-mac", "claude-vm", "codex"];
const START = "2026-06-21";

/** Runs every log view-model check against the modules lm and m. */
export function runLogModelTests(lm: typeof LogModel, m: typeof Model) {
  const { rows, glossary } = lm.decodeLog(FIXTURE_LOG, START, LANES);
  assert.deepEqual(
    rows.map((r) => [r.i, r.day, r.lane]),
    [
      [0, 0, 0],
      [1, 2, 0],
      [2, 4, 1],
      [3, 7, 2],
      [4, 9, 0],
      [5, 9, 0],
    ],
  );
  assert.equal(rows[0].minute, 22 * 60 + 30);
  assert.equal(lm.fmtPacific("2026-06-22T05:30:00Z"), "2026-06-21 22:30");
  assert.equal(lm.fmtPacific("2026-12-01T08:05:00Z"), "2026-12-01 00:05");
  assert.equal(glossary.R7, "a porting rule");
  assert.throws(() => lm.decodeLog({ rows: "no" }, START, LANES));
  assert.throws(() =>
    lm.decodeLog(
      { glossary: {}, rows: [{ ...FIXTURE_LOG.rows[0], tool: "x" }] },
      START,
      LANES,
    ),
  );

  // The same rows as ticks: filterRows and applyFilter agree on every filter.
  const ticks: Model.Tick[] = rows.map((r) => ({
    i: r.i,
    day: r.day,
    minute: r.minute,
    lane: r.lane,
    log2chars: 1,
    flags: lm.rowFlags(r),
    chars: r.chars,
    words: 1,
    model: r.model,
    effort: r.effort,
    mode: r.mode,
    phase: r.phase,
    session: r.session,
    area: r.area,
  }));
  const def = m.defaultFilter(10, 3);
  const filters: Model.Filter[] = [
    def,
    { ...def, from: 2, to: 7 },
    { ...def, lanes: new Set([0, 2]) },
    { ...def, lanes: new Set() },
    { ...def, decisionsOnly: true },
    { ...def, hideArticle: true },
    { ...def, model: "gpt-5.6-sol" },
    { ...def, model: "unknown" },
    { ...def, effort: "unknown" },
    { ...def, phase: "tightened" },
    { ...def, query: "CONTINUE" },
    { ...def, query: "codex-mac-d" },
    { ...def, query: "continue", hideArticle: true },
    { ...def, query: "nothing like this" },
    { ...def, from: 5, to: 6 },
  ];
  for (const f of filters) {
    const want = m
      .applyFilter(
        ticks,
        f,
        f.query ? (i) => m.matchesQuery(rows[i], f.query as string) : undefined,
      )
      .map((t) => t.i);
    assert.deepEqual(
      lm.filterRows(rows, f).map((r) => r.i),
      want,
      m.encodeFilter(f),
    );
  }
  // Search: case-insensitive on text, exact (case-insensitive) on the id.
  const q = (query: string) =>
    lm.filterRows(rows, { ...def, query }).map((r) => r.id);
  assert.deepEqual(q("CONTINUE"), [
    "claude-mac-b",
    "claude-mac-e",
    "claude-mac-f",
  ]);
  assert.deepEqual(q("CODEX-MAC-D"), ["codex-mac-d"]);
  assert.deepEqual(q("codex-mac"), []);
  assert.deepEqual(q("<script>"), ["claude-mac-b"]);
  assert.deepEqual(q("zzz"), []);
  // Flags as the builder writes them.
  assert.equal(lm.rowFlags(rows[0]), m.FLAG.decision | m.FLAG.settingsUnknown);
  assert.equal(lm.rowFlags(rows[1]), m.FLAG.pastedLost | m.FLAG.inferred);
  assert.equal(lm.rowFlags(rows[2]), m.FLAG.command | m.FLAG.settingsUnknown);
  assert.equal(lm.rowFlags(rows[5]) & m.FLAG.redacted, m.FLAG.redacted);
  assert.deepEqual(lm.badges(rows[5]), ["settings unknown", "redacted"]);
  assert.deepEqual(lm.badges(rows[4]), ["article work"]);
  // The website-folder prompts are not in the log: no flag, badge or "other repo" note exists for them.
  assert.equal("website" in m.FLAG, false);
  assert.equal("OTHER_REPO_NOTE" in lm, false);

  // Most repeated: commands ignored; case, whitespace and trailing ".?!" normalized.
  assert.deepEqual(
    lm.topRepeated(
      [
        { kind: "prompt", text: "continue" },
        { kind: "command", text: "/compact" },
        { kind: "command", text: "/compact" },
        { kind: "command", text: "/compact" },
        { kind: "prompt", text: " continue\n" },
        { kind: "prompt", text: "Continue!" },
        { kind: "prompt", text: "go  ahead" },
        { kind: "prompt", text: "Go ahead." },
        { kind: "prompt", text: "Status" },
        { kind: "prompt", text: "status?" },
        { kind: "prompt", text: "status ?! " },
        { kind: "prompt", text: "continue" },
        { kind: "prompt", text: "once" },
        { kind: "prompt", text: "?status" },
        { kind: "prompt", text: "   " },
        { kind: "prompt", text: "?!" },
        { kind: "prompt", text: "..." },
      ],
      5,
    ),
    [
      { text: "continue", count: 4 },
      { text: "Status", count: 3 },
      { text: "go ahead", count: 2 },
    ],
  );
  assert.equal(lm.repeatKey("  Go   AHEAD?! "), "go ahead");
  assert.equal(lm.repeatKey("why? ok"), "why? ok");
  assert.deepEqual(lm.topRepeated([], 5), []);
  assert.equal(
    lm.topRepeated(
      Array(9)
        .fill({ kind: "prompt", text: "a" })
        .concat(Array(3).fill({ kind: "prompt", text: "b" })),
      1,
    ).length,
    1,
  );

  // Scroll correction: only rows wholly above the view move the box, never the toggled row.
  const ch = (id: string, top: number, oldH: number, newH: number) => ({
    id,
    top,
    oldH,
    newH,
  });
  assert.equal(lm.scrollShift([ch("a", 0, 100, 300)], 100), 200);
  assert.equal(lm.scrollShift([ch("a", 0, 100, 300)], 99), 0, "partly visible");
  assert.equal(
    lm.scrollShift([ch("a", 40, 100, 300)], 60),
    0,
    "row straddles the top",
  );
  assert.equal(
    lm.scrollShift([ch("a", 0, 100, 300)], 500, "a"),
    0,
    "toggled row",
  );
  assert.equal(
    lm.scrollShift(
      [ch("a", 0, 100, 80), ch("b", 100, 100, 150), ch("c", 400, 50, 90)],
      250,
    ),
    30,
  );

  // Reveal: relax only the conditions hiding the row, then the row passes.
  const busy: Model.Filter = {
    from: 5,
    to: 6,
    lanes: new Set([1]),
    decisionsOnly: true,
    hideArticle: true,
    model: "claude-opus-4-8",
    effort: "unknown",
    phase: "tightened",
    query: "fix",
  };
  const rev = lm.revealFilter(busy, rows[4]); // article work, gpt high, day 9, "  continue"
  assert.deepEqual(
    { ...rev, lanes: [...rev.lanes].sort() },
    {
      from: 5,
      to: 9,
      lanes: [0, 1],
      decisionsOnly: false,
      hideArticle: false,
      model: undefined,
      effort: undefined,
      phase: "tightened",
      query: undefined,
    },
  );
  for (const r of rows) {
    assert.ok(lm.filterRows(rows, lm.revealFilter(busy, r)).includes(r), r.id);
  }
  const keep = lm.revealFilter({ ...def, query: "fix" }, rows[3]);
  assert.equal(keep.query, "fix");

  // Glossary split: ids as whole tokens with their own spelling; joining gives the text back.
  const parts = lm.splitGlossed("fix UD131 then R7", glossary);
  assert.deepEqual(parts, [
    { t: "fix " },
    { t: "UD131", id: "UD131" },
    { t: " then " },
    { t: "R7", id: "R7" },
  ]);
  assert.deepEqual(lm.splitGlossed("R7", glossary), [{ t: "R7", id: "R7" }]);
  assert.deepEqual(lm.splitGlossed("R70 r7 UD1311 xR7 P3-W0.", glossary), [
    { t: "R70 r7 UD1311 xR7 " },
    { t: "P3-W0", id: "P3-W0" },
    { t: "." },
  ]);
  assert.deepEqual(lm.splitGlossed("", glossary), []);
  assert.deepEqual(lm.splitGlossed("plain", {}), [{ t: "plain" }]);
  // Markup characters stay literal text in the parts.
  const odd = "<b>R7</b> {x} **y** `z` [redacted: secret] 🦀\n\tnext";
  const oddParts = lm.splitGlossed(odd, glossary);
  assert.equal(oddParts.map((p) => p.t).join(""), odd);
  assert.deepEqual(oddParts[1], { t: "R7", id: "R7" });

  assert.equal(lm.fmtBytes(453588), "443 KB");
  assert.equal(lm.fmtBytes(900), "900 B");

  // The widget renders prompt text as text nodes only: no component injects HTML.
  const dir = new URL("./", import.meta.url);
  for (const f of readdirSync(dir)) {
    if (!/\.tsx?$/.test(f) || f.endsWith(".test.ts")) continue;
    const src = readFileSync(new URL(f, dir), "utf8");
    assert.doesNotMatch(
      src,
      /dangerouslySetInnerHTML|innerHTML|insertAdjacentHTML|outerHTML/,
      f,
    );
  }
  runCardTests(lm, m);
}

/** CARD_LOG is a seven-prompt log in four sessions (one command-only), with categories. */
const CARD_LOG = {
  glossary: {},
  sessions: {
    "claude-mac-s1": {
      title: "One",
      titleSource: "claude-code",
      area: "engine",
      branches: [],
      models: [],
      commits: [],
      commitsAmbiguous: 0,
    },
    "claude-mac-s2": {
      title: "Two",
      titleSource: "claude-written",
      area: "tooling",
      branches: [],
      models: [],
      commits: [],
      commitsAmbiguous: 0,
    },
    "claude-mac-s3": {
      title: "Command-only session",
      titleSource: "claude-written",
      area: "command-only",
      branches: [],
      models: [],
      commits: [],
      commitsAmbiguous: 0,
    },
    "claude-mac-s4": {
      title: "Four",
      titleSource: "claude-code",
      area: "engine",
      branches: [],
      models: [],
      commits: [],
      commitsAmbiguous: 0,
    },
  },
  rows: [
    logRow({
      id: "r0",
      ts: "2026-06-22T18:00:00Z",
      session: "claude-mac-s1",
      category: "instruction",
      text: "port it",
    }),
    logRow({
      id: "r1",
      ts: "2026-06-22T19:00:00Z",
      session: "claude-mac-s2",
      category: "instruction",
      text: "tool it",
    }),
    logRow({
      id: "r2",
      ts: "2026-06-22T20:00:00Z",
      session: "claude-mac-s1",
      category: "setting",
      kind: "command",
      text: "/model",
    }),
    logRow({
      id: "r3",
      ts: "2026-06-23T18:00:00Z",
      session: "claude-mac-s3",
      category: "session",
      kind: "command",
      text: "/resume",
    }),
    logRow({
      id: "r4",
      ts: "2026-06-24T18:00:00Z",
      session: "claude-mac-s1",
      category: "decision",
      decision: true,
      text: "yes",
    }),
    logRow({
      id: "r5",
      ts: "2026-06-25T18:00:00Z",
      session: "claude-mac-s4",
      category: "setting",
      kind: "command",
      text: "/effort",
    }),
    logRow({
      id: "r6",
      ts: "2026-06-26T18:00:00Z",
      session: "claude-mac-s3",
      category: "shell",
      text: "!ls",
    }),
  ],
};

/** Session-card checks: grouping, filtering inside cards, the command-only group and revealing a card. */
function runCardTests(lm: typeof LogModel, m: typeof Model) {
  const { rows } = lm.decodeLog(CARD_LOG, "2026-06-21", [
    "claude-mac",
    "claude-vm",
    "codex",
  ]);
  assert.deepEqual(
    rows.map((r) => [r.session, r.area, r.category]),
    [
      ["claude-mac-s1", "engine", "instruction"],
      ["claude-mac-s2", "tooling", "instruction"],
      ["claude-mac-s1", "engine", "setting"],
      ["claude-mac-s3", "command-only", "session"],
      ["claude-mac-s1", "engine", "decision"],
      ["claude-mac-s4", "engine", "setting"],
      ["claude-mac-s3", "command-only", "shell"],
    ],
  );
  const co = (k: string) => k === "claude-mac-s3";
  const all = m.defaultFilter(10, 3);
  // Every prompt: one card per session in first-prompt order, rows in order.
  const cards = lm.groupBySession(rows, lm.filterRows(rows, all), co);
  assert.deepEqual(
    cards.map((c) => [c.key, c.rows.map((r) => r.id), c.total, c.commandOnly]),
    [
      ["claude-mac-s1", ["r0", "r2", "r4"], 3, false],
      ["claude-mac-s2", ["r1"], 1, false],
      ["claude-mac-s3", ["r3", "r6"], 2, true],
      ["claude-mac-s4", ["r5"], 1, false],
    ],
  );
  // The default categories apply inside cards ("2 of 3 shown"); cards with nothing left are hidden.
  const def = m.defaultFilter(10, 3, m.CATEGORY_DEFAULT);
  const dc = lm.groupBySession(rows, lm.filterRows(rows, def), co);
  assert.deepEqual(
    dc.map((c) => [c.key, c.rows.length, c.total]),
    [
      ["claude-mac-s1", 2, 3],
      ["claude-mac-s2", 1, 1],
    ],
  );
  // Other filters too: a day range keeps part of a card.
  const day = lm.groupBySession(
    rows,
    lm.filterRows(rows, { ...all, from: 1, to: 2 }),
    co,
  );
  assert.deepEqual(
    day.map((c) => [c.key, c.rows.map((r) => r.id)]),
    [
      ["claude-mac-s1", ["r0", "r2"]],
      ["claude-mac-s2", ["r1"]],
      ["claude-mac-s3", ["r3"]],
    ],
  );
  assert.deepEqual(lm.groupBySession(rows, [], co), []);

  // Entries: command-only sessions collapse into one group at the end, with counts.
  const e0 = lm.cardEntries(cards, new Set(), false);
  assert.deepEqual(
    e0.map((e) => e.id),
    [
      "card:claude-mac-s1",
      "card:claude-mac-s2",
      "card:claude-mac-s4",
      lm.GROUP_ID,
    ],
  );
  const g = e0[3];
  assert.ok(g.kind === "group" && g.cards === 1 && g.prompts === 2 && !g.open);
  // An open card lists its rows; an open group lists its cards.
  const e1 = lm.cardEntries(
    cards,
    new Set(["claude-mac-s1", "claude-mac-s3"]),
    true,
  );
  assert.deepEqual(
    e1.map((e) => e.id),
    [
      "card:claude-mac-s1",
      "r0",
      "r2",
      "r4",
      "card:claude-mac-s2",
      "card:claude-mac-s4",
      lm.GROUP_ID,
      "card:claude-mac-s3",
      "r3",
      "r6",
    ],
  );
  // With commands hidden there is no group, unless asked for (to say they are hidden).
  assert.equal(
    lm.cardEntries(dc, new Set(), false).some((e) => e.kind === "group"),
    false,
  );
  const empty = lm.cardEntries(dc, new Set(), false, true);
  assert.deepEqual(empty[empty.length - 1], {
    kind: "group",
    id: lm.GROUP_ID,
    cards: 0,
    prompts: 0,
    open: false,
  });

  // Nested list places: level, set size and position per level.
  assert.deepEqual(
    lm.entryPlaces(e1).map((p) => [p.level, p.setsize, p.posinset]),
    [
      [1, 4, 1], // card s1
      [2, 3, 1], // r0
      [2, 3, 2], // r2
      [2, 3, 3], // r4
      [1, 4, 2], // card s2
      [1, 4, 3], // card s4
      [1, 4, 4], // group
      [2, 1, 1], // card s3 (command-only, in the group)
      [3, 2, 1], // r3
      [3, 2, 2], // r6
    ],
  );
  const flat = rows.map((r) => ({
    kind: "row" as const,
    id: r.id,
    row: r,
    card: "",
  }));
  assert.deepEqual(lm.entryPlaces(flat)[6], {
    level: 1,
    setsize: 7,
    posinset: 7,
  });
  assert.equal(lm.rowDomId("claude-mac-a.1"), "ep-row-claude-mac-a_1");

  // Jump to a session (timeline or URL): what opens, and where its card lands.
  assert.deepEqual(lm.revealCard(cards, "claude-mac-s4"), {
    card: "claude-mac-s4",
    group: false,
  });
  assert.deepEqual(lm.revealCard(cards, "claude-mac-s3"), {
    card: "claude-mac-s3",
    group: true,
  });
  assert.equal(lm.revealCard(dc, "claude-mac-s3"), null);
  const rv = lm.revealCard(cards, "claude-mac-s3");
  const opened = lm.cardEntries(
    cards,
    new Set([rv?.card ?? ""]),
    rv?.group ?? false,
  );
  const k = lm.entryIndex(opened, "card:claude-mac-s3");
  assert.ok(k > 0);
  assert.equal((opened[k] as { open: boolean }).open, true);
  assert.equal(opened[k + 1].id, "r3");
  // A session filter leaves just that card, open-able at index 0.
  const one = lm.groupBySession(
    rows,
    lm.filterRows(rows, { ...all, session: "claude-mac-s2" }),
    co,
  );
  assert.deepEqual(
    lm.cardEntries(one, new Set(["claude-mac-s2"]), false).map((e) => e.id),
    ["card:claude-mac-s2", "r1"],
  );

  // Categories in the fragment: default omitted, "all" and lists round-trip, unknown names dropped.
  const allowed = {
    models: [],
    efforts: [],
    phases: [],
    categories: [
      "instruction",
      "decision",
      "shell",
      "setting",
      "session",
      "other-command",
    ],
  };
  assert.equal(m.encodeFilter(def, def).includes("c="), false);
  assert.equal(m.isDefault(def, def), true);
  const showAll = { ...def, categories: undefined };
  const encAll = m.encodeFilter(showAll, def);
  assert.ok(encAll.endsWith("&c=all"), encAll);
  assert.equal(m.decodeFilter(encAll, def, allowed).categories, undefined);
  const shell = { ...def, categories: new Set(["shell", "instruction"]) };
  const encShell = m.encodeFilter(shell, def);
  assert.ok(encShell.endsWith("&c=instruction,shell"), encShell);
  assert.deepEqual(
    [...(m.decodeFilter(encShell, def, allowed).categories ?? [])].sort(),
    ["instruction", "shell"],
  );
  assert.deepEqual(
    [...(m.decodeFilter("every-prompt?c=", def, allowed).categories ?? ["x"])],
    [],
  );
  assert.deepEqual(
    [
      ...(m.decodeFilter("every-prompt?c=bogus", def, allowed).categories ??
        []),
    ].sort(),
    ["decision", "instruction"],
  );
  assert.deepEqual(
    [
      ...(m.decodeFilter("every-prompt?c=bogus,shell", def, allowed)
        .categories ?? []),
    ],
    ["shell"],
  );
  assert.deepEqual(
    [
      ...(m.decodeFilter("every-prompt?from=1", def, allowed).categories ?? []),
    ].sort(),
    ["decision", "instruction"],
  );
  // co=1 round-trips and is off by default.
  assert.equal(
    m.decodeFilter(
      m.encodeFilter({ ...def, commandOnly: true }, def),
      def,
      allowed,
    ).commandOnly,
    true,
  );
  assert.equal(
    m.decodeFilter("every-prompt?co=0", def, allowed).commandOnly,
    undefined,
  );
  assert.equal(m.isDefault({ ...def, commandOnly: true }, def), false);
  // Labels.
  const order = allowed.categories;
  assert.equal(
    m.categoriesLabel(new Set(m.CATEGORY_DEFAULT), order),
    "instructions and decisions",
  );
  assert.equal(m.categoriesLabel(undefined, order), "all prompts");
  assert.equal(m.categoriesLabel(new Set(order), order), "all prompts");
  assert.equal(m.categoriesLabel(new Set(), order), "no prompts");
  assert.equal(
    m.categoriesLabel(new Set(["shell", "setting", "instruction"]), order),
    "instructions, shell and settings",
  );
  // A prompt jump relaxes a category that hides the prompt.
  assert.equal(lm.revealFilter(def, rows[2]).categories, undefined);
  assert.deepEqual(
    [...(lm.revealFilter(def, rows[0]).categories ?? [])].sort(),
    ["decision", "instruction"],
  );
}
