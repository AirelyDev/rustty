"""
_g3b_not_carried.py — the audited NOT-CARRIED allowance ledger for G3b
(`check_comment_coverage.py`). Parity tail S-8d, 2026-09-24.

WHY. G3b demands that every distinctive reference comment appear in the port.
Some reference comments are deliberately not carried:

  * `no-counterpart` — the comment documents (or sits inside) reference code that
    has no Rust counterpart by a RECORDED decision (a NON_PORTS row, a DEFERRALS
    entry, an UPSTREAM-DEFECTS or ADJUDICATION entry). Restoring it would leave an
    orphan comment describing code that does not exist.
  * `gag-prose` — snarky/gag editorializing stripped under the prose-fidelity
    carve-out (`strip-ghostty-gag-prose`, `.port/phase-3/NON_PORTS.md`).

Before this ledger those comments stayed MISSING for ever, so a crate with any of
them could never reach 0 and every real drop was buried in permanent noise. This
module lets a reviewer declare them — and makes each declaration carry
machine-checkable evidence, so the ledger cannot become an escape hatch.

THE FILE: `.port/parity-closure/COMMENT-NOT-CARRIED.tsv`, 7 tab-separated columns

  oracle_file  path relative to the oracle's `src/` (a crate-map file).
  line         1-based line of the comment in that file AT THE PIN (`ORACLE_PIN`).
  class        `no-counterpart` | `gag-prose`.
  decl         no-counterpart: the reference identifier the comment documents, sits
               inside, or explains (a fn/const/var it declares, or an identifier its
               code uses), or `Container.member` for a member whose name is too
               generic to be unique workspace-wide (then the absence is proven
               inside `impl <Container>` blocks, and the evidence must name both).
               gag-prose: `-`.
  evidence     `<doc path>#<needle>`: a literal needle in one of EVIDENCE_DOCS.
  text         the comment exactly as it reads in the oracle, marker stripped.
  reason       mandatory, individual justification (>= REASON_MIN_WORDS words).

plus a `#!not-carried-cap entries=N` directive: a two-way ratchet (a missing
directive BLOCKS; more or fewer rows than N is RED), the LOG-DEAD-SITES
precedent.

WHAT EVERY ROW MUST PROVE (any failure is RED):

  1. It exists in the oracle AT THE PIN: `git show <pin>:src/<oracle_file>` line
     `line` carries a comment whose marker-stripped body is exactly `text`, and
     that comment is DISTINCTIVE (G3b would count it). No row may govern a line
     G3b never examines.
  2. It is still NOT carried: the comment is MISSING or REVIEW against the
     crate's port pool. A row whose comment the port now carries is STALE.
  3. The class rules:
     no-counterpart
       a. The comment is ATTACHED to oracle code naming `decl`: it lies inside a
          brace-balanced block opened on a code line naming `decl` (the
          declaration `const DllMain = ... struct {`, or a switch arm
          `adw.MessageDialog => {`), or `decl` appears in code on the comment's
          line or within the next ATTACH_CODE_LINES code lines.
       b. `decl` has NO Rust counterpart anywhere in the workspace (crates/,
          crates-sys/, xtask/ — comments and strings stripped; the R4 "ported
          somewhere else" trap), over every G1 name candidate of `decl`:
          - `decl` DECLARED in the oracle file (`fn|const|var <decl>`): no Rust
            item (`fn|const|static|struct|enum|type|mod|trait|union`) has it;
          - otherwise (an identifier the oracle code uses): no Rust code
            mentions it at all.
       c. `evidence` names an EVIDENCE_DOCS file, the needle occurs in it, the
          record around the needle (its table row, or its section up to the
          next heading) names `decl`, and that record is not struck or marked
          RESOLVED/PORTED (a struck row must not vote).
     gag-prose
       a. `decl` is `-`; `evidence` is an EVIDENCE_DOCS anchor whose record names
          `strip-ghostty-gag-prose`.
       b. The text carries no technical content: no digits, backticks, `@`,
          `::`, `()`, snake_case / camelCase / dotted identifiers or ALL-CAPS
          tokens (bar the ALL_CAPS_PROSE words). Never overridable — a comment
          carrying technical information can never be gag prose.
       c. At most GAG_MAX_WORDS words unless the reason starts `JUSTIFIED-LONG:`.
  4. No duplicate (oracle_file, line).

HOW A ROW IS APPLIED. A row removes its normalized text from the crate's MISSING
/ REVIEW lists only if EVERY distinctive occurrence of that normalized text in the
crate's reference files is covered by a row — so a phrase that also appears at a
site that must be carried stays MISSING. Applied rows are PRINTED, never hidden:
`NOT-CARRIED (<class>): <file>:<line> <text>`.
"""
import os
import re

