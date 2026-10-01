//! PORTS: ghostty src/apprt/embedded.zig — the `Surface` struct, its
//! `Options`, the `init`/`deinit` lifecycle and event-sink callbacks
//! (chunks E2+E3, embedded.zig 425-990), and the `apprt_api::Surface`
//! growth-point table (embedded.zig 597-982).

use super::*;

// pub const Surface = struct { ... };
//
// PORT NOTE (ADV-FIX 9): the reference fields are file-private; the apprt-c C-ABI
// wrappers reach them, so they are `pub` here.
pub struct Surface {
    // app: *App,
    pub app: *mut App,
    // platform: Platform,
    pub platform: Platform,
    // userdata: ?*anyopaque = null,
    pub userdata: *mut c_void,
    // core_surface: CoreSurface,
    pub core_surface: CoreSurface,
    // content_scale: apprt.ContentScale,
    pub content_scale: ContentScale,
    // size: apprt.SurfaceSize,
    pub size: SurfaceSize,
    // cursor_pos: apprt.CursorPos,
    pub cursor_pos: CursorPos,
    // inspector: ?*Inspector = null,
    pub inspector: Option<*mut Inspector>,

    /// The current title of the surface. The embedded apprt saves this so
    /// that getTitle works without the implementer needing to save it.
    //
    // title: ?[:0]const u8 = null,
    //
    // PORT NOTE: `?[:0]const u8` → an owned NUL-terminated `Option<CString>`
    // (written by `App::performPreAction`'s `set_title` arm via `rt_surface`).
    pub title: Option<CString>,

    // PORT NOTE: reference defect (R13), UD68. No reference field: the
    // reference's `newSurfaceOptions` hands the host a GPA `dupeZ` of the
    // inherited cwd that nothing ever frees. The port keeps that string here,
    // surface-side, and frees it on the next `newSurfaceOptions` or on
    // `deinit`. See [`InheritedWorkingDirectory`].
    inherited_working_directory: InheritedWorkingDirectory,
}

/// Surface initialization options.
//
// PORT NOTE: the reference nests this as `Surface.Options` (the
// `rustty_surface_config_s` extern struct, embedded.zig:425); a top-level
// `#[repr(C)] SurfaceOptions` here (ADV-FIX 9: reached by the apprt-c wrappers /
// backend `App::newSurface`). Field defaults (Zig `= …`) are not expressible on a
// Rust struct — the C host (or `rustty_surface_config_new`, E6) supplies a
// fully-zeroed/populated value. The `?[*:0]const u8` / `?[*]EnvVar` / `?*anyopaque`
// nullable pointers map to nullable raw pointers; `callconv`-free, so no lint.
#[repr(C)]
pub struct SurfaceOptions {
    /// The platform that this surface is being initialized for and
    /// the associated platform-specific configuration.
    // platform_tag: c_int = 0,
    pub platform_tag: c_int,
    // platform: Platform.C = undefined,
    pub platform: PlatformC,

    /// Userdata passed to some of the callbacks.
    // userdata: ?*anyopaque = null,
    pub userdata: *mut c_void,

    /// The scale factor of the screen.
    // scale_factor: f64 = 1,
    pub scale_factor: f64,

    /// The font size to inherit. If 0, default font size will be used.
    // font_size: f32 = 0,
    pub font_size: f32,

    /// The working directory to load into.
    // working_directory: ?[*:0]const u8 = null,
    pub working_directory: *const c_char,

    /// The command to run in the new surface. If this is set then
    /// the "wait-after-command" option is also automatically set to true,
    /// since this is used for scripting.
    ///
    /// This command always run in a shell (e.g. via `/bin/sh -c`),
    /// despite rustty allowing directly executed commands via config.
    /// This is a legacy thing and we should probably change it in the
    /// future once we have a concrete use case.
    // command: ?[*:0]const u8 = null,
    pub command: *const c_char,

    /// Extra environment variables to set for the surface.
    // env_vars: ?[*]EnvVar = null,
    pub env_vars: *const EnvVar,
    // env_var_count: usize = 0,
    pub env_var_count: usize,

    /// Input to send to the command after it is started.
    // initial_input: ?[*:0]const u8 = null,
    pub initial_input: *const c_char,

    /// Wait after the command exits
    // wait_after_command: bool = false,
    pub wait_after_command: bool,

    /// Context for the new surface
    // context: apprt.surface.NewSurfaceContext = .window,
    pub context: NewSurfaceContext,
}

impl Surface {
    /// reference `pub fn init(self: *Surface, app: *App, opts: Options) !void`
    /// (embedded.zig:467). Out-pointer form (`surface` is the uninitialized heap
    /// allocation from [`App::newSurface`]) preserving the reference placement-init
    /// and [ADV-FIX 8] PINNING: core `Surface::init` writes
    /// `core_surface.rt_surface = &*surface`, so the embedded Surface is heap-boxed
    /// and NEVER moved after init.
    ///
    /// # Safety
    ///
    /// `surface` must point at a live, uninitialized `Surface`-sized allocation and
    /// `app` at a live, pinned [`App`].
    pub unsafe fn init(
        surface: *mut Surface,
        app: *mut App,
        opts: SurfaceOptions,
    ) -> Result<(), ApprtError> {
        // self.* = .{
        //     .app = app,
        //     .platform = try .init(opts.platform_tag, opts.platform),
        //     ...
        // };
        //
        // [ADV-FIX 8] out-pointer placement-init: the fallible `platform` is
        // computed FIRST so a `Platform.init` error returns before ANY field is
        // written (mirroring the reference struct-literal, where `try .init` runs
        // before the `self.* = .{…}` store commits). `core_surface` is left
        // uninitialized and written in place by `core_surface.init` below
        // (self-referential — the core `Surface::init` ptr::write idiom).
        let platform = Platform::init(opts.platform_tag, opts.platform).map_err(|_| ApprtError)?;

        unsafe {
            // .app = app,
            std::ptr::write(&raw mut (*surface).app, app);
            // .platform = try .init(...),
            std::ptr::write(&raw mut (*surface).platform, platform);
            // .userdata = opts.userdata,
            std::ptr::write(&raw mut (*surface).userdata, opts.userdata);
            // .core_surface = undefined,  (written in place by core_surface.init)
            // .content_scale = .{ .x = @floatCast(opts.scale_factor), .y = @floatCast(opts.scale_factor) },
            std::ptr::write(
                &raw mut (*surface).content_scale,
                ContentScale {
                    x: opts.scale_factor as f32,
                    y: opts.scale_factor as f32,
                },
            );
            // .size = .{ .width = 800, .height = 600 },
            std::ptr::write(
                &raw mut (*surface).size,
                SurfaceSize {
                    width: 800,
                    height: 600,
                },
            );
            // .cursor_pos = .{ .x = -1, .y = -1 },
            std::ptr::write(
                &raw mut (*surface).cursor_pos,
                CursorPos { x: -1.0, y: -1.0 },
            );
            // (field defaults) inspector: ?*Inspector = null, title: ?[:0]const u8 = null
            std::ptr::write(&raw mut (*surface).inspector, None);
            std::ptr::write(&raw mut (*surface).title, None);
            // (no reference field — UD68)
            std::ptr::write(
                &raw mut (*surface).inherited_working_directory,
                InheritedWorkingDirectory::default(),
            );
        }

        // Add ourselves to the list of surfaces on the app.
        // try app.core_app.addSurface(self);
        // (PORT NOTE: rustty `addSurface` is infallible — no `try`.)
        unsafe { (*(*app).core_app).addSurface(surface) };
        // errdefer app.core_app.deleteSurface(self);
        // (an errdefer — realized manually on the fallible paths below.)

        // Shallow copy the config so that we can modify it.
        // var config = try apprt.surface.newConfig(app.core_app, &app.config, opts.context);
        // defer config.deinit();
        //
        // (PORT NOTE: rustty `new_config` is infallible; `config` has an explicit
        // `deinit` — no Drop — realized on the success and error paths below.)
        let mut config = rustty_core::app::new_config(
            unsafe { &*(*app).core_app },
            unsafe { &*(*app).config.get() },
            opts.context,
        );

        // If we have a working directory from the options then we set it.
        // if (opts.working_directory) |c_wd| {
        if !opts.working_directory.is_null() {
            // const wd = std.mem.sliceTo(c_wd, 0);
            let wd = unsafe { CStr::from_ptr(opts.working_directory) }.to_bytes();
            // if (wd.len > 0) wd: {
            if !wd.is_empty() {
                'wd: {
                    // var dir = std.Io.Dir.openDirAbsolute(global.io(), wd, .{}) catch |err| { ...; break :wd; };
                    // defer dir.close(global.io());
                    // const stat = dir.stat(global.io()) catch |err| { ...; break :wd; };
                    // if (stat.kind != .directory) { ...; break :wd; }
                    //
                    // The three checks and their three diagnostics are in
                    // `requested_working_directory_is_usable` (task tail-s1b).
                    if !requested_working_directory_is_usable(wd) {
                        break 'wd;
                    }

                    // var wd_val: configpkg.WorkingDirectory = .{ .path = wd };
                    let mut wd_val = WorkingDirectory::Path(wd.to_vec());
                    // if (wd_val.finalize(config.arenaAlloc())) |_| {
                    //     config.@"working-directory" = wd_val;
                    // } else |err| {
                    //     log.warn("error finalizing working directory config dir={s} err={}", .{ wd_val.path, err });
                    // }
                    //
                    // PORT NOTE: rustty `WorkingDirectory::finalize` is infallible
                    // (owned allocation; `arenaAlloc` collapses), so the reference
                    // `else |err|` log arm is unreachable and elided.
                    wd_val.finalize();
                    // config.@"working-directory" = wd_val;
                    config.working_directory = Some(wd_val);
                }
            }
        }

