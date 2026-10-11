/** Pure statistics for the "What it cost" widget: token anatomy, delegation, spend by model, eras, context per call, concurrency and the Codex meter. Imports only types so Node can run it directly. */
import type { CostData, Scope } from "./types";

/** One token kind of the anatomy chart, with its token and dollar shares and its $ per million tokens. */
export type MixRow = {
  key: string;
  name: string;
  color: string;
  tok: number;
  usd: number;
  shareTok: number;
  shareUsd: number;
  perM: number;
};

/** MIX_KINDS are the token kinds in drawing order: key, name, colour. */
export const MIX_KINDS: [string, string, string][] = [
  ["cache_read", "cache read", "#d97934"],
  ["cache_write", "cache write", "#ffbc7a"],
  ["input", "uncached input", "#61aeee"],
  ["output", "output", "#c678dd"],
];

/** Returns the token-anatomy rows for a scope. */
export function tokenMix(d: CostData, sc: Scope): MixRow[] {
  const m = d.mix[sc];
  const tt = MIX_KINDS.reduce((a, [k]) => a + m[k].tok, 0);
  const tu = MIX_KINDS.reduce((a, [k]) => a + m[k].usd, 0);
  return MIX_KINDS.map(([key, name, color]) => ({
    key,
    name,
    color,
    tok: m[key].tok,
    usd: m[key].usd,
    shareTok: m[key].tok / tt,
    shareUsd: m[key].usd / tu,
    perM: (m[key].usd / m[key].tok) * 1e6,
  }));
}

/** A thread kind: main thread, subagents or workflow runs. */
export type ThreadKind = "main" | "subagent" | "workflow";

/** DELEGATION_ROWS are the delegation bars: label and the tool/kind keys they sum. */
const DELEGATION_ROWS: [string, string[]][] = [
  ["Claude", ["claude/main", "claude/subagent", "claude/workflow"]],
  ["Codex", ["codex/main", "codex/subagent"]],
  [
    "all",
    [
      "claude/main",
      "codex/main",
      "claude/subagent",
      "codex/subagent",
      "claude/workflow",
    ],
  ],
];

/** Returns $ by thread kind for Claude, Codex and both, and the delegated share (subagents + workflow runs) of all $. */
export function delegation(
  d: CostData,
  sc: Scope,
): {
  rows: { name: string; parts: Record<ThreadKind, number>; total: number }[];
  delegated: number;
} {
  const kd = d.kinds[sc];
  const rows = DELEGATION_ROWS.map(([name, keys]) => {
    const parts: Record<ThreadKind, number> = {
      main: 0,
      subagent: 0,
      workflow: 0,
    };
    for (const k of keys) {
      if (kd[k]) parts[k.split("/")[1] as ThreadKind] += kd[k].usd;
    }
    return {
      name,
      parts,
      total: parts.main + parts.subagent + parts.workflow,
    };
  });
  const all = rows[2];
  return { rows, delegated: 1 - all.parts.main / all.total };
}

/** KIND_NAMES are the rows of the unit-cost table: key and label. */
const KIND_NAMES: [string, string][] = [
  ["claude/main", "Claude main"],
  ["claude/subagent", "Claude subagents"],
  ["claude/workflow", "Claude workflow runs"],
  ["codex/main", "Codex main"],
  ["codex/subagent", "Codex subagents"],
];

/** Returns the unit-cost table rows: calls, $ per call, median context and $ per million tokens. */
export function kindTable(
  d: CostData,
  sc: Scope,
): {
  key: string;
  name: string;
  calls: number;
  perCall: number;
  ctx: number | null;
  perM: number;
}[] {
  const kd = d.kinds[sc];
  return KIND_NAMES.filter(([k]) => kd[k]).map(([key, name]) => ({
    key,
    name,
    calls: kd[key].calls,
    perCall: kd[key].usd / kd[key].calls,
    ctx: kd[key].ctx_med,
    perM: (kd[key].usd / kd[key].tok) * 1e6,
  }));
}

/** Turns a "tool:model" key into a display name: "Opus 5.5", "Codex gpt-5.5". */
function modelName(key: string): string {
  const [tool, model] = key.split(":");
  if (tool === "codex") return `Codex ${model}`;
  const [family, ...ver] = model
    .replace(/^claude-/, "")
    .split("-")
    .filter((p) => !/^\d{8}$/.test(p));
  return `${family[0].toUpperCase()}${family.slice(1)} ${ver.join(".")}`;
}

