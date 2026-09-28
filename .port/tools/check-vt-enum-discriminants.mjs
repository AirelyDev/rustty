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
//   4. Check that each positional pair NAMES the same thing (NEW-FINDINGS
//      N27): the enumerator must be `<prefix><SUFFIX>` where SUFFIX, with its
//      underscores removed, equals the Rust variant name upper-cased with its
//      underscores removed, and `<prefix>` (ending in `_`) must be the same for
//      every enumerator of the enum. Without this, transposing two
//      IMPLICIT-value enumerators renumbers both in place and every positional
//      value assert still holds. The few variants whose Rust name is not the
//      header's suffix are listed in NAME_ALIASES with a reason.
//
// NON-VACUITY
//   * a curated enum with no Rust counterpart is a FAIL unless it is in
//     UNMAPPED below WITH a reason;
//   * a UNMAPPED entry that HAS acquired a counterpart is a FAIL (a rotting
//     allowlist is how a gate stops binding);
//   * a differing enumerator count is a FAIL — that is the arm that catches
//     "upstream added a variant and the header did not";
//   * zero emitted asserts is a FAIL;
//   * a NAME_ALIASES row whose enum or variant no longer exists is a FAIL.
//
// SELF-TEST (`--self-test`): runs the same check over fixture trees written to
// a temp dir: a matching fixture must pass, and a fixture whose header swaps
// two implicit-value enumerators must FAIL on the name arm while its value
// asserts still compile (the blind spot this arm closes), and a stale alias
// must FAIL.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

import { mkdtempSync, mkdirSync, rmSync } from "node:fs";

const REPO_ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
const TMP = process.env.TMPDIR || "/tmp";

