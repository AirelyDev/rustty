#!/usr/bin/env python3
"""Rebuild Rustty's icon assets from committed sources. See images/tools/README.md."""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from iconlib import paths, tools  # noqa: E402
from iconlib import mascot, official  # noqa: E402


def cmd_paths(args) -> int:
    print(f"repo: {paths.REPO}")
    for p in (paths.FERRIS_SVG, paths.ICON_JSON):
        print(f"{'ok ' if p.is_file() else 'MISSING '}{paths.rel(p)}")
    return 0 if paths.FERRIS_SVG.is_file() and paths.ICON_JSON.is_file() else 1


COMMANDS = {"paths": cmd_paths, "mascot": mascot.run_mascot, "official": official.run}
COMMAND_TOOLS = {"paths": [], "mascot": ["resvg"], "official": []}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="build_icons.py", description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("paths", help="print the repo root and check inputs")
    sub.add_parser("mascot", help="write the Ferris mascot layers")
    sub.add_parser("official", help="centre the mascot and tint the glass in Rustty.icon/icon.json")
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        tools.require(COMMAND_TOOLS[args.cmd])
    except tools.ToolMissing as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return COMMANDS[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
