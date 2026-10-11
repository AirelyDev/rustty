"""Data builder for the "Every prompt" widget (slug every-prompt).

Reads the prompt extract of the Rustty research directory (made by the research branch's
.port/porting-article/tools/extract_prompts.py) and the author's reviews of it:

- research/data/prompts-raw-2026-10-11.jsonl  every human prompt, UNREDACTED (never shipped as is),
  the founding prompts (typed outside the repository, mostly before it existed) included (`founding: true`)
- research/data/prompts-manifest-2026-10-11.json  public counts per source, rules, cutoff
- research/data/prompts-redactions-proposed-2026-10-11.csv  the scanner's redaction candidates
- research/data/prompts-redactions-approved-2026-10-11.csv  the author's decision per candidate
  (redact or keep), keyed by (id, start, end)
- research/data/prompts-sessions-2026-10-11.jsonl  one aggregate per session (tool, machine, id)
- research/data/prompts-sessions-proposed-2026-10-11.csv  a proposed title and area per session
- research/data/prompts-sessions-approved-2026-10-11.csv  the author's decision per session
  (accept, edit with edited_title/edited_area, or exclude), with the proposal it reviewed
- research/data/prompts-founding-sessions-proposed-2026-10-11.csv and -approved-2026-10-11.csv
  the founding-session candidates and the author's include/exclude decision for each
- research/data/prompts-glossary-extra-2026-10-10.tsv  plain words for ids the glossary lacks
- research/data/prompts-article-overrides-2026-10-10.csv  the author's per-session corrections of
  the article-work tag
- research/data/r2-glossary-ids.csv  the glossary's table form (research/r2-glossary.md)
- research/data/r2-human-story-day-hour-commits-prompts-2026-10-10.csv  commits per Pacific hour

and writes two files: src/app/porting/data/every-prompt.json (the summary bundled with the page:
timeline ticks, commits per day, phases, weekly settings, totals) and, through build_data.py's
"__extra__" hook, public/porting/every-prompt-log.json (the full log, fetched on demand).

Privacy: the builder refuses to run while any proposed redaction, session title/area or founding
session lacks a decision (or was reviewed against a proposal that has since changed), and only
text with every approved redaction applied ever leaves this function. Session titles must not
contain an approved redacted value or an internal id the glossary lacks. ticks[i] and rows[i]
describe the same prompt (both are in time order).

A tick is [dayIndex, minuteOfDay, laneIndex, log2chars, flags, chars, words, modelIndex,
effortIndex, sessionIndex, modeIndex, categoryIndex]: chars and words (whitespace-separated) are of the redacted
text, the model, effort and mode indices point into the summary's "models", "efforts" and "modes"
lists (-1 when unknown), sessionIndex into its "sessions" list and categoryIndex into its
"categories" list (CATEGORIES); FLAG_INFERRED marks a model
taken from commit trailers. So the widget's tiles, filters and weekly settings bars agree exactly
with the log without fetching it.

A session's "area" is one of AREAS, or AREA_COMMAND_ONLY for a session whose every prompt is a
slash command or a shell command (a pseudo-area: those sessions form their own collapsed group and are left out of
the per-area counts).
"""
import csv
import json
import re
import statistics
import sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from zoneinfo import ZoneInfo

from .static_charts import TIMELINE_ERAS

SLUG = "every-prompt"

# DATE_TAG is the extract's file-name tag (the 2026-10-11 refresh with the website folders, "e");
# the glossary extra and the commits per hour were made for the 2026-10-10 extract and the founding
# list for the first 2026-10-11 extract; they still apply.
DATE_TAG = "2026-10-11e"
PREV_TAG = "2026-10-10"
FOUNDING_TAG = "2026-10-11"
RAW = f"prompts-raw-{DATE_TAG}.jsonl"
MANIFEST = f"prompts-manifest-{DATE_TAG}.json"
PROPOSED = f"prompts-redactions-proposed-{DATE_TAG}.csv"
APPROVED = f"prompts-redactions-approved-{DATE_TAG}.csv"
SESSIONS = f"prompts-sessions-{DATE_TAG}.jsonl"
SESSIONS_PROPOSED = f"prompts-sessions-proposed-{DATE_TAG}.csv"
SESSIONS_APPROVED = f"prompts-sessions-approved-{DATE_TAG}.csv"
FOUNDING_PROPOSED = f"prompts-founding-sessions-proposed-{DATE_TAG}.csv"
FOUNDING_APPROVED = f"prompts-founding-sessions-approved-{FOUNDING_TAG}.csv"
# TITLE_REDACTIONS holds scanner hits on proposed titles (optional); a hit is resolved by editing
# the title at review, so the builder refuses while the file has any row.
TITLE_REDACTIONS = f"prompts-redactions-titles-proposed-{DATE_TAG}.csv"
# REDACTION_REVIEWS are every extract's approved redactions; a title must carry no value redacted in any.
REDACTION_REVIEWS = [f"prompts-redactions-approved-{PREV_TAG}.csv",
                     f"prompts-redactions-approved-{FOUNDING_TAG}.csv",
                     "prompts-redactions-approved-2026-10-11d.csv", APPROVED]
GLOSSARY_EXTRA = f"prompts-glossary-extra-{PREV_TAG}.tsv"
OVERRIDES = f"prompts-article-overrides-{DATE_TAG}.csv"
# COUNTS is the post-override counts file committed next to the redacted log on the research
# branch (one value per line, so facts can link a line); `counts()` makes its content.
COUNTS = f"prompts-counts-{DATE_TAG}.json"
GLOSSARY = "r2-glossary-ids.csv"
DAY_HOUR = f"r2-human-story-day-hour-commits-prompts-{PREV_TAG}.csv"

# LOG_PATH is where build_data.py writes the log, relative to the repository root; LOG_URL is
# the path the page fetches it from.
LOG_PATH = "public/porting/every-prompt-log.json"
LOG_URL = "/porting/every-prompt-log.json"

# SOURCES_HREF is the source link of the section's data: the redacted log as committed on the
# research branch (byte-identical to LOG_PATH), next to the counts-only manifest.
SOURCES_HREF = ("https://github.com/AirelyDev/rustty/blob/064f0f66ecb10b451f9e99d48faca34fbc6afdb5/"
                ".port/porting-article/research/data/prompts-redacted-2026-10-11e.json")

