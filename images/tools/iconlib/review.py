"""Review sheets for the user gates, written to target/icon-review/ (git-ignored)."""
from __future__ import annotations

import io
import shutil
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
    tools.require(["ictool"])
    for extra in ("xcrun", "iconutil"):
        if shutil.which(extra) is None:
            raise tools.ToolMissing(f"{extra} not found on PATH (review official needs Xcode command line tools)")
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


def run(args) -> int:
    for p in SHEETS[args.what](args):
        print(f"wrote {paths.rel(p)}")
    return 0