/** Returns recovered Rustty spend by model, largest first, with each model's share of the recovered total. */
export function topModels(
  d: CostData,
): { name: string; usd: number; share: number }[] {
  return Object.entries(d.model_usd)
    .sort((a, b) => b[1] - a[1])
    .map(([k, usd]) => ({
      name: modelName(k),
      usd,
      share: usd / d.api.recovered,
    }));
}

/** An era with its cost [low, central, high] and per-unit figures. */
export type EraStat = {
  name: string;
  start: string;
  end: string;
  recovered: number;
  cost: [number, number, number];
  /** True when less than half of the central cost is recovered (the rest is modeled). */
  mostlyModeled: boolean;
  commits: number;
  prompts: number;
  codeLines: number;
  perCommit: [number, number, number];
  perPrompt: [number, number, number];
  perKLines: [number, number, number];
  commitsPerPrompt: number;
};

/** Returns the three eras with recovered + modeled cost and $ per commit, per prompt and per 1k code lines. */
export function eraStats(d: CostData): EraStat[] {
  return d.eras.map((e) => {
    const cost = [e.ext.low, e.ext.central, e.ext.high].map(
      (v) => v + e.recovered,
    ) as [number, number, number];
    /** Divides each cost bound by n. */
    const per = (n: number) =>
      cost.map((v) => v / n) as [number, number, number];
    return {
      name: e.name,
      start: e.start,
      end: e.end,
      recovered: e.recovered,
      cost,
      mostlyModeled: e.recovered / cost[1] < 0.5,
      commits: e.commits,
      prompts: e.prompts,
      codeLines: e.code_lines,
      perCommit: per(e.commits),
      perPrompt: per(e.prompts),
      perKLines: per(e.code_lines / 1000),
      commitsPerPrompt: e.commits / e.prompts,
    };
  });
}

/** UNIT_MIN_USD is the smallest recovered week counted in the $/commit range (a near-idle week is noise). */
const UNIT_MIN_USD = 100;

/** Returns the [min, max] $ per commit over the fully recovered quota weeks with real spend. */
export function unitRange(d: CostData): [number, number] {
  const v = d.units
    .filter((u) => u.usd >= UNIT_MIN_USD && u.commits > 0)
    .map((u) => u.usd / u.commits);
  return [Math.min(...v), Math.max(...v)];
}

/** CTX_MIN_CALLS is the fewest Claude calls a week needs before its median context is quoted. */
export const CTX_MIN_CALLS = 100;

/** Returns the median context per call of the first quoted week, of the largest week, and the largest single context. */
export function contextGrowth(d: CostData): {
  from: { w: string; med: number };
  to: { w: string; med: number };
  ratio: number;
  max: number;
} {
  const cl = d.ctx.filter((c) => c.tool === "claude" && c.n >= CTX_MIN_CALLS);
  const from = cl[0];
  const to = cl.reduce((a, c) => (c.med > a.med ? c : a), cl[0]);
  return {
    from: { w: from.w, med: from.med },
    to: { w: to.w, med: to.med },
    ratio: to.med / from.med,
    max: Math.max(...d.ctx.filter((c) => c.tool === "claude").map((c) => c.mx)),
  };
}

/** BUSY_MIN_HOURS is the busy time under which a day's concurrency is a small-denominator artifact. */
export const BUSY_MIN_HOURS = 0.5;

/** Returns the highest meaningful concurrency (agent-thread hours ÷ busy hours) with its day and threads. */
export function concurrencyPeak(d: CostData): {
  d: string;
  ratio: number;
  threads: number;
} {
  const ok = d.act.filter((a) => a.b >= BUSY_MIN_HOURS);
  const pk = ok.reduce((a, c) => (c.a / c.b > a.a / a.b ? c : a), ok[0]);
  return { d: pk.d, ratio: pk.a / pk.b, threads: pk.th };
}

/** DROP_HOUR is the hour the Codex meter reading fell from 16% to 1–6% (unexplained in the logs). */
export const DROP_HOUR = "2026-08-01T04";

/** Returns list-price $ per weekly-meter point, from the 1% reading after the drop to the last reading. */
export function codexPerPoint(d: CostData): {
  usd: number;
  points: number;
  perPoint: number;
} {
  const m = d.meter;
  const i = m.findIndex((r) => r[0] === DROP_HOUR);
  const last = m[m.length - 1];
  const usd = last[4] - m[i - 1][4];
  const points = last[1] - m[i][2];
  return { usd, points, perPoint: usd / points };
}