PACIFIC = ZoneInfo("America/Los_Angeles")
# WINDOW is in Pacific days: the founding session started on the evening of 20 June Pacific
# (21 June UTC), before the first method era; its prompts take the first era (see _phase).
WINDOW = ("2026-06-20", "2026-10-11")
PURGED_BEFORE = "2026-09-08"
# ARTICLE_CUTOFF is the moment from which a session's prompts can be article work (extractor rule).
ARTICLE_CUTOFF = datetime(2026, 10, 8, tzinfo=PACIFIC)
# OTHER_FIGURES_CUTOFF is where the other prompt figures of the article stop (the session-record
# snapshot: 14:00 Pacific on 8 October); `counts()` counts the prompts after it separately, and
# the Claude Code prompts before it reconcile with the article's 898.
OTHER_FIGURES_CUTOFF = datetime(2026, 10, 8, 14, tzinfo=PACIFIC)

# LANES orders the timeline lanes; a row's lane is "<tool>-<machine>", Codex runs only on the Mac.
LANES = ["claude-mac", "claude-vm", "codex"]

# FLAG_* are the tick flag bits.
FLAG_DECISION, FLAG_COMMAND, FLAG_ARTICLE, FLAG_UNKNOWN, FLAG_PASTED_LOST, FLAG_REDACTED = 1, 2, 4, 8, 16, 32
FLAG_INFERRED = 64
# FLAG_WEBSITE marks a prompt typed in the website folder (the extractor's project marker "website").
FLAG_WEBSITE = 128
# LINKED_BASES are the commit bases whose commits are linked; "other-repo" (website-folder prompts,
# whose commits are in the website repository) and the rest carry no links.
LINKED_BASES = ("session-window", "time-window-branch")
COMMIT_BASES = (*LINKED_BASES, "none", "ambiguous", "other-repo")
# A tick is [day, minute of day, lane, flags, chars, words, model index, effort index, session
# index, mode index, category index]; T_* name the positions. The length on the log scale is derived
# from chars by the page (not sent: the summary must stay under 60 KB).
T_DAY, T_MINUTE, T_LANE, T_FLAGS, T_CHARS, T_WORDS = 0, 1, 2, 3, 4, 5
T_MODEL, T_EFFORT, T_SESSION, T_MODE, TICK_CATEGORY = 6, 7, 8, 9, 10

# REDACTION_LABELS are the plain words shown in "[redacted: <label>]" per scanner reason.
REDACTION_LABELS = {
    "secret": "secret",
    "email": "email address",
    "user-host": "login",
    "ip": "IP address",
    "phone": "phone number",
    "person": "name",
    "unrelated-path": "unrelated path",
    "unrelated-project": "unrelated project",
}

# PHASES are the article's three method eras (static_charts.TIMELINE_ERAS) with data keys.
PHASE_KEYS = ["founding", "tightened", "never-port-a-bug"]

# AREAS are the session areas in their fixed display order (exact labels); AREA_COMMAND_ONLY is the
# pseudo-area of command-only sessions (not in AREAS, not in the per-area counts).
AREAS = ["engine", "renderer and fonts", "macOS app", "GTK/Linux", "performance", "upstream catch-up",
         "verification/CI", "defects", "tooling", "website", "article"]
AREA_COMMAND_ONLY = "command-only"
# SESSION_AREAS index a summary session row's area (AREAS, then the command-only pseudo-area).
SESSION_AREAS = [*AREAS, AREA_COMMAND_ONLY]
TITLE_SOURCES = ("claude-code", "claude-written")
# PROMPT_BUCKETS are the lower bounds of the session bar thickness buckets (prompts per session).
PROMPT_BUCKETS = [1, 2, 5, 15, 50]
SESSION_RULE = ("A session is one Claude Code or Codex session id on one machine. It spans the Pacific days "
                "from its first to its last human prompt (a resumed session keeps its id, so its span includes "
                "the days between). A session counts as open on a Pacific day when it has at least one prompt "
                "that day that is an instruction or a decision (not a slash command or a shell command). A "
                "session whose every prompt is a slash command or a shell command is a command-only session.")
# SESSION_FIELDS name the values of a summary session row, in order: key, lane index, first and
# last prompt (day index, minute of the Pacific day), prompts, thickness bucket (index into
# PROMPT_BUCKETS), area (an AREAS label or AREA_COMMAND_ONLY), title, title source, founding,
# article work, command-only, linked commits (count), the packed timeline row and the Pacific days
# spanned from the first to the last prompt (inclusive).
SESSION_FIELDS = ["key", "lane", "firstDay", "firstMin", "lastDay", "lastMin", "prompts", "promptsBucket", "area",
                  "title", "titleSource", "founding", "articleWork", "commandsOnly", "commits", "row",
                  "spannedDays"]
# The session timeline's geometry, in viewBox units (SessionTimeline.tsx; about 1 px each at
# desktop width): PLOT_UNITS is the plot width the window spans, BAR_MIN_UNITS the narrowest bar,
# HIT_UNITS the narrowest pointer target (centred on its bar) and GAP_UNITS the least space
# between two bars on one row: 3 units of visible gap plus half a hit target, so neighbours never
# read as one bar and their hit targets never overlap.
PLOT_UNITS = 882
BAR_MIN_UNITS = 2
HIT_UNITS = 7
GAP_UNITS = 3 + HIT_UNITS / 2
# PACK_MINUTES is the shortest span a session takes on a packed row in pack_rows' default call
# (about 2 units at the window's scale); build passes the window's own scale.
PACK_MINUTES = 480

# Copied from extract_prompts.py (research worktree, commit 4564311f0): prompt_kind,
# DECISION_WORDS, DECISION_RULE and is_decision. Slash commands are never decisions.
DECISION_WORDS = {"ok", "yes", "y", "no", "go", "ahead", "continue", "proceed", "sure", "agreed",
                  "lgtm", "approve", "approved", "a", "b", "c", "d"}
DECISION_RULE = ("After lowercasing and replacing punctuation with spaces, the prompt is a single choice "
                 "letter (a-d) or number (one or two digits, answering a numbered choice), or 1-3 words all drawn from {" + ", ".join(sorted(DECISION_WORDS)) + "}. "
                 "Slash commands are never decisions.")


def prompt_kind(text: str) -> str:
    """`command` for a slash command (`/clear`, `/goal …`); a path such as `/Users/…` is a prompt."""
    return "command" if re.match(r"/[A-Za-z][\w:-]*(?:\s|$)", text) else "prompt"


