/**
 * Layout of the "Cutting the cycle" SVG (viewBox 680 × 620): the radial file
 * layout of stages 1-2, the depth-column crate layout of stages 3-4, edge
 * paths and the morph targets. All positions derive from the precomputed
 * layout in the data file, so the drawing is deterministic.
 */
import { PALETTE } from "../lib/palette";
import { type CcData, type CrateNode, crateNodes } from "./model";

/** VIEW_W and VIEW_H are the main SVG's viewBox size. */
export const VIEW_W = 680;
/** VIEW_H is the main SVG's viewBox height. */
export const VIEW_H = 620;
/** CENTRE is the centre of the radial file layout (global.zig sits here). */
const CENTRE = { x: 340, y: 306 };
/** DEPTH_X0 and DEPTH_W place depth column d at DEPTH_X0 + d × DEPTH_W / max depth. */
export const DEPTH_X0 = 34;
/** DEPTH_W is the horizontal span of the depth columns. */
const DEPTH_W = 600;

/** TIER_COLOR maps each crate tier to its colour. */
export const TIER_COLOR: Record<string, string> = {
  "VT engine": PALETTE.rust,
  "C ABI / embedding": PALETTE.gold,
  Font: PALETTE.purple,
  Rendering: PALETTE.blue,
  "Runtime (termio/core/apprt)": PALETTE.pass,
  "Input / config": PALETTE.yellow,
  "CLI / binary / tools": PALETTE.orange,
  Foundations: PALETTE.gray6,
  "Ported Zig dependencies": PALETTE.upstream,
  "FFI -sys bindings": PALETTE.gray4,
};

/** TIER_SHORT maps each crate tier to the plain name used in text. */
export const TIER_SHORT: Record<string, string> = {
  "VT engine": "terminal engine",
  "C ABI / embedding": "C ABI / embedding",
  Font: "font",
  Rendering: "rendering",
  "Runtime (termio/core/apprt)": "runtime (termio, core, apprt)",
  "Input / config": "input / config",
  "CLI / binary / tools": "CLI / binary / tools",
  Foundations: "foundations",
  "Ported Zig dependencies": "ported Zig dependencies",
  "FFI -sys bindings": "C library wrappers (-sys)",
};

/** Returns the column x of crate depth d when the graph is maxDepth deep. */
export function depthX(d: number, maxDepth: number): number {
  return DEPTH_X0 + (d * DEPTH_W) / maxDepth;
}

/** Returns the short display name of a package id (rustty-vt → vt). */
export function shortName(id: string): string {
  return id === "rustty" || id === "xtask" ? id : id.replace(/^rustty-/, "");
}

/** Returns the package id of a short crate name (vt → rustty-vt). */
export function fullName(c: string): string {
  return c === "rustty" || c === "xtask" ? c : `rustty-${c}`;
}

/** A positioned file dot. */
export type FilePt = {
  x: number;
  y: number;
  r: number;
  color: string;
  /** Position of the dot inside its crate's circle (morph target). */
  home: { x: number; y: number };
  /** Depth of the crate it became (morph stagger). */
  depth: number;
};

/** A positioned crate node. */
export type CratePt = CrateNode & { x: number; y: number; r: number };

/** The full layout: files, crates, sector arcs and labels, and the layout radius. */
export type Layout = {
  files: FilePt[];
  crates: CratePt[];
  crateIndex: Map<string, number>;
  sectors: {
    d: string;
    label: string;
    arc: string;
    x: number;
    y: number;
    anchor: "start" | "middle" | "end";
  }[];
  centre: { x: number; y: number };
  /** Depth of the crate graph (longest dependency chain, in hops). */
  maxDepth: number;
};

/** Returns the SVG arc path of radius r from angle a0 to a1 around the centre. */
function arcPath(a0: number, a1: number, r: number): string {
  /** Returns the 'x y' point at angle a on the arc. */
  const p = (a: number) =>
    `${(CENTRE.x + r * Math.cos(a)).toFixed(1)} ${(CENTRE.y + r * Math.sin(a)).toFixed(1)}`;
  return `M${p(a0)}A${r} ${r} 0 ${a1 - a0 > Math.PI ? 1 : 0} 1 ${p(a1)}`;
}

