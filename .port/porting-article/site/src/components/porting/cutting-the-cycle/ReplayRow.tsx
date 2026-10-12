"use client";

import { fmtInt } from "../lib/format";
import type { ReplayState } from "../lib/replay";
import ReplayControls from "../ui/ReplayControls";
import s from "./CuttingTheCycle.module.css";
import { dayNumber, type ReplayFrame } from "./model";

/**
 * Stage 4 controls and counters: play/pause, restart and the date scrubber,
 * then packages created, physical lines, crates past 1,000 lines and the
 * weekly #[test] scan on the current date.
 */
export default function ReplayRow({
  replay,
  last,
  frame,
  d0,
  packages,
  onPlay,
  onPause,
  onSeek,
}: {
  replay: ReplayState;
  last: number;
  frame: ReplayFrame;
  d0: string;
  packages: number;
  onPlay: () => void;
  onPause: () => void;
  onSeek: (i: number) => void;
}) {
  return (
    <>
      <div className={s.replayRow}>
        <ReplayControls
          index={replay.index}
          lastIndex={last}
          playing={replay.playing}
          onPlay={onPlay}
          onPause={onPause}
          onSeek={onSeek}
          label={`${frame.date} · day ${dayNumber(d0, frame.date)}`}
        />
      </div>
      <div className={s.counters}>
        <div>
          <b>
            {frame.born}/{packages}
          </b>
          <span>packages created</span>
        </div>
        <div>
          <b>{fmtInt(frame.total)}</b>
          <span>physical .rs lines, vendored code excluded</span>
        </div>
        <div>
          <b>{frame.past1k}</b>
          <span>crates past 1,000 lines</span>
        </div>
        <div>
          <b>{fmtInt(frame.tests)}</b>
          <span>#[test] fns (last weekly scan)</span>
        </div>
      </div>
    </>
  );
}
