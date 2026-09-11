//! PORTS: ghostty `src/apprt/action.zig` — the `Target`/`Action` types and every
//! action payload.
//!
//! LIFETIMES: `Action<'a>` (and the borrow-carrying payloads) carry ONE lifetime
//! `'a`: every reference-slice field `[]const u8`/`[:0]const u8` becomes a
//! borrowed `&'a [u8]` (raw bytes, never a lossy `String`; see the byte-slice
//! convention) and `*const Config` becomes `&'a Config`.
//!
//! TARGET SEAM: `Target<'s, C: CoreSurfaceHandle>` reaches the opaque
//! `rt_surface` handle through the [`CoreSurfaceHandle`] trait, so this crate
//! never names rustty-core — the edge that breaks the apprt<->core cycle.
//!
//! C-ABI PROJECTION (deferred pinning): the `CValue` extern unions, the `C`
//! extern structs, and `cval()` reproduce the reference's C projection, and the
//! `comptime { assert(@sizeOf(CValue) == ...) }` block is reproduced as a Rust
//! `const _: () = assert!(size_of::<...>() == EXPECTED)` computed from the ACTUAL
//! Rust layout. Byte-exact matching of `rustty.h` (the reference's 16/24 counts)
//! is DEFERRED to the apprt embedded C-API milestone (the vt-c D2 cbindgen +
//! pinned-padding pattern); the numbers here are the Rust layout, NOT a
//! guaranteed ABI.

use core::ffi::c_void;

use rustty_config::value_types::Duration;
use rustty_config::Config;
use rustty_input::binding::{Trigger, TriggerC};
use rustty_renderer::Health;
use rustty_vt::mouse::Shape as MouseShape;
use rustty_vt::osc::{ProgressReport, ProgressReportC};
use rustty_vt::pagelist::grow::{Scrollbar, ScrollbarC};

use crate::seam::CoreSurfaceHandle;
use crate::surface::ChildExited;

/// The namespace `@typeName(@This())` prints in front of the payload type names in
/// this file's `format` functions: `src/apprt/action.zig` is reached from the
/// reference's module root as `apprt.action`, so e.g. `SetTitle` prints as
/// `apprt.action.SetTitle{ ... }`. Measured, not inferred: Zig 0.16.0 (the pin's
/// `minimum_zig_version`) prints exactly `apprt.action.SetTitle` for a struct in
/// `apprt/action.zig` imported through `apprt.zig` from a `src/main.zig` root.
pub(crate) const TYPE_NAME_PREFIX: &str = "apprt.action.";

/// The target for an action. This is generally the thing that had focus
/// while the action was made but the concept of "focus" is not guaranteed
/// since actions can also be triggered by timers, scripts, etc.
//
// PORT NOTE: reference `Target = union(Key) { app, surface: *CoreSurface }`. To
// avoid naming rustty-core, the `surface` arm borrows a `C: CoreSurfaceHandle`
// (implemented by core's `Surface`) — the cycle-break bridge.
pub enum Target<'s, C: CoreSurfaceHandle> {
    App,
    Surface(&'s C),
}

// Sync with: rustty_target_tag_e
/// PORT NOTE: reference `Target.Key = enum(c_int) { app, surface }`. The
/// `ghostty.h Target.Key` -> `checkGhosttyHEnum` test lives in `action_tests.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TargetKey {
    App,
    Surface,
}

// Sync with: rustty_target_u
//
// PORT NOTE: reference `Target.CValue = extern union { app: void, surface:
// *apprt.Surface }`. The `surface` arm is the opaque runtime handle; modeled as
// `*mut c_void`. Union fields are the C-ABI projection surface (read by the
// future embedded C-API), hence `allow(dead_code)`.
#[allow(dead_code)]
#[repr(C)]
pub union TargetCValue {
    pub app: (),
    pub surface: *mut c_void,
}

// Sync with: rustty_target_s
#[repr(C)]
pub struct TargetC {
    pub key: TargetKey,
    pub value: TargetCValue,
}

impl<'s, C: CoreSurfaceHandle> Target<'s, C> {
    /// The active tag (`@as(Key, self)`).
    pub fn key(&self) -> TargetKey {
        match self {
            Target::App => TargetKey::App,
            Target::Surface(_) => TargetKey::Surface,
        }
    }

    /// Convert to rustty_target_s.
    //
    // PORT NOTE: reference `.surface => |v| .{ .surface = v.rt_surface }` — reach
    // the opaque `rt_surface` through the [`CoreSurfaceHandle`] trait.
    pub fn cval(&self) -> TargetC {
        TargetC {
            key: self.key(),
            value: match self {
                Target::App => TargetCValue { app: () },
                Target::Surface(v) => TargetCValue {
                    surface: v.rt_surface_ptr(),
                },
            },
        }
    }
}

