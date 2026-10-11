import { useMemo } from "react";
import type { MtData } from "./model";
import { insights } from "./stats";
import s from "./MovingTarget.module.css";

/** The three insight tiles under the matrix, derived from the data. */
export default function Insights({ data }: { data: MtData }) {
  const tiles = useMemo(() => insights(data), [data]);
  return (
    <div className={s.insights}>
      {tiles.map((t) => (
        <div key={t.n} className={s.ins}>
          <span className={`${s.insN} ${s.mono}`}>{t.n}</span>
          {t.t}
        </div>
      ))}
    </div>
  );
}
