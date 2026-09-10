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

EXCLUSION BUDGET: an exclusion retires a reference file rather than reconciling
it, erasing its whole deficit — the widest lever here, and once the only number
that was not ratcheted (retiring `src/termio/Exec.zig` wiped 58 sites and stayed
GREEN). The portmap's `#!exclusion-cap files=N sites=N` directive freezes it, both
ways; a missing directive is BLOCKED.

COVERAGE: the gate enumerates the REFERENCE TREE, not the portmap. This file was
first seeded from the audit's LOG rows and so inherited the audit's blind spots —
112 reference files holding 523 live diagnostics, 37% of the total, sat outside it
with nothing in the output to distinguish "reconciles" from "never looked at"
(N15). Every log-bearing reference file must now be declared or EXCLUDED with a
category, or the gate is RED.

SECOND RATCHET: counting alone is not enough. A logFn-shaped `eprintln!` counts as
an emission (the diagnostic does reach a human), so a stage could close a count
deficit by adding more of them. `LOG-PORTMAP.tsv`'s `bypass_cap` column caps the
facade-BYPASSING subset per file, on the same ratchet — finding N14.

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
DIVERGENCES = os.path.join(PORT_DIR, "parity-closure", "LOG-TEXT-DIVERGENCES.tsv")
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


def direct_print_sites(code, nc):
    """Offsets of emissions that BYPASS the log facade entirely.

    A `eprintln!("warn(gtk): …")` reproduces `logFn`'s OUTPUT but not its
    behaviour: it cannot be silenced by `Logging.stderr` (disabled by default in
    lib mode), never reaches macOS unified logging, and offers no level or scope
    to filter on. `rustty-apprt`'s GTK subtree does this 244 times — finding N14.

    They ARE counted as emissions above, because the reference's diagnostic does
    reach a human. That is exactly why they need their own ratchet: "restore the
    missing sites" and "stop bypassing the facade" pull in opposite directions,
    and without this a stage could close a count deficit by adding MORE direct
    prints and the gate would read it as an improvement.
    """
    return [
        m.start()
        for m in EPRINTLN_LOGFN.finditer(nc)
        if code[m.start() : m.start() + 9] == "eprintln!"
    ]


def rust_direct_prints(paths, sources):
    """[(path, line)] facade-bypassing emission sites across `paths`."""
    hits = []
    for path in paths:
        code, nc = sources[path]
        for pos in direct_print_sites(code, nc):
            hits.append((path, code.count("\n", 0, pos) + 1))
    return hits
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
        # Raw string literals: `r"..."`, `r#"..."#`, and the BYTE forms `br"..."`
        # / `br#"..."#`. The byte prefix is not cosmetic here: a `br` literal
        # reaching the plain `"` scanner below would have its `\"` honoured as an
        # escape, which raw strings do not process, and `br"foo\"` then desyncs
        # the rest of the file — the same false-green failure class as the char
        # literal above. No mapped port file contains one today; the branch is
        # here so that stays true by construction rather than by luck.
        if c == "b" and i + 1 < n and src[i + 1] == "r":
            if (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")) and (
                i + 2 < n and src[i + 2] in '#"'
            ):
                raw_start, scan = i, i + 2
            else:
                raw_start, scan = None, None
        elif c == "r" and (
            i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")
        ) and i + 1 < n and src[i + 1] in '#"':
            raw_start, scan = i, i + 1
        else:
            raw_start, scan = None, None
        if raw_start is not None:
            j, hashes = scan, 0
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
# Message CONTENT reconciliation, and stale preserved comments
# ---------------------------------------------------------------------------
# WHY A COUNT GATE CANNOT DO THIS. Everything above counts emissions. A message
# that is PRESENT but wrong reconciles perfectly: one funnel emitted only the
# `err=` half of `"error handling VT action action={} err={}"` and every count in
# this gate was green. The same stage then reintroduced the class four times in
# its own fix commit, dropping the space before a placeholder so the port emitted
# `mode:Flags { … }` where the reference emits `mode: <value>`. Discovering a
# defect class and recommitting it in the same change is the argument for
# machine-checking it.
#
# The check compares each live emission's format string against the reference
# call preserved as a comment above it, with placeholder CONTENTS normalised and
# nothing else — spacing is the payload, since `mode: {}` vs `mode:{}` is exactly
# the bug. Runs of 2+ spaces collapse to one, because the R1 comment often wraps
# a single reference line across two comment lines; a 1-to-0 space change is
# still caught, a 3-to-1 change is not, and that is recorded rather than hidden.
#
# THE INVERSE CHECK RIDES ALONG. Some preserved comments quote reference calls
# that DO NOT EXIST at the pin — stranded by our own Phase 6 pin advance (N20).
# Nothing emits them today, so nothing is wrong; the risk is that a future stage
# works by the standard method, reads the comment and faithfully restores a call
# the reference never had. Both checks extract a message from a commented
# reference call; one compares it to the port, the other to the reference. One
# scan, two verdicts.
ZIG_CALL_IN_COMMENT = re.compile(
    r"(?:std\.log|log|glib_log)\.(warn|err|info|debug)\s*\("
)
LOGFN_PREFIX_TEXT = re.compile(r"^(?:warn|debug|info|err|error)\([^)]*\):\s?")
LEVEL_ALIAS = {"error": "err"}


def normalise_message(text):
    """Placeholder contents erased, `{{`/`}}` preserved, runs of spaces collapsed."""
    if text is None:
        return None
    text = LOGFN_PREFIX_TEXT.sub("", text)  # a bypass print carries logFn's prefix
    text = text.replace("{{", "\x00").replace("}}", "\x01")
    text = re.sub(r"\{[^{}]*\}", "{}", text)
    text = re.sub(r"[ \t]{2,}", " ", text)
    return text.replace("\x00", "{").replace("\x01", "}")


