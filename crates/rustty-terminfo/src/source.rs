//! Terminfo source format. This can be used to encode terminfo files.
//! This cannot parse terminfo source files yet because it isn't something
//! I need to do but this can be added later.
//!
//! Background: https://invisible-island.net/ncurses/man/terminfo.5.html

use std::collections::HashMap;
use std::io::{self, Write};

/// The terminfo source model (the Zig `Source = @This()` struct).
///
/// Byte-slices in the reference (`[]const u8` / `[]const []const u8`) are ported
/// as owned `Vec<u8>` / `&[u8]` per R1 (never lossy `String`): terminfo
/// capability values are raw bytes (they embed control chars like `0x1B`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The set of names for the terminal. These match the TERM environment variable
    /// and are used to look up this terminal. Historically, the final name in the
    /// list was the most common name for the terminal and contains spaces and
    /// other characters. See terminfo(5) for details.
    pub names: Vec<Vec<u8>>,

    /// The set of capabilities in this terminfo file.
    pub capabilities: Vec<Capability>,
}

/// A capability in a terminfo file. This also includes any "use" capabilities
/// since they behave just like other capabilities as documented in terminfo(5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// The name of capability. This is the "Cap-name" value in terminfo(5).
    pub name: Vec<u8>,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Canceled value, i.e. suffixed with @
    Canceled,

    /// Boolean values are always true if they exist so there is no value.
    Boolean,

    /// Numeric values are always "unsigned decimal integers". The size
    /// of the integer is unspecified in terminfo(5). I chose 32-bits
    /// because it is a common integer size but this may be wrong.
    Numeric(u32),

    String(Vec<u8>),
}

impl Source {
    /// Encode as a terminfo source file. The encoding is always done in a
    /// human-readable format with whitespace. Fields are always written in the
    /// order of the slices on this struct; this will not do any reordering.
    pub fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        // Encode the names in the order specified
        for (i, name) in self.names.iter().enumerate() {
            if i != 0 {
                writer.write_all(b"|")?;
            }
            writer.write_all(name)?;
        }
        writer.write_all(b",\n")?;

        // Encode each of the capabilities in the order specified
        for cap in &self.capabilities {
            writer.write_all(b"\t")?;
            writer.write_all(&cap.name)?;
            match &cap.value {
                Value::Canceled => writer.write_all(b"@")?,
                Value::Boolean => {}
                Value::Numeric(v) => write!(writer, "#{v}")?,
                Value::String(v) => {
                    writer.write_all(b"=")?;
                    writer.write_all(v)?;
                }
            }
            writer.write_all(b",\n")?;
        }

        Ok(())
    }

    /// Returns a `StaticStringMap` for all of the capabilities in this terminfo.
    /// The value is the value that should be sent as a response to XTGETTCAP.
    /// Important: the value is the FULL response included the escape sequences.
    ///
    /// The responses are null-terminated so that they can be handed directly
    /// to APIs that require a sentinel (e.g. the libghostty-vt write_pty
    /// callback) without copying.
    ///
    /// The reference builds a comptime `std.StaticStringMap([:0]const u8)`; the
    /// faithful Rust port builds the same key/value pairs into a `HashMap` at
    /// runtime (the keys/values are raw bytes, see [`Source`]).
    ///
    /// PORT NOTE (`74efadb44`): the reference's `[]const u8` -> `[:0]const u8`
    /// value change (and the sentinel-driven buffer growth: numeric buf 10->11,
    /// response buf +1, `comptimeReplace` +1) exists only for the Zig `[:0]`
    /// sentinel that its `write_pty` callback type requires. rustty's
    /// `write_pty` takes a plain `&[u8]` (the established DCS-response
    /// precedent in `stream_terminal.rs`), so no NUL byte is materialized here
    /// and the response BYTES are unchanged.
    pub fn xtgettcap_map(&self) -> HashMap<Vec<u8>, Vec<u8>> {
        // We have all of our capabilities plus To, TN, and RGB which aren't
        // in the capabilities list but are query-able.
        let len = self.capabilities.len() + 3;
        let mut kvs: Vec<(Vec<u8>, Vec<u8>)> = vec![(Vec::new(), Vec::new()); len];

        // We first build all of our entries with raw K=V pairs.
        kvs[0] = (b"TN".to_vec(), self.names[0].clone());
        kvs[1] = (b"Co".to_vec(), b"256".to_vec());
        kvs[2] = (b"RGB".to_vec(), b"8".to_vec());
        for (i, cap) in self.capabilities.iter().enumerate() {
            let i = i + 3;
            kvs[i] = (
                cap.name.clone(),
                match &cap.value {
                    Value::Canceled => panic!("canceled not handled yet"),
                    Value::Boolean => Vec::new(),
                    Value::String(v) => {
                        // If a string contains parameters, then we do not escape
                        // anything within the string. I BELIEVE the history here is
                        // xterm initially only supported specific capabilities and none
                        // had parameters so it returned the tcap encoded form. Later,
                        // Kitty added support for more capabilities some of which
                        // have parameters. But Kitty returned them in terminfo source
                        // format. So we need to handle both cases.
                        if v.contains(&b'%') {
                            v.clone()
                        } else {
                            // No-parameters. Encode and return.
                            // First replace \E with the escape char (0x1B)
                            let mut result = comptime_replace(v, b"\\E", b"\x1b");
                            // Replace '^' with the control char version of that char.
                            while let Some(idx) = result.iter().position(|&c| c == b'^') {
                                if idx > 0 {
                                    panic!("handle control-char in middle of string");
                                }
                                let replacement: u8 = match result[idx + 1] {
                                    b'?' => 0x7F, // DEL, special cased from ncurses
                                    c => c - 64,
                                };
                                result = comptime_replace(
                                    &result,
                                    &result[idx..idx + 2],
                                    &[replacement],
                                );
                            }
                            result
                        }
                    }
                    Value::Numeric(v) => {
                        let mut buf = Vec::new();
                        write!(&mut buf, "{v}").unwrap();
                        buf
                    }
                },
            );
        }

        // Now go through and convert them all to hex-encoded strings.
        for entry in &mut kvs {
            // The key is just the raw hex-encoded string
            entry.0 = hexencode(&entry.0);

            // The value is more complex
            let out: Vec<u8> = if entry.1 == b"" {
                let mut buf = Vec::new();
                // important: hex-encoded name
                write!(&mut buf, "\x1bP1+r").unwrap();
                buf.extend_from_slice(&entry.0);
                write!(&mut buf, "\x1b\\").unwrap();
                buf
            } else {
                let mut buf = Vec::new();
                // important: hex-encoded name
                write!(&mut buf, "\x1bP1+r").unwrap();
                buf.extend_from_slice(&entry.0);
                buf.push(b'=');
                buf.extend_from_slice(&hexencode(&entry.1));
                write!(&mut buf, "\x1b\\").unwrap();
                buf
            };

            entry.1 = out;
        }

        kvs.into_iter().collect()
    }
}

