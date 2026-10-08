//! OSC 7501: Program Status Protocol
//!
//! Specification: https://www.superlogical.com/rex/docs/build/program-status
//!
//! A program uses this protocol to tell the terminal what it is doing:
//! idle, working, done, waiting on the user, or failed. Each report is a
//! list of `key=value` pairs separated by colons:
//!
//! ```text
//! ESC ] 7501 ; state=working:app=cargo:progress=40 ESC \
//! ```
//!
//! A terminal keeps one record per id, and each report replaces its
//! record completely. This parser only reads and validates one report at
//! a time. Keeping records is up to the code that handles the command.
//! In libghostty-vt, that is the `program_status` effect of
//! `stream_terminal.Handler`.
//!
//! A program checks whether the terminal supports the protocol by sending
//! `?` as the body. A terminal that supports it replies with the same
//! sequence. See `Command::Query`.
//!
//! Example:
//!
//! ```text
//! var p: osc.Parser = .init(alloc);
//! defer p.deinit();
//!
//! p.nextSlice("7501;state=working:id=sync:progress=40");
//! const report = p.end('\x1b').?.program_status.report;
//! report.state; // .working
//! report.readOption(.id); // "sync"
//! report.readOption(.progress); // 40
//! report.readOption(.app); // null, the program didn't send one
//! ```
//!
//! PORTS: ghostty src/terminal/osc/parsers/program_status.zig
//!
//! PORT STATUS: ported — `parse`, `validate`, `Report` and the 15
//! `OSC 7501:` tests (bae2c3cdb). The C-API half (`Command.C`, the
//! `program_status` effect) is P8-W5.

use crate::apc::glyph::request::{Base64Decoder, STANDARD_DECODER, STANDARD_NO_PAD_DECODER};
use crate::osc::encoding;
use crate::osc::kitty_metadata::ValueIterator;
use crate::osc::{Command as OSCCommand, Parser, State as ParserState, Terminator};

// const log = std.log.scoped(.osc_program_status);
// The scope, carried through the `log` facade as `target: LOG_SCOPE`.
const LOG_SCOPE: &str = "osc_program_status";

// Size limits from the specification. Every limit counts bytes, not
// characters. A report that breaks any limit is discarded as a whole.

/// The longest whole sequence, from `ESC ]` through the terminator.
pub const MAX_SEQUENCE_BYTES: usize = 4096;

/// The longest body, everything after `7501;`. This assumes the
/// longer terminator (`ESC \`), so a sequence ended with BEL is limited
/// to one byte less than the specification allows. The specification
/// lets terminals choose lower limits.
pub const MAX_BODY_BYTES: usize = MAX_SEQUENCE_BYTES - b"\x1b]7501;".len() - b"\x1b\\".len();

/// The longest key, including keys this parser doesn't know.
pub const MAX_KEY_BYTES: usize = 16;

/// The longest `msg` value before and after base64 decoding.
pub const MAX_MSG_ENCODED_BYTES: usize = 2732;
pub const MAX_MSG_BYTES: usize = 2048;

/// The longest `title` value before and after base64 decoding.
pub const MAX_TITLE_ENCODED_BYTES: usize = 256;
pub const MAX_TITLE_BYTES: usize = 192;

/// The longest `app` value.
pub const MAX_APP_BYTES: usize = 32;

/// The longest `id` value, the longest part between two slashes, and the
/// most parts an id can have.
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_ID_SEGMENT_BYTES: usize = 32;
pub const MAX_ID_DEPTH: usize = 8;

/// A parsed OSC 7501 sequence: either a support query or a report.
// PORT NOTE: `pub const C = void;` and the `Report`/`State`/`Kind`/`OptionKey`
// aliases are the C-API/namespace re-exports; the types live at module level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command<'p> {
    /// The program asked whether the terminal supports the protocol by
    /// sending `OSC 7501 ; ? ST`. A terminal that supports it sends the
    /// same sequence back, ended with this terminator so the program
    /// recognizes it. A program that gets no reply assumes the protocol
    /// is unsupported.
    Query(Terminator),

    /// A valid status report for one record.
    Report(Report<'p>),
}

