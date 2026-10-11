/**
 * Minimal scale and tick helpers for the porting widgets' inline SVG charts.
 * Pure functions with no dependencies, so they run under Node's type stripping
 * in scripts/verify-porting-helpers.mjs as well as in the browser.
 */

/** A numeric [start, end] pair: a scale's domain or its pixel range. */
export type Extent = [number, number];

/** A continuous scale mapping domain values to range values, with its inverse. */
export type ContinuousScale = {
  (v: number): number;
  invert: (px: number) => number;
};

/** A time scale mapping dates to range values; invert returns a Date. */
export type TimeScale = {
  (v: Date | number): number;
  invert: (px: number) => Date;
};

/** A band scale: the start of each key's band, plus the band width. */
export type BandScale = {
  (k: string): number;
  bandwidth: number;
  step: number;
};

/** Returns a linear scale mapping domain onto range (unclamped), with invert. */
export function linear(domain: Extent, range: Extent): ContinuousScale {
  const [d0, d1] = domain;
  const [r0, r1] = range;
  const k = d1 === d0 ? 0 : (r1 - r0) / (d1 - d0);
  const scale = ((v: number) => r0 + (v - d0) * k) as ContinuousScale;
  scale.invert = (px: number) => (k === 0 ? d0 : d0 + (px - r0) / k);
  return scale;
}

/** Returns a logarithmic scale (domain must be strictly positive), with invert. */
export function log(domain: Extent, range: Extent): ContinuousScale {
  const inner = linear([Math.log(domain[0]), Math.log(domain[1])], range);
  const scale = ((v: number) => inner(Math.log(v))) as ContinuousScale;
  scale.invert = (px: number) => Math.exp(inner.invert(px));
  return scale;
}

/** Returns a linear time scale over [start, end] dates; accepts Dates or epoch ms. */
export function time(domain: [Date, Date], range: Extent): TimeScale {
  const inner = linear([domain[0].getTime(), domain[1].getTime()], range);
  const scale = ((v: Date | number) =>
    inner(typeof v === "number" ? v : v.getTime())) as TimeScale;
  scale.invert = (px: number) => new Date(inner.invert(px));
  return scale;
}

/**
 * Returns a band scale placing keys in equal bands across range. padding is
 * the fraction of each step left empty, applied between bands and at both
 * ends (like d3's scaleBand().padding()). Unknown keys map to NaN.
 */
export function band(keys: string[], range: Extent, padding = 0.1): BandScale {
  const [r0, r1] = range;
  const n = keys.length;
  const step = n === 0 ? 0 : (r1 - r0) / (n + padding);
  const index = new Map(keys.map((k, i) => [k, i]));
  const scale = ((k: string) => {
    const i = index.get(k);
    return i === undefined ? Number.NaN : r0 + step * (padding + i);
  }) as BandScale;
  scale.bandwidth = step * (1 - padding);
  scale.step = step;
  return scale;
}

/** Rounds away floating-point noise from a tick value (e.g. 0.30000000000000004). */
function clean(v: number): number {
  return Number.parseFloat(v.toPrecision(12));
}

/**
 * Returns about count evenly spaced "nice" ticks (1, 2 or 5 × a power of ten
 * apart) inside domain, ascending.
 */
export function ticksLinear(domain: Extent, count: number): number[] {
  const lo = Math.min(domain[0], domain[1]);
  const hi = Math.max(domain[0], domain[1]);
  if (!(hi > lo) || count < 1) return Number.isFinite(lo) ? [lo] : [];
  const raw = (hi - lo) / count;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const err = raw / pow;
  const step = pow * (err >= 7.5 ? 10 : err >= 3.5 ? 5 : err >= 1.5 ? 2 : 1);
  const ticks: number[] = [];
  for (let i = Math.ceil(lo / step); i * step <= hi + step * 1e-9; i++) {
    ticks.push(clean(i * step));
  }
  return ticks;
}

/** Returns the 1-2-5 multiples of each power of ten that lie inside domain, ascending. */
export function ticksLog(domain: Extent): number[] {
  const lo = Math.min(domain[0], domain[1]);
  const hi = Math.max(domain[0], domain[1]);
  if (!(lo > 0)) return [];
  const ticks: number[] = [];
  for (
    let e = Math.floor(Math.log10(lo));
    e <= Math.ceil(Math.log10(hi));
    e++
  ) {
    for (const m of [1, 2, 5]) {
      const v = clean(e < 0 ? m / 10 ** -e : m * 10 ** e);
      if (v >= lo && v <= hi) ticks.push(v);
    }
  }
  return ticks;
}
