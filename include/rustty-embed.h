// Rustty embedding API — token-renamed from the Ghostty embedding-API
// umbrella (oracle: include/ghostty.h).
//
// The documentation for the embedding API is only within the Zig source
// files that define the implementations. This isn't meant to be a general
// purpose embedding API (yet) so there hasn't been documentation or example
// work beyond that.
//
// The only consumer of this API is the macOS app, but the API is built to
// be more general purpose.
#ifndef RUSTTY_H
#define RUSTTY_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef _MSC_VER
#include <BaseTsd.h>
typedef SSIZE_T ssize_t;
#else
#include <sys/types.h>
#endif

//-------------------------------------------------------------------
// Macros

#define RUSTTY_SUCCESS 0

// Symbol visibility for shared library builds. On Windows, functions
// are exported from the DLL when building and imported when consuming.
// On other platforms with GCC/Clang, functions are marked with default
// visibility so they remain accessible when the library is built with
// -fvisibility=hidden. For static library builds, define RUSTTY_STATIC
// before including this header to make this a no-op.
#ifndef RUSTTY_API
#if defined(RUSTTY_STATIC)
  #define RUSTTY_API
#elif defined(_WIN32) || defined(_WIN64)
  #ifdef RUSTTY_BUILD_SHARED
    #define RUSTTY_API __declspec(dllexport)
  #else
    #define RUSTTY_API __declspec(dllimport)
  #endif
#elif defined(__GNUC__) && __GNUC__ >= 4
  #define RUSTTY_API __attribute__((visibility("default")))
#else
  #define RUSTTY_API
#endif
#endif

//-------------------------------------------------------------------
// Types

// Opaque types
typedef void* rustty_app_t;
typedef void* rustty_config_t;
typedef void* rustty_surface_t;
typedef void* rustty_inspector_t;

// All the types below are fully defined and must be kept in sync with
// their Zig counterparts. Any changes to these types MUST have an associated
// Zig change.
typedef enum {
  RUSTTY_PLATFORM_INVALID,
  RUSTTY_PLATFORM_MACOS,
  RUSTTY_PLATFORM_IOS,
} rustty_platform_e;

typedef enum {
  RUSTTY_CLIPBOARD_STANDARD,
  RUSTTY_CLIPBOARD_SELECTION,
} rustty_clipboard_e;

typedef struct {
  const char *mime;
  const char *data;
} rustty_clipboard_content_s;

typedef enum {
  RUSTTY_CLIPBOARD_REQUEST_PASTE,
  RUSTTY_CLIPBOARD_REQUEST_OSC_52_READ,
  RUSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE,
} rustty_clipboard_request_e;

typedef enum {
  RUSTTY_MOUSE_RELEASE,
  RUSTTY_MOUSE_PRESS,
} rustty_input_mouse_state_e;

typedef enum {
  RUSTTY_MOUSE_UNKNOWN,
  RUSTTY_MOUSE_LEFT,
  RUSTTY_MOUSE_RIGHT,
  RUSTTY_MOUSE_MIDDLE,
  RUSTTY_MOUSE_FOUR,
  RUSTTY_MOUSE_FIVE,
  RUSTTY_MOUSE_SIX,
  RUSTTY_MOUSE_SEVEN,
  RUSTTY_MOUSE_EIGHT,
  RUSTTY_MOUSE_NINE,
  RUSTTY_MOUSE_TEN,
  RUSTTY_MOUSE_ELEVEN,
} rustty_input_mouse_button_e;

typedef enum {
  RUSTTY_MOUSE_MOMENTUM_NONE,
  RUSTTY_MOUSE_MOMENTUM_BEGAN,
  RUSTTY_MOUSE_MOMENTUM_STATIONARY,
  RUSTTY_MOUSE_MOMENTUM_CHANGED,
  RUSTTY_MOUSE_MOMENTUM_ENDED,
  RUSTTY_MOUSE_MOMENTUM_CANCELLED,
  RUSTTY_MOUSE_MOMENTUM_MAY_BEGIN,
} rustty_input_mouse_momentum_e;

typedef enum {
  RUSTTY_COLOR_SCHEME_LIGHT = 0,
  RUSTTY_COLOR_SCHEME_DARK = 1,
} rustty_color_scheme_e;

// This is a packed struct (see src/input/mouse.zig) but the C standard
// afaik doesn't let us reliably define packed structs so we build it up
// from scratch.
typedef int rustty_input_scroll_mods_t;

typedef enum {
  RUSTTY_MODS_NONE = 0,
  RUSTTY_MODS_SHIFT = 1 << 0,
  RUSTTY_MODS_CTRL = 1 << 1,
  RUSTTY_MODS_ALT = 1 << 2,
  RUSTTY_MODS_SUPER = 1 << 3,
  RUSTTY_MODS_CAPS = 1 << 4,
  RUSTTY_MODS_NUM = 1 << 5,
  RUSTTY_MODS_SHIFT_RIGHT = 1 << 6,
  RUSTTY_MODS_CTRL_RIGHT = 1 << 7,
  RUSTTY_MODS_ALT_RIGHT = 1 << 8,
  RUSTTY_MODS_SUPER_RIGHT = 1 << 9,
} rustty_input_mods_e;

