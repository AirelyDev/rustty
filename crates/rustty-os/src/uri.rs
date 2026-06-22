//! rustty-os::uri
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/uri.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).
//!
//! PORT (R3, no ghostty/Zig-std dependency): the reference wraps `std.Uri`.
//! Rust's `url` crate has DIFFERENT (WHATWG) semantics, so it is NOT used. We
//! faithfully reimplement the subset of Zig 0.15.2 `std.Uri` that `os/uri.zig`
//! relies on — `parse`, `parseAfterScheme`, the `Component` union (`raw` /
//! `percent_encoded`, `isEmpty`, `toRaw`, `percentEncode`), the `ParseError`
//! set, and the scheme/authority/path scanning algorithm — directly from the
//! documented std.Uri RFC-3986 behavior. The expected slices in the three ported
//! tests come from that exact std.Uri behavior (cross-checked against the
//! reference's pinned std.Uri implementation via an independent Zig probe), not
//! re-derived from this reimplementation.

#![allow(non_snake_case)]

use crate::string_encoding::Writer;

// ===========================================================================
// std.Uri reimplementation (the portion os/uri.zig depends on).
// Mirrors Zig 0.15.2 lib/std/Uri.zig:
//   "Uniform Resource Identifier (URI) parsing roughly adhering to
//    <https://tools.ietf.org/html/rfc3986>.
//    Does not do perfect grammar and character class checking, but should be
//    robust against URIs in the wild."
// ===========================================================================

/// PORT: `std.Uri.Component` — a tagged union of `raw` (invalid characters must
/// be percent encoded before printing) and `percent_encoded` (already
/// percent-encoded, can be printed directly). Slices point into the original
/// parsed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component<'a> {
    /// Invalid characters in this component must be percent encoded
    /// before being printed as part of a URI.
    Raw(&'a [u8]),
    /// This component is already percent-encoded, it can be printed
    /// directly as part of a URI.
    PercentEncoded(&'a [u8]),
}

impl<'a> Component<'a> {
    /// pub const empty: Component = .{ .percent_encoded = "" };
    pub const EMPTY: Component<'static> = Component::PercentEncoded(b"");

    pub fn isEmpty(&self) -> bool {
        match self {
            Component::Raw(string) | Component::PercentEncoded(string) => string.is_empty(),
        }
    }

    /// PORT: `percent_encoded` accessor — `os/uri.zig` reads `.percent_encoded`
    /// directly on components it knows are percent-encoded. Mirrors a union field
    /// access; for a `Raw` component this is the raw bytes (the reference would
    /// not access `.percent_encoded` on a `raw`-tagged value).
    pub fn percent_encoded(&self) -> &'a [u8] {
        match self {
            Component::Raw(s) | Component::PercentEncoded(s) => s,
        }
    }

    /// Returned value may point into `buffer` or be the original string.
    pub fn toRaw<'b>(&self, buffer: &'b mut [u8]) -> Result<&'b [u8], NoSpaceLeft>
    where
        'a: 'b,
    {
        match self {
            Component::Raw(raw) => Ok(raw),
            Component::PercentEncoded(percent_encoded) => {
                if percent_encoded.contains(&b'%') {
                    let mut w = FixedBuf::new(buffer);
                    self.formatRaw(&mut w).map_err(|_| NoSpaceLeft)?;
                    let n = w.len;
                    Ok(&buffer[0..n])
                } else {
                    Ok(percent_encoded)
                }
            }
        }
    }

    /// PORT: `std.Uri.Component.formatRaw` — writes the component with each valid
    /// `%XX` escape decoded back to its byte; invalid escapes are passed through.
    pub fn formatRaw<W: Writer>(&self, w: &mut W) -> Result<(), W::Error> {
        match self {
            Component::Raw(raw) => w.writeAll(raw),
            Component::PercentEncoded(percent_encoded) => {
                let mut start: usize = 0;
                let mut index: usize = 0;
                while let Some(rel) = find_scalar_pos(percent_encoded, index, b'%') {
                    let percent = rel;
                    index = percent + 1;
                    if percent_encoded.len() - index < 2 {
                        continue;
                    }
                    let percent_encoded_char =
                        match parse_int_u8_hex(&percent_encoded[index..index + 2]) {
                            Some(v) => v,
                            None => continue,
                        };
                    w.writeAll(&percent_encoded[start..percent])?;
                    w.writeByte(percent_encoded_char)?;
                    start = percent + 3;
                    index = percent + 3;
                }
                w.writeAll(&percent_encoded[start..])
            }
        }
    }

    /// PORT: `std.Uri.Component.percentEncode` — writes `raw`, percent-encoding
    /// every byte for which `isValidChar` returns false as `%XX` (uppercase hex).
    pub fn percentEncode<W: Writer>(
        w: &mut W,
        raw: &[u8],
        isValidChar: fn(u8) -> bool,
    ) -> Result<(), W::Error> {
        let mut start: usize = 0;
        for (index, &char) in raw.iter().enumerate() {
            if isValidChar(char) {
                continue;
            }
            w.writeAll(&raw[start..index])?;
            w.writeAll(format!("%{:02X}", char).as_bytes())?;
            start = index + 1;
        }
        w.writeAll(&raw[start..])
    }
}

