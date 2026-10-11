// wasm_examples_browser_gate.mjs — the wasm example pages, Rustty against Ghostty,
// in headless Chrome.
//
// example/wasm-vt, example/wasm-sgr and example/wasm-key-encode are Ghostty's
// example pages with the names changed. This gate loads each page twice, once
// from this checkout (rustty-vt.wasm) and once from the oracle checkout at the
// pin (ghostty-vt.wasm, built the way Ghostty's CI builds it), drives the same
// inputs into both, and requires the `#output` text to match after Ghostty's
// names are mapped to Rustty's (Ghostty -> Rustty, ghostty -> rustty,
// GHOSTTY -> RUSTTY). Until this gate, G-EXAMPLES listed these pages as
// "covered by the --webgl lane" and nothing in that lane opened them.
//
// The inputs:
//   wasm-vt          the page's own default run, then each VT_CASES entry
//                    (written into #vtInput / #cols / #rows, then Run);
//   wasm-sgr         the page's own default run, then each SGR_CASES entry
//                    (written into #sequence, then an `input` event);
//   wasm-key-encode  every KEYS entry as a synthetic `keydown` on #keyInput
//                    (the handler reads only `code`, `key` and the modifier
//                    flags), under two Kitty flag settings (all on, the
//                    page's default; all off, legacy encoding) and the three
//                    actions (press, release, repeat).
//
// UD380: Ghostty's wasm-sgr page hard-codes the SGR attribute tags with a
// `RESET_UNDERLINE: 8` entry that sgr.h does not have, so every tag from 8 up is
// off by one (a 256-colour underline is shown as RGB, resets as other
// attributes). Rustty's page has the corrected table. So that the comparison
// still tests the wasm modules and not the two tables, the Ghostty side is
// served from a temporary mirror of the oracle whose wasm-sgr page gets the
// same correction. If the oracle's page no longer has the extra entry, the gate
// goes RED so the patch is removed rather than silently applied to nothing.
//
// MANIFEST ARM: every constant table the pages hard-code (the SGR tags, the key
// codes, the key actions) must equal the values in the module's own
// `<prefix>_type_json` manifest, on both sides (the Ghostty side after the
// UD380 correction). The differential cannot see a wrong table, since both
// pages share it; this arm can.
//
// ANTI-VACUITY and NEGATIVE CONTROLS (each must hold on every run):
//   - no compared output may be empty, a placeholder ("Waiting for ...") or an
//     error, and neither page may log a console error or throw;
//   - NC1: one VT case is run with a different input on the Rustty side only;
//     the outputs MUST differ, or the comparison is not looking at the output;
//   - NC2: the same for one key: Rustty gets KeyB where Ghostty gets KeyA;
//   - NC3: the manifest arm, run on the oracle's uncorrected SGR table, MUST
//     report mismatches (it is the arm that would have caught UD380).
//
// Node-only, no npm dependencies (CDP over node's built-in WebSocket), like
// ws14_webgl_browser_gate.mjs, whose Chrome and server plumbing this follows.
//
// EXIT CODES
//   0  every case matched, every table matched, and NC1-NC3 behaved
//   1  a mismatch, a vacuous output, a control that did not differ, or a page error
//   3  environment missing (Chrome, python3, zig, or the oracle checkout) — never green
//
// Usage: node .port/tools/gates/wasm_examples_browser_gate.mjs [--show]
//   --show prints every compared Rustty output (truncated), as evidence.

import { spawn, spawnSync, execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const HERE = import.meta.dirname;
const ROOT = resolve(HERE, "..", "..", "..");

const CHROME_CANDIDATES = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
];

const VT_CASES = [
  { cols: 80, rows: 24, input: "plain text, no escapes" },
  { cols: 20, rows: 5, input: "wraps past twenty columns into the next row\\r\\nsecond" },
  { cols: 80, rows: 24, input: "\\x1b[2J\\x1b[5;10Hplaced\\x1b[1;1Htop-left" },
  { cols: 80, rows: 24, input: "tab\\there\\r\\nback\\x08\\x08XY\\r\\n\\x1b[7mreverse\\x1b[27m" },
  { cols: 10, rows: 3, input: "1\\r\\n2\\r\\n3\\r\\n4\\r\\n5 scrolled" },
  { cols: 80, rows: 24, input: "\\x1b]0;title\\x07after osc \\x1b[38;5;196mred\\x1b[m" },
  { cols: 80, rows: 24, input: "wide: 中文 and emoji \u{1F600} end" },
];