typedef enum {
  RUSTTY_BINDING_FLAGS_CONSUMED = 1 << 0,
  RUSTTY_BINDING_FLAGS_ALL = 1 << 1,
  RUSTTY_BINDING_FLAGS_GLOBAL = 1 << 2,
  RUSTTY_BINDING_FLAGS_PERFORMABLE = 1 << 3,
} rustty_binding_flags_e;

typedef enum {
  RUSTTY_ACTION_RELEASE,
  RUSTTY_ACTION_PRESS,
  RUSTTY_ACTION_REPEAT,
} rustty_input_action_e;

// Based on: https://www.w3.org/TR/uievents-code/
typedef enum {
  RUSTTY_KEY_UNIDENTIFIED,

  // "Writing System Keys" § 3.1.1
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

  // "Functional Keys" § 3.1.2
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

  // "Control Pad Section" § 3.2
  RUSTTY_KEY_DELETE,
  RUSTTY_KEY_END,
  RUSTTY_KEY_HELP,
  RUSTTY_KEY_HOME,
  RUSTTY_KEY_INSERT,
  RUSTTY_KEY_PAGE_DOWN,
  RUSTTY_KEY_PAGE_UP,

  // "Arrow Pad Section" § 3.3
  RUSTTY_KEY_ARROW_DOWN,
  RUSTTY_KEY_ARROW_LEFT,
  RUSTTY_KEY_ARROW_RIGHT,
  RUSTTY_KEY_ARROW_UP,

  // "Numpad Section" § 3.4
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

  // "Function Section" § 3.5
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

  // "Media Keys" § 3.6
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

  // "Legacy, Non-standard, and Special Keys" § 3.7
  RUSTTY_KEY_COPY,
  RUSTTY_KEY_CUT,
  RUSTTY_KEY_PASTE,
} rustty_input_key_e;

typedef struct {
  rustty_input_action_e action;
  rustty_input_mods_e mods;
  rustty_input_mods_e consumed_mods;
  uint32_t keycode;
  const char* text;
  uint32_t unshifted_codepoint;
  bool composing;
} rustty_input_key_s;

typedef enum {
  RUSTTY_TRIGGER_PHYSICAL,
  RUSTTY_TRIGGER_UNICODE,
  RUSTTY_TRIGGER_CATCH_ALL,
} rustty_input_trigger_tag_e;

typedef union {
  rustty_input_key_e physical;
  uint32_t unicode;
  // catch_all has no payload
} rustty_input_trigger_key_u;

typedef struct {
  rustty_input_trigger_tag_e tag;
  rustty_input_trigger_key_u key;
  rustty_input_mods_e mods;
} rustty_input_trigger_s;

typedef struct {
  const char* action_key;
  const char* action;
  const char* title;
  const char* description;
} rustty_command_s;

typedef enum {
  RUSTTY_BUILD_MODE_DEBUG,
  RUSTTY_BUILD_MODE_RELEASE_SAFE,
  RUSTTY_BUILD_MODE_RELEASE_FAST,
  RUSTTY_BUILD_MODE_RELEASE_SMALL,
} rustty_build_mode_e;

typedef struct {
  rustty_build_mode_e build_mode;
  const char* version;
  uintptr_t version_len;
} rustty_info_s;

typedef struct {
  const char* message;
} rustty_diagnostic_s;

typedef struct {
  const char* ptr;
  uintptr_t len;
  bool sentinel;
} rustty_string_s;

typedef struct {
  double tl_px_x;
  double tl_px_y;
  uint32_t offset_start;
  uint32_t offset_len;
  const char* text;
  uintptr_t text_len;
} rustty_text_s;

typedef enum {
  RUSTTY_POINT_ACTIVE,
  RUSTTY_POINT_VIEWPORT,
  RUSTTY_POINT_SCREEN,
  RUSTTY_POINT_SURFACE,
} rustty_point_tag_e;

typedef enum {
  RUSTTY_POINT_COORD_EXACT,
  RUSTTY_POINT_COORD_TOP_LEFT,
  RUSTTY_POINT_COORD_BOTTOM_RIGHT,
} rustty_point_coord_e;

typedef struct {
  rustty_point_tag_e tag;
  rustty_point_coord_e coord;
  uint32_t x;
  uint32_t y;
} rustty_point_s;

typedef struct {
  rustty_point_s top_left;
  rustty_point_s bottom_right;
  bool rectangle;
} rustty_selection_s;

typedef struct {
  const char* key;
  const char* value;
} rustty_env_var_s;

typedef struct {
  void* nsview;
} rustty_platform_macos_s;

typedef struct {
  void* uiview;
} rustty_platform_ios_s;

typedef union {
  rustty_platform_macos_s macos;
  rustty_platform_ios_s ios;
} rustty_platform_u;

typedef enum {
  RUSTTY_SURFACE_CONTEXT_WINDOW = 0,
  RUSTTY_SURFACE_CONTEXT_TAB = 1,
  RUSTTY_SURFACE_CONTEXT_SPLIT = 2,
} rustty_surface_context_e;

typedef struct {
  rustty_platform_e platform_tag;
  rustty_platform_u platform;
  void* userdata;
  double scale_factor;
  float font_size;
  const char* working_directory;
  const char* command;
  rustty_env_var_s* env_vars;
  size_t env_var_count;
  const char* initial_input;
  bool wait_after_command;
  rustty_surface_context_e context;
} rustty_surface_config_s;