/// The possible actions an apprt has to react to. Actions are one-way
/// messages that are sent to the app runtime to trigger some behavior.
///
/// Actions are very often key binding actions but can also be triggered
/// by lifecycle events. For example, the `quit_timer` action is not bindable.
///
/// Importantly, actions are generally OPTIONAL to implement by an apprt.
/// Required functionality is called directly on the runtime structure so
/// there is a compiler error if an action is not implemented.
pub enum Action<'a> {
    // A GUIDE TO ADDING NEW ACTIONS:
    //
    // 1. Add the action to the `Key` enum. The order of the enum matters
    //    because it maps directly to the libghostty C enum. For ABI
    //    compatibility, new actions should be added to the end of the enum.
    //
    // 2. Add the action and optional value to the Action union.
    //
    // 3. If the value type is not void, ensure the value is C ABI
    //    compatible (extern). If it is not, add a `C` decl to the value
    //    and a `cval` function to convert to the C ABI compatible value.
    //
    // 4. Update `include/rustty.h`: add the new key, value, and union
    //    entry. If the value type is void then only the key needs to be
    //    added. Ensure the order matches exactly with the Zig code.
    /// Quit the application.
    Quit,

    /// Open a new window. The target determines whether properties such
    /// as font size should be inherited.
    NewWindow,

    /// Open a new tab. If the target is a surface it should be opened in
    /// the same window as the surface. If the target is the app then
    /// the tab should be opened in a new window.
    NewTab,

    /// Closes the tab belonging to the currently focused split, or all other
    /// tabs, depending on the mode.
    CloseTab(CloseTabMode),

    /// Create a new split. The value determines the location of the split
    /// relative to the target.
    NewSplit(SplitDirection),

    /// Close all open windows.
    CloseAllWindows,

    /// Toggle maximized window state.
    ToggleMaximize,

    /// Toggle fullscreen mode.
    ToggleFullscreen(Fullscreen),

    /// Toggle tab overview.
    ToggleTabOverview,

    /// Toggle whether window directions are shown.
    ToggleWindowDecorations,

    /// Toggle the quick terminal in or out.
    ToggleQuickTerminal,

    /// Toggle the command palette.
    ToggleCommandPalette,

    /// Toggle the visibility of all rustty terminal windows.
    ToggleVisibility,

    /// Toggle the window background opacity. This only has an effect
    /// if the window started as transparent (non-opaque), and toggles
    /// it between fully opaque and the configured background opacity.
    ToggleBackgroundOpacity,

    /// Moves a tab by a relative offset.
    ///
    /// Adjusts the tab position based on `offset` (e.g., -1 for left, +1
    /// for right). If the new position is out of bounds, it wraps around
    /// cyclically within the tab range.
    MoveTab(MoveTab),

    /// Jump to a specific tab. Must handle the scenario that the tab
    /// value is invalid.
    GotoTab(GotoTab),

    /// Jump to a specific split.
    GotoSplit(GotoSplit),

    /// Jump to next/previous window.
    GotoWindow(GotoWindow),

    /// Resize the split in the given direction.
    ResizeSplit(ResizeSplit),

    /// Equalize all the splits in the target window.
    EqualizeSplits,

    /// Toggle whether a split is zoomed or not. A zoomed split is resized
    /// to take up the entire window.
    ToggleSplitZoom,

    /// Present the target terminal whether its a tab, split, or window.
    PresentTerminal,

    /// Sets a size limit (in pixels) for the target terminal.
    SizeLimit(SizeLimit),

    /// Resets the window size to the default size. See the
    /// `reset_window_size` keybinding for more information.
    ResetWindowSize,

    /// Specifies the initial size of the target terminal.
    ///
    /// This may be sent once during the initialization of a surface
    /// (as part of the init call) to indicate the initial size requested
    /// for the window if it is not maximized or fullscreen.
    ///
    /// This may also be sent at any time after the surface is initialized
    /// to note the new "default size" of the window. This should in general
    /// be ignored, but may be useful if the apprt wants to support
    /// a "return to default size" action.
    InitialSize(InitialSize),

    /// The cell size has changed to the given dimensions in pixels.
    CellSize(CellSize),

    /// The scrollbar is updating.
    Scrollbar(Scrollbar),

    /// The target should be re-rendered. This usually has a specific
    /// surface target but if the app is targeted then all active
    /// surfaces should be redrawn.
    Render,

    /// Control whether the inspector is shown or hidden.
    Inspector(Inspector),

    /// Show the GTK inspector.
    ShowGtkInspector,

    /// The inspector for the given target has changes and should be
    /// rendered at the next opportunity.
    RenderInspector,

    /// Export the Terminal IO inspector event log.
    ExportTerminalIO(ExportTerminalIO<'a>),

    /// Show a desktop notification.
    DesktopNotification(DesktopNotification<'a>),

    /// Set the title of the target to the requested value.
    SetTitle(SetTitle<'a>),

    /// Set the tab title override for the target's tab.
    SetTabTitle(SetTitle<'a>),

    /// Set the window title override for the target's tab.
    SetWindowTitle(SetTitle<'a>),

    /// Set the title of the target to a prompted value. It is up to
    /// the apprt to prompt. The value specifies whether to prompt for the
    /// surface title or the tab title.
    PromptTitle(PromptTitle),

    /// The current working directory has changed for the target terminal.
    Pwd(Pwd<'a>),

    /// Set the mouse cursor shape.
    MouseShape(MouseShape),

    /// Set whether the mouse cursor is visible or not.
    MouseVisibility(MouseVisibility),

    /// Called when the mouse is over or recently left a link.
    MouseOverLink(MouseOverLink<'a>),

    /// The health of the renderer has changed.
    RendererHealth(Health),

    /// Open the rustty configuration. This is platform-specific about
    /// what it means; it can mean opening a dedicated UI or just opening
    /// a file in a text editor.
    OpenConfig(OpenConfig),

    /// Called when there are no more surfaces and the app should quit
    /// after the configured delay.
    ///
    /// Despite the name, this is the notification that librustty sends
    /// when there are no more surfaces regardless of if the configuration
    /// wants to quit after close, has any delay set, etc. It's up to the
    /// apprt to implement the proper logic based on the config.
    ///
    /// This can be cancelled by sending another quit_timer action with "stop".
    /// Multiple "starts" shouldn't happen and can be ignored or cause a
    /// restart it isn't that important.
    QuitTimer(QuitTimer),

    /// Set the window floating state. A floating window is one that is
    /// always on top of other windows even when not focused.
    FloatWindow(FloatWindow),

    /// Set the secure input functionality on or off. "Secure input" means
    /// that the user is currently at some sort of prompt where they may be
    /// entering a password or other sensitive information. This can be used
    /// by the app runtime to change the appearance of the cursor, setup
    /// system APIs to not log the input, etc.
    SecureInput(SecureInput),

    /// A sequenced key binding has started, continued, or stopped.
    /// The UI should show some indication that the user is in a sequenced
    /// key mode because other input may be ignored.
    KeySequence(KeySequence),

    /// A key table has been activated or deactivated.
    KeyTable(KeyTable<'a>),

    /// A terminal color was changed programmatically through things
    /// such as OSC 10/11.
    ColorChange(ColorChange),

    /// A request to reload the configuration. The reload request can be
    /// from a user or for some internal reason. The reload request may
    /// request it is a soft reload or a full reload. See the struct for
    /// more documentation.
    ///
    /// The configuration should be passed to updateConfig either at the
    /// app or surface level depending on the target.
    ReloadConfig(ReloadConfig),

    /// The configuration has changed. The value is a pointer to the new
    /// configuration. The pointer is only valid for the duration of the
    /// action and should not be stored.
    ///
    /// This should be used by apprts to update any internal state that
    /// depends on configuration for the given target (i.e. headerbar colors).
    /// The apprt should copy any data it needs since the memory lifetime
    /// is only valid for the duration of the action.
    ///
    /// This allows an apprt to have config-dependent state reactively
    /// change without having to store the entire configuration or poll
    /// for changes.
    ConfigChange(ConfigChange<'a>),

    /// Closes the currently focused window.
    CloseWindow,

    /// Called when the bell character is seen. The apprt should do whatever
    /// it needs to ring the bell. This is usually a sound or visual effect.
    RingBell,

    /// Called when the active selection changes. The apprt should read the
    /// current selection itself; this carries no payload.
    SelectionChanged,

    /// Undo the last action. See the "undo" keybinding for more
    /// details on what can and cannot be undone.
    Undo,

    /// Redo the last undone action.
    Redo,

    CheckForUpdates,

    /// Open a URL using the native OS mechanisms. On macOS this might be `open`
    /// or on Linux this might be `xdg-open`. The exact mechanism is up to the
    /// apprt.
    OpenUrl(OpenUrl<'a>),

    /// Show a native GUI notification that the child process has exited.
    ShowChildExited(ChildExited),

    /// Show a native GUI notification about the progress of some TUI operation.
    ProgressReport(ProgressReport),

    /// Show the on-screen keyboard.
    ShowOnScreenKeyboard,

    /// A command has finished,
    CommandFinished(CommandFinished),

    /// Start the search overlay with an optional initial needle. If the
    /// search is already active and the needle is non-empty, update the
    /// current search needle and focus the search input.
    StartSearch(StartSearch<'a>),

    /// End the search overlay, clearing the search state and hiding it.
    EndSearch,

    /// The total number of matches found by the search.
    SearchTotal(SearchTotal),

    /// The currently selected search match index (1-based).
    SearchSelected(SearchSelected),

    /// The readonly state of the surface has changed.
    Readonly(Readonly),

    /// Copy the effective title of the surface to the clipboard.
    /// The effective title is the user-overridden title if set,
    /// otherwise the terminal-set title.
    CopyTitleToClipboard,

    /// Move a tab to a new window.
    MoveTabToNewWindow,
}

/// Sync with: rustty_action_tag_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ActionKey {
    Quit,
    NewWindow,
    NewTab,
    CloseTab,
    NewSplit,
    CloseAllWindows,
    ToggleMaximize,
    ToggleFullscreen,
    ToggleTabOverview,
    ToggleWindowDecorations,
    ToggleQuickTerminal,
    ToggleCommandPalette,
    ToggleVisibility,
    ToggleBackgroundOpacity,
    MoveTab,
    GotoTab,
    GotoSplit,
    GotoWindow,
    ResizeSplit,
    EqualizeSplits,
    ToggleSplitZoom,
    PresentTerminal,
    SizeLimit,
    ResetWindowSize,
    InitialSize,
    CellSize,
    Scrollbar,
    Render,
    Inspector,
    ShowGtkInspector,
    RenderInspector,
    ExportTerminalIO,
    DesktopNotification,
    SetTitle,
    SetTabTitle,
    SetWindowTitle,
    PromptTitle,
    Pwd,
    MouseShape,
    MouseVisibility,
    MouseOverLink,
    RendererHealth,
    OpenConfig,
    QuitTimer,
    FloatWindow,
    SecureInput,
    KeySequence,
    KeyTable,
    ColorChange,
    ReloadConfig,
    ConfigChange,
    CloseWindow,
    RingBell,
    SelectionChanged,
    Undo,
    Redo,
    CheckForUpdates,
    OpenUrl,
    ShowChildExited,
    ProgressReport,
    ShowOnScreenKeyboard,
    CommandFinished,
    StartSearch,
    EndSearch,
    SearchTotal,
    SearchSelected,
    Readonly,
    CopyTitleToClipboard,
    MoveTabToNewWindow,
}

/// Sync with: rustty_action_u
///
/// PORT NOTE: reference `Action.CValue` is a comptime-generated `extern union`
/// with one field per key — void keys get a `void` field, others get the
/// payload's `C` form (`Type.C` when it exists, else the payload type). Rust has
/// no comptime type synthesis, so the union is written out explicitly. Fields
/// are the C-ABI projection surface (read only by the future embedded C-API),
/// hence `allow(dead_code)`. Reference comptime-loop comment preserved verbatim:
//
//   // Types can provide custom types for their CValue.
//   if (Type != void and @hasDecl(Type, "C")) break :t Type.C;
#[allow(dead_code)]
#[repr(C)]
pub union ActionCValue {
    pub quit: (),
    pub new_window: (),
    pub new_tab: (),
    pub close_tab: CloseTabMode,
    pub new_split: SplitDirection,
    pub close_all_windows: (),
    pub toggle_maximize: (),
    pub toggle_fullscreen: Fullscreen,
    pub toggle_tab_overview: (),
    pub toggle_window_decorations: (),
    pub toggle_quick_terminal: (),
    pub toggle_command_palette: (),
    pub toggle_visibility: (),
    pub toggle_background_opacity: (),
    pub move_tab: MoveTab,
    pub goto_tab: GotoTab,
    pub goto_split: GotoSplit,
    pub goto_window: GotoWindow,
    pub resize_split: ResizeSplit,
    pub equalize_splits: (),
    pub toggle_split_zoom: (),
    pub present_terminal: (),
    pub size_limit: SizeLimit,
    pub reset_window_size: (),
    pub initial_size: InitialSize,
    pub cell_size: CellSize,
    pub scrollbar: ScrollbarC,
    pub render: (),
    pub inspector: Inspector,
    pub show_gtk_inspector: (),
    pub render_inspector: (),
    pub export_terminal_io: ExportTerminalIOC,
    pub desktop_notification: DesktopNotificationC,
    pub set_title: SetTitleC,
    pub set_tab_title: SetTitleC,
    pub set_window_title: SetTitleC,
    pub prompt_title: PromptTitle,
    pub pwd: PwdC,
    pub mouse_shape: MouseShape,
    pub mouse_visibility: MouseVisibility,
    pub mouse_over_link: MouseOverLinkC,
    pub renderer_health: Health,
    pub open_config: OpenConfig,
    pub quit_timer: QuitTimer,
    pub float_window: FloatWindow,
    pub secure_input: SecureInput,
    pub key_sequence: KeySequenceC,
    pub key_table: KeyTableC,
    pub color_change: ColorChange,
    pub reload_config: ReloadConfig,
    pub config_change: ConfigChangeC,
    pub close_window: (),
    pub ring_bell: (),
    pub selection_changed: (),
    pub undo: (),
    pub redo: (),
    pub check_for_updates: (),
    pub open_url: OpenUrlC,
    pub show_child_exited: ChildExited,
    pub progress_report: ProgressReportC,
    pub show_on_screen_keyboard: (),
    pub command_finished: CommandFinishedC,
    pub start_search: StartSearchC,
    pub end_search: (),
    pub search_total: SearchTotalC,
    pub search_selected: SearchSelectedC,
    pub readonly: Readonly,
    pub copy_title_to_clipboard: (),
    pub move_tab_to_new_window: (),
}

/// Sync with: rustty_action_s
#[repr(C)]
pub struct ActionC {
    pub key: ActionKey,
    pub value: ActionCValue,
}

// PORT NOTE: reference `comptime { assert(@sizeOf(CValue) == switch(@sizeOf(usize))
// { 4 => 24, 8 => 24 }); }` (0a183c923 widened the 4-byte arm 16 -> 24).
// Reference comptime-block comments preserved verbatim:
//   For ABI compatibility, we expect that this is our union size.
//   At the time of writing, we don't promise ABI compatibility
//   so we can change this but I want to be aware of it.
// Reproduced against the ACTUAL Rust layout (64-bit: `OpenUrlC` = kind:i32 +
// url:*const u8 + len:usize = 24 bytes is the widest arm). Byte-exact `rustty.h`
// ABI pinning is DEFERRED to the embedded C-API milestone; this asserts the Rust
// layout, not a guaranteed ABI.
const _: () = assert!(core::mem::size_of::<ActionCValue>() == 24);

impl<'a> Action<'a> {
    /// The active tag (`@as(Key, self)`).
    pub fn key(&self) -> ActionKey {
        match self {
            Action::Quit => ActionKey::Quit,
            Action::NewWindow => ActionKey::NewWindow,
            Action::NewTab => ActionKey::NewTab,
            Action::CloseTab(_) => ActionKey::CloseTab,
            Action::NewSplit(_) => ActionKey::NewSplit,
            Action::CloseAllWindows => ActionKey::CloseAllWindows,
            Action::ToggleMaximize => ActionKey::ToggleMaximize,
            Action::ToggleFullscreen(_) => ActionKey::ToggleFullscreen,
            Action::ToggleTabOverview => ActionKey::ToggleTabOverview,
            Action::ToggleWindowDecorations => ActionKey::ToggleWindowDecorations,
            Action::ToggleQuickTerminal => ActionKey::ToggleQuickTerminal,
            Action::ToggleCommandPalette => ActionKey::ToggleCommandPalette,
            Action::ToggleVisibility => ActionKey::ToggleVisibility,
            Action::ToggleBackgroundOpacity => ActionKey::ToggleBackgroundOpacity,
            Action::MoveTab(_) => ActionKey::MoveTab,
            Action::GotoTab(_) => ActionKey::GotoTab,
            Action::GotoSplit(_) => ActionKey::GotoSplit,
            Action::GotoWindow(_) => ActionKey::GotoWindow,
            Action::ResizeSplit(_) => ActionKey::ResizeSplit,
            Action::EqualizeSplits => ActionKey::EqualizeSplits,
            Action::ToggleSplitZoom => ActionKey::ToggleSplitZoom,
            Action::PresentTerminal => ActionKey::PresentTerminal,
            Action::SizeLimit(_) => ActionKey::SizeLimit,
            Action::ResetWindowSize => ActionKey::ResetWindowSize,
            Action::InitialSize(_) => ActionKey::InitialSize,
            Action::CellSize(_) => ActionKey::CellSize,
            Action::Scrollbar(_) => ActionKey::Scrollbar,
            Action::Render => ActionKey::Render,
            Action::Inspector(_) => ActionKey::Inspector,
            Action::ShowGtkInspector => ActionKey::ShowGtkInspector,
            Action::RenderInspector => ActionKey::RenderInspector,
            Action::ExportTerminalIO(_) => ActionKey::ExportTerminalIO,
            Action::DesktopNotification(_) => ActionKey::DesktopNotification,
            Action::SetTitle(_) => ActionKey::SetTitle,
            Action::SetTabTitle(_) => ActionKey::SetTabTitle,
            Action::SetWindowTitle(_) => ActionKey::SetWindowTitle,
            Action::PromptTitle(_) => ActionKey::PromptTitle,
            Action::Pwd(_) => ActionKey::Pwd,
            Action::MouseShape(_) => ActionKey::MouseShape,
            Action::MouseVisibility(_) => ActionKey::MouseVisibility,
            Action::MouseOverLink(_) => ActionKey::MouseOverLink,
            Action::RendererHealth(_) => ActionKey::RendererHealth,
            Action::OpenConfig(_) => ActionKey::OpenConfig,
            Action::QuitTimer(_) => ActionKey::QuitTimer,
            Action::FloatWindow(_) => ActionKey::FloatWindow,
            Action::SecureInput(_) => ActionKey::SecureInput,
            Action::KeySequence(_) => ActionKey::KeySequence,
            Action::KeyTable(_) => ActionKey::KeyTable,
            Action::ColorChange(_) => ActionKey::ColorChange,
            Action::ReloadConfig(_) => ActionKey::ReloadConfig,
            Action::ConfigChange(_) => ActionKey::ConfigChange,
            Action::CloseWindow => ActionKey::CloseWindow,
            Action::RingBell => ActionKey::RingBell,
            Action::SelectionChanged => ActionKey::SelectionChanged,
            Action::Undo => ActionKey::Undo,
            Action::Redo => ActionKey::Redo,
            Action::CheckForUpdates => ActionKey::CheckForUpdates,
            Action::OpenUrl(_) => ActionKey::OpenUrl,
            Action::ShowChildExited(_) => ActionKey::ShowChildExited,
            Action::ProgressReport(_) => ActionKey::ProgressReport,
            Action::ShowOnScreenKeyboard => ActionKey::ShowOnScreenKeyboard,
            Action::CommandFinished(_) => ActionKey::CommandFinished,
            Action::StartSearch(_) => ActionKey::StartSearch,
            Action::EndSearch => ActionKey::EndSearch,
            Action::SearchTotal(_) => ActionKey::SearchTotal,
            Action::SearchSelected(_) => ActionKey::SearchSelected,
            Action::Readonly(_) => ActionKey::Readonly,
            Action::CopyTitleToClipboard => ActionKey::CopyTitleToClipboard,
            Action::MoveTabToNewWindow => ActionKey::MoveTabToNewWindow,
        }
    }

    // PORT NOTE: reference `pub fn Value(comptime key: Key) type` — its doc
    // comment "Returns the value type for the given key." is preserved verbatim
    // here. It is a comptime type-level function with no runtime Rust equivalent
    // (Rust functions cannot return types); it is inherent in the `Action`/
    // `ActionCValue` field correspondence above and needs no runtime form.

    /// Convert to rustty_action_s.
    //
    // PORT NOTE: reference `cval` does `inline else => |v, tag| @unionInit(CValue,
    // @tagName(tag), if hasDecl(v, "cval") v.cval() else v)`. Written out as a
    // match: payloads with a `cval()` (external `Scrollbar`/`ProgressReport` and
    // the local `*C` types) are projected; the rest go in directly.
    pub fn cval(&self) -> ActionC {
        let value: ActionCValue = match self {
            Action::Quit => ActionCValue { quit: () },
            Action::NewWindow => ActionCValue { new_window: () },
            Action::NewTab => ActionCValue { new_tab: () },
            Action::CloseTab(v) => ActionCValue { close_tab: *v },
            Action::NewSplit(v) => ActionCValue { new_split: *v },
            Action::CloseAllWindows => ActionCValue {
                close_all_windows: (),
            },
            Action::ToggleMaximize => ActionCValue {
                toggle_maximize: (),
            },
            Action::ToggleFullscreen(v) => ActionCValue {
                toggle_fullscreen: *v,
            },
            Action::ToggleTabOverview => ActionCValue {
                toggle_tab_overview: (),
            },
            Action::ToggleWindowDecorations => ActionCValue {
                toggle_window_decorations: (),
            },
            Action::ToggleQuickTerminal => ActionCValue {
                toggle_quick_terminal: (),
            },
            Action::ToggleCommandPalette => ActionCValue {
                toggle_command_palette: (),
            },
            Action::ToggleVisibility => ActionCValue {
                toggle_visibility: (),
            },
            Action::ToggleBackgroundOpacity => ActionCValue {
                toggle_background_opacity: (),
            },
            Action::MoveTab(v) => ActionCValue { move_tab: *v },
            Action::GotoTab(v) => ActionCValue { goto_tab: *v },
            Action::GotoSplit(v) => ActionCValue { goto_split: *v },
            Action::GotoWindow(v) => ActionCValue { goto_window: *v },
            Action::ResizeSplit(v) => ActionCValue { resize_split: *v },
            Action::EqualizeSplits => ActionCValue {
                equalize_splits: (),
            },
            Action::ToggleSplitZoom => ActionCValue {
                toggle_split_zoom: (),
            },
            Action::PresentTerminal => ActionCValue {
                present_terminal: (),
            },
            Action::SizeLimit(v) => ActionCValue { size_limit: *v },
            Action::ResetWindowSize => ActionCValue {
                reset_window_size: (),
            },
            Action::InitialSize(v) => ActionCValue { initial_size: *v },
            Action::CellSize(v) => ActionCValue { cell_size: *v },
            Action::Scrollbar(v) => ActionCValue {
                scrollbar: v.cval(),
            },
            Action::Render => ActionCValue { render: () },
            Action::Inspector(v) => ActionCValue { inspector: *v },
            Action::ShowGtkInspector => ActionCValue {
                show_gtk_inspector: (),
            },
            Action::RenderInspector => ActionCValue {
                render_inspector: (),
            },
            Action::ExportTerminalIO(v) => ActionCValue {
                export_terminal_io: v.cval(),
            },
            Action::DesktopNotification(v) => ActionCValue {
                desktop_notification: v.cval(),
            },
            Action::SetTitle(v) => ActionCValue {
                set_title: v.cval(),
            },
            Action::SetTabTitle(v) => ActionCValue {
                set_tab_title: v.cval(),
            },
            Action::SetWindowTitle(v) => ActionCValue {
                set_window_title: v.cval(),
            },
            Action::PromptTitle(v) => ActionCValue { prompt_title: *v },
            Action::Pwd(v) => ActionCValue { pwd: v.cval() },
            Action::MouseShape(v) => ActionCValue { mouse_shape: *v },
            Action::MouseVisibility(v) => ActionCValue {
                mouse_visibility: *v,
            },
            Action::MouseOverLink(v) => ActionCValue {
                mouse_over_link: v.cval(),
            },
            Action::RendererHealth(v) => ActionCValue {
                renderer_health: *v,
            },
            Action::OpenConfig(v) => ActionCValue { open_config: *v },
            Action::QuitTimer(v) => ActionCValue { quit_timer: *v },
            Action::FloatWindow(v) => ActionCValue { float_window: *v },
            Action::SecureInput(v) => ActionCValue { secure_input: *v },
            Action::KeySequence(v) => ActionCValue {
                key_sequence: v.cval(),
            },
            Action::KeyTable(v) => ActionCValue {
                key_table: v.cval(),
            },
            Action::ColorChange(v) => ActionCValue { color_change: *v },
            Action::ReloadConfig(v) => ActionCValue { reload_config: *v },
            Action::ConfigChange(v) => ActionCValue {
                config_change: v.cval(),
            },
            Action::CloseWindow => ActionCValue { close_window: () },
            Action::RingBell => ActionCValue { ring_bell: () },
            Action::SelectionChanged => ActionCValue {
                selection_changed: (),
            },
            Action::Undo => ActionCValue { undo: () },
            Action::Redo => ActionCValue { redo: () },
            Action::CheckForUpdates => ActionCValue {
                check_for_updates: (),
            },
            Action::OpenUrl(v) => ActionCValue { open_url: v.cval() },
            Action::ShowChildExited(v) => ActionCValue {
                show_child_exited: *v,
            },
            Action::ProgressReport(v) => ActionCValue {
                progress_report: v.cval(),
            },
            Action::ShowOnScreenKeyboard => ActionCValue {
                show_on_screen_keyboard: (),
            },
            Action::CommandFinished(v) => ActionCValue {
                command_finished: v.cval(),
            },
            Action::StartSearch(v) => ActionCValue {
                start_search: v.cval(),
            },
            Action::EndSearch => ActionCValue { end_search: () },
            Action::SearchTotal(v) => ActionCValue {
                search_total: v.cval(),
            },
            Action::SearchSelected(v) => ActionCValue {
                search_selected: v.cval(),
            },
            Action::Readonly(v) => ActionCValue { readonly: *v },
            Action::CopyTitleToClipboard => ActionCValue {
                copy_title_to_clipboard: (),
            },
            Action::MoveTabToNewWindow => ActionCValue {
                move_tab_to_new_window: (),
            },
        };

        ActionC {
            key: self.key(),
            value,
        }
    }
}

// This is made extern (c_int) to make interop easier with our embedded
// runtime. The small size cost doesn't make a difference in our union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum SplitDirection {
    Right,
    Down,
    Left,
    Up,
}

// This is made extern (c_int) to make interop easier with our embedded
// runtime. The small size cost doesn't make a difference in our union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum GotoSplit {
    Previous,
    Next,

    Up,
    Left,
    Down,
    Right,
}

// This is made extern (c_int) to make interop easier with our embedded
// runtime. The small size cost doesn't make a difference in our union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum GotoWindow {
    Previous,
    Next,
}

/// The amount to resize the split by and the direction to resize it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ResizeSplit {
    pub amount: u16,
    pub direction: ResizeSplitDirection,
}

// PORT NOTE: reference `ResizeSplit.Direction`; hoisted to a sibling
// `ResizeSplitDirection` (Rust cannot nest a type inside a struct's value
// namespace). The `ghostty.h ResizeSplit.Direction` test lives in
// `action_tests.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ResizeSplitDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct MoveTab {
    pub amount: isize,
}

/// The tab to jump to. This is non-exhaustive so that integer values represent
/// the index (zero-based) of the tab to jump to. Negative values are special
/// values.
//
// PORT NOTE: reference `enum(c_int) { previous=-1, next=-2, last=-3, _ }` is a
// NON-EXHAUSTIVE enum (open over `c_int`). Rust enums cannot be open over
// arbitrary integers, so this is a `#[repr(transparent)]` newtype over `i32`
// with the special values as associated consts; any other value is a zero-based
// tab index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct GotoTab(pub i32);

impl GotoTab {
    pub const PREVIOUS: GotoTab = GotoTab(-1);
    pub const NEXT: GotoTab = GotoTab(-2);
    pub const LAST: GotoTab = GotoTab(-3);
    // 0+ values indicate the zero-based index of the tab to jump to.
}

// TODO: check non-exhaustive enums
// test "ghostty.h GotoTab" {
//     try lib.checkGhosttyHEnum(GotoTab, "GHOSTTY_GOTO_TAB_");
// }

/// The fullscreen mode to toggle to if we're moving to fullscreen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Fullscreen {
    Native,

    /// macOS has a non-native fullscreen mode that is more like a maximized
    /// window. This is much faster to enter and exit than the native mode.
    MacosNonNative,
    MacosNonNativeVisibleMenu,
    MacosNonNativePaddedNotch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FloatWindow {
    On,
    Off,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum SecureInput {
    On,
    Off,
    Toggle,
}

/// The inspector mode to toggle to if we're toggling the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Inspector {
    Toggle,
    Show,
    Hide,
}

/// Terminal IO inspector contents to export. The contents are only valid for
/// the duration of the action callback.
//
// PORT NOTE: reference `contents: []const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportTerminalIO<'a> {
    pub contents: &'a [u8],
}

// Sync with: rustty_action_export_terminal_io_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ExportTerminalIOC {
    pub contents: *const u8,
    pub len: usize,
}

impl<'a> ExportTerminalIO<'a> {
    pub fn cval(&self) -> ExportTerminalIOC {
        ExportTerminalIOC {
            contents: self.contents.as_ptr(),
            len: self.contents.len(),
        }
    }
}

impl core::fmt::Display for ExportTerminalIO<'_> {
    // PORT NOTE: reference `format` prints `"{s}{{ contents: {d} bytes }}"` of
    // `@typeName` + `contents.len` (the byte payload itself is never rendered).
    //
    // CORRECTION (task 20, S45-F2): this printed the bare `ExportTerminalIO`.
    // `@typeName(@This())` is the FULLY-QUALIFIED name, `apprt.action.ExportTerminalIO`
    // — measured with Zig 0.16.0 on a module rooted like the reference's
    // (`src/main.zig` importing `apprt.zig` importing `apprt/action.zig`). See
    // [`TYPE_NAME_PREFIX`].
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{TYPE_NAME_PREFIX}ExportTerminalIO{{ contents: {} bytes }}",
            self.contents.len()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum QuitTimer {
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Readonly {
    Off,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum MouseVisibility {
    Visible,
    Hidden,
}

/// Whether to prompt for the surface, tab, or window title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum PromptTitle {
    Surface,
    Tab,
    Window,
}

/// PORT NOTE: reference `url: [:0]const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseOverLink<'a> {
    pub url: &'a [u8],
}

// Sync with: rustty_action_mouse_over_link_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MouseOverLinkC {
    pub url: *const u8,
    pub len: usize,
}

impl<'a> MouseOverLink<'a> {
    pub fn cval(&self) -> MouseOverLinkC {
        MouseOverLinkC {
            url: self.url.as_ptr(),
            len: self.url.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SizeLimit {
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct InitialSize {
    pub width: u32,
    pub height: u32,
    // PORT NOTE: reference `getGObjectType` (`gobject.ext.defineBoxed(...,
    // "GhosttyApprtInitialSize")`, gated on `app_runtime == .gtk`) is deferred to
    // rustty-apprt's gtk backend; this crate defines the plain struct only.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

/// PORT NOTE: reference `title: [:0]const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetTitle<'a> {
    pub title: &'a [u8],
}

// Sync with: rustty_action_set_title_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SetTitleC {
    // PORT NOTE: reference `[*:0]const u8` (null-terminated). Modeled as
    // `*const u8`; the sentinel guarantee is part of the deferred C-ABI pinning.
    pub title: *const u8,
}

impl<'a> SetTitle<'a> {
    pub fn cval(&self) -> SetTitleC {
        SetTitleC {
            title: self.title.as_ptr(),
        }
    }
}

impl core::fmt::Display for SetTitle<'_> {
    // PORT NOTE: reference `format` prints `"{s}{{ {s} }}"` of `@typeName` +
    // `title`. The byte payload stays `&[u8]`; only this display path renders it
    // via `from_utf8_lossy` (display-only, not a data conversion).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // CORRECTION (task 20, S45-F2): printed the bare `SetTitle`; see
        // [`TYPE_NAME_PREFIX`].
        write!(
            f,
            "{TYPE_NAME_PREFIX}SetTitle{{ {} }}",
            String::from_utf8_lossy(self.title)
        )
    }
}

/// PORT NOTE: reference `pwd: [:0]const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pwd<'a> {
    pub pwd: &'a [u8],
}

// Sync with: rustty_action_set_pwd_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct PwdC {
    pub pwd: *const u8,
}