const SGR_CASES = [
  "0",
  "1;3;4;9",
  "38;5;196;48;5;21",
  "38;2;10;20;30",
  "4:3;58;5;100",
  "2;5;7;8;53",
  "22;23;24;25;27;28;29;39;49;59",
  "90;101;97;40",
  "38:2::255:128:0",
  "999;1",
];

// [code, key, modifiers]
const KEYS = [
  ["KeyA", "a", {}],
  ["KeyA", "A", { shiftKey: true }],
  ["KeyC", "c", { ctrlKey: true }],
  ["KeyX", "x", { altKey: true }],
  ["KeyZ", "z", { ctrlKey: true, shiftKey: true }],
  ["Enter", "Enter", {}],
  ["Enter", "Enter", { shiftKey: true }],
  ["Tab", "Tab", { shiftKey: true }],
  ["Escape", "Escape", {}],
  ["Backspace", "Backspace", { ctrlKey: true }],
  ["ArrowUp", "ArrowUp", {}],
  ["ArrowLeft", "ArrowLeft", { altKey: true }],
  ["Home", "Home", { ctrlKey: true }],
  ["PageDown", "PageDown", {}],
  ["F1", "F1", {}],
  ["F12", "F12", { shiftKey: true }],
  ["Digit1", "!", { shiftKey: true }],
  ["Space", " ", { ctrlKey: true }],
  ["Numpad5", "5", {}],
  ["ShiftRight", "Shift", { shiftKey: true }],
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function die(code, msg) {
  console.error(msg);
  process.exit(code);
}

// --------------------------------------------------------------- environment
const CHROME = CHROME_CANDIDATES.find((p) => existsSync(p));
if (!CHROME) die(3, "wasm examples gate BLOCKED: no Chrome/Chromium binary found");
if (spawnSync("python3", ["--version"]).status !== 0)
  die(3, "wasm examples gate BLOCKED: python3 not found (it serves the pages)");

const ORACLE = execFileSync(
  "bash",
  ["-c", '. "$1" || exit 3; rustty_oracle_root "$2"', "oracle_root",
    join(ROOT, ".port/tools/gates/_oracle_root.sh"), ROOT],
  {
    env: Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("GIT_"))),
    encoding: "utf8",
  },
).trim();
if (!existsSync(join(ORACLE, "example/wasm-vt/index.html")))
  die(3, `wasm examples gate BLOCKED: no oracle checkout with example/wasm-vt at ${ORACLE}`);
if (spawnSync("zig", ["version"]).status !== 0)
  die(3, "wasm examples gate BLOCKED: zig not found (it builds the oracle's ghostty-vt.wasm)");

// Both modules are rebuilt every run (incremental, so cheap when current): a
// stale artifact from an older pin or an older tree would compare the wrong code.
console.log("--- building rustty-vt.wasm (.port/tools/build-vt-wasm.sh)");
let b = spawnSync("sh", [join(ROOT, ".port/tools/build-vt-wasm.sh")], { cwd: ROOT, stdio: "inherit" });
if (b.status !== 0) die(1, "wasm examples gate RED: build-vt-wasm.sh failed");
// Ghostty's CI command (test.yml `build-libghostty-vt-wasm`). zig-out and
// .zig-cache are gitignored in the oracle, so its tracked tree is untouched.
console.log("--- building ghostty-vt.wasm in the oracle (zig build -Demit-lib-vt -Dtarget=wasm32-freestanding)");
b = spawnSync(
  "zig",
  ["build", "-Demit-lib-vt", "-Dtarget=wasm32-freestanding", "-Doptimize=ReleaseSmall"],
  { cwd: ORACLE, stdio: "inherit" },
);
if (b.status !== 0) die(1, "wasm examples gate RED: the oracle's wasm build failed");
for (const p of [join(ROOT, "zig-out/bin/rustty-vt.wasm"), join(ORACLE, "zig-out/bin/ghostty-vt.wasm")])
  if (!existsSync(p)) die(1, `wasm examples gate RED: ${p} missing after its build`);