/// The value of the required `state` key.
///
/// - `idle`: at rest, waiting for the user's next instruction.
/// - `working`: running. May carry a progress percentage.
/// - `done`: finished a piece of work the user hasn't looked at yet.
/// - `blocked`: can't continue until the user does something. The
///   report's `kind` says what. May carry a progress percentage.
/// - `error`: failed and stopped.
/// - `clear`: not a real state. Removes the addressed record and every
///   record beneath it. Without an id, it removes every record.
// PORT NOTE: `lib.Enum(lib.target, ...)`; the `.c` target is a `c_int` enum
// numbered in declaration order, hence `repr(i32)`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    Clear,
}

impl State {
    /// `std.meta.stringToEnum(State, s)`.
    pub fn from_name(s: &[u8]) -> Option<State> {
        match s {
            b"idle" => Some(State::Idle),
            b"working" => Some(State::Working),
            b"done" => Some(State::Done),
            b"blocked" => Some(State::Blocked),
            b"error" => Some(State::Error),
            b"clear" => Some(State::Clear),
            _ => None,
        }
    }
}

/// The value of the `kind` key, which says what a blocked program needs
/// from the user.
///
/// - `permission`: approval to do something.
/// - `question`: an answer the user has to type.
/// - `auth`: a login, token, or other credential.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Permission,
    Question,
    Auth,
}

impl Kind {
    /// `std.meta.stringToEnum(Kind, s)`.
    pub fn from_name(s: &[u8]) -> Option<Kind> {
        match s {
            b"permission" => Some(Kind::Permission),
            b"question" => Some(Kind::Question),
            b"auth" => Some(Kind::Auth),
            _ => None,
        }
    }
}

/// A status report that passed every check in the specification.
///
/// `state` is always present. Read everything else with `read_option`,
/// and the text values with `write_text`:
///
/// ```text
/// if let OptionValue::Progress(Some(percent)) = report.read_option(OptionKey::Progress) { .. }
///
/// let mut buf = Vec::new();
/// report.write_text(OptionKey::Msg, &mut buf)?;
/// ```
///
/// The values point into the parser's buffer, so they are only valid
/// until the next call to the parser. Copy anything you want to keep.
///
/// A key the program didn't send reads as null. A report replaces its
/// record completely, so a null value means the record no longer has
/// that value, not that the old value should be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report<'p> {
    /// What the program is doing.
    pub state: State,

    /// Everything after `7501;`, already validated by the parser. Read
    /// it with `read_option` and `write_text`.
    pub data: &'p [u8],
}

/// The value `Report::read_option` produces. The active variant matches
/// `OptionKey::value_type` of the option that was read.
// PORT NOTE: stands in for the comptime-dependent `option.Type()` return type
// of `Report.readOption` in the reference:
//   .kind => ?Kind, .progress => ?u8, .id/.app/.title/.msg => ?[]const u8
// (the same modeling as `context_signal::FieldValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionValue<'a> {
    Kind(Option<Kind>),
    Progress(Option<u8>),
    /// PORT NOTE (fidelity): `[]const u8` — RAW bytes, borrowed from the
    /// report.
    Str(Option<&'a [u8]>),
}

impl<'p> Report<'p> {
    /// Read the value of a key, or null if the program didn't send it.
    /// If the program sent a key more than once, the last value wins.
    pub fn read_option(&self, option: OptionKey) -> OptionValue<'p> {
        let v = last_value(option.tag_name(), self.data);

