import AppIntents
import Cocoa
import SwiftUI
import RusttyKit

extension Rustty {
    struct Input {}

    // MARK: Keyboard Shortcuts

    /// Return the key equivalent for the given trigger.
    ///
    /// Returns nil if the trigger doesn't have an equivalent KeyboardShortcut. This is possible
    /// because Rustty input triggers are a superset of what can be represented by a macOS
    /// KeyboardShortcut. For example, macOS doesn't have any way to represent function keys
    /// (F1, F2, ...) with a KeyboardShortcut. This doesn't represent a practical issue because input
    /// handling for Rustty is handled at a lower level (usually). This function should generally only
    /// be used for things like NSMenu that only support keyboard shortcuts anyways.
    @MainActor static func keyboardShortcut(for trigger: rustty_input_trigger_s) -> KeyboardShortcut? {
        let modifierFlags = Self.eventModifierFlags(mods: trigger.mods)
        let key: KeyEquivalent
        switch trigger.tag {
        case RUSTTY_TRIGGER_PHYSICAL:
            let physical = trigger.key.physical
            if let equivalent = Self.keyToEquivalent[physical] {
                key = equivalent
            } else {
                guard
                    Self.writingSystemKeyRange.contains(physical.rawValue),
                    let inputKey = Input.Key(cKey: physical),
                    let keyCode = inputKey.keyCode,
                    // Command can select a distinct layout table. Other modifiers remain
                    // separate in the menu's modifier mask and must not affect this character.
                    let character = KeyboardLayout.character(
                        for: keyCode,
                        modifiers: modifierFlags.intersection(.command))
                else { return nil }

                // Printable physical keys must be translated through the current layout.
                key = KeyEquivalent(character)
            }

        case RUSTTY_TRIGGER_UNICODE:
            guard
                let scalar = UnicodeScalar(trigger.key.unicode),
                let normalized = Character(scalar).lowercased().first
            else { return nil }
            key = KeyEquivalent(normalized)

        case RUSTTY_TRIGGER_CATCH_ALL:
            // catch_all matches any key, so it can't be represented as a KeyboardShortcut
            return nil

        default:
            return nil
        }

        return KeyboardShortcut(
            key,
            modifiers: EventModifiers(nsFlags: modifierFlags))
    }

    // MARK: Mods

    /// Returns the event modifier flags set for the Rustty mods enum.
    static func eventModifierFlags(mods: rustty_input_mods_e) -> NSEvent.ModifierFlags {
        var flags = NSEvent.ModifierFlags(rawValue: 0)
        if mods.rawValue & RUSTTY_MODS_SHIFT.rawValue != 0 { flags.insert(.shift) }
        if mods.rawValue & RUSTTY_MODS_CTRL.rawValue != 0 { flags.insert(.control) }
        if mods.rawValue & RUSTTY_MODS_ALT.rawValue != 0 { flags.insert(.option) }
        if mods.rawValue & RUSTTY_MODS_SUPER.rawValue != 0 { flags.insert(.command) }
        return flags
    }

    /// Translate event modifier flags to a rustty mods enum.
    static func rusttyMods(_ flags: NSEvent.ModifierFlags) -> rustty_input_mods_e {
        var mods: UInt32 = RUSTTY_MODS_NONE.rawValue

        if flags.contains(.shift) { mods |= RUSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control) { mods |= RUSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option) { mods |= RUSTTY_MODS_ALT.rawValue }
        if flags.contains(.command) { mods |= RUSTTY_MODS_SUPER.rawValue }
        if flags.contains(.capsLock) { mods |= RUSTTY_MODS_CAPS.rawValue }

        // Handle sided input. We can't tell that both are pressed in the
        // Rustty structure but that's okay -- we don't use that information.
        let rawFlags = flags.rawValue
        if rawFlags & UInt(NX_DEVICERSHIFTKEYMASK) != 0 { mods |= RUSTTY_MODS_SHIFT_RIGHT.rawValue }
        if rawFlags & UInt(NX_DEVICERCTLKEYMASK) != 0 { mods |= RUSTTY_MODS_CTRL_RIGHT.rawValue }
        if rawFlags & UInt(NX_DEVICERALTKEYMASK) != 0 { mods |= RUSTTY_MODS_ALT_RIGHT.rawValue }
        if rawFlags & UInt(NX_DEVICERCMDKEYMASK) != 0 { mods |= RUSTTY_MODS_SUPER_RIGHT.rawValue }

        return rustty_input_mods_e(mods)
    }

    /// A map from the Rustty key enum to the keyEquivalent string for shortcuts. Note that
    /// not all rustty key enum values are represented here because not all of them can be
    /// mapped to a KeyEquivalent.
    static let keyToEquivalent: [rustty_input_key_e: KeyEquivalent] = [
        // Function keys
        RUSTTY_KEY_ARROW_UP: .upArrow,
        RUSTTY_KEY_ARROW_DOWN: .downArrow,
        RUSTTY_KEY_ARROW_LEFT: .leftArrow,
        RUSTTY_KEY_ARROW_RIGHT: .rightArrow,
        RUSTTY_KEY_HOME: .home,
        RUSTTY_KEY_END: .end,
        RUSTTY_KEY_DELETE: .deleteForward,
        RUSTTY_KEY_PAGE_UP: .pageUp,
        RUSTTY_KEY_PAGE_DOWN: .pageDown,
        RUSTTY_KEY_ESCAPE: .escape,
        RUSTTY_KEY_ENTER: .return,
        RUSTTY_KEY_TAB: .tab,
        RUSTTY_KEY_BACKSPACE: .delete,
        RUSTTY_KEY_SPACE: .space,
    ]

    /// The contiguous W3C "Writing System Keys" § 3.1.1 key range.
    private static let writingSystemKeyRange =
        RUSTTY_KEY_BACKQUOTE.rawValue...RUSTTY_KEY_SLASH.rawValue
}

// MARK: Rustty.Input.BindingFlags

extension Rustty.Input {
    /// `rustty_binding_flags_e`
    struct BindingFlags: OptionSet, Sendable {
        let rawValue: UInt32

