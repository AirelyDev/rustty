#!/usr/bin/env python3
"""G-LOGFILE — reconcile diagnostics per ORACLE FILE, not per audit row.

WHY: `check_parity_closure.py` (G-PARITY) counts ROWS. Every audit row cites a
RANGE of reference line numbers, so anything outside every row's range is
unaudited and no row will ever claim it. Measured on 2026-09-10: `Surface.zig`
has 38 live `log.*` call sites ABOVE the lowest line any LOG row cites,
`gtk/class/application.zig` 20, `Config.zig` 14 — 101 unclaimed head sites over
14 files. That is a gate blind spot, and unlike PROCEDURE.md's blind spots 1-3 it
can be caught by machine.

WHAT: for every reference file cited by a LOG-class ledger row, count the
reference's own live `log.*` call sites, count the port's live emissions in the
Rust file(s) declared to port it, and RED if the port is shorter than the
declared allowance permits.

R5 — this gate never lowers its own bar:
  * every non-zero allowance MUST carry a reason, the same rule G-PARITY applies
    to a WAIVED row's `owner`;
  * an allowance that is LARGER than the measured deficit is RED, not green: it
    is a ratchet, so restoring a site must tighten the allowance in the same
    commit and can never leave slack behind for a future regression to hide in;
  * NOTHING here is env-settable. The oracle path is the one `check_test_counts.py`
    uses (R3: the oracle is an external read-only reference), and an absent oracle
    is BLOCKED (3), never a vacuous green.

VACUITY: the reference side binds against an external checkout. With it absent
every count would be 0 and every file would "reconcile". Three defenses, all
exit 3: the oracle checkout must exist and be at the pinned commit, the portmap
must be non-empty, and the total reference site count must be non-zero.

WHAT IT DOES NOT CLAIM: this is a COUNT reconciliation, not a per-site
attribution. Where two reference files port into one Rust file (e.g. both
`renderer/generic.zig` and `renderer/opengl/Frame.zig` land sites in
`generic/linux_impl.rs`) each reference file is credited with that file's whole
count, so the arm can read greener than a per-site audit would. The portmap
records every such overlap in its `reason` column. A green here means "no
diagnostic mass was lost", not "every individual site is live".

EXIT: 0 green, 1 red, 3 blocked. `--self-test` runs the gate's own arms;
`--report` additionally prints the full per-reference-file deficit table (the
actionable artifact — the pass/fail verdict alone is a ratchet and stays quiet
about debt that is already declared).
"""
import os
import re
import sys

GATES_DIR = os.path.dirname(os.path.abspath(__file__))
PORT_DIR = os.path.dirname(os.path.dirname(GATES_DIR))
ROOT = os.path.dirname(PORT_DIR)
PORTMAP = os.path.join(PORT_DIR, "parity-closure", "LOG-PORTMAP.tsv")
LEDGER = os.path.join(PORT_DIR, "parity-closure", "LEDGER.tsv")

# R3: the oracle is an EXTERNAL read-only checkout, never shipped into the repo.
# Same location `check_test_counts.py` and every other oracle-reading gate uses.
ORACLE = os.path.join(os.path.dirname(ROOT), "ghostty")
ORACLE_PIN = "20abdb50a6216c450d6d4d010c41c7edf5ab15b2"

# ---------------------------------------------------------------------------
# Reference side (Zig)
# ---------------------------------------------------------------------------
# UNANCHORED on purpose. Some reference sites call `std.log.warn` directly rather
# than through the file's scoped `const log` alias (two were found in
# `renderer/Overlay.zig` and `renderer/shadertoy.zig` alone), and a pattern
# anchored on the alias cannot see them. `glib_log` is the one other alias bound
# to `std.log.scoped` anywhere in the reference tree.
ZIG_CALL = re.compile(
    r"(?<![A-Za-z0-9_.])(?:std\.log|log|glib_log)\.(?:warn|err|info|debug)\s*\("
)


def zig_strip(line):
    """Blank out `//` comments and string bodies on one Zig line.

    Calls COMMENTED OUT IN THE REFERENCE must not be counted: `Surface.zig`
    alone carries six, and counting them would demand emissions the reference
    does not itself make. Zig has no block comments, so a line scanner is exact.
    """
    stripped = line.lstrip()
    if stripped.startswith("\\\\"):  # multiline string literal: all content
        return line[: len(line) - len(stripped)]
    out = []
    i, n, instr = 0, len(line), False
    while i < n:
        c = line[i]
        if instr:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                instr = False
            out.append(" ")
            i += 1
            continue
        if c == '"':
            instr = True
            out.append(" ")
            i += 1
            continue
        if c == "'":
            j = i + 1
            while j < n:
                if line[j] == "\\":
                    j += 2
                    continue
                if line[j] == "'":
                    break
                j += 1
            out.append(" " * (j - i + 1))
            i = j + 1
            continue
        if c == "/" and i + 1 < n and line[i + 1] == "/":
            break
        out.append(c)
        i += 1
    return "".join(out)


