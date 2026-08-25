# Phase 5 — upstream resync (pin → oracle head, 2026-08-25)

Phase 5 closes `12967b68f..683d8db64` with **no deferrals**, immediately after Phase 4's
finale (same advance-first doctrine; both gates were GREEN at `12967b68f` minutes before
this advance — `w0/*-before-at-12967b68f.txt`).

## Range

```
from  12967b68f7d46bdbfb2cfffb6768332fb9db68c0   2026-08-17  libghostty: option to retain continuations on snapshot decode (#13878)
to    683d8db643b95cf229bfb5fe9fab9ae677920343   2026-08-25  renderer: release GPU resources for hidden surfaces (macOS) (#14017)
```

245 commits (169 non-merge). **155 files, +25,534 / −3,360.** No upstream deletions.

G1b arrived RED: **295 dropped tests in 5 modules** (`w0/g1b-after-at-683d8db64.txt`):
kitty **202** (NEW subsystems: clipboard protocol `kitty/clipboard*.zig` ×5, drag-and-drop
`kitty/dnd*.zig` ×5, graphics_animation, graphics_pixel), osc **47** (NEW
`osc/kitty_metadata.zig` + `osc/parsers/kitty_desktop_notification.zig`), stream_terminal
**38**, input_paste **7** (NEW `src/terminal/paste.zig`), parser **1**.
G3b after-state: `w0/g3b-after-at-683d8db64.txt`. **Both MUST be GREEN at the finale.**

## Artifacts

Same layout as `.port/phase-4/` (UPSTREAM_DIFF.patch / _STAT / COMMITS / w0 gate evidence /
assessments/ / wf-*.js). Binding rules: `.port/phase-4/README.md` §"Binding rules" applies
verbatim (range-diff end state, re-vet don't assume, negative controls, R1/R5/R7/R9/DG5,
SB raw-ptr discipline, oracle tree IS the target).

## Area breakdown

src/terminal 51 files (kitty new subsystems dominate; also paste.zig, osc parsers,
stream/stream_terminal churn), macos 13 (AppleScript key-event tests, KeyboardLayout),
include 10, apprt 6, pkg/wuffs 6 (vendored C shim headers stdlib.h/string.h), termio 5,
renderer 4 (incl. the head commit: GPU resource release for hidden surfaces), cli 4, po 4,
lib 3, example 3, simd 2, os 2, config 2, .github 2, singletons (Surface.zig, lib_vt.zig,
input, font, build, apprt.zig, pkg/afl, typos, valgrind.supp).
