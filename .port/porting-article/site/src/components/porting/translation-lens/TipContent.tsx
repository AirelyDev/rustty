import { fmtInt } from "../lib/format";
import { KIND_NAME } from "./CodeLens";
import {
  groups,
  moduleDots,
  type PatternCrates,
  rangeText,
  type Sub,
  type TlData,
  type UnsafeMetric,
  unsafeRows,
} from "./model";
import s from "./TranslationLens.module.css";

/** Props for TipContent. */
type TipContentProps = {
  /** Tooltip id: "map:<group>", "mod:<row>" or "uns:<row>". */
  id: string;
  data: TlData;
  sub: Sub;
  pc: PatternCrates;
  modMetric: 0 | 1 | 2;
  unsMetric: UnsafeMetric;
};

/** Renders the floating tooltip for a map ribbon, a module row or a crate row. */
export default function TipContent({
  id,
  data,
  sub,
  pc,
  modMetric,
  unsMetric,
}: TipContentProps) {
  const [kind, raw] = id.split(":");
  const i = Number(raw);
  if (kind === "map") {
    const g = groups(sub)[i];
    if (!g) return null;
    return (
      <>
        <b>{KIND_NAME[g.k]}</b>
        <span className={s.tipLine}>
          Zig {rangeText(g.z)} → Rust {rangeText(g.r)}
        </span>
      </>
    );
  }
  if (kind === "mod") {
    const d = moduleDots(data.modules, modMetric, pc)[i];
    if (!d) return null;
    const m = d.row;
    const sites = m.crates
      .filter((c) => pc.all[c])
      .map((c) => `${c} ${pc.all[c]}`);
    return (
      <>
        <b>{m.module}</b> → {m.crates.join(" + ")}
        <span className={s.tipLine}>
          code {fmtInt(m.zig[0])} → {fmtInt(m.rust[0])}
          {m.zig[0] ? ` (${(m.rust[0] / m.zig[0]).toFixed(2)}×)` : ""}
        </span>
        <span className={s.tipLine}>
          comments {fmtInt(m.zig[1])} → {fmtInt(m.rust[1])}
        </span>
        <span className={s.tipLine}>
          tests {fmtInt(m.zig[2])} → {fmtInt(m.rust[2])}
        </span>
        {sites.length ? (
          <span className={s.tipGold}>pattern sites: {sites.join(", ")}</span>
        ) : null}
      </>
    );
  }
  if (kind === "uns") {
    const c = unsafeRows(data.unsafeCrates, unsMetric)[i];
    if (!c) return null;
    const on = pc.top.includes(c.crate);
    return (
      <>
        <b>{c.crate}</b> ({c.tier})
        <span className={s.tipLine}>
          {fmtInt(c.sites)} unsafe sites, {fmtInt(c.nonTest)} outside tests
        </span>
        <span className={s.tipLine}>
          {c.perKloc} / kLOC all · {c.ntPerKloc} / kLOC non-test
        </span>
        <span className={s.tipLine}>
          {fmtInt(c.code)} code lines · {fmtInt(c.safety)} SAFETY comments
        </span>
        {on ? (
          <span className={s.tipGold}>
            ◆ {pc.all[c.crate]} sites of this pattern (primary marker or trait
            methods)
          </span>
        ) : null}
      </>
    );
  }
  return null;
}