def zig_sites(text):
    """Line numbers of live `log.*` calls in Zig source text."""
    return [
        k
        for k, line in enumerate(text.split("\n"), 1)
        if ZIG_CALL.search(zig_strip(line))
    ]


# ---------------------------------------------------------------------------
# Port side (Rust)
# ---------------------------------------------------------------------------
# THREE emission shapes coexist in this port; a scanner that knows only the first
# reads whole crates as emitting nothing (see `discover_emitters`).
#   1. `log::warn!(target: LOG_SCOPE, ...)`  — the `log` facade, most crates.
#   2. `eprintln!("warn(gtk): ...")`         — crates that reproduce Zig's default
#      `logFn` output inline, prefix and all (`rustty-apprt`'s GTK tree, and every
#      `log_*` helper in `rustty-os` / `rustty-config`). Only an `eprintln!` whose
#      format string CARRIES THAT PREFIX counts: a bare `eprintln!` is ordinary
#      program output (help-text failures, test-harness skip lines) and counting
#      it would manufacture emissions the reference never asked for.
#   3. a `log_*` macro_rules / fn facade wrapping either of the above.
LOGFN_PREFIX = r'"\s*(?:warn|debug|info|err|error)\s*\('
FACADE_CALL = re.compile(r"\blog::(?:warn|error|info|debug|trace)!")
EPRINTLN_LOGFN = re.compile(r"\beprintln!\s*\(\s*" + LOGFN_PREFIX)


def primitive_sites(code, nc):
    """Offsets of live direct emissions. `code` has strings blanked, `nc` does not."""
    hits = [m.start() for m in FACADE_CALL.finditer(code)]
    hits += [m.start() for m in EPRINTLN_LOGFN.finditer(nc) if code[m.start():m.start() + 9] == "eprintln!"]
    return sorted(hits)


def has_primitive(code, nc):
    return bool(primitive_sites(code, nc))
# Inside a `log_*` definition the shape of the write no longer matters: the name
# says it is the diagnostic path, so any stderr write makes it an emitter.
WRAPPER_WRITE = re.compile(r"\beprintln!|\beprint!|\bwrite!|\bwriteln!")
MACRO_DEF = re.compile(r"\bmacro_rules!\s+(log_[A-Za-z0-9_]*)")
FN_DEF = re.compile(r"\bfn\s+(log_[A-Za-z0-9_]*)\s*[(<]")


