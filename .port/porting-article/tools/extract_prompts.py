#!/usr/bin/env python3
"""Extract every human prompt of the Rustty port for the article's "Every prompt" section.

Sources (all read-only):
  - Claude Code prompt history, Mac (~/.claude/history.jsonl) and Linux VM (a copy of the VM's
    ~/.claude/history.jsonl fetched with rsync), one row per typed prompt or slash command;
  - Codex app rollout files (~/.codex/sessions/**, ~/.codex/archived_sessions/**), human turns only;
  - settings: live Claude transcripts (~/.claude/projects/*/<session>.jsonl), then the
    rustty-stats tables (sessions.csv, messages.csv.gz), then Co-Authored-By commit trailers;
  - commits: `git log --all --no-merges` of the Rustty repo, branch membership from merges.

Outputs (UNREDACTED, keep them in the untracked research data dir; never commit them):
  prompts-raw-<date>.jsonl, prompts-manifest-<date>.json, prompts-redactions-proposed-<date>.csv,
  prompts-glossary-missing-<date>.txt, prompts-article-sessions-<date>.csv,
  prompts-founding-sessions-proposed-<date>.csv, prompts-sessions-<date>.jsonl

Standard library only. See README.md next to this file for the rules and how to run it.
"""
import argparse
import collections
import csv
import datetime as dt
import glob
import gzip
import json
import os
import re
import subprocess
import sys
from zoneinfo import ZoneInfo

PACIFIC = ZoneInfo("America/Los_Angeles")
UTC = dt.timezone.utc
DATE_TAG = "2026-10-11"
WINDOW_START_DAY = "2026-06-21"  # Pacific calendar day the section's window opens
WINDOW_END_DAY = "2026-10-10"
PURGED_BEFORE = "2026-09-08"  # Mac transcripts before this day were purged (Claude Code cleanup)
WINDOW_CAP_S = 12 * 3600  # a prompt's commit window never spans more than 12 hours
CALL_SLACK_S = 2.0  # a model call this close before the prompt's history timestamp still answers it
PROJECT_MARK = "rustty"
WEBSITE_MARK = "ghostty-website"

# ---------------------------------------------------------------------------- time helpers


def parse_ts(s):
    """ISO-8601 string (Z or offset) -> epoch seconds (float)."""
    s = s.strip()
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    d = dt.datetime.fromisoformat(s)
    if d.tzinfo is None:
        d = d.replace(tzinfo=UTC)
    return d.timestamp()


def iso_utc(epoch, ms=False):
    d = dt.datetime.fromtimestamp(epoch, UTC)
    if ms:
        return d.strftime("%Y-%m-%dT%H:%M:%S.") + f"{d.microsecond // 1000:03d}Z"
    return d.strftime("%Y-%m-%dT%H:%M:%SZ")


def pacific_day(epoch):
    return dt.datetime.fromtimestamp(epoch, PACIFIC).date().isoformat()


def pacific_midnight(day):
    return dt.datetime.fromisoformat(day).replace(tzinfo=PACIFIC).timestamp()


ARTICLE_CUTOFF = pacific_midnight("2026-10-08")  # 2026-10-08T00:00-07:00
ARTICLE_RE = re.compile(r"\b(article|journey|website|porting page|disclosure)\b", re.I)

# ---------------------------------------------------------------------------- rules


def prompt_kind(text):
    """`command` for a slash command (`/clear`, `/goal …`); a path such as `/Users/…` is a prompt."""
    return "command" if re.match(r"/[A-Za-z][\w:-]*(?:\s|$)", text) else "prompt"


DECISION_WORDS = {"ok", "yes", "y", "no", "go", "ahead", "continue", "proceed", "sure", "agreed",
                  "lgtm", "a", "b", "c", "d"}
DECISION_RULE = ("After lowercasing and replacing punctuation with spaces, the prompt is a single choice "
                 "letter, or 1-3 words all drawn from {" + ", ".join(sorted(DECISION_WORDS)) + "}. "
                 "Slash commands are never decisions.")


def is_decision(text):
    """A prompt that only answers a choice or acknowledges (A, b, ok, go ahead, continue). See DECISION_RULE."""
    if prompt_kind(text.strip()) == "command":
        return False
    words = re.sub(r"[^\w\s]|_", " ", text.lower()).split()
    if len(words) == 1 and re.fullmatch(r"[a-d]", words[0]):
        return True
    return 1 <= len(words) <= 3 and all(w in DECISION_WORDS for w in words)


ARTICLE_RULE = ("A session is article work if it has a prompt on or after 2026-10-08 00:00 Pacific AND "
                "(any of its prompts was typed in a project path containing 'ghostty-website', OR any of "
                "its prompts matches \\b(article|journey|website|porting page|disclosure)\\b, case-insensitive). "
                "Only that session's prompts on or after the cutoff are tagged; earlier prompts never are.")


def article_sessions(prompts):
    """Return {session: reason} for the sessions that are article work (see ARTICLE_RULE)."""
    by = collections.defaultdict(list)
    for p in prompts:
        by[p["session"]].append(p)
    out = {}
    for s, ps in by.items():
        if not any(p["ts_epoch"] >= ARTICLE_CUTOFF for p in ps):
            continue
        if any(WEBSITE_MARK in (p.get("project") or "") for p in ps):
            out[s] = "project:" + WEBSITE_MARK
            continue
        for p in sorted(ps, key=lambda p: p["ts_epoch"]):
            m = ARTICLE_RE.search(p["text"])
            if m:
                out[s] = "keyword:" + m.group(1).lower()
                break
    return out


def is_article_work(p, sessions):
    return p["session"] in sessions and p["ts_epoch"] >= ARTICLE_CUTOFF


# ---------------------------------------------------------------------------- founding sessions

FOUNDING_FLOOR = parse_ts("2026-06-20T00:00:00Z")
FOUNDING_RULE = (
    "Founding prompts were typed before the repository existed. Candidates: Claude Code history sessions with "
    "prompts on or after 2026-06-20T00:00Z in the projects folder itself or a 'rusty*' folder before the first "
    "prompt in a 'rustty' path, or in the reference checkout (a 'ghostty*' folder that is not the website) up to "
    "the cutoff. Each candidate gets one reason: ghostty-checkout (reference checkout), port-decision (a prompt "
    "about porting and Zig, Rust or Ghostty), naming (a prompt naming the project, or the early 'rusty' folder), "
    "else other. Candidates are only proposed; prompts are kept only for sessions the author lists with "
    "decision 'include' (--founding), from 2026-06-20T00:00Z to the cutoff, whatever their folder.")
_HOME_PROJECTS = r"^(?:/Users|/home)/[^/]+/projects"
FOUNDING_ROOT_RE = re.compile(_HOME_PROJECTS + r"/?$")
FOUNDING_RUSTY_RE = re.compile(_HOME_PROJECTS + r"/rusty(?!t)[^/]*(?:/.*)?$")
FOUNDING_CHECKOUT_RE = re.compile(_HOME_PROJECTS + r"/ghostty(?!-website)[^/]*(?:/.*)?$")
PORT_RE = re.compile(r"\bport(?:ing|ed)?\b", re.I)
PORT_SUBJECT_RE = re.compile(r"\b(?:zig|rust|ghostty)\b", re.I)
NAMING_RE = re.compile(r"\b(?:rustty|rusty|name it|naming|call it)\b", re.I)
FOUNDING_FIELDS = ["session", "tool", "machine", "project", "first_ts", "last_ts", "prompts", "reason", "decision"]
FOUNDING_DECISIONS = {"include", "exclude"}


def _tilde(path):
    m = re.match(r"(?:/Users|/home)/[^/]+(/.*)?$", path or "")
    return "~" + (m.group(1) or "") if m else (path or "")


