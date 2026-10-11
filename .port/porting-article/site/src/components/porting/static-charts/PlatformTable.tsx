import data from "@/app/porting/data/platform-table.json";
import CommitLink from "../ui/CommitLink";
import Tooltip from "../ui/Tooltip";
import { TipBody } from "./Hot";
import { statusMark } from "./model";
import s from "./StaticCharts.module.css";

/** One status cell of the platform table: status, short text and detail. */
type Cell = { status: string; text: string; detail: string };

/** STATUS_CLASS maps a status to its colour class (the glyph and word carry it too). */
const STATUS_CLASS: Record<string, string> = {
  yes: s.stYes,
  partial: s.stPartial,
  no: s.stNo,
  na: s.stNa,
};

/** One status cell: glyph, plain status word and short text, with the detail in a tooltip. */
function StatusCell({
  id,
  column,
  platform,
  cell,
}: {
  id: string;
  column: string;
  platform: string;
  cell: Cell;
}) {
  const mark = statusMark(cell.status);
  return (
    <td data-label={column}>
      <Tooltip
        id={id}
        content={
          <TipBody
            title={`${platform} · ${column}: ${mark.text}`}
            rows={[cell.detail]}
          />
        }
      >
        <span className={`${s.status} ${STATUS_CLASS[cell.status] ?? ""}`}>
          <span aria-hidden="true">{mark.glyph}</span>
          <span className={s.srOnly}>{mark.text}: </span>
          {cell.text}
        </span>
      </Tooltip>
    </td>
  );
}

/**
 * What runs where: one row per platform with what builds, what runs, what was
 * tested by hand, which verification lanes ran and the latest evidence date,
 * then the platform's one-line claim. On phones each platform becomes a card.
 */
export default function PlatformTable() {
  return (
    <div className={s.tableWrap}>
      <table className={s.table}>
        <thead>
          <tr>
            <th scope="col">Platform</th>
            {data.columns.map((c) => (
              <th key={c.key} scope="col">
                {c.label}
              </th>
            ))}
            <th scope="col">Latest evidence</th>
          </tr>
        </thead>
        {data.rows.map((r) => (
          <tbody key={r.key} className={s.platform}>
            <tr>
              <th scope="row">
                <span className={s.platName}>{r.platform}</span>
                <span className={s.platSub}>{r.sub}</span>
              </th>
              {data.columns.map((c) => (
                <StatusCell
                  key={c.key}
                  id={`plat-${r.key}-${c.key}`}
                  column={c.label}
                  platform={r.platform}
                  cell={(r as unknown as Record<string, Cell>)[c.key]}
                />
              ))}
              <td data-label="Latest evidence" className={s.mono}>
                {r.last}
                {"lastRef" in r && r.lastRef ? (
                  <>
                    {" "}
                    <span className={s.platSub}>
                      as of <CommitLink hash={r.lastRef} />
                    </span>
                  </>
                ) : null}
              </td>
            </tr>
            <tr className={s.claimRow}>
              <td colSpan={data.columns.length + 2}>{r.claim}</td>
            </tr>
          </tbody>
        ))}
      </table>
      <p className={s.legendLine}>
        <span className={s.stYes}>● yes</span>
        <span className={s.stPartial}>◐ partly</span>
        <span className={s.stNo}>○ no</span>
        <span className={s.stNa}>– not applicable</span>
        <span>Hover, tap or focus a cell for the evidence.</span>
      </p>
    </div>
  );
}