        static let consumed = BindingFlags(rawValue: RUSTTY_BINDING_FLAGS_CONSUMED.rawValue)
        static let all = BindingFlags(rawValue: RUSTTY_BINDING_FLAGS_ALL.rawValue)
        static let global = BindingFlags(rawValue: RUSTTY_BINDING_FLAGS_GLOBAL.rawValue)
        static let performable = BindingFlags(rawValue: RUSTTY_BINDING_FLAGS_PERFORMABLE.rawValue)

        init(rawValue: UInt32) {
            self.rawValue = rawValue
        }

        init(cFlags: rustty_binding_flags_e) {
            self.rawValue = cFlags.rawValue
        }

        var cFlags: rustty_binding_flags_e {
            rustty_binding_flags_e(rawValue)
        }
    }
}

// MARK: Rustty.Input.KeyEvent

extension Rustty.Input {
    /// `rustty_input_key_s`
    struct KeyEvent {
        let action: Action
        let key: Key
        let text: String?
        let composing: Bool
        let mods: Mods
        let consumedMods: Mods
        let unshiftedCodepoint: UInt32

        init(
            key: Key,
            action: Action = .press,
            text: String? = nil,
            composing: Bool = false,
            mods: Mods = [],
            consumedMods: Mods = [],
            unshiftedCodepoint: UInt32 = 0
        ) {
            self.key = key
            self.action = action
            self.text = text
            self.composing = composing
            self.mods = mods
            self.consumedMods = consumedMods
            self.unshiftedCodepoint = unshiftedCodepoint
        }

        init?(cValue: rustty_input_key_s) {
            // Convert action
            switch cValue.action {
            case RUSTTY_ACTION_PRESS: self.action = .press
            case RUSTTY_ACTION_RELEASE: self.action = .release
            case RUSTTY_ACTION_REPEAT: self.action = .repeat
            default: self.action = .press
            }

            // Convert key from keycode
            guard let key = Key(keyCode: UInt16(cValue.keycode)) else { return nil }
            self.key = key

            // Convert text
            if let textPtr = cValue.text {
                self.text = String(cString: textPtr)
            } else {
                self.text = nil
            }

            // Set composing state
            self.composing = cValue.composing

            // Convert modifiers
            self.mods = Mods(cMods: cValue.mods)
            self.consumedMods = Mods(cMods: cValue.consumed_mods)

            // Set unshifted codepoint
            self.unshiftedCodepoint = cValue.unshifted_codepoint
        }

        /// Executes a closure with a temporary C representation of this KeyEvent.
        ///
        /// This method safely converts the Swift KeyEntity to a C `rustty_input_key_s` struct
        /// and passes it to the provided closure. The C struct is only valid within the closure's
        /// execution scope. The text field's C string pointer is managed automatically and will
        /// be invalid after the closure returns.
        ///
        /// - Parameter execute: A closure that receives the C struct and returns a value
        /// - Returns: The value returned by the closure
        @discardableResult
        func withCValue<T>(execute: (rustty_input_key_s) -> T) -> T {
            var keyEvent = rustty_input_key_s()
            keyEvent.action = action.cAction
            keyEvent.keycode = UInt32(key.keyCode ?? 0)
            keyEvent.composing = composing
            keyEvent.mods = mods.cMods
            keyEvent.consumed_mods = consumedMods.cMods
            keyEvent.unshifted_codepoint = unshiftedCodepoint

            // Handle text with proper memory management
            if let text = text {
                return text.withCString { textPtr in
                    keyEvent.text = textPtr
                    return execute(keyEvent)
                }
            } else {
                keyEvent.text = nil
                return execute(keyEvent)
            }
        }
    }
}

// MARK: Rustty.Input.Action

extension Rustty.Input {
    /// `rustty_input_action_e`
    enum Action: String, CaseIterable {
        case release
        case press
        case `repeat`

        var cAction: rustty_input_action_e {
            switch self {
            case .release: RUSTTY_ACTION_RELEASE
            case .press: RUSTTY_ACTION_PRESS
            case .repeat: RUSTTY_ACTION_REPEAT
            }
        }
    }
}

extension Rustty.Input.Action: AppEnum {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Key Action")

    static var caseDisplayRepresentations: [Rustty.Input.Action: DisplayRepresentation] = [
        .release: "Release",
        .press: "Press",
        .repeat: "Repeat"
    ]
}

// MARK: Rustty.Input.MouseEvent

extension Rustty.Input {
    /// Represents a mouse input event with button state, button type, and modifier keys.
    struct MouseButtonEvent {
        let action: MouseState
        let button: MouseButton
        let mods: Mods

        init(
            action: MouseState,
            button: MouseButton,
            mods: Mods = []
        ) {
            self.action = action
            self.button = button
            self.mods = mods
        }

        /// Creates a MouseEvent from C enum values.
        ///
        /// This initializer converts C-style mouse input enums to Swift types.
        /// Returns nil if any of the C enum values are invalid or unsupported.
        ///
        /// - Parameters:
        ///   - state: The mouse button state (press/release)
        ///   - button: The mouse button that was pressed/released
        ///   - mods: The modifier keys held during the mouse event
        init?(state: rustty_input_mouse_state_e, button: rustty_input_mouse_button_e, mods: rustty_input_mods_e) {
            // Convert state
            switch state {
            case RUSTTY_MOUSE_RELEASE: self.action = .release
            case RUSTTY_MOUSE_PRESS: self.action = .press
            default: return nil
            }

            // Convert button
            switch button {
            case RUSTTY_MOUSE_UNKNOWN: self.button = .unknown
            case RUSTTY_MOUSE_LEFT: self.button = .left
            case RUSTTY_MOUSE_RIGHT: self.button = .right
            case RUSTTY_MOUSE_MIDDLE: self.button = .middle
            default: return nil
            }

            // Convert modifiers
            self.mods = Mods(cMods: mods)
        }
    }

    /// Represents a mouse position/movement event with coordinates and modifier keys.
    struct MousePosEvent {
        let x: Double
        let y: Double
        let mods: Mods

        init(
            x: Double,
            y: Double,
            mods: Mods = []
        ) {
            self.x = x
            self.y = y
            self.mods = mods
        }
    }