        // If we have a command from the options then we set it.
        // if (opts.command) |c_command| {
        if !opts.command.is_null() {
            // const cmd = std.mem.sliceTo(c_command, 0);
            let cmd = unsafe { CStr::from_ptr(opts.command) }.to_bytes();
            // if (cmd.len > 0) {
            if !cmd.is_empty() {
                // config.command = .{ .shell = cmd };
                config.command = Some(Command::Shell(cmd.to_vec()));
                // config.@"wait-after-command" = true;
                config.wait_after_command = true;
            }
        }

        // Apply any environment variables that were requested.
        // if (opts.env_var_count > 0) {
        if opts.env_var_count > 0 {
            // const alloc = config.arenaAlloc();  (collapses — owned allocation)
            // for (opts.env_vars.?[0..opts.env_var_count]) |env_var| {
            let env_vars = unsafe { std::slice::from_raw_parts(opts.env_vars, opts.env_var_count) };
            for env_var in env_vars {
                // const key = std.mem.sliceTo(env_var.key, 0);
                let key = unsafe { CStr::from_ptr(env_var.key) }.to_bytes();
                // const value = std.mem.sliceTo(env_var.value, 0);
                let value = unsafe { CStr::from_ptr(env_var.value) }.to_bytes();
                // try config.env.map.put(alloc, try alloc.dupeZ(u8, key), try alloc.dupeZ(u8, value));
                config.env.map.put(key.to_vec(), value.to_vec());
            }
        }

        // If we have an initial input then we set it.
        // if (opts.initial_input) |c_input| {
        if !opts.initial_input.is_null() {
            // const alloc = config.arenaAlloc();  (collapses — owned allocation)

            // We need to escape the string because the "raw" field
            // expects a Zig string.
            // var buf: std.Io.Writer.Allocating = .init(alloc);
            // defer buf.deinit();
            // try std.zig.stringEscape(std.mem.sliceTo(c_input, 0), &buf.writer);
            let mut buf: Vec<u8> = Vec::new();
            zig_string_escape(
                &mut buf,
                unsafe { CStr::from_ptr(opts.initial_input) }.to_bytes(),
            );

            // config.input.list.clearRetainingCapacity();
            config.input.list.clear();
            // try config.input.list.append(alloc, .{ .raw = try buf.toOwnedSliceSentinel(0) });
            // (PORT NOTE: rustty `ReadableIO::Raw` stores an owned `Vec<u8>` — no NUL
            // sentinel; the Zig `toOwnedSliceSentinel(0)` sentinel is a storage
            // detail with no rustty counterpart.)
            config.input.list.push(ReadableIO::Raw(buf));
        }

        // Wait after command
        // if (opts.wait_after_command) {
        if opts.wait_after_command {
            // config.@"wait-after-command" = true;
            config.wait_after_command = true;
        }

        // Initialize our surface right away. We're given a view that is
        // ready to use.
        // try self.core_surface.init(app.core_app.alloc, &config, app.core_app, app, self);
        let init_res = unsafe {
            CoreSurface::init(
                &raw mut (*surface).core_surface,
                (*(*app).core_app).alloc,
                &config,
                (*app).core_app,
                app,
                surface,
            )
        };
        if init_res.is_err() {
            // errdefers (LIFO): `defer config.deinit()`, then
            // `errdefer app.core_app.deleteSurface(self)`.
            config.deinit();
            unsafe { (*(*app).core_app).deleteSurface(surface) };
            return Err(ApprtError);
        }
        // errdefer self.core_surface.deinit();
        // (an errdefer — realized on the setFontSize failure path below.)

        // If our options requested a specific font-size, set that.
        // if (opts.font_size != 0) {
        if opts.font_size != 0.0 {
            // var font_size = self.core_surface.font_size;
            let mut font_size = unsafe { (*surface).core_surface.font_size };
            // font_size.points = opts.font_size;
            font_size.points = opts.font_size;
            // try self.core_surface.setFontSize(font_size);
            if unsafe { (*surface).core_surface.setFontSize(font_size) }.is_err() {
                // errdefers (LIFO): `errdefer self.core_surface.deinit()`, then
                // `defer config.deinit()`, then
                // `errdefer app.core_app.deleteSurface(self)`.
                unsafe { (*surface).core_surface.deinit() };
                config.deinit();
                unsafe { (*(*app).core_app).deleteSurface(surface) };
                return Err(ApprtError);
            }
        }

