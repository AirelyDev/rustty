//! `rustty_translate` KAT — proves the C-ABI export performs a REAL catalog
//! lookup, not the identity passthrough it shipped as until WS13.
//!
//! Non-vacuity: "returns a string" proves nothing here, because the stub also
//! returned a string — the SAME string. So every assertion below is paired with a
//! control that must differ:
//!
//! * positive : LANG=de_DE.UTF-8, msgid "About Rustty"  -> "Über Rustty"
//! * control A: LANG=C           , msgid "About Rustty"  -> "About Rustty"
//! * control B: LANG=de_DE.UTF-8, msgid "About Ghostty" -> "About Ghostty"
//!   (a msgid no catalog carries — the exact failure mode WS13 T2 fixed in the
//!   `.blp`; if this ever starts translating, a stale Ghostty msgid has been
//!   re-added to the catalogs)
//! * control C: LANG=ja_JP.UTF-8, msgid "About Rustty"  -> the ja msgstr,
//!   which differs from both the German and the English
//!
//! The catalog tree is built here by `msgfmt` from the committed `po/`, so the
//! test also covers the `GhosttyI18n.zig:33-43` compile step end to end.

// Not on Windows (task W-2, R14 form 1): the reference's `-Di18n` default is
// `false` there (`build/Config.zig:263-271`: only macOS/iOS, and Linux/FreeBSD
// with glibc), so `rustty_translate` is the identity and there is no catalog
// to look up.
#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::process::Command;

/// `msgfmt`, preferring the keg-only Homebrew gettext on macOS (the same
/// resolution `xtask`'s `res_misc::msgfmt_path()` performs).
fn msgfmt() -> PathBuf {
    let brew = Path::new("/opt/homebrew/opt/gettext/bin/msgfmt");
    if brew.is_file() {
        return brew.to_path_buf();
    }
    PathBuf::from("msgfmt")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Build `<tmp>/share/locale/<l>/LC_MESSAGES/io.airely.rustty.mo` for each locale
/// and return the RESOURCES dir (`<tmp>/share/rustty`), whose PARENT is the share
/// dir `i18n::init` derives the locale path from (`os/i18n.zig:38-46`).
fn build_locale_tree(locales: &[&str]) -> PathBuf {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("ws13-i18n-kat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let share = tmp.join("share");
    for l in locales {
        let dir = share.join("locale").join(l).join("LC_MESSAGES");
        std::fs::create_dir_all(&dir).expect("create locale dir");
        let po = root.join("po").join(format!("{l}.po"));
        let mo = dir.join("io.airely.rustty.mo");
        let out = Command::new(msgfmt())
            .arg("-o")
            .arg(&mo)
            .arg(&po)
            .output()
            .expect("spawn msgfmt (macOS: install the Homebrew gettext keg)");
        assert!(
            out.status.success(),
            "msgfmt {}: {}",
            po.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            std::fs::metadata(&mo).map(|m| m.len()).unwrap_or(0) > 0,
            "msgfmt produced an empty catalog for {l}"
        );
    }
    let resources = share.join("rustty");
    std::fs::create_dir_all(&resources).expect("create resources dir");
    resources
}

/// Run the probe child with `lang` in its ENVIRONMENT (see the example's module
/// docs for why in-process `setlocale` is not enough).
fn probe(resources_dir: &Path, lang: &str, msgid: &str) -> String {
    let exe = env!("CARGO_BIN_EXE_translate_probe");
    let out = Command::new(exe)
        .arg(resources_dir)
        .arg(msgid)
        .env_remove("LC_MESSAGES")
        .env_remove("LANGUAGE")
        .env("LANG", lang)
        .env("LC_ALL", lang)
        .output()
        .expect("spawn translate_probe");
    assert!(
        out.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// glibc names the locale `de_DE.utf8`, macOS/libintl `de_DE.UTF-8`. Try both
/// rather than relaxing the assertion, and report which spelling resolved.
fn probe_locale(resources_dir: &Path, lang_variants: &[&str], msgid: &str, expect: &str) -> String {
    let mut last = String::new();
    for lang in lang_variants {
        last = probe(resources_dir, lang, msgid);
        if last == expect {
            eprintln!("locale spelling that resolved: {lang}");
            return last;
        }
    }
    last
}

#[test]
fn rustty_translate_performs_a_real_catalog_lookup() {
    let res = build_locale_tree(&["de", "ja"]);

    let de = probe_locale(
        &res,
        &["de_DE.UTF-8", "de_DE.utf8"],
        "About Rustty",
        "Über Rustty",
    );
    let c = probe(&res, "C", "About Rustty");
    let stale = probe(&res, "de_DE.UTF-8", "About Ghostty");
    let ja = probe_locale(
        &res,
        &["ja_JP.UTF-8", "ja_JP.utf8"],
        "About Rustty",
        "Rustty について",
    );

    assert_eq!(de, "Über Rustty", "po/de.po msgstr for 'About Rustty'");
    assert_eq!(c, "About Rustty", "LANG=C must fall through to the msgid");
    assert_ne!(
        de, c,
        "if these match, rustty_translate is still the identity stub"
    );
    assert_eq!(
        stale, "About Ghostty",
        "a msgid no catalog carries must pass through"
    );
    assert_ne!(ja, de, "a different locale must yield a different string");
    assert_ne!(ja, c, "the ja catalog must actually be consulted");
}
