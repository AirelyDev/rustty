// web-canvas-mock.mjs — a headless (no-DOM) mock of the browser 2d-canvas API,
// so the freestanding wasm module can be instantiated + exercised in node
// WITHOUT a real Chrome. Returns a `createCanvas` factory for
// `makeWebCanvasEnv`. The mock produces plausible `TextMetrics` and `ImageData`
// so the web_canvas face's measure/render path runs end-to-end.
//
// This is a MOCK for headless verification only; the real browser harness
// (example/wasm-web-canvas/index.html) uses actual `document.createElement`.

// Options:
//   pixel: [r, g, b, a] — when given, `getImageData` returns EVERY pixel with
//     this RGBA value (canvas ImageData is always RGBA) instead of the default
//     alpha ramp. Used to assert the atlas byte ORDER for a BGRA atlas: a
//     non-symmetric color such as opaque red distinguishes RGBA from BGRA.
export function makeMockCanvas({ pixel } = {}) {
  return () => {
    const c = { width: 0, height: 0, style: {} };
    let font = "10px monospace";
    let fillStyle = "black";
    let textBaseline = "alphabetic";
    const ctx = {
      canvas: c,
      set font(v) {
        // Browsers PARSE the CSS "font" shorthand and IGNORE an invalid value,
        // leaving the previous one in place (that is why the reference face
        // reads the property back and warns when it didn't take). Model that:
        // anything with a control character (e.g. raw font-FILE bytes) is not a
        // CSS value, so the assignment is a no-op.
        if (/[\u0000-\u001f\u007f]/.test(v)) return;
        font = v;
      },
      get font() {
        return font;
      },
      set fillStyle(v) {
        fillStyle = v;
      },
      get fillStyle() {
        return fillStyle;
      },
      set textBaseline(v) {
        textBaseline = v;
      },
      get textBaseline() {
        return textBaseline;
      },
      clearRect() {},
      fillRect() {},
      fillText() {},
      measureText(s) {
        const size = parseInt(font, 10) || 10;
        const w = s.length * size * 0.6;
        return {
          width: w,
          actualBoundingBoxLeft: 0,
          actualBoundingBoxRight: w,
          actualBoundingBoxAscent: size * 0.75,
          actualBoundingBoxDescent: size * 0.2,
        };
      },
      getImageData(x, y, w, h) {
        // Mock a rendered glyph: a grayscale wedge in the alpha channel so the
        // face's presentation inspection + atlas store have real bytes to copy.
        // ImageData is ALWAYS RGBA (the byte order the face has to convert from).
        const data = new Uint8ClampedArray(w * h * 4);
        for (let i = 0; i < w * h; i++) {
          if (pixel) {
            data[i * 4 + 0] = pixel[0];
            data[i * 4 + 1] = pixel[1];
            data[i * 4 + 2] = pixel[2];
            data[i * 4 + 3] = pixel[3];
          } else {
            data[i * 4 + 3] = (i * 37) % 256; // alpha ramp
          }
        }
        return { data, width: w, height: h };
      },
    };
    c.getContext = () => ctx;
    return c;
  };
}
