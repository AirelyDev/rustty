//! rustty-cli::version
//!
//! PORTS: ghostty src/cli/version.zig — the `+version` / `--version` action.
//!
//! PORT STATUS: done, nothing deferred (WS3-B + WS19 T12). The renderer / libxev
//! / toolchain lines carry real values: `build_config::renderer()` mapped to the
//! reference `@typeName` strings, `rustty_xev::backend()` (the reference
//! `xev.backend`), and `build_config::RUSTC_VERSION` (the
//! `builtin.zig_version_string` analog). WS19 T12 closed the last gap — the OSC-8
//! commit hyperlink now emits: `build.rs` captures the source commit (the
//! `+BUILD`-metadata analog) and `rustty_identity::COMMIT_URL_PREFIX` supplies
//! the repo (R2).

// const std = @import("std");
// const build_options = @import("build_options");
// const Allocator = std.mem.Allocator;
// const builtin = @import("builtin");
// const build_config = @import("../build_config.zig");
use rustty_build_config as build_config;
// const internal_os = @import("../os/main.zig");
// const xev = @import("../global.zig").xev;          -> rustty_xev (WS3-B)
// const renderer = @import("../renderer.zig");        -> renderer_type_name() (WS3-B)
//
// const gtk_version = @import("../apprt/gtk/gtk_version.zig");
// const adw_version = @import("../apprt/gtk/adw_version.zig");
//   -> only referenced in the comptime `app_runtime == .gtk` block, which is
//      statically false for the headless `.none` build; not imported here.

use crate::rustty::Error;

// pub const Options = struct {};
pub struct Options {}

/// The renderer type name, exactly as the reference prints it: `{}` on the
/// comptime type `renderer.Renderer` is `@typeName`, and renderer.zig:38-42
/// selects `.metal => GenericRenderer(Metal)`, `.opengl => GenericRenderer(OpenGL)`,
/// `.webgl => WebGL`. Derived from `build_config::renderer()` — the SAME
/// `build_config.renderer` the reference switch reads — so the CLI keeps no
/// rustty-renderer edge.
fn renderer_type_name() -> &'static str {
    match build_config::renderer() {
        build_config::renderer::Backend::Metal => "renderer.generic.Renderer(renderer.Metal)",
        build_config::renderer::Backend::Opengl => "renderer.generic.Renderer(renderer.OpenGL)",
        build_config::renderer::Backend::Webgl => "renderer.WebGL",
    }
}

/// The `{t}` tag name of the libxev backend (the reference `@tagName` of
/// `xev.backend`).
fn libxev_backend_tag() -> &'static str {
    match rustty_xev::backend() {
        rustty_xev::Backend::IoUring => "io_uring",
        rustty_xev::Backend::Epoll => "epoll",
        rustty_xev::Backend::Kqueue => "kqueue",
        rustty_xev::Backend::WasiPoll => "wasi_poll",
        rustty_xev::Backend::Iocp => "iocp",
    }
}

/// Reference: `build_config.version.build` — the `+BUILD` metadata (commit hash)
/// of the SemanticVersion, which `build.zig` fills in from the source checkout.
///
/// The Rust runtime SemanticVersion view has no `+BUILD` channel, so `build.rs`
/// captures the commit (`git rev-parse --short HEAD`, overridable via
/// `RUSTTY_COMMIT_HASH`) and bakes it into the crate env instead. It is EMPTY —
/// hence `None` here, hence no hyperlink — when the source is not a git checkout
/// (release tarball / vendored crate / docs.rs), which is exactly the reference's
/// own `build_config.version.build == null` case.
fn commit_hash() -> Option<&'static str> {
    let hash = env!("RUSTTY_COMMIT_HASH");
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}

/// The web URL for a source commit: the product's repo commit-URL prefix
/// (R2 — `rustty_identity::COMMIT_URL_PREFIX`, the reference's is
/// `https://github.com/ghostty-org/ghostty/commit/`) with the hash appended.
fn commit_url(commit_hash: &str) -> String {
    format!("{}{commit_hash}", rustty_identity::COMMIT_URL_PREFIX)
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
    use std::io::IsTerminal;

    // var buffer: [1024]u8 = undefined;
    // const stdout_file: std.fs.File = .stdout();
    let stdout_file = std::io::stdout();
    // const tty = stdout_file.isTty();
    let tty = stdout_file.is_terminal();
    // var stdout_writer = stdout_file.writer(&buffer); const stdout = &..interface;
    let mut stdout = std::io::BufWriter::new(stdout_file.lock());

    write_version(&mut stdout, tty)
}

