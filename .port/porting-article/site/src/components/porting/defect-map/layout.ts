/**
 * Treemap layout for the defect-map widget: squarified tiles of Ghostty's
 * Zig source (area = lines), the dependency strip and one gridded mark per
 * defect, plus the mark glyph paths. Ported from the mockup (08-defect-map).
 */
import type { Def, DmData, Glyph } from "./model";

/** DEP_NAMES names the dependency-strip tiles, in tie-break order. */
export const DEP_NAMES: Readonly<Record<string, string>> = {
  "dep:libvaxis": "libvaxis",
  "dep:libxev": "libxev",
  "dep:zig": "Zig std",
  "dep:google-highway": "Highway",
  "dep:GStreamer": "GStreamer",
  "dep:gtk": "GTK",
  "dep:zf": "zf",
  nonzig: "Ghostty, non-Zig build files",
};

/** The display name of a defect's project: Ghostty or the dependency's tile name. */
export function projectName(p: string): string {
  return p === "ghostty" ? "Ghostty" : (DEP_NAMES[`dep:${p}`] ?? p);
}

/** GEO is the map geometry in viewBox units: treemap height, dependency strip row, total height. */
export const GEO = {
  W: 960,
  mapH: 468,
  stripY: 488,
  stripH: 66,
  H: 560,
} as const;

/** MIN_SHARE is the minimum share of the total area a leaf gets per three marks it holds. */
export const MIN_SHARE = 0.0025;

/** LABEL_CHAR is the approximate width in viewBox units of one 9 px mono label character. */
const LABEL_CHAR = 5.6;

/** A rectangle with a name and value, as squarify lays it out. */
export type Rect = {
  n: string;
  v: number;
  x: number;
  y: number;
  w: number;
  h: number;
};

/**
 * Squarified treemap (Bruls et al.): lays items (value v) out in the box
 * x,y,w,h, rows along the short side, each rectangle's area ∝ v.
 */
export function squarify<T extends { n: string; v: number }>(
  items: readonly T[],
  x0: number,
  y0: number,
  w0: number,
  h0: number,
): (T & Rect)[] {
  const out: (T & Rect)[] = [];
  const total = items.reduce((s, i) => s + i.v, 0);
  if (!total) return out;
  let [x, y, w, h] = [x0, y0, Math.max(0, w0), Math.max(0, h0)];
  const scale = (w * h) / total;
  let rest = items.map((i) => ({ item: i, a: i.v * scale }));
  while (rest.length) {
    const short = Math.min(w, h);
    let row: typeof rest = [];
    let best = Number.POSITIVE_INFINITY;
    for (const q of rest) {
      const r2 = [...row, q];
      const s = r2.reduce((a, b) => a + b.a, 0);
      const mx = Math.max(...r2.map((z) => z.a));
      const mn = Math.min(...r2.map((z) => z.a));
      const worst = Math.max(
        (short * short * mx) / (s * s),
        (s * s) / (short * short * mn),
      );
      // The first item always starts a row (a zero-area box yields NaN ratios).
      if (!row.length || worst <= best) {
        best = worst;
        row = r2;
      } else break;
    }
    const s = row.reduce((a, b) => a + b.a, 0);
    if (w >= h) {
      const cw = h ? s / h : 0;
      let yy = y;
      for (const q of row) {
        const hh = cw ? q.a / cw : 0;
        out.push({ ...q.item, x, y: yy, w: cw, h: hh });
        yy += hh;
      }
      x += cw;
      w -= cw;
    } else {
      const ch = w ? s / w : 0;
      let xx = x;
      for (const q of row) {
        const ww = ch ? q.a / ch : 0;
        out.push({ ...q.item, x: xx, y, w: ww, h: ch });
        xx += ww;
      }
      y += ch;
      h -= ch;
    }
    rest = rest.slice(row.length);
  }
  return out;
}

/** A text label placed on the map. */
export type MapLabel = { x: number; y: number; text: string; dim: boolean };

/** A leaf tile: its laid-out (minimum-area) rect, real Zig lines v0 and parent top tile name. */
export type Leaf = Rect & { v0: number; top: string; label: MapLabel | null };

/** A top-level tile with its leaves and optional label. */
export type Top = Rect & { v0: number; kids: Leaf[]; label: string | null };

/** A dependency-strip tile (sized by defect count, not code size). */
export type DepTile = Rect & { label: string; count: number };

/** One defect mark: index into defs, centre and radius. */
export type Mark = { i: number; x: number; y: number; r: number };

