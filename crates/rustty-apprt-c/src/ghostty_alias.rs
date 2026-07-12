//! `ghostty_*` C-ABI alias layer (Branch A) — GENERATED for the macOS Swift-app tier.
//!
//! Each entry is a thin `#[no_mangle]` forwarder `ghostty_<name>` that calls the
//! canonical `rustty_<name>` (`crate::rustty_*`) with byte-identical arguments. This
//! gives `librustty_apprt_c.a` BOTH symbol spellings from one body so the reused
//! upstream Ghostty Swift app (which hard-codes `ghostty_*`) links unchanged, while
//! `rustty_*` stays the canonical rustty-native ABI (R2). PREPORT_SWIFT_APP §3
//! Branch A, mechanism (a) (thin wrappers; staticlib + xcframework-safe, avoids the
//! linker-alias / `objcopy --add-symbol` portability risk). Compiled only under the
//! `identity-ghostty` cargo feature.
//!
//! GENERATED from the `rustty_*` decls in `lib.rs` — one forwarder per app export
//! DEFINED in this crate (the `apprt/embedded` surface + `rustty_init` /
//! `rustty_string_free` + the `main_c` hub's `rustty_cli_try_action` / `rustty_info` /
//! `rustty_translate`). The 6 Darwin-only exports (PREPORT §0) are gated
//! `any(target_os = "macos", target_os = "ios")` — the iOS-in-scope lock widens the
//! upstream `isDarwin()` comptime gate (PREPORT §5). The `config` / `benchmark`
//! exports of the 90-symbol app C-ABI live in their own crates (`rustty-config` /
//! `rustty-bench`) and carry their own alias layers; with `translate` / `info` now
//! ported, this crate's forwarders cover all four `main_c` hub exports.

#![allow(clippy::missing_safety_doc)]

use super::*;

