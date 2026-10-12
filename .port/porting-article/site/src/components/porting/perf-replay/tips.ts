/** Tooltip text for every hover target of the perf-replay widget, keyed by its data-tip id. */
import { hhmm, ratio, spread } from "./model";
import {
  advPinRows,
  apcRows,
  full33Rows,
  julyRows,
  memoryRows,
  pinRows,
  type RatioRow,
  type Tip,
} from "./rows";
import type { PrData } from "./types";

/** CHART_ROWS maps a ratio chart's key to its row builder. */
export const CHART_ROWS: Record<string, (d: PrData) => RatioRow[]> = {
  july: julyRows,
  full33: full33Rows,
  adv: advPinRows,
  apc: apcRows,
  pin: pinRows,
  mem: memoryRows,
};

/** Formats a ratio as "2.85× faster" / "1.20× slower" / "parity". */
export function ratioText(v: number): string {
  if (Math.abs(v - 1) < 1e-9) return "parity";
  return v < 1
    ? `${(1 / v).toFixed(v > 0.5 ? 2 : 1)}× faster`
    : `${v.toFixed(2)}× slower`;
}

/** Signed number with U+2212 for negatives. */
function signed(n: number, digits = 1): string {
  const s = Math.abs(n).toFixed(digits);
  return n > 0 ? `+${s}` : n < 0 ? `−${s}` : s;
}

/** Tooltip of a replay row: Rustty and Zig runs at the current step, ratio, range and the 07-15 start. */
function rowTip(data: PrData, b: string, cur: number): Tip {
  const st = data.stages[cur];
  const R = st.rust[b];
  const Z = st.zig[b];
  const r = ratio(st, b);
  const [lo, hi] = spread(st, b);
  return {
    head: `${b}${data.floor.includes(b) ? " (spawn-floor config)" : ""}`,
    lines: [
      `rustty-bench: ${R[0]} ms (min ${R[1]}, max ${R[2]})`,
      `ghostty-bench: ${Z[0]} ms (min ${Z[1]}, max ${Z[2]})`,
      `ratio ${r.toFixed(3)} · ${ratioText(r)}`,
      `range ${lo.toFixed(3)} – ${hi.toFixed(3)}`,
    ],
    foot: `07-15 start: ${ratio(data.stages[0], b).toFixed(3)}`,
  };
}

/** Returns the tooltip for a data-tip id at replay step cur, or null for an unknown id. */
export function tipFor(data: PrData, id: string, cur: number): Tip | null {
  const [kind, a, b] = id.split(":");
  if (kind === "row") return rowTip(data, a, cur);
  if (kind === "node") {
    const s = data.stages[Number(a)];
    return {
      head: `${hhmm(s.t)} · ${s.commit}`,
      lines: [
        s.label,
        `${s.W}/${s.T}/${s.L} · suite ${signed(s.suiteD)}% · geomean ${s.geo}`,
      ],
      foot: "Enter or click: show this step",
    };
  }
  if (kind === "commit") {
    const c = data.commits[Number(a)];
    return {
      head: `${c.h} · ${c.t.replace("T", " ")}`,
      lines: [c.s],
      foot: [c.stat, c.files.join(", ")].filter(Boolean).join(" · "),
    };
  }
  if (kind === "chain") {
    const c = data.lto.chain[Number(a)];
    return {
      head: `${c.fix} · ${c.commit} · ${c.verdict}`,
      lines: [
        "built with the rejected LTO profile:",
        ...Object.entries(c.ms).map(([k, v]) => `${k} ${v} ms`),
      ],
    };
  }
  if (kind === "neg") {
    const n = data.negMarks[Number(a)];
    return {
      head: n.exp,
      lines: [n.effect],
      foot: "placed at the round it was tried in (approximate)",
    };
  }
  if (kind === "wf") {
    const w = data.waterfall[Number(a)];
    const s = w.steps[Number(b)];
    return {
      head: `${w.bench} · ${s.step}`,
      lines: [
        `commits: ${s.commits.join(", ") || "none (re-measure)"}`,
        `Rust ${s.rust} ms · Zig ${s.zig} ms (same session)`,
      ],
      foot: `results: ${s.results}`,
    };
  }
  if (kind === "lto") {
    const p = data.lto.profiles[Number(b)];
    const v = p.ms[a];
    const base = data.lto.profiles[1].ms[a];
    return {
      head: a,
      lines: [p.name, `${v} ms · ${signed((v / base - 1) * 100)}% vs shipped`],
    };
  }
  if (kind === "ct") {
    const c = data.comptime[Number(a)];
    const s = c.steps[Number(b)];
    const unit = c.ratio ? "×" : "%";
    return {
      head: `step ${s.order}: ${s.fix}`,
      lines: [
        s.cls ? `class: ${s.cls}` : "starting gap",
        `after: ${c.ratio ? s.value : signed(s.value)}${unit}`,
      ],
      foot: s.src,
    };
  }
  if (kind === "rc") {
    const rows = CHART_ROWS[a]?.(data);
    return rows?.[Number(b)]?.tip ?? null;
  }
  return null;
}
