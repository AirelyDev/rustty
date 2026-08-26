// PORTS: ghostty example/swift-vt-xcframework/Sources/main.swift, with the
// canonical `rustty_*` / `Rustty*` spellings and the `RusttyVt` module name.
import Foundation
import RusttyVt

// Create a terminal with a small grid
var terminal: RusttyTerminal?
let result = rustty_terminal_new(nil, &terminal, 80, 24)
guard result == RUSTTY_SUCCESS, let terminal else {
    fatalError("Failed to create terminal")
}

// Write some VT-encoded content
let text = "Hello from \u{1b}[1mSwift\u{1b}[0m via xcframework!\r\n"
text.withCString { ptr in
    rustty_terminal_vt_write(terminal, ptr, strlen(ptr))
}

// Format the terminal contents as plain text
var fmtOpts = RusttyFormatterTerminalOptions()
fmtOpts.size = MemoryLayout<RusttyFormatterTerminalOptions>.size
fmtOpts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN
fmtOpts.trim = true

var formatter: RusttyFormatter?
let fmtResult = rustty_formatter_terminal_new(nil, &formatter, terminal, fmtOpts)
guard fmtResult == RUSTTY_SUCCESS, let formatter else {
    fatalError("Failed to create formatter")
}

var buf: UnsafeMutablePointer<UInt8>?
var len: Int = 0
let allocResult = rustty_formatter_format_alloc(formatter, nil, &buf, &len)
guard allocResult == RUSTTY_SUCCESS, let buf else {
    fatalError("Failed to format")
}

print("Plain text (\(len) bytes):")
let data = Data(bytes: buf, count: len)
print(String(data: data, encoding: .utf8) ?? "<invalid UTF-8>")

rustty_free(nil, buf, len)
rustty_formatter_free(formatter)
rustty_terminal_free(terminal)
