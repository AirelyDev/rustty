import CommitLink from "../ui/CommitLink";
import { PALETTE } from "../lib/palette";
import Tooltip from "../ui/Tooltip";
import { fmt, type SkipData } from "./model";
import s from "./TestContract.module.css";

/** SILENT_SKIPS_URL is the permalink of the silent-skip check in the commit that added it. */
const SILENT_SKIPS_URL =
  "https://github.com/AirelyDev/rustty/blob/1096fa4cd0ddf6d877190c443cbd52e1b2a12e29/.port/tools/gates/check_silent_skips.py";

/** One stacked-bar segment: count, colour and label. */
type Seg = [number, string, string];

/** COMPILE_TIME are the guard classes that are compile-time scopes (the copper family). */
const COMPILE_TIME = [
  "build feature / option",
  "target OS / arch",
  "build profile",
];

/** GUARD_COLORS maps each guard class to its colour: copper family compile-time, blue family runtime. */
const GUARD_COLORS: Record<string, string> = {
  "build feature / option": PALETTE.rust,
  "target OS / arch": "#c96d2e",
  "build profile": "#a85a26",
  "GPU / display / window server": PALETTE.blue,
  "system fonts": "#4f93cc",
  "reference build / corpus / git checkout": "#3f7aab",
  "pty / tty": "#36688f",
  "optional decoder not installed": "#2f5a7c",
};

/** KIND_NAMES are the reader-facing names of the lint's finding kinds. */
const KIND_NAMES: Record<string, string> = {
  RET: "early return, nothing asserted",
  EMPTY: "body entirely #[cfg]-gated",
  MSG: '"skipping" print, no strict switch',
  WRAP: "asserts only inside if, no else",
};

/** A stacked bar whose segments show their label and count on hover, tap or focus. */
function Stack({ id, parts }: { id: string; parts: Seg[] }) {
  return (
    <div className={s.stack}>
      {parts.map(([n, color, label]) => (
        <div key={label} className={s.seg} style={{ flex: `${n} 0 0` }}>
          <Tooltip
            id={`${id}-${label}`}
            content={
              <>
                {label}: <b>{fmt(n)}</b>
              </>
            }
          >
            <span className={s.segFill} style={{ background: color }}>
              <span className={s.srOnly}>{label}</span>
            </span>
          </Tooltip>
        </div>
      ))}
    </div>
  );
}

/** Horizontal bars: label, bar scaled to max, count. */
function Bars({
  rows,
  max,
  color,
}: {
  rows: [string, number][];
  max: number;
  color: (k: string) => string;
}) {
  return (
    <div>
      {rows.map(([k, n]) => (
        <div key={k} className={s.bar}>
          <span>{k}</span>
          <div
            className={s.barFill}
            style={{
              width: `${((100 * n) / max).toFixed(1)}%`,
              background: color(k),
            }}
          />
          <span className={s.barN}>{n}</span>
        </div>
      ))}
    </div>
  );
}

/** A coloured square marker used in notes. */
function Sq({ color }: { color: string }) {
  return <span style={{ color }}>■</span>;
}

/**
 * The silent-skip census: Zig's skip-capable tests and the Rust suite by skip
 * mechanism, the 185 silent skips by what their guard checked, by lint finding
 * and by crate, and three insight tiles.
 */
