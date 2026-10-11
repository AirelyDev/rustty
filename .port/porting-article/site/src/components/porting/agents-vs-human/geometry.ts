/**
 * Layout of the main SVG (viewBox units), shared by its sub-views and the
 * pointer hit-testing in MainChart. Values follow the mockup.
 */

/** VIEW_W is the main SVG's viewBox width. */
export const VIEW_W = 960;
/** HEAD_H is the height of the column-head band above the first row. */
export const HEAD_H = 40;
/** GRID_H is the total height available to the day rows. */
export const GRID_H = 660;
/** PROFILE_Y is the top of the hour-of-day profile under the grid. */
export const PROFILE_Y = HEAD_H + GRID_H + 16;
/** PROFILE_H is the height of the hour-of-day profile. */
export const PROFILE_H = 70;
/** VIEW_H is the main SVG's viewBox height. */
export const VIEW_H = PROFILE_Y + PROFILE_H + 26;
/** MAX_ROW_H caps a day row's height when zoomed in. */
const MAX_ROW_H = 60;

/** X holds the x positions and widths of every column of the main SVG. */
export const X = {
  gantt: 4,
  ev: 56,
  evl: 64,
  dateR: 186,
  grid: 192,
  cw: 16,
  model: 584,
  mw: 16,
  cost: 606,
  costW: 50,
  thr: 662,
  thrW: 40,
  com: 712,
  comW: 80,
  pro: 798,
  proW: 50,
  lin: 854,
  linW: 104,
} as const;

/** The visible day range and the derived row geometry. */
export type Rows = {
  /** First and last visible day index. */
  d0: number;
  d1: number;
  /** Row height. */
  h: number;
  /** Top y of day i's row. */
  y: (i: number) => number;
  /** Day index at y, or null outside the rows. */
  at: (y: number) => number | null;
  /** Inset of bars inside a row. */
  pad: number;
};

/** Returns the row geometry for visible days [d0, d1]. */
export function rows(d0: number, d1: number): Rows {
  const h = Math.min(GRID_H / (d1 - d0 + 1), MAX_ROW_H);
  return {
    d0,
    d1,
    h,
    y: (i) => HEAD_H + (i - d0) * h,
    at: (y) => {
      const i = d0 + Math.floor((y - HEAD_H) / h);
      return y < HEAD_H || i < d0 || i > d1 ? null : i;
    },
    pad: h > 5 ? 1 : 0.5,
  };
}

/** Returns the height of a bar inside a row (never thinner than 0.6). */
export function barH(r: Rows): number {
  return Math.max(0.6, r.h - 2 * r.pad);
}
