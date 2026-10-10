# Round 2: per-platform status (macOS, Linux GTK, FreeBSD, Windows, WASM, libvt)

Research date 2026-10-08. Read-only on the repo, on `~/.claude`, on the rustty-stats cache and on the
Linux VM (`ssh rvm`: `ls`, `cat` and `grep` over the in-progress P8-W9 T2 log dir, no builds).
Provenance tags: **M** = MEASURED (command given), **D** = DOCUMENTED (path given), **E** = ESTIMATED.

Data files:
- `data/r2-platforms-matrix.csv`: the per-platform table below, one row per platform.
- `data/r2-platforms-gtk-linux-timeline.csv`: GTK/Linux milestones with hashes and the host each ran on.
- `data/r2-platforms-rvm-commit-attribution.csv`: every commit 2026-07-15..08-25, with flags for "hash
  appears in an rvm transcript commit result", "rvm transcript activity within 15 min before" and
  "touches Linux paths".
- `data/r2-platforms-rvm-lanes-2026-10-08.csv`: today's P8-W9 T2 lanes on rvm (start, seconds, rc).

## 1. Headline: the "not yet live" note is stale

`AGENTS.md` says the GTK app is "not yet live". That sentence was written on **2026-07-15** in
`648d1afeb` ("top-level project files port"), two days before any Linux work, as Rustty's adaptation of
Ghostty's line "GTK (Linux and FreeBSD) app: `src/apprt/gtk`". Nobody has touched that line since. **M**:
`git log -S'not yet live' -- AGENTS.md` returns only `648d1afeb`. Ghostty's own `AGENTS.md` has no such
note (**M**: `grep -n gtk ~/projects/ghostty/AGENTS.md`).

GTK has worked in a real window since 2026-07-18. The author tested it by hand on GNOME (Wayland), X11
(Xfce) and sway. Automated live-window tests have run on every phase since then. The article should not
repeat "not yet live". BRIEF §17 needs correcting (see brief changes).

## 2. Per-platform table