// ------------------------------------------------------- the oracle mirror
// zig-out and every example are symlinks into the oracle; only wasm-sgr's page
// is a corrected copy (UD380): the extra RESET_UNDERLINE entry dropped and every
// tag set to its value in Ghostty's own sgr.h.
const MIRROR = mkdtempSync(join(tmpdir(), "wasm-examples-oracle-"));
let ORIGINAL_SGR_TAGS = null;
{
  symlinkSync(join(ORACLE, "zig-out"), join(MIRROR, "zig-out"));
  mkdirSync(join(MIRROR, "example"));
  for (const d of readdirSync(join(ORACLE, "example"))) {
    if (d !== "wasm-sgr") symlinkSync(join(ORACLE, "example", d), join(MIRROR, "example", d));
  }
  const header = readFileSync(join(ORACLE, "include/ghostty/vt/sgr.h"), "utf8");
  const tags = Object.fromEntries(
    [...header.matchAll(/GHOSTTY_SGR_ATTR_([A-Z0-9_]+) = (\d+)/g)].map((m) => [m[1], m[2]]),
  );
  const page = readFileSync(join(ORACLE, "example/wasm-sgr/index.html"), "utf8");
  const m = page.match(/( *)const SGR_ATTR_TAGS = \{\n([\s\S]*?)\n *\};/);
  if (!m) die(1, "wasm examples gate RED: the oracle's wasm-sgr page has no SGR_ATTR_TAGS table");
  const names = [...m[2].matchAll(/([A-Z0-9_]+):/g)].map((x) => x[1]);
  if (!names.includes("RESET_UNDERLINE") || "RESET_UNDERLINE" in tags)
    die(1, "wasm examples gate RED: the oracle's wasm-sgr page no longer has UD380's extra RESET_UNDERLINE tag; " +
      "drop the UD380 patch from this gate and re-sync example/wasm-sgr");
  ORIGINAL_SGR_TAGS = Object.fromEntries([...m[2].matchAll(/([A-Z0-9_]+): (\d+)/g)].map((x) => [x[1], Number(x[2])]));
  const fixed = names.filter((n) => n in tags).map((n) => `${m[1]}    ${n}: ${tags[n]}`).join(",\n");
  mkdirSync(join(MIRROR, "example/wasm-sgr"));
  writeFileSync(
    join(MIRROR, "example/wasm-sgr/index.html"),
    page.replace(m[0], `${m[1]}const SGR_ATTR_TAGS = {\n${fixed}\n${m[1]}};`),
  );
}

// ------------------------------------------------------------------- plumbing
const procs = [];
let profile = null;
function cleanup() {
  for (const p of procs) p.kill();
  // Chrome may still be writing its profile as it exits; a leftover temp dir is
  // not a verdict.
  for (const d of [profile, MIRROR]) {
    try {
      if (d) rmSync(d, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    } catch {}
  }
}
process.on("exit", cleanup);

// Kill the server and Chrome, wait (bounded) for them to exit so Chrome is no
// longer writing its profile, then remove the temp dirs and exit.
async function finish(code) {
  const exited = procs.map((p) =>
    p.exitCode !== null || p.signalCode !== null ? null : new Promise((res) => p.once("exit", res)),
  );
  for (const p of procs) p.kill();
  await Promise.race([Promise.all(exited), sleep(5000)]);
  cleanup();
  process.exit(code);
}

async function pickPort() {
  const net = await import("node:net");
  return new Promise((res, rej) => {
    const s = net.createServer();
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => res(port));
    });
    s.on("error", rej);
  });
}

async function serve(dir, probe) {
  const port = await pickPort();
  procs.push(spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], { cwd: dir, stdio: "ignore" }));
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; ; i++) {
    try {
      await fetch(base + probe, { method: "HEAD" });
      return base;
    } catch {
      if (i > 100) die(3, `wasm examples gate BLOCKED: http.server for ${dir} never came up`);
      await sleep(50);
    }
  }
}

