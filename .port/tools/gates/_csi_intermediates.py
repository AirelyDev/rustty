"""The intermediate-byte sequences one CSI dispatch arm distinguishes (P8-W10 T4).

G9 used to key CSI coverage on the final byte only (N523), so two commands that
share a final were one id: DECSTR (`CSI ! p`) was invisible behind DECRQM
(`CSI $ p` / `CSI ? $ p`). The reference stream dispatch tells them apart by
`input.intermediates` (the parser collects the private marker `?`, `>`, `<`,
`=` there too), so a CSI id is now `<intermediates><final>`: `p` alone is not
an id, `!p`, `$p` and `?$p` are.

`arm_intermediates(body, lang)` reads ONE top-level arm body (Zig `stream.zig`
csiDispatch for the parity-surface generator, Rust `csi_dispatch.rs` for the
gate; the same reader on both sides) and returns the sorted set of intermediate
strings it handles. The recognised forms are the ones both trees use:

  E1  `intermediates[0] == 'X'` / `!= 'X'`, optionally followed by
      `and`/`&&` `intermediates[1] == 'Y'`             -> "X" / "XY"
  E2  `intermediates.len == 0` (`.len() == 0`)           -> ""
  E3  an arm of `switch (input.intermediates.len)` /
      `match input.intermediates.len()` whose label holds 0 -> ""
  E4  an arm of `switch (input.intermediates[0])` /
      `match input.intermediates[0]` labelled with char literals -> each "X"
  E5  an arm body that names none of these              -> "" (final alone)

Comments and string literals are removed first, so the Zig source the Rust
port quotes in `//` comments never counts. An arm label inside an E3/E4 switch
that this reader cannot parse raises ValueError (BLOCKED by the callers),
never a silent miss.
"""
import re

_ZIG_CHAR = re.compile(r"'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]+\}|.)|[^'\\])'")
_RUST_CHAR = re.compile(r"b?'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]+\}|.)|[^'\\])'")
_ESC = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", "'": "'", '"': '"'}


def _char_value(lit):
    body = lit[lit.index("'") + 1:-1]
    if not body.startswith("\\"):
        return body
    esc = body[1:]
    if esc.startswith("x"):
        return chr(int(esc[1:], 16))
    if esc.startswith("u{"):
        return chr(int(esc[2:-1], 16))
    if esc in _ESC:
        return _ESC[esc]
    raise ValueError(f"unknown escape in char literal {lit!r}")


def normalize(text, lang):
    """Comments and strings removed; every char literal replaced by `CH<hex>`
    (so braces or quotes inside one cannot desync the brace matcher); Rust's
    `.len()`, `&&`, `||` and `match X {` spelled as Zig's `.len`, `and`, `or`
    and `switch (X) {`."""
    char_re = _ZIG_CHAR if lang == "zig" else _RUST_CHAR
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j == -1 else j
            continue
        if lang == "rust" and text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j == -1 else j + 2
            out.append(" ")
            continue
        if lang == "zig" and text.startswith("\\\\", i):
            j = text.find("\n", i)
            i = n if j == -1 else j
            continue
        if c == '"' or (lang == "rust" and c in "rb" and re.match(r'b?r#*"', text[i:i + 8])
                        and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_"))):
            m = re.match(r'(b?r)(#*)"', text[i:i + 8]) if c != '"' else None
            if m:
                close = '"' + m.group(2)
                j = text.find(close, i + m.end())
                i = n if j == -1 else j + len(close)
            else:
                j = i + 1
                while j < n and text[j] != '"':
                    j += 2 if text[j] == "\\" else 1
                i = j + 1
            out.append('""')
            continue
        if c in "'b":
            m = char_re.match(text, i)
            if m and (c == "'" or text[i + 1:i + 2] == "'") and \
                    (c == "'" or i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
                out.append(" CH%02x " % ord(_char_value(m.group())))
                i = m.end()
                continue
        out.append(c)
        i += 1
    s = "".join(out)
    if lang == "rust":
        s = re.sub(r"\.len\(\)", ".len", s)
        s = s.replace("&&", " and ").replace("||", " or ")
        s = re.sub(r"\bmatch\s+(input\.intermediates(?:\.len|\[\d\]))\s*\{", r"switch (\1) {", s)
    return s


def _block_end(s, open_idx):
    depth = 0
    for k in range(open_idx, len(s)):
        if s[k] == "{":
            depth += 1
        elif s[k] == "}":
            depth -= 1
            if depth == 0:
                return k
    raise ValueError("unbalanced braces in a CSI arm body")


def _switch_labels(s, open_idx, close_idx):
    """Labels (text before `=>` on its line) of the arms at depth 0 of the
    block s[open_idx]..s[close_idx]."""
    labels = []
    depth = 0
    k = open_idx + 1
    while k < close_idx:
        ch = s[k]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif depth == 0 and s.startswith("=>", k):
            # Both trees put each arm label at the start of its own line
            # (zig fmt, rustfmt); a label that is not alone there is
            # unreadable and raises in _parse_label.
            line_start = max(s.rfind("\n", open_idx, k), open_idx)
            labels.append(s[line_start + 1:k].strip())
            k += 2
            continue
        k += 1
    return labels


_LABEL_TOKEN = re.compile(r"^(?:CH([0-9a-f]{2})|(\d+)|else|_)$")


def _parse_label(label):
    toks = [t.strip() for t in re.split(r"[,|]", label) if t.strip()]
    if not toks:
        raise ValueError(f"empty arm label {label!r}")
    vals = []
    for t in toks:
        m = _LABEL_TOKEN.match(t)
        if not m:
            raise ValueError(f"unreadable intermediates arm label {label!r}")
        if m.group(1):
            vals.append(("char", chr(int(m.group(1), 16))))
        elif m.group(2):
            vals.append(("int", int(m.group(2))))
        else:
            vals.append(("else", None))
    return vals


_E1 = re.compile(r"intermediates\[0\]\s*[!=]=\s*CH([0-9a-f]{2})"
                 r"(?:\s*and\s*(?:input\.)?intermediates\[1\]\s*[!=]=\s*CH([0-9a-f]{2}))?")
_E2 = re.compile(r"intermediates\.len\s*==\s*0\b")
_SW = re.compile(r"\bswitch\s*\(\s*input\.intermediates(\.len|\[0\])\s*\)\s*\{")


def arm_intermediates(body, lang):
    """Sorted list of the intermediate strings one CSI arm body handles."""
    s = normalize(body, lang)
    found = set()
    for m in _E1.finditer(s):
        found.add(chr(int(m.group(1), 16)) + (chr(int(m.group(2), 16)) if m.group(2) else ""))
    if _E2.search(s):
        found.add("")
    for m in _SW.finditer(s):
        open_idx = m.end() - 1
        close_idx = _block_end(s, open_idx)
        for label in _switch_labels(s, open_idx, close_idx):
            for kind, v in _parse_label(label):
                if m.group(1) == ".len" and kind == "int" and v == 0:
                    found.add("")
                elif m.group(1) == "[0]" and kind == "char":
                    found.add(v)
                elif m.group(1) == "[0]" and kind == "int":
                    raise ValueError(f"integer label {v} in a switch on intermediates[0]")
    if not found:
        # E5: no discriminator, so the arm handles the final whatever the
        # intermediates (a body that discriminates only in forms this reader
        # does not know would land here too; on the Rust side that reads as
        # missing ids, RED, never a silent pass).
        found.add("")
    return sorted(found)


def csi_id(inter, final):
    """The G9 id of one CSI command: its intermediates then its final."""
    return inter + final
