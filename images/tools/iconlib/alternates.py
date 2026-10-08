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
# Explicit per-icon nudge applied after the D2 placement (controller ruling R8). Microchip:
# the orange trace (y 520-591) is the icon's focal bar, so Ferris sits just above it.
PLACEMENT_OFFSET: dict[str, tuple[int, int]] = {"Microchip": (0, -103)}


def screen_bbox(name: str) -> tuple:
    return SCREEN_BBOX.get(name, SCREEN_BBOX_DEFAULT)


def placement(name: str) -> tuple:
    """Ferris's (x, y, w, h): the D2 box moved by PLACEMENT_OFFSET."""
    x, y, w, h = raster.ferris_box(screen_bbox(name))
    dx, dy = PLACEMENT_OFFSET.get(name, (0, 0))
    return (x + dx, y + dy, w, h)


def compose(name: str) -> Image.Image:
    plate = Image.open(paths.plate_path(name)).convert("RGBA")
    x, y, w, h = placement(name)
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
