// web-canvas-glue.mjs — the `env` Canvas FFI import layer for the
// rustty-web-canvas freestanding wasm module (crates/rustty-font-shaper's
// `face/web_canvas.rs`). Implements EXACTLY the `rustty_wc_*` Canvas ops the
// browser-Canvas font face declares, against a real 2d canvas context.
//
// The FFI is primitives + pointer/len buffers only (no JS objects cross the
// boundary). Canvas/context objects are kept in integer handle tables on this
// side. `createCanvas` is injected so the SAME glue drives a real browser
// `<canvas>`/`OffscreenCanvas` (index.html) and a headless JS mock (the node
// instantiate test) — only the canvas object differs; the marshalling is shared.
//
// Usage:
//   const env = makeWebCanvasEnv({ memory: () => inst.exports.memory, createCanvas });
//   inst = await WebAssembly.instantiate(mod, { env });

export function makeWebCanvasEnv({ memory, createCanvas }) {
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();

  const canvases = new Map();
  let nextCanvas = 1;
  const contexts = new Map();
  let nextCtx = 1;

  const u8 = () => new Uint8Array(memory().buffer);
  const readStr = (ptr, len) => decoder.decode(u8().subarray(ptr, ptr + len));

  return {
    // document.createElement("canvas") -> handle
    rustty_wc_canvas_new() {
      const h = nextCanvas++;
      canvases.set(h, createCanvas());
      return h;
    },
    rustty_wc_canvas_free(h) {
      canvases.delete(h);
    },
    // canvas.getContext("2d") -> handle (stable per canvas, as the reference relies on)
    rustty_wc_canvas_get_context(h) {
      const c = canvases.get(h);
      if (!c) return 0;
      if (c.__ctxHandle) return c.__ctxHandle;
      const ch = nextCtx++;
      contexts.set(ch, c.getContext("2d"));
      c.__ctxHandle = ch;
      return ch;
    },
    // canvas.width/height + style.width/height ("{n}px")
    rustty_wc_canvas_set_size(h, w, ht) {
      const c = canvases.get(h);
      if (!c) return;
      c.width = w;
      c.height = ht;
      if (c.style) {
        c.style.width = w + "px";
        c.style.height = ht + "px";
      }
    },

    // ctx.clearRect(0, 0, canvas.width, canvas.height)
    rustty_wc_context_clear(ch) {
      const ctx = contexts.get(ch);
      if (!ctx) return;
      ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    },
    rustty_wc_context_set_font(ch, ptr, len) {
      const ctx = contexts.get(ch);
      if (ctx) ctx.font = readStr(ptr, len);
    },
    rustty_wc_context_get_font(ch, ptr, cap) {
      const ctx = contexts.get(ch);
      const bytes = encoder.encode(ctx ? ctx.font : "");
      const n = Math.min(bytes.length, cap);
      u8().set(bytes.subarray(0, n), ptr);
      return n;
    },
    rustty_wc_context_set_fill_style(ch, ptr, len) {
      const ctx = contexts.get(ch);
      if (ctx) ctx.fillStyle = readStr(ptr, len);
    },
    rustty_wc_context_set_text_baseline(ch, ptr, len) {
      const ctx = contexts.get(ch);
      if (ctx) ctx.textBaseline = readStr(ptr, len);
    },
    rustty_wc_context_fill_rect(ch, x, y, w, h) {
      const ctx = contexts.get(ch);
      if (ctx) ctx.fillRect(x, y, w, h);
    },
    rustty_wc_context_fill_text(ch, ptr, len, x, y) {
      const ctx = contexts.get(ch);
      if (ctx) ctx.fillText(readStr(ptr, len), x, y);
    },
    // ctx.measureText(str) -> writes 5 f32:
    // [width, boxLeft, boxRight, boxAscent, boxDescent]
    rustty_wc_context_measure_text(ch, ptr, len, outPtr) {
      const ctx = contexts.get(ch);
      const m = ctx ? ctx.measureText(readStr(ptr, len)) : {};
      const f = new Float32Array(memory().buffer, outPtr, 5);
      f[0] = m.width || 0;
      f[1] = m.actualBoundingBoxLeft || 0;
      f[2] = m.actualBoundingBoxRight || 0;
      f[3] = m.actualBoundingBoxAscent || 0;
      f[4] = m.actualBoundingBoxDescent || 0;
    },
    // ctx.getImageData(x, y, w, h).data (Uint8ClampedArray, RGBA) -> copy into wasm memory
    rustty_wc_context_get_image_data(ch, x, y, w, h, outPtr, cap) {
      const ctx = contexts.get(ch);
      if (!ctx) return 0;
      const img = ctx.getImageData(x, y, w, h);
      const n = Math.min(img.data.length, cap);
      u8().set(img.data.subarray(0, n), outPtr);
      return img.data.length;
    },
    // document.getElementById(id).append(canvas); 0 ok, -1 not found
    rustty_wc_debug_append(h, idPtr, idLen) {
      const c = canvases.get(h);
      if (!c || typeof document === "undefined") return -1;
      const el = document.getElementById(readStr(idPtr, idLen));
      if (!el) return -1;
      el.append(c);
      return 0;
    },
  };
}