EVIDENCE_DOCS = (
    ".port/phase-3/NON_PORTS.md",
    ".port/phase-4/NON_PORTS.md",
    ".port/phase-2/DEFERRALS.md",
    ".port/parity-closure/UPSTREAM-DEFECTS.md",
    ".port/parity-closure/ADJUDICATION.md",
)
CLASSES = ("no-counterpart", "gag-prose")
CAP_RE = re.compile(r"^#!\s*not-carried-cap\s+entries=(\d+)\s*$")
GAG_MAX_WORDS = 20
REASON_MIN_WORDS = 6
ATTACH_CODE_LINES = 8
GAG_RULE = "strip-ghostty-gag-prose"
RUST_ITEM_KW = r"(?:fn|const|static|struct|enum|type|mod|trait|union)"
# W4 (S-8e review): an evidence record is at most this many lines — a heading
# needle yields its own section up to the NEXT heading of any level, and a
# larger record is RED (a big section "names" almost any identifier).
RECORD_MAX_LINES = 30
# W1 (S-8e review): identifiers that can never be a Rust item name, or that are
# too common a local binding to stand for one reference declaration. A decl on
# this list is RED whatever else holds (review attack A-1 used `priv`).
ZIG_KEYWORDS = frozenset("""
addrspace align allowzero and anyframe anytype asm async await break callconv catch
comptime const continue defer else enum errdefer error export extern fn for if inline
linksection noalias noinline nosuspend opaque or orelse packed pub resume return
struct suspend switch test threadlocal try union unreachable usingnamespace var
volatile while
""".split())
RUST_KEYWORDS = frozenset("""
as async await break const continue crate dyn else enum extern false fn for if impl
in let loop match mod move mut pub ref return self Self static struct super trait
true type unsafe use where while abstract become box do final macro override priv
typeof unsized virtual yield try
""".split())
LOCAL_NAMES = frozenset("""
self alloc allocator arena priv obj it value result err buf len i j n x y v s ptr
data list map key item entry node args opts options config cfg state ctx context
gpa app surface window widget parent child
""".split())
DECL_DENYLIST = ZIG_KEYWORDS | RUST_KEYWORDS | LOCAL_NAMES


class LedgerError(Exception):
    """The ledger file is malformed (BLOCKED, exit 2)."""


def ledger_path(root):
    return os.path.join(root, ".port", "parity-closure", "COMMENT-NOT-CARRIED.tsv")


def load(path):
    """(rows, cap). A missing file is (None, None): no ledger at all."""
    if not os.path.exists(path):
        return None, None
    rows, cap = [], None
    with open(path, encoding="utf-8", newline="") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            m = CAP_RE.match(line)
            if m:
                cap = int(m.group(1))
                continue
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if parts[0] == "oracle_file":
                continue
            if len(parts) != 7:
                raise LedgerError(f"{path}:{lineno}: expected 7 tab-separated columns, got {len(parts)}")
            try:
                ln = int(parts[1])
            except ValueError:
                raise LedgerError(f"{path}:{lineno}: line {parts[1]!r} is not an integer")
            rows.append({
                "file": parts[0].strip(), "line": ln, "class": parts[2].strip(),
                "decl": parts[3].strip(), "evidence": parts[4].strip(),
                "text": parts[5], "reason": parts[6].strip(), "lineno": lineno,
            })
    return rows, cap


# ---------------------------------------------------------------------------
# Oracle side
# ---------------------------------------------------------------------------
def oracle_reader_at_pin(G):
    """rel -> list of lines of `src/<rel>` at the pinned commit, or raise
    LedgerError (BLOCKED: an unreadable oracle is never a pass)."""
    pin = G.oracle_pin()
    if not pin:
        raise LedgerError("ORACLE_PIN missing or malformed")
    oracle = G.oracle_root()
    if not os.path.isdir(oracle):
        raise LedgerError(f"the oracle checkout {oracle} does not exist")
    cache = {}

    def read(rel):
        if rel not in cache:
            try:
                r = G.oracle_git(oracle, "show", f"{pin}:src/{rel}", text=True)
            except OSError as e:
                raise LedgerError(f"cannot run git on the oracle: {e}")
            cache[rel] = r.stdout.split("\n") if r.returncode == 0 else None
        return cache[rel]
    return read


