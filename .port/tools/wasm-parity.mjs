// Exact-parity harness: run the SAME wasm-vt flow (terminal_new -> vt_write ->
// formatter -> format_alloc) through BOTH ghostty-vt.wasm and rustty_vt_c.wasm
// and assert the formatted output bytes are IDENTICAL. This is the byte-level
// gate behind the side-by-side browser examples (example/wasm-vt).
//
// Usage: node .port/tools/wasm-parity.mjs
import { readFileSync } from "node:fs";

const GHOSTTY = "/Users/gael/projects/ghostty/zig-out/bin/ghostty-vt.wasm";
const RUSTTY =
  "/Users/gael/projects/rustty/target/wasm32-unknown-unknown/release/rustty_vt_c.wasm";

// The exact default input from the wasm-vt example (\x1b/\r/\n unescaped).
const VT =
  "Hello, World!\r\n\x1b[1;32mGreen Bold\x1b[0m and \x1b[4mUnderline\x1b[0m\r\n" +
  "Line 3: placeholder\r\n\x1b[3;1H\x1b[2KLine 3: Overwritten!";
const SUCCESS = 0,
  EMIT_PLAIN = 0;

async function runFlow(path, P /* symbol prefix: ghostty|rustty */) {
  const mod = await WebAssembly.compile(readFileSync(path));
  const inst = await WebAssembly.instantiate(mod, { env: { log() {} } });
  const ex = inst.exports;
  const buf = () => ex.memory.buffer;
  const fn = (s) => ex[`${P}_${s}`];
  // type layout
  const jp = fn("type_json")();
  let e = jp;
  const mem0 = new Uint8Array(buf());
  while (mem0[e]) e++;
  const L = JSON.parse(Buffer.from(mem0.subarray(jp, e)).toString());
  const K = P === "ghostty" ? "Ghostty" : "Rustty";
  const setField = (base, sname, f, v) => {
    const fi = L[`${K}${sname}`].fields[f];
    const dv = new DataView(buf(), base + fi.offset, fi.size);
    if (fi.type === "u8" || fi.type === "bool") dv.setUint8(0, v);
    else if (fi.type === "u16") dv.setUint16(0, v, true);
    else if (fi.type === "u32" || fi.type === "enum") dv.setUint32(0, v, true);
    else if (fi.type === "u64") dv.setBigUint64(0, BigInt(v), true);
    else throw new Error(`bad field type ${fi.type} for ${sname}.${f}`);
  };
  const rdU32 = (p) => new DataView(buf()).getUint32(p, true);

  // terminal_new(80x24)
  const TOPT = L[`${K}TerminalOptions`].size;
  const op = fn("wasm_alloc_u8_array")(TOPT);
  new Uint8Array(buf(), op, TOPT).fill(0);
  setField(op, "TerminalOptions", "cols", 80);
  setField(op, "TerminalOptions", "rows", 24);
  setField(op, "TerminalOptions", "max_scrollback", 0);
  const ts = fn("wasm_alloc_opaque")();
  if (fn("terminal_new")(0, ts, op) !== SUCCESS) throw new Error("terminal_new");
  const term = rdU32(ts);
  fn("wasm_free_u8_array")(op, TOPT);
  fn("wasm_free_opaque")(ts);

  // vt_write
  const vb = new TextEncoder().encode(VT);
  const dp = fn("wasm_alloc_u8_array")(vb.length);
  new Uint8Array(buf()).set(vb, dp);
  fn("terminal_vt_write")(term, dp, vb.length);
  fn("wasm_free_u8_array")(dp, vb.length);

  // formatter (plain, trim)
  const FOPT = L[`${K}FormatterTerminalOptions`].size;
  const fp = fn("wasm_alloc_u8_array")(FOPT);
  new Uint8Array(buf(), fp, FOPT).fill(0);
  setField(fp, "FormatterTerminalOptions", "size", FOPT);
  setField(fp, "FormatterTerminalOptions", "emit", EMIT_PLAIN);
  setField(fp, "FormatterTerminalOptions", "unwrap", 0);
  setField(fp, "FormatterTerminalOptions", "trim", 1);
  const exOff = L[`${K}FormatterTerminalOptions`].fields.extra.offset;
  new DataView(buf()).setUint32(
    exOff + L[`${K}FormatterTerminalExtra`].fields.size.offset,
    L[`${K}FormatterTerminalExtra`].size,
    true,
  );
  const scOff = L[`${K}FormatterTerminalExtra`].fields.screen.offset;
  new DataView(buf()).setUint32(
    exOff + scOff + L[`${K}FormatterScreenExtra`].fields.size.offset,
    L[`${K}FormatterScreenExtra`].size,
    true,
  );
  const fs = fn("wasm_alloc_opaque")();
  if (fn("formatter_terminal_new")(0, fs, term, fp) !== SUCCESS)
    throw new Error("formatter_terminal_new");
  fn("wasm_free_u8_array")(fp, FOPT);
  const fmt = rdU32(fs);
  fn("wasm_free_opaque")(fs);

  // format_alloc -> bytes
  const opp = fn("wasm_alloc_opaque")();
  const olp = fn("wasm_alloc_usize")();
  if (fn("formatter_format_alloc")(fmt, 0, opp, olp) !== SUCCESS)
    throw new Error("format_alloc");
  const outP = rdU32(opp),
    outL = rdU32(olp);
  const out = new Uint8Array(buf(), outP, outL).slice(); // copy out before frees
  fn("formatter_free")(fmt);
  fn("terminal_free")(term);
  return { bytes: out, inputLen: vb.length };
}

const g = await runFlow(GHOSTTY, "ghostty");
const r = await runFlow(RUSTTY, "rustty");
const same =
  g.bytes.length === r.bytes.length && g.bytes.every((b, i) => b === r.bytes[i]);

console.log(`input:  ${g.inputLen} bytes (both)`);
console.log(`ghostty output: ${g.bytes.length} bytes`);
console.log(`rustty  output: ${r.bytes.length} bytes`);
console.log(`ghostty text: ${JSON.stringify(new TextDecoder().decode(g.bytes))}`);
console.log(`rustty  text: ${JSON.stringify(new TextDecoder().decode(r.bytes))}`);
console.log(`\nBYTE-EXACT PARITY: ${same ? "YES ✓" : "NO ✗"}`);
process.exit(same ? 0 : 1);