# CATEGORIES are the prompt categories in their fixed order (author decision, 2026-10-11); every
# prompt has exactly one (`category`). SETTING_COMMANDS and SESSION_COMMANDS name the slash
# commands of the setting and session categories; any other slash command is other-command.
CATEGORIES = ["instruction", "decision", "shell", "setting", "session", "other-command"]
SUBSTANTIVE = ("instruction", "decision")
SETTING_COMMANDS = {"/model", "/effort", "/fast", "/config", "/permissions", "/output-style"}
SESSION_COMMANDS = {"/clear", "/resume", "/compact", "/remote-control", "/continue", "/rename"}
# COMMAND_ALIASES map a slash command typed under another name to the command it stands for: /rc is
# Claude Code's short form of /remote-control, and /moidel a typo of /model in the data. An explicit
# list (no fuzzy matching), so a new name never changes category silently.
COMMAND_ALIASES = {"/rc": "/remote-control", "/moidel": "/model"}
# TEXT_COMMANDS carry the prompt itself after the command (/btw asks a side question, /goal sets a
# goal in words): with text after them they are instructions; bare, they are other commands.
TEXT_COMMANDS = {"/btw", "/goal"}
CATEGORY_RULE = ("Every prompt has one category. A slash command is a setting (/model, /effort, /fast, /config, "
                 "/permissions, /output-style), a session command (/clear, /resume, /compact, /remote-control, "
                 "/continue, /rename; /rc is short for /remote-control and the typo /moidel counts as /model) or "
                 "another command (any other slash command), except that /btw or /goal followed by text is an "
                 "instruction (the text is the prompt). Text starting with ! is a "
                 "shell command (Claude Code runs it in the shell). Of the rest, a prompt that only answers a "
                 "choice or acknowledges is a decision, and everything else is an instruction.")


def command_name(text: str) -> str | None:
    """The lowercased slash command a prompt starts with (`/model`), or None for any other prompt."""
    m = re.match(r"(/[A-Za-z][\w:-]*)(?:\s|$)", text)
    return m.group(1).lower() if m else None


def category(text: str) -> str:
    """The prompt's category (see CATEGORY_RULE)."""
    name = command_name(text)
    if name is not None:
        name = COMMAND_ALIASES.get(name, name)
        if name in TEXT_COMMANDS and text.strip()[len(name):].strip():
            return "instruction"
        if name in SETTING_COMMANDS:
            return "setting"
        return "session" if name in SESSION_COMMANDS else "other-command"
    if text.lstrip().startswith("!"):
        return "shell"
    return "decision" if is_decision(text) else "instruction"


def is_decision(text: str) -> bool:
    """A prompt that only answers a choice or acknowledges (A, b, 2, ok, go ahead, continue). See DECISION_RULE."""
    if prompt_kind(text.strip()) == "command":
        return False
    words = re.sub(r"[^\w\s]|_", " ", text.lower()).split()
    if len(words) == 1 and re.fullmatch(r"[a-d]|\d{1,2}", words[0]):
        return True
    return 1 <= len(words) <= 3 and all(w in DECISION_WORDS for w in words)


# ID_RE finds the internal ids that get a glossary hover; it mirrors jargon.ID_PATTERN (a sync test
# keeps them equal) and stays JavaScript-compatible so the widget could reuse it.
ID_RE = re.compile(
    r"\b(?:UD\d+|N\d{1,3}|U-\w+|R\d{1,2}[a-z]?|D\d[a-z]?|G\d+[a-z]?(?:-SELF)?|G-[A-Z]+|"
    r"A\d|P\d+-W\d+[a-z]?|W\d+\.T\d+[a-z]?|T\d+[a-z]|FX\d+)\b"
)


def dump(data) -> str:
    """Serialize data the way build_data.py writes it."""
    return json.dumps(data, ensure_ascii=False, separators=(",", ":"))


def _num(x):
    """x as an int when whole (a median of ints), else as is."""
    return int(x) if x == int(x) else x


def _fail(name: str, msg: str):
    """Stop the build with a message naming the research file at fault."""
    raise SystemExit(f"every-prompt: {name}: {msg}")


def _csv(path: Path) -> list[dict]:
    """Read a CSV file as dict rows."""
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def _pacific(ts: str) -> datetime:
    """Parse an ISO UTC timestamp ("...Z") into Pacific time."""
    return datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone(PACIFIC)


def _approved(data: Path, raw: dict) -> dict:
    """Return {prompt id: [(start, end, reason)]} of the approved redactions.

    Both CSVs must carry a `match` column (the redacted span as typed). Refuses (SystemExit naming
    the file and the prompt id) while any proposed candidate has no decision, a decision other than
    redact/keep, a `match` that differs between the two files or from the raw text at
    [start:end] (a stale review against a re-extracted prompt), or a span that does not fit its
    prompt; or when spans overlap. An approved row with no proposal is applied (the author may add
    redactions) but reported on stderr.
    """
    for name in (APPROVED, PROPOSED):
        with open(data / name, newline="") as f:
            header = next(csv.reader(f), [])
        if "match" not in header:
            _fail(name, "missing the required `match` column")
    decided = {}
    for r in _csv(data / APPROVED):
        d = (r.get("decision") or "").strip().lower()
        key = (r["id"], int(r["start"]), int(r["end"]))
        if d not in ("redact", "keep"):
            _fail(APPROVED, f"{r['id']} [{r['start']}:{r['end']}] has decision {r.get('decision')!r} "
                            "(expected redact or keep)")
        if r["id"] not in raw:
            _fail(APPROVED, f"{r['id']} is not in {RAW}")
        if raw[r["id"]]["text"][key[1]:key[2]] != r["match"]:
            _fail(APPROVED, f"{r['id']} [{r['start']}:{r['end']}]: `match` is not the prompt's text at that "
                            "span (stale review? re-run the extractor and review again)")
        decided[key] = (d, r["reason"], r["match"])
    proposed = set()
    for r in _csv(data / PROPOSED):
        key = (r["id"], int(r["start"]), int(r["end"]))
        proposed.add(key)
        if key not in decided:
            _fail(APPROVED, f"proposed redaction {r['id']} [{r['start']}:{r['end']}] ({r['reason']}) is not "
                            "reviewed; no text is emitted until every proposal has a decision")
        if decided[key][2] != r["match"]:
            _fail(APPROVED, f"{r['id']} [{r['start']}:{r['end']}]: `match` differs from {PROPOSED}")
        if r["id"] in raw and raw[r["id"]]["text"][key[1]:key[2]] != r["match"]:
            _fail(PROPOSED, f"{r['id']} [{r['start']}:{r['end']}]: `match` is not the prompt's text at that span")
    for key in sorted(set(decided) - proposed):
        print(f"every-prompt: warning: {APPROVED}: {key[0]} [{key[1]}:{key[2]}] has no proposal in {PROPOSED}",
              file=sys.stderr)
    out: dict = {}
    for (pid, start, end), (d, reason, _match) in decided.items():
        if pid not in raw:
            _fail(APPROVED, f"{pid} is not in {RAW}")
        if not 0 <= start < end <= len(raw[pid]["text"]):
            _fail(APPROVED, f"{pid} [{start}:{end}] does not fit the prompt")
        if reason not in REDACTION_LABELS:
            _fail(APPROVED, f"{pid}: unknown reason {reason!r}")
        if d == "redact":
            out.setdefault(pid, []).append((start, end, reason))
    for pid, spans in out.items():
        spans.sort()
        for a, b in zip(spans, spans[1:]):
            if b[0] < a[1]:
                _fail(APPROVED, f"{pid}: redactions [{a[0]}:{a[1]}] and [{b[0]}:{b[1]}] overlap")
    return out