impl<'a> Pwd<'a> {
    pub fn cval(&self) -> PwdC {
        PwdC {
            pwd: self.pwd.as_ptr(),
        }
    }
}

impl core::fmt::Display for Pwd<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `try writer.print("{s}{{ {s} }}", .{ @typeName(@This()), value.pwd })`.
        // CORRECTION (task 20, S45-F2): printed the bare `Pwd`; see
        // [`TYPE_NAME_PREFIX`].
        write!(
            f,
            "{TYPE_NAME_PREFIX}Pwd{{ {} }}",
            String::from_utf8_lossy(self.pwd)
        )
    }
}

/// The desktop notification to show.
//
// PORT NOTE: reference `title`/`body: [:0]const u8` -> borrowed raw bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopNotification<'a> {
    pub title: &'a [u8],
    pub body: &'a [u8],
}

// Sync with: rustty_action_desktop_notification_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DesktopNotificationC {
    pub title: *const u8,
    pub body: *const u8,
}

impl<'a> DesktopNotification<'a> {
    pub fn cval(&self) -> DesktopNotificationC {
        DesktopNotificationC {
            title: self.title.as_ptr(),
            body: self.body.as_ptr(),
        }
    }
}

impl core::fmt::Display for DesktopNotification<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            // CORRECTION (task 20, S45-F2): printed the bare `DesktopNotification`;
            // see [`TYPE_NAME_PREFIX`].
            "{TYPE_NAME_PREFIX}DesktopNotification{{ title: {}, body: {} }}",
            String::from_utf8_lossy(self.title),
            String::from_utf8_lossy(self.body),
        )
    }
}