typedef struct {
  uint16_t columns;
  uint16_t rows;
  uint32_t width_px;
  uint32_t height_px;
  uint32_t cell_width_px;
  uint32_t cell_height_px;
} rustty_surface_size_s;

// Config types

// config.Path
typedef struct {
  const char* path;
  bool optional;
} rustty_config_path_s;

// config.Color
typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
} rustty_config_color_s;

// config.ColorList
typedef struct {
  const rustty_config_color_s* colors;
  size_t len;
} rustty_config_color_list_s;

// config.RepeatableCommand
typedef struct {
  const rustty_command_s* commands;
  size_t len;
} rustty_config_command_list_s;

// config.Palette
typedef struct {
  rustty_config_color_s colors[256];
} rustty_config_palette_s;

// config.QuickTerminalSize
typedef enum {
  RUSTTY_QUICK_TERMINAL_SIZE_NONE,
  RUSTTY_QUICK_TERMINAL_SIZE_PERCENTAGE,
  RUSTTY_QUICK_TERMINAL_SIZE_PIXELS,
} rustty_quick_terminal_size_tag_e;

typedef union {
  float percentage;
  uint32_t pixels;
} rustty_quick_terminal_size_value_u;

typedef struct {
  rustty_quick_terminal_size_tag_e tag;
  rustty_quick_terminal_size_value_u value;
} rustty_quick_terminal_size_s;

typedef struct {
  rustty_quick_terminal_size_s primary;
  rustty_quick_terminal_size_s secondary;
} rustty_config_quick_terminal_size_s;

// config.Fullscreen
typedef enum {
  RUSTTY_CONFIG_FULLSCREEN_FALSE,
  RUSTTY_CONFIG_FULLSCREEN_TRUE,
  RUSTTY_CONFIG_FULLSCREEN_NON_NATIVE,
  RUSTTY_CONFIG_FULLSCREEN_NON_NATIVE_VISIBLE_MENU,
  RUSTTY_CONFIG_FULLSCREEN_NON_NATIVE_PADDED_NOTCH,
} rustty_config_fullscreen_e;

// apprt.Target.Key
typedef enum {
  RUSTTY_TARGET_APP,
  RUSTTY_TARGET_SURFACE,
} rustty_target_tag_e;

typedef union {
  rustty_surface_t surface;
} rustty_target_u;

typedef struct {
  rustty_target_tag_e tag;
  rustty_target_u target;
} rustty_target_s;

// apprt.action.SplitDirection
typedef enum {
  RUSTTY_SPLIT_DIRECTION_RIGHT,
  RUSTTY_SPLIT_DIRECTION_DOWN,
  RUSTTY_SPLIT_DIRECTION_LEFT,
  RUSTTY_SPLIT_DIRECTION_UP,
} rustty_action_split_direction_e;

// apprt.action.GotoSplit
typedef enum {
  RUSTTY_GOTO_SPLIT_PREVIOUS,
  RUSTTY_GOTO_SPLIT_NEXT,
  RUSTTY_GOTO_SPLIT_UP,
  RUSTTY_GOTO_SPLIT_LEFT,
  RUSTTY_GOTO_SPLIT_DOWN,
  RUSTTY_GOTO_SPLIT_RIGHT,
} rustty_action_goto_split_e;

// apprt.action.GotoWindow
typedef enum {
  RUSTTY_GOTO_WINDOW_PREVIOUS,
  RUSTTY_GOTO_WINDOW_NEXT,
} rustty_action_goto_window_e;

// apprt.action.ResizeSplit.Direction
typedef enum {
  RUSTTY_RESIZE_SPLIT_UP,
  RUSTTY_RESIZE_SPLIT_DOWN,
  RUSTTY_RESIZE_SPLIT_LEFT,
  RUSTTY_RESIZE_SPLIT_RIGHT,
} rustty_action_resize_split_direction_e;

// apprt.action.ResizeSplit
typedef struct {
  uint16_t amount;
  rustty_action_resize_split_direction_e direction;
} rustty_action_resize_split_s;

// apprt.action.MoveTab
typedef struct {
  ssize_t amount;
} rustty_action_move_tab_s;

// apprt.action.GotoTab
typedef enum {
  RUSTTY_GOTO_TAB_PREVIOUS = -1,
  RUSTTY_GOTO_TAB_NEXT = -2,
  RUSTTY_GOTO_TAB_LAST = -3,
} rustty_action_goto_tab_e;

// apprt.action.Fullscreen
typedef enum {
  RUSTTY_FULLSCREEN_NATIVE,
  RUSTTY_FULLSCREEN_MACOS_NON_NATIVE,
  RUSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU,
  RUSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH,
} rustty_action_fullscreen_e;

// apprt.action.FloatWindow
typedef enum {
  RUSTTY_FLOAT_WINDOW_ON,
  RUSTTY_FLOAT_WINDOW_OFF,
  RUSTTY_FLOAT_WINDOW_TOGGLE,
} rustty_action_float_window_e;

// apprt.action.SecureInput
typedef enum {
  RUSTTY_SECURE_INPUT_ON,
  RUSTTY_SECURE_INPUT_OFF,
  RUSTTY_SECURE_INPUT_TOGGLE,
} rustty_action_secure_input_e;

