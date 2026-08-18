// Check (e) of the vt-headers ABI gate (A5): every enumerator the curated
// include/rustty/vt.h family declares has the SAME integer value as the
// corresponding variant of rustty's `#[repr(C)]` Rust enum.
//
// This is the missing analogue of upstream's `checkGhosttyHEnum`
// (`src/lib/enum.zig`), which validates every Zig enum field against the
// matching macro translated out of the real header. Before this arm existed
// NOTHING in the port compared a `#[repr(C)]` discriminant to a header
// enumerator: arm (b) compares symbol NAMES, arm (c) compares struct LAYOUTS,
// and neither looks at an enum's values. `P3-W3`/`P3-W6` are about to add
// `RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES = 27`, `RUSTTY_TERMINAL_DATA_*`
// 33..35 and two more `OPT_*`, and a header/Rust skew there is a silent
// mis-dispatch, not a compile error.
//
// METHOD
//   1. Parse every `#[repr(C)] pub enum` under crates/rustty-vt{,-c}/src that
//      carries a `C: [Ghostty… ->] Rustty…` doc line. That doc line is the
//      existing, already-load-bearing port convention for "this Rust type IS
//      that C type"; using it means the mapping cannot drift out of sync with
//      the source the way a separate ledger file would.
//   2. Parse every `typedef enum … } Rustty…;` in include/rustty/**, taking the
//      enumerator NAMES in declaration order. Their VALUES are deliberately not
//      parsed — clang evaluates them in the probe, so `= 1 << 3`, a chained
//      reference, or an implicit increment are all handled exactly as a real C
//      caller would see them.
//   3. Emit `_Static_assert((int)<ENUMERATOR> == <rust discriminant>)` pairing
//      the two lists positionally, and compile with clang.
//
// NON-VACUITY
//   * a curated enum with no Rust counterpart is a FAIL unless it is in
//     UNMAPPED below WITH a reason;
//   * a UNMAPPED entry that HAS acquired a counterpart is a FAIL (a rotting
//     allowlist is how a gate stops binding);
//   * a differing enumerator count is a FAIL — that is the arm that catches
//     "upstream added a variant and the header did not";
//   * zero emitted asserts is a FAIL.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
const INCDIR = `${ROOT}/include`;
const TMP = process.env.TMPDIR || "/tmp";

// Curated header enums that have NO `#[repr(C)]` Rust enum. Each entry must say
// WHY. This list can only shrink; a stale row fails the gate.
const UNMAPPED = {
  // These two are `#[repr(transparent)]` newtypes with associated consts, not
  // Rust enums, because the oracle declares them as OPEN enums (`enum(c_int) {
  // …, _ }`) and an out-of-range C value must not be materialized as a Rust
  // enum tag. Their named values therefore live as `pub const NAME: T = T(n);`
  // and this arm's `pub enum` parser cannot see them.
  // TODO(P3): extend the parser to `#[repr(transparent)]` + associated consts
  // so these two stop being exceptions. Tracked here, not silently dropped.
  RusttyModeReportState:
    "open enum: crates/rustty-vt-c/src/modes.rs:26 `pub struct ReportState(pub c_int)`",
  RusttyClipboardWriteResult:
    "open enum: crates/rustty-vt/src/clipboard.rs:56 `pub struct WriteResult(pub i32)`",
};

// Curated enums the header deliberately exposes as a PREFIX of the Rust enum.
// The listed `headerCount` is asserted exactly, so a header that later drifts
// further behind — or catches up — fails instead of silently widening the
// exception. The first `headerCount` discriminants are still fully checked.
// EMPTY as of P4-W5 Bundle T: the one exception (RusttyOscCommandType,
// header stopped at KITTY_TEXT_SIZING = 22 mirroring the OLD-pin oracle
// header) retired itself exactly as designed — the oracle's osc.h gained
// KITTY_CLIPBOARD_PROTOCOL/KITTY_DND_PROTOCOL/CONTEXT_SIGNAL (23..25) in the
// 6ad1fe7d8..12967b68f range (9673a22b0), rustty's osc.h now carries them
// too, and the exact `headerCount` assert forced this row's deletion rather
// than a silent widening.
const PREFIX_OK = {};