def redact(text: str, spans: list) -> str:
    """Replace each (start, end, reason) span of text with "[redacted: <label>]"."""
    for start, end, reason in sorted(spans, reverse=True):
        text = text[:start] + f"[redacted: {REDACTION_LABELS[reason]}]" + text[end:]
    return text


def _glossary(data: Path) -> dict:
    """Map each glossary id to its plain description (collisions joined with "Or: ").

    The glossary's "the pin" (the port's word for the Ghostty reference commit) is written out.
    """
    out: dict = {}
    for r in _csv(data / GLOSSARY):
        key = r["id"].split("⚠")[0].strip().split()[0] if r["id"].strip() else ""
        desc = r["plain_description"].strip()
        if key and desc and desc not in out.get(key, []):
            out.setdefault(key, []).append(desc)
    with open(data / GLOSSARY_EXTRA) as f:
        for line in f:
            if line.strip() and not line.startswith("#"):
                key, desc = line.rstrip("\n").split("\t", 1)
                out[key.strip()] = [desc.strip()]
    return {k: re.sub(r"\bthe pin\b", "the reference commit", " Or: ".join(v)) for k, v in out.items()}


def _overrides(data: Path) -> dict:
    """Map session -> article_work bool from the author's override file."""
    out = {}
    for r in _csv(data / OVERRIDES):
        v = r["article_work"].strip().lower()
        if v not in ("true", "false"):
            _fail(OVERRIDES, f"{r['session']}: article_work is {r['article_work']!r}")
        out[r["session"]] = v == "true"
    return out


def _phase(day: str) -> str:
    """The method era key of a Pacific day; a founding day before the first era takes the first,
    and a day of the window after the last era (the extract ran into 11 October) takes the last."""
    if WINDOW[0] <= day < TIMELINE_ERAS[0]["start"]:
        return PHASE_KEYS[0]
    if TIMELINE_ERAS[-1]["end"] < day <= WINDOW[1]:
        return PHASE_KEYS[-1]
    for key, era in zip(PHASE_KEYS, TIMELINE_ERAS):
        if era["start"] <= day <= era["end"]:
            return key
    raise SystemExit(f"every-prompt: day {day} is outside every era")


def _founding(data: Path) -> set:
    """The session ids the author included as founding sessions.

    Refuses while a proposed candidate has no include/exclude decision, or its approved row
    describes another session span than the proposal (stale review).
    """
    approved = {}
    for r in _csv(data / FOUNDING_APPROVED):
        d = (r.get("decision") or "").strip().lower()
        if d not in ("include", "exclude"):
            _fail(FOUNDING_APPROVED, f"{r['session']} has decision {r.get('decision')!r} (expected include or exclude)")
        if r["session"] in approved:
            _fail(FOUNDING_APPROVED, f"{r['session']} is listed twice")
        approved[r["session"]] = (d, r)
    for p in _csv(data / FOUNDING_PROPOSED):
        if p["session"] not in approved:
            _fail(FOUNDING_APPROVED, f"founding candidate {p['session']} is not reviewed")
        a = approved[p["session"]][1]
        for k in ("tool", "machine", "first_ts", "last_ts", "prompts"):
            if a.get(k) != p.get(k):
                _fail(FOUNDING_APPROVED, f"{p['session']}: {k} differs from {FOUNDING_PROPOSED} (stale review)")
    return {s for s, (d, _r) in approved.items() if d == "include"}


def _session_reviews(data: Path) -> tuple[dict, set]:
    """({session id: (key, title, title source, area)}, {excluded session ids}) from the review.

    Refuses while a proposed session has no decision (accept, edit or exclude), an approved row has
    no proposal, or an approved row carries another key, title, title source or area than the
    proposal it reviewed (stale review). An edit takes edited_title and edited_area; an edited
    title is written text (title source claude-written) unless the row's edited_title_source says
    claude-code (a typo fix of Claude Code's own title).
    """
    proposed = {}
    for r in _csv(data / SESSIONS_PROPOSED):
        if r["session"] in proposed:
            _fail(SESSIONS_PROPOSED, f"{r['session']} is listed twice")
        proposed[r["session"]] = r
    approved = {}
    for r in _csv(data / SESSIONS_APPROVED):
        if r["session"] in approved:
            _fail(SESSIONS_APPROVED, f"{r['session']} is listed twice")
        if r["session"] not in proposed:
            _fail(SESSIONS_APPROVED, f"{r['session']} has no proposal in {SESSIONS_PROPOSED}")
        approved[r["session"]] = r
    out, excluded = {}, set()
    for sid, p in proposed.items():
        if sid not in approved:
            _fail(SESSIONS_APPROVED, f"session {p['key']} is not reviewed; no title ships until every session "
                                     "has a decision")
        a = approved[sid]
        for k in ("key", "title", "title_source", "area"):
            if a[k] != p[k]:
                _fail(SESSIONS_APPROVED, f"session {p['key']}: the reviewed {k} {a[k]!r} is not the proposed "
                                         f"{p[k]!r} (stale review? review the new proposal)")
        d = (a.get("decision") or "").strip().lower()
        if d == "exclude":
            excluded.add(sid)
            continue
        if d == "accept":
            title, area, source = p["title"], p["area"], p["title_source"]
        elif d == "edit":
            title, area = (a.get("edited_title") or "").strip(), (a.get("edited_area") or "").strip()
            if not title or not area:
                _fail(SESSIONS_APPROVED, f"session {p['key']}: an edit needs edited_title and edited_area")
            source = (a.get("edited_title_source") or "").strip() or "claude-written"
        else:
            _fail(SESSIONS_APPROVED, f"session {p['key']} has decision {a.get('decision')!r} "
                                     "(expected accept, edit or exclude)")
        if area not in AREAS and area != AREA_COMMAND_ONLY:
            _fail(SESSIONS_APPROVED, f"session {p['key']}: unknown area {area!r}")
        if source not in TITLE_SOURCES:
            _fail(SESSIONS_APPROVED, f"session {p['key']}: unknown title source {source!r}")
        out[sid] = (p["key"], title, source, area)
    return out, excluded