def _zig_code(line, CC):
    """The code part of a Zig line (comment removed), or '' for a comment-only /
    blank line. Multi-line string lines (`\\\\...`) are code."""
    idx = CC.find_comment_start(line)
    return (line if idx is None else line[:idx]).rstrip()


def _strip_zig_strings(code):
    if code.lstrip().startswith("\\\\"):
        return ""
    code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)
    return re.sub(r"'(?:\\.|[^'\\])+'", "''", code)


def decl_lines(lines, decl):
    """Lines where the oracle file DECLARES `decl`: `fn|const|var <decl>`, a
    container field `<decl>: T`, or an enum member `<decl>,` / `@"<decl>",`."""
    e = re.escape(decl)
    pats = [re.compile(r"\b(?:fn|const|var)\s+" + e + r"\b"),
            re.compile(r"^\s*(?:pub\s+)?" + e + r"\s*:\s*[^=\s]"),
            re.compile(r'^\s*(?:' + e + r'|@"' + e + r'")\s*(?:,|=[^=>])')]
    return [i for i, ln in enumerate(lines, 1)
            if any(p.search(ln.split("//", 1)[0]) for p in pats)]


def brace_span_end(lines, d, CC):
    """Last line of the brace-balanced body opened at/after decl line `d`
    (1-based), or `d` when the declaration has no body before its `;`."""
    depth, opened = 0, False
    for i in range(d, len(lines) + 1):
        code = _strip_zig_strings(_zig_code(lines[i - 1], CC))
        for ch in code:
            if ch == "{":
                depth += 1
                opened = True
            elif ch == "}":
                depth -= 1
                if opened and depth == 0:
                    return i
            elif ch == ";" and not opened:
                return i
    return len(lines)


def attached(lines, L, decl, CC):
    """Is the comment on line L attached to reference code named by `decl`?

    Either L lies inside a brace-balanced block opened on a code line that names
    `decl` (its declaration `const DllMain = ... struct {`, or a switch arm
    `adw.MessageDialog => {`), or `decl` appears in code on L itself or within
    the next ATTACH_CODE_LINES code lines (the doc block above a declaration,
    or a comment above the statement it explains)."""
    word = re.compile(r"\b" + re.escape(decl) + r"\b")
    for d, ln in enumerate(lines, 1):
        if d > L:
            break
        code = _zig_code(ln, CC)
        if word.search(code) and "{" in _strip_zig_strings(code):
            if L <= brace_span_end(lines, d, CC):
                return True
    if word.search(_zig_code(lines[L - 1], CC)):
        return True
    i, seen = L, 0
    while i < len(lines) and seen < ATTACH_CODE_LINES:
        i += 1
        code = _zig_code(lines[i - 1], CC)
        if not code.strip():
            continue
        seen += 1
        if word.search(code):
            return True
    return False


# ---------------------------------------------------------------------------
# Evidence side
# ---------------------------------------------------------------------------
def evidence_record(root, evidence):
    """(doc, record_text, error). The record is the needle's table row, or its
    section (heading to next heading of the same or higher level), or its
    paragraph."""
    if "#" not in evidence:
        return None, None, f"evidence {evidence!r} is not `<doc>#<needle>`"
    doc, needle = evidence.split("#", 1)
    if doc not in EVIDENCE_DOCS:
        return None, None, f"evidence doc {doc!r} is not an allowed record ({', '.join(EVIDENCE_DOCS)})"
    if not needle.strip():
        return None, None, "evidence needle is empty"
    try:
        lines = open(os.path.join(root, doc), encoding="utf-8").read().split("\n")
    except OSError:
        return None, None, f"evidence doc {doc!r} does not exist"
    hits = [i for i, ln in enumerate(lines) if needle in ln]
    if not hits:
        return None, None, f"evidence needle {needle!r} not found in {doc}"
    if len(hits) > 1:
        return None, None, (f"evidence needle {needle!r} is ambiguous in {doc} "
                            f"({len(hits)} lines) — use a needle that names one record")
    i = hits[0]
    line = lines[i]
    if line.lstrip().startswith("|"):
        rec = [line]
    elif re.match(r"^#+\s", line):
        # W4: the SMALLEST enclosing record — up to the next heading of ANY level.
        end = i + 1
        while end < len(lines) and not re.match(r"^#+\s", lines[end]):
            end += 1
        rec = lines[i:end]
    else:
        s = i
        while s > 0 and lines[s - 1].strip():
            s -= 1
        e = i
        while e + 1 < len(lines) and lines[e + 1].strip():
            e += 1
        rec = lines[s:e + 1]
    while rec and not rec[-1].strip():
        rec.pop()
    if len(rec) > RECORD_MAX_LINES:
        return None, None, (f"evidence record in {doc} is {len(rec)} lines (> "
                            f"{RECORD_MAX_LINES}); cite a table row, a paragraph or a small section")
    return doc, "\n".join(rec), None