#[no_mangle]
pub unsafe extern "C" fn ghostty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_init(argc, argv) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_cli_try_action() {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_cli_try_action() }
}
#[no_mangle]
pub extern "C" fn ghostty_info() -> Info {
    crate::rustty_info()
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_translate(msgid: *const c_char) -> *const c_char {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_translate(msgid) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_new(opts: *const Options, config: *const Config) -> *mut App {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_new(opts, config) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_tick(v: *mut App) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_tick(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_userdata(v: *mut App) -> *mut c_void {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_userdata(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_free(v: *mut App) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_free(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_set_focus(app: *mut App, focused: bool) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_set_focus(app, focused) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_key(app: *mut App, event: KeyEvent) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_key(app, event) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_key_is_binding(
    config: *mut Config,
    event: KeyEvent,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_config_key_is_binding(config, event) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_keyboard_changed(v: *mut App) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_keyboard_changed(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_open_config(v: *mut App) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_open_config(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_update_config(v: *mut App, config: *const Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_update_config(v, config) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_needs_confirm_quit(v: *mut App) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_needs_confirm_quit(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_has_global_keybinds(v: *mut App) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_has_global_keybinds(v) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_app_set_color_scheme(v: *mut App, scheme_raw: c_int) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_app_set_color_scheme(v, scheme_raw) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_string_free(str: String) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_string_free(str) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_config_new() -> SurfaceOptions {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_config_new() }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_new(
    app: *mut App,
    opts: *const SurfaceOptions,
) -> *mut Surface {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_new(app, opts) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_free(ptr: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_free(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_userdata(surface: *mut Surface) -> *mut c_void {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_userdata(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_app(surface: *mut Surface) -> *mut App {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_app(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_inherited_config(
    surface: *mut Surface,
    source: NewSurfaceContext,
) -> SurfaceOptions {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_inherited_config(surface, source) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_update_config(
    surface: *mut Surface,
    config: *const Config,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_update_config(surface, config) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_needs_confirm_quit(surface: *mut Surface) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_needs_confirm_quit(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_process_exited(surface: *mut Surface) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_process_exited(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_has_selection(surface: *mut Surface) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_has_selection(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_read_selection(
    surface: *mut Surface,
    result: *mut Text,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_read_selection(surface, result) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_read_text(
    surface: *mut Surface,
    sel: Selection,
    result: *mut Text,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_read_text(surface, sel, result) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_free_text(_surface: *mut Surface, ptr: *mut Text) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_free_text(_surface, ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_refresh(surface: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_refresh(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_draw(surface: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_draw(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_size(surface: *mut Surface, w: u32, h: u32) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_size(surface, w, h) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_size(surface: *mut Surface) -> SurfaceSize {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_size(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_foreground_pid(surface: *mut Surface) -> u64 {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_foreground_pid(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_tty_name(surface: *mut Surface) -> String {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_tty_name(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_color_scheme(
    surface: *mut Surface,
    scheme_raw: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_color_scheme(surface, scheme_raw) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_content_scale(surface: *mut Surface, x: f64, y: f64) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_content_scale(surface, x, y) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_focus(surface: *mut Surface, focused: bool) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_focus(surface, focused) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_occlusion(surface: *mut Surface, visible: bool) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_occlusion(surface, visible) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_key_translation_mods(
    surface: *mut Surface,
    mods_raw: c_int,
) -> c_int {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_key_translation_mods(surface, mods_raw) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_key(surface: *mut Surface, event: KeyEvent) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_key(surface, event) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_key_is_binding(
    surface: *mut Surface,
    event: KeyEvent,
    c_flags: *mut FlagsC,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_key_is_binding(surface, event, c_flags) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_text(surface: *mut Surface, ptr: *const u8, len: usize) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_text(surface, ptr, len) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_preedit(
    surface: *mut Surface,
    ptr: *const u8,
    len: usize,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_preedit(surface, ptr, len) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_mouse_captured(surface: *mut Surface) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_mouse_captured(surface) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_mouse_button(
    surface: *mut Surface,
    action: mouse::ButtonState,
    button: mouse::Button,
    mods: c_int,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_mouse_button(surface, action, button, mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_mouse_pos(
    surface: *mut Surface,
    x: f64,
    y: f64,
    mods: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_mouse_pos(surface, x, y, mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_mouse_scroll(
    surface: *mut Surface,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_mouse_scroll(surface, x, y, scroll_mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_mouse_pressure(
    surface: *mut Surface,
    stage_raw: u32,
    pressure: f64,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_mouse_pressure(surface, stage_raw, pressure) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_ime_point(
    surface: *mut Surface,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_ime_point(surface, x, y, width, height) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_request_close(ptr: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_request_close(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_split(ptr: *mut Surface, direction: SplitDirection) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_split(ptr, direction) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_split_focus(ptr: *mut Surface, direction: GotoSplit) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_split_focus(ptr, direction) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_split_resize(
    ptr: *mut Surface,
    direction: ResizeSplitDirection,
    amount: u16,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_split_resize(ptr, direction, amount) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_split_equalize(ptr: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_split_equalize(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_binding_action(
    ptr: *mut Surface,
    action_ptr: *const u8,
    action_len: usize,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_binding_action(ptr, action_ptr, action_len) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_complete_clipboard_request(
    ptr: *mut Surface,
    str: *const c_char,
    state: *mut ClipboardRequest,
    confirmed: bool,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_complete_clipboard_request(ptr, str, state, confirmed) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_inspector(ptr: *mut Surface) -> *mut Inspector {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_inspector(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_free(ptr: *mut Surface) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_free(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_set_size(ptr: *mut Inspector, w: u32, h: u32) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_set_size(ptr, w, h) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_set_content_scale(ptr: *mut Inspector, x: f64, y: f64) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_set_content_scale(ptr, x, y) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_mouse_button(
    ptr: *mut Inspector,
    action: mouse::ButtonState,
    button: mouse::Button,
    mods: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_mouse_button(ptr, action, button, mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_mouse_pos(ptr: *mut Inspector, x: f64, y: f64) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_mouse_pos(ptr, x, y) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_mouse_scroll(
    ptr: *mut Inspector,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_mouse_scroll(ptr, x, y, scroll_mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_key(
    ptr: *mut Inspector,
    action: key::Action,
    key: key::Key,
    c_mods: c_int,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_key(ptr, action, key, c_mods) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_text(ptr: *mut Inspector, str: *const c_char) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_text(ptr, str) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_set_focus(ptr: *mut Inspector, focused: bool) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_set_focus(ptr, focused) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_set_window_background_blur(app: *mut App, window: *mut c_void) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_set_window_background_blur(app, window) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_set_display_id(ptr: *mut Surface, display_id: u32) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_set_display_id(ptr, display_id) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_quicklook_font(ptr: *mut Surface) -> *mut c_void {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_quicklook_font(ptr) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_surface_quicklook_word(
    ptr: *mut Surface,
    result: *mut Text,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_surface_quicklook_word(ptr, result) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_metal_init(
    ptr: *mut Inspector,
    device: *mut c_void,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_metal_init(ptr, device) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_metal_render(
    ptr: *mut Inspector,
    command_buffer: *mut c_void,
    descriptor: *mut c_void,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_metal_render(ptr, command_buffer, descriptor) }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn ghostty_inspector_metal_shutdown(ptr: *mut Inspector) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::rustty_inspector_metal_shutdown(ptr) }
}
