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
