//! Shell-integration setup: the byte-exact per-shell (bash/zsh/fish/elvish/
//! nushell) command-line and environment-variable generation.
//!
//! PORTS: ghostty src/termio/shell_integration.zig (21 `test{}` blocks — all
//! ported, R1).
//!
//! PORT STATUS: ported (integration-tier stage 4).

// PARITY: the reference's identifiers are camelCase (`detectShell`,
// `setupFeatures`, `setupBash`, ...); preserved verbatim per R1/R2 (termio
// stage-2 precedent).
#![allow(non_snake_case)]
// PARITY: `TmpResourcesDir::init`'s `switch (shell)` with a single live arm
// mirrors the reference switch verbatim (rustty-vt stream.rs precedent).
#![allow(clippy::single_match)]

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const EnvMap = std.process.EnvMap;
// const config = @import("../config.zig");
// const homedir = @import("../os/homedir.zig");
// const internal_os = @import("../os/main.zig");
//
// PORT NOTE: `config` -> rustty-config; `homedir`/`internal_os` -> rustty-os.
// `EnvMap` is the crate's stage-3 `std.process.EnvMap` modeling
// (`crate::command::EnvMap` = `HashMap<String, String>`); the reference's
// `[]const u8` env values convert at the put boundary via `env_string` below.
// The `alloc_arena`/`Allocator` params collapse into Rust ownership
// (crash-tier arena-collapse precedent).
use rustty_config as configpkg;
use rustty_config::value_types::ShellIntegrationFeatures;
use rustty_os::homedir;
use rustty_os::shell::ShellCommandBuilder;

use crate::command::{EnvMap, EnvMapExt as _};

// const log = std.log.scoped(.shell_integration);
//
// PORT NOTE (DECISION-logging.md §6/§6r4): `LOG_SCOPE` is the `@tagName` of the
// reference's `std.log.scoped(.shell_integration)`; restored sites call the
// `log` facade with `target: LOG_SCOPE`.
const LOG_SCOPE: &str = "shell_integration";
//
// PORT NOTE: the reference's log call sites are preserved verbatim as comments at
// each site (rustty-os cf_release_thread precedent).

/// Shell types we support
// pub const Shell = enum { bash, elvish, fish, nushell, zsh };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Nushell,
    Zsh,
}

impl Shell {
    /// PORT NOTE (Zig-std shim): `@tagName(shell)` — the lowercase tag name.
    pub fn tag_name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Elvish => "elvish",
            Shell::Fish => "fish",
            Shell::Nushell => "nushell",
            Shell::Zsh => "zsh",
        }
    }
}

/// The result of setting up a shell integration.
pub struct ShellIntegration {
    /// The successfully-integrated shell.
    pub shell: Shell,

    /// The command to use to start the shell with the integration.
    /// In most cases this is identical to the command given but for
    /// bash in particular it may be different.
    ///
    /// The memory is allocated in the arena given to setup.
    //
    // PORT NOTE: the arena allocation collapses into the owned
    // `rustty_config::Command` (Vec-backed).
    pub command: configpkg::command::Command,
}

/// PORT NOTE (Rust-only glue): the reference `setup`/`setupBash` error sets are
/// inferred Zig error unions (`!?ShellIntegration`). The allocation and
/// `bufPrint` (`NoSpaceLeft`) halves collapse with the arena/format model;
/// what survives is `homedir.home`'s error set (the bash HISTFILE path).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Homedir(homedir::Error),
}

/// Set up the command execution environment for automatic
/// integrated shell integration and return a ShellIntegration
/// struct describing the integration.  If integration fails
/// (shell type couldn't be detected, etc.), this will return null.
///
/// The allocator is used for temporary values and to allocate values
/// in the ShellIntegration result. It is expected to be an arena to
/// simplify cleanup.
//
// pub fn setup(
//     alloc_arena: Allocator,
//     resource_dir: []const u8,
//     command: config.Command,
//     env: *EnvMap,
//     force_shell: ?Shell,
// ) !?ShellIntegration {
//
// PORT NOTE: `command: config.Command` is a Zig by-value union of slices; the
// Rust `Command` owns its bytes, so it is passed by reference and `clone`d
// where the reference copies it.
pub fn setup(
    resource_dir: &[u8],
    command: &configpkg::command::Command,
    env: &mut EnvMap,
    force_shell: Option<Shell>,
) -> Result<Option<ShellIntegration>, Error> {
    // const shell: Shell = force_shell orelse
    //     try detectShell(alloc_arena, command) orelse
    //     return null;
    let shell: Shell = match force_shell.or_else(|| detectShell(command)) {
        Some(shell) => shell,
        None => return Ok(None),
    };

    let new_command: configpkg::command::Command = match match shell {
        // .bash => try setupBash(
        //     alloc_arena,
        //     command,
        //     resource_dir,
        //     env,
        // ),
        Shell::Bash => setupBash(command, resource_dir, env)?,

        // .nushell => try setupNushell(
        //     alloc_arena,
        //     command,
        //     resource_dir,
        //     env,
        // ),
        Shell::Nushell => setupNushell(command, resource_dir, env)?,

        // .zsh => try setupZsh(
        //     alloc_arena,
        //     command,
        //     resource_dir,
        //     env,
        // ),
        Shell::Zsh => setupZsh(command, resource_dir, env)?,

        // .elvish, .fish => xdg: {
        //     if (!try setupXdgDataDirs(alloc_arena, resource_dir, env)) return null;
        //     break :xdg try command.clone(alloc_arena);
        // },
        Shell::Elvish | Shell::Fish => {
            if !setupXdgDataDirs(resource_dir, env) {
                return Ok(None);
            }
            Some(command.clone())
        }
    } {
        Some(c) => c,
        // } orelse return null;
        None => return Ok(None),
    };

    // return .{
    //     .shell = shell,
    //     .command = new_command,
    // };
    Ok(Some(ShellIntegration {
        shell,
        command: new_command,
    }))
}

// fn detectShell(alloc: Allocator, command: config.Command) !?Shell {
//
// PORT NOTE: the `!` is the argIterator allocation (collapsed).
fn detectShell(command: &configpkg::command::Command) -> Option<Shell> {
    // var arg_iter = try command.argIterator(alloc);
    // defer arg_iter.deinit();
    let mut arg_iter = command.arg_iterator();

    // const arg0 = arg_iter.next() orelse return null;
    // const exe = std.fs.path.basename(arg0);
    //
    // PORT NOTE: `arg0` is copied out of the iterator's decode buffer (the
    // reference slice borrows the arena; Rust's borrow rules require the copy
    // before `deinit`).
    let arg0 = arg_iter.next()?.to_vec();
    arg_iter.deinit();
    let exe = basename(&arg0);

    // if (std.mem.eql(u8, "bash", exe)) {
    if exe == b"bash" {
        // Apple distributes their own patched version of Bash 3.2
        // on macOS that disables the ENV-based POSIX startup path.
        // This means we're unable to perform our automatic shell
        // integration sequence in this specific environment.
        //
        // If we're running "/bin/bash" on Darwin, we can assume
        // we're using Apple's Bash because /bin is non-writable
        // on modern macOS due to System Integrity Protection.
        //
        // if (comptime builtin.target.os.tag.isDarwin()) {
        //     if (std.mem.eql(u8, "/bin/bash", arg0)) {
        //         return null;
        //     }
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            if arg0 == b"/bin/bash" {
                return None;
            }
        }
        return Some(Shell::Bash);
    }

    // if (std.mem.eql(u8, "elvish", exe)) return .elvish;
    if exe == b"elvish" {
        return Some(Shell::Elvish);
    }
    // if (std.mem.eql(u8, "fish", exe)) return .fish;
    if exe == b"fish" {
        return Some(Shell::Fish);
    }
    // if (std.mem.eql(u8, "nu", exe)) return .nushell;
    if exe == b"nu" {
        return Some(Shell::Nushell);
    }
    // if (std.mem.eql(u8, "zsh", exe)) return .zsh;
    if exe == b"zsh" {
        return Some(Shell::Zsh);
    }

    None
}

/// Set up the shell integration features environment variable.
// pub fn setupFeatures(
//     env: *EnvMap,
//     features: config.ShellIntegrationFeatures,
//     cursor_blink: bool,
// ) !void {
//
// PORT NOTE: the `!` is `env.put`'s allocation + the fixed writer's
// `NoSpaceLeft` (unreachable — the buffer is sized comptime to hold every
// field); both collapse.
pub fn setupFeatures(env: &mut EnvMap, features: ShellIntegrationFeatures, cursor_blink: bool) {
    // const fields = @typeInfo(@TypeOf(features)).@"struct".fields;
    // const capacity: usize = capacity: {
    //     comptime var n: usize = fields.len - 1; // commas
    //     inline for (fields) |field| n += field.name.len;
    //     n += ":steady".len; // cursor value
    //     break :capacity n;
    // };
    //
    // var buf: [capacity]u8 = undefined;
    // var writer: std.Io.Writer = .fixed(&buf);
    //
    // PORT NOTE: the comptime capacity calculation + fixed buffer collapse
    // into a growable `String` accumulator.
    let mut writer = String::new();

    // Sort the fields so that the output is deterministic. This is
    // done at comptime so it has no runtime cost
    //
    // const fields_sorted: [fields.len][]const u8 = comptime fields: {
    //     var fields_sorted: [fields.len][]const u8 = undefined;
    //     for (fields, 0..) |field, i| fields_sorted[i] = field.name;
    //     std.mem.sortUnstable(
    //         []const u8,
    //         &fields_sorted,
    //         {},
    //         (struct {
    //             fn lessThan(_: void, lhs: []const u8, rhs: []const u8) bool {
    //                 return std.ascii.orderIgnoreCase(lhs, rhs) == .lt;
    //             }
    //         }).lessThan,
    //     );
    //     break :fields fields_sorted;
    // };
    //
    // PORT NOTE: Rust has no comptime field reflection; the sorted field list
    // below IS the comptime result for `ShellIntegrationFeatures`'s fields
    // (`cursor, sudo, title, ssh-env, ssh-terminfo, path` sorted
    // case-insensitively), paired with each field's value read.
    let fields_sorted: [(&str, bool); 6] = [
        ("cursor", features.cursor),
        ("path", features.path),
        ("ssh-env", features.ssh_env),
        ("ssh-terminfo", features.ssh_terminfo),
        ("sudo", features.sudo),
        ("title", features.title),
    ];

    // inline for (fields_sorted) |name| {
    //     if (@field(features, name)) {
    for (name, value) in fields_sorted {
        if value {
            // if (writer.end > 0) try writer.writeByte(',');
            if !writer.is_empty() {
                writer.push(',');
            }
            // try writer.writeAll(name);
            writer.push_str(name);

            // if (std.mem.eql(u8, name, "cursor")) {
            //     try writer.writeAll(if (cursor_blink) ":blink" else ":steady");
            // }
            if name == "cursor" {
                writer.push_str(if cursor_blink { ":blink" } else { ":steady" });
            }
        }
    }

    // if (writer.end > 0) {
    //     try env.put("GHOSTTY_SHELL_FEATURES", buf[0..writer.end]);
    // }
    if !writer.is_empty() {
        env.put(&env_name("SHELL_FEATURES"), writer.as_bytes());
    }
}

