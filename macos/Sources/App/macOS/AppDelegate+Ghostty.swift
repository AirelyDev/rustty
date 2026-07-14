import AppKit

// MARK: Rustty Delegate

/// This implements the Rustty app delegate protocol which is used by the Rustty
/// APIs for app-global information.
extension AppDelegate: Rustty.Delegate {
    func ghosttySurface(id: UUID) -> Rustty.SurfaceView? {
        for window in NSApp.windows {
            guard let controller = window.windowController as? BaseTerminalController else {
                continue
            }

            for surface in controller.surfaceTree where surface.id == id {
                return surface
            }
        }

        return nil
    }
}
