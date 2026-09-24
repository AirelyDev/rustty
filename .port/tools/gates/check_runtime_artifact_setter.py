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

That design is sound only while the app tier can never set the value. Two
things must hold: no code outside rustty-vt-c can write the cell, and no
app-tier crate can reach rustty-vt-c (whose exports and `pub` items all run the
setter, directly or through `install_log_backend`). The gate proves both, over
the tracked sources and the workspace manifests:

  CALL    `set_runtime_artifact_lib` is named only in files under
          `crates/rustty-vt-c/src/`, apart from its definition in rustty-vt's
          options.rs (mentions inside `//` comments do not count);
  PRESENT at least one such call exists, inside `install_log_backend` in
          `crates/rustty-vt-c/src/sys.rs` (anti-vacuity: a gate whose only
          caller vanished would stay green while the lib artifact silently
          became the app);
  STORE   the cell `RUNTIME_ARTIFACT` is written exactly once, by
          `RUNTIME_ARTIFACT.store(` inside `fn set_runtime_artifact_lib` in
          options.rs. Every other mention in options.rs must be the (non-`pub`)
          `static` declaration or a `.load(`; any other use (a second store, a
          swap, a reference handed out) is RED, and so is any mention of the
          cell outside options.rs;
  DEPS    the workspace packages that depend on `rustty-vt-c` (any
          dependency kind, any target, renamed or not; `cargo metadata
          --no-deps`) are a subset of {rustty-web-canvas}, itself a lib-vt
          artifact; and `rustty-vt-c` is a workspace package (anti-vacuity).

Scans every tracked `*.rs` file (`git ls-files`), so a caller in a test, an
example or xtask is caught too.

Known limits (lexical, not a parser):
  - a `//` inside a string literal earlier on a line is taken as a comment, so
    the rest of that line is not scanned (e.g. `f("http://x", set_...())`);
  - an identifier built by a macro (`paste!`, `concat_idents!`) is invisible;
  - DEPS sees workspace members only; a crate outside the workspace that
    depends on `crates/rustty-vt-c` by path is not scanned.

Usage: check_runtime_artifact_setter.py              # scan the checkout (binding)
       check_runtime_artifact_setter.py --self-test  # prove each RED arm and the GREEN arm
Exit 0 GREEN, 1 RED.
"""
import json
import os
import re
import subprocess
import sys

SETTER = "set_runtime_artifact_lib"
CELL = "RUNTIME_ARTIFACT"
DEF_FILE = "crates/rustty-vt/src/options.rs"
CALLER_DIR = "crates/rustty-vt-c/src/"
REQUIRED_CALLER = "crates/rustty-vt-c/src/sys.rs"
REQUIRED_FN = "install_log_backend"
VTC_PACKAGE = "rustty-vt-c"
VTC_DEPENDENTS_ALLOWED = {"rustty-web-canvas"}
SETTER_RE = re.compile(r"\b" + SETTER + r"\b")
CELL_RE = re.compile(r"\b" + CELL + r"\b")
CELL_DECL_RE = re.compile(r"^\s*static\s+" + CELL + r"\s*:")
CELL_LOAD_RE = re.compile(r"\b" + CELL + r"\s*\.\s*load\s*\(")
CELL_STORE_RE = re.compile(r"\b" + CELL + r"\s*\.\s*store\s*\(")


def code_part(line):
    """The line with any `//` comment removed (string literals are not
    special-cased; see "Known limits")."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def enclosing_fn(lines, idx):
    """Name of the nearest `fn` declared at or above line `idx`."""
    for j in range(idx, -1, -1):
        m = re.search(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)", code_part(lines[j]))
        if m:
            return m.group(1)
    return None


