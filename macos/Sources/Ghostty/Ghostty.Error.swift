extension Rustty {
    /// Possible errors from internal Rustty calls.
    enum Error: Swift.Error, CustomLocalizedStringResourceConvertible {
        case apiFailed

        var localizedStringResource: LocalizedStringResource {
            switch self {
            case .apiFailed: return "librustty API call failed"
            }
        }
    }
}
