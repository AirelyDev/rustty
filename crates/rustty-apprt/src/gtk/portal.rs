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
//! `:687` — the note here said `:625`; verified against the pin by task-18 — a
//! product-prefixed token). They are not shared upstream and are not
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

/// PORTS: `portal.zig:13-17` `generateToken`. A random token suitable for use in
/// requests to the XDG Desktop Portal.
///
/// CORRECTION (task-18, closes S48-F4). The note that stood here said "the
/// reference reaches `std.crypto.random`. The port has no CSPRNG dependency in
/// this crate and does not need one", and drew from `glib::random_int`. Both
/// halves were wrong, and the second one was wrong in the direction that matters.
///
/// * **The entropy claim was stale.** At the pin the reference does not name
///   `std.crypto.random` at all; `e8525c0fd` ("Update to Zig 0.16.0") rewrote it
///   to `std.Random.IoSource{ .io = global.io() }`. That is not a downgrade:
///   `std.Io.random` is documented as "obtains entropy from a cryptographically
///   secure pseudo-random number generator", seeded by `randomSecure` (getrandom
///   / arc4random / CNG) with an ASLR+time fallback ONLY when the OS entropy
///   source is unavailable. So the reference used a CSPRNG before the rename and
///   uses a CSPRNG after it — the source CLASS never changed.
/// * **"No CSPRNG dependency in this crate" was false.** This crate's OTHER
///   portal token generator — [`crate::gtk::class::global_shortcuts`]'s
///   `generate_token`, the twin the module note below already points at — has
///   called `rustty_os::file::crypto_random_bytes` all along. There was never a
///   dependency to avoid, so the substitution bought nothing and cost the source
///   class. It now draws from the same shim, which is what `std.Io.random`
///   resolves to on every platform rustty targets.
/// * The `glib::random_int` loop also carried a latent 32-bit bug: on a target
///   where `usize` is 32 bits, `token << 32` is a shift overflow (a debug-build
///   panic, a wrong value in release). The byte fill below has no width
///   assumption.
///
/// Why the source class is worth keeping even though the token is not a secret:
/// the portal spec requires the client to compute the `Request` object path and
/// subscribe to it BEFORE issuing the call, so the token's job is to be unique.
/// A predictable token is not obviously exploitable — the path is namespaced by
/// the bus-assigned unique name, which a peer cannot forge. But "not obviously
/// exploitable" is the argument, not the requirement, and it is an argument about
/// the portal protocol that would have to be re-made every time the token's use
/// changed. Matching the reference's source costs one function call, so the port
/// does not spend a security argument to save it.
pub fn generate_token() -> usize {
    // const rng_impl: std.Random.IoSource = .{ .io = global.io() };
    // return rng_impl.interface().int(usize);
    let mut buf = [0u8; size_of::<usize>()];
    rustty_os::file::crypto_random_bytes(&mut buf);
    usize::from_ne_bytes(buf)
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
    // return std.fmt.parseUnsigned(usize, token, 16) catch return null;
    parse_unsigned_hex(token.as_bytes())
}

/// `std.fmt.parseUnsigned(usize, buf, 16)` (Zig 0.16, the reference's toolchain),
/// with every error mapped to `None`.
///
/// CORRECTION (task T-D, S48-F3): this was `usize::from_str_radix(token, 16)`,
/// which disagrees with Zig on two characters: Rust accepts a leading `'+'`, which
/// Zig rejects (`parseUnsigned` has no sign handling), and Rust rejects `'_'`
/// digit separators, which Zig skips anywhere except the first and last byte.
/// The rules below were MEASURED with Zig 0.16.0 (`std.fmt.parseUnsigned(usize,
/// in, 16)` over the inputs of `portal_parse_request_path_token_matches_zig`),
/// not inferred.
fn parse_unsigned_hex(buf: &[u8]) -> Option<usize> {
    // `if (buf.len == 0) return error.InvalidCharacter;`
    let (&first, &last) = (buf.first()?, buf.last()?);
    // `if (buf[0] == '_' or buf[buf.len - 1] == '_') return error.InvalidCharacter;`
    if first == b'_' || last == b'_' {
        return None;
    }
    let mut x: usize = 0;
    for &c in buf {
        // `if (c == '_') continue;`
        if c == b'_' {
            continue;
        }
        // `charToDigit(c, 16)` → `error.InvalidCharacter`.
        let digit = char::from(c).to_digit(16)?;
        // `math.mul` / `math.add` → `error.Overflow`.
        x = x.checked_mul(16)?.checked_add(digit as usize)?;
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S48-F4 regression pin. Two properties the `glib::random_int` loop this
    /// replaced could not hold: the draw must span the FULL `usize` width (that
    /// loop built the value with `token << 32`, a shift overflow on a 32-bit
    /// target — debug panic, wrong value in release), and consecutive tokens must
    /// differ (a token that repeats collides with this process's own in-flight
    /// `Request` object path, which is the one thing the token exists to prevent).
    ///
    /// Not a test that the source is a CSPRNG — no black-box test can be. That
    /// property is held by the CALL, `rustty_os::file::crypto_random_bytes`, whose
    /// own platform arms are tested in rustty-os; this pins that the call's full
    /// output reaches the token.
    #[test]
    fn portal_generate_token_spans_the_full_usize_width() {
        let half = usize::BITS / 2;
        let mut high = false;
        let mut low = false;
        let mut distinct = false;
        let first = generate_token();
        for _ in 0..64 {
            let t = generate_token();
            distinct |= t != first;
            high |= (t >> half) != 0;
            low |= (t & (usize::MAX >> half)) != 0;
        }
        assert!(high, "no token set a bit in the high half of `usize`");
        assert!(low, "no token set a bit in the low half of `usize`");
        assert!(distinct, "64 consecutive tokens were all identical");
    }

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

    /// S48-F3: `parseRequestPathToken`'s `std.fmt.parseUnsigned(usize, token, 16)`
    /// over edge inputs, each expected value MEASURED with Zig 0.16.0 (the
    /// reference's `minimum_zig_version`) on the same strings: `_` separators are
    /// skipped except at either end, a sign is never accepted, `0x` is not a
    /// prefix at an explicit base, and overflow is an error. (64-bit `usize`, the
    /// only width the GTK apprt builds for.)
    #[test]
    fn portal_parse_request_path_token_matches_zig() {
        let cases: &[(&str, Option<usize>)] = &[
            ("", None),
            ("+", None),
            ("-", None),
            ("+1", None),
            ("-1", None),
            ("_", None),
            ("_1", None),
            ("1_", None),
            ("1_0", Some(0x10)),
            ("1__0", Some(0x10)),
            ("a_b_c", Some(0xabc)),
            ("0x10", None),
            ("10", Some(0x10)),
            ("ff", Some(0xff)),
            ("FF", Some(0xff)),
            ("fG", None),
            (" 1", None),
            ("1 ", None),
            ("ffffffffffffffff", Some(usize::MAX)),
            ("10000000000000000", None),
            ("0000000000000000000042", Some(0x42)),
            ("1_ffffffffffffffff", None),
            ("+_1", None),
            ("\u{e9}", None),
        ];
        for &(token, want) in cases {
            let path = format!("/org/freedesktop/portal/desktop/request/1_42/{token}");
            assert_eq!(parse_request_path_token(&path), want, "token {token:?}");
        }
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