        // defer config.deinit(); — the success-path realization.
        config.deinit();
        Ok(())
    }

    /// Initialize the inspector instance. A surface can only have one
    /// inspector at any given time, so this will return the previous inspector
    /// if it was already initialized.
    //
    // pub fn initInspector(self: *Surface) !*Inspector { ... }
    //
    // PORT NOTE: deferred FROM E3 (its reference range, embedded.zig:614) TO E4,
    // because it constructs the real [`Inspector`] (which E2 carried only as a
    // sequencing stub). Now that E4 provides the inhabited `Inspector`, it is ported
    // LIVE. `pub` because the E9 CAPI `rustty_surface_inspector` wrapper reaches it
    // across the crate boundary (ADV-FIX 9). Reaches the Phase-3 dcimgui leaf
    // `Inspector::init` at RUNTIME (the panic is in the leaf), so a live inspector
    // cannot yet exist — `self.inspector` stays `None` in practice this milestone.
    pub fn initInspector(&mut self) -> Result<*mut Inspector, ApprtError> {
        // if (self.inspector) |v| return v;
        if let Some(v) = self.inspector {
            return Ok(v);
        }

        // const alloc = self.app.core_app.alloc;
        // const inspector = try alloc.create(Inspector);
        // errdefer alloc.destroy(inspector);
        // inspector.* = try .init(self);
        //
        // PORT NOTE: the reference allocates the `Inspector` slot FIRST then
        // placement-inits into it (with `errdefer alloc.destroy` to free on init
        // failure). In Rust the fallible `Inspector::init` is evaluated BEFORE the
        // heap `Box`, so an init error returns WITHOUT allocating (the `errdefer` is
        // subsumed — nothing to free). `Inspector::init` is a Phase-3 dcimgui leaf
        // (`unimplemented!()`), so this line panics once a live inspector is
        // requested — the honest Phase-3 boundary. The `Box` heap-pins the
        // `Inspector`; its `surface` back-pointer stays valid because the embedded
        // `Surface` is itself heap-pinned (ADV-FIX 8).
        let inspector = Inspector::init(core::ptr::from_mut(self))?;
        let inspector: *mut Inspector = Box::into_raw(Box::new(inspector));

        // self.inspector = inspector;
        self.inspector = Some(inspector);

        // return inspector;
        Ok(inspector)
    }

    // pub fn freeInspector(self: *Surface) void { ... }
    //
    // PORT NOTE: `initInspector`'s inverse (both land in E4). `Inspector`
    // create/deinit is a Phase-3 dcimgui+Metal leaf; reached by [`deinit`]. It is a
    // no-op while `inspector` is `None` — which it stays in practice this milestone,
    // since `initInspector` reaches the `Inspector::init` dcimgui leaf
    // (`unimplemented!()`) before it can set `inspector`.
    pub fn freeInspector(&mut self) {
        // if (self.inspector) |v| {
        if let Some(v) = self.inspector {
            // v.deinit();
            unsafe { (*v).deinit() };
            // self.app.core_app.alloc.destroy(v);
            //
            // PORT NOTE: `Inspector` is heap-boxed by `initInspector` (E4); free the
            // box WITHOUT re-running `Inspector`'s drop (the `deinit`-then-`destroy`
            // contract — the `App::closeSurface` precedent).
            unsafe { drop(Box::from_raw(v.cast::<ManuallyDrop<Inspector>>())) };
            // self.inspector = null;
            self.inspector = None;
        }
    }

    /// Complete a Kitty clipboard protocol write request. The apprt
    /// contributes nothing to the completion (the authoritative
    /// contents live in the request itself), but a completion that
    /// requires confirmation diverts into the confirm callback, which
    /// keeps the state alive for the asynchronous prompt.
    //
    // fn completeKittyClipboardWrite(
    //     self: *Surface,
    //     state: *apprt.ClipboardRequest,
    // ) void { ... }
    //
    /// # Safety
    /// `self_` must be the live, heap-stable embedded Surface; `state` must
    /// be a `Box::into_raw` heap request whose variant is `KittyWrite`.
    #[allow(non_snake_case)]
    unsafe fn completeKittyClipboardWrite(self_: *mut Surface, state: *mut ClipboardRequest) {
        // const alloc = self.app.core_app.alloc;
        // const kitty = state.kitty_write;
        let kitty: *mut rustty_apprt_api::KittyWrite = match unsafe { *state } {
            ClipboardRequest::KittyWrite(v) => v,
            _ => unreachable!(),
        };

        // self.core_surface.completeClipboardRequest(state.*, .{}) catch |err| switch (err) {
        //
        // (narrow raw projection to the core field — no whole-Surface `&mut`.)
        let core: *mut CoreSurface = unsafe { &raw mut (*self_).core_surface };
        match unsafe {
            (*core).completeClipboardRequest(
                *state,
                rustty_core::surface::CompleteClipboard::default(),
            )
        } {
            Ok(()) => {}

            // error.UnauthorizedPaste => { ... return; },
            Err(CoreSurfaceError::UnauthorizedPaste) => {
                // Convert the would-be contents so the permission
                // prompt can display exactly what would be written.
                // var stack = std.heap.stackFallback(1024, alloc);
                // const contents = conv_alloc.alloc(CAPI.ClipboardContent, kitty.contents.len) catch ...;
                // defer conv_alloc.free(contents);
                //
                // (the stackFallback conversion buffer is an owned `Vec` — the
                // crate's global-alloc collapse; the alloc-failure arm folds
                // into Rust's infallible allocation. The `mime` fields need a
                // NUL sentinel for the C `[*:0]` contract — the owned
                // `CString`s live for the callback's duration.)
                let kitty_ref = unsafe { &*kitty };
                let mime_cstrings: Vec<std::ffi::CString> = kitty_ref
                    .contents
                    .iter()
                    .map(|src| {
                        std::ffi::CString::new(src.mime.clone())
                            .unwrap_or_else(|_| std::ffi::CString::new("").unwrap())
                    })
                    .collect();
                let contents: Vec<ClipboardContent> = kitty_ref
                    .contents
                    .iter()
                    .zip(mime_cstrings.iter())
                    .map(|(src, mime)| ClipboardContent {
                        // .mime = src.mime, .data = src.data.ptr, .len = src.data.len,
                        mime: mime.as_ptr(),
                        data: src.data.as_ptr(),
                        len: src.data.len(),
                    })
                    .collect();
                // (the name needs the same NUL treatment.)
                let name_c: Option<std::ffi::CString> = if !kitty_ref.name.is_empty() {
                    std::ffi::CString::new(kitty_ref.name.clone()).ok()
                } else {
                    None
                };

                // self.app.opts.confirm_read_clipboard(
                //     self.userdata,
                //     &.{ .contents, .contents_len, .available = null, .available_len = 0,
                //         .name = ..., .can_remember = kitty.pw.len > 0 },
                //     state,
                //     state.*,
                // );
                let confirm = ClipboardConfirm {
                    contents: contents.as_ptr(),
                    contents_len: contents.len(),
                    available: core::ptr::null(),
                    available_len: 0,
                    name: name_c
                        .as_ref()
                        .map(|c| c.as_ptr())
                        .unwrap_or(core::ptr::null()),
                    can_remember: !kitty_ref.pw.is_empty(),
                };
                unsafe {
                    ((*(*self_).app).opts.confirm_read_clipboard)(
                        (*self_).userdata,
                        &confirm,
                        state,
                        ClipboardRequestType::KittyWrite,
                    );
                }

                // return;
                return;
            }

            // else => log.err("error completing clipboard request err={}", .{err}),
            Err(err) => {
                // log.err("error completing clipboard request err={}", .{err});
                log::error!(target: LOG_SCOPE, "error completing clipboard request err={err:?}");
            }
        }

        // We don't defer this because the clipboard confirmation route
        // preserves the clipboard request.
        // alloc.destroy(state);
        unsafe { drop(Box::from_raw(state)) };
    }

    // fn completeClipboardRequest(
    //     self: *Surface,
    //     complete: *const CAPI.ClipboardComplete,
    //     state: *apprt.ClipboardRequest,
    // ) void { ... }
    //
    // PORT NOTE (ADV-FIX 9 pub-widening): the reference is a file-private `fn`, but
    // the apprt-c `rustty_surface_complete_clipboard_request` wrapper (E9) reaches
    // it across the crate boundary, so it is `pub` here. `state` stays a raw
    // `*mut ClipboardRequest` (the heap request from [`Surface::clipboard_request`],
    // Box-allocated there).
    ///
    /// # Safety
    /// `complete` must point at a valid `ClipboardComplete` whose borrowed
    /// memory lives for the duration of the call; `state` must be the request
    /// pointer handed to the host by the read callback.
    pub unsafe fn completeClipboardRequest(
        &mut self,
        complete: *const ClipboardComplete,
        state: *mut ClipboardRequest,
    ) {
        // const alloc = self.app.core_app.alloc;
        // (PORT NOTE: `state` is a Rust `Box` from `clipboard_request`; it is reclaimed +
        // freed via `Box::from_raw` below — no core-app `alloc` handle is needed.)

        // Convert the C representations to the core types. Everything
        // remains borrowed from the caller for the duration of the call.
        // var stack = std.heap.stackFallback(1024, alloc);
        // const conv_alloc = stack.get();
        // (the stackFallback buffers are owned `Vec`s — the global-alloc
        // collapse; the alloc-failure arms fold into infallible allocation.)
        let complete_ref = unsafe { &*complete };

        // const raw_contents: []const CAPI.ClipboardContent =
        //     if (complete.contents) |v| v[0..complete.contents_len] else &.{};
        let raw_contents: &[ClipboardContent] = if !complete_ref.contents.is_null() {
            unsafe { core::slice::from_raw_parts(complete_ref.contents, complete_ref.contents_len) }
        } else {
            &[]
        };
        // for (raw_contents, contents) |raw, *content| content.* = .{
        //     .mime = std.mem.sliceTo(raw.mime, 0),
        //     .data = raw.data[0..raw.len],
        // };
        let contents: Vec<rustty_vt::clipboard::Content<'_>> = raw_contents
            .iter()
            .map(|raw| rustty_vt::clipboard::Content {
                mime: unsafe { CStr::from_ptr(raw.mime) }.to_bytes(),
                data: unsafe { core::slice::from_raw_parts(raw.data, raw.len) },
            })
            .collect();

        // const raw_available: []const [*:0]const u8 =
        //     if (complete.available) |v| v[0..complete.available_len] else &.{};
        let raw_available: &[*const c_char] = if !complete_ref.available.is_null() {
            unsafe {
                core::slice::from_raw_parts(complete_ref.available, complete_ref.available_len)
            }
        } else {
            &[]
        };
        // for (raw_available, available) |raw, *mime| { mime.* = std.mem.sliceTo(raw, 0); }
        let available: Vec<&[u8]> = raw_available
            .iter()
            .map(|raw| unsafe { CStr::from_ptr(*raw) }.to_bytes())
            .collect();

        // Attempt to complete the request, but we may request
        // confirmation.
        // self.core_surface.completeClipboardRequest(state.*, .{
        //     .contents = contents,
        //     .available = available,
        //     .confirmed = complete.confirmed,
        //     .remember = complete.remember,
        // }) catch |err| switch (err) {
        match self.core_surface.completeClipboardRequest(
            unsafe { *state },
            rustty_core::surface::CompleteClipboard {
                contents: &contents,
                available: &available,
                confirmed: complete_ref.confirmed,
                remember: complete_ref.remember,
            },
        ) {
            Ok(()) => {}

            // error.UnsafePaste,
            // error.UnauthorizedPaste,
            // => { ... return; },
            Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                // Session grant information for the permission prompt,
                // carried only by Kitty clipboard protocol requests.
                // const name: ?[*:0]const u8, const can_remember: bool = switch (state.*) {
                //     inline .kitty_read, .kitty_write => |kitty| .{ ... },
                //     else => .{ null, false },
                // };
                //
                // (the owned-name NUL dupe — see completeKittyClipboardWrite.)
                let (name_c, can_remember): (Option<std::ffi::CString>, bool) =
                    match unsafe { *state } {
                        ClipboardRequest::KittyRead(kitty) => {
                            let k = unsafe { &*kitty };
                            (
                                if !k.name.is_empty() {
                                    std::ffi::CString::new(k.name.clone()).ok()
                                } else {
                                    None
                                },
                                !k.pw.is_empty(),
                            )
                        }
                        ClipboardRequest::KittyWrite(kitty) => {
                            let k = unsafe { &*kitty };
                            (
                                if !k.name.is_empty() {
                                    std::ffi::CString::new(k.name.clone()).ok()
                                } else {
                                    None
                                },
                                !k.pw.is_empty(),
                            )
                        }
                        _ => (None, false),
                    };

                // PORT NOTE: the reference's 4th arg `state.*` (an
                // `apprt.ClipboardRequest`) IMPLICITLY coerces to the callback's
                // `apprt.ClipboardRequestType` param (Zig tagged-union → tag enum).
                // Rust has no such coercion — the tag is projected explicitly.
                let state_type = match unsafe { *state } {
                    ClipboardRequest::Paste(_) => ClipboardRequestType::Paste,
                    ClipboardRequest::Osc52Read(_) => ClipboardRequestType::Osc52Read,
                    ClipboardRequest::Osc52Write(_) => ClipboardRequestType::Osc52Write,
                    ClipboardRequest::KittyRead(_) => ClipboardRequestType::KittyRead,
                    ClipboardRequest::KittyWrite(_) => ClipboardRequestType::KittyWrite,
                    ClipboardRequest::List(_) => ClipboardRequestType::List,
                };

                // self.app.opts.confirm_read_clipboard(
                //     self.userdata,
                //     &.{
                //         .contents = complete.contents,
                //         .contents_len = complete.contents_len,
                //         .available = complete.available,
                //         .available_len = complete.available_len,
                //         .name = name,
                //         .can_remember = can_remember,
                //     },
                //     state,
                //     state.*,
                // );
                let confirm = ClipboardConfirm {
                    contents: complete_ref.contents,
                    contents_len: complete_ref.contents_len,
                    available: complete_ref.available,
                    available_len: complete_ref.available_len,
                    name: name_c
                        .as_ref()
                        .map(|c| c.as_ptr())
                        .unwrap_or(core::ptr::null()),
                    can_remember,
                };
                unsafe {
                    ((*self.app).opts.confirm_read_clipboard)(
                        self.userdata,
                        &confirm,
                        state,
                        state_type,
                    );
                }

                // return;
                return;
            }

            // else => log.err("error completing clipboard request err={}", .{err}),
            Err(err) => {
                // log.err("error completing clipboard request err={}", .{err});
                log::error!(target: LOG_SCOPE, "error completing clipboard request err={err:?}");
            }
        }

        // We don't defer this because the clipboard confirmation route
        // preserves the clipboard request.
        // alloc.destroy(state);
        //
        // (PORT NOTE: reclaim + drop the `Box` allocated in [`Surface::clipboard_request`].)
        unsafe { drop(Box::from_raw(state)) };
    }

    // fn denyClipboardRequest(
    //     self: *Surface,
    //     state: *apprt.ClipboardRequest,
    // ) void {
    //     self.core_surface.denyClipboardRequest(state.*);
    //     self.app.core_app.alloc.destroy(state);
    // }
    //
    // PORT NOTE (ADV-FIX 9 pub-widening): file-private in the reference;
    // reached by the apprt-c `rustty_surface_deny_clipboard_request` wrapper.
    ///
    /// # Safety
    /// `state` must be the live request pointer handed to the host by the
    /// read callback (it is invalidated by this call).
    pub unsafe fn denyClipboardRequest(&mut self, state: *mut ClipboardRequest) {
        // self.core_surface.denyClipboardRequest(state.*);
        self.core_surface.denyClipboardRequest(unsafe { *state });
        // self.app.core_app.alloc.destroy(state);
        // (PORT NOTE: reclaim + drop the `Box` allocated in [`Surface::clipboard_request`].)
        unsafe { drop(Box::from_raw(state)) };
    }

    // pub fn refresh(self: *Surface) void { ... }
    pub fn refresh(&mut self) {
        // self.core_surface.refreshCallback() catch |err| {
        //     log.err("error in refresh callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.refreshCallback() {
            // log.err("error in refresh callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in refresh callback err={err:?}");
        }
    }

    // pub fn draw(self: *Surface) void { ... }
    //
    // PORT NOTE: `draw` reaches the Phase-3 GPU renderer leaf at RUNTIME (core
    // `Surface::draw` → the raw-self `draw_frame_raw` chain, W5); the delegation is
    // ported LIVE (EMBEDDED_BLUEPRINT §1).
    //
    // W5 (`W5_DESIGN §2.5` caller 2 / §3.5): RAW-self, per the Zig `self: *Surface`
    // receiver. `core_surface` is embedded by value (`:889`), so an `&mut self`
    // receiver would protect it against the concurrent render thread; the raw-self
    // cascade terminates at the C-ABI caller (`rustty_surface_draw`).
    //
    /// # Safety
    /// `self_` is the stable embedded Surface allocation; no protected `&mut Surface`
    /// may span the call.
    pub unsafe fn draw(self_: *mut Surface) {
        // self.core_surface.draw() catch |err| {
        //     log.err("error in draw err={}", .{err});
        //     return;
        // };
        if let Err(err) = unsafe { CoreSurface::draw(&raw mut (*self_).core_surface) } {
            // log.err("error in draw err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in draw err={err:?}");
        }
    }

    // pub fn updateContentScale(self: *Surface, x: f64, y: f64) void { ... }
    pub fn updateContentScale(&mut self, x: f64, y: f64) {
        // We are an embedded API so the caller can send us all sorts of
        // garbage. We want to make sure that the float values are valid
        // and we don't want to support fractional scaling below 1.
        // const x_scaled = @max(1, if (std.math.isNan(x)) 1 else x);
        let x_scaled = (if x.is_nan() { 1.0 } else { x }).max(1.0);
        // const y_scaled = @max(1, if (std.math.isNan(y)) 1 else y);
        let y_scaled = (if y.is_nan() { 1.0 } else { y }).max(1.0);

        // self.content_scale = .{
        //     .x = @floatCast(x_scaled),
        //     .y = @floatCast(y_scaled),
        // };
        self.content_scale = ContentScale {
            x: x_scaled as f32,
            y: y_scaled as f32,
        };

        // self.core_surface.contentScaleCallback(self.content_scale) catch |err| {
        //     log.err("error in content scale callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.contentScaleCallback(self.content_scale) {
            // log.err("error in content scale callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in content scale callback err={err:?}");
        }
    }

    // pub fn updateSize(self: *Surface, width: u32, height: u32) void { ... }
    pub fn updateSize(&mut self, width: u32, height: u32) {
        // Runtimes sometimes generate superfluous resize events even
        // if the size did not actually change (SwiftUI). We check
        // that the size actually changed from what we last recorded
        // since resizes are expensive.
        // if (self.size.width == width and self.size.height == height) return;
        if self.size.width == width && self.size.height == height {
            return;
        }

        // self.size = .{
        //     .width = width,
        //     .height = height,
        // };
        self.size = SurfaceSize { width, height };

        // Call the primary callback.
        // self.core_surface.sizeCallback(self.size) catch |err| {
        //     log.err("error in size callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.sizeCallback(self.size) {
            // log.err("error in size callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in size callback err={err:?}");
        }
    }

    // pub fn colorSchemeCallback(self: *Surface, scheme: apprt.ColorScheme) void { ... }
    //
    // PORT NOTE (N456, ruling 57): raw-self. The core callback runs the host's
    // soft `reload_config` handler, which calls `rustty_surface_update_config`
    // on this surface synchronously, so no protected reference to it may span
    // the call.
    ///
    /// # Safety
    /// `self_` is a live surface from `rustty_surface_new`; main thread; no
    /// protected reference to it may span the call.
    pub unsafe fn colorSchemeCallback(self_: *mut Self, scheme: ColorScheme) {
        // self.core_surface.colorSchemeCallback(scheme) catch |err| {
        //     log.err("error setting color scheme err={}", .{err});
        //     return;
        // };
        // SAFETY: `self_` is live (this fn's contract); the core surface is
        // reached by place, never through a `&mut Self`.
        if let Err(err) =
            unsafe { CoreSurface::colorSchemeCallback(&raw mut (*self_).core_surface, scheme) }
        {
            // log.err("error setting color scheme err={}", .{err});
            log::error!(target: LOG_SCOPE, "error setting color scheme err={err:?}");
        }
    }

    // pub fn mouseButtonCallback(
    //     self: *Surface,
    //     action: input.MouseButtonState,
    //     button: input.MouseButton,
    //     mods: input.Mods,
    // ) bool { ... }
    pub fn mouseButtonCallback(
        &mut self,
        action: mouse::ButtonState,
        button: mouse::Button,
        mods: Mods,
    ) -> bool {
        // return self.core_surface.mouseButtonCallback(action, button, mods) catch |err| {
        //     log.err("error in mouse button callback err={}", .{err});
        //     return false;
        // };
        //
        // PARITY: not `.unwrap_or(false)` — the arm IS the reference's catch block
        // with its log site (the core surface.rs `performAction`-result precedent).
        #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
        match self.core_surface.mouseButtonCallback(action, button, mods) {
            Ok(v) => v,
            Err(err) => {
                // log.err("error in mouse button callback err={}", .{err});
                log::error!(target: LOG_SCOPE, "error in mouse button callback err={err:?}");
                false
            }
        }
    }

    // pub fn mousePressureCallback(
    //     self: *Surface,
    //     stage: input.MousePressureStage,
    //     pressure: f64,
    // ) void { ... }
    pub fn mousePressureCallback(&mut self, stage: mouse::PressureStage, pressure: f64) {
        // self.core_surface.mousePressureCallback(stage, pressure) catch |err| {
        //     log.err("error in mouse pressure callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.mousePressureCallback(stage, pressure) {
            // log.err("error in mouse pressure callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in mouse pressure callback err={err:?}");
        }
    }

    // pub fn scrollCallback(
    //     self: *Surface,
    //     xoff: f64,
    //     yoff: f64,
    //     mods: input.ScrollMods,
    // ) void { ... }
    pub fn scrollCallback(&mut self, xoff: f64, yoff: f64, mods: mouse::ScrollMods) {
        // self.core_surface.scrollCallback(xoff, yoff, mods) catch |err| {
        //     log.err("error in scroll callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.scrollCallback(xoff, yoff, mods) {
            // log.err("error in scroll callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in scroll callback err={err:?}");
        }
    }

    // pub fn cursorPosCallback(
    //     self: *Surface,
    //     x: f64,
    //     y: f64,
    //     mods: input.Mods,
    // ) void { ... }
    pub fn cursorPosCallback(&mut self, x: f64, y: f64, mods: Mods) {
        // Convert our unscaled x/y to scaled.
        // const pos = self.cursorPosToPixels(.{
        //     .x = @floatCast(x),
        //     .y = @floatCast(y),
        // }) catch |err| {
        //     log.err(
        //         "error converting cursor pos to scaled pixels in cursor pos callback err={}",
        //         .{err},
        //     );
        //     return;
        // };
        let pos = match self.cursorPosToPixels(CursorPos {
            x: x as f32,
            y: y as f32,
        }) {
            Ok(v) => v,
            Err(err) => {
                // log.err(
                //     "error converting cursor pos to scaled pixels in cursor pos callback err={}",
                //     .{err},
                // );
                log::error!(target: LOG_SCOPE, "error converting cursor pos to scaled pixels in cursor pos callback err={err:?}");
                return;
            }
        };

        // There are cases where the platform reports a mouse motion event
        // without the cursor actually moving. For example, on macOS, updating
        // the window title can trigger a phantom mouse-move event at the same
        // coordinates. This can cause the mouse to incorrectly unhide when
        // mouse-hide-while-typing is enabled (commonly seen with TUI apps
        // like Zellij that frequently update the title). To prevent incorrect
        // behavior, we only continue with callback logic if the cursor has
        // actually moved.
        // if (@abs(self.cursor_pos.x - pos.x) < 1 and
        //     @abs(self.cursor_pos.y - pos.y) < 1) return;
        if (self.cursor_pos.x - pos.x).abs() < 1.0 && (self.cursor_pos.y - pos.y).abs() < 1.0 {
            return;
        }

        // self.cursor_pos = pos;
        self.cursor_pos = pos;

        // self.core_surface.cursorPosCallback(self.cursor_pos, mods) catch |err| {
        //     log.err("error in cursor pos callback err={}", .{err});
        //     return;
        // };
        //
        // PORT NOTE: core `cursorPosCallback` takes `mods: ?input.Mods`; the
        // reference's non-optional `mods` coerces to `Some`.
        if let Err(err) = self
            .core_surface
            .cursorPosCallback(self.cursor_pos, Some(mods))
        {
            // log.err("error in cursor pos callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in cursor pos callback err={err:?}");
        }
    }

    // pub fn preeditCallback(self: *Surface, preedit_: ?[]const u8) void { ... }
    pub fn preeditCallback(&mut self, preedit_: Option<&[u8]>) {
        // _ = self.core_surface.preeditCallback(preedit_) catch |err| {
        //     log.err("error in preedit callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.preeditCallback(preedit_) {
            // log.err("error in preedit callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
        }
    }

    // pub fn textCallback(self: *Surface, text: []const u8) void { ... }
    pub fn textCallback(&mut self, text: &[u8]) {
        // _ = self.core_surface.textCallback(text) catch |err| {
        //     log.err("error in key callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.textCallback(text) {
            // log.err("error in key callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in key callback err={err:?}");
        }
    }

    // pub fn focusCallback(self: *Surface, focused: bool) void { ... }
    pub fn focusCallback(&mut self, focused: bool) {
        // self.core_surface.focusCallback(focused) catch |err| {
        //     log.err("error in focus callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.focusCallback(focused) {
            // log.err("error in focus callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in focus callback err={err:?}");
        }
    }

    // pub fn occlusionCallback(self: *Surface, visible: bool) void { ... }
    pub fn occlusionCallback(&mut self, visible: bool) {
        // self.core_surface.occlusionCallback(visible) catch |err| {
        //     log.err("error in occlusion callback err={}", .{err});
        //     return;
        // };
        if let Err(err) = self.core_surface.occlusionCallback(visible) {
            // log.err("error in occlusion callback err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in occlusion callback err={err:?}");
        }
    }

    /// reference `fn queueInspectorRender(self: *Surface) void` (embedded.zig:922)
    /// — called by [`App::redrawInspector`]. Dispatches the `render_inspector`
    /// action to the C host (the same `performAction` -> host-callback seam every
    /// other apprt action already routes through), which schedules the next Metal
    /// frame that ultimately reaches [`Inspector::renderMetal`] via the apprt-c
    /// `rustty_inspector_metal_render` export (INV-12's render-thread wakeup is a
    /// separate, `Inspector::render`-internal notify — this fn only asks the host
    /// to draw a frame at all).
    //
    // fn queueInspectorRender(self: *Surface) void {
    //     _ = self.app.performAction(
    //         .{ .surface = &self.core_surface },
    //         .render_inspector,
    //         {},
    //     ) catch |err| {
    //         log.err("error rendering the inspector err={}", .{err});
    //         return;
    //     };
    // }
    pub fn queueInspectorRender(&mut self) {
        // SAFETY: `self.app` is a live `*mut App` for the lifetime of this Surface
        // (E1/E2 invariant — the embedded app outlives every surface it owns).
        if let Err(err) = unsafe { &*self.app }.perform_action(
            Target::surface_ref(&self.core_surface),
            Action::RenderInspector,
        ) {
            // log.err("error rendering the inspector err={}", .{err});
            log::error!(target: LOG_SCOPE, "error rendering the inspector err={err:?}");
        }
    }

    // pub fn newSurfaceOptions(self: *const Surface, context: apprt.surface.NewSurfaceContext) apprt.Surface.Options { ... }
    //
    // PORT NOTE: `apprt.Surface.Options` is the embedded backend's own
    // [`SurfaceOptions`] (the `rustty_surface_config_s`); the reference struct-literal
    // relies on Zig field defaults for every field it does not set. Rust has no struct
    // field defaults, so the elided fields are spelled out below with those exact
    // defaults. Reached only by the E7 CAPI `rustty_surface_inherited_config`
    // (returned BY VALUE to the C host).
    pub fn newSurfaceOptions(&self, context: NewSurfaceContext) -> SurfaceOptions {
        // PORT NOTE: `self.app.config` is read through the E1 `UnsafeCell` accessor.
        let config = unsafe { &*(*self.app).config.get() };

        // const font_size: f32 = font_size: {
        //     if (!self.app.config.@"window-inherit-font-size") break :font_size 0;
        //     break :font_size self.core_surface.font_size.points;
        // };
        let font_size: f32 = 'font_size: {
            if !config.window_inherit_font_size {
                break 'font_size 0.0;
            }
            self.core_surface.font_size.points
        };

        // const working_directory: ?[*:0]const u8 = wd: {
        //     if (!apprt.surface.shouldInheritWorkingDirectory(context, &self.app.config)) break :wd null;
        //     const cwd = self.core_surface.pwd(self.app.core_app.alloc) catch null orelse break :wd null;
        //     defer self.app.core_app.alloc.free(cwd);
        //     break :wd self.app.core_app.alloc.dupeZ(u8, cwd) catch null;
        // };
        //
        // PORT NOTE: rustty `pwd` is infallible (`Option<Vec<u8>>`; the reference's
        // `catch null` collapses to the `None` arm). `cwd` is an owned `Vec<u8>`
        // dropped at scope end (the reference `defer …free(cwd)`). An interior-NUL
        // `cwd` is REACHABLE (OSC 7 `%00`) — see `inherited_working_directory_c`.
        //
        // PORT NOTE: reference defect (R13), UD68. The reference's `dupeZ` is a
        // GPA allocation that nobody frees: the macOS host copies it
        // (`String(cString:)`, `SurfaceView.swift:606`) and the C ABI gives it no
        // free function, so every inherited tab, split and window leaks one. The
        // port keeps the C ABI (the same `const char*` in the same by-value
        // struct) but keeps OWNERSHIP surface-side: the string lives in
        // `self.inherited_working_directory` until the next call on this surface
        // or its `deinit`, whichever comes first. The host reads it before
        // either can happen.
        let working_directory: Option<CString> = 'wd: {
            if !should_inherit_working_directory(context, config) {
                break 'wd None;
            }
            let cwd = match self
                .core_surface
                .pwd(unsafe { (*(*self.app).core_app).alloc })
            {
                Some(v) => v,
                None => break 'wd None,
            };
            inherited_working_directory_c(cwd)
        };
        let working_directory = self.inherited_working_directory.hand_out(working_directory);

        // return .{
        //     .font_size = font_size,
        //     .working_directory = working_directory,
        //     .context = context,
        // };
        SurfaceOptions {
            font_size,
            working_directory,
            context,
            // (elided reference fields → their `Surface.Options` Zig field defaults)
            // platform_tag: c_int = 0,
            platform_tag: 0,
            // platform: Platform.C = undefined,
            //
            // PORT NOTE: Zig `undefined` is never read here (the C host overwrites
            // `platform`/`platform_tag` before `rustty_surface_new`); a zeroed union
            // is the faithful placeholder.
            platform: PlatformC {
                macos: PlatformCMacOS {
                    nsview: core::ptr::null_mut(),
                },
            },
            // userdata: ?*anyopaque = null,
            userdata: core::ptr::null_mut(),
            // scale_factor: f64 = 1,
            scale_factor: 1.0,
            // command: ?[*:0]const u8 = null,
            command: core::ptr::null(),
            // env_vars: ?[*]EnvVar = null,
            env_vars: core::ptr::null(),
            // env_var_count: usize = 0,
            env_var_count: 0,
            // initial_input: ?[*:0]const u8 = null,
            initial_input: core::ptr::null(),
            // wait_after_command: bool = false,
            wait_after_command: false,
        }
    }

    /// The cursor position from the host directly is in screen coordinates but
    /// all our interface works in pixels.
    //
    // fn cursorPosToPixels(self: *const Surface, pos: apprt.CursorPos) !apprt.CursorPos { ... }
    fn cursorPosToPixels(&self, pos: CursorPos) -> Result<CursorPos, ApprtError> {
        // const scale = try self.getContentScale();
        let scale = self.get_content_scale()?;
        // return .{ .x = pos.x * scale.x, .y = pos.y * scale.y };
        Ok(CursorPos {
            x: pos.x * scale.x,
            y: pos.y * scale.y,
        })
    }
}

