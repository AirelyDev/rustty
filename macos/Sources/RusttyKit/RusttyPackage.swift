import os
import SwiftUI
import RusttyKit

// MARK: C Extensions

/// A command is fully self-contained so it is Sendable.
extension rustty_command_s: @unchecked @retroactive Sendable {}

/// A surface is sendable because it is just a reference type. Using the surface in parameters
/// may be unsafe but the value itself is safe to send across threads.
extension rustty_surface_t: @unchecked @retroactive Sendable {}

extension Rustty {
    // The user notification category identifier
    static let userNotificationCategory = "io.airely.rustty.userNotification"

    // The user notification "Show" action
    static let userNotificationActionShow = "io.airely.rustty.userNotification.Show"
}

// MARK: Build Info

extension Rustty {
    struct Info {
        var mode: rustty_build_mode_e
        var version: String
    }

    static var info: Info {
        let raw = rustty_info()
        let version = NSString(
            bytes: raw.version,
            length: Int(raw.version_len),
            encoding: NSUTF8StringEncoding
        ) ?? "unknown"

        return Info(mode: raw.build_mode, version: String(version))
    }
}

// MARK: General Helpers

extension Rustty {
    enum LaunchSource: String {
        case cli
        case app
        case zig_run
    }

    /// Returns the mechanism that launched the app. This is based on an env var so
    /// its up to the env var being set in the correct circumstance.
    static var launchSource: LaunchSource {
        guard let envValue = ProcessInfo.processInfo.environment["RUSTTY_MAC_LAUNCH_SOURCE"] else {
            // We default to the CLI because the app bundle always sets the
            // source. If its unset we assume we're in a CLI environment.
            return .cli
        }

        // If the env var is set but its unknown then we default back to the app.
        return LaunchSource(rawValue: envValue) ?? .app
    }
}

// MARK: Swift Types for C Types

extension Rustty {
    class AllocatedString {
        private let cString: rustty_string_s

        init(_ c: rustty_string_s) {
            self.cString = c
        }

        var string: String {
            guard let ptr = cString.ptr else { return "" }
            let data = Data(bytes: ptr, count: Int(cString.len))
            return String(data: data, encoding: .utf8) ?? ""
        }

        deinit {
            rustty_string_free(cString)
        }
    }
}

extension Rustty {
    enum SetFloatWIndow {
        case on
        case off
        case toggle

        static func from(_ c: rustty_action_float_window_e) -> Self? {
            switch c {
            case RUSTTY_FLOAT_WINDOW_ON:
                return .on

            case RUSTTY_FLOAT_WINDOW_OFF:
                return .off

            case RUSTTY_FLOAT_WINDOW_TOGGLE:
                return .toggle

            default:
                return nil
            }
        }
    }

    enum SetSecureInput {
        case on
        case off
        case toggle

        static func from(_ c: rustty_action_secure_input_e) -> Self? {
            switch c {
            case RUSTTY_SECURE_INPUT_ON:
                return .on

            case RUSTTY_SECURE_INPUT_OFF:
                return .off

            case RUSTTY_SECURE_INPUT_TOGGLE:
                return .toggle

            default:
                return nil
            }
        }
    }

    /// An enum that is used for the directions that a split focus event can change.
    enum SplitFocusDirection {
        case previous, next, up, down, left, right

        /// Initialize from a Rustty API enum.
        static func from(direction: rustty_action_goto_split_e) -> Self? {
            switch direction {
            case RUSTTY_GOTO_SPLIT_PREVIOUS:
                return .previous

            case RUSTTY_GOTO_SPLIT_NEXT:
                return .next

            case RUSTTY_GOTO_SPLIT_UP:
                return .up

            case RUSTTY_GOTO_SPLIT_DOWN:
                return .down

            case RUSTTY_GOTO_SPLIT_LEFT:
                return .left

            case RUSTTY_GOTO_SPLIT_RIGHT:
                return .right

            default:
                return nil
            }
        }

        func toNative() -> rustty_action_goto_split_e {
            switch self {
            case .previous:
                return RUSTTY_GOTO_SPLIT_PREVIOUS

            case .next:
                return RUSTTY_GOTO_SPLIT_NEXT

            case .up:
                return RUSTTY_GOTO_SPLIT_UP

            case .down:
                return RUSTTY_GOTO_SPLIT_DOWN

            case .left:
                return RUSTTY_GOTO_SPLIT_LEFT

            case .right:
                return RUSTTY_GOTO_SPLIT_RIGHT
            }
        }
    }

    /// Enum used for resizing splits. This is the direction the split divider will move.
    enum SplitResizeDirection {
        case up, down, left, right

        static func from(direction: rustty_action_resize_split_direction_e) -> Self? {
            switch direction {
            case RUSTTY_RESIZE_SPLIT_UP:
                return .up
            case RUSTTY_RESIZE_SPLIT_DOWN:
                return .down
            case RUSTTY_RESIZE_SPLIT_LEFT:
                return .left
            case RUSTTY_RESIZE_SPLIT_RIGHT:
                return .right
            default:
                return nil
            }
        }