def zig_format_arg(blob):
    """The reference call's format argument: every literal in it, concatenated.

    Stops at the `.{...}` args tuple or at the `)` that closes the call. Both
    stops are load-bearing: `Overlay.zig` and `Face.zig` split their format
    across two `++`-joined literals, so taking only the first TRUNCATES the
    message — manufacturing the very defect this check hunts — while PROSE after
    the reproduced call ("the reference's \"new_tab_to_new_window\" text,
    verbatim") gets concatenated on without the paren stop.
    """
    depth, stop, p, instr = 0, len(blob), 0, False
    while p < len(blob):
        c = blob[p]
        if instr:
            if c == "\\":
                p += 2
                continue
            if c == '"':
                instr = False
            p += 1
            continue
        if c == '"':
            instr = True
            p += 1
            continue
        if blob.startswith(".{", p):
            stop = p
            break
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth <= 0:
                stop = p
                break
        p += 1
    scan, parts, p = blob[:stop], [], 0
    while p < len(scan):
        if scan[p] != '"':
            p += 1
            continue
        p += 1
        out = []
        while p < len(scan):
            c = scan[p]
            if c == "\\":
                nxt = scan[p + 1] if p + 1 < len(scan) else ""
                out.append({"n": "\n", "t": "\t", "r": "\r"}.get(nxt, nxt))
                p += 2
                continue
            if c == '"':
                p += 1
                break
            out.append(c)
            p += 1
        parts.append("".join(out))
    return "".join(parts) if parts else None


