import { fmt, md, type SyncTable as SyncTableData } from "./model";
import s from "./TestContract.module.css";

/** BIG_SYNC is the "Zig +" size above which a table row is highlighted. */
const BIG_SYNC = 300;

/** The per-sync table (upstream commits, Zig tests added, pre-covered, all met again) and the chart's method note. */
export default function SyncTable({
  dates,
  table,
}: {
  dates: string[];
  table: SyncTableData;
}) {
  return (
    <>
      <div className={s.tableWrap}>
        <table className={s.table}>
          <thead>
            <tr>
              <th scope="col">sync</th>
              <th scope="col">upstream</th>
              <th scope="col">Zig +</th>
              <th scope="col">pre-covered</th>
              <th scope="col">all met</th>
            </tr>
          </thead>
          <tbody>
            {table.rows.map((r) => (
              <tr key={r.k} className={r.added > BIG_SYNC ? s.hl : undefined}>
                <td>
                  {r.k} · {md(r.date)}
                </td>
                <td>{r.upstream}</td>
                <td>+{r.added}</td>
                <td>
                  {r.pre} ({Math.round((100 * r.pre) / r.added)}%)
                </td>
                <td>
                  {r.allMet < 0 ? (
                    "—"
                  ) : (
                    <>
                      {md(dates[r.allMet])}{" "}
                      {r.lag > 0 ? (
                        <span className={s.lag}>+{r.lag}d</span>
                      ) : (
                        <span className={s.same}>same day</span>
                      )}
                    </>
                  )}
                </td>
              </tr>
            ))}
            <tr>
              <td className={s.total}>total</td>
              <td>{fmt(table.upstream)}</td>
              <td>+{fmt(table.added)}</td>
              <td>
                {table.pre} ({Math.round((100 * table.pre) / table.added)}%)
              </td>
              <td />
            </tr>
          </tbody>
        </table>
      </div>
      <p className={s.note}>
        Teal steps: Zig test blocks (regex <code>^\s*test\s*("|{"{"})</code>{" "}
        over <code>src/</code>), and the 211-module mapped subset (dashed,
        includes the libxev/libvaxis/zf/wuffs packages). Copper: Rust{" "}
        <code>#[test]</code> over <code>crates/ crates-sys/ xtask/</code>,
        daily. "Pre-covered" is how many of the new Zig tests a module's
        existing surplus already covered by count the day before the sync. A
        count can be met by unrelated extra tests, which is why the name-level
        checklist exists. "All met" excludes one mapping artifact (a config test
        that lived in another file until a 10-01 split).
      </p>
    </>
  );
}
