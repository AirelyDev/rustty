"""Unit tests for the porting article data pipeline."""
import csv, json, os, re, statistics, subprocess, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import jargon  # noqa: E402

# RESEARCH and MOCKBUILD are the pipeline inputs: PORTING_RESEARCH / PORTING_MOCKBUILD when set,
# else the author's rustty checkout (scripts/porting/README.md).
RESEARCH = Path(os.environ.get("PORTING_RESEARCH", "/Users/gael/projects/rustty/.port/journey-article/research"))
MOCKBUILD = Path(os.environ.get("PORTING_MOCKBUILD", "/Users/gael/projects/rustty/.port/journey-article/mockup-build"))


class JargonTest(unittest.TestCase):
    """strip_internal_ids removes internal identifiers and reports changes."""

    def test_strips_known_id_families(self):
        for raw in ["fix UD123 crash", "gate G3b green", "rule R13 says", "N511 apc",
                    "P8-W9 T1b check", "D1 layering", "A4 sret gate", "G6c lane"]:
            clean, changed = jargon.strip_internal_ids(raw)
            self.assertTrue(changed, raw)
            for tok in ["UD123", "G3b", "R13", "N511", "P8-W9", "T1b", "D1", "A4", "G6c"]:
                self.assertNotIn(tok, clean)

    def test_strips_process_labels(self):
        cases = {
            "Inspector Phase-3 placeholder": "Inspector placeholder",
            "Phase 3 upstream-resync master plan": "upstream-resync master plan",
            "Phase 7 finale": "finale",
            "renderer: offscreen seams (Phase A)": "renderer: offscreen seams",
            "port examples(phase B) and docs": "port examples and docs",
            "Perf wave 2: restoring inlining": "Perf: restoring inlining",
            "collect the Chunk-3a re-attempt validation workflow": "collect the re-attempt validation workflow",
            "vaxis chunk 3b: cell writer": "vaxis: cell writer",
            "Batch 1 (docs, records)": "(docs, records)",
            "batch-1 safe clippy fixes": "safe clippy fixes",
            "sub-project #3 — embedding API": "embedding API",
            "rustty: perf — r3-clone: hoist the style lookup": "rustty: perf: hoist the style lookup",
            "perf r5-print-outline: inline the fast path": "perf: inline the fast path",
            "parity task T-C: port parseInt": "parity: port parseInt",
            "rustty: Terminal erase methods + tests (vt-12b-3a)": "rustty: Terminal erase methods + tests",
            "Phases 0-1: reference baseline": "reference baseline",
        }
        for raw, want in cases.items():
            self.assertEqual(jargon.strip_internal_ids(raw), (want, True), raw)

    def test_leaves_process_words_without_labels_alone(self):
        for raw in ["phase out the old loop", "a wave of fixes", "batch the writes", "chunked reads",
                    "dispatch stage follows", "subproject layout"]:
            self.assertEqual(jargon.strip_internal_ids(raw), (raw, False))

    def test_leaves_plain_text_and_hashes_alone(self):
        for raw in ["vt: fix search aliasing (9e3fe1c25)", "M4 Pro", "UTF-8", "x86_64", "AArch64"]:
            self.assertEqual(jargon.strip_internal_ids(raw), (raw, False))


class FactsTest(unittest.TestCase):
    """facts.csv is well formed and the pipeline emits facts.json."""

    def test_facts_csv_rows_valid(self):
        with open(HERE / "facts.csv", newline="") as f:
            rows = list(csv.DictReader(f))
        self.assertGreaterEqual(len(rows), 30)
        ids = [r["id"] for r in rows]
        self.assertEqual(len(ids), len(set(ids)), "duplicate fact ids")
        for r in rows:
            self.assertRegex(r["id"], r"^[a-z0-9_]+$")
            self.assertIn(r["provenance"], {"MEASURED", "DOCUMENTED", "ESTIMATED"})
            self.assertTrue(r["display"] and r["source"] and r["as_of"], r["id"])
            self.assertFalse(jargon.strip_internal_ids(r["display"])[1], r["id"])
            self.assertRegex(r["href"], r"^https://", r["id"])
            self.assertTrue(r["href_label"], r["id"])

    def test_cli_writes_facts_json(self):
        with tempfile.TemporaryDirectory() as out:
            subprocess.run([sys.executable, str(HERE / "build_data.py"), "--research", str(RESEARCH),
                            "--mockbuild", str(MOCKBUILD), "--out", out, "--only", "facts"], check=True)
            facts = json.loads((Path(out) / "facts.json").read_text())
            self.assertEqual(facts["commits_head"]["display"], "3,085")
            self.assertEqual(facts["perf_suite_delta_july"]["display"], "−21.7%")
            self.assertTrue(all(f["href"] for f in facts.values()), "every fact links its source")

    def test_cli_writes_commits_json(self):
        """commits.json maps commit tokens to full shas in the right repository; unresolvable rows are left out."""
        with tempfile.TemporaryDirectory() as out:
            subprocess.run([sys.executable, str(HERE / "build_data.py"), "--research", str(RESEARCH),
                            "--mockbuild", str(MOCKBUILD), "--out", out, "--only", "commits"], check=True)
            commits = json.loads((Path(out) / "commits.json").read_text())
            self.assertEqual(commits["58aa62a81"], "r:58aa62a81e7db5f8762b0158acc661dafc31f230")
            self.assertEqual(commits["246f70287"], "g:246f702876b924a1cb7cade1e99274d1470302fc")
            self.assertEqual(commits["7cde540171"], "google/highway:7cde540171")
            for h in ("97e244c", "2826a37a", "72e83b00"):
                self.assertNotIn(h, commits)
            for k, v in commits.items():
                if v[:2] in ("r:", "g:"):
                    self.assertRegex(v, r"^[rg]:[0-9a-f]{40}$", k)
                    self.assertTrue(v[2:].startswith(k), k)


class PipelineContractTest(unittest.TestCase):
    """build_data enforces the _meta contract and rejects unknown slugs."""

    def test_validate_meta(self):
        import build_data
        good = {"_meta": {"slug": "w", "generatedFrom": ["a"], "asOf": "x"}}
        build_data.validate_meta("w", good)
        for bad in [{}, {"_meta": {"slug": "w"}}, {"_meta": {"slug": "other", "generatedFrom": [], "asOf": "x"}}]:
            with self.assertRaises(ValueError):
                build_data.validate_meta("w", bad)

    def test_main_fails_on_builder_without_meta(self):
        import build_data
        from unittest import mock
        argv = ["build_data.py", "--research", "r", "--mockbuild", "m", "--out", tempfile.mkdtemp(), "--only", "fake"]
        with mock.patch.dict(build_data.REGISTRY, {"fake": lambda r, m: {"x": 1}}), \
                mock.patch.object(sys, "argv", argv), self.assertRaises(SystemExit) as cm:
            build_data.main()
        self.assertNotIn(cm.exception.code, (0, None))

    def test_unknown_only_slug_exits_nonzero(self):
        res = subprocess.run([sys.executable, str(HERE / "build_data.py"), "--research", str(RESEARCH),
                              "--mockbuild", str(MOCKBUILD), "--out", tempfile.mkdtemp(), "--only", "nope"],
                             capture_output=True, text=True)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("unknown slug", res.stderr)


def walk_strings(node):
    """Yield every string anywhere inside a nested dict/list structure."""
    if isinstance(node, str):
        yield node
    elif isinstance(node, dict):
        for v in node.values():
            yield from walk_strings(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk_strings(v)


# REFERENCE_PIN_RE finds "pin" meaning the Ghostty reference commit (the port's internal word);
# Ghostty's own Pin type ("track a pin", "viewport pin", "untrack_pin") does not match.
REFERENCE_PIN_RE = re.compile(
    r"\b(?:the|at|current|new|each|Ghostty's)(?: (?:first|last|next|previous|current|new|old))? pin\b"
    r"|\bpin (?:moves?|moved|advances?)\b"
    r"|\b[Cc]urrent-pin\b|\b[Pp]in [0-9a-f]{7,}\b|\bPin \+\d|\b[Uu]pstream pin\b|\bresync\w*|\boracle\b")
# PIN_ALLOWED are strings that use "the pin" for Ghostty's Pin type, quote a ledger row, or name
# macOS's display-link resync; PIN_KEYS are data ids, never shown (agents-vs-human shows "upstream").
PIN_ALLOWED = ("removes the pin with", "matching the oracle", "display-link resync")
PIN_KEYS = {"resync"}
# SUBJECT_KEYS hold quoted commit subjects (git history, shown verbatim).
SUBJECT_KEYS = {"_meta", "subjects", "s"}


class PlainWordsTest(unittest.TestCase):
    """Generated page data never says "pin", "resync" or "oracle" for the reference commit."""

    def test_no_reference_pin_wording_in_visible_data(self):
        """Scan every generated JSON file's strings, skipping metadata and quoted commit subjects."""
        hits = []

        def walk(o, where):
            """Collect matches in every visible string under o."""
            if isinstance(o, dict):
                for k, v in o.items():
                    if k not in SUBJECT_KEYS:
                        walk(v, f"{where}.{k}")
            elif isinstance(o, list):
                for v in o:
                    walk(v, where)
            elif (isinstance(o, str) and o not in PIN_KEYS and REFERENCE_PIN_RE.search(o)
                  and not any(a in o for a in PIN_ALLOWED)):
                hits.append((where, REFERENCE_PIN_RE.search(o).group(0), o[:80]))

        data_dir = Path(__file__).resolve().parents[2] / "src/app/porting/data"
        for f in sorted(data_dir.glob("*.json")):
            if f.name != "facts.json":
                walk(json.loads(f.read_text()), f.stem)
        self.assertEqual(hits, [])


class AgentsVsHumanDataTest(unittest.TestCase):
    """agents-vs-human: the 111-day × 24-hour grid, commit lags, prompts and annotations."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["agents-vs-human"](RESEARCH, MOCKBUILD)
        cls.summary = json.loads((RESEARCH / "data/mock-agents-vs-human-summary.json").read_text())

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "agents-vs-human")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_grid_is_111_days_by_24_hours(self):
        days = self.data["daily"]["date"]
        self.assertEqual(len(days), 111)
        self.assertEqual((days[0], days[-1]), ("2026-06-20", "2026-10-08"))
        for k in ("commits", "merges", "prompts", "added", "deleted"):
            self.assertEqual(len(self.data["grid"][k]), 111 * 24, k)

    def test_grid_commits_match_summary(self):
        self.assertEqual(sum(self.data["grid"]["commits"]), self.summary["n_commits"])
        self.assertEqual(self.summary["n_commits"], 2718)
        self.assertEqual(len(self.data["commits"]["t"]), self.summary["n_commits"])

    def test_origin_prompts_are_founding_prompts_before_the_first_commit(self):
        """Hollow rings: the approved founding sessions' prompts typed before the first commit (11)."""
        o = self.data["originPrompts"]
        self.assertEqual(o, sorted(o))
        self.assertEqual(len(o), 11)
        self.assertLess(max(o), min(self.data["commits"]["t"]))
        self.assertEqual(o[0], 21 * 60 + 42)  # 06-20 21:42 PT, the first founding prompt

    def test_prompts_match_grid(self):
        cells = [0] * (111 * 24)
        for t in self.data["prompts"]:
            cells[t // 60] += 1
        self.assertEqual(cells, self.data["grid"]["prompts"])

    def test_share_over_one_hour_unattended(self):
        lags = self.data["commits"]["lag"]
        share = sum(1 for x in lags if x > 60) / len(lags)
        self.assertAlmostEqual(share, 0.52, delta=0.005)

    def test_longest_unattended_run_is_180(self):
        import bisect
        prompts, counts = self.data["prompts"], {}
        for t in self.data["commits"]["t"]:
            i = bisect.bisect_left(prompts, t + 1) - 1
            counts[i] = counts.get(i, 0) + 1
        self.assertEqual(max(counts.values()), 180)

    def test_series_match_mockup_embed(self):
        mock = json.loads((MOCKBUILD / "w02-data.json").read_text())
        g = self.data["grid"]
        self.assertEqual([g["commits"], g["merges"], g["prompts"], g["added"], g["deleted"]],
                         [mock["G"][k] for k in "cmpad"])
        self.assertEqual([self.data["commits"]["t"], self.data["commits"]["lag"]], [mock["C"]["t"], mock["C"]["l"]])
        dy = self.data["daily"]
        self.assertEqual([dy["mix"], dy["costMacClaude"], dy["lost"], dy["threads"]],
                         [mock["DY"][k] for k in ("mix", "cm", "lost", "th")])
        self.assertEqual(sorted(self.data["subjects"]), sorted(mock["S"]))

    def test_no_process_labels_in_any_string(self):
        label = re.compile(r"\b(?:phases? ?-?\d|phase[- ][a-z]\b|wave[- ]?\d|chunk[- ]?\d|(?:sub-)?batch[- ]?\d|"
                           r"sub-project #\d|r\d+-[a-z]+|task[- ]\S*\d|task t-|vt-\d+[a-z]|stage [a-z]\b|"
                           r"tier-\d|par-\d|t-[a-z]\b|tail-s\d|\(\s*\d+/n\s*\))", re.IGNORECASE)
        for s in walk_strings(self.data):
            self.assertNotRegex(s, label, s)

    def test_catch_up_numbers_match_moving_target(self):
        """Every catch-up era is numbered by its reference move in the moving-target data,
        and each move inside this widget's window (it stops at 2026-10-08, ruling R33) appears
        exactly once, in order."""
        from widgets import REGISTRY
        adv = REGISTRY["moving-target"](RESEARCH, MOCKBUILD)["adv"]
        moves = [(a["n"], a["from"], a["to"]) for a in adv if a["adv"][:10] <= "2026-10-08"]
        self.assertEqual(len(moves), 7)
        seen = []
        for grp, _s, _e, label in self.data["eras"]:
            if grp != "resync":
                continue
            m = re.match(r"^Upstream catch-up (\d+) \((\w+) → (\w+)\b", label)
            self.assertIsNotNone(m, label)
            seen.append((int(m.group(1)), m.group(2), m.group(3)))
        self.assertEqual(seen, moves)

    def test_no_internal_ids_and_no_bun(self):
        for s in walk_strings(self.data):
            self.assertEqual(jargon.strip_internal_ids(s), (s, False), s)
            self.assertNotRegex(s, r"\bBun\b", s)


class TestContractDataTest(unittest.TestCase):
    """test-contract: per-module Zig counts per reference commit, Rust counts per day, skip census."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["test-contract"](RESEARCH, MOCKBUILD)

    @staticmethod
    def _expand(changes, n):
        """Expand [[day, value], …] change points into a value per day."""
        out, k, v = [], 0, 0
        for i in range(n):
            while k < len(changes) and changes[k][0] <= i:
                v = changes[k][1]
                k += 1
            out.append(v)
        return out

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "test-contract")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_112_days_and_9_reference_commits(self):
        self.assertEqual(len(self.data["dates"]), 112)
        self.assertEqual((self.data["dates"][0], self.data["dates"][-1]), ("2026-06-21", "2026-10-10"))
        for k in ("all", "vt", "vtc"):
            self.assertEqual(len(self.data["rust"][k]), 112, k)
        self.assertEqual(len(self.data["revs"]), 112)
        self.assertEqual(self.data["revs"][-1], "58aa62a81")
        self.assertEqual(len(self.data["pins"]), 9)
        self.assertEqual(len(self.data["pins"]) - 1, 8, "8 resync markers")
        self.assertEqual([p["src"] for p in self.data["pins"]][::8], [3189, 4433])
        self.assertEqual(self.data["rust"]["all"][-1], 7287)

    def test_4182_squares_at_current_reference(self):
        mods = self.data["mods"]
        self.assertEqual(sum(o["z"][-1] for o in mods), 4182)
        self.assertEqual(sum(max(o["z"]) for o in mods), 4182)
        self.assertEqual(self.data["pins"][-1]["mapped"], 4182)
        self.assertEqual(self.data["pins"][-1]["upstream"], 12)

    def test_eighth_move_was_covered_in_advance(self):
        """Contract_precovered 616 of 1,049: the eighth move's 10 new Zig tests all fell inside existing Rust surplus."""
        mods, last_day = self.data["mods"], len(self.data["dates"]) - 1
        added = [(o["m"], o["z"][-1] - o["z"][-2]) for o in mods if o["z"][-1] > o["z"][-2]]
        self.assertEqual(sum(n for _, n in added), 10)
        before = self.data["dates"].index("2026-10-08")
        for o in mods:
            if o["z"][-1] > o["z"][-2]:
                self.assertGreaterEqual(self._expand(o["r"], before + 1)[before], o["z"][-1], o["m"])
        self.assertGreaterEqual(last_day, before)

    def test_211_modules_none_below_diagonal(self):
        mods = self.data["mods"]
        self.assertEqual(len(mods), 211)
        n = len(self.data["dates"])
        for o in mods:
            self.assertGreaterEqual(self._expand(o["r"], n)[-1], o["z"][-1], o["m"])

    def test_silent_skips(self):
        sk = self.data["skip"]
        self.assertEqual(sum(sk["byKind"].values()), 185)
        self.assertEqual(sum(sk["byCls"].values()), 185)
        self.assertEqual(sum(n for _, n in sk["byCrate"]), 185)
        self.assertEqual(sk["grp"]["compile-time scope"], 134)
        self.assertEqual(sk["grp"]["runtime resource"], 51)
        self.assertEqual(sk["zigPort"], 68)
        self.assertEqual((sk["zig"]["total"], sk["zig"]["skip"], sk["zig"]["build"], sk["zig"]["runtime"]),
                         (4423, 199, 167, 32))
        self.assertEqual(sk["rust"]["total"], 7270)
        # Fact-check (BRIEF row 92): 136 carry only a platform/feature ignore, 152 counting
        # the 16 that also have a Miri ignore; 475 Miri ignores.
        self.assertEqual(sk["rust"]["cfgIgnore"] + sk["rust"]["miriAndCfgIgnore"], 152)
        self.assertEqual(sk["rust"]["miri"], 475)

    def test_series_match_mockup_embed(self):
        """The mockup's 110 days and 8 reference commits are an exact prefix of the 2026-10-10 snapshot."""
        mock = json.loads((MOCKBUILD / "w03_embed.json").read_text())
        n, k = len(mock["dates"]), len(mock["pins"])
        self.assertEqual((n, k), (110, 8))
        self.assertEqual(self.data["dates"][:n], mock["dates"])
        self.assertEqual(self.data["revs"][:n], mock["revs"])
        self.assertEqual([[o["m"], o["c"], o["z"][:k], [p for p in o["r"] if p[0] < n]] for o in self.data["mods"]],
                         [[m, c, z, [p for p in r if p[0] < n]] for m, c, z, r in mock["mods"]])
        # The mockup kept the CSV's pre-recount 3,153 for the first mapped total; the
        # data uses the recounted per-module sum (3,134, the day-1 contract size).
        mock["pins"][0][2] = 3134
        self.assertEqual([[p["date"], p["sha"], p["mapped"], p["src"], p["upstream"]] for p in self.data["pins"][:k]],
                         mock["pins"])
        r = self.data["rust"]
        self.assertEqual([[a, v, c] for a, v, c in zip(r["all"][:n], r["vt"][:n], r["vtc"][:n])],
                         [d[4:7] for d in mock["daily"]])
        # The builder shows the "oracle" guard class in plain words; counts are unchanged.
        mock["skip"]["byCls"]["reference build / corpus / git checkout"] = mock["skip"]["byCls"].pop(
            "oracle / corpus / git checkout")
        for k in ("byCls", "byKind", "byCrate", "grp", "zigPort"):
            self.assertEqual(self.data["skip"][k], mock["skip"][k], k)

    def test_no_internal_ids_and_no_bun(self):
        # _meta holds source file names (e.g. r2-skip-summary.csv), never shown on the page.
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.strip_internal_ids(s), (s, False), s)
            self.assertNotRegex(s, r"\bBun\b", s)


class CuttingTheCycleDataTest(unittest.TestCase):
    """cutting-the-cycle: the 345-file import cycle, its cuts, the 61-package crate graph and the replay."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["cutting-the-cycle"](RESEARCH, MOCKBUILD)

    @staticmethod
    def _largest_cross(n, edges, files):
        """Return the size of the largest multi-file cycle spanning more than one future crate."""
        adj = [[] for _ in range(n)]
        for a, b in edges:
            adj[a].append(b)
        index, low, on, stack, comps, counter = {}, {}, set(), [], [], [0]

        def dfs(v):
            """Tarjan's strongly-connected-components visit of file v."""
            index[v] = low[v] = counter[0]
            counter[0] += 1
            stack.append(v)
            on.add(v)
            for w in adj[v]:
                if w not in index:
                    dfs(w)
                    low[v] = min(low[v], low[w])
                elif w in on:
                    low[v] = min(low[v], index[w])
            if low[v] == index[v]:
                comp = []
                while True:
                    w = stack.pop()
                    on.discard(w)
                    comp.append(w)
                    if w == v:
                        break
                comps.append(comp)

        for v in range(n):
            if v not in index:
                dfs(v)
        cross = [len(c) for c in comps if len(c) > 1 and len({files[i][7] for i in c}) > 1]
        return max(cross, default=0)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "cutting-the-cycle")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_cycle_is_345_files_332_without_entry_points(self):
        z = self.data["zig"]
        self.assertEqual(len(z["files"]), 345)
        self.assertEqual(z["stats"]["scc"], 345)
        self.assertEqual(len(z["edges"]), 1410)
        self.assertEqual(z["stats"]["scc_edges"], 1410)
        self.assertEqual(z["stats"]["cum"][0], 332)
        self.assertEqual(sorted(z["files"][i][0] for i in z["entry"]), ["main_c.zig", "main_ghostty.zig"])

    def test_14_cuts_and_5_extra_seams(self):
        t = self.data["text"]
        self.assertEqual([c["n"] for c in t["cuts"]], list(range(1, 15)))
        self.assertEqual([c["n"] for c in t["extra"]], list(range(15, 20)))
        mech = {}
        for c in t["cuts"]:
            mech[c["m"]] = mech.get(c["m"], 0) + 1
        self.assertEqual(mech, {"trait": 6, "move": 5, "split": 2, "inject": 1})

    def test_crate_graph_61_packages_313_edges_depth_18(self):
        cr = self.data["crates"]
        self.assertEqual(len(cr["nodes"]), 61)
        self.assertEqual(max(n[2] for n in cr["nodes"]), 18)
        self.assertEqual(len(cr["edges"]), 313)
        kinds = {}
        for e in cr["edges"]:
            kinds[e[2]] = kinds.get(e[2], 0) + 1
        self.assertEqual(kinds, {"normal": 287, "dev": 21, "build": 5})
        self.assertEqual(len(cr["chain"]), 19)
        self.assertEqual(cr["cratesIo"], {"resolved": 138, "direct": 37})
        sys_nodes = [n for n in cr["nodes"] if n[0].endswith("-sys")]
        self.assertEqual(len(sys_nodes), 16)
        self.assertEqual(sum(n[24] for n in sys_nodes), 1058459, "vendored C/C++ lines")
        self.assertEqual(sum(n[3] for n in sys_nodes), 4100, "Rust glue lines")

    def test_plan_reproduces_the_shatter_curve(self):
        z = self.data["zig"]
        plan = {e: k for e, k in z["plan"]}
        entry = set(z["entry"])
        curve = []
        for k in range(16):
            kept = [(a, b) for i, (a, b, t) in enumerate(z["edges"])
                    if b not in entry and not (0 < plan.get(i, 0) <= k) and not (k >= 15 and t == 15)]
            curve.append(self._largest_cross(len(z["files"]), kept, z["files"]))
        self.assertEqual(curve, z["stats"]["cum"])
        self.assertEqual((curve[13], curve[14]), (219, 45))

    def test_matches_mockup_embed(self):
        mock = json.loads((RESEARCH / "data/mock-cutting-the-cycle-embed.json").read_text())
        # Fact-check (codebase.md row 7): rustty-vt has 7,984 unsafe sites, not the census's 7,985.
        vt = next(n for n in mock["crates"]["nodes"] if n[0] == "rustty-vt")
        self.assertEqual(vt[22], 7985)
        vt[22], vt[23] = 7984, round(7984 / vt[3] * 1000, 2)
        # The -sys series loses the 1,432 vendored HarfBuzz .rs lines from 2026-06-30 on.
        start, sys_lines = mock["replay"]["s"]["__sys"]
        mock["replay"]["s"]["__sys"] = [start, [v - 1432 if mock["replay"]["dates"][start + i] >= "2026-06-30" else v
                                                for i, v in enumerate(sys_lines)]]
        for k in ("zig", "crates", "replay"):
            mine = {kk: vv for kk, vv in self.data[k].items() if kk not in ("plan", "entry", "cratesIo")}
            self.assertEqual(mine, mock[k], k)

    def test_last_day_physical_lines_match_fact_check(self):
        r = self.data["replay"]
        last = r["n"] - 1
        total = sum(vals[min(last - start, len(vals) - 1)] for start, vals in r["s"].values() if last >= start)
        self.assertEqual(total, 836905)
        first = sum(vals[0] for start, vals in r["s"].values() if start == 0)
        self.assertEqual(first, 33040)

    def test_no_internal_ids_and_no_bun(self):
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.strip_internal_ids(s), (s, False), s)
            self.assertNotRegex(s, r"\bBun\b", s)


