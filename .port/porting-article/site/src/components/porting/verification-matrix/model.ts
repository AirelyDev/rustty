/** Pure view-model for the verification-matrix widget: data types, cell lookup, history layout and the totals behind the hero, bar charts and legends. */

/** A run's status: green, found bugs then fixed and green ("fixed"), did not complete, or pending. */
export type Status = "green" | "fixed" | "incomplete" | "pending";

/** One recorded (or pending) run of a tool on a host at a reference commit, as generated into verification-matrix.json. */
export type Run = {
  /** Index of the reference commit in `pins`. */
  pin: number;
  /** Tool key (see `tools`). */
  tool: string;
  /** Host key (see `hosts`). */
  host: string;
  st: Status;
  /** Tests run, or null when the record gives none. */
  tests: number | null;
  /** True when the suite covered only the packages the move touched, not the whole workspace. */
  targeted: boolean;
  /** Wall time in seconds, or null when not recorded. */
  secs: number | null;
  /** Date as MM-DD (2026). */
  date: string;
  /** The record sentence. */
  txt: string;
  /** Defects in Rustty's own code found by the run. */
  port: number;
  /** Defects in Ghostty or its dependencies found by the run. */
  gh: number;
  /** Where the run is written down. */
  src: string;
};

/** A reference commit of the port. */
export type Pin = { sha: string; date: string; label: string; note: string };
/** A host column: key, short name and hardware line. */
export type Host = { key: string; name: string; hw: string };
/** A tool row: key and spelled-out name. */
export type Tool = { key: string; name: string };

/** The generated data file. */
export type VmData = {
  pins: Pin[];
  hosts: Host[];
  tools: Tool[];
  /** Reasons a tool does not exist on a host, keyed "tool|host". */
  na: Record<string, string>;
  runs: Run[];
  durations: { name: string; host: string; secs: number }[];
  /** The reference commit whose runs the wall-time bars show. */
  durationsRef: string;
  hero: {
    /** The current reference commit. */
    ref: string;
    /** The headline lanes, each at the latest reference commit it completed at. */
    lanes: HeroLane[];
    runs: number;
    ghostty: number;
    /** The reference commit of the Miri seconds tile (the first complete matrix). */
    miriRef: string;
    miriSecs: number;
    crates: number;
    speedup: number;
    before: number;
    after: number;
  };
  foundNote: string;
};

/** One headline lane: a sanitizer or Valgrind on a host, with its tests at the latest reference commit it completed at. */
export type HeroLane = {
  tool: string;
  host: string;
  tests: number;
  /** The reference commit of that run. */
  pin: string;
};

/** Returns the headline lanes grouped by reference commit, in order of first appearance: [commit, lanes]. */
export function heroByPin(data: VmData): [string, HeroLane[]][] {
  const out: [string, HeroLane[]][] = [];
  for (const l of data.hero.lanes) {
    const g = out.find(([p]) => p === l.pin);
    if (g) g[1].push(l);
    else out.push([l.pin, [l]]);
  }
  return out;
}

/** What the matrix shows for a (commit, tool, host) cell. */
export type CellView =
  | { kind: "none" }
  | { kind: "na"; reason: string }
  | {
      kind: "run";
      /** The latest run recorded in the cell. */
      run: Run;
      /** Index of `run` in `data.runs`. */
      index: number;
      /** Port defects summed over every run in the cell. */
      port: number;
      /** Ghostty-side defects summed over every run in the cell. */
      gh: number;
      /** Number of earlier runs recorded in the same cell. */
      earlier: number;
    };

/** Display name, colour (the article palette: pass, fail, yellow, gray) and marker shape of each status; colour is never the only encoding. The model imports nothing so Node can run it directly. */
export const STATUS_INFO: Record<
  Status,
  {
    name: string;
    cell: string;
    color: string;
    shape: "circle" | "diamond" | "triangle" | "ring";
  }