async function startChrome() {
  profile = mkdtempSync(join(tmpdir(), "wasm-examples-chrome-"));
  procs.push(
    spawn(CHROME, [
      "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`,
      "--no-first-run", "--no-default-browser-check", "--disable-extensions", "about:blank",
    ], { stdio: "ignore" }),
  );
  for (let i = 0; i < 300; i++) {
    const f = join(profile, "DevToolsActivePort");
    if (existsSync(f)) {
      const first = readFileSync(f, "utf8").split("\n")[0].trim();
      if (first) return first;
    }
    await sleep(50);
  }
  die(3, "wasm examples gate BLOCKED: Chrome never published DevToolsActivePort");
}

// One CDP session per page, with its console errors and exceptions collected.
async function openPage(devPort, url) {
  const t = await (await fetch(`http://127.0.0.1:${devPort}/json/new?${url}`, { method: "PUT" })).json();
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.addEventListener("open", res, { once: true });
    ws.addEventListener("error", rej, { once: true });
  });
  const pending = new Map();
  const errors = [];
  let id = 0;
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id !== undefined && pending.has(msg.id)) {
      const { res, rej } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) rej(new Error(JSON.stringify(msg.error)));
      else res(msg.result);
    } else if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      errors.push("console.error: " + msg.params.args.map((a) => a.value ?? a.description).join(" "));
    } else if (msg.method === "Runtime.exceptionThrown") {
      errors.push("exception: " + (msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text));
    }
  });
  const send = (method, params = {}) =>
    new Promise((res, rej) => {
      const myId = ++id;
      pending.set(myId, { res, rej });
      ws.send(JSON.stringify({ id: myId, method, params }));
    });
  await send("Runtime.enable");
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails)
      throw new Error(`${url}: ${r.exceptionDetails.exception?.description || r.exceptionDetails.text}`);
    return r.result.value;
  };
  // Ready when the page has cleared its "Loading WebAssembly module..." status
  // and enabled its input (both happen after the module instantiated).
  for (let i = 0; ; i++) {
    const ready = await evaluate(
      `document.readyState === "complete" && document.getElementById("status")?.textContent === "" && ` +
        `[...document.querySelectorAll("textarea,input")].some((e) => !e.disabled)`,
    ).catch(() => false);
    if (ready) break;
    if (i > 200) {
      const status = await evaluate(`document.getElementById("status")?.textContent`).catch(() => "?");
      throw new Error(`${url}: never became ready (status: ${JSON.stringify(status)}; ${errors.join("; ")})`);
    }
    await sleep(50);
  }
  return { evaluate, errors, close: () => ws.close() };
}

const output = (page) => page.evaluate(`document.getElementById("output").innerText`);
const toRustty = (s) => s.replaceAll("Ghostty", "Rustty").replaceAll("ghostty", "rustty").replaceAll("GHOSTTY", "RUSTTY");

function vacuous(s) {
  return !s || !s.trim() || /^Waiting for/.test(s) || /^Error/.test(s);
}

// ----------------------------------------------------------------- the cases
const failures = [];
let compared = 0;

const SHOW = process.argv.includes("--show");
function compare(label, g, r) {
  compared++;
  if (SHOW) console.log(`    ${label}\n      ${JSON.stringify(r).slice(0, 160)}`);
  if (vacuous(g) || vacuous(r)) {
    failures.push(`${label}: vacuous output (ghostty ${JSON.stringify(g?.slice(0, 80))}, rustty ${JSON.stringify(r?.slice(0, 80))})`);
    return;
  }
  if (toRustty(g) !== r) {
    failures.push(`${label}: outputs differ\n      ghostty: ${JSON.stringify(toRustty(g)).slice(0, 400)}\n      rustty:  ${JSON.stringify(r).slice(0, 400)}`);
  }
}

