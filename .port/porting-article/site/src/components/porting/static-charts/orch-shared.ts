/** Layout constants and small helpers shared by the orchestration chart files. */
import { PALETTE } from "../lib/palette";

/** MODEL_COLOURS colour the model layers in data order (shared with the hour-by-hour widget's model colours). */
export const MODEL_COLOURS = [
  PALETTE.gold,
  PALETTE.yellow,
  PALETTE.purple,
  PALETTE.blue,
  PALETTE.pass,
  PALETTE.orange,
  PALETTE.gray6,
  PALETTE.gray3,
];

/**
 * MODEL_TEXTURE marks the layers that carry a stripe. It alternates, so every
 * pair of neighbouring layers differs by texture as well as colour: gold and
 * yellow are close for everyone, purple and blue under deuteranopia.
 */
export const MODEL_TEXTURE = [
  false,
  true,
  false,
  true,
  false,
  true,
  false,
  true,
];

/** H is the viewBox height of the chart (both widths). */
export const H = 480;

/** Rows of the chart: stream, context and daily panels (top and bottom y). */
export const STREAM = { top: 34, bottom: 184 };
export const CTX = { top: 214, bottom: 314 };
export const DAILY = { top: 344, axis: 394, bottom: 444 };

/** CTX_MAX is the top of the context axis in tokens. */
export const CTX_MAX = 550_000;

/** Returns the Monday (ISO date) of the week holding iso. */
export function mondayOf(iso: string): string {
  const d = new Date(`${iso}T00:00Z`);
  d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  return d.toISOString().slice(0, 10);
}

/** Formats a token count as "77k". */
export function kTok(n: number): string {
  return `${Math.round(n / 1000)}k`;
}