def founding_reason(rows):
    """One reason per candidate session (see FOUNDING_RULE)."""
    if any(FOUNDING_CHECKOUT_RE.match(r.get("project") or "") for r in rows):
        return "ghostty-checkout"
    texts = [r.get("display") or "" for r in rows]
    if any(PORT_RE.search(t) and PORT_SUBJECT_RE.search(t) for t in texts):
        return "port-decision"
    if any(NAMING_RE.search(t) for t in texts) or any(FOUNDING_RUSTY_RE.match(r.get("project") or "") for r in rows):
        return "naming"
    return "other"


def founding_candidates(sources, cutoff):
    """sources: [(machine, history rows)]. Returns candidate sessions (FOUNDING_FIELDS dicts, decision empty),
    ordered by first prompt. The first `rustty` prompt is taken over every source."""
    stamps = [int(r["timestamp"]) / 1000.0 for _m, rows in sources for r in rows
              if r and PROJECT_MARK in (r.get("project") or "")]
    first_rustty = min(stamps) if stamps else cutoff
    by = collections.OrderedDict()
    for machine, rows in sources:
        for r in rows:
            if not r:
                continue
            ts = int(r["timestamp"]) / 1000.0
            proj = r.get("project") or ""
            if ts < FOUNDING_FLOOR or ts >= cutoff:
                continue
            early = (FOUNDING_ROOT_RE.match(proj) or FOUNDING_RUSTY_RE.match(proj)) and ts < first_rustty
            if not (early or FOUNDING_CHECKOUT_RE.match(proj)):
                continue
            by.setdefault((machine, r.get("sessionId") or ""), []).append((ts, r))
    out = []
    for (machine, sid), items in by.items():
        items.sort(key=lambda x: x[0])
        rows = [r for _t, r in items]
        out.append(dict(session=sid, tool="claude", machine=machine,
                        project="|".join(sorted({_tilde(r.get("project")) for r in rows})),
                        first_ts=iso_utc(items[0][0]), last_ts=iso_utc(items[-1][0]), prompts=len(rows),
                        reason=founding_reason(rows), decision=""))
    out.sort(key=lambda c: (c["first_ts"], c["machine"], c["session"]))
    return out


def parse_founding(lines):
    """Approved founding-sessions CSV (FOUNDING_FIELDS) -> {(tool, machine, session)} with decision `include`.
    Refuses a row without a decision (unreviewed) or with an unknown one."""
    out = set()
    for r in csv.DictReader(lines):
        d = (r.get("decision") or "").strip().lower()
        if d not in FOUNDING_DECISIONS:
            raise SystemExit(f"founding list: session {r.get('session')!r} has no valid decision ({d!r}); "
                             f"expected one of {sorted(FOUNDING_DECISIONS)}")
        if d == "include":
            out.add((r["tool"].strip(), r["machine"].strip(), r["session"].strip()))
    return out


def load_founding(path):
    with open(path, newline="") as f:
        return parse_founding(f)


# ---------------------------------------------------------------------------- sessions

SESSION_RULE = ("A session is (tool, machine, session id). Per session: first and last prompt, prompt count, "
                "commands_only (every prompt is a slash command), founding and article_work (any of its prompts), "
                "branches and models (union, where known), commits (union of its prompts' links after the dedupe) "
                "and commits_ambiguous (its prompts left with basis 'ambiguous'). cc_title: Claude Code's own "
                "title from rustty-stats sessions.csv (ai_title, else title), Claude sessions only.")


def titles_from_rows(rows):
    """rustty-stats sessions.csv rows -> {(stats machine, session id): title}; `ai_title` preferred, else
    `title`; Claude sessions only."""
    out = {}
    for r in rows:
        if r.get("tool") != "claude":
            continue
        t = (r.get("ai_title") or "").strip() or (r.get("title") or "").strip()
        if t:
            out[(r["machine"], r["session_id"])] = t
    return out


def load_titles(stats_dir):
    with open(os.path.join(stats_dir, "sessions.csv"), newline="") as f:
        return titles_from_rows(csv.DictReader(f))


STAT_MACHINE = {"mac": "mac", "vm": "rvm"}


def session_aggregates(prompts, titles=None):
    """One object per (tool, machine, session) (see SESSION_RULE), ordered by first prompt. Keys are
    `<tool>-<machine>-<id prefix>`, the prefix 8 characters long or longer when needed to stay unique."""
    titles = titles or {}
    by = collections.defaultdict(list)
    for p in prompts:
        by[(p["tool"], p["machine"], p["session"])].append(p)
    out = []
    for (tool, machine, sid), ps in by.items():
        ps.sort(key=lambda p: (p["ts_epoch"], p.get("order", 0)))
        if len({bool(p.get("founding")) for p in ps}) > 1:
            raise SystemExit(f"session {tool}-{machine}-{sid} mixes founding and non-founding prompts")
        commits = []
        for p in ps:
            for sha in p.get("commits") or []:
                if sha not in commits:
                    commits.append(sha)
        out.append(dict(
            session=sid, key=None, tool=tool, machine=machine,
            first_ts=iso_utc(ps[0]["ts_epoch"]), last_ts=iso_utc(ps[-1]["ts_epoch"]), prompts=len(ps),
            commands_only=all(p["kind"] == "command" for p in ps),
            founding=bool(ps[0].get("founding")), article_work=any(p.get("article_work") for p in ps),
            branches=sorted({p["branch"] for p in ps if p.get("branch")}),
            models=sorted({p["model"] for p in ps if p.get("model")}),
            commits=commits, commits_ambiguous=sum(1 for p in ps if p.get("commits_basis") == "ambiguous"),
            cc_title=titles.get((STAT_MACHINE.get(machine, machine), sid)) if tool == "claude" else None))
    groups = collections.defaultdict(list)
    for s in out:
        groups[(s["tool"], s["machine"])].append(s)
    for (tool, machine), ss in groups.items():
        for s in ss:
            n = 8
            while n < len(s["session"]) and any(o is not s and o["session"][:n] == s["session"][:n] for o in ss):
                n += 1
            s["key"] = f"{tool}-{machine}-{s['session'][:n]}"
    out.sort(key=lambda s: (s["first_ts"], s["tool"], s["machine"], s["session"]))
    return out


# Internal ids. ID_PATTERN and PROCESS_LABELS are copied verbatim from the website repo's
# scripts/porting/jargon.py (find_internal_ids), ghostty-website-journey commit 438cb814,
# so the article and this extractor agree on what an id is. One extension, marked below:
# the FX<n> family (fix ids seen in prompts), which jargon.py does not list yet.
ID_PATTERN = re.compile(
    r"\b(?:UD\d+|N\d{1,3}|U-\w+|R\d{1,2}[a-z]?|D\d[a-z]?|G\d+[a-z]?(?:-SELF)?|G-[A-Z]+|"
    r"A\d|P\d+-W\d+[a-z]?|W\d+\.T\d+[a-z]?|T\d+[a-z])\b"
)
ID_PATTERN_EXTRA = re.compile(r"\bFX\d+\b")  # extension, not in jargon.py
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


def find_internal_ids(text):
    """Distinct ids/process labels in order of first appearance (jargon.py find_internal_ids + FX)."""
    hits = [(m.start(), m.group(0), "id") for m in ID_PATTERN.finditer(text)]
    hits += [(m.start(), m.group(0), "id") for m in ID_PATTERN_EXTRA.finditer(text)]
    for pat, _rep in PROCESS_LABELS:
        hits += [(m.start(), m.group(0), "label") for m in re.finditer(pat, text)]
    out = []
    for _pos, s, _fam in sorted(hits):
        if s not in out:
            out.append(s)
    return out


def id_family(s):
    return "id" if (ID_PATTERN.fullmatch(s) or ID_PATTERN_EXTRA.fullmatch(s)) else "process-label"


def norm_id(s):
    return re.sub(r"[\s_-]+", "-", s.strip().lower())


