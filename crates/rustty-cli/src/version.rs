//! rustty-cli::version
//!
//! PORTS: ghostty src/cli/version.zig — the `+version` / `--version` action.
//!
//! PORT STATUS: PARTIAL (ADV-FIX 1, CLI_BLUEPRINT.md §0). This file
//! unconditionally reads `renderer.Renderer` (a Phase-3 leaf: rustty-renderer's
//! `Renderer` is commented out until R-d) and `xev.backend` (unimplemented in
//! rustty-xev; D6). Those two lines emit documented STUB strings until R-d/D6
//! land; the `+version` KAT does NOT assert their content. 0-test file → no test
//! contract breakage. (See DEFERRALS.md "cli C1 — version(partial)".)

// const std = @import("std");
// const build_options = @import("build_options");
// const Allocator = std.mem.Allocator;
// const builtin = @import("builtin");
// const build_config = @import("../build_config.zig");
use rustty_build_config as build_config;
// const internal_os = @import("../os/main.zig");
// const xev = @import("../global.zig").xev;          -> STUB (D6)
// const renderer = @import("../renderer.zig");        -> STUB (R-d)
//
// const gtk_version = @import("../apprt/gtk/gtk_version.zig");
// const adw_version = @import("../apprt/gtk/adw_version.zig");
//   -> only referenced in the comptime `app_runtime == .gtk` block, which is
//      statically false for the headless `.none` build; not imported here.

use crate::rustty::Error;

// pub const Options = struct {};
pub struct Options {}

/// STUB (ADV-FIX 1): `renderer.Renderer` is the concrete GPU renderer TYPE, whose
/// home (rustty-renderer/lib.rs) is a Phase-3 leaf (commented out). Until R-d
/// lands, the renderer type-name is a documented stub. The `+version` KAT does
/// NOT assert this line's content.
const RENDERER_STUB: &str = "(deferred: rustty-renderer — Phase-3 / R-d)";

/// STUB (ADV-FIX 1): `xev.backend` is libxev's selected backend; rustty-xev is
/// unimplemented (D6). Documented stub; the `+version` KAT does NOT assert this.
const LIBXEV_STUB: &str = "(deferred: rustty-xev / libxev — D6)";

/// The reference prints `builtin.zig_version_string` (the Zig compiler version).
/// The Rust port has no Zig toolchain and no build script capturing the rustc
/// version at compile time, so this reports a documented placeholder. 0-test
/// line; not KAT-asserted.
const TOOLCHAIN_VERSION_STUB: &str = "(rust toolchain)";

/// Reference: `build_config.version.build` — the `+BUILD` metadata (commit hash)
/// of the SemanticVersion. The Rust runtime SemanticVersion view drops `+BUILD`
/// (like it drops `-PRE`), so Rustty's "0.1.0" carries no commit hash; this is
/// always `None` and the OSC-8 hyperlink branch below never emits.
fn commit_hash() -> Option<&'static str> {
    None
}

/// The tag name (`{t}`) of a [`build_config::ReleaseChannel`] — lowercase, as the
/// reference's `{t}` formatting produces.
fn release_channel_tag(channel: build_config::ReleaseChannel) -> &'static str {
    match channel {
        build_config::ReleaseChannel::Tip => "tip",
        build_config::ReleaseChannel::Stable => "stable",
    }
}