    /// Represents a mouse scroll event with scroll deltas and modifier keys.
    struct MouseScrollEvent {
        let x: Double
        let y: Double
        let mods: ScrollMods

        init(
            x: Double,
            y: Double,
            mods: ScrollMods = .init(rawValue: 0)
        ) {
            self.x = x
            self.y = y
            self.mods = mods
        }
    }
}

// MARK: Rustty.Input.MouseState

extension Rustty.Input {
    /// `rustty_input_mouse_state_e`
    enum MouseState: String, CaseIterable {
        case release
        case press

        var cMouseState: rustty_input_mouse_state_e {
            switch self {
            case .release: RUSTTY_MOUSE_RELEASE
            case .press: RUSTTY_MOUSE_PRESS
            }
        }
    }
}

extension Rustty.Input.MouseState: AppEnum {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Mouse State")

    static var caseDisplayRepresentations: [Rustty.Input.MouseState: DisplayRepresentation] = [
        .release: "Release",
        .press: "Press"
    ]
}

// MARK: Rustty.Input.MouseButton

extension Rustty.Input {
    /// `rustty_input_mouse_button_e`
    enum MouseButton: String, CaseIterable {
        case unknown
        case left
        case right
        case middle
        case four
        case five
        case six
        case seven
        case eight
        case nine
        case ten
        case eleven

        var cMouseButton: rustty_input_mouse_button_e {
            switch self {
            case .unknown: RUSTTY_MOUSE_UNKNOWN
            case .left: RUSTTY_MOUSE_LEFT
            case .right: RUSTTY_MOUSE_RIGHT
            case .middle: RUSTTY_MOUSE_MIDDLE
            case .four: RUSTTY_MOUSE_FOUR
            case .five: RUSTTY_MOUSE_FIVE
            case .six: RUSTTY_MOUSE_SIX
            case .seven: RUSTTY_MOUSE_SEVEN
            case .eight: RUSTTY_MOUSE_EIGHT
            case .nine: RUSTTY_MOUSE_NINE
            case .ten: RUSTTY_MOUSE_TEN
            case .eleven: RUSTTY_MOUSE_ELEVEN
            }
        }

        /// Initialize from NSEvent.buttonNumber
        /// NSEvent buttonNumber: 0=left, 1=right, 2=middle, 3=back (button 8), 4=forward (button 9), etc.
        init(fromNSEventButtonNumber buttonNumber: Int) {
            switch buttonNumber {
            case 0: self = .left
            case 1: self = .right
            case 2: self = .middle
            case 3: self = .eight   // Back button
            case 4: self = .nine    // Forward button
            case 5: self = .six
            case 6: self = .seven
            case 7: self = .four
            case 8: self = .five
            case 9: self = .ten
            case 10: self = .eleven
            default: self = .unknown
            }
        }
    }
}

extension Rustty.Input.MouseButton: AppEnum {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Mouse Button")

    static var caseDisplayRepresentations: [Rustty.Input.MouseButton: DisplayRepresentation] = [
        .unknown: "Unknown",
        .left: "Left",
        .right: "Right",
        .middle: "Middle"
    ]

    static var allCases: [Rustty.Input.MouseButton] = [
        .left,
        .right,
        .middle,
    ]
}

// MARK: Rustty.Input.ScrollMods

extension Rustty.Input {
    /// `rustty_input_scroll_mods_t` - Scroll event modifiers
    ///
    /// This is a packed bitmask that contains precision and momentum information
    /// for scroll events, matching the Zig `ScrollMods` packed struct.
    struct ScrollMods {
        let rawValue: Int32

        /// True if this is a high-precision scroll event (e.g., trackpad, Magic Mouse)
        var precision: Bool {
            rawValue & 0b0000_0001 != 0
        }

        /// The momentum phase of the scroll event for inertial scrolling
        var momentum: Momentum {
            let momentumBits = (rawValue >> 1) & 0b0000_0111
            return Momentum(rawValue: UInt8(momentumBits)) ?? .none
        }

        init(precision: Bool = false, momentum: Momentum = .none) {
            var value: Int32 = 0
            if precision {
                value |= 0b0000_0001
            }
            value |= Int32(momentum.rawValue) << 1
            self.rawValue = value
        }

        init(rawValue: Int32) {
            self.rawValue = rawValue
        }

        var cScrollMods: rustty_input_scroll_mods_t {
            rawValue
        }
    }
}

// MARK: Rustty.Input.Momentum

extension Rustty.Input {
    /// `rustty_input_mouse_momentum_e` - Momentum phase for scroll events
    enum Momentum: UInt8, CaseIterable {
        case none = 0
        case began = 1
        case stationary = 2
        case changed = 3
        case ended = 4
        case cancelled = 5
        case mayBegin = 6

        var cMomentum: rustty_input_mouse_momentum_e {
            switch self {
            case .none: RUSTTY_MOUSE_MOMENTUM_NONE
            case .began: RUSTTY_MOUSE_MOMENTUM_BEGAN
            case .stationary: RUSTTY_MOUSE_MOMENTUM_STATIONARY
            case .changed: RUSTTY_MOUSE_MOMENTUM_CHANGED
            case .ended: RUSTTY_MOUSE_MOMENTUM_ENDED
            case .cancelled: RUSTTY_MOUSE_MOMENTUM_CANCELLED
            case .mayBegin: RUSTTY_MOUSE_MOMENTUM_MAY_BEGIN
            }
        }
    }
}

extension Rustty.Input.Momentum: AppEnum {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Scroll Momentum")

    static var caseDisplayRepresentations: [Rustty.Input.Momentum: DisplayRepresentation] = [
        .none: "None",
        .began: "Began",
        .stationary: "Stationary",
        .changed: "Changed",
        .ended: "Ended",
        .cancelled: "Cancelled",
        .mayBegin: "May Begin"
    ]
}

import AppKit

extension Rustty.Input.Momentum {
    /// Create a Momentum from an NSEvent.Phase
    init(_ phase: NSEvent.Phase) {
        switch phase {
        case .began: self = .began
        case .stationary: self = .stationary
        case .changed: self = .changed
        case .ended: self = .ended
        case .cancelled: self = .cancelled
        case .mayBegin: self = .mayBegin
        default: self = .none
        }
    }
}