STATUS_RE = re.compile(r"~~|\bRESOLVED\b|\bPORTED\b")


def record_is_struck(record):
    """W3: a strike-through or a RESOLVED/PORTED status marker ANYWHERE in the
    record (not only its first line) means the record no longer votes."""
    return bool(STATUS_RE.search(record))


# ---------------------------------------------------------------------------
# Port side
# ---------------------------------------------------------------------------
def workspace_rust_items(root, G, cache={}):
    """All comment- and string-stripped Rust source in the workspace (one blob
    per file), for the no-counterpart absence check."""
    key = root
    if key not in cache:
        blobs = []
        for top in ("crates", "crates-sys", "xtask"):
            base = os.path.join(root, top)
            for dirpath, dirnames, files in os.walk(base):
                dirnames[:] = [d for d in dirnames if d != "target"]
                for fn in files:
                    if fn.endswith(".rs"):
                        p = os.path.join(dirpath, fn)
                        try:
                            text = open(p, encoding="utf-8", errors="replace").read()
                        except OSError:
                            continue
                        blobs.append((os.path.relpath(p, root), G._strip_rust(text)[0],
                                      _citing_comment_lines(text)))
        cache[key] = blobs
    return cache[key]


def _citing_comment_lines(text):
    """The comment lines that can CITE a reference name as the thing a Rust item
    ports: `///` item docs (e.g. `BASELINE`'s "`adw_version.zig:15`
    `comptime_version`") and any comment line carrying a `PORTS:` marker."""
    out = []
    for ln in text.split("\n"):
        t = ln.lstrip()
        if t.startswith("///") or ("//" in ln and "PORTS:" in ln.split("//", 1)[1]):
            out.append(ln)
    return out


def _blob(t):
    """(rel, stripped code, citing comment lines) from a 2- or 3-tuple."""
    return t[0], t[1], (t[2] if len(t) > 2 else [])