def glossary_ids(md):
    """Ids named in the first column of the glossary's markdown tables."""
    out = set()
    for line in md.splitlines():
        if not line.startswith("|") or re.match(r"\|\s*-", line):
            continue
        cell = line.split("|")[1].strip().replace("**", "").replace("`", "")
        cell = cell.split("⚠")[0].strip()
        cell = re.sub(r"\s*\([^)]*\)\s*$", "", cell).strip()
        if not cell or cell.lower() == "id":
            continue
        out.add(cell)
        out.add(cell.split()[0])
    return out


def phase_for(day, phases, founding=False):
    """phases: sorted [(start_day, key)]; the key of the last phase starting on or before day. A founding
    prompt dated before the first phase starts takes the first phase."""
    cur = phases[0][1] if founding and phases else None
    for start, key in phases:
        if start <= day:
            cur = key
        else:
            break
    return cur


def load_phases(path):
    """Contiguous `era` runs of the strategy timeline. `external` rows (an outside event, not a
    working phase) are skipped."""
    rows = sorted(csv.DictReader(open(path, newline="")), key=lambda r: r["date"])
    out = []
    for r in rows:
        if r["era"] == "external":
            continue
        if not out or out[-1][1] != r["era"]:
            out.append((r["date"], r["era"]))
    return out


# ---------------------------------------------------------------------------- pasted text

PASTE_RE = re.compile(r"\[Pasted text #(\d+)")


def pasted_flag(display, pasted_contents, cache_hashes=frozenset()):
    """none: no placeholder; kept: every referenced paste body survives (inline or in the paste
    cache); lost: at least one does not. The text itself is never expanded."""
    refs = PASTE_RE.findall(display)
    if not refs:
        return "none"
    pc = pasted_contents or {}
    for r in refs:
        e = pc.get(r) or pc.get(int(r)) if isinstance(pc, dict) else None
        if not e:
            return "lost"
        if e.get("content"):
            continue
        if e.get("contentHash") and e["contentHash"] in cache_hashes:
            continue
        return "lost"
    return "kept"


# ---------------------------------------------------------------------------- sources


class SourceResult:
    def __init__(self):
        self.prompts = []
        self.dropped = collections.Counter()
        self.in_source = 0
        self.last = {}  # session -> last recorded activity (Codex)

    def drop(self, reason):
        self.dropped[reason] += 1


def _in_project(path, ts):
    path = path or ""
    return PROJECT_MARK in path or (WEBSITE_MARK in path and ts >= ARTICLE_CUTOFF)


def history_prompts(rows, machine, window, cache_hashes=frozenset(), founding=frozenset()):
    """Claude Code history rows -> prompts. `rows` may hold None for unparseable lines.

    founding: approved founding sessions {(tool, machine, session)} (see FOUNDING_RULE); their prompts
    are kept from FOUNDING_FLOOR on, whatever their folder, and flagged `founding`."""
    res = SourceResult()
    start, end = window
    for i, r in enumerate(rows):
        res.in_source += 1
        if r is None:
            res.drop("unparseable")
            continue
        ts = int(r["timestamp"]) / 1000.0
        is_founding = ("claude", machine, r.get("sessionId")) in founding and FOUNDING_FLOOR <= ts < end
        if not is_founding:
            if ts < start:
                res.drop("before-window")
                continue
            if ts >= end:
                res.drop("after-cutoff")
                continue
            if not _in_project(r.get("project"), ts):
                res.drop("other-project")
                continue
        text = r.get("display") or ""
        if not text.strip():
            res.drop("empty")
            continue
        res.prompts.append(dict(
            tool="claude", machine=machine, session=r.get("sessionId") or f"nosession-{machine}-{i}",
            ts_epoch=ts, order=i, text=text, chars=len(text), kind=prompt_kind(text),
            pasted=pasted_flag(text, r.get("pastedContents"), cache_hashes), project=r.get("project"),
            model=None, effort=None, mode=None, settings_source="unknown", approval=None, sandbox=None,
            branch=None, founding=is_founding))
    res.dropped = dict(res.dropped)
    return res


INJECT_TAGS = ("recommended_plugins", "environment_context", "codex_internal_context", "subagent_notification",
               "user_instructions", "permissions instructions", "turn_aborted", "user_shell_command",
               "collaboration_mode", "skill", "INSTRUCTIONS")
INJECT_WRAPPED = re.compile(r"^\s*<([A-Za-z_][\w-]*)(?:\s[^>]*)?>[\s\S]*</\1>\s*$")


def is_injected(text):
    t = text.lstrip()
    if t.startswith("# AGENTS.md instructions") or t.startswith("<INSTRUCTIONS>"):
        return True
    if any(t.startswith("<" + tag) for tag in INJECT_TAGS):
        return True
    return bool(INJECT_WRAPPED.match(t))


def codex_mode(approval, sandbox, collab):
    if collab == "plan":
        return "plan"
    if approval == "never" and sandbox == "danger-full-access":
        return "bypass"
    return "default"


def codex_prompts(sessions, window):
    """Codex rollout files (each a list of decoded records) -> human turns.

    A source unit is a user-role `message` item (`response_item` records). Tool results are
    `function_call_output` items, not user messages, so they are not units at all."""
    res = SourceResult()
    start, end = window
    seen = set()
    for recs in sessions:
        meta = next((r.get("payload") or {} for r in recs if r.get("type") == "session_meta"), {})
        src = meta.get("source")
        models = {(r.get("payload") or {}).get("model") for r in recs if r.get("type") == "turn_context"}
        auto = ("codex-auto-review" in models or meta.get("thread_source") == "guardian_review"
                or (isinstance(src, dict) and "guardian" in json.dumps(src.get("subagent", ""))))
        sub = (isinstance(src, dict) and "subagent" in src) or meta.get("thread_source") == "subagent"
        cwd = meta.get("cwd") or ""
        branch = (meta.get("git") or {}).get("branch")
        sid = meta.get("id") or meta.get("session_id") or ""
        ctx = None
        waiting = []
        stamps = [r["timestamp"] for r in recs if r.get("timestamp")]
        if sid and stamps:
            res.last[sid] = max(res.last.get(sid, 0), parse_ts(max(stamps)))
        for r in recs:
            p = r.get("payload") or {}
            if r.get("type") == "turn_context":
                sb = p.get("sandbox_policy")
                cm = p.get("collaboration_mode")
                ctx = dict(model=p.get("model"),
                           effort=p.get("effort") or (p.get("reasoning") or {}).get("effort") or None,
                           approval=p.get("approval_policy") if isinstance(p.get("approval_policy"), str)
                           else json.dumps(p.get("approval_policy")),
                           sandbox=sb.get("type") if isinstance(sb, dict) else sb,
                           collab=cm.get("mode") if isinstance(cm, dict) else cm)
                for w in waiting:
                    _apply_codex_ctx(w, ctx)
                waiting = []
                continue
            if not (r.get("type") == "response_item" and p.get("type") == "message" and p.get("role") == "user"):
                continue
            res.in_source += 1
            ts = parse_ts(r["timestamp"]) if r.get("timestamp") else None
            if auto:
                res.drop("auto-review")
                continue
            if sub:
                res.drop("subagent")
                continue
            if ts is None:
                res.drop("unparseable")
                continue
            if not _in_project(cwd, ts):
                res.drop("other-project")
                continue
            if ts < start:
                res.drop("before-window")
                continue
            if ts >= end:
                res.drop("after-cutoff")
                continue
            blocks = [b.get("text") or "" for b in (p.get("content") or [])
                      if isinstance(b, dict) and b.get("type") in ("input_text", "text")]
            human = [b for b in blocks if b.strip() and not is_injected(b)]
            if not human:
                res.drop("injected-context" if blocks else "empty")
                continue
            text = "\n".join(human)
            key = (r.get("timestamp"), text)
            if key in seen:
                res.drop("duplicate")
                continue
            seen.add(key)
            pr = dict(tool="codex", machine="mac", session=sid, ts_epoch=ts, order=res.in_source, text=text,
                      chars=len(text), kind=prompt_kind(text), pasted="none", project=cwd, founding=False,
                      model=None, effort=None, mode=None, settings_source="unknown", approval=None,
                      sandbox=None, branch=branch if branch and branch != "HEAD" else None)
            if ctx:
                _apply_codex_ctx(pr, ctx)
            else:
                waiting.append(pr)
            res.prompts.append(pr)
    res.dropped = dict(res.dropped)
    return res


