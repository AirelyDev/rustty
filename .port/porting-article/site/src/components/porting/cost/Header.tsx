import s from "./Cost.module.css";
import {
  type CostData,
  fmtMoney,
  leverage,
  recoveredParts,
  type Scope,
  type Unit,
} from "./model";

/** Props for the header: data, the two toggles' values and their setters. */
type Props = {
  d: CostData;
  u: Unit;
  sc: Scope;
  onUnit: (u: Unit) => void;
  onScope: (sc: Scope) => void;
};

/** A two-button segmented toggle. */
function Seg<T extends string>({
  label,
  value,
  options,
  onPick,
}: {
  label: string;
  value: T;
  options: [T, string][];
  onPick: (v: T) => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a two-button toggle row, not a form fieldset
    <div className={s.seg} role="group" aria-label={label}>
      {options.map(([v, text]) => (
        <button
          key={v}
          type="button"
          aria-pressed={value === v}
          onClick={() => onPick(v)}
        >
          {text}
        </button>
      ))}
    </div>
  );
}

/**
 * The widget header: the unit ($ list price | tokens) and scope (Rustty
 * only | all projects) toggles; the two headline numbers, what was paid
 * (white) and the API-equivalent reference value (gold); and four tiles:
 * recovered (measured), lost window (modeled), leverage and the measured
 * Rustty share that supports the 50% assumption.
 */
export default function Header({ d, u, sc, onUnit, onScope }: Props) {
  const parts = recoveredParts(d);
  const lev = leverage(d);
  const [tl, tc, th] = d.api.total;
  const [ml, mc, mh] = d.api.modeled;
  return (
    <>
      <div className={s.tg}>
        <Seg
          label="Unit"
          value={u}
          options={[
            ["usd", "$ list price"],
            ["tok", "tokens"],
          ]}
          onPick={onUnit}
        />
        <Seg
          label="Scope"
          value={sc}
          options={[
            ["rustty", "Rustty only"],
            ["all", "all projects"],
          ]}
          onPick={onScope}
        />
      </div>
      <div className={s.heads}>
        <div>
          <div
            className={s.big}
          >{`~$${Math.round(d.paid.central / 10) * 10}`}</div>
          <div className={s.bigsub}>
            paid: Rustty's share of Claude Max 20x and one ChatGPT Pro month
            (range ~${Math.round(d.paid.central / 10) * 10}–${d.paid.high}, plus
            unknown extra-usage credits)
          </div>
        </div>
        <div>
          <div className={`${s.big} ${s.gold}`}>{fmtMoney(tc)}</div>
          <div className={s.bigsub}>
            the same usage at API list price, central estimate ({fmtMoney(tl)}–
            {fmtMoney(th)}). A reference value for scale, not money spent.
          </div>
        </div>
      </div>
      <div className={s.kpis}>
        <div className={s.kpi}>
          <div className={s.kv}>
            {fmtMoney(d.api.recovered)} <small>measured</small>
          </div>
          <div className={s.kl}>
            recovered Rustty usage at list price: Mac Claude{" "}
            {fmtMoney(parts.mac)}, Codex {fmtMoney(parts.codex)}, Linux VM
            Claude {fmtMoney(parts.vm)} (a lower bound)
          </div>
        </div>
        <div className={s.kpi}>
          <div className={s.kv}>
            {fmtMoney(mc)}{" "}
            <small>
              ${(ml / 1000).toFixed(1)}k–${(mh / 1000).toFixed(1)}k
            </small>
          </div>
          <div className={s.kl}>
            lost Mac Claude window (06-21 → 09-07), modeled as a maxed
            subscription × 50%
          </div>
        </div>
        <div className={s.kpi}>
          <div className={s.kv}>
            ~{Math.round(lev.central)}×{" "}
            <small>
              ≈{Math.round(lev.low)}–{Math.round(lev.high)}×
            </small>
          </div>
          <div className={s.kl}>
            list-price usage per subscription dollar attributable to Rustty
          </div>
        </div>
        <div className={s.kpi}>
          <div className={s.kv}>
            {(d.share * 100).toFixed(1)}% <small>measured</small>
          </div>
          <div className={s.kl}>
            Rustty's share of the four complete recovered quota weeks, which
            supports the 50% assumption
          </div>
        </div>
      </div>
    </>
  );
}