def _ident_cands(name, G):
    cands = {c for c in G.name_candidates(name) if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", c)}
    cands.add(name.lstrip("_") or name)
    return sorted(cands)


def rust_member_counterpart(container, member, blobs, G):
    """First (file, name) where an item named like `member` is declared inside an
    `impl` block whose header names `container` (any G1 name form), else None.
    For a qualified decl `Container.member`: a member name like `getMut` is too
    generic to be unique workspace-wide (`ScreenMap::get_mut` exists), so the
    absence is proven per containing type instead."""
    ctn = [re.compile(r"\b" + re.escape(c) + r"\b") for c in _ident_cands(container, G)]
    mem = [re.compile(r"\b" + RUST_ITEM_KW + r"\s+" + re.escape(c) + r"\b")
           for c in _ident_cands(member, G)]
    impl_re = re.compile(r"\bimpl\b([^{;]*)\{")
    for t in blobs:
        rel, blob, _ = _blob(t)
        for m in impl_re.finditer(blob):
            if not any(rx.search(m.group(1)) for rx in ctn):
                continue
            body = blob[m.end() - 1:G._find_balanced_block(blob, m.end() - 1)]
            for rx in mem:
                hit = rx.search(body)
                if hit:
                    return rel, f"impl {m.group(1).strip()} {{ {hit.group(0)} }}"
    return None


def rust_counterpart(decl, blobs, G):
    """First (file, what) proving `decl` HAS a Rust counterpart, else None:
      * a Rust ITEM (fn/const/static/struct/enum/type/mod/trait/union) or a
        FIELD (`name: T`) named by any G1 name candidate of `decl`; or
      * a RENAMED counterpart (W1, S-8e review): a `///` item doc or a `PORTS:`
        comment anywhere in the workspace that names `decl` itself — the Rust
        item it documents stands for the reference declaration
        (`BASELINE`'s doc cites `comptime_version`).
    Uses are not counted (`getMut`'s snake form `get_mut` is called all over
    the workspace on std types, which says nothing about the reference fn)."""
    alt = "|".join(re.escape(c) for c in _ident_cands(decl, G))
    item = re.compile(r"\b" + RUST_ITEM_KW + r"\s+(" + alt + r")\b")
    field = re.compile(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(" + alt + r")[ \t]*:(?!:)")
    cite = re.compile(r"\b" + re.escape(decl) + r"\b")
    for t in blobs:
        rel, blob, docs = _blob(t)
        m = item.search(blob)
        if m:
            return rel, m.group(1)
        m = field.search(blob)
        if m:
            return rel, f"field {m.group(1)}"
        for ln in docs:
            if cite.search(ln):
                return rel, f"doc/PORTS citation: {ln.strip()[:80]}"
    return None


# ---------------------------------------------------------------------------
# Gag-prose content rule
# ---------------------------------------------------------------------------
TECHNICAL_RES = [
    (re.compile(r"[0-9]"), "a digit"),
    (re.compile(r"`"), "a backtick"),
    (re.compile(r"@"), "`@`"),
    (re.compile(r"::"), "`::`"),
    (re.compile(r"\(\)"), "`()`"),
    (re.compile(r"\b\w+_\w+\b"), "a snake_case identifier"),
    (re.compile(r"\b[a-z]+[A-Z]\w*"), "a camelCase identifier"),
    (re.compile(r"\b\w+\.\w+"), "a dotted identifier"),
]
ALL_CAPS_RE = re.compile(r"\b[A-Z][A-Z0-9_]+\b")
# ALL-CAPS words that are prose, not identifiers: an annotation marker and two
# proper nouns. Anything else in capitals (an errno, a C macro, a constant) is
# technical and disqualifies the row. Growing this list is a gate change (R5).
ALL_CAPS_PROSE = frozenset({"NOTE", "XDG", "GNOME"})


# W2 (S-8e review): plain-English technical words. A gag line may not use one
# of these, nor any word that is also an identifier (or an identifier part) of
# the oracle code around it — unless the row proves the line is MIXED and its
# fact is carried (see `mixed_unproven`). Growing or shrinking this list is a
# gate change (R5).
TECH_TERMS = frozenset("""
address alloc allocate allocated allocation allocator arena argument array atomic
buffer byte bytes cache call callback callbacks client code compile compiler config
connection crash data deadlock decode encode error errors event events field file
files flag flags function handle handler header heap index input key keys layout
leak length lock memory message method mutex null object output overflow packet
parse parser path pixel pixels pointer process protocol queue race request requests
response return runtime server session signal signals size socket stack state string
struct surface symbol symbols syscall thread threads timeout token type value
version vtable window
""".split())
CODE_WINDOW = 15
MIXED_RE = re.compile(r'^MIXED-CARRIED:\s*"([^"]+)"')


def _words_of(text):
    return {w for w in re.findall(r"[a-z]+", text.lower()) if len(w) >= 4}


def code_words(lines, L, CC):
    """Identifier words (whole identifiers and their camel/snake parts, >= 4
    letters, lower-cased) of the oracle code within CODE_WINDOW lines of L."""
    out = set()
    for ln in lines[max(0, L - 1 - CODE_WINDOW):L + CODE_WINDOW]:
        for ident in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", _zig_code(ln, CC)):
            parts = re.split(r"_|(?<=[a-z0-9])(?=[A-Z])", ident)
            for w in [ident] + parts:
                if len(w) >= 4:
                    out.add(w.lower())
    return out


def technical_words(text, near_code):
    vocab = TECH_TERMS | set(near_code)
    out = []
    for w in sorted(_words_of(text)):
        if w in vocab or (w.endswith("s") and w[:-1] in vocab):
            out.append(w)
    return out


def gag_violations(text, reason, near_code=frozenset()):
    out = [f"gag-prose text contains {what}" for rx, what in TECHNICAL_RES if rx.search(text)]
    caps = [w for w in ALL_CAPS_RE.findall(text) if w not in ALL_CAPS_PROSE]
    if caps:
        out.append(f"gag-prose text contains ALL-CAPS token(s) {caps}")
    tech = technical_words(text, near_code)
    if tech:
        m = MIXED_RE.match(reason)
        if not m:
            out.append(f"gag-prose text uses technical word(s) {tech} (a technical term, or "
                       f"an identifier of the surrounding oracle code); a mixed line needs a "
                       f'`MIXED-CARRIED: "<the port comment carrying its fact>"` reason')
        else:
            carried = _words_of(m.group(1))
            missing = [w for w in tech if w not in carried and w[:-1] not in carried]
            if missing:
                out.append(f"MIXED-CARRIED phrase does not carry the technical word(s) {missing}")
    if len(text.split()) > GAG_MAX_WORDS and not reason.startswith("JUSTIFIED-LONG:"):
        out.append(f"gag-prose text is longer than {GAG_MAX_WORDS} words without a "
                   f"`JUSTIFIED-LONG:` reason")
    return out


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------
def validate(rows, cap, *, root, G, CC, strip_marker, norm, idmap, distinctive,
             read_oracle, blobs):
    """Check every row's oracle, class and evidence rules (1, 3, 4 of the module
    doc) and the cap. Returns the error strings. Sets on each row `ok` (it
    proved itself; only such rows may ever excuse a comment), `crate` and
    `norm` (the normalized oracle comment, as G3b compares it).

    Rule 2 (stale: the port now carries it) needs the crate's port pool, so
    the caller checks it per examined crate — see `stale_rows`."""
    errs = []
    if cap is None:
        errs.append("BLOCKED: no `#!not-carried-cap entries=N` directive")
    elif cap != len(rows):
        errs.append(f"cap mismatch: directive says {cap}, ledger has {len(rows)} row(s) "
                    f"(two-way ratchet)")
    f2c = G.file_to_crate()
    seen = set()
    for r in rows:
        where = f"COMMENT-NOT-CARRIED.tsv:{r['lineno']} ({r['file']}:{r['line']})"
        r["ok"], r["crate"], r["norm"] = True, f2c.get(r["file"]), None

        def bad(msg, r=r, where=where):
            r["ok"] = False
            errs.append(f"{where}: {msg}")

        key = (r["file"], r["line"])
        if key in seen:
            bad("duplicate row")
        seen.add(key)
        if r["class"] not in CLASSES:
            bad(f"unknown class {r['class']!r}")
            continue
        if len(r["reason"].split()) < REASON_MIN_WORDS:
            bad(f"reason missing or shorter than {REASON_MIN_WORDS} words")
        if r["crate"] is None:
            bad("oracle file is not in the crate map")
            continue
        lines = read_oracle(r["file"])
        if lines is None:
            bad("oracle file does not exist at the pin")
            continue
        if not (1 <= r["line"] <= len(lines)):
            bad("line out of range at the pin")
            continue
        src = lines[r["line"] - 1]
        idx = CC.find_comment_start(src)
        body = None if idx is None else strip_marker(src[idx:])
        if body != r["text"]:
            bad(f"STALE — the oracle line at the pin is {src.strip()!r}, not the declared text")
            continue
        if not distinctive(src[idx:]):
            bad("the comment is not distinctive (G3b never counts it)")
            continue
        r["norm"] = norm(src[idx:], idmap)
        base = os.path.basename(r["file"])
        if r["class"] == "no-counterpart":
            m = re.fullmatch(r"(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Za-z_][A-Za-z0-9_]*)", r["decl"])
            if not m:
                bad(f"decl {r['decl']!r} is not an identifier or `Container.member`")
                continue
            container, d = m.group(1), m.group(2)
            if d in DECL_DENYLIST or (container or "") in DECL_DENYLIST:
                bad(f"decl {r['decl']!r} is a keyword or a common local name — it can "
                    f"stand for no single reference declaration")
                continue
            if not decl_lines(lines, d):
                bad(f"decl {d!r} is not DECLARED in {r['file']} (fn/const/var, field or "
                    f"enum member)")
            if not attached(lines, r["line"], d, CC):
                bad(f"the comment is not attached to {d!r} in the oracle")
            if container:
                # A qualified member: its (generic) name may be an item elsewhere;
                # only a doc/PORTS citation of it or an item inside `impl Container`
                # counts.
                hit = rust_counterpart(d, [(_blob(t)[0], "", _blob(t)[2]) for t in blobs], G) \
                    or rust_member_counterpart(container, d, blobs, G)
            else:
                hit = rust_counterpart(d, blobs, G)
            if hit:
                bad(f"{r['decl']!r} HAS a Rust counterpart (`{hit[1]}` in {hit[0]}) — "
                    f"carry the comment there")
            doc, rec, e = evidence_record(root, r["evidence"])
            named = [x for x in (container, d) if x]
            if e:
                bad(e)
            elif not all(re.search(r"\b" + re.escape(x) + r"\b", rec) for x in named):
                bad(f"evidence record in {doc} does not name {' and '.join(map(repr, named))}")
            elif base not in rec:
                bad(f"evidence record in {doc} does not name the oracle file {base!r}")
            elif record_is_struck(rec):
                bad(f"evidence record in {doc} is struck/RESOLVED/PORTED — it cannot vote")
        else:  # gag-prose
            if r["decl"] != "-":
                bad("gag-prose rows carry decl `-`")
            doc, rec, e = evidence_record(root, r["evidence"])
            site = f"{r['file']}:{r['line']}"
            if e:
                bad(e)
            elif GAG_RULE not in rec:
                bad(f"evidence record in {doc} does not name {GAG_RULE}")
            elif site not in rec:
                bad(f"evidence record in {doc} does not list this line ({site}) — each "
                    f"gag line needs its own entry in the record")
            elif record_is_struck(rec):
                bad(f"evidence record in {doc} is struck/RESOLVED/PORTED — it cannot vote")
            for v in gag_violations(r["text"], r["reason"], code_words(lines, r["line"], CC)):
                bad(v)
    return errs


def mixed_unproven(rows, norm, idmap, port_norm):
    """Gag rows whose `MIXED-CARRIED: "<phrase>"` reason names a phrase the
    crate's port comments do NOT contain (normalized substring). Each is an
    error: the line's fact must actually be carried next to the stripped joke."""
    blob = " \n ".join(port_norm)
    out = []
    for r in rows:
        m = MIXED_RE.match(r.get("reason", ""))
        if r.get("class") == "gag-prose" and m and norm("// " + m.group(1), idmap) not in blob:
            out.append(r)
    return out


def stale_rows(rows, classify, port_norm):
    """Rows (already `ok`) whose comment the port pool now carries: neither
    MISSING nor REVIEW. Each is an error — delete the row (rule 2)."""
    texts = sorted({r["norm"] for r in rows})
    if not texts:
        return []
    missing, review = classify(texts, port_norm)
    still = set(missing) | {t for t, _ in review}
    return [r for r in rows if r["norm"] not in still]


def occurrences(crate_ref_files, norm, idmap, distinctive, CC):
    """normtext -> set of (rel, line) of every distinctive comment in the given
    (rel, lines) reference files."""
    occ = {}
    for rel, lines in crate_ref_files:
        for i, ln in enumerate(lines, 1):
            idx = CC.find_comment_start(ln)
            if idx is None or not ln[idx:].strip():
                continue
            if distinctive(ln[idx:]):
                occ.setdefault(norm(ln[idx:], idmap), set()).add((rel, i))
    return occ


def apply(rows, occ, missing, review):
    """Split MISSING/REVIEW into (missing', review', applied). Only `ok` rows
    count, and a normalized text is removed only when EVERY one of its
    occurrences in the crate's reference files is covered by an `ok` row."""
    rows = [r for r in rows if r.get("ok")]
    by_text = {}
    covered = {(r["file"], r["line"]) for r in rows}
    for r in rows:
        by_text.setdefault(r["norm"], []).append(r)
    applied = []

    def excused(t):
        sites = occ.get(t)
        return bool(sites) and t in by_text and sites <= covered

    m2 = []
    for t in missing:
        if excused(t):
            applied.extend((r, "MISSING") for r in by_text[t])
        else:
            m2.append(t)
    r2 = []
    for t, ratio in review:
        if excused(t):
            applied.extend((r, "REVIEW") for r in by_text[t])
        else:
            r2.append((t, ratio))
    return m2, r2, applied
