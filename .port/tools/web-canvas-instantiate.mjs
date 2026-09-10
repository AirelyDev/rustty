// web-canvas-instantiate.mjs — headless verification of the freestanding
// (wasm32-unknown-unknown) rustty-web-canvas module. Proves, as far as is
// possible without a real Chrome:
//   (a) the module has ZERO wasi imports (only the `env` Canvas imports),
//   (b) it INSTANTIATES with a MOCK Canvas import object,
//   (c) the font-shaper web_canvas exports are CALLABLE without trapping:
//       - create a Canvas `Face` (drives measureText/getContext via the mock),
//       - render a glyph into an `Atlas` (drives fillText/getImageData),
//       - create a `Shaper` and shape a sample string.
//
// The CONTROLLER opens example/wasm-web-canvas/index.html in a real Chrome for
// the visual/interactive proof; this script is the headless CI-able check.
//
// Run:  node .port/tools/web-canvas-instantiate.mjs [--debug]
//
// !! NO GATE RUNS THIS SCRIPT. It is the standing evidence for `W9F-03`/`W9F-04`
// (`.port/phase-2/DEFERRALS.md`) and the only headless check of the freestanding
// link, but nothing in `.port/tools/ci.sh` invokes it — so when the artifact grew
// an `env.log` import (`9a41c0c2`, this plan's own wasm log backend) that
// `makeWebCanvasEnv` did not supply, every check below sat dead behind a
// `LinkError` and nobody noticed. See `.port/parity-closure/NEW-FINDINGS.md` N10.
// The glue is fixed; the GATING is not. Wiring this into `run_webgl_lane` beside
// `wasm-log-emission.mjs` (which IS gated, for exactly this reason) is three
// lines and is the open residual of N10.
import { readFileSync } from "node:fs";
import { makeWebCanvasEnv } from "./web-canvas-glue.mjs";
import { makeMockCanvas } from "./web-canvas-mock.mjs";

const WASM = new URL(
  "../../target/wasm32-unknown-unknown/release/rustty_web_canvas.wasm",
  import.meta.url,
);

const mod = await WebAssembly.compile(readFileSync(WASM));

// (a) prove the freestanding link: 0 wasi imports, only `env`.
const imports = WebAssembly.Module.imports(mod);
const wasi = imports.filter((i) => /wasi/i.test(i.module));
const nonEnv = imports.filter((i) => i.module !== "env");
if (wasi.length !== 0) throw new Error("FAIL: wasi imports present: " + wasi.length);
if (nonEnv.length !== 0)
  throw new Error("FAIL: non-env imports: " + JSON.stringify(nonEnv.map((i) => i.module)));
console.log(
  `imports: OK — 0 wasi, ${imports.length} env Canvas imports only`,
);

// (b) instantiate with the MOCK Canvas import object.
let inst;
const env = makeWebCanvasEnv({
  memory: () => inst.exports.memory,
  createCanvas: makeMockCanvas(),
});
inst = await WebAssembly.instantiate(mod, { env });
const ex = inst.exports;
console.log("instantiate: OK (mock Canvas env)");

const enc = new TextEncoder();
const memU8 = () => new Uint8Array(ex.memory.buffer);

function writeBytes(bytes) {
  const p = ex.rustty_wc_alloc(bytes.length);
  memU8().set(bytes, p);
  return p;
}

// (c1) create a Canvas Face (presentation 0 = text) at 16px.
const fontBytes = enc.encode("monospace");
const fptr = writeBytes(fontBytes);
const face = ex.rustty_wc_face_new(fptr, fontBytes.length, 16.0, 0);
ex.rustty_wc_free(fptr, fontBytes.length);
if (face === 0) throw new Error("FAIL: rustty_wc_face_new returned null");
console.log("face_new: OK (handle=" + face + ")");

// (c2) render glyph 'A' into a grayscale atlas.
const atlas = ex.rustty_wc_atlas_new(512, 0); // 0 = grayscale
if (atlas === 0) throw new Error("FAIL: atlas_new returned null");
const glyphBuf = ex.rustty_wc_alloc(64); // >= sizeof(Glyph)
const rc = ex.rustty_wc_face_render_glyph(face, atlas, "A".codePointAt(0), glyphBuf);
if (rc !== 0) throw new Error("FAIL: rustty_wc_face_render_glyph rc=" + rc);
const gv = new DataView(ex.memory.buffer, glyphBuf, 24);
console.log(
  "face_render_glyph 'A': OK (width=" +
    gv.getUint32(0, true) +
    " height=" +
    gv.getUint32(4, true) +
    ")",
);
ex.rustty_wc_free(glyphBuf, 64);

