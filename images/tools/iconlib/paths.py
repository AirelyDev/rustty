"""Every input and output path the icon generator touches."""
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
IMAGES = REPO / "images"

FERRIS_SVG = IMAGES / "ferris" / "rustacean-flat-happy.svg"

ICON_DIR = IMAGES / "Rustty.icon"
ICON_JSON = ICON_DIR / "icon.json"
ICON_MASCOT = ICON_DIR / "Assets" / "Rustty.png"

XCASSETS = REPO / "macos" / "Assets.xcassets"
APPICON_IMAGESET = XCASSETS / "AppIconImage.imageset"
APPICON_FILES = {
    256: "macOS-AppIcon-256px-128pt@2x.png",
    512: "macOS-AppIcon-512px.png",
    1024: "macOS-AppIcon-1024px.png",
}
CUSTOM_DIR = XCASSETS / "Custom Icon"
CUSTOM_MASCOT = CUSTOM_DIR / "CustomIconGhost.imageset" / "ghosty.png"

GNOME_DIR = IMAGES / "gnome"
GNOME_SIZES = (16, 32, 64, 128, 256, 512, 1024, 2048)
APPRT_GNOME_DIR = REPO / "crates" / "rustty-apprt" / "assets" / "images" / "gnome"

ICONS_DIR = IMAGES / "icons"
ICONS_MAP = {
    "icon_16.png": 16, "icon_16@2x.png": 32,
    "icon_32.png": 32, "icon_32@2x.png": 64,
    "icon_128.png": 128, "icon_128@2x.png": 256,
    "icon_256.png": 256, "icon_256@2x.png": 512,
    "icon_512.png": 512, "icon_512@2x.png": 1024,
    "icon_1024.png": 1024, "icon_1024@2x.png": 2048,
}

WIN_ICO = REPO / "dist" / "windows" / "rustty.ico"
ICO_SIZES = (16, 32, 48, 256)
FAVICON = REPO / "dist" / "doxygen" / "favicon.png"
FAVICON_SIZE = 32

ALT_NAMES = ("Blueprint", "Chalkboard", "Glass", "Holographic", "Microchip", "Paper", "Retro", "Xray")

PRE_REBRAND_REV = "401b95b0e"
REVIEW_DIR = REPO / "target" / "icon-review"


def alt_output(name: str) -> Path:
    return XCASSETS / "Alternate Icons" / f"{name}Image.imageset" / "macOS-AppIcon-1024px.png"


def plate_path(name: str) -> Path:
    return IMAGES / "alternates" / name.lower() / "plate.png"


def rel(path: Path) -> str:
    return Path(path).resolve().relative_to(REPO).as_posix()
