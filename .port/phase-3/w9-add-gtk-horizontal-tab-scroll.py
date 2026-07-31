#!/usr/bin/env python3
"""P3-W9 T3 — register `gtk-horizontal-tab-scroll` at every rustty-config site.

Oracle: 5ea45919d (the option) + b5f5ca078 (its "Available since 1.4.0." line).
The struct field + doc block is inserted by hand; this script does the 12
mechanical registration sites so none is missed.
"""

P = "crates/rustty-config/src/config_struct.rs"
src = open(P).read()

# (anchor, replacement) — each anchor must occur EXACTLY once.
EDITS = [
    # Key enum (declaration order)
    ("    GtkWideTabs,\n    GtkCustomCss,\n",
     "    GtkWideTabs,\n    GtkHorizontalTabScroll,\n    GtkCustomCss,\n"),
    # key parse (ALPHABETICAL)
    ('            b"gtk-custom-css" => Some(Key::GtkCustomCss),\n',
     '            b"gtk-custom-css" => Some(Key::GtkCustomCss),\n'
     '            b"gtk-horizontal-tab-scroll" => Some(Key::GtkHorizontalTabScroll),\n'),
    # TypeId table (declaration order)
    ("        Key::GtkWideTabs => TypeId::of::<bool>(),\n",
     "        Key::GtkWideTabs => TypeId::of::<bool>(),\n"
     "        Key::GtkHorizontalTabScroll => TypeId::of::<bool>(),\n"),
    # default()
    ("            gtk_wide_tabs: true,\n",
     "            gtk_wide_tabs: true,\n            gtk_horizontal_tab_scroll: true,\n"),
    # c!() dispatch
    ("            Key::GtkWideTabs => c!(gtk_wide_tabs),\n",
     "            Key::GtkWideTabs => c!(gtk_wide_tabs),\n"
     "            Key::GtkHorizontalTabScroll => c!(gtk_horizontal_tab_scroll),\n"),
    # clone
    ("        cv!(gtk_wide_tabs);\n",
     "        cv!(gtk_wide_tabs);\n        cv!(gtk_horizontal_tab_scroll);\n"),
    # CONFIG_FIELDS (declaration order)
    ('    (b"gtk-wide-tabs", Key::GtkWideTabs),\n',
     '    (b"gtk-wide-tabs", Key::GtkWideTabs),\n'
     '    (\n        b"gtk-horizontal-tab-scroll",\n        Key::GtkHorizontalTabScroll,\n    ),\n'),
    # formatEntry (declaration order)
    ('            // @"gtk-wide-tabs": bool\n'
     '            b"gtk-wide-tabs" => f_display!(gtk_wide_tabs),\n',
     '            // @"gtk-wide-tabs": bool\n'
     '            b"gtk-wide-tabs" => f_display!(gtk_wide_tabs),\n'
     '            // @"gtk-horizontal-tab-scroll": bool\n'
     '            b"gtk-horizontal-tab-scroll" => f_display!(gtk_horizontal_tab_scroll),\n'),
    # parse (declaration order)
    ('            // @"gtk-wide-tabs": bool\n'
     '            b"gtk-wide-tabs" => field_bool!(gtk_wide_tabs),\n',
     '            // @"gtk-wide-tabs": bool\n'
     '            b"gtk-wide-tabs" => field_bool!(gtk_wide_tabs),\n'
     '            // @"gtk-horizontal-tab-scroll": bool\n'
     '            b"gtk-horizontal-tab-scroll" => field_bool!(gtk_horizontal_tab_scroll),\n'),
    # CONFIG_FIELD_NAMES (ALPHABETICAL)
    ('        b"gtk-custom-css",\n        b"gtk-opengl-debug",\n',
     '        b"gtk-custom-css",\n        b"gtk-horizontal-tab-scroll",\n'
     '        b"gtk-opengl-debug",\n'),
    # ALL_KEYS (declaration order)
    ("        Key::GtkWideTabs,\n        Key::GtkCustomCss,\n",
     "        Key::GtkWideTabs,\n        Key::GtkHorizontalTabScroll,\n        Key::GtkCustomCss,\n"),
]

for anchor, repl in EDITS:
    n = src.count(anchor)
    assert n == 1, f"anchor occurs {n}x, expected 1: {anchor!r}"
    src = src.replace(anchor, repl)

open(P, "w").write(src)

# c_get.rs (idempotent: skip if already registered)
C = "crates/rustty-config/src/c_get.rs"
csrc = open(C).read()
if "GtkHorizontalTabScroll" not in csrc:
    a = "        Key::GtkWideTabs => g_bool!(gtk_wide_tabs),\n"
    assert csrc.count(a) == 1
    csrc = csrc.replace(
        a, a + "        Key::GtkHorizontalTabScroll => g_bool!(gtk_horizontal_tab_scroll),\n"
    )
    open(C, "w").write(csrc)
print("ok")