// prove atlas received bytes.
const outLenP = ex.rustty_wc_alloc(4);
const dataPtr = ex.rustty_wc_atlas_data(atlas, outLenP);
const atlasLen = new DataView(ex.memory.buffer, outLenP, 4).getUint32(0, true);
let nonzero = 0;
{
  const d = new Uint8Array(ex.memory.buffer, dataPtr, atlasLen);
  for (let i = 0; i < d.length; i++) if (d[i] !== 0) nonzero++;
}
ex.rustty_wc_free(outLenP, 4);
console.log("atlas: OK (" + atlasLen + " bytes, " + nonzero + " non-zero glyph pixels)");

// (c3) create a Shaper and shape a sample string.
const shaper = ex.rustty_wc_shaper_new(80);
if (shaper === 0) throw new Error("FAIL: rustty_wc_shaper_new returned null");
const text = "Hi!";
const cps = [...text].map((ch) => ch.codePointAt(0));
const n = cps.length;
const cpsPtr = ex.rustty_wc_alloc(n * 4);
const clPtr = ex.rustty_wc_alloc(n * 4);
{
  const cpsV = new Uint32Array(ex.memory.buffer, cpsPtr, n);
  const clV = new Uint32Array(ex.memory.buffer, clPtr, n);
  for (let i = 0; i < n; i++) {
    cpsV[i] = cps[i];
    clV[i] = i;
  }
}
const outCells = ex.rustty_wc_alloc(n * 16);
// grid=0 (null): single-codepoint cells need no font grid.
const count = ex.rustty_wc_shaper_shape(shaper, 0, cpsPtr, clPtr, n, outCells, n);
if (count < 0) throw new Error("FAIL: rustty_wc_shaper_shape rc=" + count);
console.log('shaper_shape "' + text + '": OK (' + count + " cells produced)");

// (c4) the web_canvas `DeferredFace` arms (familyName / name / load /
// hasCodepoint). These are ASSERTED, not merely "did not trap": the payload
// carries a known font string and a known presentation, so a regression in any
// arm changes an observable value.
{
  const dfBytes = enc.encode("bold 1em monospace");
  const dfPtr = writeBytes(dfBytes);
  // presentation 0 = text.
  const df = ex.rustty_wc_deferred_new(dfPtr, dfBytes.length, 0);
  ex.rustty_wc_free(dfPtr, dfBytes.length);
  if (df === 0) throw new Error("FAIL: rustty_wc_deferred_new returned null");

  const readInto = (fn) => {
    const cap = 128;
    const p = ex.rustty_wc_alloc(cap);
    const len = fn(df, p, cap);
    if (len === 0xffffffff || len > cap)
      throw new Error("FAIL: deferred name/family_name error (len=" + len + ")");
    const s = new TextDecoder().decode(memU8().slice(p, p + len));
    ex.rustty_wc_free(p, cap);
    return s;
  };

  const nm = readInto(ex.rustty_wc_deferred_name);
  if (nm !== "bold 1em monospace")
    throw new Error("FAIL: deferred name = " + JSON.stringify(nm));
  const fam = readInto(ex.rustty_wc_deferred_family_name);
  if (fam !== "bold 1em monospace")
    throw new Error("FAIL: deferred family_name = " + JSON.stringify(fam));
  console.log("deferred name/family_name: OK (" + JSON.stringify(nm) + ")");

  // hasCodepoint: presentation 2 = null (`?Presentation` = none) -> slow path.
  const cpA = "A".codePointAt(0);
  if (ex.rustty_wc_deferred_has_codepoint(df, cpA, 2) !== 1)
    throw new Error("FAIL: deferred has_codepoint('A', null) should be true");
  // Matching presentation (text) -> still true.
  if (ex.rustty_wc_deferred_has_codepoint(df, cpA, 0) !== 1)
    throw new Error("FAIL: deferred has_codepoint('A', text) should be true");
  // MISMATCHED presentation (emoji vs a text payload) -> the reference's
  // fast-path returns false WITHOUT initializing a face.
  if (ex.rustty_wc_deferred_has_codepoint(df, cpA, 1) !== 0)
    throw new Error("FAIL: deferred has_codepoint('A', emoji) should be false");
  console.log("deferred has_codepoint: OK (null=true, text=true, emoji=false)");

  // load(): yields a real Canvas Face carrying the same font string.
  const loaded = ex.rustty_wc_deferred_load(df, 14.0);
  if (loaded === 0) throw new Error("FAIL: rustty_wc_deferred_load returned null");
  ex.rustty_wc_face_free(loaded);
  console.log("deferred load: OK (handle=" + loaded + ")");

  ex.rustty_wc_deferred_free(df);
}

