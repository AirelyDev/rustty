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
# Batch 2. Holographic: its ghost drop shadow reaches y 560, so the box is taller. Glass:
# smooth gradient, one tall mirror box. Microchip: the orange trace starts at y 520, so the
# mirror stops at y 520 and a second mirror (x >= 335 only) plus a shift from the clean
# board below clear the ghost's bottom lip without touching the trace. Two shifts from the board to the left (bounded at y 523, just above the trace) soften the last ghost-lip shadow now that the trace's left end is visible. Xray: the board is
# not symmetric (the left pad has two circles, the right one), so the mirror starts at
# x 226, a shift repairs the pad's right edge, and a mirror about x = 225 (dx = 573)
# re-creates the hidden right half of the left pad. Two more shifts take board from clean
# neighbours: one clears the ghost's left rim below the pad, one removes a spurious fourth cap.
PASSES: dict[str, list[tuple]] = {
    "Blueprint": [("mirror", GHOST_BOX, 5)],
    "Chalkboard": [("mirror", GHOST_BOX, 7)],
    "Glass": [("mirror", (190, 180, 520, 590), 7)],
    "Holographic": [("mirror", (190, 180, 505, 590))],
    "Microchip": [
        ("mirror", (190, 180, 520, 520), 7),
        ("shift", (235, 470, 340, 540), 0, 60),
        ("mirror", (335, 490, 520, 546), 7),
        ("shift", (296, 470, 396, 523), -66, 0),
        ("shift", (336, 470, 440, 523), -126, 0),
    ],
    "Paper": [("mirror", (198, 222, 505, 548), 7), ("mirror", (228, 203, 505, 270), 7)],
    "Retro": [("shift", (460, 150, 625, 620), 150, 0), ("mirror", (150, 140, 512, 620), 7)],
    "Xray": [
        ("mirror", (226, 180, 520, 545), 0),
        ("shift", (254, 196, 310, 356), 44, 0),
        ("shift", (214, 380, 286, 504), -40, 0),
        ("shift", (256, 190, 330, 282), 0, 70),
        ("mirror", (214, 236, 276, 412), 573),
    ],
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