        match option {
            // The parser already discarded reports with an invalid id.
            OptionKey::Id => OptionValue::Str(v),
            OptionKey::App => OptionValue::Str(v.filter(|v| is_name(v))),
            OptionKey::Kind => OptionValue::Kind(match v {
                Some(v) if self.state == State::Blocked => Kind::from_name(v),
                _ => None,
            }),
            OptionKey::Progress => OptionValue::Progress(match v {
                Some(v) if matches!(self.state, State::Working | State::Blocked) => {
                    parse_progress(v)
                }
                _ => None,
            }),
            OptionKey::Title | OptionKey::Msg => OptionValue::Str(v.filter(|v| !v.is_empty())),
        }
    }

    /// Write the decoded text of `title` or `msg` to `writer`. Writes
    /// nothing if the program didn't send it.
    ///
    /// The text is valid UTF-8 with no control characters, but it is
    /// still untrusted text from the program. It is at most
    /// `MAX_TITLE_BYTES` or `MAX_MSG_BYTES` long.
    pub fn write_text(
        &self,
        option: OptionKey,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<()> {
        // comptime assert(option == .title or option == .msg);
        assert!(option == OptionKey::Title || option == OptionKey::Msg);
        let OptionValue::Str(Some(encoded)) = self.read_option(option) else {
            return Ok(());
        };

        // The parser already checked that this decodes, so it can't fail.
        let mut buf = [0u8; MAX_MSG_BYTES];
        writer.write_all(decode_text(encoded, &mut buf).expect("validated by the parser"))
    }
}

/// The keys of a report, other than `state`.
///
/// - `id`: the record this report is about. Without it, the report is
///   about the root record, the program itself. A program that reports on
///   several things at once, like a deploy to several regions, gives each
///   one an id. A `/` makes one record the child of another, so
///   `deploy/us-east` is a child of `deploy`. The parent doesn't have to
///   exist.
/// - `kind`: what a blocked program needs from the user. Only read when
///   the state is `blocked`. An unknown kind reads as null.
/// - `progress`: how far along the work is, from 0 to 100. Only read when
///   the state is `working` or `blocked`. A value outside that range
///   reads as null.
/// - `app`: a stable name for the program that a machine can match on,
///   such as `cargo` or `terraform`.
/// - `title`: a short label for the record, meant for people. Programs
///   that report several records use this to tell them apart.
/// - `msg`: one line of text for people, saying what the record is
///   doing, waiting for, or has finished. Don't try to read meaning into
///   it.
///
/// `read_option` returns `title` and `msg` still encoded as base64. Use
/// `write_text` to read their text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKey {
    Id,
    Kind,
    Progress,
    App,
    Title,
    Msg,
}

impl OptionKey {
    /// `@tagName(option)`.
    fn tag_name(self) -> &'static [u8] {
        match self {
            OptionKey::Id => b"id",
            OptionKey::Kind => b"kind",
            OptionKey::Progress => b"progress",
            OptionKey::App => b"app",
            OptionKey::Title => b"title",
            OptionKey::Msg => b"msg",
        }
    }
}

/// Parse OSC 7501. The body is everything after `7501;`.
///
/// The parser is forgiving about single pairs and strict about the
/// report as a whole, as the specification requires. It skips a pair
/// that is malformed or has a key it doesn't know, and keeps going:
///
/// ```text
/// state=idle:future=yes:oops   ->  a report with state idle
/// ```
///
/// It returns null, so nothing from the report is applied, when:
///
/// - `state` is missing or isn't a state this parser knows.
/// - Any key or value is longer than its limit, even one that a later
///   pair with the same key replaces.
/// - The id isn't made of valid names joined by single slashes. This
///   throws out the report instead of skipping the pair, so a report
///   meant for one record never falls back to the root record. For
///   `state=clear`, that would remove every record.
/// - `title` or `msg` isn't valid base64, or decodes to text that isn't
///   UTF-8 or contains a control character.
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<OSCCommand<'a>> {
    // assert(parser.state == .@"7501");
    assert!(parser.state == ParserState::S7501);

    // PORT NOTE (reshape): the verdict is computed in a scope that ends the
    // borrow of the capture, so the failure arms can set `parser.state`; the
    // success arm then re-borrows the data for `'a` (the reference holds the
    // slice across both).
    let verdict: Result<Verdict, Error> = {
        // const cap = if (parser.capture) |*c| c else { ... };
        let Some(cap) = parser.capture.as_ref() else {
            parser.state = ParserState::Invalid;
            return None;
        };

        // const data = cap.trailing();
        let data = cap.trailing(&parser.buffer);
        if data.len() > MAX_BODY_BYTES {
            log::warn!(
                target: LOG_SCOPE,
                "OSC 7501 sequence too long len={}",
                data.len()
            );
            parser.state = ParserState::Invalid;
            return None;
        }

        if data == b"?" {
            Ok(Verdict::Query)
        } else {
            validate(data).map(Verdict::Report)
        }
    };

    match verdict {
        Ok(Verdict::Query) => Some(OSCCommand::ProgramStatus(Command::Query(Terminator::init(
            terminator_ch,
        )))),
        Ok(Verdict::Report(state)) => {
            let cap = parser.capture.as_ref().expect("checked above");
            Some(OSCCommand::ProgramStatus(Command::Report(Report {
                state,
                data: cap.trailing(&parser.buffer),
            })))
        }
        Err(err) => {
            // `{}` of a Zig error prints its name.
            log::warn!(target: LOG_SCOPE, "invalid OSC 7501 report err={err:?}");
            parser.state = ParserState::Invalid;
            None
        }
    }
}