// The EMBEDDED_BLUEPRINT §2 growth-point table: the exact set of methods core's
// `Surface.zig`/`App.zig` call on `rt_surface`, ported faithfully from
// embedded.zig:597-982. In the reference these are plain `pub fn`s on `Surface`;
// Rust routes them through the `apprt_api::Surface` trait (the A5 seam). This is
// the FIRST concrete backend to inhabit `type Core =
// rustty_core::Surface<embedded::App>`.
impl ApprtSurface for Surface {
    type App = App;
    type Core = CoreSurface;

    // pub fn core(self: *Surface) *CoreSurface { return &self.core_surface; }
    fn core(&self) -> *mut Self::Core {
        // The returned pointer is as stable as the backend's surface allocation
        // (the embedded Surface is heap-boxed + pinned, ADV-FIX 8).
        core::ptr::from_ref(&self.core_surface).cast_mut()
    }

    // pub fn rtApp(self: *const Surface) *App { return self.app; }
    fn rt_app(&self) -> *mut Self::App {
        self.app
    }

    // W-ATTACH: expose the host view pointer + content scale so core routes
    // them into the renderer's windowed layer-attach at finalizeSurfaceInit
    // (embedded holds the raw view in `platform.macos.nsview` /
    // `platform.ios.uiview`, blueprint §5/§8.1).
    //
    // PORT NOTE (R1 parity): the reference `Metal.init` reads BOTH the macOS
    // `nsview` and the iOS `uiview` off `apprt.Surface`'s platform union; the iOS
    // arm was previously dropped (returned null), so restore it. The `nsview` /
    // `uiview` field access only exists on darwin (`MacOS`/`IOS` are ZSTs
    // off-darwin, §2.9), so both concrete arms are `#[cfg]`-gated to darwin and a
    // non-darwin wildcard returns null (the platform never renders there).
    fn host_view(&self) -> *mut c_void {
        platform_host_view(&self.platform)
    }