// MARK: Rustty.Input.Mods

extension Rustty.Input {
    /// `rustty_input_mods_e`
    struct Mods: OptionSet {
        let rawValue: UInt32

        static let none = Mods(rawValue: RUSTTY_MODS_NONE.rawValue)
        static let shift = Mods(rawValue: RUSTTY_MODS_SHIFT.rawValue)
        static let ctrl = Mods(rawValue: RUSTTY_MODS_CTRL.rawValue)
        static let alt = Mods(rawValue: RUSTTY_MODS_ALT.rawValue)
        static let `super` = Mods(rawValue: RUSTTY_MODS_SUPER.rawValue)
        static let caps = Mods(rawValue: RUSTTY_MODS_CAPS.rawValue)
        static let shiftRight = Mods(rawValue: RUSTTY_MODS_SHIFT_RIGHT.rawValue)
        static let ctrlRight = Mods(rawValue: RUSTTY_MODS_CTRL_RIGHT.rawValue)
        static let altRight = Mods(rawValue: RUSTTY_MODS_ALT_RIGHT.rawValue)
        static let superRight = Mods(rawValue: RUSTTY_MODS_SUPER_RIGHT.rawValue)

        var cMods: rustty_input_mods_e {
            rustty_input_mods_e(rawValue)
        }

        init(rawValue: UInt32) {
            self.rawValue = rawValue
        }

        init(cMods: rustty_input_mods_e) {
            self.rawValue = cMods.rawValue
        }

        init(nsFlags: NSEvent.ModifierFlags) {
            self.init(cMods: Rustty.rusttyMods(nsFlags))
        }

        var nsFlags: NSEvent.ModifierFlags {
            Rustty.eventModifierFlags(mods: cMods)
        }
    }
}

// MARK: Rustty.Input.Key

extension Rustty.Input {
    /// `rustty_input_key_e`
    enum Key: String {
        // Writing System Keys
        case backquote
        case backslash
        case bracketLeft
        case bracketRight
        case comma
        case digit0
        case digit1
        case digit2
        case digit3
        case digit4
        case digit5
        case digit6
        case digit7
        case digit8
        case digit9
        case equal
        case intlBackslash
        case intlRo
        case intlYen
        case a
        case b
        case c
        case d
        case e
        case f
        case g
        case h
        case i
        case j
        case k
        case l
        case m
        case n
        case o
        case p
        case q
        case r
        case s
        case t
        case u
        case v
        case w
        case x
        case y
        case z
        case minus
        case period
        case quote
        case semicolon
        case slash

        // Functional Keys
        case altLeft
        case altRight
        case backspace
        case capsLock
        case contextMenu
        case controlLeft
        case controlRight
        case enter
        case metaLeft
        case metaRight
        case shiftLeft
        case shiftRight
        case space
        case tab
        case convert
        case kanaMode
        case nonConvert

        // Control Pad Section
        case delete
        case end
        case help
        case home
        case insert
        case pageDown
        case pageUp

        // Arrow Pad Section
        case arrowDown
        case arrowLeft
        case arrowRight
        case arrowUp

        // Numpad Section
        case numLock
        case numpad0
        case numpad1
        case numpad2
        case numpad3
        case numpad4
        case numpad5
        case numpad6
        case numpad7
        case numpad8
        case numpad9
        case numpadAdd
        case numpadBackspace
        case numpadClear
        case numpadClearEntry
        case numpadComma
        case numpadDecimal
        case numpadDivide
        case numpadEnter
        case numpadEqual
        case numpadMemoryAdd
        case numpadMemoryClear
        case numpadMemoryRecall
        case numpadMemoryStore
        case numpadMemorySubtract
        case numpadMultiply
        case numpadParenLeft
        case numpadParenRight
        case numpadSubtract
        case numpadSeparator
        case numpadUp
        case numpadDown
        case numpadRight
        case numpadLeft
        case numpadBegin
        case numpadHome
        case numpadEnd
        case numpadInsert
        case numpadDelete
        case numpadPageUp
        case numpadPageDown

        // Function Section
        case escape
        case f1
        case f2
        case f3
        case f4
        case f5
        case f6
        case f7
        case f8
        case f9
        case f10
        case f11
        case f12
        case f13
        case f14
        case f15
        case f16
        case f17
        case f18
        case f19
        case f20
        case f21
        case f22
        case f23
        case f24
        case f25
        case fn
        case fnLock
        case printScreen
        case scrollLock
        case pause

        // Media Keys
        case browserBack
        case browserFavorites
        case browserForward
        case browserHome
        case browserRefresh
        case browserSearch
        case browserStop
        case eject
        case launchApp1
        case launchApp2
        case launchMail
        case mediaPlayPause
        case mediaSelect
        case mediaStop
        case mediaTrackNext
        case mediaTrackPrevious
        case power
        case sleep
        case audioVolumeDown
        case audioVolumeMute
        case audioVolumeUp
        case wakeUp

        // Legacy, Non-standard, and Special Keys
        case copy
        case cut
        case paste

        init?(cKey: rustty_input_key_e) {
            guard let key = Key.allCases.first(where: { $0.cKey == cKey }) else { return nil }
            self = key
        }

        /// Get a key from a keycode
        init?(keyCode: UInt16) {
            guard let key = Key.allCases.first(where: { $0.keyCode == keyCode }) else { return nil }
            self = key
        }

