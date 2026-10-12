/** Colour roles, categorical series order and marker shapes for the porting widgets. */

/** PALETTE maps each colour role to its hex value (see the article's design constraints). */
export const PALETTE = {
  rust: "#d97934",
  upstream: "#64d8c6",
  gold: "#ffbc7a",
  pass: "#98c379",
  fail: "#e06c75",
  blue: "#61aeee",
  purple: "#c678dd",
  yellow: "#e6c07b",
  orange: "#d19a66",
  gray0: "#0f1111",
  gray1: "#171a19",
  gray2: "#262927",
  gray3: "#4a4f4b",
  gray4: "#858a84",
  gray5: "#9aa09a",
  gray6: "#c1c8c0",
  gray7: "#d9ded6",
  gray8: "#f4f1ed",
  card: "#0b0c0c",
} as const;

/** SERIES is the ordered categorical colour list: copper, teal, gold, blue, purple, yellow, orange, green, red. */
export const SERIES: readonly string[] = [
  PALETTE.rust,
  PALETTE.upstream,
  PALETTE.gold,
  PALETTE.blue,
  PALETTE.purple,
  PALETTE.yellow,
  PALETTE.orange,
  PALETTE.pass,
  PALETTE.fail,
];

/** A marker shape used to encode a category redundantly with colour. */
export type Shape = "circle" | "square" | "triangle" | "diamond" | "cross";

/** SHAPES is the ordered marker-shape list paired with SERIES, so colour is never the only encoding. */
export const SHAPES: readonly Shape[] = [
  "circle",
  "square",
  "triangle",
  "diamond",
  "cross",
];
