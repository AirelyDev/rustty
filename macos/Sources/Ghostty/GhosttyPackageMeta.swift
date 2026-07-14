import Foundation
import os

// This defines the minimal information required so all other files can do
// `extension Rustty` to add more to it. This purposely has minimal
// dependencies so things like our dock tile plugin can use it.
enum Rustty {
    // The primary logger used by the RusttyKit libraries.
    static let logger = Logger(
        subsystem: Bundle.main.bundleIdentifier!,
        category: "rustty"
    )

    // All the notifications that will be emitted will be put here.
    struct Notification {}
}