        var cKey: rustty_input_key_e {
            switch self {
            // Writing System Keys
            case .backquote: RUSTTY_KEY_BACKQUOTE
            case .backslash: RUSTTY_KEY_BACKSLASH
            case .bracketLeft: RUSTTY_KEY_BRACKET_LEFT
            case .bracketRight: RUSTTY_KEY_BRACKET_RIGHT
            case .comma: RUSTTY_KEY_COMMA
            case .digit0: RUSTTY_KEY_DIGIT_0
            case .digit1: RUSTTY_KEY_DIGIT_1
            case .digit2: RUSTTY_KEY_DIGIT_2
            case .digit3: RUSTTY_KEY_DIGIT_3
            case .digit4: RUSTTY_KEY_DIGIT_4
            case .digit5: RUSTTY_KEY_DIGIT_5
            case .digit6: RUSTTY_KEY_DIGIT_6
            case .digit7: RUSTTY_KEY_DIGIT_7
            case .digit8: RUSTTY_KEY_DIGIT_8
            case .digit9: RUSTTY_KEY_DIGIT_9
            case .equal: RUSTTY_KEY_EQUAL
            case .intlBackslash: RUSTTY_KEY_INTL_BACKSLASH
            case .intlRo: RUSTTY_KEY_INTL_RO
            case .intlYen: RUSTTY_KEY_INTL_YEN
            case .a: RUSTTY_KEY_A
            case .b: RUSTTY_KEY_B
            case .c: RUSTTY_KEY_C
            case .d: RUSTTY_KEY_D
            case .e: RUSTTY_KEY_E
            case .f: RUSTTY_KEY_F
            case .g: RUSTTY_KEY_G
            case .h: RUSTTY_KEY_H
            case .i: RUSTTY_KEY_I
            case .j: RUSTTY_KEY_J
            case .k: RUSTTY_KEY_K
            case .l: RUSTTY_KEY_L
            case .m: RUSTTY_KEY_M
            case .n: RUSTTY_KEY_N
            case .o: RUSTTY_KEY_O
            case .p: RUSTTY_KEY_P
            case .q: RUSTTY_KEY_Q
            case .r: RUSTTY_KEY_R
            case .s: RUSTTY_KEY_S
            case .t: RUSTTY_KEY_T
            case .u: RUSTTY_KEY_U
            case .v: RUSTTY_KEY_V
            case .w: RUSTTY_KEY_W
            case .x: RUSTTY_KEY_X
            case .y: RUSTTY_KEY_Y
            case .z: RUSTTY_KEY_Z
            case .minus: RUSTTY_KEY_MINUS
            case .period: RUSTTY_KEY_PERIOD
            case .quote: RUSTTY_KEY_QUOTE
            case .semicolon: RUSTTY_KEY_SEMICOLON
            case .slash: RUSTTY_KEY_SLASH

            // Functional Keys
            case .altLeft: RUSTTY_KEY_ALT_LEFT
            case .altRight: RUSTTY_KEY_ALT_RIGHT
            case .backspace: RUSTTY_KEY_BACKSPACE
            case .capsLock: RUSTTY_KEY_CAPS_LOCK
            case .contextMenu: RUSTTY_KEY_CONTEXT_MENU
            case .controlLeft: RUSTTY_KEY_CONTROL_LEFT
            case .controlRight: RUSTTY_KEY_CONTROL_RIGHT
            case .enter: RUSTTY_KEY_ENTER
            case .metaLeft: RUSTTY_KEY_META_LEFT
            case .metaRight: RUSTTY_KEY_META_RIGHT
            case .shiftLeft: RUSTTY_KEY_SHIFT_LEFT
            case .shiftRight: RUSTTY_KEY_SHIFT_RIGHT
            case .space: RUSTTY_KEY_SPACE
            case .tab: RUSTTY_KEY_TAB
            case .convert: RUSTTY_KEY_CONVERT
            case .kanaMode: RUSTTY_KEY_KANA_MODE
            case .nonConvert: RUSTTY_KEY_NON_CONVERT

            // Control Pad Section
            case .delete: RUSTTY_KEY_DELETE
            case .end: RUSTTY_KEY_END
            case .help: RUSTTY_KEY_HELP
            case .home: RUSTTY_KEY_HOME
            case .insert: RUSTTY_KEY_INSERT
            case .pageDown: RUSTTY_KEY_PAGE_DOWN
            case .pageUp: RUSTTY_KEY_PAGE_UP

            // Arrow Pad Section
            case .arrowDown: RUSTTY_KEY_ARROW_DOWN
            case .arrowLeft: RUSTTY_KEY_ARROW_LEFT
            case .arrowRight: RUSTTY_KEY_ARROW_RIGHT
            case .arrowUp: RUSTTY_KEY_ARROW_UP

            // Numpad Section
            case .numLock: RUSTTY_KEY_NUM_LOCK
            case .numpad0: RUSTTY_KEY_NUMPAD_0
            case .numpad1: RUSTTY_KEY_NUMPAD_1
            case .numpad2: RUSTTY_KEY_NUMPAD_2
            case .numpad3: RUSTTY_KEY_NUMPAD_3
            case .numpad4: RUSTTY_KEY_NUMPAD_4
            case .numpad5: RUSTTY_KEY_NUMPAD_5
            case .numpad6: RUSTTY_KEY_NUMPAD_6
            case .numpad7: RUSTTY_KEY_NUMPAD_7
            case .numpad8: RUSTTY_KEY_NUMPAD_8
            case .numpad9: RUSTTY_KEY_NUMPAD_9
            case .numpadAdd: RUSTTY_KEY_NUMPAD_ADD
            case .numpadBackspace: RUSTTY_KEY_NUMPAD_BACKSPACE
            case .numpadClear: RUSTTY_KEY_NUMPAD_CLEAR
            case .numpadClearEntry: RUSTTY_KEY_NUMPAD_CLEAR_ENTRY
            case .numpadComma: RUSTTY_KEY_NUMPAD_COMMA
            case .numpadDecimal: RUSTTY_KEY_NUMPAD_DECIMAL
            case .numpadDivide: RUSTTY_KEY_NUMPAD_DIVIDE
            case .numpadEnter: RUSTTY_KEY_NUMPAD_ENTER
            case .numpadEqual: RUSTTY_KEY_NUMPAD_EQUAL
            case .numpadMemoryAdd: RUSTTY_KEY_NUMPAD_MEMORY_ADD
            case .numpadMemoryClear: RUSTTY_KEY_NUMPAD_MEMORY_CLEAR
            case .numpadMemoryRecall: RUSTTY_KEY_NUMPAD_MEMORY_RECALL
            case .numpadMemoryStore: RUSTTY_KEY_NUMPAD_MEMORY_STORE
            case .numpadMemorySubtract: RUSTTY_KEY_NUMPAD_MEMORY_SUBTRACT
            case .numpadMultiply: RUSTTY_KEY_NUMPAD_MULTIPLY
            case .numpadParenLeft: RUSTTY_KEY_NUMPAD_PAREN_LEFT
            case .numpadParenRight: RUSTTY_KEY_NUMPAD_PAREN_RIGHT
            case .numpadSubtract: RUSTTY_KEY_NUMPAD_SUBTRACT
            case .numpadSeparator: RUSTTY_KEY_NUMPAD_SEPARATOR
            case .numpadUp: RUSTTY_KEY_NUMPAD_UP
            case .numpadDown: RUSTTY_KEY_NUMPAD_DOWN
            case .numpadRight: RUSTTY_KEY_NUMPAD_RIGHT
            case .numpadLeft: RUSTTY_KEY_NUMPAD_LEFT
            case .numpadBegin: RUSTTY_KEY_NUMPAD_BEGIN
            case .numpadHome: RUSTTY_KEY_NUMPAD_HOME
            case .numpadEnd: RUSTTY_KEY_NUMPAD_END
            case .numpadInsert: RUSTTY_KEY_NUMPAD_INSERT
            case .numpadDelete: RUSTTY_KEY_NUMPAD_DELETE
            case .numpadPageUp: RUSTTY_KEY_NUMPAD_PAGE_UP
            case .numpadPageDown: RUSTTY_KEY_NUMPAD_PAGE_DOWN

            // Function Section
            case .escape: RUSTTY_KEY_ESCAPE
            case .f1: RUSTTY_KEY_F1
            case .f2: RUSTTY_KEY_F2
            case .f3: RUSTTY_KEY_F3
            case .f4: RUSTTY_KEY_F4
            case .f5: RUSTTY_KEY_F5
            case .f6: RUSTTY_KEY_F6
            case .f7: RUSTTY_KEY_F7
            case .f8: RUSTTY_KEY_F8
            case .f9: RUSTTY_KEY_F9
            case .f10: RUSTTY_KEY_F10
            case .f11: RUSTTY_KEY_F11
            case .f12: RUSTTY_KEY_F12
            case .f13: RUSTTY_KEY_F13
            case .f14: RUSTTY_KEY_F14
            case .f15: RUSTTY_KEY_F15
            case .f16: RUSTTY_KEY_F16
            case .f17: RUSTTY_KEY_F17
            case .f18: RUSTTY_KEY_F18
            case .f19: RUSTTY_KEY_F19
            case .f20: RUSTTY_KEY_F20
            case .f21: RUSTTY_KEY_F21
            case .f22: RUSTTY_KEY_F22
            case .f23: RUSTTY_KEY_F23
            case .f24: RUSTTY_KEY_F24
            case .f25: RUSTTY_KEY_F25
            case .fn: RUSTTY_KEY_FN
            case .fnLock: RUSTTY_KEY_FN_LOCK
            case .printScreen: RUSTTY_KEY_PRINT_SCREEN
            case .scrollLock: RUSTTY_KEY_SCROLL_LOCK
            case .pause: RUSTTY_KEY_PAUSE

            // Media Keys
            case .browserBack: RUSTTY_KEY_BROWSER_BACK
            case .browserFavorites: RUSTTY_KEY_BROWSER_FAVORITES
            case .browserForward: RUSTTY_KEY_BROWSER_FORWARD
            case .browserHome: RUSTTY_KEY_BROWSER_HOME
            case .browserRefresh: RUSTTY_KEY_BROWSER_REFRESH
            case .browserSearch: RUSTTY_KEY_BROWSER_SEARCH
            case .browserStop: RUSTTY_KEY_BROWSER_STOP
            case .eject: RUSTTY_KEY_EJECT
            case .launchApp1: RUSTTY_KEY_LAUNCH_APP_1
            case .launchApp2: RUSTTY_KEY_LAUNCH_APP_2
            case .launchMail: RUSTTY_KEY_LAUNCH_MAIL
            case .mediaPlayPause: RUSTTY_KEY_MEDIA_PLAY_PAUSE
            case .mediaSelect: RUSTTY_KEY_MEDIA_SELECT
            case .mediaStop: RUSTTY_KEY_MEDIA_STOP
            case .mediaTrackNext: RUSTTY_KEY_MEDIA_TRACK_NEXT
            case .mediaTrackPrevious: RUSTTY_KEY_MEDIA_TRACK_PREVIOUS
            case .power: RUSTTY_KEY_POWER
            case .sleep: RUSTTY_KEY_SLEEP
            case .audioVolumeDown: RUSTTY_KEY_AUDIO_VOLUME_DOWN
            case .audioVolumeMute: RUSTTY_KEY_AUDIO_VOLUME_MUTE
            case .audioVolumeUp: RUSTTY_KEY_AUDIO_VOLUME_UP
            case .wakeUp: RUSTTY_KEY_WAKE_UP

            // Legacy, Non-standard, and Special Keys
            case .copy: RUSTTY_KEY_COPY
            case .cut: RUSTTY_KEY_CUT
            case .paste: RUSTTY_KEY_PASTE
            }
        }