// The manifest arm: a page's hard-coded table against its module's manifest.
// `prefix` is "Ghostty" or "Rustty"; `table` evaluates to a {name: value} object
// in the page; `type` is the manifest type without its prefix.
async function manifestArm(label, page, prefix, table, type, mapName = (n) => n) {
  const r = await page.evaluate(
    `(() => { const t = ${table}; const v = typeLayout.types["${prefix}${type}"]?.values;` +
      ` return { t, v }; })()`,
  );
  if (!r.v) {
    failures.push(`${label}: the manifest has no ${prefix}${type}`);
    return;
  }
  let n = 0;
  const before = failures.length;
  for (const [k, val] of Object.entries(r.t)) {
    const name = mapName(k);
    if (name === null) continue;
    n++;
    if (r.v[name] !== val) failures.push(`${label}: ${k} = ${val} in the page, ${r.v[name]} in the manifest (${name})`);
  }
  if (n === 0) failures.push(`${label}: no entries checked`);
  else if (failures.length === before) console.log(`    ok      ${label}: ${n} page constants match ${prefix}${type}`);
}

// W3C `KeyboardEvent.code` -> the key enum's name (`KeyA` -> `A`, `Digit0` ->
// `DIGIT_0`, `LaunchApp1` -> `LAUNCH_APP_1`); the pages' comments name the same
// enum entry for each code.
const keyEnumName = (code) => {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^F\d+$/.test(code)) return code;
  return code.replace(/([a-z])([A-Z0-9])/g, "$1_$2").replace(/([A-Z])([A-Z][a-z])/g, "$1_$2").toUpperCase();
};
const ACTION_NAMES = { Press: "PRESS", Release: "RELEASE", Repeat: "REPEAT" };
const actionTable =
  `Object.fromEntries([...document.querySelectorAll('input[name="action"]')]` +
  `.map((r) => [r.parentElement.textContent.trim(), Number(r.value)]))`;

const vtRun = (c) =>
  `(() => { const s = (id, v) => { const e = document.getElementById(id); e.value = v; };` +
  ` s("cols", ${c.cols}); s("rows", ${c.rows}); s("vtInput", ${JSON.stringify(c.input)});` +
  ` document.getElementById("runBtn").click(); return true; })()`;
const sgrRun = (seq) =>
  `(() => { const e = document.getElementById("sequence"); e.value = ${JSON.stringify(seq)};` +
  ` e.dispatchEvent(new Event("input")); return true; })()`;
const keyRun = ([code, key, mods]) =>
  `(() => { document.getElementById("keyInput").dispatchEvent(new KeyboardEvent("keydown",` +
  ` ${JSON.stringify({ code, key, bubbles: true, cancelable: true, ...mods })})); return true; })()`;
const setFlags = (on) =>
  `(() => { document.querySelectorAll('.checkbox-group input[type="checkbox"]').forEach((c) => {` +
  ` if (c.checked !== ${on}) { c.checked = ${on}; c.dispatchEvent(new Event("change")); } }); return true; })()`;
const setAction = (v) =>
  `(() => { const r = document.querySelector('input[name="action"][value="${v}"]');` +
  ` if (!r) throw new Error("no action radio ${v}"); r.checked = true; r.dispatchEvent(new Event("change")); return true; })()`;

