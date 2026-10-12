import CommitLink from "../ui/CommitLink";
import { type MsRule, shortDate } from "./model";
import s from "./MemorySafety.module.css";

/**
 * The list of the eight rule flags under the chart: number, date, the rule in
 * plain words and the commit that introduced it. Hovering or focusing an item
 * highlights its flag and dashed guide in the chart.
 */
export default function RuleList({
  rules,
  hover,
  onHover,
}: {
  rules: MsRule[];
  hover: number | null;
  onHover: (i: number | null) => void;
}) {
  return (
    <ol className={s.rules} aria-label="Working rules the bug waves produced">
      {rules.map((r, i) => (
        <li
          key={r.ref}
          className={hover === i ? s.ruleOn : undefined}
          // biome-ignore lint/a11y/noNoninteractiveTabindex: focusing a rule highlights its flag in the chart
          tabIndex={0}
          onMouseEnter={() => onHover(i)}
          onMouseLeave={() => onHover(null)}
          onFocus={() => onHover(i)}
          onBlur={() => onHover(null)}
        >
          <span className={s.ruleNo}>{i + 1}</span>
          <span className={s.ruleDate}>{shortDate(r.d)}</span>
          <span>
            {r.t} <CommitLink hash={r.ref} />
          </span>
        </li>
      ))}
    </ol>
  );
}