> = {
  green: { name: "green", cell: "green", color: "#98c379", shape: "circle" },
  fixed: {
    name: "found bugs, fixed, green",
    cell: "green after fixes",
    color: "#e06c75",
    shape: "diamond",
  },
  incomplete: {
    name: "did not complete",
    cell: "did not complete",
    color: "#e6c07b",
    shape: "triangle",
  },
  pending: {
    name: "pending",
    cell: "pending",
    color: "#858a84",
    shape: "ring",
  },
};

/** Formats an integer with thousands separators, or "" for null. */
export function fmt(n: number | null): string {
  return n === null ? "" : n.toLocaleString("en-US");
}

/** Formats a wall time: hours with one decimal from one hour, whole minutes from one minute, else seconds; "" for null. */
export function dur(s: number | null): string {
  if (s === null) return "";
  if (s >= 3600) return `${(s / 3600).toFixed(1)} h`;
  if (s >= 60) return `${Math.round(s / 60)} min`;
  return `${s} s`;
}

/** Returns the tool's spelled-out name. */
export function toolName(data: VmData, key: string): string {
  return data.tools.find((t) => t.key === key)?.name ?? key;
}

/** Returns the host's short name. */
export function hostName(data: VmData, key: string): string {
  return data.hosts.find((h) => h.key === key)?.name ?? key;
}

/** Returns every run recorded for a tool on a host at a reference commit, in record order. */
export function cellRuns(
  data: VmData,
  pin: number,
  tool: string,
  host: string,
): Run[] {
  return data.runs.filter(
    (r) => r.pin === pin && r.tool === tool && r.host === host,
  );
}

/** Returns what the matrix shows in a cell: the latest run with summed defects, the "not available" reason, or no record. */
export function cellView(
  data: VmData,
  pin: number,
  tool: string,
  host: string,
): CellView {
  const rs = cellRuns(data, pin, tool, host);
  if (rs.length === 0) {
    const reason = data.na[`${tool}|${host}`];
    return reason ? { kind: "na", reason } : { kind: "none" };
  }
  const run = rs[rs.length - 1];
  return {
    kind: "run",
    run,
    index: data.runs.indexOf(run),
    port: rs.reduce((a, r) => a + r.port, 0),
    gh: rs.reduce((a, r) => a + r.gh, 0),
    earlier: rs.length - 1,
  };
}

/** Returns the status of every run at a reference commit, in record order (the dot strip of the selector). */
export function pinStrip(data: VmData, pin: number): Status[] {
  return data.runs.filter((r) => r.pin === pin).map((r) => r.st);
}

/** The sanitizer and Valgrind tools whose test counts make up the headline run count. */
export const SANITIZER_TOOLS: readonly string[] = [
  "asan",
  "lsan",
  "msan",
  "tsan",
  "hwasan",
  "vg",
];

/** Returns the tests run under a sanitizer or Valgrind at a reference commit, one entry per run in record order. */
export function sanitizerRuns(
  data: VmData,
  pin: number,
): { tool: string; host: string; tests: number }[] {
  return data.runs
    .filter(
      (r) =>
        r.pin === pin && SANITIZER_TOOLS.includes(r.tool) && r.tests !== null,
    )
    .map((r) => ({ tool: r.tool, host: r.host, tests: r.tests as number }));
}

/** Returns the total of sanitizerRuns at a reference commit. */
export function sanitizerTotal(data: VmData, pin: number): number {
  return sanitizerRuns(data, pin).reduce((a, r) => a + r.tests, 0);
}

/** Returns the defects found across all recorded runs: in Rustty's code and in Ghostty. */
export function defectTotals(data: VmData): { port: number; gh: number } {
  return {
    port: data.runs.reduce((a, r) => a + r.port, 0),
    gh: data.runs.reduce((a, r) => a + r.gh, 0),
  };
}

/** One bar of the "what the lanes found" chart. */
export type FoundRow = { key: string; name: string; port: number; gh: number };

