#!/usr/bin/env python3
"""Static predicates for explicit non-macOS port gaps found by this audit."""

from pathlib import Path

RUSTTY = Path(__file__).resolve().parents[3]
GHOSTTY = RUSTTY.parent / "ghostty"


def require(label: str, condition: bool) -> None:
    if not condition:
        raise SystemExit(f"FAIL {label}")
    print(f"PASS {label}")


xev_backend = (RUSTTY / "crates/rustty-xev/src/backend.rs").read_text()
xev_root = (RUSTTY / "crates/rustty-xev/src/lib.rs").read_text()
require(
    "rustty-freebsd-selects-kqueue",
    'target_os = "freebsd"' in xev_backend and "Backend::Kqueue" in xev_backend,
)
require(
    "rustty-kqueue-module-is-macos-only",
    '#[cfg(target_os = "macos")]\n#[allow(dead_code)]\nmod kqueue;' in xev_root,
)
require(
    "rustty-freebsd-enters-no-backend-seam",
    'mod no_backend_seam' in xev_root
    and 'target_os = "freebsd"' not in xev_root.split("mod no_backend_seam", 1)[0][-300:],
)
require(
    "rustty-no-backend-seam-panics",
    'unimplemented!("{}", MSG)' in xev_root,
)

ghostty_xev = next((GHOSTTY / "zig-pkg").glob("libxev-*/src/backend.zig"))
ghostty_kqueue = next((GHOSTTY / "zig-pkg").glob("libxev-*/src/backend/kqueue.zig"))
require(
    "ghostty-libxev-supports-freebsd-kqueue",
    ".freebsd => .kqueue" in ghostty_xev.read_text()
    and ".freebsd => true" in ghostty_kqueue.read_text(),
)

linux_renderer = (RUSTTY / "crates/rustty-renderer/src/generic/linux_impl.rs").read_text()
ghostty_renderer = (GHOSTTY / "src/renderer/generic.zig").read_text()
for label, rust_marker, ghost_marker in (
    ("kitty-images", "MILESTONE-SCOPE-CUT (kitty images", "kittyRequiresUpdate"),
    ("background-image", "no `bg_image` field", "prepBackgroundImage"),
    ("inspector-overlay", "MILESTONE-SCOPE-CUT (inspector debug overlay)", "rebuildOverlay"),
    ("custom-shaders", "MILESTONE-SCOPE-CUT (custom shaders)", "custom_shaders"),
):
    require(
        f"rustty-linux-explicitly-omits-{label}",
        rust_marker in linux_renderer and ghost_marker in ghostty_renderer,
    )

require(
    "rustty-renderer-mailbox-is-noop",
    "pub struct SurfaceMailbox;" in linux_renderer
    and "pub fn push<M>(&self, _msg: M, _timeout: MailboxTimeout) -> usize {\n        0"
    in linux_renderer,
)
require(
    "ghostty-renderer-publishes-scrollbar-and-health",
    ".scrollbar = self.scrollbar" in ghostty_renderer
    and ".renderer_health = health" in ghostty_renderer,
)

print("All explicit-gap predicates reproduced.")
