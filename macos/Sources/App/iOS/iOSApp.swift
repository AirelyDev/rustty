import SwiftUI
import RusttyKit

@main
struct Ghostty_iOSApp: App {
    @StateObject private var rustty_app: Ghostty.App

    init() {
        if rustty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv) != RUSTTY_SUCCESS {
            preconditionFailure("Initialize ghostty backend failed")
        }
        _rustty_app = StateObject(wrappedValue: Ghostty.App())
    }

    var body: some Scene {
        WindowGroup {
            iOS_GhosttyTerminal()
                .environmentObject(rustty_app)
        }
    }
}

struct iOS_GhosttyTerminal: View {
    @EnvironmentObject private var rustty_app: Ghostty.App

    var body: some View {
        ZStack {
            // Make sure that our background color extends to all parts of the screen
            Color(rustty_app.config.backgroundColor).ignoresSafeArea()

            Ghostty.Terminal()
        }
    }
}

struct iOS_GhosttyInitView: View {
    @EnvironmentObject private var rustty_app: Ghostty.App

    var body: some View {
        VStack {
            Image("AppIconImage")
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(maxHeight: 96)
            Text("Rustty")
            Text("State: \(rustty_app.readiness.rawValue)")
        }
        .padding()
    }
}
