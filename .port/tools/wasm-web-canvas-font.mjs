// wasm-web-canvas-font.mjs — EXECUTE the web_canvas face's `context()` (the CSS
// `ctx.font` it sets, and the read-back check after it) in the DEBUG
// rustty_web_canvas.wasm, against the headless mock canvas.
//
// Parity rows (task T-F):
//   S91-F2  `web_canvas.zig:310-314` formats the size with `{d}` on an f32, so
//           13.5 points is "13.5px <font>". The port printed "13px <font>".
//   S91-F5  `web_canvas.zig:316-324` reads the WHOLE `ctx.font` back
//           (`getAlloc`) and warns only when it differs. The port read at most
//           256 bytes, so a longer font always "differed" and warned.
//
// Run:
//   .port/tools/build-web-canvas-wasm.sh            # DEBUG, no --release
//   node .port/tools/wasm-web-canvas-font.mjs
//
// DEBUG ARTIFACT ONLY, as for wasm-log-emission.mjs: the warn is stripped from
// a release build, and the arms below assert on it.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { makeWebCanvasEnv } from "./web-canvas-glue.mjs";
import { makeMockCanvas } from "./web-canvas-mock.mjs";

const WASM = fileURLToPath(
  new URL("../../target/wasm32-unknown-unknown/debug/rustty_web_canvas.wasm", import.meta.url),
);
const WARN_MARKER = "canvas font didn't set, fonts may be broken";

let bytes;
try {
  bytes = readFileSync(WASM);
} catch {
  console.error(`BLOCKED: wasm artifact not found: ${WASM}`);
  console.error(`         Build it with: .port/tools/build-web-canvas-wasm.sh   (NO --release)`);
  process.exit(3);
}
if (!Buffer.from(bytes).includes(WARN_MARKER)) {
  console.error(`FAIL: ${WASM} lacks ${JSON.stringify(WARN_MARKER)}: a RELEASE artifact.`);
  process.exit(1);
}
const mod = await WebAssembly.compile(bytes);

const fail = (m) => {
  console.error(`FAIL: ${m}`);
  process.exit(1);
};

// A mock canvas whose `ctx.font` setter records every value, and whose getter
// optionally rewrites what it returns (as a browser that normalizes the
// shorthand would).
function recordingCanvas(sets, rewrite) {
  const base = makeMockCanvas();
  return () => {
    const c = base();
    const ctx = c.getContext();
    const desc = Object.getOwnPropertyDescriptor(ctx, "font");
    Object.defineProperty(ctx, "font", {
      set(v) {
        sets.push(v);
        desc.set.call(ctx, v);
      },
      get() {
        const v = desc.get.call(ctx);
        return rewrite ? rewrite(v) : v;
      },
    });
    return c;
  };
}

async function loadFace(fontStr, points, rewrite) {
  const sets = [];
  const logs = [];
  let inst;
  const dec = new TextDecoder();
  const env = {
    ...makeWebCanvasEnv({
      memory: () => inst.exports.memory,
      createCanvas: recordingCanvas(sets, rewrite),
    }),
    log(ptr, len) {
      logs.push(dec.decode(new Uint8Array(inst.exports.memory.buffer, ptr, len)));
    },
  };
  inst = await WebAssembly.instantiate(mod, { env });
  const ex = inst.exports;
  const raw = new TextEncoder().encode(fontStr);
  const p = ex.rustty_wc_alloc(raw.length);
  new Uint8Array(ex.memory.buffer).set(raw, p);
  const df = ex.rustty_wc_deferred_new(p, raw.length, 0);
  ex.rustty_wc_free(p, raw.length);
  if (df === 0) fail("rustty_wc_deferred_new returned null");
  const face = ex.rustty_wc_deferred_load(df, points);
  if (face === 0) fail(`rustty_wc_deferred_load(${points}) returned null`);
  ex.rustty_wc_face_free(face);
  ex.rustty_wc_deferred_free(df);
  return { sets, logs };
}

// --- S91-F2: the size is formatted as Zig's {d} formats an f32 ---------------
{
  const { sets } = await loadFace("monospace", 13.5);
  if (!sets.includes("13.5px monospace"))
    fail(`13.5 points: expected ctx.font = "13.5px monospace", got sets ${JSON.stringify(sets)}`);
  if (sets.includes("13px monospace")) fail(`13.5 points was truncated to "13px monospace"`);
  const whole = await loadFace("monospace", 12);
  if (!whole.sets.includes("12px monospace"))
    fail(`12 points: expected ctx.font = "12px monospace", got ${JSON.stringify(whole.sets)}`);
  console.log(`S91-F2: OK — 13.5 points sets "13.5px monospace", 12 points "12px monospace"`);
}

// --- S91-F5: a font value longer than 256 bytes reads back whole ------------
const LONG = "monospace-" + "x".repeat(290); // 300 bytes; "12px " + LONG is 305
{
  const { sets, logs } = await loadFace(LONG, 12);
  const want = `12px ${LONG}`;
  if (!sets.includes(want)) fail(`the long font was not set: ${JSON.stringify(sets.map((s) => s.length))}`);
  const warned = logs.filter((m) => m.includes(WARN_MARKER));
  if (warned.length !== 0) fail(`a ${want.length}-byte font that DID set still warned: ${JSON.stringify(warned)}`);
  console.log(`S91-F5: OK — a ${want.length}-byte ctx.font read back equal; no warning`);
}

// --- control: the check still fires, and reports the whole value it read ----
// The rewritten value is 3 bytes longer than what was set, so the first read
// (sized for the set value plus one) is short and the second read runs.
{
  const { logs } = await loadFace(LONG, 12, (v) => v + "!!!");
  const want = `12px ${LONG}`;
  const expected = `warning(font_face): ${WARN_MARKER}, expected=${want} got=${want}!!!`;
  if (!logs.includes(expected))
    fail(
      `a read-back that differs must warn with the full value.\n` +
        `      expected: ${JSON.stringify(expected)}\n      captured: ${JSON.stringify(logs)}`,
    );
  console.log(`control: OK — a differing ${want.length + 3}-byte read-back (longer than the first read's buffer) warns, quoting all of it`);
}

console.log("\nWASM WEB_CANVAS FONT: PASS");
