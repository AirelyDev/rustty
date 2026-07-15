// Runtime roundtrip proof for rustty_vt_c.wasm — mirrors ghostty's
// example/wasm-vt/index.html flow (terminal_new -> vt_write -> format -> read)
// against the rustty_* C-ABI. Proves the VT engine (incl. the wasm page
// allocator) actually RUNS as WebAssembly, not just links.
import { readFileSync } from 'node:fs';
import { WASI } from 'node:wasi';

// wasm path resolves relative to the repo root (this file is .port/tools/)
const WASM = new URL("../../target/wasm32-wasip1/release/rustty_vt_c.wasm", import.meta.url);
const SUCCESS = 0;
const EMIT_PLAIN = 0;

const wasi = new WASI({ version: 'preview1', args: [], env: {} });
const mod = await WebAssembly.compile(readFileSync(WASM));
const inst = await WebAssembly.instantiate(mod, wasi.getImportObject());
wasi.initialize(inst);
const ex = inst.exports;
const buf = () => ex.memory.buffer;

// --- read the module's own type layout (offsets/sizes) ---
const jp = ex.rustty_type_json();
{ let e = jp; const m = new Uint8Array(buf()); while (m[e]) e++; var LAYOUT = JSON.parse(Buffer.from(m.subarray(jp, e)).toString()); }
const field = (t, f) => LAYOUT[t].fields[f];
const setField = (base, t, f, val) => {
  const fi = field(t, f); const dv = new DataView(buf(), base + fi.offset, fi.size);
  if (fi.size === 1) dv.setUint8(0, val);
  else if (fi.size === 2) dv.setUint16(0, val, true);
  else dv.setUint32(0, val, true); // u32/usize/enum (wasm32)
};
const rdU32 = (p) => new DataView(buf()).getUint32(p, true);

// --- 1. terminal_new(cols=80, rows=24) ---
const TOPT = LAYOUT['RusttyTerminalOptions'].size;
const optsPtr = ex.rustty_wasm_alloc_u8_array(TOPT);
new Uint8Array(buf(), optsPtr, TOPT).fill(0);
setField(optsPtr, 'RusttyTerminalOptions', 'cols', 80);
setField(optsPtr, 'RusttyTerminalOptions', 'rows', 24);
setField(optsPtr, 'RusttyTerminalOptions', 'max_scrollback', 0);
const termSlot = ex.rustty_wasm_alloc_opaque();
const r1 = ex.rustty_terminal_new(0, termSlot, optsPtr);
ex.rustty_wasm_free_u8_array(optsPtr, TOPT);
if (r1 !== SUCCESS) throw new Error('rustty_terminal_new -> ' + r1);
const term = rdU32(termSlot);
ex.rustty_wasm_free_opaque(termSlot);
console.log('terminal_new: OK (handle=' + term + ')');

// --- 2. vt_write: some VT-encoded data (bold + color + text + newline) ---
const vt = '\x1b[1;32mHello from rustty-vt.wasm!\x1b[0m\r\nLine two: \x1b[31mred\x1b[0m box.';
const vtBytes = new TextEncoder().encode(vt);
const dataPtr = ex.rustty_wasm_alloc_u8_array(vtBytes.length);
new Uint8Array(buf()).set(vtBytes, dataPtr);
ex.rustty_terminal_vt_write(term, dataPtr, vtBytes.length);
ex.rustty_wasm_free_u8_array(dataPtr, vtBytes.length);
console.log('vt_write: OK (' + vtBytes.length + ' bytes parsed into the page tree)');

// --- 3. plain-text formatter over the terminal ---
const FOPT = LAYOUT['RusttyFormatterTerminalOptions'].size;
const fPtr = ex.rustty_wasm_alloc_u8_array(FOPT);
new Uint8Array(buf(), fPtr, FOPT).fill(0);
setField(fPtr, 'RusttyFormatterTerminalOptions', 'size', FOPT);
setField(fPtr, 'RusttyFormatterTerminalOptions', 'emit', EMIT_PLAIN);
setField(fPtr, 'RusttyFormatterTerminalOptions', 'unwrap', 0);
setField(fPtr, 'RusttyFormatterTerminalOptions', 'trim', 1);
// nested sized-struct `size` fields (extra + extra.screen)
const extraOff = field('RusttyFormatterTerminalOptions', 'extra').offset;
new DataView(buf()).setUint32(extraOff + field('RusttyFormatterTerminalExtra', 'size').offset, LAYOUT['RusttyFormatterTerminalExtra'].size, true);
const scrOff = field('RusttyFormatterTerminalExtra', 'screen').offset;
new DataView(buf()).setUint32(extraOff + scrOff + field('RusttyFormatterScreenExtra', 'size').offset, LAYOUT['RusttyFormatterScreenExtra'].size, true);

const fSlot = ex.rustty_wasm_alloc_opaque();
const r2 = ex.rustty_formatter_terminal_new(0, fSlot, term, fPtr);
ex.rustty_wasm_free_u8_array(fPtr, FOPT);
if (r2 !== SUCCESS) throw new Error('rustty_formatter_terminal_new -> ' + r2);
const fmt = rdU32(fSlot);
ex.rustty_wasm_free_opaque(fSlot);

// --- 4. format_alloc -> read the plain-text screen contents ---
const outPP = ex.rustty_wasm_alloc_opaque();
const outLP = ex.rustty_wasm_alloc_usize();
const r3 = ex.rustty_formatter_format_alloc(fmt, 0, outPP, outLP);
if (r3 !== SUCCESS) throw new Error('rustty_formatter_format_alloc -> ' + r3);
const outPtr = rdU32(outPP), outLen = rdU32(outLP);
const text = new TextDecoder().decode(new Uint8Array(buf(), outPtr, outLen));
console.log('format_alloc: OK (' + outLen + ' bytes)');
console.log('\n===== formatted terminal contents (from wasm) =====');
console.log(text.replace(/\s+$/,''));
console.log('===================================================');

ex.rustty_formatter_free(fmt);
ex.rustty_terminal_free(term);
console.log('\nROUNDTRIP OK: new -> vt_write -> format -> read, all in WebAssembly.');
