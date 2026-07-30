// glue.mjs — the `env` import object for `rustty_wasm.wasm`.
//
// This file exists so that the harness page and the WS14 gate load IDENTICAL glue
// through one stable relative path: the page imports it as a module, and
// `gate.js` reaches it with a dynamic `import()` (so the same assertion body works
// verbatim from CDP `Runtime.evaluate`, which cannot do a static import).
//
// The whole `env` object is the `rustty_wc_*` Canvas FFI the port already ships for
// the `web_canvas` font face (`.port/tools/web-canvas-glue.mjs`). `rustty_wasm.wasm`
// imports a strict SUBSET of it — only the four ops `Atlas.Wasm`'s
// `atlas_debug_canvas` needs — and `WebAssembly.instantiate` ignores the extras.

export { makeWebCanvasEnv } from "../../.port/tools/web-canvas-glue.mjs";
