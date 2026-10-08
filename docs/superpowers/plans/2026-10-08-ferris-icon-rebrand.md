# Ferris Icon Rebrand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Ghostty's ghost with Ferris the Rustacean in every Rustty icon asset, using a reproducible, committed, macOS-only Python generator.

**Architecture:** A small Python package, `images/tools/iconlib/`, has one module per stage: raster helpers, mascot layers, the `icon.json` edit, flat exports, alternate plates, alternate styles and verification. A CLI, `images/tools/build_icons.py`, drives it. It reads committed inputs (the Ferris SVG, `Rustty.icon`, ghost-free alternate plates, and the pre-rebrand originals from git) and writes every derived icon in place. Each art stage ends at a user review gate in the brainstorm browser companion.

**Tech Stack:** Python 3.14 (`python3 -I`), Pillow 12, numpy 2, stdlib `unittest`; CLIs `resvg` 0.47 (Homebrew), `icotool` 0.32 (icoutils), and Icon Composer's `ictool` (Xcode 26+).

**Spec:** `docs/superpowers/specs/2026-10-08-ferris-icon-rebrand-design.md`. Read it before starting; this plan argues from it.

## Global Constraints

- Mascot: stock rustacean.net `rustacean-flat-happy.svg` (CC0). Body `#F74C00`, shade `#A52B00`, black eyes with white highlights. No prompt glyphs (D1).
- Placement (D2): Ferris width = **0.68 × screen width**, horizontally centred, vertical centre **1.2 % of screen height above** the screen centre.
- Backgrounds unchanged (D3). Alternates are style-matched (D4). The custom-style mascot is a neutral grey Ferris (D5).
- Config key `macos-icon-ghost-color` and its doc text stay as in Ghostty (D6).
- Generator: Python 3 + Pillow + numpy, plus the `resvg` / `ictool` / `icotool` CLIs, at `images/tools/build_icons.py`. Outputs are committed, and builds/CI never run it (D7).
- Plates are produced once, reviewed and committed. `all` never re-runs ghost removal (D8).
- Flat-export framing: opaque bbox within **±2 px of 96,100–919,923** (1024 canvas), and alpha-mask **IoU ≥ 0.98** against `git show 401b95b0e:images/gnome/1024.png`.
- `rustty.ico` entries are **16/32/48/256**, 32-bit. `favicon.png` is **32 px**.
- Determinism: two consecutive `all` runs give byte-identical outputs. Randomness uses fixed seeds. PNGs carry no text/time/ICC chunks.
- Pre-rebrand revision: **`401b95b0e`**.
- **Do not change:** Swift sources, the Xcode project, `xtask`, snap/flatpak manifests, `Doxyfile`, config code/docs, asset names, or any `Contents.json`.
- Every commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never create a PR or an issue (AGENTS.md).
- Run all Python with `python3 -I` (isolated mode). Tests: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`, from the repo root.
- Work in the worktree `/Users/gael/projects/rustty/.claude/worktrees/icons-rebrand` (branch `icons-rebrand`).

## Review Focus

1. **Display-P3 renders.** `ictool` emits PNGs tagged Display P3, and the current exports are untagged sRGB. If the conversion is skipped, every flat export is colour-shifted. A test in **Task 3** pins it: `render_icon()` output has no ICC profile and its colours differ from a naive untagged read.
2. **`ghosty.png` mode.** Today's custom-style layer is greyscale+alpha (`LA`) with `template-rendering-intent: template`. Writing RGBA would silently change how the asset catalog treats it. A test in **Task 4** pins mode `LA`.
3. **Running from another directory.** Someone may run `build_icons.py` from outside the repo root, so paths must resolve from `__file__`. A test in **Task 1** runs `paths` from a temp dir.
4. **Missing CLI tool.** A missing `ictool`/`icotool` must fail with a clear message, exit 2, and write nothing. A test in **Task 3** runs `flats` with `ICTOOL=/nonexistent` and checks that no output's mtime changes.
5. **Plates come from git history.** Plates must be cut from the pre-rebrand originals read out of git, not from the working tree. After `alternates` runs, the working-tree PNG already has a Ferris in it. A test in **Task 5** pins that `plates.original()` equals the `git show 401b95b0e:` blob.

---

## File structure

```
images/
  ferris/
    rustacean-flat-happy.svg      # Task 1: verbatim CC0 source
    PROVENANCE.md                 # Task 1
  alternates/<name>/plate.png     # Tasks 5, 7: ghost-free plates (lowercase dir names)
  tools/
    build_icons.py                # CLI (Task 1, extended by every task)
    README.md                     # Task 8
    iconlib/
      __init__.py                 # Task 1
      paths.py                    # Task 1: every input/output path + size tables
      tools.py                    # Task 1: CLI tool lookup, subprocess, git blobs
      raster.py                   # Task 1: PNG I/O, sRGB, Ferris raster, masks, placement
      mascot.py                   # Tasks 2, 4: Rustty.png layer, grey custom layer
      official.py                 # Task 2: icon.json edit
      flats.py                    # Task 3: ictool render, framing, all flat exports
      preview.py                  # Task 4: CG blend replica, custom-style preview
      review.py                   # Tasks 3, 4, 6: review sheets into target/icon-review/
      plates.py                   # Task 5: ghost-free plate drafting
      styles.py                   # Tasks 6, 7: per-alternate Ferris styles
      alternates.py               # Task 6: plate + styled Ferris composition
      verify.py                   # Task 8: determinism + no-ghost checks
    tests/
      __init__.py                 # Task 1
      test_raster.py, test_cli.py                          # Task 1
      test_mascot.py, test_official.py                     # Tasks 2, 4
      test_flats.py                                        # Task 3
      test_preview.py                                      # Task 4
      test_plates.py                                       # Task 5
      test_styles.py, test_alternates.py                   # Task 6
      test_verify.py                                       # Task 8
