//! PORTS: ghostty `src/apprt/gtk/portal.zig` — the shared XDG Desktop Portal
//! helpers: request-token generation, the fixed-width token formatting used in
//! portal object paths and payloads, construction of the `Request` object path
//! for the current process, and parsing a token back out of such a path.
//!
//! See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html>
//! for the protocol of the `Request` interface.
//!
//! PORT NOTE (why this module is separate from
//! [`crate::gtk::class::global_shortcuts`]'s own token helpers): the reference has
//! TWO independent token generators — `portal.zig`'s (`usize`, formatted as a
//! fixed-width hex string) and `class/global_shortcuts.zig`'s (`generateToken`,
//! `:625`, a product-prefixed token). They are not shared upstream and are not
//! shared here; each site keeps the token shape its own reference site emits, so
//! the on-the-wire `handle_token` values match the oracle per call site.

use std::mem::size_of;

pub mod open_uri;

pub use open_uri::OpenUri;

/// PORTS: `portal.zig:8` `pub const token_hex_len = @sizeOf(usize) * 2`. The
/// number of hex digits a formatted token always occupies — the token is
/// zero-padded to this width so a path built from it has a stable shape.
pub const TOKEN_HEX_LEN: usize = size_of::<usize>() * 2;

/// PORTS: `portal.zig:46` — the portal `Request` object-path prefix.
const REQUEST_PATH_PREFIX: &str = "/org/freedesktop/portal/desktop/request";

/// PORTS: `portal.zig:13-15` `generateToken`. A random token suitable for use in
/// requests to the XDG Desktop Portal.
///
/// PORT NOTE: the reference reaches `std.crypto.random`. The port has no CSPRNG
/// dependency in this crate and does not need one: the token is not a secret, it
/// is a per-request DISAMBIGUATOR that must merely be unpredictable enough not to
/// collide with a concurrent request from this or another process. `glib`'s
/// `random_int` (the GLib Mersenne-Twister PRNG, seeded from `/dev/urandom` at
/// first use) is the same class of source the rest of the GTK tier already relies
/// on. Two 32-bit draws fill a `usize` on 64-bit targets.
pub fn generate_token() -> usize {
    let mut token: usize = 0;
    let mut filled = 0;
    while filled < size_of::<usize>() {
        token = (token << 32) | (glib::random_int() as usize);
        filled += 4;
    }
    token
}

/// PORTS: `portal.zig:18-20` `formatToken`. Format a request token consistently
/// for use in portal object paths and payloads: lowercase hex, zero-padded to
/// [`TOKEN_HEX_LEN`].
pub fn format_token(token: usize) -> String {
    format!("{token:0width$x}", width = TOKEN_HEX_LEN)
}

/// PORTS: `portal.zig:26-35` `getRequestPath`. The XDG portal request path for the
/// current process, derived from the D-Bus connection's unique name.
///
/// Returns `None` when the connection has no unique name — the reference's
/// `error.NoDBusUniqueName`. That _should_ be impossible on an ordinary
/// message-bus connection, but is handled rather than asserted (`portal.zig:29-31`).
pub fn request_path(dbus: &gio::DBusConnection, token: usize) -> Option<String> {
    use gio::prelude::DBusConnectionExt as _;

    // Get the unique name from D-Bus and strip the leading `:` (`portal.zig:27-32`).
    let unique_name = dbus.unique_name()?;
    let unique_name = unique_name.as_str().strip_prefix(':')?;
    Some(build_request_path(unique_name, token))
}

/// PORTS: `portal.zig:38-58` `buildRequestPath`.
///
/// The unique name is sanitized by replacing every `.` with `_`, turning a unique
/// name like `1.192` into `1_192`. This sounds arbitrary, but it is part of the
/// `Request` protocol (`portal.zig:52-55`).
pub fn build_request_path(unique_name: &str, token: usize) -> String {
    let joined = format!(
        "{REQUEST_PATH_PREFIX}/{unique_name}/{token}",
        token = format_token(token),
    );
    // `std.mem.replaceScalar(u8, object_path, '.', '_')` (`portal.zig:55`). The
    // reference replaces over the WHOLE joined path; the prefix contains no `.`,
    // so this is equivalent and is spelled the same way.
    joined.replace('.', "_")
}