export default function SkipPanel({ skip }: { skip: SkipData }) {
  const z = skip.zig;
  const r = skip.rust;
  const total = Object.values(skip.byKind).reduce((a, b) => a + b, 0);
  const compile = skip.grp["compile-time scope"];
  const runtime = skip.grp["runtime resource"];
  const cls = Object.entries(skip.byCls).sort(
    (a, b) =>
      Number(COMPILE_TIME.includes(b[0])) -
        Number(COMPILE_TIME.includes(a[0])) || b[1] - a[1],
  );
  const kinds = Object.entries(skip.byKind)
    .sort((a, b) => b[1] - a[1])
    .map(([k, n]) => [KIND_NAMES[k] ?? k, n] as [string, number]);
  /** Returns the number of the 185 sites in guard class k. */
  const runtimeCls = (k: string) => skip.byCls[k] ?? 0;

  return (
    <div className={s.subPanel}>
      <div className={s.subHead}>
        <a className={`${s.subPill} ${s.subPillLink}`} href={SILENT_SKIPS_URL}>
          check_silent_skips.py re-run on 1096fa4cd^ · skip census
        </a>
        <span className={s.subLabel}>
          Tests that passed while checking nothing
        </span>
      </div>
      <div className={s.arrow}>
        <del className={s.struck}>{total}</del> →{" "}
        <span className={s.zero}>0</span>{" "}
        <span className={s.arrowNote}>silent skips, 2026-09-23</span>
      </div>
      <div className={s.sub}>
        One lint found {total} Rust test sites that returned <code>ok</code>{" "}
        when something they needed was missing. All were fixed the same day, and
        a hard check keeps the class out.
      </div>
      <div className={s.skipGrid}>
        <div>
          <p className={s.h4}>
            Zig's own suite at the current reference commit
          </p>
          <Stack
            id="zig"
            parts={[
              [z.total - z.skip, "#1f2a28", "no skip in body"],
              [z.build, PALETTE.upstream, "build-time guard"],
              [z.runtime, "#b8e9e0", "runtime guard"],
            ]}
          />
          <div className={s.note}>
            <b className={s.strong}>
              {z.skip} of {fmt(z.total)}
            </b>{" "}
            test blocks can skip themselves with <code>error.SkipZigTest</code>:{" "}
            <Sq color={PALETTE.upstream} /> {z.build} on a build-time platform
            or option, <Sq color="#b8e9e0" /> {z.runtime} at run time (Valgrind,
            a missing font or PNG decoder, GTK init). A floor: skips reached
            through helpers are not counted.
          </div>
          <p className={`${s.h4} ${s.h4Gap}`}>
            Rust suite today ({fmt(r.total)} tests)
          </p>
          <Stack
            id="rust"
            parts={[
              [r.none, "#2b1d12", "no skip mechanism"],
              [r.miri, PALETTE.rust, "Miri ignore (skipped only under Miri)"],
              [r.cfgOnly, PALETTE.gold, "compiled only on some targets"],
              [r.cfgIgnore, PALETTE.orange, "platform/feature ignore only"],
              [r.ignore, PALETTE.fail, "#[ignore], run on request"],
            ]}
          />
          <div className={s.note}>
            <b className={s.strong}>
              {fmt(r.none)} ({Math.round((100 * r.none) / r.total)}%)
            </b>{" "}
            have no skip mechanism. <Sq color={PALETTE.rust} /> {r.miri} carry a
            Miri ignore ({r.miriDebugOff} run in a separate debug-assertions-off
            Miri lane; {r.miriAndCfgIgnore} of them also carry a platform or
            feature ignore). <Sq color={PALETTE.gold} /> {r.cfgOnly} compile
            only on some platforms or features. <Sq color={PALETTE.orange} />{" "}
            {r.cfgIgnore} are ignored only by a platform or feature attribute (
            {r.cfgIgnore + r.miriAndCfgIgnore} counting the {r.miriAndCfgIgnore}{" "}
            that also have a Miri ignore). <Sq color={PALETTE.fail} />{" "}
            {r.ignore} plain <code>#[ignore]</code>, 101 of which need a live
            display, GTK, D-Bus or a device. Silent skips:{" "}
            <b style={{ color: PALETTE.pass }}>0</b>.
          </div>
        </div>
        <div>
          <p className={s.h4}>
            The {total} sites before the fix, by what the guard checked
          </p>
          <Stack
            id="guard"
            parts={cls.map(([k, n]) => [
              n,
              GUARD_COLORS[k] ?? PALETTE.gray4,
              k,
            ])}
          />
          <div className={`${s.note} ${s.noteGap}`}>
            <Sq color={PALETTE.rust} /> compile-time scope written as a runtime{" "}
            <code>return</code> ({compile}) &nbsp; <Sq color={PALETTE.blue} />{" "}
            missing runtime resource ({runtime})
          </div>
          <Bars
            rows={cls}
            max={80}
            color={(k) => GUARD_COLORS[k] ?? PALETTE.gray4}
          />
          <p className={`${s.h4} ${s.h4Gap}`}>By lint finding · by crate</p>
          <Bars rows={kinds} max={150} color={() => PALETTE.gray5} />
          <div className={s.barsGap}>
            <Bars
              rows={skip.byCrate.slice(0, 8)}
              max={150}
              color={() => PALETTE.gray3}
            />
          </div>
        </div>
      </div>
      <div className={s.insights}>
        <div className={s.ins}>
          <b>
            {compile} of {total}
          </b>
          were compile-time scopes (a Cargo feature, target OS or build profile)
          written as a runtime <code>return</code>. A default build reported
          them as passed. Zig's equivalent is the {z.build} build-time{" "}
          <code>SkipZigTest</code> guards.
        </div>
        <div className={s.ins}>
          <b>
            {skip.zigPort} of {total}
          </b>
          sat in tests whose body still quotes the Zig skip line it was ported
          from. Translating a skip faithfully turned it into a silent pass,
          because libtest has no "skipped" result.
        </div>
        <div className={s.ins}>
          <b>{runtime} runtime</b>
          needed Metal or a display (
          {runtimeCls("GPU / display / window server")}
          ), system fonts ({runtimeCls("system fonts")}), the reference checkout
          or fuzz corpus (
          {runtimeCls("reference build / corpus / git checkout")}), a pty (
          {runtimeCls("pty / tty")}) or a PNG decoder (
          {runtimeCls("optional decoder not installed")}). They now fail loudly
          under a <code>RUSTTY_REQUIRE_*</code> switch, or are{" "}
          <code>#[ignore]</code>d.
        </div>
      </div>
      <p className={s.note}>
        The {total} were reproduced by running the lint as committed in{" "}
        <CommitLink hash="1096fa4cd" /> on the tree just before it:{" "}
        {skip.byKind.RET} early returns, {skip.byKind.EMPTY} bodies that are
        entirely <code>#[cfg]</code>-gated, {skip.byKind.MSG} "skipping" prints
        with no strict-mode switch, and {skip.byKind.WRAP} test whose only
        assertions sat inside an <code>if</code> with no <code>else</code>.
        Guard classes are a keyword classifier over each test body (estimated).
        Zig skips come from a brace-matched scan for{" "}
        <code>error.SkipZigTest</code> (a floor; the {z.build}/{z.runtime} split
        is estimated). Rust categories overlap ({r.miriAndCfgIgnore} tests carry
        both a Miri and a platform ignore).
      </p>
    </div>
  );
}
