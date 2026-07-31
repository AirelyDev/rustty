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

// WS-P3-W0 T2 step 4 — arm (c'): make this arm HEADER-DRIVEN.
//
// Before this, the loop below iterated `type_json`'s keys and skipped anything
// the headers did not mention. The inverse — an aggregate TYPEDEF'D IN THE
// HEADER but never registered in `crates/rustty-vt-c/src/types.rs` — produced
// ZERO assertions, silently. That is the vacuous-shrink shape again: the way to
// make this arm pass is to not register your struct. `P3-W6 T3` is about to add
// two such typedefs.
//
// So: enumerate the aggregates the curated headers actually define, and REQUIRE
// each to be a key of `rustty_type_json()` unless it is listed below WITH a
// reason. A stale row (an allowlisted name that is now registered, or that no
// longer exists) is itself a failure.
// Every row here was checked against the ORACLE's own registry
// (`origin/main:src/terminal/c/types.zig`, the `structs` StaticStringMap) and
// is absent there too, so omitting it is R1-faithful, not a porting gap. The
// oracle-parity check below enforces that claim mechanically: if upstream ever
// registers one of these, rustty's key set diverges and this arm goes RED.
const ORACLE_ALSO_OMITS =
  "R1-faithful: absent from the oracle's own " +
  "origin/main:src/terminal/c/types.zig `structs` registry too";
const NOT_IN_TYPE_JSON = {
  RusttyAllocator: ORACLE_ALSO_OMITS,
  RusttyAllocatorVtable: ORACLE_ALSO_OMITS,
  RusttyKittyGraphicsPlacementRenderInfo: ORACLE_ALSO_OMITS,
  RusttyPointValue: ORACLE_ALSO_OMITS,
  RusttyRenderStateRowSelection: ORACLE_ALSO_OMITS,
  RusttySgrAttribute: ORACLE_ALSO_OMITS,
  RusttySgrAttributeValue: ORACLE_ALSO_OMITS,
  RusttySgrUnknown: ORACLE_ALSO_OMITS,
  RusttyStyleColorValue: ORACLE_ALSO_OMITS,
  RusttySysImage: ORACLE_ALSO_OMITS,
  RusttyTerminalScrollViewportValue: ORACLE_ALSO_OMITS,
  RusttyTerminalSelectionFormatOptions: ORACLE_ALSO_OMITS,
};

