"""Extract the JSON-literal data a mockup HTML embeds in its <script>, so a widget builder can read it from a committed input file.

Usage: python3 extract_embed.py <mockup.html> <out.json>

For the verification matrix (mockup 10) the literals are the constants PINS, HOSTS, TOOLS,
NA, the run list R, the per-commit notes and the wall-time rows of durChart. A literal is
found by name, cut out with a string-aware bracket matcher and parsed with json.loads
(unquoted integer object keys are quoted first). Nothing is evaluated.
"""
import json
import re
import sys
from pathlib import Path

# CONSTANTS maps the output key to the declaration that precedes its literal in the mockup script.
CONSTANTS = {
    "pins": "const PINS=",
    "hosts": "const HOSTS=",
    "tools": "const TOOLS=",
    "na": "const NA=",
    "runs": "const R=",
    "notes": "const notes=",
    "durations": "function durChart(){const rows=",
}


def literal_after(text: str, marker: str) -> str:
    """Return the bracketed literal that starts right after marker, matching [] and {} outside strings."""
    start = text.index(marker) + len(marker)
    opener = text[start]
    if opener not in "[{":
        raise ValueError(f"no literal after {marker!r}")
    depth, i, quote = 0, start, None
    while i < len(text):
        c = text[i]
        if quote:
            if c == "\\":
                i += 1
            elif c == quote:
                quote = None
        elif c in "\"'":
            quote = c
        elif c in "[{":
            depth += 1
        elif c in "]}":
            depth -= 1
            if depth == 0:
                return text[start : i + 1]
        i += 1
    raise ValueError(f"unterminated literal after {marker!r}")


def parse_literal(src: str):
    """Parse a JS literal made only of arrays, objects, strings, numbers and null (bare integer keys allowed).

    Integer keys are quoted only outside string literals, so text such as "a, 3: b" is never touched.
    """
    parts = re.split(r'("(?:[^"\\]|\\.)*")', src)
    fixed = [p if i % 2 else re.sub(r"([{,])\s*(\d+)\s*:", r'\1"\2":', p) for i, p in enumerate(parts)]
    return json.loads("".join(fixed))


def extract(html: str) -> dict:
    """Return the embedded data of the mockup html as {key: parsed literal} for every entry of CONSTANTS."""
    script = html[html.index("<script>") :]
    return {key: parse_literal(literal_after(script, marker)) for key, marker in CONSTANTS.items()}


def main(argv: list) -> None:
    """Command line entry: extract argv[1] (mockup HTML) into argv[2] (JSON)."""
    data = extract(Path(argv[1]).read_text())
    Path(argv[2]).write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main(sys.argv)
