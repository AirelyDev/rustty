/**
 * Unit tests of the sessions view-model of the "Every prompt" widget, run by
 * scripts/verify-porting-helpers.mjs (npm run verify:porting), which passes
 * the imported modules in.
 */
import assert from "node:assert/strict";
import type * as Model from "./model";
import type * as SessionModel from "./session-model";

/** FIELDS are the summary's sessionFields. */
const FIELDS = [
  "key",
  "lane",
  "firstDay",
  "firstMin",
  "lastDay",
  "lastMin",
  "prompts",
  "promptsBucket",
  "area",
  "title",
  "titleSource",
  "founding",
  "articleWork",
  "commandsOnly",
  "commits",
  "row",
  "spannedDays",
];

/** Builds a session row: key, lane, first/last day and minute, prompts, area, founding, row. */
function sess(
  key: string,
  lane: number,
  first: [number, number],
  last: [number, number],
  prompts: number,
  area: string,
  founding = false,
  row = 0,
): unknown[] {
  return [
    key,
    lane,
    first[0],
    first[1],
    last[0],
    last[1],
    prompts,
    0,
    area,
    `Title of ${key}`,
    "claude-code",
    founding,
    false,
    area === "command-only",
    0,
    row,
    last[0] - first[0] + 1,
  ];
}

/** Builds a tick row: [day, minute, lane, flags, chars, words, model, effort, session, mode]. */
function tick(
  day: number,
  minute: number,
  lane: number,
  flags: number,
  session: number,
  model = -1,
  effort = -1,
  mode = -1,
): number[] {
  return [day, minute, lane, flags, 8, 2, model, effort, session, mode];
}

/** AREAS is the fixture's area order. */
const AREAS = ["engine", "tooling", "article"];

/**
 * FIXTURE is a ten-day summary with five sessions:
 *  0 A engine, Mac, day 1 23:00 → day 2 01:00 (open across midnight), founding
 *  1 B tooling, VM, day 2 only
 *  2 C command-only, Mac, day 2 (slash commands only)
 *  3 D engine, Codex, days 5–7 (resumed: no prompt on day 6)
 *  4 E article, Mac, day 2 → day 8
 */
const FIXTURE = {
  window: ["2026-06-20", "2026-06-29"],
  purgedBefore: "2026-06-24",
  lanes: ["claude-mac", "claude-vm", "codex"],
  models: ["claude-opus-4-8"],
  efforts: ["high"],
  modes: ["bypass", "default"],
  areas: AREAS,
  promptBuckets: [1, 2, 5, 15, 50],
  sessionFields: FIELDS,
  sessions: [
    sess("claude-mac-aaaa0001", 0, [1, 1380], [2, 60], 3, "engine", true, 0),
    sess("claude-vm-bbbb0002", 1, [2, 600], [2, 700], 2, "tooling", false, 1),
    sess("claude-mac-cccc0003", 0, [2, 800], [2, 800], 1, "command-only"),
    sess("codex-mac-dddd0004", 2, [5, 600], [7, 600], 2, "engine", false, 0),
    sess("claude-mac-eeee0005", 0, [2, 900], [8, 600], 3, "article", false, 2),
  ],
  ticks: [
    tick(1, 1380, 0, 0, 0), // 0: A, day 1
    tick(1, 1400, 0, 1, 0), // 1: A, decision
    tick(2, 60, 0, 64, 0, 0), // 2: A, after midnight, Opus 4.8 inferred from commits
    tick(2, 600, 1, 0, 1), // 3: B
    tick(2, 700, 1, 2, 1), // 4: B, a command
    tick(2, 800, 0, 2, 2), // 5: C, command only
    tick(2, 900, 0, 4, 4), // 6: E, article work
    tick(5, 600, 2, 0, 3, 0, 0, 0), // 7: D, Opus 4.8 high, bypass
    tick(7, 600, 2, 0, 3), // 8: D
    tick(3, 600, 0, 4, 4), // 9: E
    tick(8, 600, 0, 4, 4), // 10: E
  ],
  commitsPerDay: [0, 1, 2, 0, 0, 1, 0, 0, 3, 0],
  phases: [
    { key: "founding", label: "Founding rules", from: 0, to: 3 },
    { key: "tightened", label: "Rules tightened", from: 4, to: 9 },
  ],
  weeks: [
    { week: "2026-06-15", settings: {}, inferred: {}, modes: {} },
    { week: "2026-06-22", settings: {}, inferred: {}, modes: {} },
    { week: "2026-06-29", settings: {}, inferred: {}, modes: {} },
  ],
  totals: {} as Model.Totals,
  logUrl: "",
  logBytes: 1,
  rules: { decision: "", articleWork: "", commits: "" },
  sourcesHref: "",
} as unknown as Model.EpData;

