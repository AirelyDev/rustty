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
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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

// --- oracle reads ------------------------------------------------------------
// EVERY git call in this file goes through `oracleGit`, which runs git with every
// GIT_* variable removed from its environment. A plain `git -C <oracle> show`
// honours GIT_DIR, so a decoy repository could answer the registry-parity check
// below with any type list (R5). The oracle path itself is fixed (no override).
function oracleGit(dir, args, opts = {}) {
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith("GIT_")),
  );
  return execFileSync("git", ["-C", dir, ...args], {
    stdio: "pipe",
    maxBuffer: 1 << 24,
    ...opts,
    env,
  });
}
// The ONLY read of the oracle's contents: the registry names of
// origin/main:src/terminal/c/types.zig, without their `Ghostty` prefix.
function oracleRegistryKeys(dir) {
  const text = oracleGit(dir, ["show", "origin/main:src/terminal/c/types.zig"]).toString();
  return new Set(
    [...text.matchAll(/\.init[A-Za-z]+\(\s*"Ghostty([A-Za-z0-9_]+)"/g)].map((m) => m[1]),
  );
}

// --self-test-oracle (hermetic; run by A5's `--self-test-oracle`, which
// G-ORACLE-SH-SELF runs): a fixture oracle and a decoy, each registering one
// different type at origin/main. With GIT_DIR + GIT_WORK_TREE naming the decoy,
// `oracleRegistryKeys` on the fixture must return the fixture's type.
if (process.argv.includes("--self-test-oracle")) {
  const { mkdtempSync, mkdirSync, rmSync } = await import("node:fs");
  const base = mkdtempSync(`${process.env.TMPDIR || "/tmp"}/a5c_oracle.`);
  const ident = [
    "-c", "user.name=selftest", "-c", "user.email=selftest@invalid",
    "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
  ];
  let got = "<read failed>";
  try {
    for (const [repo, name] of [["ghostty", "FixtureReal"], ["decoy", "DecoyOnly"]]) {
      const dir = `${base}/${repo}`;
      mkdirSync(`${dir}/src/terminal/c`, { recursive: true });
      writeFileSync(`${dir}/src/terminal/c/types.zig`, `.initStruct("Ghostty${name}"),\n`);
      oracleGit(dir, ["init", "-q"]);
      oracleGit(dir, [...ident, "add", "-A"]);
      oracleGit(dir, [...ident, "commit", "-q", "-m", "fixture"]);
      oracleGit(dir, ["update-ref", "refs/remotes/origin/main", "HEAD"]);
    }
    process.env.GIT_DIR = `${base}/decoy/.git`;
    process.env.GIT_WORK_TREE = `${base}/decoy`;
    process.chdir(`${base}/decoy`);
    try {
      got = [...oracleRegistryKeys(`${base}/ghostty`)].join(",");
    } catch {
      got = "<read failed>";
    }
  } finally {
    delete process.env.GIT_DIR;
    delete process.env.GIT_WORK_TREE;
    rmSync(base, { recursive: true, force: true });
  }
  if (got === "FixtureReal") {
    console.log("  ok   A5 arm (c): the oracle registry read ignores a GIT_DIR + GIT_WORK_TREE decoy");
    process.exit(0);
  }
  console.log(`  FAIL A5 arm (c): the oracle registry read returned '${got}', want 'FixtureReal'`);
  process.exit(1);
}

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
const MANIFEST = JSON.parse(
  execFileSync(runnerBin, [], {
    // DYLD_ on macOS; LD_ on Linux, where `-lrustty_vt_c` links the `.so`
    // (N486, P7-W24).
    env: {
      ...process.env,
      DYLD_LIBRARY_PATH: `${ROOT}/target/release`,
      LD_LIBRARY_PATH: `${ROOT}/target/release`,
    },
  }).toString(),
);
// P4-W5 Bundle T: `rustty_type_json` now returns the versioned ABI manifest
// (oracle 9673a22b0 / c75559589): `{schema, abi, ..., types: {...}}` with
// per-kind descriptors (struct/union/enum/packed/alias/opaque). This arm
// consumes the struct/union descriptors (they carry `fields` with offsets);
// an unknown schema version means the document's shape can no longer be
// assumed, which is a FAIL rather than a skip.
if (MANIFEST.schema !== 1) {
  console.log(
    `    FAIL: rustty_type_json() reports schema ${MANIFEST.schema}; this ` +
      `checker understands schema 1.`,
  );
  process.exit(1);
}
const LAYOUT = MANIFEST.types;

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
// EMPTY as of P4-W5 Bundle T: the registry expansion (oracle 309440e07 "all
// public structs" + 9673a22b0) registered every previously-allowlisted
// aggregate, so each old row tripped the "type_json now reports it — delete
// the row" stale check and was retired by REGISTERING the struct, exactly as
// designed.
const NOT_IN_TYPE_JSON = {};

// --- oracle parity for the REGISTRY ITSELF ---------------------------------
// The allowlist above is only trustworthy if "the oracle also omits it" is
// checked rather than asserted. Read the oracle's registry (git show only —
// NEVER a checkout, and only through `oracleGit`, GIT_* stripped) and require
// rustty's type_json key set to equal it after the R2 rename. Oracle absent =>
// exit 3 BLOCKED, never a silent green. `origin/main`, not the pin, is
// deliberate (the lib-vt C-API surface lives beyond the pin), so no pin check.
// N483: the oracle is `ghostty` beside the MAIN checkout, resolved by the shell twin
// `_oracle_root.sh` (one rule for every gate, no environment override).
function resolveOracleRoot() {
  const here = fileURLToPath(new URL(".", import.meta.url));
  const root = resolve(here, "..", "..");
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith("GIT_")),
  );
  return execFileSync(
    "bash",
    ["-c", '. "$1" || exit 3; rustty_oracle_root "$2"', "oracle_root",
      join(root, ".port/tools/gates/_oracle_root.sh"), root],
    { env, encoding: "utf8" },
  ).trim();
}
const ORACLE = resolveOracleRoot();
try {
  oracleGit(ORACLE, ["rev-parse", "--git-dir"]);
} catch {
  console.log(
    "    BLOCKED: ghostty oracle checkout not found at " + ORACLE + " (R3);" +
      " the type_json registry-parity check cannot bind here. NOT a pass.",
  );
  process.exit(3);
}
// P4-W5 Bundle T: the oracle registry is now the `type_decls` list of
// `.initStruct/.initTaggedStruct/.initUnion/.initEnum/.initEnumSentinel/
// .initPacked/.initAlias/.initOpaque("Ghostty...")` rows (all six kinds, not
// only structs) — the parity below therefore binds the WHOLE registry.
const oracleKeys = oracleRegistryKeys(ORACLE);
if (oracleKeys.size === 0) {
  console.log(
    "    BLOCKED: read 0 struct names from the oracle registry. An empty" +
      " reference side would make this check VACUOUS (R5).",
  );
  process.exit(3);
}
const rusttyKeys = new Set(Object.keys(LAYOUT).map((k) => k.replace(/^Rustty/, "")));
// Documented BEYOND-PIN residuals. The oracle sync point is `6ad1fe7d8` since
// `P3-W10 T3` (2026-07-31; it was `4749c4e93` when this list was written), and
// rustty matches that tree's registry EXACTLY; any row below would be a registry
// change upstream made beyond the pin and a port item, not a defect.
// Each row must name an owner and a trigger. The stale check underneath means
// this list can only shrink: port the change and the row must be deleted.
//
// EMPTY as of P3-W6 T3: both rows (TerminalDesktopNotification,
// TerminalProgressReport) were retired by porting the structs, exactly as their
// triggers prescribed. The stale check below is what forced it — registering the
// two structs turned the gate RED with "delete the row" rather than letting the
// allowlist rot silently.
//
// RE-POPULATED by P4-W5 Bundle T with the 12 rows whose C types rode the later
// W5 bundles, then EMPTIED again as each bundle landed: Bundle R and Bundle M
// rows with P4-W5 T2/T3, and the final five (Reader, Writer, SnapshotDecoder,
// SnapshotDecoderData, SnapshotDecoderOption) with P4-W5 T5 Bundle S — each
// deletion forced by the stale check below, exactly as designed. The registry
// now matches the oracle's `type_decls` EXACTLY.
const REGISTRY_RESIDUALS = {};
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
  // P4-W5 Bundle T: the pre-manifest registry modeled tagged unions as a
  // single whole-struct "opaque" blob and this loop skipped their offsets.
  // The manifest now reports REAL fields for every struct AND union (union
  // member offsets are all 0, matching the C header's members), so every
  // field row asserts — the whole-opaque skip is retired (tighten-only).
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
      `[type_json registry vs oracle: ${oracleKeys.size} types, ` +
      `${Object.keys(REGISTRY_RESIDUALS).length} residuals owned by the later P4-W5 bundles] ` +
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
