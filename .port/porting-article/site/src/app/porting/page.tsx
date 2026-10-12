import { promises as fs } from "node:fs";
import * as nodePath from "node:path";
import type { Metadata } from "next";
import {
  PortingPhoneToc,
  PortingSideToc,
  type TocGroup,
} from "@/components/porting/toc/PortingToc";
import WidgetHashScroll from "@/components/porting/ui/WidgetHashScroll";
import { H1, P } from "@/components/text";
import customMdxStyles from "@/lib/docs/docs-mdx.module.css";
import { extractPageHeaders } from "@/lib/docs/page";
import Content from "./content.mdx";
import s from "./PortingPage.module.css";

/** Page metadata for the porting article. */
export const metadata: Metadata = {
  title: "Porting Ghostty to Rust",
  description:
    "How Ghostty was ported from Zig to Rust: architecture, aliasing, verification, defects found, performance and cost, with the data behind every number.",
};

/** CONTENT_PATH is the article source, read at build time for TOC extraction. */
const CONTENT_PATH = nodePath.join(
  process.cwd(),
  "src/app/porting/content.mdx",
);

/**
 * TOC_GROUPS opens a table-of-contents group at each listed section (keyed by
 * heading id). A section belongs to the last group opened at or before it, so
 * a newly inserted heading joins the group it falls in.
 */
const TOC_GROUPS: Readonly<Record<string, string>> = {
  "try-it-in-your-browser": "Try it",
  "at-a-glance": "Context",
  "cutting-a-345-file-cycle-into-crates": "Building it",
  "zig-pointers-in-rust's-aliasing-model": "Correctness",
  "how-the-orchestration-changed": "Process and cost",
  "limitations-and-what's-left": "Wrap-up",
};

/** HASH_FILTER_WIDGETS are the widgets whose filter can be linked as "#<slug>?…". */
const HASH_FILTER_WIDGETS = ["every-prompt"];

/**
 * Groups the article's level-2 headings for the table of contents, numbering
 * them continuously. A group key missing from the headings fails the build.
 */
function groupSections(
  headers: { id: string; title: string; depth: number }[],
): TocGroup[] {
  const sections = headers.filter((h) => h.depth === 2);
  for (const key of Object.keys(TOC_GROUPS)) {
    if (!sections.some((h) => h.id === key)) {
      throw new Error(`porting TOC: group heading #${key} not found`);
    }
  }
  const groups: TocGroup[] = [];
  sections.forEach((h, i) => {
    const label = TOC_GROUPS[h.id];
    if (label || groups.length === 0) {
      groups.push({ key: h.id, label: label ?? "Contents", items: [] });
    }
    groups[groups.length - 1].items.push({
      id: h.id,
      title: h.title,
      number: i + 1,
    });
  });
  return groups;
}

/** Renders the porting article with its table of contents. */
export default async function PortingPage() {
  const headers = await extractPageHeaders(
    await fs.readFile(CONTENT_PATH, "utf8"),
  );
  const groups = groupSections(headers);
  return (
    <div className={s.shell}>
      <WidgetHashScroll slugs={HASH_FILTER_WIDGETS} />
      <PortingPhoneToc groups={groups} />
      <div className={s.page}>
        <main className={s.main}>
          <header className={s.heading}>
            <H1>Porting Ghostty to Rust</H1>
            <P weight="regular" className={s.dek}>
              A technical account of porting the Ghostty terminal emulator from
              Zig to Rust with agents.
            </P>
            <P weight="regular" className={s.byline}>
              By{" "}
              <a href="https://www.linkedin.com/in/gaelhatchue">Gael Hatchue</a>{" "}
              · written with <a href="https://claude.com/claude-code">Claude</a>
            </P>
          </header>
          <article className={`${customMdxStyles.customMDX} ${s.article}`}>
            <Content />
          </article>
        </main>
        <PortingSideToc className={s.sidecar} groups={groups} />
      </div>
    </div>
  );
}