fn hexencode(input: &[u8]) -> Vec<u8> {
    bytes_to_hex(input)
}

/// `std.mem.replace` but comptime-only so we can return the string
/// allocated in comptime memory.
///
/// In Rust this is a plain runtime function returning an owned `Vec<u8>`; the
/// reference's comptime-only constraint exists to satisfy Zig comptime
/// memory-allocation rules which do not apply here.
fn comptime_replace(input: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if !needle.is_empty() && input[i..].starts_with(needle) {
            buf.extend_from_slice(replacement);
            i += needle.len();
        } else {
            buf.push(input[i]);
            i += 1;
        }
    }
    buf
}

/// Faithful port of `std.fmt.bytesToHex(input, .upper)`: every input byte is
/// rendered as two uppercase hex digits.
fn bytes_to_hex(input: &[u8]) -> Vec<u8> {
    const CHARSET: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(input.len() * 2);
    for &b in input {
        out.push(CHARSET[(b >> 4) as usize]);
        out.push(CHARSET[(b & 0xf) as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xtgettcap_map_test() {
        let src = Source {
            names: vec![
                b"ghostty".to_vec(),
                b"xterm-ghostty".to_vec(),
                b"Ghostty".to_vec(),
            ],

            capabilities: vec![
                Capability {
                    name: b"am".to_vec(),
                    value: Value::Boolean,
                },
                Capability {
                    name: b"colors".to_vec(),
                    value: Value::Numeric(256),
                },
                Capability {
                    name: b"kx".to_vec(),
                    value: Value::String(b"^?".to_vec()),
                },
                Capability {
                    name: b"kbs".to_vec(),
                    value: Value::String(b"^H".to_vec()),
                },
                Capability {
                    name: b"kf1".to_vec(),
                    value: Value::String(b"\\EOP".to_vec()),
                },
                Capability {
                    name: b"Smulx".to_vec(),
                    value: Value::String(b"\\E[4:%p1%dm".to_vec()),
                },
            ],
        };

        let map = src.xtgettcap_map();
        assert_eq!(
            b"\x1bP1+r616D\x1b\\".to_vec(),
            *map.get(&hexencode(b"am")).unwrap(),
        );
        assert_eq!(
            b"\x1bP1+r6B78=7F\x1b\\".to_vec(),
            *map.get(&hexencode(b"kx")).unwrap(),
        );
        assert_eq!(
            b"\x1bP1+r6B6273=08\x1b\\".to_vec(),
            *map.get(&hexencode(b"kbs")).unwrap(),
        );
        assert_eq!(
            b"\x1bP1+r6B6631=1B4F50\x1b\\".to_vec(),
            *map.get(&hexencode(b"kf1")).unwrap(),
        );
        assert_eq!(
            b"\x1bP1+r536D756C78=5C455B343A25703125646D\x1b\\".to_vec(),
            *map.get(&hexencode(b"Smulx")).unwrap(),
        );
    }

    #[test]
    fn encode() {
        let src = Source {
            names: vec![
                b"ghostty".to_vec(),
                b"xterm-ghostty".to_vec(),
                b"Ghostty".to_vec(),
            ],

            capabilities: vec![
                Capability {
                    name: b"am".to_vec(),
                    value: Value::Boolean,
                },
                Capability {
                    name: b"ccc".to_vec(),
                    value: Value::Canceled,
                },
                Capability {
                    name: b"colors".to_vec(),
                    value: Value::Numeric(256),
                },
                Capability {
                    name: b"bel".to_vec(),
                    value: Value::String(b"^G".to_vec()),
                },
            ],
        };

        // Encode
        let mut buf: Vec<u8> = Vec::new();
        src.encode(&mut buf).unwrap();

        let expected: &[u8] = b"ghostty|xterm-ghostty|Ghostty,\n\
            \tam,\n\
            \tccc@,\n\
            \tcolors#256,\n\
            \tbel=^G,\n";
        assert_eq!(expected, buf.as_slice());
    }
}
