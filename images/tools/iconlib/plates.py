"""One-off drafting of ghost-free alternate plates (spec D8). Not part of `all`."""
from __future__ import annotations

import io

from PIL import Image, ImageDraw, ImageFilter, ImageOps

from . import paths, raster, tools

# Covers the ghost AND its glow on every alternate (ghost bbox approx. 220-470 x 200-520).
GHOST_BOX = (190, 180, 505, 545)
FEATHER = 8
INSET = 12

# Mirror works where the plate is left/right symmetric. The axis is x = 508 (sum of
# mirrored columns 1015-1016) for the frames, 509 for the Blueprint grid, so the
# optional third element of a mirror pass offsets the source (7 px; Blueprint 5 px).
# Paper's box stays clear of the frame's inner corner arc, so a second strip
# covers the ghost's top scribble. Retro has a wide gloss highlight whose lower edge
# is an ellipse, so the ghost's right-hand glow tail (x 500-600) is first replaced by a
# horizontal shift (the gloss is uniform there), then the left half is mirrored.
PASSES: dict[str, list[tuple]] = {
    "Blueprint": [("mirror", GHOST_BOX, 5)],
    "Chalkboard": [("mirror", GHOST_BOX, 7)],
    "Glass": [("mirror", GHOST_BOX)],
    "Holographic": [("mirror", GHOST_BOX)],
    "Microchip": [("mirror", GHOST_BOX)],
    "Paper": [("mirror", (198, 222, 505, 548), 7), ("mirror", (228, 203, 505, 270), 7)],
    "Retro": [("shift", (460, 150, 625, 620), 150, 0), ("mirror", (150, 140, 512, 620), 7)],
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
        # Optional third element: the plate's symmetry axis is not exactly x = 511.5
        # (the alternates' frames sit at 96-919, the Blueprint grid at 1018 - x), so
        # `dx` moves the mirrored source sideways to line the features up.
        dx = p[2] if len(p) > 2 else 0
        src = ImageOps.mirror(img).crop((x0 + dx, y0, x1 + dx, y1))
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
