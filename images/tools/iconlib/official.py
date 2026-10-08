"""Edit Rustty.icon/icon.json: centre the mascot layers and tint the glass orange."""
import json
from pathlib import Path

from . import paths

ORANGE_FILL = "extended-srgb:0.96863,0.29804,0.00000,1.00000"  # #F74C00
MASCOT_POSITION = {"scale": 1, "translation-in-points": [0, -10]}


def _dump(obj) -> str:
    # Icon Composer's own format: 2-space indent, "key" : value, no trailing newline.
    return json.dumps(obj, indent=2, separators=(",", " : "))


def update_icon_json(path: Path = paths.ICON_JSON) -> bool:
    raw = Path(path).read_text()
    doc = json.loads(raw)
    for group in doc["groups"]:
        for layer in group["layers"]:
            if layer.get("image-name") != "Rustty.png":
                continue
            layer["position"] = dict(MASCOT_POSITION)
            if layer.get("name") == "RusttyBlur":
                layer["fill"] = {"solid": ORANGE_FILL}
    out = _dump(doc)
    if out == raw:
        return False
    Path(path).write_text(out)
    return True


def run(args) -> int:
    changed = update_icon_json(paths.ICON_JSON)
    print(f"{'updated' if changed else 'unchanged'} {paths.rel(paths.ICON_JSON)}")
    return 0