| Platform | Builds | Runs | Tested by hand | Test / sanitizer lanes there | Last evidence | Honest one-line claim |
|---|---|---|---|---|---|---|
| **macOS** (arm64, Swift/AppKit app over the embedded C API, Metal) | Yes: workspace + `macos/build-rustty.sh` (xcodebuild BUILD SUCCEEDED) | Yes: first pixels 07-09 (`74c57204f`), runnable 07-12 (`d1b4df097`) | Yes. It is the porting host. The menu-shortcut parity harness matches on 51 actions, byte for byte | Full `ci.sh`; Miri SB on 38 crates, G6c, Tree Borrows; ASan 9,850; LSan 9,850; TSan 4,380; XCTest 404/0; perf 14/14 | 2026-10-08 (`3b6e760f2`, `1d22ba80d`) | The main platform. It builds, runs, and carries all the Miri evidence and most of the sanitizer evidence. |
| **Linux GTK4/libadwaita** (aarch64 Fedora 44 VM, Wayland + X11, OpenGL) | Yes: `build` + `clippy -D warnings --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland`, release build, Flatpak | Yes: a live terminal in a GTK window since 07-18 (`9bc62e574`), on software GL (llvmpipe) only | Yes. The author tested 07-18..07-23 on GNOME 50 Wayland, an Xfce X11 session and a real sway session. Later checks covered nested KWin 6.7.3, headless mutter and the GlobalShortcuts portal | Workspace tests with GTK features: 6,748 pass (10-08); G-GTKFEAT (232 GTK-gated tests); 18/18 live GTK KATs in headless sway; LSan 10,111; MSan 6,183; HWASan 9,987; TSan 4,428; Valgrind 6,614; LSan-GTK; examples; esctest | 2026-10-08 (`25f8da30c`, unmerged branch `phase-8/w9t2v`; `1d22ba80d` on main) | The GTK app is ported and works in a Linux VM under Wayland and X11. It was tested by hand and by live-window tests. It has never run on real GPU hardware or on x86_64 Linux. |
| **FreeBSD** | Partly: `rustty-xev` `cargo check` + `clippy` for `x86_64-unknown-freebsd` from macOS; `cargo tree` resolves the GTK graph; `rustty-os` needs a FreeBSD sysroot | No | No | None on FreeBSD | 2026-09-24 (V23) | Ported, but never built natively or run. Only type-checked from macOS. |
| **Windows** (Win 11 ARM64 VM `rwin`, `x86_64-pc-windows-msvc` under x64 emulation; also `-gnu`) | Yes: workspace `build` + `clippy -D warnings` for MSVC (excluding libxml2-sys, fontconfig-sys, sentry-sys); vt-c release; font-shaper for GNU | Library tier only. `rustty.exe` builds but opens no window, which matches Ghostty | No GUI exists to test. ConPTY was driven in a unit test. The `c-vt-effects` C example output is byte-identical to the golden | xev 42-45, termio 65 lib + integration, vaxis 157, vt `kitty::` 393 + `mem::` 4, font-shaper 133 (GNU). No full-workspace test run (V26) | 2026-10-08 (`1d22ba80d`) | It builds, lints and passes targeted tests on Windows. Like Ghostty at this pin, there is no Windows app. |
| **WASM** (`wasm32-unknown-unknown`: `rustty-wasm`, `rustty-web-canvas`; `rustty-vt-c`) | Yes: the freestanding build is in the default `ci.sh` (`3d3210a64`, restored 10-07 after a regression, N526); G-XTARGET also covers `wasm32-wasip1` | Yes, headless. A browser gate ran in headless SwiftShader and in live Chrome on ANGLE Metal, with byte-identical output (`c94051557`, 07-30). A logging site was executed in Node (`2ca1a947f`, 09-10) | One live Chrome run (07-30) | A browser gate with negative controls. The WebGL stub was removed to match upstream (`279df0dd8`). The libxev `wasi_poll` backend has never been executed (V8). Sizes were not measured | 2026-10-07 | The terminal core and the font demo compile to WebAssembly and were run in a browser and in Node. There is no web terminal app, and Ghostty has none either. |
| **libvt C API** (`rustty-vt-c`) | Yes: macOS, Linux, Windows MSVC (release), wasm32 | Yes. The C examples build and run (29 on macOS, 25 on Linux in Phase 6; G-EXAMPLES GREEN on rvm 10-08). On Windows the `c-vt-effects` output is byte-identical to the golden | n/a | vt-c tests 586-604 per host; Miri on vt-c; header and ABI gates | 2026-10-08 | It works on all three desktop OSes and in WebAssembly, with example programs checked against goldens. |

Sources for the table: **D** `.port/phase-8/README.md` (finale cross-host table),
`.port/phase-8/w9/cross-host-build-at-a60e9e2a5.txt`, `.port/phase-7/README.md` lines 88-92,
`.port/phase-6/README.md` lines 104-140, `.port/phase-2/VERIFICATION_DEBT.md` (V2, V3, V5, V8-V12, V23,
V26, V30-V33), `data/bugs-verification-gates.csv`. The 6,748 figure is **M**:
`ssh rvm 'grep "test result" ~/projects/p8w9t2-logs/fulllinux.log' | awk '{p+=$4;f+=$6}…'` gives 6,748
passed and 0 failed. The record is `25f8da30c:.port/phase-8/w9/full-linux-lane-at-a60e9e2a5.txt`.

## 3. How VM-originated work was identified

All commits share one author email and one timezone (**M**: `git log --all --format=%ad
--date=format:%z | sort | uniq -c`, which gives 2,984 commits at -0700 and 10 at +0000). The timezone
cannot separate the hosts, so four independent signals were used:

1. **The author's own prompts on the Mac** (`~/.claude/history.jsonl`, **M**):
   - 07-16 19:07 -0700: "my main focus is to port the remaining GTK app. what do you advise from a macOS laptop"
   - 07-16 19:11: "help on setting up a GNOME-on-Wayland desktop VM"
   - 07-16 20:11: "My VM is running at … 192.168.64.3 … Setup the build environment for me"
   - 07-18 02:50: "I just pulled those linux changes … ensure that MacOS still builds fine"
   - 07-23: "Linux VM is running at …192.168.64.4"
