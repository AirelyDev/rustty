// glue.mjs — the `env` import object for `rustty_wasm.wasm`.
//
// This file exists so that the harness page and the WS14 gate load IDENTICAL glue
// through one stable relative path: the page imports it as a module, and
// `gate.js` reaches it with a dynamic `import()` (so the same assertion body works
// verbatim from CDP `Runtime.evaluate`, which cannot do a static import).
//
// The whole `env` object is the `rustty_wc_*` Canvas FFI the port already ships for
// the `web_canvas` font face (`.port/tools/web-canvas-glue.mjs`), PLUS the `log`
// sink that `os/wasm/log.zig` imports (`extern "env" fn log(ptr, len)`) and that
// the config wasm helpers log their errors through. `rustty_wasm.wasm` imports a
// strict SUBSET of the Canvas ops — only the four `Atlas.Wasm`'s
// `atlas_debug_canvas` needs — and `WebAssembly.instantiate` ignores the extras;
// `env.log` however is a REQUIRED import of the artifact, so it must be provided
// here or instantiation fails.

import { makeWebCanvasEnv as makeCanvasOnlyEnv } from "../../.port/tools/web-canvas-glue.mjs";

/// The `env.log` host sink — the same `(ptr, len)` -> `console.log` shape the
/// other wasm examples use (`example/wasm-vt/index.html`, `wasm-sgr`,
/// `wasm-key-encode`), which is how the reference's browser harnesses bind
/// `os/wasm/log.zig`'s import.
export function makeWebCanvasEnv({ memory, createCanvas }) {
  return {
    ...makeCanvasOnlyEnv({ memory, createCanvas }),
    log: (ptr, len) => {
      const bytes = new Uint8Array(memory().buffer, ptr, len);
      const text = new TextDecoder().decode(bytes);
      console.log("[wasm]", text);
    },
  };
}
