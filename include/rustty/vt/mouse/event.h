/**
 * @file event.h
 *
 * Mouse event representation and manipulation.
 */

#ifndef RUSTTY_VT_MOUSE_EVENT_H
#define RUSTTY_VT_MOUSE_EVENT_H

#include <stdbool.h>
#include <rustty/vt/allocator.h>
#include <rustty/vt/key/event.h>
#include <rustty/vt/types.h>

/**
 * Opaque handle to a mouse event.
 *
 * This handle represents a normalized mouse input event containing
 * action, button, modifiers, and surface-space position.
 *
 * @ingroup mouse
 */
typedef struct RusttyMouseEventImpl *RusttyMouseEvent;

/**
 * Mouse event action type.
 *
 * @ingroup mouse
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Mouse button was pressed. */
  RUSTTY_MOUSE_ACTION_PRESS = 0,

  /** Mouse button was released. */
  RUSTTY_MOUSE_ACTION_RELEASE = 1,

  /** Mouse moved. */
  RUSTTY_MOUSE_ACTION_MOTION = 2,
  RUSTTY_MOUSE_ACTION_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyMouseAction;

/**
 * Mouse button identity.
 *
 * @ingroup mouse
 */
typedef enum RUSTTY_ENUM_TYPED {
  RUSTTY_MOUSE_BUTTON_UNKNOWN = 0,
  RUSTTY_MOUSE_BUTTON_LEFT = 1,
  RUSTTY_MOUSE_BUTTON_RIGHT = 2,
  RUSTTY_MOUSE_BUTTON_MIDDLE = 3,
  RUSTTY_MOUSE_BUTTON_FOUR = 4,
  RUSTTY_MOUSE_BUTTON_FIVE = 5,
  RUSTTY_MOUSE_BUTTON_SIX = 6,
  RUSTTY_MOUSE_BUTTON_SEVEN = 7,
  RUSTTY_MOUSE_BUTTON_EIGHT = 8,
  RUSTTY_MOUSE_BUTTON_NINE = 9,
  RUSTTY_MOUSE_BUTTON_TEN = 10,
  RUSTTY_MOUSE_BUTTON_ELEVEN = 11,
  RUSTTY_MOUSE_BUTTON_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyMouseButton;

/**
 * Mouse position in surface-space pixels.
 *
 * @ingroup mouse
 */
typedef struct {
  float x;
  float y;
} RusttyMousePosition;

/**
 * Create a new mouse event instance.
 *
 * @param allocator Pointer to allocator, or NULL to use the default allocator
 * @param event Pointer to store the created event handle
 * @return RUSTTY_SUCCESS on success, or an error code on failure
 *
 * @ingroup mouse
 */
RUSTTY_API RusttyResult rustty_mouse_event_new(const RusttyAllocator *allocator,
                                      RusttyMouseEvent *event);

/**
 * Free a mouse event instance.
 *
 * @param event The mouse event handle to free (may be NULL)
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_free(RusttyMouseEvent event);

/**
 * Set the event action.
 *
 * @param event The event handle, must not be NULL
 * @param action The action to set
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_set_action(RusttyMouseEvent event,
                                    RusttyMouseAction action);

/**
 * Get the event action.
 *
 * @param event The event handle, must not be NULL
 * @return The event action
 *
 * @ingroup mouse
 */
RUSTTY_API RusttyMouseAction rustty_mouse_event_get_action(RusttyMouseEvent event);

/**
 * Set the event button.
 *
 * This sets a concrete button identity for the event.
 * To represent "no button" (for motion events), use
 * rustty_mouse_event_clear_button().
 *
 * @param event The event handle, must not be NULL
 * @param button The button to set
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_set_button(RusttyMouseEvent event,
                                    RusttyMouseButton button);

/**
 * Clear the event button.
 *
 * This sets the event button to "none".
 *
 * @param event The event handle, must not be NULL
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_clear_button(RusttyMouseEvent event);

/**
 * Get the event button.
 *
 * @param event The event handle, must not be NULL
 * @param out_button Output pointer for the button value (may be NULL)
 * @return true if a button is set, false if no button is set
 *
 * @ingroup mouse
 */
RUSTTY_API bool rustty_mouse_event_get_button(RusttyMouseEvent event,
                                    RusttyMouseButton *out_button);

/**
 * Set keyboard modifiers held during the event.
 *
 * @param event The event handle, must not be NULL
 * @param mods Modifier bitmask
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_set_mods(RusttyMouseEvent event,
                                  RusttyMods mods);

/**
 * Get keyboard modifiers held during the event.
 *
 * @param event The event handle, must not be NULL
 * @return Modifier bitmask
 *
 * @ingroup mouse
 */
RUSTTY_API RusttyMods rustty_mouse_event_get_mods(RusttyMouseEvent event);

/**
 * Set the event position in surface-space pixels.
 *
 * @param event The event handle, must not be NULL
 * @param position The position to set
 *
 * @ingroup mouse
 */
RUSTTY_API void rustty_mouse_event_set_position(RusttyMouseEvent event,
                                      RusttyMousePosition position);

/**
 * Get the event position in surface-space pixels.
 *
 * @param event The event handle, must not be NULL
 * @return The current event position
 *
 * @ingroup mouse
 */
RUSTTY_API RusttyMousePosition rustty_mouse_event_get_position(RusttyMouseEvent event);

#endif /* RUSTTY_VT_MOUSE_EVENT_H */