2. **Two VMs.** The handoff plan `docs/superpowers/plans/2026-07-18-linux-gtk-finish-with-3d-accel.md`
   (**D**) was written by "the session that built L0–L5 on a VM where 3D acceleration was disabled". It
   records that `main` was "~76 commits ahead of `origin/main` and NOT pushed (this VM had no SSH key)".
   **M**: `git log --all --since='2026-07-16 20:58 -0700' --until='2026-07-18 02:14 -0700' | wc -l` gives
   **77 commits**, from the first Linux commit `85d19023a` to the handoff `8d38349c8`. All are non-merge.
   VM #1 had no Claude transcript cached. The second VM, `rvm` (Fedora 44 Workstation aarch64, 8 vCPU,
   about 10 GB RAM, GTK 4.22.4, libadwaita 1.9.2, `virtio_gpu` with no virgl), took over on 07-18. **M**:
   `ssh rvm 'cat /etc/os-release; nproc; free -g; rpm -q gtk4 libadwaita'`.
3. **rvm Claude Code transcripts** (`rustty-stats/.cache/rvm`, 79 sessions from 07-18 10:40 to
   08-22 17:28 UTC, 165 prompts). The Bash tool calls that ran `git commit` were parsed, and their results
   were matched to `[branch hash]`:
   - 40 hashes were captured. 37 resolve in the Mac repo, all between 07-18 and 07-22 local time. The other
     3 were later amended.
   - Many commits used `git commit -q`, which prints no hash. A wider test was used for those: "rvm
     transcript activity within the 15 minutes before the commit". **75 commits** pass it (07-18: 14 of 16,
     07-19: 51 of 51, 07-22: 10 of 10). 56 of them touch Linux paths.
   - The rvm transcripts contain no `git commit` after 07-23. From 07-23 on, the VM was driven over
     `ssh rvm` from Mac sessions, so the commits were made on the Mac.
   - **M**: scratchpad script `rvm_commits.py` / `attrib.py`. The output is
     `data/r2-platforms-rvm-commit-attribution.csv`.
4. **`/home/gael/` paths in commit content.** **M**: `git log --all -G'/home/gael/'`. The 07-16..07-19
   plans cite the oracle as `/home/gael/projects/ghostty`, which is the Linux home directory, not the
   macOS one.

Result (**E**, built from the measured pieces above):
- The GTK/Linux port itself was written **on the Linux VMs by Claude Code sessions running there**: about
  77 commits on VM #1 (07-16 to 07-18) and about 75 on rvm (07-18 to 07-23).
- Everything Linux after 07-23 is Mac-hosted sessions driving rvm over ssh: verification, sanitizers,
  Flatpak and the cross-host builds.
- These figures also correct round 1's usage note that rvm-made commits were "double-counted". The overlap
  is the roughly 75 commits from 07-18 to 07-23, not the whole 07-18 to 08-14 window. The 77 VM #1 commits
  have no recovered cost at all, so they belong in the lost-transcript estimate.

## 4. What the author tested by hand on Linux (rvm prompts, M)

All of these come from `~/.cache/rvm/history.jsonl`, with UTC timestamps:
- **07-18.** "In the rustty window, typing doesn't work." The cause was placeholder overlays that started
  visible, fixed in `5ce0d0269`. Then "there seems to be a crash after right click", fixed in `486793623`.
  A close-tab and close-window check followed. Then: "Ghostty shows shortcuts for various menu options, but
  Rustty doesn't" (`abb3c51a3`).
- **07-18/19.** Copy and paste. The window title differed from Ghostty ("[redacted]" vs "~"). Ctrl+L,
  and `clear` failing with "'xterm-rustty': unknown terminal type".
- **07-19.** X11 checks in three tiers, ending with "I've restarted in Xfce, run the tier 3 test" and then
  "X11 is all validated". Next came the quick terminal in a real sway session, with Ghostty run first as
  the baseline. The author's verdict: "Is works exactly as Ghostty to me".
- **07-23.** "Why does my Rustty version have a striped title bar while Ghostty does not?" This led to the
  `loadRuntimeCss` port (`cddffa093`), then "commit and push".
- **07-31.** "Run the build and run the rustty app". This is the last interactive launch request in the rvm
  history. Later Linux work used automated live-window tests in headless sway.

These are good material for the article: a human found real bugs by using the GTK app next to Ghostty
(input swallowed, crash on right-click, missing menu accelerators, title-format drift, terminfo name).

