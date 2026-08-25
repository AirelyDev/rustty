/**
 * Smoke-test the low-level allocation helpers exported by librustty-vt Wasm.
 *
 * The test instantiates a release artifact, forces linear memory growth,
 * verifies generic allocation and reusable pointer slots, and checks that
 * public C structs can be allocated from the ABI manifest.
 *
 * Build and run locally with:
 *
 *   .port/tools/build-vt-wasm.sh
 *   node test/wasm-alloc.mjs zig-out/bin/rustty-vt.wasm
 */
import assert from "node:assert/strict";
import fs from "node:fs/promises";

const path = process.argv[2];
if (path === undefined) {
  console.error("usage: node test/wasm-alloc.mjs <rustty-vt.wasm>");
  process.exit(2);
}

const module = await WebAssembly.compile(await fs.readFile(path));
const instance = await WebAssembly.instantiate(module, {});
const { exports } = instance;
const memory = exports.memory;

let cachedBuffer = null;
let cachedLength = 0;
let cachedBytes = null;

/** Return a byte view over the current linear-memory buffer. */
function memoryBytes() {
  const buffer = memory.buffer;
  if (buffer !== cachedBuffer || buffer.byteLength !== cachedLength) {
    cachedBuffer = buffer;
    cachedLength = buffer.byteLength;
    cachedBytes = new Uint8Array(buffer);
  }

  return cachedBytes;
}

// Read target-specific sizes, alignment, and result values from the ABI
// manifest so this test does not duplicate properties of the Wasm target.
const typeJsonPtr = exports.rustty_type_json();
const typeBytes = memoryBytes();
const typeJsonEnd = typeBytes.indexOf(0, typeJsonPtr);
assert.notEqual(typeJsonEnd, -1);
const typeLayout = JSON.parse(
  new TextDecoder().decode(typeBytes.subarray(typeJsonPtr, typeJsonEnd)),
);
const allocationAlignment = typeLayout.abi.max_alignment;
const resultValues = typeLayout.types.RusttyResult.values;

function check(result) {
  assert.equal(
    result,
    resultValues.SUCCESS,
    `librustty-vt call failed with ${result}`,
  );
}

// A block larger than the current memory guarantees allocator-driven growth.
// Verify that the documented lazy view-refresh pattern observes the new buffer.
const oldBuffer = memory.buffer;
const allocationLength = oldBuffer.byteLength + 1;
const allocation = exports.rustty_wasm_alloc(allocationLength);
assert.notEqual(allocation, 0);
assert.equal(allocation % allocationAlignment, 0);
assert.notEqual(memory.buffer, oldBuffer);

// Distinct arbitrary sentinels verify that both ends of the allocation are
// writable after refreshing the linear-memory view.
const firstSentinel = 0x12;
const lastSentinel = 0x34;
const bytes = memoryBytes();
bytes[allocation] = firstSentinel;
bytes[allocation + allocationLength - 1] = lastSentinel;
assert.equal(bytes[allocation], firstSentinel);
assert.equal(bytes[allocation + allocationLength - 1], lastSentinel);
exports.rustty_wasm_free(allocation, allocationLength);
assert.equal(exports.rustty_wasm_alloc(0), 0);

// A pointer slot starts cleared, remains cleared after a failed constructor,
// and can be reused across successful constructors without a DataView read.
const slot = exports.rustty_wasm_alloc_opaque();
assert.notEqual(slot, 0);
assert.equal(exports.rustty_wasm_take_opaque(slot), 0);

const terminalColumns = 80;
const terminalRows = 24;
assert.equal(
  exports.rustty_terminal_new(0, slot, 0, terminalRows),
  resultValues.INVALID_VALUE,
);
assert.equal(exports.rustty_wasm_take_opaque(slot), 0);

check(exports.rustty_terminal_new(0, slot, terminalColumns, terminalRows));
const terminal = exports.rustty_wasm_take_opaque(slot);
assert.notEqual(terminal, 0);

check(exports.rustty_render_state_new(0, slot));
const renderState = exports.rustty_wasm_take_opaque(slot);
assert.notEqual(renderState, 0);

check(exports.rustty_render_state_row_iterator_new(0, slot));
const rowIterator = exports.rustty_wasm_take_opaque(slot);
assert.notEqual(rowIterator, 0);

check(exports.rustty_render_state_row_cells_new(0, slot));
const rowCells = exports.rustty_wasm_take_opaque(slot);
assert.notEqual(rowCells, 0);
assert.equal(exports.rustty_wasm_take_opaque(slot), 0);

// Public struct storage uses the generic allocator and exported type layout.
assert.equal(
  typeLayout.abi.pointer_size,
  typeLayout.types.RusttyBuffer.fields.ptr.size,
);
assert.equal(
  typeLayout.abi.usize_size,
  typeLayout.types.RusttyBuffer.fields.len.size,
);
const sgrAttributeSize = typeLayout.types.RusttySgrAttribute.size;
const sgrAttribute = exports.rustty_wasm_alloc(sgrAttributeSize);
assert.notEqual(sgrAttribute, 0);
assert.equal(sgrAttribute % allocationAlignment, 0);
exports.rustty_wasm_free(sgrAttribute, sgrAttributeSize);

exports.rustty_render_state_row_cells_free(rowCells);
exports.rustty_render_state_row_iterator_free(rowIterator);
exports.rustty_render_state_free(renderState);
exports.rustty_terminal_free(terminal);
exports.rustty_wasm_free_opaque(slot);

console.log(`Wasm allocator smoke test passed: ${path}`);
