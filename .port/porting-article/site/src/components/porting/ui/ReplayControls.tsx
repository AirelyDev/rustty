"use client";

import s from "./ui.module.css";

/** Props for ReplayControls; the parent owns the replay state (see lib/replay). */
type ReplayControlsProps = {
  /** Current frame. */
  index: number;
  /** Last frame index (frames are 0..lastIndex). */
  lastIndex: number;
  /** Whether the replay is auto-advancing. */
  playing: boolean;
  /** Starts auto-advancing from the current frame. */
  onPlay: () => void;
  /** Stops auto-advancing. */
  onPause: () => void;
  /** Jumps to a frame (scrubber, restart). */
  onSeek: (index: number) => void;
  /** Text describing the current frame, announced politely to screen readers. */
  label: string;
};

/** Renders play/pause and restart buttons, a frame scrubber and a live frame label. */
export default function ReplayControls({
  index,
  lastIndex,
  playing,
  onPlay,
  onPause,
  onSeek,
  label,
}: ReplayControlsProps) {
  const atEnd = index >= lastIndex;

  /** Plays, restarting from the first frame when the replay is at its end. */
  const play = () => {
    if (atEnd) onSeek(0);
    onPlay();
  };

  return (
    <div className={s.replay}>
      <button
        type="button"
        className={s.replayButton}
        onClick={playing ? onPause : play}
        aria-label={playing ? "Pause" : "Play"}
      >
        {playing ? "❚❚" : "▶"}
      </button>
      <button
        type="button"
        className={s.replayButton}
        onClick={() => onSeek(0)}
        aria-label="Restart from the first step"
      >
        ↻
      </button>
      <input
        type="range"
        className={s.scrubber}
        min={0}
        max={lastIndex}
        step={1}
        value={index}
        onChange={(e) => onSeek(Number(e.target.value))}
        aria-label="Replay step"
        aria-valuetext={label}
      />
      <span className={s.replayLabel} aria-live="polite">
        {label}
      </span>
    </div>
  );
}