        // Based on src/input/keycodes.zig
        var keyCode: UInt16? {
            switch self {
            // Writing System Keys
            case .backquote: return 0x0032
            case .backslash: return 0x002a
            case .bracketLeft: return 0x0021
            case .bracketRight: return 0x001e
            case .comma: return 0x002b
            case .digit0: return 0x001d
            case .digit1: return 0x0012
            case .digit2: return 0x0013
            case .digit3: return 0x0014
            case .digit4: return 0x0015
            case .digit5: return 0x0017
            case .digit6: return 0x0016
            case .digit7: return 0x001a
            case .digit8: return 0x001c
            case .digit9: return 0x0019
            case .equal: return 0x0018
            case .intlBackslash: return 0x000a
            case .intlRo: return 0x005e
            case .intlYen: return 0x005d
            case .a: return 0x0000
            case .b: return 0x000b
            case .c: return 0x0008
            case .d: return 0x0002
            case .e: return 0x000e
            case .f: return 0x0003
            case .g: return 0x0005
            case .h: return 0x0004
            case .i: return 0x0022
            case .j: return 0x0026
            case .k: return 0x0028
            case .l: return 0x0025
            case .m: return 0x002e
            case .n: return 0x002d
            case .o: return 0x001f
            case .p: return 0x0023
            case .q: return 0x000c
            case .r: return 0x000f
            case .s: return 0x0001
            case .t: return 0x0011
            case .u: return 0x0020
            case .v: return 0x0009
            case .w: return 0x000d
            case .x: return 0x0007
            case .y: return 0x0010
            case .z: return 0x0006
            case .minus: return 0x001b
            case .period: return 0x002f
            case .quote: return 0x0027
            case .semicolon: return 0x0029
            case .slash: return 0x002c

            // Functional Keys
            case .altLeft: return 0x003a
            case .altRight: return 0x003d
            case .backspace: return 0x0033
            case .capsLock: return 0x0039
            case .contextMenu: return 0x006e
            case .controlLeft: return 0x003b
            case .controlRight: return 0x003e
            case .enter: return 0x0024
            case .metaLeft: return 0x0037
            case .metaRight: return 0x0036
            case .shiftLeft: return 0x0038
            case .shiftRight: return 0x003c
            case .space: return 0x0031
            case .tab: return 0x0030
            case .convert: return nil // No Mac keycode
            case .kanaMode: return nil // No Mac keycode
            case .nonConvert: return nil // No Mac keycode

            // Control Pad Section
            case .delete: return 0x0075
            case .end: return 0x0077
            case .help: return nil // No Mac keycode
            case .home: return 0x0073
            case .insert: return 0x0072
            case .pageDown: return 0x0079
            case .pageUp: return 0x0074

            // Arrow Pad Section
            case .arrowDown: return 0x007d
            case .arrowLeft: return 0x007b
            case .arrowRight: return 0x007c
            case .arrowUp: return 0x007e

            // Numpad Section
            case .numLock: return 0x0047
            case .numpad0: return 0x0052
            case .numpad1: return 0x0053
            case .numpad2: return 0x0054
            case .numpad3: return 0x0055
            case .numpad4: return 0x0056
            case .numpad5: return 0x0057
            case .numpad6: return 0x0058
            case .numpad7: return 0x0059
            case .numpad8: return 0x005b
            case .numpad9: return 0x005c
            case .numpadAdd: return 0x0045
            case .numpadBackspace: return nil // No Mac keycode
            case .numpadClear: return nil // No Mac keycode
            case .numpadClearEntry: return nil // No Mac keycode
            case .numpadComma: return 0x005f
            case .numpadDecimal: return 0x0041
            case .numpadDivide: return 0x004b
            case .numpadEnter: return 0x004c
            case .numpadEqual: return 0x0051
            case .numpadMemoryAdd: return nil // No Mac keycode
            case .numpadMemoryClear: return nil // No Mac keycode
            case .numpadMemoryRecall: return nil // No Mac keycode
            case .numpadMemoryStore: return nil // No Mac keycode
            case .numpadMemorySubtract: return nil // No Mac keycode
            case .numpadMultiply: return 0x0043
            case .numpadParenLeft: return nil // No Mac keycode
            case .numpadParenRight: return nil // No Mac keycode
            case .numpadSubtract: return 0x004e
            case .numpadSeparator: return nil // No Mac keycode
            case .numpadUp: return nil // No Mac keycode
            case .numpadDown: return nil // No Mac keycode
            case .numpadRight: return nil // No Mac keycode
            case .numpadLeft: return nil // No Mac keycode
            case .numpadBegin: return nil // No Mac keycode
            case .numpadHome: return nil // No Mac keycode
            case .numpadEnd: return nil // No Mac keycode
            case .numpadInsert: return nil // No Mac keycode
            case .numpadDelete: return nil // No Mac keycode
            case .numpadPageUp: return nil // No Mac keycode
            case .numpadPageDown: return nil // No Mac keycode

            // Function Section
            case .escape: return 0x0035
            case .f1: return 0x007a
            case .f2: return 0x0078
            case .f3: return 0x0063
            case .f4: return 0x0076
            case .f5: return 0x0060
            case .f6: return 0x0061
            case .f7: return 0x0062
            case .f8: return 0x0064
            case .f9: return 0x0065
            case .f10: return 0x006d
            case .f11: return 0x0067
            case .f12: return 0x006f
            case .f13: return 0x0069
            case .f14: return 0x006b
            case .f15: return 0x0071
            case .f16: return 0x006a
            case .f17: return 0x0040
            case .f18: return 0x004f
            case .f19: return 0x0050
            case .f20: return 0x005a
            case .f21: return nil // No Mac keycode
            case .f22: return nil // No Mac keycode
            case .f23: return nil // No Mac keycode
            case .f24: return nil // No Mac keycode
            case .f25: return nil // No Mac keycode
            case .fn: return nil // No Mac keycode
            case .fnLock: return nil // No Mac keycode
            case .printScreen: return nil // No Mac keycode
            case .scrollLock: return nil // No Mac keycode
            case .pause: return nil // No Mac keycode

            // Media Keys
            case .browserBack: return nil // No Mac keycode
            case .browserFavorites: return nil // No Mac keycode
            case .browserForward: return nil // No Mac keycode
            case .browserHome: return nil // No Mac keycode
            case .browserRefresh: return nil // No Mac keycode
            case .browserSearch: return nil // No Mac keycode
            case .browserStop: return nil // No Mac keycode
            case .eject: return nil // No Mac keycode
            case .launchApp1: return nil // No Mac keycode
            case .launchApp2: return nil // No Mac keycode
            case .launchMail: return nil // No Mac keycode
            case .mediaPlayPause: return nil // No Mac keycode
            case .mediaSelect: return nil // No Mac keycode
            case .mediaStop: return nil // No Mac keycode
            case .mediaTrackNext: return nil // No Mac keycode
            case .mediaTrackPrevious: return nil // No Mac keycode
            case .power: return nil // No Mac keycode
            case .sleep: return nil // No Mac keycode
            case .audioVolumeDown: return 0x0049
            case .audioVolumeMute: return 0x004a
            case .audioVolumeUp: return 0x0048
            case .wakeUp: return nil // No Mac keycode

            // Legacy, Non-standard, and Special Keys
            case .copy: return nil // No Mac keycode
            case .cut: return nil // No Mac keycode
            case .paste: return nil // No Mac keycode
            }
        }
    }
}