// apprt.action.Inspector
typedef enum {
  RUSTTY_INSPECTOR_TOGGLE,
  RUSTTY_INSPECTOR_SHOW,
  RUSTTY_INSPECTOR_HIDE,
} rustty_action_inspector_e;

// apprt.action.QuitTimer
typedef enum {
  RUSTTY_QUIT_TIMER_START,
  RUSTTY_QUIT_TIMER_STOP,
} rustty_action_quit_timer_e;

// apprt.action.Readonly
typedef enum {
  RUSTTY_READONLY_OFF,
  RUSTTY_READONLY_ON,
} rustty_action_readonly_e;

// apprt.action.DesktopNotification.C
typedef struct {
  const char* title;
  const char* body;
} rustty_action_desktop_notification_s;

// apprt.action.SetTitle.C
typedef struct {
  const char* title;
} rustty_action_set_title_s;

// apprt.action.PromptTitle
typedef enum {
  RUSTTY_PROMPT_TITLE_SURFACE,
  RUSTTY_PROMPT_TITLE_TAB,
} rustty_action_prompt_title_e;

// apprt.action.Pwd.C
typedef struct {
  const char* pwd;
} rustty_action_pwd_s;

// terminal.MouseShape
typedef enum {
  RUSTTY_MOUSE_SHAPE_DEFAULT,
  RUSTTY_MOUSE_SHAPE_CONTEXT_MENU,
  RUSTTY_MOUSE_SHAPE_HELP,
  RUSTTY_MOUSE_SHAPE_POINTER,
  RUSTTY_MOUSE_SHAPE_PROGRESS,
  RUSTTY_MOUSE_SHAPE_WAIT,
  RUSTTY_MOUSE_SHAPE_CELL,
  RUSTTY_MOUSE_SHAPE_CROSSHAIR,
  RUSTTY_MOUSE_SHAPE_TEXT,
  RUSTTY_MOUSE_SHAPE_VERTICAL_TEXT,
  RUSTTY_MOUSE_SHAPE_ALIAS,
  RUSTTY_MOUSE_SHAPE_COPY,
  RUSTTY_MOUSE_SHAPE_MOVE,
  RUSTTY_MOUSE_SHAPE_NO_DROP,
  RUSTTY_MOUSE_SHAPE_NOT_ALLOWED,
  RUSTTY_MOUSE_SHAPE_GRAB,
  RUSTTY_MOUSE_SHAPE_GRABBING,
  RUSTTY_MOUSE_SHAPE_ALL_SCROLL,
  RUSTTY_MOUSE_SHAPE_COL_RESIZE,
  RUSTTY_MOUSE_SHAPE_ROW_RESIZE,
  RUSTTY_MOUSE_SHAPE_N_RESIZE,
  RUSTTY_MOUSE_SHAPE_E_RESIZE,
  RUSTTY_MOUSE_SHAPE_S_RESIZE,
  RUSTTY_MOUSE_SHAPE_W_RESIZE,
  RUSTTY_MOUSE_SHAPE_NE_RESIZE,
  RUSTTY_MOUSE_SHAPE_NW_RESIZE,
  RUSTTY_MOUSE_SHAPE_SE_RESIZE,
  RUSTTY_MOUSE_SHAPE_SW_RESIZE,
  RUSTTY_MOUSE_SHAPE_EW_RESIZE,
  RUSTTY_MOUSE_SHAPE_NS_RESIZE,
  RUSTTY_MOUSE_SHAPE_NESW_RESIZE,
  RUSTTY_MOUSE_SHAPE_NWSE_RESIZE,
  RUSTTY_MOUSE_SHAPE_ZOOM_IN,
  RUSTTY_MOUSE_SHAPE_ZOOM_OUT,
} rustty_action_mouse_shape_e;

// apprt.action.MouseVisibility
typedef enum {
  RUSTTY_MOUSE_VISIBLE,
  RUSTTY_MOUSE_HIDDEN,
} rustty_action_mouse_visibility_e;

// apprt.action.MouseOverLink
typedef struct {
  const char* url;
  size_t len;
} rustty_action_mouse_over_link_s;

// apprt.action.SizeLimit
typedef struct {
  uint32_t min_width;
  uint32_t min_height;
  uint32_t max_width;
  uint32_t max_height;
} rustty_action_size_limit_s;

// apprt.action.InitialSize
typedef struct {
  uint32_t width;
  uint32_t height;
} rustty_action_initial_size_s;

// apprt.action.CellSize
typedef struct {
  uint32_t width;
  uint32_t height;
} rustty_action_cell_size_s;

// renderer.Health
typedef enum {
  RUSTTY_RENDERER_HEALTH_HEALTHY,
  RUSTTY_RENDERER_HEALTH_UNHEALTHY,
} rustty_action_renderer_health_e;

// apprt.action.KeySequence
typedef struct {
  bool active;
  rustty_input_trigger_s trigger;
} rustty_action_key_sequence_s;

// apprt.action.KeyTable.Tag
typedef enum {
  RUSTTY_KEY_TABLE_ACTIVATE,
  RUSTTY_KEY_TABLE_DEACTIVATE,
  RUSTTY_KEY_TABLE_DEACTIVATE_ALL,
} rustty_action_key_table_tag_e;