const CC = (() => {
  try {
    return execFileSync("xcrun", ["--find", "clang"]).toString().trim();
  } catch {
    return "/usr/bin/clang";
  }
})();
const SYSROOT = (() => {
  try {
    return execFileSync("xcrun", ["--show-sdk-path"]).toString().trim();
  } catch {
    return null;
  }
})();

function walk(dir, filter, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, filter, out);
    else if (filter(e.name)) out.push(p);
  }
  return out;
}

// --- 1. Rust `#[repr(C)]` enums, keyed by their `C: Rustty…` doc name -------
function rustEnums() {
  const byC = new Map();
  const byName = new Map();
  const aliases = [];
  const files = [
    ...walk(`${ROOT}/crates/rustty-vt-c/src`, (n) => n.endsWith(".rs")),
    ...walk(`${ROOT}/crates/rustty-vt/src`, (n) => n.endsWith(".rs")),
  ];
  for (const f of files) {
    const lines = readFileSync(f, "utf8").split("\n");
    for (let i = 0; i < lines.length; i++) {
      // `pub type RusttyX = Y;` with a `C:` doc line is the port's alias form
      // (e.g. `pub type TerminalScreen = ScreenSetKey;`). Record the alias and
      // resolve it to the real enum after every file has been scanned.
      const alias = lines[i].match(/^\s*pub type\s+([A-Za-z0-9_]+)\s*=\s*([A-Za-z0-9_]+)\s*;/);
      if (alias) {
        let j2 = i - 1;
        let cn = null;
        while (j2 >= 0 && (/^\s*(#\[|\/\/)/.test(lines[j2]) || lines[j2].trim() === "")) {
          const m2 = lines[j2].match(
            /C:\s*(?:Ghostty[A-Za-z0-9_]+\s*->\s*)?(Rustty[A-Za-z0-9_]+)/,
          );
          if (m2 && !cn) cn = m2[1];
          j2--;
        }
        if (cn) aliases.push([cn, alias[2]]);
        continue;
      }
      const head = lines[i].match(/^\s*pub enum\s+([A-Za-z0-9_]+)\s*\{/);
      if (!head) continue;
      // Look back over the attribute/doc block for #[repr(C)] and the C: line.
      let j = i - 1;
      let repr = false;
      let cname = null;
      while (j >= 0 && (/^\s*(#\[|\/\/)/.test(lines[j]) || lines[j].trim() === "")) {
        if (/#\[repr\(C\)\]/.test(lines[j])) repr = true;
        const m = lines[j].match(
          /C:\s*(?:Ghostty[A-Za-z0-9_]+\s*->\s*)?(Rustty[A-Za-z0-9_]+)/,
        );
        if (m && !cname) cname = m[1];
        j--;
      }
      // `repr` is recorded but not required. Many of these enums cross the ABI
      // as a `c_int` narrowed by an explicit `match` (the D11 policy) rather
      // than as a `#[repr(C)]` tag; their discriminants are still exactly what
      // the header's enumerators must equal, so they are checked too.
      void repr;

      // Body: variants at depth 1 only. Commented-out oracle text (the port's
      // convention is to keep the Zig source as `//` comments) is skipped.
      const variants = [];
      let depth = 1;
      let next = 0;
      let dataCarrying = false;
      for (let k = i + 1; k < lines.length && depth > 0; k++) {
        const raw = lines[k];
        const line = raw.trim();
        for (const ch of raw) {
          if (ch === "{") depth++;
          else if (ch === "}") depth--;
        }
        if (depth <= 0) break;
        if (line.startsWith("//") || line.startsWith("#[") || line === "") continue;
        if (line.includes("=>") || line.includes("::")) continue;
        // Variants may be raw identifiers (`r#true`) and lowercase where the
        // port keeps the oracle's key spelling verbatim (`OptionAsAlt`).
        const v = line.match(/^(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\s*(?:=\s*(-?\d+)\s*)?[,{(]/);
        if (!v) continue;
        if (/^(?:r#)?[A-Za-z_][A-Za-z0-9_]*\s*[({]/.test(line)) dataCarrying = true;
        const value = v[2] !== undefined ? parseInt(v[2], 10) : next;
        next = value + 1;
        variants.push({ name: v[1], value });
      }
      const rec = {
        explicit: cname,
        repr,
        rustName: head[1],
        file: f.replace(ROOT + "/", ""),
        variants,
        dataCarrying,
      };
      if (cname) byC.set(cname, rec);
      // Name-derived fallback: `pub enum TerminalCompressionMode` answers for
      // `RusttyTerminalCompressionMode` even without an explicit `C:` line.
      // Only used when no explicit mapping claims the header name, and never
      // when two Rust enums share a bare name (ambiguous -> no fallback).
      if (byName.has(head[1])) byName.set(head[1], "AMBIGUOUS");
      else byName.set(head[1], rec);
    }
  }
  for (const [cn, target] of aliases) {
    const rec = byName.get(target);
    if (rec && rec !== "AMBIGUOUS" && !byC.has(cn)) byC.set(cn, { ...rec, explicit: cn });
  }
  for (const [n, rec] of byName) {
    if (rec === "AMBIGUOUS") continue;
    const cname = `Rustty${n}`;
    if (!byC.has(cname)) byC.set(cname, rec);
  }
  return byC;
}

// --- 2. curated header enums ------------------------------------------------
function headerEnums() {
  const out = new Map();
  for (const f of walk(`${INCDIR}/rustty`, (n) => n.endsWith(".h"))) {
    const txt = readFileSync(f, "utf8");
    const re = /typedef\s+enum[^{]*\{([\s\S]*?)\}\s*(Rustty[A-Za-z0-9_]+)\s*;/g;
    let m;
    while ((m = re.exec(txt)) !== null) {
      const body = m[1];
      const names = [];
      for (const line of body.split("\n")) {
        const t = line.trim();
        if (t.startsWith("/") || t.startsWith("*") || t === "") continue;
        const e = t.match(/^(RUSTTY_[A-Z0-9_]+)/);
        if (!e) continue;
        if (e[1].endsWith("_MAX_VALUE")) continue; // the open-enum sentinel
        names.push(e[1]);
      }
      out.set(m[2], { file: f.replace(ROOT + "/", ""), names });
    }
  }
  return out;
}

const rust = rustEnums();
const hdrs = headerEnums();

const failures = [];
let probe = "#include <rustty/vt.h>\n";
let asserts = 0;
let mapped = 0;

for (const [cname, h] of [...hdrs].sort()) {
  const r = rust.get(cname);
  if (!r) {
    if (UNMAPPED[cname]) continue;
    failures.push(
      `  ${cname} (${h.file}): declared in the curated headers but no ` +
        `#[repr(C)] Rust enum carries a "C: ${cname}" doc line.\n` +
        `      Either add the doc line to the Rust enum, or add a row to ` +
        `UNMAPPED in this script WITH a reason.`,
    );
    continue;
  }
  if (r.dataCarrying) {
    failures.push(
      `  ${cname}: the Rust enum ${r.rustName} (${r.file}) carries data, so its ` +
        `variants are not plain C enumerators. Map it explicitly or split it.`,
    );
    continue;
  }
  const prefix = PREFIX_OK[cname];
  if (prefix) {
    if (h.names.length !== prefix.headerCount) {
      failures.push(
        `  ${cname}: PREFIX_OK expects ${prefix.headerCount} header ` +
          `enumerator(s) but ${h.file} has ${h.names.length}. The documented ` +
          `exception no longer describes the tree; re-derive it.\n` +
          `      reason on file: ${prefix.reason}`,
      );
      continue;
    }
    if (r.variants.length < h.names.length) {
      failures.push(
        `  ${cname}: PREFIX_OK, but the Rust enum ${r.rustName} now has FEWER ` +
          `variants (${r.variants.length}) than the header (${h.names.length}).`,
      );
      continue;
    }
  } else if (r.variants.length !== h.names.length) {
    failures.push(
      `  ${cname}: ${h.names.length} enumerator(s) in ${h.file} but ` +
        `${r.variants.length} variant(s) in ${r.rustName} (${r.file}).\n` +
        `      header: ${h.names.join(", ")}\n` +
        `      rust:   ${r.variants.map((v) => `${v.name}=${v.value}`).join(", ")}`,
    );
    continue;
  }
  mapped++;
  for (let i = 0; i < h.names.length; i++) {
    probe +=
      `_Static_assert((int)${h.names[i]} == ${r.variants[i].value}, ` +
      `"${cname}.${h.names[i]} != ${r.rustName}::${r.variants[i].name}");\n`;
    asserts++;
  }
}

// stale PREFIX_OK rows
for (const cname of Object.keys(PREFIX_OK)) {
  if (!hdrs.has(cname)) {
    failures.push(
      `  ${cname}: listed in PREFIX_OK but no longer declared in any curated ` +
        `header. Delete the row.`,
    );
  }
}

// stale UNMAPPED rows
for (const cname of Object.keys(UNMAPPED)) {
  if (!hdrs.has(cname)) {
    failures.push(
      `  ${cname}: listed in UNMAPPED but no longer declared in any curated ` +
        `header. Delete the row.`,
    );
  } else if (rust.has(cname)) {
    failures.push(
      `  ${cname}: listed in UNMAPPED but a Rust counterpart now exists ` +
        `(${rust.get(cname).rustName}). Delete the row so the enum is checked.`,
    );
  }
}

// Rust doc lines naming a header type that does not exist at all.
for (const [cname, r] of rust) {
  if (r.explicit !== cname) continue; // name-derived fallback, not a claim
  if (hdrs.has(cname)) continue;
  const anywhere = walk(`${INCDIR}/rustty`, (n) => n.endsWith(".h")).some((f) =>
    readFileSync(f, "utf8").includes(cname),
  );
  if (!anywhere) {
    failures.push(
      `  ${cname}: ${r.file} claims "C: ${cname}" for Rust enum ${r.rustName}, ` +
        `but that name appears in NO curated header. Stale or misspelled mapping.`,
    );
  }
}

if (asserts === 0) {
  failures.push(
    "  VACUOUS: zero enumerator asserts were emitted. A parser that matches " +
      "nothing must be a loud failure, not a green run.",
  );
}

if (failures.length) {
  console.log("    ENUM-DISCRIMINANT PARITY FAILURES:");
  for (const f of failures) console.log(f);
  process.exit(1);
}

probe += "int main(void){return 0;}\n";
const probePath = `${TMP}/vt_enum_probe.c`;
writeFileSync(probePath, probe);
const args = ["-fsyntax-only", "-I", INCDIR];
if (SYSROOT) args.push("-isysroot", SYSROOT);
args.push(probePath);
try {
  execFileSync(CC, args, { stdio: "pipe" });
  console.log(
    `    ${asserts} enumerator discriminants across ${mapped} enums OK ` +
      `(${Object.keys(UNMAPPED).length} documented unmapped)`,
  );
  process.exit(0);
} catch (err) {
  const out = (err.stderr || err.stdout || Buffer.from("")).toString();
  console.log("    ENUM-DISCRIMINANT MISMATCH(es):");
  console.log(
    out
      .split("\n")
      .filter((l) => /static_assert|error:/i.test(l))
      .map((l) => "      " + l.trim())
      .join("\n"),
  );
  process.exit(1);
}
