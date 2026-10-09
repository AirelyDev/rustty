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