// apprt.action.KeyTable.CValue
typedef union {
  struct {
    const char *name;
    size_t len;
  } activate;
} rustty_action_key_table_u;

// apprt.action.KeyTable.C
typedef struct {
  rustty_action_key_table_tag_e tag;
  rustty_action_key_table_u value;
} rustty_action_key_table_s;

// apprt.action.ColorKind
typedef enum {
  RUSTTY_ACTION_COLOR_KIND_FOREGROUND = -1,
  RUSTTY_ACTION_COLOR_KIND_BACKGROUND = -2,
  RUSTTY_ACTION_COLOR_KIND_CURSOR = -3,
} rustty_action_color_kind_e;

// apprt.action.ColorChange
typedef struct {
  rustty_action_color_kind_e kind;
  uint8_t r;
  uint8_t g;
  uint8_t b;
} rustty_action_color_change_s;

// apprt.action.ConfigChange
typedef struct {
  rustty_config_t config;
} rustty_action_config_change_s;

// apprt.action.ReloadConfig
typedef struct {
  bool soft;
} rustty_action_reload_config_s;

// apprt.action.OpenUrlKind
typedef enum {
  RUSTTY_ACTION_OPEN_URL_KIND_UNKNOWN,
  RUSTTY_ACTION_OPEN_URL_KIND_TEXT,
  RUSTTY_ACTION_OPEN_URL_KIND_HTML,
} rustty_action_open_url_kind_e;

// apprt.action.OpenUrl.C
typedef struct {
  rustty_action_open_url_kind_e kind;
  const char* url;
  uintptr_t len;
} rustty_action_open_url_s;

// apprt.action.CloseTabMode
typedef enum {
  RUSTTY_ACTION_CLOSE_TAB_MODE_THIS,
  RUSTTY_ACTION_CLOSE_TAB_MODE_OTHER,
  RUSTTY_ACTION_CLOSE_TAB_MODE_RIGHT,
} rustty_action_close_tab_mode_e;

// apprt.surface.Message.ChildExited
typedef struct {
  uint32_t exit_code;
  uint64_t timetime_ms;
} rustty_surface_message_childexited_s;

// terminal.osc.Command.ProgressReport.State
typedef enum {
  RUSTTY_PROGRESS_STATE_REMOVE,
  RUSTTY_PROGRESS_STATE_SET,
  RUSTTY_PROGRESS_STATE_ERROR,
  RUSTTY_PROGRESS_STATE_INDETERMINATE,
  RUSTTY_PROGRESS_STATE_PAUSE,
} rustty_action_progress_report_state_e;

// terminal.osc.Command.ProgressReport.C
typedef struct {
  rustty_action_progress_report_state_e state;
  // -1 if no progress was reported, otherwise 0-100 indicating percent
  // completeness.
  int8_t progress;
} rustty_action_progress_report_s;

// apprt.action.CommandFinished.C
typedef struct {
  // -1 if no exit code was reported, otherwise 0-255
  int16_t exit_code;
  // number of nanoseconds that command was running for
  uint64_t duration;
} rustty_action_command_finished_s;

// apprt.action.StartSearch.C
typedef struct {
  const char* needle;
} rustty_action_start_search_s;

// apprt.action.SearchTotal
typedef struct {
  ssize_t total;
} rustty_action_search_total_s;

// apprt.action.SearchSelected
typedef struct {
  ssize_t selected;
} rustty_action_search_selected_s;

// terminal.Scrollbar
typedef struct {
  uint64_t total;
  uint64_t offset;
  uint64_t len;
} rustty_action_scrollbar_s;