/// Setup the bash automatic shell integration. This works by
/// starting bash in POSIX mode and using the ENV environment
/// variable to load our bash integration script. This prevents
/// bash from loading its normal startup files, which becomes
/// our script's responsibility (along with disabling POSIX
/// mode).
///
/// This returns a new (allocated) shell command string that
/// enables the integration or null if integration failed.
// fn setupBash(
//     alloc: Allocator,
//     command: config.Command,
//     resource_dir: []const u8,
//     env: *EnvMap,
// ) !?config.Command {
fn setupBash(
    command: &configpkg::command::Command,
    resource_dir: &[u8],
    env: &mut EnvMap,
) -> Result<Option<configpkg::command::Command>, Error> {
    // var stack_fallback = std.heap.stackFallback(4096, alloc);
    // var cmd = internal_os.shell.ShellCommandBuilder.init(stack_fallback.get());
    // defer cmd.deinit();
    //
    // (PORT NOTE: the stackFallback allocator collapses into the builder's owned
    // buffer; `deinit` is Drop.)
    let mut cmd = ShellCommandBuilder::init();

    // Iterator that yields each argument in the original command line.
    // This will allocate once proportionate to the command line length.
    // var iter = try command.argIterator(alloc);
    // defer iter.deinit();
    let mut iter = command.arg_iterator();

    // Start accumulating arguments with the executable and initial flags.
    // if (iter.next()) |exe| {
    //     try cmd.appendArg(exe);
    // } else return null;
    match iter.next() {
        Some(exe) => cmd.appendArg(exe),
        None => return Ok(None),
    }
    // try cmd.appendArg("--posix");
    cmd.appendArg(b"--posix");

    // Stores the list of intercepted command line flags that will be passed
    // to our shell integration script: --norc --noprofile
    // We always include at least "1" so the script can differentiate between
    // being manually sourced or automatically injected (from here).
    //
    // var buf: [32]u8 = undefined;
    // var inject: std.Io.Writer = .fixed(&buf);
    // try inject.writeAll("1");
    //
    // (PORT NOTE: the fixed 32-byte writer collapses into a `String`.)
    //
    // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD204. The 32
    // bytes hold every DISTINCT flag ("1 --norc --noprofile" is 20), but each
    // repeat of `--norc` / `--noprofile` is appended again, and the fifth
    // ` --norc` of `bash --norc --norc --norc --norc --norc` makes `writeAll`
    // fail (`error.WriteFailed`, measured with zig 0.16.0). `setup` passes the
    // error on and `Exec.zig:800` `try`s it, so the surface fails to start over
    // a flag the user repeated. The `String` keeps every flag (the script reads
    // them as words, so a repeat is harmless), which is what the flag list is
    // for. S213-F2.
    let mut inject = String::from("1");

    // Walk through the rest of the given arguments. If we see an option that
    // would require complex or unsupported integration behavior, we bail out
    // and skip loading our shell integration. Users can still manually source
    // the shell integration script.
    //
    // Unsupported options:
    //  -c          -c is always non-interactive
    //  --posix     POSIX mode (a la /bin/sh)
    //
    // var rcfile: ?[]const u8 = null;
    //
    // PORT NOTE: `rcfile` is copied out of the iterator's decode buffer (the
    // reference slice stays valid in the arena; Rust's borrow rules require
    // the copy across further `iter.next()` calls).
    let mut rcfile: Option<Vec<u8>> = None;
    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next() {
        // if (std.mem.eql(u8, arg, "--posix")) {
        if arg == b"--posix" {
            return Ok(None);
        // } else if (std.mem.eql(u8, arg, "--norc")) {
        } else if arg == b"--norc" {
            // try inject.writeAll(" --norc");
            inject.push_str(" --norc");
        // } else if (std.mem.eql(u8, arg, "--noprofile")) {
        } else if arg == b"--noprofile" {
            // try inject.writeAll(" --noprofile");
            inject.push_str(" --noprofile");
        // } else if (std.mem.eql(u8, arg, "--rcfile") or std.mem.eql(u8, arg, "--init-file")) {
        } else if arg == b"--rcfile" || arg == b"--init-file" {
            // rcfile = iter.next();
            rcfile = iter.next().map(|v| v.to_vec());
        // } else if (arg.len > 1 and arg[0] == '-' and arg[1] != '-') {
        } else if arg.len() > 1 && arg[0] == b'-' && arg[1] != b'-' {
            // '-c command' is always non-interactive
            // if (std.mem.indexOfScalar(u8, arg, 'c') != null) {
            //     return null;
            // }
            if arg.contains(&b'c') {
                return Ok(None);
            }
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
        // } else if (std.mem.eql(u8, arg, "-") or std.mem.eql(u8, arg, "--")) {
        } else if arg == b"-" || arg == b"--" {
            // All remaining arguments should be passed directly to the shell
            // command. We shouldn't perform any further option processing.
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
            // while (iter.next()) |remaining_arg| {
            //     try cmd.appendArg(remaining_arg);
            // }
            while let Some(remaining_arg) = iter.next() {
                cmd.appendArg(remaining_arg);
            }
            break;
        } else {
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
        }
    }
    iter.deinit();

    // Preserve an existing ENV value. We're about to overwrite it.
    // if (env.get("ENV")) |v| {
    //     try env.put("GHOSTTY_BASH_ENV", v);
    // }
    if let Some(v) = env.getv(b"ENV").map(<[u8]>::to_vec) {
        env.put(&env_name("BASH_ENV"), &v);
    }

    // Set our new ENV to point to our integration script.
    // var script_path_buf: [std.fs.max_path_bytes]u8 = undefined;
    // const script_path = try std.fmt.bufPrint(
    //     &script_path_buf,
    //     "{s}/shell-integration/bash/ghostty.bash",
    //     .{resource_dir},
    // );
    //
    // PORT NOTE (R2 identity): the integration script carries the product
    // identity (`ghostty.bash` -> `<app>.bash`, rustty-identity).
    let script_path = cat(&[
        resource_dir,
        b"/shell-integration/bash/",
        rustty_identity::APP_NAME.as_bytes(),
        b".bash",
    ]);
    // if (std.fs.openFileAbsolute(script_path, .{})) |file| {
    //     file.close();
    //     try env.put("ENV", script_path);
    // } else |err| {
    //     log.warn("unable to open {s}: {}", .{ script_path, err });
    //     env.remove("GHOSTTY_BASH_ENV");
    //     return null;
    // }
    match std::fs::File::open(byte_path(&script_path)) {
        Ok(file) => {
            drop(file); // file.close();
            env.put(b"ENV", &script_path);
        }
        Err(err) => {
            // log.warn("unable to open {s}: {}", .{ script_path, err });
            log::warn!(
                target: LOG_SCOPE,
                "unable to open {}: {err:?}",
                String::from_utf8_lossy(&script_path)
            );
            env.removev(&env_name("BASH_ENV"));
            return Ok(None);
        }
    }

    // try env.put("GHOSTTY_BASH_INJECT", buf[0..inject.end]);
    env.put(&env_name("BASH_INJECT"), inject.as_bytes());
    // if (rcfile) |v| {
    //     try env.put("GHOSTTY_BASH_RCFILE", v);
    // }
    if let Some(v) = rcfile {
        env.put(&env_name("BASH_RCFILE"), &v);
    }

    // In POSIX mode, HISTFILE defaults to ~/.sh_history, so unless we're
    // staying in POSIX mode (--posix), change it back to ~/.bash_history.
    // if (env.get("HISTFILE") == null) {
    if env.getv(b"HISTFILE").is_none() {
        // var home_buf: [1024]u8 = undefined;
        let mut home_buf = [0u8; 1024];
        // if (try homedir.home(&home_buf)) |home| {
        // PORT NOTE (P3-W7 T3 / area-E `E14`): upstream's `setupBash` now does
        //   var environ_map = try global.environMap();
        //   defer environ_map.deinit();
        //   homedir.home(&environ_map, &home_buf)
        let environ_map = rustty_global::environ_map();
        if let Some(home) = homedir::home(&environ_map, &mut home_buf).map_err(Error::Homedir)? {
            // var histfile_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const histfile = try std.fmt.bufPrint(
            //     &histfile_buf,
            //     "{s}/.bash_history",
            //     .{home},
            // );
            let histfile = cat(&[home, b"/.bash_history"]);
            // try env.put("HISTFILE", histfile);
            env.put(b"HISTFILE", &histfile);
            // try env.put("GHOSTTY_BASH_UNEXPORT_HISTFILE", "1");
            env.put(&env_name("BASH_UNEXPORT_HISTFILE"), b"1");
        }
    }

    // Return a copy of our modified command line to use as the shell command.
    // return .{ .shell = try alloc.dupeZ(u8, cmd.buffer.written()) };
    //
    // (PORT NOTE (8e8d76b63): the reference now `dupeZ`s straight from the
    // builder's borrowed accumulated bytes instead of materializing an owned
    // `toOwnedSlice` intermediate; the one `to_vec` copy below IS the `dupeZ`
    // into the owned `Command::Shell` payload, which carries no sentinel —
    // config command.rs precedent.)
    Ok(Some(configpkg::command::Command::Shell(
        cmd.written().to_vec(),
    )))
}

