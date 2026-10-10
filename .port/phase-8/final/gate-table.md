| Gate | Kind | Mac | s | rvm | s | Note |
|---|---|---|---|---|---|---|
| A3 | PROGRESS | GREEN | 2 | GREEN | 1 |  |
| A4 | PROGRESS | GREEN | 9 | GREEN | 2 |  |
| A5 | PROGRESS | GREEN | 4 | GREEN | 4 |  |
| A5-MASK-SELF | PROGRESS | GREEN | 3 | GREEN | 2 |  |
| D1 | PROGRESS | GREEN | 2 | GREEN | 3 |  |
| D10 | PROGRESS | GREEN | 10 | GREEN | 6 |  |
| D11 | HARD | GREEN | 0 | GREEN | 1 |  |
| D11r | HARD | GREEN | 26 | GREEN | 21 |  |
| D2 | PROGRESS | GREEN | 3 | GREEN | 1 |  |
| D3 | PROGRESS | GREEN | 4 | GREEN | 11 |  |
| D4 | PROGRESS | GREEN | 0 | GREEN | 0 |  |
| D5 | PROGRESS | GREEN | 0 | GREEN | 0 |  |
| D6 | PROGRESS | GREEN | 2 | GREEN | 9 |  |
| D8 | PROGRESS | GREEN | 1 | GREEN | 6 |  |
| D9 | PROGRESS | GREEN | 23 | GREEN | 12 |  |
| G-ABISCHEMA | PROGRESS | GREEN | 2 | GREEN | 1 |  |
| G-ARTIFACT | HARD | GREEN | 1 | GREEN | 3 |  |
| G-ARTIFACT-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-ASAN | HARD | RED | 1307 | - |  | disk floor; rerun GREEN (G-ASAN-rerun-mac.log) |
| G-BENCH | HARD | GREEN | 264 | GREEN | 309 |  |
| G-BLUEPRINT | HARD | BLOCKED | 0 | GREEN | 6 |  |
| G-CCONSUMER | HARD | GREEN | 17 | GREEN | 19 |  |
| G-CCONSUMER-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-CENSUS | HARD | GREEN | 3 | GREEN | 6 |  |
| G-CENSUS-SELF | HARD | GREEN | 2 | GREEN | 4 |  |
| G-CI-SELF | HARD | GREEN | 76 | GREEN | 65 |  |
| G-DIST | HARD | GREEN | 22 | GREEN | 138 |  |
| G-DOXYGEN | HARD | GREEN | 0 | GREEN | 0 |  |
| G-EOL | HARD | GREEN | 22 | GREEN | 7 |  |
| G-ESCTEST | HARD | GREEN | 174 | GREEN | 199 |  |
| G-ESCTEST-SELF | HARD | GREEN | 2 | GREEN | 1 |  |
| G-EXAMPLES | HARD | GREEN | 266 | GREEN | 41 |  |
| G-FEATMATRIX | HARD | GREEN | 21 | - |  |  |
| G-FILES | HARD | GREEN | 249 | GREEN | 23 |  |
| G-FONTBACKEND | HARD | GREEN | 484 | - |  |  |
| G-GTKFEAT | HARD | BLOCKED | 0 | GREEN | 421 |  |
| G-GTKINIT | HARD | GREEN | 1 | GREEN | 0 |  |
| G-HWASAN | HARD | BLOCKED | 0 | RED | 495 | disk floor, not a result; DEFERRED |
| G-I18N | HARD | GREEN | 0 | GREEN | 0 |  |
| G-KAITAI | HARD | GREEN | 2 | GREEN | 4 |  |
| G-KAITAI-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-LINT | HARD | GREEN | 25 | GREEN | 48 |  |
| G-LOGFILE | HARD | GREEN | 84 | GREEN | 111 |  |
| G-LOGFILE-SELF | HARD | GREEN | 3 | GREEN | 5 |  |
| G-LSAN | HARD | RED | 1349 | RED | 622 | disk floor; rerun found N548 (fixed FX12); FX12 verify SCOPED-GREEN rustty-vt 6898; disk floor (14 GB < 15), not a result; DEFERRED to later full rerun |
| G-LSAN-GTK | HARD | BLOCKED | 0 | - |  |  |
| G-LZ4 | PROGRESS | GREEN | 13 | GREEN | 64 |  |
| G-MIRIRUN-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-MSAN | HARD | BLOCKED | 0 | BLOCKED | 467 | disk floor; DEFERRED |
| G-NIX | HARD | BLOCKED | 0 | BLOCKED | 554 |  |
| G-NIX-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-ONIG | HARD | GREEN | 663 | GREEN | 983 |  |
| G-ORACLE-SH-SELF | HARD | GREEN | 8 | GREEN | 2 |  |
| G-P2 | HARD | GREEN | 1 | GREEN | 0 |  |
| G-P2-SELF | HARD | GREEN | 0 | GREEN | 1 |  |
| G-PARITY | HARD | GREEN | 0 | GREEN | 0 |  |
| G-PARITY-SELF | HARD | GREEN | 1 | GREEN | 0 |  |
| G-REFGEOM | HARD | GREEN | 223 | GREEN | 318 |  |
| G-REFGEOM-SELF | HARD | GREEN | 2 | GREEN | 3 |  |
| G-REL | HARD | RED | 360 | - |  | N566 latency test under full-CI load; quiet 10x rerun in PERF |
| G-SAN-SELF | HARD | GREEN | 202 | GREEN | 360 |  |
| G-SKIP | HARD | GREEN | 8 | GREEN | 8 |  |
| G-SKIP-SELF | HARD | GREEN | 2 | GREEN | 1 |  |
| G-SYMS | HARD | GREEN | 4 | GREEN | 5 |  |
| G-SYMS-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-TESTS | HARD | GREEN | 24 | GREEN | 53 |  |
| G-TESTS-SELF | HARD | GREEN | 2 | GREEN | 3 |  |
| G-TGTDIR | HARD | GREEN | 136 | - |  |  |
| G-THEMES | HARD | GREEN | 1 | GREEN | 1 |  |
| G-TSAN | HARD | RED | 3307 | - |  | N547 sys_log_no_alloc flake (fixed FX11); FX11 verify SCOPED-GREEN rustty-vt-c 604 |
| G-UDDOCS | HARD | GREEN | 0 | GREEN | 0 |  |
| G-UDDOCS-SELF | HARD | GREEN | 0 | GREEN | 0 |  |
| G-VALGRIND | HARD | BLOCKED | 0 | KILLED | 15510 | COMPLETE GREEN: 1/4 + 2/4 (ci.sh) + 8 manual sub-shards |
| G-VALGRIND-SELF | HARD | GREEN | 9 | GREEN | 8 |  |
| G-VALGRIND[1/4] | SHARD | BLOCKED | 0 | GREEN | 12758 |  |
| G-VALGRIND[2/4] | SHARD | BLOCKED | 0 | GREEN | 15191 |  |
| G-VALGRIND[3/4] | SHARD | BLOCKED | 0 | KILLED | 11 | duplicate killed; covered by manual 8 sub-shards {3,7,11,15}/16 GREEN |
| G-VALGRIND[4/4] | SHARD | BLOCKED | 0 | KILLED | 7 | duplicate killed; covered by manual sub-shards {4,8,12,16}/16 GREEN |
| G-VTALLOC | HARD | GREEN | 0 | GREEN | 0 |  |
| G-VTALLOC-SELF | HARD | GREEN | 1 | GREEN | 3 |  |
| G-VTCLIB | HARD | GREEN | 9 | GREEN | 12 |  |
| G-VTCLIB-SELF | HARD | GREEN | 13 | GREEN | 15 |  |
| G-VTFEAT | PROGRESS | GREEN | 184 | GREEN | 188 |  |
| G-VTFEAT-SELF | HARD | GREEN | 1 | GREEN | 1 |  |
| G-WINCHK | HARD | GREEN | 61 | GREEN | 41 |  |
| G-WINPROTO-DEFAULT | HARD | GREEN | 0 | GREEN | 1 |  |
| G-XCF | HARD | GREEN | 8 | - |  |  |
| G-XCF-SELF | HARD | GREEN | 2 | BLOCKED | 0 |  |
| G-XEV-IOS | HARD | GREEN | 8 | GREEN | 10 |  |
| G-XEVREACH | HARD | GREEN | 1 | GREEN | 1 |  |
| G-XTARGET | HARD | GREEN | 11 | GREEN | 39 |  |
| G-XTARGET-SELF | HARD | BLOCKED | 0 | GREEN | 0 |  |
| G-XTASK | HARD | GREEN | 21 | GREEN | 2 |  |
| G1 | PROGRESS | GREEN | 8 | GREEN | 10 |  |
| G1-SELF | HARD | GREEN | 3 | GREEN | 5 |  |
| G1b | PROGRESS | GREEN | 3 | GREEN | 2 |  |
| G2 | PROGRESS | GREEN | 13 | GREEN | 14 |  |
| G3 | PROGRESS | GREEN | 4 | GREEN | 5 |  |
| G3b | PROGRESS | GREEN | 229 | GREEN | 267 |  |
| G4 | HARD | BLOCKED | 0 | BLOCKED | 1 |  |
| G4-uni | HARD | GREEN | 9 | GREEN | 17 |  |
| G5 | PROGRESS | GREEN | 72 | - |  |  |
| G6 | PROGRESS | GREEN | 25300 | - |  |  |
| G6b | HARD | GREEN | 16 | GREEN | 25 |  |
| G6c | PROGRESS | GREEN | 2123 | - |  |  |
| G6t | PROGRESS | GREEN | 23305 | - |  |  |
| G7 | HARD | GREEN | 0 | GREEN | 1 |  |
| G8 | ARMED | GREEN | 0 | - |  |  |
| G8b | PERF-FENCE | BLOCKED | 602 | - |  | load cooldown BLOCK in the full run; binding run in PERF |
| G8b-SELF | HARD | GREEN | 2 | GREEN | 2 |  |
| G9 | PROGRESS | GREEN | 14 | GREEN | 39 |  |
| cargo build --workspace | CARGO | GREEN | 44 | GREEN | 104 |  |
| cargo check -p rustty-wasm --target wasm32-unknown-unknown | CARGO | GREEN | 7 | GREEN | 7 |  |
| cargo clippy -D warnings | CARGO | GREEN | 44 | GREEN | 77 |  |
| cargo fmt --check | CARGO | GREEN | 3 | GREEN | 3 |  |
| cargo test --workspace | CARGO | GREEN | 428 | GREEN | 624 |  |
| cargo test -p rustty-os --features flatpak | CARGO | - |  | GREEN | 3 |  |