        func toNative() -> rustty_action_resize_split_direction_e {
            switch self {
            case .up:
                return RUSTTY_RESIZE_SPLIT_UP
            case .down:
                return RUSTTY_RESIZE_SPLIT_DOWN
            case .left:
                return RUSTTY_RESIZE_SPLIT_LEFT
            case .right:
                return RUSTTY_RESIZE_SPLIT_RIGHT
            }
        }
    }
}

#if canImport(AppKit)
// MARK: SplitFocusDirection Extensions

extension Rustty.SplitFocusDirection {
    /// Convert to a SplitTree.FocusDirection for the given ViewType.
    func toSplitTreeFocusDirection<ViewType>() -> SplitTree<ViewType>.FocusDirection {
        switch self {
        case .previous:
            return .previous

        case .next:
            return .next

        case .up:
            return .spatial(.up)

        case .down:
            return .spatial(.down)

        case .left:
            return .spatial(.left)

        case .right:
            return .spatial(.right)
        }
    }
}
#endif

extension Rustty {
    /// The type of a clipboard request
    enum ClipboardRequest {
        /// A direct paste of clipboard contents
        case paste

        /// An application is attempting to read from the clipboard using OSC 52
        case osc_52_read

        /// An application is attempting to write to the clipboard using OSC 52
        case osc_52_write(OSPasteboard?)

        /// The text to show in the clipboard confirmation prompt for a given request type
        func text() -> String {
            switch self {
            case .paste:
                return """
                Pasting this text to the terminal may be dangerous as it looks like some commands may be executed.
                """
            case .osc_52_read:
                return """
                An application is attempting to read from the clipboard.
                The current clipboard contents are shown below.
                """
            case .osc_52_write:
                return """
                An application is attempting to write to the clipboard.
                The content to write is shown below.
                """
            }
        }

        static func from(request: rustty_clipboard_request_e) -> ClipboardRequest? {
            switch request {
            case RUSTTY_CLIPBOARD_REQUEST_PASTE:
                return .paste
            case RUSTTY_CLIPBOARD_REQUEST_OSC_52_READ:
                return .osc_52_read
            case RUSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE:
                return .osc_52_write(nil)
            default:
                return nil
            }
        }
    }

    struct ClipboardContent {
        let mime: String
        let data: String

        static func from(content: rustty_clipboard_content_s) -> ClipboardContent? {
            guard let mimePtr = content.mime,
                  let dataPtr = content.data else {
                return nil
            }

            return ClipboardContent(
                mime: String(cString: mimePtr),
                data: String(cString: dataPtr)
            )
        }
    }

    /// Enum for the macos-window-buttons config option
    enum MacOSWindowButtons: String {
        case visible
        case hidden
    }

    /// Enum for the macos-titlebar-proxy-icon config option
    enum MacOSTitlebarProxyIcon: String {
        case visible
        case hidden
    }

    /// Enum for auto-update-channel config option
    enum AutoUpdateChannel: String {
        case tip
        case stable
    }
}

// MARK: Surface Notification

extension Notification.Name {
    /// Configuration change. If the object is nil then it is app-wide. Otherwise its surface-specific.
    static let rusttyConfigDidChange = Notification.Name("io.airely.rustty.configDidChange")
    static let RusttyConfigChangeKey = rusttyConfigDidChange.rawValue

    /// Color change. Object is the surface changing.
    static let rusttyColorDidChange = Notification.Name("io.airely.rustty.rusttyColorDidChange")
    static let RusttyColorChangeKey = rusttyColorDidChange.rawValue

    /// Goto tab. Has tab index in the userinfo.
    static let rusttyMoveTab = Notification.Name("io.airely.rustty.moveTab")
    static let RusttyMoveTabKey = rusttyMoveTab.rawValue

    /// Close tab
    static let rusttyCloseTab = Notification.Name("io.airely.rustty.closeTab")

    /// Close other tabs
    static let rusttyCloseOtherTabs = Notification.Name("io.airely.rustty.closeOtherTabs")

    /// Close tabs to the right of the focused tab
    static let rusttyCloseTabsOnTheRight = Notification.Name("io.airely.rustty.closeTabsOnTheRight")

    /// Close window
    static let rusttyCloseWindow = Notification.Name("io.airely.rustty.closeWindow")

    /// Resize the window to a default size.
    static let rusttyResetWindowSize = Notification.Name("io.airely.rustty.resetWindowSize")

    /// Ring the bell
    static let rusttyBellDidRing = Notification.Name("io.airely.rustty.rusttyBellDidRing")

    /// The active selection changed
    static let rusttySelectionDidChange = Notification.Name("io.airely.rustty.rusttySelectionDidChange")