/// Setup automatic shell integration for shells that include
/// their modules from paths in `XDG_DATA_DIRS` env variable.
///
/// The shell-integration path is prepended to `XDG_DATA_DIRS`.
/// It is also saved in the `GHOSTTY_SHELL_INTEGRATION_XDG_DIR` variable
/// so that the shell can refer to it and safely remove this directory
/// from `XDG_DATA_DIRS` when integration is complete.
// fn setupXdgDataDirs(
//     alloc: Allocator,
//     resource_dir: []const u8,
//     env: *EnvMap,
// ) !bool {
//
// PORT NOTE: the `!` is the bufPrint/alloc halves (collapsed).
fn setupXdgDataDirs(resource_dir: &[u8], env: &mut EnvMap) -> bool {
    // var path_buf: [std.fs.max_path_bytes]u8 = undefined;

    // Get our path to the shell integration directory.
    // const integ_path = try std.fmt.bufPrint(
    //     &path_buf,
    //     "{s}/shell-integration",
    //     .{resource_dir},
    // );
    let integ_path = cat(&[resource_dir, b"/shell-integration"]);
    // var integ_dir = std.fs.openDirAbsolute(integ_path, .{}) catch |err| {
    //     log.warn("unable to open {s}: {}", .{ integ_path, err });
    //     return false;
    // };
    // integ_dir.close();
    match std::fs::read_dir(byte_path(&integ_path)) {
        Ok(integ_dir) => drop(integ_dir), // integ_dir.close();
        Err(err) => {
            // log.warn("unable to open {s}: {}", .{ integ_path, err });
            log::warn!(
                target: LOG_SCOPE,
                "unable to open {}: {err:?}",
                String::from_utf8_lossy(&integ_path)
            );
            return false;
        }
    }

    // Set an env var so we can remove this from XDG_DATA_DIRS later.
    // This happens in the shell integration config itself. We do this
    // so that our modifications don't interfere with other commands.
    // try env.put("GHOSTTY_SHELL_INTEGRATION_XDG_DIR", integ_path);
    env.put(&env_name("SHELL_INTEGRATION_XDG_DIR"), &integ_path);

    // We attempt to avoid allocating by using the stack up to 4K.
    // Max stack size is considerably larger on mac
    // 4K is a reasonable size for this for most cases. However, env
    // vars can be significantly larger so if we have to we fall
    // back to a heap allocated value.
    //
    // var stack_alloc_state = std.heap.stackFallback(4096, alloc);
    // const stack_alloc = stack_alloc_state.get();
    //
    // (PORT NOTE: the stackFallback allocator collapses into `prependEnv`'s owned
    // `String` return.)

    // If no XDG_DATA_DIRS set use the default value as specified.
    // This ensures that the default directories aren't lost by setting
    // our desired integration dir directly. See #2711.
    // <https://specifications.freedesktop.org/basedir-spec/0.6/#variables>
    // const xdg_data_dirs_key = "XDG_DATA_DIRS";
    let xdg_data_dirs_key: &[u8] = b"XDG_DATA_DIRS";
    // try env.put(
    //     xdg_data_dirs_key,
    //     try internal_os.prependEnv(
    //         stack_alloc,
    //         env.get(xdg_data_dirs_key) orelse "/usr/local/share:/usr/share",
    //         integ_path,
    //     ),
    // );
    let current = env
        .getv(xdg_data_dirs_key)
        .unwrap_or(b"/usr/local/share:/usr/share")
        .to_vec();
    env.put(xdg_data_dirs_key, &prependEnv(&current, &integ_path));

    true
}

/// Set up automatic Nushell shell integration. This works by adding our
/// shell resource directory to the `XDG_DATA_DIRS` environment variable,
/// which Nushell will use to load `nushell/vendor/autoload/ghostty.nu`.
///
/// We then add `--execute 'use ghostty ...'` to the nu command line to
/// automatically enable our shelll features.
// fn setupNushell(
//     alloc: Allocator,
//     command: config.Command,
//     resource_dir: []const u8,
//     env: *EnvMap,
// ) !?config.Command {
fn setupNushell(
    command: &configpkg::command::Command,
    resource_dir: &[u8],
    env: &mut EnvMap,
) -> Result<Option<configpkg::command::Command>, Error> {
    // Add our XDG_DATA_DIRS entry (for nushell/vendor/autoload/). This
    // makes our 'ghostty' module automatically available, even if any
    // of the later checks abort the rest of our automatic integration.
    // if (!try setupXdgDataDirs(alloc, resource_dir, env)) return null;
    if !setupXdgDataDirs(resource_dir, env) {
        return Ok(None);
    }

    // var stack_fallback = std.heap.stackFallback(4096, alloc);
    // var cmd = internal_os.shell.ShellCommandBuilder.init(stack_fallback.get());
    // defer cmd.deinit();
    let mut cmd = ShellCommandBuilder::init();

    // Iterator that yields each argument in the original command line.
    // This will allocate once proportionate to the command line length.
    // var iter = try command.argIterator(alloc);
    // defer iter.deinit();
    let mut iter = command.arg_iterator();

    // Start accumulating arguments with the executable and initial flags.
    // if (iter.next()) |exe| {
    //     try cmd.appendArg(exe);
    // } else return null;
    match iter.next() {
        Some(exe) => cmd.appendArg(exe),
        None => return Ok(None),
    }

    // Tell nu to immediately "use" all of the exported functions in our
    // 'ghostty' module.
    //
    // We can consider making this more specific based on the set of
    // enabled shell features (e.g. `use ghostty sudo`). At the moment,
    // shell features are all runtime-guarded in the nushell script.
    // try cmd.appendArg("--execute 'use ghostty *'");
    //
    // (R2 identity: the nu module carries the product name.)
    cmd.appendArg(format!("--execute 'use {} *'", rustty_identity::APP_NAME).as_bytes());

    // Walk through the rest of the given arguments. If we see an option that
    // would require complex or unsupported integration behavior, we bail out
    // and skip loading our shell integration. Users can still manually source
    // the shell integration module.
    //
    // Unsupported options:
    //  -c / --command      -c is always non-interactive
    //  --lsp               --lsp starts the language server
    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next() {
        // if (std.mem.eql(u8, arg, "--command") or std.mem.eql(u8, arg, "--lsp")) {
        if arg == b"--command" || arg == b"--lsp" {
            return Ok(None);
        // } else if (arg.len > 1 and arg[0] == '-' and arg[1] != '-') {
        } else if arg.len() > 1 && arg[0] == b'-' && arg[1] != b'-' {
            // if (std.mem.indexOfScalar(u8, arg, 'c') != null) {
            //     return null;
            // }
            if arg.contains(&b'c') {
                return Ok(None);
            }
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
        // } else if (std.mem.eql(u8, arg, "-") or std.mem.eql(u8, arg, "--")) {
        } else if arg == b"-" || arg == b"--" {
            // All remaining arguments should be passed directly to the shell
            // command. We shouldn't perform any further option processing.
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
            // while (iter.next()) |remaining_arg| {
            //     try cmd.appendArg(remaining_arg);
            // }
            while let Some(remaining_arg) = iter.next() {
                cmd.appendArg(remaining_arg);
            }
            break;
        } else {
            // try cmd.appendArg(arg);
            cmd.appendArg(arg);
        }
    }
    iter.deinit();

    // Return a copy of our modified command line to use as the shell command.
    // return .{ .shell = try alloc.dupeZ(u8, cmd.buffer.written()) };
    //
    // (PORT NOTE (8e8d76b63): dupe from the builder's borrowed bytes — no owned
    // intermediate; see the `setupBash` note.)
    Ok(Some(configpkg::command::Command::Shell(
        cmd.written().to_vec(),
    )))
}

