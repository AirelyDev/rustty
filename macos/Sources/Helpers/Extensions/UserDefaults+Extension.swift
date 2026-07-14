import Foundation

extension UserDefaults {
    static var rusttySuite: String? {
        #if DEBUG
        ProcessInfo.processInfo.environment["RUSTTY_USER_DEFAULTS_SUITE"]
        #else
        nil
        #endif
    }

    static var rustty: UserDefaults {
        rusttySuite.flatMap(UserDefaults.init(suiteName:)) ?? .standard
    }
}
