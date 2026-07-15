//! rustty-cli::ssh_cache::entry
//!
//! PORTS: ghostty src/cli/ssh-cache/Entry.zig — a single entry within the SSH
//! terminfo cache. Byte-faithful (R1): the reference `[]const u8` fields become
//! owned `Vec<u8>` (R3, never lossy String). The default terminfo token
//! `xterm-ghostty` is routed through `rustty-identity` (R2 / ADV-FIX 5) so it
//! stays consistent with the rustty-terminfo entry name (`xterm-rustty`).

/// A single entry within our SSH entry cache. Our SSH entry cache
/// stores which hosts we've sent our terminfo to so that we don't have
/// to send it again. It doesn't store any sensitive information.
// const Entry = @This();
//
// const std = @import("std");
//
// PORT NOTE: the reference `Entry` fields are `[]const u8` slices borrowing into
// the parsed line. The Rust port owns its bytes (`Vec<u8>`, R3) so an `Entry`
// outlives the line it was parsed from — exactly how `DiskCache.readEntries`
// dupes the reference's borrows into owned copies before storing them in the map.
#[derive(Debug, Clone)]
pub struct Entry {
    // hostname: []const u8,
    pub hostname: Vec<u8>,
    // timestamp: i64,
    pub timestamp: i64,
    // terminfo_version: []const u8,
    pub terminfo_version: Vec<u8>,
}

impl Entry {
    // pub fn parse(line: []const u8) ?Entry {
    pub fn parse(line: &[u8]) -> Option<Entry> {
        // const trimmed = std.mem.trim(u8, line, " \t\r\n");
        let trimmed = trim(line, b" \t\r\n");
        // if (trimmed.len == 0) return null;
        if trimmed.is_empty() {
            return None;
        }

        // Parse format: hostname|timestamp|terminfo_version
        // var iter = std.mem.tokenizeScalar(u8, trimmed, '|');
        let mut iter = TokenizeScalar::new(trimmed, b'|');
        // const hostname = iter.next() orelse return null;
        let hostname = iter.next()?;
        // const timestamp_str = iter.next() orelse return null;
        let timestamp_str = iter.next()?;
        // const terminfo_version = iter.next() orelse "xterm-ghostty";
        //   R2 / ADV-FIX 5: the default terminfo token is the rustty product
        //   identity's primary terminfo name (`xterm-rustty`), routed through
        //   rustty-identity so it matches the actual rustty-terminfo entry name.
        let terminfo_version = iter
            .next()
            .unwrap_or_else(|| rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes());
        // const timestamp = std.fmt.parseInt(i64, timestamp_str, 10) catch |err| {
        let timestamp = match parse_int_i64(timestamp_str) {
            Ok(t) => t,
            Err(err) => {
                // std.log.warn(
                //     "Invalid timestamp in cache entry: {s} err={}",
                //     .{ timestamp_str, err },
                // );
                log_warn(&format!(
                    "Invalid timestamp in cache entry: {} err={}",
                    String::from_utf8_lossy(timestamp_str),
                    err,
                ));
                // return null;
                return None;
            }
        };

        // return .{
        //     .hostname = hostname,
        //     .timestamp = timestamp,
        //     .terminfo_version = terminfo_version,
        // };
        Some(Entry {
            hostname: hostname.to_vec(),
            timestamp,
            terminfo_version: terminfo_version.to_vec(),
        })
    }

    // pub fn format(self: Entry, writer: *std.Io.Writer) FormatError!void {
    //
    // PORT NOTE: `FormatError = std.Io.Writer.Error` -> `std::io::Error`; the
    // reference formats into a `*std.Io.Writer`, the port writes into any
    // `std::io::Write` (DiskCache.writeCacheFile passes a growable `Vec<u8>`).
    pub fn format<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // try writer.print(
        //     "{s}|{d}|{s}\n",
        //     .{ self.hostname, self.timestamp, self.terminfo_version },
        // );
        writer.write_all(&self.hostname)?;
        writer.write_all(b"|")?;
        writer.write_all(self.timestamp.to_string().as_bytes())?;
        writer.write_all(b"|")?;
        writer.write_all(&self.terminfo_version)?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the `std.mem`/`std.fmt`/`std.log` facilities used above.
// ---------------------------------------------------------------------------

/// `std.mem.trim(u8, s, set)`: strip leading/trailing bytes that appear in `set`.
fn trim<'a>(s: &'a [u8], set: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && set.contains(&s[start]) {
        start += 1;
    }
    while end > start && set.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

/// `std.mem.tokenizeScalar(u8, buf, delim)`: iterate the non-empty runs between
/// `delim` bytes (leading/trailing/consecutive delimiters yield no empty tokens).
struct TokenizeScalar<'a> {
    rest: &'a [u8],
    delim: u8,
}

