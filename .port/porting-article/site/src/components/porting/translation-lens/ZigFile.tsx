import data from "@/app/porting/data/translation-lens.json";
import SourceLink from "../ui/SourceLink";
import { NOT_IN_GHOSTTY } from "./files";
import s from "./TranslationLens.module.css";

/** Renders a ledger row's Zig file (relative to Ghostty's src/), linked at the lens's Ghostty commit. */
export default function ZigFile({ file }: { file: string }) {
  const at = data.commits.zig;
  if (NOT_IN_GHOSTTY.has(file)) return <span className={s.file}>{file}</span>;
  return (
    <span className={s.file}>
      <SourceLink at={at} path={`src/${file}`} text={file} />
    </span>
  );
}