def _apply_codex_ctx(pr, ctx):
    pr.update(model=ctx["model"], effort=ctx["effort"], approval=ctx["approval"], sandbox=ctx["sandbox"],
              mode=codex_mode(ctx["approval"], ctx["sandbox"], ctx["collab"]),
              settings_source="transcript" if ctx["model"] else "unknown")


def source_summary(results):
    return {name: {"in_source": r.in_source, "kept": len(r.prompts), "dropped": dict(sorted(r.dropped.items()))}
            for name, r in results.items()}


def check_reconciles(sources):
    for name, s in sources.items():
        if s["kept"] + sum(s["dropped"].values()) != s["in_source"]:
            raise SystemExit(f"manifest does not reconcile for {name}: {s}")


# ---------------------------------------------------------------------------- settings

MODE_MAP = {"bypassPermissions": "bypass", "plan": "plan", "default": "default", "acceptEdits": "accept-edits",
            "auto": "auto", "dontAsk": "dont-ask"}


def norm_mode(m):
    if not m:
        return None
    return MODE_MAP.get(m, m)


def trailer_model(coauthor):
    """'Claude Opus 4.8 (1M context) <noreply@anthropic.com>' -> 'claude-opus-4-8'."""
    m = re.match(r"\s*Claude ([A-Z][a-z]+) (\d+(?:\.\d+)*)", coauthor or "")
    if not m:
        return None
    return f"claude-{m.group(1).lower()}-{m.group(2).replace('.', '-')}"


def _single(field):
    vals = [v for v in (field or "").split("|") if v]
    return vals[0] if len(vals) == 1 else None


def join_settings(p, end, tx, stats_calls, stats_session, trailer_models):
    """Model/effort/mode for a Claude prompt, best evidence first:
    transcript call answering the prompt (live transcript, else rustty-stats per-call rows) ->
    session-level single model (rustty-stats) -> Co-Authored-By trailer of its commits -> unknown."""
    ts = p["ts_epoch"]
    out = dict(model=None, effort=None, mode=None, settings_source="unknown")
    calls = (tx or {}).get("calls") or stats_calls or []
    perms = (tx or {}).get("perms") or []
    mode = None
    before = [m for t, m in perms if t <= ts + 5]
    if before:
        mode = norm_mode(before[-1])
    elif stats_session:
        mode = norm_mode(stats_session.get("permission_mode"))
    hit = next((c for c in calls if ts - CALL_SLACK_S <= c[0] < end), None)
    if hit is None and calls:
        prior = [c for c in calls if c[0] < ts]
        hit = prior[-1] if prior else None
    if hit is not None:
        out.update(model=hit[1] or None, effort=hit[2] or None, mode=mode, settings_source="transcript")
        return out
    if stats_session and _single(stats_session.get("models")):
        out.update(model=_single(stats_session.get("models")), effort=_single(stats_session.get("efforts")),
                   mode=mode, settings_source="session")
        return out
    if trailer_models:
        model = collections.Counter(trailer_models).most_common(1)[0][0]
        out.update(model=model, settings_source="commit-trailer", mode=mode)
        return out
    out["mode"] = mode
    return out


# ---------------------------------------------------------------------------- commits

COMMITS_RULE = ("A prompt's window runs from the prompt to the next human prompt of the same session (for the "
                "session's last prompt: to the session's last recorded activity, or 12 h when unknown), capped "
                "at 12 h and never past the extraction cutoff. Commits authored in the window are linked as follows. A commit is never linked when "
                "its Claude-Session trailer, or the `[branch sha]` git output in a surviving transcript, ties it "
                "to other human sessions only. session-window: the session's git branch is known (transcript, "
                "rustty-stats or Codex metadata) and the commit either shows up in this session's own "
                "transcripts (main, subagent or workflow files) or landed on that branch (made on it, merged "
                "into it through a chain of merges, or reachable from its current tip). time-window-branch: the "
                "branch is unknown and every commit in the window carries the same branch label. none: "
                "otherwise. Branch labels come from the `[branch sha]` lines git prints on commit (surviving "
                "transcripts), else from the oldest merge that brought the commit in, else from the local "
                "branch that reaches it.")


def window_end(start, next_ts, last_ts, cutoff=None):
    if next_ts is not None:
        e = next_ts
    elif last_ts is not None:
        e = max(start, last_ts)
    else:
        e = start + WINDOW_CAP_S
    e = min(e, start + WINDOW_CAP_S)
    return min(e, cutoff) if cutoff is not None else e


def commit_window(start, end, branch, commits, foreign_cloud=frozenset(), session=None, cutoff=None):
    """commits: [{sha, ts, label, on: set(branch names), cloud: set(cloud session ids),
    sessions: set(human sessions whose transcripts show the commit)}]. The window never runs past `cutoff`."""
    if cutoff is not None:
        end = min(end, cutoff)
    win = []
    for c in commits:
        if not (start <= c["ts"] < end):
            continue
        if c["cloud"] & foreign_cloud and not (c["cloud"] - foreign_cloud):
            continue
        if c.get("sessions") and session not in c["sessions"]:
            continue
        win.append(c)
    if not win:
        return [], "none"
    if branch:
        hits = [c["sha"] for c in win if branch in c["on"] or (session and session in c.get("sessions", ()))]
        return (hits, "session-window") if hits else ([], "none")
    labels = {c["label"] for c in win}
    if len(labels) == 1 and None not in labels:
        return [c["sha"] for c in win], "time-window-branch"
    return [], "none"


DEDUPE_RULE = ("After linking, a commit linked from prompts of two or more human sessions keeps only the links "
               "backed by evidence: the commit's `[branch sha]` git output appears in that session's own transcripts, "
               "or its Claude-Session trailer names a cloud session bridged from that session. Other links to it are "
               "removed; a prompt left with no commits gets basis `ambiguous`.")


def dedupe_links(prompts, by_sha, owned_clouds):
    """Apply DEDUPE_RULE in place. owned_clouds: {session: set(cloud ids bridged from it)}.
    Returns counts {contested_commits, links_removed, prompts_ambiguous}."""
    claimed = collections.defaultdict(set)
    for p in prompts:
        for sha in p.get("commits") or []:
            claimed[sha].add(p["session"])
    contested = {sha for sha, ss in claimed.items() if len(ss) >= 2}
    removed = ambiguous = 0
    for p in prompts:
        if not p.get("commits"):
            continue
        keep = []
        for sha in p["commits"]:
            c = by_sha[sha]
            if sha not in contested or p["session"] in c.get("sessions", ()) or \
                    (c.get("cloud", set()) & owned_clouds.get(p["session"], set())):
                keep.append(sha)
            else:
                removed += 1
        if not keep:
            p["commits_basis"] = "ambiguous"
            ambiguous += 1
        p["commits"] = keep
    return {"contested_commits": len(contested), "links_removed": removed, "prompts_ambiguous": ambiguous}


def merge_subject_branch(subject):
    m = re.match(r"Merge (?:remote-tracking )?branch '([^']+)'", subject)
    if m:
        return m.group(1)
    m = re.match(r"Merge ([\w.]+(?:[/-][\w.]+)+)\b", subject)
    if m:
        return m.group(1)
    return None


def _git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True, check=True).stdout


def load_commits(repo):
    fmt = ("%H%x09%aI%x09%D%x09%(trailers:key=Claude-Session,valueonly,separator=%x2C)"
           "%x09%(trailers:key=Co-Authored-By,valueonly,separator=%x2C)")
    out = []
    for line in _git(repo, "log", "--all", "--no-merges", f"--format={fmt}").splitlines():
        parts = line.split("\t")
        if len(parts) < 5:
            continue
        sha, date, _refs, sess, coauth = parts[:5]
        cloud = set(re.findall(r"(session_\w+)", sess))
        models = [m for m in (trailer_model(a) for a in coauth.split(",")) if m]
        out.append(dict(sha=sha, ts=parse_ts(date), cloud=cloud, models=models, label=None, on=set()))
    return out