    fn host_content_scale(&self) -> f64 {
        self.content_scale.x.into()
    }

    // pub fn close(self: *const Surface, process_alive: bool) void { ... }
    //
    // PORT NOTE (N465): raw-self (see the seam). `self_` is read by place for
    // `app` and `userdata`; no reference to this surface (which holds the
    // core surface inline) spans `func`, whose macOS handler may re-enter it.
    unsafe fn close(self_: *const Self, process_alive: bool) {
        // const func = self.app.opts.close_surface orelse {
        //     log.info("runtime embedder does not support closing a surface", .{});
        //     return;
        // };
        let app = unsafe { (*self_).app };
        let func = match unsafe { (*app).opts.close_surface } {
            Some(func) => func,
            None => {
                // log.info("runtime embedder does not support closing a surface", .{});
                log::info!(target: LOG_SCOPE, "runtime embedder does not support closing a surface");
                return;
            }
        };

        // func(self.userdata, process_alive);
        let userdata = unsafe { (*self_).userdata };
        func(userdata, process_alive);
    }

    // pub fn deinit(self: *Surface) void { ... }
    fn deinit(&mut self) {
        // Shut down our inspector
        // self.freeInspector();
        self.freeInspector();

        // Free our title
        // if (self.title) |v| self.app.core_app.alloc.free(v);
        // (PORT NOTE: drop the owned `CString`.)
        self.title = None;

        // PORT NOTE: reference defect (R13), UD68 — free the last inherited
        // working directory handed to the host (the reference leaks it).
        self.inherited_working_directory.clear();

        // Remove ourselves from the list of known surfaces in the app.
        // self.app.core_app.deleteSurface(self);
        //
        // PORT NOTE: `self.app` (a raw `*mut App`) is captured BEFORE the raw
        // `self` pointer so `deleteSurface(self_ptr)` uses only raw pointers (the
        // `&mut`-args-strongly-protected discipline — Zig pointers are unprotected).
        let app = self.app;
        let self_ptr = core::ptr::from_mut(self);
        unsafe { (*(*app).core_app).deleteSurface(self_ptr) };

        // Clean up our core surface so that all the rendering and IO stop.
        // self.core_surface.deinit();
        unsafe { self.core_surface.deinit() };
    }

