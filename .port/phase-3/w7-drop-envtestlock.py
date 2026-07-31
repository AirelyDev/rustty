#!/usr/bin/env python3
"""P3-W7 T3 (area-E `E15`) — delete the `ENV_TEST_LOCK` test-only serialization.

It existed ONLY because rustty's `desktop`/`xdg`/`homedir` tests mutated the
PROCESS environment with `setenv`/`unsetenv` while `cargo test` runs tests in
parallel. `E15` removes every one of those calls (the tests build local
environment maps now, as upstream's do), so the lock has no remaining purpose —
and the plan is explicit that porting `E15` must DELETE the hazard rather than
paper over it with a global test mutex."""
import sys

P = "crates/rustty-os/src/lib.rs"
src = open(P, encoding="utf-8").read()

start = src.index(
    '// PORT NOTE (test-only glue, R1 "no-loss, not no-addition"): the reference\'s Zig test'
)
end = src.index("#[cfg(test)]\nmod tests {")

replacement = """// PORT NOTE (P3-W7 T3 / area-E `E15`): a test-only `ENV_TEST_LOCK` used to live
// here. It existed only because this crate's `desktop`/`xdg`/`homedir` tests
// mutated the PROCESS environment with `setenv`/`unsetenv` — which Zig's serial
// test runner tolerates and Rust's parallel `libtest` runner does not (`setenv`
// is not thread-safe against a concurrent `getenv` in glibc or Apple libc; see
// the recorded `parallel-cargo-test-exposes-c-init-races` lesson). Upstream
// `e8525c0fd` + `f8e13f31e` rewrote those tests to build LOCAL environment maps
// and deleted every `setenv`/`unsetenv` call, so the hazard is gone at the
// source and the lock is deleted with it rather than kept as a workaround.

"""

src = src[:start] + replacement + src[end:]
open(P, "w", encoding="utf-8").write(src)
print("patched", P)
sys.exit(0)
