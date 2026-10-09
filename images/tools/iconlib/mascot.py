"""Mascot layers: the official Icon Composer layer and the custom-style layer."""
import numpy as np
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


# The screen-mask layer's opaque bbox is 174..848 inclusive (half-open 849).
CUSTOM_SCREEN_BBOX = (174, 174, 849, 849)
# Mid gray chosen at review gate 4 (spec: As built); lighter levels give pastel tints.
CUSTOM_LEVELS = {"body": 135, "shade": 85, "dark": 20, "white": 255}
CUSTOM_GLOW_RADIUS = 14
CUSTOM_GLOW_STRENGTH = 0.5


def custom_mascot() -> Image.Image:
    """Neutral gray Ferris for ColorizedRusttyIcon: the user's tint supplies the hue (spec D5)."""
    x, y, w, h = raster.ferris_box(CUSTOM_SCREEN_BBOX)
    f = raster.ferris_at(w)
    m = raster.classify(f)
    lum = np.zeros((h, w))
    for name, level in CUSTOM_LEVELS.items():
        lum[getattr(m, name)] = level
    gray = np.dstack([lum, lum, lum, m.alpha * 255]).round().astype(np.uint8)
    canvas = Image.new("RGBA", (1024, 1024), (255, 255, 255, 0))
    alpha = Image.new("L", canvas.size, 0)
    alpha.paste(f.getchannel("A"), (x, y))
    canvas.alpha_composite(raster.glow(alpha, (255, 255, 255), CUSTOM_GLOW_RADIUS, CUSTOM_GLOW_STRENGTH))
    canvas.alpha_composite(Image.fromarray(gray, "RGBA"), (x, y))
    return canvas.convert("LA")


def run_mascot(args) -> int:
    raster.save_png(icon_mascot(), paths.ICON_MASCOT)
    raster.save_png(custom_mascot(), paths.CUSTOM_MASCOT)
    print(f"wrote {paths.rel(paths.ICON_MASCOT)}, {paths.rel(paths.CUSTOM_MASCOT)}")
    return 0
