/**
 * Pure view-model for the "Cutting the cycle" widget: the file import graph
 * of Ghostty's 345-file cycle, the shatter curve under the planned cuts,
 * shortest cycles, the crate graph and the replay frames.
 */

import type { CcData, Cut } from "./types";

export type { CcData, CrateRow, Cut, FileRow, Layer, ZigEdge } from "./types";

/** CRASH_SEAM is the number of the unplanned seam (crash reporter → Surface). */
export const CRASH_SEAM = 15;

/**
 * Returns the strongly connected components of a directed graph with
 * Tarjan's algorithm (iterative, so deep graphs cannot overflow the stack).
 */
export function tarjan(adj: number[][]): number[][] {
  const n = adj.length;
  const index = new Array<number>(n).fill(-1);
  const low = new Array<number>(n).fill(0);
  const onStack = new Array<boolean>(n).fill(false);
  const stack: number[] = [];
  const out: number[][] = [];
  let counter = 0;
  for (let root = 0; root < n; root++) {
    if (index[root] >= 0) continue;
    const work: [number, number][] = [[root, 0]];
    while (work.length) {
      const top = work[work.length - 1];
      const v = top[0];
      if (top[1] === 0) {
        index[v] = low[v] = counter++;
        stack.push(v);
        onStack[v] = true;
      }
      if (top[1] < adj[v].length) {
        const w = adj[v][top[1]++];
        if (index[w] < 0) work.push([w, 0]);
        else if (onStack[w]) low[v] = Math.min(low[v], index[w]);
        continue;
      }
      work.pop();
      if (work.length) {
        const parent = work[work.length - 1][0];
        low[parent] = Math.min(low[parent], low[v]);
      }
      if (low[v] === index[v]) {
        const comp: number[] = [];
        let w = -1;
        while (w !== v) {
          w = stack.pop() as number;
          onStack[w] = false;
          comp.push(w);
        }
        out.push(comp);
      }
    }
  }
  return out;
}

/**
 * Returns the number of files in the largest cycle that spans more than one
 * future crate after planned cuts 1..k (k = 15 adds the crash seam), with the
 * entry points treated as pure sources, as the mockup's shatter curve does.
 */
export function cycleSizeAfter(data: CcData, k: number): number {
  const z = data.zig;
  const plan = new Map(z.plan);
  const entry = new Set(z.entry);
  const adj: number[][] = z.files.map(() => []);
  z.edges.forEach(([a, b, t], i) => {
    if (entry.has(b)) return;
    const p = plan.get(i) ?? 0;
    if (p > 0 && p <= k) return;
    if (k >= CRASH_SEAM && t === CRASH_SEAM) return;
    adj[a].push(b);
  });
  let best = 0;
  for (const c of tarjan(adj)) {
    if (c.length < 2 || c.length <= best) continue;
    if (new Set(c.map((i) => z.files[i][7])).size > 1) best = c.length;
  }
  return best;
}

/** Returns the shatter curve: cycleSizeAfter for k = 0 (no cuts) through 15 (all cuts plus the crash seam). */
export function shatterCurve(data: CcData): number[] {
  return Array.from({ length: CRASH_SEAM + 1 }, (_, k) =>
    cycleSizeAfter(data, k),
  );
}

/** The file import graph: outgoing targets (sorted by file name) and incoming sources, plus edge indices. */
export type FileGraph = {
  names: string[];
  outs: number[][];
  ins: number[][];
  outEdges: number[][];
  inEdges: number[][];
};

/** Builds the file graph of the cycle; outgoing lists are sorted by target name, the order the BFS uses. */
export function fileGraph(data: CcData): FileGraph {
  const names = data.zig.files.map((f) => f[0]);
  const outEdges: number[][] = names.map(() => []);
  const inEdges: number[][] = names.map(() => []);
  data.zig.edges.forEach(([a, b], i) => {
    outEdges[a].push(i);
    inEdges[b].push(i);
  });
  const edges = data.zig.edges;
  for (const list of outEdges) {
    list.sort((x, y) => (names[edges[x][1]] < names[edges[y][1]] ? -1 : 1));
  }
  return {
    names,
    outs: outEdges.map((l) => l.map((e) => edges[e][1])),
    ins: inEdges.map((l) => l.map((e) => edges[e][0])),
    outEdges,
    inEdges,
  };
}