// (c5) ATLAS BYTE ORDER (W9F-03). Canvas `getImageData` is RGBA; the `Atlas`
// format 2 is BGRA (and is uploaded as a BGRA texture by the GL/Metal
// renderers), so the face MUST swizzle R<->B. A fresh instance is used whose
// mock canvas paints every pixel opaque RED — a non-symmetric color, so RGBA
// (255,0,0,255) and BGRA (0,0,255,255) are distinguishable.
{
  let inst2;
  const env2 = makeWebCanvasEnv({
    memory: () => inst2.exports.memory,
    createCanvas: makeMockCanvas({ pixel: [255, 0, 0, 255] }),
  });
  inst2 = await WebAssembly.instantiate(mod, { env: env2 });
  const e2 = inst2.exports;

  const fb = enc.encode("monospace");
  const fp = e2.rustty_wc_alloc(fb.length);
  new Uint8Array(e2.memory.buffer).set(fb, fp);
  const f2 = e2.rustty_wc_face_new(fp, fb.length, 16.0, 0);
  e2.rustty_wc_free(fp, fb.length);
  if (f2 === 0) throw new Error("FAIL: face_new(monospace) returned null (BGRA case)");

  const a2 = e2.rustty_wc_atlas_new(512, 2); // 2 = BGRA
  if (a2 === 0) throw new Error("FAIL: atlas_new(BGRA) returned null");
  const gb = e2.rustty_wc_alloc(64);
  const rc2 = e2.rustty_wc_face_render_glyph(f2, a2, "A".codePointAt(0), gb);
  if (rc2 !== 0) throw new Error("FAIL: render_glyph into a BGRA atlas rc=" + rc2);
  e2.rustty_wc_free(gb, 64);

  const lp = e2.rustty_wc_alloc(4);
  const dp = e2.rustty_wc_atlas_data(a2, lp);
  const alen = new DataView(e2.memory.buffer, lp, 4).getUint32(0, true);
  const d = new Uint8Array(e2.memory.buffer, dp, alen);
  let px = null;
  for (let i = 0; i + 4 <= d.length; i += 4) {
    if (d[i] || d[i + 1] || d[i + 2] || d[i + 3]) {
      px = [d[i], d[i + 1], d[i + 2], d[i + 3]];
      break;
    }
  }
  e2.rustty_wc_free(lp, 4);
  if (px === null) throw new Error("FAIL: BGRA atlas has no glyph pixels");
  const want = [0, 0, 255, 255];
  if (px.join(",") !== want.join(","))
    throw new Error(
      "FAIL: BGRA atlas stores [" +
        px.join(",") +
        "]; opaque red must be stored as [" +
        want.join(",") +
        "] (RGBA bytes handed through unconverted?)",
    );
  console.log("atlas byte order: OK (canvas RGBA red -> BGRA [0,0,255,255])");

  e2.rustty_wc_face_free(f2);
  e2.rustty_wc_atlas_free(a2);
}

// (c6) FONT-FILE BYTES ARE NOT A CSS FONT (W9F-04). The Canvas face takes a CSS
// "font" value; handing it real font FILE bytes (what the modern generic
// `Face::init` loader passes on every other backend) must FAIL, not silently
// succeed on whatever font the canvas already had.
{
  const ttf = readFileSync(
    new URL("../../crates/rustty-font/res/JetBrainsMono-Variable.ttf", import.meta.url),
  ).subarray(0, 256);
  const bp = writeBytes(ttf);
  const bad = ex.rustty_wc_face_new(bp, ttf.length, 16.0, 0);
  ex.rustty_wc_free(bp, ttf.length);
  if (bad !== 0) {
    ex.rustty_wc_face_free(bad);
    throw new Error(
      "FAIL: face_new accepted embedded TTF bytes as a CSS font value " +
        "(the canvas silently keeps its fallback font)",
    );
  }
  // Control: a real CSS value still works on the same instance.
  const okBytes = enc.encode("monospace");
  const op = writeBytes(okBytes);
  const okFace = ex.rustty_wc_face_new(op, okBytes.length, 16.0, 0);
  ex.rustty_wc_free(op, okBytes.length);
  if (okFace === 0) throw new Error("FAIL: face_new(monospace) control returned null");
  ex.rustty_wc_face_free(okFace);
  console.log("face_new(font-file bytes): OK (rejected; monospace control accepted)");
}

// cleanup
ex.rustty_wc_free(cpsPtr, n * 4);
ex.rustty_wc_free(clPtr, n * 4);
ex.rustty_wc_free(outCells, n * 16);
ex.rustty_wc_shaper_free(shaper);
ex.rustty_wc_face_free(face);
ex.rustty_wc_atlas_free(atlas);

console.log(
  "\nALL OK: freestanding wasm (0 wasi) instantiated with a mock Canvas; " +
    "web_canvas face + atlas render + shaper all callable without trapping.",
);
