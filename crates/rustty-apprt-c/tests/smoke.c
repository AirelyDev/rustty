/*
 * smoke.c — the libghostty C-ABI compile+link+run known-answer test (chunk E11,
 * EMBEDDED_BLUEPRINT §6.3 [ADV-FIX 2]).
 *
 * This is a REAL C host that consumes the published rustty embedding C ABI:
 *   - it #includes the cbindgen-generated `include/rustty-apprt.h` (the
 *     `rustty_*` exports DEFINED IN rustty-apprt-c + the hand-pinned extern
 *     structs), and
 *   - it links the `rustty-apprt-c` staticlib (which bundles the config C-ABI —
 *     `rustty_config_*` from rustty-config — as a transitive dependency).
 *
 * It exercises the RUNTIME-LIVE subset (EMBEDDED_BLUEPRINT §1: the ~14 non-GPU,
 * non-xev exports): bootstrap + config + app lifecycle. It MUST NOT call
 * `rustty_surface_new` (the Phase-3 GPU/xev leaf boundary) or
 * `rustty_set_window_background_blur` (the macOS objc/CGS leaf, ADV-FIX 3) — both
 * would `unimplemented!()`-panic at runtime.
 *
 * The fn-ptr-ABI backstop (ADV-FIX 7): the runtime-config callbacks below are
 * REAL typed `extern "C"` stubs whose signatures the C compiler type-checks
 * against the cbindgen-emitted `Options` callback typedefs. A drift in any
 * callback signature fails THIS compile — the actual guard, since `size_of` is
 * invariant to fn-ptr signature changes.
 *
 * Driven by `tests/smoke.rs` (shells cc/clang to compile+link+run, asserts
 * exit 0).
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "rustty-apprt.h"

/*
 * The config C-ABI (rustty-config `src/c_api.rs`, config C3) is ALREADY shipped
 * as `rustty_config_*` symbols, and the public embedding umbrella
 * `include/rustty-embed.h` declares them — but rustty-apprt.h cannot: cbindgen
 * runs with `parse_deps = false`, so it never sees a sibling crate (the header's
 * own banner lists the whole sibling-crate tail, W92-B02-02). A dedicated
 * generated config header stays consumer-triggered (DEFERRALS.md, WS19 T19).
 * This KAT deliberately includes ONLY rustty-apprt.h — that is the artifact
 * under test — so it forward-declares just the three config entrypoints it
 * calls. `Config` itself is the opaque handle declared in rustty-apprt.h.
 * [ADV-FIX 2]: the KAT links BOTH apprt-c and this config C-ABI; it does not
 * fabricate a `&cfg`.
 */
Config *rustty_config_new(void);
void rustty_config_finalize(Config *self);
void rustty_config_free(Config *self);

/* ---- REAL typed no-op runtime-config callback stubs (the fn-ptr backstop) ---- */

static void cb_wakeup(void *userdata) {
    (void)userdata;
}

static bool cb_action(App *app, TargetC target, ActionC action) {
    (void)app;
    (void)target;
    (void)action;
    return false;
}

/* 0ce9054bf end state: the read callback reports a tri-state result and
 * receives the requested MIME list + the listing flag. */
static ClipboardReadResult cb_read_clipboard(void *userdata,
                                             int req_type,
                                             ClipboardRequest *state,
                                             const char *const *mimes,
                                             uintptr_t mimes_len,
                                             bool list) {
    (void)userdata;
    (void)req_type;
    (void)state;
    (void)mimes;
    (void)mimes_len;
    (void)list;
    /* The request is never started. */
    return CLIPBOARD_READ_UNSUPPORTED;
}

static void cb_confirm_read_clipboard(void *userdata,
                                      const ClipboardConfirm *confirm,
                                      ClipboardRequest *state,
                                      ClipboardRequestType req_type) {
    (void)userdata;
    (void)confirm;
    (void)state;
    (void)req_type;
}

static void cb_write_clipboard(void *userdata,
                               int req_type,
                               const ClipboardContent *contents,
                               uintptr_t len,
                               bool confirmed) {
    (void)userdata;
    (void)req_type;
    (void)contents;
    (void)len;
    (void)confirmed;
}

static void cb_close_surface(void *userdata, bool process_alive) {
    (void)userdata;
    (void)process_alive;
}

int main(int argc, char **argv) {
    /* Bootstrap rustty global state (the `ghostty_init` precursor). MUST come
     * before rustty_app_new — it sets rustty_core::STATE (global.alloc). */
    if (rustty_init((uintptr_t)argc, argv) != 0) {
        return 10;
    }

    /* Build the Config the way a real host must, via the config C-ABI. */
    Config *cfg = rustty_config_new();
    if (cfg == NULL) {
        return 11;
    }
    rustty_config_finalize(cfg);

    /* Exercise the pure runtime-live surface-config constructor (returns a
     * zeroed/defaulted SurfaceOptions; no surface is created). This is distinct
     * from the runtime `Options` below: `rustty_surface_config_new` builds the
     * per-surface `ghostty_surface_config_s`, NOT the app runtime config. */
    SurfaceOptions surface_cfg = rustty_surface_config_new();
    if (surface_cfg.scale_factor != 1.0) {
        return 12;
    }

    /* The app runtime config (`ghostty_runtime_config_s`): the host callbacks.
     * Passing these REAL typed stubs is the fn-ptr-ABI backstop. */
    Options rt_cfg = {
        .userdata = NULL,
        .supports_selection_clipboard = false,
        .wakeup = cb_wakeup,
        .action = cb_action,
        .read_clipboard = cb_read_clipboard,
        .confirm_read_clipboard = cb_confirm_read_clipboard,
        .write_clipboard = cb_write_clipboard,
        .close_surface = cb_close_surface,
    };

    /* app_new builds CoreApp + keymap + config clone — NO renderer, NO xev
     * (EMBEDDED_BLUEPRINT §1: runtime-live). Since P8-W6 T2 (`40d5b860d`)
     * CoreApp also creates the app-scoped render device (Metal / EGL), so a
     * host with no render device returns NULL here, as upstream does. */
    App *app = rustty_app_new(&rt_cfg, cfg);
    if (app == NULL) {
        rustty_config_free(cfg);
        return 13;
    }

    /* `rustty_config_key_is_binding` — a runtime-live config query over a raw
     * (empty) key event. */
    KeyEvent ev = {
        .action = RUSTTY_INPUT_ACTION_PRESS,
        .mods = 0,
        .consumed_mods = 0,
        .keycode = 0,
        .text = NULL,
        .unshifted_codepoint = 0,
        .composing = false,
    };
    (void)rustty_config_key_is_binding(cfg, ev);

    /* `rustty_app_needs_confirm_quit` — a runtime-live app query. */
    (void)rustty_app_needs_confirm_quit(app);

    /* Tear down in reverse order. */
    rustty_app_free(app);
    rustty_config_free(cfg);

    return 0;
}
