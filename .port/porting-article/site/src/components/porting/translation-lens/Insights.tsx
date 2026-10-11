import CommitLink from "../ui/CommitLink";
import { insights, type TlData } from "./model";
import s from "./TranslationLens.module.css";

/** Renders the six computed insight tiles and the sources and methods note. */
export default function Insights({ data }: { data: TlData }) {
  return (
    <>
      <div className={s.ins}>
        {insights(data).map((it) => (
          <div key={it.value} className={s.in}>
            <div className={s.inV}>{it.value}</div>
            <div className={s.inD}>{it.text}</div>
          </div>
        ))}
      </div>
      <p className={s.foot}>
        Code: Ghostty at the reference commit{" "}
        <CommitLink hash={data.commits.zig} /> (libxev from its vendored
        package) and Rustty at <CommitLink hash={data.commits.rust} />,
        excerpted by line range with the real line numbers. Code mappings are
        drawn by hand; carried comments and Zig lines kept as Rust comments are
        matched automatically by exact text. Counts: <code>git grep -c</code>{" "}
        over Ghostty <code>src/</code> (plus libxev <code>src/</code> where
        marked) and Rustty <code>crates/</code>, tests included. Ledger: the
        port's pointer-lifetime ledger (
        {data.ledgerClasses.reduce((a, c) => a + c.n, 0)} rows,{" "}
        {data.ledgerClasses.length} ownership classes). Modules: tokei code
        lines per module group. unsafe: the unsafe census per crate, plus the
        same-line heuristic above. Miri: the isolated harness records. Perf: the
        recorded comptime-flattening fixes.
      </p>
    </>
  );
}
