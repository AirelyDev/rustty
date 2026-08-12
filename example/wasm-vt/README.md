# WebAssembly VT Terminal Example

This example demonstrates how to use the Rustty VT library from WebAssembly
to initialize a terminal, write VT-encoded data to it, and format the
terminal contents as plain text.

## Building

First, build the WebAssembly module (the freestanding VT C-ABI — the rustty
analog of ghostty's `ghostty-vt.wasm`):

```bash
.port/tools/build-vt-wasm.sh
```

This will create `zig-out/bin/rustty-vt.wasm` (pure Rust,
`wasm32-unknown-unknown`, zero imports) — the analog of ghostty's installed
`zig-out/bin/ghostty-vt.wasm`, linked with the indirect function table exported
and patched growable so browser JS can install effect callbacks.

## Running

**Important:** You must serve this via HTTP, not open it as a file directly.
Browsers block loading WASM files from `file://` URLs.

From the **root of the rustty repository**, serve with a local HTTP server:

```bash
# Using Python (recommended)
python3 -m http.server 8000

# Or using Node.js
npx serve .

# Or using PHP
php -S localhost:8000
```

Then open your browser to:

```
http://localhost:8000/example/wasm-vt/
```