// apprt.Action.Key
typedef enum {
  RUSTTY_ACTION_QUIT,
  RUSTTY_ACTION_NEW_WINDOW,
  RUSTTY_ACTION_NEW_TAB,
  RUSTTY_ACTION_CLOSE_TAB,
  RUSTTY_ACTION_NEW_SPLIT,
  RUSTTY_ACTION_CLOSE_ALL_WINDOWS,
  RUSTTY_ACTION_TOGGLE_MAXIMIZE,
  RUSTTY_ACTION_TOGGLE_FULLSCREEN,
  RUSTTY_ACTION_TOGGLE_TAB_OVERVIEW,
  RUSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS,
  RUSTTY_ACTION_TOGGLE_QUICK_TERMINAL,
  RUSTTY_ACTION_TOGGLE_COMMAND_PALETTE,
  RUSTTY_ACTION_TOGGLE_VISIBILITY,
  RUSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY,
  RUSTTY_ACTION_MOVE_TAB,
  RUSTTY_ACTION_GOTO_TAB,
  RUSTTY_ACTION_GOTO_SPLIT,
  RUSTTY_ACTION_GOTO_WINDOW,
  RUSTTY_ACTION_RESIZE_SPLIT,
  RUSTTY_ACTION_EQUALIZE_SPLITS,
  RUSTTY_ACTION_TOGGLE_SPLIT_ZOOM,
  RUSTTY_ACTION_PRESENT_TERMINAL,
  RUSTTY_ACTION_SIZE_LIMIT,
  RUSTTY_ACTION_RESET_WINDOW_SIZE,
  RUSTTY_ACTION_INITIAL_SIZE,
  RUSTTY_ACTION_CELL_SIZE,
  RUSTTY_ACTION_SCROLLBAR,
  RUSTTY_ACTION_RENDER,
  RUSTTY_ACTION_INSPECTOR,
  RUSTTY_ACTION_SHOW_GTK_INSPECTOR,
  RUSTTY_ACTION_RENDER_INSPECTOR,
  RUSTTY_ACTION_DESKTOP_NOTIFICATION,
  RUSTTY_ACTION_SET_TITLE,
  RUSTTY_ACTION_SET_TAB_TITLE,
  RUSTTY_ACTION_PROMPT_TITLE,
  RUSTTY_ACTION_PWD,
  RUSTTY_ACTION_MOUSE_SHAPE,
  RUSTTY_ACTION_MOUSE_VISIBILITY,
  RUSTTY_ACTION_MOUSE_OVER_LINK,
  RUSTTY_ACTION_RENDERER_HEALTH,
  RUSTTY_ACTION_OPEN_CONFIG,
  RUSTTY_ACTION_QUIT_TIMER,
  RUSTTY_ACTION_FLOAT_WINDOW,
  RUSTTY_ACTION_SECURE_INPUT,
  RUSTTY_ACTION_KEY_SEQUENCE,
  RUSTTY_ACTION_KEY_TABLE,
  RUSTTY_ACTION_COLOR_CHANGE,
  RUSTTY_ACTION_RELOAD_CONFIG,
  RUSTTY_ACTION_CONFIG_CHANGE,
  RUSTTY_ACTION_CLOSE_WINDOW,
  RUSTTY_ACTION_RING_BELL,
  RUSTTY_ACTION_SELECTION_CHANGED,
  RUSTTY_ACTION_UNDO,
  RUSTTY_ACTION_REDO,
  RUSTTY_ACTION_CHECK_FOR_UPDATES,
  RUSTTY_ACTION_OPEN_URL,
  RUSTTY_ACTION_SHOW_CHILD_EXITED,
  RUSTTY_ACTION_PROGRESS_REPORT,
  RUSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD,
  RUSTTY_ACTION_COMMAND_FINISHED,
  RUSTTY_ACTION_START_SEARCH,
  RUSTTY_ACTION_END_SEARCH,
  RUSTTY_ACTION_SEARCH_TOTAL,
  RUSTTY_ACTION_SEARCH_SELECTED,
  RUSTTY_ACTION_READONLY,
  RUSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD,
} rustty_action_tag_e;

typedef union {
  rustty_action_split_direction_e new_split;
  rustty_action_fullscreen_e toggle_fullscreen;
  rustty_action_move_tab_s move_tab;
  rustty_action_goto_tab_e goto_tab;
  rustty_action_goto_split_e goto_split;
  rustty_action_goto_window_e goto_window;
  rustty_action_resize_split_s resize_split;
  rustty_action_size_limit_s size_limit;
  rustty_action_initial_size_s initial_size;
  rustty_action_cell_size_s cell_size;
  rustty_action_scrollbar_s scrollbar;
  rustty_action_inspector_e inspector;
  rustty_action_desktop_notification_s desktop_notification;
  rustty_action_set_title_s set_title;
  rustty_action_set_title_s set_tab_title;
  rustty_action_prompt_title_e prompt_title;
  rustty_action_pwd_s pwd;
  rustty_action_mouse_shape_e mouse_shape;
  rustty_action_mouse_visibility_e mouse_visibility;
  rustty_action_mouse_over_link_s mouse_over_link;
  rustty_action_renderer_health_e renderer_health;
  rustty_action_quit_timer_e quit_timer;
  rustty_action_float_window_e float_window;
  rustty_action_secure_input_e secure_input;
  rustty_action_key_sequence_s key_sequence;
  rustty_action_key_table_s key_table;
  rustty_action_color_change_s color_change;
  rustty_action_reload_config_s reload_config;
  rustty_action_config_change_s config_change;
  rustty_action_open_url_s open_url;
  rustty_action_close_tab_mode_e close_tab_mode;
  rustty_surface_message_childexited_s child_exited;
  rustty_action_progress_report_s progress_report;
  rustty_action_command_finished_s command_finished;
  rustty_action_start_search_s start_search;
  rustty_action_search_total_s search_total;
  rustty_action_search_selected_s search_selected;
  rustty_action_readonly_e readonly;
} rustty_action_u;

typedef struct {
  rustty_action_tag_e tag;
  rustty_action_u action;
} rustty_action_s;

typedef void (*rustty_runtime_wakeup_cb)(void*);
typedef bool (*rustty_runtime_read_clipboard_cb)(void*,
                                                  rustty_clipboard_e,
                                                  void*);
typedef void (*rustty_runtime_confirm_read_clipboard_cb)(
    void*,
    const char*,
    void*,
    rustty_clipboard_request_e);
typedef void (*rustty_runtime_write_clipboard_cb)(void*,
                                                   rustty_clipboard_e,
                                                   const rustty_clipboard_content_s*,
                                                   size_t,
                                                   bool);
typedef void (*rustty_runtime_close_surface_cb)(void*, bool);
typedef bool (*rustty_runtime_action_cb)(rustty_app_t,
                                          rustty_target_s,
                                          rustty_action_s);

