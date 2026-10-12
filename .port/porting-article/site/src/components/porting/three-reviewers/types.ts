/** Shape of src/app/porting/data/three-reviewers.json (scripts/porting/widgets/w01_three_reviewers.py). */

/** One rail event: a curated port memory-safety event and the tool that found it. */
export type RailEvent = {
  /** Date, YYYY-MM-DD. */
  d: string;
  /** Commit hash or record reference (may be empty). */
  ref: string;
  /** Lane key (see Lane). */
  lane: string;
  /** Detecting tool as recorded. */
  tool: string;
  /** Bug class. */
  cls: string;
  /** Whether the defect is in production code (filled dot) or a test, harness or bench (hollow). */
  prod: boolean;
  /** Scope as recorded (production, test, …). */
  scope: string;
  /** Crate and area. */
  area: string;
  /** Defects in the event. */
  n: number;
  /** One-line summary. */
  sum: string;
};

/** One rail lane: key and label. */
export type Lane = { key: string; label: string };

/** One code excerpt: header (file, function or harness) and [line number, text] rows. */
export type Seg = { file: string; lines: [number, string][] };

/** A memory access shown at a step: whether Miri allowed it, and what it was. */
export type Access = { ok: boolean; text: string };

/** Visual state of a borrow-stack item at a step. */
export type ItemState = "new" | "pop" | "bad";

/** One borrow-stack item (Stacked Borrows) or borrow-tree node (Tree Borrows). */
export type StackItem = {
  /** Miri tag number, "wildcard", "self", or "·" when Miri did not print it. */
  id: string;
  /** Permission (SB) or state (TB): Unique, SharedRW, Reserved, Frozen, Disabled, … */
  perm: string;
  /** What created the tag. */
  label: string;
  /** Whether the tag is protected (a function argument for the whole call). */
  prot?: boolean;
  /** Tree depth (Tree Borrows only). */
  depth?: number;
  /** State at this step, when it changes. */
  st?: ItemState;
};

/** One replay step: the code lines executed, narration, the access and the stack after it. */
export type Step = {
  /** Executed lines as [trace file header, line number]. */
  at: [string, number][];
  say: string;
  acc?: Access;
  /** Stack bottom first (SB) or tree in pre-order (TB). */
  items: StackItem[];
};

/** The aliasing model a replay runs under. */
export type Model = "sb" | "tb";

/** A commit: short hash, date and subject (internal ids removed). */
export type Commit = { hash: string; date: string; subject: string };

/** One replayed bug (A–D). */
export type Bug = {
  key: string;
  /** Fix commit (also the rail event's ref). */
  ref: string;
  title: string;
  short: string;
  crate: string;
  scope: string;
  models: string;
  caughtBy: string;
  mechanism: string;
  /** The bug's life in git: first written → fixed. */
  life: { what: string; hash: string; date: string; subject: string }[];
  impl: { commit: Commit | null; note: string; segs: Seg[] };
  review: {
    /** "none" when no reviewer caught it, else "review" or "audit". */
    kind: string;
    head: string;
    quotes: { who: string; src: string; text: string }[];
  };
  miri: {
    cmd: string;
    cmdTb: string;
    loc: string;
    /** How harness lines map to the real code, when the trace is a harness. */
    map: string | null;
    trace: Seg[];
    sb: Step[];
    tb: Step[];
    logSb: string;
    logTb: string;
  };
  fix: {
    commit: Commit;
    stat: string;
    /** Diff hunks (lines with their +/-/space prefix), or null when beforeAfter is shown. */
    diff: string[][] | null;
    beforeAfter: { before: Seg; after: Seg[] } | null;
    /** Verdict rows: [shape, SB verdict, TB verdict]. */
    matrix: [string, string, string][];
    cleanCmd: string;
    cleanCmdTb: string;
    cleanSb: string;
    cleanTb: string | null;
  };
  rule: { text: string; src: string };
  count: {
    what: string;
    series: [string, number][];
    note: string;
    /** Harness-crate birth dates (bug B only). */
    births?: string[];
  };
};

/** The whole data file. */
export type TrData = {
  /** Head commit the "today" counts are taken at. */
  head: string;
  lanes: Lane[];
  timeline: RailEvent[];
  bugs: Bug[];
};
