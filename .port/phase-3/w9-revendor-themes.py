#!/usr/bin/env python3
"""P3-W9 T5 step 1 — re-vendor the iterm2 theme corpus.

Usage: w9-revendor-themes.py <extracted-tarball-dir>

Copies every upstream theme file verbatim, deletes vendored files upstream
dropped, and preserves the ONE R2 filename rename (`Ghostty Default Style
Dark` -> `Rustty Default Style Dark`; content is untouched and byte-identical
either way). `.md` files in the vendored dir are rustty's own provenance /
licence notes and are never touched.
"""
import os
import shutil
import sys

VEND = "crates/rustty-extra/themes"
NEW = sys.argv[1]

# R2 (casing-preserving per token). The corpus contains exactly ONE identity
# token in a FILENAME and none in any file BODY (verified by the byte-compare
# in w9-themes-diff.py: the renamed file's content is byte-identical upstream).
RENAME = {"Ghostty Default Style Dark": "Rustty Default Style Dark"}

upstream = sorted(os.listdir(NEW))
want = {RENAME.get(f, f) for f in upstream}

added = removed = updated = 0
for f in upstream:
    dst_name = RENAME.get(f, f)
    src = os.path.join(NEW, f)
    dst = os.path.join(VEND, dst_name)
    new_bytes = open(src, "rb").read()
    if os.path.exists(dst):
        if open(dst, "rb").read() == new_bytes:
            continue
        updated += 1
    else:
        added += 1
    with open(dst, "wb") as fh:
        fh.write(new_bytes)

for f in sorted(os.listdir(VEND)):
    if f.endswith(".md"):
        continue
    if f not in want:
        os.remove(os.path.join(VEND, f))
        removed += 1

total = len([f for f in os.listdir(VEND) if not f.endswith(".md")])
print(f"added={added} updated={updated} removed={removed} total_non_md={total}")
assert total == len(upstream), f"{total} != {len(upstream)}"
