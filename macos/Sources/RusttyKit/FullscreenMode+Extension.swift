import RusttyKit

extension FullscreenMode {
    /// Initialize from a Rustty fullscreen action.
    static func from(rustty: rustty_action_fullscreen_e) -> Self? {
        return switch rustty {
        case RUSTTY_FULLSCREEN_NATIVE:
                .native

        case RUSTTY_FULLSCREEN_MACOS_NON_NATIVE:
                .nonNative

        case RUSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU:
                .nonNativeVisibleMenu

        case RUSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH:
                .nonNativePaddedNotch

        default:
            nil
        }
    }
}
