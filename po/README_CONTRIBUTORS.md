# Localizing Rustty: The Contributors' Guide

Rustty uses the `gettext` library/framework for localization, which has the
distinct benefit of being able to be consumed directly by our two main
app runtimes: macOS and GTK (Linux). The core would ideally remain agnostic
to localization efforts, as not all consumers of librustty would be interested
in localization support. Thus, implementors of app runtimes are left responsible
for any localization that they may add.

> [!NOTE]
> Rustty's GTK app runtime is not yet live. Until it lands, the template
> (`po/io.airely.rustty.pot`) and the catalogs (`po/*.po`) carry the rebranded
> **upstream Rustty UI-string translations**. Once Rustty's own GTK UI exists,
> the strings below are extracted from Rustty's source and the template and
> catalogs are regenerated with `cargo run -p xtask -- update-translations`.

## GTK

In the GTK app runtime, translatable strings are mainly sourced from Blueprint
files (located under `src/apprt/gtk/ui`). Blueprints have a native syntax for
translatable strings, which look like this:

```zig
// Translators: This is the name of the button that opens the about dialog.
title: _("About Rustty");
```

The `// Translators:` comment provides additional context to the translator
if the string itself is unclear as to what its purpose is or where it's located.

By default identical strings are collapsed together into one translatable entry.
To avoid this, assign a _context_ to the string:

```zig
label: C_("menu action", "Copy");
```

Translatable strings can also be sourced from source files. This is useful
when the string must be chosen dynamically at runtime, or when it requires
additional formatting.

```rust
use rustty_os::i18n;

let text = if awesome {
    i18n::_("My awesome label :D")
} else {
    i18n::_("My not-so-awesome label :(")
};

let label = gtk::Label::new(Some(text));
```

If a string must be stored untranslated and only translated later, use
`i18n::N_` instead. This marks the string for extraction into the translation
template but returns the original msgid unchanged. A common use case is
compile-time or static metadata that is translated only when it is presented
to the user.

```rust
use rustty_os::i18n;

struct Command {
    title: &'static str,
}

let cmd = Command {
    title: i18n::N_("Reset Terminal"),
};

let label = gtk::Label::new(Some(i18n::_(cmd.title)));
```

`i18n::_` is a runtime catalog lookup, so it cannot appear in a `const` or
`static` initializer. For strings that are stored untranslated and translated
later, prefer `i18n::N_`.

All translatable strings are extracted into the _translation template file_,
located under `po/io.airely.rustty.pot`. **This file must stay in sync with
the list of translatable strings present in source code or Blueprints at all times.**
A CI action would be run for every PR, which checks if the translation template
requires any updates. You can update the translation template by running
`cargo run -p xtask -- update-translations`, which would also synchronize
translation files for other locales (`.po` files) to reflect the state of the
template file.

During the build process, each locale in `.po` files is compiled
into binary `.mo` files, stored under `share/locale/<LOCALE>/LC_MESSAGES/io.airely.rustty.mo`.
This can be directly accessed by `libintl`, which provide the various `gettext`
C functions that can be called either by Rust code directly, or by the GTK builder
(recommended).

> [!NOTE]
> For the vast majority of users, no additional library needs to be installed
> in order to get localizations, since `libintl` is a part of the GNU C standard
> library. For users using alternative C standard libraries like musl, they must
> use a stub implementation such as [`gettext-tiny`](https://github.com/sabotage-linux/gettext-tiny)
> that offer no-op symbols for the translation functions, or by using a build of
> `libintl` that works for them.

## macOS

> [!NOTE]
> The localization system is not yet implemented for macOS.