// --- oracle parity for the REGISTRY ITSELF ---------------------------------
// The allowlist above is only trustworthy if "the oracle also omits it" is
// checked rather than asserted. Read the oracle's registry (git show only —
// NEVER a checkout) and require rustty's type_json key set to equal it after
// the R2 rename. Oracle absent => exit 3 BLOCKED, never a silent green.
const ORACLE = "/Users/gael/projects/ghostty";
try {
  execFileSync("git", ["-C", ORACLE, "rev-parse", "--git-dir"], { stdio: "pipe" });
} catch {
  console.log(
    "    BLOCKED: ghostty oracle checkout not found at " + ORACLE + " (R3);" +
      " the type_json registry-parity check cannot bind here. NOT a pass.",
  );
  process.exit(3);
}
const oracleTypes = execFileSync(
  "git",
  ["-C", ORACLE, "show", "origin/main:src/terminal/c/types.zig"],
  { stdio: "pipe", maxBuffer: 1 << 24 },
).toString();
const oracleKeys = new Set(
  [...oracleTypes.matchAll(/\.\{\s*"Ghostty([A-Za-z0-9_]+)"/g)].map((m) => m[1]),
);
if (oracleKeys.size === 0) {
  console.log(
    "    BLOCKED: read 0 struct names from the oracle registry. An empty" +
      " reference side would make this check VACUOUS (R5).",
  );
  process.exit(3);
}
const rusttyKeys = new Set(Object.keys(LAYOUT).map((k) => k.replace(/^Rustty/, "")));
// Documented BEYOND-PIN residuals. rustty is pinned at 4749c4e93 and matches
// that tree's registry EXACTLY; every row below is a registry change upstream
// made inside 4749c4e93..6ad1fe7d8 and is a Phase 3 port item, not a defect.
// Each row must name an owner and a trigger. The stale check underneath means
// this list can only shrink: port the change and the row must be deleted.
const REGISTRY_RESIDUALS = {
  // in the oracle registry at origin/main, absent at the pin and in rustty
  TerminalDesktopNotification:
    "added upstream beyond the pin; owner P3-W6 (C-API surface + effects). " +
    "Trigger: porting the desktop-notification C struct.",
  TerminalProgressReport:
    "added upstream beyond the pin; owner P3-W6 (C-API surface + effects). " +
    "Trigger: porting the ConEmu progress-report C struct.",
};
const missingHere = [...oracleKeys]
  .filter((k) => !rusttyKeys.has(k) && !REGISTRY_RESIDUALS[k])
  .sort();
const extraHere = [...rusttyKeys]
  .filter((k) => !oracleKeys.has(k) && !REGISTRY_RESIDUALS[k])
  .sort();
const staleResiduals = Object.keys(REGISTRY_RESIDUALS)
  .filter((k) => oracleKeys.has(k) === rusttyKeys.has(k))
  .sort();
if (staleResiduals.length) {
  console.log("    STALE type_json REGISTRY RESIDUAL LIST:");
  for (const k of staleResiduals)
    console.log(
      `      ${k}: listed as a beyond-pin residual but the oracle and rustty ` +
        `now agree about it. Delete the row (a rotting allowlist is how a ` +
        `gate stops binding).`,
    );
  process.exit(1);
}
if (missingHere.length || extraHere.length) {
  console.log("    type_json REGISTRY DIVERGES FROM THE ORACLE:");
  for (const k of missingHere)
    console.log(`      Ghostty${k}: in the oracle registry, absent from rustty_type_json()`);
  for (const k of extraHere)
    console.log(`      Rustty${k}: in rustty_type_json(), absent from the oracle registry`);
  process.exit(1);
}

const aggregates = new Map(); // RusttyX -> file
for (const f of allHeaders(`${INCDIR}/rustty`)) {
  const txt = readFileSync(f, "utf8");
  const re = /typedef\s+(?:struct|union)[^{;]*\{[\s\S]*?\}\s*(Rustty[A-Za-z0-9_]+)\s*;/g;
  let m;
  while ((m = re.exec(txt)) !== null) {
    aggregates.set(m[1], f.replace(ROOT + "/", ""));
  }
}
const layoutFailures = [];
for (const [name, file] of [...aggregates].sort()) {
  if (LAYOUT[name]) continue;
  if (NOT_IN_TYPE_JSON[name]) continue;
  layoutFailures.push(
    `      ${name} (${file}): defined as an aggregate in a curated header but ` +
      `absent from rustty_type_json(). Register it in ` +
      `crates/rustty-vt-c/src/types.rs, or add a row to NOT_IN_TYPE_JSON in ` +
      `this script WITH a reason. "In the header, absent from type_json" is a ` +
      `FAIL, not a skip.`,
  );
}
for (const name of Object.keys(NOT_IN_TYPE_JSON)) {
  if (!aggregates.has(name)) {
    layoutFailures.push(
      `      ${name}: listed in NOT_IN_TYPE_JSON but no longer defined as an ` +
        `aggregate in any curated header. Delete the row.`,
    );
  } else if (LAYOUT[name]) {
    layoutFailures.push(
      `      ${name}: listed in NOT_IN_TYPE_JSON but rustty_type_json() now ` +
        `reports it. Delete the row so its layout is checked.`,
    );
  }
}
if (layoutFailures.length) {
  console.log("    HEADER AGGREGATES WITH NO type_json ROW:");
  for (const f of layoutFailures) console.log(f);
  process.exit(1);
}

// --- 3. emit static_asserts for every declared struct present in type_json ---
// _Static_assert is a C11 keyword (no header); offsetof needs <stddef.h>.
let probe = "#include <rustty/vt.h>\n#include <stddef.h>\n";
let checked = 0;
let sizeofs = 0;
for (const [name, info] of Object.entries(LAYOUT)) {
  if (!declared.has(name)) continue; // header doesn't expose this struct
  if (!info || typeof info.size !== "number" || !info.fields) continue;
  probe += `_Static_assert(sizeof(${name})==${info.size}, "sizeof ${name}");\n`;
  sizeofs++;
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
  // Print the two assert populations SEPARATELY. The old single-number success
  // line ("N field offsets ... OK") read as a pass even for a run that emitted
  // only `sizeof` asserts and zero offsets, and neither count was allowed to be
  // zero-checked. Both are now reported and both must be non-zero.
  if (sizeofs === 0 || checked === 0) {
    console.log(
      `    VACUOUS: ${sizeofs} sizeof assert(s) and ${checked} offsetof ` +
        `assert(s) were emitted; both must be non-zero.`,
    );
    process.exit(1);
  }
  console.log(
    `    ${sizeofs} sizeof + ${checked} offsetof asserts across ` +
      `${aggregates.size} header aggregates OK ` +
      `[type_json registry vs oracle: ${oracleKeys.size} structs, ` +
      `${Object.keys(REGISTRY_RESIDUALS).length} beyond-pin residuals owned by P3-W6] ` +
      `(${Object.keys(NOT_IN_TYPE_JSON).length} documented as not in type_json)`,
  );
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
