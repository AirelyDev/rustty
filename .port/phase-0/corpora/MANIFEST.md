# Phase 0 — Golden Corpora MANIFEST

**Date:** 2026-06-21
**Reference tree:** `/Users/gael/projects/ghostty` @ `4749c4e` (Zig 0.15.2) — READ-ONLY, untouched.
**Purpose:** byte-exact golden outputs the rustty differential harness diffs the
port against. Every golden below was produced from the **prebuilt** reference
artifacts (no rebuild). Each entry records: source, exact reproduce command, and
determinism status (every golden was re-run and is byte-stable unless noted).

All paths are absolute. Reproduce commands assume:

```
EX=/Users/gael/projects/rustty/.port/phase-0/examples-out      # prebuilt c-vt-* binaries
GB=/Users/gael/projects/ghostty/zig-out/Ghostty.app/Contents/MacOS/ghostty   # prebuilt ghostty binary
TI=/Users/gael/projects/ghostty/zig-out/share/terminfo          # shipped terminfo db
```

> **Binary-availability note (R2/parity-relevant).** This revision's `zig build`
> emits **no `zig-out/bin/ghostty`** CLI binary (only `ghostty-bench`,
> `ghostty-gen`, `props-unigen`, `symbols-unigen` — see `build-summary.md`). The
> only runnable full `ghostty` binary is the macOS app binary
> `zig-out/Ghostty.app/Contents/MacOS/ghostty` (universal x86_64+arm64, app
> runtime `.none`, Metal renderer, CoreText). The task's literal
> `zig-out/bin/ghostty +show-config` is therefore satisfied via that app binary —
> the same prebuilt artifact, run read-only (it only reads config and writes to
> our output paths; the ghostty tree is never modified).

---

## Category 1 — c-vt / cpp-vt example consumers  (21 goldens)

**Dir:** `cvt/`
**Source:** the libghostty-vt C/C++ example programs in
`/Users/gael/projects/ghostty/example/<dir>/src/main.c|main.cpp`, prebuilt by the
build stage into `$EX/<dir>/zig-out/bin/<exe>` (see `build-summary.md` §5).

**Representative input:** each program is **self-contained** — it hardcodes its
VT/OSC/key/mouse/etc. input in source and takes **no stdin and no argv**. Running
the binary IS feeding it representative input. Output is fully deterministic
(verified: each binary run twice → identical sha256).

**Capture method:** stdout+stderr captured **combined, in arrival order** into
`<name>.golden` (this is the harness diff target). For provenance we also keep
`<name>.stdout`, `<name>.stderr`, and `<name>.exit` sidecars. For all 21,
stderr is empty and exit code is 0.

**Reproduce one golden:**
```
$EX/<dir>/zig-out/bin/<exe> > cvt/<dir>.golden 2>&1
```
**Reproduce all + determinism check:**
```
/Users/gael/projects/rustty/.port/phase-0/run-cvt-corpora.sh
```

| golden (name = example dir) | binary | bytes | exit | stderr |
|---|---|---:|:--:|:--:|
| c-vt.golden                  | c_vt                  |   39 | 0 | empty |
| c-vt-build-info.golden       | c_vt_build_info       |  177 | 0 | empty |
| c-vt-colors.golden           | c_vt_colors           |  964 | 0 | empty |
| c-vt-effects.golden          | c_vt_effects          |  306 | 0 | empty |
| c-vt-encode-focus.golden     | c_vt_encode_focus     |   21 | 0 | empty |
| c-vt-encode-key.golden       | c_vt_encode_key       |  **0** | 0 | empty |
| c-vt-encode-mouse.golden     | c_vt_encode_mouse     |    9 | 0 | empty |
| c-vt-formatter.golden        | c_vt_formatter        |  184 | 0 | empty |
| c-vt-grid-ref-tracked.golden | c_vt_grid_ref_tracked |   97 | 0 | empty |
| c-vt-grid-traverse.golden    | c_vt_grid_traverse    |  128 | 0 | empty |
| c-vt-kitty-graphics.golden   | c_vt_kitty_graphics   |  364 | 0 | empty |
| c-vt-modes.golden            | c_vt_modes            |   86 | 0 | empty |
| c-vt-paste.golden            | c_vt_paste            |  136 | 0 | empty |
| c-vt-render.golden           | c_vt_render           | 1221 | 0 | empty |
| c-vt-selection.golden        | c_vt_selection        |  192 | 0 | empty |
| c-vt-selection-gesture.golden| c_vt_selection_gesture|   44 | 0 | empty |
| c-vt-sgr.golden              | c_vt_sgr              |  325 | 0 | empty |
| c-vt-size-report.golden      | c_vt_size_report      |   38 | 0 | empty |
| c-vt-static.golden           | c_vt_static           |   39 | 0 | empty |
| c-vt-stream.golden           | c_vt_stream           |   76 | 0 | empty |
| cpp-vt-stream.golden         | cpp_vt_stream         |   27 | 0 | empty |

