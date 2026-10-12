/** Checks the live-terminal widget's pure modules and shipped wasm manifest. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { DEMO_SCRIPTS } from "../src/components/porting/live-terminal/demo-scripts.ts";
import { ENGINES } from "../src/components/porting/live-terminal/engines.ts";
import {
  ENV,
  OutputLog,
  OutputRouter,
  prompt,
  seedFiles,
} from "../src/components/porting/live-terminal/session.ts";

// Engines: ids unique, each wasm exists on disk under public/ and matches the manifest.
const manifest = JSON.parse(readFileSync("public/porting/wterm/manifest.json", "utf8"));
assert.deepEqual(ENGINES.map((e) => e.id).sort(), ["ghostty", "rustty"]);
for (const e of ENGINES) {
  const file = `public${e.wasmPath}`;
  assert.ok(existsSync(file), `${file} missing`);
  const bytes = readFileSync(file);
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    manifest[e.id].sha256,
    `${e.id} sha256`,
  );
  assert.equal(statSync(file).size, manifest[e.id].bytes, `${e.id} size`);
  assert.ok(bytes.subarray(0, 4).equals(Buffer.from([0, 0x61, 0x73, 0x6d])), `${e.id} is not wasm`);
}

// The redistributed Zig engine binary must ship with wterm's Apache-2.0 licence text.
const licence = "public/porting/wterm/LICENSE-wterm.txt";
assert.ok(existsSync(licence), `${licence} missing`);
assert.match(readFileSync(licence, "utf8"), /Apache License\s+Version 2\.0/, "wterm licence is not Apache-2.0");

// Seeded files: absolute paths under home, every demo script present with a shebang.
const files = seedFiles(DEMO_SCRIPTS);
for (const [path, text] of Object.entries(files)) {
  assert.ok(path.startsWith("/home/user/"), `seed outside home: ${path}`);
  assert.equal(typeof text, "string", `${path} is not text`);
}
assert.ok(Object.keys(DEMO_SCRIPTS).length > 0, "no demo scripts");
for (const path of Object.keys(DEMO_SCRIPTS)) {
  assert.ok(path in files, `demo script not seeded: ${path}`);
  assert.ok(files[path].startsWith("#!/bin/bash\n"), `${path} lacks shebang`);
}

// Scripts must not reach the network or run unbounded loops.
for (const [path, text] of Object.entries(DEMO_SCRIPTS)) {
  assert.ok(!/\b(curl|wget|nc|ssh)\b/.test(text), `${path} uses the network`);
  assert.ok(!/while\s+true|for\s*\(\(\s*;;/.test(text), `${path} loops forever`);
  assert.ok(text.length < 4000, `${path} is large`);
}

// Environment and prompt.
assert.equal(ENV.TERM, "xterm-256color");
assert.match(prompt("/home/user"), /\x1b\[1;32m.*\x1b\[0m:.*~/);

// OutputLog: bounded, drops oldest whole chunks, never splits a chunk.
const log = new OutputLog(64);
for (let i = 0; i < 20; i++) log.push(`chunk-${i}-`.padEnd(10, "x"));
assert.ok(log.text().length <= 64);
assert.ok(log.text().endsWith("chunk-19-x"), "keeps the newest chunk");
assert.ok(/^chunk-\d+-x+/.test(log.text()), "starts on a chunk boundary");
log.clear();
assert.equal(log.text(), "");

// Lazy-load guard: nothing wterm-related may be statically imported outside
// live-terminal/TerminalView.tsx, and TerminalView must only be reached through
// a dynamic import from Body.
/** Returns every file path under dir, recursively. */
const walk = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((d) =>
    d.isDirectory() ? walk(`${dir}/${d.name}`) : [`${dir}/${d.name}`],
  );
