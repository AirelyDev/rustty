// ws14_webgl_browser_gate.mjs — the WS14 browser gate driver.
//
// Renders the `Atlas.Wasm` texture through WebGL2 in headless Chrome and asserts
// per-cell ink counts against a CPU expectation computed from the SAME atlas bytes
// the wasm module holds, plus TWO negative controls that MUST fail on every run.
//
// TOLERANCE RULE (verbatim, do not soften): exact equality is the gate; any future
// widening must be accompanied by a re-run proving NC1 and NC2 still FAIL under the
// widened rule.
//
// Node-only, ZERO npm dependencies (node's built-in `WebSocket` + `fetch` speak
// CDP directly). Serving is `python3 -m http.server --bind 127.0.0.1` from the repo
// root, because `fetch()` of a `.wasm` cannot work from `file://`. Nothing leaves
// loopback.
//
// Headless is not a weaker proxy: `--headless=new --enable-unsafe-swiftshader` was
// observed to produce BYTE-IDENTICAL ink counts to a live, GPU-backed Chrome on the
// same host (both the design-time probe and WS14 T10's live run).
//
// EXIT CODES
//   0  pass: the positive arm matched AND both negative controls failed
//   1  assertion failure (or a page exception / console error)
//   3  environment missing (no wasm artifact, no Chrome) — NEVER silently green
//
// Usage: .port/tools/gates/ws14_webgl_browser_gate.sh [--keep-open]

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const HERE = import.meta.dirname;
const ROOT = resolve(HERE, "..", "..", "..");
const ARTIFACT = join(ROOT, "target/wasm32-unknown-unknown/release/rustty_wasm.wasm");
const PAGE = "/example/wasm-webgl/index.html";
const GATE_JS = join(ROOT, "example/wasm-webgl/gate.js");

const CHROME_CANDIDATES = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function die(code, msg) {
  console.error(msg);
  process.exit(code);
}

// ------------------------------------------------------------------ environment
if (!existsSync(ARTIFACT)) {
  die(
    3,
    `WS14 gate: MISSING the wasm artifact\n  ${ARTIFACT}\n\n` +
      `Build it first:\n  .port/tools/build-main-wasm.sh --release\n\n` +
      `Exiting 3 (environment missing) rather than passing vacuously.`,
  );
}
const CHROME = CHROME_CANDIDATES.find((p) => existsSync(p));
if (!CHROME) {
  die(
    3,
    `WS14 gate: no Chrome/Chromium binary found. Looked in:\n` +
      CHROME_CANDIDATES.map((p) => "  " + p).join("\n") +
      `\n\nExiting 3 (environment missing) rather than passing vacuously.`,
  );
}

// ------------------------------------------------------------------------- serve
let server = null;
let chrome = null;
let ws = null;
let profile = null;

async function pickPort() {
  const net = await import("node:net");
  return new Promise((res, rej) => {
    const s = net.createServer();
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => res(port));
    });
    s.on("error", rej);
  });
}