**Refresh 2026-08-10 (F-016):** `c-vt-effects.golden` (195→306 B) and
`c-vt-kitty-graphics.golden` (329→364 B) were re-captured — the originals
predated the 2026-07-31 pin advance, whose example sources gained the
clipboard-write effect and "Storage generation" output. The pre-pin
`examples-out/` zig-built binaries no longer exist (and the ghostty tree's own
`example/*/zig-out` binaries abort against the advanced dylib), so the refresh
capture is the PIN's example `src/main.c` compiled with clang against the
pin's Mach-O static lib
(`$GHOSTTY/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a`)
— the same recipe `build_port_driver.sh` uses for the port side, so both sides
of G4 leg B are captured identically. Determinism re-verified (two runs,
identical bytes); exit 0, stderr empty for both. The other 19 goldens still
byte-match the pinned oracle and were left untouched.

**Observations (faithful, not defects):**
- `c-vt-encode-key.golden` is **0 bytes** (exit 0). The program encodes a
  Ctrl+C press under the Kitty keyboard protocol (`GHOSTTY_KITTY_KEY_ALL`) and
  `fwrite`s the result; the reference encoder writes **zero bytes** for this
  event. The golden faithfully captures that; the port must also emit exactly 0
  bytes here. (Flagged for the differential harness so a 0-byte result isn't
  mistaken for a capture failure.)
- `c-vt.golden` and `c-vt-static.golden` are byte-identical (same sha256
  `9504c1b2…`): both exercise the same OSC-title-parse example, dynamic vs static
  linking — identical output is expected.

---

## Category 2 — terminfo  (6 goldens)

**Dir:** `terminfo/`
**Source:** the terminfo/termcap the reference ships in
`$TI` (`zig-out/share/terminfo`), built from
`/Users/gael/projects/ghostty/src/terminfo/ghostty.zig`. Entry names:
`xterm-ghostty` (primary) and aliases `ghostty`, `Ghostty`.

| golden | what it is | how produced |
|---|---|---|
| `ghostty.terminfo`        | human-readable terminfo **source** as shipped       | `cp $TI/ghostty.terminfo` |
| `ghostty.termcap`         | termcap **source** as shipped                       | `cp $TI/ghostty.termcap` |
| `xterm-ghostty.infocmp`   | decompiled `xterm-ghostty` entry                    | `infocmp -A $TI xterm-ghostty` |
| `ghostty.infocmp`         | decompiled `ghostty` alias entry                    | `infocmp -A $TI ghostty` |
| `compiled/xterm-ghostty.compiled` | bit-exact compiled entry (R1 artifact)      | `cp $TI/78/xterm-ghostty` |
| `compiled/ghostty.compiled`       | bit-exact compiled entry (alias dir)        | `cp $TI/67/ghostty` |

**Reproduce:**
```
cp $TI/ghostty.terminfo terminfo/ghostty.terminfo
cp $TI/ghostty.termcap  terminfo/ghostty.termcap
infocmp -A $TI xterm-ghostty > terminfo/xterm-ghostty.infocmp
infocmp -A $TI ghostty       > terminfo/ghostty.infocmp
cp $TI/78/xterm-ghostty terminfo/compiled/xterm-ghostty.compiled
cp $TI/67/ghostty       terminfo/compiled/ghostty.compiled
```

**Notes:**
- `infocmp` (from ncurses) and `tic` are available at `/usr/bin/{infocmp,tic}`.
  `infocmp -A` re-run yields a byte-identical decompile (deterministic).
- The two `compiled/*.compiled` files are **byte-identical** (sha256
  `979287b0…`) — they are the same compiled entry placed under both the
  `78/xterm-ghostty` and `67/ghostty` hashed directories.
- **Environment-dependent line:** `*.infocmp` files begin with an `infocmp`
  comment header `# Reconstructed via infocmp from file: <ABSOLUTE PATH under
  /Users/gael/projects/ghostty/...>`. That single comment line embeds this
  machine's path; the actual terminfo capability body below it is
  machine-independent. The harness should compare from the second line, OR
  compare the `.terminfo` source / `compiled` entries (which carry no such
  header) instead. **R2 caveat:** under rustty identity these entries become
  `xterm-rustty` / `rustty`; this corpus is the *ghostty-profile* oracle
  (`identity-oracle`), matched byte-for-byte only by the parity build.

---

## Category 3 — config parse goldens  (6 goldens)

**Dir:** `config/`
**Source:** the prebuilt `ghostty` app binary (`$GB`). All goldens are
stdout-only, exit 0, empty stderr, and verified byte-stable across re-runs.

