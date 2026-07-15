/**
 * @file event.h
 *
 * Key event representation and manipulation.
 */

#ifndef RUSTTY_VT_KEY_EVENT_H
#define RUSTTY_VT_KEY_EVENT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <rustty/vt/types.h>
#include <rustty/vt/allocator.h>

/**
 * Opaque handle to a key event.
 * 
 * This handle represents a keyboard input event containing information about
 * the physical key pressed, modifiers, and generated text.
 *
 * @ingroup key
 */
typedef struct RusttyKeyEventImpl *RusttyKeyEvent;

/**
 * Keyboard input event types.
 *
 * @ingroup key
 */
typedef enum RUSTTY_ENUM_TYPED {
    /** Key was released */
    RUSTTY_KEY_ACTION_RELEASE = 0,
    /** Key was pressed */
    RUSTTY_KEY_ACTION_PRESS = 1,
    /** Key is being repeated (held down) */
    RUSTTY_KEY_ACTION_REPEAT = 2,
    RUSTTY_KEY_ACTION_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyKeyAction;

/**
 * Keyboard modifier keys bitmask.
 *
 * A bitmask representing all keyboard modifiers. This tracks which modifier keys 
 * are pressed and, where supported by the platform, which side (left or right) 
 * of each modifier is active.
 *
 * Use the RUSTTY_MODS_* constants to test and set individual modifiers.
 *
 * Modifier side bits are only meaningful when the corresponding modifier bit is set.
 * Not all platforms support distinguishing between left and right modifier 
 * keys and Rustty is built to expect that some platforms may not provide this
 * information.
 *
 * @ingroup key
 */
typedef uint16_t RusttyMods;

/** Shift key is pressed */
#define RUSTTY_MODS_SHIFT (1 << 0)
/** Control key is pressed */
#define RUSTTY_MODS_CTRL (1 << 1)
/** Alt/Option key is pressed */
#define RUSTTY_MODS_ALT (1 << 2)
/** Super/Command/Windows key is pressed */
#define RUSTTY_MODS_SUPER (1 << 3)
/** Caps Lock is active */
#define RUSTTY_MODS_CAPS_LOCK (1 << 4)
/** Num Lock is active */
#define RUSTTY_MODS_NUM_LOCK (1 << 5)

/**
 * Right shift is pressed (0 = left, 1 = right).
 * Only meaningful when RUSTTY_MODS_SHIFT is set.
 */
#define RUSTTY_MODS_SHIFT_SIDE (1 << 6)
/**
 * Right ctrl is pressed (0 = left, 1 = right).
 * Only meaningful when RUSTTY_MODS_CTRL is set.
 */
#define RUSTTY_MODS_CTRL_SIDE (1 << 7)
/**
 * Right alt is pressed (0 = left, 1 = right).
 * Only meaningful when RUSTTY_MODS_ALT is set.
 */
#define RUSTTY_MODS_ALT_SIDE (1 << 8)
/**
 * Right super is pressed (0 = left, 1 = right).
 * Only meaningful when RUSTTY_MODS_SUPER is set.
 */
#define RUSTTY_MODS_SUPER_SIDE (1 << 9)

/**
 * Physical key codes.
 *
 * The set of key codes that Rustty is aware of. These represent physical keys 
 * on the keyboard and are layout-independent. For example, the "a" key on a US 
 * keyboard is the same as the "ф" key on a Russian keyboard, but both will 
 * report the same key_a value.
 *
 * Layout-dependent strings are provided separately as UTF-8 text and are produced 
 * by the platform. These values are based on the W3C UI Events KeyboardEvent code 
 * standard. See: https://www.w3.org/TR/uievents-code
 *
 * @ingroup key
 */
typedef enum RUSTTY_ENUM_TYPED {
    RUSTTY_KEY_UNIDENTIFIED = 0,

    // Writing System Keys (W3C § 3.1.1)
    RUSTTY_KEY_BACKQUOTE,
    RUSTTY_KEY_BACKSLASH,
    RUSTTY_KEY_BRACKET_LEFT,
    RUSTTY_KEY_BRACKET_RIGHT,
    RUSTTY_KEY_COMMA,
    RUSTTY_KEY_DIGIT_0,
    RUSTTY_KEY_DIGIT_1,
    RUSTTY_KEY_DIGIT_2,
    RUSTTY_KEY_DIGIT_3,
    RUSTTY_KEY_DIGIT_4,
    RUSTTY_KEY_DIGIT_5,
    RUSTTY_KEY_DIGIT_6,
    RUSTTY_KEY_DIGIT_7,
    RUSTTY_KEY_DIGIT_8,
    RUSTTY_KEY_DIGIT_9,
    RUSTTY_KEY_EQUAL,
    RUSTTY_KEY_INTL_BACKSLASH,
    RUSTTY_KEY_INTL_RO,
    RUSTTY_KEY_INTL_YEN,
    RUSTTY_KEY_A,
    RUSTTY_KEY_B,
    RUSTTY_KEY_C,
    RUSTTY_KEY_D,
    RUSTTY_KEY_E,
    RUSTTY_KEY_F,
    RUSTTY_KEY_G,
    RUSTTY_KEY_H,
    RUSTTY_KEY_I,
    RUSTTY_KEY_J,
    RUSTTY_KEY_K,
    RUSTTY_KEY_L,
    RUSTTY_KEY_M,
    RUSTTY_KEY_N,
    RUSTTY_KEY_O,
    RUSTTY_KEY_P,
    RUSTTY_KEY_Q,
    RUSTTY_KEY_R,
    RUSTTY_KEY_S,
    RUSTTY_KEY_T,
    RUSTTY_KEY_U,
    RUSTTY_KEY_V,
    RUSTTY_KEY_W,
    RUSTTY_KEY_X,
    RUSTTY_KEY_Y,
    RUSTTY_KEY_Z,
    RUSTTY_KEY_MINUS,
    RUSTTY_KEY_PERIOD,
    RUSTTY_KEY_QUOTE,
    RUSTTY_KEY_SEMICOLON,
    RUSTTY_KEY_SLASH,

    // Functional Keys (W3C § 3.1.2)
    RUSTTY_KEY_ALT_LEFT,
    RUSTTY_KEY_ALT_RIGHT,
    RUSTTY_KEY_BACKSPACE,
    RUSTTY_KEY_CAPS_LOCK,
    RUSTTY_KEY_CONTEXT_MENU,
    RUSTTY_KEY_CONTROL_LEFT,
    RUSTTY_KEY_CONTROL_RIGHT,
    RUSTTY_KEY_ENTER,
    RUSTTY_KEY_META_LEFT,
    RUSTTY_KEY_META_RIGHT,
    RUSTTY_KEY_SHIFT_LEFT,
    RUSTTY_KEY_SHIFT_RIGHT,
    RUSTTY_KEY_SPACE,
    RUSTTY_KEY_TAB,
    RUSTTY_KEY_CONVERT,
    RUSTTY_KEY_KANA_MODE,
    RUSTTY_KEY_NON_CONVERT,

    // Control Pad Section (W3C § 3.2)
    RUSTTY_KEY_DELETE,
    RUSTTY_KEY_END,
    RUSTTY_KEY_HELP,
    RUSTTY_KEY_HOME,
    RUSTTY_KEY_INSERT,
    RUSTTY_KEY_PAGE_DOWN,
    RUSTTY_KEY_PAGE_UP,

    // Arrow Pad Section (W3C § 3.3)
    RUSTTY_KEY_ARROW_DOWN,
    RUSTTY_KEY_ARROW_LEFT,
    RUSTTY_KEY_ARROW_RIGHT,
    RUSTTY_KEY_ARROW_UP,

    // Numpad Section (W3C § 3.4)
    RUSTTY_KEY_NUM_LOCK,
    RUSTTY_KEY_NUMPAD_0,
    RUSTTY_KEY_NUMPAD_1,
    RUSTTY_KEY_NUMPAD_2,
    RUSTTY_KEY_NUMPAD_3,
    RUSTTY_KEY_NUMPAD_4,
    RUSTTY_KEY_NUMPAD_5,
    RUSTTY_KEY_NUMPAD_6,
    RUSTTY_KEY_NUMPAD_7,
    RUSTTY_KEY_NUMPAD_8,
    RUSTTY_KEY_NUMPAD_9,
    RUSTTY_KEY_NUMPAD_ADD,
    RUSTTY_KEY_NUMPAD_BACKSPACE,
    RUSTTY_KEY_NUMPAD_CLEAR,
    RUSTTY_KEY_NUMPAD_CLEAR_ENTRY,
    RUSTTY_KEY_NUMPAD_COMMA,
    RUSTTY_KEY_NUMPAD_DECIMAL,
    RUSTTY_KEY_NUMPAD_DIVIDE,
    RUSTTY_KEY_NUMPAD_ENTER,
    RUSTTY_KEY_NUMPAD_EQUAL,
    RUSTTY_KEY_NUMPAD_MEMORY_ADD,
    RUSTTY_KEY_NUMPAD_MEMORY_CLEAR,
    RUSTTY_KEY_NUMPAD_MEMORY_RECALL,
    RUSTTY_KEY_NUMPAD_MEMORY_STORE,
    RUSTTY_KEY_NUMPAD_MEMORY_SUBTRACT,
    RUSTTY_KEY_NUMPAD_MULTIPLY,
    RUSTTY_KEY_NUMPAD_PAREN_LEFT,
    RUSTTY_KEY_NUMPAD_PAREN_RIGHT,
    RUSTTY_KEY_NUMPAD_SUBTRACT,
    RUSTTY_KEY_NUMPAD_SEPARATOR,
    RUSTTY_KEY_NUMPAD_UP,
    RUSTTY_KEY_NUMPAD_DOWN,
    RUSTTY_KEY_NUMPAD_RIGHT,
    RUSTTY_KEY_NUMPAD_LEFT,
    RUSTTY_KEY_NUMPAD_BEGIN,
    RUSTTY_KEY_NUMPAD_HOME,
    RUSTTY_KEY_NUMPAD_END,
    RUSTTY_KEY_NUMPAD_INSERT,
    RUSTTY_KEY_NUMPAD_DELETE,
    RUSTTY_KEY_NUMPAD_PAGE_UP,
    RUSTTY_KEY_NUMPAD_PAGE_DOWN,

    // Function Section (W3C § 3.5)
    RUSTTY_KEY_ESCAPE,
    RUSTTY_KEY_F1,
    RUSTTY_KEY_F2,
    RUSTTY_KEY_F3,
    RUSTTY_KEY_F4,
    RUSTTY_KEY_F5,
    RUSTTY_KEY_F6,
    RUSTTY_KEY_F7,
    RUSTTY_KEY_F8,
    RUSTTY_KEY_F9,
    RUSTTY_KEY_F10,
    RUSTTY_KEY_F11,
    RUSTTY_KEY_F12,
    RUSTTY_KEY_F13,
    RUSTTY_KEY_F14,
    RUSTTY_KEY_F15,
    RUSTTY_KEY_F16,
    RUSTTY_KEY_F17,
    RUSTTY_KEY_F18,
    RUSTTY_KEY_F19,
    RUSTTY_KEY_F20,
    RUSTTY_KEY_F21,
    RUSTTY_KEY_F22,
    RUSTTY_KEY_F23,
    RUSTTY_KEY_F24,
    RUSTTY_KEY_F25,
    RUSTTY_KEY_FN,
    RUSTTY_KEY_FN_LOCK,
    RUSTTY_KEY_PRINT_SCREEN,
    RUSTTY_KEY_SCROLL_LOCK,
    RUSTTY_KEY_PAUSE,

    // Media Keys (W3C § 3.6)
    RUSTTY_KEY_BROWSER_BACK,
    RUSTTY_KEY_BROWSER_FAVORITES,
    RUSTTY_KEY_BROWSER_FORWARD,
    RUSTTY_KEY_BROWSER_HOME,
    RUSTTY_KEY_BROWSER_REFRESH,
    RUSTTY_KEY_BROWSER_SEARCH,
    RUSTTY_KEY_BROWSER_STOP,
    RUSTTY_KEY_EJECT,
    RUSTTY_KEY_LAUNCH_APP_1,
    RUSTTY_KEY_LAUNCH_APP_2,
    RUSTTY_KEY_LAUNCH_MAIL,
    RUSTTY_KEY_MEDIA_PLAY_PAUSE,
    RUSTTY_KEY_MEDIA_SELECT,
    RUSTTY_KEY_MEDIA_STOP,
    RUSTTY_KEY_MEDIA_TRACK_NEXT,
    RUSTTY_KEY_MEDIA_TRACK_PREVIOUS,
    RUSTTY_KEY_POWER,
    RUSTTY_KEY_SLEEP,
    RUSTTY_KEY_AUDIO_VOLUME_DOWN,
    RUSTTY_KEY_AUDIO_VOLUME_MUTE,
    RUSTTY_KEY_AUDIO_VOLUME_UP,
    RUSTTY_KEY_WAKE_UP,

    // Legacy, Non-standard, and Special Keys (W3C § 3.7)
    RUSTTY_KEY_COPY,
    RUSTTY_KEY_CUT,
    RUSTTY_KEY_PASTE,
    RUSTTY_KEY_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyKey;

/**
 * Create a new key event instance.
 * 
 * Creates a new key event with default values. The event must be freed using
 * rustty_key_event_free() when no longer needed.
 * 
 * @param allocator Pointer to the allocator to use for memory management, or NULL to use the default allocator
 * @param event Pointer to store the created key event handle
 * @return RUSTTY_SUCCESS on success, or an error code on failure
 * 
 * @ingroup key
 */
RUSTTY_API RusttyResult rustty_key_event_new(const RusttyAllocator *allocator, RusttyKeyEvent *event);

/**
 * Free a key event instance.
 * 
 * Releases all resources associated with the key event. After this call,
 * the event handle becomes invalid and must not be used.
 * 
 * @param event The key event handle to free (may be NULL)
 * 
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_free(RusttyKeyEvent event);

/**
 * Set the key action (press, release, repeat).
 *
 * @param event The key event handle, must not be NULL
 * @param action The action to set
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_action(RusttyKeyEvent event, RusttyKeyAction action);

/**
 * Get the key action (press, release, repeat).
 *
 * @param event The key event handle, must not be NULL
 * @return The key action
 *
 * @ingroup key
 */
RUSTTY_API RusttyKeyAction rustty_key_event_get_action(RusttyKeyEvent event);

/**
 * Set the physical key code.
 *
 * @param event The key event handle, must not be NULL
 * @param key The physical key code to set
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_key(RusttyKeyEvent event, RusttyKey key);

/**
 * Get the physical key code.
 *
 * @param event The key event handle, must not be NULL
 * @return The physical key code
 *
 * @ingroup key
 */
RUSTTY_API RusttyKey rustty_key_event_get_key(RusttyKeyEvent event);

/**
 * Set the modifier keys bitmask.
 *
 * @param event The key event handle, must not be NULL
 * @param mods The modifier keys bitmask to set
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_mods(RusttyKeyEvent event, RusttyMods mods);

/**
 * Get the modifier keys bitmask.
 *
 * @param event The key event handle, must not be NULL
 * @return The modifier keys bitmask
 *
 * @ingroup key
 */
RUSTTY_API RusttyMods rustty_key_event_get_mods(RusttyKeyEvent event);

/**
 * Set the consumed modifiers bitmask.
 *
 * @param event The key event handle, must not be NULL
 * @param consumed_mods The consumed modifiers bitmask to set
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_consumed_mods(RusttyKeyEvent event, RusttyMods consumed_mods);

/**
 * Get the consumed modifiers bitmask.
 *
 * @param event The key event handle, must not be NULL
 * @return The consumed modifiers bitmask
 *
 * @ingroup key
 */
RUSTTY_API RusttyMods rustty_key_event_get_consumed_mods(RusttyKeyEvent event);

/**
 * Set whether the key event is part of a composition sequence.
 *
 * @param event The key event handle, must not be NULL
 * @param composing Whether the key event is part of a composition sequence
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_composing(RusttyKeyEvent event, bool composing);

/**
 * Get whether the key event is part of a composition sequence.
 *
 * @param event The key event handle, must not be NULL
 * @return Whether the key event is part of a composition sequence
 *
 * @ingroup key
 */
RUSTTY_API bool rustty_key_event_get_composing(RusttyKeyEvent event);

/**
 * Set the UTF-8 text generated by the key for the current keyboard layout.
 *
 * Must contain the unmodified character before any Ctrl/Meta transformations.
 * The encoder derives modifier sequences from the logical key and mods
 * bitmask, not from this text. Do not pass C0 control characters
 * (U+0000-U+001F, U+007F) or platform function key codes (e.g. macOS PUA
 * U+F700-U+F8FF); pass NULL instead and let the encoder use the logical key.
 *
 * The key event does NOT take ownership of the text pointer. The caller
 * must ensure the string remains valid for the lifetime needed by the event.
 *
 * @param event The key event handle, must not be NULL
 * @param utf8 The UTF-8 text to set (or NULL for empty)
 * @param len Length of the UTF-8 text in bytes
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_utf8(RusttyKeyEvent event, const char *utf8, size_t len);

/**
 * Get the UTF-8 text generated by the key event.
 *
 * The returned pointer is valid until the event is freed or the UTF-8 text is modified.
 *
 * @param event The key event handle, must not be NULL
 * @param len Pointer to store the length of the UTF-8 text in bytes (may be NULL)
 * @return The UTF-8 text (or NULL for empty)
 *
 * @ingroup key
 */
RUSTTY_API const char *rustty_key_event_get_utf8(RusttyKeyEvent event, size_t *len);

/**
 * Set the unshifted Unicode codepoint.
 *
 * @param event The key event handle, must not be NULL
 * @param codepoint The unshifted Unicode codepoint to set
 *
 * @ingroup key
 */
RUSTTY_API void rustty_key_event_set_unshifted_codepoint(RusttyKeyEvent event, uint32_t codepoint);

/**
 * Get the unshifted Unicode codepoint.
 *
 * @param event The key event handle, must not be NULL
 * @return The unshifted Unicode codepoint
 *
 * @ingroup key
 */
RUSTTY_API uint32_t rustty_key_event_get_unshifted_codepoint(RusttyKeyEvent event);

#endif /* RUSTTY_VT_KEY_EVENT_H */
