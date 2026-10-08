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