extension Rustty.Input.Key: AppEnum {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Key")

    // Only include keys that have Mac keycodes for App Intents
    static var allCases: [Rustty.Input.Key] {
        return [
            // Letters (A-Z)
            .a, .b, .c, .d, .e, .f, .g, .h, .i, .j, .k, .l, .m, .n, .o, .p, .q, .r, .s, .t, .u, .v, .w, .x, .y, .z,

            // Numbers (0-9)
            .digit0, .digit1, .digit2, .digit3, .digit4, .digit5, .digit6, .digit7, .digit8, .digit9,

            // Common Control Keys
            .space, .enter, .tab, .backspace, .escape, .delete,

            // Arrow Keys
            .arrowUp, .arrowDown, .arrowLeft, .arrowRight,

            // Navigation Keys
            .home, .end, .pageUp, .pageDown, .insert,

            // Function Keys (F1-F20)
            .f1, .f2, .f3, .f4, .f5, .f6, .f7, .f8, .f9, .f10, .f11, .f12,
            .f13, .f14, .f15, .f16, .f17, .f18, .f19, .f20,

            // Modifier Keys
            .shiftLeft, .shiftRight, .controlLeft, .controlRight, .altLeft, .altRight,
            .metaLeft, .metaRight, .capsLock,

            // Punctuation & Symbols
            .minus, .equal, .backquote, .bracketLeft, .bracketRight, .backslash,
            .semicolon, .quote, .comma, .period, .slash,

            // Numpad
            .numLock, .numpad0, .numpad1, .numpad2, .numpad3, .numpad4, .numpad5,
            .numpad6, .numpad7, .numpad8, .numpad9, .numpadAdd, .numpadSubtract,
            .numpadMultiply, .numpadDivide, .numpadDecimal, .numpadEqual,
            .numpadEnter, .numpadComma,

            // Media Keys
            .audioVolumeUp, .audioVolumeDown, .audioVolumeMute,

            // International Keys
            .intlBackslash, .intlRo, .intlYen,

            // Other
            .contextMenu
        ]
    }