/// PORT: `error{NoSpaceLeft}` — `Component.toRaw`'s error.
#[derive(Debug, PartialEq, Eq)]
pub struct NoSpaceLeft;

/// PORT: `std.Uri` — only the fields `os/uri.zig` touches are exercised, but all
/// are modeled to mirror the struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uri<'a> {
    pub scheme: &'a [u8],
    pub user: Option<Component<'a>>,
    pub password: Option<Component<'a>>,
    pub host: Option<Component<'a>>,
    pub port: Option<u16>,
    pub path: Component<'a>,
    pub query: Option<Component<'a>>,
    pub fragment: Option<Component<'a>>,
}

/// pub const ParseError = error{ UnexpectedCharacter, InvalidFormat, InvalidPort };
#[derive(Debug, PartialEq, Eq)]
pub enum StdParseError {
    UnexpectedCharacter,
    InvalidFormat,
    InvalidPort,
}

/// Parses the URI or returns an error. This function is not compliant, but is required to parse
/// some forms of URIs in the wild, such as HTTP Location headers.
/// The return value will contain strings pointing into the original `text`.
/// Each component that is provided, will be non-`null`.
pub fn parseAfterScheme<'a>(scheme: &'a [u8], text: &'a [u8]) -> Result<Uri<'a>, StdParseError> {
    let mut uri = Uri {
        scheme,
        user: None,
        password: None,
        host: None,
        port: None,
        path: Component::PercentEncoded(b""), // path: undefined (set below)
        query: None,
        fragment: None,
    };
    let mut i: usize = 0;

    'a: {
        if text.starts_with(b"//") {
            i = index_of_any_pos(text, 2, &AUTHORITY_SEP).unwrap_or(text.len());
            let authority = &text[2..i];
            if authority.is_empty() {
                if !text[2..].starts_with(b"/") {
                    return Err(StdParseError::InvalidFormat);
                }
                break 'a;
            }

            let mut start_of_host: usize = 0;
            if let Some(index) = index_of(authority, b"@") {
                start_of_host = index + 1;
                let user_info = &authority[0..index];

                if let Some(idx) = index_of(user_info, b":") {
                    uri.user = Some(Component::PercentEncoded(&user_info[0..idx]));
                    if idx < user_info.len() - 1 {
                        // empty password is also "no password"
                        uri.password = Some(Component::PercentEncoded(&user_info[idx + 1..]));
                    }
                } else {
                    uri.user = Some(Component::PercentEncoded(user_info));
                    uri.password = None;
                }
            }

            // only possible if uri consists of only `userinfo@`
            if start_of_host >= authority.len() {
                break 'a;
            }

            let mut end_of_host: usize = authority.len();

            // if  we see `]` first without `@`
            if authority[start_of_host] == b']' {
                return Err(StdParseError::InvalidFormat);
            }

            if authority.len() > start_of_host && authority[start_of_host] == b'[' {
                // IPv6
                end_of_host = match last_index_of(authority, b"]") {
                    Some(v) => v,
                    None => return Err(StdParseError::InvalidFormat),
                };
                end_of_host += 1;

                if let Some(index) = last_index_of(authority, b":") {
                    if index >= end_of_host {
                        // if not part of the V6 address field
                        end_of_host = end_of_host.min(index);
                        uri.port = Some(
                            parse_int_u16(&authority[index + 1..])
                                .ok_or(StdParseError::InvalidPort)?,
                        );
                    }
                }
            } else if let Some(index) = last_index_of(authority, b":") {
                if index >= start_of_host {
                    // if not part of the userinfo field
                    end_of_host = end_of_host.min(index);
                    uri.port = Some(
                        parse_int_u16(&authority[index + 1..]).ok_or(StdParseError::InvalidPort)?,
                    );
                }
            }

            if start_of_host >= end_of_host {
                return Err(StdParseError::InvalidFormat);
            }
            uri.host = Some(Component::PercentEncoded(
                &authority[start_of_host..end_of_host],
            ));
        }
    }

    let path_start = i;
    i = index_of_any_pos(text, path_start, &PATH_SEP).unwrap_or(text.len());
    uri.path = Component::PercentEncoded(&text[path_start..i]);

    if text[i..].starts_with(b"?") {
        let query_start = i + 1;
        i = find_scalar_pos(text, query_start, b'#').unwrap_or(text.len());
        uri.query = Some(Component::PercentEncoded(&text[query_start..i]));
    }

    if text[i..].starts_with(b"#") {
        uri.fragment = Some(Component::PercentEncoded(&text[i + 1..]));
    }

    Ok(uri)
}