async function main() {
  const gBase = await serve(MIRROR, "/example/wasm-vt/index.html");
  const rBase = await serve(ROOT, "/example/wasm-vt/index.html");
  const devPort = await startChrome();
  const pages = [];
  const pair = async (ex) => {
    const g = await openPage(devPort, `${gBase}/example/${ex}/index.html`);
    const r = await openPage(devPort, `${rBase}/example/${ex}/index.html`);
    pages.push([ex, g, r]);
    return [g, r];
  };

  // wasm-vt
  {
    const [g, r] = await pair("wasm-vt");
    compare("wasm-vt default", await output(g), await output(r));
    for (const [i, c] of VT_CASES.entries()) {
      await g.evaluate(vtRun(c));
      await r.evaluate(vtRun(c));
      compare(`wasm-vt case ${i + 1}`, await output(g), await output(r));
    }
    // NC1: a different input on the Rustty side only must change the verdict.
    await g.evaluate(vtRun(VT_CASES[0]));
    await r.evaluate(vtRun({ ...VT_CASES[0], input: VT_CASES[0].input + " (control)" }));
    const ng = await output(g), nr = await output(r);
    if (vacuous(ng) || vacuous(nr) || toRustty(ng) === nr)
      failures.push("NC1: wasm-vt outputs did not differ for different inputs; the comparison is not seeing the output");
    else console.log("    ok      NC1: different wasm-vt inputs give different outputs");
  }

  // wasm-sgr
  {
    const [g, r] = await pair("wasm-sgr");
    await manifestArm("wasm-sgr tags (ghostty, UD380-corrected)", g, "Ghostty", "SGR_ATTR_TAGS", "SgrAttributeTag");
    await manifestArm("wasm-sgr tags (rustty)", r, "Rustty", "SGR_ATTR_TAGS", "SgrAttributeTag");
    // NC3: the manifest arm on the oracle's UNCORRECTED table must fail.
    const before = failures.length;
    await manifestArm("NC3 probe", g, "Ghostty", JSON.stringify(ORIGINAL_SGR_TAGS), "SgrAttributeTag");
    const caught = failures.splice(before);
    if (caught.length === 0) failures.push("NC3: the manifest arm passed the oracle's uncorrected SGR table (UD380)");
    else console.log(`    ok      NC3: the manifest arm flags the uncorrected table (${caught.length} wrong tags)`);
    compare("wasm-sgr default", await output(g), await output(r));
    for (const seq of SGR_CASES) {
      await g.evaluate(sgrRun(seq));
      await r.evaluate(sgrRun(seq));
      compare(`wasm-sgr ${JSON.stringify(seq)}`, await output(g), await output(r));
    }
  }

  // wasm-key-encode
  {
    const [g, r] = await pair("wasm-key-encode");
    for (const [side, page, prefix] of [["ghostty", g, "Ghostty"], ["rustty", r, "Rustty"]]) {
      await manifestArm(`wasm-key-encode key codes (${side})`, page, prefix, "keyCodeMap", "Key", keyEnumName);
      await manifestArm(`wasm-key-encode actions (${side})`, page, prefix, actionTable, "KeyAction", (n) => ACTION_NAMES[n] ?? n);
    }
    for (const flags of [true, false]) {
      await g.evaluate(setFlags(flags));
      await r.evaluate(setFlags(flags));
      for (const action of ["1", "0", "2"]) {
        await g.evaluate(setAction(action));
        await r.evaluate(setAction(action));
        for (const k of KEYS) {
          await g.evaluate(keyRun(k));
          await r.evaluate(keyRun(k));
          const label = `wasm-key-encode flags=${flags ? "all" : "none"} action=${action} ${k[0]}${JSON.stringify(k[2])}`;
          compare(label, await output(g), await output(r));
        }
      }
    }
    // NC2: KeyA on the Ghostty side, KeyB on the Rustty side.
    await g.evaluate(setFlags(true));
    await r.evaluate(setFlags(true));
    await g.evaluate(setAction("1"));
    await r.evaluate(setAction("1"));
    await g.evaluate(keyRun(["KeyA", "a", {}]));
    await r.evaluate(keyRun(["KeyB", "b", {}]));
    const ng = await output(g), nr = await output(r);
    if (vacuous(ng) || vacuous(nr) || toRustty(ng) === nr)
      failures.push("NC2: wasm-key-encode outputs did not differ for different keys");
    else console.log("    ok      NC2: different keys give different wasm-key-encode outputs");
  }

  for (const [ex, g, r] of pages) {
    for (const e of g.errors) failures.push(`${ex} (ghostty page) ${e}`);
    for (const e of r.errors) failures.push(`${ex} (rustty page) ${e}`);
    g.close();
    r.close();
  }

  if (failures.length) {
    console.error(`\nwasm examples gate RED: ${failures.length} problem(s) in ${compared} comparisons`);
    for (const f of failures.slice(0, 40)) console.error("  - " + f);
    await finish(1);
  }
  console.log(`\nWASM EXAMPLES GATE: GREEN (${compared} page outputs byte-identical to Ghostty's after the rename; tables match the manifests; NC1-NC3 behaved)`);
  await finish(0);
}

main().catch(async (e) => {
  console.error("wasm examples gate RED: " + (e.stack || e.message));
  await finish(1);
});
