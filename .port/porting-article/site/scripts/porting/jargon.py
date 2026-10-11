"""Strip internal Rustty port identifiers from text destined for the published article."""
import re

# ID_PATTERN matches the internal id families listed in research/r2-glossary.md, plus the FX<n>
# fix labels seen in the author's prompts.
ID_PATTERN = re.compile(
    r"\b(?:UD\d+|N\d{1,3}|U-\w+|R\d{1,2}[a-z]?|D\d[a-z]?|G\d+[a-z]?(?:-SELF)?|G-[A-Z]+|"
    r"A\d|P\d+-W\d+[a-z]?|W\d+\.T\d+[a-z]?|T\d+[a-z]|FX\d+)\b"
)

# PROCESS_LABELS are the port's planning labels (phases, waves, chunks, batches,
# sub-projects, tasks, stages, tiers, perf rounds) as (pattern, replacement).
# The process word itself is matched case-insensitively; a bare word with no
# number or letter label ("phase out", "batch the writes") is left alone.
PROCESS_LABELS = [
    (r"\b[Pp]hases \d+-\d+\b", ""),
    (r"\b[Pp]hase[- ]?(?:\d+[a-z]?(?:/\d+)?(?:/task-\d+)?|[A-Z])\b", ""),
    (r"\b[Ww]ave[- ]?\d+\b", ""),
    (r"\bGPU [Cc]hunk[- ]?\d+[a-z]?\b", "GPU"),
    (r"\b[Cc]hunk[- ]?\d+[a-z]?\b", ""),
    (r"\b(?:[Ss]ub-)?[Bb]atch[- ]?\d+[a-z]?\b", ""),
    (r"\b[Ss]ub-project #\d+", ""),
    (r"\br\d+-[a-z]+(?:-[a-z]+)*\b", ""),
    (r"\b[Tt]ask[- ](?:T-[A-Z]|tail-\w+|\d+\w*(?:-\w+)*)", ""),
    (r"\bparity task\b", "parity"),
    (r"\btask\b(?=\s*[(:])", ""),
    (r"\(\s*\d+/n\s*\)", ""),
    (r"\bvt-\d+[a-z]*(?:-\w+)*", ""),
    (r"\bStage [A-Z](?:\.\d+)?\b", ""),
    (r"\btier-\d+[a-z]\b|\bpar-\d+\b|\bT-[A-Z]\b|\bC-T\b|\(Part [A-Z]\)|\btail-s\d\w*", ""),
    (r"\bfinale\(\d+\)", "finale"),
    (r"\bfix-review r\d+", "fix review"),
]


def _tidy(text: str) -> str:
    """Tidy punctuation left behind after labels are cut out of a sentence."""
    out = re.sub(r"\(\s*[,;/+&\s]*\)|\[\s*\]", "", text)
    out = re.sub(r"\(\s*[,:]\s*", "(", out)
    out = re.sub(r",\s*\)", ")", out)
    out = re.sub(r":\s*[—-]\s+", ": ", out)
    out = re.sub(r"\bfor\s+(?=—)", "", out)
    out = re.sub(r"\s[—-]\s*(?=[:,;]|$)", "", out)
    out = re.sub(r"\s+([,;:)])", r"\1", out)
    out = re.sub(r"([(])\s+", r"\1", out)
    out = re.sub(r"^\s*[:,;—-]+\s*", "", out)
    out = re.sub(r":\s*[,;:]+", ":", out)
    return out


def find_internal_ids(text: str) -> list[str]:
    """Return every internal id or process label found in text, without changing it.

    For code-like strings (source lines, Zig/Rust types) where strip_internal_ids's
    punctuation tidy would also drop `[]` and `()`.
    """
    found = [m.group(0) for m in ID_PATTERN.finditer(text)]
    for pat, _rep in PROCESS_LABELS:
        found += [m.group(0) for m in re.finditer(pat, text)]
    return found


def strip_internal_ids(text: str) -> tuple[str, bool]:
    """Return (text without internal ids or process labels, whether anything was removed)."""
    clean = ID_PATTERN.sub("", text)
    labelled = clean
    for pat, rep in PROCESS_LABELS:
        clean = re.sub(pat, rep, clean)
    if clean != labelled:
        clean = _tidy(clean)
    clean = re.sub(r"\(\s*\)|\[\s*\]", "", clean)
    clean = re.sub(r"\s{2,}", " ", clean).strip(" :;,-")
    return (clean, True) if clean != text else (text, False)
