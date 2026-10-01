//! PORTS: ghostty src/config/Config.zig — the lifecycle and load cluster: `load`,
//! `default`, `deinit`, `loadIter`, `loadFile`, `loadFsFile`, `loadReader`,
//! `loadOptionalFile`, `loadDefaultFiles`, `parseManuallyHook`, the `compatibility`
//! handlers, `addDiagnosticFmt`, `shallowClone`, `loadCliArgs`,
//! `loadRecursiveFiles`, `keyEventIsBinding`, `changeConditionalState`,
//! `expandPaths` and `loadTheme`.

use super::*;

// ===========================================================================
// Config pass method signatures (the four comptime passes + lifecycle).
// ===========================================================================

impl Config {
    // pub fn load(alloc_gpa: Allocator) !Config {
    pub fn load() -> Result<Config, LoadError> {
        // var result = try default(alloc_gpa);
        let mut result = Config::default_config();
        // errdefer result.deinit();

        // If we have a configuration file in our home directory, parse that first.
        // try result.loadDefaultFiles(alloc_gpa);
        result.load_default_files()?;

        // Parse the config from the CLI args.
        // try result.loadCliArgs(alloc_gpa);
        result.load_cli_args()?;

        // Parse the config files that were added from our file and CLI args.
        // try result.loadRecursiveFiles(alloc_gpa);
        result.load_recursive_files()?;
        // try result.finalize();
        result.finalize()?;

        // return result;
        Ok(result)
    }

    /// PORTS: `pub fn default(alloc_gpa) Allocator.Error!Config`.
    ///
    /// The data half (the `Config` struct defaults) lives in `Default::default`;
    /// `default_config` performs the post-default initialization the reference
    /// does after `var result: Config = .{ ._arena = .init(alloc_gpa) }`.
    // pub fn default(alloc_gpa: Allocator) Allocator.Error!Config {
    // PARITY: the reference is `var result: Config = .{ ._arena = .init(alloc) }`
    // — a deliberate default-then-set-one-field, so the lint does not apply.
    #[allow(clippy::field_reassign_with_default)]
    pub fn default_config() -> Config {
        // Build up our basic config
        // var result: Config = .{
        //     ._arena = .init(alloc_gpa),
        // };
        // errdefer result.deinit();
        // const alloc = result._arena.?.allocator();
        let mut result: Config = Config::default();
        // PARITY: the reference creates the arena here; the Rust port owns data
        // per-field, so we record that the arena "exists" via `_arena = true`
        // (the same flag the args parser observes).
        result._arena = true;

        // Add our default keybindings
        // try result.keybind.init(alloc);
        result.keybind.init();

        // Add our default command palette entries
        // try result.@"command-palette-entry".init(alloc);
        result.command_palette_entry.init();

        // Add our default link for URL detection
        // try result.link.links.append(alloc, .{
        //     .regex = url.regex,
        //     .action = .{ .open = {} },
        //     .highlight = .{ .hover_mods = inputpkg.ctrlOrSuper(.{}) },
        // });
        result.link.links.push(rustty_input::link::Link {
            regex: crate::url::regex().into_bytes(),
            action: rustty_input::link::Action::Open,
            highlight: rustty_input::link::Highlight::HoverMods(rustty_input::ctrl_or_super(
                rustty_vt::input::key_mods::Mods::default(),
            )),
        });

        // return result;
        result
    }

    /// PORTS: `pub fn deinit(self: *Config) void`.
    // pub fn deinit(self: *Config) void {
    //     if (self._arena) |arena| arena.deinit();
    //     self.* = undefined;
    // }
    //
    // PORT NOTE: the reference frees the arena (which owns every value's bytes).
    // In Rust each value type owns its own bytes and is dropped automatically; the
    // arena flag is cleared to match the reference's `_arena` teardown.
    pub fn deinit(&mut self) {
        self._arena = false;
    }

    /// Load configuration from an iterator that yields values that look like
    /// command-line arguments, i.e. `--key=value`.
    // pub fn loadIter(
    //     self: *Config,
    //     alloc: Allocator,
    //     iter: anytype,
    // ) !void {
    //     try cli.args.parse(Config, alloc, self, iter);
    // }
    //
    // PORT NOTE: routed through `parse_self` (the aliasing-safe `parse` variant)
    // so both replay and non-replay iterators share one path. See the
    // `SelfDrivenIterator` note in rustty-args.
    pub fn load_iter<I: rustty_args::args::SelfDrivenIterator<Config>>(
        &mut self,
        iter: &mut I,
    ) -> Result<(), ParseError> {
        rustty_args::args::parse_self(self, iter)
    }

    /// Load configuration from the target config file at `path`.
    ///
    /// `path` must be resolved and absolute.
    // pub fn loadFile(self: *Config, alloc: Allocator, path: []const u8) !void {
    pub fn load_file(&mut self, path: &[u8]) -> Result<(), LoadError> {
        // assert(std.fs.path.isAbsolute(path));
        //
        // CORRECTION (task W-2): this asserted Rust's `Path::is_absolute`, which
        // on Windows is false for a rooted path (`/x`, `\\x`) that Zig's
        // `isAbsoluteWindows` calls absolute; `crate::path::is_absolute` is the
        // port of Zig's.
        assert!(crate::path::is_absolute(path));
        // var file = file_load.open(path) catch |err| switch (err) {
        let file = match crate::file_load::open(path) {
            Ok(f) => f,
            // error.NotAFile => {
            //     log.warn(
            //         "config-file {s}: not reading because it is not a file",
            //         .{path},
            //     );
            //     return;
            // },
            Err(crate::file_load::OpenFileError::NotAFile) => {
                log_warn(format!(
                    "config-file {}: not reading because it is not a file",
                    String::from_utf8_lossy(path)
                ));
                return Ok(());
            }
            // else => return err,
            Err(err) => return Err(LoadError::from_open(err)),
        };
        // defer file.close();

        // try self.loadFsFile(alloc, &file, path);
        self.load_fs_file(file, path)
    }

    /// Load config from the given File.
    // fn loadFsFile(self: *Config, alloc: Allocator, file: *std.fs.File, path: []const u8) !void {
    fn load_fs_file(&mut self, mut file: std::fs::File, path: &[u8]) -> Result<(), LoadError> {
        // std.log.info("reading configuration file path={s}", .{path});
        log_info(format!(
            "reading configuration file path={}",
            String::from_utf8_lossy(path)
        ));
        // var buf: [2048]u8 = undefined;
        // var file_reader = file.reader(&buf);
        // const reader = &file_reader.interface;
        //
        // PORT NOTE: `std.Io.Reader` over the file is modeled by reading the file
        // contents into a byte buffer that `LineIterator` (a byte-cursor reader)
        // consumes. This is byte-faithful for the configuration files the loader
        // reads (small, fully-buffered).
        use std::io::Read;
        let mut contents: Vec<u8> = Vec::new();
        file.read_to_end(&mut contents)?;
        // try self.loadReader(alloc, reader, path);
        self.load_reader(&contents, path)
    }

