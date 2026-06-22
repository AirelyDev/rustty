//! rustty-os::i18n_locales
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/i18n_locales.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).
//!
//! PARITY (prerequisite for the `i18n` group): the `i18n` module ports
//! `src/os/i18n.zig`, which imports this file (`locales`/`staticLocale`/
//! `locales_map`). This data-only companion is filled here so the `i18n` port
//! compiles and is verifiable; it contains no logic and no `test {}` blocks
//! (the reference has none either — Phase-0 inventory: `os/i18n_locales.zig`
//! has 0 tests).

// PARITY: the reference identifier is `locales` (lower-case); preserved per R1.
#![allow(non_upper_case_globals)]

// NOTE: This is in a separate file because our build depends on it and
// we want to minimize the transitive dependencies of the build binary
// itself.

/// Supported locales for the application. This must be kept up to date
/// with the translations available in the `po/` directory; this is used
/// by our build process as well as runtime librustty APIs.
///
/// The order also matters. For incomplete locale information (i.e. only
/// a language code available), the first match is used. For example, if
/// we know the user requested `zh` but has no script code, then we'd pick
/// the first locale that matches `zh`.
///
/// For ordering, we prefer:
///
///   1. The most common locales first, since there are places in the code
///      where we do linear searches for a locale and we want to minimize
///      the number of iterations for the common case.
///
///   2. Alphabetical for otherwise equally common locales.
///
///   3. Most preferred locale for a language without a country code.
///
/// Note for "most common" locales, this is subjective and based on
/// the perceived userbase of Rustty, which may not be representative
/// of general populations or global language distribution. Also note
/// that ordering may be weird when we first merge a new locale since
/// we don't have a good way to determine this. We can always reorder
/// with some data.
pub const locales: &[&str] = &[
    "zh_CN", "de", "fr", "ja", "nl", "nb", "ru", "uk", "pl", "ko_KR", "mk", "tr", "id", "es_BO",
    "es_AR", "es_ES", "pt_BR", "ca", "it", "bg", "ga", "hu", "he", "zh_TW", "hr", "lt", "lv", "vi",
    "kk", "be", "eu",
];
