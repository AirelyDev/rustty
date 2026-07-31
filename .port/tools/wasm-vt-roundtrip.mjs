// Runtime roundtrip proof for rustty_vt_c.wasm — mirrors ghostty's
// example/wasm-vt/index.html flow (terminal_new -> vt_write -> format -> read)
// against the rustty_* C-ABI. Proves the VT engine (incl. the wasm page
// allocator) actually RUNS as WebAssembly, not just links.
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { WASI } from 'node:wasi';

// wasm path resolves relative to the repo root (this file is .port/tools/)
//
// FIXED (WS10): this pointed at `target/wasm32-wasip1/release/rustty_vt_c.wasm`,
// which is the artifact of `build-wasm.sh` (the wasip1 variant that needs zig as
// the C/C++ cross-compiler). But the script it is documented and invoked with is
// `build-vt-wasm.sh`, whose artifact is the PURE-RUST, zero-import
// `wasm32-unknown-unknown` build — a different directory. So the roundtrip could
// never find its module and exited 1 on every tree, for a path typo rather than
// a real defect. Prefer the pure-Rust artifact (building it if absent, since it
// is a build product of this repo and needs no C toolchain), and fall back to a
// wasip1 build if one happens to be present.
const UU = new URL("../../target/wasm32-unknown-unknown/release/rustty_vt_c.wasm", import.meta.url);
const WASIP1 = new URL("../../target/wasm32-wasip1/release/rustty_vt_c.wasm", import.meta.url);
let WASM = existsSync(UU) ? UU : (existsSync(WASIP1) ? WASIP1 : null);
if (!WASM) {
  const build = new URL("./build-vt-wasm.sh", import.meta.url);
  console.error("wasm-vt-roundtrip: building the vt wasm artifact (absent) …");
  const r = spawnSync(fileURLToPath(build), [], { stdio: ["ignore", "inherit", "inherit"] });
  if (r.status !== 0 || !existsSync(UU)) {
    console.error(
      "wasm-vt-roundtrip: could not build " + fileURLToPath(UU) +
      "\n  (is the wasm32-unknown-unknown target installed? `rustup target add wasm32-unknown-unknown`)");
    process.exit(3);   // environment missing — never a silent pass
  }
  WASM = UU;
}
const SUCCESS = 0;
const EMIT_PLAIN = 0;

const wasi = new WASI({ version: 'preview1', args: [], env: {} });
const mod = await WebAssembly.compile(readFileSync(WASM));
// The pure-Rust module imports NOTHING; only the wasip1 build needs the WASI
// import object and the `_initialize` call.
const needsWasi = WebAssembly.Module.imports(mod)
  .some((i) => i.module.startsWith("wasi_"));
const inst = await WebAssembly.instantiate(mod, needsWasi ? wasi.getImportObject() : {});
if (needsWasi) wasi.initialize(inst);
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
//
// STALE-TOOL FIX (P3-W6 T7): this used to build a `RusttyTerminalOptions` struct
// in wasm memory and pass a pointer to it. `P3-W3 T5` (oracle 03d5fa268) REMOVED
// that struct from the ABI — `rustty_terminal_new` now takes `cols`/`rows`
// directly, and `types.rs:762` asserts the name is ABSENT from `type_json`. So
// `LAYOUT['RusttyTerminalOptions'].size` threw a TypeError on the first line,
// i.e. this tool has been dead since that commit.
//
// It has no CI caller (nothing under .port/ or .github/ invokes it), which is
// exactly why nobody noticed. Ported forward rather than deleted: it is the only
// end-to-end wasm exercise of the C ABI that exists.
const termSlot = ex.rustty_wasm_alloc_opaque();
const r1 = ex.rustty_terminal_new(0, termSlot, 80, 24);
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

// The roundtrip only ever checked C-ABI RETURN CODES, so a module that parsed
// the input into the wrong cells still printed "ROUNDTRIP OK". Assert the
// CONTENT: the SGR sequences must be stripped by the plain formatter and the two
// printed lines must survive the page tree intact. This is the arm that fails if
// the VT engine miscompiles for wasm.
const EXPECT = ['Hello from rustty-vt.wasm!', 'Line two: red box.'];
const got = text.replace(/\s+$/, '').split('\n').map((l) => l.replace(/\s+$/, ''));
if (got.length !== EXPECT.length || EXPECT.some((e, i) => got[i] !== e)) {
  throw new Error(
    'roundtrip CONTENT mismatch:\n  expected ' + JSON.stringify(EXPECT) +
    '\n  got      ' + JSON.stringify(got));
}
if (text.includes('\x1b')) throw new Error('plain formatter leaked an ESC byte');
console.log('\nROUNDTRIP OK: new -> vt_write -> format -> read, all in WebAssembly.');
console.log('  content asserted: ' + JSON.stringify(EXPECT));
