/** Copies the two engine binaries into public/porting/wterm and records sizes and hashes. */
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";

const RUSTY = process.argv[2];
if (!RUSTY) throw new Error("usage: sync-wterm-wasm.mjs <path to rustty_wterm.wasm>");
const ZIG = "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm";
const OUT = "public/porting/wterm";
mkdirSync(OUT, { recursive: true });
copyFileSync(RUSTY, `${OUT}/rustty-wterm.wasm`);
copyFileSync(ZIG, `${OUT}/ghostty-vt.wasm`);

const entry = (file, extra) => {
  const bytes = readFileSync(file);
  return {
    bytes: statSync(file).size,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    ...extra,
  };
};
const wterm = JSON.parse(readFileSync("node_modules/@wterm/ghostty/package.json", "utf8")).version;
writeFileSync(
  `${OUT}/manifest.json`,
  `${JSON.stringify(
    {
      rustty: entry(`${OUT}/rustty-wterm.wasm`, { source: "rustty-wterm" }),
      ghostty: entry(`${OUT}/ghostty-vt.wasm`, { source: `@wterm/ghostty@${wterm}` }),
    },
    null,
    2,
  )}\n`,
);
console.log("synced");