// Curated header enums that have NO `#[repr(C)]` Rust enum. Each entry must say
// WHY. This list can only shrink; a stale row fails the gate.
const UNMAPPED = {
  // A `#[repr(transparent)]` newtype for an OPEN oracle enum (`enum(c_int) {
  // …, _ }`) is checked through its `pub const NAME: T = T(n);` consts (see
  // `rustEnums`). This one has no consts: its named values are the tags of
  // `modes::State`, reached through `to_zig`.
  // CORRECTION (batch T-A, parity S150-F2): this list also carried
  // `RusttyClipboardWriteResult` and `RusttyClipboardReadResult`, with a
  // "TODO(P3): extend the parser to `#[repr(transparent)]` + associated consts
  // so these two stop being exceptions". The parser now reads the consts, and
  // both are checked.
  RusttyModeReportState:
    "open enum with no associated consts: crates/rustty-vt-c/src/modes.rs `pub struct ReportState(pub c_int)`",
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

// Variants whose Rust name is not the header enumerator's suffix (step 4).
// Keyed by header enum, then Rust variant name, giving the header suffix that
// follows the enum's common prefix. Each row says why. A row whose enum or
// variant no longer exists fails the gate.
const NAME_ALIASES = {
  // The Rust variants are the Zig tags (`sgr.Attribute.Tag`), which put the
  // colour kind first ("256_underline_color", "8_bg", ...); the curated header
  // names them attribute-first. `crates/rustty-vt-c/src/sgr.rs` `Tag`.
  RusttySgrAttributeTag: {
    reason: "Zig tag names (`8_bg`, `256_fg`, ...) are colour-kind-first; sgr.h is attribute-first",
    variants: {
      Color256Underline: "UNDERLINE_COLOR_256",
      Color8Bg: "BG_8",
      Color8Fg: "FG_8",
      Color8BrightBg: "BRIGHT_BG_8",
      Color8BrightFg: "BRIGHT_FG_8",
      Color256Bg: "BG_256",
      Color256Fg: "FG_256",
    },
  },
  // `RUSTTY_TERMINAL_OPT_SIZE` sets the size CALLBACK; the Rust option is
  // named for the callback (`SizeCb`) like its `*Cb` siblings' payloads.
  RusttyTerminalOption: {
    reason: "the size option carries the size callback; Rust names it SizeCb",
    variants: { SizeCb: "SIZE" },
  },
  // The W3C letter keys are `KeyA`..`KeyZ` in Rust (the Zig `key_a` tags) and
  // `RUSTTY_KEY_A`.. in key/event.h, whose prefix already says KEY: the
  // leading `Key` of a one-letter key is not part of the header suffix.
  RusttyKey: {
    reason: "letter keys: Rust `KeyA` is key/event.h `RUSTTY_KEY_A`",
    strip: /^Key(?=[A-Z]$)/,
    variants: {},
  },
};

// The candidate `<prefix>`es under which `enumerator` names `variant`.
function namePrefixes(cname, enumerator, variant) {
  const out = new Set();
  const alias = NAME_ALIASES[cname]?.variants?.[variant];
  if (alias !== undefined) {
    if (enumerator.endsWith(`_${alias}`)) {
      out.add(enumerator.slice(0, enumerator.length - alias.length));
    }
    return out;
  }
  const strip = NAME_ALIASES[cname]?.strip;
  const want = (strip ? variant.replace(strip, "") : variant).replace(/_/g, "").toUpperCase();
  for (let i = 1; i < enumerator.length; i++) {
    if (enumerator[i - 1] !== "_") continue;
    if (enumerator.slice(i).replace(/_/g, "") === want) out.add(enumerator.slice(0, i));
  }
  return out;
}

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
function rustEnums(ROOT) {
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
      // `#[repr(transparent)] pub struct X(pub i32);` with a `C:` doc line and
      // `pub const Name: X = X(n);` associated consts is the port's form of an
      // OPEN oracle enum (`enum(c_int) { ..., _ }`). The consts, in file order,
      // are its enumerators. Only an explicit `C:` claim WITH consts is
      // recorded; a newtype without consts (e.g. `ReportState`) stays in
      // UNMAPPED. (Batch T-A, parity S150-F2: net-new coverage — the consts of
      // `clipboard::Location`, `WriteStatus` and `ReadStatus` were unchecked
      // before, or checked only while `Location` was a closed enum.)
      const tnew = lines[i].match(
        /^\s*pub struct\s+([A-Za-z0-9_]+)\s*\(\s*pub\s+[A-Za-z0-9_:]+\s*\)\s*;/,
      );
      if (tnew) {
        let j3 = i - 1;
        let transparent = false;
        let cn = null;
        while (j3 >= 0 && (/^\s*(#\[|\/\/)/.test(lines[j3]) || lines[j3].trim() === "")) {
          if (/#\[repr\(transparent\)\]/.test(lines[j3])) transparent = true;
          const m3 = lines[j3].match(
            /C:\s*(?:Ghostty[A-Za-z0-9_]+\s*->\s*)?(Rustty[A-Za-z0-9_]+)/,
          );
          if (m3 && !cn) cn = m3[1];
          j3--;
        }
        if (!transparent || !cn) continue;
        const name = tnew[1];
        const constRe = new RegExp(
          `^\\s*pub const\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*:\\s*${name}\\s*=\\s*` +
            `${name}\\s*\\(\\s*(-?\\d+)\\s*\\)\\s*;`,
        );
        const variants = [];
        for (const l of lines) {
          const mc = l.match(constRe);
          if (mc) variants.push({ name: mc[1], value: parseInt(mc[2], 10) });
        }
        if (variants.length === 0) continue;
        byC.set(cn, {
          explicit: cn,
          repr: true,
          rustName: name,
          file: f.replace(ROOT + "/", ""),
          variants,
          dataCarrying: false,
        });
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
function headerEnums(ROOT) {
  const INCDIR = `${ROOT}/include`;
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

function check(ROOT) {
  const rust = rustEnums(ROOT);
  const hdrs = headerEnums(ROOT);
  const INCDIR = `${ROOT}/include`;

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
    // Step 4: the positional partners must name the same thing, under one
    // prefix for the whole enum.
    let common = null;
    const nameErrs = [];
    for (let i = 0; i < h.names.length; i++) {
      const cands = namePrefixes(cname, h.names[i], r.variants[i].name);
      if (cands.size === 0) {
        nameErrs.push(`${h.names[i]} (slot ${i}) does not name ${r.rustName}::${r.variants[i].name}`);
        continue;
      }
      common = common === null ? cands : new Set([...common].filter((p) => cands.has(p)));
    }
    if (nameErrs.length === 0 && common !== null && common.size === 0) {
      nameErrs.push("the enumerators match their variants only under different prefixes");
    }
    if (nameErrs.length) {
      failures.push(
        `  ${cname} (${h.file}) vs ${r.rustName} (${r.file}): positional name mismatch ` +
          `(a reordered header, or a variant missing from NAME_ALIASES):\n` +
          nameErrs.map((e) => `      ${e}`).join("\n"),
      );
    }
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

  // stale NAME_ALIASES rows
  for (const [cname, a] of Object.entries(NAME_ALIASES)) {
    const r = rust.get(cname);
    if (!hdrs.has(cname) || !r) {
      failures.push(`  ${cname}: listed in NAME_ALIASES but not a mapped curated enum. Delete the row.`);
      continue;
    }
    if (a.strip && !r.variants.some((x) => a.strip.test(x.name))) {
    failures.push(
      `  ${cname}: NAME_ALIASES strip rule ${a.strip} matches no variant of ${r.rustName}. Delete it.`,
    );
  }
  for (const v of Object.keys(a.variants)) {
      if (!r.variants.some((x) => x.name === v)) {
        failures.push(
          `  ${cname}: NAME_ALIASES names variant ${v}, which ${r.rustName} no longer has. Delete it.`,
        );
      }
    }
  }

  if (asserts === 0) {
    failures.push(
      "  VACUOUS: zero enumerator asserts were emitted. A parser that matches " +
        "nothing must be a loud failure, not a green run.",
    );
  }
  probe += "int main(void){return 0;}\n";
  return { failures, probe, asserts, mapped, INCDIR };
}

// Compiles the value asserts; returns null on success, else the clang errors.
function compileProbe(probe, incdir) {
  const dir = mkdtempSync(`${TMP}/vt_enum_probe.`);
  const probePath = `${dir}/vt_enum_probe.c`;
  writeFileSync(probePath, probe);
  const args = ["-fsyntax-only", "-I", incdir];
  if (SYSROOT) args.push("-isysroot", SYSROOT);
  args.push(probePath);
  try {
    execFileSync(CC, args, { stdio: "pipe" });
    return null;
  } catch (err) {
    return (err.stderr || err.stdout || Buffer.from("")).toString();
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function selfTest() {
  const hdr = (order) =>
    "#ifndef RUSTTY_VT_H\n#define RUSTTY_VT_H\n" +
    `typedef enum {\n${order.map((n) => `  RUSTTY_FIXTURE_${n},`).join("\n")}\n` +
    "  RUSTTY_FIXTURE_MAX_VALUE = 0x7fffffff,\n} RusttyFixtureKind;\n#endif\n";
  const rs =
    "/// C: RusttyFixtureKind\n#[repr(C)]\npub enum FixtureKind {\n" +
    "    Alpha,\n    BetaGamma,\n    Delta,\n}\n";
  const fixture = (order, extraAlias) => {
    const dir = mkdtempSync(`${TMP}/vt_enum_selftest.`);
    mkdirSync(`${dir}/include/rustty`, { recursive: true });
    mkdirSync(`${dir}/crates/rustty-vt/src`, { recursive: true });
    mkdirSync(`${dir}/crates/rustty-vt-c/src`, { recursive: true });
    writeFileSync(`${dir}/include/rustty/vt.h`, hdr(order));
    writeFileSync(`${dir}/crates/rustty-vt-c/src/lib.rs`, rs);
    writeFileSync(`${dir}/crates/rustty-vt/src/lib.rs`, "\n");
    return dir;
  };
  // Only the fixture's enum is mapped there, so the real NAME_ALIASES rows
  // are stale in every fixture; drop those failures and judge the rest.
  // The same holds for the real UNMAPPED rows.
  const own = (fs) =>
    fs.filter(
      (f) => !/NAME_ALIASES but not a mapped/.test(f) && !/listed in UNMAPPED but no longer declared/.test(f),
    );
  let ok = true;
  const arm = (label, cond, detail) => {
    console.log(`    self-test ${cond ? "PASS" : "FAIL"}: ${label}`);
    if (!cond) {
      ok = false;
      if (detail) console.log(detail);
    }
  };
  const dirs = [];
  try {
    // (1) matching header: no failure, value asserts compile.
    const good = fixture(["ALPHA", "BETA_GAMMA", "DELTA"]);
    dirs.push(good);
    const g = check(good);
    arm(
      "matching fixture is green",
      own(g.failures).length === 0 && g.asserts === 3 && compileProbe(g.probe, g.INCDIR) === null,
      own(g.failures).join("\n"),
    );
    // (2) two implicit-value enumerators transposed: the value asserts still
    // compile (the positional blind spot), and the name arm fails.
    const swapped = fixture(["BETA_GAMMA", "ALPHA", "DELTA"]);
    dirs.push(swapped);
    const w = check(swapped);
    arm("transposed fixture: value asserts alone stay green", compileProbe(w.probe, w.INCDIR) === null);
    arm(
      "transposed fixture: the name arm is RED",
      own(w.failures).some((f) => /positional name mismatch/.test(f) && /RUSTTY_FIXTURE_BETA_GAMMA \(slot 0\)/.test(f)),
      own(w.failures).join("\n") || "      (no failure)",
    );
    // (3) a boundary-only match: `..._XALPHA` must not name `Alpha`.
    const near = fixture(["XALPHA", "BETA_GAMMA", "DELTA"]);
    dirs.push(near);
    arm(
      "a suffix that is not at a `_` boundary is RED",
      own(check(near).failures).some((f) => /RUSTTY_FIXTURE_XALPHA \(slot 0\)/.test(f)),
    );
    // (4) the real NAME_ALIASES rows are reported stale when their enum is gone.
    arm(
      "a stale NAME_ALIASES row is RED",
      g.failures.some((f) => /RusttySgrAttributeTag: listed in NAME_ALIASES but not a mapped/.test(f)),
    );
  } finally {
    for (const d of dirs) rmSync(d, { recursive: true, force: true });
  }
  console.log(`    SELF-TEST: ${ok ? "PASS" : "FAIL"}`);
  process.exit(ok ? 0 : 1);
}

if (process.argv[2] === "--self-test") selfTest();

const { failures, probe, asserts, mapped, INCDIR } = check(REPO_ROOT);
if (failures.length) {
  console.log("    ENUM-DISCRIMINANT PARITY FAILURES:");
  for (const f of failures) console.log(f);
  process.exit(1);
}
const out = compileProbe(probe, INCDIR);
if (out === null) {
  console.log(
    `    ${asserts} enumerator discriminants and names across ${mapped} enums OK ` +
      `(${Object.keys(UNMAPPED).length} documented unmapped, ` +
      `${Object.values(NAME_ALIASES).reduce((n, a) => n + Object.keys(a.variants).length, 0)} name alias(es))`,
  );
  process.exit(0);
} else {
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