/** The full laid-out map. */
export type MapLayout = { tops: Top[]; deps: DepTile[]; marks: Mark[] };

/** Returns defect indices grouped by leaf name. */
export function byLeaf(defs: readonly Def[]): Map<string, number[]> {
  const m = new Map<string, number[]>();
  defs.forEach((d, i) => {
    const l = m.get(d.leaf);
    if (l) l.push(i);
    else m.set(d.leaf, [i]);
  });
  return m;
}

/** Formats a Zig line count as the tile label does: 182k, 4.4k, 870. */
export function fmtLines(v: number): string {
  if (v < 1000) return String(v);
  return `${(v / 1000).toFixed(v >= 10000 ? 0 : 1)}k`;
}

/** Cuts a label to the width w (viewBox units), ending with an ellipsis. */
function fit(name: string, w: number): string {
  return name.length * LABEL_CHAR > w - 6
    ? `${name.slice(0, Math.max(0, Math.floor((w - 8) / LABEL_CHAR)))}…`
    : name;
}

/**
 * Grids the marks of one tile: class order (order) then method, as large as fits
 * (cell ≤ 15), below a label row when the tile carries one at the top.
 */
function placeMarks(
  defs: readonly Def[],
  order: readonly string[],
  list: readonly number[],
  k: Rect,
  labelTop: boolean,
): Mark[] {
  const n = list.length;
  const top = labelTop ? 14 : 2;
  const aw = k.w - 4;
  const ah = k.h - top - (k.h > 30 && k.w > 46 ? 12 : 2);
  let cell = Math.min(15, Math.sqrt(Math.max(0, aw * ah) / n));
  let cols = Math.max(1, Math.floor(aw / cell));
  let rows = Math.ceil(n / cols);
  while (rows * cell > ah && cell > 5) {
    cell -= 0.5;
    cols = Math.max(1, Math.floor(aw / cell));
    rows = Math.ceil(n / cols);
  }
  const r = Math.max(2.2, Math.min(5.2, cell * 0.36));
  const ox = k.x + 2 + (aw - cols * cell) / 2 + cell / 2;
  const oy = k.y + top + cell / 2 + Math.max(0, (ah - rows * cell) / 2);
  const sorted = [...list].sort(
    (a, b) =>
      order.indexOf(defs[a].c) - order.indexOf(defs[b].c) ||
      defs[a].m.localeCompare(defs[b].m),
  );
  return sorted.map((i, j) => ({
    i,
    x: ox + (j % cols) * cell,
    y: oy + Math.floor(j / cols) * cell,
    r,
  }));
}

/**
 * Lays out the treemap (area = Zig lines, leaves holding defects raised to
 * MIN_SHARE of the total per three marks), the dependency strip (width ∝
 * defects + 1.6) and one mark per defect, each tile's marks sorted by
 * classOrder then method.
 */
export function layoutMap(
  data: DmData,
  classOrder: readonly string[],
): MapLayout {
  const leaves = byLeaf(data.defs);
  const total = data.tree.reduce((s, t) => s + t.v, 0);
  const tops = data.tree
    .map((t) => {
      const kids = t.k.map(([n, v]) => {
        const c = leaves.get(n)?.length ?? 0;
        return {
          n,
          v0: v,
          v: c ? Math.max(v, total * MIN_SHARE * Math.max(1, c / 3)) : v,
        };
      });
      return { n: t.n, v0: t.v, v: kids.reduce((s, k) => s + k.v, 0), kids };
    })
    .sort((a, b) => b.v - a.v);
  const marks: Mark[] = [];
  const laid: Top[] = squarify(tops, 0, 0, GEO.W, GEO.mapH).map((t) => {
    const pad = t.w > 60 && t.h > 40 ? 14 : 2;
    // Insets shrink with slivers so a sub-pixel tile's leaves stay inside it.
    const ix = Math.min(2, t.w / 2);
    const iy = Math.min(pad, t.h / 2);
    const kids = squarify(
      [...t.kids].sort((a, b) => b.v - a.v),
      t.x + ix,
      t.y + iy,
      t.w - 2 * ix,
      t.h - iy - Math.min(2, t.h / 2),
    ).map((k): Leaf => {
      const has = leaves.has(k.n);
      const name = k.n.split("/").pop() ?? k.n;
      let label: MapLabel | null = null;
      if (k.w > 46 && k.h > 24 && !has)
        label = { x: k.x + 4, y: k.y + 12, text: fit(name, k.w), dim: true };
      else if (k.w > 46 && k.h > 30 && has)
        label = {
          x: k.x + 4,
          y: k.y + k.h - 5,
          text: fit(name, k.w),
          dim: false,
        };
      return { ...k, top: t.n, label };
    });
    for (const k of kids) {
      const list = leaves.get(k.n);
      if (list)
        marks.push(...placeMarks(data.defs, classOrder, list, k, false));
    }
    const label = t.w > 70 && t.h > 22 ? t.n : null;
    return { ...t, kids, label };
  });
  const depKeys = Object.keys(DEP_NAMES)
    .filter((k) => leaves.has(k))
    .sort(
      (a, b) => (leaves.get(b)?.length ?? 0) - (leaves.get(a)?.length ?? 0),
    );
  const dt = depKeys.reduce(
    (s, k) => s + (leaves.get(k)?.length ?? 0) + 1.6,
    0,
  );
  let xx = 0;
  const deps = depKeys.map((k): DepTile => {
    const count = leaves.get(k)?.length ?? 0;
    const w = ((count + 1.6) / dt) * GEO.W;
    const tile = {
      n: k,
      v: count,
      x: xx,
      y: GEO.stripY,
      w: w - 3,
      h: GEO.stripH,
      label: DEP_NAMES[k],
      count,
    };
    marks.push(
      ...placeMarks(data.defs, classOrder, leaves.get(k) ?? [], tile, true),
    );
    xx += w;
    return tile;
  });
  return { tops: laid, deps, marks };
}