/**
 * Returns the shortest cycle through file f as a path [f, …, f] (breadth-first
 * search over imports in name order), or [] when f is on no cycle.
 */
export function shortestCycle(g: FileGraph, f: number): number[] {
  const prev = new Map<number, number>([[f, -1]]);
  const queue = [f];
  for (let qi = 0; qi < queue.length; qi++) {
    const x = queue[qi];
    for (const w of g.outs[x]) {
      if (w === f) {
        const path = [x];
        let p = prev.get(x) as number;
        while (p >= 0) {
          path.push(p);
          p = prev.get(p) as number;
        }
        path.reverse();
        path.push(f);
        return path;
      }
      if (!prev.has(w)) {
        prev.set(w, x);
        queue.push(w);
      }
    }
  }
  return [];
}

/** Returns how many files have a shortest cycle of each length (in hops), keyed by length. */
export function cycleLengthCounts(g: FileGraph): Record<string, number> {
  const out: Record<string, number> = {};
  g.names.forEach((_, f) => {
    const k = String(shortestCycle(g, f).length - 1);
    out[k] = (out[k] ?? 0) + 1;
  });
  return out;
}

/** Returns the n most-imported files inside the cycle (ties keep data order). */
export function hubs(
  data: CcData,
  n: number,
): { index: number; name: string; in: number }[] {
  return data.zig.files
    .map((f, index) => ({ index, name: f[0], in: f[4] }))
    .sort((a, b) => b.in - a.in)
    .slice(0, n);
}

/** Returns how many of the 14 planned cuts use each mechanism. */
export function mechanismCounts(data: CcData): Record<Cut["m"], number> {
  const out = { trait: 0, move: 0, split: 0, inject: 0, other: 0 };
  for (const c of data.text.cuts) out[c.m]++;
  return out;
}

/** Returns the sorted, distinct cut numbers (1-20) on edges into or out of file f. */
export function cutsTouching(data: CcData, g: FileGraph, f: number): number[] {
  const tags = [...g.outEdges[f], ...g.inEdges[f]]
    .map((e) => data.zig.edges[e][2])
    .filter((t) => t > 0 && t < 21);
  return [...new Set(tags)].sort((a, b) => a - b);
}

/** A crate-graph node with named fields. */
export type CrateNode = {
  id: string;
  tier: string;
  depth: number;
  loc: number;
  tests: number;
  yf: number;
  fanin: number;
  fanout: number;
  tin: number;
  tout: number;
  zf: number;
  zscc: number;
  zdirs: [string, number][];
  mod: string | null;
  modc: string | null;
  zcode: number | null;
  mrcode: number | null;
  ztests: number | null;
  mrtests: number | null;
  birth: string | null;
  bcommit: string | null;
  k1000: string | null;
  uns: number | null;
  unsk: number | null;
  vend: number | null;
  pkg: string | null;
  sys: boolean;
};

/** Converts the crate rows of the data file into named node objects. */
export function crateNodes(data: CcData): CrateNode[] {
  return data.crates.nodes.map((r) => ({
    id: r[0],
    tier: r[1],
    depth: r[2],
    loc: r[3],
    tests: r[4],
    yf: r[5],
    fanin: r[6],
    fanout: r[7],
    tin: r[8],
    tout: r[9],
    zf: r[10],
    zscc: r[11],
    zdirs: r[12],
    mod: r[13],
    modc: r[14],
    zcode: r[15],
    mrcode: r[16],
    ztests: r[17],
    mrtests: r[18],
    birth: r[19],
    bcommit: r[20],
    k1000: r[21],
    uns: r[22],
    unsk: r[23],
    vend: r[24],
    pkg: r[25],
    sys: r[0].endsWith("-sys"),
  }));
}

/**
 * Returns the longest dependency chain of the crate graph (normal and build
 * edges): from the deepest package, repeatedly step to the deepest
 * dependency (ties broken by the larger name), as the compute script does.
 */