def _redacted_values(data: Path) -> list[str]:
    """The approved redacted values (lowercased) of this and every earlier review, for the title check."""
    out = set()
    for name in REDACTION_REVIEWS:
        if (data / name).exists():
            out |= {r["match"].lower() for r in _csv(data / name)
                    if (r.get("decision") or "").strip().lower() == "redact" and r["match"]}
    return sorted(out)


def _bucket(prompts: int) -> int:
    """The thickness bucket of a session's prompt count (index into PROMPT_BUCKETS)."""
    return max(i for i, low in enumerate(PROMPT_BUCKETS) if prompts >= low)


def pack_rows(spans: list, min_len: float = PACK_MINUTES, gap: float = 0) -> list:
    """Greedy first-fit packing of (start, end) minute spans sorted by start: the row of each span.

    A span occupies at least min_len; a row takes a span only when the span starts more than gap
    after the row's last span ends.
    """
    ends: list = []
    out = []
    for start, end in spans:
        end = max(end, start + min_len)
        for i, e in enumerate(ends):
            if start > e + gap:
                ends[i] = end
                out.append(i)
                break
        else:
            ends.append(end)
            out.append(len(ends) - 1)
    return out


def _week(day: date) -> str:
    """The Monday that starts day's week."""
    return (day - timedelta(days=day.weekday())).isoformat()