## 5. GTK/Linux timeline (hashes; full list in data/r2-platforms-gtk-linux-timeline.csv)

| Date | Hash | Host | Milestone |
|---|---|---|---|
| 07-15 | `648d1afeb` | macOS | AGENTS.md gets the "not yet live" note (before any Linux work) |
| 07-16 | `85d19023a` | VM #1 | First Linux commit: -sys build breaks on aarch64 glibc / GCC 16 |
| 07-16 | `f32352ab1` | VM #1 | GTK roadmap + L1 plan (xev epoll/io_uring/Dynamic) |
| 07-17 | `3dad6d7a2` | VM #1 | **GTK first light**: a GLArea renders a live terminal frame (KAT) |
| 07-17 | `269845ec9` | VM #1 | GL-ATTACH: draws into GDK's GLArea (first light on screen) |
| 07-18 | `9bc62e574` | VM #1 | A live terminal in the window (Window/Tab/Surface) |
| 07-18 | `8d38349c8` | VM #1 | Handoff: 77 unpushed commits; move to a VM with 3D acceleration |
| 07-18 | `5ce0d0269` | rvm | First fix from hand-testing: input swallowed |
| 07-18 | `42749b5e3` | rvm | Linux/GTK manual test plan (parity checklist vs Ghostty) |
| 07-19 | `4006e602a` | rvm | X11 + Wayland winproto arms; X11 validated in Xfce |
| 07-19 | `631742691` | rvm | GTK inspector (Dear ImGui over OpenGL) KAT |
| 07-19 | `a62f11f87` | rvm | Quick terminal (layer-shell), checked against Ghostty in sway |
| 07-19 | `9de16be19` | rvm | libxev File/TCP/UDP/ThreadPool complete on Linux |
| 07-22 | `cddffa093` | rvm | `loadRuntimeCss` (striped title bar); pushed from rvm |
| 07-29 | `2690b7139` | rvm via ssh | GlobalShortcuts portal round trip on GNOME 50 |
| 07-29 | `07b64dbef` | rvm via ssh | X11 residual + live verification |
| 07-29 | `b12ea5ec8` | rvm via ssh | `set_urgent` against a real headless mutter |
| 07-31 | `2dfee2973` | rvm via ssh | Wayland global hotkeys landed whole |
| 08-26 | `b8e6a9eb8` | rvm via ssh | Flatpak built, installed and run ("Rustty 0.1.0") |
| 09-08 | `d328db1f7` | rvm via ssh | Phase 6 Linux lane: 3 defects macOS cannot see |
| 09-29 | `5af770b30` | rvm via ssh | Canonical headless-sway live GTK test harness |
| 10-02 | `1a13056ae` | rvm via ssh | HWASan GREEN (9,987 tests); LSan/MSan GREEN that week |
| 10-07 | `f31fcc032` | rvm via ssh | Valgrind GREEN (6,614 tests) |
| 10-07 | `4a32358ae` | rvm via ssh | TSan GREEN on Linux (4,428); LSan on a live GTK surface |
| 10-08 | `1d22ba80d` | rvm + rwin + macOS | Cross-host build GREEN at pin `a60e9e2a5` |
| 10-08 | `25f8da30c` | rvm via ssh | P8-W9 T2 lanes (unmerged): 6,748 tests pass; MSan/TSan/HWASan provisionally RED |

## 6. Limits the article must state

- **No real GPU on Linux.** All Linux rendering ran on llvmpipe (software GL 4.6 core over virtio-gpu, no
  virgl). The project treats llvmpipe as a valid correctness environment: glyph output was checked against
  Ghostty 1.3.1 on the same stack. Performance and behaviour under hardware GL were never checked (**D**
  VERIFICATION_DEBT V2). The 07-18 handoff hoped the second VM would have hardware GL. It did not.
- **aarch64 Linux only.** Both VMs are aarch64 on Apple silicon. The x86_64 Ubuntu `ci.yml` legs are
  written but have never run on GitHub's runners (**D** V25, V30). HWASan is aarch64-only and runs only on
  rvm (**D** V31). No x86_64 Linux run was found (**E**, from the absence of any record).