export function longestChain(data: CcData): string[] {
  const adj = new Map<string, string[]>();
  for (const [s, t, k] of data.crates.edges) {
    if (k === "dev") continue;
    adj.set(s, [...(adj.get(s) ?? []), t]);
  }
  const memo = new Map<string, number>();
  /** Returns the length of the longest path from v to a leaf. */
  const depth = (v: string): number => {
    const known = memo.get(v);
    if (known !== undefined) return known;
    const d = 1 + Math.max(-1, ...(adj.get(v) ?? []).map(depth));
    memo.set(v, d);
    return d;
  };
  /** Orders a before b by (depth, name), the compute script's max key. */
  const better = (a: string, b: string) =>
    depth(a) > depth(b) || (depth(a) === depth(b) && a > b);
  let cur = data.crates.nodes
    .map((n) => n[0])
    .reduce((a, b) => (better(b, a) ? b : a));
  const out = [cur];
  for (let next = adj.get(cur); next?.length; next = adj.get(cur)) {
    cur = next.reduce((a, b) => (better(b, a) ? b : a));
    out.push(cur);
  }
  return out;
}

/** Returns the physical lines of replay series key on day index i (0 before the series starts). */
export function seriesValue(data: CcData, key: string, i: number): number {
  const s = data.replay.s[key];
  if (!s || i < s[0]) return 0;
  return s[1][Math.min(i - s[0], s[1].length - 1)];
}

/**
 * Returns the birth date of a crate: its first commit, or for a package with
 * no recorded first commit (xtask) the first day its line series is non-zero.
 */
export function birthOf(data: CcData, n: CrateNode): string | null {
  if (n.birth) return n.birth;
  const s = data.replay.s[n.id];
  return s ? data.replay.dates[s[0]] : null;
}

/** The replay state on one day: counters, the per-depth share of final lines, and lines per crate. */
export type ReplayFrame = {
  date: string;
  born: number;
  total: number;
  past1k: number;
  tests: number;
  /** Share of each depth layer's final lines reached so far (null: no lines at that depth). */
  share: (number | null)[];
  /** Lines per crate id on the day (sys crates are one aggregate, so null). */
  lines: Record<string, number | null>;
  /** Whether each crate id exists on the day. */
  alive: Record<string, boolean>;
};

/** Returns the replay frame for day index i (0 = 2026-06-21). */
export function replayFrame(data: CcData, i: number): ReplayFrame {
  const date = data.replay.dates[i];
  const last = data.replay.n - 1;
  const nodes = crateNodes(data);
  const maxDepth = Math.max(...nodes.map((n) => n.depth));
  const now = new Array<number>(maxDepth + 1).fill(0);
  const fin = new Array<number>(maxDepth + 1).fill(0);
  const lines: Record<string, number | null> = {};
  const bornMap: Record<string, boolean> = {};
  let born = 0;
  let past1k = 0;
  for (const n of nodes) {
    const b = birthOf(data, n);
    bornMap[n.id] = b !== null && b <= date;
    if (bornMap[n.id]) born++;
    if (n.sys) {
      lines[n.id] = null;
      continue;
    }
    const v = seriesValue(data, n.id, i);
    lines[n.id] = v;
    now[n.depth] += v;
    fin[n.depth] += seriesValue(data, n.id, last);
    if (v >= 1000) past1k++;
  }
  const total = Object.keys(data.replay.s).reduce(
    (a, k) => a + seriesValue(data, k, i),
    0,
  );
  let tests = 0;
  for (const [d, t] of data.replay.tests) if (d <= date) tests = t;
  return {
    date,
    born,
    total,
    past1k,
    tests,
    share: fin.map((f, d) => (f ? now[d] / f : null)),
    lines,
    alive: bornMap,
  };
}

/** Returns the largest final physical-line count of any crate series (sys aggregate excluded), the replay's size scale. */
export function maxReplayLines(data: CcData): number {
  const last = data.replay.n - 1;
  return Math.max(
    ...Object.keys(data.replay.s)
      .filter((k) => k !== "__sys")
      .map((k) => seriesValue(data, k, last)),
  );
}

/** Returns the day number of ISO date d counted from d0 (d0 is day 1). */
export function dayNumber(d0: string, d: string): number {
  return Math.round((Date.parse(d) - Date.parse(d0)) / 864e5) + 1;
}
