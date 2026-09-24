#!/usr/bin/env python3
"""G-ARTIFACT: only rustty-vt-c may mark the process as the lib artifact.

Parity S6-F2, controller Decision A. The reference compiles its terminal
module once per artifact, each copy with its own comptime
`terminal_options.artifact` (`.ghostty` for every exe, `.lib` for
libghostty-vt). Cargo compiles rustty-vt once per build selection, so the port
keeps the comptime `options::ARTIFACT` at `Ghostty` and adds a process-wide
RUN-TIME value, `options::runtime_artifact()`, that `rustty-vt-c` sets to
`Lib` in `sys::install_log_backend` (the first thing every `*_new` does).
Today it decides mode 5522's `.disabled`; the panic hook (`panic.rs`) is
installed in the same place.

That design is sound only while the app tier can never set the value: the app
binary must not contain a caller of the setter, so it stays `Ghostty` in every
build selection. This gate pins it:

  CALL   `set_runtime_artifact_lib` is called only from files under
         `crates/rustty-vt-c/src/` (the definition in rustty-vt's options.rs
         and mentions inside `//` comments do not count);
  PRESENT at least one such call exists, inside `install_log_backend` in
         `crates/rustty-vt-c/src/sys.rs` (anti-vacuity: a gate whose only
         caller vanished would stay green while the lib artifact silently
         became the app);
  STORE  `RUNTIME_ARTIFACT` is written (`.store(` / `.swap(` /
         `.compare_exchange` / `.fetch_`) only in rustty-vt's options.rs.

Scans every tracked `*.rs` file (`git ls-files`), so a caller in a test, an
example or xtask is caught too.

Usage: check_runtime_artifact_setter.py              # scan the checkout (binding)
       check_runtime_artifact_setter.py --self-test  # prove each RED arm and the GREEN arm
Exit 0 GREEN, 1 RED.
"""
import os
import re
import subprocess
import sys

SETTER = "set_runtime_artifact_lib"
DEF_FILE = "crates/rustty-vt/src/options.rs"
CALLER_DIR = "crates/rustty-vt-c/src/"
REQUIRED_CALLER = "crates/rustty-vt-c/src/sys.rs"
REQUIRED_FN = "install_log_backend"
STORE_RE = re.compile(r"\bRUNTIME_ARTIFACT\s*\.\s*(store|swap|compare_exchange\w*|fetch_\w+)\s*\(")
SETTER_RE = re.compile(r"\b" + SETTER + r"\b")


def code_part(line):
    """The line with any `//` comment removed (string literals are not
    special-cased: the setter name never appears inside one)."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def enclosing_fn(lines, idx):
    """Name of the nearest `fn` declared at or above line `idx`."""
    for j in range(idx, -1, -1):
        m = re.search(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)", code_part(lines[j]))
        if m:
            return m.group(1)
    return None


def check(files):
    """files: {repo-relative path: text}. Returns a list of RED messages."""
    errors = []
    required_seen = False
    for path in sorted(files):
        lines = files[path].split("\n")
        for i, line in enumerate(lines):
            code = code_part(line)
            if SETTER_RE.search(code):
                is_def = path == DEF_FILE and re.search(r"\bfn\s+" + SETTER + r"\b", code)
                if is_def:
                    pass
                elif not path.startswith(CALLER_DIR):
                    errors.append(f"CALL  {path}:{i + 1}: {SETTER} called outside {CALLER_DIR}")
                elif path == REQUIRED_CALLER and enclosing_fn(lines, i) == REQUIRED_FN:
                    required_seen = True
            if STORE_RE.search(code) and path != DEF_FILE:
                errors.append(f"STORE {path}:{i + 1}: RUNTIME_ARTIFACT written outside {DEF_FILE}")
    if not required_seen:
        errors.append(f"PRESENT: no {SETTER}() call inside {REQUIRED_FN} in {REQUIRED_CALLER}")
    return errors


def load_checkout(root):
    out = subprocess.run(
        ["git", "ls-files", "*.rs"], cwd=root, capture_output=True, text=True, check=True
    ).stdout.split()
    files = {}
    for rel in out:
        p = os.path.join(root, rel)
        if os.path.isfile(p):
            with open(p, encoding="utf-8", errors="replace") as f:
                files[rel] = f.read()
    return files


GOOD = {
    DEF_FILE: "static RUNTIME_ARTIFACT: AtomicU8 = AtomicU8::new(0);\n"
    "pub fn set_runtime_artifact_lib() {\n    RUNTIME_ARTIFACT.store(1, Relaxed);\n}\n",
    REQUIRED_CALLER: "pub fn install_log_backend() {\n"
    "    rustty_vt::options::set_runtime_artifact_lib();\n}\n",
    "crates/rustty-core/src/lib.rs": "// set_runtime_artifact_lib is rustty-vt-c's alone\nfn f() {}\n",
}


def self_test():
    cases = [
        ("GREEN: definition, the one vt-c caller, a comment mention", GOOD, []),
        (
            "RED CALL: an app-tier caller",
            {**GOOD, "crates/rustty-core/src/app.rs": "fn g() { rustty_vt::options::set_runtime_artifact_lib(); }\n"},
            ["CALL"],
        ),
        (
            "RED CALL: a caller in a test outside vt-c",
            {**GOOD, "crates/rustty-vt/src/modes.rs": "#[test]\nfn t() {\n    crate::options::set_runtime_artifact_lib();\n}\n"},
            ["CALL"],
        ),
        (
            "RED PRESENT: the vt-c call removed",
            {**GOOD, REQUIRED_CALLER: "pub fn install_log_backend() {\n    // set_runtime_artifact_lib();\n}\n"},
            ["PRESENT"],
        ),
        (
            "RED PRESENT: the vt-c call moved out of install_log_backend",
            {**GOOD, REQUIRED_CALLER: "pub fn install_log_backend() {}\nfn other() {\n    rustty_vt::options::set_runtime_artifact_lib();\n}\n"},
            ["PRESENT"],
        ),
        (
            "RED STORE: a second writer of the cell",
            {**GOOD, "crates/rustty-vt/src/modes.rs": "fn h() { crate::options::RUNTIME_ARTIFACT.store(1, Relaxed); }\n"},
            ["STORE"],
        ),
    ]
    ok = True
    for name, files, want in cases:
        got = check(files)
        kinds = sorted({e.split()[0].rstrip(":") for e in got})
        passed = kinds == sorted(want)
        ok &= passed
        print(f"  {'PASS' if passed else 'FAIL'}  {name}  (got {kinds or 'GREEN'})")
    # The binding scan must itself see the real caller (the checkout is GREEN
    # only if PRESENT found it, which check() already requires).
    print("G-ARTIFACT self-test", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    root = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
    if "--self-test" in sys.argv[1:]:
        return self_test()
    errors = check(load_checkout(root))
    if errors:
        for e in errors:
            print(e)
        print(f"G-ARTIFACT RED ({len(errors)} finding(s))")
        return 1
    print("G-ARTIFACT GREEN: set_runtime_artifact_lib is called only by rustty-vt-c's install_log_backend")
    return 0


if __name__ == "__main__":
    sys.exit(main())
