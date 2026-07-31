#!/usr/bin/env bash
# P3-W9 T3 — regenerate the 5 rustty-extra config-derived goldens after a
# Config field-set change. Each is the byte-exact output of a ported generator
# that reflects over `Config`'s field names, so ANY field addition changes them.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
T=crates/rustty-extra/testdata
cargo run -q -p xtask -- gen bash    > "$T/rustty.bash"
cargo run -q -p xtask -- gen fish    > "$T/rustty.fish"
cargo run -q -p xtask -- gen zsh     > "$T/_rustty"
cargo run -q -p xtask -- gen sublime > "$T/rustty.sublime-syntax"
cargo run -q -p xtask -- gen vim-syntax > "$T/vim-syntax.vim"
git --no-pager diff --stat -- "$T"