for (const file of walk("src").filter((f) => /\.(ts|tsx|mdx)$/.test(f))) {
  const text = readFileSync(file, "utf8");
  const heavy = /from\s+["'](@wterm\/[^"']+|just-bash)["']|import\s+["'](@wterm\/[^"']+|just-bash)["']/.test(text);
  if (heavy) {
    assert.equal(
      file,
      "src/components/porting/live-terminal/TerminalView.tsx",
      `${file} statically imports wterm/just-bash`,
    );
  }
}
const body = readFileSync("src/components/porting/live-terminal/Body.tsx", "utf8");
assert.ok(/import\(\s*["']\.\/TerminalView["']\s*\)/.test(body), "Body must dynamic-import TerminalView");
assert.ok(!/from\s+["']\.\/TerminalView["']/.test(body), "Body must not static-import TerminalView");
// A failed chunk load or render throw must reach Body's own failed panel
// (fallback link + retry), not the card's generic error boundary.
assert.ok(/getDerivedStateFromError/.test(body) && /componentDidCatch/.test(body), "Body must wrap TerminalView in an error boundary");
assert.ok(!/\blazy\(/.test(body), "Body must not cache the TerminalView import in React.lazy (a rejected lazy never retries)");

// OutputRouter: output reaches every attached terminal and the log; a terminal
// that joins late (or rejoins) first receives the whole session as one replay.
{
  const router = new OutputRouter(new OutputLog());
  const a = [];
  const b = [];
  router.attach("rustty", (c) => a.push(c));
  assert.deepEqual(a, [], "nothing to replay for the first terminal");
  router.write("one");
  router.write("two");
  router.attach("ghostty", (c) => b.push(c));
  assert.deepEqual(b, ["onetwo"], "a late terminal gets the session so far in one delivery");
  router.write("three");
  assert.deepEqual(a, ["one", "two", "three"]);
  assert.deepEqual(b, ["onetwo", "three"], "both terminals receive live output");
  router.detach("rustty");
  router.write("while-detached"); // recorded, delivered only to the terminal still attached
  assert.deepEqual(a, ["one", "two", "three"]);
  assert.equal(b.at(-1), "while-detached");
  router.attach("rustty", (c) => a.push(c));
  assert.equal(a.at(-1), "onetwothreewhile-detached", "a rejoining terminal replays what it missed");
}

// OutputRouter.writing is true exactly while output or a replay is delivered
// (a terminal's reply to a query happens inside that call and is dropped), and
// a terminal that throws neither sticks the flag nor starves the others.
{
  const router = new OutputRouter(new OutputLog());
  const seen = [];
  router.attach("x", () => seen.push(router.writing));
  router.write("q");
  router.attach("y", () => seen.push(router.writing));
  assert.deepEqual(seen, [true, true]);
  assert.equal(router.writing, false);
  const got = [];
  const r2 = new OutputRouter(new OutputLog());
  r2.attach("bad", () => {
    throw new Error("boom");
  });
  r2.attach("good", (c) => got.push(c));
  assert.throws(() => r2.write("x"), /boom/);
  assert.deepEqual(got, ["x"], "the other terminal still got the chunk");
  assert.equal(r2.writing, false, "writing is cleared after a throw");
}

// The article places the widget once, in a section that credits and links
// both Vercel Labs projects it runs (wterm and just-bash).
{
  const mdx = readFileSync("src/app/porting/content.mdx", "utf8");
  const placements = mdx.match(/<PortingWidget slug="live-terminal"/g) ?? [];
  assert.equal(placements.length, 1, "content.mdx must place the live-terminal widget exactly once");
  const at = mdx.indexOf('<PortingWidget slug="live-terminal"');
  const start = mdx.lastIndexOf("\n## ", at);
  const next = mdx.indexOf("\n## ", at);
  const section = mdx.slice(start, next < 0 ? mdx.length : next);
  assert.ok(/\]\(https:\/\/github\.com\/vercel-labs\/wterm[/)]/.test(section), "the live-terminal section must link wterm");
  assert.ok(/\]\(https:\/\/github\.com\/vercel-labs\/just-bash\)/.test(section), "the live-terminal section must link just-bash");
}

console.log("verify-porting-terminal: ok");