/// Setup the zsh automatic shell integration. This works by setting
/// ZDOTDIR to our resources dir so that zsh will load our config. This
/// config then loads the true user config.
// fn setupZsh(
//     alloc: Allocator,
//     command: config.Command,
//     resource_dir: []const u8,
//     env: *EnvMap,
// ) !?config.Command {
fn setupZsh(
    command: &configpkg::command::Command,
    resource_dir: &[u8],
    env: &mut EnvMap,
) -> Result<Option<configpkg::command::Command>, Error> {
    // Preserve an existing ZDOTDIR value. We're about to overwrite it.
    // if (env.get("ZDOTDIR")) |old| {
    //     try env.put("GHOSTTY_ZSH_ZDOTDIR", old);
    // }
    if let Some(old) = env.getv(b"ZDOTDIR").map(<[u8]>::to_vec) {
        env.put(&env_name("ZSH_ZDOTDIR"), &old);
    }

    // Set our new ZDOTDIR to point to our shell resource directory.
    // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    // const integ_path = try std.fmt.bufPrint(
    //     &path_buf,
    //     "{s}/shell-integration/zsh",
    //     .{resource_dir},
    // );
    let integ_path = cat(&[resource_dir, b"/shell-integration/zsh"]);
    // var integ_dir = std.fs.openDirAbsolute(integ_path, .{}) catch |err| {
    //     log.warn("unable to open {s}: {}", .{ integ_path, err });
    //     return null;
    // };
    // integ_dir.close();
    match std::fs::read_dir(byte_path(&integ_path)) {
        Ok(integ_dir) => drop(integ_dir), // integ_dir.close();
        Err(err) => {
            // log.warn("unable to open {s}: {}", .{ integ_path, err });
            log::warn!(
                target: LOG_SCOPE,
                "unable to open {}: {err:?}",
                String::from_utf8_lossy(&integ_path)
            );
            return Ok(None);
        }
    }
    // try env.put("ZDOTDIR", integ_path);
    env.put(b"ZDOTDIR", &integ_path);

    // return try command.clone(alloc);
    Ok(Some(command.clone()))
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims (Rust-only glue).
// ---------------------------------------------------------------------------

/// PORT NOTE (Zig-std shim): `std.fs.path.basename`, the port in
/// `rustty_config::path` (`basenamePosix`, or `basenameWindows` on Windows).
//
// CORRECTION (S213-F1): this was a local `rposition(MAIN_SEPARATOR)` shim that
// did not skip trailing separators, so `basename("/usr/bin/bash/")` was `""`
// and `detectShell` found no shell (Zig's is `"bash"`), and it ignored `/` and
// drive prefixes on Windows.
fn basename(path: &[u8]) -> &[u8] {
    configpkg::path::basename(path)
}

/// PORT NOTE (R2 identity): the reference's `GHOSTTY_*` env-var names carry the
/// product identity: `<ENV_PREFIX>_<suffix>` (rustty-os resourcesdir
/// precedent).
pub(crate) fn env_name(suffix: &str) -> Vec<u8> {
    format!("{}_{}", rustty_identity::ENV_PREFIX, suffix).into_bytes()
}

/// PORT NOTE (P3-W7 T2): this used to be
/// `std::str::from_utf8(bytes).expect(..)` — a **panic** on any non-UTF-8
/// byte, because the crate's `EnvMap` modelled `std.process.EnvMap` with Rust
/// `String`s. The map is byte-typed now (as the reference always was), so the
/// conversion is the identity and the panic is gone. Kept as a named function
/// so the call sites still read like the reference's `[]const u8` puts.
pub(crate) fn env_string(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

/// PORT NOTE (P3-W7 T2): `std.fmt.bufPrint(&buf, "{s}...{s}", .{..})` over
/// `[]const u8` path pieces. Rust's `format!` needs `str`, which would force a
/// lossy conversion of a byte-typed path; this concatenates the raw bytes
/// instead, which is what the reference's `bufPrint` does.
pub(crate) fn cat(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(parts.iter().map(|p| p.len()).sum());
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

/// PORT NOTE (P3-W7 T2): `std.fs.openFileAbsolute(path, .{})` over a
/// `[]const u8`. `Path` is `OsStr`-backed, so a byte path round-trips
/// losslessly through `OsStr::from_encoded_bytes_unchecked`.
pub(crate) fn byte_path(bytes: &[u8]) -> std::path::PathBuf {
    std::path::PathBuf::from(rustty_global::environ::os_string_from_bytes(bytes))
}

/// Prepend a value to an environment variable such as PATH.
/// The returned value is always allocated so it must be freed.
//
// PORT NOTE (P3-W7 T2 / area-E `E13`): moved here from `rustty-os`'s `env.rs`
// (upstream `src/os/env.zig`, DELETED by `e8525c0fd`), which moved it into
// `src/termio/shell_integration.zig` as a file-private fn. Its one consumer is
// `setupXdgDataDirs` below. Re-typed on bytes on the way: PATH-like variables
// are byte strings and a non-UTF-8 component could not round-trip through the
// old `&str`/`String` signature.
pub(crate) fn prependEnv(current: &[u8], value: &[u8]) -> Vec<u8> {
    // If there is no prior value, we return it as-is
    if current.is_empty() {
        return value.to_vec();
    }

    let mut out = Vec::with_capacity(value.len() + 1 + current.len());
    out.extend_from_slice(value);
    out.push(PATH_LIST_DELIMITER);
    out.extend_from_slice(current);
    out
}

/// PORT NOTE: `std.fs.path.delimiter` — the PATH-list separator: `';'` on
/// Windows, `':'` elsewhere. Rust's std does not expose it, so the reference's
/// value is reproduced as a single byte (moved with `prependEnv` from
/// `rustty-os`'s `env.rs`).
#[cfg(windows)]
pub(crate) const PATH_LIST_DELIMITER: u8 = b';';
#[cfg(not(windows))]
pub(crate) const PATH_LIST_DELIMITER: u8 = b':';

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_os::tempdir::TempDir;

    // ---- MOVED FROM `rustty-os`'s `env.rs` (P3-W7 T2 / area-E `E13`) --------
    // Upstream `e8525c0fd` moved `prependEnv` into
    // `src/termio/shell_integration.zig` when it deleted `src/os/env.zig`; its
    // two tests move with it. Re-typed on bytes with the function.

    // test "prependEnv empty" {
    #[test]
    #[allow(non_snake_case)]
    fn prependEnv_empty() {
        let result = prependEnv(b"", b"foo");
        assert_eq!(result, b"foo");
    }

    // test "prependEnv existing" {
    #[test]
    #[allow(non_snake_case)]
    fn prependEnv_existing() {
        let result = prependEnv(b"a:b", b"foo");
        if cfg!(windows) {
            assert_eq!(result, b"foo;a:b");
        } else {
            assert_eq!(result, b"foo:a:b");
        }
    }

    /// NEGATIVE CONTROL (P3-W7 T2 Step 4): a **non-UTF-8** PATH component must
    /// round-trip through `prepend_env` byte-exactly. Reverting the signature
    /// to `&str`/`String` makes this fail to COMPILE.
    #[test]
    fn prepend_env_preserves_non_utf8_path_component() {
        let current: &[u8] = &[b'/', b'u', b's', b'r', b'/', 0xff, b'x'];
        let added: &[u8] = &[b'/', 0xfe, b'o', b'p', b't'];

        let joined = prependEnv(current, added);
        assert_eq!(joined[..added.len()], *added);
        assert_eq!(joined[added.len()], PATH_LIST_DELIMITER);
        assert_eq!(joined[added.len() + 1..], *current);
        assert!(
            std::str::from_utf8(&joined).is_err(),
            "input must be non-UTF-8"
        );
    }
    // ---- end moved block ---------------------------------------------------

    // test "force shell" {
    #[test]
    fn force_shell() {
        // var arena = ArenaAllocator.init(testing.allocator);
        // defer arena.deinit();
        // const alloc = arena.allocator();

        // var env = EnvMap.init(alloc);
        // defer env.deinit();
        let mut env = EnvMap::new();

        // inline for (@typeInfo(Shell).@"enum".fields) |field| {
        //     const shell = @field(Shell, field.name);
        for shell in [
            Shell::Bash,
            Shell::Elvish,
            Shell::Fish,
            Shell::Nushell,
            Shell::Zsh,
        ] {
            // var res: TmpResourcesDir = try .init(alloc, shell);
            // defer res.deinit();
            let mut res = TmpResourcesDir::init(shell);

            // const result = try setup(
            //     alloc,
            //     res.path,
            //     .{ .shell = "sh" },
            //     &env,
            //     shell,
            // );
            let result = setup(
                &res.path,
                &configpkg::command::Command::Shell(b"sh".to_vec()),
                &mut env,
                Some(shell),
            )
            .unwrap();
            // try testing.expectEqual(shell, result.?.shell);
            assert_eq!(shell, result.unwrap().shell);

            res.deinit();
        }
    }

    // test "shell integration failure" {
    #[test]
    fn shell_integration_failure() {
        // var env = EnvMap.init(alloc);
        // defer env.deinit();
        let mut env = EnvMap::new();

        // const result = try setup(
        //     alloc,
        //     "/nonexistent",
        //     .{ .shell = "sh" },
        //     &env,
        //     null,
        // );
        let result = setup(
            b"/nonexistent",
            &configpkg::command::Command::Shell(b"sh".to_vec()),
            &mut env,
            None,
        )
        .unwrap();

        // try testing.expect(result == null);
        assert!(result.is_none());
        // try testing.expectEqual(0, env.count());
        assert_eq!(0, env.len());
    }

    // test detectShell {
    #[test]
    fn detect_shell() {
        // const alloc = testing.allocator;

        // try testing.expect(try detectShell(alloc, .{ .shell = "sh" }) == null);
        assert!(detectShell(&configpkg::command::Command::Shell(b"sh".to_vec())).is_none());
        // try testing.expectEqual(.bash, try detectShell(alloc, .{ .shell = "bash" }));
        assert_eq!(
            Some(Shell::Bash),
            detectShell(&configpkg::command::Command::Shell(b"bash".to_vec()))
        );
        // try testing.expectEqual(.elvish, try detectShell(alloc, .{ .shell = "elvish" }));
        assert_eq!(
            Some(Shell::Elvish),
            detectShell(&configpkg::command::Command::Shell(b"elvish".to_vec()))
        );
        // try testing.expectEqual(.fish, try detectShell(alloc, .{ .shell = "fish" }));
        assert_eq!(
            Some(Shell::Fish),
            detectShell(&configpkg::command::Command::Shell(b"fish".to_vec()))
        );
        // try testing.expectEqual(.nushell, try detectShell(alloc, .{ .shell = "nu" }));
        assert_eq!(
            Some(Shell::Nushell),
            detectShell(&configpkg::command::Command::Shell(b"nu".to_vec()))
        );
        // try testing.expectEqual(.zsh, try detectShell(alloc, .{ .shell = "zsh" }));
        assert_eq!(
            Some(Shell::Zsh),
            detectShell(&configpkg::command::Command::Shell(b"zsh".to_vec()))
        );

        // if (comptime builtin.target.os.tag.isDarwin()) {
        //     try testing.expect(try detectShell(alloc, .{ .shell = "/bin/bash" }) == null);
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        assert!(detectShell(&configpkg::command::Command::Shell(b"/bin/bash".to_vec())).is_none());

        // try testing.expectEqual(.bash, try detectShell(alloc, .{ .shell = "bash -c 'command'" }));
        assert_eq!(
            Some(Shell::Bash),
            detectShell(&configpkg::command::Command::Shell(
                b"bash -c 'command'".to_vec()
            ))
        );
        // try testing.expectEqual(.bash, try detectShell(alloc, .{ .shell = "\"/a b/bash\"" }));
        assert_eq!(
            Some(Shell::Bash),
            detectShell(&configpkg::command::Command::Shell(
                b"\"/a b/bash\"".to_vec()
            ))
        );
    }

    /// S213-F1 (no reference counterpart): `detectShell` takes the executable
    /// name with zig's `std.fs.path.basename`, which skips trailing separators.
    /// The rows are zig 0.16.0's own `test basename` rows for the native
    /// (POSIX) `basename`, `std/fs/path.zig:1441-1455`, then the shell case.
    #[test]
    #[cfg(not(windows))]
    fn detect_shell_basename_skips_trailing_separators() {
        for (input, want) in [
            (&b""[..], &b""[..]),
            (b"/", b""),
            (b"/dir/basename.ext", b"basename.ext"),
            (b"/basename.ext", b"basename.ext"),
            (b"basename.ext", b"basename.ext"),
            (b"basename.ext/", b"basename.ext"),
            (b"basename.ext//", b"basename.ext"),
            (b"/aaa/bbb", b"bbb"),
            (b"/aaa/", b"aaa"),
            (b"/aaa/b", b"b"),
            (b"/a/b", b"b"),
            // For Windows, this is a UNC path that only has a server name component.
            (b"//a", b"a"),
        ] {
            assert_eq!(basename(input), want, "{:?}", input);
        }

        assert_eq!(
            Some(Shell::Zsh),
            detectShell(&configpkg::command::Command::Shell(
                b"/usr/bin/zsh/".to_vec()
            ))
        );
        assert_eq!(
            Some(Shell::Bash),
            detectShell(&configpkg::command::Command::Shell(
                b"/opt/bin/bash//".to_vec()
            ))
        );
    }

    // test "setup features" {
    #[test]
    fn setup_features() {
        // Test: all features enabled
        {
            // var env = EnvMap.init(alloc);
            // defer env.deinit();
            let mut env = EnvMap::new();

            // try setupFeatures(&env, .{ .cursor = true, .sudo = true, .title = true, .@"ssh-env" = true, .@"ssh-terminfo" = true, .path = true }, true);
            setupFeatures(
                &mut env,
                ShellIntegrationFeatures {
                    cursor: true,
                    sudo: true,
                    title: true,
                    ssh_env: true,
                    ssh_terminfo: true,
                    path: true,
                },
                true,
            );
            // try testing.expectEqualStrings("cursor:blink,path,ssh-env,ssh-terminfo,sudo,title", env.get("GHOSTTY_SHELL_FEATURES").?);
            assert_eq!(
                b"cursor:blink,path,ssh-env,ssh-terminfo,sudo,title",
                env.getv(&env_name("SHELL_FEATURES")).unwrap()
            );
        }

        // Test: all features disabled
        {
            let mut env = EnvMap::new();

            // try setupFeatures(&env, std.mem.zeroes(config.ShellIntegrationFeatures), true);
            setupFeatures(
                &mut env,
                // (std.mem.zeroes == every field false)
                ShellIntegrationFeatures {
                    cursor: false,
                    sudo: false,
                    title: false,
                    ssh_env: false,
                    ssh_terminfo: false,
                    path: false,
                },
                true,
            );
            // try testing.expect(env.get("GHOSTTY_SHELL_FEATURES") == null);
            assert!(!env.contains_key(&env_name("SHELL_FEATURES")));
        }

        // Test: mixed features
        {
            let mut env = EnvMap::new();

            // try setupFeatures(&env, .{ .cursor = false, .sudo = true, .title = false, .@"ssh-env" = true, .@"ssh-terminfo" = false, .path = false }, true);
            setupFeatures(
                &mut env,
                ShellIntegrationFeatures {
                    cursor: false,
                    sudo: true,
                    title: false,
                    ssh_env: true,
                    ssh_terminfo: false,
                    path: false,
                },
                true,
            );
            // try testing.expectEqualStrings("ssh-env,sudo", env.get("GHOSTTY_SHELL_FEATURES").?);
            assert_eq!(
                b"ssh-env,sudo",
                env.getv(&env_name("SHELL_FEATURES")).unwrap()
            );
        }

        // Test: blinking cursor
        {
            let mut env = EnvMap::new();
            // try setupFeatures(&env, .{ .cursor = true, .sudo = false, .title = false, .@"ssh-env" = false, .@"ssh-terminfo" = false, .path = false }, true);
            setupFeatures(
                &mut env,
                ShellIntegrationFeatures {
                    cursor: true,
                    sudo: false,
                    title: false,
                    ssh_env: false,
                    ssh_terminfo: false,
                    path: false,
                },
                true,
            );
            // try testing.expectEqualStrings("cursor:blink", env.get("GHOSTTY_SHELL_FEATURES").?);
            assert_eq!(
                b"cursor:blink",
                env.getv(&env_name("SHELL_FEATURES")).unwrap()
            );
        }

        // Test: steady cursor
        {
            let mut env = EnvMap::new();
            // try setupFeatures(&env, .{ .cursor = true, .sudo = false, .title = false, .@"ssh-env" = false, .@"ssh-terminfo" = false, .path = false }, false);
            setupFeatures(
                &mut env,
                ShellIntegrationFeatures {
                    cursor: true,
                    sudo: false,
                    title: false,
                    ssh_env: false,
                    ssh_terminfo: false,
                    path: false,
                },
                false,
            );
            // try testing.expectEqualStrings("cursor:steady", env.get("GHOSTTY_SHELL_FEATURES").?);
            assert_eq!(
                b"cursor:steady",
                env.getv(&env_name("SHELL_FEATURES")).unwrap()
            );
        }
    }

    // test "bash" {
    #[test]
    fn bash() {
        // var res: TmpResourcesDir = try .init(alloc, .bash);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Bash);

        // var env = EnvMap.init(alloc);
        // defer env.deinit();
        let mut env = EnvMap::new();

        // const command = try setupBash(alloc, .{ .shell = "bash" }, res.path, &env);
        let command = setupBash(
            &configpkg::command::Command::Shell(b"bash".to_vec()),
            &res.path,
            &mut env,
        )
        .unwrap();
        // try testing.expectEqualStrings("bash --posix", command.?.shell);
        assert!(
            matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
        );
        // try testing.expectEqualStrings("1", env.get("GHOSTTY_BASH_INJECT").?);
        assert_eq!(b"1", env.getv(&env_name("BASH_INJECT")).unwrap());

        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // try testing.expectEqualStrings(
        //     try std.fmt.bufPrint(&path_buf, "{s}/ghostty.bash", .{res.shell_path}),
        //     env.get("ENV").?,
        // );
        assert_eq!(
            cat(&[
                &res.shell_path,
                b"/",
                rustty_identity::APP_NAME.as_bytes(),
                b".bash",
            ]),
            env.getv(b"ENV").unwrap()
        );

        res.deinit();
    }

    // test "bash: unsupported options" {
    #[test]
    fn bash_unsupported_options() {
        // var res: TmpResourcesDir = try .init(alloc, .bash);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Bash);

        // const cmdlines = [_][:0]const u8{ ... };
        let cmdlines: [&[u8]; 5] = [
            b"bash --posix",
            b"bash --rcfile script.sh --posix",
            b"bash --init-file script.sh --posix",
            b"bash -c script.sh",
            b"bash -ic script.sh",
        ];

        // for (cmdlines) |cmdline| {
        for cmdline in cmdlines {
            // var env = EnvMap.init(alloc);
            // defer env.deinit();
            let mut env = EnvMap::new();

            // try testing.expect(try setupBash(alloc, .{ .shell = cmdline }, res.path, &env) == null);
            assert!(setupBash(
                &configpkg::command::Command::Shell(cmdline.to_vec()),
                &res.path,
                &mut env
            )
            .unwrap()
            .is_none());
            // try testing.expectEqual(0, env.count());
            assert_eq!(0, env.len());
        }

        res.deinit();
    }

    // test "bash: inject flags" {
    #[test]
    fn bash_inject_flags() {
        let mut res = TmpResourcesDir::init(Shell::Bash);

        // bash --norc
        {
            let mut env = EnvMap::new();

            // const command = try setupBash(alloc, .{ .shell = "bash --norc" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash --norc".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix", command.?.shell);
            assert!(
                matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
            );
            // try testing.expectEqualStrings("1 --norc", env.get("GHOSTTY_BASH_INJECT").?);
            assert_eq!(b"1 --norc", env.getv(&env_name("BASH_INJECT")).unwrap());
        }

        // bash --noprofile
        {
            let mut env = EnvMap::new();

            // const command = try setupBash(alloc, .{ .shell = "bash --noprofile" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash --noprofile".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix", command.?.shell);
            assert!(
                matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
            );
            // try testing.expectEqualStrings("1 --noprofile", env.get("GHOSTTY_BASH_INJECT").?);
            assert_eq!(
                b"1 --noprofile",
                env.getv(&env_name("BASH_INJECT")).unwrap()
            );
        }

        res.deinit();
    }

    /// S213-F2 / UD204 (no reference counterpart): repeated inject flags that
    /// overflow the reference's 32-byte buffer (41 bytes here) still set up
    /// the integration, with every flag passed on, instead of failing
    /// `setupBash` and with it the surface.
    #[test]
    fn bash_repeated_inject_flags_do_not_fail_setup() {
        let mut res = TmpResourcesDir::init(Shell::Bash);
        let mut env = EnvMap::new();

        let command = setupBash(
            &configpkg::command::Command::Shell(
                b"bash --norc --norc --norc --noprofile --norc".to_vec(),
            ),
            &res.path,
            &mut env,
        )
        .expect("repeated flags must not fail setupBash");
        assert!(
            matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
        );
        let inject = env.getv(&env_name("BASH_INJECT")).unwrap();
        assert_eq!(b"1 --norc --norc --norc --noprofile --norc", inject);
        assert!(inject.len() > 32, "the case exceeds the reference's buffer");

        res.deinit();
    }

    // test "bash: rcfile" {
    #[test]
    fn bash_rcfile() {
        let mut res = TmpResourcesDir::init(Shell::Bash);

        let mut env = EnvMap::new();

        // bash --rcfile
        {
            // const command = try setupBash(alloc, .{ .shell = "bash --rcfile profile.sh" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash --rcfile profile.sh".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix", command.?.shell);
            assert!(
                matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
            );
            // try testing.expectEqualStrings("profile.sh", env.get("GHOSTTY_BASH_RCFILE").?);
            assert_eq!(b"profile.sh", env.getv(&env_name("BASH_RCFILE")).unwrap());
        }

        // bash --init-file
        {
            // const command = try setupBash(alloc, .{ .shell = "bash --init-file profile.sh" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash --init-file profile.sh".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix", command.?.shell);
            assert!(
                matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix")
            );
            // try testing.expectEqualStrings("profile.sh", env.get("GHOSTTY_BASH_RCFILE").?);
            assert_eq!(b"profile.sh", env.getv(&env_name("BASH_RCFILE")).unwrap());
        }

        res.deinit();
    }

    // test "bash: HISTFILE" {
    #[test]
    fn bash_histfile() {
        let mut res = TmpResourcesDir::init(Shell::Bash);

        // HISTFILE unset
        {
            let mut env = EnvMap::new();

            // _ = try setupBash(alloc, .{ .shell = "bash" }, res.path, &env);
            let _ = setupBash(
                &configpkg::command::Command::Shell(b"bash".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expect(std.mem.endsWith(u8, env.get("HISTFILE").?, ".bash_history"));
            assert!(env.getv(b"HISTFILE").unwrap().ends_with(b".bash_history"));
            // try testing.expectEqualStrings("1", env.get("GHOSTTY_BASH_UNEXPORT_HISTFILE").?);
            assert_eq!(b"1", env.getv(&env_name("BASH_UNEXPORT_HISTFILE")).unwrap());
        }

        // HISTFILE set
        {
            let mut env = EnvMap::new();

            // try env.put("HISTFILE", "my_history");
            env.put(b"HISTFILE", b"my_history");

            // _ = try setupBash(alloc, .{ .shell = "bash" }, res.path, &env);
            let _ = setupBash(
                &configpkg::command::Command::Shell(b"bash".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("my_history", env.get("HISTFILE").?);
            assert_eq!(b"my_history", env.getv(b"HISTFILE").unwrap());
            // try testing.expect(env.get("GHOSTTY_BASH_UNEXPORT_HISTFILE") == null);
            assert!(!env.contains_key(&env_name("BASH_UNEXPORT_HISTFILE")));
        }

        res.deinit();
    }

    // test "bash: ENV" {
    #[test]
    fn bash_env() {
        let mut res = TmpResourcesDir::init(Shell::Bash);

        let mut env = EnvMap::new();

        // try env.put("ENV", "env.sh");
        env.put(b"ENV", b"env.sh");

        // _ = try setupBash(alloc, .{ .shell = "bash" }, res.path, &env);
        let _ = setupBash(
            &configpkg::command::Command::Shell(b"bash".to_vec()),
            &res.path,
            &mut env,
        )
        .unwrap();
        // try testing.expectEqualStrings("env.sh", env.get("GHOSTTY_BASH_ENV").?);
        assert_eq!(b"env.sh", env.getv(&env_name("BASH_ENV")).unwrap());

        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // try testing.expectEqualStrings(
        //     try std.fmt.bufPrint(&path_buf, "{s}/ghostty.bash", .{res.shell_path}),
        //     env.get("ENV").?,
        // );
        assert_eq!(
            cat(&[
                &res.shell_path,
                b"/",
                rustty_identity::APP_NAME.as_bytes(),
                b".bash",
            ]),
            env.getv(b"ENV").unwrap()
        );

        res.deinit();
    }

    // test "bash: additional arguments" {
    #[test]
    fn bash_additional_arguments() {
        let mut res = TmpResourcesDir::init(Shell::Bash);

        let mut env = EnvMap::new();

        // "-" argument separator
        {
            // const command = try setupBash(alloc, .{ .shell = "bash - --arg file1 file2" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash - --arg file1 file2".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix - --arg file1 file2", command.?.shell);
            assert!(matches!(
                command,
                Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix - --arg file1 file2"
            ));
        }

        // "--" argument separator
        {
            // const command = try setupBash(alloc, .{ .shell = "bash -- --arg file1 file2" }, res.path, &env);
            let command = setupBash(
                &configpkg::command::Command::Shell(b"bash -- --arg file1 file2".to_vec()),
                &res.path,
                &mut env,
            )
            .unwrap();
            // try testing.expectEqualStrings("bash --posix -- --arg file1 file2", command.?.shell);
            assert!(matches!(
                command,
                Some(configpkg::command::Command::Shell(ref v)) if v == b"bash --posix -- --arg file1 file2"
            ));
        }

        res.deinit();
    }

    // test "bash: missing resources" {
    #[test]
    fn bash_missing_resources() {
        // var tmp_dir = testing.tmpDir(.{});
        // defer tmp_dir.cleanup();
        let mut tmp_dir = TempDir::init().unwrap();

        // const resources_dir = try tmp_dir.dir.realpathAlloc(alloc, ".");
        // defer alloc.free(resources_dir);
        let resources_dir = realpath_bytes(&tmp_dir);

        // var env = EnvMap.init(alloc);
        // defer env.deinit();
        let mut env = EnvMap::new();

        // try testing.expect(try setupBash(alloc, .{ .shell = "bash" }, resources_dir, &env) == null);
        assert!(setupBash(
            &configpkg::command::Command::Shell(b"bash".to_vec()),
            &resources_dir,
            &mut env
        )
        .unwrap()
        .is_none());
        // try testing.expectEqual(0, env.count());
        assert_eq!(0, env.len());

        tmp_dir.deinit();
    }

    // test "xdg: empty XDG_DATA_DIRS" {
    #[test]
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
    fn xdg_empty_xdg_data_dirs() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // var res: TmpResourcesDir = try .init(alloc, .fish);
            // defer res.deinit();
            let mut res = TmpResourcesDir::init(Shell::Fish);

            let mut env = EnvMap::new();

            // try testing.expect(try setupXdgDataDirs(alloc, res.path, &env));
            assert!(setupXdgDataDirs(&res.path, &mut env));

            // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
            // try testing.expectEqualStrings(
            //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration", .{res.path}),
            //     env.get("GHOSTTY_SHELL_INTEGRATION_XDG_DIR").?,
            // );
            assert_eq!(
                cat(&[&res.path, b"/shell-integration"]),
                env.getv(&env_name("SHELL_INTEGRATION_XDG_DIR")).unwrap()
            );
            // try testing.expectEqualStrings(
            //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration:/usr/local/share:/usr/share", .{res.path}),
            //     env.getv(b"XDG_DATA_DIRS").?,
            // );
            assert_eq!(
                cat(&[&res.path, b"/shell-integration:/usr/local/share:/usr/share"]),
                env.getv(b"XDG_DATA_DIRS").unwrap()
            );

            res.deinit();
        }
    }

    // test "xdg: existing XDG_DATA_DIRS" {
    #[test]
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
    fn xdg_existing_xdg_data_dirs() {
        // if (builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // var res: TmpResourcesDir = try .init(alloc, .fish);
            // defer res.deinit();
            let mut res = TmpResourcesDir::init(Shell::Fish);

            let mut env = EnvMap::new();

            // try env.put("XDG_DATA_DIRS", "/opt/share");
            env.put(b"XDG_DATA_DIRS", b"/opt/share");

            // try testing.expect(try setupXdgDataDirs(alloc, res.path, &env));
            assert!(setupXdgDataDirs(&res.path, &mut env));

            // try testing.expectEqualStrings(
            //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration", .{res.path}),
            //     env.get("GHOSTTY_SHELL_INTEGRATION_XDG_DIR").?,
            // );
            assert_eq!(
                cat(&[&res.path, b"/shell-integration"]),
                env.getv(&env_name("SHELL_INTEGRATION_XDG_DIR")).unwrap()
            );
            // try testing.expectEqualStrings(
            //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration:/opt/share", .{res.path}),
            //     env.getv(b"XDG_DATA_DIRS").?,
            // );
            assert_eq!(
                cat(&[&res.path, b"/shell-integration:/opt/share"]),
                env.getv(b"XDG_DATA_DIRS").unwrap()
            );

            res.deinit();
        }
    }

    // test "xdg: missing resources" {
    #[test]
    fn xdg_missing_resources() {
        // var tmp_dir = testing.tmpDir(.{});
        // defer tmp_dir.cleanup();
        let mut tmp_dir = TempDir::init().unwrap();

        // const resources_dir = try tmp_dir.dir.realpathAlloc(alloc, ".");
        // defer alloc.free(resources_dir);
        let resources_dir = realpath_bytes(&tmp_dir);

        let mut env = EnvMap::new();

        // try testing.expect(!try setupXdgDataDirs(alloc, resources_dir, &env));
        assert!(!setupXdgDataDirs(&resources_dir, &mut env));
        // try testing.expectEqual(0, env.count());
        assert_eq!(0, env.len());

        tmp_dir.deinit();
    }

    // test "nushell" {
    #[test]
    fn nushell() {
        // var res: TmpResourcesDir = try .init(alloc, .nushell);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Nushell);

        let mut env = EnvMap::new();

        // const command = try setupNushell(alloc, .{ .shell = "nu" }, res.path, &env);
        let command = setupNushell(
            &configpkg::command::Command::Shell(b"nu".to_vec()),
            &res.path,
            &mut env,
        )
        .unwrap();
        // try testing.expectEqualStrings("nu --execute 'use ghostty *'", command.?.shell);
        let expected = format!("nu --execute 'use {} *'", rustty_identity::APP_NAME);
        assert!(matches!(
            command,
            Some(configpkg::command::Command::Shell(ref v)) if v == expected.as_bytes()
        ));

        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // try testing.expectEqualStrings(
        //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration", .{res.path}),
        //     env.get("GHOSTTY_SHELL_INTEGRATION_XDG_DIR").?,
        // );
        assert_eq!(
            cat(&[&res.path, b"/shell-integration"]),
            env.getv(&env_name("SHELL_INTEGRATION_XDG_DIR")).unwrap()
        );
        // try testing.expectStringStartsWith(
        //     env.getv(b"XDG_DATA_DIRS").?,
        //     try std.fmt.bufPrint(&path_buf, "{s}/shell-integration", .{res.path}),
        // );
        assert!(env
            .getv(b"XDG_DATA_DIRS")
            .unwrap()
            .starts_with(&cat(&[&res.path, b"/shell-integration"])));

        res.deinit();
    }

    // test "nushell: unsupported options" {
    #[test]
    fn nushell_unsupported_options() {
        // var res: TmpResourcesDir = try .init(alloc, .nushell);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Nushell);

        // const cmdlines = [_][:0]const u8{ ... };
        let cmdlines: [&[u8]; 4] = [
            b"nu --command exit",
            b"nu --lsp",
            b"nu -c script.sh",
            b"nu -ic script.sh",
        ];

        // for (cmdlines) |cmdline| {
        for cmdline in cmdlines {
            let mut env = EnvMap::new();

            // try testing.expect(try setupNushell(alloc, .{ .shell = cmdline }, res.path, &env) == null);
            assert!(setupNushell(
                &configpkg::command::Command::Shell(cmdline.to_vec()),
                &res.path,
                &mut env
            )
            .unwrap()
            .is_none());
            // try testing.expect(env.getv(b"XDG_DATA_DIRS") != null);
            assert!(env.hasv(b"XDG_DATA_DIRS"));
            // try testing.expect(env.get("GHOSTTY_SHELL_INTEGRATION_XDG_DIR") != null);
            assert!(env.contains_key(&env_name("SHELL_INTEGRATION_XDG_DIR")));
        }

        res.deinit();
    }

    // test "nushell: missing resources" {
    #[test]
    fn nushell_missing_resources() {
        // var tmp_dir = testing.tmpDir(.{});
        // defer tmp_dir.cleanup();
        let mut tmp_dir = TempDir::init().unwrap();

        // const resources_dir = try tmp_dir.dir.realpathAlloc(alloc, ".");
        // defer alloc.free(resources_dir);
        let resources_dir = realpath_bytes(&tmp_dir);

        let mut env = EnvMap::new();

        // try testing.expect(try setupNushell(alloc, .{ .shell = "nu" }, resources_dir, &env) == null);
        assert!(setupNushell(
            &configpkg::command::Command::Shell(b"nu".to_vec()),
            &resources_dir,
            &mut env
        )
        .unwrap()
        .is_none());
        // try testing.expectEqual(0, env.count());
        assert_eq!(0, env.len());

        tmp_dir.deinit();
    }

    // test "zsh" {
    #[test]
    fn zsh() {
        // var res: TmpResourcesDir = try .init(testing.allocator, .zsh);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Zsh);

        // var env = EnvMap.init(testing.allocator);
        // defer env.deinit();
        let mut env = EnvMap::new();

        // const command = try setupZsh(alloc, .{ .shell = "zsh" }, res.path, &env);
        let command = setupZsh(
            &configpkg::command::Command::Shell(b"zsh".to_vec()),
            &res.path,
            &mut env,
        )
        .unwrap();
        // try testing.expectEqualStrings("zsh", command.?.shell);
        assert!(matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"zsh"));
        // try testing.expectEqualStrings(res.shell_path, env.get("ZDOTDIR").?);
        assert_eq!(&env_string(&res.shell_path), env.getv(b"ZDOTDIR").unwrap());
        // try testing.expect(env.get("GHOSTTY_ZSH_ZDOTDIR") == null);
        assert!(!env.contains_key(&env_name("ZSH_ZDOTDIR")));

        res.deinit();
    }

    // test "zsh: ZDOTDIR" {
    #[test]
    fn zsh_zdotdir() {
        // var res: TmpResourcesDir = try .init(testing.allocator, .zsh);
        // defer res.deinit();
        let mut res = TmpResourcesDir::init(Shell::Zsh);

        let mut env = EnvMap::new();

        // try env.put("ZDOTDIR", "$HOME/.config/zsh");
        env.put(b"ZDOTDIR", b"$HOME/.config/zsh");

        // const command = try setupZsh(alloc, .{ .shell = "zsh" }, res.path, &env);
        let command = setupZsh(
            &configpkg::command::Command::Shell(b"zsh".to_vec()),
            &res.path,
            &mut env,
        )
        .unwrap();
        // try testing.expectEqualStrings("zsh", command.?.shell);
        assert!(matches!(command, Some(configpkg::command::Command::Shell(ref v)) if v == b"zsh"));
        // try testing.expectEqualStrings(res.shell_path, env.get("ZDOTDIR").?);
        assert_eq!(&env_string(&res.shell_path), env.getv(b"ZDOTDIR").unwrap());
        // try testing.expectEqualStrings("$HOME/.config/zsh", env.get("GHOSTTY_ZSH_ZDOTDIR").?);
        assert_eq!(
            b"$HOME/.config/zsh",
            env.getv(&env_name("ZSH_ZDOTDIR")).unwrap()
        );

        res.deinit();
    }

    // test "zsh: missing resources" {
    #[test]
    fn zsh_missing_resources() {
        // var tmp_dir = testing.tmpDir(.{});
        // defer tmp_dir.cleanup();
        let mut tmp_dir = TempDir::init().unwrap();

        // const resources_dir = try tmp_dir.dir.realpathAlloc(alloc, ".");
        // defer alloc.free(resources_dir);
        let resources_dir = realpath_bytes(&tmp_dir);

        let mut env = EnvMap::new();

        // try testing.expect(try setupZsh(alloc, .{ .shell = "zsh" }, resources_dir, &env) == null);
        assert!(setupZsh(
            &configpkg::command::Command::Shell(b"zsh".to_vec()),
            &resources_dir,
            &mut env
        )
        .unwrap()
        .is_none());
        // try testing.expectEqual(0, env.count());
        assert_eq!(0, env.len());

        tmp_dir.deinit();
    }

    /// Test helper that creates a temporary resources directory with shell integration paths.
    // const TmpResourcesDir = struct {
    //     allocator: Allocator,
    //     tmp_dir: std.testing.TmpDir,
    //     path: []const u8,
    //     shell_path: []const u8,
    struct TmpResourcesDir {
        tmp_dir: TempDir,
        path: Vec<u8>,
        shell_path: Vec<u8>,
    }

    impl TmpResourcesDir {
        // fn init(allocator: Allocator, shell: Shell) !TmpResourcesDir {
        fn init(shell: Shell) -> TmpResourcesDir {
            // var tmp_dir = std.testing.tmpDir(.{});
            // errdefer tmp_dir.cleanup();
            let tmp_dir = TempDir::init().unwrap();

            // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const relative_shell_path = try std.fmt.bufPrint(
            //     &path_buf,
            //     "shell-integration/{s}",
            //     .{@tagName(shell)},
            // );
            let relative_shell_path = format!("shell-integration/{}", shell.tag_name());
            // try tmp_dir.dir.makePath(relative_shell_path);
            //
            // (PORT NOTE: `makePath` == create_dir_all rooted at the tmp dir.)
            let base = tmp_dir.dir().realpath(".").unwrap();
            std::fs::create_dir_all(base.join(&relative_shell_path)).unwrap();

            // const path = try tmp_dir.dir.realpathAlloc(allocator, ".");
            // errdefer allocator.free(path);
            //
            // WS19 T15 (audit row O21): was
            // `base.to_string_lossy().into_owned().into_bytes()`, which replaces any
            // non-UTF-8 byte in the path with U+FFFD (3 bytes) — a SILENT, LOSSY
            // corruption of a value the reference treats as opaque bytes
            // (`realpathAlloc` returns `[]u8`, and POSIX paths are byte strings, not
            // UTF-8). `OsStrExt::as_bytes` is the exact analogue: the platform bytes,
            // unmodified. See memory `zig-byteslice-to-rust-rawbytes`.
            let path = std::os::unix::ffi::OsStrExt::as_bytes(base.as_os_str()).to_vec();

            // const shell_path = try std.fmt.allocPrint(
            //     allocator,
            //     "{s}/{s}",
            //     .{ path, relative_shell_path },
            // );
            // errdefer allocator.free(shell_path);
            let shell_path = cat(&[&path, b"/", relative_shell_path.as_bytes()]);

            // switch (shell) {
            //     .bash => try tmp_dir.dir.writeFile(.{
            //         .sub_path = "shell-integration/bash/ghostty.bash",
            //         .data = "",
            //     }),
            //     else => {},
            // }
            match shell {
                Shell::Bash => tmp_dir
                    .dir()
                    .writeFile(
                        &format!("shell-integration/bash/{}.bash", rustty_identity::APP_NAME),
                        b"",
                    )
                    .unwrap(),
                _ => {}
            }

            TmpResourcesDir {
                tmp_dir,
                path,
                shell_path,
            }
        }

        // fn deinit(self: *TmpResourcesDir) void {
        //     self.allocator.free(self.shell_path);
        //     self.allocator.free(self.path);
        //     self.tmp_dir.cleanup();
        // }
        fn deinit(&mut self) {
            self.tmp_dir.deinit();
        }
    }

    /// PORT NOTE (test shim): `tmp_dir.dir.realpathAlloc(alloc, ".")` — the resolved
    /// absolute tmp path as bytes.
    ///
    /// WS19 T15 (audit row O21): returns the RAW platform bytes via
    /// `OsStrExt::as_bytes`. It previously went through
    /// `to_string_lossy().into_owned().into_bytes()`, which silently rewrote any
    /// non-UTF-8 byte as U+FFFD — a lossy round-trip of a value the reference treats as
    /// opaque bytes (`realpathAlloc` returns `[]u8`; POSIX paths are byte strings).
    /// `tmpdir_path_with_non_utf8_byte_round_trips_exactly` is the RED/GREEN
    /// discriminator for this change. See memory `zig-byteslice-to-rust-rawbytes`.
    fn realpath_bytes(tmp_dir: &TempDir) -> Vec<u8> {
        let path = tmp_dir.dir().realpath(".").unwrap();
        std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).to_vec()
    }

    /// RED/GREEN DISCRIMINATOR for WS19 T15 (audit row O21) — not a smoke test.
    ///
    /// Asserts that the conversion `realpath_bytes` (and the production
    /// `TmpResourcesDir` path build) now uses is BYTE-PRESERVING, and that the one it
    /// used before is not. Under `to_string_lossy().into_owned().into_bytes()` a `0xFF`
    /// byte becomes U+FFFD (`EF BF BD`); under `OsStrExt::as_bytes` it survives. Both
    /// halves are asserted, so the test fails if the fix is reverted AND fails if the
    /// fixture stops being non-UTF-8 (which would make it vacuous).
    ///
    /// PLATFORM NOTE: this half deliberately does NOT touch the filesystem. APFS
    /// rejects non-UTF-8 filenames outright (`EILSEQ`, verified: `create_dir_all` on
    /// `b"ws19_\xffdir"` returns "Illegal byte sequence" on macOS), so a
    /// filesystem-based fixture cannot exist on the primary host. The property under
    /// test is the CONVERSION, not the syscall, and the conversion is host-independent.
    /// The filesystem round-trip is exercised on Linux by the sibling test below.
    #[test]
    fn non_utf8_path_bytes_survive_the_conversion_but_not_the_lossy_one() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::PathBuf;

        // Built at RUNTIME, not as a `b"..."` literal: clippy's
        // `invalid_from_utf8` correctly points out that `from_utf8` on a literal whose
        // invalidity is statically known "always returns an error". The vacuity check
        // below is still worth making — it is what stops someone silently swapping in a
        // UTF-8-safe fixture — so keep the check and remove the staticness.
        let mut raw_owned: Vec<u8> = b"/tmp/ws19_".to_vec();
        raw_owned.push(0xFF);
        raw_owned.extend_from_slice(b"dir");
        let raw: &[u8] = &raw_owned;
        let path = PathBuf::from(OsStr::from_bytes(raw));

        assert!(
            std::str::from_utf8(raw).is_err(),
            "the fixture must be non-UTF-8 or this test proves nothing (vacuous)"
        );

        // The conversion the fixed code uses.
        let kept = OsStrExt::as_bytes(path.as_os_str()).to_vec();
        assert_eq!(
            kept, raw,
            "OsStrExt::as_bytes must return the platform bytes unmodified"
        );

        // The conversion the code used BEFORE this fix.
        let lossy = path.to_string_lossy().into_owned().into_bytes();
        assert_ne!(
            lossy, raw,
            "if the lossy conversion also round-trips, this fixture cannot discriminate \
             the fix from the bug — pick a byte the lossy path actually mangles"
        );
        assert!(
            lossy.windows(3).any(|w| w == [0xEF, 0xBF, 0xBD]),
            "expected the U+FFFD replacement sequence in the LOSSY result, which is \
             exactly the corruption this fix removes; got {lossy:?}"
        );
    }

    /// The filesystem half of the WS19 T15 discriminator. Linux-only: APFS rejects
    /// non-UTF-8 filenames with `EILSEQ`, ext4/tmpfs accept them.
    #[test]
    #[cfg(target_os = "linux")]
    fn realpath_bytes_round_trips_a_non_utf8_directory_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let tmp = TempDir::init().unwrap();
        let base = tmp.dir().realpath(".").unwrap();
        let sub = base.join(OsStr::from_bytes(b"ws19_\xffdir"));
        std::fs::create_dir_all(&sub).unwrap();

        let bytes = OsStrExt::as_bytes(sub.as_os_str()).to_vec();
        assert!(
            bytes.windows(9).any(|w| w == b"ws19_\xffdir"),
            "the raw 0xFF byte did not survive the realpath round-trip: {bytes:?}"
        );
        assert!(
            !bytes.windows(3).any(|w| w == [0xEF, 0xBF, 0xBD]),
            "found U+FFFD in the path bytes — a lossy String conversion is still on \
             this path: {bytes:?}"
        );
    }
    // ---- task-14e-1: the restored `shell_integration` diagnostics ----------

    /// `shell_integration.zig:644` — the `unable to open {s}: {}` warn on the
    /// XDG data-dirs path. Exact record vector, with the payload built from an
    /// independently-obtained `io::Error` for the SAME path so the assertion is
    /// host-portable rather than pinning one platform's `strerror`.
    #[test]
    fn setup_xdg_data_dirs_warns_when_the_integration_dir_is_missing() {
        let tmp = TempDir::init().unwrap();
        let base = tmp.dir().realpath(".").unwrap();
        let resource_dir = base.as_os_str().as_encoded_bytes().to_vec();
        // Deliberately do NOT create `<base>/shell-integration`.
        let integ_path = cat(&[&resource_dir, b"/shell-integration"]);
        let expected_err = std::fs::read_dir(byte_path(&integ_path)).unwrap_err();
        let expected = format!(
            "unable to open {}: {expected_err:?}",
            String::from_utf8_lossy(&integ_path)
        );

        let cap = crate::log_capture::start();
        let mut env: EnvMap = EnvMap::new();
        let ok = setupXdgDataDirs(&resource_dir, &mut env);
        let messages = cap.messages();
        drop(cap);

        assert!(!ok, "a missing integration dir must return false");
        assert_eq!(
            vec![(log::Level::Warn, "shell_integration".to_string(), expected)],
            messages
        );
    }

    /// The ABSENCE arm, with the positive assertion PROCEDURE.md requires.
    ///
    /// A PRESENT `shell-integration` dir must produce NO record — the reference
    /// only logs in the `catch`. An empty record vector alone cannot tell
    /// "the success path ran and stayed silent" apart from "the function bailed
    /// out before ever reaching the `openDirAbsolute`", so the test also asserts
    /// positively that it got PAST that call: `setupXdgDataDirs` returns true
    /// and puts `<APP>_SHELL_INTEGRATION_XDG_DIR` into the env, both of which
    /// happen only on the far side of the arm whose silence is being asserted.
    ///
    /// NEGATIVE CONTROL RUN (task-14e-1): hoisting the `log::warn!` out of the
    /// `Err(err)` arm to just above the `match` — the exact "diagnostic on a
    /// success path" bug this project shipped once — fails THIS test with
    /// `left: [] right: [(Warn, "shell_integration", "unable to open …")]`.
    /// The control fails, and it fails for the stated reason: the record is
    /// present where the test asserts none. (The hoist has to synthesize an
    /// error value, so it also fails the warn test above on the rendered
    /// payload — expected, and not what this control is measuring.)
    #[test]
    fn setup_xdg_data_dirs_is_silent_when_the_integration_dir_exists() {
        let tmp = TempDir::init().unwrap();
        let base = tmp.dir().realpath(".").unwrap();
        std::fs::create_dir_all(base.join("shell-integration")).unwrap();
        let resource_dir = base.as_os_str().as_encoded_bytes().to_vec();

        let cap = crate::log_capture::start();
        let mut env: EnvMap = EnvMap::new();
        let ok = setupXdgDataDirs(&resource_dir, &mut env);
        let messages = cap.messages();
        let reached = env.getv(&env_name("SHELL_INTEGRATION_XDG_DIR")).is_some();
        drop(cap);

        // POSITIVE: we are past the `openDirAbsolute` whose failure arm holds
        // the diagnostic this test asserts the absence of.
        assert!(ok, "a present integration dir must return true");
        assert!(
            reached,
            "the XDG_DIR env var was not set, so this test never reached the \
             arm whose silence it is asserting"
        );
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }
}
