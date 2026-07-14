import Foundation

extension Rustty {
    /// This is a delegate that should be applied to your global app delegate for RusttyKit
    /// to perform app-global operations.
    protocol Delegate {
        /// Look up a surface within the application by ID.
        func rusttySurface(id: UUID) -> SurfaceView?
    }
}