typedef struct {
  void* userdata;
  bool supports_selection_clipboard;
  rustty_runtime_wakeup_cb wakeup_cb;
  rustty_runtime_action_cb action_cb;
  rustty_runtime_read_clipboard_cb read_clipboard_cb;
  rustty_runtime_confirm_read_clipboard_cb confirm_read_clipboard_cb;
  rustty_runtime_write_clipboard_cb write_clipboard_cb;
  rustty_runtime_close_surface_cb close_surface_cb;
} rustty_runtime_config_s;

// apprt.ipc.Target.Key
typedef enum {
  RUSTTY_IPC_TARGET_CLASS,
  RUSTTY_IPC_TARGET_DETECT,
} rustty_ipc_target_tag_e;

typedef union {
  char *klass;
} rustty_ipc_target_u;

typedef struct {
  rustty_ipc_target_tag_e tag;
  rustty_ipc_target_u target;
} chostty_ipc_target_s;

// apprt.ipc.Action.NewWindow
typedef struct {
  // This should be a null terminated list of strings.
  const char **arguments;
} rustty_ipc_action_new_window_s;

typedef union {
  rustty_ipc_action_new_window_s new_window;
} rustty_ipc_action_u;

// apprt.ipc.Action.Key
typedef enum {
  RUSTTY_IPC_ACTION_NEW_WINDOW,
  RUSTTY_IPC_ACTION_TOGGLE_QUICK_TERMINAL,
} rustty_ipc_action_tag_e;

//-------------------------------------------------------------------
// Published API

RUSTTY_API int rustty_init(uintptr_t, char**);
RUSTTY_API void rustty_cli_try_action(void);
RUSTTY_API rustty_info_s rustty_info(void);
RUSTTY_API const char* rustty_translate(const char*);
RUSTTY_API void rustty_string_free(rustty_string_s);

RUSTTY_API rustty_config_t rustty_config_new();
RUSTTY_API void rustty_config_free(rustty_config_t);
RUSTTY_API rustty_config_t rustty_config_clone(rustty_config_t);
RUSTTY_API void rustty_config_load_cli_args(rustty_config_t);
RUSTTY_API void rustty_config_load_file(rustty_config_t, const char*);
RUSTTY_API void rustty_config_load_default_files(rustty_config_t);
RUSTTY_API void rustty_config_load_recursive_files(rustty_config_t);
RUSTTY_API void rustty_config_finalize(rustty_config_t);
RUSTTY_API bool rustty_config_get(rustty_config_t, void*, const char*, uintptr_t);
RUSTTY_API rustty_input_trigger_s rustty_config_trigger(rustty_config_t,
                                                              const char*,
                                                              uintptr_t);
RUSTTY_API bool rustty_config_key_is_binding(rustty_config_t, rustty_input_key_s);
RUSTTY_API uint32_t rustty_config_diagnostics_count(rustty_config_t);
RUSTTY_API rustty_diagnostic_s rustty_config_get_diagnostic(rustty_config_t, uint32_t);
RUSTTY_API rustty_string_s rustty_config_open_path(void);

RUSTTY_API rustty_app_t rustty_app_new(const rustty_runtime_config_s*,
                                             rustty_config_t);
RUSTTY_API void rustty_app_free(rustty_app_t);
RUSTTY_API void rustty_app_tick(rustty_app_t);
RUSTTY_API void* rustty_app_userdata(rustty_app_t);
RUSTTY_API void rustty_app_set_focus(rustty_app_t, bool);
RUSTTY_API bool rustty_app_key(rustty_app_t, rustty_input_key_s);
RUSTTY_API void rustty_app_keyboard_changed(rustty_app_t);
RUSTTY_API void rustty_app_open_config(rustty_app_t);
RUSTTY_API void rustty_app_update_config(rustty_app_t, rustty_config_t);
RUSTTY_API bool rustty_app_needs_confirm_quit(rustty_app_t);
RUSTTY_API bool rustty_app_has_global_keybinds(rustty_app_t);
RUSTTY_API void rustty_app_set_color_scheme(rustty_app_t, rustty_color_scheme_e);

RUSTTY_API rustty_surface_config_s rustty_surface_config_new();

RUSTTY_API rustty_surface_t rustty_surface_new(rustty_app_t,
                                                     const rustty_surface_config_s*);
RUSTTY_API void rustty_surface_free(rustty_surface_t);
RUSTTY_API void* rustty_surface_userdata(rustty_surface_t);
RUSTTY_API rustty_app_t rustty_surface_app(rustty_surface_t);
RUSTTY_API rustty_surface_config_s rustty_surface_inherited_config(rustty_surface_t, rustty_surface_context_e);
RUSTTY_API void rustty_surface_update_config(rustty_surface_t, rustty_config_t);
RUSTTY_API bool rustty_surface_needs_confirm_quit(rustty_surface_t);
RUSTTY_API bool rustty_surface_process_exited(rustty_surface_t);
RUSTTY_API void rustty_surface_refresh(rustty_surface_t);
RUSTTY_API void rustty_surface_draw(rustty_surface_t);
RUSTTY_API void rustty_surface_set_content_scale(rustty_surface_t, double, double);
RUSTTY_API void rustty_surface_set_focus(rustty_surface_t, bool);
RUSTTY_API void rustty_surface_set_occlusion(rustty_surface_t, bool);
RUSTTY_API void rustty_surface_set_size(rustty_surface_t, uint32_t, uint32_t);
RUSTTY_API rustty_surface_size_s rustty_surface_size(rustty_surface_t);
RUSTTY_API uint64_t rustty_surface_foreground_pid(rustty_surface_t);
RUSTTY_API rustty_string_s rustty_surface_tty_name(rustty_surface_t);
RUSTTY_API void rustty_surface_set_color_scheme(rustty_surface_t,
                                                     rustty_color_scheme_e);
