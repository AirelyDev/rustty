#!/usr/bin/env python3
"""
Extract EVERY inline Zig `test {...}` block from all .zig files under a src tree.

This is a real tokenizer for the subset of Zig lexical structure needed to find
test-block boundaries and count assertions correctly:

  - line comments:        // ... (to EOL)        -> NOT counted, not scanned
  - string literals:      "..."  with \\" escapes  -> braces inside ignored
  - char literals:        '.'    with escapes      -> braces inside ignored
  - multiline strings:    \\... (to EOL)            -> braces inside ignored
  - nested braces:        balanced { } tracked depth-correctly

A `test` declaration is recognized only at token position (the identifier `test`
as a keyword, i.e. preceded by non-identifier char / start, followed by a name
or `{`). The block body is from the opening `{` of the test to its matching `}`.

Assertions are counted by scanning the *code* of each test body (comments and
string/char literal contents are excluded) for known assertion call kinds.

Outputs:
  inventory/tests.jsonl          one JSON object per test block
  inventory/tests-summary.json   aggregate counts + breakdown + per-file

Usage:
  extract_tests.py <src_root> <out_dir>
"""

import sys, os, json, re

# Assertion kinds. Each entry: (kind_name, compiled regex matched against the
# code-only stream of a test body). Patterns are anchored at a token boundary.
# Order matters only for classification; every assertion is classified to exactly
# one kind via first-match priority below.

# We detect assertion *call sites*: an identifier path ending in one of these
# names, immediately followed by `(`. The leading boundary (?<![\w.]) ... we
# instead match the full dotted path then check the tail name.

# Specific testing.expect* variants (longest first so expectEqualStrings is not
# eaten by expectEqual, etc.)
TESTING_EXPECT_VARIANTS = [
    "expectEqualStrings",
    "expectEqualSentinel",
    "expectEqualSlices",
    "expectEqualDeep",
    "expectApproxEqAbs",
    "expectApproxEqRel",
    "expectStringStartsWith",
    "expectStringEndsWith",
    "expectFmt",
    "expectError",
    "expectEqual",
    "expect",
]

# Build a single regex that captures a dotted identifier path ending in `(`.
# Example matches: std.testing.expectEqual(  testing.expect(  std.debug.assert(
#                  try testing.foo(  t.expectEqual(
CALL_RE = re.compile(r'(?<![\w.])((?:[A-Za-z_]\w*\.)*[A-Za-z_]\w*)\s*\(')

DEBUG_ASSERT_RE = re.compile(r'(?:^|[^\w.])(?:std\.)?debug\.assert\s*\(')

def classify_assertion(path):
    """Given a dotted call path (e.g. 'std.testing.expectEqual'), return the
    assertion kind, or None if not an assertion."""
    parts = path.split('.')
    name = parts[-1]

    # std.debug.assert  /  debug.assert
    if name == "assert" and len(parts) >= 2 and parts[-2] == "debug":
        return "std.debug.assert"

    # testing.* family: path contains a 'testing' component, OR the call comes
    # via a local alias to std.testing. We can only see the path here; the
    # `try testing.*` requirement and alias handling are applied at scan time.
    if "testing" in parts[:-1]:
        for v in TESTING_EXPECT_VARIANTS:
            if name == v:
                return f"testing.{v}"
        # any other testing.* call (e.g. testing.allocator usage is not a call
        # we count; but `testing.expectEqualSentinel`-like or future ones):
        return "testing.other"

    return None


def strip_code_stream(body):
    """Return the test body with comments and string/char literal *contents*
    removed (replaced by spaces to preserve offsets minimally), so assertion
    scanning never matches inside strings/comments. Multiline strings (\\...)
    are removed to EOL."""
    out = []
    i = 0
    n = len(body)
    while i < n:
        c = body[i]
        # line comment OR multiline string literal: both start with chars we
        # must consume to end of line. // and \\
        if c == '/' and i + 1 < n and body[i+1] == '/':
            # consume to newline
            while i < n and body[i] != '\n':
                i += 1
            continue
        if c == '\\' and i + 1 < n and body[i+1] == '\\':
            # multiline string literal line: consume to newline
            while i < n and body[i] != '\n':
                i += 1
            continue
        if c == '"':
            out.append(' ')
            i += 1
            while i < n:
                ch = body[i]
                if ch == '\\' and i + 1 < n:
                    i += 2
                    continue
                if ch == '"':
                    i += 1
                    break
                if ch == '\n':
                    break
                i += 1
            continue
        if c == "'":
            # char literal
            j = i + 1
            consumed = False
            k = j
            while k < n:
                ch = body[k]
                if ch == '\\' and k + 1 < n:
                    k += 2
                    continue
                if ch == "'":
                    k += 1
                    consumed = True
                    break
                if ch == '\n':
                    break
                k += 1
            if consumed:
                out.append(' ')
                i = k
                continue
            else:
                out.append(c)
                i += 1
                continue
        out.append(c)
        i += 1
    return ''.join(out)