def rust_message_at(nc, pos):
    """The MESSAGE literal of the emission at `pos`, skipping a `target:` argument."""
    i = nc.find('"', pos)
    while i >= 0:
        if re.search(r"target\s*:\s*$", nc[max(0, i - 40) : i]):
            j = i + 1
            while j < len(nc):
                if nc[j] == "\\":
                    j += 2
                    continue
                if nc[j] == '"':
                    break
                j += 1
            i = nc.find('"', j + 1)
            continue
        break
    if i < 0:
        return None
    j, out = i + 1, []
    while j < len(nc):
        c = nc[j]
        if c == "\\":
            nxt = nc[j + 1] if j + 1 < len(nc) else ""
            if nxt == "\n":  # Rust line continuation: newline + indent elided
                j += 2
                while j < len(nc) and nc[j] in " \t":
                    j += 1
                continue
            out.append({"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\"}.get(nxt, nxt))
            j += 2
            continue
        if c == '"':
            break
        out.append(c)
        j += 1
    return "".join(out)


def commented_reference_calls(raw, code):
    """[(line, level, message)] for reference `log.*` calls preserved in comments."""
    raw_lines, code_lines = raw.split("\n"), code.split("\n")
    bodies = {}
    for idx, line in enumerate(raw_lines):
        m = re.search(r"//", line)
        if m and code_lines[idx][m.start() : m.start() + 2] != "//":
            bodies[idx + 1] = line[m.start() + 2 :]
    found = []
    for lineno in sorted(bodies):
        m = ZIG_CALL_IN_COMMENT.search(bodies[lineno])
        if not m:
            continue
        blob, j = bodies[lineno][m.end() - 1 :], lineno + 1
        while j in bodies and blob.count("(") > blob.count(")") and j - lineno < 8:
            blob += " " + re.sub(r"^[/\s]+", "", bodies[j])
            j += 1
        found.append((lineno, m.group(1), zig_format_arg(blob)))
    return found


def live_emission_messages(code, nc):
    """[(line, level, message)] for live emissions."""
    out = []
    for pos in primitive_sites(code, nc):
        msg = rust_message_at(nc, pos)
        m = re.match(r"log::(warn|error|info|debug|trace)!", code[pos : pos + 24])
        if m:
            level = LEVEL_ALIAS.get(m.group(1), m.group(1))
        else:  # a facade-bypassing print carries the level in its logFn prefix
            pm = LOGFN_PREFIX_TEXT.match(msg or "")
            level = LEVEL_ALIAS.get(pm.group(0).split("(")[0], pm.group(0).split("(")[0]) if pm else None
        out.append((code.count("\n", 0, pos) + 1, level, msg))
    return out


def pair_messages(raw, code, nc, window=14):
    """[(ref_line, ref_msg, emit_line, emit_msg)] — comment paired to emission.

    Three rules, each of which removed a wall of phantom mismatches when added:
      * the LEVEL must match. A comment block often holds calls at several levels
        with only some restored, and pairing a `log.debug` comment with a
        `log::warn!` two lines down compares two unrelated calls.
      * an EXACT normalised match among the pending comments wins. Ports restore
        a block in a different order often enough that positional pairing shifts
        every later pair.
      * otherwise the OLDEST pending comment within the window is used, matching
        the order a block is written and restored in.
    """
    refs = commented_reference_calls(raw, code)
    ems = live_emission_messages(code, nc)
    events = sorted(
        [(l, "r", (m, lv)) for l, lv, m in refs] + [(l, "e", (m, lv)) for l, lv, m in ems]
    )
    pending, out = [], []
    for line, kind, payload in events:
        if kind == "r":
            pending.append((line, payload))
            continue
        emsg, elevel = payload
        pending = [x for x in pending if line - x[0] <= window]
        same = [x for x in pending if x[1][1] == elevel or elevel is None]
        if not same:
            continue
        pending = [x for x in pending if x not in same] + same
        base = len(pending) - len(same)
        hit = next(
            (
                base + k
                for k, (_l, m) in enumerate(same)
                if m[0] is not None and normalise_message(m[0]) == normalise_message(emsg)
            ),
            None,
        )
        rline, m = pending.pop(hit if hit is not None else base)
        out.append((rline, m[0], line, emsg))
    return out


def reference_message_index(zig_walk):
    """{normalised reference message} over the whole reference tree, for N20."""
    # LIVE **and** COMMENTED-OUT reference calls both count as "the reference has
    # this". A port comment that faithfully reproduces a call the reference itself
    # keeps commented out (`Surface.zig`'s `SCROLL:` debug scaffolding, six such
    # sites) is not stranded — it is exactly as faithful as the reference is.
    index = set()
    for _rel, text in zig_walk():
        lines = text.split("\n")
        for i, line in enumerate(lines):
            if not (ZIG_CALL.search(zig_strip(line)) or ZIG_CALL_IN_COMMENT.search(line)):
                continue
            blob = " ".join(lines[i : i + 4])
            m = ZIG_CALL_IN_COMMENT.search(blob)
            if not m:
                continue
            msg = zig_format_arg(blob[m.end() - 1 :])
            if msg:
                index.add(normalise_message(msg))
    return index


ELIDED = ("\u2026", "...")


def is_stranded_comment(rn, ref_index):
    """True only for a comment that quotes a call the reference does not have.

    Precision matters more than recall here, because this fires on EVERY
    preserved comment in the port and a check that cries wolf is ignored within a
    day. A naive "message not in the index" test reports 101 sites, of which the
    known real population is 3; the other 98 are R1 comments that are
    deliberately not verbatim. Three filters, each removing a class that is
    legitimately non-verbatim rather than stranded:

      * ELIDED comments (`"… err={}"`, `"... err={}"`) quote a call by shape, not
        by text, and were never meant to be reproducible.
      * SHORT or single-word messages (`"render"`) are paraphrases; a stranded
        reference call is a real sentence.
      * ABBREVIATED comments quote a genuine call's message in part
        (`"cancelled by the user"` for `"open uri request was cancelled by the
        user"`), so a message that is a SUBSTRING of a real reference message is
        an abbreviation, not a stranding.

    The residue is what actually needs a human: text that reads like a real
    reference message and appears nowhere in the reference at the pin.
    """
    if not rn or any(e in rn for e in ELIDED):
        return False
    if len(rn) < 14 or " " not in rn.strip():
        return False
    if rn in ref_index:
        return False
    if any(rn in known for known in ref_index):
        return False
    # A comment may differ from the reference in PLACEHOLDER DECORATION
    # (`err=E{s}` written as `err={}`) or PARAMETERISE a literal the reference
    # spells out (`unable to remove {}` for `unable to remove rebuild source`).
    # Neither is a stranding: the call is there, the comment is just not verbatim.
    # Compare on the literal prefix before the first placeholder, which survives
    # both — and which is still a whole phrase, so it does not collapse distinct
    # messages together.
    prefix = rn.split("{")[0]
    if len(prefix) >= 14 and any(known.startswith(prefix) for known in ref_index):
        return False
    return True


DIVERGENCE_KINDS = (
    "identity-rename",
    "error-name-literal",
    "abbreviated-comment",
    "format-shape",
    "parameterised",
    "payload-absent",
    "stale-comment",
)
DIVERGENCE_DIRECTIVE = re.compile(r"^#!\s*divergence-cap\s+entries=(\d+)\s*$")


def load_divergences(path):
    """(rows, cap). Rows key on the MESSAGE PAIR, never on a line number.

    Line numbers drift — `LEDGER.tsv`'s rustty-side anchors have moved up to 70
    lines on rows closed earlier in this plan, because restoration moves code.
    A declaration keyed on text survives that; one keyed on a line does not.
    """
    rows, cap = [], None
    if not os.path.exists(path):
        return rows, cap
    with open(path, encoding="utf-8", newline="") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            m = DIVERGENCE_DIRECTIVE.match(line)
            if m:
                cap = int(m.group(1))
                continue
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if parts[0].strip() == "kind":
                continue
            if len(parts) < 5:
                raise ValueError(
                    f"{path}:{lineno}: expected 5 tab-separated columns "
                    "(kind, port_file, reference_message, port_message, reason)"
                )
            def unesc(v):
                # Messages can contain a TAB (`"[rebuildCells time] {}\t{}"`) or a
                # newline, and this file is tab-separated. They are stored escaped.
                return v.replace("\\t", "\t").replace("\\n", "\n").replace("\\\\", "\\")

            rows.append(
                {
                    "kind": parts[0].strip(),
                    "file": parts[1].strip(),
                    "ref": unesc(parts[2]),
                    "port": unesc(parts[3]),
                    "reason": parts[4].strip(),
                    "lineno": lineno,
                }
            )
    return rows, cap


# ---------------------------------------------------------------------------
# Portmap
# ---------------------------------------------------------------------------
EXCLUSION_DIRECTIVE = re.compile(
    r"^#!\s*exclusion-cap\s+files=(\d+)\s+sites=(\d+)\s*$"
)


def load_exclusion_cap(path):
    """The frozen exclusion budget, from the portmap's `#!exclusion-cap` directive.

    WHY THIS IS NOT OPTIONAL. Every other number in this gate is a ratchet:
    allowances ratchet, bypass caps ratchet. The exclusion list did not, and it is
    the WIDEST possible lever — reclassifying `src/termio/Exec.zig` as
    `no-port: nothing implements this` erased 58 sites of deficit and left the
    gate GREEN. A stage under pressure could retire a whole file instead of
    restoring it and the gate would applaud. The whole argument for a closed
    category set is that an exclusion can be counted and argued; counting it means
    counting it AGAINST A FROZEN NUMBER.

    Absent directive is BLOCKED, not a default: a cap that can be disabled by
    deleting a line is not a cap.
    """
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            m = EXCLUSION_DIRECTIVE.match(raw.rstrip("\n"))
            if m:
                return {"files": int(m.group(1)), "sites": int(m.group(2))}
    return None


def load_portmap(path):
    """Rows of the declared reference -> port mapping.

    Columns: oracle_file, port_arms, allowance, bypass_cap, reason.
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
            if len(parts) < 5:
                raise ValueError(
                    f"{path}:{lineno}: expected 5 tab-separated columns "
                    "(oracle_file, port_arms, allowance, bypass_cap, reason)"
                )
            # `-` in port_arms marks an EXCLUSION: a reference file with live log
            # sites that this port deliberately does not carry. It still has to be
            # DECLARED, because silence is indistinguishable from coverage.
            excluded = parts[1].strip() == "-"
            arms = (
                []
                if excluded
                else [
                    [f.strip() for f in arm.split("+") if f.strip()]
                    for arm in parts[1].split("|")
                ]
            )
            rows.append(
                {
                    "oracle": parts[0].strip(),
                    "arms": [a for a in arms if a],
                    "excluded": excluded,
                    "allowance": parts[2].strip(),
                    "bypass_cap": parts[3].strip(),
                    "reason": parts[4].strip(),
                    "lineno": lineno,
                }
            )
    return rows


# An exclusion must say WHICH KIND it is, from a closed set. A free-text excuse is
# how a coverage hole becomes permanent; a category can be counted and argued.
EXCLUSION_KINDS = ("no-port", "build-tooling", "vendored", "unresolved")


def oracle_files_with_log_sites(zig_walk):
    """{reference path: live log site count} over the WHOLE reference tree.

    THE POINT: the portmap was originally seeded from the audit's LOG rows, so it
    inherited exactly the audit's blind spots — the unaudited-head problem one
    level up, at FILE granularity. Measured 2026-09-10: 172 reference files carry
    live `log.*` sites and only 60 of them were declared, leaving 523 sites in
    112 files that the gate could not see and would never mention. Enumerating the
    reference directly is the only way a missing file can be a finding rather than
    a silence.
    """
    out = {}
    for rel, text in zig_walk():
        n = len(zig_sites(text))
        if n:
            out[rel] = n
    return out


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
def verdict(portmap, claims, zig_text, rust_text, facade_sources=None, report=False,
            oracle_log_files=None, exclusion_cap=None, raw_sources=None,
            ref_index=None, divergences=None, divergence_cap=None):
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

    rows, total_expected, total_actual, total_bypass = [], 0, 0, 0
    missing_oracle, excluded_rows = [], []
    for r in portmap:
        if r.get("excluded"):
            excluded_rows.append(r)
            continue
        txt = zig_text(r["oracle"])
        if txt is None:
            missing_oracle.append(r["oracle"])
            continue
        expected = len(zig_sites(txt))
        total_expected += expected
        arm_counts, arm_bypass = [], []
        for arm in r["arms"]:
            present = [f for f in arm if f in pool]
            arm_counts.append(
                len(rust_emissions(present, pool, emitters, defs)) if present else 0
            )
            arm_bypass.append(len(rust_direct_prints(present, pool)) if present else 0)
        actual = min(arm_counts) if arm_counts else 0
        # MAX over arms, not min: a bypass on ANY platform arm is a bypass.
        bypass = max(arm_bypass) if arm_bypass else 0
        total_actual += actual
        total_bypass += bypass
        deficit = max(0, expected - actual)
        rows.append((r, expected, arm_counts, actual, deficit, bypass))

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
        for r, expected, arm_counts, actual, deficit, bypass in sorted(
            rows, key=lambda x: (-x[4], x[0]["oracle"])
        ):
            out.append(
                f"{deficit:>7}  {expected:>4} {actual:>5}  {r['oracle']}"
                f"   allowance={r['allowance']}"
                + (f"   bypass={bypass}/{r['bypass_cap']}" if bypass or r["bypass_cap"] not in ("0", "") else "")
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
            f"{sum(1 for x in rows if x[4])} file(s); "
            f"{sum(x[5] for x in rows)} facade-BYPASSING emission(s) over "
            f"{sum(1 for x in rows if x[5])} file(s) ---"
        )
        out.append("")
        out = []

    shortfalls, stale, unreasoned, bad = [], [], [], []
    bypass_over, bypass_stale = [], []
    for r, expected, arm_counts, actual, deficit, bypass in rows:
        try:
            allowance = int(r["allowance"])
            cap = int(r["bypass_cap"])
        except ValueError:
            bad.append(
                f"{r['oracle']}: allowance={r['allowance']!r} / "
                f"bypass_cap={r['bypass_cap']!r} is not an integer"
            )
            continue
        if allowance < 0 or cap < 0:
            bad.append(f"{r['oracle']}: allowance/bypass_cap must not be negative")
            continue
        if (allowance or cap) and not r["reason"]:
            unreasoned.append(r["oracle"])
        if deficit > allowance:
            shortfalls.append((r, expected, arm_counts, actual, deficit, allowance))
        elif deficit < allowance:
            stale.append((r["oracle"], allowance, deficit))
        if bypass > cap:
            bypass_over.append((r["oracle"], bypass, cap))
        elif bypass < cap:
            bypass_stale.append((r["oracle"], cap, bypass))

    out.append(
        f"G-LOGFILE: {len(rows)} reference file(s) — "
        f"{total_expected} live reference log site(s), {total_actual} live port "
        f"emission(s) on the thinnest arm ({total_bypass} of them BYPASSING the "
        f"facade — N14), {len(emitters)} log facade name(s) resolved"
    )
    if excluded_rows:
        kinds = {}
        for r in excluded_rows:
            k = r["reason"].split(":", 1)[0].strip()
            kinds[k] = kinds.get(k, 0) + 1
        ex_sites = sum(
            (oracle_log_files or {}).get(r["oracle"], 0) for r in excluded_rows
        )
        budget = (
            f" (budget {exclusion_cap['files']} file(s) / {exclusion_cap['sites']} site(s))"
            if exclusion_cap
            else ""
        )
        out.append(
            f"           {len(excluded_rows)} reference file(s) DECLARED-EXCLUDED "
            f"holding {ex_sites} site(s){budget}: "
            + ", ".join(f"{v} {k}" for k, v in sorted(kinds.items()))
        )
    if oracle_log_files:
        total_ref = sum(oracle_log_files.values())
        live_rows = set(oracle_log_files) & {x[0]["oracle"] for x in rows}
        live_excl = set(oracle_log_files) & {r["oracle"] for r in excluded_rows}
        out.append(
            f"           coverage: {len(live_rows)} of {len(oracle_log_files)} reference "
            f"file(s) with live log sites reconciled"
            + (f" (+{len(live_excl)} excluded)" if live_excl else "")
            + f", {total_expected} of {total_ref} site(s) "
            f"({100 * total_expected // max(1, total_ref)}%)"
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
            "RED: a non-zero allowance or bypass_cap MUST name its justification in "
            "the `reason` "
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
    # --- COVERAGE: every reference file with live log sites is declared or excluded
    if oracle_log_files:
        known = {r["oracle"] for r in portmap}
        uncovered = sorted(set(oracle_log_files) - known)
        if uncovered:
            n = sum(oracle_log_files[f] for f in uncovered)
            out.append(
                f"RED: {len(uncovered)} reference file(s) carry live `log.*` sites and are "
                f"neither declared nor excluded ({n} site(s) invisible to this gate). "
                "Silence is not coverage — declare the port arms or record an exclusion:"
            )
            for f in sorted(uncovered, key=lambda x: -oracle_log_files[x])[:12]:
                out.append(f"       {oracle_log_files[f]:>4}  {f}")
            if len(uncovered) > 12:
                out.append(f"       ... and {len(uncovered) - 12} more")
            rc = 1
    if exclusion_cap is not None:
        n_files = len(excluded_rows)
        n_sites = sum(
            (oracle_log_files or {}).get(r["oracle"], 0) for r in excluded_rows
        )
        for label, got, cap in (
            ("excluded file", n_files, exclusion_cap["files"]),
            ("excluded site", n_sites, exclusion_cap["sites"]),
        ):
            if got > cap:
                out.append(
                    f"RED: {got} {label}(s), budget {cap}. An exclusion RETIRES a "
                    "reference file instead of reconciling it, so it is the widest "
                    "lever in this gate — reclassifying one large file as `no-port:` "
                    "erases its whole deficit and would otherwise stay GREEN. Raising "
                    "this budget is a deliberate, reviewed edit to the portmap's "
                    "`#!exclusion-cap` directive, never a side effect."
                )
                rc = 1
            elif got < cap:
                out.append(
                    f"RED: {got} {label}(s), budget {cap} — the same RATCHET as the "
                    "allowances and the bypass caps: tighten `#!exclusion-cap` in the "
                    "commit that brought the file back into scope."
                )
                rc = 1
    bad_excl = []
    for r in excluded_rows:
        kind = r["reason"].split(":", 1)[0].strip()
        if kind not in EXCLUSION_KINDS:
            bad_excl.append((r["oracle"], r["reason"][:40]))
    if bad_excl:
        out.append(
            f"RED: {len(bad_excl)} exclusion(s) do not name a category from "
            f"{EXCLUSION_KINDS}. A free-text excuse is how a coverage hole becomes "
            "permanent; a category can be counted and argued:"
        )
        for f, why in bad_excl[:8]:
            out.append(f"       {f}: reason starts {why!r}")
        rc = 1
    if bypass_over:
        out.append(
            f"RED: {len(bypass_over)} file(s) emit MORE facade-bypassing direct prints "
            "than declared. `eprintln!(\"warn(scope): ...\")` reproduces logFn's output "
            "but not its behaviour — it ignores Logging.stderr, never reaches macOS "
            "unified logging, and has no level or scope to filter on (N14). Adding one "
            "is not a way to close a count deficit:"
        )
        for f, got, cap in bypass_over[:10]:
            out.append(f"       {f}: {got} direct print(s), cap {cap}")
        rc = 1
    if bypass_stale:
        out.append(
            f"RED: {len(bypass_stale)} bypass cap(s) now exceed the measured count — "
            "the same RATCHET as the allowances: tighten them in the commit that routed "
            "the sites through the facade."
        )
        for f, cap, got in bypass_stale[:10]:
            out.append(f"       {f}: cap={cap}, measured={got}")
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
    # --- MESSAGE CONTENT + stale preserved comments (one scan, two verdicts) ----
    if raw_sources is not None and ref_index is not None:
        decl = divergences or []
        seen = set()
        mismatches, stale = [], []
        n_paired = n_emissions = 0
        for path in sorted(pool):
            raw = raw_sources.get(path)
            if raw is None:
                continue
            code, nc = pool[path]
            n_emissions += len(primitive_sites(code, nc))
            for rline, rmsg, eline, emsg in pair_messages(raw, code, nc):
                if rmsg is None or emsg is None:
                    continue
                n_paired += 1
                rn, en = normalise_message(rmsg), normalise_message(emsg)
                if rn != en:
                    mismatches.append((path, eline, rn, en))
            for rline, _lvl, rmsg in commented_reference_calls(raw, code):
                if rmsg is None:
                    continue
                rn = normalise_message(rmsg)
                if is_stranded_comment(rn, ref_index):
                    stale.append((path, rline, rn))

        def declared(kind_set, path, rn, en):
            for d in decl:
                if (
                    d["kind"] in kind_set
                    and d["file"] == path
                    and normalise_message(d["ref"]) == rn
                    and normalise_message(d["port"] or "") == (en or "")
                ):
                    seen.add(d["lineno"])
                    return True
            return False

        content_kinds = tuple(k for k in DIVERGENCE_KINDS if k != "stale-comment")
        undeclared_mm = [
            x for x in mismatches if not declared(content_kinds, x[0], x[2], x[3])
        ]
        undeclared_st = [
            x for x in stale if not declared(("stale-comment",), x[0], x[2], "")
        ]
        bad_kind = [d for d in decl if d["kind"] not in DIVERGENCE_KINDS]
        no_reason = [d for d in decl if not d["reason"]]
        dead = [d for d in decl if d["lineno"] not in seen]
        out.append(
            f"           message content: {n_paired} of {n_emissions} emission(s) have a "
            f"preserved reference comment to check against "
            f"({100 * n_paired // max(1, n_emissions)}%; the rest are UNCHECKED by this "
            "pass — it can only compare what the port wrote down)"
        )
        out.append(
            f"           message content: {len(mismatches)} divergence(s) and "
            f"{len(stale)} stale preserved comment(s), {len(decl)} declared"
            + (f" (cap {divergence_cap})" if divergence_cap is not None else "")
        )
        if undeclared_mm:
            out.append(
                f"RED: {len(undeclared_mm)} emission(s) whose text does not match the "
                "reference call preserved above them. A count gate cannot see a message "
                "that is PRESENT but truncated — that is why this check exists. Fix the "
                "text, or declare the divergence in LOG-TEXT-DIVERGENCES.tsv:"
            )
            for path, line, rn, en in undeclared_mm[:10]:
                out.append(f"       {path}:{line}")
                out.append(f"           reference {rn!r}")
                out.append(f"           port      {en!r}")
            rc = 1
        if undeclared_st:
            out.append(
                f"RED: {len(undeclared_st)} preserved comment(s) quote a reference call "
                "that does not exist at the pin (N20). Nothing emits them, so nothing is "
                "wrong today — the risk is a future stage reading the comment and "
                "faithfully restoring a call the reference never had:"
            )
            for path, line, rn in undeclared_st[:10]:
                out.append(f"       {path}:{line}  {rn!r}")
            rc = 1
        if bad_kind:
            out.append(
                f"RED: {len(bad_kind)} declared divergence(s) with a kind outside "
                f"{DIVERGENCE_KINDS}"
            )
            rc = 1
        if no_reason:
            out.append(
                f"RED: {len(no_reason)} declared divergence(s) with no reason — the same "
                "rule as an allowance or a WAIVED row's owner"
            )
            rc = 1
        if dead:
            out.append(
                f"RED: {len(dead)} declared divergence(s) match nothing in the tree. A "
                "declaration that outlives what it excused is how a waiver becomes "
                "permanent; delete it in the commit that fixed the text:"
            )
            for d in dead[:6]:
                out.append(f"       {d['file']}: {normalise_message(d['ref'])!r}")
            rc = 1
        if divergence_cap is not None:
            if len(decl) > divergence_cap:
                out.append(
                    f"RED: {len(decl)} declared divergence(s), cap {divergence_cap}. "
                    "Declaring a text divergence excuses a real difference from the "
                    "reference; the number of excuses is capped and reviewed."
                )
                rc = 1
            elif len(decl) < divergence_cap:
                out.append(
                    f"RED: {len(decl)} declared divergence(s), cap {divergence_cap} — the "
                    "same RATCHET as everything else here: tighten it in the commit that "
                    "removed the declaration."
                )
                rc = 1

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


LITERAL_SHAPES = [
    # (name, source, expected live `log::` sites)
    ("b'\"' — the DEC intermediate that started all this",
     'fn a() { match x { b\'"\' => {} }; log::warn!(target: S, "live"); }\n', 1),
    ("b'\\\\' — a char literal whose content IS a backslash",
     'fn a() { match x { b\'\\\\\' => {} }; log::warn!(target: S, "live"); }\n', 1),
    ("'\\u{1f600}' — a unicode escape",
     'fn a() { let c = \'\\u{1f600}\'; log::warn!(target: S, "live"); }\n', 1),
    ("a lifetime is not a char literal",
     'fn a<\'x>(v: &\'x str) { log::warn!(target: S, "live"); }\n', 1),
    ("an apostrophe in a doc comment",
     '/// don\'t do this\nfn a() { log::warn!(target: S, "live"); }\n', 1),
    ("an escaped quote inside a normal string",
     'fn a() { let s = "he said \\"hi\\""; log::warn!(target: S, "live"); }\n', 1),
    ("r\"...\\\" — a raw string ending in a backslash",
     'fn a() { let x = r"foo\\"; log::warn!(target: S, "live"); }\n', 1),
    ("br\"...\\\" — a RAW BYTE string ending in a backslash",
     'fn a() { let x = br"foo\\"; log::warn!(target: S, "live"); }\n', 1),
    ("br#\"...\"# — a hashed raw byte string",
     'fn a() { let x = br#"a"b\\"#; log::warn!(target: S, "live"); }\n', 1),
    ("b\"...\" — a byte string, where escapes DO apply",
     'fn a() { let x = b"foo\\""; log::warn!(target: S, "live"); }\n', 1),
    ("an identifier ending in `br` is not a raw literal",
     'fn a() { let abr = 1; log::warn!(target: S, "live"); }\n', 1),
    ("a `log::` inside a raw byte string is NOT code",
     'fn a() { let x = br"log::warn!(x)"; }\n', 0),
]


def literal_shape_check():
    """(ok, lines) — every Rust literal shape that can desync a naive scanner.

    Two entries here are regression pins for bugs that shipped: `b'"'` (fix round
    1, which blanked ~1200 lines of `stream.rs` and froze a phantom 49-site
    deficit) and `br"...\\"` (fix round 2, dormant — no mapped port file has one —
    but the identical failure class, and a dormant scanner desync fails
    FALSE-GREEN). Both are cheap to hold onto and expensive to rediscover.
    """
    out, ok = [], True
    for name, src, want in LITERAL_SHAPES:
        code, nc = rust_strip(src), rust_strip_comments_only(src)
        offsets = len(code) == len(src) and len(nc) == len(src)
        got = len(primitive_sites(code, nc))
        good = got == want and offsets
        if not good:
            ok = False
            out.append(
                f"  [FAIL] literal shape: {name} -> {got} live (expected {want}), "
                f"offsets preserved={offsets}"
            )
    out.insert(
        0,
        f"  [{'ok ' if ok else 'FAIL'}] {len(LITERAL_SHAPES)} Rust literal shape(s) "
        "parsed correctly (char, byte, raw, raw-byte, lifetime, doc comment)",
    )
    return ok, out


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
    olf = {"src/ok.zig": 1, "src/two.zig": 2}  # the "reference tree" for coverage arms

    def run(portmap_rows, claims=None, coverage=None, excap=None):
        # rows are (oracle, arms, allowance, reason) or (…, allowance, cap, reason)
        pm = []
        for row in portmap_rows:
            o, arms, a = row[0], row[1], row[2]
            cap, r = ("0", row[3]) if len(row) == 4 else (row[3], row[4])
            pm.append(
                {
                    "oracle": o,
                    "excluded": arms.strip() == "-",
                    "arms": (
                        []
                        if arms.strip() == "-"
                        else [
                            [f.strip() for f in arm.split("+")]
                            for arm in arms.split("|")
                        ]
                    ),
                    "allowance": a,
                    "bypass_cap": cap,
                    "reason": r,
                    "lineno": 0,
                }
            )
        return verdict(
            pm, claims or {}, zt, rt, oracle_log_files=coverage, exclusion_cap=excap
        )

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
         run([("src/eprint.zig", "c/eprint.rs", "0", "1", "N14: declared bypass")])[0], 0)
    )
    cases.append(
        ("a bare eprintln! is NOT an emission",
         run([("src/eprint2.zig", "c/eprint.rs", "0", "1", "N14")])[0], 1)
    )
    # 10c. the BYPASS ratchet: a direct print over the declared cap is RED, and a
    #      cap larger than what is measured is RED too (same ratchet as allowances).
    #      `c/eprint.rs` carries exactly one facade-bypassing print.
    cases.append(
        ("facade bypass over the declared cap",
         run([("src/eprint.zig", "c/eprint.rs", "0", "0", "")])[0], 1)
    )
    cases.append(
        ("facade bypass within a reasoned cap",
         run([("src/eprint.zig", "c/eprint.rs", "0", "1", "N14: GTK subtree")])[0], 0)
    )
    cases.append(
        ("stale (too-large) bypass cap",
         run([("src/eprint.zig", "c/eprint.rs", "0", "2", "N14")])[0], 1)
    )
    cases.append(
        ("bypass cap with no reason",
         run([("src/eprint.zig", "c/eprint.rs", "0", "1", "")])[0], 1)
    )
    # 10d. a facade emission is NOT a bypass — `c/ok.rs` uses `log::warn!`.
    cases.append(
        ("a log:: emission is not counted as a bypass",
         run([("src/ok.zig", "c/ok.rs", "0", "0", "")])[0], 0)
    )
    # 10e. COVERAGE: a reference file with live log sites that the portmap neither
    #      declares nor excludes is RED. This is the hole that let 112 files and 523
    #      sites sit outside the gate entirely — the portmap was seeded from the
    #      audit's LOG rows, so it inherited the audit's blind spots.
    cases.append(
        ("a reference file with log sites and no portmap row",
         run([("src/ok.zig", "c/ok.rs", "0", "0", "")], None, olf)[0], 1)
    )
    cases.append(
        ("...covered once both files are declared",
         run([("src/ok.zig", "c/ok.rs", "0", "0", ""),
              ("src/two.zig", "c/mac.rs", "0", "0", "")], None, olf)[0], 0)
    )
    cases.append(
        ("...or once the second is EXCLUDED with a category",
         run([("src/ok.zig", "c/ok.rs", "0", "0", ""),
              ("src/two.zig", "-", "0", "0", "no-port: nothing implements this")], None, olf)[0], 0)
    )
    cases.append(
        ("an exclusion with no category is RED",
         run([("src/ok.zig", "c/ok.rs", "0", "0", ""),
              ("src/two.zig", "-", "0", "0", "we did not get to it")], None, olf)[0], 1)
    )
    # 10f. THE EXCLUSION RATCHET. An exclusion retires a reference file rather than
    #      reconciling it, so it erases that file's whole deficit — the widest lever
    #      in the gate, and the only number that was unratcheted. `src/two.zig` holds
    #      2 live sites; the budget below is 1 file / 2 sites.
    EXCAP = {"files": 1, "sites": 2}
    ok_excl = [("src/ok.zig", "c/ok.rs", "0", "0", ""),
               ("src/two.zig", "-", "0", "0", "no-port: nothing implements this")]
    cases.append(
        ("exclusions exactly at budget",
         run(ok_excl, None, olf, EXCAP)[0], 0)
    )
    cases.append(
        ("one exclusion OVER budget (a file retired to erase its deficit)",
         run([("src/ok.zig", "-", "0", "0", "no-port: x")] + ok_excl[1:],
             None, olf, EXCAP)[0], 1)
    )
    cases.append(
        ("exclusions UNDER budget — stale, must be tightened",
         run([("src/ok.zig", "c/ok.rs", "0", "0", ""),
              ("src/two.zig", "c/mac.rs", "0", "0", "")], None, olf, EXCAP)[0], 1)
    )
    cases.append(
        ("right file count, too many excluded SITES",
         run([("src/ok.zig", "c/ok.rs", "0", "0", ""),
              ("src/two.zig", "-", "0", "0", "no-port: x")],
             None, olf, {"files": 1, "sites": 1})[0], 1)
    )
    cases.append(
        ("right file count, budgeted for more SITES than are excluded",
         run(ok_excl, None, olf, {"files": 1, "sites": 3})[0], 1)
    )
    # 10g. MESSAGE CONTENT. A count gate cannot see a message that is present but
    #      wrong; these arms are the proof that this one can. `c/txt.rs` carries a
    #      preserved reference comment above a matching emission.
    TXT_OK = (
        'fn a() {\n    // log.warn("mode: {} err={}", .{ m, err });\n'
        '    log::warn!(target: S, "mode: {m} err={err:?}");\n}\n'
    )
    TXT_TRUNC = TXT_OK.replace('"mode: {m} err={err:?}"', '"err={err:?}"')
    TXT_NOSPACE = TXT_OK.replace('"mode: {m} err={err:?}"', '"mode:{m} err={err:?}"')
    TXT_STALE = (
        'fn a() {\n    // log.warn("a message the reference does not have", .{});\n}\n'
    )
    REF_INDEX = {"mode: {} err={}"}

    def run_txt(src, decl=(), cap=None, allowance="0"):
        rs2 = dict(rs, **{"c/txt.rs": src})
        pm = [{"oracle": "src/ok.zig", "excluded": False, "arms": [["c/txt.rs"]],
               "allowance": allowance, "bypass_cap": "0",
               "reason": "no emission in this fixture" if allowance != "0" else "",
               "lineno": 0}]
        return verdict(
            pm, {}, zt, lambda p: rs2.get(p), oracle_log_files=None,
            raw_sources={"c/txt.rs": src}, ref_index=REF_INDEX,
            divergences=list(decl), divergence_cap=cap,
        )[0]

    cases.append(("message text matches the preserved reference comment",
                  run_txt(TXT_OK), 0))
    cases.append(("HALF THE MESSAGE MISSING — the defect a count gate cannot see",
                  run_txt(TXT_TRUNC), 1))
    cases.append(("a dropped space before a placeholder (`mode:{}` for `mode: {}`)",
                  run_txt(TXT_NOSPACE), 1))
    cases.append(("...unless the divergence is declared with a reason",
                  run_txt(TXT_TRUNC, [{"kind": "format-shape", "file": "c/txt.rs",
                                       "ref": "mode: {} err={}", "port": "err={}",
                                       "reason": "declared", "lineno": 1}]), 0))
    cases.append(("a declaration with no reason is RED",
                  run_txt(TXT_TRUNC, [{"kind": "format-shape", "file": "c/txt.rs",
                                       "ref": "mode: {} err={}", "port": "err={}",
                                       "reason": "", "lineno": 1}]), 1))
    cases.append(("a declaration matching nothing is RED",
                  run_txt(TXT_OK, [{"kind": "format-shape", "file": "c/txt.rs",
                                    "ref": "gone", "port": "gone",
                                    "reason": "x", "lineno": 1}]), 1))
    cases.append(("a comment quoting a call the reference does not have (N20)",
                  run_txt(TXT_STALE, allowance="1"), 1))
    cases.append(("...unless it is declared as a stale comment",
                  run_txt(TXT_STALE, [{"kind": "stale-comment", "file": "c/txt.rs",
                                       "ref": "a message the reference does not have",
                                       "port": "", "reason": "N20", "lineno": 1}],
                          allowance="1"), 0))
    cases.append(("divergence cap exceeded",
                  run_txt(TXT_OK, [], cap=1), 1))
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

    lit_ok, lit_lines = literal_shape_check()
    live_ok, live_lines = live_scanner_check()
    ok = lit_ok and live_ok
    for line in lit_lines + live_lines:
        print(line)
    for name, got, want in cases:
        mark = "ok " if got == want else "FAIL"
        if got != want:
            ok = False
        print(f"  [{mark}] {name:52} -> {got} (expect {want})")
    print(
        "SELF-TEST GREEN: the gate flips RED on a short file, an unreasoned or stale\n"
        "allowance, a short PLATFORM TWIN that summing would have hidden, a no-op\n"
        "log stub, an undeclared file, a missing port file, a vacuous scan, and a\n"
        "facade-BYPASSING direct print over its declared cap, a TRUNCATED or\n"
        "respaced message text, a comment quoting a call the reference does not have,\n"
        "an UNDECLARED reference\n"
        "file with live log sites and an uncategorised exclusion — and the scanner agrees\n"
        "with a naive grep on every mapped port file and on every Rust literal shape."
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
    exclusion_cap = load_exclusion_cap(PORTMAP)
    if exclusion_cap is None:
        print(
            "BLOCKED: the portmap carries no `#!exclusion-cap files=N sites=N`\n"
            "         directive. An exclusion retires a reference file instead of\n"
            "         reconciling it, so it is the widest lever in this gate; a cap\n"
            "         that can be disabled by deleting a line is not a cap.",
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

    raw_sources = {}
    for r in portmap:
        for arm in r["arms"]:
            for f in arm:
                full = os.path.join(ROOT, f)
                if os.path.isfile(full) and f not in raw_sources:
                    with open(full, encoding="utf-8", errors="replace") as fh:
                        raw_sources[f] = fh.read()
    try:
        divergences, divergence_cap = load_divergences(DIVERGENCES)
    except ValueError as exc:
        print(f"BLOCKED: {exc}", file=sys.stderr)
        return 3
    if divergence_cap is None:
        print(
            "BLOCKED: LOG-TEXT-DIVERGENCES.tsv carries no `#!divergence-cap\n"
            "         entries=N` directive. A declared text divergence excuses a real\n"
            "         difference from the reference; a cap that can be disabled by\n"
            "         deleting a line is not a cap.",
            file=sys.stderr,
        )
        return 3

    def rust_text(rel):
        p = os.path.join(ROOT, rel)
        if not os.path.isfile(p):
            return None
        with open(p, encoding="utf-8", errors="replace") as fh:
            return fh.read()

    def zig_walk():
        for root in ("src", "pkg"):
            base = os.path.join(ORACLE, root)
            for dirpath, _dirs, names in os.walk(base):
                for name in names:
                    if not name.endswith(".zig"):
                        continue
                    full = os.path.join(dirpath, name)
                    with open(full, encoding="utf-8", errors="replace") as fh:
                        yield os.path.relpath(full, ORACLE), fh.read()

    oracle_log_files = oracle_files_with_log_sites(zig_walk)
    ref_index = reference_message_index(zig_walk)
    if not oracle_log_files:
        print(
            "BLOCKED: the oracle tree yielded no files with live log sites — the "
            "reference scanner is not binding",
            file=sys.stderr,
        )
        return 3

    # Every .rs in the source root that owns a mapped port file, for facade
    # discovery: `opengl/gl.rs` defines the macros `opengl/gl/sys.rs` emits
    # through, and `xtask/` is a source root that is not under `crates/`.
    facade = {}
    crates_seen = set()
    for r in portmap:
        for arm in r["arms"]:
            for f in arm:
                parts = f.split("/")
                if len(parts) > 2 and parts[0] == "crates":
                    crates_seen.add(os.path.join(parts[0], parts[1], "src"))
                elif len(parts) > 1:
                    crates_seen.add(os.path.join(parts[0], "src"))
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
        portmap,
        claims,
        zig_text,
        rust_text,
        facade,
        report="--report" in sys.argv,
        oracle_log_files=oracle_log_files,
        exclusion_cap=exclusion_cap,
        raw_sources=raw_sources,
        ref_index=ref_index,
        divergences=divergences,
        divergence_cap=divergence_cap,
    )
    print("\n".join(lines))
    return rc


if __name__ == "__main__":
    sys.exit(main())