class TranslationLensDataTest(unittest.TestCase):
    """translation-lens: six Zig → Rust patterns with real excerpts, line mappings, ledger and counts."""

    GHOSTTY = Path("/Users/gael/projects/ghostty")
    RUSTTY = Path("/Users/gael/projects/rustty")

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["translation-lens"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "translation-lens")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_six_patterns_with_excerpts_and_mappings(self):
        pats = self.data["patterns"]
        self.assertEqual([p["id"] for p in pats], ["rawptr", "intrusive", "comptime", "packed", "errdefer", "bytes"])
        for p in pats:
            self.assertGreaterEqual(len(p["subs"]), 1, p["id"])
            for s in p["subs"]:
                zig = {ln[0] for ln in s["zig"] if ln}
                rust = {ln[0] for ln in s["rust"] if ln}
                self.assertTrue(zig and rust, (p["id"], s["key"]))
                self.assertGreaterEqual(len(s["map"]), 1, (p["id"], s["key"]))
                for z, r in s["map"]:
                    self.assertTrue(set(z) <= zig and set(r) <= rust, (p["id"], s["key"], z, r))
                for z, r in s["carried"] + s["echoes"]:
                    self.assertTrue(z in zig and r in rust, (p["id"], s["key"], z, r))
        self.assertEqual([s["key"] for s in pats[1]["subs"]], ["queue", "parent"])

    def test_ledger_314_rows_18_classes(self):
        cls = self.data["ledgerClasses"]
        self.assertEqual(len(cls), 18)
        self.assertEqual(sum(c["n"] for c in cls), 314)
        for c in cls:
            self.assertEqual(len(c["sample"]), min(8, c["n"]), c["cls"])
        self.assertEqual([p["ledgerCount"] for p in self.data["patterns"]], [31, 41, 11, 3, 6, 51])

    def test_code_ratio_1_68(self):
        mods = self.data["modules"]
        self.assertEqual(len(mods), 34)
        z = sum(m["zig"][0] for m in mods)
        r = sum(m["rust"][0] for m in mods)
        self.assertEqual((z, r), (302669, 507628))
        self.assertEqual(round(r / z, 2), 1.68)

    def test_excerpt_stats_62_of_73_carried(self):
        st = [s["stats"] for p in self.data["patterns"] for s in p["subs"]]
        self.assertEqual(len(st), 7)
        self.assertEqual(sum(x["carried"] for x in st), 62)
        self.assertEqual(sum(x["zig_comments"] for x in st), 73)

    def test_counts_match_counts_csv(self):
        with open(RESEARCH / "data/mock-translation-lens-counts.csv", newline="") as f:
            rows = list(csv.DictReader(f))
        mine = []
        for p in self.data["patterns"]:
            for it in [p["counts"]["primary"]] + p["counts"]["items"]:
                for side in ("zig", "rust", "ledger"):
                    if side in it:
                        mine.append((p["id"], side, int(it[side])))
        self.assertEqual(mine, [(r["pattern"], r["side"], int(r["count"])) for r in rows])

    def test_unsafe_census_fact_checked(self):
        self.assertEqual(self.data["unsafeTotal"], 22310)
        vt = next(c for c in self.data["unsafeCrates"] if c["crate"] == "rustty-vt")
        self.assertEqual(vt["sites"], 7984)
        self.assertTrue(all(c["sites"] >= 20 and c["tier"] != "sys" for c in self.data["unsafeCrates"]))
        h = self.data["unsafeHeur"]
        self.assertEqual(h["total"], 8027)
        self.assertEqual(sum(n for _k, n in h["cats"]), 8027)

    def test_excerpts_are_verbatim_except_marked_lines(self):
        """Every excerpt line equals the source at the pinned commits; marked lines only lose ids."""
        if not (self.GHOSTTY / ".git").exists() or not (self.RUSTTY / ".git").exists():
            self.skipTest("source checkouts not available")
        meta = self.data["commits"]
        xev = sorted(self.GHOSTTY.glob("zig-pkg/libxev-*"))[0]

        def lines(repo, commit, path):
            """Return the lines of path at commit (libxev from the vendored package)."""
            if path.startswith("libxev/"):
                return (xev / path[len("libxev/"):]).read_text().split("\n")
            out = subprocess.run(["git", "-C", str(repo), "show", f"{commit}:{path}"],
                                 capture_output=True, text=True, check=True).stdout
            return out.split("\n")

        marked = 0
        for p in self.data["patterns"]:
            for s in p["subs"]:
                for side, repo, commit, path in (("zig", self.GHOSTTY, meta["zig"], s["zigFile"]),
                                                 ("rust", self.RUSTTY, meta["rust"], s["rustFile"])):
                    src = lines(repo, commit, path)
                    for ln in s[side]:
                        if not ln:
                            continue
                        orig = src[ln[0] - 1]
                        if len(ln) > 2:
                            marked += 1
                            self.assertNotEqual(orig, ln[1])
                            self.assertTrue(orig.lstrip().startswith("//"), orig)
                            self.assertEqual(orig[: len(orig) - len(orig.lstrip())], ln[1][: len(ln[1]) - len(ln[1].lstrip())])
                        else:
                            self.assertEqual(orig, ln[1], (path, ln[0]))
        self.assertEqual(marked, 6)

    def test_no_internal_ids_and_no_bun(self):
        # Code and ledger text keep [] and () (strip_internal_ids's punctuation tidy is not
        # identity on code), so visible strings are checked for ids and process labels directly.
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.find_internal_ids(s), [], s)
            self.assertNotRegex(s, r"\bBun\b", s)
            self.assertNotRegex(s, r"\(\s*[,;/]|…\s*[,;:)]|(?:^| )\(\s*\)|mirihar|\.port/", s)

    def test_mapping_review_lists_every_hand_drawn_mapping(self):
        rows = (HERE / "mapping-review.txt").read_text().splitlines()
        body = [r for r in rows if r.startswith("translation-lens\t")]
        self.assertEqual(len(body), sum(len(s["map"]) for p in self.data["patterns"] for s in p["subs"]))


class ThreeReviewersDataTest(unittest.TestCase):
    """three-reviewers: four replayed bugs, the 37-event rail (test-isolation races excluded, R37/R39), borrow-stack steps and Miri output."""

    RUSTTY = Path("/Users/gael/projects/rustty")
    FIXES = ["3662e56bf", "f046d41a6", "9e3fe1c25", "dcae8d61e"]

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["three-reviewers"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "three-reviewers")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_four_bugs_with_fix_commits(self):
        bugs = self.data["bugs"]
        self.assertEqual([b["fix"]["commit"]["hash"] for b in bugs], self.FIXES)
        self.assertEqual([b["ref"] for b in bugs], self.FIXES)
        self.assertEqual([b["life"][-1]["hash"] for b in bugs], self.FIXES)

    def test_timeline_37_events_80_defects(self):
        tl = self.data["timeline"]
        self.assertEqual(len(tl), 37)
        self.assertEqual(sum(e["n"] for e in tl), 80)
        self.assertFalse(any("SO_REUSEPORT" in e["cls"] for e in tl), "UDP same-port test race excluded (R39)")
        self.assertEqual(sum(e["n"] for e in tl if "Miri" in e["tool"]), 62)
        self.assertEqual(sum(1 for e in tl if e["prod"]), 29)
        lanes = [ln["key"] for ln in self.data["lanes"]]
        self.assertEqual(lanes, ["sb", "tb", "harness", "review", "san", "symptom"])
        self.assertTrue(all(e["lane"] in lanes for e in tl))
        for b in self.data["bugs"]:
            self.assertEqual(sum(1 for e in tl if e["ref"] == b["ref"]), 1, b["key"])

    def test_steps_and_miri_output_for_both_models(self):
        want = {"clear": (5, 3), "shaper": (5, 3), "search": (6, 4), "mpsc": (5, 4)}
        for b in self.data["bugs"]:
            m = b["miri"]
            self.assertEqual((len(m["sb"]), len(m["tb"])), want[b["key"]], b["key"])
            for model in ("sb", "tb"):
                steps = m[model]
                self.assertEqual(steps[-1]["acc"]["ok"], False, (b["key"], model))
                self.assertTrue(all(st.get("acc", {}).get("ok", True) for st in steps[:-1]), (b["key"], model))
                self.assertTrue(any(it.get("st") == "bad" for it in steps[-1]["items"]), (b["key"], model))
                for st in steps:
                    self.assertTrue(st["at"] and st["items"] and st["say"], (b["key"], model))
            self.assertIn("Undefined Behavior", m["logSb"], b["key"])
            self.assertIn("Undefined Behavior", m["logTb"], b["key"])
            self.assertIn("Tree Borrows", m["logTb"], b["key"])
            self.assertTrue(m["cmd"].startswith("cargo +nightly miri "), b["key"])
            self.assertTrue(m["cmdTb"].startswith("MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri "), b["key"])

    def test_pattern_counts_match_csv(self):
        with open(RESEARCH / "data/mock-three-reviewers-pattern-counts.csv", newline="") as f:
            rows = list(csv.DictReader(f))
        mine = [(b["key"], label, n) for b in self.data["bugs"] for label, n in b["count"]["series"]]
        self.assertEqual(mine, [(r["bug"], r["snapshot"], int(r["count"])) for r in rows])
        self.assertEqual(len(self.data["bugs"][1]["count"]["births"]), 33)

    def test_code_excerpts_verbatim_at_pre_fix_commits(self):
        """Implementer excerpts of A, C and D equal the files at the commit before the fix."""
        if not (self.RUSTTY / ".git").exists():
            self.skipTest("rustty checkout not available")
        elided = 0
        for b, rev in zip(self.data["bugs"], ["c8053eccc", None, "9e3fe1c25~1", "dcae8d61e~1"]):
            if rev is None:
                continue
            for seg in b["impl"]["segs"]:
                src = subprocess.run(["git", "-C", str(self.RUSTTY), "show", f"{rev}:{seg['file']}"],
                                     capture_output=True, text=True, check=True).stdout.split("\n")
                for n, text in seg["lines"]:
                    if text.startswith("…"):
                        # The mockup elided the head of a comment line that cited an internal id.
                        elided += 1
                        self.assertTrue(src[n - 1].lstrip().startswith("//"), src[n - 1])
                        self.assertTrue(src[n - 1].endswith(text[1:]), (b["key"], n))
                    else:
                        self.assertEqual(src[n - 1], text, (b["key"], seg["file"], n))
        self.assertEqual(elided, 1)

    def test_no_internal_ids_paths_or_bun(self):
        # Code, logs and quotes keep [] and () (strip_internal_ids's punctuation tidy is
        # not identity on code), so visible strings are checked for ids directly.
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.find_internal_ids(s), [], s)
            self.assertNotRegex(s, r"\bBun\b|mirihar|\.port/|less capable|author to confirm|glossary", s)