def check_sources(files):
    """files: {repo-relative path: text}. Returns a list of RED messages."""
    errors = []
    required_seen = False
    stores_in_setter = 0
    for path in sorted(files):
        lines = files[path].split("\n")
        for i, line in enumerate(lines):
            code = code_part(line)
            where = f"{path}:{i + 1}"
            if SETTER_RE.search(code):
                is_def = path == DEF_FILE and re.search(r"\bfn\s+" + SETTER + r"\b", code)
                if is_def:
                    pass
                elif not path.startswith(CALLER_DIR):
                    errors.append(f"CALL  {where}: {SETTER} named outside {CALLER_DIR}")
                elif path == REQUIRED_CALLER and enclosing_fn(lines, i) == REQUIRED_FN:
                    required_seen = True
            n_mentions = len(CELL_RE.findall(code))
            if not n_mentions:
                continue
            if path != DEF_FILE:
                errors.append(f"STORE {where}: {CELL} named outside {DEF_FILE}")
                continue
            if CELL_DECL_RE.search(code):
                n_mentions -= 1
            n_loads = len(CELL_LOAD_RE.findall(code))
            n_stores = len(CELL_STORE_RE.findall(code))
            if n_stores:
                if enclosing_fn(lines, i) == SETTER:
                    stores_in_setter += n_stores
                else:
                    errors.append(f"STORE {where}: {CELL} stored outside fn {SETTER}")
            if n_mentions - n_loads - n_stores > 0:
                errors.append(
                    f"STORE {where}: {CELL} used other than by its non-pub static, a .load( or the setter's .store("
                )
    if stores_in_setter != 1:
        errors.append(f"STORE {DEF_FILE}: {stores_in_setter} {CELL}.store( in fn {SETTER}, expected exactly 1")
    if not required_seen:
        errors.append(f"PRESENT: no {SETTER}() call inside {REQUIRED_FN} in {REQUIRED_CALLER}")
    return errors


def check_deps(packages):
    """packages: `cargo metadata` package objects (name, dependencies[].name)."""
    errors = []
    names = {p["name"] for p in packages}
    if VTC_PACKAGE not in names:
        errors.append(f"DEPS  {VTC_PACKAGE} is not a workspace package (the arm would be vacuous)")
    for p in sorted(packages, key=lambda p: p["name"]):
        if p["name"] in VTC_DEPENDENTS_ALLOWED:
            continue
        for d in p.get("dependencies", []):
            if d.get("name") == VTC_PACKAGE:
                kind = d.get("kind") or "normal"
                errors.append(
                    f"DEPS  {p['name']} depends on {VTC_PACKAGE} ({kind}"
                    + (f", as {d['rename']}" if d.get("rename") else "")
                    + f"); allowed: {sorted(VTC_DEPENDENTS_ALLOWED)}"
                )
    return errors


def check(files, packages):
    return check_sources(files) + check_deps(packages)


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


def load_packages(root):
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return json.loads(out)["packages"]


GOOD = {
    DEF_FILE: "static RUNTIME_ARTIFACT: AtomicU8 = AtomicU8::new(0);\n"
    "const RUNTIME_ARTIFACT_LIB: u8 = 1;\n"
    "pub fn runtime_artifact() -> u8 {\n    RUNTIME_ARTIFACT.load(Relaxed)\n}\n"
    "pub fn set_runtime_artifact_lib() {\n    RUNTIME_ARTIFACT.store(RUNTIME_ARTIFACT_LIB, Relaxed);\n}\n",
    REQUIRED_CALLER: "pub fn install_log_backend() {\n"
    "    rustty_vt::options::set_runtime_artifact_lib();\n}\n",
    "crates/rustty-core/src/lib.rs": "// set_runtime_artifact_lib is rustty-vt-c's alone\nfn f() {}\n",
}

GOOD_PACKAGES = [
    {"name": "rustty-vt", "dependencies": []},
    {"name": VTC_PACKAGE, "dependencies": [{"name": "rustty-vt", "kind": None}]},
    {"name": "rustty-web-canvas", "dependencies": [{"name": VTC_PACKAGE, "kind": None}]},
    {"name": "rustty-core", "dependencies": [{"name": "rustty-vt", "kind": None}]},
]