def branch_info(repo, commits, tx_branch=None):
    """Label each commit with the branch it was made on (main first-parent line, else the branch
    named by the oldest merge that brought it in, else the local branch that reaches it) and the
    set of branches it landed on."""
    label = {}
    for sha in _git(repo, "rev-list", "--first-parent", "main").split():
        label[sha] = "main"
    merges = []
    for line in _git(repo, "log", "--all", "--merges", "--format=%H%x09%P%x09%aI%x09%s").splitlines():
        h, parents, date, subj = line.split("\t", 3)
        ps = parents.split()
        if len(ps) >= 2:
            merges.append((parse_ts(date), h, ps, subj))
    merges.sort()
    first_merge = {}
    for _t, h, ps, subj in merges:
        name = merge_subject_branch(subj) or f"merge:{h[:9]}"
        assigned = 0
        for sha in _git(repo, "rev-list", ps[1], "^" + ps[0]).split():
            if sha not in label:
                label[sha] = name
                assigned += 1
        if assigned and name not in first_merge:
            first_merge[name] = h
    reach = {}
    for ref in _git(repo, "for-each-ref", "--format=%(refname:short)", "refs/heads").split():
        reach[ref] = set(_git(repo, "rev-list", ref).split())
        for sha in reach[ref]:
            label.setdefault(sha, ref)
    # rename merge-derived labels after the branch git printed for their commits, when transcripts survive
    tx_branch = tx_branch or {}
    votes = collections.defaultdict(collections.Counter)
    for sha, b in tx_branch.items():
        if sha in label:
            votes[label[sha]][b] += 1
    alias = {lab: v.most_common(1)[0][0] for lab, v in votes.items() if lab != "main"}
    for sha in list(label):
        label[sha] = tx_branch.get(sha) or alias.get(label[sha], label[sha])
    first_merge = {alias.get(n, n): h for n, h in first_merge.items()}
    parent = {name: label.get(h) for name, h in first_merge.items() if label.get(h) and label.get(h) != name}

    def chain(name):
        out, cur = [], name
        while cur and cur not in out:
            out.append(cur)
            cur = parent.get(cur)
        return out

    for c in commits:
        c["label"] = label.get(c["sha"])
        c["on"] = set(chain(c["label"])) | {ref for ref, s in reach.items() if c["sha"] in s}
    return commits


# ---------------------------------------------------------------------------- transcripts / stats


def load_claude_transcripts(claude_dir, sessions):
    """Per session: model calls [(ts, model, effort)], permission modes [(ts, mode)], branches
    [(ts, branch)], last activity."""
    out = {}
    for path in glob.glob(os.path.join(claude_dir, "projects", "*", "*.jsonl")):
        sid = os.path.basename(path)[:-6]
        if sid not in sessions:
            continue
        d = out.setdefault(sid, dict(calls=[], perms=[], branches=[], last=None))
        seen = set()
        for line in open(path, errors="replace"):
            try:
                j = json.loads(line)
            except ValueError:
                continue
            t = j.get("type")
            ts = parse_ts(j["timestamp"]) if j.get("timestamp") else None
            if ts is not None:
                d["last"] = max(d["last"] or ts, ts)
            if t == "assistant" and ts is not None and not j.get("isSidechain"):
                m = j.get("message") or {}
                if m.get("model") in (None, "<synthetic>"):
                    continue
                key = (m.get("id"), j.get("requestId"))
                if key in seen:
                    continue
                seen.add(key)
                d["calls"].append((ts, m.get("model"), j.get("perTurnEffort") or j.get("effort") or None))
            elif t == "user" and ts is not None and not j.get("isSidechain"):
                if j.get("permissionMode"):
                    d["perms"].append((ts, j["permissionMode"]))
                if j.get("gitBranch"):
                    d["branches"].append((ts, j["gitBranch"]))
        for k in ("calls", "perms", "branches"):
            d[k].sort()
    return out


def load_bridge_owners(claude_dir):
    """Claude-Session cloud id (session_X) -> set of local session ids, from bridge-session records
    (one cloud session can be bridged from several local sessions)."""
    owners = collections.defaultdict(set)
    for path in glob.glob(os.path.join(claude_dir, "projects", "**", "*.jsonl"), recursive=True):
        if "/subagents/" in path:
            continue
        for line in open(path, errors="replace"):
            if line.startswith('{"type":"bridge-session"'):
                try:
                    j = json.loads(line)
                except ValueError:
                    continue
                owners[j["bridgeSessionId"].replace("cse_", "session_")].add(j["sessionId"])
    return owners


COMMIT_LINE = re.compile(rb"\[([\w./+-]+)(?: \(root-commit\))? ([0-9a-f]{7,40})\] ")


def load_transcript_commits(claude_dir, commits):
    """Scan every Rustty transcript (main, subagent and workflow files) for the `[branch sha] subject`
    line git prints on commit. Returns ({sha: branch}, {sha: set(owning top-level session ids)})."""
    by_prefix = collections.defaultdict(list)
    for c in commits:
        by_prefix[c["sha"][:7]].append(c["sha"])
    branch, owners = {}, collections.defaultdict(set)
    root = os.path.join(claude_dir, "projects")
    for d in sorted(os.listdir(root)) if os.path.isdir(root) else []:
        if PROJECT_MARK not in d:
            continue
        top = os.path.join(root, d)
        for dp, _dn, fn in os.walk(top):
            for f in fn:
                if not f.endswith(".jsonl"):
                    continue
                rel = os.path.relpath(os.path.join(dp, f), top).split(os.sep)
                owner = rel[0][:-6] if len(rel) == 1 else rel[0]
                data = open(os.path.join(dp, f), "rb").read()
                for m in COMMIT_LINE.finditer(data):
                    h = m.group(2).decode()
                    full = [x for x in by_prefix.get(h[:7], ()) if x.startswith(h)]
                    if len(full) == 1:
                        branch.setdefault(full[0], m.group(1).decode())
                        owners[full[0]].add(owner)
    return branch, owners


def load_stats(stats_dir, keys):
    """rustty-stats sessions.csv rows and per-call (ts, model, effort) for (machine, session) keys."""
    sess, calls = {}, collections.defaultdict(list)
    for r in csv.DictReader(open(os.path.join(stats_dir, "sessions.csv"), newline="")):
        k = (r["machine"], r["session_id"])
        if k in keys:
            sess[k] = r
    with gzip.open(os.path.join(stats_dir, "messages.csv.gz"), "rt", newline="") as f:
        for r in csv.DictReader(f):
            k = (r["machine"], r["session_id"])
            if k in keys and r["tool"] == "claude" and r["is_subagent"] == "0" and r["ts"]:
                calls[k].append((parse_ts(r["ts"]), r["model"], r["effort"] or None))
    for v in calls.values():
        v.sort()
    return sess, calls


# ---------------------------------------------------------------------------- redaction scanner

_COMMON_AFTER_CREDENTIAL = (
    "is|was|the|a|an|and|or|to|for|of|in|on|at|with|field|fields|manager|prompt|prompts|reset|change|changed|"
    "protected|less|auth|authentication|login|output|outputs|command|shows|showed|prints|returns|returned|"
    "should|would|will|can|could|must|may|be|not|here|there|that|this|it|from|as|by|so|if|when|then|"
    "directory|dir|path|cwd|again|first|only|also|still|now|set|setup|required|needed|works|worked|ok")
_NOT_COMMON = r"(?!(?:" + _COMMON_AFTER_CREDENTIAL + r")\b)(?![.,;:!?)\]`'\"])"