/// The `version` command is used to display information about Rustty. Recognized
/// as either `+version` or `--version`.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    use std::io::{IsTerminal, Write};

    // var buffer: [1024]u8 = undefined;
    // const stdout_file: std.fs.File = .stdout();
    let stdout_file = std::io::stdout();
    // const tty = stdout_file.isTty();
    let tty = stdout_file.is_terminal();
    // var stdout_writer = stdout_file.writer(&buffer); const stdout = &..interface;
    let mut stdout = std::io::BufWriter::new(stdout_file.lock());

    // if (tty) if (build_config.version.build) |commit_hash| {
    //     try stdout.print("\x1b]8;;https://github.com/ghostty-org/ghostty/commit/{s}\x1b\\", ...);
    // };
    if tty {
        if let Some(commit_hash) = commit_hash() {
            // Reference host: https://github.com/ghostty-org/ghostty/commit/<hash>
            // (R2: the Rustty repo commit-URL is a deferred product-identity value
            // — no repo-URL constant in rustty-identity yet, DEFERRALS.md. This
            // branch never runs since `commit_hash()` is always None for Rustty,
            // so nothing is emitted.)
            write!(stdout, "\x1b]8;;{commit_hash}\x1b\\")?;
        }
    }
    // try stdout.print("Ghostty {s}\n\n", .{build_config.version_string});
    //   R2: "Ghostty" -> the Rustty product name (rustty-identity
    //   APP_NAME_TITLECASE; the oracle prints the capitalized product name here).
    write!(
        stdout,
        "{} {}",
        rustty_identity::APP_NAME_TITLECASE,
        build_config::VERSION_STRING
    )?;
    writeln!(stdout)?;
    writeln!(stdout)?;
    // if (tty) try stdout.print("\x1b]8;;\x1b\\", .{});
    if tty {
        write!(stdout, "\x1b]8;;\x1b\\")?;
    }

    // try stdout.print("Version\n", .{});
    writeln!(stdout, "Version")?;
    // try stdout.print("  - version: {s}\n", .{build_config.version_string});
    writeln!(stdout, "  - version: {}", build_config::VERSION_STRING)?;
    // try stdout.print("  - channel: {t}\n", .{build_config.release_channel});
    writeln!(
        stdout,
        "  - channel: {}",
        release_channel_tag(build_config::release_channel())
    )?;

    // try stdout.print("Build Config\n", .{});
    writeln!(stdout, "Build Config")?;
    // try stdout.print("  - Zig version   : {s}\n", .{builtin.zig_version_string});
    //   PORT NOTE: the Rust port has no Zig toolchain — reported as the analogous
    //   Rust toolchain version (documented stub; see TOOLCHAIN_VERSION_STUB).
    writeln!(stdout, "  - Rust version  : {TOOLCHAIN_VERSION_STUB}")?;
    // try stdout.print("  - build mode    : {}\n", .{builtin.mode});
    writeln!(
        stdout,
        "  - build mode    : {}",
        build_config::mode_string()
    )?;
    // try stdout.print("  - app runtime   : {}\n", .{build_config.app_runtime});
    writeln!(
        stdout,
        "  - app runtime   : {:?}",
        build_config::app_runtime()
    )?;
    // try stdout.print("  - font engine   : {}\n", .{build_config.font_backend});
    writeln!(
        stdout,
        "  - font engine   : {:?}",
        build_config::font_backend()
    )?;
    // try stdout.print("  - renderer      : {}\n", .{renderer.Renderer});
    writeln!(stdout, "  - renderer      : {RENDERER_STUB}")?;
    // try stdout.print("  - libxev        : {t}\n", .{xev.backend});
    writeln!(stdout, "  - libxev        : {LIBXEV_STUB}")?;
    // PORT NOTE: the GTK block below is a comptime branch
    // (`build_config.app_runtime == .gtk`), statically false for the headless
    // `.none` build this milestone targets, so it is compiled out. It ports with
    // the GTK apprt (Phase-3), where the `gtk` runtime and its
    // `internal_os.getKernelInfo`/`desktopEnvironment` + `gtk_version`/
    // `adw_version` + `build_options.x11`/`.wayland` helpers land. Its inner
    // comments are preserved verbatim here (R1):
    //
    //     if (comptime build_config.app_runtime == .gtk) {
    //         if (comptime builtin.os.tag == .linux) {
    //             const kernel_info = internal_os.getKernelInfo(alloc);
    //             defer if (kernel_info) |k| alloc.free(k);
    //             try stdout.print("  - kernel version: {s}\n", .{kernel_info orelse "Kernel information unavailable"});
    //         }
    //         try stdout.print("  - desktop env   : {t}\n", .{internal_os.desktopEnvironment()});
    //         try stdout.print("  - GTK version   :\n", .{});
    //         try stdout.print("    build         : {f}\n", .{gtk_version.comptime_version});
    //         try stdout.print("    runtime       : {f}\n", .{gtk_version.getRuntimeVersion()});
    //         try stdout.print("  - libadwaita    : enabled\n", .{});
    //         try stdout.print("    build         : {f}\n", .{adw_version.comptime_version});
    //         try stdout.print("    runtime       : {f}\n", .{adw_version.getRuntimeVersion()});
    //         if (comptime build_options.x11) {
    //             try stdout.print("  - libX11        : enabled\n", .{});
    //         } else {
    //             try stdout.print("  - libX11        : disabled\n", .{});
    //         }
    //
    //         // We say `libwayland` since it is possible to build Rustty without
    //         // Wayland integration but with Wayland-enabled GTK
    //         if (comptime build_options.wayland) {
    //             try stdout.print("  - libwayland    : enabled\n", .{});
    //         } else {
    //             try stdout.print("  - libwayland    : disabled\n", .{});
    //         }
    //     }

    // Don't forget to flush!
    // try stdout.flush();
    stdout.flush()?;
    // return 0;
    Ok(0)
}