class MemorySafetyDataTest(unittest.TestCase):
    """memory-safety: 39 curated events (83 defects; the two test-isolation races are left out, R37/R39), grouped by detector, class and crate, plus 8 rule flags."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["memory-safety"](RESEARCH, MOCKBUILD)

    def test_catch_up_wording_matches_three_reviewers(self):
        """An event both widgets show names the same upstream catch-up (9d8fafb82: the first)."""
        from widgets import REGISTRY
        rail = {(e["d"], e["ref"]): e["tool"] for e in REGISTRY["three-reviewers"](RESEARCH, MOCKBUILD)["timeline"]}
        mine = {(e["d"], e["ref"]): e["tool"] for e in self.data["events"]}
        shared = [k for k in rail.keys() & mine.keys() if "catch-up" in rail[k] + mine[k]]
        self.assertIn(("2026-07-30", "9d8fafb82"), shared)
        for k in shared:
            self.assertEqual(rail[k], mine[k], k)
        self.assertEqual(mine[("2026-07-30", "9d8fafb82")],
                         "Miri (terminal-engine sweep during the first upstream catch-up)")

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "memory-safety")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_invariants(self):
        ev = self.data["events"]
        self.assertEqual(len(ev), 39)
        self.assertEqual(sum(e["n"] for e in ev), 83)
        self.assertFalse(any("SO_REUSEPORT" in e["bc"] for e in ev), "UDP same-port test race excluded (R39)")
        self.assertFalse(any(e["ref"] == "490a0e122" for e in ev), "test-isolation race excluded (R37)")
        self.assertEqual(sum(e["n"] for e in ev if e["lane"] in ("sb", "har", "tb")), 64)
        self.assertEqual(sum(e["n"] for e in ev if e["cls"] in ("alias", "prov")), 62)
        self.assertEqual(sum(1 for e in ev if e["scope"].startswith("production")), 29)
        self.assertEqual(sum(e["n"] for e in ev if e["scope"].startswith("production")), 61)
        self.assertEqual(len(self.data["rules"]), 8)

    def test_lane_totals_match_the_spec(self):
        tot = {}
        for e in self.data["events"]:
            tot[e["lane"]] = tot.get(e["lane"], 0) + e["n"]
        self.assertEqual([tot[k] for k in ("sb", "har", "tb", "san", "rev", "sym")], [30, 19, 15, 7, 4, 8])

    def test_rules_cite_commits_in_date_order(self):
        rules = self.data["rules"]
        self.assertEqual([r["ref"] for r in rules], ["3662e56bf", "6cf99b4cd", "f046d41a6", "8f6d781dc",
                                                     "90fa4ff71", "40727a2cb", "6a07ed468", "5e8cbd918"])
        self.assertEqual([r["d"] for r in rules], sorted(r["d"] for r in rules))

    def test_refs_are_commits_or_ledger_counts(self):
        for e in self.data["events"]:
            self.assertTrue(bool(e["ref"]) != bool(e["entries"]), e)

    def test_no_internal_ids_and_no_bun(self):
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            # Code-like summaries keep () and []; check for ids directly (as three-reviewers does).
            self.assertEqual(jargon.find_internal_ids(s), [], s)
            self.assertNotRegex(s, r"\bBun\b|mirihar|\.port/", s)


class MovingTargetDataTest(unittest.TestCase):
    """moving-target: 8 reference advances absorbing 1,169 upstream non-merge commits, with the size series and area matrix."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["moving-target"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "moving-target")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_eight_advances_and_commit_sum(self):
        adv = self.data["adv"]
        self.assertEqual(len(adv), 8)
        self.assertEqual(sum(a["nm"] for a in adv), 1169)
        self.assertEqual([a["n"] for a in adv], [1, 2, 3, 4, 5, 6, 7, 8])
        self.assertEqual((adv[-1]["to"], adv[-1]["commit"], adv[-1]["cm"]), ("246f70287", "2bf89b95a", 19))

    def test_wait_hours(self):
        lags = [a["lag"] for a in self.data["adv"]]
        self.assertEqual(lags[0], 47.6)
        self.assertEqual(min(lags), 1.7)
        self.assertEqual(lags, [47.6, 8.8, 1.7, 3.5, 1.9, 7.4, 14.9, 7.7])
        srt = sorted(lags)
        self.assertEqual(round((srt[3] + srt[4]) / 2, 2), 7.55, "median of the eight waits (fact wait_median)")

    def test_pin_chain_and_sizes(self):
        adv = self.data["adv"]
        self.assertEqual(adv[0]["from"], "4749c4e93")
        self.assertEqual(adv[-1]["to"], "246f70287")
        self.assertEqual(len([adv[0]["from"]] + [a["to"] for a in adv]), 9, "9 reference commits: baseline + 8 moves")
        for a, b in zip(adv, adv[1:]):
            self.assertEqual(a["to"], b["from"])
        self.assertEqual(self.data["baseline"]["loc"], 273827)
        self.assertEqual(adv[-1]["loc"], 367013)

    def test_area_matrix_sums_to_the_range_diffstat(self):
        total_ins = total_del = 0
        for i, a in enumerate(self.data["adv"]):
            cells = [r["v"][i] for r in self.data["rows"] if r["v"][i]]
            ins = sum(c[0] for c in cells)
            dele = sum(c[1] for c in cells)
            if a["n"] != 4:
                self.assertEqual((ins, dele), (a["ins"], a["del"]), a["n"])
            total_ins += ins
            total_del += dele
        self.assertEqual((total_ins, total_del), (185834, 33878))
        self.assertEqual(len(self.data["rows"]), 20)
        self.assertGreater(len(self.data["otherNames"]), 5)

    def test_series(self):
        self.assertEqual(self.data["gh"][0], ["2026-06-20", 273827])
        self.assertEqual(self.data["gh"][-1], ["2026-10-09", 367013])
        self.assertEqual(self.data["rs"][0], ["2026-06-21", 33040])
        self.assertEqual(self.data["rs"][-1], ["2026-10-10", 859089])

    def test_no_internal_ids_and_no_bun(self):
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.strip_internal_ids(s), (s, False), s)
            self.assertNotRegex(s, r"\bBun\b|oracle|(?i:finale)|\.port/", s)


class PerfReplayDataTest(unittest.TestCase):
    """perf-replay: 14 benchmarks, the 14 recorded sweeps of 07-15/16, side branches and the measured pin slot."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["perf-replay"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "perf-replay")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_fourteen_benchmarks_and_sweeps(self):
        self.assertEqual(len(self.data["bench"]), 14)
        stages = self.data["stages"]
        self.assertEqual(len(stages), 14)
        for st in stages:
            self.assertRegex(st["t"], r"^2026-07-1[56]T")
            self.assertEqual(sorted(st["rust"]), sorted(self.data["bench"]))
            self.assertEqual(sorted(st["zig"]), sorted(self.data["bench"]))

    def test_final_july_suite(self):
        last = self.data["stages"][-1]
        self.assertEqual((round(last["suiteR"]), round(last["suiteZ"])), (2269, 2897))
        self.assertEqual(last["suiteD"], -21.7)
        self.assertEqual((last["W"], last["T"], last["L"]), (14, 0, 0))
        self.assertEqual(last["geo"], 0.758)
        first = self.data["stages"][0]
        self.assertEqual((first["W"], first["suiteD"]), (0, 42.0))

    def test_official_stages_match_the_scoreboard(self):
        with open(RESEARCH / "data/perf-scoreboard-by-stage.csv", newline="") as f:
            board = list(csv.DictReader(f))
        by_key = {s["key"]: s for s in self.data["stages"]}
        for key, row in zip(["base", "session", "profile", "round1", "round2", "round3", "round5"], board):
            st = by_key[key]
            self.assertEqual((st["W"], st["T"], st["L"]),
                             (int(row["wins"]), int(row["ties_abs_lt_1.5pct"]), int(row["losses"])), key)
            self.assertAlmostEqual(st["suiteR"], float(row["suite_rust_ms"]), delta=0.15)
            self.assertEqual(st["suiteD"], float(row["suite_delta_pct"]), key)

    def test_lto_detour_and_negative_results(self):
        tp = [c for c in self.data["lto"]["chain"] if "terminal-parser" in c["ms"]]
        self.assertEqual(round(tp[-1]["ms"]["terminal-parser"]), 531)
        self.assertEqual(len(self.data["lto"]["chain"]), 12)
        self.assertEqual(len(self.data["lto"]["profiles"]), 5)
        self.assertEqual(len(self.data["negatives"]), 11)
        last = self.data["lto"]["chain"][-1]
        self.assertEqual((last["commit"], len(last["ms"])), ("d211a84", 14))
        self.assertEqual(round(sum(last["ms"].values())), 2948)

    def test_provisional_remeasure_comes_from_the_timeline(self):
        """The 07-30 "13/13" headline is read from perf-timeline.csv, not typed in."""
        self.assertEqual(self.data["provisional"], {"date": "2026-07-30", "wins": "13/13"})

    PIN_HEADER = ("bench,ghostty_ms,ghostty_min_ms,ghostty_max_ms,rustty_ms,rustty_min_ms,rustty_max_ms,"
                  "date,rustty_head,provenance\n")

    def _pin(self, lines):
        """Return current_pin over a temporary research dir whose PIN_FILE holds lines (after the header)."""
        from widgets import w05_perf_replay
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "data").mkdir()
            (Path(tmp) / "data" / w05_perf_replay.PIN_FILE).write_text(self.PIN_HEADER + "".join(lines))
            return w05_perf_replay.current_pin(Path(tmp), self.data["bench"])

    def test_current_pin_slot_is_measured(self):
        """The slot at 246f70287 holds exactly the 14 suite benchmarks, positive medians, provenance MEASURED."""
        pin = self.data["pin"]
        self.assertEqual((pin["rev"], pin["status"], pin["head"], pin["date"]),
                         ("246f70287", "measured", "11b4c74d0", "2026-10-10"))
        self.assertEqual(sorted(r["bench"] for r in pin["rows"]), sorted(self.data["bench"]))
        for r in pin["rows"]:
            self.assertGreater(r["rustty"][0], 0, r)
            self.assertGreater(r["ghostty"][0], 0, r)
        with open(RESEARCH / "data" / "perf-ratio-pin-246f70287.csv", newline="") as f:
            self.assertEqual({r["provenance"] for r in csv.DictReader(f)}, {"MEASURED"})
        wins = sum(r["rustty"][0] < r["ghostty"][0] for r in pin["rows"])
        sum_r = sum(r["rustty"][0] for r in pin["rows"])
        sum_g = sum(r["ghostty"][0] for r in pin["rows"])
        self.assertEqual((wins, round((sum_r / sum_g - 1) * 100, 1)), (14, -27.4), "facts perf_wins_pin, perf_suite_delta_pin")
        self.assertIn("terminal-stream", pin["note"])

    def test_current_pin_accepts_empty_min_max(self):
        """The 246f70287 run kept medians only: empty min/max become null, not an error."""
        for r in self.data["pin"]["rows"]:
            self.assertEqual((r["rustty"][1:], r["ghostty"][1:]), ([None, None], [None, None]), r["bench"])
        lines = [f"{b},10,,,9,,,2026-10-10,abc123def,MEASURED\n" for b in self.data["bench"]]
        lines[0] = f"{self.data['bench'][0]},590,580,600,130,128,133,2026-10-10,abc123def,MEASURED\n"
        pin = self._pin(lines)
        self.assertEqual(pin["rows"][0]["rustty"], [130.0, 128.0, 133.0])
        self.assertEqual(pin["rows"][1]["rustty"], [9.0, None, None])

    def test_current_pin_rejects_bad_files(self):
        """Fewer than 14 benchmarks, a non-positive median, a provenance other than MEASURED, or mixed runs raise."""
        good = [f"{b},10,,,9,,,2026-10-10,abc123def,MEASURED\n" for b in self.data["bench"]]
        bad = [good[:13],
               [good[0].replace(",9,", ",0,")] + good[1:],
               [good[0].replace("MEASURED", "ESTIMATED")] + good[1:],
               [good[0].replace("2026-10-10", "2026-10-09")] + good[1:],
               good[:13] + [good[0]]]
        for lines in bad:
            with self.assertRaises(ValueError):
                self._pin(lines)

    def test_side_tracks(self):
        self.assertEqual(len(self.data["full33"]), 33)
        self.assertEqual(len(self.data["advPin"]["rows"]), 14)
        self.assertEqual(len(self.data["apc"]), 9)
        self.assertEqual(len(self.data["gaps"]), 11)
        self.assertEqual(len(self.data["commits"]), 37)
        self.assertEqual(len(self.data["waterfall"]), 5)

    def test_no_internal_ids_round_labels_or_bun(self):
        # Code-like subjects keep () (the stripper's tidy drops it), so those are checked for ids only.
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.find_internal_ids(s), [], s)
            if "()" not in s:
                self.assertEqual(jargon.strip_internal_ids(s), (s, False), s)
            self.assertNotRegex(s, r"\bBun\b|\.port/|SHIP-RF|rustty-bench-|NONE-SAFE|WS10|\br\d\b|TIER-", s)


class DefectMapDataTest(unittest.TestCase):
    """defect-map: the 206 ledger entries on Ghostty's Zig treemap, plus the lazily loaded detail texts."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data and its detail chunk once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["defect-map"](RESEARCH, MOCKBUILD)
        cls.details = REGISTRY["defect-map-details"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        for slug, d in (("defect-map", self.data), ("defect-map-details", self.details)):
            self.assertIn("_meta", d)
            self.assertEqual(d["_meta"]["slug"], slug)
            self.assertLess(len(json.dumps(d)), 200 * 1024, slug)

    def test_counts(self):
        defs = self.data["defs"]
        self.assertEqual(len(defs), 206)
        self.assertEqual(sum(1 for d in defs if d["p"] == "ghostty"), 173)
        self.assertEqual(sum(1 for d in defs if d["p"] != "ghostty"), 33)
        live = {"crash", "ub", "leak", "hang", "intent"}
        self.assertEqual(sum(1 for d in defs if d["c"] in live), 136)
        cls = {}
        for d in defs:
            cls[d["c"]] = cls.get(d["c"], 0) + 1
        self.assertEqual(cls, {"crash": 60, "ub": 19, "leak": 14, "hang": 9, "intent": 34, "latent": 59, "test": 11})

    def test_releasefast_split(self):
        rf = [d["rf"] for d in self.data["defs"] if d["c"] == "crash"]
        self.assertEqual((rf.count("ub"), rf.count("wrong"), rf.count("term")), (37, 13, 10))
        # UD131's published ReleaseFast field: the process still aborts (ruling R19).
        self.assertEqual(next(d for d in self.data["defs"] if d["n"] == 131)["rf"], "term")
        self.assertTrue(all(d["rf"] == "" for d in self.data["defs"] if d["c"] != "crash"))

    def test_upstream_status(self):
        defs = self.data["defs"]
        fixed = [d["p"] for d in defs if d["s"] == "fixed"]
        self.assertEqual(sorted(fixed), ["ghostty", "ghostty", "ghostty", "google-highway"])
        st = {}
        for d in defs:
            st[d["s"]] = st.get(d["s"], 0) + 1
        self.assertEqual(st, {"nr": 194, "partly": 5, "fixed": 4, "priv": 2, "obsolete": 1})
        self.assertEqual(self.data["reported"], 0)

    def test_tree_and_placement(self):
        tree = self.data["tree"]
        self.assertEqual((len(tree), sum(len(t["k"]) for t in tree)), (46, 276))
        self.assertEqual(sum(t["v"] for t in tree), 367013)
        for t in tree:
            self.assertEqual(sum(v for _n, v in t["k"]), t["v"], t["n"])
        leaves = {n for t in tree for n, _v in t["k"]}
        for d in self.data["defs"]:
            if d["p"] == "ghostty" and d["leaf"] != "nonzig":
                self.assertIn(d["leaf"], leaves, d["t"])
            elif d["p"] != "ghostty":
                self.assertEqual(d["leaf"], "dep:" + d["p"])
        self.assertEqual(sum(1 for d in self.data["defs"] if d["leaf"] == "nonzig"), 4)
        term = next(t for t in tree if t["n"] == "src/terminal")
        self.assertEqual(term["v"], 182418)

    def test_method_buckets(self):
        m = {}
        for d in self.data["defs"]:
            m[d["m"]] = m.get(d["m"], 0) + 1
        self.assertEqual(m, {"read": 93, "arith": 30, "xplat": 25, "compile": 17, "fault": 12, "testport": 11,
                             "types": 5, "build": 4, "live": 4, "san": 3, "withheld": 2})
        conf = [d["conf"] for d in self.data["defs"]]
        self.assertEqual((conf.count("h"), conf.count("m"), conf.count("k")), (74, 23, 109))

    def test_ids_hidden_and_private_entries_published(self):
        for d in self.data["defs"]:
            self.assertNotIn("id", d)
            self.assertIsInstance(d["n"], int)
        nums = [d["n"] for d in self.data["defs"]]
        self.assertEqual(len(set(nums)), 206)
        det = {row[0]: row[1:] for row in self.details["d"]}
        self.assertEqual(sorted(det), sorted(nums))
        for n, path in ((131, "src/terminal/Screen.zig"), (351, "src/termio/stream_handler.zig")):
            d = next(x for x in self.data["defs"] if x["n"] == n)
            self.assertEqual(d["f"], path)
            mech, rfast, fix, _us = det[n]
            self.assertNotIn("Withheld", mech)
            self.assertTrue(mech and rfast and fix, n)

    def test_hashes_are_commits(self):
        for d in self.data["defs"]:
            self.assertRegex(d["k"], r"^[0-9a-f]{9}$")
            self.assertRegex(d["dt"], r"^2026-\d\d-\d\d$")

    def test_no_internal_ids_or_bun(self):
        # Ledger text quotes code (`()`, `[]`), which strip_internal_ids's punctuation tidy is not
        # identity on, so code-ish strings are checked with find_internal_ids; plain ones must also
        # pass strip_internal_ids unchanged apart from its edge trim (a quoted `glist;` ends a text).
        for data in (self.data, self.details):
            visible = {k: v for k, v in data.items() if k != "_meta"}
            for s in walk_strings(visible):
                self.assertEqual(jargon.find_internal_ids(s), [], s)
                if not re.search(r"[()\[\]]", s):
                    self.assertEqual(jargon.strip_internal_ids(s)[0], s.strip(" :;,-"), s)
                self.assertNotRegex(s, r"\bBun\b|\.port/|PORT NOTE|\bUD\d", s)


class VerificationMatrixDataTest(unittest.TestCase):
    """verification-matrix: 11 tools x 4 hosts across 9 reference commits; every lane at its latest completed run; no pending cell."""

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["verification-matrix"](RESEARCH, MOCKBUILD)

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "verification-matrix")
        self.assertEqual(self.data["_meta"]["asOf"], "246f70287")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_shape(self):
        self.assertEqual(len(self.data["tools"]), 11)
        self.assertEqual(len(self.data["hosts"]), 4)
        self.assertEqual(len(self.data["pins"]), 9)
        self.assertEqual([p["sha"] for p in self.data["pins"]],
                         ["4749c4e93", "6ad1fe7d8", "12967b68f", "683d8db64", "88f57ee66",
                          "20abdb50a", "12752b2ac", "a60e9e2a5", "246f70287"])
        self.assertEqual([p["label"] for p in self.data["pins"]][-2:], ["move 7 · previous", "move 8 · current"])
        tools = {t["key"] for t in self.data["tools"]}
        hosts = {h["key"] for h in self.data["hosts"]}
        for r in self.data["runs"]:
            self.assertIn(r["tool"], tools)
            self.assertIn(r["host"], hosts)
            self.assertIn(r["st"], ("green", "fixed", "incomplete", "pending"))
            self.assertTrue(0 <= r["pin"] < 9)
        for k in self.data["na"]:
            t, h = k.split("|")
            self.assertTrue(t in tools and h in hosts, k)

    def test_no_pending_cell_where_a_result_exists(self):
        """Every lane that was pending at a60e9e2a5 now holds its result; no cell anywhere is pending."""
        self.assertEqual([r for r in self.data["runs"] if r["st"] == "pending"], [])
        prev = {(r["tool"], r["host"]): r for r in self.data["runs"] if r["pin"] == 7}
        for lane in [("asan", "lin"), ("lsan", "lin"), ("msan", "lin"), ("tsan", "lin"), ("hwasan", "lin"),
                     ("vg", "lin"), ("sb", "mac"), ("tb", "mac"), ("capi", "mac")]:
            self.assertIn(prev[lane]["st"], ("green", "fixed"), lane)
        self.assertEqual(prev[("tsan", "lin")]["port"], 1, "the Oniguruma init race")
        self.assertEqual(prev[("tb", "mac")]["port"], 2, "two test-only Tree Borrows bugs")

    def test_current_pin_lanes(self):
        """At 246f70287: the Mac lanes, Valgrind and the Linux and Windows workspace runs; the Linux sanitizers not yet."""
        cur = {(r["tool"], r["host"]): r for r in self.data["runs"] if r["pin"] == 8}
        self.assertEqual(set(cur), {("sb", "mac"), ("tb", "mac"), ("capi", "mac"), ("asan", "mac"), ("lsan", "mac"),
                                    ("tsan", "mac"), ("ts", "mac"), ("xb", "mac"), ("vg", "lin"), ("ts", "lin"),
                                    ("xb", "lin"), ("ts", "msvc")})
        self.assertEqual(cur[("ts", "msvc")]["tests"], 6418)
        self.assertEqual(cur[("vg", "lin")]["tests"], 6746)
        self.assertIsNone(cur[("sb", "mac")]["secs"], "Mac timings from the contended full run are not shown (R34)")
        self.assertEqual(cur[("ts", "msvc")]["secs"], 948)
        self.assertIn("still to come", self.data["pins"][8]["note"])
        self.assertNotIn("pending", self.data["pins"][7]["note"])

    def test_headline_lanes_at_their_latest_run(self):
        """The eight lanes of 61,403 (first complete matrix), each at its latest completed run: 58,767 (fact sanitizer_runs_latest)."""
        h = self.data["hero"]
        first = sum(r["tests"] for r in self.data["runs"]
                    if r["pin"] == 6 and r["tool"] in ("asan", "lsan", "msan", "tsan", "hwasan", "vg"))
        self.assertEqual(first, 61403)
        self.assertEqual(h["runs"], 58767)
        self.assertEqual(sum(x["tests"] for x in h["lanes"]), h["runs"])
        at = {(x["tool"], x["host"]): x["pin"] for x in h["lanes"]}
        self.assertEqual(at, {("asan", "mac"): "246f70287", ("lsan", "mac"): "246f70287", ("tsan", "mac"): "246f70287",
                              ("vg", "lin"): "246f70287", ("lsan", "lin"): "a60e9e2a5", ("msan", "lin"): "a60e9e2a5",
                              ("tsan", "lin"): "a60e9e2a5", ("hwasan", "lin"): "a60e9e2a5"})
        self.assertEqual((h["miriRef"], h["miriSecs"]), ("12752b2ac", 13354), "Miri seconds keep the first complete run")
        self.assertEqual(self.data["durationsRef"], "12752b2ac")

    def test_defect_totals(self):
        """Miri 41 (18 + 15 + 8); other lanes 10: test-only flakes are not port defects (ruling R34)."""
        runs = self.data["runs"]
        by = lambda tools, key: sum(r[key] for r in runs if r["tool"] in tools)
        self.assertEqual(by(("sb",), "port"), 18)
        self.assertEqual(by(("tb",), "port"), 15)
        self.assertEqual(by(("capi",), "port"), 8)
        self.assertEqual(by(("asan", "lsan", "msan", "tsan", "hwasan", "vg", "xb", "ts"), "port"), 10)
        self.assertEqual(self.data["hero"]["ghostty"], 3)

    def test_sixty_five_runs(self):
        self.assertEqual(len(self.data["runs"]), 65)

    def test_partial_counts_are_not_shown_as_cell_totals(self):
        runs = {(r["pin"], r["tool"], r["host"]): r for r in self.data["runs"]}
        for key in [(6, "ts", "lin"), (6, "sb", "mac"), (6, "tb", "mac")]:
            self.assertIsNone(runs[key]["tests"], key)
        self.assertIn("3,332", runs[(6, "ts", "lin")]["txt"])
        self.assertIn("3,104", runs[(6, "sb", "mac")]["txt"])
        self.assertFalse(runs[(7, "ts", "lin")]["targeted"], "the full Linux run replaced the targeted one")
        self.assertEqual(runs[(6, "ts", "msvc")]["tests"], 442, "393 + 4 + 45, all parts named")
        self.assertFalse(runs[(6, "asan", "mac")]["targeted"])

    def test_commit_note_matches_the_diamond_encoding(self):
        note = self.data["pins"][6]["note"]
        self.assertIn("diamond", note)
        self.assertNotIn("red-dot", note)

    def test_plain_words(self):
        text = " ".join(walk_strings({k: v for k, v in self.data.items() if k != "_meta"}))
        self.assertNotRegex(text, r"IOCP|XCTest|\bDLL\b|\bRED\b")

    def test_tools_are_spelled_out(self):
        names = {t["key"]: t["name"] for t in self.data["tools"]}
        self.assertEqual(names["asan"], "AddressSanitizer")
        self.assertEqual(names["sb"], "Miri Stacked Borrows")
        self.assertEqual(names["tb"], "Miri Tree Borrows")
        self.assertEqual(self.data["hosts"][0]["hw"], "MacBook Pro, Apple M4 Pro")

    def test_no_internal_ids_and_no_bun(self):
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.strip_internal_ids(s)[0], s, s)
            self.assertNotRegex(s, r"\bBun\b|\.port/|phase-\d|rvm", s)