SECRET_PATTERNS = [
    r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{16,}",
    r"\bgh[pousr]_[A-Za-z0-9]{20,}",
    r"\bgithub_pat_[A-Za-z0-9_]{20,}",
    r"\bxox[abposr]-[A-Za-z0-9-]{10,}",
    r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
    r"-----BEGIN[A-Z0-9 ]*-----",
    # credential words, with or without a separator ("password is X", "pwd X", "pw: X"); the value may not be
    # a common word, so "password field", "pwd output", "the password manager" stay quiet
    r"(?i)\b(?:password|passwd|passw|passphrase|pwd|pw)\b(?:\s*(?:is|=|:)\s*|\s+)" + _NOT_COMMON + r"[^\s,;]+",
    # bare "pass" only with an explicit separator ("pass: X", "pass=X"), never "first pass", "pass the …"
    r"(?i)\bpass\s*[:=]\s*[^\s,;]+",
    # "login user/pass", "creds: user/pass", "user/pass: a/b"
    r"(?i)\b(?:login|logins?|creds?|credentials|user(?:name)?/pass(?:word)?)\b\s*(?:is|=|:)?\s*[^\s/,;]+/[^\s,;]+",
    r"(?i)\b(?:api[ _-]?key|access[ _-]?token|auth[ _-]?token|secret[ _-]?key|token|secret)\s*(?:is|=|:)\s*"
    r"[A-Za-z0-9_./+=-]{8,}",
    r"\bBearer\s+[A-Za-z0-9._~+/=-]{16,}",
    r"\bsshpass\s+-p\s*\S+",
]
EMAIL_RE = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
EMAIL_ALWAYS_OK = {"noreply@anthropic.com"}
PHONE_RE = re.compile(r"(?<![\w.+/-])(?:\+\d{1,3}[\s.-]?)?(?:\(\d{3}\)|\d{3})[\s.-]\d{3}[\s.-]\d{4}(?![\w.-])")
PATH_RE = re.compile(r"(?:/Users|/home)/[A-Za-z0-9._-]+(?:/[^\s'\"`)\]>,;]*)?|~/[^\s'\"`)\]>,;]+")
PATH_OK = re.compile(r"^(?:projects/(?:rustty|ghostty)[^/]*(?:/.*)?|projects/?|"
                     r"\.claude/projects/-(?:Users|home)-[^/]*-projects-(?:rustty|ghostty)[^/]*(?:/.*)?)$")
IPV4_RE = re.compile(r"(?<![\w.])(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?![\w.]*\d)")
# user@host / user@IP that is not an email address (no alphabetic TLD), e.g. ssh targets, shell prompts
USER_HOST_RE = re.compile(r"(?<![\w.@/-])[A-Za-z_][\w.-]*@(?:[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*)(?::[~/][^\s'\"`]*)?")
REASON_PRIORITY = {"secret": 0, "email": 1, "user-host": 2, "ip": 3, "phone": 4, "unrelated-path": 5, "person": 6,
                   "unrelated-project": 7}


def _path_ok(path):
    m = re.match(r"(?:/Users|/home)/[^/]+/?(.*)$", path)
    rel = m.group(1) if m else path[2:]
    rel = rel.rstrip(".:")
    if m and not rel:
        return True  # a bare home directory names nothing
    return bool(PATH_OK.match(rel))


def scan_redactions(text, people=(), allowlist=(), projects=()):
    """Candidate spans to redact: [{start, end, match, reason}], non-overlapping, in order."""
    hits = []
    for pat in SECRET_PATTERNS:
        for m in re.finditer(pat, text):
            s = m.group(0).rstrip(".:)'\"`")
            hits.append((m.start(), m.start() + len(s), "secret"))
    emails = []
    for m in EMAIL_RE.finditer(text):
        emails.append((m.start(), m.end()))
        if m.group(0).lower() not in EMAIL_ALWAYS_OK:
            hits.append((m.start(), m.end(), "email"))
    for m in USER_HOST_RE.finditer(text):
        if any(a <= m.start() < b for a, b in emails):
            continue
        s = m.group(0).rstrip(".,:)")
        host = re.split(r"[:]", s.split("@", 1)[1])[0]
        if not re.search(r"[A-Za-z]", host) and not IPV4_RE.fullmatch(host):
            continue  # package@version, not a login
        hits.append((m.start(), m.start() + len(s), "user-host"))
    hits += [(m.start(), m.end(), "ip") for m in IPV4_RE.finditer(text)]
    hits += [(m.start(), m.end(), "phone") for m in PHONE_RE.finditer(text)]
    for m in PATH_RE.finditer(text):
        s = m.group(0).rstrip(".,:")
        if not _path_ok(s):
            hits.append((m.start(), m.start() + len(s), "unrelated-path"))
    for name in people:
        hits += [(m.start(), m.end(), "person") for m in re.finditer(r"\b" + re.escape(name) + r"\b", text)]
    for name in projects:
        hits += [(m.start(), m.end(), "unrelated-project")
                 for m in re.finditer(r"(?<![\w/.-])" + re.escape(name) + r"(?![\w-])", text, re.I)]
    safe = []
    low = text.lower()
    for phrase in allowlist:
        pl = phrase.lower()
        i = low.find(pl)
        while i >= 0:
            safe.append((i, i + len(pl)))
            i = low.find(pl, i + 1)
    hits = [h for h in hits if not any(a <= h[0] and h[1] <= b for a, b in safe)]
    hits.sort(key=lambda h: (h[0], REASON_PRIORITY[h[2]], -(h[1] - h[0])))
    out, last = [], -1
    for s, e, reason in hits:
        if s < last:
            continue
        out.append(dict(start=s, end=e, match=text[s:e], reason=reason))
        last = e
    return out


PUBLIC_LEAKS = re.compile(r"/Users/|/home/|~/|" + IPV4_RE.pattern)


def check_public_manifest(manifest):
    """The manifest is published: counts and rules only, no home paths or IP addresses."""
    blob = json.dumps(manifest, ensure_ascii=False)
    m = PUBLIC_LEAKS.search(blob)
    if m:
        raise SystemExit(f"manifest would publish a private detail: {blob[max(0, m.start() - 40):m.end() + 40]!r}")


def read_list(path):
    if not path or not os.path.exists(path):
        return []
    out = []
    for line in open(path):
        line = line.split("#", 1)[0].strip() if not line.lstrip().startswith("#") else ""
        if line:
            out.append(line)
    return out


OTHER_PROJECT_STOP = {"projects", "documents", "gael", "spaces", "cube", "testapp", "rusty", "desktop", "downloads",
                      "skills"}


def other_project_names(history_rows):
    """Leaf names of the non-Rustty projects in the prompt history (common words excluded)."""
    names = set()
    for r in history_rows:
        if not r:
            continue
        proj = r.get("project") or ""
        if PROJECT_MARK in proj or "ghostty" in proj:
            continue
        leaf = os.path.basename(proj.rstrip("/"))
        if len(leaf) >= 4 and leaf.lower() not in OTHER_PROJECT_STOP:
            names.add(leaf)
    return sorted(names)


# ---------------------------------------------------------------------------- ids


def assign_ids(prompts):
    """`<tool>-<machine>-<UTC second>-<n>`; n counts prompts of that tool/machine in that second."""
    prompts.sort(key=lambda p: (p["tool"], p["machine"], p["ts_epoch"], p["order"]))
    count = collections.Counter()
    for p in prompts:
        sec = dt.datetime.fromtimestamp(p["ts_epoch"], UTC).strftime("%Y%m%dT%H%M%SZ")
        key = (p["tool"], p["machine"], sec)
        p["id"] = f"{p['tool']}-{p['machine']}-{sec}-{count[key]}"
        count[key] += 1
        p["ts"] = iso_utc(p["ts_epoch"])
    return prompts


# ---------------------------------------------------------------------------- main


def read_jsonl(path):
    rows = []
    for line in open(path, errors="replace"):
        if not line.strip():
            continue
        try:
            rows.append(json.loads(line))
        except ValueError:
            rows.append(None)
    return rows


