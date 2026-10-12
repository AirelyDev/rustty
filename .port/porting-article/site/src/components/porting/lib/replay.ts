/** Step-replay state shared by the replay widgets (perf replay, test contract, …). */

/** Current replay frame and whether it is auto-advancing. */
export type ReplayState = { index: number; playing: boolean };

/**
 * Returns the initial replay state: with reduced motion the replay starts on
 * its final frame and never auto-plays; otherwise it starts paused at frame 0.
 */
export function replayStart(
  reducedMotion: boolean,
  lastIndex: number,
): ReplayState {
  return reducedMotion
    ? { index: lastIndex, playing: false }
    : { index: 0, playing: false };
}

/** Advances one frame, stopping (playing: false) when the last frame is reached. */
export function replayStep(state: ReplayState, lastIndex: number): ReplayState {
  const index = Math.min(state.index + 1, lastIndex);
  return { index, playing: state.playing && index < lastIndex };
}
