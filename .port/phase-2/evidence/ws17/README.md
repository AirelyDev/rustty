# WS17 evidence

Raw, unedited command output captured while discharging `.port/phase-2/VERIFICATION_DEBT.md`.
Every claim in that ledger dated 2026-07-29 points at a file here. Nothing in this
directory is hand-written prose about what "would" happen — it is captured output only.

| File | Item | What it is |
|---|---|---|
| `v2-hw-gl-probe-controls.txt` | V2 | `hw_gl_probe.sh --self-test` (both fixture arms) + the live VM run (FAIL) |
| `v2-llvmpipe-vs-ghostty.txt` | V2 | Same-box rustty-vs-ghostty llvmpipe narrowing run (narrowing only — NOT a GPU sign-off) |
| `v3-kde-globals-under-kwin.txt` | V3 | Bound-globals set under a live nested KWin 6.7.3, plus the GNOME/mutter negative control failing |
| `v4-set-urgent-real-mutter.txt` | V4 | `set_urgent` KAT against a real headless mutter on its own socket, plus the no-compositor control |

Fixtures used by `hw_gl_probe.sh` live beside the script in
`.port/tools/gates/fixtures/`, not here, because the script depends on them at runtime:

* `glxinfo-llvmpipe.txt` — a REAL capture from this VM (negative control).
* `glxinfo-hw-radeonsi.txt` — **SYNTHETIC**, and labelled as such inside the file. It
  exists only to prove `hw_gl_probe.sh`'s PASS arm is reachable on a box with no GPU. It
  is not evidence about any hardware, and V2 stays open until a real box exits 0.
