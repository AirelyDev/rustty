import { barPercents, BIRTHS_W, birthDots } from "./model";
import Rich from "./Rich";
import s from "./ThreeReviewers.module.css";
import type { Bug } from "./types";

/**
 * Bottom row: the rule that came out of the bug (verbatim, with its source)
 * and how often the pattern that fixed it appears before the fix, at the fix
 * and today; bug B adds a strip of harness-crate birth dates.
 */
export default function Bottom({ bug }: { bug: Bug }) {
  const c = bug.count;
  const pct = barPercents(c.series);
  return (
    <div className={s.bottom}>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>The rule that came out of it</span>
        </div>
        <blockquote className={s.rule}>
          <Rich text={bug.rule.text} />
        </blockquote>
        <div className={s.ruleSrc}>
          <Rich text={bug.rule.src} links />
        </div>
      </div>
      <div className={s.panel}>
        <div className={s.ph}>
          <span>How often the pattern appears now</span>
          <i>counted with a signature scan</i>
        </div>
        <div className={s.note}>
          <Rich text={c.what} />
        </div>
        {c.series.map(([label, n], i) => (
          <div key={label} className={s.bar}>
            <span>
              <Rich text={label} links />
            </span>
            <span className={s.tr}>
              <span className={s.fl} style={{ width: `${pct[i]}%` }} />
            </span>
            <span className={s.num}>{n}</span>
          </div>
        ))}
        {c.births ? (
          <svg
            className={s.births}
            viewBox={`0 0 ${BIRTHS_W} 64`}
            role="img"
            aria-label={`${c.births.length} harness crates added between ${c.births[0]} and ${c.births[c.births.length - 1]}`}
          >
            {birthDots(c.births).map((d, i) => (
              <circle
                key={`${i}-${d.cx}`}
                cx={d.cx}
                cy={d.cy}
                r={2.1}
                fill="#ffbc7a"
              />
            ))}
            <line x1={6} x2={BIRTHS_W - 6} y1={60} y2={60} stroke="#262927" />
            <text
              x={6}
              y={8}
              fontSize="7"
              fill="#858a84"
              fontFamily="monospace"
            >
              harness crates added, 06-21 → 10-08
            </text>
          </svg>
        ) : null}
        <div className={`${s.ruleSrc} ${s.countNote}`}>
          <Rich text={c.note} />
        </div>
      </div>
    </div>
  );
}
