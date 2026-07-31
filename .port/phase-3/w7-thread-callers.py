#!/usr/bin/env python3
"""P3-W7 T3 (area-E `E15`) — feed the new explicit-environment-map parameter at
every remaining call site. Upstream does exactly this at its own call sites:

    var environ_map = try global.environMap();
    defer environ_map.deinit();
    ... internal_os.expandHome(&environ_map, path, &buf) ...

(see `src/config/Config.zig:4684`, `:5415`, `src/crash/dir.zig:9`).
"""
import sys

NOTE = (
    "// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream feeds these the captured\n"
    "// environment via `var environ_map = try global.environMap();` at the call\n"
    "// site (`src/config/Config.zig:4684`, `:5415`).\n"
)

EDITS = {
    "crates/rustty-config/src/theme.rs": [
        (
            """                match internal_os::xdg::config(internal_os::xdg::Options {""",
            """                match internal_os::xdg::config(
                    &rustty_global::environ_map(),
                    internal_os::xdg::Options {""",
        ),
    ],
    "crates/rustty-config/src/path.rs": [
        (
            """match rustty_os::homedir::expandHome(&path, &mut buf) {""",
            """match rustty_os::homedir::expandHome(
                        &rustty_global::environ_map(),
                        &path,
                        &mut buf,
                    ) {""",
        ),
    ],
    "crates/rustty-config/src/config_struct.rs": [
        (
            """                            if let Ok(Some(home)) = rustty_os::homedir::home(&mut buf) {""",
            """                            if let Ok(Some(home)) = rustty_os::homedir::home(
                                &rustty_global::environ_map(),
                                &mut buf,
                            ) {""",
        ),
        (
            """        let expected: Vec<u8> = rustty_os::expandHome(b"~/projects/ghostty", &mut buf)""",
            """        let expected: Vec<u8> = rustty_os::expandHome(
            &rustty_global::environ_map(),
            b"~/projects/ghostty",
            &mut buf,
        )""",
        ),
    ],
    "crates/rustty-config/src/value_types.rs": [
        (
            """            let expanded = match rustty_os::homedir::expandHome(&path, &mut buf) {""",
            """            let expanded = match rustty_os::homedir::expandHome(
                &rustty_global::environ_map(),
                &path,
                &mut buf,
            ) {""",
        ),
        (
            """            match rustty_os::homedir::expandHome(b"~/projects/ghostty", &mut buf) {""",
            """            match rustty_os::homedir::expandHome(
                &rustty_global::environ_map(),
                b"~/projects/ghostty",
                &mut buf,
            ) {""",
        ),
    ],
    "crates/rustty-config/src/file_load.rs": [
        (
            """        Ok(rustty_os::xdg::config(rustty_os::xdg::Options {
            subdir: Some(&subdir),
            ..Default::default()
        })?)""",
            """        Ok(rustty_os::xdg::config(
            &rustty_global::environ_map(),
            rustty_os::xdg::Options {
                subdir: Some(&subdir),
                ..Default::default()
            },
        )?)""",
        ),
    ],
    "crates/rustty-termio/src/shell_integration.rs": [
        (
            """        if let Some(home) = homedir::home(&mut home_buf).map_err(Error::Homedir)? {""",
            """        // PORT NOTE (P3-W7 T3 / area-E `E14`): upstream's `setupBash` now does
        //   var environ_map = try global.environMap();
        //   defer environ_map.deinit();
        //   homedir.home(&environ_map, &home_buf)
        let environ_map = rustty_global::environ_map();
        if let Some(home) =
            homedir::home(&environ_map, &mut home_buf).map_err(Error::Homedir)?
        {""",
        ),
    ],
    "crates/rustty-cli/src/new_window.rs": [
        (
            """                expandHome(stripped, &mut expandhome_buf).map_err(CheckArgError::ExpandHome)?;""",
            """                expandHome(&rustty_global::environ_map(), stripped, &mut expandhome_buf)
                    .map_err(CheckArgError::ExpandHome)?;""",
        ),
    ],
    "crates/rustty-cli/src/ssh_cache/disk_cache.rs": [
        (
            """        let state_dir = xdg::state(xdg::Options {""",
            """        let state_dir = xdg::state(
            &rustty_global::environ_map(),
            xdg::Options {""",
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