class Lexer:
    """Walks source char-by-char respecting comments/strings/chars/multiline
    strings, yielding only the positions of structurally significant braces and
    the `test` keyword occurrences. Used to locate test blocks."""
    def __init__(self, text):
        self.text = text
        self.n = len(text)

    def find_test_blocks(self):
        """Yield (decl_start_idx, name_or_None, brace_open_idx, brace_close_idx)
        for each top-level/any-level `test` declaration found at token boundary."""
        text = self.text
        n = self.n
        i = 0
        results = []
        while i < n:
            c = text[i]
            # skip line comment
            if c == '/' and i + 1 < n and text[i+1] == '/':
                while i < n and text[i] != '\n':
                    i += 1
                continue
            # skip multiline string literal line
            if c == '\\' and i + 1 < n and text[i+1] == '\\':
                while i < n and text[i] != '\n':
                    i += 1
                continue
            # skip string literal
            if c == '"':
                i += 1
                while i < n:
                    ch = text[i]
                    if ch == '\\' and i + 1 < n:
                        i += 2
                        continue
                    if ch == '"' or ch == '\n':
                        i += 1
                        break
                    i += 1
                continue
            # skip char literal
            if c == "'":
                j = i + 1
                k = j
                consumed = False
                while k < n:
                    ch = text[k]
                    if ch == '\\' and k + 1 < n:
                        k += 2
                        continue
                    if ch == "'":
                        k += 1
                        consumed = True
                        break
                    if ch == '\n':
                        break
                    k += 1
                if consumed:
                    i = k
                    continue
                else:
                    i += 1
                    continue
            # identifier? check for `test` keyword at token boundary
            if c == 't' and text.startswith('test', i):
                before_ok = (i == 0) or (not (text[i-1].isalnum() or text[i-1] == '_'))
                after = text[i+4] if i + 4 < n else ''
                after_ok = not (after.isalnum() or after == '_')
                if before_ok and after_ok:
                    decl_start = i
                    # advance past 'test'
                    j = i + 4
                    # parse the test declaration: optional name then '{'
                    name, brace_idx = self._parse_test_header(j)
                    if brace_idx is not None:
                        close_idx = self._match_brace(brace_idx)
                        if close_idx is not None:
                            results.append((decl_start, name, brace_idx, close_idx))
                            i = close_idx + 1
                            continue
                    # not a real test decl (e.g. `test` used elsewhere); skip kw
                    i = j
                    continue
            i += 1
        return results

    def _skip_ws_and_comments(self, i):
        text = self.text
        n = self.n
        while i < n:
            c = text[i]
            if c in ' \t\r\n':
                i += 1
                continue
            if c == '/' and i + 1 < n and text[i+1] == '/':
                while i < n and text[i] != '\n':
                    i += 1
                continue
            break
        return i

    def _parse_test_header(self, i):
        """At i (just after 'test'), parse optional name (string or identifier)
        then expect '{'. Returns (name_or_None, brace_open_idx_or_None)."""
        text = self.text
        n = self.n
        i = self._skip_ws_and_comments(i)
        if i >= n:
            return (None, None)
        c = text[i]
        if c == '{':
            return (None, i)  # anonymous
        # named with string literal: "..." (possibly @"..." too)
        if c == '"':
            j = i + 1
            buf = []
            while j < n:
                ch = text[j]
                if ch == '\\' and j + 1 < n:
                    buf.append(text[j:j+2]); j += 2; continue
                if ch == '"':
                    j += 1
                    break
                if ch == '\n':
                    break
                buf.append(ch); j += 1
            name = ''.join(buf)
            k = self._skip_ws_and_comments(j)
            if k < n and text[k] == '{':
                return (name, k)
            return (None, None)
        # named with identifier:  test detectShell {   (Zig: test <decl ref>)
        if c.isalpha() or c == '_' or c == '@':
            j = i
            if c == '@' and j + 1 < n and text[j+1] == '"':
                # @"..." identifier
                j += 2
                buf = ['@"']
                while j < n:
                    ch = text[j]
                    if ch == '\\' and j + 1 < n:
                        buf.append(text[j:j+2]); j += 2; continue
                    if ch == '"':
                        buf.append('"'); j += 1; break
                    buf.append(ch); j += 1
                name = ''.join(buf)
            else:
                start = j
                while j < n and (text[j].isalnum() or text[j] == '_'):
                    j += 1
                name = text[start:j]
            k = self._skip_ws_and_comments(j)
            if k < n and text[k] == '{':
                return (name, k)
            return (None, None)
        return (None, None)

    def _match_brace(self, open_idx):
        """Given index of an opening '{', return index of its matching '}' with
        full lexical awareness (strings/chars/comments/multiline)."""
        text = self.text
        n = self.n
        depth = 0
        i = open_idx
        while i < n:
            c = text[i]
            if c == '/' and i + 1 < n and text[i+1] == '/':
                while i < n and text[i] != '\n':
                    i += 1
                continue
            if c == '\\' and i + 1 < n and text[i+1] == '\\':
                while i < n and text[i] != '\n':
                    i += 1
                continue
            if c == '"':
                i += 1
                while i < n:
                    ch = text[i]
                    if ch == '\\' and i + 1 < n:
                        i += 2; continue
                    if ch == '"' or ch == '\n':
                        i += 1; break
                    i += 1
                continue
            if c == "'":
                j = i + 1
                k = j
                consumed = False
                while k < n:
                    ch = text[k]
                    if ch == '\\' and k + 1 < n:
                        k += 2; continue
                    if ch == "'":
                        k += 1; consumed = True; break
                    if ch == '\n':
                        break
                    k += 1
                if consumed:
                    i = k; continue
                else:
                    i += 1; continue
            if c == '{':
                depth += 1
            elif c == '}':
                depth -= 1
                if depth == 0:
                    return i
            i += 1
        return None


