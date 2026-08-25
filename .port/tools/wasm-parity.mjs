// Exact-parity harness: run the SAME wasm-vt flow (terminal_new -> vt_write ->
// formatter -> format_alloc) through BOTH ghostty-vt.wasm and rustty-vt.wasm
// and assert the formatted output bytes are IDENTICAL. This is the byte-level
// gate behind the side-by-side browser examples (example/wasm-vt).
//
// Also gates the F-006 embedder-callback contract on both modules: an exported
// GROWABLE `__indirect_function_table` (ghostty: `exe.export_table = true` +
// wasm_patch_growable_table; rustty: `--export-table` + the ported xtask
// `wasm-patch-growable-table` — see .port/tools/build-vt-wasm.sh) so browser JS
// can grow the table, insert a callback entry, and hand its index to
// <prefix>_sys_set.
//
// Usage: node .port/tools/wasm-parity.mjs [--table-only]
//   --table-only runs ONLY the F-006 embedder-callback gate on both modules
//   (compile + instantiate + grow + <prefix>_sys_set). That gate depends on
//   nothing but the link/patch pipeline, so it stays runnable independently of
//   the byte-parity flow below, which additionally needs an oracle
//   `ghostty-vt.wasm` whose `type_json` matches the flow's struct expectations.
import { readFileSync } from "node:fs";

const TABLE_ONLY = process.argv.includes("--table-only");

const GHOSTTY = "/Users/gael/projects/ghostty/zig-out/bin/ghostty-vt.wasm";
const RUSTTY = "/Users/gael/projects/rustty/zig-out/bin/rustty-vt.wasm";

// The exact default input from the wasm-vt example (\x1b/\r/\n unescaped).
const VT =
  "Hello, World!\r\n\x1b[1;32mGreen Bold\x1b[0m and \x1b[4mUnderline\x1b[0m\r\n" +
  "Line 3: placeholder\r\n\x1b[3;1H\x1b[2KLine 3: Overwritten!";
const SUCCESS = 0,
  EMIT_PLAIN = 0;

// F-006: embedder-callback contract. The module must export a GROWABLE
// funcref table named `__indirect_function_table` so browser JS can install
// effect callbacks; a bounded (unpatched) table makes `grow` throw RangeError,
// and a private (unexported) table leaves `ex.__indirect_function_table`
// undefined. Both are checked, because the two halves of the pipeline —
// `--export-table` at link time and `wasm-patch-growable-table` after — fail
// independently.
function tableGate(mod, ex, P) {
  if (
    !WebAssembly.Module.exports(mod).some(
      (e) => e.kind === "table" && e.name === "__indirect_function_table",
    )
  )
    throw new Error(`${P}: __indirect_function_table not exported`);
  const table = ex.__indirect_function_table;
  const cbIdx = table.length;
  table.grow(1); // RangeError here = table max was not patched away
  // A grown slot's index is a valid fn-pointer value for the sys API: install
  // it as the log callback (option 2 = LOG), then clear it (0 = null fn ptr).
  const sysSet = ex[`${P}_sys_set`];
  if (sysSet(2, cbIdx) !== SUCCESS)
    throw new Error(`${P}: sys_set(LOG, table index) failed`);
  if (sysSet(2, 0) !== SUCCESS)
    throw new Error(`${P}: sys_set(LOG, null) failed`);
  return cbIdx;
}

async function load(path, P) {
  const mod = await WebAssembly.compile(readFileSync(path));
  const inst = await WebAssembly.instantiate(mod, { env: { log() {} } });
  return { mod, ex: inst.exports };
}

async function runFlow(path, P /* symbol prefix: ghostty|rustty */) {
  const { mod, ex } = await load(path, P);
  const buf = () => ex.memory.buffer;
  const fn = (s) => ex[`${P}_${s}`];

  tableGate(mod, ex, P);

  // type layout
  //
  // 9673a22b0/c75559589: `type_json` is now the versioned ABI manifest — type
  // rows live under `.types` (with an `.abi` block carrying usize_size etc.).
  const jp = fn("type_json")();
  let e = jp;
  const mem0 = new Uint8Array(buf());
  while (mem0[e]) e++;
  const MANIFEST = JSON.parse(Buffer.from(mem0.subarray(jp, e)).toString());
  const L = MANIFEST.types;
  const USIZE = MANIFEST.abi.usize_size;
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
  //
  // 03d5fa268 removed the `TerminalOptions` struct from the ABI —
  // `terminal_new` takes cols/rows directly. a8e9b413f removed the per-type
  // wasm exports (`wasm_alloc_u8_array` et al.) for the byte-based
  // `wasm_alloc`/`wasm_free` pair.
  const ts = fn("wasm_alloc_opaque")();
  if (fn("terminal_new")(0, ts, 80, 24) !== SUCCESS)
    throw new Error("terminal_new");
  const term = rdU32(ts);
  fn("wasm_free_opaque")(ts);

  // vt_write
  const vb = new TextEncoder().encode(VT);
  const dp = fn("wasm_alloc")(vb.length);
  new Uint8Array(buf()).set(vb, dp);
  fn("terminal_vt_write")(term, dp, vb.length);
  fn("wasm_free")(dp, vb.length);

  // formatter (plain, trim)
  const FOPT = L[`${K}FormatterTerminalOptions`].size;
  const fp = fn("wasm_alloc")(FOPT);
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
  fn("wasm_free")(fp, FOPT);
  const fmt = rdU32(fs);
  fn("wasm_free_opaque")(fs);

  // format_alloc -> bytes
  const opp = fn("wasm_alloc_opaque")();
  const olp = fn("wasm_alloc")(USIZE);
  if (fn("formatter_format_alloc")(fmt, 0, opp, olp) !== SUCCESS)
    throw new Error("format_alloc");
  const outP = rdU32(opp),
    outL = rdU32(olp);
  const out = new Uint8Array(buf(), outP, outL).slice(); // copy out before frees
  fn("wasm_free_opaque")(opp);
  fn("wasm_free")(olp, USIZE);
  fn("formatter_free")(fmt);
  fn("terminal_free")(term);
  return { bytes: out, inputLen: vb.length };
}

if (TABLE_ONLY) {
  for (const [path, P] of [
    [GHOSTTY, "ghostty"],
    [RUSTTY, "rustty"],
  ]) {
    const { mod, ex } = await load(path, P);
    const idx = tableGate(mod, ex, P);
    console.log(
      `${P}: exported growable __indirect_function_table OK ` +
        `(grew to slot ${idx}, ${P}_sys_set(LOG, ${idx}) accepted)`,
    );
  }
  console.log("\nF-006 EMBEDDER-CALLBACK GATE: PASS ✓");
  process.exit(0);
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