/// PORT NOTE: reference `KeySequence = union(enum) { trigger: input.Trigger,
/// end }`. `trigger` is `Copy`, so no lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySequence {
    Trigger(Trigger),
    End,
}

// Sync with: rustty_action_key_sequence_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeySequenceC {
    pub active: bool,
    pub trigger: TriggerC,
}

impl KeySequence {
    pub fn cval(&self) -> KeySequenceC {
        match self {
            KeySequence::Trigger(t) => KeySequenceC {
                active: true,
                trigger: t.cval(),
            },
            // PORT NOTE: reference `.end => .{ .active = false, .trigger = .{} }` —
            // `.{}` is a zero-initialized `extern struct`. `TriggerC` is a
            // repr(C) POD with no `Default`, so a zeroed value is the faithful
            // equivalent (the `active: false` flag makes the trigger inert).
            KeySequence::End => KeySequenceC {
                active: false,
                // SAFETY: `TriggerC` is a repr(C) POD (tag enum + union key +
                // bitflags mods); the all-zero bit pattern is a valid value.
                trigger: unsafe { core::mem::zeroed() },
            },
        }
    }
}

/// PORT NOTE: reference `activate: []const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTable<'a> {
    Activate(&'a [u8]),
    Deactivate,
    DeactivateAll,
}