enum Verdict {
    Query,
    Report(State),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Error {
    InvalidState,
    InvalidId,
    InvalidText,
    TooLong,
}

/// Check a report body against the specification and return its state.
fn validate(data: &[u8]) -> Result<State, Error> {
    // A key that is too long breaks a limit, even a key we don't know.
    for pair in data.split(|&b| b == b':') {
        let Some(eq) = pair.iter().position(|&b| b == b'=') else {
            continue;
        };
        let key = trim_whitespace(&pair[0..eq]);
        if key.len() > MAX_KEY_BYTES {
            return Err(Error::TooLong);
        }
    }

    // Every value is checked against its limit, even one a later pair
    // replaces, because any limit violation discards the whole report.
    let mut ids = ValueIterator::init(b"id", Some(VALUE_BYTES), data);
    while let Some(v) = ids.next() {
        validate_id(v)?;
    }
    for (key, limit) in [
        (&b"app"[..], MAX_APP_BYTES),
        (b"title", MAX_TITLE_ENCODED_BYTES),
        (b"msg", MAX_MSG_ENCODED_BYTES),
    ] {
        let mut it = ValueIterator::init(key, Some(VALUE_BYTES), data);
        while let Some(v) = it.next() {
            if v.len() > limit {
                return Err(Error::TooLong);
            }
        }
    }

    let mut buf = [0u8; MAX_MSG_BYTES];
    if let Some(v) = last_value(b"title", data) {
        if decode_text(v, &mut buf)?.len() > MAX_TITLE_BYTES {
            return Err(Error::TooLong);
        }
    }
    if let Some(v) = last_value(b"msg", data) {
        decode_text(v, &mut buf)?;
    }

    // An unknown state discards the report rather than guessing, so a
    // state added in a later revision never turns into something else.
    last_value(b"state", data)
        .and_then(State::from_name)
        .ok_or(Error::InvalidState)
}

/// The last value of `key` in a report body. Pairs without an `=` and
/// values with bytes outside the value alphabet are skipped.
fn last_value<'a>(key: &'static [u8], data: &'a [u8]) -> Option<&'a [u8]> {
    let mut it = ValueIterator::init(key, Some(VALUE_BYTES), data);
    let mut last = None;
    while let Some(v) = it.next() {
        last = Some(v);
    }
    last
}

/// The bytes allowed in a name and a value. A name is the form of an
/// `app` value and of each part of an `id`.
const NAME_BYTES: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.+-";
// `name_bytes ++ ",/="`
const VALUE_BYTES: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.+-,/=";

/// `std.ascii.whitespace`, trimmed from both ends by `std.mem.trim`.
fn trim_whitespace(s: &[u8]) -> &[u8] {
    const WS: &[u8] = &[b' ', b'\t', b'\n', b'\r', 0x0B, 0x0C];
    let mut start = 0;
    let mut end = s.len();
    while start < end && WS.contains(&s[start]) {
        start += 1;
    }
    while end > start && WS.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

fn is_name(s: &[u8]) -> bool {
    // s.len > 0 and std.mem.indexOfNone(u8, s, name_bytes) == null
    !s.is_empty() && s.iter().all(|b| NAME_BYTES.contains(b))
}

/// Validate an id: names joined by single slashes, within the limits.
fn validate_id(value: &[u8]) -> Result<(), Error> {
    if value.len() > MAX_ID_BYTES {
        return Err(Error::TooLong);
    }
    let mut depth: usize = 0;
    for segment in value.split(|&b| b == b'/') {
        depth += 1;
        if depth > MAX_ID_DEPTH {
            return Err(Error::TooLong);
        }
        if segment.len() > MAX_ID_SEGMENT_BYTES {
            return Err(Error::TooLong);
        }
        if !is_name(segment) {
            return Err(Error::InvalidId);
        }
    }
    Ok(())
}

/// Parse a whole number from 0 to 100. Anything else is treated as
/// absent. This doesn't use std.fmt because it accepts underscores.
fn parse_progress(value: &[u8]) -> Option<u8> {
    if value.is_empty() {
        return None;
    }
    let mut n: u16 = 0;
    for &c in value {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + u16::from(c - b'0');
        if n > 100 {
            return None;
        }
    }
    Some(n as u8)
}

/// Decode a base64 title or msg into `buf` and check that the text is
/// safe to show. Padding is optional. The caller must have checked the
/// encoded length against `MAX_MSG_ENCODED_BYTES`.
fn decode_text<'b>(encoded: &[u8], buf: &'b mut [u8; MAX_MSG_BYTES]) -> Result<&'b [u8], Error> {
    let decoder: &Base64Decoder = if encoded.ends_with(b"=") {
        &STANDARD_DECODER
    } else {
        &STANDARD_NO_PAD_DECODER
    };