- **Desktop residuals.**
  - KDE: the decoration globals were confirmed under a nested KWin, but slide and visual blur remain open
    (V3).
  - X11: a native-session physical keypress and the KWin-on-X11 items are open (V5).
  - The RTL layout sign-off is open (V7).
- **Sanitizer state at the newest pin is mid-run.**
  - Phase 7 recorded LSan, MSan, HWASan, TSan and Valgrind GREEN on rvm.
  - Today's P8-W9 T2 run at `a60e9e2a5` is in progress on an unmerged branch:
    - ASan, LSan and LSan-GTK are GREEN.
    - MSan, TSan and HWASan are RED, with provisional causes: an uninstrumented libEGL reached by a test new
      at this pin, an Oniguruma global-init race plus Mesa-internal races (not yet established), and a
      host-built `libz.a` in the archive check.
    - Full `ci.sh` was still running when this was captured.
  - **M**: `ssh rvm 'cat ~/projects/p8w9t2-logs/*.rc'`; record `25f8da30c:.port/phase-8/w9/findings-vm.md`.
  - The article should date its sanitizer claims to Phase 7 (2026-10-02..07) unless T2 closes first.
- **FreeBSD.** Never built natively and never run (V23). The macOS cross-check stops in `rustty-os`'s build
  script, which needs the target `termios.h`.
- **Windows.**
  - Ghostty at the pin has no Windows app: its embedded init returns `UnsupportedOSForCApi`. So "no Windows
    window" is parity, not a gap (**D** V9 W-2).
  - The full-workspace test run under x64 emulation was deferred by the author for time (V26).
  - The Windows bring-up found real defects: per BRIEF #36, 16 upstream-defect entries, 5 of them crashes.
- **WASM.** Executed in a browser and in Node, but the `wasi_poll` event-loop backend was never executed
  (V8). Binary sizes were never measured.

## 7. Suggested article copy (plain, per platform)

- **macOS:** "The primary platform. The Swift app runs on the ported core and passes its 404 XCTest app tests. All
  Miri lanes and most sanitizer runs happen here."
- **Linux:**
  - "The GTK4/libadwaita app is a native Rust port of Ghostty's GTK runtime. It shows a live terminal in a
    window, tested by hand on GNOME (Wayland), Xfce (X11) and sway in an aarch64 Fedora VM, plus 18
    automated live-window tests and the full sanitizer matrix."
  - "It has not been run on a real GPU or on x86_64."
- **FreeBSD:** "The FreeBSD code paths are ported and type-checked, but have never been built or run on
  FreeBSD."
- **Windows:** "The libraries build and pass targeted tests on Windows 11. As with Ghostty today, there is
  no Windows app."
- **Web:** "The VT core and a font-rendering demo compile to WebAssembly and were run in Chrome and Node."
- **libvt:** "The C library works on macOS, Linux and Windows. Its example programs are checked against
  golden output."

## Fact-check

Independent re-derivation on 2026-10-08 (own commands; read-only apart from this section).

1. **AGENTS.md "not yet live" note is stale.** CONFIRMED. `git log -S'not yet live' -- AGENTS.md` returns only
   648d1afeb (2026-07-15 19:34 -0700), the day before the first Linux commit (85d19023a, 07-16 20:59), so the line
   was never touched again. 3dad6d7a2 = 07-17 19:30 "FIRST LIGHT"; 9bc62e574 = 07-18 02:00 "a live terminal in the
   window". Ghostty's AGENTS.md:32 reads "GTK (Linux and FreeBSD) app: `src/apprt/gtk`", with no such note.
2. **VM #1: 77 commits, unpushed, no 3D; rvm took over 07-18.** CONFIRMED. `git log --all` over the window = 77
   (first 85d19023a 07-16 20:59, last 8d38349c8 07-18 02:13, the handoff plan itself); 0 of them touch `macos/`. The
   handoff doc says "~76 commits ahead of origin/main and NOT pushed", "no SSH key", "3D acceleration was
   disabled". rvm-side count: my own parse of `git commit` tool results in the rvm cache (`[branch hash]` lines)
   finds 40 short hashes, 37 of which resolve in the repo (3 do not, likely amended/rebased), matching the CSV's
   37 `rvm_transcript_hash=True`. Commits 07-18 02:14 to 07-23 00:00 -0700 = 76 (07-18: 14, 07-19: 51, 07-21: 1,
   07-22: 10); 75 have rvm activity within 15 min; the odd one out is 23af48edc, a dependabot bump. "About 75" holds.
