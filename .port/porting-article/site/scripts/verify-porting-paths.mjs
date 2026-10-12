/**
 * Verifies that every GitHub file or directory permalink the porting article
 * renders exists at its commit: literal <SourceLink> props in the prose and
 * components, blob/tree URLs in the prose, facts, registry and components,
 * and the data-driven file links (cutting-the-cycle crate-detail files and
 * directories, defect-map files,
 * translation-lens ledger files). Checks run with `git cat-file` against local
 * clones: $GHOSTTY_REPO (default /Users/gael/projects/ghostty) and
 * $RUSTTY_REPO (default /Users/gael/projects/rustty).
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { NOT_IN_GHOSTTY } from "../src/components/porting/translation-lens/files.ts";

/** REPOS maps a GitHub owner/name to its local clone. */
const REPOS = {
  "ghostty-org/ghostty": process.env.GHOSTTY_REPO ?? "/Users/gael/projects/ghostty",
  "AirelyDev/rustty": process.env.RUSTTY_REPO ?? "/Users/gael/projects/rustty",
};

/** DATA_DIR holds the generated article data. */
const DATA_DIR = "src/app/porting/data";

/** commits maps commit tokens to "<repo code or owner/name>:<sha>". */
const commits = JSON.parse(readFileSync(`${DATA_DIR}/commits.json`, "utf8"));

/** CODES expands the short repository codes of commits.json. */
const CODES = { r: "AirelyDev/rustty", g: "ghostty-org/ghostty" };

/** Resolves a commit token to [owner/name, sha], or null. */
function resolve(token) {
  const v = commits[token] ?? Object.values(commits).find((x) => x.slice(x.lastIndexOf(":") + 1).startsWith(token) && x.length > 40);
  if (!v) return null;
  const code = v.slice(0, v.lastIndexOf(":"));
  return [CODES[code] ?? code, v.slice(v.lastIndexOf(":") + 1)];
}

/** Every check: [repo owner/name, sha, path, where]. */
const wanted = [];

/** Lists files under dir recursively. */
function walk(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

/** SOURCES are the files whose literal links are checked. */
const SOURCES = ["src/app/porting/content.mdx", "src/components/porting/lib/registry.ts", ...walk("src/components/porting").filter((f) => /\.tsx?$/.test(f))];
for (const file of [...new Set(SOURCES)]) {
  const text = readFileSync(file, "utf8");
  for (const m of text.matchAll(/<SourceLink\s+at="([0-9a-f]+)"\s+path="([^"]+)"/g)) {
    const c = resolve(m[1]);
    if (!c) wanted.push([null, m[1], m[2], file]);
    else wanted.push([c[0], c[1], m[2], file]);
  }
  for (const m of text.matchAll(/https:\/\/github\.com\/(AirelyDev\/rustty|ghostty-org\/ghostty)\/(?:blob|tree)\/([0-9a-f]{40})\/([^\s"')#]+)/g)) {
    wanted.push([m[1], m[2], m[3], file]);
  }
}
const facts = JSON.parse(readFileSync(`${DATA_DIR}/facts.json`, "utf8"));
for (const [id, f] of Object.entries(facts)) {
  const m = f.href?.match(/^https:\/\/github\.com\/(AirelyDev\/rustty|ghostty-org\/ghostty)\/(?:blob|tree)\/([0-9a-f]{40})\/([^#]+)/);
  if (m) wanted.push([m[1], m[2], m[3], `fact ${id}`]);
}

/** Adds data-driven Ghostty file links at a commit token. */
function ghosttyFiles(token, paths, where) {
  const c = resolve(token);
  for (const p of paths) wanted.push([c?.[0] ?? null, c?.[1] ?? token, p, where]);
}
const cc = JSON.parse(readFileSync(`${DATA_DIR}/cutting-the-cycle.json`, "utf8"));
ghosttyFiles("4749c4e93", cc.zig.files.map((f) => `src/${f[0]}`), "cutting-the-cycle files");
ghosttyFiles("4749c4e93", cc.crates.nodes.flatMap((n) => (n[12] ?? []).map(([d]) => `src/${d}`)), "cutting-the-cycle crate detail files");
const dm = JSON.parse(readFileSync(`${DATA_DIR}/defect-map.json`, "utf8"));
ghosttyFiles("246f70287", dm.defs.filter((d) => d.p === "ghostty" && d.f).map((d) => d.f), "defect-map files");
const tl = JSON.parse(readFileSync(`${DATA_DIR}/translation-lens.json`, "utf8"));
const tlFiles = new Set([...tl.patterns.flatMap((p) => p.ledger.map((r) => r[0])), ...tl.ledgerClasses.flatMap((c) => c.sample.map((r) => r[0]))]);
ghosttyFiles(tl.commits.zig, [...tlFiles].filter((f) => !NOT_IN_GHOSTTY.has(f)).map((f) => `src/${f}`), "translation-lens ledger files");

/** Collected failures. */
const failures = [];
/** byRepo groups the unique checks per repository. */
const byRepo = new Map();
for (const [repo, sha, path, where] of wanted) {
  if (!repo) {
    failures.push(`${where}: commit ${sha} resolves in no repository (${path})`);
    continue;
  }
  const key = `${sha}:${path.replace(/\/+$/, "")}`;
  if (!byRepo.has(repo)) byRepo.set(repo, new Map());
  byRepo.get(repo).set(key, where);
}
for (const [repo, checks] of byRepo) {
  const dir = REPOS[repo];
  if (!existsSync(dir)) {
    failures.push(`no local clone of ${repo} at ${dir} (set ${repo.startsWith("ghostty") ? "GHOSTTY_REPO" : "RUSTTY_REPO"})`);
    continue;
  }
  const keys = [...checks.keys()];
  const out = execFileSync("git", ["-C", dir, "cat-file", "--batch-check"], { input: `${keys.join("\n")}\n`, encoding: "utf8" });
  out.trim().split("\n").forEach((line, i) => {
    if (/ missing$/.test(line)) failures.push(`${checks.get(keys[i])}: ${repo} has no ${keys[i]}`);
  });
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log(`porting paths OK (${wanted.length} links, ${[...byRepo.values()].reduce((a, m) => a + m.size, 0)} unique)`);
