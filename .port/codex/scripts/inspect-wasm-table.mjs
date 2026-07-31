#!/usr/bin/env node

import fs from "node:fs";

const path = process.argv[2];
if (!path) {
  console.error("usage: inspect-wasm-table.mjs <module.wasm>");
  process.exit(2);
}

const bytes = fs.readFileSync(path);
const module = new WebAssembly.Module(bytes);

let offset = 8;
function readUleb() {
  let value = 0;
  let shift = 0;
  let byte;
  do {
    byte = bytes[offset++];
    value |= (byte & 0x7f) << shift;
    shift += 7;
  } while ((byte & 0x80) !== 0);
  return value >>> 0;
}

const tables = [];
while (offset < bytes.length) {
  const id = bytes[offset++];
  const length = readUleb();
  const end = offset + length;
  if (id === 4) {
    const count = readUleb();
    for (let index = 0; index < count; index += 1) {
      const refType = bytes[offset++];
      const flags = readUleb();
      const min = readUleb();
      const max = (flags & 1) !== 0 ? readUleb() : null;
      tables.push({ index, refType: `0x${refType.toString(16)}`, flags, min, max });
    }
  }
  offset = end;
}

console.log(JSON.stringify({
  path,
  bytes: bytes.length,
  tableExports: WebAssembly.Module.exports(module).filter(({ kind }) => kind === "table"),
  imports: WebAssembly.Module.imports(module),
  tables,
}, null, 2));