| golden | command | bytes | notes |
|---|---|---:|---|
| `show-config.default.golden`      | `$GB +show-config --default=true`            | 27530 | code-derived defaults; **machine-independent** (no abs paths, `config-file=` empty) |
| `show-config.default-docs.golden` | `$GB +show-config --default=true --docs=true`| 177712 | defaults + per-option doc comments. The one `com.mitchellh.ghostty` string is a **static doc string** for the `class` default, not machine state |
| `list-keybinds-default.golden`    | `$GB +list-keybinds --default=true`          | 3581  | default keybind table |
| `list-actions.golden`             | `$GB +list-actions`                          | 1308  | all keybind actions (static) |
| `version.golden`                  | `$GB +version`                               | 303   | pinned-build identity: `1.3.2-main-+4749c4e93`, Zig 0.15.2, mode `.Debug`, runtime `.none`, Metal/CoreText. R2: under rustty these strings flip to rustty values |
| `help.golden`                     | `$GB +help` (== `$GB --help`)                | 1426  | top-level usage + action list |

**Reproduce:**
```
$GB +show-config --default=true                 > config/show-config.default.golden
$GB +show-config --default=true --docs=true     > config/show-config.default-docs.golden
$GB +list-keybinds --default=true               > config/list-keybinds-default.golden
$GB +list-actions                               > config/list-actions.golden
$GB +version                                    > config/version.golden
$GB +help                                       > config/help.golden
```

**Why no user/sample-config-file parse goldens (TODO-1):**
The macOS app binary's `+show-config` action accepts **only** its own four flags
(`--default`, `--changes-only`, `--docs`, `--no-pager`). Passing any config key
on the CLI (e.g. `--font-size=16`, `--config-file=…`, `--config-default-files=false`)
makes the action **exit 1 with no output** — CLI config-key injection into
actions is not supported on the macOS app build ("On macOS, launching the
terminal emulator from the CLI is not supported", per `+help`). Running
`+show-config --changes-only` (or any non-`--default` form) instead loads the
**real user's** macOS app config and emits machine-dependent `[sentry] … /Users/…`
debug lines on stderr — non-reproducible. An attempt to isolate via a temp
`$HOME`/`$XDG_CONFIG_HOME` still leaked the user's macOS app preferences
(`theme = Vibrant Ink`) and sentry paths, so that variant was **discarded** (not
stored). Therefore sample-config-file parse goldens are deferred until a
`zig-out/bin/ghostty` CLI binary (app runtime `.none`, no sentry) is available —
the rustty `+show-config` itself will fill this gap during the port. The
`--default` goldens above fully cover the *default-config* serialization oracle,
which is machine-independent.

---

## Category 4 — vttest / esctest conformance  (0 goldens — TODO-2)

Probed PATH and common install locations (`/opt/homebrew/bin`,
`/usr/local/bin`, `/Applications`):

| tool | status |
|---|---|
| `vttest`  | **ABSENT** (not on PATH, not in homebrew/usr-local) |
| `esctest` / `esctest.py` / `esctest2` | **ABSENT** |

`brew` is present (`/opt/homebrew/bin/brew`) but neither tool is installed; no
network install was performed in this task. **No corpora produced; recorded as a
TODO, not a failure** (per task instructions). When stood up, esctest goldens
should be driven against the rustty terminal (its conformance output is the
oracle) and vttest used interactively/scripted.

---

## TODO summary

- **TODO-1 (config sample-file parse goldens):** blocked — no `zig-out/bin/ghostty`
  CLI binary; the macOS app `+show-config` rejects CLI config keys / config-file
  args (exit 1) and otherwise leaks machine-dependent user config + sentry stderr.
  Default-config goldens captured; sample-file parse goldens deferred to the
  rustty CLI.
- **TODO-2 (vttest/esctest):** both absent from this machine. Install
  (`brew install vttest`; clone esctest) and capture conformance goldens in a
  later pass.
- **TODO-3 (cmake c-vt examples):** `c-vt-cmake{,-static,-cross}` were not built
  (no `cmake`; they also `FetchContent` from GitHub `main`, not the pin) — they
  demonstrate CMake consumption, not VT behavior, so they are not parity oracles.

## Determinism / integrity

Every `cvt/*.golden` and every `config/*.golden` and `terminfo/*.infocmp` was
re-generated a second time and compared by sha256 — all byte-stable. Full sha256
list: see `MANIFEST.sha256` (sibling file) or run:
```
( cd /Users/gael/projects/rustty/.port/phase-0/corpora && \
  find cvt terminfo config -type f \
    \( -name '*.golden' -o -name '*.terminfo' -o -name '*.termcap' \
       -o -name '*.infocmp' -o -name '*.compiled' \) | sort | xargs shasum -a 256 )
```
