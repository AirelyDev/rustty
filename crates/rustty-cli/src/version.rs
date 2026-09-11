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
//!
//! CORRECTION (task-18, S70-F1): "done, nothing deferred" was WRONG when it was
//! written — the whole `app_runtime == .gtk` block (`version.zig:49-75`: kernel
//! version, desktop env, GTK build/runtime, libadwaita build/runtime, libX11,
//! libwayland) was absent, kept only as a comment. It is ported now. Two things
//! for whoever reads this next: the block compiles ONLY under
//! `--features gtk` on Linux/FreeBSD, so the macOS lane can never exercise it —
//! [`write_gtk_block`] exists precisely so the FORMAT is still asserted on every
//! host; and `libX11`/`libwayland` read `cfg!(feature = ...)` IN THIS CRATE, so
//! `rustty-cli` needs its own `x11`/`wayland` features (it has them) and
//! `crates/rustty` must forward them (it does). If you add a build option the
//! block prints, it has to reach this crate the same way.

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
//
// CORRECTION (task-18, closes S70-F1). The note that stood here said these were
// "only referenced in the comptime `app_runtime == .gtk` block, which is
// statically false for the headless `.none` build; not imported here." That was
// true when it was written and is no longer: `rustty-cli` gained a `gtk` feature
// (forwarding `rustty-apprt/gtk`) and the GTK apprt's `gtk_version`/`adw_version`
// modules landed, so the block is now PORTED below under
// `#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]`
// and these two modules ARE imported there.
#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
use rustty_apprt::gtk::{adw_version, gtk_version};

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

/// The `{t}` tag name of a [`rustty_os::desktop::DesktopEnvironment`] — the
/// reference's `{t}` on `internal_os.desktopEnvironment(&environ_map)`
/// (`desktop.zig:73-78`: `gnome, macos, other, windows`).
// Used ONLY by the `cfg`-gated GTK block in `write_version` (and by the tests).
// Off that lane it is legitimately unused, so the `dead_code` allow is scoped to
// exactly that case rather than blanket-applied.
#[cfg_attr(
    not(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd"))),
    allow(dead_code)
)]
fn desktop_environment_tag(de: rustty_os::desktop::DesktopEnvironment) -> &'static str {
    match de {
        rustty_os::desktop::DesktopEnvironment::Gnome => "gnome",
        rustty_os::desktop::DesktopEnvironment::Macos => "macos",
        rustty_os::desktop::DesktopEnvironment::Other => "other",
        rustty_os::desktop::DesktopEnvironment::Windows => "windows",
    }
}

/// The already-read inputs to the reference's `app_runtime == .gtk` block
/// (`version.zig:49-75`).
///
/// PORT NOTE: a seam, not a divergence — same shape as [`write_version`] itself.
/// The block is `cfg`-gated to a GTK Linux/FreeBSD build, so a test that called
/// the real `gtk_version::runtime_version()` could not compile, let alone run, on
/// the macOS lane. Lifting the four reads out makes the FORMAT — which is the
/// whole of what S70-F1 reported missing — assertable on every host.
// Used ONLY by the `cfg`-gated GTK block in `write_version` (and by the tests).
// Off that lane it is legitimately unused, so the `dead_code` allow is scoped to
// exactly that case rather than blanket-applied.
#[cfg_attr(
    not(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd"))),
    allow(dead_code)
)]
struct GtkBlock<'a> {
    /// `None` = the target is not Linux, so the reference's inner
    /// `builtin.os.tag == .linux` branch does not run and the line is omitted.
    /// `Some(None)` = Linux, but `getKernelInfo` failed.
    kernel_info: Option<Option<&'a [u8]>>,
    desktop_env: &'static str,
    gtk_build: (u16, u16, u16),
    gtk_runtime: (u16, u16, u16),
    adw_build: (u16, u16, u16),
    adw_runtime: (u16, u16, u16),
    x11: bool,
    wayland: bool,
}

