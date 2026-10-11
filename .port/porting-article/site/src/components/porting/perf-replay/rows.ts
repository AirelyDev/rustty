/**
 * Rows of the widget's log-ratio charts (July final, 33 configs, advanced pin,
 * APC fix, current pin, memory and size), each with its tooltip text. Pure and
 * free of runtime imports so the helper tests can load it directly.
 */
import type { MedianRuns, PrData, Runs } from "./types";

/** Tooltip text: a bold head, plain lines and an optional muted footer. */
export type Tip = { head: string; lines: string[]; foot?: string };

/** One row of a log-ratio chart (ratios are Rustty ÷ Ghostty; < 1 is faster or smaller). */
export type RatioRow = {
  name: string;
  /** Spawn-floor or short config (percentages mean little). */
  floor?: boolean;
  /** Hollow "before" ratio, if any. */
  a?: number;
  /** Filled "now" ratio. */
  b?: number;
  /** Range of the "now" ratio. */
  blo?: number;
  bhi?: number;
  /** Range of the "before" ratio. */
  alo?: number;
  ahi?: number;
  /** Right-hand value text. */
  lab?: string;
  /** Small gold mark above the dot. */
  mark?: string;
  /** No value yet: drawn as a hatched slot. */
  pending?: boolean;
  tip?: Tip;
};

/** "92.47 ms [91.2–94.0]" for a [median, min, max] triple. */
function runs(r: Runs): string {
  return `${r[0]} ms [${r[1]}–${r[2]}]`;
}

/** The final July sweep against the 07-15 start, in replay order (largest speed-up first). */
export function julyRows(data: PrData): RatioRow[] {
  const first = data.stages[0];
  const fin = data.stages[data.stages.length - 1];
  const floor = new Set(data.floor);
  const r = (b: string) => fin.rust[b][0] / fin.zig[b][0];
  return [...data.bench]
    .sort((x, y) => r(x) - r(y))
    .map((b) => {
      const R = fin.rust[b];
      const Z = fin.zig[b];
      return {
        name: b,
        floor: floor.has(b),
        a: first.rust[b][0] / first.zig[b][0],
        b: R[0] / Z[0],
        blo: R[1] / Z[2],
        bhi: R[2] / Z[1],
        lab: `${(Z[0] / R[0]).toFixed(2)}× faster`,
        tip: { head: b, lines: [`Rust ${runs(R)}`, `Zig ${runs(Z)}`] },
      };
    });
}

/** The 33 configs vs Ghostty main: before (hollow, thin range) → after (filled, thick range), sorted. */
export function full33Rows(data: PrData): RatioRow[] {
  return data.full33
    .map((r) => {
      const b = r.fR[0] / r.fZ[0];
      const a = r.bR[0] / r.bZ[0];
      return {
        name: r.id.split("/")[0],
        floor: r.fR[0] < 20 && r.fZ[0] < 20,
        a,
        b,
        blo: r.fR[1] / r.fZ[2],
        bhi: r.fR[2] / r.fZ[1],
        alo: r.bR[1] / r.bZ[2],
        ahi: r.bR[2] / r.bZ[1],
        lab: `${b.toFixed(3)}  ${r.fR[0].toFixed(0)}/${r.fZ[0].toFixed(0)} ms`,
        tip: {
          head: r.id,
          lines: [
            `before fixes: Rust ${r.bR[0]} / Zig ${r.bZ[0]} ms (ratio ${a.toFixed(3)})`,
            `after: Rust ${runs(r.fR)} / Zig ${runs(r.fZ)}`,
          ],
        },
      };
    })
    .sort((x, y) => x.b - y.b);
}

/** The advanced-pin re-measure: dot = first run, bar = range across the two runs; thinnest marked. */
export function advPinRows(data: PrData): RatioRow[] {
  const rows = data.advPin.rows;
  const thin = rows.reduce((m, r) => (r.run1 > m.run1 ? r : m), rows[0]);
  return rows
    .map((r) => ({
      name: r.bench,
      floor: r.ghostty < 10,
      b: r.run1,
      blo: Math.min(r.run1, r.run2),
      bhi: Math.max(r.run1, r.run2),
      lab: `${r.run1.toFixed(3)} / ${r.run2.toFixed(3)}`,
      mark: r === thin ? "thinnest" : undefined,
      tip: {
        head: r.bench,
        lines: [
          `Ghostty ${r.ghostty} ms · Rustty ${r.rustty} ms`,
          `run 1 (9 paired runs) ${r.run1} · run 2 (full check) ${r.run2}`,
        ],
      },
    }))
    .sort((x, y) => x.b - y.b);
}

/** The APC scan and hyperlink fix: 3 configs × 3 stages, Rustty ÷ Ghostty. */
export function apcRows(data: PrData): RatioRow[] {
  return data.apc.map((r) => ({
    name: `${r.config} · ${r.stage}`,
    b: r.ratio,
    lab: `${r.ratio.toFixed(2)}× · ${r.commit}`,
    tip: { head: `${r.config} · ${r.stage}`, lines: [r.metric] },
  }));
}

/** "92.47 ms", plus " [min–max]" when the run recorded its range. */
function medianRuns(r: MedianRuns): string {
  return r[1] === null || r[2] === null
    ? `${r[0]} ms`
    : `${r[0]} ms [${r[1]}–${r[2]}]`;
}

/** Whether every pin row carries its min and max (else the chart draws medians only). */
export function pinHasRanges(data: PrData): boolean {
  return data.pin.rows.every((r) =>
    [r.rustty[1], r.rustty[2], r.ghostty[1], r.ghostty[2]].every(
      (v) => v !== null,
    ),
  );
}

/** The current-pin slot: hatched pending rows until a run is recorded, then its medians and, when recorded, ranges. */
export function pinRows(data: PrData): RatioRow[] {
  const floor = new Set(data.floor);
  if (data.pin.status !== "measured") {
    return julyRows(data).map((r) => ({
      name: r.name,
      floor: floor.has(r.name),
      pending: true,
    }));
  }
  return data.pin.rows
    .map((r) => {
      const b = r.rustty[0] / r.ghostty[0];
      const [, rlo, rhi] = r.rustty;
      const [, glo, ghi] = r.ghostty;
      const ranged =
        rlo !== null && rhi !== null && glo !== null && ghi !== null;
      return {
        name: r.bench,
        floor: floor.has(r.bench),
        b,
        blo: ranged ? rlo / ghi : undefined,
        bhi: ranged ? rhi / glo : undefined,
        lab: `${b.toFixed(3)}  ${r.rustty[0]}/${r.ghostty[0]} ms`,
        tip: {
          head: r.bench,
          lines: [
            `Rustty ${medianRuns(r.rustty)}`,
            `Ghostty ${medianRuns(r.ghostty)}`,
          ],
        },
      };
    })
    .sort((x, y) => x.b - y.b);
}

/** Memory and binary size, Rustty ÷ Ghostty. */
export function memoryRows(data: PrData): RatioRow[] {
  return data.memory.map((m) => {
    const b = m.rust / m.zig;
    return {
      name: m.name,
      b,
      lab: m.lab,
      tip: {
        head: m.name,
        lines: [`Rustty ÷ Ghostty = ${b.toFixed(3)}`, m.lab],
        foot: m.note || undefined,
      },
    };
  });
}