RUSTTY_API rustty_input_mods_e rustty_surface_key_translation_mods(rustty_surface_t,
                                                                         rustty_input_mods_e);
RUSTTY_API bool rustty_surface_key(rustty_surface_t, rustty_input_key_s);
RUSTTY_API bool rustty_surface_key_is_binding(rustty_surface_t,
                                                   rustty_input_key_s,
                                                   rustty_binding_flags_e*);
RUSTTY_API void rustty_surface_text(rustty_surface_t, const char*, uintptr_t);
RUSTTY_API void rustty_surface_preedit(rustty_surface_t, const char*, uintptr_t);
RUSTTY_API bool rustty_surface_mouse_captured(rustty_surface_t);
RUSTTY_API bool rustty_surface_mouse_button(rustty_surface_t,
                                                 rustty_input_mouse_state_e,
                                                 rustty_input_mouse_button_e,
                                                 rustty_input_mods_e);
RUSTTY_API void rustty_surface_mouse_pos(rustty_surface_t,
                                              double,
                                              double,
                                              rustty_input_mods_e);
RUSTTY_API void rustty_surface_mouse_scroll(rustty_surface_t,
                                                 double,
                                                 double,
                                                 rustty_input_scroll_mods_t);
RUSTTY_API void rustty_surface_mouse_pressure(rustty_surface_t, uint32_t, double);
RUSTTY_API void rustty_surface_ime_point(rustty_surface_t, double*, double*, double*, double*);
RUSTTY_API void rustty_surface_request_close(rustty_surface_t);
RUSTTY_API void rustty_surface_split(rustty_surface_t, rustty_action_split_direction_e);
RUSTTY_API void rustty_surface_split_focus(rustty_surface_t,
                                                rustty_action_goto_split_e);
RUSTTY_API void rustty_surface_split_resize(rustty_surface_t,
                                                 rustty_action_resize_split_direction_e,
                                                 uint16_t);
RUSTTY_API void rustty_surface_split_equalize(rustty_surface_t);
RUSTTY_API bool rustty_surface_binding_action(rustty_surface_t, const char*, uintptr_t);
RUSTTY_API void rustty_surface_complete_clipboard_request(rustty_surface_t,
                                                               const char*,
                                                               void*,
                                                               bool);
RUSTTY_API bool rustty_surface_has_selection(rustty_surface_t);
RUSTTY_API bool rustty_surface_read_selection(rustty_surface_t, rustty_text_s*);
RUSTTY_API bool rustty_surface_read_text(rustty_surface_t,
                                              rustty_selection_s,
                                              rustty_text_s*);
RUSTTY_API void rustty_surface_free_text(rustty_surface_t, rustty_text_s*);

#ifdef __APPLE__
RUSTTY_API void rustty_surface_set_display_id(rustty_surface_t, uint32_t);
RUSTTY_API void* rustty_surface_quicklook_font(rustty_surface_t);
RUSTTY_API bool rustty_surface_quicklook_word(rustty_surface_t, rustty_text_s*);
#endif

RUSTTY_API rustty_inspector_t rustty_surface_inspector(rustty_surface_t);
RUSTTY_API void rustty_inspector_free(rustty_surface_t);
RUSTTY_API void rustty_inspector_set_focus(rustty_inspector_t, bool);
RUSTTY_API void rustty_inspector_set_content_scale(rustty_inspector_t, double, double);
RUSTTY_API void rustty_inspector_set_size(rustty_inspector_t, uint32_t, uint32_t);
RUSTTY_API void rustty_inspector_mouse_button(rustty_inspector_t,
                                                   rustty_input_mouse_state_e,
                                                   rustty_input_mouse_button_e,
                                                   rustty_input_mods_e);
RUSTTY_API void rustty_inspector_mouse_pos(rustty_inspector_t, double, double);
RUSTTY_API void rustty_inspector_mouse_scroll(rustty_inspector_t,
                                                   double,
                                                   double,
                                                   rustty_input_scroll_mods_t);
RUSTTY_API void rustty_inspector_key(rustty_inspector_t,
                                          rustty_input_action_e,
                                          rustty_input_key_e,
                                          rustty_input_mods_e);
RUSTTY_API void rustty_inspector_text(rustty_inspector_t, const char*);

#ifdef __APPLE__
RUSTTY_API bool rustty_inspector_metal_init(rustty_inspector_t, void*);
RUSTTY_API void rustty_inspector_metal_render(rustty_inspector_t, void*, void*);
RUSTTY_API bool rustty_inspector_metal_shutdown(rustty_inspector_t);
#endif

// APIs I'd like to get rid of eventually but are still needed for now.
// Don't use these unless you know what you're doing.
RUSTTY_API void rustty_set_window_background_blur(rustty_app_t, void*);

// Benchmark API, if available.
RUSTTY_API bool rustty_benchmark_cli(const char*, const char*);

#ifdef __cplusplus
}
#endif

#endif /* RUSTTY_H */
