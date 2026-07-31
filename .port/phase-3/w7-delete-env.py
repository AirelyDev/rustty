#!/usr/bin/env python3
"""P3-W7 T4 Step 2 — delete `crates/rustty-os/src/env.rs` (upstream deleted
`src/os/env.zig`, −178) and migrate every remaining consumer.

Mapping (oracle `e8525c0fd`):

    getEnvMap(alloc)          -> global.environMap()          [flatpak case gone]
    getenv / getenvNotEmpty   -> environ_map.get(key) or global.environ().getPosix/getAlloc
    setenv / unsetenv         -> gone from os/; the only remaining in-process
                                 mutator is os/locale.zig, which declares its own
                                 file-local `extern "c"` decls (done in T3). The
                                 GTK application arm follows that precedent.
    appendEnv/appendEnvAlways -> moved into termio/Exec       [done in T2]
    prependEnv                -> moved into shell_integration [done in T2]
"""
import sys

EDITS = {
    "crates/rustty-apprt/src/embedded.rs": [
        (
            """        // const alloc = self.app.core_app.alloc;
        // var env = try internal_os.getEnvMap(alloc);
        // errdefer env.deinit();
        //
        // (PORT NOTE: rustty `getEnvMap` is infallible and returns an owned map; on
        // non-darwin `env` is never mutated below — silence `unused_mut` there.)
        #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_mut))]
        let mut env = rustty_os::env::getEnvMap();""",
            """        // const alloc = self.app.core_app.alloc;
        // var env = try global.environMap();
        // errdefer env.deinit();
        //
        // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` was deleted with
        // `src/os/env.zig` (`e8525c0fd`); the replacement is the captured
        // environment block's map. rustty's `environ_map` is infallible and
        // returns an owned map; on non-darwin `env` is never mutated below —
        // silence `unused_mut` there.
        #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_mut))]
        let mut env = rustty_global::environ_map();""",
        ),
    ],
    "crates/rustty-apprt/src/gtk/class/surface.rs": [
        (
            """        let mut env = rustty_os::env::getEnvMap();""",
            """        // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` -> `global.environMap()`
        // (`src/os/env.zig` was DELETED by `e8525c0fd`).
        let mut env = rustty_global::environ_map();""",
        ),
    ],
    "crates/rustty-core/src/surface.rs": [
        (
            """                    internal_os::env::getEnvMap()""",
            """                    // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` ->
                    // `global.environMap()` (`src/os/env.zig` was DELETED by
                    // `e8525c0fd`). This is the site area E §3.1 identified as
                    // the live non-UTF-8 PANIC; it is byte-typed now.
                    rustty_global::environ_map()""",
        ),
    ],
    "crates/rustty-cli/src/pager.rs": [
        (
            """    let rustty_var = rustty_os::env::getenv(b"RUSTTY_PAGER").unwrap_or(None);""",
            """    // PORT NOTE (P3-W7 T4): `internal_os.getenv` was deleted with
    // `src/os/env.zig`; callers use the captured block's accessors.
    let environ = rustty_global::environ();
    let rustty_var = environ.get_alloc(b"RUSTTY_PAGER");""",
        ),
        (
            """    let pager_var = rustty_os::env::getenv(b"PAGER").unwrap_or(None);""",
            """    let pager_var = environ.get_alloc(b"PAGER");""",
        ),
        (
            """        if let Some(v) = &rustty_var {
            break 'cmd if !v.value.is_empty() {
                Some(rustty_global::environ::os_string_from_bytes(&v.value))
            } else {
                None
            };
        }
        if let Some(v) = &pager_var {
            break 'cmd if !v.value.is_empty() {
                Some(rustty_global::environ::os_string_from_bytes(&v.value))
            } else {
                None
            };
        }""",
            """        if let Some(v) = &rustty_var {
            break 'cmd if v.is_empty() {
                None
            } else {
                Some(rustty_global::environ::os_string_from_bytes(v))
            };
        }
        if let Some(v) = &pager_var {
            break 'cmd if v.is_empty() {
                None
            } else {
                Some(rustty_global::environ::os_string_from_bytes(v))
            };
        }""",
        ),
    ],
}

for path, edits in EDITS.items():
    src = open(path, encoding="utf-8").read()
    for old, new in edits:
        if old not in src:
            print(f"MISS in {path}: {old.splitlines()[0][:70]}", file=sys.stderr)
            sys.exit(1)
        src = src.replace(old, new, 1)
    open(path, "w", encoding="utf-8").write(src)
    print("patched", path)
