// gate.js — the WS14 WebGL2 pixel assertion body.
//
// ONE file, TWO drivers: the harness page (`index.html`) evaluates it as a module
// script, and `.port/tools/gates/ws14_webgl_browser_gate.mjs` sends its literal
// text through CDP `Runtime.evaluate {returnByValue: true, awaitPromise: true}`.
// Sharing the body verbatim is what makes it impossible for the gate to drift from
// what a human sees on the page.
//
// It is written as an async IIFE that EVALUATES TO a Promise of a plain JSON
// object, so `returnByValue` can marshal it. No static imports (Runtime.evaluate
// forbids them) — the glue arrives via dynamic `import()`.
//
// WHAT IS ASSERTED (see the plan's gate section):
//   1. every wasm import's module is "env"; zero /wasi/i modules
//   2. atlas_new / atlas_reserve / atlas_set all succeed (non-null / rc 0)
//   3. shader compile+link status is checked explicitly; gl.getError() === 0
//   4. per-cell ink counts from `readPixels` EQUAL an expectation computed on the
//      CPU from the atlas bytes THE MODULE HOLDS (read back through
//      `rustty_wasm_atlas_data`), and the one undrawn cell column is EXACTLY 0
//
// Exactness is a design property, not luck: `antialias:false`, R8 texture with
// UNPACK_ALIGNMENT 1, NEAREST min/mag, integer-aligned quads, 1:1 texel:pixel.
//
// NEVER call alert/confirm/prompt here — a modal wedges the automation channel.
(async () => {
  const WASM_URL = "/target/wasm32-unknown-unknown/release/rustty_wasm.wasm";
  const CELL = 32; // px pitch of one cell column
  const CELLS = 3; // 2 drawn + 1 deliberately undrawn (NC3)
  const INK = 16; // a pixel counts as ink when its value exceeds this

  const log = [];
  const say = (m) => {
    log.push(m);
    const el =
      typeof document !== "undefined" && document.getElementById("log");
    if (el) el.textContent += m + "\n";
  };

  // ---------------------------------------------------------------- wasm module
  const mod = await WebAssembly.compileStreaming(fetch(WASM_URL));
  const imports = WebAssembly.Module.imports(mod);
  const badModules = imports
    .filter((i) => i.module !== "env")
    .map((i) => i.module);
  const wasiImports = imports.filter(
    (i) => /wasi/i.test(i.module) || /wasi/i.test(i.name),
  ).length;
  say(
    `wasm imports: ${imports.length} (${[...new Set(imports.map((i) => i.module))].join(",")})`,
  );

  const { makeWebCanvasEnv } = await import("/example/wasm-webgl/glue.mjs");
  let inst;
  const env = makeWebCanvasEnv({
    memory: () => inst.exports.memory,
    // `rustty_wc_canvas_new` — what `atlas_debug_canvas` calls. On the page the
    // canvas is APPENDED to the `#atlas` section: a detached element is
    // unreachable from `document.querySelectorAll`, so leaving it detached
    // rendered that section blank while the handle still came back live.
    // Headlessly (`Runtime.evaluate` in a page that has no `#atlas`, and the
    // OffscreenCanvas branch when there is no document at all) nothing is shown
    // and nothing needs to be.
    createCanvas: () => {
      if (typeof document === "undefined") return new OffscreenCanvas(1, 1);
      const c = document.createElement("canvas");
      const host = document.getElementById("atlas");
      if (host) host.append(c);
      return c;
    },
  });
  inst = await WebAssembly.instantiate(mod, { env });
  const ex = inst.exports;
  const u8 = () => new Uint8Array(ex.memory.buffer);
  const u32 = () => new Uint32Array(ex.memory.buffer);

  // ------------------------------------------------------------------- glyph set
  // Deterministic 12x16 "glyph" coverage bitmaps. They are written INTO the wasm
  // atlas through `atlas_set`; the expectation is then read back out of the atlas,
  // so the numbers below are never themselves the expectation.
  const GW = 12;
  const GH = 16;
  const glyphBitmap = (kind) => {
    const b = new Uint8Array(GW * GH);
    const put = (x, y, v) => {
      if (x >= 0 && x < GW && y >= 0 && y < GH) b[y * GW + x] = v;
    };
    if (kind === "A") {
      // a filled triangle plus a crossbar
      for (let y = 0; y < GH; y++) {
        const half = Math.round(((y + 1) / GH) * (GW / 2));
        for (let x = GW / 2 - half; x < GW / 2 + half; x++) put(x, y, 255);
      }
      for (let x = 2; x < GW - 2; x++) put(x, GH - 6, 128);
    } else {
      // two stacked rounded blocks with a hollow centre
      for (let y = 1; y < GH - 1; y++) {
        for (let x = 1; x < GW - 3; x++) {
          const hollow = (y > 3 && y < 6) || (y > 10 && y < 13);
          put(x, y, hollow && x > 3 && x < GW - 5 ? 0 : 200);
        }
      }
    }
    return b;
  };
  const glyphs = [glyphBitmap("A"), glyphBitmap("B")];

  // -------------------------------------------------------------- fill the atlas
  const ATLAS_SIZE = 128;
  const atlas = ex.atlas_new(ATLAS_SIZE, 0 /* Format.grayscale */);
  if (atlas === 0) throw new Error("atlas_new returned null");

  const rects = [];
  for (const bmp of glyphs) {
    const reg = ex.atlas_reserve(atlas, GW, GH);
    if (reg === 0) throw new Error("atlas_reserve returned null");
    // Region is `repr(C) { x, y, width, height }` — four u32.
    const r = u32().subarray(reg >>> 2, (reg >>> 2) + 4);
    const rect = { x: r[0], y: r[1], w: r[2], h: r[3] };
    if (!(rect.w > 0 && rect.h > 0))
      throw new Error(`bad region ${JSON.stringify(rect)}`);

    const buf = ex.malloc(bmp.length);
    if (buf === 0) throw new Error("malloc returned null");
    u8().set(bmp, buf);
    ex.atlas_set(atlas, reg, buf, bmp.length);
    ex.free(buf);
    rects.push(rect);
  }
  say(`atlas ${ATLAS_SIZE}x${ATLAS_SIZE}, regions ${JSON.stringify(rects)}`);

  // Read the atlas back OUT OF THE MODULE (never a JS copy of what we wrote).
  // `.slice()` because a later allocation can detach the buffer.
  const lenPtr = ex.malloc(4);
  const dataPtr = ex.rustty_wasm_atlas_data(atlas, lenPtr);
  const dataLen = u32()[lenPtr >>> 2];
  ex.free(lenPtr);
  const atlasBytes = u8().slice(dataPtr, dataPtr + dataLen);
  const atlasSize = ex.rustty_wasm_atlas_size(atlas);
  if (atlasSize !== ATLAS_SIZE)
    throw new Error(`atlas size ${atlasSize} != ${ATLAS_SIZE}`);
  if (dataLen !== ATLAS_SIZE * ATLAS_SIZE)
    throw new Error(`atlas data len ${dataLen}`);

  // THE EXPECTATION: ink counted on the CPU, in the atlas the module holds.
  const expect = rects.map((r) => {
    let n = 0;
    for (let y = 0; y < r.h; y++) {
      for (let x = 0; x < r.w; x++) {
        if (atlasBytes[(r.y + y) * atlasSize + (r.x + x)] > INK) n++;
      }
    }
    return n;
  });
  say(`expect (CPU, from the module's atlas): [${expect.join(", ")}]`);

  // ---------------------------------------------- product-sensitivity assertions
  // The equality assertion below compares two views of the SAME atlas bytes, so on
  // its own it exercises the JS/WebGL sampling path and not the ported atlas code.
  // These four make the gate genuinely sensitive to `Atlas::reserve`/`Atlas::set`:
  //
  //   P1  each glyph must have actually landed in the atlas (non-zero ink), so a
  //       `set` that writes nothing cannot pass with expect == [0, 0]
  //   P2  the two reserved regions must not overlap, so a `reserve` that hands out
  //       the same rect twice is caught
  //   P3  the ink in the WHOLE atlas must equal the sum of the per-region inks —
  //       i.e. `set` wrote NOTHING outside the rect it was given. This is what
  //       catches a wrong destination offset or a wrong row stride, which would
  //       otherwise shift the expectation and the sampled rect together and stay
  //       invisible to the equality assertion.
  //   P4  the two distinct bitmaps must produce distinct counts, so a `set` that
  //       ignores its region and always writes the same place is caught
  const totalInk = atlasBytes.reduce((n, v) => n + (v > INK ? 1 : 0), 0);
  const expectSum = expect.reduce((a, b) => a + b, 0);
  const overlap = (a, b) =>
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
  const product = {
    p1_glyphs_nonzero: expect.every((e) => e > 0),
    p2_regions_disjoint: !overlap(rects[0], rects[1]),
    p3_no_ink_outside_regions: totalInk === expectSum,
    p4_glyphs_distinct: expect[0] !== expect[1],
    totalInk,
    expectSum,
  };
  say(
    `product: nonzero=${product.p1_glyphs_nonzero} disjoint=${product.p2_regions_disjoint} ` +
      `atlasInk=${totalInk} sum(expect)=${expectSum} distinct=${product.p4_glyphs_distinct}`,
  );

  // ------------------------------------------------------------------- the WebGL
  const VS = `#version 300 es
in vec2 a_pos;      // clip space
in vec2 a_uv;       // atlas texel coords, normalized
out vec2 v_uv;
void main() { v_uv = a_uv; gl_Position = vec4(a_pos, 0.0, 1.0); }`;

  const FS_GOOD = `#version 300 es
precision highp float;
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 o;
void main() { float a = texture(u_tex, v_uv).r; o = vec4(a, a, a, 1.0); }`;

  // NC2: a plausible "sampler unbound / wrong swizzle" bug.
  const FS_BLACK = `#version 300 es
precision highp float;
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 o;
void main() { o = vec4(0.0, 0.0, 0.0, 1.0); }`;

  function render(mutate) {
    const canvas =
      typeof document !== "undefined"
        ? document.getElementById("gl") || document.createElement("canvas")
        : new OffscreenCanvas(CELL * CELLS, CELL);
    canvas.width = CELL * CELLS;
    canvas.height = CELL;
    const gl = canvas.getContext("webgl2", {
      antialias: false,
      preserveDrawingBuffer: true,
      premultipliedAlpha: false,
    });
    if (!gl) throw new Error("webgl2 unavailable");

    const sh = (type, src) => {
      const s = gl.createShader(type);
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
        throw new Error("shader compile failed: " + gl.getShaderInfoLog(s));
      }
      return s;
    };
    const prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS));
    gl.attachShader(
      prog,
      sh(gl.FRAGMENT_SHADER, mutate === "black" ? FS_BLACK : FS_GOOD),
    );
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error("program link failed: " + gl.getProgramInfoLog(prog));
    }
    gl.useProgram(prog);

    // The atlas as an R8 texture, NEAREST, tightly packed.
    const tex = gl.createTexture();
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.R8,
      atlasSize,
      atlasSize,
      0,
      gl.RED,
      gl.UNSIGNED_BYTE,
      atlasBytes,
    );
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.uniform1i(gl.getUniformLocation(prog, "u_tex"), 0);

    // One integer-aligned quad per glyph, 1:1 texel:pixel, into cell column i.
    const verts = [];
    rects.forEach((r, i) => {
      // NC1: a plausible off-by-N atlas packing bug.
      const ax = r.x + (mutate === "offset" ? 3 : 0);
      const ay = r.y + (mutate === "offset" ? 3 : 0);
      const px0 = i * CELL;
      const py0 = 0;
      const toClipX = (px) => (px / (CELL * CELLS)) * 2 - 1;
      const toClipY = (py) => 1 - (py / CELL) * 2;
      const x0 = toClipX(px0);
      const x1 = toClipX(px0 + r.w);
      const y0 = toClipY(py0);
      const y1 = toClipY(py0 + r.h);
      const u0 = ax / atlasSize;
      const u1 = (ax + r.w) / atlasSize;
      const v0 = ay / atlasSize;
      const v1 = (ay + r.h) / atlasSize;
      // two triangles
      verts.push(x0, y0, u0, v0, x1, y0, u1, v0, x0, y1, u0, v1);
      verts.push(x1, y0, u1, v0, x1, y1, u1, v1, x0, y1, u0, v1);
    });

    const vbo = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(verts), gl.STATIC_DRAW);
    const aPos = gl.getAttribLocation(prog, "a_pos");
    const aUv = gl.getAttribLocation(prog, "a_uv");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 16, 0);
    gl.enableVertexAttribArray(aUv);
    gl.vertexAttribPointer(aUv, 2, gl.FLOAT, false, 16, 8);

    gl.viewport(0, 0, CELL * CELLS, CELL);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.TRIANGLES, 0, verts.length / 4);

    const px = new Uint8Array(CELL * CELLS * CELL * 4);
    gl.readPixels(0, 0, CELL * CELLS, CELL, gl.RGBA, gl.UNSIGNED_BYTE, px);
    const glError = gl.getError();

    const counts = [];
    for (let c = 0; c < CELLS; c++) {
      let n = 0;
      for (let y = 0; y < CELL; y++) {
        for (let x = c * CELL; x < (c + 1) * CELL; x++) {
          if (px[(y * CELL * CELLS + x) * 4] > INK) n++;
        }
      }
      counts.push(n);
    }
    const renderer = gl.getParameter(gl.RENDERER);
    return { counts, glError, renderer };
  }

  const matches = (counts) =>
    expect.every((e, i) => counts[i] === e) && counts[CELLS - 1] === 0;

  // ORDER MATTERS FOR THE PAGE, NOT FOR THE ASSERTIONS. All three arms share the
  // one `#gl` canvas and each fully re-creates its GL state, so the arms are
  // independent — but whichever runs LAST is what a human sees. Run the two
  // negative controls first and the positive arm last, so the page ends showing
  // the glyphs it claims to show instead of NC2's all-black frame. (Observed:
  // with `black` last, the on-page canvas read back 0 ink while the JSON said
  // PASS — correct assertions, misleading picture.)
  const offset = render("offset");
  const black = render("black");
  const good = render(null);

  say(
    `good   (WebGL readback): [${good.counts.join(", ")}]  glError=${good.glError}`,
  );
  say(
    `NC1 offset: [${offset.counts.join(", ")}] -> ${matches(offset.counts) ? "MATCHES (BAD)" : "FAILS (good)"}`,
  );
  say(
    `NC2 black:  [${black.counts.join(", ")}] -> ${matches(black.counts) ? "MATCHES (BAD)" : "FAILS (good)"}`,
  );

  // Exercise the ported `atlas_debug_canvas` (the reference's own readback path)
  // and, on the page, show it. A zero handle means the JS bridge failed.
  let debugCanvas = 0;
  try {
    debugCanvas = ex.atlas_debug_canvas(atlas);
  } catch (e) {
    say("atlas_debug_canvas threw: " + e);
  }
  say(`atlas_debug_canvas handle = ${debugCanvas}`);

  // P5: the reference's OWN readback path must agree with the WebGL one. A live
  // handle alone asserts nothing, so read the canvas back. The grayscale arm of
  // `atlas_debug_canvas_` writes A8 into the ALPHA byte and leaves RGB at zero
  // (`buf[(i * 4) + 3] = value`), so the ink lives in the alpha channel — a
  // wrong destination byte, a wrong `ImageData` stride, or a dropped
  // `putImageData` all show up here as a count that no longer equals `totalInk`.
  let debugCanvasInk = null;
  const dbg =
    typeof document !== "undefined"
      ? document.querySelector("#atlas canvas")
      : null;
  if (dbg) {
    const scratch = document.createElement("canvas");
    scratch.width = dbg.width;
    scratch.height = dbg.height;
    const sctx = scratch.getContext("2d");
    sctx.drawImage(dbg, 0, 0);
    const dd = sctx.getImageData(0, 0, scratch.width, scratch.height).data;
    debugCanvasInk = 0;
    for (let i = 0; i < dd.length; i += 4)
      if (dd[i + 3] > INK) debugCanvasInk++;
    say(
      `atlas_debug_canvas ink (alpha) = ${debugCanvasInk} (atlas ink ${totalInk})`,
    );
  }
  const p5_debug_canvas_agrees =
    debugCanvasInk === null ? null : debugCanvasInk === totalInk;

  ex.atlas_free(atlas);

  return {
    ...product,
    expect,
    good: good.counts,
    offset: offset.counts,
    black: black.counts,
    pass_good: matches(good.counts),
    neg_offset_fails: !matches(offset.counts),
    neg_black_fails: !matches(black.counts),
    undrawn_cell_is_zero: good.counts[CELLS - 1] === 0,
    glError: good.glError,
    renderer: good.renderer,
    wasiImports,
    badImportModules: badModules,
    debugCanvas,
    debugCanvasInk,
    p5_debug_canvas_agrees,
    log,
  };
})();