    /// Readonly mode changed
    static let rusttyDidChangeReadonly = Notification.Name("io.airely.rustty.didChangeReadonly")
    static let ReadonlyKey = rusttyDidChangeReadonly.rawValue + ".readonly"
    static let rusttyCommandPaletteDidToggle = Notification.Name("io.airely.rustty.commandPaletteDidToggle")

    /// Toggle maximize of current window
    static let rusttyMaximizeDidToggle = Notification.Name("io.airely.rustty.maximizeDidToggle")

    /// Notification sent when scrollbar updates
    static let rusttyDidUpdateScrollbar = Notification.Name("io.airely.rustty.didUpdateScrollbar")
    static let ScrollbarKey = rusttyDidUpdateScrollbar.rawValue + ".scrollbar"

    /// Focus the search field
    static let rusttySearchFocus = Notification.Name("io.airely.rustty.searchFocus")
}

// NOTE: I am moving all of these to Notification.Name extensions over time. This
// namespace was the old namespace.
extension Rustty.Notification {
    /// Used to pass a configuration along when creating a new tab/window/split.
    static let NewSurfaceConfigKey = "io.airely.rustty.newSurfaceConfig"

    /// Posted when a new split is requested. The sending object will be the surface that had focus. The
    /// userdata has one key "direction" with the direction to split to.
    static let rusttyNewSplit = Notification.Name("io.airely.rustty.newSplit")

    /// Close the calling surface.
    static let rusttyCloseSurface = Notification.Name("io.airely.rustty.closeSurface")

    /// Focus previous/next split. Has a SplitFocusDirection in the userinfo.
    static let rusttyFocusSplit = Notification.Name("io.airely.rustty.focusSplit")
    static let SplitDirectionKey = rusttyFocusSplit.rawValue

    /// Goto tab. Has tab index in the userinfo.
    static let rusttyGotoTab = Notification.Name("io.airely.rustty.gotoTab")
    static let GotoTabKey = rusttyGotoTab.rawValue

    /// New tab. Has base surface config requested in userinfo.
    static let rusttyNewTab = Notification.Name("io.airely.rustty.newTab")

    /// New window. Has base surface config requested in userinfo.
    static let rusttyNewWindow = Notification.Name("io.airely.rustty.newWindow")

    /// Present terminal. Bring the surface's window to focus without activating the app.
    static let rusttyPresentTerminal = Notification.Name("io.airely.rustty.presentTerminal")

    /// Toggle fullscreen of current window
    static let rusttyToggleFullscreen = Notification.Name("io.airely.rustty.toggleFullscreen")
    static let FullscreenModeKey = rusttyToggleFullscreen.rawValue

    /// Notification sent to toggle split maximize/unmaximize.
    static let didToggleSplitZoom = Notification.Name("io.airely.rustty.didToggleSplitZoom")

    /// Notification
    static let didReceiveInitialWindowFrame = Notification.Name("io.airely.rustty.didReceiveInitialWindowFrame")
    static let FrameKey = "io.airely.rustty.frame"

    /// Notification to render the inspector for a surface
    static let inspectorNeedsDisplay = Notification.Name("io.airely.rustty.inspectorNeedsDisplay")

    /// Notification to show/hide the inspector
    static let didControlInspector = Notification.Name("io.airely.rustty.didControlInspector")

    static let confirmClipboard = Notification.Name("io.airely.rustty.confirmClipboard")
    static let ConfirmClipboardStrKey = confirmClipboard.rawValue + ".str"
    static let ConfirmClipboardStateKey = confirmClipboard.rawValue + ".state"
    static let ConfirmClipboardRequestKey = confirmClipboard.rawValue + ".request"

    /// Notification sent to the active split view to resize the split.
    static let didResizeSplit = Notification.Name("io.airely.rustty.didResizeSplit")
    static let ResizeSplitDirectionKey = didResizeSplit.rawValue + ".direction"
    static let ResizeSplitAmountKey = didResizeSplit.rawValue + ".amount"

    /// Notification sent to the split root to equalize split sizes
    static let didEqualizeSplits = Notification.Name("io.airely.rustty.didEqualizeSplits")

    /// Notification that renderer health changed
    static let didUpdateRendererHealth = Notification.Name("io.airely.rustty.didUpdateRendererHealth")

    /// Notifications related to key sequences
    static let didContinueKeySequence = Notification.Name("io.airely.rustty.didContinueKeySequence")
    static let didEndKeySequence = Notification.Name("io.airely.rustty.didEndKeySequence")
    static let KeySequenceKey = didContinueKeySequence.rawValue + ".key"

    /// Notifications related to key tables
    static let didChangeKeyTable = Notification.Name("io.airely.rustty.didChangeKeyTable")
    static let KeyTableKey = didChangeKeyTable.rawValue + ".action"
}

// Make the input enum hashable.
extension rustty_input_key_e: @retroactive Hashable {}
