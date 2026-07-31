#!/usr/bin/env python3
"""P3-W9 T1 step 2 — hoist rustty-vt's inlined URL-regex fixture out of its
`#[cfg(all(test, feature = "oniguruma"))]` test module to crate-public module
scope, so the byte-identity gate in rustty-config can run under DEFAULT
features (CI never enables `oniguruma`)."""

p = "crates/rustty-vt/src/string_map.rs"
lines = open(p).read().split("\n")

# 1-based 216..318 inclusive -> 0-based 215..318
block = lines[215:318]
assert block[0].strip().startswith("// PORT NOTE (config-layering fixture)"), block[0]
assert block[-1].strip() == "}", repr(block[-1])
assert lines[224].strip() == "mod url {", repr(lines[224])

rest = lines[:215] + lines[318:]
ded = [(l[4:] if l.startswith("    ") else l) for l in block]
ded = [
    l.replace("mod url {", "pub mod url {", 1) if l.strip() == "mod url {" else l
    for l in ded
]

extra = [
    "//",
    "// PORT NOTE (why this fixture is NOT `#[cfg(test)]`-gated): `rustty-config`'s",
    "// `url::regex()` and this copy are two independent transcriptions of ONE oracle",
    "// constant (`src/config/url.zig`'s `pub const regex`), and nothing detected a",
    "// one-sided edit — the standing fidelity hole area G's OQ-4 names, and the exact",
    "// hazard oracle 9fc2a3085 (delete `trailing_spaces_at_eol`) walked into. The gate",
    "// is `rustty_config::url`'s `regex_is_byte_identical_to_the_vt_copy`, which",
    "// byte-compares the two produced strings. That gate MUST run in the default CI",
    "// lane: `.port/tools/ci.sh` never passes `--features oniguruma` (verified: `grep",
    "// -rn oniguruma .port/tools/ci.sh` -> 0 hits), so anything behind that feature is",
    "// exercised by NO CI test. Hence this module is compiled unconditionally and is",
    "// `pub` so the config-side gate can reach it (`cfg(test)` is not transitive).",
]
idx = [i for i, l in enumerate(ded) if l.strip() == "pub mod url {"][0]
ded = ded[:idx] + extra + ded[idx:]

# Insert at module scope, immediately BEFORE the `#[cfg(all(test, ...))]` attribute
# on `mod tests` (0-based 208).
assert rest[208].startswith('#[cfg(all(test, feature = "oniguruma"))]'), rest[208]
out = rest[:208] + ded + [""] + rest[208:]
open(p, "w").write("\n".join(out))
print("ok")