impl<'a> TokenizeScalar<'a> {
    fn new(buf: &'a [u8], delim: u8) -> TokenizeScalar<'a> {
        TokenizeScalar { rest: buf, delim }
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        // Skip any leading delimiters (tokenize never yields empty tokens).
        while !self.rest.is_empty() && self.rest[0] == self.delim {
            self.rest = &self.rest[1..];
        }
        if self.rest.is_empty() {
            return None;
        }
        let mut i = 0;
        while i < self.rest.len() && self.rest[i] != self.delim {
            i += 1;
        }
        let tok = &self.rest[..i];
        self.rest = &self.rest[i..];
        Some(tok)
    }
}

/// `std.fmt.parseInt(i64, s, 10)`: parse a base-10 signed integer, erroring on
/// non-digit input or overflow (the reference's `catch` path returns null).
fn parse_int_i64(s: &[u8]) -> Result<i64, std::num::ParseIntError> {
    // `str::parse::<i64>` accepts an optional leading `+`/`-` and rejects
    // non-digit bytes and out-of-range values, matching `std.fmt.parseInt`.
    match std::str::from_utf8(s) {
        Ok(text) => text.parse::<i64>(),
        // Non-UTF8 input can't be a valid base-10 integer; reuse a parse error
        // so the caller's `catch` (warn + return null) path is taken.
        Err(_) => "".parse::<i64>(),
    }
}

// std.log.scoped default: warnings surface on stderr. Mirrors the os/open.rs
// `log_warn` convention (the reference here uses the unscoped `std.log.warn`).
fn log_warn(msg: &str) {
    eprintln!("[warn] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "cache entry parsing valid formats"
    #[test]
    fn cache_entry_parsing_valid_formats() {
        // The terminfo token is the rustty product identity's primary terminfo
        // name (ADV-FIX 5); building the fixtures from it keeps the parse
        // round-trip self-consistent under both the default and `identity-oracle`
        // profiles.
        let ti = rustty_identity::TERMINFO_PRIMARY_NAME;

        // const entry = Entry.parse("example.com|1640995200|xterm-ghostty").?;
        let entry = Entry::parse(format!("example.com|1640995200|{ti}").as_bytes()).unwrap();
        // try testing.expectEqualStrings("example.com", entry.hostname);
        assert_eq!(b"example.com".as_slice(), entry.hostname.as_slice());
        // try testing.expectEqual(@as(i64, 1640995200), entry.timestamp);
        assert_eq!(1640995200_i64, entry.timestamp);
        // try testing.expectEqualStrings("xterm-ghostty", entry.terminfo_version);
        assert_eq!(ti.as_bytes(), entry.terminfo_version.as_slice());

        // Test default terminfo version
        // const entry_no_version = Entry.parse("test.com|1640995200").?;
        let entry_no_version = Entry::parse(b"test.com|1640995200").unwrap();
        // try testing.expectEqualStrings("xterm-ghostty", entry_no_version.terminfo_version);
        assert_eq!(ti.as_bytes(), entry_no_version.terminfo_version.as_slice());

        // Test complex hostnames
        // const complex_entry = Entry.parse("user@server.example.com|1640995200|xterm-ghostty").?;
        let complex_entry =
            Entry::parse(format!("user@server.example.com|1640995200|{ti}").as_bytes()).unwrap();
        // try testing.expectEqualStrings("user@server.example.com", complex_entry.hostname);
        assert_eq!(
            b"user@server.example.com".as_slice(),
            complex_entry.hostname.as_slice(),
        );
    }

    // test "cache entry parsing invalid formats"
    #[test]
    fn cache_entry_parsing_invalid_formats() {
        // try testing.expect(Entry.parse("") == null);
        assert!(Entry::parse(b"").is_none());

        // Invalid format (no pipe)
        // try testing.expect(Entry.parse("v1") == null);
        assert!(Entry::parse(b"v1").is_none());

        // Missing timestamp
        // try testing.expect(Entry.parse("example.com") == null);
        assert!(Entry::parse(b"example.com").is_none());

        // Invalid timestamp
        // try testing.expect(Entry.parse("example.com|invalid") == null);
        assert!(Entry::parse(b"example.com|invalid").is_none());

        // Empty terminfo should default
        // try testing.expect(Entry.parse("example.com|1640995200|") != null);
        assert!(Entry::parse(b"example.com|1640995200|").is_some());
    }

    // test "cache entry parsing malformed data resilience"
    #[test]
    fn cache_entry_parsing_malformed_data_resilience() {
        // Extra pipes should not break parsing
        // try testing.expect(Entry.parse("host|123|term|extra") != null);
        assert!(Entry::parse(b"host|123|term|extra").is_some());

        // Whitespace handling
        // try testing.expect(Entry.parse("  host|123|term  ") != null);
        assert!(Entry::parse(b"  host|123|term  ").is_some());
        // try testing.expect(Entry.parse("\n") == null);
        assert!(Entry::parse(b"\n").is_none());
        // try testing.expect(Entry.parse("   \t  \n") == null);
        assert!(Entry::parse(b"   \t  \n").is_none());

        // Extremely large timestamp
        // try testing.expect(
        //     Entry.parse("host|999999999999999999999999999999999999999999999999|xterm-ghostty") == null,
        // );
        assert!(Entry::parse(
            b"host|999999999999999999999999999999999999999999999999|xterm-rustty"
        )
        .is_none(),);
    }
}
