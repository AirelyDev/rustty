"""Tests for extract_prompts.py. In-memory fixtures only; no real files are read.

Run: python3 -m unittest discover -s .port/porting-article/tools -p 'test_*.py' -v
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import extract_prompts as ep  # noqa: E402


def ms(iso):
    return int(ep.parse_ts(iso) * 1000)


def hist(display, iso, project="/Users/gael/projects/rustty", session="s1", pasted=None):
    return {"display": display, "pastedContents": pasted or {}, "timestamp": ms(iso),
            "project": project, "sessionId": session}


WINDOW = (ep.parse_ts("2026-06-21T07:00:00Z"), ep.parse_ts("2026-10-11T07:00:00Z"))


class HistoryTest(unittest.TestCase):
    def test_history_rows_filtered_by_project(self):
        rows = [hist("hello", "2026-07-01T10:00:00Z"),
                hist("other", "2026-07-01T10:01:00Z", project="/Users/gael/projects/bgco"),
                hist("worktree", "2026-07-01T10:02:00Z", project="/Users/gael/projects/rustty/.claude/worktrees/x")]
        res = ep.history_prompts(rows, "mac", WINDOW)
        self.assertEqual([p["text"] for p in res.prompts], ["hello", "worktree"])
        self.assertEqual(res.dropped, {"other-project": 1})
        self.assertEqual(res.in_source, 3)

    def test_history_window_and_website_rule(self):
        rows = [hist("early", "2026-06-01T10:00:00Z"),
                hist("site before", "2026-10-07T10:00:00Z", project="/Users/gael/projects/ghostty-website"),
                hist("site after", "2026-10-08T08:00:00Z", project="/Users/gael/projects/ghostty-website-journey")]
        res = ep.history_prompts(rows, "mac", WINDOW)
        self.assertEqual([p["text"] for p in res.prompts], ["site after"])
        self.assertEqual(res.dropped, {"before-window": 1, "other-project": 1})

    def test_slash_commands_kind_command(self):
        self.assertEqual(ep.prompt_kind("/clear"), "command")
        self.assertEqual(ep.prompt_kind("/goal See rustty"), "command")
        self.assertEqual(ep.prompt_kind("/Users/gael/projects/rustty is the repo"), "prompt")
        self.assertEqual(ep.prompt_kind("continue"), "prompt")
        res = ep.history_prompts([hist("/clear", "2026-07-01T10:00:00Z")], "mac", WINDOW)
        self.assertEqual(res.prompts[0]["kind"], "command")

    def test_pasted_flag(self):
        lost = hist("[Pasted text #1 +26 lines]", "2026-07-01T10:00:00Z")
        kept = hist("see [Pasted text #1 +3 lines]", "2026-07-01T10:01:00Z",
                    pasted={"1": {"id": 1, "type": "text", "content": "SECRET PASTE BODY"}})
        hashed = hist("[Pasted text #1 +3 lines]", "2026-07-01T10:02:00Z",
                      pasted={"1": {"id": 1, "type": "text", "contentHash": "abc"}})
        plain = hist("no paste here", "2026-07-01T10:03:00Z")
        res = ep.history_prompts([lost, kept, hashed, plain], "mac", WINDOW)
        by = {p["text"]: p for p in res.prompts}
        self.assertEqual(by["[Pasted text #1 +26 lines]"]["pasted"], "lost")
        self.assertEqual(by["see [Pasted text #1 +3 lines]"]["pasted"], "kept")
        self.assertEqual(by["no paste here"]["pasted"], "none")
        # contentHash with no cache file is lost; with the cache file it is kept
        self.assertEqual(by["[Pasted text #1 +3 lines]"]["pasted"], "lost")
        self.assertEqual(ep.pasted_flag("[Pasted text #1 +3 lines]", hashed["pastedContents"], {"abc"}), "kept")
        # verbatim display only: the paste body is never expanded into the text
        self.assertNotIn("SECRET PASTE BODY", by["see [Pasted text #1 +3 lines]"]["text"])
        self.assertEqual(by["see [Pasted text #1 +3 lines]"]["chars"], len("see [Pasted text #1 +3 lines]"))


def codex_session(model="gpt-5.5", thread_source="user", cwd="/Users/gael/projects/rustty", sub=False):
    src = {"subagent": {"thread_spawn": {}}} if sub else "vscode"
    meta = {"type": "session_meta", "timestamp": "2026-07-31T18:50:00.000Z",
            "payload": {"id": "cx1", "session_id": "cx1", "cwd": cwd, "thread_source": thread_source,
                        "source": src, "git": {"branch": "main"}}}
    ctx = {"type": "turn_context", "timestamp": "2026-07-31T18:50:01.000Z",
           "payload": {"model": model, "effort": "high", "approval_policy": "never",
                       "sandbox_policy": {"type": "danger-full-access"},
                       "collaboration_mode": {"mode": "default"}}}

    def user(ts, *texts):
        return {"type": "response_item", "timestamp": ts,
                "payload": {"type": "message", "role": "user",
                            "content": [{"type": "input_text", "text": t} for t in texts]}}
    return [
        meta,
        user("2026-07-31T18:50:00.500Z", "<recommended_plugins>\nlist\n</recommended_plugins>",
             "<environment_context>\n  <cwd>/x</cwd>\n</environment_context>"),
        user("2026-07-31T18:50:00.600Z", "# AGENTS.md instructions for /Users/gael/projects/rustty\n\n<INSTRUCTIONS>x</INSTRUCTIONS>"),
        ctx,
        user("2026-07-31T18:50:02.000Z", "Run the build please\n"),
        {"type": "response_item", "timestamp": "2026-07-31T18:50:03.000Z",
         "payload": {"type": "function_call_output", "call_id": "c1", "output": "build ok"}},
        user("2026-07-31T18:51:00.000Z", "<codex_internal_context source=\"goal\">\nContinue\n</codex_internal_context>"),
        user("2026-07-31T18:52:00.000Z", "Go ahead\n"),
    ]


class CodexTest(unittest.TestCase):
    def test_codex_human_turns_only(self):
        res = ep.codex_prompts([codex_session()], WINDOW)
        self.assertEqual([p["text"] for p in res.prompts], ["Run the build please\n", "Go ahead\n"])
        self.assertEqual(res.prompts[0]["model"], "gpt-5.5")
        self.assertEqual(res.prompts[0]["effort"], "high")
        self.assertEqual(res.prompts[0]["mode"], "bypass")
        self.assertEqual(res.prompts[0]["settings_source"], "transcript")
        self.assertEqual(res.dropped, {"injected-context": 3})
        self.assertEqual(res.in_source, 5)

    def test_codex_auto_review_and_subagent_dropped(self):
        res = ep.codex_prompts([codex_session(model="codex-auto-review", thread_source="subagent", sub=True)], WINDOW)
        self.assertEqual(res.prompts, [])
        self.assertEqual(res.dropped, {"auto-review": 5})
        res = ep.codex_prompts([codex_session(sub=True, thread_source="subagent")], WINDOW)
        self.assertEqual(res.prompts, [])
        self.assertEqual(res.dropped, {"subagent": 5})

    def test_codex_other_project_and_duplicates(self):
        res = ep.codex_prompts([codex_session(cwd="/Users/gael/projects/bgco")], WINDOW)
        self.assertEqual(res.dropped, {"other-project": 5})
        res = ep.codex_prompts([codex_session(), codex_session()], WINDOW)
        self.assertEqual(len(res.prompts), 2)
        self.assertEqual(res.dropped, {"injected-context": 6, "duplicate": 2})


class SettingsTest(unittest.TestCase):
    def test_settings_purged_window(self):
        p = {"ts_epoch": ep.parse_ts("2026-08-01T10:00:00Z"), "machine": "mac", "tool": "claude"}
        s = ep.join_settings(p, p["ts_epoch"] + 600, tx=None, stats_calls=None, stats_session=None, trailer_models=[])
        self.assertEqual((s["model"], s["effort"], s["settings_source"]), (None, None, "unknown"))
        s = ep.join_settings(p, p["ts_epoch"] + 600, tx=None, stats_calls=None, stats_session=None,
                             trailer_models=["claude-opus-4-8"])
        self.assertEqual((s["model"], s["effort"], s["settings_source"]), ("claude-opus-4-8", None, "commit-trailer"))

        p2 = {"ts_epoch": ep.parse_ts("2026-09-20T10:00:00Z"), "machine": "mac", "tool": "claude"}
        tx = {"calls": [(ep.parse_ts("2026-09-20T09:59:00Z"), "claude-sonnet-5", "low"),
                        (ep.parse_ts("2026-09-20T10:00:05Z"), "claude-opus-4-8", "xhigh"),
                        (ep.parse_ts("2026-09-20T10:01:00Z"), "claude-fable-5", "medium")],
              "perms": [(ep.parse_ts("2026-09-20T10:00:00.2Z"), "bypassPermissions")]}
        s = ep.join_settings(p2, p2["ts_epoch"] + 600, tx=tx, stats_calls=None, stats_session=None,
                             trailer_models=["claude-x"])
        self.assertEqual((s["model"], s["effort"], s["mode"], s["settings_source"]),
                         ("claude-opus-4-8", "xhigh", "bypass", "transcript"))

    def test_settings_session_level(self):
        p = {"ts_epoch": ep.parse_ts("2026-07-20T10:00:00Z"), "machine": "vm", "tool": "claude"}
        s = ep.join_settings(p, p["ts_epoch"] + 600, tx=None, stats_calls=[],
                             stats_session={"models": "claude-opus-4-8", "efforts": "", "permission_mode": "bypassPermissions"},
                             trailer_models=[])
        self.assertEqual((s["model"], s["effort"], s["mode"], s["settings_source"]),
                         ("claude-opus-4-8", None, "bypass", "session"))

    def test_trailer_model_name(self):
        self.assertEqual(ep.trailer_model("Claude Opus 4.8 (1M context) <noreply@anthropic.com>"), "claude-opus-4-8")
        self.assertEqual(ep.trailer_model("Claude Fable 5 <noreply@anthropic.com>"), "claude-fable-5")
        self.assertIsNone(ep.trailer_model("Someone <a@b.c>"))


class CommitWindowTest(unittest.TestCase):
    def setUp(self):
        t = ep.parse_ts("2026-09-20T10:00:00Z")
        self.t = t
        self.commits = [
            {"sha": "a" * 40, "ts": t + 60, "label": "phase-8", "on": {"phase-8", "main"}, "cloud": set()},
            {"sha": "b" * 40, "ts": t + 120, "label": "other", "on": {"other", "main"}, "cloud": set()},
            {"sha": "c" * 40, "ts": t + 4000, "label": "phase-8", "on": {"phase-8", "main"}, "cloud": set()},
            {"sha": "d" * 40, "ts": t - 10, "label": "phase-8", "on": {"phase-8", "main"}, "cloud": set()},
        ]

    def test_commit_window(self):
        shas, basis = ep.commit_window(self.t, self.t + 3600, "phase-8", self.commits)
        self.assertEqual((shas, basis), (["a" * 40], "session-window"))
        shas, basis = ep.commit_window(self.t, self.t + 3600, "main", self.commits)
        self.assertEqual((shas, basis), (["a" * 40, "b" * 40], "session-window"))
        # no branch known, two branches active in the window -> none
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, None, self.commits), ([], "none"))
        # no branch known, a single branch active -> time-window-branch
        self.assertEqual(ep.commit_window(self.t + 3000, self.t + 5000, None, self.commits),
                         (["c" * 40], "time-window-branch"))
        # empty window
        self.assertEqual(ep.commit_window(self.t + 9000, self.t + 9100, "main", self.commits), ([], "none"))

    def test_commit_window_foreign_cloud_session(self):
        commits = [dict(self.commits[0], cloud={"session_X"})]
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, "phase-8", commits, foreign_cloud={"session_X"}),
                         ([], "none"))
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, "phase-8", commits, foreign_cloud=set()),
                         (["a" * 40], "session-window"))

    def test_commit_window_transcript_evidence(self):
        # shown in this session's own transcripts: linked even off the session's branch
        mine = dict(self.commits[1], sessions={"s1"})
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, "phase-8", [mine], session="s1"),
                         (["b" * 40], "session-window"))
        # shown only in another human session's transcripts: never linked
        theirs = dict(self.commits[0], sessions={"s2"})
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, "phase-8", [theirs], session="s1"), ([], "none"))
        # a trailer naming both a foreign and an unknown cloud session is still linkable
        mixed = dict(self.commits[0], cloud={"session_X", "session_Y"})
        self.assertEqual(ep.commit_window(self.t, self.t + 3600, "phase-8", [mixed], foreign_cloud={"session_X"}),
                         (["a" * 40], "session-window"))

    def test_dedupe_multi_session_commits(self):
        by_sha = {
            "a": {"sha": "a", "sessions": {"s1"}, "cloud": set()},         # transcript evidence for s1
            "b": {"sha": "b", "sessions": set(), "cloud": {"session_Z"}},  # trailer bridged from s2
            "c": {"sha": "c", "sessions": set(), "cloud": set()},          # no evidence at all
            "d": {"sha": "d", "sessions": set(), "cloud": set()},          # claimed by s1 only
        }
        prompts = [
            {"session": "s1", "commits": ["a", "b", "c", "d"], "commits_basis": "session-window"},
            {"session": "s2", "commits": ["a", "b", "c"], "commits_basis": "time-window-branch"},
            {"session": "s2", "commits": ["c"], "commits_basis": "time-window-branch"},
        ]
        stats = ep.dedupe_links(prompts, by_sha, {"s2": {"session_Z"}})
        self.assertEqual(prompts[0]["commits"], ["a", "d"])
        self.assertEqual(prompts[0]["commits_basis"], "session-window")
        self.assertEqual(prompts[1]["commits"], ["b"])
        self.assertEqual((prompts[2]["commits"], prompts[2]["commits_basis"]), ([], "ambiguous"))
        self.assertEqual(stats, {"contested_commits": 3, "links_removed": 5, "prompts_ambiguous": 1})

    def test_window_end(self):
        self.assertEqual(ep.window_end(100.0, 200.0, None), 200.0)
        self.assertEqual(ep.window_end(100.0, None, 5000.0), 5000.0)
        self.assertEqual(ep.window_end(100.0, None, None), 100.0 + ep.WINDOW_CAP_S)
        self.assertEqual(ep.window_end(100.0, 100.0 + 10 * ep.WINDOW_CAP_S, None), 100.0 + ep.WINDOW_CAP_S)

    def test_merge_subject_branch(self):
        self.assertEqual(ep.merge_subject_branch("Merge branch 'parity-closure' into tail"), "parity-closure")
        self.assertEqual(ep.merge_subject_branch("Merge phase-7/w2 (W2 gate)"), "phase-7/w2")
        self.assertIsNone(ep.merge_subject_branch("merge: Phase 3 work"))


class RulesTest(unittest.TestCase):
    def test_decision_rule(self):
        for t in ["A", "b", "ok", "yes", "go", "continue", "ok.", "B, go ahead", "Go ahead\n", "LGTM!"]:
            self.assertTrue(ep.is_decision(t), t)
        for t in ["ok but change X", "", "continue with the plan", "/clear", "e", "go go go go"]:
            self.assertFalse(ep.is_decision(t), t)

    def test_article_work_rule(self):
        def p(session, iso, text, project="/Users/gael/projects/rustty"):
            return {"session": session, "ts_epoch": ep.parse_ts(iso), "text": text, "project": project}
        prompts = [
            p("s1", "2026-10-09T10:00:00Z", "draft the article section"),
            p("s1", "2026-10-09T11:00:00Z", "continue"),
            p("s2", "2026-10-09T10:00:00Z", "fix the build", project="/Users/gael/projects/ghostty-website-journey"),
            p("s3", "2026-10-09T10:00:00Z", "fix the renderer"),
            p("s4", "2026-10-01T10:00:00Z", "the journey article"),
            p("s5", "2026-10-07T10:00:00Z", "write the website copy"),
            p("s5", "2026-10-08T08:00:00Z", "continue"),
            # 00:00 Pacific on 2026-10-08 is 07:00Z; just before it is not article work
            p("s6", "2026-10-08T06:59:59Z", "the article"),
        ]
        sessions = ep.article_sessions(prompts)
        self.assertEqual(set(sessions), {"s1", "s2", "s5"})
        flags = [ep.is_article_work(x, sessions) for x in prompts]
        self.assertEqual(flags, [True, True, True, False, False, False, True, False])

    def test_ids_extracted(self):
        text = "fix UD131 then R7 and G3b; N550 in P8-W10, also FX13. phase 7 too"
        ids = ep.find_internal_ids(text)
        for want in ["UD131", "R7", "G3b", "N550", "P8-W10", "FX13"]:
            self.assertIn(want, ids)
        self.assertEqual(ep.find_internal_ids("ok"), [])
        self.assertEqual(ep.find_internal_ids("UD131 and UD131"), ["UD131"])

    def test_phase_for(self):
        phases = [("2026-06-21", "pre-Bun"), ("2026-07-10", "post-Bun"), ("2026-09-11", "post-R13")]
        self.assertIsNone(ep.phase_for("2026-06-01", phases))
        self.assertEqual(ep.phase_for("2026-07-09", phases), "pre-Bun")
        self.assertEqual(ep.phase_for("2026-07-10", phases), "post-Bun")
        self.assertEqual(ep.phase_for("2026-10-10", phases), "post-R13")

    def test_glossary_ids(self):
        md = ("| ID | Plain |\n|---|---|\n| R1 | Full fidelity |\n"
              "| R2 ⚠ Also 'DECISIONS.md R2' | Identity |\n| D1 (decision) ⚠ Collides | Layering |\n"
              "| **N###** | ledger |\n")
        ids = ep.glossary_ids(md)
        self.assertTrue({"R1", "R2", "D1"} <= ids)


class RedactionTest(unittest.TestCase):
    def scan(self, text, people=(), allow=()):
        return [(m["match"], m["reason"]) for m in ep.scan_redactions(text, people=people, allowlist=allow)]

    def test_redaction_scanner(self):
        allow = ["Ghostty", "Mitchell Hashimoto", "noreply@anthropic.com", "git@github.com"]
        people = ["Mitchell", "Alice Example"]
        cases = {
            "key sk-abcdefghijklmnopqrstuv1234 here": ("sk-abcdefghijklmnopqrstuv1234", "secret"),
            "tok ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789": ("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", "secret"),
            "slack xoxb-1234567890-abcdefghij": ("xoxb-1234567890-abcdefghij", "secret"),
            "aws AKIAABCDEFGHIJKLMNOP": ("AKIAABCDEFGHIJKLMNOP", "secret"),
            "-----BEGIN OPENSSH PRIVATE KEY-----": ("-----BEGIN OPENSSH PRIVATE KEY-----", "secret"),
            "VM at x, password is hunter2\n": ("password is hunter2", "secret"),
            "mail bob@example.com please": ("bob@example.com", "email"),
            "call +1 415-555-0134 now": ("+1 415-555-0134", "phone"),
            "see /Users/gael/projects/bgco/src/x.rs": ("/Users/gael/projects/bgco/src/x.rs", "unrelated-path"),
            "ask Mitchell about it": ("Mitchell", "person"),
            "cc Alice Example": ("Alice Example", "person"),
        }
        for text, want in cases.items():
            self.assertIn(want, self.scan(text, people, allow), text)
        clean = [
            "Co-Authored-By: Claude <noreply@anthropic.com>",
            "port of Ghostty by Mitchell Hashimoto",
            "open /Users/gael/projects/rustty/crates/x.rs and /Users/gael/projects/ghostty/src/a.zig",
            "stats in /Users/gael/projects/rustty-stats/data and ~/projects/rustty-journey-research",
            "worktree /Users/gael/projects/ghostty-website-journey/src",
            "commit 931369637a3cdf857d and date 2026-10-10 and run 1234567890",
            "remote git@github.com:AirelyDev/rustty.git",
        ]
        for text in clean:
            self.assertEqual(self.scan(text, people, allow), [], text)

    def test_credentials_any_phrasing(self):
        positives = {
            "freshly installed, pwd hunter2, help me": "pwd hunter2",
            "user gael pw: hunter2": "pw: hunter2",
            "pass: s3cret!": "pass: s3cret!",
            "login gael/hunter2 on the box": "login gael/hunter2",
            "the password hunter2 works": "password hunter2",
            "password is hunter2-example.": "password is hunter2-example",
            "creds: admin/admin": "creds: admin/admin",
            "api_key = abcdef1234567890": "api_key = abcdef1234567890",
            "sshpass -p hunter2 ssh box": "sshpass -p hunter2",
            "on the vm, user gael passw hunter2": "passw hunter2",
        }
        for text, want in positives.items():
            self.assertIn((want, "secret"), self.scan(text), text)
        negatives = ["do a review pass", "first pass done", "pass the flag through", "run pwd and ls",
                     "the password field is empty", "print `pwd` first", "pwd output shows the path",
                     "a password is required", "password manager", "pass it to the builder",
                     "no token is needed", "we passed the test"]
        for text in negatives:
            self.assertEqual([h for h in self.scan(text) if h[1] == "secret"], [], text)

    def test_ip_and_user_host(self):
        self.assertIn(("203.0.113.7", "ip"), self.scan("VM at 203.0.113.7, fresh"))
        self.assertIn(("user@203.0.113.7", "user-host"), self.scan("running at user@203.0.113.7 (repo)"))
        self.assertIn(("user@devbox:~/projects/rustty$", "user-host"), self.scan("user@devbox:~/projects/rustty$ clear"))
        self.assertIn(("bob@example.com", "email"), self.scan("mail bob@example.com"))
        self.assertNotIn("user-host", [r for _m, r in self.scan("mail bob@example.com")])
        for text in ["version 1.2.3", "zig 0.16.0", "oracle 12752b2ac", "review @docs/ and @.port/",
                     "react@18.3.1 is pinned", "v1.2.3.4.5"]:
            self.assertEqual([h for h in self.scan(text) if h[1] in ("ip", "user-host")], [], text)

    def test_unrelated_project_names(self):
        hits = [(m["match"], m["reason"]) for m in
                ep.scan_redactions("compare with manga-reader please", projects=["manga-reader"])]
        self.assertEqual(hits, [("manga-reader", "unrelated-project")])

    def test_spans_are_offsets(self):
        text = "x bob@example.com y"
        m = ep.scan_redactions(text)[0]
        self.assertEqual(text[m["start"]:m["end"]], "bob@example.com")


class ManifestTest(unittest.TestCase):
    def test_manifest_reconciles(self):
        rows = [hist("hello", "2026-07-01T10:00:00Z"),
                hist("other", "2026-07-01T10:01:00Z", project="/Users/gael/projects/bgco"),
                hist("early", "2026-05-01T10:00:00Z")]
        mac = ep.history_prompts(rows, "mac", WINDOW)
        cx = ep.codex_prompts([codex_session(), codex_session(model="codex-auto-review")], WINDOW)
        sources = ep.source_summary({"claude-mac": mac, "codex": cx})
        for name, s in sources.items():
            self.assertEqual(s["kept"] + sum(s["dropped"].values()), s["in_source"], name)
        self.assertEqual(sources["claude-mac"], {"in_source": 3, "kept": 1,
                                                 "dropped": {"before-window": 1, "other-project": 1}})
        ep.check_reconciles(sources)
        with self.assertRaises(SystemExit):
            ep.check_reconciles({"x": {"in_source": 3, "kept": 1, "dropped": {"a": 1}}})


class PublicManifestTest(unittest.TestCase):
    def test_public_manifest_has_no_private_details(self):
        ep.check_public_manifest({"counts": {"prompts": 3}, "vm_status": "fetched (copy dated 2026-10-11T01:28:00Z)"})
        for leak in ["/Users/gael/.claude/history.jsonl", "/home/gael/x", "~/projects/x", "rvm at 192.168.64.4"]:
            with self.assertRaises(SystemExit):
                ep.check_public_manifest({"inputs": {"x": leak}})


class IdAssignmentTest(unittest.TestCase):
    def test_assign_ids_stable(self):
        ps = [{"tool": "claude", "machine": "mac", "ts_epoch": ep.parse_ts("2026-07-13T09:15:02.100Z"), "order": 0},
              {"tool": "claude", "machine": "mac", "ts_epoch": ep.parse_ts("2026-07-13T09:15:02.900Z"), "order": 1},
              {"tool": "codex", "machine": "mac", "ts_epoch": ep.parse_ts("2026-07-13T09:15:02.500Z"), "order": 0}]
        ep.assign_ids(ps)
        self.assertEqual([p["id"] for p in ps], ["claude-mac-20260713T091502Z-0", "claude-mac-20260713T091502Z-1",
                                                 "codex-mac-20260713T091502Z-0"])
        self.assertEqual(ps[0]["ts"], "2026-07-13T09:15:02Z")


if __name__ == "__main__":
    unittest.main()