def _rust_scan(src, keep_strings):
    """Blank out Rust comments (and, unless `keep_strings`, string bodies).

    Offsets and line numbers are preserved: every removed byte becomes a space,
    every newline survives. ONE scanner serves both variants on purpose — this
    file previously carried two near-identical copies and the char-literal branch
    was missing from both, which is how the bug below got in.

    Reference calls reproduced as R1 comments (`// log.warn("...")`) are the
    dominant shape in this port — `stream.rs` alone carried ~120 at one point —
    so a scanner that cannot tell a comment from code would report a port that
    emits nothing as fully reconciled.

    CHAR AND BYTE LITERALS ARE NOT OPTIONAL, and getting them wrong is not a
    local error. `crates/rustty-vt/src/stream.rs:3428` is `b'"' => {` — the DEC
    intermediate. A scanner without this branch sees a bare `"`, opens a phantom
    string, blanks everything up to the next `"` anywhere in the file, and stays
    desynchronised to EOF: ~1200 lines of real code erased, 59 live emissions
    invisible, and the gate reporting a 49-site deficit that did not exist. It
    fails FALSE-GREEN in the general case. The Zig side (`zig_strip`) always had
    this branch; a Zig desync also cannot escape its line, while this one runs to
    end of file. Lifetimes (`'a`, `'static`) are NOT char literals and must fall
    through to the code path — hence the two-character lookahead.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = i
            while j < n and src[j] != "\n":
                j += 1
            out.append(" " * (j - i))
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src[j] == "/" and j + 1 < n and src[j + 1] == "*":
                    depth += 1
                    j += 2
                    continue
                if src[j] == "*" and j + 1 < n and src[j + 1] == "/":
                    depth -= 1
                    j += 2
                    continue
                j += 1
            out.append("".join(ch if ch == "\n" else " " for ch in src[i:j]))
            i = j
            continue
        if c == "'":
            # `'\n'`, `'\''`, `'\\'`, `'\u{1f600}'`. A char literal holds EXACTLY
            # one escape sequence, so consume it positionally — a "scan to the
            # next quote, skipping escapes" loop mis-handles `b'\\'` (the ESC
            # dispatch's backslash) by treating the literal's own second
            # backslash as an escape and swallowing the closing quote.
            if i + 1 < n and src[i + 1] == "\\":
                j = i + 2
                if j < n and src[j] == "u" and j + 1 < n and src[j + 1] == "{":
                    j = src.find("}", j + 2)
                    j = n if j < 0 else j + 1
                else:
                    j += 1
                if j < n and src[j] == "'":
                    j += 1
                out.append("".join(ch if ch == "\n" else " " for ch in src[i:j]))
                i = j
                continue
            # `'x'` / `b'"'` — exactly one character, then the close quote.
            if i + 2 < n and src[i + 2] == "'":
                out.append("   ")
                i += 3
                continue
            # Anything else beginning with `'` is a LIFETIME. Emit it as code.
            out.append(c)
            i += 1
            continue
        if (
            c == "r"
            and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_"))
            and i + 1 < n
            and src[i + 1] in '#"'
        ):
            j, hashes = i + 1, 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                k = n if k < 0 else k + len(term)
                if keep_strings:
                    out.append(src[i:k])
                else:
                    out.append("".join(ch if ch == "\n" else " " for ch in src[i:k]))
                i = k
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            j = min(j, n)
            if keep_strings:
                out.append(src[i:j])
            else:
                out.append("".join(ch if ch == "\n" else " " for ch in src[i:j]))
            i = j
            continue
        out.append(c)
        i += 1
    return "".join(out)


def rust_strip(src):
    """Comments AND string bodies blanked. What brace matching needs."""
    return _rust_scan(src, keep_strings=False)


def rust_strip_comments_only(src):
    """Comments blanked, string bodies KEPT — offset-identical to `rust_strip`.

    `rust_strip` destroys the one thing the `eprintln!` arm must read: the
    `logFn` prefix inside the format string. Both variants are offset-identical,
    so a match found in one is testable at the same offset in the other.
    """
    return _rust_scan(src, keep_strings=True)


def _brace_body(src, start):
    """(open, close) offsets of the `{...}` body that follows `start`, or None."""
    i, paren = start, 0
    while i < len(src):
        c = src[i]
        if c in "(<[":
            paren += 1
        elif c in ")>]":
            paren -= 1
        elif c == "{" and paren <= 0:
            break
        elif c == ";" and paren <= 0:
            return None
        i += 1
    if i >= len(src):
        return None
    depth, j = 0, i
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return (i, j)
        j += 1
    return None


def rust_definitions(src):
    """[(kind, name, decl_start, body_open, body_close)] for log_* macros and fns."""
    defs = []
    for m in MACRO_DEF.finditer(src):
        body = _brace_body(src, m.end())
        if body:
            defs.append(("macro", m.group(1), m.start(), body[0], body[1]))
    for m in FN_DEF.finditer(src):
        body = _brace_body(src, m.end() - 1)
        if body:
            returns = "->" in src[m.end() - 1 : body[0]]
            defs.append(("fn" if not returns else "fn_ret", m.group(1), m.start(),
                         body[0], body[1]))
    return defs


def discover_emitters(sources):
    """Names of log_* macros/fns whose body really reaches the log facade.

    THIS IS THE POINT OF THE PASS. Four emission shapes coexist in this port and
    a gate that knows only the first reads two whole crates as emitting nothing:
      1. `log::warn!(target: LOG_SCOPE, ...)`               — the common form;
      2. `log_warn!(...)`, a macro_rules facade            — `opengl/gl.rs`;
      3. `log_warn(...)`, a fn facade                       — `config_struct.rs`;
      4. `sys::log_fn(...)`, ending in a C callback pointer — `rustty-vt-c`.
    A log_* fn or macro whose body does NOT reach the facade is a NO-OP STUB —
    `rustty-os/src/locale.rs` has six and `cf_release_thread.rs` eight, all
    literally empty; 17 are reported tree-wide — and its call sites are dropped
    diagnostics, so it deliberately does NOT become an emitter.

    `sources` maps path -> (code, nc): `code` has comments AND string bodies
    blanked (so brace matching is exact), `nc` has only comments blanked (so the
    `logFn` prefix inside an `eprintln!` is still readable). Both preserve
    offsets. Iterated to a fixed point so a facade built on another is caught.
    """
    emitters, stubs = set(), {}
    defs = {p: rust_definitions(v[0]) for p, v in sources.items()}
    changed = True
    while changed:
        changed = False
        for path, (code, nc) in sources.items():
            for _kind, name, _decl, bo, bc in defs[path]:
                if name in emitters:
                    continue
                body, body_nc = code[bo : bc + 1], nc[bo : bc + 1]
                # A `log_*` definition that returns NOTHING and whose body is not
                # empty is a diagnostic sink whatever it writes through — vt-c's
                # `sys::log_fn` ends in a C callback pointer, not in `log::` or
                # `eprintln!`, and a rule that demanded a known write primitive
                # read that whole crate as emitting nothing. A `log_*` fn that
                # RETURNS a value is a helper, not a sink, so it qualifies only
                # on a real primitive. This is load-bearing:
                # `stream.rs:4671`'s `log_unsupported_once` is a dedup PREDICATE
                # called immediately before three real `log::warn!`s, and
                # counting its call sites would inflate that file by 3. (It is
                # correctly the 17th entry in the no-op NOTE. Before the
                # char-literal fix of 2026-09-10 this branch was unreachable —
                # the definition sat inside the phantom-string region and
                # `rust_definitions` never saw it.)
                thin_sink = _kind == "fn" and body.strip("{} \t\r\n") != ""
                if (
                    has_primitive(body, body_nc)
                    or WRAPPER_WRITE.search(body)
                    or thin_sink
                    or any(
                        re.search(
                            r"(?<![A-Za-z0-9_.])" + re.escape(e) + r"\s*[!(]", body
                        )
                        for e in emitters
                    )
                ):
                    emitters.add(name)
                    changed = True
    for path, (code, _nc) in sources.items():
        for _kind, name, _decl, _bo, _bc in defs[path]:
            if name not in emitters:
                stubs.setdefault(path, []).append(
                    (name, code.count("\n", 0, _decl) + 1)
                )
    return emitters, defs, stubs


def rust_emissions(paths, sources, emitters, defs):
    """[(path, line, what)] live emission sites across `paths`."""
    hits = []
    for path in paths:
        code, nc = sources[path]
        spans = [
            (decl, bc) for _k, name, decl, _bo, bc in defs[path] if name in emitters
        ]

        def inside(pos):
            return any(a <= pos <= b for a, b in spans)

        for pos in primitive_sites(code, nc):
            if not inside(pos):
                hits.append((path, code.count("\n", 0, pos) + 1, "direct"))
        for name in emitters:
            for m in re.finditer(
                r"(?<![A-Za-z0-9_.])" + re.escape(name) + r"\s*[!(]", code
            ):
                if not inside(m.start()):
                    hits.append((path, code.count("\n", 0, m.start()) + 1, name))
    return hits


# ---------------------------------------------------------------------------
# Portmap
# ---------------------------------------------------------------------------
def load_portmap(path):
    """Rows of the declared reference -> port mapping.

    Columns: oracle_file, port_arms, allowance, reason.
    `port_arms` is `|`-separated ARMS; each arm is `+`-separated FILES.
    Files inside one arm are SUMMED (a reference file split across several Rust
    files). Arms are compared INDEPENDENTLY and the file's actual is the MINIMUM
    over arms, because arms are PLATFORM TWINS: only one is compiled on a given
    target, so summing them would let a fully-restored macOS twin hide a Linux
    twin that emits nothing. That is exactly finding N9.
    """
    rows = []
    with open(path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if parts[0].strip() == "oracle_file":  # header, wherever it sits
                continue
            if len(parts) < 4:
                raise ValueError(f"{path}:{lineno}: expected 4 tab-separated columns")
            arms = [
                [f.strip() for f in arm.split("+") if f.strip()]
                for arm in parts[1].split("|")
            ]
            rows.append(
                {
                    "oracle": parts[0].strip(),
                    "arms": [a for a in arms if a],
                    "allowance": parts[2].strip(),
                    "reason": parts[3].strip(),
                    "lineno": lineno,
                }
            )
    return rows


def ledger_log_files(path):
    """Reference .zig files cited by a LOG-class ledger row -> {row ids}."""
    import csv

    out = {}
    with open(path, newline="", encoding="utf-8") as fh:
        for r in csv.DictReader(fh, delimiter="\t"):
            if (r.get("class") or "").strip() != "LOG":
                continue
            for f in set(
                re.findall(r"(?:src|pkg)/[A-Za-z0-9_./-]+\.zig", r.get("oracle") or "")
            ):
                out.setdefault(f, set()).add(f"{r['id']}({r['state']})")
    return out


# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------
def verdict(portmap, claims, zig_text, rust_text, facade_sources=None, report=False):
    """Return (rc, lines). rc 0 GREEN, 1 RED. Pure — all I/O is in the callers.

    `zig_text(path)` / `rust_text(path)` return source text or None if absent.
    """
    out, rc = [], 0
    declared = {r["oracle"] for r in portmap}
    undeclared = sorted(set(claims) - declared)

    # Build the Rust source pool once, so facade discovery is workspace-wide:
    # `opengl/gl.rs` defines the macros that `opengl/gl/sys.rs` emits through.
    pool, missing_files = {}, []
    for r in portmap:
        for arm in r["arms"]:
            for f in arm:
                if f in pool:
                    continue
                txt = rust_text(f)
                if txt is None:
                    missing_files.append((r["oracle"], f))
                else:
                    pool[f] = (rust_strip(txt), rust_strip_comments_only(txt))
    # Facade discovery must see MORE than the mapped files: `opengl/gl.rs` defines
    # the `log_warn!` macro that `opengl/gl/sys.rs` emits through, and gl.rs is not
    # itself a mapped port file. Counting still happens only over `pool`.
    wide = dict(facade_sources or {})
    wide.update(pool)
    emitters, wide_defs, stubs = discover_emitters(wide)
    defs = {p: wide_defs[p] for p in pool}
    stubs = {p: v for p, v in stubs.items() if p in pool}

    rows, total_expected, total_actual = [], 0, 0
    missing_oracle = []
    for r in portmap:
        txt = zig_text(r["oracle"])
        if txt is None:
            missing_oracle.append(r["oracle"])
            continue
        expected = len(zig_sites(txt))
        total_expected += expected
        arm_counts = []
        for arm in r["arms"]:
            present = [f for f in arm if f in pool]
            arm_counts.append(
                len(rust_emissions(present, pool, emitters, defs)) if present else 0
            )
        actual = min(arm_counts) if arm_counts else 0
        total_actual += actual
        deficit = max(0, expected - actual)
        rows.append((r, expected, arm_counts, actual, deficit))

    # --- vacuity defenses (exit 3 territory is handled by main; these are RED) --
    if not rows:
        out.append("RED: portmap produced no comparable rows")
        return 1, out
    if total_expected == 0:
        out.append(
            "RED: every reference file scanned to ZERO log sites — the reference "
            "scanner is not binding; refusing to report a vacuous green"
        )
        return 1, out

    report_lines = []
    if report:
        out = report_lines
        out.append("")
        out.append("--- per reference file (deficit = reference - thinnest arm) ---")
        out.append(f"{'deficit':>7}  {'ref':>4} {'port':>5}  oracle file / arms / rows")
        for r, expected, arm_counts, actual, deficit in sorted(
            rows, key=lambda x: (-x[4], x[0]["oracle"])
        ):
            out.append(
                f"{deficit:>7}  {expected:>4} {actual:>5}  {r['oracle']}"
                f"   allowance={r['allowance']}"
            )
            out.append(
                "                       arms: "
                + " | ".join(f"{'+'.join(a)}={c}" for a, c in zip(r["arms"], arm_counts))
            )
            out.append(
                "                       rows: "
                + (", ".join(sorted(claims.get(r["oracle"], []))) or "-")
            )
        out.append(
            f"--- total deficit {sum(x[4] for x in rows)} over "
            f"{sum(1 for x in rows if x[4])} file(s) ---"
        )
        out.append("")
        out = []

    shortfalls, stale, unreasoned, bad = [], [], [], []
    for r, expected, arm_counts, actual, deficit in rows:
        try:
            allowance = int(r["allowance"])
        except ValueError:
            bad.append(f"{r['oracle']}: allowance={r['allowance']!r} is not an integer")
            continue
        if allowance < 0:
            bad.append(f"{r['oracle']}: allowance={allowance} is negative")
            continue
        if allowance and not r["reason"]:
            unreasoned.append(r["oracle"])
        if deficit > allowance:
            shortfalls.append((r, expected, arm_counts, actual, deficit, allowance))
        elif deficit < allowance:
            stale.append((r["oracle"], allowance, deficit))

    out.append(
        f"G-LOGFILE: {len(rows)} reference file(s) — "
        f"{total_expected} live reference log site(s), {total_actual} live port "
        f"emission(s) on the thinnest arm, {len(emitters)} log facade name(s) resolved"
    )
    out.extend(report_lines)

    if missing_oracle:
        out.append(
            f"RED: {len(missing_oracle)} declared reference file(s) not in the oracle: "
            f"{missing_oracle[:5]}"
        )
        rc = 1
    if missing_files:
        out.append(f"RED: {len(missing_files)} declared port file(s) do not exist:")
        for oracle, f in missing_files[:8]:
            out.append(f"       {oracle} -> {f}")
        rc = 1
    if undeclared:
        out.append(
            f"RED: {len(undeclared)} reference file(s) cited by a LOG ledger row have "
            "no portmap entry — a missing mapping is itself a finding:"
        )
        for f in undeclared[:10]:
            out.append(f"       {f}   rows={sorted(claims[f])}")
        rc = 1
    if bad:
        out.append(f"RED: {len(bad)} malformed allowance(s): {bad[:5]}")
        rc = 1
    if unreasoned:
        out.append(
            "RED: a non-zero allowance MUST name its justification in the `reason` "
            f"column (G-PARITY's WAIVED-owner rule); missing on: {unreasoned[:5]}"
        )
        rc = 1
    if stale:
        out.append(
            f"RED: {len(stale)} allowance(s) now exceed the measured deficit. This "
            "gate is a RATCHET: tighten them in the same commit that restored the "
            "sites, so no slack is left for a future regression to hide in."
        )
        for f, allowance, deficit in stale[:10]:
            out.append(f"       {f}: allowance={allowance}, measured deficit={deficit}")
        rc = 1
    if shortfalls:
        out.append(
            f"RED: {len(shortfalls)} reference file(s) whose port emits fewer "
            "diagnostics than the allowance permits:"
        )
        for r, expected, arm_counts, actual, deficit, allowance in shortfalls:
            arms = " | ".join(
                f"{'+'.join(a)}={c}" for a, c in zip(r["arms"], arm_counts)
            )
            out.append(
                f"       {r['oracle']}: reference={expected} port={actual} "
                f"deficit={deficit} allowance={allowance}"
            )
            out.append(f"           arms: {arms}")
            out.append(f"           rows: {sorted(claims.get(r['oracle'], [])) or '-'}")
        rc = 1

    # Only stubs that are actually CALLED from a mapped port file matter: an
    # uncalled `log_*` test helper is not a dropped diagnostic.
    noop = sorted(
        (p, n, ln)
        for p, v in stubs.items()
        for n, ln in v
        if any(
            re.search(
                r"(?<![A-Za-z0-9_.])(?<!fn )" + re.escape(n) + r"\s*[!(]", code
            )
            for code, _nc in pool.values()
        )
    )
    if noop:
        out.append(
            f"NOTE: {len(noop)} `log_*` definition(s) never reach the log facade "
            "(no-op stubs; their call sites are counted as dropped, not emitted):"
        )
        for p, n, ln in noop[:6]:
            out.append(f"       {p}:{ln} {n}")
        if len(noop) > 6:
            out.append(f"       ... and {len(noop) - 6} more")

    if rc == 0:
        out.append(
            "GREEN: every reference file's port emits at least "
            "reference-minus-allowance diagnostics on every platform arm."
        )
    return rc, out


# ---------------------------------------------------------------------------
# Ground truth: the scanner against a naive grep, on the LIVE tree
# ---------------------------------------------------------------------------
def scanner_violations(raw, code):
    """Raw `log::` occurrences the scanner dropped that are NOT in a comment.

    WHY THIS EXISTS. Synthetic fixtures cannot catch a scanner DESYNC, because a
    desync depends on real file content: one unhandled `b'"'` in
    `crates/rustty-vt/src/stream.rs` blanked ~1200 lines below it, hid 59 live
    emissions, and made the gate report — and freeze into the baseline — a
    49-site deficit that did not exist. Every synthetic arm stayed green through
    all of it. The scanner was never once checked against a plain `grep -c` on
    the file with the largest deficit, and one such check would have caught it
    immediately.

    So: a live-tree arm. Every raw occurrence the scanner removes must be
    accounted for by a comment marker earlier on its own line or by an open block
    comment. Anything else is the scanner eating code, which fails FALSE-GREEN.
    """
    bad = []
    for m in FACADE_CALL.finditer(raw):
        if code[m.start() : m.end()] == m.group(0):
            continue  # survived the strip — live code, fine
        line_start = raw.rfind("\n", 0, m.start()) + 1
        prefix = raw[line_start : m.start()]
        if "//" in prefix or "/*" in prefix or prefix.lstrip().startswith("*"):
            continue  # dropped because it is commented out — correct
        # Could still be inside a multi-line block comment opened earlier.
        before = raw[:line_start]
        if before.rfind("/*") > before.rfind("*/"):
            continue
        bad.append((raw.count("\n", 0, m.start()) + 1, raw[line_start : m.end()].strip()))
    return bad


def live_scanner_check():
    """(ok, lines) — run `scanner_violations` over every mapped port file."""
    out, ok = [], True
    try:
        portmap = load_portmap(PORTMAP)
    except (OSError, ValueError) as exc:
        return False, [f"  [FAIL] live-tree arm could not load the portmap: {exc}"]
    paths = sorted({f for r in portmap for arm in r["arms"] for f in arm})
    if not paths:
        return False, ["  [FAIL] live-tree arm: portmap declares no port files"]
    scanned, total_live, worst = 0, 0, ("", 0)
    for rel in paths:
        full = os.path.join(ROOT, rel)
        if not os.path.isfile(full):
            continue
        with open(full, encoding="utf-8", errors="replace") as fh:
            raw = fh.read()
        code = rust_strip(raw)
        if len(code) != len(raw):
            ok = False
            out.append(f"  [FAIL] {rel}: strip changed the file length — offsets desynced")
            continue
        scanned += 1
        live = len(FACADE_CALL.findall(code))
        total_live += live
        if live > worst[1]:
            worst = (rel, live)
        for lineno, text in scanner_violations(raw, code):
            ok = False
            out.append(f"  [FAIL] {rel}:{lineno} live `log::` eaten by the scanner: {text}")
    if scanned == 0:
        return False, ["  [FAIL] live-tree arm scanned no files — cannot validate"]
    if total_live == 0:
        return False, ["  [FAIL] live-tree arm found zero live emissions tree-wide"]
    out.insert(
        0,
        f"  [{'ok ' if ok else 'FAIL'}] live tree: {scanned} port file(s), {total_live} live "
        f"`log::` site(s), every dropped occurrence accounted for by a comment\n"
        f"         (busiest: {worst[0]} = {worst[1]})",
    )
    return ok, out


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------
SELF_ZIG_OK = 'const log = std.log.scoped(.x);\nfn a() void { log.warn("a", .{}); }\n'
SELF_ZIG_TWO = (
    'const log = std.log.scoped(.x);\n'
    'fn a() void { log.warn("a", .{}); }\n'
    'fn b() void { std.log.err("b", .{}); }\n'
)
SELF_ZIG_COMMENT = (
    'const log = std.log.scoped(.x);\n'
    'fn a() void { log.warn("a", .{}); }\n'
    '// log.warn("commented in the REFERENCE, not a site", .{});\n'
    'const s = "log.warn(\\"in a string\\", .{})";\n'
)
SELF_RS_ONE = 'fn a() { log::warn!(target: S, "a"); }\n'
SELF_RS_TWO = 'fn a() { log::warn!(target: S, "a"); }\nfn b() { log::error!(target: S, "b"); }\n'
SELF_RS_NONE = '// log.warn("a", .{});\nfn a() {}\n'
SELF_RS_MACRO = (
    "macro_rules! log_warn { ($($a:tt)*) => { log::warn!(target: S, $($a)*) }; }\n"
    'fn a() { log_warn!("a"); }\n'
)
SELF_RS_STUB = 'fn log_warn_a() {\n    // log.warn("a", .{});\n}\nfn a() { log_warn_a(); }\n'
# `rustty-apprt`'s GTK tree reproduces Zig's default `logFn` OUTPUT inline. The
# prefix is the whole signal: the second `eprintln!` here is ordinary program
# output and must NOT be counted, or the gate manufactures emissions.
SELF_RS_EPRINTLN = (
    'fn a() { eprintln!("warn(gtk): could not do the thing err={err:?}"); }\n'
    'fn b() { eprintln!("usage: rustty [options]"); }\n'
)


def self_test():
    print("=== G-LOGFILE SELF-TEST ===")
    zig = {
        "src/ok.zig": SELF_ZIG_OK,
        "src/two.zig": SELF_ZIG_TWO,
        "src/cmt.zig": SELF_ZIG_COMMENT,
        "src/twin.zig": SELF_ZIG_TWO,
        "src/split.zig": SELF_ZIG_TWO,
        "src/macro.zig": SELF_ZIG_OK,
        "src/stub.zig": SELF_ZIG_OK,
        "src/eprint.zig": SELF_ZIG_OK,
        "src/eprint2.zig": SELF_ZIG_TWO,
    }
    rs = {
        "c/ok.rs": SELF_RS_ONE,
        "c/short.rs": SELF_RS_NONE,
        "c/mac.rs": SELF_RS_TWO,
        "c/lin.rs": SELF_RS_ONE,
        "c/split_a.rs": SELF_RS_ONE,
        "c/split_b.rs": SELF_RS_ONE,
        "c/macro.rs": SELF_RS_MACRO,
        "c/stub.rs": SELF_RS_STUB,
        "c/eprint.rs": SELF_RS_EPRINTLN,
    }
    zt = lambda p: zig.get(p)
    rt = lambda p: rs.get(p)

    def run(portmap_rows, claims=None):
        pm = [
            {
                "oracle": o,
                "arms": [
                    [f.strip() for f in arm.split("+")] for arm in arms.split("|")
                ],
                "allowance": a,
                "reason": r,
                "lineno": 0,
            }
            for o, arms, a, r in portmap_rows
        ]
        return verdict(pm, claims or {}, zt, rt)

    cases = []
    # 1. reconciles exactly
    cases.append(("a file that reconciles", run([("src/ok.zig", "c/ok.rs", "0", "")])[0], 0))
    # 2. short
    cases.append(("a file that is short", run([("src/ok.zig", "c/short.rs", "0", "")])[0], 1))
    # 3. short but within a reasoned allowance
    cases.append(
        ("short, inside a reasoned allowance",
         run([("src/ok.zig", "c/short.rs", "1", "unrestorable: infallible in Rust")])[0], 0)
    )
    # 4. an allowance with no reason
    cases.append(
        ("allowance with no reason",
         run([("src/ok.zig", "c/short.rs", "1", "")])[0], 1)
    )
    # 5. RATCHET: allowance larger than the measured deficit
    cases.append(
        ("stale (too-large) allowance",
         run([("src/ok.zig", "c/ok.rs", "1", "stale")])[0], 1)
    )
    # 6. PLATFORM TWINS: mac arm full, linux arm short -> RED (finding N9's shape).
    #    Summing the arms would give 2+1=3 >= 2 and pass. Taking the MIN catches it.
    cases.append(
        ("platform twins, one arm short",
         run([("src/twin.zig", "c/mac.rs|c/lin.rs", "0", "")])[0], 1)
    )
    cases.append(
        ("platform twins, both arms full",
         run([("src/twin.zig", "c/mac.rs|c/mac.rs", "0", "")])[0], 0)
    )
    # 7. one reference file SPLIT across two Rust files: summed within the arm
    cases.append(
        ("one reference file split across two port files",
         run([("src/split.zig", "c/split_a.rs+c/split_b.rs", "0", "")])[0], 0)
    )
    # 8. a commented-out reference call is NOT a site (and neither is one in a
    #    string literal): cmt.zig has 1 live site, port emits 1.
    cases.append(
        ("commented-out reference call is not counted",
         run([("src/cmt.zig", "c/ok.rs", "0", "")])[0], 0)
    )
    # 9. macro_rules facade counts as an emission
    cases.append(
        ("macro_rules! log_warn facade counts",
         run([("src/macro.zig", "c/macro.rs", "0", "")])[0], 0)
    )
    # 10. a no-op log_* stub does NOT count as an emission
    cases.append(
        ("no-op log_* stub does not count",
         run([("src/stub.zig", "c/stub.rs", "0", "")])[0], 1)
    )
    # 10b. the eprintln!("warn(scope): ...") facade counts, and only WITH the prefix
    cases.append(
        ("eprintln! logFn facade counts (1 of the 2 in the file)",
         run([("src/eprint.zig", "c/eprint.rs", "0", "")])[0], 0)
    )
    cases.append(
        ("a bare eprintln! is NOT an emission",
         run([("src/eprint2.zig", "c/eprint.rs", "0", "")])[0], 1)
    )
    # 11. a LOG row citing a file with no portmap entry
    cases.append(
        ("undeclared reference file",
         run([("src/ok.zig", "c/ok.rs", "0", "")], {"src/two.zig": {"SX-F1(CLOSED)"}})[0], 1)
    )
    # 12. a declared port file that does not exist
    cases.append(
        ("declared port file missing",
         run([("src/ok.zig", "c/nope.rs", "0", "")])[0], 1)
    )
    # 13. VACUITY: the reference side binds to nothing -> RED, not "0 shortfalls"
    cases.append(
        ("reference scanner binds to nothing",
         verdict(
             [{"oracle": "src/empty.zig", "arms": [["c/ok.rs"]], "allowance": "0",
               "reason": "", "lineno": 0}],
             {}, lambda p: "fn a() void {}\n", rt)[0], 1)
    )

    live_ok, live_lines = live_scanner_check()
    ok = live_ok
    for line in live_lines:
        print(line)
    for name, got, want in cases:
        mark = "ok " if got == want else "FAIL"
        if got != want:
            ok = False
        print(f"  [{mark}] {name:52} -> {got} (expect {want})")
    print(
        "SELF-TEST GREEN: the gate flips RED on a short file, an unreasoned or stale\n"
        "allowance, a short PLATFORM TWIN that summing would have hidden, a no-op\n"
        "log stub, an undeclared file, a missing port file, and a vacuous scan —\n"
        "and the scanner agrees with a naive grep on every mapped port file."
        if ok
        else "SELF-TEST RED: the gate did not flip."
    )
    return 0 if ok else 1


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if not os.path.exists(PORTMAP):
        print(f"BLOCKED: portmap not found at {PORTMAP}", file=sys.stderr)
        return 3
    if not os.path.isdir(os.path.join(ORACLE, "src")):
        print(
            f"BLOCKED: the read-only ghostty oracle checkout is absent at {ORACLE}.\n"
            "         Without it every reference file would scan to zero log sites\n"
            "         and every port would 'reconcile' — a vacuous green.",
            file=sys.stderr,
        )
        return 3
    head = os.path.join(ORACLE, ".git", "HEAD")
    try:
        with open(head, encoding="utf-8") as fh:
            ref = fh.read().strip()
        if ref.startswith("ref: "):
            with open(os.path.join(ORACLE, ".git", ref[5:]), encoding="utf-8") as fh:
                ref = fh.read().strip()
    except OSError:
        ref = ""
    if ref != ORACLE_PIN:
        print(
            f"BLOCKED: the oracle checkout is at {ref or '<unknown>'}, not the pin\n"
            f"         {ORACLE_PIN}. Line-for-line counts are only meaningful at the pin.",
            file=sys.stderr,
        )
        return 3
    try:
        portmap = load_portmap(PORTMAP)
    except ValueError as exc:
        print(f"BLOCKED: {exc}", file=sys.stderr)
        return 3
    if not portmap:
        print(
            "BLOCKED: portmap has no rows — refusing to report a vacuous green",
            file=sys.stderr,
        )
        return 3
    claims = ledger_log_files(LEDGER) if os.path.exists(LEDGER) else {}
    if not claims:
        print(
            "BLOCKED: no LOG-class rows found in the ledger — the claims side is not\n"
            "         binding, so 'no undeclared file' would be vacuously true",
            file=sys.stderr,
        )
        return 3

    def zig_text(rel):
        p = os.path.join(ORACLE, rel)
        if not os.path.isfile(p):
            return None
        with open(p, encoding="utf-8", errors="replace") as fh:
            return fh.read()

    def rust_text(rel):
        p = os.path.join(ROOT, rel)
        if not os.path.isfile(p):
            return None
        with open(p, encoding="utf-8", errors="replace") as fh:
            return fh.read()

    # Every .rs in the crates that own a mapped port file, for facade discovery.
    facade = {}
    crates_seen = set()
    for r in portmap:
        for arm in r["arms"]:
            for f in arm:
                parts = f.split("/")
                if len(parts) > 2 and parts[0] == "crates":
                    crates_seen.add(os.path.join(parts[0], parts[1], "src"))
    for rel in sorted(crates_seen):
        base = os.path.join(ROOT, rel)
        for dirpath, _dirs, names in os.walk(base):
            for name in names:
                if not name.endswith(".rs"):
                    continue
                full = os.path.join(dirpath, name)
                key = os.path.relpath(full, ROOT)
                with open(full, encoding="utf-8", errors="replace") as fh:
                    raw = fh.read()
                facade[key] = (rust_strip(raw), rust_strip_comments_only(raw))

    rc, lines = verdict(
        portmap, claims, zig_text, rust_text, facade, report="--report" in sys.argv
    )
    print("\n".join(lines))
    return rc


if __name__ == "__main__":
    sys.exit(main())
