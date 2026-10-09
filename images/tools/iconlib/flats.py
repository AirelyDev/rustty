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