/// PORTS: `version.zig:49-75` — the body of the `app_runtime == .gtk` block.
///
/// PORT NOTE (`build :`). The reference prints `gtk_version.comptime_version` /
/// `adw_version.comptime_version`, read from the BUILD BOX's `gtk/version.h`.
/// rustty has no such header include and therefore no `comptime_version`; the
/// documented stand-in is the pinned API BASELINE the crate compiles against
/// (`gtk_version.rs:38-47`, `adw_version.rs:43`), which is the same quantity the
/// reference's value serves as — the version the binary was COMPILED against.
/// Passed in by the caller so this stays host-independent.
// Used ONLY by the `cfg`-gated GTK block in `write_version` (and by the tests).
// Off that lane it is legitimately unused, so the `dead_code` allow is scoped to
// exactly that case rather than blanket-applied.
#[cfg_attr(
    not(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd"))),
    allow(dead_code)
)]
fn write_gtk_block<W: std::io::Write>(stdout: &mut W, b: &GtkBlock<'_>) -> Result<(), Error> {
    // if (comptime builtin.os.tag == .linux) {
    if let Some(kernel_info) = b.kernel_info {
        // try stdout.print("  - kernel version: {s}\n", .{kernel_info orelse "Kernel information unavailable"});
        write!(stdout, "  - kernel version: ")?;
        stdout.write_all(kernel_info.unwrap_or(b"Kernel information unavailable"))?;
        writeln!(stdout)?;
    }
    // try stdout.print("  - desktop env   : {t}\n", .{internal_os.desktopEnvironment(&environ_map)});
    writeln!(stdout, "  - desktop env   : {}", b.desktop_env)?;
    // try stdout.print("  - GTK version   :\n", .{});
    writeln!(stdout, "  - GTK version   :")?;
    // try stdout.print("    build         : {f}\n", .{gtk_version.comptime_version});
    let (a, i, c) = b.gtk_build;
    writeln!(stdout, "    build         : {a}.{i}.{c}")?;
    // try stdout.print("    runtime       : {f}\n", .{gtk_version.getRuntimeVersion()});
    let (a, i, c) = b.gtk_runtime;
    writeln!(stdout, "    runtime       : {a}.{i}.{c}")?;
    // try stdout.print("  - libadwaita    : enabled\n", .{});
    writeln!(stdout, "  - libadwaita    : enabled")?;
    // try stdout.print("    build         : {f}\n", .{adw_version.comptime_version});
    let (a, i, c) = b.adw_build;
    writeln!(stdout, "    build         : {a}.{i}.{c}")?;
    // try stdout.print("    runtime       : {f}\n", .{adw_version.getRuntimeVersion()});
    let (a, i, c) = b.adw_runtime;
    writeln!(stdout, "    runtime       : {a}.{i}.{c}")?;
    // if (comptime build_options.x11) {
    //     try stdout.print("  - libX11        : enabled\n", .{});
    // } else {
    //     try stdout.print("  - libX11        : disabled\n", .{});
    // }
    writeln!(
        stdout,
        "  - libX11        : {}",
        if b.x11 { "enabled" } else { "disabled" }
    )?;

    // We say `libwayland` since it is possible to build Rustty without
    // Wayland integration but with Wayland-enabled GTK
    // if (comptime build_options.wayland) {
    //     try stdout.print("  - libwayland    : enabled\n", .{});
    // } else {
    //     try stdout.print("  - libwayland    : disabled\n", .{});
    // }
    writeln!(
        stdout,
        "  - libwayland    : {}",
        if b.wayland { "enabled" } else { "disabled" }
    )?;
    Ok(())
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
    // CORRECTION (task-18, closes S70-F1). The note that stood here said the GTK
    // block "ports with the GTK apprt (Phase-3)" and kept the reference body as a
    // preserved comment. Both halves of that premise have since landed —
    // `rustty-apprt/src/gtk/{gtk_version,adw_version}.rs`, `rustty_os::
    // {getKernelInfo,desktopEnvironment}`, and the `x11`/`wayland` cargo features
    // — so the block is LIVE below and the note was describing a finished thing
    // as pending. The reference lines are preserved inline at each statement (R1)
    // rather than as one dead block.
    //
    // if (comptime build_config.app_runtime == .gtk) {
    //
    // PORT NOTE (the condition). The reference's `build_config.app_runtime` is a
    // BUILD option; rustty's analog for "this build links the GTK apprt" is the
    // cargo `gtk` feature, which is what `crates/rustty/Cargo.toml` turns on for
    // exactly the targets whose `Runtime::default_for` is `.gtk`
    // (`apprt/runtime.zig:14-24` -> linux + freebsd). Reading
    // `build_config::app_runtime()` instead would be WRONG here: it is a const fn
    // of the host target, so it says `Gtk` on Linux even in a build that never
    // linked gtk4 and where `gtk_version` does not exist to call.
    #[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
    {
        //     if (comptime builtin.os.tag == .linux) {
        //         const kernel_info = internal_os.getKernelInfo(alloc);
        //         defer if (kernel_info) |k| alloc.free(k);
        //         try stdout.print("  - kernel version: {s}\n", .{kernel_info orelse "Kernel information unavailable"});
        //     }
        // PORT NOTE: `getKernelInfo` owns its buffer (`Vec<u8>`), so the
        // reference's `defer alloc.free` is the drop at the end of the block.
        // Bytes, not `String` (memory: zig-byteslice-to-rust-rawbytes) — it is
        // `uname(2)` output.
        #[cfg(target_os = "linux")]
        let kernel_info: Option<Option<Vec<u8>>> = Some(rustty_os::getKernelInfo());
        #[cfg(not(target_os = "linux"))]
        let kernel_info: Option<Option<Vec<u8>>> = None;

        //     try stdout.print("  - desktop env   : {t}\n", .{internal_os.desktopEnvironment(&environ_map)});
        // var environ_map = try global.environMap(); defer environ_map.deinit();
        let environ_map = rustty_global::environ_map();

        write_gtk_block(
            stdout,
            &GtkBlock {
                kernel_info: kernel_info.as_ref().map(Option::as_deref),
                desktop_env: desktop_environment_tag(rustty_os::desktopEnvironment(&environ_map)),
                gtk_build: gtk_version::BASELINE,
                gtk_runtime: gtk_version::runtime_version(),
                adw_build: adw_version::BASELINE,
                adw_runtime: adw_version::runtime_version(),
                x11: cfg!(feature = "x11"),
                wayland: cfg!(feature = "wayland"),
            },
        )?;
    }

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
    // MIRI EXCEPTION (rustty-cli version, process/file-I/O class): the test asks
    // the environment whether it IS a git checkout by running `git rev-parse`,
    // which Miri's isolation forbids ("`open` not available when isolation is
    // enabled"), aborting the whole rustty-cli lib binary. Removing the probe
    // would make the test vacuous in a tarball build — the exact hole WS19 T12
    // closed. Same class as `rustty-vaxis/tests/ucd_provenance.rs`. Registered
    // in .port/phase-2/MIRI_EXCEPTIONS.md ("rustty-cli — the 2026-07-31 CI triage").
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli): spawns `git rev-parse`; \
         Miri isolation forbids it — see MIRI_EXCEPTIONS.md"
    )]
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

    /// PORTS the FORMAT of `version.zig:49-75` (the `app_runtime == .gtk` block),
    /// which `+version` omitted entirely until task-18 (S70-F1).
    ///
    /// Column alignment is load-bearing: the reference pads every label to the
    /// same width (`"  - kernel version: "`, `"  - desktop env   : "`,
    /// `"    build         : "`), and a `+version` diff against upstream is how
    /// anyone would notice. Asserted byte-for-byte against the reference's own
    /// literals rather than by `contains`.
    #[test]
    fn gtk_block_matches_the_reference_format_on_linux() {
        let mut buf: Vec<u8> = Vec::new();
        write_gtk_block(
            &mut buf,
            &GtkBlock {
                kernel_info: Some(Some(b"Linux 6.11.3-200.fc40.x86_64")),
                desktop_env: "gnome",
                gtk_build: (4, 14, 0),
                gtk_runtime: (4, 22, 4),
                adw_build: (1, 5, 0),
                adw_runtime: (1, 6, 2),
                x11: true,
                wayland: false,
            },
        )
        .expect("write_gtk_block");
        assert_eq!(
            String::from_utf8(buf).expect("utf-8"),
            concat!(
                "  - kernel version: Linux 6.11.3-200.fc40.x86_64\n",
                "  - desktop env   : gnome\n",
                "  - GTK version   :\n",
                "    build         : 4.14.0\n",
                "    runtime       : 4.22.4\n",
                "  - libadwaita    : enabled\n",
                "    build         : 1.5.0\n",
                "    runtime       : 1.6.2\n",
                "  - libX11        : enabled\n",
                "  - libwayland    : disabled\n",
            )
        );
    }

    /// The reference's inner `if (comptime builtin.os.tag == .linux)` — a
    /// FreeBSD GTK build prints the block WITHOUT the kernel line — and the
    /// `orelse "Kernel information unavailable"` fallback, which is the arm a
    /// `/proc`-less or failing `uname` takes.
    #[test]
    fn gtk_block_kernel_line_is_linux_only_and_has_a_fallback() {
        let mut buf: Vec<u8> = Vec::new();
        write_gtk_block(
            &mut buf,
            &GtkBlock {
                kernel_info: None,
                desktop_env: "other",
                gtk_build: (4, 14, 0),
                gtk_runtime: (4, 14, 0),
                adw_build: (1, 5, 0),
                adw_runtime: (1, 5, 0),
                x11: false,
                wayland: true,
            },
        )
        .expect("write_gtk_block");
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(
            !out.contains("kernel version"),
            "non-Linux GTK build must omit the kernel line: {out:?}"
        );
        assert!(out.starts_with("  - desktop env   : other\n"), "{out:?}");
        assert!(
            out.ends_with("  - libX11        : disabled\n  - libwayland    : enabled\n"),
            "{out:?}"
        );

        let mut buf: Vec<u8> = Vec::new();
        write_gtk_block(
            &mut buf,
            &GtkBlock {
                kernel_info: Some(None),
                desktop_env: "gnome",
                gtk_build: (4, 14, 0),
                gtk_runtime: (4, 14, 0),
                adw_build: (1, 5, 0),
                adw_runtime: (1, 5, 0),
                x11: true,
                wayland: true,
            },
        )
        .expect("write_gtk_block");
        assert!(String::from_utf8(buf)
            .expect("utf-8")
            .starts_with("  - kernel version: Kernel information unavailable\n"));
    }

    /// The `{t}` tag names the reference prints for `desktopEnvironment`
    /// (`os/desktop.zig:73-78`).
    #[test]
    fn desktop_environment_tags_are_the_reference_tag_names() {
        use rustty_os::desktop::DesktopEnvironment as De;
        assert_eq!(desktop_environment_tag(De::Gnome), "gnome");
        assert_eq!(desktop_environment_tag(De::Macos), "macos");
        assert_eq!(desktop_environment_tag(De::Other), "other");
        assert_eq!(desktop_environment_tag(De::Windows), "windows");
    }

    /// Off the GTK lane `+version` must be UNCHANGED — the block is the
    /// reference's `comptime` branch, so the headless build emits nothing extra.
    #[test]
    #[cfg(not(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd"))))]
    fn non_gtk_version_output_has_no_gtk_block() {
        let mut buf: Vec<u8> = Vec::new();
        write_version(&mut buf, false).expect("write_version");
        let out = String::from_utf8(buf).expect("utf-8");
        for marker in [
            "kernel version",
            "desktop env",
            "GTK version",
            "libadwaita",
            "libX11",
            "libwayland",
        ] {
            assert!(
                !out.contains(marker),
                "{marker:?} leaked into a non-GTK +version: {out:?}"
            );
        }
    }

    /// On the GTK lane it must be PRESENT — the arm that actually closes S70-F1.
    /// Runs only where the block compiles (the Linux VM).
    #[test]
    #[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
    fn gtk_version_output_carries_the_gtk_block() {
        let mut buf: Vec<u8> = Vec::new();
        write_version(&mut buf, false).expect("write_version");
        let out = String::from_utf8(buf).expect("utf-8");
        for marker in [
            "  - desktop env   : ",
            "  - GTK version   :\n",
            "  - libadwaita    : enabled\n",
            "  - libX11        : ",
            "  - libwayland    : ",
        ] {
            assert!(
                out.contains(marker),
                "{marker:?} missing from the GTK +version: {out:?}"
            );
        }
        #[cfg(target_os = "linux")]
        assert!(out.contains("  - kernel version: "), "{out:?}");
        // The block follows the libxev line, as in the reference.
        let xev = out.find("  - libxev        : ").expect("libxev line");
        let de = out.find("  - desktop env   : ").expect("desktop env line");
        assert!(
            xev < de,
            "the GTK block must follow the libxev line: {out:?}"
        );
    }
}
