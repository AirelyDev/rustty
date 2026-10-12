/** Data-file types of the "Cutting the cycle" widget (src/app/porting/data/cutting-the-cycle.json). */

/** A file of the cycle: name, directory index, layout x, y (0-1000), in/out degree inside the cycle, lines, crate it became (short name). */
export type FileRow = [
  string,
  number,
  number,
  number,
  number,
  number,
  number,
  string,
];

/** A file import edge: source index, target index, seam tag (0 none, 1-14 cuts, 15-19 extra seams, 21 bottom-up import of global.zig). */
export type ZigEdge = [number, number, number];

/** A crate-graph node row, field order as in the data file (see crateNodes). */
export type CrateRow = [
  string,
  string,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  [string, number][],
  string | null,
  string | null,
  number | null,
  number | null,
  number | null,
  number | null,
  string | null,
  string | null,
  string | null,
  number | null,
  number | null,
  number | null,
  string | null,
];

/** One cut or extra seam in plain words: number, name, mechanism, carrying crate (short), explanation. */
export type Cut = {
  n: number;
  name: string;
  m: "trait" | "move" | "split" | "inject" | "other";
  cr: string | null;
  d: string;
};

/** A layering decision: the crate edges it explains, its text and the crates that list it. */
export type Layer = { e: [string, string][]; t: string; c: string[] };

/** The widget's generated data file (src/app/porting/data/cutting-the-cycle.json). */
export type CcData = {
  zig: {
    dirs: string[];
    sectors: [number, number][];
    files: FileRow[];
    edges: ZigEdge[];
    stats: {
      files_total: number;
      runtime_edges: number;
      scc: number;
      scc_edges: number;
      other_sccs: number[];
      cycle_len_dist: Record<string, number>;
      now: { commit: string; files: number; scc: number; scc_edges: number };
      cum: number[];
      inverted: number;
      cross_crate: number;
      ablation: Record<string, number>;
    };
    skip_new: Record<string, string[][]>;
    examples: string[][];
    plan: [number, number][];
    entry: number[];
  };
  crates: {
    nodes: CrateRow[];
    edges: [string, string, "normal" | "dev" | "build"][];
    chain: string[];
    rho: number;
    rho_n: number;
    bands: Record<
      string,
      { n: number; median: string; first: string; last: string }
    >;
    cratesIo: { resolved: number; direct: number };
  };
  replay: {
    d0: string;
    n: number;
    dates: string[];
    s: Record<string, [number, number[]]>;
    tests: [string, number][];
  };
  text: {
    cuts: Cut[];
    extra: Cut[];
    layer: Layer[];
    crateCuts: Record<string, number[]>;
  };
};