3. **No rvm-transcript commits after 07-23.** CONFIRMED. Last hash in my parse: cddffa0 at 2026-07-23T05:57Z
   (07-22 22:57 -0700). rvm history.jsonl spans 07-18 10:40Z to 08-22 17:27Z; prompts after 07-23 are 07-31 (disk
   analysis, /tmp, "Run the build and run the rustty app"), 08-14 (space cleanup, /login, /resume) and 08-22
   (clean up merged worktrees and branches). Later Linux-verification commits (2690b7139 07-29, b8e6a9eb8 08-26) do
   not appear in the rvm transcripts.
4. **Mac-side prompts that started the Linux effort.** CONFIRMED (times -0700): 18:57 Linux build-env question,
   19:07 "my main focus is to port the remaining GTK app. what do you advise from a macOS laptop", 19:11
   "help on setting up a GNOME-on-Wayland desktop VM", 20:11 "My VM is running at [redacted] ...". The
   20:11 prompt also contains the VM login password. Do not quote it past "192.168.64.3".
5. **Hands-on GTK testing on rvm against Ghostty.** CONFIRMED. rvm prompts (-0700): 07-18 12:07 "crash after
   right click", 07-18 19:27 "'xterm-rustty': unknown terminal type. It works in Ghostty", 07-18 18:11 title
   drift ("[redacted]" vs "~"), 07-19 03:23 "X11 is all validated" (after "I've restarted in Xfce, run the tier
   3 test"), 07-19 09:06 "Is works exactly as Ghostty to me" (the sway quick-terminal test, right after the
   Ghostty baseline in a real sway session), 07-22 22:02 "striped title bar". Fix commits 5ce0d0269 (input
   swallowed), 486793623 (crash on interaction), abb3c51a3 (menu accelerators), all 07-18, and cddffa093 (07-22,
   loadRuntimeCss) exist. The 07-18 16:20 prompt also reports copy/paste not working; it could join the list.
6. **rvm hardware/software.** CONFIRMED live over ssh: Fedora Linux 44 (Workstation Edition), 6.19.10-300.fc44.aarch64,
   nproc 8, 9,906 MiB total RAM, gtk4-4.22.4, libadwaita-1.9.2, the `virtio_gpu` module loaded. V2 says "llvmpipe
   (software) over virtio-gpu, with no hardware path".
7. **Never on real GPU or x86_64.** CONFIRMED as documented: V2 (llvmpipe only), V25 and V30 (CI jobs and x86_64
   sanitizers job "never run on GitHub's runners"), V31 (HWASan aarch64-only). VM #1 was aarch64 too (85d19023a
   "aarch64 glibc / GCC 16").
8. **Full Linux workspace run 6,748 / 0 / 431 s.** CONFIRMED. Summed `test result` lines on rvm
   (`~/projects/p8w9t2-logs/fulllinux.log`, 207 result lines): 6,748 passed, 0 failed; `fulllinux.rc` = 0; log
   mtime 2026-10-08 08:09. `25f8da30c:.port/phase-8/w9/full-linux-lane-at-a60e9e2a5.txt` says "wall: 431 s".
   25f8da30c is only on `phase-8/w9t2v`, so it is unmerged.
9. **18 of 18 live GTK tests; 232 gated tests.** CONFIRMED. `.port/phase-8/README.md:93` and
   `.port/phase-7/README.md:90` both say "PASS, 18 of 18". The g-gtkfeat record on phase-8/w9t2v says "272 listed with
   features, 40 without -> 232 gated", G-GTKFEAT GREEN.
10. **Linux sanitizer lanes.** CONFIRMED. bugs-verification-gates.csv: LSan rvm 10,111 (10-02), MSan rvm 6,183
    (10-02), Valgrind rvm 6,614 (10-07); 1a13056ae subject: HWASan 9,987 tests GREEN on rvm (10-02); V32 says "G-TSAN
    GREEN, 4,428 tests ... 0 reports, 20.75 min" on rvm, discharged 10-07; V33 says LSan-GTK ran on rvm only (P7-W24,
    10-07). Note that the CSV's TSan row is the macOS run (4,380), not rvm.