/// The body of `run`, over an arbitrary sink and an explicit `tty` flag.
///
/// PORT NOTE: a seam, not a divergence — the reference already threads `stdout`
/// (a `*std.Io.Writer`) and the `tty` bool through this code. Splitting them out
/// of the `std.fs.File.stdout()`/`isTty()` acquisition is what lets the tty-only
/// OSC-8 hyperlink branch be asserted in-suite: `+version`'s stdout is a PIPE
/// under `cargo test`, so the binary-level `cli_smoke` KAT can never reach it.
fn write_version<W: std::io::Write>(stdout: &mut W, tty: bool) -> Result<u8, Error> {
    // if (tty) if (build_config.version.build) |commit_hash| {
    //     try stdout.print("\x1b]8;;https://github.com/ghostty-org/ghostty/commit/{s}\x1b\\", ...);
    // };
    if tty {
        if let Some(commit_hash) = commit_hash() {
            // R2: the commit-URL host is product identity —
            // `rustty_identity::COMMIT_URL_PREFIX` (reference:
            // `https://github.com/ghostty-org/ghostty/commit/`).
            write!(stdout, "\x1b]8;;{}\x1b\\", commit_url(commit_hash))?;
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
    //   PORT NOTE: the Rust port's toolchain is rustc — the analogous version,
    //   captured at compile time (build_config::RUSTC_VERSION).
    writeln!(
        stdout,
        "  - Rust version  : {}",
        build_config::RUSTC_VERSION
    )?;
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
    writeln!(stdout, "  - renderer      : {}", renderer_type_name())?;
    // try stdout.print("  - libxev        : {t}\n", .{xev.backend});
    writeln!(stdout, "  - libxev        : {}", libxev_backend_tag())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the crate source tree is a live git checkout — the exact condition
    /// under which `build.rs` bakes a hash. Keeps the assertion below from being
    /// vacuous in a checkout AND from being wrong in a tarball build.
    fn source_is_git_checkout() -> bool {
        std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// The `+BUILD`-metadata analog must actually CARRY a commit when built from a
    /// git checkout. This is the assertion that was dead before WS19 T12
    /// (`commit_hash()` returned `None` unconditionally).
    #[test]
    fn commit_hash_is_captured_in_a_git_checkout() {
        if !source_is_git_checkout() {
            // Not a checkout: the reference's own `version.build == null` case.
            assert_eq!(commit_hash(), None);
            return;
        }
        let hash = commit_hash().expect(
            "built from a git checkout, so build.rs must have baked RUSTTY_COMMIT_HASH; \
             `commit_hash()` returning None means the +BUILD metadata analog is dead again",
        );
        assert!(
            hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "commit hash should be a short hex object name, got {hash:?}"
        );
    }

    /// The hyperlink URL is the PRODUCT's repo (R2), with the hash appended.
    #[test]
    fn commit_url_is_the_product_repo_commit_url() {
        assert_eq!(
            commit_url("deadbee"),
            format!("{}deadbee", rustty_identity::COMMIT_URL_PREFIX)
        );
        // The exact per-profile prefix value is pinned by rustty-identity's own
        // profile tests (rustty-cli has no `identity-oracle` feature of its own to
        // cfg on). Here: it must be an absolute commit URL, not a bare hash — the
        // pre-WS19-T12 branch wrote `\x1b]8;;{hash}` with no prefix at all.
        assert!(
            commit_url("deadbee").starts_with("https://")
                && commit_url("deadbee").ends_with("/commit/deadbee"),
            "commit_url must be an absolute <repo>/commit/<hash> URL, got {:?}",
            commit_url("deadbee")
        );
    }

    /// On a TTY the version header is wrapped in a well-formed OSC-8 hyperlink
    /// whose URI is the real commit URL; the closing `OSC 8 ; ; ST` follows the
    /// blank line after it. This is the branch `cli_smoke` can never see (its
    /// stdout is a pipe).
    #[test]
    fn tty_emits_the_osc8_commit_hyperlink() {
        let mut buf: Vec<u8> = Vec::new();
        let rc = write_version(&mut buf, true).expect("write_version");
        assert_eq!(rc, 0);
        let out = String::from_utf8(buf).expect("+version output is UTF-8");

        let Some(hash) = commit_hash() else {
            // Tarball build: assert the reference's null-metadata behavior — no
            // OPENING hyperlink, so nothing is linked. (The CLOSING `OSC 8 ; ; ST`
            // IS still written on a tty even with no link open — the reference
            // guards it on `tty` alone, not on the hash.)
            assert!(
                !out.starts_with("\x1b]8;;"),
                "no commit metadata, so no hyperlink should be opened: {out:?}"
            );
            return;
        };

        let open = format!("\x1b]8;;{}\x1b\\", commit_url(hash));
        let header = format!(
            "{} {}",
            rustty_identity::APP_NAME_TITLECASE,
            build_config::VERSION_STRING
        );
        assert!(
            out.starts_with(&format!("{open}{header}\n\n\x1b]8;;\x1b\\")),
            "expected the version header wrapped in an OSC-8 hyperlink to {:?}, got: {out:?}",
            commit_url(hash)
        );
    }

    /// …and NOT off a TTY: the reference guards both writes with `if (tty)`.
    #[test]
    fn non_tty_emits_no_osc8() {
        let mut buf: Vec<u8> = Vec::new();
        write_version(&mut buf, false).expect("write_version");
        let out = String::from_utf8(buf).expect("+version output is UTF-8");
        assert!(
            !out.contains("\x1b]8;;"),
            "no OSC-8 may be emitted when stdout is not a tty: {out:?}"
        );
        assert!(out.starts_with(rustty_identity::APP_NAME_TITLECASE));
    }
}