/** Computes the layout of both graphs from the data file. */
export function layout(data: CcData): Layout {
  const z = data.zig;
  const nodes = crateNodes(data);
  const maxLoc = Math.max(...nodes.map((n) => n.loc));
  const maxDepth = Math.max(...nodes.map((n) => n.depth));
  const crates: CratePt[] = nodes.map((n) => ({
    ...n,
    x: depthX(n.depth, maxDepth),
    y: 40 + n.yf * 545,
    r: 2.6 + 16 * Math.sqrt(n.loc / maxLoc),
  }));
  const crateIndex = new Map(crates.map((c, i) => [c.id, i]));
  const xs = z.files.map((f) => f[2]);
  const scale = 470 / (Math.max(...xs) - Math.min(...xs));
  const files: FilePt[] = z.files.map((f, i) => {
    const c = crates[crateIndex.get(fullName(f[7])) ?? 0];
    const a = i * 2.399963;
    const rr = c.r * 0.75 * Math.sqrt(((i * 7) % 13) / 13);
    return {
      x: CENTRE.x + (f[2] - 500) * scale,
      y: CENTRE.y + (f[3] - 500) * scale,
      r: f[0] === "global.zig" ? 6 : Math.min(5.5, 2 + Math.sqrt(f[4]) * 0.42),
      color: TIER_COLOR[c.tier] ?? PALETTE.gray4,
      home: { x: c.x + rr * Math.cos(a), y: c.y + rr * Math.sin(a) },
      depth: c.depth,
    };
  });
  const rad = Math.max(
    ...files.map((f) => Math.hypot(f.x - CENTRE.x, f.y - CENTRE.y)),
  );
  const sectors = z.dirs.map((d, i) => {
    const [a0, a1] = z.sectors[i];
    const m = (a0 + a1) / 2;
    const cos = Math.cos(m);
    const count = z.files.filter((f) => f[1] === i).length;
    return {
      d,
      label: `${d === "(root)" ? "src/*.zig" : `${d}/`} ${count}`,
      arc: arcPath(a0, a1, rad + 7),
      x: CENTRE.x + (rad + 16) * cos,
      y: CENTRE.y + (rad + 16) * Math.sin(m) + 3,
      anchor: (cos > 0.25 ? "start" : cos < -0.25 ? "end" : "middle") as
        | "start"
        | "middle"
        | "end",
    };
  });
  return { files, crates, crateIndex, sectors, centre: CENTRE, maxDepth };
}

/** Returns the bundled quadratic path of a file edge (pulled 60% toward the centre). */
export function filePath(a: FilePt, b: FilePt): string {
  const mx = (a.x + b.x) / 2;
  const my = (a.y + b.y) / 2;
  const cx = mx + (CENTRE.x - mx) * 0.6;
  const cy = my + (CENTRE.y - my) * 0.6;
  return `M${a.x.toFixed(1)} ${a.y.toFixed(1)}Q${cx.toFixed(1)} ${cy.toFixed(1)} ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
}

/** Returns one path string drawing every listed file edge (one SVG path per style keeps the DOM small). */
export function filePaths(
  data: CcData,
  files: FilePt[],
  edges: number[],
): string {
  return edges
    .map((e) =>
      filePath(files[data.zig.edges[e][0]], files[data.zig.edges[e][1]]),
    )
    .join("");
}

/** Returns the horizontal cubic path of a crate edge from s to t. */
export function cratePath(
  s: { x: number; y: number },
  t: { x: number; y: number },
): string {
  const dx = (s.x - t.x) * 0.5;
  return `M${s.x.toFixed(1)} ${s.y.toFixed(1)}C${(s.x - dx).toFixed(1)} ${s.y.toFixed(1)} ${(t.x + dx).toFixed(1)} ${t.y.toFixed(1)} ${t.x.toFixed(1)} ${t.y.toFixed(1)}`;
}

/** Returns the eased morph progress (0-1) of a file at crate depth d at overall time p (0-1), staggered by depth out of maxDepth. */
export function morphProgress(p: number, d: number, maxDepth: number): number {
  const e = p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
  const delay = (d / maxDepth) * 0.35;
  return Math.max(0, Math.min(1, (e - delay) / (1 - 0.35)));
}