/// The return value will contain strings pointing into the original `text`.
/// Each component that is provided will be non-`null`.
pub fn std_parse(text: &[u8]) -> Result<Uri<'_>, StdParseError> {
    let mut end = text.len();
    for (i, &byte) in text.iter().enumerate() {
        if !isSchemeChar(byte) {
            end = i;
            break;
        }
    }
    // After the scheme, a ':' must appear.
    if end >= text.len() {
        return Err(StdParseError::InvalidFormat);
    }
    if text[end] != b':' {
        return Err(StdParseError::UnexpectedCharacter);
    }
    parseAfterScheme(&text[0..end], &text[end + 1..])
}

/// scheme      = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
fn isSchemeChar(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.')
}

const AUTHORITY_SEP: [u8; 3] = [b'/', b'?', b'#'];
const PATH_SEP: [u8; 2] = [b'?', b'#'];

// PORT helpers reproducing the `std.mem`/`std.fmt` primitives std.Uri uses.
fn index_of_any_pos(haystack: &[u8], start: usize, needles: &[u8]) -> Option<usize> {
    // std.mem.indexOfAnyPos(u8, haystack, start, needles)
    (start..haystack.len()).find(|&i| needles.contains(&haystack[i]))
}

fn find_scalar_pos(haystack: &[u8], start: usize, needle: u8) -> Option<usize> {
    // std.mem.indexOfScalarPos(u8, haystack, start, needle)
    (start..haystack.len()).find(|&i| haystack[i] == needle)
}

fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    // std.mem.indexOf(u8, haystack, needle)
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn last_index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    // std.mem.lastIndexOf(u8, haystack, needle)
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn parse_int_u16(s: &[u8]) -> Option<u16> {
    // std.fmt.parseInt(u16, s, 10)
    if s.is_empty() {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse::<u16>().ok()
}

fn parse_int_u8_hex(s: &[u8]) -> Option<u8> {
    // std.fmt.parseInt(u8, s, 16)
    if s.is_empty() {
        return None;
    }
    let txt = std::str::from_utf8(s).ok()?;
    u8::from_str_radix(txt, 16).ok()
}

/// PORT: a fixed-buffer writer used to back `Component.toRaw`'s `bufPrint`.
struct FixedBuf<'b> {
    buf: &'b mut [u8],
    len: usize,
}
impl<'b> FixedBuf<'b> {
    fn new(buf: &'b mut [u8]) -> FixedBuf<'b> {
        FixedBuf { buf, len: 0 }
    }
}
impl Writer for FixedBuf<'_> {
    type Error = NoSpaceLeft;
    fn writeByte(&mut self, byte: u8) -> Result<(), NoSpaceLeft> {
        if self.len >= self.buf.len() {
            return Err(NoSpaceLeft);
        }
        self.buf[self.len] = byte;
        self.len += 1;
        Ok(())
    }
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), NoSpaceLeft> {
        if self.len + bytes.len() > self.buf.len() {
            return Err(NoSpaceLeft);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

// ===========================================================================
// os/uri.zig
// ===========================================================================

// PORT: `ParseOptions` uses field defaults (`= false`). Rust has no struct field
// defaults; `#[derive(Default)]` reproduces the reference defaults (both false),
// and callers/tests construct it explicitly via `ParseOptions { .. }`.
#[derive(Default)]
pub struct ParseOptions {
    /// Parse MAC addresses in the host component.
    ///
    /// This is useful when the "Private Wi-Fi address" is enabled on macOS,
    /// which sets the hostname to a rotating MAC address (12:34:56:ab:cd:ef).
    pub mac_address: bool,

    /// Return the full, raw, unencoded path string. Any query and fragment
    /// values will be return as part of the path instead of as distinct
    /// fields.
    pub raw_path: bool,
}

/// pub const ParseError = std.Uri.ParseError || error{InvalidMacAddress};
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedCharacter,
    InvalidFormat,
    InvalidPort,
    InvalidMacAddress,
}

impl From<StdParseError> for ParseError {
    fn from(e: StdParseError) -> ParseError {
        match e {
            StdParseError::UnexpectedCharacter => ParseError::UnexpectedCharacter,
            StdParseError::InvalidFormat => ParseError::InvalidFormat,
            StdParseError::InvalidPort => ParseError::InvalidPort,
        }
    }
}

/// Parses a URI from the given string.
///
/// This extends std.Uri.parse with some additional ParseOptions.
pub fn parse(text: &[u8], options: ParseOptions) -> Result<Uri<'_>, ParseError> {
    let mut uri = match std_parse(text) {
        Ok(u) => u,
        Err(err) => {
            // We can attempt to re-parse the text as a URI that has a MAC address
            // in its host field (which tripped up std.Uri.parse's port parsing):
            //
            //      file://12:34:56:78:90:aa/path/to/file
            //                            ^^ InvalidPort
            //
            if err != StdParseError::InvalidPort || !options.mac_address {
                return Err(err.into());
            }

            // We can assume that the initial Uri.parse already validated the
            // scheme, so we only need to find its bounds within the string.
            let scheme_end = match index_of(text, b"://") {
                Some(v) => v,
                None => {
                    return Err(ParseError::InvalidFormat);
                }
            };
            let scheme = &text[0..scheme_end];

            // We similarly find the bounds of the host component by looking
            // for the first slash (/) after the scheme. This is all we need
            // for this case because the resulting slice can be unambiguously
            // determined to be a MAC address (or not).
            let host_start = scheme_end + "://".len();
            let host_end = find_scalar_pos(text, host_start, b'/').unwrap_or(text.len());
            let mac_address = &text[host_start..host_end];
            if !isValidMacAddress(mac_address) {
                return Err(ParseError::InvalidMacAddress);
            }

            // Parse the rest of the text (starting with the path component) as a
            // partial URI and then add our MAC address as its host component.
            let mut uri = parseAfterScheme(scheme, &text[host_end..])?;
            uri.host = Some(Component::PercentEncoded(mac_address));
            uri
        }
    };

    // When MAC address parsing is enabled, we need to handle the case where
    // std.Uri.parse parsed the address's last octet as a numeric port number.
    // We use a few heuristics to identify this case (14 characters, 4 colons)
    // and then "repair" the result by reassign the .host component to the full
    // MAC address and clearing the .port component.
    //
    //    12:34:56:78:90:99 -> [12:34:56:78:90, 99] -> 12:34:56:78:90:99
    //    (original host)      (parsed host + port)    (restored host)
    //
    'mac: {
        if options.mac_address && uri.host.is_some() {
            let host = uri.host.unwrap().percent_encoded();
            if host.len() != 14 || count(host, b':') != 4 {
                break 'mac;
            }

            let port = match uri.port {
                Some(p) => p,
                None => break 'mac,
            };
            if port > 99 {
                break 'mac;
            }

            // std.Uri.parse returns slices pointing into the original text string.
            let host_start = ptr_offset(host, text);
            let path_start = ptr_offset(uri.path.percent_encoded(), text);
            let mac_address = &text[host_start..path_start];
            if !isValidMacAddress(mac_address) {
                return Err(ParseError::InvalidMacAddress);
            }

            uri.host = Some(Component::PercentEncoded(mac_address));
            uri.port = None;
        }
    }

    // When the raw_path option is active, return everything after the authority
    // (host) in the .path component, including any query and fragment values.
    if options.raw_path {
        // std.Uri.parse returns slices pointing into the original text string.
        let path_start = ptr_offset(uri.path.percent_encoded(), text);
        uri.path = Component::Raw(&text[path_start..]);
        uri.query = None;
        uri.fragment = None;
    }

    Ok(uri)
}

