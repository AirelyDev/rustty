/**
 * Verifies that every quote in the porting article that links a committed
 * Rustty source quotes it verbatim: the quoted span (a "…" around the link,
 * or the whole blockquote line that holds the link), with markup
 * stripped, must appear within the anchored line range at that sha, modulo
 * […] elisions, [bracketed insertions], whitespace, line breaks and markdown
 * emphasis. Reads the source with `git show` from $RUSTTY_REPO (default
 * /Users/gael/projects/rustty).
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/** RUSTTY_REPO is the local Rustty clone the quoted sources are read from. */
const RUSTTY_REPO = process.env.RUSTTY_REPO ?? "/Users/gael/projects/rustty";

/** LINK_RE finds a markdown link to a Rustty blob with a line anchor. */
const LINK_RE =
  /\[((?:[^\]\\]|\\.)*)\]\(https:\/\/github\.com\/AirelyDev\/rustty\/blob\/([0-9a-f]{40})\/([^)#]+)#L(\d+)(?:-L(\d+))?\)/g;

/** Replaces every markdown link in text by its link text. */
function unlink(text) {
  return text.replace(/\[((?:[^\]\\]|\\.)*)\]\([^)\s]*\)/g, "$1");
}

/** Normalises text for comparison: emphasis, code ticks, comment and list markers, whitespace. */
function norm(text) {
  return text
    .replace(/^\s*(?:#|>|-)\s?/gm, " ")
    .replace(/\*\*|\*|`/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Returns the quoted span around a link at [start, end) of line, or null when the link is not quoted. */
function quotedSpan(line, start, end) {
  if (line[start - 1] === '"') {
    const close = line.indexOf('"', end);
    return close < 0 ? null : line.slice(start, close);
  }
  if (/^>\s/.test(line)) return line.replace(/^>\s*/, "");
  return null;
}

/** Results per quote: [line number, path#range, ok, quote]. */
const results = [];
const lines = readFileSync("src/app/porting/content.mdx", "utf8").split("\n");
lines.forEach((line, i) => {
  for (const m of line.matchAll(LINK_RE)) {
    const span = quotedSpan(line, m.index, m.index + m[0].length);
    if (span === null) continue;
    const [, , sha, path, a, b] = m;
    const src = execFileSync("git", ["-C", RUSTTY_REPO, "show", `${sha}:${path}`], { encoding: "utf8", maxBuffer: 64 << 20 })
      .split("\n")
      .slice(Number(a) - 1, Number(b ?? a))
      .join("\n");
    const hay = norm(src);
    // […] marks an elision; other [bracketed] text is an insertion and is dropped.
    const parts = unlink(span)
      .split("[…]")
      .map((p) => norm(p.replace(/\[[^\]]*\]/g, " ")))
      .filter(Boolean);
    let at = 0;
    let ok = true;
    for (const p of parts) {
      const k = hay.indexOf(p, at);
      if (k < 0) {
        ok = false;
        break;
      }
      at = k + p.length;
    }
    results.push([i + 1, `${path}#L${a}${b ? `-L${b}` : ""}`, ok, norm(unlink(span))]);
  }
});

for (const [n, where, ok, q] of results) console.log(`${ok ? "ok  " : "MISS"} ${n} ${where}: ${q.slice(0, 90)}`);
const misses = results.filter((r) => !r[2]);
if (misses.length) {
  console.error(`${misses.length} quote(s) not verbatim in their linked source`);
  process.exit(1);
}
console.log(`porting quotes OK (${results.length} linked quotes verbatim)`);
