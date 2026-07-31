# GTK / FreeBSD audit verification log

Freshly reproduced on 2026-07-31. Commands were read-only with respect to both
repositories; Cargo artifacts were redirected to `/tmp/rustty-audit-gtk-bsd-final`.

## Audit universe

```text
Rustty  5243303fbb2f2cfd0832edfb8fa49d72ce322d81
Ghostty 6ad1fe7d8cbda36c77b337a96c9bea8a77883699
rustc   1.93.1 (01f6ddf75 2026-02-11)
host    aarch64-apple-darwin
```

Installed Rust targets:

```text
aarch64-apple-darwin
wasm32-unknown-unknown
wasm32-wasip1
x86_64-pc-windows-msvc
```

No Linux or FreeBSD standard-library target was installed. A direct
`cargo check -p rustty --target x86_64-unknown-freebsd` exited 101 with
`error[E0463]: can't find crate for std`. Source `cfg` analysis and
target-resolved `cargo tree` were therefore used for those target claims.

## Static reproducer

Command:

```sh
sh .port/codex/scripts/gtk-bsd/check_runtime_gaps.sh
```

Observed result, exit 0:

```text
PASS ghostty-freebsd-default-gtk
PASS rustty-freebsd-default-gtk
PASS rustty-gtk-module-linux-only
PASS rustty-run-gtk-linux-only
PASS rustty-missing-backend-panics
PASS rustty-default-gtk-only
PASS rustty-wayland-opt-in
PASS rustty-x11-opt-in
PASS rustty-saved-language-hardcoded-none
PASS rustty-app-quit-warning-only
PASS rustty-app-quit-window-noop
PASS rustty-debug-warning-empty
All static GTK/FreeBSD gap predicates reproduced.
```

## Focused checks

Command:

```sh
CARGO_TARGET_DIR=/tmp/rustty-audit-gtk-bsd-final \
  cargo test -p rustty-build-config
```

Observed result, exit 0:

```text
running 1 test
test tests::rustc_version_is_captured ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Command:

```sh
CARGO_TARGET_DIR=/tmp/rustty-audit-gtk-bsd-final \
  cargo check -p rustty --no-default-features
```

Observed result, exit 0 after 51.65 seconds. The only warning was the expected
macOS renderer fallback after `xcrun metal` could not precompile the shader.

## Target-resolved dependency controls

The default Linux graph contains the general GTK stack:

```sh
CARGO_TARGET_DIR=/tmp/rustty-audit-gtk-bsd-final \
  cargo tree -p rustty --target x86_64-unknown-linux-gnu \
  -e normal,build,features | rg 'gtk4 v|libadwaita v|gdk4 v'
```

Observed result: exit 0, with `gdk4 v0.11.4`, `gtk4 v0.11.4`, and
`libadwaita v0.9.2` present.

The same default Linux graph has no native protocol backend:

```sh
CARGO_TARGET_DIR=/tmp/rustty-audit-gtk-bsd-final \
  cargo tree -p rustty --target x86_64-unknown-linux-gnu \
  -e normal,build,features | \
  rg 'wayland-client|gdk4-x11|rustty-apprt feature "(wayland|x11)"'
```

Observed result: exit 1, no matches.

The default FreeBSD graph has no native GTK or protocol dependency:

```sh
CARGO_TARGET_DIR=/tmp/rustty-audit-gtk-bsd-final \
  cargo tree -p rustty --target x86_64-unknown-freebsd \
  -e normal,build,features | \
  rg 'gtk4 v|gdk4 v|libadwaita v|wayland-client|gdk4-x11'
```

Observed result: exit 1, no matches. As an adversarial control,
`cargo tree -p rustty --target x86_64-unknown-freebsd -e features -i
rustty-apprt` does show `rustty-apprt feature "gtk"`: the top-level feature is
activated, but its target-gated implementation and native dependencies are
absent. The analogous Linux inverse tree shows `gtk` but neither `wayland` nor
`x11`.

No production build/package file enables Rustty's native protocol features:

```sh
rg -n --glob '!target/**' --glob '!.git/**' --glob '!.port/**' \
  --glob '!docs/**' --glob '!Cargo.lock' -- \
  '--features[ =](wayland|x11)|features = \[[^]]*(wayland|x11)' .
```

Observed result: exit 1, no matches.

No Rust/TOML/shell/workflow/Nix production build file references any Linux
desktop/DBus/systemd/AppStream input or install destination:

```sh
rg -n --glob '*.rs' --glob '*.toml' --glob '*.sh' --glob '*.yml' \
  --glob '*.yaml' --glob '*.nix' --glob '!target/**' --glob '!.git/**' \
  --glob '!.port/**' --glob '!docs/**' \
  'app\.desktop\.in|dbus\.service\.in|systemd\.service\.in|metainfo\.xml\.in|share/applications|share/dbus-1|systemd/user|share/metainfo' .
```

Observed result: exit 1, no matches. `rg --files crates/rustty` independently
showed only `Cargo.toml`, two source files, and two test files; there is no build
script for the executable crate.

## Repository-write control

Before the audit Rustty reported only `?? .port/codex/`; Ghostty was clean. At
the final verification checkpoint Ghostty was still clean, and Rustty still
reported only the allowed `?? .port/codex/` audit tree.
