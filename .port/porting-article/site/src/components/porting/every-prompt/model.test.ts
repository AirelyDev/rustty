/**
 * Unit tests of the "Every prompt" view-model, run by
 * scripts/verify-porting-helpers.mjs (npm run verify:porting), which passes
 * the imported model module in.
 */
import assert from "node:assert/strict";
import type * as Model from "./model";

/** Builds a tick array row: [day, minute, lane, flags, chars, words, model, effort]. */
function row(
  day: number,
  lane: number,
  flags: number,
  chars: number,
  words: number,
  model = -1,
  effort = -1,
): number[] {
  return [day, 600, lane, flags, chars, words, model, effort];
}

/** FIXTURE is a five-prompt summary over a ten-day window. */
const FIXTURE = {
  window: ["2026-06-21", "2026-06-30"],
  purgedBefore: "2026-06-25",
  lanes: ["claude-mac", "claude-vm", "codex"],
  models: ["claude-opus-4-8", "gpt-5.6-sol"],
  efforts: ["high", "medium"],
  ticks: [
    row(1, 0, 1 | 8, 2, 1), // "ok", decision, settings unknown
    row(2, 0, 8 | 16, 400, 70, 0), // long, pasted lost, Opus 4.8 (inferred)
    row(4, 1, 2, 6, 1, 0), // command on the VM
    row(7, 2, 0, 50, 9, 1, 1), // Codex, gpt medium
    row(9, 0, 4 | 32, 120, 20, 1, 0), // article work, redacted
  ],
  commitsPerDay: [0, 3, 5, 0, 2, 0, 0, 4, 0, 1],
  phases: [
    { key: "founding", label: "Founding rules", from: 0, to: 4 },
    { key: "tightened", label: "Rules tightened", from: 5, to: 9 },
  ],
  weeks: [
    {
      week: "2026-06-15",
      settings: { unknown: 1 },
      inferred: { "claude-opus-4-8": 1 },
      modes: { unknown: 2 },
    },
    { week: "2026-06-22", settings: {}, inferred: {}, modes: {} },
    {
      week: "2026-06-29",
      settings: {
        "gpt-5.6-sol|medium": 1,
        "gpt-5.6-sol|high": 1,
        "claude-opus-4-8|unknown": 1,
      },
      inferred: {},
      modes: { bypass: 2, unknown: 1 },
    },
  ],
  totals: {} as Model.Totals,
  logUrl: "/porting/every-prompt-log.json",
  logBytes: 1,
  rules: { decision: "", articleWork: "", commits: "" },
  sourcesHref: "",
} as unknown as Model.EpData;

