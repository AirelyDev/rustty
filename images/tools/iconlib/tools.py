"""External tool lookup and subprocess helpers."""
import os
import shutil
import subprocess

from . import paths

ICTOOL_DEFAULT = "/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool"
BREW_HINT = {"resvg": "brew install resvg", "icotool": "brew install icoutils"}


class ToolMissing(RuntimeError):
    pass


def tool_path(name: str) -> str:
    if name == "ictool":
        p = os.environ.get("ICTOOL", ICTOOL_DEFAULT)
        if os.path.isfile(p) and os.access(p, os.X_OK):
            return p
        raise ToolMissing(f"ictool not found at {p!r} (needs Xcode 26+ Icon Composer; override with ICTOOL=)")
    p = shutil.which(name)
    if p is None:
        raise ToolMissing(f"{name} not found on PATH ({BREW_HINT.get(name, 'install it')})")
    return p


def require(names) -> None:
    for n in names:
        tool_path(n)


def run(args) -> None:
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"{args[0]} failed ({r.returncode}): {r.stderr.strip() or r.stdout.strip()}")


def git_blob(rev: str, relpath: str) -> bytes:
    r = subprocess.run(["git", "-C", str(paths.REPO), "show", f"{rev}:{relpath}"], capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(f"git show {rev}:{relpath} failed (shallow clone?): {r.stderr.decode().strip()}")
    return r.stdout