/// PORTS: `portal.zig:61-65` `parseRequestPathToken`. Try and parse the token out
/// of a request path; `None` when the trailing component is not hex.
pub fn parse_request_path_token(request_path: &str) -> Option<usize> {
    let (_, token) = request_path.rsplit_once('/')?;
    usize::from_str_radix(token, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `portal.zig:67-75` `test "formatToken pads to fixed width"`.
    #[test]
    fn portal_format_token_pads_to_fixed_width() {
        let token = format_token(0x42);
        assert_eq!(token.len(), TOKEN_HEX_LEN);
        assert_eq!(token, "0000000000000042");
    }

    /// PORTS: `portal.zig:77-86` `test "buildRequestPath"`.
    #[test]
    fn portal_build_request_path() {
        assert_eq!(
            build_request_path("1.42", 0x75af01a79c6fea34),
            "/org/freedesktop/portal/desktop/request/1_42/75af01a79c6fea34",
        );
    }

    /// PORTS: `portal.zig:88-97` `test "buildRequestPath pads token"`.
    #[test]
    fn portal_build_request_path_pads_token() {
        assert_eq!(
            build_request_path("1.42", 0x42),
            "/org/freedesktop/portal/desktop/request/1_42/0000000000000042",
        );
    }

    /// PORTS: `portal.zig:99-105` `test "parseRequestPathToken"`.
    #[test]
    fn portal_parse_request_path_token() {
        assert_eq!(
            parse_request_path_token(
                "/org/freedesktop/portal/desktop/request/1_42/75af01a79c6fea34"
            ),
            Some(0x75af01a79c6fea34),
        );
        // A non-hex digit (`G`) in the token component.
        assert_eq!(
            parse_request_path_token(
                "/org/freedesktop/portal/desktop/request/1_42/75af01a79c6fGa34"
            ),
            None,
        );
        // No `/` at all.
        assert_eq!(parse_request_path_token("75af01a79c6fea34"), None);
    }

    /// The round trip the protocol actually depends on: a path we BUILD must parse
    /// back to the token we put in. Falsifiable — a padding, radix or separator
    /// change on either side breaks it, and a constant-returning
    /// `generate_token` would fail the distinctness arm below.
    #[test]
    fn portal_token_path_round_trip() {
        for _ in 0..64 {
            let token = generate_token();
            let path = build_request_path("1.192", token);
            assert_eq!(
                parse_request_path_token(&path),
                Some(token),
                "path={path} token={token:x}"
            );
        }
    }

    /// A token generator that returned a constant would still satisfy the round
    /// trip above, so assert non-constancy separately.
    #[test]
    fn portal_generate_token_is_not_constant() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            seen.insert(generate_token());
        }
        assert!(
            seen.len() > 1,
            "generate_token returned a constant across 64 draws"
        );
        // The high half must be populated too: a `usize` token filled from a
        // single 32-bit draw would leave the top 32 bits always zero, which
        // silently narrows the collision space AND changes the formatted path
        // shape's entropy. Over 64 draws at least one must exceed 32 bits.
        assert!(
            seen.iter().any(|t| *t > u32::MAX as usize),
            "every token fit in 32 bits: the high half of the usize is never filled"
        );
    }

    /// A request path must be a syntactically valid D-Bus object path, or
    /// `signal_subscribe` silently never fires (the same anti-vacuity check
    /// `global_shortcuts.rs` carries for its own path builder).
    #[test]
    fn portal_request_path_is_a_valid_object_path() {
        let path = build_request_path("1.192", generate_token());
        assert!(
            glib::Variant::is_object_path(&path),
            "not a valid object path: {path}"
        );
        // And the sanitization is load-bearing: an UNSANITIZED name with a `.`
        // would NOT be a valid object path. This proves the replace is required,
        // so a future "simplification" that drops it fails here.
        assert!(
            !glib::Variant::is_object_path(&format!(
                "/org/freedesktop/portal/desktop/request/1.192/{}",
                format_token(0x42)
            )),
            "a dotted bus name was accepted as an object path; the `.`->`_` \
             sanitization would then be untested"
        );
    }
}
