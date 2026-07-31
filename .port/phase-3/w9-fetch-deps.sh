#!/usr/bin/env bash
# P3-W9 T6 — fetch the ported-Zig-library tarballs at BOTH the pin and oracle
# head so the deltas can be diffed. Extracts to /tmp/w9deps/<name>.
set -euo pipefail
D=/tmp/w9deps
mkdir -p "$D"
for u in \
  vaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558 \
  vaxis-1dbbe575dff4586fe51e3217aa5c3fecdcbb6089 \
  zf-3c52637b7e937c5ae61fd679717da3e276765b23 \
  zf-c35c421f84895193246db06c40683c1a30e616ef \
  libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e \
  libxev-9ce8e8e6ff89e583258a7f8e7adeeeaeae8611bf \
  uucode-2826a37a4562284fdacd8fa029d49509cc9bffcd
do
  if [ ! -f "$D/$u.tar.gz" ]; then
    curl -sS -L --max-time 300 -o "$D/$u.tar.gz" "https://deps.files.ghostty.org/$u.tar.gz"
  fi
  mkdir -p "$D/$u"
  tar xzf "$D/$u.tar.gz" -C "$D/$u"
  echo "$u  $(du -sh "$D/$u.tar.gz" | cut -f1)"
done
# uucode at the PIN is a versioned tarball, not a rev tarball.
u=uucode-0.2.0-ZZjBPqZVVABQepOqZHR7vV_NcaN-wats0IB6o-Exj6m9
if [ ! -f "$D/$u.tar.gz" ]; then
  curl -sS -L --max-time 300 -o "$D/$u.tar.gz" "https://deps.files.ghostty.org/$u.tar.gz"
fi
mkdir -p "$D/$u"
tar xzf "$D/$u.tar.gz" -C "$D/$u"
echo "$u  $(du -sh "$D/$u.tar.gz" | cut -f1)"