/** Runs every model check against the model module m. */
export function runModelTests(m: typeof Model) {
  const ticks = m.decodeTicks(FIXTURE);
  assert.equal(ticks.length, 5);
  // The log-scale length is derived from chars (not sent), rounded as the builder used to send it.
  assert.deepEqual(
    ticks.map((t) => [t.chars, t.log2chars]),
    [
      [2, 1],
      [400, 8.6],
      [6, 2.6],
      [50, 5.6],
      [120, 6.9],
    ],
  );
  assert.equal(m.log2Chars(0), 0);
  // Indexed session fields: a number points into the names, a string (older data) is kept.
  assert.equal(m.named(1, ["engine", "website"]), "website");
  assert.equal(m.named("tooling", ["engine"]), "tooling");
  assert.equal(m.named(7, ["engine"]), "");
  assert.deepEqual(
    [ticks[3].model, ticks[3].effort, ticks[3].phase, ticks[3].i],
    ["gpt-5.6-sol", "medium", "tightened", 3],
  );
  assert.equal(ticks[0].model, null);
  const def = m.defaultFilter(10, 3);
  const ids = (f: Model.Filter, match?: (i: number) => boolean) =>
    m.applyFilter(ticks, f, match).map((t) => t.i);

  // Range, lanes and flags.
  assert.deepEqual(ids(def), [0, 1, 2, 3, 4]);
  assert.deepEqual(ids({ ...def, from: 2, to: 7 }), [1, 2, 3]);
  assert.deepEqual(ids({ ...def, lanes: new Set([0, 2]) }), [0, 1, 3, 4]);
  assert.deepEqual(ids({ ...def, lanes: new Set() }), []);
  assert.deepEqual(ids({ ...def, decisionsOnly: true }), [0]);
  assert.deepEqual(ids({ ...def, hideArticle: true }), [0, 1, 2, 3]);
  assert.deepEqual(ids({ ...def, model: "claude-opus-4-8" }), [1, 2]);
  assert.deepEqual(ids({ ...def, model: "unknown" }), [0]);
  assert.deepEqual(ids({ ...def, effort: "unknown" }), [0, 1, 2]);
  assert.deepEqual(ids({ ...def, effort: "medium" }), [3]);
  assert.deepEqual(ids({ ...def, phase: "tightened" }), [3, 4]);
  // The query applies only with a matcher (the loaded log).
  assert.deepEqual(ids({ ...def, query: "x" }), [0, 1, 2, 3, 4]);
  assert.deepEqual(
    ids({ ...def, query: "x" }, (i) => i === 2),
    [2],
  );
  assert.equal(
    m.matchesQuery({ id: "codex-mac-1", text: "Go AHEAD" }, "go ahead"),
    true,
  );
  assert.equal(
    m.matchesQuery({ id: "codex-mac-1", text: "x" }, "CODEX-mac-1"),
    true,
  );
  assert.equal(
    m.matchesQuery({ id: "codex-mac-1", text: "x" }, "codex"),
    false,
  );

  // Tiles: live values, and an empty result.
  const all = m.tiles(ticks, { perDay: FIXTURE.commitsPerDay, from: 0, to: 9 });
  assert.deepEqual(all, {
    prompts: 5,
    medianChars: 50,
    underTenWordsShare: 0.6,
    decisions: 1,
    commitsPerPrompt: 3,
  });
  assert.equal(m.tiles(ticks.slice(0, 2)).medianChars, 201);
  assert.equal(m.tiles(ticks).commitsPerPrompt, null);
  const none = m.applyFilter(ticks, { ...def, from: 5, to: 6 });
  assert.deepEqual(
    m.tiles(none, { perDay: FIXTURE.commitsPerDay, from: 5, to: 6 }),
    {
      prompts: 0,
      medianChars: null,
      underTenWordsShare: null,
      decisions: 0,
      commitsPerPrompt: null,
    },
  );
  // Commits per prompt only for day (and phase) selections.
  const P = FIXTURE.phases;
  const C = FIXTURE.commitsPerDay;
  assert.deepEqual(m.commitRange({ ...def, from: 2, to: 8 }, P, C), {
    perDay: C,
    from: 2,
    to: 8,
  });
  assert.deepEqual(
    m.commitRange({ ...def, phase: "tightened", from: 3 }, P, C),
    { perDay: C, from: 5, to: 9 },
  );
  assert.equal(
    m.commitRange({ ...def, phase: "founding", from: 6 }, P, C),
    undefined,
  );
  assert.equal(m.commitRange({ ...def, decisionsOnly: true }, P, C), undefined);
  assert.equal(m.commitRange({ ...def, lanes: new Set([0]) }, P, C), undefined);
  assert.equal(m.commitRange({ ...def, query: "x" }, P, C), undefined);
  assert.equal(m.longest(ticks)?.i, 1);
  assert.equal(m.longest([]), null);

  // Encode/decode round trip, including text needing escapes.
  const f: Model.Filter = {
    from: 2,
    to: 8,
    lanes: new Set([2, 0]),
    decisionsOnly: true,
    hideArticle: false,
    model: "gpt-5.6-sol",
    effort: "unknown",
    phase: "tightened",
    query: "fix UD131 & then <R7>?",
  };
  const enc = m.encodeFilter(f);
  assert.ok(
    enc.startsWith("every-prompt?from=2&to=8&lanes=0,2&d=1&a=0&m=gpt-5.6-sol"),
    enc,
  );
  const back = m.decodeFilter(`#${enc}`, def);
  assert.deepEqual(
    { ...back, lanes: [...back.lanes] },
    { ...f, lanes: [0, 2] },
  );
  assert.equal(m.encodeFilter(back), enc);
  assert.equal(
    m.isDefault(m.decodeFilter(m.encodeFilter(def), def), def),
    true,
  );

  // Decode ignores garbage field by field.
  /** Decodes s against the defaults and returns a comparable plain object. */
  const dec = (s: string) => {
    const x = m.decodeFilter(s, def);
    return { ...x, lanes: [...x.lanes] };
  };
  const plainDef = { ...def, lanes: [0, 1, 2] };
  assert.deepEqual(dec(""), plainDef);
  assert.deepEqual(dec("#intro"), plainDef);
  assert.deepEqual(dec("every-prompt"), plainDef);
  assert.deepEqual(
    dec("every-prompt?from=abc&to=-3&lanes=x,9&d=2&a=yes&q=%E0%A4%A"),
    plainDef,
  );
  assert.deepEqual(dec("every-prompt?from=7&to=3"), plainDef);
  assert.deepEqual(dec("every-prompt?from=4&to=999").to, 9);
  assert.deepEqual(dec("every-prompt?lanes=1,7").lanes, [1]);
  assert.deepEqual(dec("every-prompt?lanes=").lanes, []);
  assert.deepEqual(dec("every-prompt?q=%20%20").query, undefined);
  assert.equal(dec(`every-prompt?q=${"a".repeat(500)}`).query?.length, 200);

  // With the data's allowed values, unknown models, efforts and phases are ignored.
  const allowed = {
    models: FIXTURE.models,
    efforts: FIXTURE.efforts,
    phases: FIXTURE.phases.map((p) => p.key),
  };
  const strict = m.decodeFilter(
    "every-prompt?m=gpt-9&e=extreme&p=nope&q=ok",
    def,
    allowed,
  );
  assert.deepEqual(
    [strict.model, strict.effort, strict.phase, strict.query],
    [undefined, undefined, undefined, "ok"],
  );
  const okd = m.decodeFilter(
    "every-prompt?m=unknown&e=high&p=tightened",
    def,
    allowed,
  );
  assert.deepEqual(
    [okd.model, okd.effort, okd.phase],
    ["unknown", "high", "tightened"],
  );
  assert.equal(m.decodeFilter(`#${enc}`, def, allowed).model, "gpt-5.6-sol");

  // Weekly settings bars: known by model/effort, inferred apart, unknown last; empty weeks kept.
  const bars = m.weekBars(FIXTURE.weeks, FIXTURE.models, "settings");
  assert.deepEqual(
    bars.map((b) => [b.week, b.total, b.segments.map((s) => s.key)]),
    [
      ["2026-06-15", 2, ["inferred|claude-opus-4-8", "unknown"]],
      ["2026-06-22", 0, []],
      [
        "2026-06-29",
        3,
        ["claude-opus-4-8|unknown", "gpt-5.6-sol|high", "gpt-5.6-sol|medium"],
      ],
    ],
  );
  assert.equal(bars[0].segments[0].kind, "inferred");
  assert.equal(bars[0].segments[1].kind, "unknown");
  const modes = m.weekBars(FIXTURE.weeks, FIXTURE.models, "modes");
  assert.deepEqual(
    modes[2].segments.map((s) => [s.key, s.kind, s.count]),
    [
      ["bypass", "known", 2],
      ["unknown", "unknown", 1],
    ],
  );

  // Labels and dates.
  assert.equal(m.modelLabel("claude-opus-4-8"), "Opus 4.8");
  assert.equal(m.modelLabel("claude-fable-5-1"), "Fable 5.1");
  assert.equal(m.modelLabel("gpt-5.6-sol"), "GPT-5.6 Sol");
  assert.equal(m.modelLabel("gpt-5.5"), "GPT-5.5");
  assert.equal(m.shortDate(m.dayDate("2026-06-21", 111)), "Oct 10");
  assert.equal(m.dayIndexOf("2026-06-21", "2026-09-08"), 79);
  const counts = m.dayCounts(ticks, 10, 3);
  assert.deepEqual(counts[4], { lanes: [0, 1, 0], total: 1 });
}
