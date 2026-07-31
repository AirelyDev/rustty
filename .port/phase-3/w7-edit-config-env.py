#!/usr/bin/env python3
"""P3-W7 T4 Step 2 — `rustty-cli`'s `edit_config` `$VISUAL`/`$EDITOR` reads.

`internal_os.getenv` / `internal_os.GetEnvResult` died with `src/os/env.zig`
(`e8525c0fd`); callers read the captured environment block instead.
"""
import sys

P = "crates/rustty-cli/src/edit_config.rs"
src = open(P, encoding="utf-8").read()

EDITS = [
    (
        """use rustty_os::{getenv, GetEnvResult, ShellEscapeWriter};""",
        """use rustty_os::ShellEscapeWriter;""",
    ),
    (
        """    // Get our editor
    // const get_env_: ?internal_os.GetEnvResult = env: {
    let get_env_: Option<GetEnvResult> = 'env: {
        // VISUAL vs. EDITOR: https://unix.stackexchange.com/questions/4859/visual-vs-editor-what-s-the-difference
        // if (try internal_os.getenv(alloc, "VISUAL")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = getenv(b"VISUAL")? {
            if !v.value.is_empty() {
                break 'env Some(v);
            }
            v.deinit();
        }

        // if (try internal_os.getenv(alloc, "EDITOR")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = getenv(b"EDITOR")? {
            if !v.value.is_empty() {
                break 'env Some(v);
            }
            v.deinit();
        }

        // break :env null;
        None
    };
    // defer if (get_env_) |v| v.deinit(alloc); — `get_env_` drops at scope end.
    // const editor: []const u8 = if (get_env_) |v| v.value else "";
    let editor: &[u8] = match &get_env_ {
        Some(v) => v.value.as_slice(),
        None => b"",
    };""",
        """    // Get our editor
    // const get_env_: ?internal_os.GetEnvResult = env: {
    //
    // PORT NOTE (P3-W7 T4): `internal_os.getenv`/`GetEnvResult` died with
    // `src/os/env.zig` (`e8525c0fd`); the read goes through the captured
    // environment block. The `deinit` calls collapse (the Windows-only free the
    // reference's `GetEnvResult.deinit` performed is Drop in Rust), and the
    // values are byte-typed, so a non-UTF-8 `$EDITOR` is no longer silently
    // reported as unset.
    let environ = rustty_global::environ();
    let get_env_: Option<Vec<u8>> = 'env: {
        // VISUAL vs. EDITOR: https://unix.stackexchange.com/questions/4859/visual-vs-editor-what-s-the-difference
        // if (try internal_os.getenv(alloc, "VISUAL")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = environ.get_alloc(b"VISUAL") {
            if !v.is_empty() {
                break 'env Some(v);
            }
        }

        // if (try internal_os.getenv(alloc, "EDITOR")) |v| {
        //     if (v.value.len > 0) break :env v;
        //     v.deinit(alloc);
        // }
        if let Some(v) = environ.get_alloc(b"EDITOR") {
            if !v.is_empty() {
                break 'env Some(v);
            }
        }

        // break :env null;
        None
    };
    // defer if (get_env_) |v| v.deinit(alloc); — `get_env_` drops at scope end.
    // const editor: []const u8 = if (get_env_) |v| v.value else "";
    let editor: &[u8] = match &get_env_ {
        Some(v) => v.as_slice(),
        None => b"",
    };""",
    ),
]

for old, new in EDITS:
    if old not in src:
        print("MISS: " + old.splitlines()[0][:80], file=sys.stderr)
        sys.exit(1)
    src = src.replace(old, new, 1)

open(P, "w", encoding="utf-8").write(src)
print("patched", P)
