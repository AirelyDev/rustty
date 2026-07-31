# Rejected theories and unresolved leads

These items were investigated but are not findings unless explicitly labeled
verification debt.

## Rejected as stale or false-positive

- The assertion-count RED for `os::desktop environment` compares an old Ghostty
  test shape. Current source behavior is represented in Rustty.
- Frozen missing-test names (`fromHex`, OS env/args, old PageList/scroll names,
  hash-map “tombstones”) were deleted, renamed, or moved in current Ghostty and
  have current Rust counterparts. The live test audit reports no unexplained gap.
- `Backend::childExitedAbnormally` is a production `unimplemented!`, but neither
  current tree calls it; both surface implementations own the real abnormal-exit
  handling. No reachable effect was found.
- Many termio/xev/font comments still say `DEFERRED` even though live bodies and
  focused tests have since landed. Marker text alone was never promoted.
- OpenGL `Program.setUniform` has no reference caller, and the reference's
  uninstantiated `Buffer.Binding.attribute` body is itself stale/broken. Their
  absence does not affect current Ghostty behavior.
- The no-op winproto does not imply all global shortcuts are broken; the portal
  fallback remains available. The finding is loss of native/default integration.
- GTK `cgroup_path()` has no current production consumer in either paired path.
- A first parallel full-corpus differential attempt reused shared temporary names.
  Its apparent divergences were a harness race; isolated repetitions matched. All
  results from that contaminated run were discarded.

## Verification debt, not promoted

- Native Linux GTK/OpenGL execution, including a real Wayland and X11 session.
- Native FreeBSD compilation/execution after the known GTK, renderer, and xev
  blockers are removed.
- Native Windows builds after the 25 Rust source errors, with a real Windows SDK,
  and execution of the ConPTY path.
- External distro packaging. An out-of-repository packager could consume Rustty's
  Linux templates; the documented gap is repository-supplied processing/install
  wiring.
- Long-running coverage-guided fuzz results. Neither a Rust fuzz target nor a
  campaign exists in this checkout.