class ExtractEmbedTest(unittest.TestCase):
    """extract_embed quotes bare integer object keys without touching ',3:' inside a string."""

    def test_integer_keys_quoted_but_strings_untouched(self):
        from widgets import extract_embed as ee
        got = ee.parse_literal('{0:"a, 3: b",1:"x",2:["{4:5}"]}')
        self.assertEqual(got, {"0": "a, 3: b", "1": "x", "2": ["{4:5}"]})


class VerificationMatrixLaneResultsTest(unittest.TestCase):
    """The per-lane results file for the last two reference commits fills, replaces and is validated."""

    HEADER = "pin,tool,host,status,tests,seconds,date,text,port_defects,ghostty_defects,source\n"

    def _build(self, body):
        """Build the widget data with a lane results file holding body (after the header)."""
        from widgets import w10_verification_matrix as w10
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "lanes.csv"
            path.write_text(self.HEADER + body)
            return w10.build(RESEARCH, MOCKBUILD, lanes_path=path)

    def test_flips_pending_cells_and_fills_the_current_pin(self):
        data = self._build("a60e9e2a5,asan,lin,green,9900,950,10-09,Clean run,0,0,Linux VM run\n"
                           "a60e9e2a5,lsan,lin,fixed,9900,1900,10-09,Fixed a leak,2,0,Linux VM run\n"
                           "246f70287,asan,mac,green,10000,1700,10-10,Clean run,0,0,Mac run\n")
        prev = {(r["tool"], r["host"]): r for r in data["runs"] if r["pin"] == 7}
        self.assertEqual(prev[("asan", "lin")]["st"], "green")
        self.assertEqual(prev[("asan", "lin")]["tests"], 9900)
        self.assertEqual(prev[("lsan", "lin")]["st"], "fixed")
        self.assertEqual(prev[("lsan", "lin")]["port"], 2)
        self.assertEqual(prev[("msan", "lin")]["st"], "pending", "a lane without a result may stay pending")
        self.assertEqual(len([r for r in data["runs"] if r["pin"] == 7]), 16)
        cur = [r for r in data["runs"] if r["pin"] == 8]
        self.assertEqual([(r["tool"], r["host"], r["tests"]) for r in cur], [("asan", "mac", 10000)])

    def test_a_row_replaces_an_existing_cell(self):
        data = self._build("a60e9e2a5,ts,lin,green,6000,431,10-08,Full run,0,0,Linux VM run\n")
        cell = [r for r in data["runs"] if (r["pin"], r["tool"], r["host"]) == (7, "ts", "lin")]
        self.assertEqual(len(cell), 1)
        self.assertEqual((cell[0]["tests"], cell[0]["targeted"]), (6000, False))

    def test_test_flakes_count_no_port_defect(self):
        data = self._build("246f70287,tsan,mac,fixed,3900,3306,10-10,Test-only flake,1,0,Mac run\n")
        cell = next(r for r in data["runs"] if (r["pin"], r["tool"], r["host"]) == (8, "tsan", "mac"))
        self.assertEqual((cell["st"], cell["port"]), ("fixed", 0))

    def test_rejects_a_wrong_pin_unknown_lane_status_or_duplicate(self):
        for body in ("12752b2ac,asan,lin,green,1,1,10-09,x,0,0,s\n",
                     "a60e9e2a5,nope,lin,green,1,1,10-09,x,0,0,s\n",
                     "a60e9e2a5,asan,lin,red,1,1,10-09,x,0,0,s\n",
                     "a60e9e2a5,msan,mac,green,1,1,10-09,x,0,0,s\n",
                     "a60e9e2a5,asan,lin,green,1,1,10-09,x,0,0,s\na60e9e2a5,asan,lin,green,2,1,10-09,y,0,0,s\n"):
            with self.assertRaises(ValueError, msg=body):
                self._build(body)

    def test_default_build_reads_the_snapshot_lane_file(self):
        from widgets import w10_verification_matrix as w10
        self.assertEqual(w10.LANES_FILE, "verification-lanes-2026-10-10.csv")
        self.assertTrue((RESEARCH / "data" / w10.LANES_FILE).exists())


class CostDataTest(unittest.TestCase):
    """cost: 17 quota weeks; recovered $11,032.43; modeled lost window $8,725 / $12,759 / $20,331; paid ~$422 to $600; delegated 75.6%."""

    EMBED = RESEARCH / "data/mock-cost-embed.json"

    @classmethod
    def setUpClass(cls):
        """Build the widget data once for every assertion in this class."""
        from widgets import REGISTRY
        cls.data = REGISTRY["cost"](RESEARCH, MOCKBUILD)
        cls.embed = json.loads(cls.EMBED.read_text())

    def test_meta_and_size(self):
        self.assertIn("_meta", self.data)
        self.assertEqual(self.data["_meta"]["slug"], "cost")
        self.assertEqual(self.data["_meta"]["asOf"], "2026-10-08")
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_seventeen_quota_weeks(self):
        self.assertEqual(len(self.data["qw"]), 17)
        self.assertEqual(self.data["qw"][0]["s"], "2026-06-16")
        self.assertEqual(self.data["qw"][-1]["s"], "2026-10-06")

    def test_recovered_rustty_usage(self):
        rec = sum(sum(q["rustty"]["usd"][0].values()) for q in self.data["qw"])
        self.assertAlmostEqual(rec, 11032.43, places=2)
        self.assertEqual(self.data["api"]["recovered"], 11032.43)

    def test_modeled_lost_window(self):
        sums = [round(sum(q["rustty"]["usd"][1][k] for q in self.data["qw"])) for k in range(3)]
        self.assertEqual(sums, [8725, 12759, 20331])
        self.assertEqual(self.data["api"]["modeled"], [8725, 12759, 20331])
        self.assertEqual(self.data["api"]["total"], [19758, 23792, 31363])

    def test_paid(self):
        p = self.data["paid"]
        self.assertEqual(round(p["central"]), 422)
        self.assertAlmostEqual(p["claude"] + p["codex"], p["central"], places=1)
        self.assertEqual(p["high"], 600)
        self.assertEqual(p["cycles"], 5, "fact-check: five billing cycles touched, not four")

    def test_delegated_share(self):
        k = self.data["kinds"]["rustty"]
        total = sum(v["usd"] for v in k.values())
        main = sum(v["usd"] for key, v in k.items() if key.endswith("/main"))
        self.assertEqual(round(1 - main / total, 3), 0.756)

    def test_ceilings(self):
        self.assertEqual(self.data["ceil"]["usd"], [1974.02, 2769.45, 4262.43])
        self.assertEqual(self.data["ceil"]["tok"], [4.648e9, 4.671e9, 11.591e9])
        self.assertEqual(self.data["ceil"]["out"][1], 9.91e6)

    def test_matches_the_mockup_series(self):
        for key in ("qw", "cov", "mix", "kinds", "ctx", "act", "meter", "units", "model_usd"):
            self.assertEqual(self.data[key], self.embed[key], key)
        self.assertEqual([e["t"] for e in self.data["ev"]], [e["t"] for e in self.embed["ev"]])

    def test_author_words_are_short_verbatim_fragments(self):
        full = {e["t"]: e["f"] for e in self.embed["ev"]}
        for e in self.data["ev"]:
            if e["k"] == "prompt":
                self.assertIn(e["s"], full[e["t"]])
                self.assertNotIn("f", e, "the whole prompt is not shipped")
                self.assertLessEqual(len(e["s"]), 40)

    def test_eras_renamed_and_codex_events_cleaned(self):
        self.assertEqual(len(self.data["eras"]), 3)
        self.assertEqual(len(self.data["cev"]), 7)
        self.assertEqual([e["commits"] for e in self.data["eras"]], [445, 947, 1310])

    def test_no_internal_ids_and_no_bun(self):
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertEqual(jargon.strip_internal_ids(s)[0], s, s)
            self.assertNotRegex(s, r"\bBun\b|\.port/|less capable", s)

    def test_no_spend_wording(self):
        """Cost framing: list-price usage is never called spend; only "not money spent" may say spent."""
        visible = {k: v for k, v in self.data.items() if k != "_meta"}
        for s in walk_strings(visible):
            self.assertNotRegex(s.replace("not money spent", ""), r"(?i)\bspen[dt]", s)