    static var caseDisplayRepresentations: [Rustty.Input.Key: DisplayRepresentation] = [
        // Letters (A-Z)
        .a: "A", .b: "B", .c: "C", .d: "D", .e: "E", .f: "F", .g: "G", .h: "H", .i: "I", .j: "J",
        .k: "K", .l: "L", .m: "M", .n: "N", .o: "O", .p: "P", .q: "Q", .r: "R", .s: "S", .t: "T",
        .u: "U", .v: "V", .w: "W", .x: "X", .y: "Y", .z: "Z",

        // Numbers (0-9)
        .digit0: "0", .digit1: "1", .digit2: "2", .digit3: "3", .digit4: "4",
        .digit5: "5", .digit6: "6", .digit7: "7", .digit8: "8", .digit9: "9",

        // Common Control Keys
        .space: "Space",
        .enter: "Enter",
        .tab: "Tab",
        .backspace: "Backspace",
        .escape: "Escape",
        .delete: "Delete",

        // Arrow Keys
        .arrowUp: "Up Arrow",
        .arrowDown: "Down Arrow",
        .arrowLeft: "Left Arrow",
        .arrowRight: "Right Arrow",

        // Navigation Keys
        .home: "Home",
        .end: "End",
        .pageUp: "Page Up",
        .pageDown: "Page Down",
        .insert: "Insert",

        // Function Keys (F1-F20)
        .f1: "F1", .f2: "F2", .f3: "F3", .f4: "F4", .f5: "F5", .f6: "F6",
        .f7: "F7", .f8: "F8", .f9: "F9", .f10: "F10", .f11: "F11", .f12: "F12",
        .f13: "F13", .f14: "F14", .f15: "F15", .f16: "F16", .f17: "F17",
        .f18: "F18", .f19: "F19", .f20: "F20",

        // Modifier Keys
        .shiftLeft: "Left Shift",
        .shiftRight: "Right Shift",
        .controlLeft: "Left Control",
        .controlRight: "Right Control",
        .altLeft: "Left Alt",
        .altRight: "Right Alt",
        .metaLeft: "Left Command",
        .metaRight: "Right Command",
        .capsLock: "Caps Lock",

        // Punctuation & Symbols
        .minus: "Minus (-)",
        .equal: "Equal (=)",
        .backquote: "Backtick (`)",
        .bracketLeft: "Left Bracket ([)",
        .bracketRight: "Right Bracket (])",
        .backslash: "Backslash (\\)",
        .semicolon: "Semicolon (;)",
        .quote: "Quote (')",
        .comma: "Comma (,)",
        .period: "Period (.)",
        .slash: "Slash (/)",

        // Numpad
        .numLock: "Num Lock",
        .numpad0: "Numpad 0", .numpad1: "Numpad 1", .numpad2: "Numpad 2",
        .numpad3: "Numpad 3", .numpad4: "Numpad 4", .numpad5: "Numpad 5",
        .numpad6: "Numpad 6", .numpad7: "Numpad 7", .numpad8: "Numpad 8", .numpad9: "Numpad 9",
        .numpadAdd: "Numpad Add (+)",
        .numpadSubtract: "Numpad Subtract (-)",
        .numpadMultiply: "Numpad Multiply (×)",
        .numpadDivide: "Numpad Divide (÷)",
        .numpadDecimal: "Numpad Decimal",
        .numpadEqual: "Numpad Equal",
        .numpadEnter: "Numpad Enter",
        .numpadComma: "Numpad Comma",

        // Media Keys
        .audioVolumeUp: "Volume Up",
        .audioVolumeDown: "Volume Down",
        .audioVolumeMute: "Volume Mute",

        // International Keys
        .intlBackslash: "International Backslash",
        .intlRo: "International Ro",
        .intlYen: "International Yen",

        // Other
        .contextMenu: "Context Menu"
    ]
}
