"""Generate src/app/porting/data/*.json for the porting article from rustty research data."""
import argparse, csv, json, os, re, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
# DEFAULT_RESEARCH and DEFAULT_MOCKBUILD are the input directories used when --research and
# --mockbuild are omitted: the PORTING_RESEARCH / PORTING_MOCKBUILD environment variables,
# falling back to the author's rustty checkout (see scripts/porting/README.md).
DEFAULT_RESEARCH = Path(os.environ.get(
    "PORTING_RESEARCH", "/Users/gael/projects/rustty/.port/journey-article/research"))
DEFAULT_MOCKBUILD = Path(os.environ.get(
    "PORTING_MOCKBUILD", "/Users/gael/projects/rustty/.port/journey-article/mockup-build"))
sys.path.insert(0, str(HERE))
from widgets import REGISTRY  # noqa: E402


def build_facts(_research: Path, _mockbuild: Path) -> dict:
    """Load facts.csv into the facts.json mapping keyed by fact id."""
    with open(HERE / "facts.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    def num(v: str):
        """Parse a CSV value as int or float, falling back to the raw string."""
        try:
            return float(v) if "." in v else int(v)
        except ValueError:
            return v

    return {r["id"]: {"display": r["display"], "value": num(r["value"]),
                      "provenance": r["provenance"], "source": r["source"], "asOf": r["as_of"],
                      "href": r["href"], "hrefLabel": r["href_label"]}
            for r in rows}


# LINK_MAP is the research link map (kind,key,url,label,notes) the commit links are generated from.
LINK_MAP = "links-2026-10-11b.csv"
# REPO_CODES abbreviates the two main repositories in commits.json ("r:<sha>", "g:<sha>").
REPO_CODES = {"AirelyDev/rustty": "r", "ghostty-org/ghostty": "g"}
# COMMIT_URL_RE splits a GitHub commit URL into owner/repo and sha.
COMMIT_URL_RE = re.compile(r"^https://github\.com/([^/]+/[^/]+)/commit/([0-9a-f]{7,40})$")


def build_commits(research: Path, _mockbuild: Path) -> dict:
    """Map every linkable commit token of the article to "<repo>:<sha>" from the research link map.

    Rustty and Ghostty commits carry the full 40-character sha under the codes in REPO_CODES;
    third-party commits keep their owner/repo and the sha as the map gives it. Rows without a
    URL (hashes that resolve in no repository) are left out, so they render as plain code.
    """
    out: dict[str, str] = {}
    with open(research / LINK_MAP, newline="") as f:
        for r in csv.DictReader(f):
            if r["kind"] != "commit" or not r["url"]:
                continue
            m = COMMIT_URL_RE.match(r["url"])
            if not m:
                raise ValueError(f"commit {r['key']}: unexpected URL {r['url']!r}")
            repo, sha = m.groups()
            if repo in REPO_CODES and len(sha) != 40:
                raise ValueError(f"commit {r['key']}: {repo} link is not a full sha")
            out[r["key"]] = f"{REPO_CODES.get(repo, repo)}:{sha}"
    return dict(sorted(out.items()))


# EXTRA_KEY is the key under which a builder returns extra output files (see main).
EXTRA_KEY = "__extra__"


def validate_meta(slug: str, data: dict) -> None:
    """Raise ValueError unless data carries a complete _meta block whose slug matches the registry slug."""
    meta = data.get("_meta")
    if not isinstance(meta, dict):
        raise ValueError(f"{slug}: builder output is missing '_meta'")
    missing = [k for k in ("slug", "generatedFrom", "asOf") if k not in meta]
    if missing:
        raise ValueError(f"{slug}: '_meta' lacks keys {missing}")
    if meta["slug"] != slug:
        raise ValueError(f"{slug}: '_meta.slug' is {meta['slug']!r}, expected {slug!r}")


def main() -> None:
    """Parse CLI args and write every (or one) generated JSON file."""
    ap = argparse.ArgumentParser()
    ap.add_argument("--research", type=Path, default=DEFAULT_RESEARCH)
    ap.add_argument("--mockbuild", type=Path, default=DEFAULT_MOCKBUILD)
    ap.add_argument("--out", type=Path, default=HERE.parent.parent / "src/app/porting/data")
    ap.add_argument("--root", type=Path, default=HERE.parent.parent,
                    help="base directory of a builder's extra outputs (default: the repository root)")
    ap.add_argument("--only")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    builders = {"facts": build_facts, "commits": build_commits, **REGISTRY}
    if a.only and a.only not in builders:
        sys.exit(f"error: unknown slug {a.only!r}; known: {', '.join(sorted(builders))}")
    for slug, fn in builders.items():
        if a.only and slug != a.only:
            continue
        data = fn(a.research, a.mockbuild)
        # A builder may return extra files under EXTRA_KEY ({path relative to --root: data}),
        # for data the page fetches instead of bundling (the every-prompt log). They are
        # written like the main file and the key is stripped.
        extras = data.pop(EXTRA_KEY, {}) if isinstance(data, dict) else {}
        if slug not in ("facts", "commits"):
            try:
                validate_meta(slug, data)
            except ValueError as e:
                sys.exit(f"error: {e}")
        (a.out / f"{slug}.json").write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")))
        print(f"wrote {slug}.json ({(a.out / f'{slug}.json').stat().st_size // 1024} KB)")
        for rel, extra in extras.items():
            path = a.root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(extra, ensure_ascii=False, separators=(",", ":")))
            print(f"wrote {rel} ({path.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