/** Returns one row per tool that found anything, port defects and Ghostty-side defects summed over its runs, largest first (ties keep tool order). */
export function foundByTool(data: VmData): FoundRow[] {
  const rows = data.tools.map((t) => ({
    key: t.key,
    name: t.name,
    port: data.runs
      .filter((r) => r.tool === t.key)
      .reduce((a, r) => a + r.port, 0),
    gh: data.runs.filter((r) => r.tool === t.key).reduce((a, r) => a + r.gh, 0),
  }));
  return rows
    .filter((r) => r.port || r.gh)
    .sort((a, b) => b.port + b.gh - (a.port + a.gh));
}

/** Geometry of the lane-history plot (SVG user units). */
export const HIST = {
  left: 150,
  width: 640,
  top: 26,
  row: 24,
  pad: 6,
  spread: 9,
} as const;

/** One dot of the lane-history plot. */
export type HistDot = {
  /** Index of the run in `data.runs`. */
  k: number;
  x: number;
  y: number;
  /** Radius: grows with the square root of the defects found; pending dots are small rings. */
  r: number;
  st: Status;
  pin: number;
};

/** Returns the x centre of a reference commit's column in the lane-history plot. */
export function histX(data: VmData, pin: number): number {
  const cw = (HIST.width - HIST.left - HIST.pad) / data.pins.length;
  return HIST.left + pin * cw + cw / 2;
}

/** Returns the y centre of a tool's row in the lane-history plot. */
export function histY(toolIndex: number): number {
  return HIST.top + toolIndex * HIST.row + HIST.row / 2;
}

/** Returns the total height of the lane-history plot for a number of tools. */
export function histHeight(tools: number): number {
  return HIST.top + tools * HIST.row + HIST.pad;
}

/** Returns one dot per recorded run, laid out per tool row and commit column (several runs in one cell spread sideways). */
export function historyDots(data: VmData): HistDot[] {
  const dots: HistDot[] = [];
  data.tools.forEach((t, ti) => {
    data.pins.forEach((_p, pi) => {
      const rs = data.runs.filter((r) => r.pin === pi && r.tool === t.key);
      rs.forEach((r, j) => {
        dots.push({
          k: data.runs.indexOf(r),
          x: histX(data, pi) + (j - (rs.length - 1) / 2) * HIST.spread,
          y: histY(ti),
          r:
            r.st === "pending"
              ? 3.5
              : 3.5 + Math.min(4, Math.sqrt(r.port + r.gh)),
          st: r.st,
          pin: pi,
        });
      });
    });
  });
  return dots;
}

/** Returns the bar length for a wall time, as a fraction of the longest duration row. */
export function durFraction(data: VmData, secs: number): number {
  return secs / Math.max(...data.durations.map((d) => d.secs));
}

/** Returns the heading of a run: "tool · host". */
export function runHeading(data: VmData, r: Run): string {
  return `${toolName(data, r.tool)} · ${hostName(data, r.host)}`;
}

/** Returns the wall-time rows of the last green full run, longest first. */
export function durationRows(data: VmData): VmData["durations"] {
  return [...data.durations].sort((a, b) => b.secs - a.secs);
}

/** TOOL_ABBR maps the sanitizer tool keys to the abbreviations the record sentences use; the widget legend defines each one once. */
export const TOOL_ABBR: Record<string, string> = {
  asan: "ASan",
  lsan: "LSan",
  msan: "MSan",
  tsan: "TSan",
  hwasan: "HWASan",
};

/** Returns the first and last date (MM-DD) of the sanitizer and Valgrind runs recorded at a reference commit, or null when there are none. */
export function sanitizerSpan(
  data: VmData,
  pin: number,
): [string, string] | null {
  const dates = data.runs
    .filter(
      (r) =>
        r.pin === pin && SANITIZER_TOOLS.includes(r.tool) && r.tests !== null,
    )
    .map((r) => r.date)
    .sort();
  return dates.length ? [dates[0], dates[dates.length - 1]] : null;
}