if __name__ == "__main__":
    unittest.main()


class StaticChartsBase(unittest.TestCase):
    """Shared checks for the static-chart builders: _meta, size, plain text, no Bun outside its one marker."""

    SLUG = ""

    @classmethod
    def setUpClass(cls):
        """Build the chart data once for every assertion in the subclass."""
        if not cls.SLUG:
            raise unittest.SkipTest("base class")
        from widgets import REGISTRY
        cls.data = REGISTRY[cls.SLUG](RESEARCH, MOCKBUILD)

    def visible_strings(self):
        """Every string in the data except the _meta block (which is not rendered)."""
        return [s for k, v in self.data.items() if k != "_meta" for s in walk_strings(v)]

    def test_meta_and_size(self):
        self.assertEqual(self.data["_meta"]["slug"], self.SLUG)
        self.assertIn("asOf", self.data["_meta"])
        self.assertLess(len(json.dumps(self.data)), 200 * 1024)

    def test_strings_are_plain(self):
        for s in self.visible_strings():
            self.assertFalse(jargon.strip_internal_ids(s)[1], s)
            self.assertNotIn("less capable", s.lower())
            self.assertNotIn(".port/", s)

    def test_no_bun_mention(self):
        allowed = getattr(self, "BUN_ALLOWED", 0)
        hits = [s for s in self.visible_strings() if re.search(r"\bBun\b", s)]
        self.assertEqual(len(hits), allowed, hits)


class StatsCardDataTest(StaticChartsBase):
    """stats-card: every tile value is a fact id; the cost tile separates paid from the API-equivalent reference."""

    SLUG = "stats-card"

    def test_every_tile_value_is_a_fact_id(self):
        with open(HERE / "facts.csv", newline="") as f:
            ids = {r["id"] for r in csv.DictReader(f)}
        self.assertGreaterEqual(len(self.data["tiles"]), 8)
        for t in self.data["tiles"]:
            self.assertIn(t["value"], ids, t)
            for k in ("compare", "extra"):
                if t.get(k):
                    self.assertIn(t[k], ids, t)
            self.assertNotRegex(t["label"] + t.get("note", ""), r"\d", "numbers come from facts only")
            for fid in re.findall(r"\{([a-z0-9_]+)\}", t.get("note", "")):
                self.assertIn(fid, ids, f"note placeholder {fid} is a fact id")

    def test_required_tiles(self):
        used = {t["value"] for t in self.data["tiles"]} | {t.get("compare") for t in self.data["tiles"]}
        for fid in ("days_total", "commits_head", "zig_code_lines", "rust_code_lines", "rust_tests",
                    "zig_tests_pin", "perf_wins_pin", "defects_total", "miri_crates_pin", "cost_paid"):
            self.assertIn(fid, used)

    def test_cost_framing(self):
        commits = next(t for t in self.data["tiles"] if t["value"] == "commits_head")
        self.assertEqual(commits["extra"], "commits_nonmerge_head", "head non-merge (2,789), not the all-refs 2,799")
        verify = next(t for t in self.data["tiles"] if t["value"] == "miri_crates_pin")
        self.assertEqual(verify["extra"], "sanitizer_runs_latest", "the same eight lanes as 61,403, each at its latest run")
        defects = next(t for t in self.data["tiles"] if t["value"] == "defects_total")
        self.assertIn("dependencies", defects["label"])
        tile = next(t for t in self.data["tiles"] if t["value"] == "cost_paid")
        self.assertEqual(tile["extra"], "cost_api_equiv")
        self.assertEqual(tile["extraLabel"], "API-equivalent (reference)")
        self.assertIn("paid", tile["label"].lower())


class StrategyTimelineDataTest(StaticChartsBase):
    """strategy-timeline: ledger 216 → 314 over 35 commits; one plain trigger marker is the only Bun mention."""

    SLUG = "strategy-timeline"
    BUN_ALLOWED = 1

    def test_ledger_series(self):
        rows = [p["rows"] for p in self.data["ledger"]]
        self.assertEqual(rows[0], 216)
        self.assertEqual(rows[-1], 314)
        self.assertEqual(len(rows), 35)
        self.assertEqual(rows, sorted(rows), "ledger never shrinks")

    def test_trigger_marker(self):
        t = self.data["trigger"]
        self.assertEqual(t["d"], "2026-07-10")
        self.assertIn("Bun", t["text"])
        self.assertNotRegex(t["text"], r"\b(vs|versus|faster|slower|more|fewer|than)\b")

    def test_events(self):
        ev = self.data["events"]
        self.assertGreaterEqual(len(ev), 25)
        self.assertEqual([e["d"] for e in ev], sorted(e["d"] for e in ev))
        lanes = {l["key"] for l in self.data["lanes"]}
        for e in ev:
            self.assertIn(e["lane"], lanes)
            self.assertTrue(e["label"] and e["text"])
        self.assertTrue(any(e["d"] == "2026-09-11" for e in ev), "never-port-a-bug change")
        self.assertTrue(any(e["d"] == "2026-07-12" for e in ev), "regression detectors")

    def test_registry_and_eras(self):
        reg = [p["n"] for p in self.data["registry"]]
        self.assertEqual((reg[0], reg[-1]), (11, 105))
        self.assertEqual([e["perDay"] for e in self.data["eras"]], [23.8, 20.0, 45.9])
        self.assertEqual(self.data["range"][1], "2026-10-10")
        self.assertEqual([e["commit"] for e in self.data["events"][-3:]], ["0182ccc35", "2bf89b95a", "58aa62a81"])
        self.assertNotIn("census", self.data, "unused")

    def test_parity_closure_wording(self):
        ev = {e["d"]: e for e in self.data["events"] if e["lane"] == "verify"}
        self.assertIn("starts", ev["2026-09-08"]["label"])
        self.assertIn("one low-severity item open", ev["2026-09-28"]["text"])
        for e in self.data["events"]:
            self.assertNotRegex(e["text"], r"\b0 open\b|closed one by one")

    def test_windows_wording(self):
        win = next(e for e in self.data["events"] if e["d"] == "2026-09-27")
        self.assertIn("targeted tests", win["text"])


class ChecksGrowthDataTest(StaticChartsBase):
    """checks-growth: 11 → 105 entries, 79 hard / 26 progress, 27 self-tests; 34,371 comments, 0 missing; C ABI pills."""

    SLUG = "checks-growth"

    def test_registry_steps(self):
        reg = self.data["registry"]
        self.assertEqual((reg[0]["n"], reg[-1]["n"]), (11, 105))
        self.assertEqual(reg[-1]["self"], 27)
        self.assertEqual([p["n"] for p in reg], sorted(p["n"] for p in reg))
        f = self.data["final"]
        self.assertEqual((f["entries"], f["hard"], f["progress"], f["self"]), (105, 79, 26, 27))
        self.assertEqual(sum(g["n"] for g in self.data["composition"]), 105)

    def test_scripts(self):
        s = self.data["scripts"]
        self.assertEqual((s[0]["total"], s[0]["selfTest"]), (12, 10))
        self.assertEqual((s[-1]["total"], s[-1]["selfTest"]), (93, 58))

    def test_comments_and_capi(self):
        c = self.data["comments"]
        self.assertEqual((c["total"], c["missing"], len(c["crates"])), (34371, 0, 29))
        self.assertEqual(sum(x["n"] for x in c["crates"]), 34371)
        pills = {p["key"]: (p["rust"], p["ref"]) for p in self.data["capi"]}
        self.assertEqual(pills["headers"], (35, 35))
        self.assertEqual(pills["functions"], (208, 208))
        self.assertEqual(pills["embed"], (91, 91))

    def test_examples_are_described_not_ids(self):
        for g in self.data["composition"]:
            self.assertTrue(g["examples"])
            for e in g["examples"]:
                self.assertEqual(jargon.find_internal_ids(e), [], e)


class AuditFunnelDataTest(StaticChartsBase):
    """audit-funnel: 659 → 179 → 147 → 144 → 140, non-increasing; the shared Codex meter (median, min–max) ends at 99%."""

    SLUG = "audit-funnel"

    def test_funnel(self):
        n = [s["n"] for s in self.data["stages"]]
        self.assertEqual(n, [659, 179, 147, 144, 140])
        self.assertEqual(n, sorted(n, reverse=True))
        self.assertEqual(sum(self.data["severity"].values()), 147)

    def test_meter(self):
        m = self.data["meter"]
        self.assertEqual(len(m), 45)
        self.assertEqual(m[-1][1], 99)
        for h, med, lo, hi, _ in m:
            self.assertLessEqual(lo, med, h)
            self.assertLessEqual(med, hi, h)
        self.assertEqual(len(self.data["events"]), 7)

    def test_one_meter_treatment_with_the_cost_widget(self):
        from widgets import REGISTRY
        cost = REGISTRY["cost"](RESEARCH, MOCKBUILD)
        self.assertEqual(self.data["meter"], cost["meter"])
        self.assertEqual(self.data["events"], cost["cev"])
        self.assertEqual(self.data["meterNote"], cost["meterNote"])
        self.assertEqual([r["label"] for r in self.data["runs"]], ["run 1", "run 2"])
        times = {e["t"]: e["label"] for e in self.data["events"]}
        self.assertEqual(times["2026-08-02T03:36"], "100th round", "fact-check: first W100 spawn 03:36 UTC")
        for e in self.data["events"]:
            self.assertNotRegex(e["label"] + e["text"], r"\baudit [12]\b", "runs are named run 1 / run 2")
        self.assertIn("6%", " ".join(self.data["meterNote"]["lines"]), "run 1 ended at 6% per the fact-check")


class OrchestrationDataTest(StaticChartsBase):
    """orchestration: 2,789 trailer commits by model (git, to 10-10); context median 77k → 204k (transcripts, to 10-08); 211 dated workflow scripts, last on 08-25."""

    SLUG = "orchestration"

    def test_models(self):
        self.assertEqual(sum(sum(w["n"]) for w in self.data["weeks"]), 2789)
        self.assertIn("Haiku 5.5", self.data["models"])
        hk = self.data["models"].index("Haiku 5.5")
        self.assertEqual(sum(w["n"][hk] for w in self.data["weeks"]), 2, "fact model_haiku55")
        self.assertEqual(len(self.data["weeks"][0]["n"]), len(self.data["models"]))
        self.assertIn("2026-08-03", [w["w"] for w in self.data["weeks"]], "empty week kept as zero")

    def test_unknown_model_column_is_an_error(self):
        from widgets import static_charts
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "data").mkdir()
            src = RESEARCH / "data" / static_charts.dated("r2-cost-model-weekly.csv")
            text = src.read_text().replace("Claude Haiku 5.5", "Claude Haiku 6", 1)
            (Path(tmp) / "data" / src.name).write_text(text)
            with self.assertRaises(ValueError):
                static_charts.build_orchestration(Path(tmp), MOCKBUILD)

    def test_context(self):
        ctx = [c for c in self.data["ctx"] if c["tool"] == "claude"]
        self.assertEqual(round(ctx[0]["median"] / 1000), 77)
        self.assertEqual(round(max(c["median"] for c in ctx) / 1000), 204)
        for c in self.data["ctx"]:
            self.assertLessEqual(c["median"], c["p90"])

    def test_scripts_and_threads(self):
        s = self.data["scripts"]
        self.assertEqual(sum(x["n"] for x in s), 211)
        self.assertEqual(s[-1]["d"], "2026-08-25")
        self.assertEqual(self.data["totals"]["scriptsCommitted"], 211)
        self.assertEqual(self.data["totals"]["subagents"], 1142)
        self.assertEqual(self.data["totals"]["worktrees"], 214)
        self.assertEqual(max(t["claude"] for t in self.data["threads"]), 452)
        self.assertEqual(max(t["codex"] for t in self.data["threads"]), 773)


class UnsafeDensityDataTest(StaticChartsBase):
    """unsafe-density: census series ends at 22,240 / 838,127 = 26.54 per 1,000 physical lines; no external band."""

    SLUG = "unsafe-density"

    def test_series(self):
        s = self.data["series"]
        self.assertEqual([p["density"] for p in s], [27.02, 28.15, 27.87, 26.66, 26.54])
        self.assertEqual((s[-2]["sites"], s[-2]["lines"]), (22310, 836905))
        self.assertEqual((s[-1]["commit"], s[-1]["sites"], s[-1]["lines"]), ("58aa62a81", 22240, 838127))
        self.assertAlmostEqual(s[-1]["sites"] / s[-1]["lines"] * 1000, 26.54, places=2)
        self.assertNotIn("band", json.dumps(self.data).lower())

    def test_crates_and_areas(self):
        c = self.data["crates"]
        self.assertEqual(sum(x["sites"] for x in c["rows"]) + c["other"]["sites"], 22240)
        vt = next(x for x in c["rows"] if x["crate"] == "rustty-vt")
        self.assertEqual((vt["sites"], vt["test"], vt["nonTest"]), (7899, 6185, 1714))
        self.assertEqual(vt["lines"], 264011, "census lines as recorded; only the fact-checked site is removed")
        self.assertEqual(sum(a["nonTest"] for a in self.data["vtAreas"]["rows"]) + self.data["vtAreas"]["other"]["nonTest"], 1708)
        self.assertIn("physical", self.data["basis"]["series"])
        self.assertIn("code lines", self.data["basis"]["areas"])


class PlatformTableDataTest(StaticChartsBase):
    """platform-table: six platforms with the fact-checked claims; Linux never on a real GPU or x86_64; Windows has no app."""

    SLUG = "platform-table"

    def test_rows(self):
        rows = {r["key"]: r for r in self.data["rows"]}
        self.assertEqual(list(rows), ["macos", "linux", "windows", "freebsd", "wasm", "libvt"])
        lin = rows["linux"]["claim"]
        self.assertIn("never", lin)
        self.assertIn("GPU", lin)
        self.assertIn("x86_64", lin)
        self.assertIn("no Windows app", rows["windows"]["claim"])
        self.assertIn("6,418 passed, 0 failed", rows["windows"]["lanes"]["detail"])
        self.assertNotIn("No full workspace test run", rows["windows"]["lanes"]["detail"])
        bsd = rows["freebsd"]
        self.assertEqual(bsd["lastRef"], "110adad44")
        for col in ("builds", "runs", "hand"):
            self.assertEqual(bsd[col]["status"], "yes", col)  # committed run record at 110adad44
        # One native run and no sanitizers: the lanes cell is partial and says so.
        self.assertEqual(bsd["lanes"]["status"], "partial")
        self.assertIn("one run, no sanitizers", bsd["lanes"]["text"])
        self.assertIn("No sanitizers", bsd["lanes"]["detail"])
        self.assertIn("6,665 passed, 23 failed", bsd["lanes"]["detail"])
        self.assertIn("15 of 18", bsd["lanes"]["detail"])
        self.assertIn("DMA-BUF", bsd["claim"])  # the three environment-blocked tests stay in the claim
        self.assertIn("amd64", bsd["claim"])
        self.assertNotIn("never built natively", bsd["claim"])
        wasm = rows["wasm"]
        self.assertEqual(wasm["lastRef"], "3a59e7edc")
        self.assertEqual(wasm["lanes"]["status"], "yes")
        self.assertIn("34 passed, 2 ignored", wasm["lanes"]["detail"])
        self.assertIn("no web terminal app", wasm["claim"])
        self.assertNotIn("never been executed", wasm["lanes"]["detail"])
        self.assertEqual(wasm["hand"]["status"], "yes")  # author statement, 10 October hand run
        self.assertNotIn("lastRef", rows["macos"])
        for r in rows.values():
            for col in ("builds", "runs", "hand", "lanes"):
                self.assertIn(r[col]["status"], {"yes", "partial", "no", "na"})
            self.assertRegex(r["last"], r"^2026-\d\d-\d\d$")


# EP_FAKE_SECRET and EP_FAKE_EMAIL are fixture-only strings (never real data).
EP_FAKE_SECRET = "hunter2-FAKE-0000"
EP_FAKE_EMAIL = "someone@example.org"


# EP_TITLES are the fixture's proposed session (title, title source, area) per session id.
EP_TITLES = {
    "f1": ("Weigh the port and plan it", "claude-written", "engine"),
    "s1": ("Log in to the test box", "claude-written", "tooling"),
    "s2": ("Command-only session", "claude-written", "engine"),
    "c1": ("Old title", "claude-code", "engine"),
    "s3": ("Draft the article", "claude-written", "article"),
    "s9": ("Check the purged window", "claude-written", "engine"),
    "sw": ("Tidy the site", "claude-written", "website"),
}
# EP_REVIEW is the fixture's default session review: (decision, edited_title, edited_area).
EP_REVIEW = {"s2": ("edit", "Command-only session", "command-only"), "c1": ("edit", "Choose option B", "defects")}


