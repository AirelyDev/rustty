/**
 * Derived, memoised structures the "Cutting the cycle" views share: layout,
 * file graph, cut lookups, grouped edge sets, crate edges and keyboard orders.
 */
import { PALETTE } from "../lib/palette";
import {
  type FilePt,
  fullName,
  type Layout,
  layout,
  shortName,
} from "./geometry";
import {
  type CcData,
  type Cut,
  type FileGraph,
  fileGraph,
  hubs,
  maxReplayLines,
} from "./model";

/** GLOBAL_TAG is the edge tag of the bottom-up imports of global.zig. */
export const GLOBAL_TAG = 21;

/** MECH_LABEL maps a mechanism to its chip text. */
export const MECH_LABEL: Record<Cut["m"], string> = {
  trait: "trait",
  move: "move",
  split: "split",
  inject: "injection",
  other: "other",
};

/** A crate edge with node indices. */
export type CrateEdge = {
  s: number;
  t: number;
  k: "normal" | "dev" | "build";
  key: string;
};

/** One stroke group of file edges: a single SVG path with one style. */
export type EdgeGroup = {
  key: string;
  d: string;
  color: string;
  opacity: number;
  width: number;
  dash?: string;
};

/** Everything the views derive from the data file once. */
export type View = {
  data: CcData;
  lay: Layout;
  g: FileGraph;
  /** The 14 planned cuts followed by the 5 extra seams. */
  all: Cut[];
  cutBy: Map<number, Cut>;
  /** File edge indices per seam tag. */
  tagEdges: Map<number, number[]>;
  crateEdges: CrateEdge[];
  chainSet: Set<string>;
  chainEdge: Set<string>;
  /** Layering-decision text per crate edge key "source>target". */
  layerEdge: Map<string, string>;
  /** Files in clockwise order around the circle (keyboard order). */
  fileOrder: number[];
  /** Crates by depth, then top to bottom (keyboard order). */
  crateOrder: number[];
  hubs: { index: number; name: string; in: number }[];
  /** Largest final physical-line count of a crate (replay size scale). */
  maxLines: number;
};

/** Builds the shared View from the data file. */
export function buildView(data: CcData): View {
  const lay = layout(data);
  const all = [...data.text.cuts, ...data.text.extra];
  const tagEdges = new Map<number, number[]>();
  data.zig.edges.forEach(([, , t], i) => {
    tagEdges.set(t, [...(tagEdges.get(t) ?? []), i]);
  });
  const crateEdges = data.crates.edges.map(([s, t, k]) => ({
    s: lay.crateIndex.get(s) ?? 0,
    t: lay.crateIndex.get(t) ?? 0,
    k,
    key: `${s}>${t}`,
  }));
  const chain = data.crates.chain;
  const layerEdge = new Map<string, string>();
  for (const l of data.text.layer) {
    for (const [a, b] of l.e) layerEdge.set(`${a}>${b}`, l.t);
  }
  /** Returns the clockwise angle of a file from 12 o'clock. */
  const angle = (f: FilePt) =>
    (Math.atan2(f.y - lay.centre.y, f.x - lay.centre.x) + 2.5 * Math.PI) %
    (2 * Math.PI);
  const fileOrder = lay.files
    .map((_, i) => i)
    .sort((a, b) => angle(lay.files[a]) - angle(lay.files[b]));
  const crateOrder = lay.crates
    .map((_, i) => i)
    .sort(
      (a, b) =>
        lay.crates[a].depth - lay.crates[b].depth ||
        lay.crates[a].y - lay.crates[b].y,
    );
  return {
    data,
    lay,
    g: fileGraph(data),
    all,
    cutBy: new Map(all.map((c) => [c.n, c])),
    tagEdges,
    crateEdges,
    chainSet: new Set(chain),
    chainEdge: new Set(chain.slice(1).map((c, i) => `${chain[i]}>${c}`)),
    layerEdge,
    fileOrder,
    crateOrder,
    hubs: hubs(data, 6),
    maxLines: maxReplayLines(data),
  };
}

/** Returns the base edge groups of the cycle view: by source colour (stage 1), or cuts red, extra seams gold, global.zig importers dashed grey and the rest faded (stage 2). */
export function baseEdgeGroups(
  v: View,
  stage: number,
  join: (edges: number[]) => string,
): EdgeGroup[] {
  const byColor = new Map<string, number[]>();
  const red: number[] = [];
  const gold: number[] = [];
  const grey: number[] = [];
  v.data.zig.edges.forEach(([a, , t], i) => {
    if (stage === 2 && t > 0 && t < GLOBAL_TAG) (t > 14 ? gold : red).push(i);
    else if (stage === 2 && t === GLOBAL_TAG) grey.push(i);
    else {
      const c = v.lay.files[a].color;
      byColor.set(c, [...(byColor.get(c) ?? []), i]);
    }
  });
  const faded = stage === 2 ? 0.035 : 0.09;
  const groups: EdgeGroup[] = [...byColor].map(([color, es]) => ({
    key: color,
    d: join(es),
    color,
    opacity: faded,
    width: 0.6,
  }));
  if (stage === 2) {
    groups.push(
      {
        key: "global",
        d: join(grey),
        color: PALETTE.gray5,
        opacity: 0.35,
        width: 0.6,
        dash: "2 2",
      },
      {
        key: "extra",
        d: join(gold),
        color: PALETTE.gold,
        opacity: 0.85,
        width: 1.4,
      },
      {
        key: "cuts",
        d: join(red),
        color: PALETTE.fail,
        opacity: 0.85,
        width: 1.4,
      },
    );
  }
  return groups;
}

/** Returns the sorted distinct cut numbers recorded for a crate: the ones it carries plus the ones that shaped it. */
export function cutsOfCrate(v: View, id: string): number[] {
  const own = v.all
    .filter((c) => c.cr && fullName(c.cr) === id)
    .map((c) => c.n);
  const more = v.data.text.crateCuts[id] ?? [];
  return [...new Set([...own, ...more])].sort((a, b) => a - b);
}

/** Returns the label of a crate for the graph (short name). */
export function crateLabel(v: View, i: number): string {
  return shortName(v.lay.crates[i].id);
}
