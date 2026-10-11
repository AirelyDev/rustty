/** Data types of the "What it cost" widget, as generated into cost.json (type-only, so Node can load the model without a runtime import). */

/** Which usage is shown: the port only, or every project on the same subscription. */
export type Scope = "rustty" | "all";
/** The unit of the weekly instrument: API list-price dollars or billable tokens. */
export type Unit = "usd" | "tok";

/** One scope and unit of a quota week: recovered value by category, then modeled lost-window [low, central, high]. */
export type WeekSeries = [Record<string, number>, [number, number, number]];

/** One quota week (Tuesday 22:00 UTC start), as generated into cost.json. */
export type QuotaWeek = {
  /** Start day (YYYY-MM-DD; the week starts at 22:00 UTC that day). */
  s: string;
  /** Share of the week inside the lost Mac Claude transcript window (0–1). */
  lf: number;
  /** Non-merge commits per Co-Authored-By model ("none" = no trailer). */
  mc: Record<string, number>;
  /** Rustty prompts, other-project prompts and non-merge commits. */
  p: number;
  po: number;
  c: number;
  /** Delegated share of recovered $ (Rustty, all projects), null when too little was recovered. */
  dl: [number | null, number | null];
  rustty: Record<Unit, WeekSeries>;
  all: Record<Unit, WeekSeries>;
};

/** A UTC day: [day, coverage class, Rustty prompts, commits, recovered $, modeled $ (central), other-project prompts]. */
export type CovDay = [string, CovClass, number, number, number, number, number];
/** What survives for a day: Mac transcripts, remnant Mac calls, only VM/Codex, prompts and commits only, nothing. */
export type CovClass = "rec" | "remnant" | "vmcodex" | "prompt" | "none";

/** A limit signal: an author prompt (short verbatim fragment), a slash command or a Claude Code limit message. */
export type LimitEvent = {
  /** UTC minute, "YYYY-MM-DDTHH:MM". */
  t: string;
  k: "prompt" | "slash" | "session" | "weekly" | "usage_credits_exhausted";
  /** 1 when in the port's project, 0 for another project. */
  r: number;
  /** Short label: the verbatim fragment, the command or the message kind. */
  s: string;
  /** Number of messages (agent threads) that received a Claude Code limit message. */
  n?: number;
  /** The Claude Code message text. */
  f?: string;
};

/** Token totals and list-price dollars of one token kind. */
export type MixPart = { tok: number; usd: number };
/** Calls, dollars, tokens, output tokens and median context of one tool/thread kind. */
export type KindRow = {
  calls: number;
  usd: number;
  tok: number;
  out: number;
  ctx_med: number | null;
};
/** An era of the port with its recovered and modeled cost and its outputs. */
export type Era = {
  name: string;
  start: string;
  end: string;
  recovered: number;
  ext: { low: number; central: number; high: number };
  commits: number;
  prompts: number;
  code_lines: number;
};

/** The generated data file. */
export type CostData = {
  qw: QuotaWeek[];
  cov: CovDay[];
  ev: LimitEvent[];
  mix: Record<Scope, Record<string, MixPart>>;
  kinds: Record<Scope, Record<string, KindRow>>;
  /** Context tokens per call by Monday week and tool. */
  ctx: {
    w: string;
    tool: string;
    n: number;
    med: number;
    p90: number;
    mx: number;
  }[];
  /** Per day and source: threads, busy wall-clock hours, agent-thread hours. */
  act: { d: string; s: string; th: number; b: number; a: number }[];
  /** Codex weekly meter per UTC hour: [hour, median %, min %, max %, cumulative Rustty $]. */
  meter: [string, number, number, number, number][];
  /** The labelled Codex audit events. */
  cev: { t: string; label: string; text: string }[];
  /** The note on the unexplained 08-01 00:00–03:59 readings, shared with the audit-funnel chart. */
  meterNote: {
    from: string;
    to: string;
    drop: string;
    lines: string[];
    text: string;
  };
  eras: Era[];
  /** Fully recovered quota weeks: index, $, commits, prompts, code lines. */
  units: {
    q: number;
    usd: number;
    commits: number;
    prompts: number;
    code_lines: number;
  }[];
  /** Recovered Rustty $ by "tool:model". */
  model_usd: Record<string, number>;
  /** Measured maxed-week ceilings (all projects), low / central / high: $, billable tokens and output tokens. */
  ceil: Record<Unit | "out", [number, number, number]>;
  paid: {
    claude: number;
    codex: number;
    central: number;
    cycles: number;
    high: number;
    months: number;
    days: number;
    price: number;
    codexShare: number;
    codexMonth: { rustty: number; other: number };
  };
  api: {
    recovered: number;
    modeled: [number, number, number];
    total: [number, number, number];
  };
  share: number;
  sensitivity: number;
  codexAudit: {
    calls: number;
    threads: number;
    tokens: number;
    cacheRead: number;
    hours: number;
    otherUsd: number;
  };
};
