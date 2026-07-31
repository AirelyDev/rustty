#!/bin/sh
set -eu

RUSTTY=${RUSTTY_REPO:-/Users/gael/projects/rustty}
GHOSTTY=${GHOSTTY_REPO:-/Users/gael/projects/ghostty}

check() {
    label=$1
    file=$2
    pattern=$3
    if rg -q "$pattern" "$file"; then
        printf 'PASS %s\n' "$label"
    else
        printf 'FAIL %s (%s / %s)\n' "$label" "$file" "$pattern" >&2
        exit 1
    fi
}

# FreeBSD selects GTK in both build-config models, but Rustty compiles its GTK
# implementation and launch function on Linux only and retains a reachable
# unimplemented fallback.
check ghostty-freebsd-default-gtk "$GHOSTTY/src/apprt/runtime.zig" '\.linux, \.freebsd => \.gtk'
check rustty-freebsd-default-gtk "$RUSTTY/crates/rustty-build-config/src/apprt.rs" 'Os::Linux \| crate::os::Os::Freebsd => Runtime::Gtk'
check rustty-gtk-module-linux-only "$RUSTTY/crates/rustty-apprt/src/lib.rs" '#\[cfg\(all\(target_os = "linux", feature = "gtk"\)\)\]'
check rustty-run-gtk-linux-only "$RUSTTY/crates/rustty/src/main_rustty.rs" '#\[cfg\(all\(target_os = "linux", feature = "gtk"\)\)\]'
check rustty-missing-backend-panics "$RUSTTY/crates/rustty/src/main_rustty.rs" 'unimplemented!\('

# The default Linux product enables GTK but neither native protocol backend.
check rustty-default-gtk-only "$RUSTTY/crates/rustty/Cargo.toml" '^default = \["gtk"\]$'
check rustty-wayland-opt-in "$RUSTTY/crates/rustty/Cargo.toml" '^wayland = \["gtk", "rustty-apprt/wayland"\]$'
check rustty-x11-opt-in "$RUSTTY/crates/rustty/Cargo.toml" '^x11 = \["gtk", "rustty-apprt/x11"\]$'

# Runtime placeholders that are reachable from the shipped GTK action/config
# paths.
saved_language_body=$(sed -n '/pub fn saved_language(&self)/,/^    }/p' \
    "$RUSTTY/crates/rustty-apprt/src/gtk/class/application.rs")
if printf '%s\n' "$saved_language_body" | rg -q '^        None$'; then
    printf 'PASS %s\n' rustty-saved-language-hardcoded-none
else
    printf 'FAIL %s\n' rustty-saved-language-hardcoded-none >&2
    exit 1
fi
check rustty-app-quit-warning-only "$RUSTTY/crates/rustty-apprt/src/gtk/class/application.rs" 'quit requires confirmation; the CloseConfirmationDialog lands with Task 24'
check rustty-app-quit-window-noop "$RUSTTY/crates/rustty-apprt/src/gtk/class/application.rs" 'let _ = window;'
check rustty-debug-warning-empty "$RUSTTY/crates/rustty-apprt/src/gtk/class/debug_warning.rs" 'pub struct DebugWarning;'

printf 'All static GTK/FreeBSD gap predicates reproduced.\n'