    // pub fn getContentScale(self: *const Surface) !apprt.ContentScale { return self.content_scale; }
    fn get_content_scale(&self) -> Result<ContentScale, ApprtError> {
        Ok(self.content_scale)
    }

    // pub fn getSize(self: *const Surface) !apprt.SurfaceSize { return self.size; }
    fn get_size(&self) -> Result<SurfaceSize, ApprtError> {
        Ok(self.size)
    }

    // pub fn getTitle(self: *Surface) ?[:0]const u8 { return self.title; }
    fn get_title(&self) -> Option<&CStr> {
        self.title.as_deref()
    }

    // pub fn supportsClipboard(self: *const Surface, clipboard_type: apprt.Clipboard) bool { ... }
    fn supports_clipboard(&self, clipboard_type: Clipboard) -> bool {
        // return switch (clipboard_type) {
        match clipboard_type {
            // .standard => true,
            Clipboard::Standard => true,
            // .selection => self.app.opts.supports_selection_clipboard,
            Clipboard::Selection => unsafe { (*self.app).opts.supports_selection_clipboard },

            // No embedder can serve the primary selection: the embedding
            // API has no option to declare support for it, and macOS (the
            // only GUI embedder) has no primary selection. The selection
            // flag above covers a distinct custom pasteboard, not this.
            // .primary => false,
            Clipboard::Primary => false,
        }
    }