def detect_testing_aliases(text):
    """Find local aliases for std.testing, e.g.
       const testing = std.testing;
       const expectEqual = std.testing.expectEqual;
    Returns (module_aliases:set, fn_aliases:dict alias->kind)."""
    module_aliases = set()
    fn_aliases = {}
    # const NAME = std.testing;
    for m in re.finditer(r'\bconst\s+(\w+)\s*=\s*std\.testing\s*;', text):
        module_aliases.add(m.group(1))
    # const NAME = <something>.testing;  (rare) -> treat as module alias too
    for m in re.finditer(r'\bconst\s+(\w+)\s*=\s*testing\s*;', text):
        module_aliases.add(m.group(1))
    # const NAME = std.testing.FUNC;  or = testing.FUNC;
    for m in re.finditer(r'\bconst\s+(\w+)\s*=\s*(?:std\.)?testing\.(\w+)\s*;', text):
        alias, fn = m.group(1), m.group(2)
        if fn in TESTING_EXPECT_VARIANTS or fn.startswith("expect"):
            fn_aliases[alias] = f"testing.{fn}"
    return module_aliases, fn_aliases


def count_assertions(body, module_aliases, fn_aliases):
    """Scan a test body's code stream and count assertion calls by kind.
    Returns (total, breakdown_dict, list_of_kinds)."""
    code = strip_code_stream(body)
    breakdown = {}
    kinds = []

    for m in CALL_RE.finditer(code):
        path = m.group(1)
        parts = path.split('.')
        name = parts[-1]
        kind = None

        # std.debug.assert / debug.assert
        if name == "assert" and len(parts) >= 2 and parts[-2] == "debug":
            kind = "std.debug.assert"
        # direct testing path (contains 'testing' before the name)
        elif "testing" in parts[:-1]:
            matched = False
            for v in TESTING_EXPECT_VARIANTS:
                if name == v:
                    kind = f"testing.{v}"; matched = True; break
            if not matched:
                # other testing.* call that is an expect-style?
                if name.startswith("expect"):
                    kind = f"testing.{name}"
                else:
                    kind = None
        # module-aliased: alias.expectEqual(  where alias == std.testing
        elif len(parts) >= 2 and parts[0] in module_aliases:
            for v in TESTING_EXPECT_VARIANTS:
                if name == v:
                    kind = f"testing.{v}"; break
            if kind is None and name.startswith("expect"):
                kind = f"testing.{name}"
        # function-aliased: const expectEqual = std.testing.expectEqual; expectEqual(
        elif path in fn_aliases:
            kind = fn_aliases[path]

        if kind is not None:
            breakdown[kind] = breakdown.get(kind, 0) + 1
            kinds.append(kind)

    total = sum(breakdown.values())
    return total, breakdown, kinds


