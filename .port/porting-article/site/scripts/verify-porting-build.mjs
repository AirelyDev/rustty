/** Verifies the statically built /porting page contains every widget's SSR shell and the TOC anchors. */
import { readFileSync, existsSync } from "node:fs";
import remarkFrontmatter from "remark-frontmatter";
import remarkGfm from "remark-gfm";
import remarkMdx from "remark-mdx";
import remarkParse from "remark-parse";
import slugify from "slugify";
import { unified } from "unified";
import { visit } from "unist-util-visit";
import { WIDGETS } from "../src/components/porting/lib/registry.ts";

/** HTML_PATH is where `next build` writes the prerendered /porting document. */
const HTML_PATH = ".next/server/app/porting.html";

/** MDX_PATH is the article source whose widgets and headings must appear in the HTML. */
const MDX_PATH = "src/app/porting/content.mdx";

/** SUMMARY_PREFIX_LENGTH is how many summary characters must appear server-rendered. */
const SUMMARY_PREFIX_LENGTH = 40;

/** HTML_ESCAPES maps each character React escapes in text to its entity. */
const HTML_ESCAPES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#x27;" };

/** Escapes text the way React does when rendering it into HTML text or attribute values. */
function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (ch) => HTML_ESCAPES[ch]);
}

/**
 * Returns the anchor ids of the MDX's level-2 headings. The MDX is parsed the
 * way the site parses it (remark-parse + remark-mdx + remark-frontmatter + remark-gfm), and each id
 * is computed exactly as src/lib/docs/remark-heading-ids.mjs does: the heading's
 * text children joined, slugify(text.toLowerCase()), and a -N suffix for the
 * Nth duplicate, counted across headings of every level.
 */
function headingIds(mdx) {
  const tree = unified().use(remarkParse).use(remarkMdx).use(remarkFrontmatter).use(remarkGfm).parse(mdx);
  const encountered = new Map();
  const ids = [];
  visit(tree, "heading", (node) => {
    if (!Array.isArray(node.children) || node.children.length === 0) return;
    const text = node.children.map((child) => (typeof child.value === "string" ? child.value : "")).join("");
    const baseId = slugify(text.toLowerCase());
    const count = (encountered.get(baseId) || 0) + 1;
    encountered.set(baseId, count);
    if (node.depth === 2) ids.push({ text, id: count >= 2 ? `${baseId}-${count}` : baseId });
  });
  return ids;
}

if (!existsSync(HTML_PATH)) {
  console.error(`${HTML_PATH} missing — run npm run build first`);
  process.exit(1);
}

/** html is the prerendered /porting document. */
const html = readFileSync(HTML_PATH, "utf8");

/** Collected failures; the script exits non-zero if any are recorded. */
const failures = [];

/** mdx is the article source. */
const mdx = readFileSync(MDX_PATH, "utf8");

for (const [, slug] of mdx.matchAll(/<PortingWidget slug="([a-z0-9-]+)"/g)) {
  const w = WIDGETS.find((x) => x.slug === slug);
  if (!html.includes(`data-porting-widget="${slug}"`)) failures.push(`${slug}: shell missing`);
  if (!w) failures.push(`${slug}: not in the registry`);
  else if (!html.includes(escapeHtml(w.summary.slice(0, SUMMARY_PREFIX_LENGTH)))) {
    failures.push(`${slug}: summary not server-rendered`);
  } else if (w.deck && !html.includes(escapeHtml(w.deck.slice(0, SUMMARY_PREFIX_LENGTH)))) {
    failures.push(`${slug}: deck not server-rendered`);
  }
}
for (const { text, id } of headingIds(mdx)) {
  if (!html.includes(`id="${escapeHtml(id)}"`)) failures.push(`anchor missing for "${text}" (#${id})`);
}
if (/\bUD\d+|\bG3b\b|\bR13\b/.test(html.replace(/<script[\s\S]*?<\/script>/g, ""))) {
  failures.push("internal id in rendered HTML");
}
if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("porting build OK");
