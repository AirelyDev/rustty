#include <stdio.h>
#include <string.h>
#include "rustty.h"

static const char *actions[] = {
    "check_for_updates","open_config","reload_config","quit",
    "new_window","new_tab","close_surface","close_tab","close_window","close_all_windows",
    "new_split:right","new_split:left","new_split:down","new_split:up",
    "undo","redo","copy_to_clipboard","paste_from_clipboard","paste_from_selection","select_all",
    "start_search","end_search","search_selection","scroll_to_selection",
    "navigate_search:next","navigate_search:previous",
    "toggle_split_zoom","goto_split:previous","goto_split:next","goto_split:up","goto_split:down",
    "goto_split:left","goto_split:right",
    "resize_split:up,10","resize_split:down,10","resize_split:right,10","resize_split:left,10",
    "equalize_splits","reset_window_size",
    "increase_font_size:1","decrease_font_size:1","reset_font_size",
    "prompt_surface_title","prompt_tab_title",
    "toggle_command_palette","toggle_quick_terminal","toggle_visibility",
    "toggle_window_float_on_top","toggle_fullscreen","toggle_secure_input","inspector:toggle",
    0
};

int main(int argc, char **argv) {
    rustty_init((uintptr_t)argc, argv);
    rustty_config_t cfg = rustty_config_new();
    rustty_config_finalize(cfg);
    for (int i = 0; actions[i]; i++) {
        const char *a = actions[i];
        rustty_input_trigger_s t = rustty_config_trigger(cfg, a, strlen(a));
        const char *tag = t.tag == RUSTTY_TRIGGER_PHYSICAL ? "Physical"
                        : t.tag == RUSTTY_TRIGGER_UNICODE ? "Unicode" : "CatchAll";
        char key[64];
        int empty = 0;
        if (t.tag == RUSTTY_TRIGGER_UNICODE) {
            snprintf(key, sizeof key, "U+%04X '%c'", t.key.unicode,
                     (t.key.unicode >= 32 && t.key.unicode < 127) ? t.key.unicode : '?');
        } else if (t.tag == RUSTTY_TRIGGER_PHYSICAL) {
            snprintf(key, sizeof key, "physical=%d", (int)t.key.physical);
            if ((int)t.key.physical == 0) empty = 1;
        } else {
            snprintf(key, sizeof key, "catch_all");
        }
        printf("%-28s %s tag=%-8s mods=0x%08x %s\n", a, empty ? "EMPTY" : "OK   ", tag, (unsigned)t.mods, key);
    }
    return 0;
}