def build(research: Path, _mockbuild: Path) -> dict:
    """Build every-prompt.json; the log rides along under "__extra__" (see build_data.py)."""
    data = research / "data"
    rows_raw = []
    with open(data / RAW) as f:
        for line in f:
            if line.strip():
                rows_raw.append(json.loads(line))
    raw = {r["id"]: r for r in rows_raw}
    if len(raw) != len(rows_raw):
        _fail(RAW, "duplicate prompt ids")
    manifest = json.loads((data / MANIFEST).read_text())
    spans = _approved(data, raw)
    glossary = _glossary(data)
    overrides = _overrides(data)
    founding = _founding(data)
    reviews, excluded = _session_reviews(data)
    if (data / TITLE_REDACTIONS).exists() and _csv(data / TITLE_REDACTIONS):
        _fail(TITLE_REDACTIONS, "has redaction hits on titles; edit those titles at review instead")

    start = date.fromisoformat(WINDOW[0])
    ndays = (date.fromisoformat(WINDOW[1]) - start).days + 1
    rows_raw.sort(key=lambda r: (r["ts"], LANES.index(_lane(r)), r["id"]))
    sessions, sess_of = _sessions(data, rows_raw, reviews, excluded, founding)

    ticks, rows, weeks, used_ids, pre_purge_tx = [], [], {}, {}, 0
    models = sorted({r["model"] for r in rows_raw if r["model"]})
    efforts = sorted({r["effort"] for r in rows_raw if r["effort"]})
    modes = sorted({r["mode"] for r in rows_raw if r["mode"]})
    totals = dict(prompts=0, bySource={k: 0 for k in LANES}, redactions=0, redactedPrompts=0, pastedLost=0,
                  settingsUnknown=0, settingsInferred=0, articleWork=0, decisions=0,
                  foundingPrompts=0, website=0, byCategory={c: 0 for c in CATEGORIES})
    for r in rows_raw:
        lane = _lane(r)
        when = _pacific(r["ts"])
        day = when.date()
        di = (day - start).days
        if not 0 <= di < ndays:
            _fail(RAW, f"{r['id']} is outside the window {WINDOW}")
        source = r["settings_source"]
        if lane == "claude-mac" and day.isoformat() < PURGED_BEFORE:
            if source == "session":
                _fail(RAW, f"{r['id']}: session-level settings claimed in the purged window")
            pre_purge_tx += source == "transcript"
        text = redact(r["text"], spans.get(r["id"], []))
        kind = prompt_kind(r["text"])
        website = r.get("project_marker") == "website"
        decision = is_decision(r["text"])
        cat = category(r["text"])
        if decision != (cat == "decision"):
            _fail(RAW, f"{r['id']}: the decision rule and the {cat!r} category disagree")
        article = r["article_work"]
        if r["session"] in overrides:
            article = overrides[r["session"]] and when >= ARTICLE_CUTOFF
        n_red = len(spans.get(r["id"], []))
        unknown = source == "unknown"
        flags = ((FLAG_DECISION if decision else 0) | (FLAG_COMMAND if kind == "command" else 0)
                 | (FLAG_ARTICLE if article else 0) | (FLAG_UNKNOWN if unknown else 0)
                 | (FLAG_PASTED_LOST if r["pasted"] == "lost" else 0) | (FLAG_REDACTED if n_red else 0)
                 | (FLAG_INFERRED if source == "commit-trailer" else 0)
                 | (FLAG_WEBSITE if website else 0))
        chars = len(text)
        sess = sess_of[r["id"]]
        sessions[sess]["articleWork"] |= article
        ticks.append([di, when.hour * 60 + when.minute, LANES.index(lane),
                      flags, chars, len(text.split()),
                      models.index(r["model"]) if r["model"] else -1,
                      efforts.index(r["effort"]) if r["effort"] else -1, sess,
                      modes.index(r["mode"]) if r["mode"] else -1, CATEGORIES.index(cat)])
        basis = r["commits_basis"]
        if basis not in COMMIT_BASES:
            _fail(RAW, f"{r['id']}: unknown commits basis {basis!r}")
        if (basis == "other-repo") != website:
            _fail(RAW, f"{r['id']}: the 'other-repo' commit basis is for website-folder prompts only")
        rows.append({
            "id": r["id"], "ts": r["ts"], "tool": r["tool"], "machine": r["machine"],
            "model": r["model"], "effort": r["effort"], "mode": r["mode"], "settingsSource": source,
            "chars": chars, "kind": kind, "category": cat, "pasted": r["pasted"], "articleWork": article,
            "decision": decision, "phase": _phase(day.isoformat()),
            "commits": list(r["commits"]) if basis in LINKED_BASES else [],
            "commitsBasis": basis, "session": sessions[sess]["key"], "website": website, "text": text,
        })
        if basis in LINKED_BASES:
            sessions[sess]["_commits"].update(r["commits"])
        for m in ID_RE.finditer(text):
            used_ids.setdefault(m.group(0), r["id"])

        totals["prompts"] += 1
        totals["bySource"][lane] += 1
        totals["redactions"] += n_red
        totals["redactedPrompts"] += bool(n_red)
        totals["pastedLost"] += r["pasted"] == "lost"
        totals["settingsUnknown"] += unknown
        totals["settingsInferred"] += source == "commit-trailer"
        totals["articleWork"] += article
        totals["decisions"] += decision
        totals["byCategory"][cat] += 1
        totals["foundingPrompts"] += bool(r.get("founding"))
        totals["website"] += website

        w = weeks.setdefault(_week(day), {"settings": {}, "inferred": {}, "modes": {}})
        if source == "commit-trailer":
            w["inferred"][r["model"]] = w["inferred"].get(r["model"], 0) + 1
        elif r["model"]:
            k = f"{r['model']}|{r['effort'] or 'unknown'}"
            w["settings"][k] = w["settings"].get(k, 0) + 1
        else:
            w["settings"]["unknown"] = w["settings"].get("unknown", 0) + 1
        mode = r["mode"] or "unknown"
        w["modes"][mode] = w["modes"].get(mode, 0) + 1

    # The manifest's mac_before_purge_with_transcript counts the pre-purge Mac prompts whose settings
    # come from a transcript that survived the purge; more "transcript" rows than that would claim
    # settings the purge destroyed. Without the field, the bound is 0 (no pre-purge transcript rows).
    allowed = manifest["counts"].get("mac_before_purge_with_transcript", 0)
    if pre_purge_tx > allowed:
        _fail(MANIFEST, f"{pre_purge_tx} Mac prompts before {PURGED_BEFORE} claim transcript settings; "
                        f"the manifest allows {allowed} (mac_before_purge_with_transcript)")
    counts = manifest["counts"]
    if totals["prompts"] != counts["prompts"] or totals["bySource"] != counts["by_source"]:
        _fail(MANIFEST, f"counts do not reconcile: built {totals['prompts']} {totals['bySource']}, "
                        f"manifest {counts['prompts']} {counts['by_source']}")
    for name, s in manifest["sources"].items():
        if "kept" in s and s["kept"] + sum(s["dropped"].values()) != s["in_source"]:
            _fail(MANIFEST, f"source {name} does not reconcile")
    totals["medianChars"] = statistics.median(r["chars"] for r in rows) if rows else 0
    # The shell escapes and slash commands the widget hides by default, and the length figures over
    # instructions and decisions only (the substantive prompts; the all-prompt figures stay above).
    totals["commandsAndShell"] = sum(n for c, n in totals["byCategory"].items() if c not in SUBSTANTIVE)
    subst = [t for t in ticks if CATEGORIES[t[TICK_CATEGORY]] in SUBSTANTIVE]
    totals["promptsSubstantive"] = len(subst)
    totals["medianCharsSubstantive"] = _num(statistics.median(t[T_CHARS] for t in subst)) if subst else 0
    totals["medianWordsSubstantive"] = _num(statistics.median(t[T_WORDS] for t in subst)) if subst else 0
    totals["underTenWordsSubstantive"] = sum(1 for t in subst if t[T_WORDS] < 10)

    if weeks:  # every week from the first to the last prompt, empty weeks included
        d, last = date.fromisoformat(min(weeks)), date.fromisoformat(max(weeks))
        while d <= last:
            weeks.setdefault(d.isoformat(), {"settings": {}, "inferred": {}, "modes": {}})
            d += timedelta(days=7)

    per_day = [0] * ndays
    for r in _csv(data / DAY_HOUR):  # indexed by its Pacific date (its day_index counts from 21 June)
        i = (date.fromisoformat(r["date_pt"]) - start).days
        if 0 <= i < ndays:
            per_day[i] += int(r["commits_non_merge"])

    log_sessions = {}
    for i, x in enumerate(sessions):
        got = x.pop("_commits")
        if got != set(x["_src"]["commits"]):
            _fail(SESSIONS, f"session {x['key']}: linked commits do not reconcile with the prompts' commits")
        src = x.pop("_src")
        x["commits"] = len(got)
        log_sessions[x["key"]] = {"title": x["title"], "titleSource": x["titleSource"], "area": x["area"],
                                  "branches": list(src["branches"]), "models": list(src["models"]),
                                  "commits": sorted(got), "commitsAmbiguous": src["commits_ambiguous"]}
        for m in ID_RE.finditer(x["title"]):
            used_ids.setdefault(m.group(0), f"title of {x['key']}")
    missing = sorted(i for i in used_ids if i not in glossary)
    if missing:
        _fail(GLOSSARY_EXTRA, f"ids without plain words: {', '.join(f'{i} ({used_ids[i]})' for i in missing)}")
    totals.update(_session_totals(sessions, start))
    for x in sessions:
        x.pop("_activeDays")
    if totals["foundingPrompts"] != sum(x["prompts"] for x in sessions if x["founding"]):
        _fail(RAW, "founding prompts do not reconcile with the founding sessions")

    log = {"glossary": {i: glossary[i] for i in sorted(used_ids)}, "sessions": log_sessions, "rows": rows}
    rules = manifest["rules"]
    untag = sum(1 for v in overrides.values() if not v)
    article_rule = rules["article_work"]
    if overrides:
        article_rule += (f" The author corrected {len(overrides)} session{'s' if len(overrides) != 1 else ''} "
                         f"by hand: {untag} untagged, {len(overrides) - untag} tagged.")
    return {
        "_meta": {"slug": SLUG, "generatedFrom": [f"data/{n}" for n in (RAW, MANIFEST, APPROVED, SESSIONS,
                                                                        SESSIONS_APPROVED, FOUNDING_APPROVED,
                                                                        GLOSSARY_EXTRA, OVERRIDES, GLOSSARY,
                                                                        DAY_HOUR)],
                  "asOf": manifest["cutoff"]},
        "window": list(WINDOW),
        "purgedBefore": PURGED_BEFORE,
        "lanes": LANES,
        "models": models,
        "efforts": efforts,
        "modes": modes,
        "ticks": ticks,
        "commitsPerDay": per_day,
        # The first era starts at the window start: the founding days before it take the first era.
        "phases": [{"key": k, "label": e["label"],
                    "from": 0 if i == 0 else (date.fromisoformat(e["start"]) - start).days,
                    "to": (ndays - 1 if i == len(PHASE_KEYS) - 1 else (date.fromisoformat(e["end"]) - start).days)}
                   for i, (k, e) in enumerate(zip(PHASE_KEYS, TIMELINE_ERAS))],
        "areas": AREAS,
        "promptBuckets": PROMPT_BUCKETS,
        # Sessions are rows of SESSION_FIELDS values (as ticks are), to keep the summary small.
        "sessionFields": SESSION_FIELDS,
        # A row's area and title source are indices into sessionAreas and titleSources.
        "sessionAreas": SESSION_AREAS,
        "titleSources": list(TITLE_SOURCES),
        "sessions": [[_session_value(x, k) for k in SESSION_FIELDS] for x in sessions],
        "weeks": [{"week": k, **weeks[k]} for k in sorted(weeks)],
        "totals": totals,
        "logUrl": LOG_URL,
        "logBytes": len(dump(log).encode()),
        "categories": CATEGORIES,
        "rules": {"category": CATEGORY_RULE, "decision": DECISION_RULE, "articleWork": article_rule,
                  "commits": rules["commits"], "sessions": SESSION_RULE},
        "sourcesHref": SOURCES_HREF,
        "__extra__": {LOG_PATH: log},
    }