def ep_fixture(root: Path, *, decisions=("redact", "keep"), extra_raw=(), manifest_prompts=None, shift="",
               pre_purge_tx=0, founding=False, founding_decision="include", review=None, approved_titles=None,
               title_hits=False):
    """Write a minimal research dir for the every-prompt builder under root; return the research path.

    shift is prefixed to the first prompt AFTER the redaction CSVs are written (a re-extract the
    review has not seen); pre_purge_tx is the manifest's mac_before_purge_with_transcript.
    founding adds a founding session f1 (one prompt on the evening of 20 June Pacific) whose
    founding-list decision is founding_decision; review overrides EP_REVIEW per session (None
    drops the approved row); approved_titles overrides the title an approved row says it reviewed
    (a stale review); title_hits writes a row into the title redaction proposals.
    """
    from widgets import w12_every_prompt as ep
    data = root / "data"
    data.mkdir(parents=True)

    def row(pid, ts, tool, machine, session, text, **kw):
        """One raw extract row with the extractor's field names."""
        base = dict(id=pid, ts=ts, tool=tool, machine=machine, session=session, text=text, chars=len(text),
                    kind=ep.prompt_kind(text), pasted="none", model=None, effort=None, mode=None,
                    settings_source="unknown", approval=None, sandbox=None, article_work=False, commits=[],
                    commits_basis="none", ids=[], phase="pre-Bun", decision=ep.is_decision(text), branch=None,
                    project="/x/rustty")
        base.update(kw)
        return base

    secret_text = f"log in with password {EP_FAKE_SECRET} and mail {EP_FAKE_EMAIL}, see UD7"
    rows = [
        row("claude-mac-20260701T170000Z-0", "2026-07-01T17:00:00Z", "claude", "mac", "s1", secret_text),
        row("claude-mac-20260702T170000Z-0", "2026-07-02T17:00:00Z", "claude", "mac", "s1", "ok",
            settings_source="commit-trailer", model="claude-opus-4-8", commits=["a" * 40],
            commits_basis="time-window-branch"),
        row("claude-vm-20260720T170000Z-0", "2026-07-20T17:00:00Z", "claude", "vm", "s2", "/clear",
            settings_source="session", model="claude-opus-4-8", effort="medium", mode="bypass", pasted="lost"),
        row("codex-mac-20260920T170000Z-0", "2026-09-20T17:00:00Z", "codex", "mac", "c1", "B",
            settings_source="transcript", model="gpt-5.5", effort="high", mode="bypass", commits=["b" * 40],
            commits_basis="ambiguous"),
        row("claude-mac-20261009T170000Z-0", "2026-10-09T17:00:00Z", "claude", "mac", "s3", "draft it",
            settings_source="transcript", model="claude-opus-5-5", effort="medium", mode="bypass"),
        *extra_raw,
    ]
    if founding:
        rows.insert(0, row("claude-mac-20260621T044235Z-0", "2026-06-21T04:42:35Z", "claude", "mac", "f1",
                           "should we port it?", founding=True, project="/x"))
    rows[1 if founding else 0]["text"] = shift + rows[1 if founding else 0]["text"]
    with open(data / ep.RAW, "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")
    by = {"claude-mac": 0, "claude-vm": 0, "codex": 0}
    for r in rows:
        by[ep._lane(r)] += 1
    n = len(rows) if manifest_prompts is None else manifest_prompts
    (data / ep.MANIFEST).write_text(json.dumps({
        "cutoff": "2026-10-11T00:00:00Z",
        "sources": {k: {"in_source": v, "kept": v, "dropped": {}} for k, v in by.items()},
        "counts": {"prompts": n, "by_source": by, "mac_before_purge_with_transcript": pre_purge_tx},
        "rules": {"article_work": "Article rule.", "commits": "Commit rule.", "decision": ep.DECISION_RULE}}))
    sessions = {}
    for r in sorted(rows, key=lambda r: r["ts"]):
        x = sessions.setdefault((r["tool"], r["machine"], r["session"]), {
            "session": r["session"], "key": f"{r['tool']}-{r['machine']}-{r['session'][:8]}", "tool": r["tool"],
            "machine": r["machine"], "first_ts": r["ts"], "last_ts": r["ts"], "prompts": 0, "commands_only": True,
            "founding": bool(r.get("founding")), "article_work": False, "branches": [], "models": [], "commits": [],
            "commits_ambiguous": 0, "cc_title": None})
        x["last_ts"] = r["ts"]
        x["prompts"] += 1
        x["commands_only"] &= ep.prompt_kind(r["text"]) == "command"
        if r["commits_basis"] in ("session-window", "time-window-branch"):
            x["commits"] = sorted(set(x["commits"]) | set(r["commits"]))
        x["commits_ambiguous"] += r["commits_basis"] == "ambiguous"
    with open(data / ep.SESSIONS, "w") as f:
        for x in sessions.values():
            f.write(json.dumps(x) + "\n")
    cols = ["session", "key", "title", "title_source", "area", "decision", "edited_title", "edited_area", "notes",
            "edited_title_source"]
    reviews = {**EP_REVIEW, **(review or {})}
    with open(data / ep.SESSIONS_PROPOSED, "w", newline="") as fp, \
            open(data / ep.SESSIONS_APPROVED, "w", newline="") as fa:
        wp, wa = csv.writer(fp), csv.writer(fa)
        wp.writerow(cols)
        wa.writerow(cols)
        for x in sessions.values():
            title, source, area = EP_TITLES[x["session"]]
            wp.writerow([x["session"], x["key"], title, source, area, "", "", "", "", ""])
            if x["session"] in reviews and reviews[x["session"]] is None:
                continue
            d, et, ea, *ets = reviews.get(x["session"], ("accept", "", ""))
            seen = (approved_titles or {}).get(x["session"], title)
            wa.writerow([x["session"], x["key"], seen, source, area, d, et, ea, "", ets[0] if ets else ""])
    fcols = ["session", "tool", "machine", "project", "first_ts", "last_ts", "prompts", "reason", "decision"]
    with open(data / ep.FOUNDING_PROPOSED, "w", newline="") as fp, \
            open(data / ep.FOUNDING_APPROVED, "w", newline="") as fa:
        wp, wa = csv.writer(fp), csv.writer(fa)
        wp.writerow(fcols)
        wa.writerow(fcols)
        if founding:
            base = ["f1", "claude", "mac", "~/x", "2026-06-21T04:42:35Z", "2026-06-21T04:42:35Z", "1", "port-decision"]
            wp.writerow([*base, ""])
            if founding_decision is not None:
                wa.writerow([*base, founding_decision])
    (data / ep.TITLE_REDACTIONS).write_text("id,start,end,match,reason,decision\n"
                                            + ("title:claude-mac-s1,0,3,Log,person,\n" if title_hits else ""))
    first = rows[1 if founding else 0]
    s = secret_text.index(EP_FAKE_SECRET)
    e = secret_text.index(EP_FAKE_EMAIL)
    spans = [(first["id"], s, s + len(EP_FAKE_SECRET), "secret"), (first["id"], e, e + len(EP_FAKE_EMAIL), "email")]
    with open(data / ep.PROPOSED, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "start", "end", "match", "reason", "decision"])
        for pid, a, b, reason in spans:
            w.writerow([pid, a, b, secret_text[a:b], reason, ""])
    with open(data / ep.APPROVED, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "start", "end", "match", "reason", "decision"])
        for (pid, a, b, reason), d in zip(spans, decisions):
            if d is not None:
                w.writerow([pid, a, b, secret_text[a:b], reason, d])
    (data / ep.GLOSSARY_EXTRA).write_text("A2\toption A2\n")
    (data / ep.OVERRIDES).write_text("session,article_work,reason\ns3,true,author: article\n")
    (data / ep.GLOSSARY).write_text("id,category,collision_note,plain_description\nUD7,ud,,an upstream defect at the pin\n")
    with open(data / ep.DAY_HOUR, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["date_pt", "day_index", "hour_pt", "commits_all_refs", "commits_non_merge", "prompts_rustty"])
        w.writerow(["2026-07-01", 10, 9, 5, 4, 1])
        w.writerow(["2026-07-01", 10, 10, 2, 2, 0])
    return root


def ep_mod():
    """The every-prompt builder module."""
    from widgets import w12_every_prompt as ep
    return ep


def session_row(data: dict, x: list) -> dict:
    """A summary session row as a dict, its area and title source decoded from their indices."""
    d = dict(zip(data["sessionFields"], x))
    d["area"] = data["sessionAreas"][d["area"]]
    d["titleSource"] = data["titleSources"][d["titleSource"]]
    return d


def _weeks_from_ticks(data: dict) -> dict:
    """Rebuilds the every-prompt weekly settings from the summary's ticks alone (as the widget does)."""
    from datetime import date, timedelta
    start = date.fromisoformat(data["window"][0])
    out = {w["week"]: {"settings": {}, "inferred": {}, "modes": {}} for w in data["weeks"]}
    for t in data["ticks"]:
        day = start + timedelta(days=t[0])
        w = out[(day - timedelta(days=day.weekday())).isoformat()]
        model = data["models"][t[ep_mod().T_MODEL]] if t[ep_mod().T_MODEL] >= 0 else None
        if t[ep_mod().T_FLAGS] & 64:
            w["inferred"][model] = w["inferred"].get(model, 0) + 1
        elif model:
            k = f"{model}|{data['efforts'][t[ep_mod().T_EFFORT]] if t[ep_mod().T_EFFORT] >= 0 else 'unknown'}"
            w["settings"][k] = w["settings"].get(k, 0) + 1
        else:
            w["settings"]["unknown"] = w["settings"].get("unknown", 0) + 1
        mode = data["modes"][t[ep_mod().T_MODE]] if t[ep_mod().T_MODE] >= 0 else "unknown"
        w["modes"][mode] = w["modes"].get(mode, 0) + 1
    return out