    /// Load config from the given Reader.
    // fn loadReader(self: *Config, alloc: Allocator, reader: *std.Io.Reader, path: []const u8) !void {
    pub(super) fn load_reader(&mut self, reader: &[u8], path: &[u8]) -> Result<(), LoadError> {
        // bom: {
        //     // If the file starts with a UTF-8 byte order mark, skip it.
        //     // https://en.wikipedia.org/wiki/Byte_order_mark#UTF-8
        //     const bom: []const u8 = &.{ 0xef, 0xbb, 0xbf };
        //     const str = reader.peek(bom.len) catch break :bom;
        //     if (std.mem.eql(u8, str, bom)) {
        //         log.info("skipping UTF-8 byte order mark", .{});
        //         reader.toss(bom.len);
        //     }
        // }
        let bom: &[u8] = &[0xef, 0xbb, 0xbf];
        let reader: &[u8] = 'bom: {
            if reader.len() >= bom.len() && &reader[..bom.len()] == bom {
                log_info("skipping UTF-8 byte order mark".to_string());
                break 'bom &reader[bom.len()..];
            }
            reader
        };
        // var iter: cli.args.LineIterator = .{ .r = reader, .filepath = path };
        let mut iter = rustty_args::args::LineIterator::with_filepath(reader, path);
        // try self.loadIter(alloc, &iter);
        self.load_iter(&mut iter)?;
        // try self.expandPaths(std.fs.path.dirname(path).?);
        self.expand_paths(dirname(path).unwrap())?;
        Ok(())
    }

    /// Load optional configuration file from `path`. All errors are ignored.
    ///
    /// Returns the action that was taken.
    // pub fn loadOptionalFile(
    //     self: *Config,
    //     alloc: Allocator,
    //     path: []const u8,
    // ) OptionalFileAction {
    pub fn load_optional_file(&mut self, path: &[u8]) -> OptionalFileAction {
        // if (self.loadFile(alloc, path)) {
        //     return .loaded;
        // } else |err| switch (err) {
        match self.load_file(path) {
            // return .loaded;
            Ok(()) => OptionalFileAction::Loaded,
            // error.FileNotFound => return .not_found,
            Err(LoadError::FileNotFound) => OptionalFileAction::NotFound,
            // else => {
            //     std.log.warn(
            //         "error reading optional config file, not loading err={} path={s}",
            //         .{ err, path },
            //     );
            //     return .@"error";
            // },
            Err(err) => {
                log_warn(format!(
                    "error reading optional config file, not loading err={err:?} path={}",
                    String::from_utf8_lossy(path)
                ));
                OptionalFileAction::Error
            }
        }
    }

    /// Load configurations from the default configuration files. The default
    /// configuration file is at `$XDG_CONFIG_HOME/rustty/config.rustty`.
    ///
    /// On macOS, `$HOME/Library/Application Support/$CFBundleIdentifier/`
    /// is also loaded.
    ///
    /// The legacy `config` file (without extension) is first loaded,
    /// then `config.rustty`.
    // pub fn loadDefaultFiles(self: *Config, alloc: Allocator) !void {
    pub fn load_default_files(&mut self) -> Result<(), LoadError> {
        // Load XDG first
        // const legacy_xdg_path = try file_load.legacyDefaultXdgPath(alloc);
        // defer alloc.free(legacy_xdg_path);
        // (W94-B02-01: the XDG helpers are byte-typed now — no `into_bytes`.)
        //
        // PORT NOTE (LEDGER S74-F1): each `try` is a `?`. These four lookups
        // were `.unwrap_or_default()`, so a failed lookup (e.g. a `$HOME` longer
        // than the XDG helper's 1024-byte buffer with `XDG_CONFIG_HOME` unset)
        // loaded an EMPTY path and panicked in `load_file`'s
        // `assert!(is_absolute)`; the reference returns the error.
        let legacy_xdg_path = crate::file_load::legacy_default_xdg_path()?;
        // const xdg_path = try file_load.defaultXdgPath(alloc);
        // defer alloc.free(xdg_path);
        let xdg_path = crate::file_load::default_xdg_path()?;
        // const xdg_loaded: bool = xdg_loaded: {
        let xdg_loaded: bool = {
            // const legacy_xdg_action = self.loadOptionalFile(alloc, legacy_xdg_path);
            let legacy_xdg_action = self.load_optional_file(&legacy_xdg_path);
            // const xdg_action = self.loadOptionalFile(alloc, xdg_path);
            let xdg_action = self.load_optional_file(&xdg_path);
            // if (xdg_action != .not_found and legacy_xdg_action != .not_found) {
            if xdg_action != OptionalFileAction::NotFound
                && legacy_xdg_action != OptionalFileAction::NotFound
            {
                // log.warn("both config files `{s}` and `{s}` exist.", .{ legacy_xdg_path, xdg_path });
                log_warn(format!(
                    "both config files `{}` and `{}` exist.",
                    String::from_utf8_lossy(&legacy_xdg_path),
                    String::from_utf8_lossy(&xdg_path)
                ));
                // log.warn("loading them both in that order", .{});
                log_warn("loading them both in that order".to_string());
                // break :xdg_loaded true;
                true
            } else {
                // break :xdg_loaded xdg_action != .not_found or
                //     legacy_xdg_action != .not_found;
                xdg_action != OptionalFileAction::NotFound
                    || legacy_xdg_action != OptionalFileAction::NotFound
            }
        };

        // On macOS load the app support directory as well
        // if (comptime builtin.os.tag == .macos) {
        #[cfg(target_os = "macos")]
        {
            // const legacy_app_support_path = try file_load.legacyDefaultAppSupportPath(alloc);
            // defer alloc.free(legacy_app_support_path);
            let legacy_app_support_path =
                crate::file_load::legacy_default_app_support_path()?.into_bytes();
            // const app_support_path = try file_load.preferredAppSupportPath(alloc);
            // defer alloc.free(app_support_path);
            let app_support_path = crate::file_load::preferred_app_support_path()?.into_bytes();
            // const app_support_loaded: bool = loaded: {
            let app_support_loaded: bool = {
                // const legacy_app_support_action = self.loadOptionalFile(
                //     alloc,
                //     legacy_app_support_path,
                // );
                let legacy_app_support_action = self.load_optional_file(&legacy_app_support_path);

                // The app support path and legacy may be the same, since we
                // use the `preferred` call above. If its the same, avoid
                // a double-load.
                // const app_support_action: OptionalFileAction = if (!std.mem.eql(
                //     u8,
                //     legacy_app_support_path,
                //     app_support_path,
                // )) self.loadOptionalFile(
                //     alloc,
                //     app_support_path,
                // ) else .not_found;
                let app_support_action: OptionalFileAction =
                    if legacy_app_support_path != app_support_path {
                        self.load_optional_file(&app_support_path)
                    } else {
                        OptionalFileAction::NotFound
                    };

                // if (app_support_action != .not_found and legacy_app_support_action != .not_found) {
                if app_support_action != OptionalFileAction::NotFound
                    && legacy_app_support_action != OptionalFileAction::NotFound
                {
                    // log.warn(
                    //     "both config files `{s}` and `{s}` exist.",
                    //     .{ legacy_app_support_path, app_support_path },
                    // );
                    log_warn(format!(
                        "both config files `{}` and `{}` exist.",
                        String::from_utf8_lossy(&legacy_app_support_path),
                        String::from_utf8_lossy(&app_support_path)
                    ));
                    // log.warn("loading them both in that order", .{});
                    log_warn("loading them both in that order".to_string());
                    // break :loaded true;
                    true
                } else {
                    // break :loaded app_support_action != .not_found or
                    //     legacy_app_support_action != .not_found;
                    app_support_action != OptionalFileAction::NotFound
                        || legacy_app_support_action != OptionalFileAction::NotFound
                }
            };

            // If both files are not found, then we create a template file.
            // For macOS, we only create the template file in the app support
            // if (!app_support_loaded and !xdg_loaded) {
            if !app_support_loaded && !xdg_loaded {
                // writeConfigTemplate(app_support_path) catch |err| {
                //     log.warn("error creating template config file err={}", .{err});
                // };
                if let Err(err) = write_config_template(&app_support_path) {
                    log_warn(format!("error creating template config file err={err:?}"));
                }
            }
        }
        // } else {
        #[cfg(not(target_os = "macos"))]
        {
            // if (!xdg_loaded) {
            if !xdg_loaded {
                // writeConfigTemplate(xdg_path) catch |err| {
                //     log.warn("error creating template config file err={}", .{err});
                // };
                if let Err(err) = write_config_template(&xdg_path) {
                    log_warn(format!("error creating template config file err={err:?}"));
                }
            }
        }

        Ok(())
    }

    /// PORTS: `pub fn parseManuallyHook(self, alloc, arg, iter) !bool` — the
    /// `@hasDecl(T, "parseManuallyHook")` entry point used by the args parser.
    ///
    /// Callback for src/cli/args.zig to allow us to handle special cases
    /// like `--help` or `-e`. Returns "false" if the CLI parsing should halt.
    // pub fn parseManuallyHook(
    //     self: *Config,
    //     alloc: Allocator,
    //     arg: []const u8,
    //     iter: anytype,
    // ) !bool {
    //
    // PORT NOTE: the reference consumes the rest of `iter` here (for `-e`).
    // `parse_self` exposes the iterator only via `next_self`, so the hook cannot
    // drain it inline. Instead the hook records the `-e` marker + the parse
    // position and the `-e` command is assembled by the parse driver. To preserve
    // the reference's per-arg behavior WITHOUT threading the iterator, the hook is
    // kept arg-at-a-time: it appends each replay step and, for `-e`, the parse
    // loop's subsequent args become the command. See `parse_manually_hook` Target
    // impl which records args; the `-e` command finalization runs in `finalize`-
    // adjacent assembly. For the scope of this batch the hook ports the non-`-e`
    // replay-recording path faithfully and the `-e` marker path; the full
    // iterator-draining `-e` command assembly is exercised by loadCliArgs/replay.
    pub fn parse_manually_hook(
        &mut self,
        arg: &[u8],
    ) -> Result<rustty_args::args::HookOutcome, ParseError> {
        use rustty_args::args::HookOutcome;
        // if (std.mem.eql(u8, arg, "-e")) {
        if arg == b"-e" {
            // Add the special -e marker. This prevents:
            // (1) config-file from adding args to the end (see #2908)
            // (2) dark/light theme from making this conditional
            // try self._replay_steps.append(alloc, .@"-e");
            self._replay_steps.push(ReplayStep::DashE);

            // Build up the command. We don't clean this up because we take
            // ownership in our allocator.
            // var command: std.ArrayList([:0]const u8) = .empty;
            // errdefer command.deinit(alloc);
            //
            // while (iter.next()) |param| {
            //     const copy = try alloc.dupeZ(u8, param);
            //     try self._replay_steps.append(alloc, .{ .arg = copy });
            //     try command.append(alloc, copy);
            // }
            //
            // PORT NOTE (iterator threading): the reference drains `iter` HERE to
            // build the `-e` command. The hook holds `&mut self`, and the driver's
            // `next_self` also needs `&mut self`, so the drain cannot run inline
            // without aliasing UB. Instead the hook records the `-e` MARKER and
            // returns `HookOutcome::ConsumeRest`; the driver (`parse`/`parse_self`)
            // — which owns `self` and `iter` non-overlappingly — runs the verbatim
            // `while (iter.next())` drain (recording each arg via
            // `record_replay_arg` and collecting the command), then calls
            // `apply_e_command` below, which ports the rest of this branch
            // (lines 4739-4763) line-for-line: the empty-command diagnostic and
            // the `initial-command = .{ .direct = ... }` + implied configs.
            return Ok(HookOutcome::ConsumeRest);
        }

        // Keep track of our input args for replay
        // try self._replay_steps.append(
        //     alloc,
        //     .{ .arg = try alloc.dupeZ(u8, arg) },
        // );
        self._replay_steps.push(ReplayStep::Arg(arg.to_vec()));

        // If we didn't find a special case, continue parsing normally
        // return true;
        Ok(HookOutcome::Continue)
    }

    /// PORTS the tail of `parseManuallyHook`'s `-e` branch (Config.zig:4739-4763),
    /// run by the driver after it has drained `iter` into `command` (each arg
    /// already appended to `_replay_steps` via `record_replay_arg`).
    pub fn apply_e_command(
        &mut self,
        command: Vec<Vec<u8>>,
        location: rustty_args::diagnostics::Location,
    ) -> bool {
        // if (command.items.len == 0) {
        if command.is_empty() {
            // try self._diagnostics.append(alloc, .{
            //     .location = try cli.Location.fromIter(iter, alloc),
            //     .message = try std.fmt.allocPrintSentinel(
            //         alloc,
            //         "missing command after {s}",
            //         .{arg},
            //         0,
            //     ),
            // });
            //
            // `arg` is always "-e" on this path (the `-e` branch).
            self._diagnostics
                .append(rustty_args::diagnostics::Diagnostic {
                    key: Vec::new(),
                    location,
                    message: b"missing command after -e".to_vec(),
                });

            // return false;
            return false;
        }

        // See "command" docs for the implied configurations and why.
        // self.@"initial-command" = .{ .direct = command.items };
        self.initial_command = Some(Command::Direct(command));
        // self.@"gtk-single-instance" = .false;
        self.gtk_single_instance = GtkSingleInstance::False;
        // self.@"quit-after-last-window-closed" = true;
        self.quit_after_last_window_closed = true;
        // self.@"quit-after-last-window-closed-delay" = null;
        self.quit_after_last_window_closed_delay = None;
        // if (self.@"shell-integration" != .none) {
        //     self.@"shell-integration" = .detect;
        // }
        if self.shell_integration != ShellIntegration::None {
            self.shell_integration = ShellIntegration::Detect;
        }

        // Do not continue, we consumed everything.
        // return false;
        false
    }

    // fn compatGtkTabsLocation(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    pub(super) fn compat_gtk_tabs_location(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "gtk-tabs-location"));
        assert!(key == b"gtk-tabs-location");

        // if (std.mem.eql(u8, value orelse "", "hidden")) {
        if value.unwrap_or(b"") == b"hidden" {
            // self.@"window-show-tab-bar" = .never;
            self.window_show_tab_bar = WindowShowTabBar::Never;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatGtkSingleInstance(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    pub(super) fn compat_gtk_single_instance(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "gtk-single-instance"));
        assert!(key == b"gtk-single-instance");

        // if (std.mem.eql(u8, value orelse "", "desktop")) {
        if value.unwrap_or(b"") == b"desktop" {
            // self.@"gtk-single-instance" = .detect;
            self.gtk_single_instance = GtkSingleInstance::Detect;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatCursorInvertFgBg(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    pub(super) fn compat_cursor_invert_fg_bg(&mut self, key: &[u8], value_: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "cursor-invert-fg-bg"));
        assert!(key == b"cursor-invert-fg-bg");

        // We don't do anything if the value is unset, which is technically
        // not EXACTLY the same as prior behavior since it would fallback
        // to doing whatever cursor-color/cursor-text were set to, but
        // I don't want to store what that is separately so this is close
        // enough.
        //
        // Realistically, these fields were mutually exclusive so anyone
        // relying on that behavior should just upgrade to the new
        // cursor-color/cursor-text fields.
        // const set = cli.args.parseBool(value_ orelse "t") catch return false;
        let set = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (set) {
        if set {
            // self.@"cursor-color" = .@"cell-foreground";
            self.cursor_color = Some(TerminalColor::CellForeground);
            // self.@"cursor-text" = .@"cell-background";
            self.cursor_text = Some(TerminalColor::CellBackground);
        }

        // return true;
        true
    }

    // fn compatSelectionInvertFgBg(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    pub(super) fn compat_selection_invert_fg_bg(
        &mut self,
        key: &[u8],
        value_: Option<&[u8]>,
    ) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "selection-invert-fg-bg"));
        assert!(key == b"selection-invert-fg-bg");

        // const set = cli.args.parseBool(value_ orelse "t") catch return false;
        let set = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (set) {
        if set {
            // self.@"selection-foreground" = .@"cell-background";
            self.selection_foreground = Some(TerminalColor::CellBackground);
            // self.@"selection-background" = .@"cell-foreground";
            self.selection_background = Some(TerminalColor::CellForeground);
        }

        // return true;
        true
    }

    // fn compatBoldIsBright(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    pub(super) fn compat_bold_is_bright(&mut self, key: &[u8], value_: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "bold-is-bright"));
        assert!(key == b"bold-is-bright");

        // const isset = cli.args.parseBool(value_ orelse "t") catch return false;
        let isset = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (isset) {
        if isset {
            // self.@"bold-color" = .bright;
            self.bold_color = Some(BoldColor::Bright);
        }

        // return true;
        true
    }

    // fn compatMacOSDockDropBehavior(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    pub(super) fn compat_macos_dock_drop_behavior(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "macos-dock-drop-behavior"));
        assert!(key == b"macos-dock-drop-behavior");

        // if (std.mem.eql(u8, value orelse "", "window")) {
        if value.unwrap_or(b"") == b"window" {
            // self.@"macos-dock-drop-behavior" = .@"new-window";
            self.macos_dock_drop_behavior = MacOSDockDropBehavior::NewWindow;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatCopyOnSelect(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    pub(super) fn compat_copy_on_select(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "copy-on-select"));
        assert!(key == b"copy-on-select");

        // if (std.mem.eql(u8, value orelse "", "true")) {
        if value.unwrap_or(b"") == b"true" {
            // self.@"copy-on-select" = switch (builtin.os.tag) {
            //     .linux, .freebsd => .primary,
            //     else => .clipboard,
            // };
            self.copy_on_select = if cfg!(any(target_os = "linux", target_os = "freebsd")) {
                CopyOnSelect::Primary
            } else {
                CopyOnSelect::Clipboard
            };
            // return true;
            return true;
        }

        // if (std.mem.eql(u8, value orelse "", "false")) {
        if value.unwrap_or(b"") == b"false" {
            // self.@"copy-on-select" = .none;
            self.copy_on_select = CopyOnSelect::None;
            // return true;
            return true;
        }

        // return false;
        false
    }

    /// Add a diagnostic message to the config with the given string.
    /// This is always added with a location of "none".
    // pub fn addDiagnosticFmt(
    //     self: *Config,
    //     comptime fmt: []const u8,
    //     args: anytype,
    // ) Allocator.Error!void {
    //
    // PORT NOTE: Rust has no comptime format string; callers pass the already-
    // formatted message bytes (the `std.fmt.allocPrintSentinel(.., fmt, args, 0)`
    // result). The location defaults to `.none` exactly as the reference.
    pub fn add_diagnostic_fmt(&mut self, message: Vec<u8>) {
        // const alloc = self._arena.?.allocator();
        // try self._diagnostics.append(alloc, .{
        //     .message = try std.fmt.allocPrintSentinel(alloc, fmt, args, 0),
        // });
        self._diagnostics
            .append(rustty_args::diagnostics::Diagnostic {
                message,
                ..Default::default()
            });
    }

    /// Create a shallow copy of this config. This will share all the memory
    /// allocated with the previous config but will have a new arena for
    /// any changes or new allocations. The config should have `deinit`
    /// called when it is complete.
    ///
    /// Beware: these shallow clones are not meant for a long lifetime,
    /// they are just meant to exist temporarily for the duration of some
    /// modifications. It is very important that the original config not
    /// be deallocated while shallow clones exist.
    // pub fn shallowClone(self: *const Config, alloc_gpa: Allocator) Config {
    //     var result = self.*;
    //     result._arena = .init(alloc_gpa);
    //     return result;
    // }
    //
    // PORT NOTE: the reference's shallow clone shares the original's bytes via a
    // bitwise struct copy and only swaps in a fresh arena. The Rust port owns data
    // per-field (no shared bump arena), so a "shallow" clone is a deep `.clone()`
    // with the `_arena` flag set — the closest faithful equivalent. Long-lived
    // aliasing of the original's bytes (the reference's caveat) does not apply
    // since nothing is shared.
    pub fn shallow_clone(&self) -> Config {
        // var result = self.*;
        let mut result = self.clone_config();
        // result._arena = .init(alloc_gpa);
        result._arena = true;
        // return result;
        result
    }

    /// Load and parse the CLI args.
    // pub fn loadCliArgs(self: *Config, alloc_gpa: Allocator) !void {
    pub fn load_cli_args(&mut self) -> Result<(), LoadError> {
        // switch (builtin.os.tag) {
        //     .windows => {},
        //     // Fast-path if we are Linux/BSD and have no args.
        //     .linux, .freebsd => if (std.os.argv.len <= 1) return,
        //     // Everything else we have to at least try because it may
        //     // not use std.os.argv.
        //     else => {},
        // }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            if std::env::args_os().len() <= 1 {
                return Ok(());
            }
        }

        // On Linux, we have a special case where if the executing
        // program is "xdg-terminal-exec" then we treat all CLI
        // args as if they are a command to execute.
        //
        // In this mode, we also behave slightly differently:
        //
        //   - The initial window title is set to the full command. This
        //     can be used with window managers to modify positioning,
        //     styling, etc. based on the command.
        //
        // See: https://github.com/Vladimir-csp/xdg-terminal-exec
        // if ((comptime builtin.os.tag == .linux) or (comptime builtin.os.tag == .freebsd)) {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            // if (internal_os.xdg.parseTerminalExec(global.args().vector)) |args| {
            //
            // PORT NOTE (NEW-FINDINGS N45): the reference passes the raw byte
            // argv. This read `std::env::args()`, which panics on any non-UTF-8
            // argument, so on Linux and FreeBSD `rustty -e cat $'\xff'` aborted at
            // startup whatever argv[0] was. The argv is now read as bytes
            // (`args_os` + `arg_bytes`, the seam `args_iterator` already uses).
            let argv: Vec<Vec<u8>> = std::env::args_os()
                .map(|arg| rustty_args::args::arg_bytes(&arg))
                .collect();
            let argv_refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
            if let Some(args) = rustty_os::xdg::parseTerminalExec(&argv_refs) {
                // const arena_alloc = self._arena.?.allocator();

                // First, we add an artificial "-e" so that if we
                // replay the inputs to rebuild the config (i.e. if
                // a theme is set) then we will get the same behavior.
                // try self._replay_steps.append(arena_alloc, .@"-e");
                self._replay_steps.push(ReplayStep::DashE);

                // Next, take all remaining args and use that to build up
                // a command to execute.
                // var builder: std.ArrayList([:0]const u8) = .empty;
                // errdefer builder.deinit(arena_alloc);
                let mut builder: Vec<Vec<u8>> = Vec::new();
                // for (args) |arg_raw| {
                for arg_raw in args {
                    // const arg = std.mem.sliceTo(arg_raw, 0);
                    // const copy = try arena_alloc.dupeZ(u8, arg);
                    let copy = arg_raw.to_vec();
                    // try self._replay_steps.append(arena_alloc, .{ .arg = copy });
                    self._replay_steps.push(ReplayStep::Arg(copy.clone()));
                    // try builder.append(arena_alloc, copy);
                    builder.push(copy);
                }

                // self.@"_xdg-terminal-exec" = true;
                self._xdg_terminal_exec = true;
                // self.@"initial-command" = .{ .direct = try builder.toOwnedSlice(arena_alloc) };
                self.initial_command = Some(Command::Direct(builder));
                // return;
                return Ok(());
            }
        }

        // We set config-default-files to true here because this
        // should always be reset so we can detect if it is set
        // in the CLI since it is documented as having no affect
        // from files.
        // self.@"config-default-files" = true;
        self.config_default_files = true;

        // Keep track of the replay steps up to this point so we
        // can replay if we are discarding the default files.
        // const replay_len_start = self._replay_steps.items.len;
        let replay_len_start = self._replay_steps.len();

        // font-family settings set via the CLI overwrite any prior values
        // rather than append. This avoids a UX oddity where you have to
        // specify `font-family=""` to clear the font families.
        // const fields = &[_][]const u8{ "font-family", ... };
        // inline for (fields) |field| @field(self, field).overwrite_next = true;
        self.font_family.overwrite_next = true;
        self.font_family_bold.overwrite_next = true;
        self.font_family_italic.overwrite_next = true;
        self.font_family_bold_italic.overwrite_next = true;
        // defer {
        //     inline for (fields) |field| @field(self, field).overwrite_next = false;
        // }
        //
        // PORT NOTE: the `defer` reset is performed at the end of this fn (Rust has
        // no `defer`; the resets are placed before each return path / at the tail).

        // Initialize our CLI iterator.
        // var iter = try cli.args.argsIterator(alloc_gpa);
        // defer iter.deinit();
        let mut iter = match rustty_args::args::args_iterator() {
            Ok(it) => it,
            Err(_) => {
                self.reset_font_family_overwrite();
                return Ok(());
            }
        };
        // try self.loadIter(alloc_gpa, &iter);
        let r = self.load_iter(&mut iter);
        iter.deinit();
        r?;

        // (defer reset, see note above)
        self.reset_font_family_overwrite();

        // If we are not loading the default files, then we need to
        // replay the steps up to this point so that we can rebuild
        // the config without it.
        // if (!self.@"config-default-files") reload: {
        if !self.config_default_files {
            'reload: {
                // const replay_len_end = self._replay_steps.items.len;
                let replay_len_end = self._replay_steps.len();
                // if (replay_len_end == replay_len_start) break :reload;
                if replay_len_end == replay_len_start {
                    break 'reload;
                }
                // log.info("config-default-files unset, discarding configuration from default files", .{});
                log_info(
                    "config-default-files unset, discarding configuration from default files"
                        .to_string(),
                );

                // var new_config = try self.cloneEmpty(alloc_gpa);
                // errdefer new_config.deinit();
                let mut new_config = self.clone_empty();
                // var it = Replay.iterator(
                //     self._replay_steps.items[replay_len_start..replay_len_end],
                //     &new_config,
                // );
                let slice: Vec<ReplayStep> =
                    self._replay_steps[replay_len_start..replay_len_end].to_vec();
                let mut it = ReplayIterator::new(slice);
                // try new_config.loadIter(alloc_gpa, &it);
                new_config.load_iter(&mut it)?;
                // self.deinit();
                self.deinit();
                // self.* = new_config;
                *self = new_config;
            }
        }

        // Any paths referenced from the CLI are relative to the current working
        // directory.
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // try self.expandPaths(try std.fs.cwd().realpath(".", &buf));
        //
        // PORT NOTE (LEDGER S74-F3): both `try`s propagate. This was
        // `.ok()...unwrap_or_default()`, so an unresolvable cwd (deleted, or
        // not searchable) expanded every CLI path against an EMPTY base; the
        // reference returns the `realPathFile` error before `expandPaths`.
        // `realPathFile(".")` on `cwd()` is libc `realpath(".")`
        // (`Io/Threaded.zig` `dirRealPathFilePosix`, the `AT.FDCWD` arm), which
        // is what `canonicalize` calls.
        // (`rustty_os::path::realpath`: the DOS form on Windows, task W-2.)
        let cwd = path_to_bytes(&rustty_os::path::realpath(std::path::Path::new("."))?);
        self.expand_paths(&cwd)?;

        Ok(())
    }

    /// Reset the `overwrite_next` flag on the font-family fields (the reference's
    /// `defer { inline for (fields) ... = false; }` in `loadCliArgs`).
    fn reset_font_family_overwrite(&mut self) {
        self.font_family.overwrite_next = false;
        self.font_family_bold.overwrite_next = false;
        self.font_family_italic.overwrite_next = false;
        self.font_family_bold_italic.overwrite_next = false;
    }

    /// Load and parse the config files that were added in the "config-file" key.
    // pub fn loadRecursiveFiles(self: *Config, alloc_gpa: Allocator) !void {
    pub fn load_recursive_files(&mut self) -> Result<(), LoadError> {
        // if (self.@"config-file".value.items.len == 0) return;
        if self.config_file.value.is_empty() {
            return Ok(());
        }
        // const arena_alloc = self._arena.?.allocator();

        // Keeps track of loaded files to prevent cycles.
        // var loaded = std.StringHashMap(void).init(alloc_gpa);
        // defer loaded.deinit();
        let mut loaded: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

        // We need to insert all of our loaded config-file values
        // PRIOR to the "-e" in our replay steps, since everything
        // after "-e" becomes an "initial-command". To do this, we
        // dupe the values if we find it.
        // var replay_suffix: std.ArrayList(Replay.Step) = .empty;
        // defer replay_suffix.deinit(alloc_gpa);
        let mut replay_suffix: Vec<ReplayStep> = Vec::new();
        // for (self._replay_steps.items, 0..) |step, i| if (step == .@"-e") {
        for i in 0..self._replay_steps.len() {
            if matches!(self._replay_steps[i], ReplayStep::DashE) {
                // We don't need to clone the steps because they should
                // all be allocated in our arena and we're keeping our
                // arena.
                // try replay_suffix.appendSlice(alloc_gpa, self._replay_steps.items[i..]);
                replay_suffix.extend_from_slice(&self._replay_steps[i..]);
                // Remove our old values. Again, don't need to free any
                // memory here because its all part of our arena.
                // self._replay_steps.shrinkRetainingCapacity(i);
                self._replay_steps.truncate(i);
                // break;
                break;
            }
        }

        // We must use a while below and not a for(items) because we
        // may add items to the list while iterating for recursive
        // config-file entries.
        // var i: usize = 0;
        let mut i: usize = 0;
        // while (i < self.@"config-file".value.items.len) : (i += 1) {
        while i < self.config_file.value.len() {
            // const path, const optional = switch (self.@"config-file".value.items[i]) {
            //     .optional => |path| .{ path, true },
            //     .required => |path| .{ path, false },
            // };
            let (path, optional): (Vec<u8>, bool) = match &self.config_file.value[i] {
                Path::Optional(path) => (path.clone(), true),
                Path::Required(path) => (path.clone(), false),
            };

            // Error paths
            // if (path.len == 0) continue;
            if path.is_empty() {
                i += 1;
                continue;
            }

            // All paths should already be absolute at this point because
            // they're fixed up after each load.
            // assert(std.fs.path.isAbsolute(path));
            // (Zig's `isAbsolute`, as in `load_file`; task W-2 CORRECTION.)
            assert!(crate::path::is_absolute(&path));

            // We must only load a unique file once
            // if (try loaded.fetchPut(path, {}) != null) {
            if !loaded.insert(path.clone()) {
                // const diag: cli.Diagnostic = .{
                //     .message = try std.fmt.allocPrintSentinel(
                //         arena_alloc, "config-file {s}: cycle detected", .{path}, 0),
                // };
                let diag = rustty_args::diagnostics::Diagnostic {
                    message: format!(
                        "config-file {}: cycle detected",
                        String::from_utf8_lossy(&path)
                    )
                    .into_bytes(),
                    ..Default::default()
                };
                // try self._diagnostics.append(arena_alloc, diag);
                self._diagnostics.append(diag.clone());
                // try self._replay_steps.append(arena_alloc, .{ .diagnostic = diag });
                self._replay_steps.push(ReplayStep::Diagnostic(diag));
                // continue;
                i += 1;
                continue;
            }

            // var file = std.fs.openFileAbsolute(path, .{}) catch |err| {
            let file = match std::fs::File::open(os_str_from_bytes(&path)) {
                Ok(f) => f,
                Err(err) => {
                    // if (err != error.FileNotFound or !optional) {
                    if err.kind() != std::io::ErrorKind::NotFound || !optional {
                        // const diag: cli.Diagnostic = .{
                        //     .message = ... "error opening config-file {s}: {}" ...
                        // };
                        //
                        // PORT NOTE (LEDGER S74-F4): `{}` of the Zig error is
                        // `error.<Name>` (this printed the `io::Error` Display,
                        // `Permission denied (os error 13)`).
                        let diag = rustty_args::diagnostics::Diagnostic {
                            message: format!(
                                "error opening config-file {}: {}",
                                String::from_utf8_lossy(&path),
                                crate::zig_error::fmt_error(&err, crate::zig_error::Call::OpenFile)
                            )
                            .into_bytes(),
                            ..Default::default()
                        };
                        // try self._diagnostics.append(arena_alloc, diag);
                        self._diagnostics.append(diag.clone());
                        // try self._replay_steps.append(arena_alloc, .{ .diagnostic = diag });
                        self._replay_steps.push(ReplayStep::Diagnostic(diag));
                    }
                    // continue;
                    i += 1;
                    continue;
                }
            };
            // defer file.close();

            // const stat = try file.stat();
            let stat = file.metadata()?;
            // switch (stat.kind) {
            //     .file => {},
            //     else => |kind| { ... "config-file {s}: not reading because file type is {s}" ... continue; },
            // }
            //
            // PORT NOTE (LEDGER S74-F4): `@tagName(kind)` names every
            // `File.Kind` (`character_device`, `named_pipe`, ...); this used to
            // collapse all but `directory`/`sym_link` to `unknown`.
            if !stat.is_file() {
                let diag = rustty_args::diagnostics::Diagnostic {
                    message: format!(
                        "config-file {}: not reading because file type is {}",
                        String::from_utf8_lossy(&path),
                        crate::zig_error::file_kind_tag_name(&stat.file_type())
                    )
                    .into_bytes(),
                    ..Default::default()
                };
                self._diagnostics.append(diag.clone());
                self._replay_steps.push(ReplayStep::Diagnostic(diag));
                i += 1;
                continue;
            }

            // try self.loadFsFile(arena_alloc, &file, path);
            self.load_fs_file(file, &path)?;
            i += 1;
        }

        // If we have a suffix, add that back.
        // if (replay_suffix.items.len > 0) {
        if !replay_suffix.is_empty() {
            // try self._replay_steps.appendSlice(arena_alloc, replay_suffix.items);
            self._replay_steps.extend(replay_suffix);
        }

        Ok(())
    }

    // Get the arena allocator associated with the configuration.
    // pub fn arenaAlloc(self: *Config) Allocator {
    //     return self._arena.?.allocator();
    // }
    //
    // PORT NOTE: there is no shared bump arena in the Rust port (each value owns
    // its bytes), so `arenaAlloc` has no observable Rust counterpart. It is
    // retained as a comment for fidelity; call sites that needed the arena now use
    // owned allocation directly.

    /// Returns true if the given key event would trigger a keybinding
    /// if it were to be processed. This is useful for determining if
    /// a key event should be sent to the terminal or not.
    // pub fn keyEventIsBinding(
    //     self: *Config,
    //     event: inputpkg.KeyEvent,
    // ) bool {
    //
    // PORT NOTE: consumed by the libghostty C-ABI export `ghostty_config_key_is_binding`
    // (embedded.zig:1470, rustty-apprt-c chunk E6). Renamed to snake_case per this
    // module's method convention (`clone_config`/`change_conditional_state`/…).
    // `self: *Config` is mutable-capable in the reference but the body only reads, so
    // `&self` is the faithful minimal. `event` is taken by value (the reference
    // `event: inputpkg.KeyEvent`); the borrow for `getEvent` is taken at the call.
    // There is no reference `test{}` for this fn.
    pub fn key_event_is_binding(&self, event: rustty_vt::input::key::KeyEvent) -> bool {
        // switch (event.action) {
        //     .release => return false,
        //     .press, .repeat => {},
        // }
        match event.action {
            rustty_vt::input::key::Action::Release => return false,
            rustty_vt::input::key::Action::Press | rustty_vt::input::key::Action::Repeat => {}
        }

        // If we have a keybinding for this event then we return true.
        // return self.keybind.set.getEvent(event) != null;
        self.keybind.set.get_event(&event).is_some()
    }

    /// Change the state of conditionals and reload the configuration
    /// based on the new state. This returns a new configuration based
    /// on the new state. The caller must free the old configuration if they
    /// wish.
    ///
    /// This returns null if the conditional state would result in no changes
    /// to the configuration. In this case, the caller can continue to use
    /// the existing configuration or clone if they want a copy.
    ///
    /// This doesn't re-read any files, it just re-applies the same
    /// configuration with the new conditional state. Importantly, this means
    /// that if you change the conditional state and the user in the interim
    /// deleted a file that was referenced in the configuration, then the
    /// configuration can still be reloaded.
    // pub fn changeConditionalState(
    //     self: *const Config,
    //     new: conditional.State,
    // ) !?Config {
    pub fn change_conditional_state(
        &self,
        new: conditional::State,
    ) -> Result<Option<Config>, LoadError> {
        // If the conditional state between the old and new is the same,
        // then we don't need to do anything.
        // relevant: {
        let relevant = 'relevant: {
            // inline for (@typeInfo(conditional.Key).@"enum".fields) |field| {
            //     const key: conditional.Key = @field(conditional.Key, field.name);
            //     // Conditional set contains the keys that this config uses. So we
            //     // only continue if we use this key.
            //     if (self._conditional_set.contains(key) and !deepEqual(
            //         @TypeOf(@field(self._conditional_state, field.name)),
            //         @field(self._conditional_state, field.name),
            //         @field(new, field.name),
            //     )) {
            //         break :relevant;
            //     }
            // }
            for key in [conditional::Key::Theme, conditional::Key::Os] {
                let differs = match key {
                    conditional::Key::Theme => self._conditional_state.theme != new.theme,
                    // PORT NOTE: `OsTag` carries only its tag-name bytes (the
                    // reference's `deepEqual` over the enum compares its tag), so
                    // we compare tag names.
                    conditional::Key::Os => {
                        self._conditional_state.os.tag_name() != new.os.tag_name()
                    }
                };
                if self._conditional_set.contains(&key) && differs {
                    break 'relevant true;
                }
            }
            // If we got here, then we didn't find any differences between
            // the old and new conditional state that would affect the
            // configuration.
            // return null;
            false
        };
        if !relevant {
            return Ok(None);
        }

        // Create our new configuration
        // const alloc_gpa = self._arena.?.child_allocator;
        // var new_config = try self.cloneEmpty(alloc_gpa);
        // errdefer new_config.deinit();
        let mut new_config = self.clone_empty();

        // Set our conditional state so the replay below can use it
        // new_config._conditional_state = new;
        new_config._conditional_state = new;

        // Replay all of our steps to rebuild the configuration
        // var it = Replay.iterator(self._replay_steps.items, &new_config);
        let mut it = ReplayIterator::new(self._replay_steps.clone());
        // try new_config.loadIter(alloc_gpa, &it);
        new_config.load_iter(&mut it)?;
        // try new_config.finalize();
        new_config.finalize()?;

        // return new_config;
        Ok(Some(new_config))
    }

    /// Expand the relative paths in config-files to be absolute paths
    /// relative to the base directory.
    // fn expandPaths(self: *Config, base: []const u8) !void {
    pub(super) fn expand_paths(&mut self, base: &[u8]) -> Result<(), LoadError> {
        // const arena_alloc = self._arena.?.allocator();

        // Keep track of this step for replays
        // try self._replay_steps.append(
        //     arena_alloc,
        //     .{ .expand = try arena_alloc.dupe(u8, base) },
        // );
        self._replay_steps.push(ReplayStep::Expand(base.to_vec()));

        // Expand all of our paths
        // inline for (@typeInfo(Config).@"struct".fields) |field| {
        //     switch (field.type) {
        //         RepeatablePath, Path => {
        //             try @field(self, field.name).expand(arena_alloc, base, &self._diagnostics);
        //         },
        //         ?RepeatablePath, ?Path => {
        //             if (@field(self, field.name)) |*path| {
        //                 try path.expand(arena_alloc, base, &self._diagnostics);
        //             }
        //         },
        //         else => {},
        //     }
        // }
        //
        // PORT NOTE: Rust has no struct reflection; the `RepeatablePath`/`Path`
        // (and optional) fields are enumerated explicitly. The set is:
        //   RepeatablePath: config_file, custom_shader, gtk_custom_css
        //   ?Path:          background_image, bell_audio_path
        self.config_file.expand(base, &mut self._diagnostics)?;
        self.custom_shader.expand(base, &mut self._diagnostics)?;
        self.gtk_custom_css.expand(base, &mut self._diagnostics)?;
        if let Some(path) = &mut self.background_image {
            path.expand(base, &mut self._diagnostics)?;
        }
        if let Some(path) = &mut self.bell_audio_path {
            path.expand(base, &mut self._diagnostics)?;
        }

        Ok(())
    }

    // fn loadTheme(self: *Config, theme: Theme) !void {
    pub(super) fn load_theme(&mut self, theme: Theme) -> Result<(), LoadError> {
        // Load the correct theme depending on the conditional state.
        // Dark/light themes were programmed prior to conditional configuration
        // so when we introduce that we probably want to replace this.
        // const name: []const u8 = switch (self._conditional_state.theme) {
        //     .light => theme.light,
        //     .dark => theme.dark,
        // };
        let name: Vec<u8> = match self._conditional_state.theme {
            conditional::Theme::Light => theme.light.clone(),
            conditional::Theme::Dark => theme.dark.clone(),
        };

        // Find our theme file and open it. See the open function for details.
        // const themefile = (try themepkg.open(
        //     self._arena.?.allocator(),
        //     name,
        //     &self._diagnostics,
        // )) orelse return;
        let themefile = match crate::theme::open(&name, &mut self._diagnostics)? {
            Some(tf) => tf,
            None => return Ok(()),
        };
        // const path = themefile.path;
        let path = themefile.path;
        // const file = themefile.file;
        // defer file.close();
        let mut file = themefile.file;

        // From this point onwards, we load the theme and do a bit of a dance
        // to achieve two separate goals:
        //
        //   (1) We want the theme to be loaded and our existing config to
        //       override the theme. So we need to load the theme and apply
        //       our config on top of it.
        //
        //   (2) We want to free existing memory that we aren't using anymore
        //       as a result of reloading the configuration.
        //
        // Point 2 is strictly a result of aur approach to point 1, but it is
        // a nice property to have to limit memory bloat as much as possible.

        // Load into a new configuration so that we can free the existing memory.
        // const alloc_gpa = self._arena.?.child_allocator;
        // var new_config = try self.cloneEmpty(alloc_gpa);
        // errdefer new_config.deinit();
        let mut new_config = self.clone_empty();

        // Load our theme
        // var buf: [2048]u8 = undefined;
        // var file_reader = file.reader(&buf);
        // const reader = &file_reader.interface;
        // var iter: cli.args.LineIterator = .{ .r = reader, .filepath = path };
        // try new_config.loadIter(alloc_gpa, &iter);
        use std::io::Read;
        let mut contents: Vec<u8> = Vec::new();
        file.read_to_end(&mut contents)?;
        {
            let mut iter = rustty_args::args::LineIterator::with_filepath(&contents, &path);
            new_config.load_iter(&mut iter)?;
        }

        // Setup our replay to be conditional.
        // conditional: for (new_config._replay_steps.items) |*item| {
        'conditional: for item in new_config._replay_steps.iter_mut() {
            // switch (item.*) {
            match item {
                // .expand, .diagnostic => {},
                ReplayStep::Expand(_) | ReplayStep::Diagnostic(_) => {}

                // If we see "-e" then we do NOT make the following arguments
                // conditional since they are supposed to be part of the
                // initial command.
                // .@"-e" => break :conditional,
                ReplayStep::DashE => break 'conditional,

                // Change our arg to be conditional on our theme.
                // .arg => |v| {
                ReplayStep::Arg(v) => {
                    // const alloc_arena = new_config._arena.?.allocator();
                    // const conds = try alloc_arena.alloc(Conditional, 1);
                    // conds[0] = .{
                    //     .key = .theme,
                    //     .op = .eq,
                    //     .value = @tagName(self._conditional_state.theme),
                    // };
                    let conds = vec![Conditional {
                        key: conditional::Key::Theme,
                        op: conditional::Op::Eq,
                        value: self._conditional_state.theme.tag_name().to_vec(),
                    }];
                    // item.* = .{ .conditional_arg = .{ .conditions = conds, .arg = v } };
                    *item = ReplayStep::ConditionalArg {
                        conditions: conds,
                        arg: std::mem::take(v),
                    };
                }

                // .conditional_arg => |v| {
                ReplayStep::ConditionalArg { conditions, arg } => {
                    // const alloc_arena = new_config._arena.?.allocator();
                    // const conds = try alloc_arena.alloc(Conditional, v.conditions.len + 1);
                    // conds[0] = .{ .key = .theme, .op = .eq, .value = @tagName(self._conditional_state.theme) };
                    // @memcpy(conds[1..], v.conditions);
                    let mut conds = Vec::with_capacity(conditions.len() + 1);
                    conds.push(Conditional {
                        key: conditional::Key::Theme,
                        op: conditional::Op::Eq,
                        value: self._conditional_state.theme.tag_name().to_vec(),
                    });
                    conds.extend(std::mem::take(conditions));
                    // item.* = .{ .conditional_arg = .{ .conditions = conds, .arg = v.arg } };
                    *item = ReplayStep::ConditionalArg {
                        conditions: conds,
                        arg: std::mem::take(arg),
                    };
                }
            }
        }

        // Replay our previous inputs so that we can override values
        // from the theme.
        // var slice_it = Replay.iterator(self._replay_steps.items, &new_config);
        // try new_config.loadIter(alloc_gpa, &slice_it);
        let mut slice_it = ReplayIterator::new(self._replay_steps.clone());
        new_config.load_iter(&mut slice_it)?;

        // Success, swap our new config in and free the old.
        // self.deinit();
        self.deinit();
        // self.* = new_config;
        *self = new_config;

        Ok(())
    }
}