def line_of(text, idx):
    return text.count('\n', 0, idx) + 1


def main():
    if len(sys.argv) != 3:
        print("usage: extract_tests.py <src_root> <out_dir>", file=sys.stderr)
        sys.exit(2)
    src_root = os.path.abspath(sys.argv[1])
    out_dir = os.path.abspath(sys.argv[2])
    inv_dir = out_dir
    os.makedirs(inv_dir, exist_ok=True)

    zig_files = []
    for dirpath, _, filenames in os.walk(src_root):
        for fn in filenames:
            if fn.endswith('.zig'):
                zig_files.append(os.path.join(dirpath, fn))
    zig_files.sort()

    tests = []
    total_assert_breakdown = {}
    per_file = {}  # rel -> {tests, assertions}
    named = 0
    anon = 0

    for path in zig_files:
        rel = os.path.relpath(path, src_root)
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
        module_aliases, fn_aliases = detect_testing_aliases(text)
        lex = Lexer(text)
        blocks = lex.find_test_blocks()
        if not blocks:
            continue
        per_file[rel] = {"tests": 0, "assertions": 0}
        for (decl_start, name, brace_open, brace_close) in blocks:
            body = text[brace_open:brace_close+1]
            start_line = line_of(text, decl_start)
            end_line = line_of(text, brace_close)
            total, breakdown, kinds = count_assertions(body, module_aliases, fn_aliases)
            is_anon = name is None
            if is_anon:
                anon += 1
            else:
                named += 1
            for k, v in breakdown.items():
                total_assert_breakdown[k] = total_assert_breakdown.get(k, 0) + v
            per_file[rel]["tests"] += 1
            per_file[rel]["assertions"] += total
            tests.append({
                "file": rel,
                "name": name if name is not None else "<anonymous>",
                "anonymous": is_anon,
                "start_line": start_line,
                "end_line": end_line,
                "assertion_count": total,
                "assertion_breakdown": breakdown,
                "assertions": kinds,
                "body": body,
            })

    # write jsonl
    jsonl_path = os.path.join(inv_dir, "tests.jsonl")
    with open(jsonl_path, 'w', encoding='utf-8') as f:
        for t in tests:
            f.write(json.dumps(t, ensure_ascii=False) + '\n')

    total_assertions = sum(total_assert_breakdown.values())
    summary = {
        "src_root": src_root,
        "total_test_blocks": len(tests),
        "named": named,
        "anonymous": anon,
        "files_with_tests": len(per_file),
        "total_assertions": total_assertions,
        "assertion_breakdown": dict(sorted(total_assert_breakdown.items())),
        "per_file": dict(sorted(per_file.items())),
    }
    summary_path = os.path.join(inv_dir, "tests-summary.json")
    with open(summary_path, 'w', encoding='utf-8') as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    # console report
    print(f"files scanned: {len(zig_files)}")
    print(f"files with tests: {len(per_file)}")
    print(f"total test blocks: {len(tests)} (named={named}, anonymous={anon})")
    print(f"total assertions: {total_assertions}")
    print("assertion breakdown:")
    for k, v in sorted(total_assert_breakdown.items()):
        print(f"  {k}: {v}")
    print(f"wrote: {jsonl_path}")
    print(f"wrote: {summary_path}")


if __name__ == "__main__":
    main()
