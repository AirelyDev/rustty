/** Number and date formatting shared by the porting widgets (en-US, tabular-friendly). */

/** MINUS is U+2212, the typographic minus sign used for negative values. */
const MINUS = "−";

/** INT is the en-US integer formatter with thousands separators. */
const INT = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

/** Returns "−" for negative values and "" otherwise (zero is unsigned). */
function minus(n: number): string {
  return n < 0 && Math.abs(n) > 0 ? MINUS : "";
}

/** Formats n rounded to an integer with thousands separators: 2968 → "2,968". */
export function fmtInt(n: number): string {
  const r = Math.round(n);
  return `${minus(r)}${INT.format(Math.abs(r))}`;
}

/**
 * Formats a value already in percent units: −21.7 → "−21.7%", 4.2 → "+4.2%".
 * Negatives use U+2212; positives get "+" when signed; zero is unsigned.
 */
export function fmtPct(x: number, digits = 1, signed = true): string {
  const abs = Math.abs(x).toFixed(digits);
  const zero = Number(abs) === 0;
  const sign = zero ? "" : x < 0 ? MINUS : signed ? "+" : "";
  return `${sign}${abs}%`;
}

/** Formats milliseconds: "2,269 ms", "42.5 ms", "3.42 ms" (more decimals for small values). */
export function fmtMs(ms: number): string {
  const abs = Math.abs(ms);
  const body =
    abs >= 100
      ? INT.format(Math.round(abs))
      : abs >= 10
        ? abs.toFixed(1)
        : abs.toFixed(2);
  return `${minus(ms)}${body} ms`;
}

/** Formats US dollars: "$11,032", or compact "$23.8k" / "$1.2M" when compact is true. */
export function fmtUsd(n: number, compact = false): string {
  const abs = Math.abs(n);
  let body: string;
  if (compact && abs >= 1e6) body = `${(abs / 1e6).toFixed(1)}M`;
  else if (compact && abs >= 1e3) body = `${(abs / 1e3).toFixed(1)}k`;
  else body = INT.format(Math.round(abs));
  return `${minus(n)}$${body}`;
}

/** Formats a ratio with two decimals and a multiplication sign: 2.849 → "2.85×". */
export function fmtRatio(x: number): string {
  return `${x.toFixed(2)}×`;
}

/** DATE_PT formats instants as short month + day in Pacific time. */
const DATE_PT = new Intl.DateTimeFormat("en-US", {
  timeZone: "America/Los_Angeles",
  month: "short",
  day: "numeric",
});

/** DATE_CAL formats bare calendar dates (parsed as UTC midnight) without shifting the day. */
const DATE_CAL = new Intl.DateTimeFormat("en-US", {
  timeZone: "UTC",
  month: "short",
  day: "numeric",
});

/**
 * Formats an ISO date or timestamp as "Jul 16". Timestamps are shown in
 * Pacific time (the port's working timezone); a bare "YYYY-MM-DD" date is
 * taken as that calendar day.
 */
export function fmtDatePT(iso: string): string {
  const d = new Date(iso);
  return /^\d{4}-\d{2}-\d{2}$/.test(iso)
    ? DATE_CAL.format(d)
    : DATE_PT.format(d);
}