class EveryPromptTest(unittest.TestCase):
    """every-prompt: refuses unreviewed redactions, applies only approved ones, reconciles with the manifest."""

    def tmp(self) -> Path:
        """A temporary directory removed after the test."""
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        return Path(d.name)

    def build(self, **kw):
        """Run the builder on a fresh fixture; return (summary, log)."""
        from widgets import w12_every_prompt as ep
        root = ep_fixture(self.tmp(), **kw)
        data = ep.build(root, None)
        log = data.pop("__extra__")[ep.LOG_PATH]
        return data, log

    def test_refuses_unreviewed_redactions(self):
        for decisions in [("redact", None), ("redact", "")]:
            with self.assertRaises(SystemExit) as cm:
                self.build(decisions=decisions)
            self.assertIn(ep_mod().APPROVED, str(cm.exception.code))

    def test_applies_only_redact_rows(self):
        data, log = self.build()
        text = log["rows"][0]["text"]
        self.assertIn("[redacted: secret]", text)
        self.assertIn(EP_FAKE_EMAIL, text)  # the author kept this one
        for blob in (json.dumps(data, ensure_ascii=False), json.dumps(log, ensure_ascii=False)):
            self.assertNotIn(EP_FAKE_SECRET, blob)
        self.assertEqual((data["totals"]["redactions"], data["totals"]["redactedPrompts"]), (1, 1))
        self.assertEqual(data["ticks"][0][ep_mod().T_FLAGS] & 32, 32)
        self.assertEqual(log["rows"][0]["chars"], len(text))

    def test_totals_reconcile_with_manifest(self):
        data, log = self.build()
        t = data["totals"]
        self.assertEqual(t["prompts"], len(log["rows"]))
        self.assertEqual(t["prompts"], sum(t["bySource"].values()))
        self.assertEqual(len(data["ticks"]), t["prompts"])
        self.assertNotIn("commands", t)  # only commandsAndShell ships
        n_cmd = sum(1 for r in log["rows"] if r["kind"] == "command")
        self.assertEqual((n_cmd, t["decisions"], t["pastedLost"], t["articleWork"]), (1, 2, 1, 1))
        with self.assertRaises(SystemExit) as cm:
            self.build(manifest_prompts=99)
        self.assertIn(ep_mod().MANIFEST, str(cm.exception.code))

    def test_rows_flags_and_commits(self):
        data, log = self.build()
        rows = log["rows"]
        self.assertEqual([r["phase"] for r in rows], ["founding", "founding", "tightened", "never-port-a-bug",
                                                       "never-port-a-bug"])
        self.assertEqual(rows[1]["commits"], ["a" * 40])
        self.assertEqual((rows[3]["commits"], rows[3]["commitsBasis"]), ([], "ambiguous"))
        self.assertTrue(rows[4]["articleWork"])  # author override
        self.assertEqual(data["ticks"][2][2], 1)  # vm lane
        self.assertEqual(data["ticks"][2][ep_mod().T_FLAGS], 2 | 16)  # command, pasted lost
        for t, r in zip(data["ticks"], rows):  # exact length, words and settings ride along
            self.assertEqual(t[ep_mod().T_CHARS:ep_mod().T_WORDS + 1], [r["chars"], len(r["text"].split())])
            self.assertEqual(data["models"][t[ep_mod().T_MODEL]] if t[ep_mod().T_MODEL] >= 0 else None, r["model"])
            self.assertEqual(data["efforts"][t[ep_mod().T_EFFORT]] if t[ep_mod().T_EFFORT] >= 0 else None, r["effort"])
            self.assertEqual(data["modes"][t[ep_mod().T_MODE]] if t[ep_mod().T_MODE] >= 0 else None, r["mode"])
            self.assertEqual(bool(t[ep_mod().T_FLAGS] & 64), r["settingsSource"] == "commit-trailer", r["id"])
        self.assertTrue(any(t[ep_mod().T_FLAGS] & 64 for t in data["ticks"]))  # the fixture has an inferred model
        self.assertEqual(data["commitsPerDay"][11], 6)  # 2026-07-01 is day 11 of a window from 20 June
        self.assertEqual(log["glossary"], {"UD7": "an upstream defect at the reference commit"})
        w = {x["week"]: x for x in data["weeks"]}
        self.assertEqual(w["2026-06-29"]["inferred"], {"claude-opus-4-8": 1})
        self.assertEqual(w["2026-06-29"]["settings"], {"unknown": 1})
        self.assertEqual(w["2026-09-14"]["settings"], {"gpt-5.5|high": 1})
        self.assertIn("2026-08-03", w)  # an empty week between prompts is kept

    def test_refuses_stale_review(self):
        """A prompt re-extracted with a shifted text no longer matches the reviewed spans."""
        with self.assertRaises(SystemExit) as cm:
            self.build(shift="Hi, ")
        msg = str(cm.exception.code)
        self.assertIn(ep_mod().APPROVED, msg)
        self.assertIn("claude-mac-20260701T170000Z-0", msg)

    def test_refuses_match_mismatch_and_missing_column(self):
        from widgets import w12_every_prompt as ep
        root = ep_fixture(self.tmp())
        path = root / "data" / ep.PROPOSED
        path.write_text(path.read_text().replace(EP_FAKE_EMAIL, "other@example.org", 1))
        with self.assertRaises(SystemExit) as cm:
            ep.build(root, None)
        self.assertIn("claude-mac-20260701T170000Z-0", str(cm.exception.code))
        for name in (ep.APPROVED, ep.PROPOSED):
            root = ep_fixture(self.tmp())
            with open(root / "data" / name, newline="") as f:
                rows = list(csv.DictReader(f))
            with open(root / "data" / name, "w", newline="") as f:
                w = csv.DictWriter(f, [k for k in rows[0] if k != "match"], extrasaction="ignore")
                w.writeheader()
                w.writerows(rows)
            with self.assertRaises(SystemExit) as cm:
                ep.build(root, None)
            self.assertIn(name, str(cm.exception.code))

    def test_warns_on_unproposed_approved_row(self):
        import contextlib, io
        from widgets import w12_every_prompt as ep
        root = ep_fixture(self.tmp())
        with open(root / "data" / ep.APPROVED, "a", newline="") as f:
            csv.writer(f).writerow(["claude-mac-20260702T170000Z-0", 0, 2, "ok", "secret", "keep"])
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            ep.build(root, None)
        self.assertIn("claude-mac-20260702T170000Z-0 [0:2] has no proposal", err.getvalue())

    def test_purged_window_transcript_rows_bounded_by_manifest(self):
        tx = {"id": "claude-mac-20260801T170000Z-0", "ts": "2026-08-01T17:00:00Z", "tool": "claude",
              "machine": "mac", "session": "s9", "text": "x", "chars": 1, "kind": "prompt", "pasted": "none",
              "model": "claude-opus-4-8", "effort": "medium", "mode": None, "settings_source": "transcript",
              "article_work": False, "commits": [], "commits_basis": "none", "ids": [], "phase": "post-Bun",
              "decision": False, "branch": None, "project": "/x/rustty"}
        self.build(extra_raw=[tx], pre_purge_tx=1)
        with self.assertRaises(SystemExit) as cm:
            self.build(extra_raw=[tx], pre_purge_tx=0)
        self.assertIn("mac_before_purge_with_transcript", str(cm.exception.code))

    def test_website_folder_prompts_are_left_out(self):
        """A website-folder prompt, and a session made only of them, leave no trace in the summary or the log."""
        web = {"id": "claude-mac-20261010T170000Z-0", "ts": "2026-10-10T17:00:00Z", "tool": "claude",
               "machine": "mac", "session": "sw", "text": "tidy the page", "chars": 13, "kind": "prompt",
               "pasted": "none", "model": "claude-opus-5-5", "effort": "medium", "mode": "bypass",
               "settings_source": "transcript", "article_work": True, "commits": ["c" * 40],
               "commits_basis": "other-repo", "ids": [], "phase": "post-Bun", "decision": False, "branch": None,
               "project": "/x/ghostty-website-journey", "project_marker": "website"}
        base_data, base_log = self.build()
        data, log = self.build(extra_raw=[web])
        self.assertEqual((data, log), (base_data, base_log))  # as if the prompt and its session did not exist
        self.assertNotIn("website", json.dumps(data) + json.dumps(log))
        self.assertNotIn("website", data["areas"])
        self.assertNotIn(web["id"], {r["id"] for r in log["rows"]})
        self.assertNotIn("Tidy the site", json.dumps(data) + json.dumps(log))
        # The article-work count of the extractor does not include it either.
        self.assertEqual(data["totals"]["articleWorkExtractor"], base_data["totals"]["articleWorkExtractor"])
        # A session that has website-folder prompts and others is refused: only a whole session is left out.
        with self.assertRaises(SystemExit) as cm:
            self.build(extra_raw=[{**web, "session": "s3"}])
        self.assertIn("only a whole session", str(cm.exception.code))
        with self.assertRaises(SystemExit):  # other-repo outside the website folder
            self.build(extra_raw=[{**web, "project_marker": "rustty"}])

    def test_purged_window_has_no_session_settings(self):
        bad = {"id": "claude-mac-20260801T170000Z-0", "ts": "2026-08-01T17:00:00Z", "tool": "claude",
               "machine": "mac", "session": "s9", "text": "x", "chars": 1, "kind": "prompt", "pasted": "none",
               "model": "claude-opus-4-8", "effort": "medium", "mode": None, "settings_source": "session",
               "article_work": False, "commits": [], "commits_basis": "none", "ids": [], "phase": "post-Bun",
               "decision": False, "branch": None, "project": "/x/rustty"}
        with self.assertRaises(SystemExit):
            self.build(extra_raw=[bad])

    def test_missing_glossary_id_refuses(self):
        from widgets import w12_every_prompt as ep
        root = ep_fixture(self.tmp())
        (root / "data" / ep.GLOSSARY).write_text("id,category,collision_note,plain_description\n")
        with self.assertRaises(SystemExit) as cm:
            ep.build(root, None)
        self.assertIn("UD7", str(cm.exception.code))

    def test_decision_rule(self):
        from widgets import w12_every_prompt as ep
        for t in ["A", "b", "ok", "yes", "go", "continue", "ok.", "B, go ahead", "Go ahead\n", "LGTM!", "2", "12.", " 3 ",
                  "approve", "Approved.", "yes, approved"]:
            self.assertTrue(ep.is_decision(t), t)
        for t in ["ok but change X", "", "continue with the plan", "/clear", "e", "go go go go", "123", "2 and 3", "2026"]:
            self.assertFalse(ep.is_decision(t), t)

    def test_id_regex_in_sync_with_jargon(self):
        from widgets import w12_every_prompt as ep
        self.assertEqual(ep.ID_RE.pattern, jargon.ID_PATTERN.pattern)
        samples = ["fix UD123 crash", "gate G3b green", "rule R13 says", "N511 apc", "P8-W9 T1b check",
                   "D1 layering", "A4 sret gate", "G6c lane", "G-UDDOCS", "G1-SELF", "W12.T2 run", "FX12 fix",
                   "U-foo", "M4 Pro", "UTF-8", "x86_64", "phase 3 batch 2", "option A2", "P3-W0"]
        for s in samples:
            want = [m for m in jargon.find_internal_ids(s) if jargon.ID_PATTERN.fullmatch(m)]
            self.assertEqual([m.group(0) for m in ep.ID_RE.finditer(s)], want, s)
        self.assertEqual(jargon.find_internal_ids("FX12 fix"), ["FX12"])

    def test_extra_outputs_written_by_cli(self):
        from widgets import w12_every_prompt as ep
        research = ep_fixture(self.tmp())
        out, root = self.tmp(), self.tmp()
        subprocess.run([sys.executable, str(HERE / "build_data.py"), "--research", str(research), "--mockbuild",
                        str(MOCKBUILD), "--out", out, "--root", root, "--only", "every-prompt"],
                       check=True, capture_output=True)
        summary = json.loads((Path(out) / "every-prompt.json").read_text())
        self.assertNotIn("__extra__", summary)
        log_bytes = (Path(root) / ep.LOG_PATH).read_bytes()
        self.assertEqual(summary["logBytes"], len(log_bytes))
        self.assertNotIn(EP_FAKE_SECRET.encode(), log_bytes)

    def test_refuses_unreviewed_sessions(self):
        from widgets import w12_every_prompt as ep
        for review in ({"s1": None}, {"s1": ("", "", "")}, {"s1": ("maybe", "", "")}, {"s1": ("edit", "", "")}):
            with self.assertRaises(SystemExit) as cm:
                self.build(review=review)
            msg = str(cm.exception.code)
            self.assertIn(ep.SESSIONS_APPROVED, msg)
            self.assertIn("claude-mac-s1", msg)

    def test_refuses_stale_session_review(self):
        """An approved row must carry the title (and area) of the proposal it reviewed."""
        from widgets import w12_every_prompt as ep
        with self.assertRaises(SystemExit) as cm:
            self.build(approved_titles={"s3": "Draft the article (old)"})
        msg = str(cm.exception.code)
        self.assertIn(ep.SESSIONS_APPROVED, msg)
        self.assertIn("claude-mac-s3", msg)
        self.assertIn("stale", msg)
        root = ep_fixture(self.tmp())
        path = root / "data" / ep.SESSIONS_APPROVED
        path.write_text(path.read_text().replace(",article,accept", ",tooling,accept"))
        with self.assertRaises(SystemExit) as cm:
            ep.build(root, None)
        self.assertIn("stale", str(cm.exception.code))

    def test_refuses_excluded_session_with_prompts_and_unknown_area(self):
        with self.assertRaises(SystemExit):
            self.build(review={"s3": ("exclude", "", "")})
        with self.assertRaises(SystemExit):
            self.build(review={"s3": ("edit", "Draft it", "writing")})
        with self.assertRaises(SystemExit):  # a command-only session needs the command-only area
            self.build(review={"s2": ("accept", "", "")})

    def test_refuses_title_redaction_hits(self):
        from widgets import w12_every_prompt as ep
        with self.assertRaises(SystemExit) as cm:
            self.build(title_hits=True)
        self.assertIn(ep.TITLE_REDACTIONS, str(cm.exception.code))

    def test_edit_rows_use_edited_title_and_area(self):
        data, log = self.build()
        f = data["sessionFields"]
        by = {x[f.index("key")]: session_row(data, x) for x in data["sessions"]}
        c1 = by["codex-mac-c1"]
        self.assertEqual((c1["title"], c1["area"], c1["titleSource"]), ("Choose option B", "defects", "claude-written"))
        self.assertEqual((by["claude-vm-s2"]["area"], by["claude-vm-s2"]["commandsOnly"]), ("command-only", True))
        self.assertEqual(by["claude-mac-s1"]["titleSource"], "claude-written")
        self.assertEqual(log["sessions"]["codex-mac-c1"]["title"], "Choose option B")
        self.assertNotIn("Old title", json.dumps(data) + json.dumps(log))
        t = data["totals"]
        self.assertEqual((t["sessions"], t["sessionsCommandOnly"]), (4, 1))
        self.assertEqual(t["sessionsByArea"]["defects"], 1)
        self.assertNotIn("command-only", t["sessionsByArea"])
        self.assertEqual(sum(t["sessionsByArea"].values()) + t["sessionsCommandOnly"], t["sessions"])
        self.assertEqual(data["areas"], ["engine", "renderer and fonts", "macOS app", "GTK/Linux", "performance",
                                         "upstream catch-up", "verification/CI", "defects", "tooling", "article"])

    def test_titles_refused_when_they_carry_a_redacted_value(self):
        from widgets import w12_every_prompt as ep
        with self.assertRaises(SystemExit) as cm:
            self.build(review={"s1": ("edit", f"Log in with {EP_FAKE_SECRET.upper()}", "tooling")})
        self.assertIn("redacted value", str(cm.exception.code))
        data, log = self.build()  # the default titles pass; no redacted value in either output
        for blob in (ep.dump(data), ep.dump(log), json.dumps(data), json.dumps(log)):
            self.assertNotIn(EP_FAKE_SECRET, blob)

    def test_every_prompt_has_exactly_one_existing_session(self):
        data, log = self.build(founding=True)
        f = data["sessionFields"]
        keys = [x[f.index("key")] for x in data["sessions"]]
        self.assertEqual(len(keys), len(set(keys)))
        self.assertEqual(set(keys), set(log["sessions"]))
        for tick, r in zip(data["ticks"], log["rows"]):
            self.assertEqual(len(tick), 11)
            self.assertEqual(keys[tick[ep_mod().T_SESSION]], r["session"])
        self.assertEqual(sum(x[f.index("prompts")] for x in data["sessions"]), data["totals"]["prompts"])
        counts = {}
        for r in log["rows"]:
            counts[r["session"]] = counts.get(r["session"], 0) + 1
        self.assertEqual(counts, {x[0]: x[f.index("prompts")] for x in data["sessions"]})
        by = {x[0]: session_row(data, x) for x in data["sessions"]}
        self.assertEqual(by["claude-mac-s1"]["commits"], 1)
        self.assertEqual(log["sessions"]["claude-mac-s1"]["commits"], ["a" * 40])
        self.assertTrue(by["claude-mac-s3"]["articleWork"])

    def test_founding_prompts_only_from_approved_founding_sessions(self):
        from widgets import w12_every_prompt as ep
        data, log = self.build(founding=True)
        t = data["totals"]
        self.assertEqual((t["foundingPrompts"], t["foundingSessions"]), (1, 1))
        first = log["rows"][0]
        self.assertEqual((first["session"], first["phase"]), ("claude-mac-f1", "founding"))
        self.assertEqual(data["ticks"][0][0], 0)  # 20 June Pacific, the window's first day
        self.assertEqual(data["phases"][0]["from"], 0)
        f = data["sessionFields"]
        self.assertTrue(session_row(data, data["sessions"][0])["founding"])
        for blob in (json.dumps(data), json.dumps(log)):
            self.assertNotIn("Bun", blob)
        for decision, name in (("exclude", ep.RAW), (None, ep.FOUNDING_APPROVED), ("", ep.FOUNDING_APPROVED)):
            with self.assertRaises(SystemExit) as cm:
                self.build(founding=True, founding_decision=decision)
            self.assertIn(name, str(cm.exception.code))

    def test_edited_title_source_column(self):
        """An edited title is Claude-written unless edited_title_source says claude-code (a typo fix)."""
        data, _ = self.build(review={"c1": ("edit", "Old title.", "defects", "claude-code"),
                                     "s3": ("edit", "Draft the article", "article")})
        f = data["sessionFields"]
        by = {x[0]: session_row(data, x) for x in data["sessions"]}
        self.assertEqual(by["codex-mac-c1"]["titleSource"], "claude-code")
        self.assertEqual(by["claude-mac-s3"]["titleSource"], "claude-written")  # same text, still edited
        with self.assertRaises(SystemExit):
            self.build(review={"c1": ("edit", "Old title.", "defects", "someone")})

    def test_busiest_day_counts_sessions_with_a_non_command_prompt(self):
        def r(pid, ts, session, text):
            """A raw row on the Mac."""
            return {"id": pid, "ts": ts, "tool": "claude", "machine": "mac", "session": session, "text": text,
                    "chars": len(text), "kind": "prompt", "pasted": "none", "model": None, "effort": None,
                    "mode": None, "settings_source": "unknown", "article_work": False, "commits": [],
                    "commits_basis": "none", "ids": [], "phase": "post-Bun", "decision": False, "branch": None,
                    "project": "/x/rustty"}
        extra = [r("claude-mac-20260801T170000Z-0", "2026-08-01T17:00:00Z", "s9", "look at the window"),
                 r("claude-mac-20260801T180000Z-0", "2026-08-01T18:00:00Z", "s1", "/resume"),
                 r("claude-mac-20260806T170000Z-0", "2026-08-06T17:00:00Z", "s9", "and again")]
        data, _ = self.build(extra_raw=extra)
        # s1 spans 1 July to 1 August, s9 is active on 1 and 6 August: a /resume does not open s1 on 1 August,
        # and no day has two sessions with a typed prompt.
        busy = data["totals"]["busiestParallelDay"]
        self.assertEqual((busy["day"], busy["open"]), ("2026-07-01", 1))
        self.assertIn("2026-08-06", busy["tied"])  # every other day with one session ties, in order
        self.assertEqual(busy["tied"], sorted(busy["tied"]))
        self.assertNotIn("2026-07-01", busy["tied"])
        f = data["sessionFields"]
        by = {x[0]: session_row(data, x) for x in data["sessions"]}
        self.assertEqual(by["claude-mac-s1"]["spannedDays"], 32)
        self.assertEqual(by["claude-mac-s9"]["spannedDays"], 6)
        t = data["totals"]
        self.assertEqual(t["longestSessionPrompts"], {"key": "claude-mac-s1", "prompts": 3, "spannedDays": 32})
        self.assertNotIn("longestSessionHours", t)
        self.assertIsInstance(t["medianPromptsPerSession"], int)
        extra.append(r("claude-mac-20260801T190000Z-0", "2026-08-01T19:00:00Z", "s1", "typed this time"))
        data, _ = self.build(extra_raw=extra)
        self.assertEqual(data["totals"]["busiestParallelDay"], {"day": "2026-08-01", "open": 2, "tied": []})

    def test_excluded_session_message(self):
        with self.assertRaises(SystemExit) as cm:
            self.build(review={"s3": ("exclude", "", "")})
        self.assertIn("has 1 prompt in", str(cm.exception.code))

    def test_title_check_reads_every_review(self):
        """A value redacted only in the earlier extract's review is refused in a title too."""
        from widgets import w12_every_prompt as ep
        root = ep_fixture(self.tmp(), review={"s3": ("edit", "Draft for zorblax-example", "article")})
        (root / "data" / ep.REDACTION_REVIEWS[0]).write_text(
            "id,start,end,match,reason,decision\nold-1,0,15,zorblax-example,person,redact\n")
        with self.assertRaises(SystemExit) as cm:
            ep.build(root, None)
        self.assertIn("redacted value", str(cm.exception.code))

    def test_weeks_recomputable_from_ticks(self):
        """The ticks carry everything the weekly settings bars need, so the widget can filter them."""
        data, _ = self.build()
        self.assertEqual(_weeks_from_ticks(data), {w["week"]: {k: w[k] for k in ("settings", "inferred", "modes")}
                                                    for w in data["weeks"]})

    def test_packing_keeps_a_gap(self):
        """Two sessions on one row are at least GAP_UNITS apart at the window's scale, so neither the
        bars nor their centred hit targets (HIT_UNITS wide at least) of neighbours ever touch."""
        from widgets import w12_every_prompt as ep
        import random
        ndays = 113
        min_len, gap = ep.pack_scale(ndays)
        per_unit = ndays * 1440 / ep.PLOT_UNITS
        self.assertGreaterEqual(ep.GAP_UNITS, 3 + ep.HIT_UNITS / 2)
        rnd = random.Random(11)
        spans = sorted((a, a + rnd.choice([0, 30, 600, 5000])) for a in (rnd.randrange(0, ndays * 1440) for _ in range(300)))
        rows = ep.pack_rows(spans, min_len, gap)
        per = {}
        for (a, b), r in zip(spans, rows):
            x1, x2 = a / per_unit, max(b, a + min_len) / per_unit
            w = x2 - x1
            hit = (x1 - max(0, ep.HIT_UNITS - w) / 2, x2 + max(0, ep.HIT_UNITS - w) / 2)
            per.setdefault(r, []).append((x1, x2, hit))
        for r, xs in per.items():
            for (_, e1, h1), (s2, _, h2) in zip(xs, xs[1:]):
                self.assertGreater(s2 - e1, 3, r)  # visible gap
                self.assertGreater(h2[0], h1[1], r)  # hit targets apart
        # Without the gap two touching spans share a row; with it they do not.
        self.assertEqual(ep.pack_rows([(0, 100), (101, 200)], 1, 0), [0, 0])
        self.assertEqual(ep.pack_rows([(0, 100), (101, 200)], 1, gap), [0, 1])

    def test_category_rules(self):
        """One category per prompt: setting, session and other slash commands, shell escapes, decisions, instructions."""
        from widgets import w12_every_prompt as ep
        cases = {
            "/model": "setting", "/model opus": "setting", "/effort high": "setting", "/fast": "setting",
            "/config": "setting", "/permissions": "setting", "/output-style": "setting", "/MODEL": "setting",
            "/clear": "session", "/resume": "session", "/compact keep it short": "session",
            "/remote-control": "session", "/continue": "session", "/rename my-session": "session",
            "/login": "other-command", "/workflows": "other-command", "/status": "other-command",
            "/btw what is X?": "instruction", "/goal port the parser": "instruction", "/BTW why?": "instruction",
            "/btw": "other-command", "/goal": "other-command", "/goal   ": "other-command",
            "/modelx": "other-command", "/rc": "session", "/moidel": "setting", "/moidel opus": "setting",
            "!cursor .": "shell", "!git status": "shell", "  !ls": "shell", "!ok": "shell",
            "ok": "decision", "B, go ahead": "decision", "continue": "decision",
            "port the parser next": "instruction", "/Users/x/file.txt fails": "instruction",
            "why does it print ! here": "instruction", "": "instruction",
        }
        for text, want in cases.items():
            self.assertEqual(ep.category(text), want, text)
        self.assertEqual(ep.CATEGORIES, ["instruction", "decision", "shell", "setting", "session", "other-command"])

    def test_categories_in_ticks_rows_and_totals(self):
        from widgets import w12_every_prompt as ep

        def r(pid, ts, session, text):
            """A raw row on the Mac."""
            return {"id": pid, "ts": ts, "tool": "claude", "machine": "mac", "session": session, "text": text,
                    "chars": len(text), "kind": ep.prompt_kind(text), "pasted": "none", "model": None,
                    "effort": None, "mode": None, "settings_source": "unknown", "article_work": False,
                    "commits": [], "commits_basis": "none", "ids": [], "phase": "post-Bun", "decision": False,
                    "branch": None, "project": "/x/rustty"}
        extra = [r("claude-mac-20260801T170000Z-0", "2026-08-01T17:00:00Z", "s9", "!cursor ."),
                 r("claude-mac-20260801T180000Z-0", "2026-08-01T18:00:00Z", "s9", "/model opus"),
                 r("claude-mac-20260801T190000Z-0", "2026-08-01T19:00:00Z", "s9", "/login")]
        data, log = self.build(extra_raw=extra, review={"s9": ("edit", "Command-only session", "command-only")})
        self.assertEqual(data["categories"], ep.CATEGORIES)
        for tick, row in zip(data["ticks"], log["rows"]):
            self.assertEqual(data["categories"][tick[ep.TICK_CATEGORY]], row["category"], row["id"])
        t = data["totals"]
        self.assertEqual(t["byCategory"], {"instruction": 2, "decision": 2, "shell": 1, "setting": 1, "session": 1,
                                           "other-command": 1})
        self.assertEqual(sum(t["byCategory"].values()), t["prompts"])
        self.assertEqual(t["commandsAndShell"], 4)
        self.assertEqual(t["byCategory"]["decision"], t["decisions"])
        # Substantive figures: instructions and decisions only (the secret prompt, "ok", "B", "draft it").
        subst = [row for row in log["rows"] if row["category"] in ("instruction", "decision")]
        self.assertEqual(t["promptsSubstantive"], 4)
        self.assertEqual(t["medianCharsSubstantive"], statistics.median(x["chars"] for x in subst))
        self.assertEqual(t["underTenWordsSubstantive"], sum(1 for x in subst if len(x["text"].split()) < 10))
        # A session of a shell escape and commands only is command-only; it needs the command-only area.
        f = data["sessionFields"]
        by = {x[0]: session_row(data, x) for x in data["sessions"]}
        self.assertTrue(by["claude-mac-s9"]["commandsOnly"])
        with self.assertRaises(SystemExit):
            self.build(extra_raw=extra)

    def test_packing_never_overlaps(self):
        from widgets import w12_every_prompt as ep
        import random
        rnd = random.Random(7)
        spans = sorted((a, a + rnd.choice([0, 30, 600, 5000])) for a in (rnd.randrange(0, 20000) for _ in range(300)))
        rows = ep.pack_rows(spans)
        self.assertEqual(ep.pack_rows([(0, 10), (5, 20), (600, 700)]), [0, 1, 0])
        per = {}
        for (a, b), r in zip(spans, rows):
            per.setdefault(r, []).append((a, max(b, a + ep.PACK_MINUTES)))
        for r, xs in per.items():
            for (a1, b1), (a2, b2) in zip(xs, xs[1:]):
                self.assertGreater(a2, b1, r)
        self.assertLessEqual(max(rows), max(sum(1 for a, b in spans if a <= t <= max(b, a + ep.PACK_MINUTES))
                                            for t, _ in spans))