11. **Newest-pin sanitizer state (provisional).** CONFIRMED live: `.rc` files show asan 0, lsan 0, lsangtk 0, msan 1,
    tsan 1, hwasan 1 (hwasanzlib 3 = the SAN_ONLY isolated run, BLOCKED by design). findings-vm.md: N620 MSan in
    `/lib64/libEGL.so.1`; N621 HWASan "All 10,182 tests passed; only the archive check failed" (host `libz.a`); N622
    TSan with races in `onig_initialize`/`onig_reg_init` and inside `libEGL_mesa`, "NOT established" whether Mesa
    false positive or Rustty. `pgrep` on rvm still shows `.port/tools/ci.sh` running. Other `.rc` files: esctest 3
    then esctest2 0, examples/rel/sanself/gtkfeat/egl 0.
12. **Further desktop checks.** CONFIRMED. 2690b7139 (07-29) "live GlobalShortcuts ... on rvm GNOME 50"; V3 table
    "nested KWin 6.7.3"; b12ea5ec8 (07-29) set_urgent "against REAL (headless) mutter"; b8e6a9eb8 (08-26) "the
    flatpak is VERIFIED — built, installed and run on Linux".
13. **Linux found 3 macOS-invisible defects in Phase 6.** CONFIRMED with a small wording fix. `.port/phase-6/README.md`
    says "three HARD-gate failures that macOS structurally cannot see". Item 1 reads "all 25 C/C++ examples were
    unbuildable on Linux" (C/C++, not just C). d38b014ba and d591a9857 are both dated 09-08. d328db1f7's subject says
    "Linux exposed three pre-existing macOS-invisible defects".
14. **FreeBSD.** CONFIRMED. V23 heading: "type-checked from macOS only, never built natively or EXECUTED" [2026-09-24].
15. **Windows.** CONFIRMED. VERIFICATION_DEBT W-1/W-2 notes: xev 42/42 (09-27) and 45 in the P8 finale table, termio
    65 lib tests, vaxis 157 passed, all on Win 11 build 26200 ARM64 targeting x86_64-pc-windows-msvc under emulation.
    Phase-8 finale table: kitty 393, mem 4, font-shaper 133 (x64 GNU), c-vt-effects output byte-identical. The
    cross-host record says workspace build + vt-c release + GNU font-shaper all RC=0. The V26 full-workspace test
    run is user-deferred. The author prompt 09-27 reads "The app seems to build on windows, but opening it doesn't
    seem to work." Not re-checked here: the Ghostty `UnsupportedOSForCApi` claim (taken from V9 W-2 as cited).
16. **WASM.** MOSTLY CONFIRMED. The c94051557 message says "(ANGLE Metal, Apple M4 Pro) matched headless SwiftShader
    byte-for-byte". 2ca1a947f (09-10) is the Node run, 3d3210a64 (10-07) the freestanding wasm in default ci.sh,
    and 279df0dd8 (10-07) the WebGL stub removal. V8 says wasi_poll was "never EXECUTED". UNVERIFIABLE: "sizes
    unmeasured". I found no size measurement in VERIFICATION_DEBT, but absence was not proven repo-wide.
    Side note: the commit names the Mac as an "Apple M4 Pro" (the author says "MacBook Pro M4").
17. **macOS at newest pin.** CONFIRMED. `.port/phase-8/README.md` finale table: "BUILD SUCCEEDED; TEST SUCCEEDED
    (404 passed, 0 failed)" and "51 actions byte-identical". The cross-host record has a macOS leg, and its header
    says "all legs RC=0, zero warnings". The Phase 7 table had 403.
18. **Double-count correction (ESTIMATED).** CONFIRMED as derived. 75 of 76 commits in 07-18..07-22 (-0700) show
    rvm activity, and 37 are hash-confirmed. The 77 VM #1 commits predate the rvm cache (it starts 07-18 10:40Z), so
    no recovered transcript covers them.

Net: no value needed correction. Two wording refinements: item 13 should say "C/C++ examples"; item 10 should
not take the TSan rvm figure from the CSV (that row is macOS). Item 4's source prompt contains a password, so do
not reproduce it verbatim.