    let len = decoder
        .calc_size_for_slice(encoded)
        .map_err(|_| Error::InvalidText)?;
    if len > buf.len() {
        return Err(Error::TooLong);
    }

    let decoded = &mut buf[0..len];
    decoder
        .decode(decoded, encoded)
        .map_err(|_| Error::InvalidText)?;
    if !encoding::is_safe_utf8(decoded) {
        return Err(Error::InvalidText);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::testing::{with_handle, CountingAllocator};

    fn parser() -> Parser {
        Parser::init(Some(crate::alloc::Global))
    }

    /// `p.end('\x1b').?.program_status` (ST) as the report.
    fn report<'a>(p: &'a mut Parser) -> Report<'a> {
        match p.end(Some(0x1b)) {
            Some(OSCCommand::ProgramStatus(Command::Report(r))) => r,
            other => panic!("expected a report, got {other:?}"),
        }
    }

    fn str_opt<'a>(r: &Report<'a>, o: OptionKey) -> Option<&'a [u8]> {
        match r.read_option(o) {
            OptionValue::Str(v) => v,
            other => panic!("expected a string option, got {other:?}"),
        }
    }

    fn kind_opt(r: &Report<'_>) -> Option<Kind> {
        match r.read_option(OptionKey::Kind) {
            OptionValue::Kind(v) => v,
            other => panic!("expected kind, got {other:?}"),
        }
    }

    fn progress_opt(r: &Report<'_>) -> Option<u8> {
        match r.read_option(OptionKey::Progress) {
            OptionValue::Progress(v) => v,
            other => panic!("expected progress, got {other:?}"),
        }
    }

    fn text(r: &Report<'_>, o: OptionKey) -> Vec<u8> {
        let mut out = Vec::new();
        r.write_text(o, &mut out).unwrap();
        out
    }

    // test "OSC 7501: query"
    #[test]
    fn osc_7501_query() {
        let mut p = parser();

        p.next_slice(b"7501;?");
        assert_eq!(
            Some(OSCCommand::ProgramStatus(Command::Query(Terminator::St))),
            p.end(Some(0x1b))
        );

        p.reset();
        p.next_slice(b"7501;?");
        assert_eq!(
            Some(OSCCommand::ProgramStatus(Command::Query(Terminator::Bel))),
            p.end(Some(0x07))
        );
        p.deinit();
    }

    // test "OSC 7501: minimal report"
    #[test]
    fn osc_7501_minimal_report() {
        let mut p = parser();

        p.next_slice(b"7501;state=idle");
        let r = report(&mut p);
        assert_eq!(State::Idle, r.state);
        assert_eq!(None, kind_opt(&r));
        assert_eq!(None, progress_opt(&r));
        assert_eq!(None, str_opt(&r, OptionKey::Id));
        assert_eq!(None, str_opt(&r, OptionKey::App));
        assert_eq!(None, str_opt(&r, OptionKey::Title));
        assert_eq!(None, str_opt(&r, OptionKey::Msg));
        p.deinit();
    }

    // test "OSC 7501: all keys"
    #[test]
    fn osc_7501_all_keys() {
        let mut p = parser();

        // "Plan" and "Apply 3 to add, 1 to change, 0 to destroy?"
        p.next_slice(
            b"7501;state=blocked:kind=permission:progress=42:id=tf/plan:app=terraform\
:title=UGxhbg==:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/",
        );
        let r = report(&mut p);
        assert_eq!(State::Blocked, r.state);
        assert_eq!(Some(Kind::Permission), kind_opt(&r));
        assert_eq!(Some(42), progress_opt(&r));
        assert_eq!(Some(&b"tf/plan"[..]), str_opt(&r, OptionKey::Id));
        assert_eq!(Some(&b"terraform"[..]), str_opt(&r, OptionKey::App));
        assert_eq!(b"Plan".to_vec(), text(&r, OptionKey::Title));
        assert_eq!(
            b"Apply 3 to add, 1 to change, 0 to destroy?".to_vec(),
            text(&r, OptionKey::Msg)
        );
        p.deinit();
    }

    // test "OSC 7501: base64 padding is optional"
    #[test]
    fn osc_7501_base64_padding_is_optional() {
        let mut p = parser();

        p.next_slice(b"7501;state=done:msg=UGxhbg");
        let r = report(&mut p);
        assert_eq!(b"Plan".to_vec(), text(&r, OptionKey::Msg));
        p.deinit();
    }

    // test "OSC 7501: whitespace around keys and values"
    #[test]
    fn osc_7501_whitespace_around_keys_and_values() {
        let mut p = parser();

        p.next_slice(b"7501; state = working :\tapp=cargo\t");
        let r = report(&mut p);
        assert_eq!(State::Working, r.state);
        assert_eq!(Some(&b"cargo"[..]), str_opt(&r, OptionKey::App));
        p.deinit();
    }

    // test "OSC 7501: last value wins"
    #[test]
    fn osc_7501_last_value_wins() {
        let mut p = parser();

        p.next_slice(b"7501;state=idle:app=a:state=done:app=b");
        let r = report(&mut p);
        assert_eq!(State::Done, r.state);
        assert_eq!(Some(&b"b"[..]), str_opt(&r, OptionKey::App));
        p.deinit();
    }

    // test "OSC 7501: malformed pairs and unknown keys are skipped"
    #[test]
    fn osc_7501_malformed_pairs_and_unknown_keys_are_skipped() {
        let mut p = parser();

        p.next_slice(b"7501;garbage:=x:Bad=1:app=b@d:future=yes:state=working:app=ok:::");
        let r = report(&mut p);
        assert_eq!(State::Working, r.state);
        assert_eq!(Some(&b"ok"[..]), str_opt(&r, OptionKey::App));
        p.deinit();
    }

    // test "OSC 7501: kind and progress only apply to some states"
    #[test]
    fn osc_7501_kind_and_progress_only_apply_to_some_states() {
        let mut p = parser();

        p.next_slice(b"7501;state=working:kind=auth:progress=10");
        let r = report(&mut p);
        assert_eq!(None, kind_opt(&r));
        assert_eq!(Some(10), progress_opt(&r));

        p.reset();
        p.next_slice(b"7501;state=done:kind=auth:progress=10");
        let r = report(&mut p);
        assert_eq!(None, kind_opt(&r));
        assert_eq!(None, progress_opt(&r));

        p.reset();
        p.next_slice(b"7501;state=blocked:kind=dance");
        let r = report(&mut p);
        assert_eq!(None, kind_opt(&r));
        p.deinit();
    }

    // test "OSC 7501: out of range progress is absent"
    #[test]
    fn osc_7501_out_of_range_progress_is_absent() {
        let mut p = parser();

        let cases: [(&[u8], Option<u8>); 11] = [
            (b"0", Some(0)),
            (b"100", Some(100)),
            (b"101", None),
            (b"256", None),
            (b"99999999999999999999", None),
            (b"-1", None),
            (b"+1", None),
            (b"4.5", None),
            (b"1_0", None),
            (b"007", Some(7)),
            (b"", None),
        ];

        for (input, want) in cases {
            p.reset();
            p.next_slice(b"7501;state=working:progress=");
            p.next_slice(input);
            let r = report(&mut p);
            assert_eq!(want, progress_opt(&r), "progress={input:?}");
        }
        p.deinit();
    }

    // test "OSC 7501: invalid app is ignored"
    #[test]
    fn osc_7501_invalid_app_is_ignored() {
        let mut p = parser();

        p.next_slice(b"7501;state=idle:app=a/b");
        let r = report(&mut p);
        assert_eq!(None, str_opt(&r, OptionKey::App));
        p.deinit();
    }

    // test "OSC 7501: ids"
    #[test]
    fn osc_7501_ids() {
        let mut p = parser();

        let max_segment = "a".repeat(MAX_ID_SEGMENT_BYTES);
        let valid = [
            "a".to_string(),
            "build/test".to_string(),
            "A-Z_0.9+".to_string(),
            max_segment.clone(),
            "a/b/c/d/e/f/g/h".to_string(),
        ];
        for id in &valid {
            p.reset();
            p.next_slice(b"7501;state=clear:id=");
            p.next_slice(id.as_bytes());
            let r = report(&mut p);
            assert_eq!(Some(id.as_bytes()), str_opt(&r, OptionKey::Id));
        }

        // An invalid id discards the report so that it can never fall back
        // to the root record.
        let invalid = [
            String::new(),
            "/".to_string(),
            "a/".to_string(),
            "/a".to_string(),
            "a//b".to_string(),
            "a,b".to_string(),
            "a=b".to_string(),
            format!("{max_segment}a"),
            "a/b/c/d/e/f/g/h/i".to_string(),
            format!("{}a", format!("{}/", "a".repeat(31)).repeat(4)),
        ];
        for id in &invalid {
            p.reset();
            p.next_slice(b"7501;state=clear:id=");
            p.next_slice(id.as_bytes());
            assert_eq!(None, p.end(Some(0x1b)), "id={id:?}");
        }
        p.deinit();
    }

    // test "OSC 7501: text must decode to safe UTF-8"
    #[test]
    fn osc_7501_text_must_decode_to_safe_utf8() {
        let mut p = parser();

        let invalid: [&[u8]; 6] = [
            // Not base64.
            b"7501;state=done:msg=a",
            b"7501;state=done:msg=QQ=",
            b"7501;state=done:title=a",
            // "a\nb", a control character.
            b"7501;state=done:msg=YQpi",
            // "\x9b", a C1 control character as UTF-8.
            b"7501;state=done:msg=wps=",
            // Invalid UTF-8.
            b"7501;state=done:msg=/w==",
        ];
        for input in invalid {
            p.reset();
            p.next_slice(input);
            assert_eq!(None, p.end(Some(0x1b)), "input={input:?}");
        }

        // Unicode is fine. "安全"
        p.reset();
        p.next_slice(b"7501;state=done:msg=5a6J5YWo");
        let r = report(&mut p);
        assert_eq!("安全".as_bytes().to_vec(), text(&r, OptionKey::Msg));
        p.deinit();
    }

    // test "OSC 7501: empty text is absent"
    #[test]
    fn osc_7501_empty_text_is_absent() {
        let mut p = parser();

        p.next_slice(b"7501;state=done:msg=:title=");
        let r = report(&mut p);
        assert_eq!(None, str_opt(&r, OptionKey::Msg));
        assert_eq!(None, str_opt(&r, OptionKey::Title));
        p.deinit();
    }

    // test "OSC 7501: discarded reports"
    #[test]
    fn osc_7501_discarded_reports() {
        let mut p = parser();

        // 192 bytes of title and 2048 bytes of message are the most allowed.
        // "QUFB" decodes to "AAA" and "QUE" to "AA".
        let title = "QUFB".repeat(MAX_TITLE_ENCODED_BYTES / 4);
        let msg = format!("{}QUE", "QUFB".repeat(MAX_MSG_BYTES / 3));
        let fill = MAX_SEQUENCE_BYTES - b"\x1b]7501;state=idle:".len() - b"\x1b\\".len();

        let cases: Vec<(String, bool)> = vec![
            // A missing or unknown state.
            ("7501;".into(), false),
            ("7501;app=cargo".into(), false),
            ("7501;state=sleeping".into(), false),
            ("7501;state=".into(), false),
            ("7501;state=IDLE".into(), false),
            // A key that is too long, even one we don't know.
            (
                format!("7501;state=idle:{}=1", "a".repeat(MAX_KEY_BYTES)),
                true,
            ),
            (
                format!("7501;state=idle:{}=1", "a".repeat(MAX_KEY_BYTES + 1)),
                false,
            ),
            // An app that is too long, even if a later pair replaces it.
            (
                format!("7501;state=idle:app={}", "a".repeat(MAX_APP_BYTES + 1)),
                false,
            ),
            (
                format!(
                    "7501;state=idle:app={}:app=ok",
                    "a".repeat(MAX_APP_BYTES + 1)
                ),
                false,
            ),
            // Text at its limits and one step over.
            (format!("7501;state=idle:title={title}"), true),
            (format!("7501;state=idle:title={title}QUFB"), false),
            (format!("7501;state=idle:msg={msg}"), true),
            (
                format!(
                    "7501;state=idle:msg={}",
                    "QUFB".repeat(MAX_MSG_BYTES / 3 + 1)
                ),
                false,
            ),
            // The whole sequence, including the terminator.
            (format!("7501;state=idle:{}", "x".repeat(fill)), true),
            (format!("7501;state=idle:{}", "x".repeat(fill + 1)), false),
            // Numbers that only start like 7501.
            ("75;state=idle".into(), false),
            ("750;state=idle".into(), false),
            ("75010;state=idle".into(), false),
            ("7501".into(), false),
        ];
        for (input, want) in &cases {
            p.reset();
            p.next_slice(input.as_bytes());
            assert_eq!(
                *want,
                p.end(Some(0x1b)).is_some(),
                "input len {}",
                input.len()
            );
        }
        p.deinit();
    }

    // test "OSC 7501: no allocator"
    #[test]
    fn osc_7501_no_allocator() {
        let mut p = Parser::init(None);

        p.next_slice(b"7501;state=working:app=cargo");
        let r = report(&mut p);
        assert_eq!(State::Working, r.state);
        assert_eq!(Some(&b"cargo"[..]), str_opt(&r, OptionKey::App));
        p.deinit();
    }

    // Rust-only: the two log sites, verbatim.
    #[test]
    fn osc_7501_log_sites() {
        let mut p = parser();

        let cap = crate::log_capture::start();
        p.next_slice(b"7501;state=sleeping");
        assert!(p.end(Some(0x1b)).is_none());
        p.reset();
        p.next_slice(b"7501;state=idle:");
        p.next_slice("x".repeat(MAX_BODY_BYTES).as_bytes());
        assert!(p.end(Some(0x1b)).is_none());
        let messages = cap.messages();
        drop(cap);
        let want = |m: &str| {
            (
                log::Level::Warn,
                "osc_program_status".to_string(),
                m.to_string(),
            )
        };
        assert_eq!(
            vec![
                want("invalid OSC 7501 report err=InvalidState"),
                want(&format!(
                    "OSC 7501 sequence too long len={}",
                    "state=idle:".len() + MAX_BODY_BYTES
                )),
            ],
            messages
        );
        p.deinit();
    }

    // Rust-only: the allocating capture is freed (upstream's
    // `testing.allocator` detects a leak).
    #[test]
    fn osc_7501_allocating_capture_is_balanced() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.next_slice(b"7501;state=working:app=cargo");
            let r = report(&mut p);
            assert_eq!(State::Working, r.state);
            p.reset();
            p.next_slice(b"7501;state=idle:bad");
            assert!(p.end(Some(0x1b)).is_some());
            p.deinit();
        });
        assert!(counting.alloc_calls.get() > 0);
        assert_eq!(0, counting.live_bytes.get());
    }
}