def self_test():
    def with_dep(pkgs, name, dep):
        return [({**p, "dependencies": p["dependencies"] + [dep]} if p["name"] == name else p) for p in pkgs]

    cases = [
        ("GREEN: definition, a load, the one vt-c caller, a comment mention, web-canvas on vt-c", GOOD, GOOD_PACKAGES, []),
        (
            "RED CALL: an app-tier caller",
            {**GOOD, "crates/rustty-core/src/app.rs": "fn g() { rustty_vt::options::set_runtime_artifact_lib(); }\n"},
            GOOD_PACKAGES,
            ["CALL"],
        ),
        (
            "RED CALL: a caller in a test outside vt-c",
            {**GOOD, "crates/rustty-vt/src/modes.rs": "#[test]\nfn t() {\n    crate::options::set_runtime_artifact_lib();\n}\n"},
            GOOD_PACKAGES,
            ["CALL"],
        ),
        (
            "RED CALL: a re-export of the setter from rustty-vt",
            {**GOOD, "crates/rustty-vt/src/lib.rs": "pub use options::set_runtime_artifact_lib as mark_lib;\n"},
            GOOD_PACKAGES,
            ["CALL"],
        ),
        (
            "RED PRESENT: the vt-c call removed",
            {**GOOD, REQUIRED_CALLER: "pub fn install_log_backend() {\n    // set_runtime_artifact_lib();\n}\n"},
            GOOD_PACKAGES,
            ["PRESENT"],
        ),
        (
            "RED PRESENT: the vt-c call moved out of install_log_backend",
            {**GOOD, REQUIRED_CALLER: "pub fn install_log_backend() {}\nfn other() {\n    rustty_vt::options::set_runtime_artifact_lib();\n}\n"},
            GOOD_PACKAGES,
            ["PRESENT"],
        ),
        (
            "RED STORE: a writer of the cell outside options.rs",
            {**GOOD, "crates/rustty-vt/src/modes.rs": "fn h() { crate::options::RUNTIME_ARTIFACT.store(1, Relaxed); }\n"},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: a second writer inside options.rs (review-T-H2 bypass a)",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE] + "pub fn mark_lib_for_host() {\n    RUNTIME_ARTIFACT.store(1, Relaxed);\n}\n"},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: a second store inside the setter",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE].replace(
                "RUNTIME_ARTIFACT.store(RUNTIME_ARTIFACT_LIB, Relaxed);",
                "RUNTIME_ARTIFACT.store(RUNTIME_ARTIFACT_LIB, Relaxed);\n    RUNTIME_ARTIFACT.store(0, Relaxed);")},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: a swap in options.rs",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE] + "fn s() { RUNTIME_ARTIFACT.swap(1, Relaxed); }\n"},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: a reference to the cell handed out of options.rs",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE] + "pub fn cell() -> &'static AtomicU8 { &RUNTIME_ARTIFACT }\n"},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: the cell made pub",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE].replace("static RUNTIME_ARTIFACT", "pub static RUNTIME_ARTIFACT")},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED STORE: the setter's store removed",
            {**GOOD, DEF_FILE: GOOD[DEF_FILE].replace("RUNTIME_ARTIFACT.store(RUNTIME_ARTIFACT_LIB, Relaxed);", "")},
            GOOD_PACKAGES,
            ["STORE"],
        ),
        (
            "RED DEPS: an app-tier crate depends on rustty-vt-c (review-T-H2 bypass b)",
            GOOD,
            with_dep(GOOD_PACKAGES, "rustty-core", {"name": VTC_PACKAGE, "kind": None}),
            ["DEPS"],
        ),
        (
            "RED DEPS: a renamed dev-dependency on rustty-vt-c",
            GOOD,
            with_dep(GOOD_PACKAGES, "rustty-core", {"name": VTC_PACKAGE, "kind": "dev", "rename": "vtc"}),
            ["DEPS"],
        ),
        (
            "RED DEPS: rustty-vt-c missing from the workspace (anti-vacuity)",
            GOOD,
            [p for p in GOOD_PACKAGES if p["name"] != VTC_PACKAGE],
            ["DEPS"],
        ),
    ]
    ok = True
    for name, files, packages, want in cases:
        got = check(files, packages)
        kinds = sorted({e.split()[0].rstrip(":") for e in got})
        passed = kinds == sorted(want)
        ok &= passed
        print(f"  {'PASS' if passed else 'FAIL'}  {name}  (got {kinds or 'GREEN'})")
    print("G-ARTIFACT self-test", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    root = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
    if "--self-test" in sys.argv[1:]:
        return self_test()
    errors = check(load_checkout(root), load_packages(root))
    if errors:
        for e in errors:
            print(e)
        print(f"G-ARTIFACT RED ({len(errors)} finding(s))")
        return 1
    print(
        "G-ARTIFACT GREEN: set_runtime_artifact_lib is called only by rustty-vt-c's install_log_backend, "
        f"{CELL} has one writer (the setter), and only {sorted(VTC_DEPENDENTS_ALLOWED)} depend on {VTC_PACKAGE}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