async function main() {
  const port = await pickPort();
  server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], {
    cwd: ROOT,
    stdio: "ignore",
  });

  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; ; i++) {
    try {
      await fetch(base + PAGE, { method: "HEAD" });
      break;
    } catch {
      if (i > 100) die(3, "WS14 gate: local http.server never came up");
      await sleep(50);
    }
  }

  profile = mkdtempSync(join(tmpdir(), "ws14-chrome-"));
  chrome = spawn(
    CHROME,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      "--enable-unsafe-swiftshader",
      "--use-gl=angle",
      base + PAGE,
    ],
    { stdio: "ignore" },
  );

  // Chrome writes the real devtools port into <profile>/DevToolsActivePort line 1.
  let devPort = null;
  for (let i = 0; i < 300; i++) {
    const f = join(profile, "DevToolsActivePort");
    if (existsSync(f)) {
      const first = readFileSync(f, "utf8").split("\n")[0].trim();
      if (first) {
        devPort = first;
        break;
      }
    }
    await sleep(50);
  }
  if (!devPort) die(3, "WS14 gate: Chrome never published DevToolsActivePort");

  // Find the harness page target.
  let target = null;
  for (let i = 0; i < 300; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${devPort}/json/list`)).json();
      target = list.find((t) => t.type === "page" && t.url.includes("/example/wasm-webgl/"));
      if (target?.webSocketDebuggerUrl) break;
    } catch {
      /* keep polling */
    }
    await sleep(50);
  }
  if (!target) die(3, "WS14 gate: never found the harness page target");

  // ------------------------------------------------------------------- CDP
  const { WebSocket } = globalThis;
  if (!WebSocket) die(3, "WS14 gate: this node has no built-in WebSocket (need node >= 22)");
  ws = new WebSocket(target.webSocketDebuggerUrl);
  const pending = new Map();
  const consoleErrors = [];
  const exceptions = [];
  let id = 0;

  await new Promise((res, rej) => {
    ws.addEventListener("open", res, { once: true });
    ws.addEventListener("error", rej, { once: true });
  });

  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id !== undefined && pending.has(msg.id)) {
      const { res, rej } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) rej(new Error(JSON.stringify(msg.error)));
      else res(msg.result);
      return;
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
    }
    if (msg.method === "Runtime.exceptionThrown") {
      exceptions.push(
        msg.params.exceptionDetails.exception?.description ||
          msg.params.exceptionDetails.text ||
          "exception",
      );
    }
  });

  const send = (method, params = {}) =>
    new Promise((res, rej) => {
      const myId = ++id;
      pending.set(myId, { res, rej });
      ws.send(JSON.stringify({ id: myId, method, params }));
    });

  await send("Runtime.enable");
  await send("Page.enable");

  // Evaluate the SHARED assertion body — the very file the page evaluates.
  const src = readFileSync(GATE_JS, "utf8");
  const r = await send("Runtime.evaluate", {
    expression: src,
    returnByValue: true,
    awaitPromise: true,
    userGesture: true,
  });
  if (r.exceptionDetails) {
    throw new Error(
      "gate.js threw: " +
        (r.exceptionDetails.exception?.description || r.exceptionDetails.text),
    );
  }
  const v = r.result.value;

  // ---------------------------------------------------------------- the verdict
  const pad = (s) => String(s).padEnd(28);
  console.log("WS14 WebGL2 browser gate");
  console.log("  renderer                  ", v.renderer);
  console.log("  wasm imports non-`env`    ", JSON.stringify(v.badImportModules));
  console.log("  wasi imports              ", v.wasiImports);
  console.log("  atlas_debug_canvas handle ", v.debugCanvas);
  console.log("");
  console.log("  " + pad("expect (CPU, from atlas)") + JSON.stringify(v.expect));
  console.log("  " + pad("good   (WebGL readback)") + JSON.stringify(v.good));
  console.log("  " + pad("NC1 offset (+3px UV)") + JSON.stringify(v.offset));
  console.log("  " + pad("NC2 black shader") + JSON.stringify(v.black));
  console.log("");
  console.log("  pass_good                 ", v.pass_good);
  console.log("  neg_offset_fails          ", v.neg_offset_fails);
  console.log("  neg_black_fails           ", v.neg_black_fails);
  console.log("  undrawn_cell_is_zero      ", v.undrawn_cell_is_zero);
  console.log("  glError                   ", v.glError);

  console.log("");
  console.log("  P1 glyphs non-zero ink    ", v.p1_glyphs_nonzero);
  console.log("  P2 regions disjoint       ", v.p2_regions_disjoint);
  console.log(
    "  P3 no ink outside regions ",
    v.p3_no_ink_outside_regions,
    `(atlas ink ${v.totalInk} vs sum(expect) ${v.expectSum})`,
  );
  console.log("  P4 glyph counts distinct  ", v.p4_glyphs_distinct);
  console.log(
    "  P5 debug canvas agrees    ",
    v.p5_debug_canvas_agrees,
    `(atlas_debug_canvas alpha ink ${v.debugCanvasInk} vs atlas ink ${v.totalInk})`,
  );

  const problems = [];
  // The P* arms are what make the gate sensitive to the PORTED atlas code rather
  // than only to the JS/WebGL sampling path — see gate.js's comment.
  if (!v.p1_glyphs_nonzero) problems.push("P1: a glyph left NO ink in the atlas (atlas_set no-op?)");
  if (!v.p2_regions_disjoint) problems.push("P2: atlas_reserve handed out overlapping regions");
  if (!v.p3_no_ink_outside_regions)
    problems.push(
      `P3: atlas ink ${v.totalInk} != sum of per-region ink ${v.expectSum} — ` +
        "atlas_set wrote outside the region it was given (wrong offset or stride)",
    );
  if (!v.p4_glyphs_distinct) problems.push("P4: the two distinct glyphs produced identical counts");
  // P5 is `null` only if the page had no `#atlas` host to attach the debug canvas
  // to — which cannot happen through this driver (it always loads index.html), so
  // a null here is itself a defect rather than a skip.
  if (v.p5_debug_canvas_agrees !== true)
    problems.push(
      `P5: atlas_debug_canvas readback ink ${v.debugCanvasInk} != atlas ink ${v.totalInk} — ` +
        "the reference's own readback path disagrees with the WebGL one " +
        "(wrong destination byte, stride, or a dropped putImageData)",
    );
  if (!v.pass_good) problems.push("positive arm did NOT match the CPU expectation");
  if (!v.neg_offset_fails) problems.push("NC1 (wrong atlas offset) did NOT fail — vacuous gate");
  if (!v.neg_black_fails) problems.push("NC2 (black shader) did NOT fail — vacuous gate");
  if (!v.undrawn_cell_is_zero) problems.push("NC3: the undrawn cell column carried ink");
  if (v.glError !== 0) problems.push(`glError ${v.glError}`);
  if (v.wasiImports !== 0) problems.push(`${v.wasiImports} wasi imports`);
  if (v.badImportModules.length) problems.push(`non-env imports: ${v.badImportModules}`);
  if (v.debugCanvas === 0) problems.push("atlas_debug_canvas returned 0 (JS canvas bridge failed)");
  if (consoleErrors.length) problems.push("console errors: " + consoleErrors.join(" | "));
  if (exceptions.length) problems.push("page exceptions: " + exceptions.join(" | "));

  if (problems.length) {
    console.error("\nWS14 GATE: RED");
    for (const p of problems) console.error("  - " + p);
    return 1;
  }
  console.log("\nWS14 GATE: GREEN (positive matched; both negative controls failed)");
  return 0;
}

let code = 1;
try {
  code = await main();
} catch (e) {
  console.error("WS14 GATE: RED — " + (e?.stack || e));
  code = 1;
} finally {
  try {
    ws?.close();
  } catch {
    /* ignore */
  }
  chrome?.kill();
  server?.kill();
  if (profile) {
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
}
process.exit(code);