/** Runs every sessions check against the modules sm and m. */
export function runSessionModelTests(sm: typeof SessionModel, m: typeof Model) {
  const sessions = sm.decodeSessions(FIXTURE);
  // The builder sends area and title source as indices into sessionAreas and titleSources.
  {
    const areaAt = FIELDS.indexOf("area");
    const srcAt = FIELDS.indexOf("titleSource");
    const sessionAreas = [...AREAS, "command-only"];
    const titleSources = ["claude-code", "claude-written"];
    const indexed = {
      ...FIXTURE,
      sessionAreas,
      titleSources,
      sessions: (FIXTURE.sessions ?? []).map((r) => {
        const out = [...r];
        out[areaAt] = sessionAreas.indexOf(String(r[areaAt]));
        out[srcAt] = titleSources.indexOf(String(r[srcAt]));
        return out;
      }),
    };
    assert.deepEqual(sm.decodeSessions(indexed), sessions);
    assert.deepEqual(
      m.decodeTicks(indexed).map((t) => t.area),
      m.decodeTicks(FIXTURE).map((t) => t.area),
    );
  }
  const ticks = m.decodeTicks(FIXTURE);
  const def = m.defaultFilter(10, 3);
  const P = FIXTURE.phases;
  /** Returns the keys' last letter group of sessions f keeps. */
  const keys = (f: Model.Filter) =>
    sm.sessionsInRange(sessions, f, P).map((s) => s.key.slice(-1));

  // Decoding.
  assert.equal(sessions.length, 5);
  assert.deepEqual(
    [sessions[0].key, sessions[0].founding, sessions[0].spannedDays],
    ["claude-mac-aaaa0001", true, 2],
  );
  assert.equal(sessions[2].commandsOnly, true);
  assert.deepEqual(
    [ticks[3].session, ticks[3].area, ticks[5].area],
    ["claude-vm-bbbb0002", "tooling", "command-only"],
  );

  // Range, lane, area, phase and session filtering.
  assert.deepEqual(keys(def), ["1", "2", "3", "4", "5"]);
  assert.deepEqual(keys({ ...def, from: 3, to: 4 }), ["5"]);
  assert.deepEqual(keys({ ...def, from: 6, to: 6 }), ["4", "5"]);
  assert.deepEqual(keys({ ...def, from: 0, to: 1 }), ["1"]);
  assert.deepEqual(keys({ ...def, lanes: new Set([1, 2]) }), ["2", "4"]);
  assert.deepEqual(keys({ ...def, area: "engine" }), ["1", "4"]);
  assert.deepEqual(keys({ ...def, phase: "tightened" }), ["4", "5"]);
  assert.deepEqual(keys({ ...def, session: "claude-vm-bbbb0002" }), ["2"]);
  // Prompt conditions do not drop sessions.
  assert.deepEqual(keys({ ...def, decisionsOnly: true }), [
    "1",
    "2",
    "3",
    "4",
    "5",
  ]);
  // The prompts filter agrees on area and session.
  const ids = (f: Model.Filter) => m.applyFilter(ticks, f).map((t) => t.i);
  assert.deepEqual(ids({ ...def, area: "engine" }), [0, 1, 2, 7, 8]);
  assert.deepEqual(ids({ ...def, session: "claude-mac-eeee0005" }), [6, 9, 10]);
  assert.deepEqual(ids({ ...def, area: "command-only" }), [5]);
  assert.equal(
    m.commitRange({ ...def, area: "engine" }, P, FIXTURE.commitsPerDay),
    undefined,
  );
  assert.equal(
    m.commitRange(
      { ...def, session: "codex-mac-dddd0004" },
      P,
      FIXTURE.commitsPerDay,
    ),
    undefined,
  );

  // Tiles on everything: the busiest day counts sessions with a prompt that is
  // not a slash command that day; A is open on day 2 through its after-midnight
  // prompt, C (commands only) never is; B counts once despite two prompts.
  const all = sm.sessionTiles(sessions, ticks, AREAS);
  assert.equal(all.sessions, 5);
  assert.equal(all.commandOnly, 1);
  assert.equal(all.founding, 1);
  assert.deepEqual(all.byArea, [
    { area: "engine", n: 2 },
    { area: "tooling", n: 1 },
    { area: "article", n: 1 },
  ]);
  assert.deepEqual(all.busiestDay, { day: 2, open: 3 });
  assert.equal(all.longestPrompts?.key, "claude-mac-aaaa0001");
  assert.equal(all.medianPrompts, 2);
  // D is open on days 5 and 7 but not 6 (no prompt that day).
  const d = sm.sessionTiles([sessions[3]], ticks, AREAS, [6, 6]);
  assert.equal(d.busiestDay, null);
  // A range limits the counted days.
  assert.deepEqual(sm.sessionTiles(sessions, ticks, AREAS, [5, 9]).busiestDay, {
    day: 5,
    open: 1,
  });
  // Ties go to the earliest day.
  assert.deepEqual(
    sm.sessionTiles(sessions.slice(3), ticks, AREAS).busiestDay,
    { day: 2, open: 1 },
  );

  // Empty input: zeros and nulls, no crash.
  const none = sm.sessionTiles([], ticks, AREAS);
  assert.deepEqual(none, {
    sessions: 0,
    commandOnly: 0,
    founding: 0,
    byArea: [
      { area: "engine", n: 0 },
      { area: "tooling", n: 0 },
      { area: "article", n: 0 },
    ],
    longestPrompts: null,
    busiestDay: null,
    medianPrompts: null,
  });
  assert.equal(sm.sessionTiles([]).busiestDay, null);

  // With categories, only instructions and decisions open a session on a day: a slash command
  // carrying text (category instruction) counts, a shell escape does not.
  const cat = (t: (typeof ticks)[number], category: string) => ({
    ...t,
    category,
  });
  const onDay5 = ticks.filter((t) => t.day === 5);
  assert.ok(onDay5.length > 0);
  const asShell = ticks.map((t) =>
    t.day === 5 ? cat(t, "shell") : cat(t, "instruction"),
  );
  assert.equal(
    sm.sessionTiles(sessions, asShell, AREAS, [5, 5]).busiestDay,
    null,
  );
  const asText = ticks.map((t) =>
    t.day === 5
      ? { ...cat(t, "instruction"), flags: t.flags | m.FLAG.command }
      : t,
  );
  assert.deepEqual(
    sm.sessionTiles(sessions, asText, AREAS, [5, 5]).busiestDay,
    {
      day: 5,
      open: 1,
    },
  );

  // revealSession: relaxes only what hides the session.
  const E = sessions[4];
  const r1 = sm.revealSession(
    { ...def, from: 0, to: 3, area: "engine" },
    E,
    ticks,
    P,
  );
  assert.deepEqual(
    [r1.from, r1.to, r1.area, r1.session],
    [0, 8, undefined, E.key],
  );
  const kept = sm.revealSession(
    {
      ...def,
      from: 1,
      to: 9,
      lanes: new Set([0]),
      phase: "tightened",
      hideArticle: false,
    },
    E,
    ticks,
    P,
  );
  assert.deepEqual(
    [kept.from, kept.to, [...kept.lanes], kept.phase],
    [1, 9, [0], "tightened"],
  );
  const B = sessions[1];
  const r2 = sm.revealSession(
    { ...def, lanes: new Set([0]), phase: "tightened", area: "tooling" },
    B,
    ticks,
    P,
  );
  assert.deepEqual(
    [[...r2.lanes].sort(), r2.phase, r2.area],
    [[0, 1], undefined, "tooling"],
  );
  // Prompt conditions: hideArticle hides all of E's prompts and goes; the
  // decision condition keeps one of A's prompts and stays.
  const r3 = sm.revealSession({ ...def, hideArticle: true }, E, ticks, P);
  assert.equal(r3.hideArticle, false);
  const A = sessions[0];
  const r4 = sm.revealSession(
    { ...def, decisionsOnly: true, hideArticle: true },
    A,
    ticks,
    P,
  );
  assert.deepEqual([r4.decisionsOnly, r4.hideArticle], [true, true]);
  assert.ok(m.applyFilter(ticks, r4).length > 0);
  // Each condition keeps some of A's prompts, but not together: both go.
  const r5 = sm.revealSession(
    { ...def, decisionsOnly: true, model: "claude-opus-4-8" },
    A,
    ticks,
    P,
  );
  assert.deepEqual([r5.decisionsOnly, r5.model], [false, undefined]);
  // A model no prompt of A has goes; the decision condition stays.
  const r6 = sm.revealSession(
    { ...def, decisionsOnly: true, model: "unknown", effort: "high" },
    A,
    ticks,
    P,
  );
  assert.deepEqual(
    [r6.decisionsOnly, r6.model, r6.effort],
    [true, "unknown", undefined],
  );
  // A search only hides with a matcher; without one it stays.
  assert.equal(
    sm.revealSession({ ...def, query: "zzz" }, A, ticks, P).query,
    "zzz",
  );
  assert.equal(
    sm.revealSession({ ...def, query: "zzz" }, A, ticks, P, () => false).query,
    undefined,
  );
  // The input filter is not changed.
  const f0 = { ...def, lanes: new Set([0]) };
  sm.revealSession(f0, B, ticks, P);
  assert.deepEqual([...f0.lanes], [0]);
  assert.equal(f0.session, undefined);
  // Every revealed filter shows some of the session's prompts.
  for (const s of sessions) {
    for (const f of [
      def,
      r2,
      r3,
      { ...def, from: 9, to: 9, decisionsOnly: true, area: "article" },
    ]) {
      const out = sm.revealSession(f, s, ticks, P);
      assert.ok(
        m.applyFilter(ticks, out).some((t) => t.session === s.key),
        `${s.key}`,
      );
    }
  }

  // Weekly settings from the ticks: the same keys the builder writes, empty weeks kept.
  assert.deepEqual(m.weeksFromTicks(ticks, FIXTURE), [
    {
      week: "2026-06-15",
      settings: { unknown: 2 },
      inferred: {},
      modes: { unknown: 2 },
    },
    {
      week: "2026-06-22",
      settings: { unknown: 7, "claude-opus-4-8|high": 1 },
      inferred: { "claude-opus-4-8": 1 },
      modes: { unknown: 8, bypass: 1 },
    },
    { week: "2026-06-29", settings: {}, inferred: {}, modes: {} },
  ]);
  // Filtered by session: only D's week, with its recorded settings and mode.
  const wd = m.weeksFromTicks(
    m.applyFilter(ticks, { ...def, session: "codex-mac-dddd0004" }),
    FIXTURE,
  );
  assert.deepEqual(
    wd.map((w) => [w.week, w.settings, w.modes]),
    [
      ["2026-06-15", {}, {}],
      [
        "2026-06-22",
        { unknown: 1, "claude-opus-4-8|high": 1 },
        { unknown: 1, bypass: 1 },
      ],
      ["2026-06-29", {}, {}],
    ],
  );
  assert.equal(m.weekOf("2026-06-20", 0), "2026-06-15");
  assert.equal(m.weekOf("2026-06-20", 2), "2026-06-22");
  assert.equal(m.weekOf("2026-06-20", 8), "2026-06-22");
  assert.equal(m.weekOf("2026-06-20", 9), "2026-06-29");

  // The day range: the filter's days narrowed to its phase.
  assert.deepEqual(
    sm.dayRange({ ...def, from: 2, to: 9, phase: "founding" }, P),
    [2, 3],
  );
  assert.deepEqual(sm.dayRange({ ...def, from: 2, to: 9 }, P), [2, 9]);

  // Bar geometry: at least BAR_MIN_W wide, hit targets at least HIT_W, centred on the bar.
  const one = sm.sessionBar(sessions[2], 10, 1000); // a single prompt: zero span
  assert.deepEqual([one.w, one.hitW], [sm.BAR_MIN_W, sm.HIT_W]);
  assert.equal(one.hitX + one.hitW / 2, one.x + one.w / 2);
  const wide = sm.sessionBar(sessions[4], 10, 1000);
  assert.equal(wide.hitW, wide.w);
  assert.ok(wide.w > 500);

  // An area that excludes the selected session drops the session (never an empty view).
  const areaOf = (k: string) => sessions.find((x) => x.key === k)?.area;
  const clash = m.resolveAreaSession(
    { ...def, area: "engine", session: "claude-vm-bbbb0002" },
    areaOf,
  );
  assert.deepEqual(
    [clash.filter.area, clash.filter.session, clash.dropped],
    ["engine", undefined, "claude-vm-bbbb0002"],
  );
  const fits = m.resolveAreaSession(
    { ...def, area: "tooling", session: "claude-vm-bbbb0002" },
    areaOf,
  );
  assert.deepEqual(
    [fits.filter.session, fits.dropped],
    ["claude-vm-bbbb0002", undefined],
  );
  assert.equal(m.applyFilter(ticks, clash.filter).length > 0, true);

  // Encode/decode with ar and s, and rejection of unknown values.
  const allowed = {
    models: FIXTURE.models,
    efforts: FIXTURE.efforts,
    phases: P.map((p) => p.key),
    areas: AREAS,
    sessions: sessions.map((s) => s.key),
  };
  const f: Model.Filter = {
    ...def,
    area: "tooling",
    session: "claude-vm-bbbb0002",
  };
  const enc = m.encodeFilter(f);
  assert.ok(enc.endsWith("&ar=tooling&s=claude-vm-bbbb0002"), enc);
  const back = m.decodeFilter(`#${enc}`, def, allowed);
  assert.deepEqual(
    [back.area, back.session],
    ["tooling", "claude-vm-bbbb0002"],
  );
  assert.equal(m.encodeFilter(back), enc);
  const area2 = m.encodeFilter({ ...def, area: "renderer and fonts" });
  assert.ok(area2.endsWith("&ar=renderer%20and%20fonts"), area2);
  assert.equal(
    m.decodeFilter(area2, def, { ...allowed, areas: ["renderer and fonts"] })
      .area,
    "renderer and fonts",
  );
  const bad = m.decodeFilter(
    "every-prompt?ar=gardening&s=claude-mac-ffffffff",
    def,
    allowed,
  );
  assert.deepEqual([bad.area, bad.session], [undefined, undefined]);
  // A link naming an area and a session outside it keeps the area and drops the session.
  const sessionAreas = Object.fromEntries(sessions.map((x) => [x.key, x.area]));
  const linked = m.decodeFilter(
    "every-prompt?ar=engine&s=claude-vm-bbbb0002",
    def,
    { ...allowed, sessionAreas },
  );
  assert.deepEqual([linked.area, linked.session], ["engine", undefined]);
  assert.equal(
    m.decodeFilter(`#${enc}`, def, { ...allowed, sessionAreas }).session,
    "claude-vm-bbbb0002",
  );
  // command-only is not an area a link may name.
  assert.equal(
    m.decodeFilter("every-prompt?ar=command-only", def, allowed).area,
    undefined,
  );
  // Without the lists, an allowed set names no area or session.
  const noLists = m.decodeFilter(`#${enc}`, def, {
    models: [],
    efforts: [],
    phases: [],
  });
  assert.deepEqual([noLists.area, noLists.session], [undefined, undefined]);
  assert.equal(m.isDefault({ ...def, session: undefined }, def), true);
  assert.equal(m.isDefault({ ...def, session: "x" }, def), false);
}
