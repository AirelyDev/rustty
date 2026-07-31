#!/usr/bin/env python3
"""Read-only checks for the shared-runtime platform selector contracts."""

from pathlib import Path
import re
import subprocess

RUSTTY = Path("/Users/gael/projects/rustty")
GHOSTTY = Path("/Users/gael/projects/ghostty")


def line(path: Path, pattern: str) -> tuple[int, str]:
    rx = re.compile(pattern)
    for n, text in enumerate(path.read_text().splitlines(), 1):
        if rx.search(text):
            return n, text.strip()
    raise SystemExit(f"missing pattern {pattern!r} in {path}")


renderer = RUSTTY / "crates/rustty-renderer/src/lib.rs"
opengl_decl = line(renderer, r'^#\[cfg\(target_os = "linux"\)\]$')
renderer_exports = [
    (n, text.strip())
    for n, text in enumerate(renderer.read_text().splitlines(), 1)
    if re.search(r'^#\[cfg\(.*target_os = "(?:linux|freebsd)"', text)
]

ghost_backend = GHOSTTY / "src/renderer/backend.zig"
ghost_fallback = line(ghost_backend, r'^\s*return \.opengl;$')

page_allocator = RUSTTY / "crates/rustty-os/src/page_allocator.rs"
windows_error = line(page_allocator, r'compile_error!\("rustty-os::page_allocator: windows')

ghost_exec = GHOSTTY / "src/termio/Exec.zig"
windows_reader = line(ghost_exec, r'^\s*fn threadMainWindows\(')
windows_conpty = line(ghost_exec, r'^\s*\.pseudo_console = if \(builtin\.os\.tag == \.windows\)')

cfg = subprocess.check_output(
    ["rustc", "--print", "cfg", "--target", "x86_64-unknown-freebsd"], text=True
)
is_freebsd = 'target_os="freebsd"' in cfg

print(f"rustty OpenGL gate: {renderer}:{opengl_decl[0]} {opengl_decl[1]}")
print(f"rustty renderer linux/freebsd cfg entries: {renderer_exports}")
print(f"ghostty non-Darwin renderer fallback: {ghost_backend}:{ghost_fallback[0]} {ghost_fallback[1]}")
print(f"freebsd target cfg present: {is_freebsd}")
print(f"rustty Windows compile stop: {page_allocator}:{windows_error[0]} {windows_error[1]}")
print(f"ghostty Windows reader: {ghost_exec}:{windows_reader[0]} {windows_reader[1]}")
print(f"ghostty ConPTY handoff: {ghost_exec}:{windows_conpty[0]} {windows_conpty[1]}")
