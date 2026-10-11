/** Types of the generated perf-replay.json data file. */

/** [median, min, max] in ms of the timed runs of one benchmark. */
export type Runs = [number, number, number];

/** What kind of sweep a replay step is. */
export type StageKind = "ship" | "stage" | "session" | "profile";

/** One recorded sweep: the Rustty build of a commit against same-session Zig. */
export type Stage = {
  /** Plain stage key (base, session, profile, round1, …, round5). */
  key: string;
  /** Results file of the Rustty sweep (under the perf records directory). */
  results: string;
  /** Results file of the same-session Zig sweep. */
  zigResults: string;
  /** Commit of the code measured. */
  commit: string;
  /** Commit time of the code measured (naive local time). */
  t: string;
  /** That commit's subject (internal labels removed). */
  subject: string;
  /** What this step measured, in plain words. */
  label: string;
  kind: StageKind;
  /** Extra note (session or profile effect), or "". */
  note: string;
  /** Rustty runs per benchmark. */
  rust: Record<string, Runs>;
  /** Same-session Zig runs per benchmark. */
  zig: Record<string, Runs>;
  W: number;
  T: number;
  L: number;
  suiteR: number;
  suiteZ: number;
  suiteD: number;
  geo: number;
  /** Benchmarks that moved ≥4% and outside their spread since the previous step: [bench, %]. */
  moved: [string, number][];
};

/** One commit of the 07-15/16 window. */
export type Commit = {
  h: string;
  t: string;
  s: string;
  stat: string;
  files: string[];
};

/** One binary of the LTO stage chain (built with the rejected LTO profile). */
export type ChainStep = {
  order: number;
  fix: string;
  commit: string;
  verdict: string;
  ms: Record<string, number>;
};

/** One build profile of the LTO experiment on identical code. */
export type LtoProfile = {
  name: string;
  label: string;
  ms: Record<string, number>;
};

/** One falsified optimisation. */
export type Negative = {
  exp: string;
  bench: string;
  date: string;
  effect: string;
  why: string;
};

/** A timeline "measured worse" mark (placed at the round it was tried in). */
export type NegMark = { exp: string; t: string; effect: string };

/** One official step of a key benchmark's waterfall. */
export type WfStep = {
  step: string;
  round: string;
  commits: string[];
  rust: number;
  zig: number;
  results: string;
};

/** A config of the 33-config matrix: before and after the fixes, Rust and Zig runs. */
export type Full33 = { id: string; bR: Runs; bZ: Runs; fR: Runs; fZ: Runs };

/** One benchmark of the advanced-pin re-measure (two independent runs). */
export type AdvPinRow = {
  bench: string;
  ghostty: number;
  rustty: number;
  run1: number;
  run2: number;
};

/** One stage of the APC scan / hyperlink fix. */
export type ApcRow = {
  config: string;
  stage: string;
  ratio: number;
  metric: string;
  commit: string;
};

/** One compile-time-flattening fix step of a benchmark. */
export type ComptimeStep = {
  order: number;
  fix: string;
  cls: string;
  value: number;
  src: string;
};

/** One hazard class of the gap table. */
export type Gap = {
  class: string;
  zig: string;
  lost: string;
  fix: string;
  example: string;
  worst: string;
};

/** One memory or size comparison (Rustty and Ghostty values in the same unit). */
export type MemRow = {
  name: string;
  rust: number;
  zig: number;
  lab: string;
  note: string;
};

/** A recorded run's [median, min, max] in ms where min and max may be missing (null): the run kept medians only. */
export type MedianRuns = [number, number | null, number | null];

/** The current-pin slot: pending until a run is recorded, then its rows. */
export type PinSlot = {
  rev: string;
  head: string;
  status: "pending" | "measured";
  date?: string;
  /** The run's record sentence about its closest benchmark, when there is one. */
  note?: string;
  rows: { bench: string; ghostty: MedianRuns; rustty: MedianRuns }[];
};

/** The generated data file. */
export type PrData = {
  bench: string[];
  floor: string[];
  args: Record<string, string>;
  stages: Stage[];
  commits: Commit[];
  lto: { chain: ChainStep[]; profiles: LtoProfile[] };
  /** The 07-30 provisional paired re-measure (perf-timeline.csv): date and "n/n" wins. */
  provisional: { date: string; wins: string };
  negatives: Negative[];
  negMarks: NegMark[];
  waterfall: { bench: string; steps: WfStep[] }[];
  full33: Full33[];
  advPin: { rev: string; date: string; head: string; rows: AdvPinRow[] };
  apc: ApcRow[];
  comptime: { bench: string; ratio: boolean; steps: ComptimeStep[] }[];
  gaps: Gap[];
  memory: MemRow[];
  pin: PinSlot;
};