// Sync with: rustty_action_key_table_tag_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum KeyTableTag {
    Activate,
    Deactivate,
    DeactivateAll,
}

// Sync with: rustty_action_key_table_u
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(C)]
pub union KeyTableCValue {
    pub activate: KeyTableActivateC,
}

// PORT NOTE: reference nests this anonymous `extern struct { name, len }` inside
// the `activate` union arm; hoisted to a named sibling.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeyTableActivateC {
    pub name: *const u8,
    pub len: usize,
}

// Sync with: rustty_action_key_table_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeyTableC {
    pub tag: KeyTableTag,
    pub value: KeyTableCValue,
}

impl<'a> KeyTable<'a> {
    pub fn cval(&self) -> KeyTableC {
        match self {
            KeyTable::Activate(name) => KeyTableC {
                tag: KeyTableTag::Activate,
                value: KeyTableCValue {
                    activate: KeyTableActivateC {
                        name: name.as_ptr(),
                        len: name.len(),
                    },
                },
            },
            // PORT NOTE: reference uses `.value = undefined` for the payload-less
            // arms; Rust has no `undefined`, so a zeroed `activate` arm stands in.
            KeyTable::Deactivate => KeyTableC {
                tag: KeyTableTag::Deactivate,
                value: KeyTableCValue {
                    activate: KeyTableActivateC {
                        name: core::ptr::null(),
                        len: 0,
                    },
                },
            },
            KeyTable::DeactivateAll => KeyTableC {
                tag: KeyTableTag::DeactivateAll,
                value: KeyTableCValue {
                    activate: KeyTableActivateC {
                        name: core::ptr::null(),
                        len: 0,
                    },
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ColorChange {
    pub kind: ColorKind,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// PORT NOTE: reference `enum(c_int) { foreground=-1, background=-2, cursor=-3, _ }`
// is NON-EXHAUSTIVE (open over `c_int`, 0+ = palette index). Modeled as a
// `#[repr(transparent)]` newtype over `i32` with the named kinds as consts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorKind(pub i32);

impl ColorKind {
    // Negative numbers indicate some named kind
    pub const FOREGROUND: ColorKind = ColorKind(-1);
    pub const BACKGROUND: ColorKind = ColorKind(-2);
    pub const CURSOR: ColorKind = ColorKind(-3);

    // 0+ values indicate a palette index
}

// TODO: check non-non-exhaustive enums
// test "ghostty.h ColorKind" {
//     try lib.checkGhosttyHEnum(ColorKind, "GHOSTTY_COLOR_KIND_");
// }

// PORT NOTE: reference field default `soft: bool = false` -> `#[derive(Default)]`
// (Rust `bool::default()` is `false`), so `ReloadConfig::default().soft == false`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(C)]
pub struct ReloadConfig {
    /// A soft reload means that the configuration doesn't need to be
    /// read off disk, but librustty needs the full config again so call
    /// updateConfig with it.
    pub soft: bool,
}

/// PORT NOTE: reference `config: *const configpkg.Config` -> borrowed `&'a Config`.
#[derive(Clone, Copy)]
pub struct ConfigChange<'a> {
    pub config: &'a Config,
}

// Sync with: rustty_action_config_change_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ConfigChangeC {
    pub config: *const Config,
}

impl<'a> ConfigChange<'a> {
    pub fn cval(&self) -> ConfigChangeC {
        ConfigChangeC {
            config: self.config as *const Config,
        }
    }
}

/// Open a URL
//
// PORT NOTE: reference `url: []const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenUrl<'a> {
    /// The type of data that the URL refers to.
    pub kind: OpenUrlKind,

    /// The URL.
    pub url: &'a [u8],
}

/// The type of the data at the URL to open. This is used as a hint to
/// potentially open the URL in a different way.
///
/// Sync with: rustty_action_open_url_kind_e
//
// PORT NOTE: reference `OpenUrl.Kind`; hoisted to a sibling `OpenUrlKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum OpenUrlKind {
    /// The type is unknown. This is the default and apprts should
    /// open the URL in the most generic way possible. For example,
    /// on macOS this would be the equivalent of `open` or on Linux
    /// this would be `xdg-open`.
    Unknown,

    /// The URL is known to be a text file. In this case, the apprt
    /// should try to open the URL in a text editor or viewer or
    /// some equivalent, if possible.
    Text,

    /// The URL is known to contain HTML content.
    Html,

    /// The URL came from an OSC 8 hyperlink. Application runtimes should
    /// treat this as untrusted terminal output and apply a platform-specific
    /// safe-opening policy.
    Osc8,
}

// Sync with: rustty_action_open_url_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct OpenUrlC {
    pub kind: OpenUrlKind,
    pub url: *const u8,
    pub len: usize,
}

impl<'a> OpenUrl<'a> {
    pub fn cval(&self) -> OpenUrlC {
        OpenUrlC {
            kind: self.kind,
            url: self.url.as_ptr(),
            len: self.url.len(),
        }
    }
}

/// sync with rustty_action_close_tab_mode_e in rustty.h
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CloseTabMode {
    /// Close the current tab.
    This,
    /// Close all other tabs.
    Other,
    /// Close all tabs to the right of the current tab.
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandFinished {
    pub exit_code: Option<u8>,
    pub duration: Duration,
}

// sync with rustty_action_command_finished_s in rustty.h
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CommandFinishedC {
    pub exit_code: i16,
    pub duration: u64,
}

impl CommandFinished {
    pub fn cval(&self) -> CommandFinishedC {
        CommandFinishedC {
            // PORT NOTE: reference `self.exit_code orelse -1`.
            exit_code: match self.exit_code {
                Some(c) => c as i16,
                None => -1,
            },
            duration: self.duration.duration,
        }
    }
}

/// PORT NOTE: reference `needle: [:0]const u8` -> borrowed raw bytes `&'a [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartSearch<'a> {
    pub needle: &'a [u8],
}

// Sync with: rustty_action_start_search_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct StartSearchC {
    pub needle: *const u8,
}

impl<'a> StartSearch<'a> {
    pub fn cval(&self) -> StartSearchC {
        StartSearchC {
            needle: self.needle.as_ptr(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchTotal {
    pub total: Option<usize>,
}

// Sync with: rustty_action_search_total_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SearchTotalC {
    pub total: isize,
}

impl SearchTotal {
    pub fn cval(&self) -> SearchTotalC {
        SearchTotalC {
            // PORT NOTE: reference `if (self.total) |t| @intCast(t) else -1`.
            total: match self.total {
                Some(t) => t as isize,
                None => -1,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchSelected {
    pub selected: Option<usize>,
}

// Sync with: rustty_action_search_selected_s
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SearchSelectedC {
    pub selected: isize,
}

impl SearchSelected {
    pub fn cval(&self) -> SearchSelectedC {
        SearchSelectedC {
            selected: match self.selected {
                Some(s) => s as isize,
                None => -1,
            },
        }
    }
}

/// Sync with: rustty_action_open_config_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum OpenConfig {
    /// Open the config in the OS default editor.
    OsOpen,

    /// Open the config in a new window using $EDITOR or $VISUAL
    NewWindow,
}