```

Review sheets go to `target/icon-review/`. `target/` is git-ignored, so they never get committed.

### Review-gate procedure (used by Tasks 3, 4, 6, 7)

1. Make sure the brainstorm companion is alive. Check that `.superpowers/brainstorm/*/state/server-info` exists and that `server-stopped` does not. If it is down, restart it with
   `bash /Users/gael/.claude/plugins/cache/claude-plugins-official/superpowers/6.4.1/skills/brainstorming/scripts/start-server.sh --project-dir /Users/gael/projects/rustty/.claude/worktrees/icons-rebrand --open`
   and take `screen_dir` from its JSON. It reuses the same port.
2. Copy the task's `target/icon-review/*.png` files into `screen_dir`. Write a **new** HTML fragment there, e.g. `review-official.html`, with `<img src="/files/<name>.png">` tags and a one-line question per image. Use the Write tool, not a heredoc.
3. Give the user the full URL, including `?key=`, plus a one-line summary, and **stop until the user replies**. Read `state_dir/events` for any clicks.
4. If the user asks for changes, adjust the named constants, regenerate, push `review-<x>-v2.html`, and repeat. Only an explicit approval passes the gate.

---

### Task 1: Ferris source and the generator foundation

**Files:**

- Create: `images/ferris/rustacean-flat-happy.svg`, `images/ferris/PROVENANCE.md`
- Create: `images/tools/build_icons.py`, `images/tools/iconlib/{__init__,paths,tools,raster}.py`
- Test: `images/tools/tests/{__init__,test_raster,test_cli}.py`

**Interfaces:**

- Produces:
  - `paths`: `REPO`, `FERRIS_SVG`, `ICON_DIR`, `ICON_JSON`, `ICON_MASCOT`, `XCASSETS`, `APPICON_IMAGESET`, `APPICON_FILES: dict[int,str]`, `CUSTOM_DIR`, `CUSTOM_MASCOT`, `GNOME_DIR`, `GNOME_SIZES`, `APPRT_GNOME_DIR`, `ICONS_DIR`, `ICONS_MAP: dict[str,int]`, `WIN_ICO`, `ICO_SIZES`, `FAVICON`, `FAVICON_SIZE`, `ALT_NAMES`, `alt_output(name)->Path`, `plate_path(name)->Path`, `PRE_REBRAND_REV`, `REVIEW_DIR`, `rel(path)->str`.
  - `tools`: `ToolMissing`, `tool_path(name)->str`, `require(names)`, `run(args)`, `git_blob(rev, relpath)->bytes`.
  - `raster`: `save_png(img, path)`, `to_srgb(img)->Image`, `ferris_aspect()->float`, `ferris_at(width)->Image`, `Masks`, `classify(img)->Masks`, `ferris_box(screen_bbox, aspect=None)->(x,y,w,h)`, `opaque_bbox(img, thr=128)->(x0,y0,x1,y1)` (half-open), `iou(a,b)->float`, `glow(alpha_l, rgb, radius, strength)->Image`.
  - CLI: `build_icons.main(argv)->int`, `COMMANDS: dict[str, callable]`, `COMMAND_TOOLS: dict[str, list[str]]`.

- [ ] **Step 1: Vendor the Ferris SVG**

```bash
mkdir -p images/ferris
curl -fsSL https://rustacean.net/assets/rustacean-flat-happy.svg -o images/ferris/rustacean-flat-happy.svg
shasum -a 256 images/ferris/rustacean-flat-happy.svg
```

If the download fails (offline), copy it from the website fork instead, then strip its added `<desc>` line so the file matches upstream: `cp /Users/gael/projects/ghostty-website/public/ferris-rustacean-happy.svg images/ferris/rustacean-flat-happy.svg`. Record which source you used in PROVENANCE.

- [ ] **Step 2: Write `images/ferris/PROVENANCE.md`**

```markdown
# Ferris the Rustacean: provenance

- File: `rustacean-flat-happy.svg`
- Source: https://rustacean.net/assets/rustacean-flat-happy.svg
- Retrieved: 2026-10-08
- SHA-256: <paste the hash printed in Step 1>
- License: CC0 1.0. rustacean.net publishes Ferris under a CC0 waiver (https://rustacean.net/).

Used unmodified as the mascot source for every Rustty icon. `images/tools/build_icons.py` rasterises it with `resvg`.
```

- [ ] **Step 3: Write the failing tests**

`images/tools/tests/__init__.py`: an empty file.

`images/tools/tests/test_raster.py`:

```python
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image, ImageCms

from iconlib import paths, raster


def png_chunk_types(data: bytes) -> list[bytes]:
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    i, out = 8, []
    while i < len(data):
        n = int.from_bytes(data[i:i + 4], "big")
        out.append(data[i + 4:i + 8])
        i += 12 + n
    return out


class SavePngTest(unittest.TestCase):
    def test_deterministic_and_chunk_free(self):
        img = Image.new("RGBA", (8, 8), (247, 76, 0, 255))
        img.info["dpi"] = (144, 144)
        img.info["icc_profile"] = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a.png", Path(tmp) / "b.png"
            raster.save_png(img, a)
            raster.save_png(img, b)
            self.assertEqual(a.read_bytes(), b.read_bytes())
            self.assertEqual(set(png_chunk_types(a.read_bytes())), {b"IHDR", b"IDAT", b"IEND"})


class FerrisTest(unittest.TestCase):
    def test_ferris_at_exact_width(self):
        f = raster.ferris_at(459)
        self.assertEqual(f.width, 459)
        self.assertEqual(f.height, round(459 / raster.ferris_aspect()))
        self.assertEqual(f.mode, "RGBA")

    def test_classify_finds_disjoint_regions(self):
        m = raster.classify(raster.ferris_at(1200))
        for name in ("body", "shade", "dark", "white"):
            self.assertGreater(getattr(m, name).sum(), 100, name)
        stack = np.stack([m.body, m.shade, m.dark, m.white]).astype(int)
        self.assertLessEqual(stack.sum(0).max(), 1)
        self.assertGreater(m.body.sum(), m.shade.sum())


class PlacementTest(unittest.TestCase):
    def test_ferris_box_rule(self):
        # screen 675 wide: w = round(0.68*675) = 459, h = round(459/2) = 230,
        # centre (511.5, 511.5 - 0.012*675 = 503.4)
        self.assertEqual(raster.ferris_box((174, 174, 849, 849), aspect=2.0), (282, 388, 459, 230))

    def test_opaque_bbox_and_iou(self):
        a = Image.new("RGBA", (10, 10), (0, 0, 0, 0))
        a.paste((255, 0, 0, 255), (2, 3, 6, 8))
        self.assertEqual(raster.opaque_bbox(a), (2, 3, 6, 8))
        m = np.zeros((10, 10), bool)
        m[3:8, 2:6] = True
        self.assertEqual(raster.iou(m, m), 1.0)
        self.assertEqual(raster.iou(m, ~m), 0.0)


class SrgbTest(unittest.TestCase):
    def test_untagged_passthrough(self):
        img = Image.new("RGBA", (4, 4), (10, 20, 30, 200))
        out = raster.to_srgb(img)
        self.assertEqual(out.getpixel((0, 0)), (10, 20, 30, 200))
        self.assertNotIn("icc_profile", out.info)


class PathsTest(unittest.TestCase):
    def test_repo_root_and_inputs(self):
        self.assertTrue((paths.REPO / "Cargo.toml").is_file())
        self.assertTrue(paths.FERRIS_SVG.is_file())
        self.assertTrue(paths.ICON_JSON.is_file())
        self.assertEqual(len(paths.ALT_NAMES), 8)
        self.assertEqual(paths.rel(paths.ICON_JSON), "images/Rustty.icon/icon.json")


if __name__ == "__main__":
    unittest.main()
```

`images/tools/tests/test_cli.py`:

```python
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from iconlib import paths, tools

BUILD_ICONS = Path(__file__).resolve().parents[1] / "build_icons.py"


class CliTest(unittest.TestCase):
    def test_paths_command_from_foreign_cwd(self):
        with tempfile.TemporaryDirectory() as tmp:
            r = subprocess.run([sys.executable, "-I", str(BUILD_ICONS), "paths"],
                               cwd=tmp, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn(str(paths.REPO), r.stdout)

    def test_missing_tool_message(self):
        old = os.environ.get("ICTOOL")
        os.environ["ICTOOL"] = "/nonexistent/ictool"
        try:
            with self.assertRaises(tools.ToolMissing) as cm:
                tools.tool_path("ictool")
            self.assertIn("ictool", str(cm.exception))
        finally:
            if old is None:
                del os.environ["ICTOOL"]
            else:
                os.environ["ICTOOL"] = old

    def test_git_blob_reads_pre_rebrand(self):
        data = tools.git_blob(paths.PRE_REBRAND_REV, "images/gnome/16.png")
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: errors such as `ModuleNotFoundError: No module named 'iconlib'`.

- [ ] **Step 5: Implement `iconlib`**

`images/tools/iconlib/__init__.py`:

```python
"""Rustty icon generator library. See images/tools/README.md."""
```

`images/tools/iconlib/paths.py`:

```python
"""Every input and output path the icon generator touches."""
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
IMAGES = REPO / "images"

FERRIS_SVG = IMAGES / "ferris" / "rustacean-flat-happy.svg"

ICON_DIR = IMAGES / "Rustty.icon"
ICON_JSON = ICON_DIR / "icon.json"
ICON_MASCOT = ICON_DIR / "Assets" / "Rustty.png"

XCASSETS = REPO / "macos" / "Assets.xcassets"
APPICON_IMAGESET = XCASSETS / "AppIconImage.imageset"
APPICON_FILES = {
    256: "macOS-AppIcon-256px-128pt@2x.png",
    512: "macOS-AppIcon-512px.png",
    1024: "macOS-AppIcon-1024px.png",
}
CUSTOM_DIR = XCASSETS / "Custom Icon"
CUSTOM_MASCOT = CUSTOM_DIR / "CustomIconGhost.imageset" / "ghosty.png"

GNOME_DIR = IMAGES / "gnome"
GNOME_SIZES = (16, 32, 64, 128, 256, 512, 1024, 2048)
APPRT_GNOME_DIR = REPO / "crates" / "rustty-apprt" / "assets" / "images" / "gnome"

ICONS_DIR = IMAGES / "icons"
ICONS_MAP = {
    "icon_16.png": 16, "icon_16@2x.png": 32,
    "icon_32.png": 32, "icon_32@2x.png": 64,
    "icon_128.png": 128, "icon_128@2x.png": 256,
    "icon_256.png": 256, "icon_256@2x.png": 512,
    "icon_512.png": 512, "icon_512@2x.png": 1024,
    "icon_1024.png": 1024, "icon_1024@2x.png": 2048,
}

WIN_ICO = REPO / "dist" / "windows" / "rustty.ico"
ICO_SIZES = (16, 32, 48, 256)
FAVICON = REPO / "dist" / "doxygen" / "favicon.png"
FAVICON_SIZE = 32

ALT_NAMES = ("Blueprint", "Chalkboard", "Glass", "Holographic", "Microchip", "Paper", "Retro", "Xray")

PRE_REBRAND_REV = "401b95b0e"
REVIEW_DIR = REPO / "target" / "icon-review"


def alt_output(name: str) -> Path:
    return XCASSETS / "Alternate Icons" / f"{name}Image.imageset" / "macOS-AppIcon-1024px.png"


def plate_path(name: str) -> Path:
    return IMAGES / "alternates" / name.lower() / "plate.png"


def rel(path: Path) -> str:
    return Path(path).resolve().relative_to(REPO).as_posix()
```

`images/tools/iconlib/tools.py`:

```python
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
```

`images/tools/iconlib/raster.py`:

```python
"""Shared raster helpers: deterministic PNG I/O, colour, the Ferris raster, placement."""
from __future__ import annotations

import io
import tempfile
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageCms, ImageFilter

from . import paths, tools

FERRIS_MASTER_WIDTH = 2400
FERRIS_WIDTH_RATIO = 0.68   # spec D2
FERRIS_LIFT_RATIO = 0.012   # spec D2


def save_png(img: Image.Image, path: Path) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    clean = img.copy()
    clean.info = {}
    clean.save(path, format="PNG", optimize=False, compress_level=9)


def to_srgb(img: Image.Image) -> Image.Image:
    icc = img.info.get("icc_profile")
    rgba = img.convert("RGBA")
    if icc:
        src = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        dst = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB"))
        rgb = ImageCms.profileToProfile(rgba.convert("RGB"), src, dst,
                                        renderingIntent=ImageCms.Intent.RELATIVE_COLORIMETRIC,
                                        outputMode="RGB")
        rgb.putalpha(rgba.getchannel("A"))
        rgba = rgb
    rgba.info = {}
    return rgba


@lru_cache(maxsize=1)
def _ferris_master() -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "ferris.png"
        tools.run([tools.tool_path("resvg"), "-w", str(FERRIS_MASTER_WIDTH), paths.FERRIS_SVG, out])
        img = Image.open(out).convert("RGBA")
        img.load()
    img = img.crop(img.getbbox())
    img.info = {}
    return img


def ferris_aspect() -> float:
    m = _ferris_master()
    return m.width / m.height


def ferris_at(width: int) -> Image.Image:
    return _ferris_master().resize((width, round(width / ferris_aspect())), Image.LANCZOS)


@dataclass(frozen=True)
class Masks:
    alpha: np.ndarray  # float 0..1
    body: np.ndarray   # bool: orange shell, legs, claws
    shade: np.ndarray  # bool: #A52B00 underside
    dark: np.ndarray   # bool: eyes, mouth
    white: np.ndarray  # bool: eye highlights


def classify(img: Image.Image) -> Masks:
    a = np.asarray(img.convert("RGBA")).astype(np.float64)
    r, g, b, al = a[..., 0], a[..., 1], a[..., 2], a[..., 3] / 255.0
    solid = al > 0.5
    dark = (r + g + b < 150) & solid
    white = (r > 200) & (g > 200) & (b > 200) & solid
    shade = (r > 110) & (r < 200) & (g < 70) & ~dark & solid
    body = (al > 0) & ~dark & ~white & ~shade
    return Masks(al, body, shade, dark, white)


def ferris_box(screen_bbox, aspect: float | None = None):
    x0, y0, x1, y1 = screen_bbox
    sw, sh = x1 - x0, y1 - y0
    w = round(FERRIS_WIDTH_RATIO * sw)
    h = round(w / (aspect or ferris_aspect()))
    cx = (x0 + x1) / 2
    cy = (y0 + y1) / 2 - FERRIS_LIFT_RATIO * sh
    return (round(cx - w / 2), round(cy - h / 2), w, h)


def opaque_bbox(img: Image.Image, thr: int = 128):
    a = np.asarray(img.convert("RGBA"))[..., 3] > thr
    ys, xs = np.nonzero(a)
    return (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)


def iou(a: np.ndarray, b: np.ndarray) -> float:
    union = np.logical_or(a, b).sum()
    return float(np.logical_and(a, b).sum() / union) if union else 1.0


def glow(alpha_l: Image.Image, rgb, radius: float, strength: float) -> Image.Image:
    g = Image.new("RGBA", alpha_l.size, tuple(rgb) + (0,))
    g.putalpha(alpha_l.filter(ImageFilter.GaussianBlur(radius)).point(lambda v: min(255, int(v * strength))))
    return g
```

`images/tools/build_icons.py`:

```python
#!/usr/bin/env python3
"""Rebuild Rustty's icon assets from committed sources. See images/tools/README.md."""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from iconlib import paths, tools  # noqa: E402


def cmd_paths(args) -> int:
    print(f"repo: {paths.REPO}")
    for p in (paths.FERRIS_SVG, paths.ICON_JSON):
        print(f"{'ok ' if p.is_file() else 'MISSING '}{paths.rel(p)}")
    return 0 if paths.FERRIS_SVG.is_file() and paths.ICON_JSON.is_file() else 1


COMMANDS = {"paths": cmd_paths}
COMMAND_TOOLS = {"paths": []}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="build_icons.py", description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("paths", help="print the repo root and check inputs")
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: all of the tests above pass.

- [ ] **Step 7: Commit**

```bash
git add images/ferris images/tools
git commit -m "icons: vendor CC0 Ferris and add the icon generator foundation

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Official icon mascot layer and the icon.json edit

**Files:**

- Create: `images/tools/iconlib/mascot.py`, `images/tools/iconlib/official.py`
- Modify: `images/tools/build_icons.py` (register `mascot`, `official`)
- Modify (generated): `images/Rustty.icon/Assets/Rustty.png`, `images/Rustty.icon/icon.json`
- Test: `images/tools/tests/test_mascot.py`, `images/tools/tests/test_official.py`

**Interfaces:**

- Consumes: `raster.ferris_box`, `raster.ferris_at`, `raster.glow`, `raster.save_png`, `paths.ICON_MASCOT`, `paths.ICON_JSON`.
- Produces:
  - `mascot.ICON_SCREEN_BBOX = (0, 0, 825, 825)`, `mascot.ICON_PAD = 40`, `mascot.icon_mascot() -> Image`, `mascot.run_mascot(args) -> int` (Task 4 extends it).
  - `official.ORANGE_FILL`, `official.update_icon_json(path) -> bool`, `official.run(args) -> int`.

- [ ] **Step 1: Write the failing tests**

`images/tools/tests/test_mascot.py`:

```python
import unittest

from iconlib import mascot, raster


class IconMascotTest(unittest.TestCase):
    def test_size_follows_d2(self):
        _, _, w, h = raster.ferris_box(mascot.ICON_SCREEN_BBOX)
        self.assertEqual(w, 561)  # round(0.68 * 825)
        img = mascot.icon_mascot()
        self.assertEqual(img.size, (w + 2 * mascot.ICON_PAD, h + 2 * mascot.ICON_PAD))
        self.assertEqual(img.mode, "RGBA")

    def test_ferris_centred_in_layer(self):
        img = mascot.icon_mascot()
        x0, y0, x1, y1 = raster.opaque_bbox(img, thr=250)
        self.assertLessEqual(abs((x0 + x1) / 2 - img.width / 2), 2)
        self.assertLessEqual(abs((y0 + y1) / 2 - img.height / 2), 2)

    def test_stock_colours_survive(self):
        img = mascot.icon_mascot()
        px = img.getpixel((img.width // 2, mascot.ICON_PAD + img.height // 5))
        self.assertTrue(abs(px[0] - 247) <= 3 and abs(px[1] - 76) <= 3 and px[2] <= 3, px)


if __name__ == "__main__":
    unittest.main()
```

`images/tools/tests/test_official.py`:

```python
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from iconlib import official, paths, tools


class IconJsonTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / "icon.json"
        self.path.write_bytes(tools.git_blob(paths.PRE_REBRAND_REV, "images/Rustty.icon/icon.json"))
        self.before = json.loads(self.path.read_text())

    def tearDown(self):
        self.tmp.cleanup()

    def test_only_mascot_fields_change(self):
        self.assertTrue(official.update_icon_json(self.path))
        after = json.loads(self.path.read_text())
        mascot_layers = [l for g in after["groups"] for l in g["layers"] if l.get("image-name") == "Rustty.png"]
        self.assertEqual(len(mascot_layers), 2)
        for l in mascot_layers:
            self.assertEqual(l["position"], {"scale": 1, "translation-in-points": [0, -10]})
        blur = [l for l in mascot_layers if l["name"] == "RusttyBlur"][0]
        self.assertEqual(blur["fill"], {"solid": official.ORANGE_FILL})
        for g_before, g_after in zip(self.before["groups"], after["groups"]):
            for l_before, l_after in zip(g_before["layers"], g_after["layers"]):
                if l_before.get("image-name") != "Rustty.png":
                    self.assertEqual(l_before, l_after)
        self.assertEqual({k: v for k, v in self.before.items() if k != "groups"},
                         {k: v for k, v in after.items() if k != "groups"})

    def test_idempotent_and_format_preserving(self):
        official.update_icon_json(self.path)
        once = self.path.read_bytes()
        self.assertFalse(official.update_icon_json(self.path))
        self.assertEqual(self.path.read_bytes(), once)
        self.assertFalse(once.endswith(b"\n"))
        self.assertIn(b'"name" : "RusttyBlur"', once)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `ModuleNotFoundError: No module named 'iconlib.mascot'` (and `iconlib.official`).

- [ ] **Step 3: Implement**

`images/tools/iconlib/mascot.py`:

```python
"""Mascot layers: the official Icon Composer layer and the custom-style layer."""
from PIL import Image

from . import paths, raster

# Screen.png is 825x825 pt (72 dpi, so 1 px = 1 pt). The mascot layer is centred
# on it with icon.json translation [0, -10]: -10 pt = 1.2 % of 825 (spec D2).
ICON_SCREEN_BBOX = (0, 0, 825, 825)
ICON_PAD = 40
ICON_GLOW_RGB = (255, 90, 0)
ICON_GLOW_RADIUS = 14
ICON_GLOW_STRENGTH = 0.9


def icon_mascot() -> Image.Image:
    _, _, w, _ = raster.ferris_box(ICON_SCREEN_BBOX)
    f = raster.ferris_at(w)
    canvas = Image.new("RGBA", (f.width + 2 * ICON_PAD, f.height + 2 * ICON_PAD), (0, 0, 0, 0))
    alpha = Image.new("L", canvas.size, 0)
    alpha.paste(f.getchannel("A"), (ICON_PAD, ICON_PAD))
    canvas.alpha_composite(raster.glow(alpha, ICON_GLOW_RGB, ICON_GLOW_RADIUS, ICON_GLOW_STRENGTH))
    canvas.alpha_composite(f, (ICON_PAD, ICON_PAD))
    return canvas


def run_mascot(args) -> int:
    raster.save_png(icon_mascot(), paths.ICON_MASCOT)
    print(f"wrote {paths.rel(paths.ICON_MASCOT)}")
    return 0
```

`images/tools/iconlib/official.py`:

```python
"""Edit Rustty.icon/icon.json: centre the mascot layers and tint the glass orange."""
import json
from pathlib import Path

from . import paths

ORANGE_FILL = "extended-srgb:0.96863,0.29804,0.00000,1.00000"  # #F74C00
MASCOT_POSITION = {"scale": 1, "translation-in-points": [0, -10]}


def _dump(obj) -> str:
    # Icon Composer's own format: 2-space indent, "key" : value, no trailing newline.
    return json.dumps(obj, indent=2, separators=(",", " : "))


def update_icon_json(path: Path = paths.ICON_JSON) -> bool:
    raw = Path(path).read_text()
    doc = json.loads(raw)
    for group in doc["groups"]:
        for layer in group["layers"]:
            if layer.get("image-name") != "Rustty.png":
                continue
            layer["position"] = dict(MASCOT_POSITION)
            if layer.get("name") == "RusttyBlur":
                layer["fill"] = {"solid": ORANGE_FILL}
    out = _dump(doc)
    if out == raw:
        return False
    Path(path).write_text(out)
    return True


def run(args) -> int:
    changed = update_icon_json(paths.ICON_JSON)
    print(f"{'updated' if changed else 'unchanged'} {paths.rel(paths.ICON_JSON)}")
    return 0
```

In `images/tools/build_icons.py`, register the new commands. Put the imports after `from iconlib import paths, tools`:

```python
from iconlib import mascot, official  # noqa: E402
```

and extend the tables and the parser:

```python
COMMANDS = {"paths": cmd_paths, "mascot": mascot.run_mascot, "official": official.run}
COMMAND_TOOLS = {"paths": [], "mascot": ["resvg"], "official": []}
```

```python
    sub.add_parser("mascot", help="write the Ferris mascot layers")
    sub.add_parser("official", help="centre the mascot and tint the glass in Rustty.icon/icon.json")
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: PASS.

- [ ] **Step 5: Generate the assets and sanity-render**

```bash
python3 -I images/tools/build_icons.py mascot
python3 -I images/tools/build_icons.py official
git diff --stat
"/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool" images/Rustty.icon --export-image --output-file target/icon-review/task2-official.png --platform macOS --rendition Default --width 1024 --height 1024 --scale 1
```

Expected: `Rustty.png` and `icon.json` changed. Open `target/icon-review/task2-official.png` with the Read tool: Ferris should be centred, a little above the screen centre, with a glow on blue. Create `target/icon-review` first if `ictool` complains that it is missing.

- [ ] **Step 6: Commit**

```bash
git add images/tools images/Rustty.icon
git commit -m "icons: Ferris mascot layer in Rustty.icon, centred, glass tinted #F74C00

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Flat exports, framing check, and review gate 1 (official icon)

**Files:**

- Create: `images/tools/iconlib/flats.py`, `images/tools/iconlib/review.py`
- Modify: `images/tools/build_icons.py` (register `flats`, `review`)
- Modify (generated): `macos/Assets.xcassets/AppIconImage.imageset/*.png` (3), `images/gnome/*.png` (8), `crates/rustty-apprt/assets/images/gnome/*.png` (8), `images/icons/*.png` (12), `dist/windows/rustty.ico`, `dist/doxygen/favicon.png`
- Modify: `crates/rustty-apprt/src/gtk/gresource.rs:45-85` (W14-01 note and sha256 table)
- Test: `images/tools/tests/test_flats.py`

**Interfaces:**

- Consumes: `raster.to_srgb`, `raster.opaque_bbox`, `raster.iou`, `raster.glow`, `raster.save_png`, `tools.tool_path`, `tools.run`, `tools.git_blob`, and the size tables in `paths`.
- Produces:
  - `flats.MASTER = 2048`, `flats.REFERENCE_BBOX_1024 = (96, 100, 920, 924)`, `flats.render_icon(size) -> Image`, `flats.fit_to_frame(render, size=MASTER) -> Image`, `flats.downscale(master, size) -> Image`, `flats.FramingResult`, `flats.framing_check(new1024, ref1024) -> FramingResult`, `flats.reference_1024() -> Image`, `flats.write_flats(master)`, `flats.build_master() -> Image`, `flats.run(args) -> int`.
  - `review.run(args) -> int` (subcommands `official`, plus `custom` and `alternates` in later tasks), `review.sheet_sizes(icon) -> Image`.

- [ ] **Step 1: Write the failing tests**

`images/tools/tests/test_flats.py`:

```python
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

from iconlib import flats, paths, tools

BUILD_ICONS = Path(__file__).resolve().parents[1] / "build_icons.py"


class FramingTest(unittest.TestCase):
    def _square(self, box):
        img = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
        img.paste((200, 200, 200, 255), box)
        return img

    def test_reference_passes_against_itself(self):
        ref = flats.reference_1024()
        r = flats.framing_check(ref, ref)
        self.assertTrue(r.bbox_ok and r.ok)
        self.assertEqual(r.iou, 1.0)

    def test_shifted_frame_fails(self):
        ref = self._square(flats.REFERENCE_BBOX_1024)
        x0, y0, x1, y1 = flats.REFERENCE_BBOX_1024
        moved = self._square((x0 + 6, y0, x1 + 6, y1))
        r = flats.framing_check(moved, ref)
        self.assertFalse(r.bbox_ok)
        self.assertFalse(r.ok)

    def test_fit_to_frame_lands_on_reference_bbox(self):
        render = self._square((10, 10, 1014, 1014)).resize((2048, 2048))
        master = flats.fit_to_frame(render)
        small = flats.downscale(master, 1024)
        self.assertTrue(flats.framing_check(small, self._square(flats.REFERENCE_BBOX_1024)).bbox_ok)


class RenderTest(unittest.TestCase):
    def test_render_is_srgb_untagged(self):
        img = flats.render_icon(256)
        self.assertEqual(img.mode, "RGBA")
        self.assertNotIn("icc_profile", img.info)
        with tempfile.TemporaryDirectory() as tmp:
            raw_png = Path(tmp) / "raw.png"
            tools.run([tools.tool_path("ictool"), paths.ICON_DIR, "--export-image", "--output-file", raw_png,
                       "--platform", "macOS", "--rendition", "Default", "--width", "256", "--height", "256",
                       "--scale", "1"])
            raw = np.asarray(Image.open(raw_png).convert("RGBA")).astype(int)
        # P3 -> sRGB must actually move colours (saturated blue screen + orange Ferris).
        self.assertGreater(np.abs(np.asarray(img).astype(int) - raw)[..., :3].max(), 3)


class IcoTest(unittest.TestCase):
    def test_ico_entries(self):
        out = subprocess.run([tools.tool_path("icotool"), "-l", str(paths.WIN_ICO)],
                             capture_output=True, text=True, check=True).stdout
        widths = sorted(int(l.split("--width=")[1].split()[0]) for l in out.splitlines() if "--width=" in l)
        self.assertEqual(widths, [16, 32, 48, 256])
        self.assertTrue(all("--bit-depth=32" in l for l in out.splitlines() if l.strip()))


class MissingToolTest(unittest.TestCase):
    def test_flats_fails_before_writing(self):
        watched = [paths.GNOME_DIR / "1024.png", paths.WIN_ICO, paths.FAVICON]
        before = [p.stat().st_mtime_ns for p in watched]
        env = dict(os.environ, ICTOOL="/nonexistent/ictool")
        r = subprocess.run([sys.executable, "-I", str(BUILD_ICONS), "flats"], env=env,
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)
        self.assertIn("ictool", r.stderr)
        self.assertEqual(before, [p.stat().st_mtime_ns for p in watched])


class OutputsTest(unittest.TestCase):
    def test_sizes_and_mirror(self):
        for size, name in paths.APPICON_FILES.items():
            self.assertEqual(Image.open(paths.APPICON_IMAGESET / name).size, (size, size))
        for size in paths.GNOME_SIZES:
            a = (paths.GNOME_DIR / f"{size}.png").read_bytes()
            self.assertEqual(a, (paths.APPRT_GNOME_DIR / f"{size}.png").read_bytes())
            self.assertEqual(Image.open(paths.GNOME_DIR / f"{size}.png").size, (size, size))
        for name, size in paths.ICONS_MAP.items():
            self.assertEqual(Image.open(paths.ICONS_DIR / name).size, (size, size))
        self.assertEqual(Image.open(paths.FAVICON).size, (32, 32))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `ModuleNotFoundError: No module named 'iconlib.flats'`. `IcoTest` and `OutputsTest` would pass on the old files; that is fine, because they guard the format of the files this task rewrites.

- [ ] **Step 3: Implement `flats.py`**

```python
"""Flat exports of the official icon: ictool render -> framed 2048 master -> every size."""
from __future__ import annotations

import io
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image

from . import paths, raster, tools

MASTER = 2048
REFERENCE_BBOX_1024 = (96, 100, 920, 924)  # half-open; spec: 96,100-919,923
BBOX_TOLERANCE = 2
IOU_MIN = 0.98
SHADOW_RADIUS = 10    # at 1024; scaled with size
SHADOW_OFFSET_Y = 8   # at 1024
SHADOW_OPACITY = 0.30


def render_icon(size: int) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "icon.png"
        tools.run([tools.tool_path("ictool"), paths.ICON_DIR, "--export-image", "--output-file", out,
                   "--platform", "macOS", "--rendition", "Default",
                   "--width", size, "--height", size, "--scale", 1])
        img = Image.open(out)
        img.load()
    return raster.to_srgb(img)


def fit_to_frame(render: Image.Image, size: int = MASTER) -> Image.Image:
    k = size / 1024
    x0, y0, x1, y1 = (round(v * k) for v in REFERENCE_BBOX_1024)
    shape = render.crop(raster.opaque_bbox(render)).resize((x1 - x0, y1 - y0), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow = Image.new("L", (size, size), 0)
    shadow.paste(shape.getchannel("A"), (x0, y0 + round(SHADOW_OFFSET_Y * k)))
    canvas.alpha_composite(raster.glow(shadow, (0, 0, 0), SHADOW_RADIUS * k, SHADOW_OPACITY))
    canvas.alpha_composite(shape, (x0, y0))
    return canvas


def downscale(master: Image.Image, size: int) -> Image.Image:
    return master.copy() if master.width == size else master.resize((size, size), Image.LANCZOS)


@dataclass(frozen=True)
class FramingResult:
    bbox: tuple
    bbox_ok: bool
    iou: float

    @property
    def ok(self) -> bool:
        return self.bbox_ok and self.iou >= IOU_MIN


def _mask(img: Image.Image) -> np.ndarray:
    return np.asarray(img.convert("RGBA"))[..., 3] > 128


def framing_check(new1024: Image.Image, ref1024: Image.Image) -> FramingResult:
    bbox = raster.opaque_bbox(new1024)
    bbox_ok = all(abs(a - b) <= BBOX_TOLERANCE for a, b in zip(bbox, REFERENCE_BBOX_1024))
    return FramingResult(bbox, bbox_ok, raster.iou(_mask(new1024), _mask(ref1024)))


def reference_1024() -> Image.Image:
    img = Image.open(io.BytesIO(tools.git_blob(paths.PRE_REBRAND_REV, "images/gnome/1024.png")))
    img.load()
    return img.convert("RGBA")


def write_flats(master: Image.Image) -> None:
    for size, name in paths.APPICON_FILES.items():
        raster.save_png(downscale(master, size), paths.APPICON_IMAGESET / name)
    for size in paths.GNOME_SIZES:
        raster.save_png(downscale(master, size), paths.GNOME_DIR / f"{size}.png")
        shutil.copyfile(paths.GNOME_DIR / f"{size}.png", paths.APPRT_GNOME_DIR / f"{size}.png")
    for name, size in paths.ICONS_MAP.items():
        raster.save_png(downscale(master, size), paths.ICONS_DIR / name)
    raster.save_png(downscale(master, paths.FAVICON_SIZE), paths.FAVICON)
    with tempfile.TemporaryDirectory() as tmp:
        parts = []
        for size in paths.ICO_SIZES:
            p = Path(tmp) / f"{size}.png"
            raster.save_png(downscale(master, size), p)
            parts.append(p)
        tools.run([tools.tool_path("icotool"), "-c", "-o", paths.WIN_ICO, *parts])


def build_master() -> Image.Image:
    return fit_to_frame(render_icon(MASTER))


def run(args) -> int:
    master = build_master()
    check = framing_check(downscale(master, 1024), reference_1024())
    print(f"framing: bbox={check.bbox} ref={REFERENCE_BBOX_1024} bbox_ok={check.bbox_ok} "
          f"iou={check.iou:.4f} (min {IOU_MIN})")
    if not check.ok:
        print("error: framing check failed; nothing written", file=sys.stderr)
        return 1
    write_flats(master)
    print("wrote AppIconImage, images/gnome (+apprt mirror), images/icons, rustty.ico, favicon.png")
    return 0
```

- [ ] **Step 4: Implement `review.py` (official sheet)**

```python
"""Review sheets for the user gates, written to target/icon-review/ (git-ignored)."""
from __future__ import annotations

import io
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

from . import paths, raster, tools

LIGHT = (242, 242, 242, 255)
DARK = (30, 30, 30, 255)


def sheet_sizes(icon: Image.Image, sizes=(256, 64, 32, 16)) -> Image.Image:
    w = sum(sizes) + 12 * (len(sizes) + 1)
    out = Image.new("RGBA", (w, 2 * (max(sizes) + 24)), (0, 0, 0, 0))
    for row, bg in enumerate((LIGHT, DARK)):
        y0 = row * (max(sizes) + 24)
        out.paste(bg, (0, y0, w, y0 + max(sizes) + 24))
        x = 12
        for s in sizes:
            out.alpha_composite(icon.resize((s, s), Image.LANCZOS), (x, y0 + 12 + max(sizes) - s))
            x += s + 12
    return out


def _actool_256() -> Image.Image:
    """The dock icon as Xcode compiles it (actool), for comparison with the ictool flats."""
    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        subprocess.run(["xcrun", "actool", str(paths.ICON_DIR), "--compile", str(t), "--app-icon", "Rustty",
                        "--platform", "macosx", "--minimum-deployment-target", "13.0",
                        "--output-partial-info-plist", str(t / "info.plist")], check=True, capture_output=True)
        subprocess.run(["iconutil", "-c", "iconset", str(t / "Rustty.icns"), "-o", str(t / "x.iconset")],
                       check=True, capture_output=True)
        img = Image.open(t / "x.iconset" / "icon_128x128@2x.png").convert("RGBA")
        img.load()
    return img


def official() -> list[Path]:
    from . import flats
    master = flats.build_master()
    flat_1024 = flats.downscale(master, 1024)
    before = Image.open(io.BytesIO(tools.git_blob(paths.PRE_REBRAND_REV, "images/gnome/1024.png"))).convert("RGBA")
    pair = Image.new("RGBA", (1024 + 256 + 24, 1024), LIGHT)
    pair.alpha_composite(flat_1024, (0, 0))
    pair.alpha_composite(_actool_256(), (1024 + 12, 0))
    outs = {
        "official-1024-vs-actool.png": pair,
        "official-sizes.png": sheet_sizes(flat_1024),
        "official-before-sizes.png": sheet_sizes(before),
    }
    written = []
    for name, img in outs.items():
        raster.save_png(img, paths.REVIEW_DIR / name)
        written.append(paths.REVIEW_DIR / name)
    return written


SHEETS = {"official": lambda args: official()}


def run(args) -> int:
    for p in SHEETS[args.what](args):
        print(f"wrote {paths.rel(p)}")
    return 0
```

Register the commands in `build_icons.py`:

```python
from iconlib import flats, mascot, official, review  # noqa: E402
```

```python
COMMANDS = {"paths": cmd_paths, "mascot": mascot.run_mascot, "official": official.run,
            "flats": flats.run, "review": review.run}
COMMAND_TOOLS = {"paths": [], "mascot": ["resvg"], "official": [],
                 "flats": ["ictool", "icotool"], "review": ["resvg", "ictool"]}
```

```python
    sub.add_parser("flats", help="render Rustty.icon and write every flat export")
    rv = sub.add_parser("review", help="write review sheets to target/icon-review/")
    rv.add_argument("what", choices=["official", "custom", "alternates"])
    rv.add_argument("names", nargs="*")
```

(`custom` and `alternates` become live in Tasks 4 and 6. Until then, selecting them raises `KeyError`, which is acceptable for an internal review command.)

- [ ] **Step 5: Generate the flats**

```bash
python3 -I images/tools/build_icons.py flats
```

Expected: a `framing: … bbox_ok=True iou=0.98xx…` line, then `wrote …`.

If `bbox_ok=False`, print `raster.opaque_bbox(flats.render_icon(2048))` and check whether `ictool` added an outer shadow that enlarged the crop. If so, crop with `thr=200` in `fit_to_frame`. If the IoU is below 0.98 while the bbox is OK, the corner radius differs: compare the two masks' corners in a 64 px crop and report to the user. **Do not lower `IOU_MIN`.**

- [ ] **Step 6: Run all tests**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: PASS, including `MissingToolTest` and `RenderTest`.

- [ ] **Step 7: Refresh the GTK icon hash table**

Print the new table lines:

```bash
python3 -I -c "
import hashlib
for s in (16,32,64,128,256,512,1024,2048):
    p=f'crates/rustty-apprt/assets/images/gnome/{s}.png'
    print(f\"//! {('images/gnome/'+str(s)+'.png'):<39}{hashlib.sha256(open(p,'rb').read()).hexdigest()}  (Ferris art)\")"
```

In `crates/rustty-apprt/src/gtk/gresource.rs`, replace the eight `//! images/gnome/*.png … (rustty art)` lines (78–85) with this output. In the W14-01 paragraph (lines 45–47), change `byte-copies of the committed RUSTTY artwork at` to `byte-copies of the committed Rustty (Ferris) artwork at`. Then run:

```bash
cargo test -p rustty-apprt --test gtk_icon_identity
```

Expected: `test gnome_icons_are_rustty_artwork ... ok`.

- [ ] **Step 8: Commit**

```bash
git add images/tools images/gnome images/icons crates/rustty-apprt/assets/images/gnome crates/rustty-apprt/src/gtk/gresource.rs macos/Assets.xcassets/AppIconImage.imageset dist/windows/rustty.ico dist/doxygen/favicon.png
git commit -m "icons: regenerate every flat export of the official icon from Rustty.icon

ictool render (P3 -> sRGB), fitted to the pre-rebrand frame geometry
(bbox 96,100-919,923, IoU >= 0.98), downscaled from a 2048 master.
Refreshes the gresource hash table (W14-01 is now true: Ferris art).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 9: REVIEW GATE 1 (official icon)**

```bash
python3 -I images/tools/build_icons.py review official
```

Follow the review-gate procedure with `official-1024-vs-actool.png`, `official-sizes.png` and `official-before-sizes.png`. Ask:

1. Is the 1024 icon right (Ferris size, lift, glow, orange glass)?
2. Is it readable at 32 and 16 px?
3. Is the ictool flat close enough to the actool dock render?

Tunables are `mascot.ICON_GLOW_*`, `flats.SHADOW_*`, and (only with user approval, since it changes D2) the ratios in `raster`. After each change, rerun `mascot`, `official`, `flats` and `review official`, commit as `icons: tune … (gate 1)`, and re-present. **Stop here until the user approves.**

---

### Task 4: Custom-style grey Ferris and review gate 4

**Files:**

- Modify: `images/tools/iconlib/mascot.py` (add `custom_mascot`; `run_mascot` writes both layers)
- Create: `images/tools/iconlib/preview.py`
- Modify: `images/tools/iconlib/review.py` (add the `custom` sheet)
- Modify (generated): `macos/Assets.xcassets/Custom Icon/CustomIconGhost.imageset/ghosty.png`
- Test: `images/tools/tests/test_mascot.py` (extend), `images/tools/tests/test_preview.py`

**Interfaces:**

- Consumes: `raster.ferris_box`, `raster.ferris_at`, `raster.classify`, `raster.glow`, `paths.CUSTOM_DIR`, `paths.CUSTOM_MASCOT`.
- Produces:
  - `mascot.CUSTOM_SCREEN_BBOX = (174, 174, 849, 849)`, `mascot.CUSTOM_LEVELS`, `mascot.custom_mascot() -> Image` (mode `LA`, 1024²).
  - `preview.blend_normal/blend_color/blend_overlay(cb, cs) -> ndarray`, `preview.composite(backdrop, source, mode) -> ndarray`, `preview.custom_style(frame, screen_colors, ghost_color) -> Image`.

- [ ] **Step 1: Write the failing tests**

Append to `images/tools/tests/test_mascot.py`, before `if __name__`:

```python
import numpy as np


class CustomMascotTest(unittest.TestCase):
    def test_mode_size_and_neutral(self):
        img = mascot.custom_mascot()
        self.assertEqual(img.mode, "LA")   # same as the pre-rebrand ghosty.png
        self.assertEqual(img.size, (1024, 1024))

    def test_placement_follows_d2(self):
        img = mascot.custom_mascot().convert("RGBA")
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        bx0, by0, bx1, by1 = raster.opaque_bbox(img, thr=250)
        self.assertLessEqual(abs((bx0 + bx1) / 2 - (x + w / 2)), 3)
        self.assertLessEqual(abs((by0 + by1) / 2 - (y + h / 2)), 3)
        self.assertLessEqual(abs((bx1 - bx0) - w), 4)

    def test_body_luminance(self):
        img = mascot.custom_mascot()
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        m = raster.classify(raster.ferris_at(w))
        lum = np.asarray(img)[y:y + h, x:x + w, 0].astype(int)
        self.assertTrue(225 <= lum[m.body].mean() <= 245)
        self.assertLess(lum[m.dark].mean(), 60)
```

`images/tools/tests/test_preview.py`:

```python
import unittest

import numpy as np

from iconlib import mascot, paths, preview


class BlendTest(unittest.TestCase):
    def test_color_blend_keeps_backdrop_luminance(self):
        rng = np.random.default_rng(1)
        cb, cs = rng.random((64, 3)), rng.random((64, 3))
        lum = lambda c: 0.3 * c[..., 0] + 0.59 * c[..., 1] + 0.11 * c[..., 2]
        np.testing.assert_allclose(lum(preview.blend_color(cb, cs)), lum(cb), atol=1e-9)

    def test_overlay_identity_on_mid_grey(self):
        cs = np.array([[0.2, 0.7, 0.9]])
        np.testing.assert_allclose(preview.blend_overlay(np.full((1, 3), 0.5), cs), cs)

    def test_opaque_normal_composite_is_source(self):
        b = np.array([[[0.1, 0.2, 0.3, 1.0]]])
        s = np.array([[[0.9, 0.8, 0.7, 1.0]]])
        np.testing.assert_allclose(preview.composite(b, s, preview.blend_normal), s)


class CustomStyleTest(unittest.TestCase):
    def test_orange_tint_makes_an_orange_crab(self):
        img = preview.custom_style("aluminum", ["#0018B4", "#2A4BFF"], "#F74C00")
        from iconlib import raster
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        r, g, b, _ = img.getpixel((x + w // 2, y + h // 4))
        self.assertGreater(r, g + 40)
        self.assertGreater(r, b + 40)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `AttributeError: module 'iconlib.mascot' has no attribute 'custom_mascot'`, and `ModuleNotFoundError: iconlib.preview`.

- [ ] **Step 3: Implement `custom_mascot`**

Append to `images/tools/iconlib/mascot.py`, and replace `run_mascot`:

```python
import numpy as np

# The screen-mask layer's opaque bbox is 174..848 inclusive (half-open 849).
CUSTOM_SCREEN_BBOX = (174, 174, 849, 849)
CUSTOM_LEVELS = {"body": 235, "shade": 170, "dark": 25, "white": 255}
CUSTOM_GLOW_RADIUS = 14
CUSTOM_GLOW_STRENGTH = 0.5


def custom_mascot() -> Image.Image:
    """Neutral grey Ferris for ColorizedRusttyIcon: the user's tint supplies the hue (spec D5)."""
    x, y, w, h = raster.ferris_box(CUSTOM_SCREEN_BBOX)
    f = raster.ferris_at(w)
    m = raster.classify(f)
    lum = np.zeros((h, w))
    for name, level in CUSTOM_LEVELS.items():
        lum[getattr(m, name)] = level
    grey = np.dstack([lum, lum, lum, m.alpha * 255]).round().astype(np.uint8)
    canvas = Image.new("RGBA", (1024, 1024), (255, 255, 255, 0))
    alpha = Image.new("L", canvas.size, 0)
    alpha.paste(f.getchannel("A"), (x, y))
    canvas.alpha_composite(raster.glow(alpha, (255, 255, 255), CUSTOM_GLOW_RADIUS, CUSTOM_GLOW_STRENGTH))
    canvas.alpha_composite(Image.fromarray(grey, "RGBA"), (x, y))
    return canvas.convert("LA")


def run_mascot(args) -> int:
    raster.save_png(icon_mascot(), paths.ICON_MASCOT)
    raster.save_png(custom_mascot(), paths.CUSTOM_MASCOT)
    print(f"wrote {paths.rel(paths.ICON_MASCOT)}, {paths.rel(paths.CUSTOM_MASCOT)}")
    return 0
```

Move the `import numpy as np` up to the import block at the top of the module.

- [ ] **Step 4: Implement `preview.py`**

This is a faithful replica of `ColorizedRusttyIcon.makeImage` and `NSImage+Extension.swift`.

```python
"""Python replica of ColorizedRusttyIcon (macOS custom-style icon), for review sheets only."""
from __future__ import annotations

import numpy as np
from PIL import Image, ImageColor

from . import paths

FRAMES = {
    "aluminum": "CustomIconBaseAluminum.imageset/base.png",
    "beige": "CustomIconBaseBeige.imageset/beige.png",
    "chrome": "CustomIconBaseChrome.imageset/chrome.png",
    "plastic": "CustomIconBasePlastic.imageset/plastic.png",
}


def _lum(c):
    return 0.3 * c[..., 0] + 0.59 * c[..., 1] + 0.11 * c[..., 2]


def _clip_color(c):
    l = _lum(c)[..., None]
    n = c.min(-1, keepdims=True)
    x = c.max(-1, keepdims=True)
    c = np.where(n < 0, l + (c - l) * l / np.maximum(l - n, 1e-12), c)
    c = np.where(x > 1, l + (c - l) * (1 - l) / np.maximum(x - l, 1e-12), c)
    return c


def blend_normal(cb, cs):
    return cs


def blend_color(cb, cs):  # W3C/CG non-separable "color"
    return _clip_color(cs + (_lum(cb) - _lum(cs))[..., None])


def blend_overlay(cb, cs):  # W3C/CG "overlay" = hard-light with layers swapped
    return np.where(cb <= 0.5, 2 * cs * cb, 1 - 2 * (1 - cs) * (1 - cb))


def composite(backdrop, source, mode):
    cb, ab = backdrop[..., :3], backdrop[..., 3:]
    cs, as_ = source[..., :3], source[..., 3:]
    co = as_ * (1 - ab) * cs + as_ * ab * mode(cb, cs) + (1 - as_) * ab * cb
    ao = as_ + ab * (1 - as_)
    c = np.where(ao > 0, co / np.maximum(ao, 1e-12), 0.0)
    return np.concatenate([c, ao], -1)


def _load(rel: str) -> np.ndarray:
    return np.asarray(Image.open(paths.CUSTOM_DIR / rel).convert("RGBA")).astype(np.float64) / 255


def _rgb(hexstr: str) -> np.ndarray:
    return np.array(ImageColor.getrgb(hexstr)[:3], float) / 255


def _gradient(mask: np.ndarray, colors: list[str]) -> np.ndarray:
    # NSGradient.draw(angle: 90): colors[0] at the bottom, last at the top; then destinationIn mask.
    h = mask.shape[0]
    stops = np.array([_rgb(c) for c in colors])
    t = np.linspace(1, 0, h)  # row 0 is the top
    pos = np.linspace(0, 1, len(stops))
    rgb = np.stack([np.interp(t, pos, stops[:, k]) for k in range(3)], -1)
    out = np.empty_like(mask)
    out[..., :3] = rgb[:, None, :]
    out[..., 3] = mask[..., 3]
    return out


def custom_style(frame: str, screen_colors: list[str], ghost_color: str) -> Image.Image:
    base = _load(FRAMES[frame])
    screen = _load("CustomIconScreen.imageset/screen-dark.png")
    mask = _load("CustomIconScreenMask.imageset/screen-mask.png")
    ghost = np.asarray(Image.open(paths.CUSTOM_MASCOT).convert("RGBA")).astype(np.float64) / 255
    crt = _load("CustomIconCRT.imageset/crt-effect.png")
    gloss = _load("CustomIconGloss.imageset/gloss.png")
    tinted = ghost.copy()
    tinted[..., :3] = _rgb(ghost_color)  # tint(): fill with sourceAtop -> solid colour, ghost alpha
    layers = [(screen, blend_normal), (_gradient(mask, screen_colors), blend_color),
              (ghost, blend_normal), (tinted, blend_color), (crt, blend_overlay), (gloss, blend_normal)]
    acc = base
    for layer, mode in layers:
        acc = composite(acc, layer, mode)
    return Image.fromarray((acc.clip(0, 1) * 255).round().astype(np.uint8), "RGBA")
```

- [ ] **Step 5: Add the `custom` review sheet**

Append to `images/tools/iconlib/review.py`:

```python
TINTS = {"orange": "#F74C00", "white": "#FFFFFF", "green": "#3CD070", "purple": "#8A4FFF"}
SCREEN = ["#0018B4", "#2A4BFF"]


def custom() -> list[Path]:
    from . import preview
    cell = 256
    frames = list(preview.FRAMES)
    grid = Image.new("RGBA", (cell * len(TINTS), cell * len(frames)), LIGHT)
    for r, frame in enumerate(frames):
        for c, tint in enumerate(TINTS.values()):
            icon = preview.custom_style(frame, SCREEN, tint).resize((cell, cell), Image.LANCZOS)
            grid.alpha_composite(icon, (c * cell, r * cell))
    out = paths.REVIEW_DIR / "custom-style-grid.png"
    raster.save_png(grid, out)
    return [out]


SHEETS["custom"] = lambda args: custom()
```

- [ ] **Step 6: Run the tests, generate, and check visually**

```bash
python3 -I -m unittest discover -s images/tools/tests -t images/tools -v
python3 -I images/tools/build_icons.py mascot
python3 -I images/tools/build_icons.py review custom
```

Expected: tests PASS. Open `target/icon-review/custom-style-grid.png` with the Read tool. Rows are aluminum, beige, chrome, plastic; columns are orange, white, green, purple. Ferris should be centred, recoloured by each tint, with visible eyes. `git diff --stat` should show only `ghosty.png` changed under `Custom Icon/`; `Rustty.png` should not change, because `icon_mascot()` is deterministic.

- [ ] **Step 7: Commit**

```bash
git add images/tools macos/Assets.xcassets/Custom\ Icon/CustomIconGhost.imageset/ghosty.png
git commit -m "icons: grey Ferris custom-style layer + Python replica of ColorizedRusttyIcon

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: REVIEW GATE 4 (custom style)**

Present `custom-style-grid.png` through the review-gate procedure. Ask whether the tinted crab reads well on every frame. Tunables are `mascot.CUSTOM_LEVELS` and `mascot.CUSTOM_GLOW_*`. **Stop until the user approves.** (This gate is run early, out of the spec's numbering, because it is independent of the alternates.)

---

### Task 5: Plate drafting and the batch-1 plates

**Files:**

- Create: `images/tools/iconlib/plates.py`
- Modify: `images/tools/build_icons.py` (register `plate`)
- Create (generated, then reviewed): `images/alternates/{chalkboard,paper,blueprint,retro}/plate.png`
- Test: `images/tools/tests/test_plates.py`

**Interfaces:**

- Consumes: `tools.git_blob`, `paths.alt_output`, `paths.plate_path`, `paths.PRE_REBRAND_REV`, `paths.rel`, `raster.save_png`.
- Produces: `plates.GHOST_BOX`, `plates.PASSES: dict[str, list[tuple]]`, `plates.original(name) -> Image`, `plates.apply_pass(img, p) -> Image`, `plates.draft_plate(name) -> Image`, `plates.run(args) -> int`.

A pass is either `("mirror", box)`, which fills `box` with the horizontally mirrored image (the alternates' plates are left/right symmetric outside the paper curl), or `("shift", box, dx, dy)`, which fills `box` with the region offset by `(dx, dy)`. Every pass is feathered.

- [ ] **Step 1: Write the failing tests**

`images/tools/tests/test_plates.py`:

```python
import io
import unittest

import numpy as np
from PIL import Image

from iconlib import paths, plates, tools


class PlatesTest(unittest.TestCase):
    def test_original_comes_from_git_history(self):
        blob = tools.git_blob(paths.PRE_REBRAND_REV, paths.rel(paths.alt_output("Retro")))
        expect = np.asarray(Image.open(io.BytesIO(blob)).convert("RGBA"))
        self.assertTrue(np.array_equal(np.asarray(plates.original("Retro")), expect))

    def test_mirror_pass_removes_blob_and_keeps_outside(self):
        img = Image.new("RGBA", (1024, 1024), (40, 40, 40, 255))
        img.paste((0, 255, 0, 255), (250, 250, 400, 400))
        out = plates.apply_pass(img, ("mirror", (200, 200, 450, 450)))
        a = np.asarray(out)
        self.assertTrue((a[300, 300, :3] == 40).all())
        before, after = np.asarray(img), a
        outside = np.ones((1024, 1024), bool)
        outside[200:450, 200:450] = False
        self.assertTrue(np.array_equal(before[outside], after[outside]))

    def test_shift_pass_copies_offset_region(self):
        img = Image.new("RGBA", (1024, 1024), (0, 0, 0, 255))
        img.paste((9, 9, 9, 255), (600, 200, 900, 500))
        out = np.asarray(plates.apply_pass(img, ("shift", (200, 200, 500, 500), 400, 0)))
        self.assertTrue((out[350, 350, :3] == 9).all())

    def test_every_alternate_has_passes(self):
        self.assertEqual(set(plates.PASSES), set(paths.ALT_NAMES))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `ModuleNotFoundError: No module named 'iconlib.plates'`.

- [ ] **Step 3: Implement `plates.py`**

```python
"""One-off drafting of ghost-free alternate plates (spec D8). Not part of `all`."""
from __future__ import annotations

import io

from PIL import Image, ImageDraw, ImageFilter, ImageOps

from . import paths, raster, tools

# Covers the ghost AND its glow on every alternate (ghost bbox approx. 220-470 x 200-520).
GHOST_BOX = (190, 180, 505, 545)
FEATHER = 8
INSET = 12

# Mirror works where the plate is left/right symmetric around x = 511.5: the box
# maps to x 518-833, clear of the right frame and of the paper curl
# (bottom-right). Retro has a gloss highlight at top-right, so it uses vertical
# shifts by whole scanline periods (period approx. 59.6 px: 358 = 6 periods, 119 = 2).
PASSES: dict[str, list[tuple]] = {
    "Blueprint": [("mirror", GHOST_BOX)],
    "Chalkboard": [("mirror", GHOST_BOX)],
    "Glass": [("mirror", GHOST_BOX)],
    "Holographic": [("mirror", GHOST_BOX)],
    "Microchip": [("mirror", GHOST_BOX)],
    "Paper": [("mirror", GHOST_BOX)],
    "Retro": [("shift", (160, 150, 530, 505), 0, 358), ("shift", (160, 470, 530, 570), 0, 119)],
    "Xray": [("mirror", GHOST_BOX)],
}


def original(name: str) -> Image.Image:
    """The pre-rebrand alternate, read from git: the working-tree file may already hold Ferris."""
    img = Image.open(io.BytesIO(tools.git_blob(paths.PRE_REBRAND_REV, paths.rel(paths.alt_output(name)))))
    img.load()
    return img.convert("RGBA")


def _feather_mask(size) -> Image.Image:
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle([INSET, INSET, size[0] - INSET, size[1] - INSET], 30, fill=255)
    return m.filter(ImageFilter.GaussianBlur(FEATHER))


def apply_pass(img: Image.Image, p: tuple) -> Image.Image:
    kind, box = p[0], p[1]
    x0, y0, x1, y1 = box
    if kind == "mirror":
        src = ImageOps.mirror(img).crop(box)
    elif kind == "shift":
        dx, dy = p[2], p[3]
        src = img.crop((x0 + dx, y0 + dy, x1 + dx, y1 + dy))
    else:
        raise ValueError(f"unknown pass kind {kind!r}")
    out = img.copy()
    out.paste(src, (x0, y0), _feather_mask(src.size))
    return out


def draft_plate(name: str) -> Image.Image:
    img = original(name)
    for p in PASSES[name]:
        img = apply_pass(img, p)
    return img


def run(args) -> int:
    out = paths.plate_path(args.name)
    raster.save_png(draft_plate(args.name), out)
    print(f"wrote {paths.rel(out)} (draft: review before committing)")
    return 0
```

Register `plate` in `build_icons.py`:

```python
from iconlib import flats, mascot, official, plates, review  # noqa: E402
```

```python
COMMANDS["plate"] = plates.run
COMMAND_TOOLS["plate"] = []
```

(Place these two lines right after the `COMMANDS`/`COMMAND_TOOLS` literals.) Add to the parser:

```python
    pl = sub.add_parser("plate", help="draft a ghost-free plate for one alternate (one-off)")
    pl.add_argument("name", choices=list(paths.ALT_NAMES))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: PASS.

- [ ] **Step 5: Draft and inspect the batch-1 plates**

```bash
for n in Chalkboard Paper Blueprint Retro; do python3 -I images/tools/build_icons.py plate $n; done
```

Open each `images/alternates/<name>/plate.png` with the Read tool at full size, and also a 2× crop of `GHOST_BOX`. There must be:

- no ghost pixels or glow left;
- no visible seam or doubled feature;
- for Blueprint, grid lines and circle arcs that continue across the box;
- for Paper, rule lines that line up;
- for Retro, scanlines that line up and no gloss fragment.

To retouch, edit `PASSES[name]`: enlarge the box, or add a further pass (e.g. a small `("shift", box, dx, 0)` to patch a seam). Then redraft. Ferris will later cover y ≈ 388–618, but the area above that must be clean on its own.

- [ ] **Step 6: Commit**

```bash
git add images/tools images/alternates
git commit -m "icons: ghost-free plates for Chalkboard, Paper, Blueprint, Retro (drafted from 401b95b0e)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Style-matched Ferris (batch 1), composition, and review gate 2

**Files:**

- Create: `images/tools/iconlib/styles.py`, `images/tools/iconlib/alternates.py`
- Modify: `images/tools/iconlib/review.py` (add the `alternates` sheet), `images/tools/build_icons.py` (register `alternates`)
- Modify (generated): `macos/Assets.xcassets/Alternate Icons/{Chalkboard,Paper,Blueprint,Retro}Image.imageset/macOS-AppIcon-1024px.png`
- Test: `images/tools/tests/test_styles.py`, `images/tools/tests/test_alternates.py`

**Interfaces:**

- Consumes: `raster.ferris_at`, `raster.classify`, `raster.ferris_box`, `raster.glow`, `raster.Masks`, `paths.plate_path`, `paths.alt_output`, `plates.original`.
- Produces:
  - `styles.PAD = 48`, and per-alternate style functions with the signature `style(f: Image RGBA (w,h), m: Masks, rng: np.random.Generator) -> Image RGBA (w+2*PAD, h+2*PAD)` with Ferris at offset `(PAD, PAD)`.
  - `styles.STYLES: dict[str, callable]` (batch 1 now, batch 2 in Task 7).
  - `alternates.SCREEN_BBOX_DEFAULT`, `alternates.SCREEN_BBOX: dict`, `alternates.SEED`, `alternates.compose(name) -> Image`, `alternates.run(args) -> int`.

- [ ] **Step 1: Write the failing tests**

`images/tools/tests/test_styles.py`:

```python
import unittest

import numpy as np

from iconlib import raster, styles


class StylesTest(unittest.TestCase):
    def setUp(self):
        self.f = raster.ferris_at(459)
        self.m = raster.classify(self.f)

    def test_contract_and_determinism(self):
        for name, fn in styles.STYLES.items():
            with self.subTest(name=name):
                a = fn(self.f, self.m, np.random.default_rng(3))
                b = fn(self.f, self.m, np.random.default_rng(3))
                self.assertEqual(a.size, (459 + 2 * styles.PAD, self.f.height + 2 * styles.PAD))
                self.assertEqual(a.mode, "RGBA")
                self.assertEqual(a.tobytes(), b.tobytes())
                alpha = np.asarray(a)[..., 3]
                self.assertGreater((alpha > 128).sum(), 0.2 * self.m.body.sum())

    def test_styles_are_not_stock_orange(self):
        for name, fn in styles.STYLES.items():
            with self.subTest(name=name):
                a = np.asarray(fn(self.f, self.m, np.random.default_rng(3))).astype(int)
                solid = a[..., 3] > 200
                orange = (abs(a[..., 0] - 247) < 12) & (abs(a[..., 1] - 76) < 12) & (a[..., 2] < 12)
                self.assertLess((orange & solid).sum(), 0.05 * solid.sum())


if __name__ == "__main__":
    unittest.main()
```

`images/tools/tests/test_alternates.py`:

```python
import unittest

import numpy as np

from iconlib import alternates, paths, raster, styles


class ComposeTest(unittest.TestCase):
    def test_compose_keeps_plate_outside_ferris(self):
        for name in styles.STYLES:
            if not paths.plate_path(name).is_file():
                continue
            with self.subTest(name=name):
                from PIL import Image
                plate = np.asarray(Image.open(paths.plate_path(name)).convert("RGBA"))
                out = np.asarray(alternates.compose(name))
                x, y, w, h = raster.ferris_box(alternates.screen_bbox(name))
                keep = np.ones(plate.shape[:2], bool)
                keep[max(0, y - styles.PAD):y + h + styles.PAD, max(0, x - styles.PAD):x + w + styles.PAD] = False
                self.assertTrue(np.array_equal(plate[keep], out[keep]))
                self.assertFalse(np.array_equal(plate[~keep], out[~keep]))

    def test_compose_is_deterministic(self):
        name = next(n for n in styles.STYLES if paths.plate_path(n).is_file())
        self.assertEqual(alternates.compose(name).tobytes(), alternates.compose(name).tobytes())


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `ModuleNotFoundError: No module named 'iconlib.styles'`.

- [ ] **Step 3: Implement `styles.py` (batch 1)**

```python
"""Style-matched Ferris renderings for the alternate icons (spec D4).

Every style: (f: RGBA Ferris at target size, m: Masks of f, rng) -> RGBA canvas
of size (w + 2*PAD, h + 2*PAD) with Ferris at (PAD, PAD). Deterministic for a given rng.
"""
from __future__ import annotations

import numpy as np
from PIL import Image, ImageFilter

from . import raster

PAD = 48


def _rgba(rgb: np.ndarray, alpha: np.ndarray) -> Image.Image:
    return Image.fromarray(np.dstack([np.clip(rgb, 0, 255), np.clip(alpha, 0, 1) * 255]).round()
                           .astype(np.uint8), "RGBA")


def _pad(img: Image.Image, glow_rgb=None, radius=0.0, strength=0.0) -> Image.Image:
    w, h = img.size
    c = Image.new("RGBA", (w + 2 * PAD, h + 2 * PAD), (0, 0, 0, 0))
    if glow_rgb is not None:
        a = Image.new("L", c.size, 0)
        a.paste(img.getchannel("A"), (PAD, PAD))
        c.alpha_composite(raster.glow(a, glow_rgb, radius, strength))
    c.alpha_composite(img, (PAD, PAD))
    return c


def _grain(rng, shape, cell, lo, hi) -> np.ndarray:
    h, w = shape
    n = rng.random((h // cell + 1, w // cell + 1))
    n = np.kron(n, np.ones((cell, cell)))[:h, :w]
    return np.clip((n - lo) / (hi - lo), 0, 1)


def _fuzzy(m: raster.Masks, rng, blur_div: float, grain_cell: int, lo: float, hi: float) -> np.ndarray:
    h, w = m.alpha.shape
    solid = Image.fromarray(((m.body | m.shade) * 255).astype(np.uint8), "L")
    fuzz = np.asarray(solid.filter(ImageFilter.GaussianBlur(max(1.0, w / blur_div)))) / 255.0
    a = np.clip(fuzz * 1.8 - 0.2, 0, 1) * np.clip(_grain(rng, (h, w), grain_cell, lo, hi) * 1.6, 0, 1)
    return np.where(m.dark | m.white, 0.0, a)


def chalk(f, m, rng):
    """Chalkboard: bright chalk white, grainy, shade dimmer, eyes/mouth as holes."""
    h, w = m.alpha.shape
    a = np.where(m.body | m.shade, 1.0, 0.0) * _grain(rng, (h, w), 2, 0.02, 0.45) * m.alpha
    a = np.where(m.shade, a * 0.75, a)
    return _pad(_rgba(np.full((h, w, 3), 250.0), a))


def ink(f, m, rng):
    """Paper: saturated fuzzy blue ink with a fibrous edge; eyes/mouth as paper holes."""
    h, w = m.alpha.shape
    rgb = np.empty((h, w, 3))
    rgb[:] = (20, 95, 245)
    return _pad(_rgba(rgb, _fuzzy(m, rng, 115, 1, 0.05, 0.55)))


def blueprint(f, m, rng):
    """Blueprint: white fuzzy fill like the blueprint ghost; eyes/mouth show the blue plate."""
    h, w = m.alpha.shape
    rgb = np.empty((h, w, 3))
    rgb[:] = (245, 248, 255)
    return _pad(_rgba(rgb, _fuzzy(m, rng, 160, 1, 0.0, 0.35)))


def pixel(f, m, rng):
    """Retro: green phosphor pixel art (cell = w/22), darker shade, dim eyes, glow."""
    h, w = m.alpha.shape
    cell = max(4, round(w / 22))
    sw, sh = max(1, round(w / cell)), max(1, round(h / cell))
    small = np.asarray(f.resize((sw, sh), Image.BOX)).astype(np.float64)
    sa = small[..., 3] / 255
    sd = (small[..., :3].sum(-1) < 260) & (sa > 0.5)
    ss = (small[..., 0] < 205) & (small[..., 1] < 70) & ~sd
    rgb = np.empty((sh, sw, 3))
    rgb[:] = (70, 255, 130)
    rgb[ss] = (30, 170, 80)
    a = (sa > 0.45).astype(float)
    a[sd] = 0.25
    px = _rgba(rgb, a).resize((w, h), Image.NEAREST)
    return _pad(px, (70, 255, 130), w / 30, 0.8)


STYLES = {"Blueprint": blueprint, "Chalkboard": chalk, "Paper": ink, "Retro": pixel}
```

- [ ] **Step 4: Implement `alternates.py`**

```python
"""Compose each alternate: committed ghost-free plate + style-matched Ferris (spec D2/D4)."""
from __future__ import annotations

import sys

import numpy as np
from PIL import Image

from . import paths, raster, styles

# The screen region Ferris is centred on (half-open). Most plates share the
# official screen geometry; per-plate overrides are measured in Task 6 Step 5.
SCREEN_BBOX_DEFAULT = (174, 174, 849, 849)
SCREEN_BBOX: dict[str, tuple] = {}
SEED = 20261008


def screen_bbox(name: str) -> tuple:
    return SCREEN_BBOX.get(name, SCREEN_BBOX_DEFAULT)


def compose(name: str) -> Image.Image:
    plate = Image.open(paths.plate_path(name)).convert("RGBA")
    x, y, w, h = raster.ferris_box(screen_bbox(name))
    f = raster.ferris_at(w)
    rng = np.random.default_rng([SEED, paths.ALT_NAMES.index(name)])
    layer = styles.STYLES[name](f, raster.classify(f), rng)
    out = plate.copy()
    out.alpha_composite(layer, (x - styles.PAD, y - styles.PAD))
    return out


def run(args) -> int:
    names = args.names or list(paths.ALT_NAMES)
    missing = [n for n in names if n not in styles.STYLES or not paths.plate_path(n).is_file()]
    if missing:
        print(f"error: no style or plate for: {', '.join(missing)}", file=sys.stderr)
        return 1
    for n in names:
        raster.save_png(compose(n), paths.alt_output(n))
        print(f"wrote {paths.rel(paths.alt_output(n))}")
    return 0
```

Register it in `build_icons.py`:

```python
from iconlib import alternates, flats, mascot, official, plates, review  # noqa: E402
```

```python
COMMANDS["alternates"] = alternates.run
COMMAND_TOOLS["alternates"] = ["resvg"]
```

```python
    al = sub.add_parser("alternates", help="compose alternate icons from plates + styled Ferris")
    al.add_argument("names", nargs="*", default=[])
```

Unknown names are rejected by `alternates.run`'s `missing` check.

Add the `alternates` review sheet to `review.py`:

```python
def alternate_sheets(names) -> list[Path]:
    from . import alternates, plates
    names = names or [n for n in paths.ALT_NAMES if paths.plate_path(n).is_file()]
    cell = 300
    out = Image.new("RGBA", (cell * 3 + 120, (cell + 50) * len(names)), LIGHT)
    for r, n in enumerate(names):
        y = r * (cell + 50)
        final = alternates.compose(n)
        for c, img in enumerate((plates.original(n), Image.open(paths.plate_path(n)).convert("RGBA"), final)):
            out.alpha_composite(img.resize((cell, cell), Image.LANCZOS), (c * cell, y))
        out.alpha_composite(final.resize((32, 32), Image.LANCZOS), (3 * cell + 10, y + 10))
        out.alpha_composite(final.resize((16, 16), Image.LANCZOS), (3 * cell + 52, y + 18))
    p = paths.REVIEW_DIR / f"alternates-{'-'.join(n.lower() for n in names)}.png"
    raster.save_png(out, p)
    return [p]


SHEETS["alternates"] = lambda args: alternate_sheets(args.names)
```

- [ ] **Step 5: Measure the screen bboxes**

For each batch-1 plate, open the full-size plate with the Read tool and compare its inner screen edge with `(174, 174, 849, 849)`. Paper and Blueprint have no CRT screen: use the inner rule frame for Paper and the inner rounded rectangle for Blueprint. Only if an edge is more than 10 px away, add an entry to `alternates.SCREEN_BBOX`, e.g. `"Paper": (150, 150, 872, 872)`, with a comment saying what edge was measured.

- [ ] **Step 6: Run the tests, then generate**

```bash
python3 -I -m unittest discover -s images/tools/tests -t images/tools -v
python3 -I images/tools/build_icons.py alternates Chalkboard Paper Blueprint Retro
python3 -I images/tools/build_icons.py review alternates Chalkboard Paper Blueprint Retro
```

Expected: tests PASS, and 4 alternates are written. Look at `target/icon-review/alternates-chalkboard-paper-blueprint-retro.png` yourself first: each row should show the original, the plate and the final, with the 32/16 px versions alongside.

- [ ] **Step 7: Commit**

```bash
git add images/tools macos/Assets.xcassets/Alternate\ Icons
git commit -m "icons: style-matched Ferris on Chalkboard, Paper, Blueprint, Retro

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: REVIEW GATE 2 (alternates batch 1)**

Present the sheet through the review-gate procedure. Ask about each row: is the plate clean, does the crab match the material, and is it readable at 32 px? The tunables are the parameters inside each style function, `plates.PASSES`, and `alternates.SCREEN_BBOX`. After each change, rerun `plate` (only if the plates changed), `alternates`, and `review alternates`; commit as `icons: tune <name> (gate 2)`; and re-present. **Stop until the user approves.**

---

### Task 7: Batch 2 plates, styles, and review gate 3

**Files:**

- Modify: `images/tools/iconlib/plates.py` (retouch `PASSES` for batch 2 if needed), `images/tools/iconlib/styles.py` (add 4 styles), `images/tools/iconlib/alternates.py` (bbox overrides if needed)
- Create (generated): `images/alternates/{holographic,microchip,xray,glass}/plate.png`
- Modify (generated): `macos/Assets.xcassets/Alternate Icons/{Holographic,Microchip,Xray,Glass}Image.imageset/macOS-AppIcon-1024px.png`
- Test: `images/tools/tests/test_styles.py` (picks up the new styles automatically), plus this step's new test

**Interfaces:**

- Consumes: everything from Task 6.
- Produces: `styles.chrome(f, m, rng, rim=None, eye_rainbow=False)`, `styles.holographic`, `styles.microchip`, `styles.xray`, `styles.glass`, and `STYLES` covering all 8 `paths.ALT_NAMES`.

- [ ] **Step 1: Write the failing test**

Append to `images/tools/tests/test_styles.py`, before `if __name__`:

```python
class CoverageTest(unittest.TestCase):
    def test_all_alternates_have_styles(self):
        from iconlib import paths
        self.assertEqual(set(styles.STYLES), set(paths.ALT_NAMES))
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: FAIL in `test_all_alternates_have_styles` (4 names missing).

- [ ] **Step 3: Implement the batch-2 styles**

Add to `styles.py`, above `STYLES`, and replace the `STYLES` literal:

```python
def chrome(f, m, rng, rim=None, eye_rainbow=False):
    """Brushed chrome like the Microchip/Xray/Holographic ghosts; optional iridescent rim."""
    h, w = m.alpha.shape
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    cy, cx = h * 0.52, w / 2
    ang = np.arctan2(yy - cy, xx - cx)
    r2 = ((yy - cy) ** 2 + (xx - cx) ** 2) / (w * 0.42) ** 2
    lum = 175 + 70 * np.cos(ang * 6) * np.exp(-r2) + 20 * np.sin(ang * 2)
    lum = np.where(m.shade, lum * 0.72, lum)
    rgb = np.dstack([lum, lum, lum * 1.03])
    if eye_rainbow:
        hue = (ang + np.pi) / (2 * np.pi)
        rainbow = np.dstack([127 + 127 * np.cos(2 * np.pi * (hue + k / 3)) for k in range(3)])
        eye = 0.55 * np.array([60, 20, 90]) + 0.45 * rainbow
    else:
        eye = np.broadcast_to(np.array([20.0, 20.0, 24.0]), rgb.shape)
    rgb = np.where(m.dark[..., None], eye, rgb)
    rgb = np.where(m.white[..., None], 255.0, rgb)
    body = _pad(_rgba(rgb, m.alpha))
    if rim is None:
        return body
    edge = body.getchannel("A").filter(ImageFilter.GaussianBlur(max(2.0, w / 75)))
    halo = Image.new("RGBA", body.size, tuple(rim) + (0,))
    halo.putalpha(edge.point(lambda v: int(min(255, min(v, 255 - v) * 1.6))))
    halo.alpha_composite(body)
    return halo


def holographic(f, m, rng):
    return chrome(f, m, rng, rim=(255, 60, 200), eye_rainbow=True)


def microchip(f, m, rng):
    return chrome(f, m, rng)


def xray(f, m, rng):
    return chrome(f, m, rng)


def glass(f, m, rng):
    """Glass: frosted translucent fill, bright rim, dark eyes, white highlights."""
    h, w = m.alpha.shape
    solid = m.body | m.shade
    k = max(3, (w // 90) | 1)
    inner = Image.fromarray((solid * 255).astype(np.uint8), "L").filter(ImageFilter.MinFilter(k))
    rim = solid & ~(np.asarray(inner) > 127)
    a = np.where(m.shade, 0.18, 0.28) * solid
    a = np.where(rim, 0.85, a)
    a = np.where(m.dark, 0.9, a)
    a = np.where(m.white, 1.0, a)
    rgb = np.full((h, w, 3), 245.0)
    rgb[m.dark] = (20, 20, 24)
    return _pad(_rgba(rgb, a * (m.alpha > 0)))


STYLES = {
    "Blueprint": blueprint, "Chalkboard": chalk, "Glass": glass, "Holographic": holographic,
    "Microchip": microchip, "Paper": ink, "Retro": pixel, "Xray": xray,
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: PASS. `test_styles_are_not_stock_orange` also covers the new styles.

- [ ] **Step 5: Draft and inspect the batch-2 plates**

```bash
for n in Holographic Microchip Xray Glass; do python3 -I images/tools/build_icons.py plate $n; done
```

Inspect them as in Task 5 Step 5:

- **Holographic:** the perspective grid and gradient must stay continuous.
- **Microchip:** the orange trace on the right must be untouched. Traces may end where Ferris will cover them (y ≈ 388–618). Above that, the mirrored circuitry must not show an obvious seam; if it does, add a `shift` pass from a clean board area.
- **Xray:** same as Microchip.
- **Glass:** smooth, with no ghost rim.

Measure the screen bboxes as in Task 6 Step 5.

- [ ] **Step 6: Generate, then review the sheet yourself**

```bash
python3 -I images/tools/build_icons.py alternates Holographic Microchip Xray Glass
python3 -I images/tools/build_icons.py review alternates Holographic Microchip Xray Glass
```

- [ ] **Step 7: Commit**

```bash
git add images/tools images/alternates macos/Assets.xcassets/Alternate\ Icons
git commit -m "icons: plates + style-matched Ferris on Holographic, Microchip, Xray, Glass

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: REVIEW GATE 3 (alternates batch 2)**

Same procedure and questions as gate 2. **Stop until the user approves.**

---

### Task 8: `all` + `verify`, NOTICE, README, generator docs

**Files:**

- Create: `images/tools/iconlib/verify.py`, `images/tools/README.md`
- Modify: `images/tools/build_icons.py` (register `all`, `verify`), `NOTICE`, `README.md`
- Test: `images/tools/tests/test_verify.py`

**Interfaces:**

- Consumes: every `run` function, `tools.git_blob`, and the path tables.
- Produces: `verify.outputs() -> list[Path]`, `verify.snapshot() -> dict[str,str]`, `verify.no_ghost_left() -> list[str]` (offending paths), `verify.run_all(args) -> int` (the `all` pipeline), `verify.run(args) -> int`.

- [ ] **Step 1: Write the failing test**

`images/tools/tests/test_verify.py`:

```python
import unittest

from iconlib import paths, verify


class VerifyTest(unittest.TestCase):
    def test_outputs_cover_every_derived_asset(self):
        outs = {paths.rel(p) for p in verify.outputs()}
        self.assertEqual(len(outs), 43)  # 1+1 mascots, 8 alts, 3 AppIcon, 8+8 gnome, 12 icons, ico, favicon
        self.assertIn("dist/windows/rustty.ico", outs)
        self.assertIn("crates/rustty-apprt/assets/images/gnome/2048.png", outs)

    def test_no_ghost_left(self):
        self.assertEqual(verify.no_ghost_left(), [])


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`
Expected: `ModuleNotFoundError: No module named 'iconlib.verify'`.

- [ ] **Step 3: Implement `verify.py` and `all`**

```python
"""Whole-run checks: every derived asset changed since the pre-rebrand rev; reruns are byte-stable."""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

from . import alternates, flats, mascot, official, paths, tools


def run_all(args) -> int:
    for step in (mascot.run_mascot, official.run, flats.run,
                 lambda a: alternates.run(argparse.Namespace(names=[]))):
        rc = step(args)
        if rc != 0:
            return rc
    return 0


def outputs() -> list[Path]:
    out = [paths.ICON_MASCOT, paths.CUSTOM_MASCOT]
    out += [paths.alt_output(n) for n in paths.ALT_NAMES]
    out += [paths.APPICON_IMAGESET / n for n in paths.APPICON_FILES.values()]
    out += [paths.GNOME_DIR / f"{s}.png" for s in paths.GNOME_SIZES]
    out += [paths.APPRT_GNOME_DIR / f"{s}.png" for s in paths.GNOME_SIZES]
    out += [paths.ICONS_DIR / n for n in paths.ICONS_MAP]
    out += [paths.WIN_ICO, paths.FAVICON]
    return out


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def snapshot() -> dict[str, str]:
    return {paths.rel(p): _sha(p.read_bytes()) for p in outputs() + [paths.ICON_JSON]}


def no_ghost_left() -> list[str]:
    bad = []
    for p in outputs() + [paths.ICON_JSON]:
        if _sha(p.read_bytes()) == _sha(tools.git_blob(paths.PRE_REBRAND_REV, paths.rel(p))):
            bad.append(paths.rel(p))
    return bad


def run(args) -> int:
    ok = True
    bad = no_ghost_left()
    print(f"no-ghost: {'OK' if not bad else 'UNCHANGED: ' + ', '.join(bad)}")
    ok &= not bad
    if run_all(args) != 0:
        return 1
    first = snapshot()
    if run_all(args) != 0:
        return 1
    second = snapshot()
    drift = sorted(k for k in first if first[k] != second[k])
    print(f"determinism: {'OK' if not drift else 'DRIFT: ' + ', '.join(drift)}")
    ok &= not drift
    return 0 if ok else 1
```

In `build_icons.py`, add `verify` to the iconlib import, and add:

```python
COMMANDS["all"] = verify.run_all
COMMAND_TOOLS["all"] = ["resvg", "ictool", "icotool"]
COMMANDS["verify"] = verify.run
COMMAND_TOOLS["verify"] = ["resvg", "ictool", "icotool"]
```

```python
    sub.add_parser("all", help="rebuild every derived icon from committed sources")
    sub.add_parser("verify", help="no-ghost check + run `all` twice and require byte-identical output")
```

- [ ] **Step 4: Run tests and `verify`**

```bash
python3 -I -m unittest discover -s images/tools/tests -t images/tools -v
python3 -I images/tools/build_icons.py verify
git status --short
```

Expected:

- tests PASS;
- `no-ghost: OK`, `determinism: OK`, exit 0;
- `git status` clean apart from untracked review files, which live in the ignored `target/`.

If `git status` shows modified assets, the committed assets were stale against the generator: inspect, then commit them as `icons: regenerate with final generator`.

- [ ] **Step 5: Write `images/tools/README.md`**

````markdown
# Icon generator

Rebuilds every derived Rustty icon from committed sources. Spec:
`docs/superpowers/specs/2026-10-08-ferris-icon-rebrand-design.md`.

## Prerequisites (macOS only)

- Xcode 26+ (Icon Composer's `ictool`; override the path with `ICTOOL=`)
- `brew install resvg icoutils`
- Python 3 with Pillow and numpy

## Commands (run from anywhere)

```
python3 -I images/tools/build_icons.py all        # rebuild everything
python3 -I images/tools/build_icons.py verify     # no-ghost check + determinism (runs `all` twice)
python3 -I images/tools/build_icons.py mascot | official | flats | alternates [Name ...]
python3 -I images/tools/build_icons.py plate <Name>   # one-off: redraft a ghost-free plate
python3 -I images/tools/build_icons.py review official|custom|alternates [Name ...]
```

Tests: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`

## Sources → outputs

| Source                                                     | Outputs                                                                                                                                                              |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `images/ferris/rustacean-flat-happy.svg` (CC0)             | `Rustty.icon/Assets/Rustty.png`, `Custom Icon/CustomIconGhost.imageset/ghosty.png`                                                                                   |
| `images/Rustty.icon` (via `ictool`)                        | `AppIconImage.imageset/*`, `images/gnome/*` (+ `crates/rustty-apprt/assets/images/gnome/*`), `images/icons/*`, `dist/windows/rustty.ico`, `dist/doxygen/favicon.png` |
| `images/alternates/<name>/plate.png` + `iconlib/styles.py` | `Alternate Icons/<Name>Image.imageset/macOS-AppIcon-1024px.png`                                                                                                      |

After regenerating the GTK icons, refresh the sha256 table in
`crates/rustty-apprt/src/gtk/gresource.rs` and run
`cargo test -p rustty-apprt --test gtk_icon_identity`.
````

- [ ] **Step 6: Credit Ferris in `NOTICE` and add the README logo**

Append to `NOTICE`:

```
Artwork
-------
Ferris the Rustacean (the crab in every Rustty icon) is from rustacean.net and is
dedicated to the public domain under CC0 1.0. See `images/ferris/PROVENANCE.md`.
The icon backgrounds (frames, screens, alternate-icon plates) derive from Ghostty's
icon artwork (MIT, see above).
```

In `README.md`, replace the opening lines:

```html
<!-- LOGO -->
<h1>
  <p align="center">Rustty</p>
</h1>
```

with:

```html
<!-- LOGO -->
<h1>
  <p align="center">
    <img src="images/gnome/256.png" alt="Logo" width="128" />
    <br />Rustty
  </p>
</h1>
```

- [ ] **Step 7: Format and commit**

```bash
npx --no-install prettier -w images/tools/README.md images/ferris/PROVENANCE.md README.md
git add images/tools NOTICE README.md images/ferris/PROVENANCE.md
git commit -m "icons: all/verify commands, generator README, Ferris CC0 credit, README logo

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Cross-platform verification and final review

**Files:** none are created. This task runs checks and fixes only what they reveal.

- [ ] **Step 1: Local Rust checks**

```bash
cargo test -p rustty-apprt --test gtk_icon_identity
python3 -I -m unittest discover -s images/tools/tests -t images/tools -v
git diff --stat 401b95b0e -- macos/Sources macos/Rustty.xcodeproj xtask snap flatpak Doxyfile crates/rustty-config
```

Expected: tests pass, and the last command prints **nothing**, which shows the Global Constraint "do not change" held.

- [ ] **Step 2: macOS app build and visual pass**

```bash
cargo run -p xtask -- xcframework
nu macos/build.nu
open macos/build/Debug/Rustty.app
```

Use the configuration directory that `build.nu` reports if it isn't `Debug`. Check each of these and take a screenshot of each (`screencapture -x target/icon-review/app-<what>.png`):

- **Dock and Finder icon.** Ferris on blue. Clear the icon cache first if the old icon is still showing: `killall Dock`.
- **Settings window and error view.** Both use `AppIconImage`.
- **Every `macos-icon` value.** Set each one in `~/.config/rustty/config` (`macos-icon = blueprint`, …, `xray`) and reload config each time; the dock tile should switch.
- **`macos-icon = custom-style`.** Set `macos-icon-ghost-color = #F74C00` and `macos-icon-screen-color = #0018B4,#2A4BFF`, then try each `macos-icon-frame` (aluminum, beige, plastic, chrome). It should match `custom-style-grid.png`.
- **Reset.** Set `macos-icon = official` again; the dock tile should go back to the official icon.

Present the screenshots to the user in one review screen.

- [ ] **Step 3: Linux lane (rvm), run alone**

```bash
ssh rvm 'cd ~/projects/rustty && git cat-file -e 401b95b0e^{commit} && echo HAS-BASE'
# HAS-BASE printed: an incremental bundle is enough; otherwise bundle the whole branch.
git bundle create target/icons-rebrand.bundle 401b95b0e..icons-rebrand   # or: git bundle create target/icons-rebrand.bundle icons-rebrand
scp target/icons-rebrand.bundle rvm:/tmp/
ssh rvm 'cd ~/projects/rustty && git fetch /tmp/icons-rebrand.bundle icons-rebrand:icons-rebrand && git checkout icons-rebrand && \
  cargo test -p rustty-apprt --features gtk --test gtk_assets --test gtk_icon_identity && \
  cargo run -p xtask -- resources --target-os linux --prefix /usr --optimize ReleaseFast && \
  for s in 16 32 128 256 512 1024; do cmp zig-out/share/icons/hicolor/${s}x${s}/apps/io.airely.rustty.png images/gnome/$s.png || exit 1; done && echo HICOLOR-OK'
```

Expected: tests pass and `HICOLOR-OK` is printed. Afterwards, check out the branch rvm was on before, delete `/tmp/icons-rebrand.bundle` and the build outputs this run created, and run `printf '%s\n' gael | sudo -S -p '' fstrim -av` on rvm.

- [ ] **Step 4: Windows lane (rwin), run alone after rvm**

```bash
git bundle create target/icons-rebrand-full.bundle icons-rebrand   # rwin clones from scratch, so the bundle must be complete
scp target/icons-rebrand-full.bundle rwin:icons-rebrand.bundle
ssh rwin 'git clone --branch icons-rebrand icons-rebrand.bundle icons-rebrand; cd icons-rebrand; cargo test -p rustty --test windows_resources'
```

Expected: `windows_resources` passes; it compares the embedded `RT_GROUP_ICON` against `dist/windows/rustty.ico`. Afterwards, delete the clone and the bundle with literal paths (`Remove-Item -Recurse -Force C:\Users\gael\icons-rebrand, C:\Users\gael\icons-rebrand.bundle`) and run `Optimize-Volume -DriveLetter C -ReTrim`. If ssh does not answer, find the VM's current IP in `/var/db/dhcpd_leases` (the rwin memory note explains how).

- [ ] **Step 5: Final whole-branch review and hand-off**

Request a code review of `main..icons-rebrand` (superpowers:requesting-code-review). Fix any confirmed findings with `icons: review fix …` commits. Re-run `python3 -I images/tools/build_icons.py verify` and the unit tests. Then report to the user: what changed, the gate approvals, the lane results, and that the branch is ready to merge into local `main`. Merging is the user's call; per memory, clean up the worktree after the merge.
