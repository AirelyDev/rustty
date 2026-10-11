import { SERIES, SHAPES, type Shape } from "../lib/palette";
import s from "./StaticCharts.module.css";

/** Returns lane i's colour and marker shape (colour is never the only cue). */
export function laneStyle(i: number): { colour: string; shape: Shape } {
  return {
    colour: SERIES[i % SERIES.length],
    shape: SHAPES[i % SHAPES.length],
  };
}

/** An SVG path for a marker of the given shape, centred on (x, y) with radius r. */
export function shapePath(
  shape: Shape,
  x: number,
  y: number,
  r: number,
): string {
  switch (shape) {
    case "square":
      return `M${x - r},${y - r}h${2 * r}v${2 * r}h${-2 * r}Z`;
    case "triangle":
      return `M${x},${y - r * 1.15}L${x + r * 1.1},${y + r * 0.85}L${x - r * 1.1},${y + r * 0.85}Z`;
    case "diamond":
      return `M${x},${y - r * 1.25}L${x + r * 1.25},${y}L${x},${y + r * 1.25}L${x - r * 1.25},${y}Z`;
    case "cross":
      return `M${x - r},${y - r}L${x + r},${y + r}M${x + r},${y - r}L${x - r},${y + r}`;
    default:
      return `M${x - r},${y}a${r},${r} 0 1,0 ${2 * r},0a${r},${r} 0 1,0 ${-2 * r},0`;
  }
}

/** A small inline SVG marker for lane i (used in legends and the phone list). */
export function LaneGlyph({ i }: { i: number }) {
  const { colour, shape } = laneStyle(i);
  return (
    <svg viewBox="0 0 12 12" className={s.glyph} aria-hidden="true">
      <path
        d={shapePath(shape, 6, 6, 4)}
        fill={shape === "cross" ? "none" : colour}
        stroke={colour}
        strokeWidth={shape === "cross" ? 2 : 0}
      />
    </svg>
  );
}