// PORT: `@intFromPtr(slice.ptr) - @intFromPtr(text.ptr)` — the byte offset of a
// sub-slice within `text`. Both slices point into the same backing buffer.
fn ptr_offset(sub: &[u8], text: &[u8]) -> usize {
    (sub.as_ptr() as usize) - (text.as_ptr() as usize)
}

// PORT: `std.mem.count(u8, host, ":")`
fn count(haystack: &[u8], needle: u8) -> usize {
    haystack.iter().filter(|&&b| b == needle).count()
}

/// Checks if a string represents a valid MAC address, e.g. 12:34:56:ab:cd:ef.
fn isValidMacAddress(s: &[u8]) -> bool {
    if s.len() != 17 {
        return false;
    }

    for (i, &c) in s.iter().enumerate() {
        if i % 3 == 2 {
            if c != b':' {
                return false;
            }
        } else {
            match c {
                b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f' => {}
                _ => return false,
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe<'a>(c: Option<Component<'a>>) -> &'a [u8] {
        // uriX.host.?.percent_encoded
        c.unwrap().percent_encoded()
    }

    #[test]
    fn parse_mac_address() {
        // Numeric MAC address without a port
        let uri1 = parse(
            b"file://00:12:34:56:78:90/path",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri1.scheme);
        assert_eq!(b"00:12:34:56:78:90".as_slice(), pe(uri1.host));
        assert_eq!(b"/path".as_slice(), uri1.path.percent_encoded());
        assert_eq!(None, uri1.port);

        // Numeric MAC address with a port
        let uri2 = parse(
            b"file://00:12:34:56:78:90:999/path",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri2.scheme);
        assert_eq!(b"00:12:34:56:78:90".as_slice(), pe(uri2.host));
        assert_eq!(b"/path".as_slice(), uri2.path.percent_encoded());
        assert_eq!(Some(999), uri2.port);

        // Alphabetic MAC address without a port
        let uri3 = parse(
            b"file://ab:cd:ef:ab:cd:ef/path",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri3.scheme);
        assert_eq!(b"ab:cd:ef:ab:cd:ef".as_slice(), pe(uri3.host));
        assert_eq!(b"/path".as_slice(), uri3.path.percent_encoded());
        assert_eq!(None, uri3.port);

        // Alphabetic MAC address with a port
        let uri4 = parse(
            b"file://ab:cd:ef:ab:cd:ef:999/path",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri4.scheme);
        assert_eq!(b"ab:cd:ef:ab:cd:ef".as_slice(), pe(uri4.host));
        assert_eq!(b"/path".as_slice(), uri4.path.percent_encoded());
        assert_eq!(Some(999), uri4.port);

        // Numeric MAC address without a path component
        let uri5 = parse(
            b"file://00:12:34:56:78:90",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri5.scheme);
        assert_eq!(b"00:12:34:56:78:90".as_slice(), pe(uri5.host));
        assert!(uri5.path.isEmpty());

        // Alphabetic MAC address without a path component
        let uri6 = parse(
            b"file://ab:cd:ef:ab:cd:ef",
            ParseOptions {
                mac_address: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri6.scheme);
        assert_eq!(b"ab:cd:ef:ab:cd:ef".as_slice(), pe(uri6.host));
        assert!(uri6.path.isEmpty());

        // Invalid MAC addresses
        assert_eq!(
            Err(ParseError::InvalidMacAddress),
            parse(
                b"file://zz:zz:zz:zz:zz:00/path",
                ParseOptions {
                    mac_address: true,
                    ..Default::default()
                },
            )
        );
        assert_eq!(
            Err(ParseError::InvalidMacAddress),
            parse(
                b"file://zz:zz:zz:zz:zz:zz/path",
                ParseOptions {
                    mac_address: true,
                    ..Default::default()
                },
            )
        );
    }

    #[test]
    fn parse_raw_path() {
        let text = b"file://localhost/path??#fragment";
        let mut buf = [0u8; 256];

        let uri1 = parse(
            text,
            ParseOptions {
                raw_path: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri1.scheme);
        assert_eq!(b"localhost".as_slice(), pe(uri1.host));
        assert_eq!(b"/path".as_slice(), uri1.path.toRaw(&mut buf).unwrap());
        assert_eq!(b"?".as_slice(), uri1.query.unwrap().percent_encoded());
        assert_eq!(
            b"fragment".as_slice(),
            uri1.fragment.unwrap().percent_encoded()
        );

        let uri2 = parse(
            text,
            ParseOptions {
                raw_path: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri2.scheme);
        assert_eq!(b"localhost".as_slice(), pe(uri2.host));
        assert_eq!(
            b"/path??#fragment".as_slice(),
            uri2.path.toRaw(&mut buf).unwrap()
        );
        assert_eq!(None, uri2.query);
        assert_eq!(None, uri2.fragment);

        let uri3 = parse(
            b"file://localhost",
            ParseOptions {
                raw_path: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b"file".as_slice(), uri3.scheme);
        assert_eq!(b"localhost".as_slice(), pe(uri3.host));
        assert!(uri3.path.isEmpty());
        assert_eq!(None, uri3.query);
        assert_eq!(None, uri3.fragment);
    }

    #[test]
    fn is_valid_mac_address() {
        assert!(isValidMacAddress(b"01:23:45:67:89:Aa"));
        assert!(isValidMacAddress(b"Aa:Bb:Cc:Dd:Ee:Ff"));

        assert!(!isValidMacAddress(b""));
        assert!(!isValidMacAddress(b"00:23:45"));
        assert!(!isValidMacAddress(b"00:23:45:Xx:Yy:Zz"));
        assert!(!isValidMacAddress(b"01-23-45-67-89-Aa"));
        assert!(!isValidMacAddress(b"01:23:45:67:89:Aa:Bb"));
    }
}