def pack_scale(ndays: int) -> tuple[float, float]:
    """(min_len, gap) in minutes for pack_rows on a window of ndays days drawn PLOT_UNITS wide."""
    per_unit = ndays * 1440 / PLOT_UNITS
    return BAR_MIN_UNITS * per_unit, GAP_UNITS * per_unit


def _sessions(data: Path, rows_raw: list, reviews: dict, excluded: set, founding: set) -> tuple[list, dict]:
    """(summary sessions in start order, {prompt id: session index}) from the session aggregates.

    rows_raw is the extract in time order. Refuses when a prompt belongs to no session, a session's
    count, span, command-only flag or founding flag disagrees with its prompts, a session is not
    reviewed (or was excluded but still has prompts), a founding prompt is outside the approved
    founding sessions, or a title carries an approved redacted value.
    """
    agg = {}
    with open(data / SESSIONS) as f:
        for line in f:
            if line.strip():
                x = json.loads(line)
                k = (x["tool"], x["machine"], x["session"])
                if k in agg:
                    _fail(SESSIONS, f"session {x['key']} is listed twice")
                agg[k] = x
    members: dict = {}
    for r in rows_raw:
        k = (r["tool"], r["machine"], r["session"])
        if k not in agg:
            _fail(SESSIONS, f"{r['id']} belongs to no session")
        if r.get("founding") and r["session"] not in founding:
            _fail(RAW, f"{r['id']} is a founding prompt outside the approved founding sessions")
        members.setdefault(k, []).append(r)
    start = date.fromisoformat(WINDOW[0])
    redacted = _redacted_values(data)
    out = []
    for k, x in agg.items():
        rs = members.get(k, [])
        sid = x["session"]
        if sid in excluded:
            _fail(SESSIONS_APPROVED, f"session {x['key']} is excluded but is in {SESSIONS}"
                                     + (f" and has {len(rs)} prompt{'s' if len(rs) != 1 else ''} in {RAW}" if rs else ""))
        if sid not in reviews:
            _fail(SESSIONS_APPROVED, f"session {x['key']} has no proposal and no review")
        key, title, source, area = reviews[sid]
        if key != x["key"]:
            _fail(SESSIONS_APPROVED, f"session {sid}: reviewed key {key!r} is not {x['key']!r}")
        if not rs or len(rs) != x["prompts"] or rs[0]["ts"] != x["first_ts"] or rs[-1]["ts"] != x["last_ts"]:
            _fail(SESSIONS, f"session {x['key']}: prompts or span do not reconcile with {RAW}")
        # Command-only: every prompt is a slash command or a shell escape (no instruction or decision).
        commands_only = all(category(r["text"]) not in SUBSTANTIVE for r in rs)
        if commands_only != (area == AREA_COMMAND_ONLY):
            _fail(SESSIONS_APPROVED, f"session {x['key']}: command-only sessions and the {AREA_COMMAND_ONLY!r} "
                                     "area must coincide")
        is_founding = sid in founding
        if is_founding != bool(x["founding"]) or any(bool(r.get("founding")) != is_founding for r in rs):
            _fail(SESSIONS, f"session {x['key']}: founding flag disagrees with {FOUNDING_APPROVED}")
        low = title.lower()
        if any(v in low for v in redacted):
            _fail(SESSIONS_APPROVED, f"session {x['key']}: the title contains an approved redacted value")
        a, b = _pacific(x["first_ts"]), _pacific(x["last_ts"])
        out.append({"key": x["key"], "lane": LANES.index(_lane(rs[0])),
                    "firstDay": (a.date() - start).days, "firstMin": a.hour * 60 + a.minute,
                    "lastDay": (b.date() - start).days, "lastMin": b.hour * 60 + b.minute,
                    "prompts": x["prompts"], "promptsBucket": _bucket(x["prompts"]), "area": area,
                    "title": title, "titleSource": source, "founding": is_founding, "articleWork": False,
                    "commandsOnly": commands_only, "commits": 0, "row": 0,
                    "spannedDays": (b.date() - a.date()).days + 1,
                    "_activeDays": sorted({(_pacific(r["ts"]).date() - start).days for r in rs
                                           if category(r["text"]) in SUBSTANTIVE}),
                    "_ts": (x["first_ts"], x["last_ts"]), "_ids": [r["id"] for r in rs], "_src": x,
                    "_commits": set()})
    for founding_sid in founding:
        if not any(x["_src"]["session"] == founding_sid for x in out):
            _fail(FOUNDING_APPROVED, f"founding session {founding_sid} is not in {SESSIONS} (re-run the extractor "
                                     "with the approved founding list)")
    out.sort(key=lambda x: (x["_ts"][0], x["lane"], x["key"]))
    spans = [((x["firstDay"] * 1440 + x["firstMin"]), (x["lastDay"] * 1440 + x["lastMin"])) for x in out]
    sess_of = {}
    ndays = (date.fromisoformat(WINDOW[1]) - date.fromisoformat(WINDOW[0])).days + 1
    for i, (x, row) in enumerate(zip(out, pack_rows(spans, *pack_scale(ndays)))):
        x["row"] = row
        for pid in x.pop("_ids"):
            sess_of[pid] = i
        x.pop("_ts")
    return out, sess_of


