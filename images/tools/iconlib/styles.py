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
    a = np.where(m.body | m.shade, 1.0, 0.0) * _grain(rng, (h, w), 2, 0.03, 0.10) * m.alpha
    a = np.where(m.shade, a * 0.85, a)
    return _pad(_rgba(np.full((h, w, 3), 250.0), a))


def ink(f, m, rng):
    """Paper: saturated fuzzy blue ink with a fibrous edge; eyes/mouth as paper holes."""
    h, w = m.alpha.shape
    rgb = np.empty((h, w, 3))
    rgb[:] = (20, 95, 245)
    return _pad(_rgba(rgb, _fuzzy(m, rng, 115, 1, 0.0, 0.25)))


def blueprint(f, m, rng):
    """Blueprint: white fuzzy fill like the blueprint ghost; eyes/mouth show the blue plate."""
    h, w = m.alpha.shape
    rgb = np.empty((h, w, 3))
    rgb[:] = (245, 248, 255)
    return _pad(_rgba(rgb, _fuzzy(m, rng, 160, 1, 0.0, 0.14)))


def pixel(f, m, rng):
    """Retro: green phosphor pixel art (cell = w/30), darker shade, dim eyes, glow."""
    h, w = m.alpha.shape
    cell = max(4, round(w / 30))
    sw, sh = max(1, round(w / cell)), max(1, round(h / cell))

    def frac(mask):
        return np.asarray(Image.fromarray((mask * 255).astype(np.uint8), "L").resize((sw, sh), Image.BOX)) / 255.0

    def sym(x):
        # Deliberate pixel-sprite symmetrization (approved at gate 2). Ferris's raster is NOT
        # exactly symmetric, but at this cell size the asymmetric eye and leg pixels read as
        # L/J shapes, so the left half is mirrored to give a clean sprite.
        return np.concatenate([x[:, :(sw + 1) // 2], x[:, :sw // 2][:, ::-1]], axis=1)

    sa = sym(frac(m.alpha > 0.5))
    sd = sym(frac(m.dark) > 0.3)
    ss = sym(frac(m.shade) > 0.4) & ~sd
    rgb = np.empty((sh, sw, 3))
    rgb[:] = (70, 255, 130)
    rgb[ss] = (30, 170, 80)
    rgb[sd] = (10, 90, 45)
    a = (sa > 0.45).astype(float)
    a[sd] = 0.7
    px = _rgba(rgb, a).resize((w, h), Image.NEAREST)
    return _pad(px, (70, 255, 130), w / 30, 0.8)


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
        eye = 0.35 * np.array([60, 20, 90]) + 0.65 * rainbow
    else:
        eye = np.broadcast_to(np.array([20.0, 20.0, 24.0]), rgb.shape)
    rgb = np.where(m.dark[..., None], eye, rgb)
    rgb = np.where(m.white[..., None], 255.0, rgb)
    body = _pad(_rgba(rgb, m.alpha))
    if rim is None:
        return body
    edge = body.getchannel("A").filter(ImageFilter.GaussianBlur(max(2.0, w / 45)))
    halo = Image.new("RGBA", body.size, tuple(rim) + (0,))
    halo.putalpha(edge.point(lambda v: int(min(255, min(v, 255 - v) * 2.4))))
    halo.alpha_composite(body)
    return halo


def holographic(f, m, rng):
    return chrome(f, m, rng, rim=(255, 60, 200), eye_rainbow=True)


def microchip(f, m, rng):
    return chrome(f, m, rng)


def xray(f, m, rng):
    return chrome(f, m, rng)


def glass(f, m, rng):
    """Glass: frosted translucent fill, bright rim, dark eyes, white highlights, soft dark edge."""
    h, w = m.alpha.shape
    solid = m.body | m.shade
    k = max(3, (w // 30) | 1)
    inner = Image.fromarray((solid * 255).astype(np.uint8), "L").filter(ImageFilter.MinFilter(k))
    rim = solid & ~(np.asarray(inner) > 127)
    a = np.where(m.shade, 0.5, 0.62) * solid
    a = np.where(rim, 1.0, a)
    a = np.where(m.dark, 0.95, a)
    a = np.where(m.white, 1.0, a)
    rgb = np.full((h, w, 3), (232.0, 242.0, 255.0))
    rgb[m.dark] = (20, 20, 24)
    body = _pad(_rgba(rgb, a * (m.alpha > 0)), (255, 255, 255), w / 70, 0.6)
    # Soft darker outer edge (like the glass ghost's rim shadow), outside the silhouette only.
    sil = Image.new("L", body.size, 0)
    sil.paste(Image.fromarray(((m.alpha > 0) * 255).astype(np.uint8), "L"), (PAD, PAD))
    edge = raster.glow(sil, (45, 60, 90), max(2.0, w / 50), 3.5)
    ea = np.asarray(edge.getchannel("A")).astype(float) * (1 - np.asarray(sil) / 255.0)
    edge.putalpha(Image.fromarray(ea.round().astype(np.uint8), "L"))
    out = Image.new("RGBA", body.size, (0, 0, 0, 0))
    out.alpha_composite(edge)
    out.alpha_composite(body)
    return out


STYLES = {
    "Blueprint": blueprint, "Chalkboard": chalk, "Glass": glass, "Holographic": holographic,
    "Microchip": microchip, "Paper": ink, "Retro": pixel, "Xray": xray,
}