    // pub fn clipboardRequest(self: *Surface, clipboard_type, state)
    //     !apprt.ClipboardReadResult { ... }
    fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<ClipboardReadResult, ApprtError> {
        // Kitty clipboard writes carry their own contents and read
        // nothing from the clipboard, so they skip the read callback
        // entirely: the request is completed immediately, and a
        // completion that requires confirmation diverts into the
        // apprt confirmation flow just like a read.
        // if (state == .kitty_write) { ... }
        if let ClipboardRequest::KittyWrite(_) = state {
            // A write to a location the embedder can't serve is
            // rejected before the transaction completes so the caller
            // answers ENOSYS without a permission prompt, matching
            // reads, where the embedder reports an unsupported
            // location from the read callback. The write callback only
            // fires after any prompt, so it is too late to reject.
            if !self.supports_clipboard(clipboard_type) {
                return Ok(ClipboardReadResult::Unsupported);
            }

            // const state_ptr = try alloc.create(apprt.ClipboardRequest);
            // state_ptr.* = state;
            // self.completeKittyClipboardWrite(state_ptr);
            // return .started;
            //
            // SAFETY / PORT NOTE (RE-ENTRANCY HAZARD, assessment C #2): the
            // reference completes the write SYNCHRONOUSLY here — a call back
            // into the core surface from inside the core surface's own
            // `startClipboardRequest` frame. The Rust flow mirrors it through
            // a raw self-pointer (`&self` is the trait receiver; the embedded
            // Surface allocation is heap-stable), so a nested `&mut
            // CoreSurface` briefly aliases the caller frames above — the
            // documented raw-alias interleave this seam inherits from the
            // Zig shape. FOLLOW-UP (mirihar doctrine): prove/shape this on an
            // isolated harness like `.port/phase-2/mirihar-chunk3b`.
            let state_ptr: *mut ClipboardRequest = Box::into_raw(Box::new(state));
            let this: *mut Surface = self as *const Surface as *mut Surface;
            unsafe { Surface::completeKittyClipboardWrite(this, state_ptr) };
            return Ok(ClipboardReadResult::Started);
        }

        // The representations the read wants served. Text-only
        // requesters ask for the canonical text type; Kitty clipboard
        // reads ask for exactly what the program requested, with
        // text-like aliases normalized so the embedder never has to
        // know about them.
        // var mimes_buf: [terminal.kitty.clipboard.max_read_mimes][*:0]const u8 = undefined;
        // const mimes: []const [*:0]const u8 = switch (state) { ... };
        //
        // (PORT NOTE: the Rust request mimes are plain `Vec<u8>` — no NUL
        // sentinel — so the non-text arms are NUL-terminated into owned
        // `CString`s for the C callback's `[*:0]` contract; the Zig `[:0]`
        // slices carried the sentinel already.)
        static TEXT_PLAIN: &[u8] = b"text/plain\0";
        let mut mime_cstrings: Vec<std::ffi::CString> = Vec::new();
        let mimes: Vec<*const c_char> = match &state {
            // .paste, .osc_52_read => &.{"text/plain"},
            ClipboardRequest::Paste(_) | ClipboardRequest::Osc52Read(_) => {
                vec![TEXT_PLAIN.as_ptr().cast()]
            }

            // A mode 5522 paste event only lists types and must not read
            // any clipboard data.
            // .list => &.{},
            ClipboardRequest::List(_) => Vec::new(),

            // .kitty_read => |kitty| mimes: { ... },
            ClipboardRequest::KittyRead(kitty) => {
                // SAFETY: `*kitty` is the leaked single-owner Box; alive for
                // this call (destroyed only by the core completion paths).
                let kitty_ref = unsafe { &**kitty };
                // assert(kitty.mimes.len <= mimes_buf.len);
                debug_assert!(kitty_ref.mimes.len() <= rustty_vt::kitty::clipboard::max_read_mimes);
                kitty_ref
                    .mimes
                    .iter()
                    .map(|mime| {
                        // dst.* = if (terminal.clipboard.isTextMime(mime))
                        //     "text/plain"
                        // else
                        //     mime.ptr;
                        if rustty_vt::clipboard::is_text_mime(mime) {
                            TEXT_PLAIN.as_ptr().cast()
                        } else {
                            let c = std::ffi::CString::new(mime.clone())
                                .unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
                            let ptr = c.as_ptr();
                            mime_cstrings.push(c);
                            ptr
                        }
                    })
                    .collect()
            }

            // Handled above.
            // .kitty_write => unreachable,
            ClipboardRequest::KittyWrite(_) => unreachable!(),

            // No clipboard write code paths travel through this function
            // .osc_52_write => unreachable,
            ClipboardRequest::Osc52Write(_) => unreachable!(),
        };
        // const list = switch (state) { ... };
        let list = match &state {
            // Paste events need the full MIME listing without reading any
            // representation. Kitty reads only ask for it when requested.
            ClipboardRequest::List(_) => true,
            ClipboardRequest::KittyRead(kitty) => unsafe { (**kitty).list },
            _ => false,
        };

        // We need to allocate to get a pointer to store our clipboard request
        // so that it is stable until the read_clipboard callback and call
        // complete_clipboard_request. This sucks but clipboard requests aren't
        // high throughput so it's probably fine.
        // const alloc = self.app.core_app.alloc;
        // const state_ptr = try alloc.create(apprt.ClipboardRequest);
        // errdefer alloc.destroy(state_ptr);
        // state_ptr.* = state;
        let state_ptr: *mut ClipboardRequest = Box::into_raw(Box::new(state));

        // const result = self.app.opts.read_clipboard(
        //     self.userdata,
        //     @intCast(@intFromEnum(clipboard_type)),
        //     state_ptr,
        //     mimes.ptr,
        //     mimes.len,
        //     list,
        // );
        //
        // PORT NOTE (D11 narrowing): the callback returns the raw `c_int`;
        // an out-of-range host value (where the reference's closed enum
        // would trap in a safety-checked build) is narrowed to `.unsupported`
        // — fail closed, the request is not considered started.
        let raw = unsafe {
            ((*self.app).opts.read_clipboard)(
                self.userdata,
                clipboard_type as c_int,
                state_ptr,
                mimes.as_ptr(),
                mimes.len(),
                list,
            )
        };
        let result =
            ClipboardReadResult::from_c_int(raw).unwrap_or(ClipboardReadResult::Unsupported);
        // (`mime_cstrings` lives to here — the callback only borrows them.)
        drop(mime_cstrings);

        // Only a started request completes later and keeps the state.
        // if (result != .started) alloc.destroy(state_ptr);
        // return result;
        if result != ClipboardReadResult::Started {
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        // (On start `state_ptr` is intentionally leaked to the host — it owns
        // the request until `complete_clipboard_request` /
        // `deny_clipboard_request` frees it, E9.)
        Ok(result)
    }

    // pub fn setClipboard(self: *const Surface, clipboard_type, contents, confirm) !void { ... }
    fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[rustty_apprt_api::structs::ClipboardContent<'_>],
        confirm: bool,
    ) -> Result<(), ApprtError> {
        // const alloc = self.app.core_app.alloc;
        // const array = try alloc.alloc(CAPI.ClipboardContent, contents.len);
        // defer alloc.free(array);
        // for (contents, 0..) |content, i| {
        //     array[i] = .{ .mime = content.mime, .data = content.data.ptr,
        //                   .len = content.data.len };
        // }
        //
        // PORT NOTE (`8c7a34d4c`): `data` is binary-safe with an explicit `len`
        // now (no sentinel); `mime` keeps the C `[*:0]` contract — the Rust
        // seam slices carry no sentinel, so the mimes are NUL-duped into owned
        // `CString`s for the duration of the callback (the heap
        // `alloc.alloc`/`defer free` pair is the owned `Vec`s, dropped at
        // scope end).
        let mime_cstrings: Vec<std::ffi::CString> = contents
            .iter()
            .map(|content| {
                std::ffi::CString::new(content.mime.to_vec())
                    .unwrap_or_else(|_| std::ffi::CString::new("").unwrap())
            })
            .collect();
        let array: Vec<ClipboardContent> = contents
            .iter()
            .zip(mime_cstrings.iter())
            .map(|(content, mime)| ClipboardContent {
                mime: mime.as_ptr(),
                data: content.data.as_ptr(),
                len: content.data.len(),
            })
            .collect();

        // self.app.opts.write_clipboard(
        //     self.userdata,
        //     @intCast(@intFromEnum(clipboard_type)),
        //     array.ptr,
        //     array.len,
        //     confirm,
        // );
        unsafe {
            ((*self.app).opts.write_clipboard)(
                self.userdata,
                clipboard_type as c_int,
                array.as_ptr(),
                array.len(),
                confirm,
            );
        }

        Ok(())
    }

    // pub fn getCursorPos(self: *const Surface) !apprt.CursorPos { return self.cursor_pos; }
    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError> {
        Ok(self.cursor_pos)
    }

    // pub fn defaultTermioEnv(self: *const Surface) !std.process.EnvMap { ... }
    fn default_termio_env(&self) -> Result<rustty_global::EnvMap, ApprtError> {
        // const alloc = self.app.core_app.alloc;
        // var env = try global.environMap();
        // errdefer env.deinit();
        //
        // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` was deleted with
        // `src/os/env.zig` (`e8525c0fd`); the replacement is the captured
        // environment block's map. rustty's `environ_map` is infallible and
        // returns an owned map; on non-darwin `env` is never mutated below —
        // silence `unused_mut` there.
        #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_mut))]
        let mut env = rustty_global::environ_map();

        // if (comptime builtin.target.os.tag.isDarwin()) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            // if (env.get("__XCODE_BUILT_PRODUCTS_DIR_PATHS") != null) {
            if env.hasv(b"__XCODE_BUILT_PRODUCTS_DIR_PATHS") {
                // env.remove("__XCODE_BUILT_PRODUCTS_DIR_PATHS");
                env.removev(b"__XCODE_BUILT_PRODUCTS_DIR_PATHS");
                env.removev(b"__XPC_DYLD_LIBRARY_PATH");
                env.removev(b"DYLD_FRAMEWORK_PATH");
                env.removev(b"DYLD_INSERT_LIBRARIES");
                env.removev(b"DYLD_LIBRARY_PATH");
                env.removev(b"LD_LIBRARY_PATH");
                env.removev(b"SECURITYSESSIONID");
                env.removev(b"XPC_SERVICE_NAME");
            }

            // Remove this so that running `rustty` within Rustty works.
            // env.remove("RUSTTY_MAC_LAUNCH_SOURCE");
            //
            // PORT NOTE (R2 identity): the launch-source env var is `RUSTTY_MAC_LAUNCH_SOURCE`
            // (the rustty-os `desktop.rs` `launchedFromDesktop` precedent).
            env.removev(b"RUSTTY_MAC_LAUNCH_SOURCE");

            // If we were launched from the desktop then we want to
            // remove the LANGUAGE env var so that we don't inherit
            // our translation settings for Rustty. If we aren't from
            // the desktop then we didn't set our LANGUAGE var so we
            // don't need to remove it.
            // if (internal_os.launchedFromDesktop()) env.remove("LANGUAGE");
            if rustty_os::desktop::launchedFromDesktop() {
                env.removev(b"LANGUAGE");
            }
        }

        // return env;
        Ok(env)
    }
}