/** Returns an SVG path for a mark glyph of radius r centred at x,y. */
export function shapePath(s: Glyph, x: number, y: number, r: number): string {
  const P = (pts: [number, number][]) =>
    `M${pts.map((p) => p.map((v) => v.toFixed(1)).join(",")).join("L")}Z`;
  const ring = (
    k: number,
    n: number,
    f: (i: number) => number,
    g: (i: number) => number,
  ) =>
    P(
      [...Array(n)].map((_, i): [number, number] => [
        x + r * k * f(i),
        y + r * k * g(i),
      ]),
    );
  switch (s) {
    case "circle":
      return `M${x - r},${y}a${r},${r} 0 1,0 ${2 * r},0a${r},${r} 0 1,0 ${-2 * r},0Z`;
    case "diamond":
      return P([
        [x, y - r * 1.25],
        [x + r * 1.25, y],
        [x, y + r * 1.25],
        [x - r * 1.25, y],
      ]);
    case "square":
      return P([
        [x - r * 0.9, y - r * 0.9],
        [x + r * 0.9, y - r * 0.9],
        [x + r * 0.9, y + r * 0.9],
        [x - r * 0.9, y + r * 0.9],
      ]);
    case "tri":
      return P([
        [x, y - r * 1.2],
        [x + r * 1.1, y + r * 0.8],
        [x - r * 1.1, y + r * 0.8],
      ]);
    case "trileft":
      return P([
        [x - r * 1.2, y],
        [x + r * 0.8, y - r * 1.1],
        [x + r * 0.8, y + r * 1.1],
      ]);
    case "tridown":
      return P([
        [x, y + r * 1.2],
        [x + r * 1.1, y - r * 0.8],
        [x - r * 1.1, y - r * 0.8],
      ]);
    case "pent":
      return ring(
        1.1,
        5,
        (i) => Math.sin((i * 2 * Math.PI) / 5),
        (i) => -Math.cos((i * 2 * Math.PI) / 5),
      );
    case "hex":
      return ring(
        1.05,
        6,
        (i) => Math.cos((i * Math.PI) / 3),
        (i) => Math.sin((i * Math.PI) / 3),
      );
    case "star":
      return P(
        [...Array(10)].map((_, i): [number, number] => {
          const rr = i % 2 ? r * 0.55 : r * 1.3;
          return [
            x + rr * Math.sin((i * Math.PI) / 5),
            y - rr * Math.cos((i * Math.PI) / 5),
          ];
        }),
      );
    case "bow":
      return P([
        [x - r, y - r],
        [x, y - r * 0.3],
        [x + r, y - r],
        [x + r, y + r],
        [x, y + r * 0.3],
        [x - r, y + r],
      ]);
    case "cross": {
      const a = r * 0.38;
      const b = r * 1.1;
      return P([
        [x - a, y - b],
        [x + a, y - b],
        [x + a, y - a],
        [x + b, y - a],
        [x + b, y + a],
        [x + a, y + a],
        [x + a, y + b],
        [x - a, y + b],
        [x - a, y + a],
        [x - b, y + a],
        [x - b, y - a],
        [x - a, y - a],
      ]);
    }
  }
}
