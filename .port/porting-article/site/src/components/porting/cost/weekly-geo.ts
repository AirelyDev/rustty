/** Geometry of the weekly quota-week instrument (mockup coordinates, viewBox 960 × 408). */
import { Q0, utc, WEEK_MS } from "./model";

/** WK holds the instrument's frame: width, height, plot left/right and bar top/bottom. */
export const WK = { W: 960, H: 408, X0: 58, X1: 944, B0: 106, B1: 306 };

/** WEEKS is the number of quota weeks drawn. */
export const WEEKS = 17;

/** T_END is the end of the last quota week, UTC ms. */
export const T_END = Q0 + WEEKS * WEEK_MS;

/** LOST_START and LOST_END bound the lost Mac Claude transcript window (end exclusive). */
export const LOST_START = "2026-06-21";
/** LOST_END is the first day with recovered Mac Claude transcripts. */
export const LOST_END = "2026-09-08";

/** Maps a UTC instant (ms) to an x position. */
export function wx(t: number): number {
  return WK.X0 + ((t - Q0) / (T_END - Q0)) * (WK.X1 - WK.X0);
}

/** Maps a UTC timestamp string to an x position. */
export function wxs(s: string): number {
  return wx(utc(s));
}

/** Returns the bar rectangle of quota week i: left x and width (5 px inset each side). */
export function barBox(i: number): { x: number; w: number } {
  const a = Q0 + i * WEEK_MS;
  return { x: wx(a) + 5, w: wx(a + WEEK_MS) - wx(a) - 10 };
}

/** MON are the English month abbreviations. */
const MON = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** Formats a UTC instant as "Jun 16". */
export function dayLabel(t: number): string {
  const d = new Date(t);
  return `${MON[d.getUTCMonth()]} ${d.getUTCDate()}`;
}