def _session_totals(sessions: list, start: date) -> dict:
    """The session figures of the totals (sessions as in the summary, commits already counted)."""
    open_per_day: dict = {}
    for x in sessions:  # open on a day = at least one prompt that is not a slash command that day
        for d in x["_activeDays"]:
            open_per_day[d] = open_per_day.get(d, 0) + 1
    busiest = min(open_per_day, key=lambda d: (-open_per_day[d], d)) if open_per_day else 0
    most = max(sessions, key=lambda x: (x["prompts"], -x["firstDay"])) if sessions else None
    median = statistics.median(x["prompts"] for x in sessions) if sessions else 0
    return {
        "sessions": len(sessions),
        "sessionsCommandOnly": sum(1 for x in sessions if x["area"] == AREA_COMMAND_ONLY),
        "foundingSessions": sum(1 for x in sessions if x["founding"]),
        "sessionsByArea": {a: sum(1 for x in sessions if x["area"] == a) for a in AREAS},
        "busiestParallelDay": {"day": (start + timedelta(days=busiest)).isoformat(),
                               "open": open_per_day.get(busiest, 0)},
        "longestSessionPrompts": {"key": most["key"] if most else "", "prompts": most["prompts"] if most else 0,
                                  "spannedDays": most["spannedDays"] if most else 0},
        "medianPromptsPerSession": int(median) if median == int(median) else median,
    }


def _session_value(x: dict, field: str):
    """A summary session row value: area and title source as indices, the rest as is."""
    if field == "area":
        return SESSION_AREAS.index(x["area"])
    if field == "titleSource":
        return TITLE_SOURCES.index(x["titleSource"])
    return x[field]


def _lane(r: dict) -> str:
    """The timeline lane of a raw row."""
    return "codex" if r["tool"] == "codex" else f"{r['tool']}-{r['machine']}"


def counts(data: dict, log: dict, manifest: dict) -> dict:
    """The section's figures after the author's overrides, flat, for the research counts file.

    `data` is build()'s summary, `log` its log output and `manifest` the public manifest. The
    extractor's own article-work count (`article_work_extractor`) is before the overrides.
    """
    t = data["totals"]
    late = [r for r in log["rows"] if _pacific(r["ts"]) >= OTHER_FIGURES_CUTOFF]
    early = [r for r in log["rows"] if _pacific(r["ts"]) < OTHER_FIGURES_CUTOFF]
    return {
        "window_start": WINDOW[0],
        "cutoff": manifest["cutoff"],
        "prompts": t["prompts"],
        "claude_mac": t["bySource"]["claude-mac"],
        "claude_vm": t["bySource"]["claude-vm"],
        "codex": t["bySource"]["codex"],
        "article_work": t["articleWork"],
        "article_work_extractor": manifest["counts"]["article_work"]["True"],
        "median_chars": t["medianChars"],
        "decisions": t["decisions"],
        "decisions_share_pct": round(100 * t["decisions"] / t["prompts"], 1),
        "redactions": t["redactions"],
        "redacted_prompts": t["redactedPrompts"],
        "pasted_lost": t["pastedLost"],
        "settings_unknown": t["settingsUnknown"],
        "settings_inferred": t["settingsInferred"],
        "other_figures_cutoff": OTHER_FIGURES_CUTOFF.isoformat(),
        "before_cutoff_claude": sum(1 for r in early if r["tool"] == "claude"),
        "before_cutoff_codex": sum(1 for r in early if r["tool"] == "codex"),
        "before_cutoff_claude_website": sum(1 for r in early if r["tool"] == "claude" and r["website"]),
        "website_prompts": t["website"],
        "after_cutoff_prompts": len(late),
        "after_cutoff_article_work": sum(1 for r in late if r["articleWork"]),
        "after_cutoff_port_work": sum(1 for r in late if not r["articleWork"]),
        "first_prompt_pacific": min(_pacific(r["ts"]) for r in log["rows"]).isoformat(),
        "last_prompt_pacific": max(_pacific(r["ts"]) for r in log["rows"]).isoformat(),
        **{f"category_{c.replace('-', '_')}": n for c, n in t["byCategory"].items()},
        "commands_and_shell": t["commandsAndShell"],
        "prompts_substantive": t["promptsSubstantive"],
        "median_chars_substantive": t["medianCharsSubstantive"],
        "median_words_substantive": t["medianWordsSubstantive"],
        "under_ten_words_substantive": t["underTenWordsSubstantive"],
        "founding_prompts": t["foundingPrompts"],
        "founding_sessions": t["foundingSessions"],
        "sessions": t["sessions"],
        "sessions_command_only": t["sessionsCommandOnly"],
        "sessions_busiest_parallel_day": t["busiestParallelDay"]["day"],
        "sessions_busiest_parallel_open": t["busiestParallelDay"]["open"],
        "sessions_longest_prompts": t["longestSessionPrompts"]["prompts"],
        "sessions_longest_spanned_days": t["longestSessionPrompts"]["spannedDays"],
        "sessions_median_prompts": t["medianPromptsPerSession"],
        **{f"sessions_area_{re.sub(r'[^a-z]+', '_', a.lower()).strip('_')}": n
           for a, n in t["sessionsByArea"].items()},
    }
