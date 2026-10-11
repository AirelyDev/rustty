/** Shape of src/app/porting/data/translation-lens.json (scripts/porting/widgets/w07_translation_lens.py). */

/** One excerpt line: [line number, text] or [n, text, 1] when an internal id was removed; null is an omitted-lines gap. */
export type CodeLine = [number, string] | [number, string, 1] | null;

/** Excerpt stats shown as chips (computed by the mockup build). */
export type SubStats = {
  zig_code: number;
  zig_comment: number;
  rust_code: number;
  rust_comment: number;
  zig_comments: number;
  carried: number;
  echoes: number;
  port_note_lines: number;
  unsafe_tokens: number;
  redacted: number;
};

/** One Zig ↔ Rust excerpt pair with its line mappings. */
export type Sub = {
  key: string;
  label: string;
  zigFile: string;
  rustFile: string;
  zig: CodeLine[];
  rust: CodeLine[];
  /** Hand-drawn mappings: [Zig lines, Rust lines]. */
  map: [number[], number[]][];
  /** Zig comment carried word for word: [Zig line, Rust line]. */
  carried: [number, number][];
  /** Zig code line kept as a Rust comment: [Zig line, Rust line]. */
  echoes: [number, number][];
  stats: SubStats;
};

/** One git grep count: exactly one of rust, zig or ledger is set. */
export type CountItem = {
  label: string;
  rust?: number;
  zig?: number;
  ledger?: number;
  /** [Ghostty, libxev] split of a Zig count. */
  zig_split?: [number, number];
};

/** Occurrence counts of one pattern and its Rust sites per crate. */
export type Counts = {
  primary: CountItem & { rust: number };
  items: CountItem[];
  crates: Record<string, number>;
};

/** One isolated Miri harness: candidates with Stacked / Tree Borrows verdicts. */
export type Harness = {
  name: string;
  date: string;
  what: string;
  rows: [string, string, string][];
};

/** One step of a measured perf fix chain (order 0 is the starting gap). */
export type PerfStep = {
  bench: string;
  order: number;
  fix: string;
  hazard: string | null;
  metric: "delta_pct_vs_zig" | "rust_over_zig_x";
  value: number;
  provenance: string;
};

/** A ledger row: [file, struct, field, Zig type, class, Rust shape]. */
export type LedgerRow = [string, string, string, string, string, string];

/** One translation pattern. */
export type Pattern = {
  id: string;
  title: string;
  short: string;
  why: string;
  ledgerNote: string;
  ledgerCount: number;
  ledger: LedgerRow[];
  harness: Harness[];
  counts: Counts;
  subs: Sub[];
  /** Cell bit layout: [field, first bit, width, type]. */
  bits?: [string, number, number, string][];
  perf?: PerfStep[];
};

/** One ledger ownership class with its first 8 rows [file, struct, field, Zig type, Rust shape]. */
export type LedgerClass = {
  cls: string;
  n: number;
  sample: [string, string, string, string, string][];
};

/** One module group: [code, comments, tests] on each side. */
export type ModuleRow = {
  module: string;
  crates: string[];
  zig: [number, number, number];
  rust: [number, number, number];
};

/** unsafe census of one crate (20 or more sites, not -sys). */
export type UnsafeCrate = {
  crate: string;
  tier: string;
  code: number;
  sites: number;
  nonTest: number;
  perKloc: number;
  ntPerKloc: number;
  safety: number;
};

/** The whole widget data file. */
export type TlData = {
  commits: { zig: string; rust: string };
  patterns: Pattern[];
  ledgerClasses: LedgerClass[];
  modules: ModuleRow[];
  unsafeCrates: UnsafeCrate[];
  unsafeTotal: number;
  unsafeHeur: { total: number; cats: [string, number][] };
  helperCallSites: number;
};
