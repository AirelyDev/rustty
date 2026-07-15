// check (c) of the vt-headers ABI gate: every struct the curated
// include/rustty/vt.h family DECLARES must have the same sizeof + field offsets
// as rustty's actual #[repr(C)] layout, as reported by rustty_type_json().
//
// Method: read type_json from the freestanding rustty_vt_c.wasm, then for each
// struct the headers typedef, emit static_assert(sizeof==N)/static_assert(
// offsetof(.,f)==M) into a probe TU that #include <rustty/vt.h> and compile it
// with clang. A failed assert => a header/ABI layout mismatch (printed by clang).
import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
const WASM = `${ROOT}/target/wasm32-unknown-unknown/release/rustty_vt_c.wasm`;
const INCDIR = `${ROOT}/include`;
const CC =
  (() => {
    try {
      return execFileSync("xcrun", ["--find", "clang"]).toString().trim();
    } catch {
      return "/usr/bin/clang";
    }
  })();
// macOS system headers (<stddef.h> for offsetof) need the SDK sysroot; node's
// clang spawn doesn't inherit it the way an interactive shell does.
const SYSROOT = (() => {
  try {
    return execFileSync("xcrun", ["--show-sdk-path"]).toString().trim();
  } catch {
    return null;
  }
})();

// --- 1. type_json from the NATIVE build (the target the c-vt examples run on).
// The wasm build reports wasm32 layouts (usize=4); the curated headers compile
// for the native host (usize=8), so we must compare native-vs-native. Build a
// tiny runner that links the dylib and prints rustty_type_json().
const ccBase = SYSROOT ? [`-isysroot`, SYSROOT] : [];
const runnerC = `${process.env.TMPDIR || "/tmp"}/vt_typejson_runner.c`;
const runnerBin = `${process.env.TMPDIR || "/tmp"}/vt_typejson_runner`;
writeFileSync(
  runnerC,
  "#include <rustty/vt.h>\n#include <stdio.h>\nint main(void){fputs(rustty_type_json(),stdout);return 0;}\n",
);
execFileSync(CC, [
  ...ccBase,
  "-I",
  INCDIR,
  "-L",
  `${ROOT}/target/release`,
  "-lrustty_vt_c",
  runnerC,
  "-o",
  runnerBin,
]);
const LAYOUT = JSON.parse(
  execFileSync(runnerBin, [], {
    env: { ...process.env, DYLD_LIBRARY_PATH: `${ROOT}/target/release` },
  }).toString(),
);

// --- 2. which structs do the curated headers actually declare? ---
function allHeaders(dir) {
  const out = [];
  for (const n of readdirSync(dir, { withFileTypes: true })) {
    const fp = join(dir, n.name);
    if (n.isDirectory()) out.push(...allHeaders(fp));
    else if (n.name.endsWith(".h")) out.push(fp);
  }
  return out;
}
const hdrText = allHeaders(`${INCDIR}/rustty`)
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");
const declared = new Set(
  [...hdrText.matchAll(/\bRustty[A-Za-z0-9_]+\b/g)].map((m) => m[0]),
);

// --- 3. emit static_asserts for every declared struct present in type_json ---
// _Static_assert is a C11 keyword (no header); offsetof needs <stddef.h>.
let probe = "#include <rustty/vt.h>\n#include <stddef.h>\n";
let checked = 0;
for (const [name, info] of Object.entries(LAYOUT)) {
  if (!declared.has(name)) continue; // header doesn't expose this struct
  if (!info || typeof info.size !== "number" || !info.fields) continue;
  probe += `_Static_assert(sizeof(${name})==${info.size}, "sizeof ${name}");\n`;
  const fields = Object.entries(info.fields);
  // type_json models some tagged-unions/handles as a single whole-struct
  // "opaque" blob (one field, offset 0, size == struct size). The hand-written
  // header exposes the real fields inside that ABI-sized region — the offset of
  // type_json's synthetic `value` field is meaningless there. For those, verify
  // sizeof only (ABI size is what matters); check field offsets otherwise.
  const wholeOpaque =
    fields.length === 1 &&
    fields[0][1].type === "opaque" &&
    fields[0][1].offset === 0 &&
    fields[0][1].size === info.size;
  if (wholeOpaque) continue;
  for (const [f, fi] of fields) {
    probe += `_Static_assert(offsetof(${name},${f})==${fi.offset}, "offsetof ${name}.${f}");\n`;
    checked++;
  }
}
probe += "int main(void){return 0;}\n";
const probePath = `${process.env.TMPDIR || "/tmp"}/vt_layout_probe.c`;
writeFileSync(probePath, probe);

// --- 4. compile: a failed _Static_assert IS the mismatch report ---
const ccArgs = ["-fsyntax-only", "-I", INCDIR];
if (SYSROOT) ccArgs.push("-isysroot", SYSROOT);
ccArgs.push(probePath);
try {
  execFileSync(CC, ccArgs, { stdio: "pipe" });
  console.log(`    ${checked} field offsets across the declared structs OK`);
  process.exit(0);
} catch (err) {
  const out = (err.stderr || err.stdout || Buffer.from("")).toString();
  console.log("    STRUCT-LAYOUT MISMATCH(es):");
  console.log(
    out
      .split("\n")
      .filter((l) => /static_assert|_Static_assert|error:/i.test(l))
      .map((l) => "      " + l.trim())
      .join("\n"),
  );
  process.exit(1);
}