def main(argv=None):
    home = os.path.expanduser("~")
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--research", default=os.path.join(home, "projects/rustty/.port/journey-article/research"))
    ap.add_argument("--rustty-repo", default=os.path.join(home, "projects/rustty"))
    ap.add_argument("--stats", default=os.path.join(home, "projects/rustty-stats/data"))
    ap.add_argument("--vm-history", default=None,
                    help="copy of the VM's ~/.claude/history.jsonl (default: <research>/tmp/vm-history.jsonl)")
    ap.add_argument("--vm-status", default=None, help="recorded in the manifest (e.g. 'fetched 2026-10-11T01:28Z')")
    ap.add_argument("--codex-dir", default=os.path.join(home, ".codex"))
    ap.add_argument("--claude-dir", default=os.path.join(home, ".claude"))
    ap.add_argument("--people", default=None, help="file: one personal name per line (default: <research>/data/prompts-people.txt)")
    ap.add_argument("--allowlist", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "redaction-allowlist.txt"))
    ap.add_argument("--cutoff", default=None, help="ISO UTC; default: now")
    ap.add_argument("--founding", default=None,
                    help="approved founding-sessions CSV (columns of prompts-founding-sessions-proposed-<date>.csv, "
                         "decision include|exclude); without it no founding prompt is kept")
    a = ap.parse_args(argv)

    data = os.path.join(a.research, "data")
    os.makedirs(data, exist_ok=True)
    cutoff = parse_ts(a.cutoff) if a.cutoff else dt.datetime.now(UTC).timestamp()
    window = (pacific_midnight(WINDOW_START_DAY), cutoff)
    vm_path = a.vm_history or os.path.join(a.research, "tmp", "vm-history.jsonl")

    # sources
    mac_rows = read_jsonl(os.path.join(a.claude_dir, "history.jsonl"))
    cache_dir = os.path.join(a.claude_dir, "paste-cache")
    cache = frozenset(os.path.splitext(f)[0] for f in os.listdir(cache_dir)) if os.path.isdir(cache_dir) else frozenset()
    founding = load_founding(a.founding) if a.founding else set()
    vm_rows = read_jsonl(vm_path) if os.path.exists(vm_path) else None
    candidates = founding_candidates([("mac", mac_rows)] + ([("vm", vm_rows)] if vm_rows is not None else []), cutoff)
    results = {"claude-mac": history_prompts(mac_rows, "mac", window, cache, founding)}
    vm_status = "pending"
    if vm_rows is not None:
        results["claude-vm"] = history_prompts(vm_rows, "vm", window, founding=founding)
        vm_status = f"fetched (VM history last modified {iso_utc(os.path.getmtime(vm_path))})"
    codex_files, seen_ids = [], set()
    for kind in ("sessions", "archived_sessions"):
        for path in sorted(glob.glob(os.path.join(a.codex_dir, kind, "**", "*.jsonl"), recursive=True)):
            sid = os.path.basename(path)[-42:-6]
            if sid in seen_ids:
                continue
            seen_ids.add(sid)
            codex_files.append(path)
    results["codex"] = codex_prompts(([r for r in read_jsonl(p) if r] for p in codex_files), window)
    sources = source_summary(results)
    if "claude-vm" not in sources:
        sources["claude-vm"] = {"status": "pending"}
    check_reconciles({k: v for k, v in sources.items() if "in_source" in v})
    prompts = [p for r in results.values() for p in r.prompts]

    # settings + commits
    mac_sessions = {p["session"] for p in prompts if p["tool"] == "claude" and p["machine"] == "mac"}
    tx = load_claude_transcripts(a.claude_dir, mac_sessions)
    stat_machine = {"mac": "mac", "vm": "rvm"}
    keys = {(stat_machine[p["machine"]], p["session"]) for p in prompts if p["tool"] == "claude"}
    stats_sess, stats_calls = load_stats(a.stats, keys)
    owners = load_bridge_owners(a.claude_dir)
    for r in csv.DictReader(open(os.path.join(a.stats, "commit_sessions.csv"), newline="")):
        if r.get("local_session_surviving"):
            owners[r["cloud_session"]].add(r["local_session_surviving"])
    commits = load_commits(a.rustty_repo)
    tx_branch, tx_owner = load_transcript_commits(a.claude_dir, commits)
    human = {p["session"] for p in prompts}
    commits = branch_info(a.rustty_repo, commits, tx_branch)
    for c in commits:
        c["sessions"] = tx_owner.get(c["sha"], set()) & human
    commits.sort(key=lambda c: c["ts"])

    by_session = collections.defaultdict(list)
    for p in prompts:
        by_session[(p["tool"], p["machine"], p["session"])].append(p)
    codex_last = results["codex"].last
    for key, ps in by_session.items():
        ps.sort(key=lambda p: (p["ts_epoch"], p["order"]))
        tool, machine, sid = key
        t = tx.get(sid) if (tool == "claude" and machine == "mac") else None
        sk = (stat_machine.get(machine), sid)
        ss = stats_sess.get(sk) if tool == "claude" else None
        last = (t or {}).get("last") or (parse_ts(ss["last_ts"]) if ss and ss.get("last_ts") else None)
        last = last or codex_last.get(sid)
        foreign = {c for c, o in owners.items() if sid not in o}
        for i, p in enumerate(ps):
            nxt = next((q["ts_epoch"] for q in ps[i + 1:] if q["ts_epoch"] > p["ts_epoch"]), None)
            end = window_end(p["ts_epoch"], nxt, last, cutoff)
            branch = p.get("branch")
            if tool == "claude":
                if t and t["branches"]:
                    near = [b for ts_, b in t["branches"] if ts_ <= p["ts_epoch"] + 5] or [t["branches"][0][1]]
                    branch = near[-1]
                elif ss and ss.get("git_branch"):
                    branch = ss["git_branch"]
            if branch == "HEAD":
                branch = None
            p["branch"] = branch
            p["commits"], p["commits_basis"] = commit_window(p["ts_epoch"], end, branch, commits, foreign, sid, cutoff)
            p["_end"], p["_tx"], p["_sk"], p["_ss"] = end, t, sk, ss

    # a commit claimed by two or more human sessions keeps only evidence-backed links
    owned = collections.defaultdict(set)
    for cloud, o in owners.items():
        for sid in o:
            owned[sid].add(cloud)
    by_sha = {c["sha"]: c for c in commits}
    dedupe_stats = dedupe_links(prompts, by_sha, owned)

    # settings (after the dedupe: trailer inference uses the prompt's own linked commits only)
    for p in prompts:
        if p["tool"] == "claude":
            tm = [m for sha in p["commits"] for m in by_sha[sha]["models"]]
            p.update(join_settings(p, p["_end"], p["_tx"], stats_calls.get(p["_sk"]), p["_ss"], tm))
        for k in ("_end", "_tx", "_sk", "_ss"):
            p.pop(k, None)

    # tags
    phases_csv = sorted(glob.glob(os.path.join(data, "strategy-timeline-*.csv")))
    phases_csv = phases_csv[-1] if phases_csv else os.path.join(data, "strategy-timeline.csv")
    phases = load_phases(phases_csv)
    art = article_sessions(prompts)
    for p in prompts:
        p["article_work"] = is_article_work(p, art)
        p["ids"] = find_internal_ids(p["text"])
        p["phase"] = phase_for(pacific_day(p["ts_epoch"]), phases, p.get("founding", False))
        p["decision"] = is_decision(p["text"])
    assign_ids(prompts)
    prompts.sort(key=lambda p: (p["ts_epoch"], p["tool"], p["machine"], p["order"]))

    # outputs
    fields = ["id", "ts", "tool", "machine", "session", "text", "chars", "kind", "pasted", "model", "effort",
              "mode", "settings_source", "approval", "sandbox", "article_work", "commits", "commits_basis", "ids",
              "phase", "decision", "branch", "project", "founding"]
    with open(os.path.join(data, f"prompts-raw-{DATE_TAG}.jsonl"), "w") as f:
        for p in prompts:
            f.write(json.dumps({k: p.get(k) for k in fields}, ensure_ascii=False) + "\n")

    people = read_list(a.people or os.path.join(data, "prompts-people.txt"))
    allow = read_list(a.allowlist)
    projects = other_project_names(mac_rows)
    red = collections.Counter()
    with open(os.path.join(data, f"prompts-redactions-proposed-{DATE_TAG}.csv"), "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "start", "end", "match", "reason", "decision"])
        for p in prompts:
            for h in scan_redactions(p["text"], people, allow, projects):
                w.writerow([p["id"], h["start"], h["end"], h["match"], h["reason"], ""])
                red[h["reason"]] += 1

    gloss_path = os.path.join(a.research, "r2-glossary.md")
    known = {norm_id(x) for x in glossary_ids(open(gloss_path).read())} if os.path.exists(gloss_path) else set()
    missing, first = collections.Counter(), {}
    for p in prompts:
        for i in p["ids"]:
            if norm_id(i) not in known:
                missing[i] += 1
                first.setdefault(i, p["id"])
    with open(os.path.join(data, f"prompts-glossary-missing-{DATE_TAG}.txt"), "w") as f:
        f.write("# id\tcount\tfirst_prompt_id\tfamily  (ids found in prompts but not in r2-glossary.md)\n")
        for i, n in sorted(missing.items(), key=lambda x: (id_family(x[0]) != "id", -x[1], x[0])):
            f.write(f"{i}\t{n}\t{first[i]}\t{id_family(i)}\n")

    with open(os.path.join(data, f"prompts-article-sessions-{DATE_TAG}.csv"), "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["session", "tool", "machine", "reason", "first_ts", "last_ts", "prompts", "article_prompts",
                    "projects"])
        for (tool, machine, sid), ps in sorted(by_session.items(), key=lambda kv: kv[1][0]["ts_epoch"]):
            if sid in art:
                w.writerow([sid, tool, machine, art[sid], ps[0]["ts"], ps[-1]["ts"], len(ps),
                            sum(p["article_work"] for p in ps), "|".join(sorted({p["project"] or "" for p in ps}))])

    with open(os.path.join(data, f"prompts-founding-sessions-proposed-{DATE_TAG}.csv"), "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=FOUNDING_FIELDS)
        w.writeheader()
        w.writerows(candidates)

    sessions = session_aggregates(prompts, load_titles(a.stats))
    if sum(s["prompts"] for s in sessions) != len(prompts):
        raise SystemExit("session prompt counts do not add up to the prompt total")
    with open(os.path.join(data, f"prompts-sessions-{DATE_TAG}.jsonl"), "w") as f:
        for s in sessions:
            f.write(json.dumps(s, ensure_ascii=False) + "\n")

    def count(k, src=None):
        c = collections.Counter()
        for p in prompts:
            if src is None or f"{p['tool']}-{p['machine']}".replace("codex-mac", "codex") == src:
                c[str(p[k])] += 1
        return dict(sorted(c.items()))

    manifest = {
        "sources": sources,
        "generated": iso_utc(dt.datetime.now(UTC).timestamp()),
        "cutoff": iso_utc(cutoff),
        "window": [WINDOW_START_DAY, WINDOW_END_DAY],
        "window_utc": [iso_utc(window[0]), iso_utc(window[1])],
        "purged_before": PURGED_BEFORE,
        "vm_status": vm_status,
        "rules": {"article_work": ARTICLE_RULE, "decision": DECISION_RULE, "commits": COMMITS_RULE + " " + DEDUPE_RULE,
                  "project": "Claude history `project` / Codex `cwd` contains 'rustty' (worktrees included, as "
                             "rustty-stats); paths containing 'ghostty-website' are also kept on or after "
                             "2026-10-08 00:00 Pacific (article work).",
                  "human_turns": "Claude: every history row (typed prompts and slash commands; subagent prompts are "
                                 "never in the history). Codex: user-role message items of root sessions, minus "
                                 "injected context (AGENTS.md, environment/plugin/goal/subagent notices), minus "
                                 "replays; subagent and auto-review sessions are dropped whole.",
                  "settings": "transcript (the call answering the prompt, live transcript or rustty-stats per-call "
                              "rows; for Codex the turn context) > session (rustty-stats, one model in the "
                              "session) > commit-trailer (Co-Authored-By model of the prompt's own linked commits, "
                              "after the multi-session dedupe; effort unknown) > unknown.",
                  "ids": "jargon.py find_internal_ids patterns (ghostty-website-journey 438cb814) + FX<n>.",
                  "founding": FOUNDING_RULE, "sessions": SESSION_RULE},
        "founding_floor": iso_utc(FOUNDING_FLOOR),
        "counts": {
            "prompts": len(prompts),
            "by_source": {k: v["kept"] for k, v in sources.items() if "kept" in v},
            "kind": count("kind"), "pasted": count("pasted"), "settings_source": count("settings_source"),
            "settings_source_by_source": {s: count("settings_source", s) for s in ("claude-mac", "claude-vm", "codex")},
            "commits_basis": count("commits_basis"), "article_work": count("article_work"),
            "decision": count("decision"),
            "mac_before_purge_with_transcript": sum(
                1 for p in prompts if p["tool"] == "claude" and p["machine"] == "mac"
                and pacific_day(p["ts_epoch"]) < PURGED_BEFORE and p["settings_source"] == "transcript"),
            "prompts_with_commits": sum(1 for p in prompts if p["commits"]),
            "commit_dedupe": dedupe_stats,
            "redaction_candidates": dict(sorted(red.items())),
            "redaction_candidate_prompts": len({p["id"] for p in prompts
                                                if scan_redactions(p["text"], people, allow, projects)}),
            "glossary_missing_ids": len(missing),
            "glossary_missing_ids_family_id": sum(1 for i in missing if id_family(i) == "id"),
            "article_sessions": len(art),
            "founding_candidates": dict(sorted(collections.Counter(c["reason"] for c in candidates).items())),
            "founding_candidate_prompts": sum(c["prompts"] for c in candidates),
            "founding_list": "approved" if a.founding else "none",
            "founding_sessions": sum(1 for s in sessions if s["founding"]),
            "founding_prompts": sum(1 for p in prompts if p["founding"]),
            "sessions": len(sessions),
            "sessions_by_source": dict(sorted(collections.Counter(
                f"{s['tool']}-{s['machine']}".replace("codex-mac", "codex") for s in sessions).items())),
            "sessions_commands_only": sum(1 for s in sessions if s["commands_only"]),
            "sessions_with_cc_title": sum(1 for s in sessions if s["cc_title"]),
        },
        "inputs": {"codex_files": len(codex_files), "transcript_sessions": len(tx),
                   "phases": os.path.basename(phases_csv), "people_names": len(people),
                   "allowlist_terms": len(allow), "other_project_names": len(projects),
                   "rustty_head": _git(a.rustty_repo, "rev-parse", "HEAD").strip(), "commits": len(commits),
                   "commits_seen_in_transcripts": len(tx_branch)},
    }
    check_public_manifest(manifest)
    with open(os.path.join(data, f"prompts-manifest-{DATE_TAG}.json"), "w") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")
    private = {"note": "untracked debugging details; never publish", "vm_note": a.vm_status,
               "founding_list": a.founding,
               "paths": {"claude_history": os.path.join(a.claude_dir, "history.jsonl"), "vm_history": vm_path,
                         "codex_dir": a.codex_dir, "stats": a.stats, "rustty_repo": a.rustty_repo},
               "other_project_names": projects}
    with open(os.path.join(data, f"prompts-manifest-private-{DATE_TAG}.json"), "w") as f:
        json.dump(private, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print(json.dumps({k: manifest[k] for k in ("sources", "cutoff", "vm_status", "counts")}, indent=2))
    return manifest


if __name__ == "__main__":
    sys.exit(0 if main() else 1)
