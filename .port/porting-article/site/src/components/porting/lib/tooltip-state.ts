/**
 * Tooltip visibility state machine: hover and keyboard focus show a tooltip
 * transiently, a tap pins it (so touch users can read it), a second tap on the
 * same target unpins it, and Escape clears everything. The visible tooltip id
 * is `pinned ?? hover`.
 */

/** Which tooltip is pinned (tap) and which is transiently shown (hover/focus). */
export type TooltipState = { pinned: string | null; hover: string | null };

/** An input event for the tooltip state machine. */
export type TooltipAction = {
  type: "enter" | "leave" | "tap" | "focus" | "blur" | "escape";
  id?: string;
};

/** TOOLTIP_IDLE is the state with no tooltip shown. */
export const TOOLTIP_IDLE: TooltipState = { pinned: null, hover: null };

/** Returns the next tooltip state for action. */
export function tooltipReducer(
  state: TooltipState,
  action: TooltipAction,
): TooltipState {
  const id = action.id ?? null;
  switch (action.type) {
    case "enter":
    case "focus":
      return { ...state, hover: id };
    case "leave":
    case "blur":
      return { ...state, hover: null };
    case "tap":
      return state.pinned === id
        ? { pinned: null, hover: null }
        : { ...state, pinned: id };
    case "escape":
      return TOOLTIP_IDLE;
  }
}

/** Returns the id of the tooltip that should be visible, or null. */
export function visibleTooltip(state: TooltipState): string | null {
  return state.pinned ?? state.hover;
}