@unittest.skipUnless((RESEARCH / "data" / "prompts-raw-2026-10-11.jsonl").exists(), "prompt extract not available")
class EveryPromptRealDataTest(unittest.TestCase):
    """every-prompt on the real extract: size, privacy, manifest, purged window, facts."""

    @classmethod
    def setUpClass(cls):
        """Build once from the research directory."""
        from widgets import w12_every_prompt as ep
        cls.ep = ep
        cls.data = ep.build(RESEARCH, MOCKBUILD)
        cls.log = cls.data.pop("__extra__")[ep.LOG_PATH]
        cls.manifest = json.loads((RESEARCH / "data" / ep.MANIFEST).read_text())
        cls.raw = {}
        with open(RESEARCH / "data" / ep.RAW) as f:
            for line in f:
                r = json.loads(line)
                cls.raw[r["id"]] = r

    def test_summary_under_60_kb(self):
        self.assertLess(len(self.ep.dump(self.data).encode()), 60 * 1024)

    def test_weeks_recomputable_from_ticks(self):
        self.assertEqual(_weeks_from_ticks(self.data), {w["week"]: {k: w[k] for k in ("settings", "inferred", "modes")}
                                                         for w in self.data["weeks"]})

    def test_redacted_spans_absent_from_both_outputs(self):
        """Every approved `match` of a redact row is absent from both outputs, raw and JSON-escaped."""
        blobs = [self.ep.dump(self.data), self.ep.dump(self.log), json.dumps(self.data), json.dumps(self.log)]
        n = 0
        in_log = {r["id"] for r in self.log["rows"]}
        with open(RESEARCH / "data" / self.ep.APPROVED, newline="") as f:
            for r in csv.DictReader(f):
                if r["decision"] != "redact":
                    continue
                n += r["id"] in in_log  # a redaction of a website-folder prompt is moot: the prompt is left out
                self.assertTrue(r["match"], r["id"])
                for b in blobs:
                    self.assertNotIn(r["match"], b, r["id"])
                    self.assertNotIn(json.dumps(r["match"], ensure_ascii=False)[1:-1], b, r["id"])
                    self.assertNotIn(json.dumps(r["match"])[1:-1], b, r["id"])
        self.assertEqual(n, self.data["totals"]["redactions"])

    def test_reconciles_with_manifest(self):
        m = json.loads((RESEARCH / "data" / self.ep.MANIFEST).read_text())
        t = self.data["totals"]
        # The manifest counts every prompt of the extract; the log leaves the website-folder prompts out.
        web = [r for r in self.raw.values() if r.get("project_marker") == self.ep.WEBSITE_MARKER]
        self.assertTrue(web)
        self.assertEqual(t["prompts"], m["counts"]["prompts"] - len(web))
        self.assertEqual(t["prompts"] + len(web), sum(s["kept"] for s in m["sources"].values() if "kept" in s))
        self.assertEqual(t["bySource"], {k: n - sum(1 for r in web if self.ep._lane(r) == k)
                                         for k, n in m["counts"]["by_source"].items()})
        self.assertEqual(t["pastedLost"], m["counts"]["pasted"].get("lost", 0) - sum(r["pasted"] == "lost" for r in web))
        self.assertEqual(t["settingsUnknown"], m["counts"]["settings_source"].get("unknown", 0)
                         - sum(r["settings_source"] == "unknown" for r in web))
        self.assertEqual(sum(1 for r in self.log["rows"] if r["kind"] == "command"),
                         m["counts"]["kind"].get("command", 0) - sum(self.ep.prompt_kind(r["text"]) == "command" for r in web))
        self.assertEqual({r["id"] for r in self.log["rows"]}, set(self.raw) - {r["id"] for r in web})
        self.assertEqual(self.data["rules"]["decision"], m["rules"]["decision"])
        for r in self.log["rows"]:
            self.assertEqual(r["decision"], self.raw[r["id"]]["decision"], r["id"])

    def test_purged_window_settings(self):
        """A Mac row before the purge has settings only from a surviving transcript or commit trailers."""
        tx = Path.home() / ".claude" / "projects"
        for r in self.log["rows"]:
            if r["tool"] == "claude" and r["machine"] == "mac" and \
                    self.ep._pacific(r["ts"]).date().isoformat() < self.data["purgedBefore"]:
                self.assertIn(r["settingsSource"], {"unknown", "commit-trailer", "transcript"}, r["id"])
                if r["settingsSource"] == "unknown":
                    self.assertIsNone(r["model"], r["id"])
                if r["settingsSource"] == "transcript" and tx.is_dir():
                    sid = self.raw[r["id"]]["session"]
                    self.assertTrue(list(tx.glob(f"*/{sid}.jsonl")), f"{r['id']}: no surviving transcript")

    def test_glossary_plain(self):
        for k, v in self.log["glossary"].items():
            self.assertTrue(v, k)
            self.assertIsNone(REFERENCE_PIN_RE.search(v.replace("resync", "")), (k, v))

    def test_facts_match_builder(self):
        with open(HERE / "facts.csv", newline="") as f:
            facts = {r["id"]: r for r in csv.DictReader(f)}
        t = self.data["totals"]
        want = {"prompts_total": t["prompts"], "prompts_claude_mac": t["bySource"]["claude-mac"],
                "prompts_claude_vm": t["bySource"]["claude-vm"], "prompts_codex": t["bySource"]["codex"],
                "prompts_median_chars": t["medianChars"],
                "prompts_decisions_share": round(100 * t["decisions"] / t["prompts"], 1),
                "prompts_redactions": t["redactions"], "prompts_pasted_lost": t["pastedLost"],
                "prompts_settings_unknown": t["settingsUnknown"],
                "prompts_settings_inferred": t["settingsInferred"], "prompts_article_work": t["articleWork"]}
        for k, v in want.items():
            self.assertEqual(float(facts[k]["value"]), float(v), k)
            self.assertEqual(facts[k]["provenance"], "MEASURED", k)
            # The extractor's manifest still counts the website-folder prompts, so every figure is read
            # from the counts file, which does not.
            self.assertEqual(facts[k]["source"], f"research/data/{self.ep.COUNTS}", k)
        self.assertEqual([k for k in facts if "website" in k], [])  # the article is about the port
        c = self.ep.counts(self.data, self.log, self.manifest)
        for k, key in {"prompts_after_cutoff": "after_cutoff_prompts",
                       "prompts_after_cutoff_port": "after_cutoff_port_work",
                       "sessions_total": "sessions", "sessions_command_only": "sessions_command_only",
                       "sessions_founding_prompts": "founding_prompts",
                       "sessions_busiest_parallel": "sessions_busiest_parallel_open",
                       "sessions_longest_prompts": "sessions_longest_prompts",
                       "sessions_longest_spanned_days": "sessions_longest_spanned_days",
                       "sessions_median_prompts": "sessions_median_prompts",
                       "prompts_instructions": "category_instruction", "prompts_decisions": "category_decision",
                       "prompts_commands_hidden": "commands_and_shell"}.items():
            self.assertEqual(float(facts[k]["value"]), float(c[key]), k)
            self.assertEqual(facts[k]["source"], f"research/data/{self.ep.COUNTS}", k)

    def test_categories_reconcile(self):
        d, log, ep = self.data, self.log, self.ep
        t = d["totals"]
        self.assertEqual(sum(t["byCategory"].values()), t["prompts"])
        self.assertEqual(t["byCategory"]["decision"], t["decisions"])
        # Every setting, session or other command is a slash command; a slash command counted as an
        # instruction is /btw or /goal with text.
        text_cmds = sum(1 for r in log["rows"] if r["kind"] == "command" and r["category"] == "instruction")
        self.assertEqual(t["byCategory"]["setting"] + t["byCategory"]["session"] + t["byCategory"]["other-command"]
                         + text_cmds, sum(1 for r in log["rows"] if r["kind"] == "command"))
        for r in log["rows"]:
            if r["kind"] == "command" and r["category"] == "instruction":
                self.assertIn(ep.command_name(self.raw[r["id"]]["text"]), ep.TEXT_COMMANDS, r["id"])
        self.assertEqual(t["commandsAndShell"], t["prompts"] - t["promptsSubstantive"])
        for tick, row in zip(d["ticks"], log["rows"]):
            self.assertEqual(d["categories"][tick[ep.TICK_CATEGORY]], row["category"], row["id"])
            self.assertEqual(row["category"], ep.category(self.raw[row["id"]]["text"]), row["id"])

    def test_sessions_reconcile(self):
        """Every prompt in exactly one reviewed session; founding prompts only from approved founding sessions."""
        d, log, ep = self.data, self.log, self.ep
        f = d["sessionFields"]
        sessions = [session_row(d, x) for x in d["sessions"]]
        keys = [x["key"] for x in sessions]
        self.assertEqual(len(keys), len(set(keys)))
        self.assertEqual(set(keys), set(log["sessions"]))
        # The manifest counts the sessions made only of website-folder prompts, which the log leaves out.
        web_sessions = {(r["tool"], r["machine"], r["session"]) for r in self.raw.values()
                        if r.get("project_marker") == ep.WEBSITE_MARKER}
        self.assertEqual(len(sessions) + len(web_sessions), self.manifest["counts"]["sessions"])
        self.assertEqual(sum(x["prompts"] for x in sessions), d["totals"]["prompts"])
        for tick, r in zip(d["ticks"], log["rows"]):
            self.assertEqual(keys[tick[ep_mod().T_SESSION]], r["session"], r["id"])
        with open(RESEARCH / "data" / ep.FOUNDING_APPROVED, newline="") as fh:
            included = {r["session"] for r in csv.DictReader(fh) if r["decision"] == "include"}
        for r in log["rows"]:
            raw = self.raw[r["id"]]
            if raw.get("founding"):
                self.assertIn(raw["session"], included, r["id"])
        self.assertEqual(d["totals"]["foundingPrompts"], self.manifest["counts"]["founding_prompts"])
        self.assertEqual(d["totals"]["foundingSessions"], self.manifest["counts"]["founding_sessions"])
        # The extractor counts sessions of slash commands only; the builder's command-only sessions
        # (no instruction or decision) are those minus the ones whose /btw or /goal carries text.
        n = sum(1 for x in sessions if x["commandsOnly"])
        self.assertEqual(n, d["totals"]["sessionsCommandOnly"])
        self.assertLessEqual(n, self.manifest["counts"]["sessions_commands_only"])
        for x in sessions:
            self.assertIn(x["area"], d["areas"] + [ep.AREA_COMMAND_ONLY], x["key"])
            self.assertEqual(x["commandsOnly"], x["area"] == ep.AREA_COMMAND_ONLY, x["key"])
            self.assertFalse(jargon.strip_internal_ids(x["title"])[1], x["key"])
        per_row = {}
        for x in sessions:
            a = x["firstDay"] * 1440 + x["firstMin"]
            per_row.setdefault(x["row"], []).append((a, max(x["lastDay"] * 1440 + x["lastMin"], a + ep.PACK_MINUTES)))
        for row, spans in per_row.items():
            spans.sort()
            for (a1, b1), (a2, b2) in zip(spans, spans[1:]):
                self.assertGreater(a2, b1, row)
        # No extractor phase key (pre-Bun, ...) in a shipped label: rows carry the era keys.
        labels = [x["title"] for x in sessions] + [p["label"] for p in d["phases"]] + [p["key"] for p in d["phases"]]
        self.assertEqual({r["phase"] for r in log["rows"]}, set(ep.PHASE_KEYS))
        for text in labels:
            self.assertNotIn("Bun", text)
            self.assertNotIn("workstream", text.lower())

    def test_counts_file_matches_builder(self):
        """The research counts file (committed next to the redacted log) is what counts() makes now."""
        c = self.ep.counts(self.data, self.log, self.manifest)
        self.assertEqual(json.loads((RESEARCH / "data" / self.ep.COUNTS).read_text()), c)
        self.assertEqual(c["after_cutoff_article_work"] + c["after_cutoff_port_work"], c["after_cutoff_prompts"])
        self.assertLessEqual(c["after_cutoff_article_work"], c["article_work"])
        # Reconciles with the article's other prompt figures (Claude Code, to the same cutoff).
        with open(HERE / "facts.csv", newline="") as f:
            facts = {r["id"]: r for r in csv.DictReader(f)}
        # The article's other figures count Claude Code prompts typed in the repository: the founding
        # prompts (before it existed) are on top of them.
        self.assertEqual(c["before_cutoff_claude"] - c["founding_prompts"], float(facts["rustty_prompts"]["value"]))
        self.assertEqual(c["before_cutoff_claude"] + c["before_cutoff_codex"] + c["after_cutoff_prompts"],
                         c["prompts"])


class ScanRedactedValuesTest(unittest.TestCase):
    """scan_redacted_values: person values count only in prompt-log dataset files; others everywhere."""

    def test_person_values_only_in_dataset_files(self):
        import scan_redacted_values as scan
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d) / "repo"
            repo.mkdir()
            git = lambda *a: subprocess.run(["git", "-C", str(repo), *a], check=True, capture_output=True)
            git("init", "-q")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Test")
            (repo / "notes.md").write_text("path /home/zorblax-example/x and someone@example.org\n")
            git("add", ".")
            git("commit", "-q", "-m", "notes by zorblax-example")
            data = repo / "public" / "porting"
            data.mkdir(parents=True)
            (data / "every-prompt-log.json").write_text('{"t":"zorblax-example"}\n')
            (repo / "r").mkdir()
            (repo / "r" / "prompts-redacted-2026-10-11.json").write_text('{"t":"Zorblax-example"}\n')
            git("add", ".")
            git("commit", "-q", "-m", "dataset")
            approved = Path(d) / "approved.csv"
            approved.write_text("id,start,end,match,reason,decision\n"
                                "a,0,15,zorblax-example,person,redact\n"
                                "b,0,19,someone@example.org,email,redact\n"
                                "c,0,5,nobody,person,keep\n")
            rows = scan.all_redact_rows([approved])
            self.assertEqual([r[1] for r in rows], ["person", "email"])
            res = scan.scan_log(str(repo), ["HEAD"], rows)
            # [whole-word hits, commits, substring hits]
            self.assertEqual(res[rows[0][0]], [2, 1, 0])  # the two dataset files only, not notes or messages
            self.assertEqual(res[rows[1][0]], [1, 1, 1])  # email: everywhere
            self.assertEqual(scan.files_at(str(repo), "HEAD", rows[0][2], True), 2)
            self.assertEqual(scan.files_at(str(repo), "HEAD", rows[0][2]), 3)
            self.assertTrue(scan.is_dataset("src/app/porting/data/every-prompt.json"))
            self.assertTrue(scan.is_dataset(".port/porting-article/research/data/prompts-redacted-2026-10-10.json"))
            self.assertFalse(scan.is_dataset("scripts/porting/facts.csv"))
            self.assertFalse(scan.is_dataset(None))
