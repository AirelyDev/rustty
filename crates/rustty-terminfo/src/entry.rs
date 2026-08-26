//! rustty's terminfo entry (ported from ghostty `src/terminfo/ghostty.zig`).
//!
//! R2: the terminal *names* carry the rustty product identity, sourced from the
//! single-source-of-truth `rustty-identity` crate (`xterm-rustty`, `rustty`,
//! `Rustty`), or the reference values under the `identity-oracle` parity
//! harness feature. The `xterm-*` compat name (an app-facing compatibility id,
//! NOT an internal identifier) is the `rustty-identity::TERMINFO_PRIMARY_NAME`
//! constant; see the HACK comment on it below, ported faithfully from the
//! reference. The capability table is identity-agnostic and ported verbatim.

use crate::source::{Capability, Source, Value};

use std::sync::OnceLock;

/// rustty's terminfo entry.
///
/// The reference is a comptime `const ghostty: Source`; the faithful Rust port
/// is a function building the same `Source` value (the names come from
/// `rustty-identity` per R2, the capabilities are ported verbatim).
pub fn rustty() -> Source {
    Source {
        names: vec![
            // We support the "xterm-" prefix because some poorly behaved programs
            // use this to detect if the terminal supports 256 colors and other
            // features.
            // HACK: This is a hack on a hack...we use "xterm-rustty" to prevent
            // vim from breaking, and when we do this as the default we break
            // tcell-based applications (lazygit, aerc, etc). tcell has a bug where
            // the primary terminfo name must be the value of TERM.
            // https://github.com/gdamore/tcell/pull/639 fixes the issue but is not
            // merged yet. Consider switching these in the future.
            rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes().to_vec(),
            // The preferred name
            rustty_identity::TERMINFO_PREFERRED_NAME.as_bytes().to_vec(),
            // Our "formal" name (the capitalized product name). Mirrors the
            // reference `"Ghostty"`; under `identity-oracle` it is the
            // reference value. Sourced from `rustty-identity` (R2), never a
            // local literal.
            rustty_identity::APP_NAME_TITLECASE.as_bytes().to_vec(),
        ],

        // NOTE: These capabilities are super underdocumented and I'm not 100%
        // I've got the list or my understanding of any in this list fully correct.
        // As we learn more, please update the comments to better explain what
        // anything means.
        //
        // I've marked some capabilities as "???" if I don't understand what they
        // mean but I just assume I support since other modern terminals do. In
        // this case, I'd love if anyone could help explain what this means and
        // verify that rustty does indeed support it and if not we can fix it.
        capabilities: vec![
            // automatic right margin -- when reaching the end of a line, text is
            // wrapped to the next line.
            boolean(b"am"),
            // background color erase -- screen is erased with the background color
            boolean(b"bce"),
            // terminal can change color definitions, i.e. we can change the color
            // palette.
            boolean(b"ccc"),
            // supports changing the window title.
            boolean(b"hs"),
            // terminal has a meta key
            boolean(b"km"),
            // terminal will not echo input on the screen on its own
            boolean(b"mc5i"),
            // safe to move (move what?) while in insert/standout mode. (???)
            boolean(b"mir"),
            boolean(b"msgr"),
            // no pad character (???)
            boolean(b"npc"),
            // newline ignored after 80 cols (???)
            boolean(b"xenl"),
            // Terminal supports default colors
            boolean(b"AX"),
            // Tmux "truecolor" mode. Other programs also use this to detect
            // if the terminal supports "truecolor". This means that the terminal
            // can display 24-bit RGB colors.
            boolean(b"Tc"),
            // Colored underlines. https://sw.kovidgoyal.net/kitty/underlines/
            boolean(b"Su"),
            // Terminal supports a number of xterm extensions
            boolean(b"XT"),
            // Full keyboard support using Kitty's keyboard protocol:
            // https://sw.kovidgoyal.net/kitty/keyboard-protocol/
            boolean(b"fullkbd"),
            // Number of colors in the color palette.
            numeric(b"colors", 256),
            // Number of columns in a line. Our terminal is variable width on
            // Window resize but this appears to just be the value set by most
            // terminals.
            numeric(b"cols", 80),
            // Initial tabstop interval.
            numeric(b"it", 8),
            // Number of lines on a page. Similar to cols this is variable width
            // but this appears to be the value set by most terminals.
            numeric(b"lines", 24),
            // Number of color pairs on the screen.
            numeric(b"pairs", 32767),
            // Alternate character set. This is the VT100 alternate character set.
            // I don't know what the value means, I copied this from Kitty and
            // verified with some other terminals (looks similar).
            string(
                b"acsc",
                b"++\\,\\,--..00``aaffgghhiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~",
            ),
            // Curly, dashed, etc underlines
            string(b"Smulx", b"\\E[4:%p1%dm"),
            // Overline
            string(b"Smol", b"\\E[53m"),
            string(b"Rmol", b"\\E[55m"),
            // Colored underlines
            string(
                b"Setulc",
                b"\\E[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%d%;m",
            ),
            // Cursor styles
            string(b"Ss", b"\\E[%p1%d q"),
            // Cursor style reset (to user configured default)
            string(b"Se", b"\\E[0 q"),
            // OSC 52 Clipboard
            string(b"Ms", b"\\E]52;%p1%s;%p2%s\\007"),
            // Synchronized output
            string(b"Sync", b"\\E[?2026%?%p1%{1}%-%tl%eh%;"),
            // Bracketed paste mode
            string(b"BD", b"\\E[?2004l"),
            string(b"BE", b"\\E[?2004h"),
            // Bracketed paste start/end
            string(b"PS", b"\\E[200~"),
            string(b"PE", b"\\E[201~"),
            // Mouse
            string(b"XM", b"\\E[?1006;1000%?%p1%{1}%=%th%el%;"),
            string(b"xm", b"\\E[<%i%p3%d;%p1%d;%p2%d;%?%p4%tM%em%;"),
            // Secondary device attributes request / response
            string(b"RV", b"\\E[>c"),
            string(b"rv", b"\\E\\\\[[0-9]+;[0-9]+;[0-9]+c"),
            // XTVERSION
            string(b"XR", b"\\E[>0q"),
            string(b"xr", b"\\EP>\\\\|[ -~]+a\\E\\\\"),
            // DECSLRM (Left/Right Margins)
            string(b"Enmg", b"\\E[?69h"),
            string(b"Dsmg", b"\\E[?69l"),
            string(b"Clmg", b"\\E[s"),
            string(b"Cmg", b"\\E[%i%p1%d;%p2%ds"),
            // Clear screen. E3 is the extension to clear scrollback
            string(b"clear", b"\\E[H\\E[2J"),
            string(b"E3", b"\\E[3J"),
            // Focus reporting. Introduced in ncurses 6.4-20231028
            string(b"fe", b"\\E[?1004h"),
            string(b"fd", b"\\E[?1004l"),
            string(b"kxIN", b"\\E[I"),
            string(b"kxOUT", b"\\E[O"),
            // These are all capabilities that should be pretty straightforward
            // and map to input sequences.
            string(b"bel", b"^G"),
            string(b"blink", b"\\E[5m"),
            string(b"bold", b"\\E[1m"),
            string(b"cbt", b"\\E[Z"),
            string(b"civis", b"\\E[?25l"),
            string(b"cnorm", b"\\E[?12l\\E[?25h"),
            string(b"cr", b"\\r"),
            string(b"csr", b"\\E[%i%p1%d;%p2%dr"),
            string(b"cub", b"\\E[%p1%dD"),
            string(b"cub1", b"^H"),
            string(b"cud", b"\\E[%p1%dB"),
            string(b"cud1", b"^J"),
            string(b"cuf", b"\\E[%p1%dC"),
            string(b"cuf1", b"\\E[C"),
            string(b"cup", b"\\E[%i%p1%d;%p2%dH"),
            string(b"cuu", b"\\E[%p1%dA"),
            string(b"cuu1", b"\\E[A"),
            string(b"cvvis", b"\\E[?12;25h"),
            string(b"dch", b"\\E[%p1%dP"),
            string(b"dch1", b"\\E[P"),
            string(b"dim", b"\\E[2m"),
            string(b"dl", b"\\E[%p1%dM"),
            string(b"dl1", b"\\E[M"),
            string(b"dsl", b"\\E]2;\\007"),
            string(b"ech", b"\\E[%p1%dX"),
            string(b"ed", b"\\E[J"),
            string(b"el", b"\\E[K"),
            string(b"el1", b"\\E[1K"),
            string(b"flash", b"\\E[?5h$<100/>\\E[?5l"),
            string(b"fsl", b"^G"),
            string(b"home", b"\\E[H"),
            string(b"hpa", b"\\E[%i%p1%dG"),
            string(b"ht", b"^I"),
            string(b"hts", b"\\EH"),
            string(b"ich", b"\\E[%p1%d@"),
            string(b"ich1", b"\\E[@"),
            string(b"il", b"\\E[%p1%dL"),
            string(b"il1", b"\\E[L"),
            string(b"ind", b"\\n"),
            string(b"indn", b"\\E[%p1%dS"),
            string(
                b"initc",
                b"\\E]4;%p1%d;rgb\\:%p2%{255}%*%{1000}%/%2.2X/%p3%{255}%*%{1000}%/%2.2X/%p4%{255}%*%{1000}%/%2.2X\\E\\\\",
            ),
            string(b"invis", b"\\E[8m"),
            string(b"oc", b"\\E]104\\007"),
            string(b"op", b"\\E[39;49m"),
            string(b"rc", b"\\E8"),
            string(b"rep", b"%p1%c\\E[%p2%{1}%-%db"),
            string(b"rev", b"\\E[7m"),
            string(b"ri", b"\\EM"),
            string(b"rin", b"\\E[%p1%dT"),
            string(b"ritm", b"\\E[23m"),
            string(b"rmacs", b"\\E(B"),
            string(b"rmam", b"\\E[?7l"),
            string(b"rmcup", b"\\E[?1049l"),
            string(b"rmir", b"\\E[4l"),
            string(b"rmkx", b"\\E[?1l\\E>"),
            string(b"rmso", b"\\E[27m"),
            string(b"rmul", b"\\E[24m"),
            string(b"rmxx", b"\\E[29m"),
            string(
                b"setab",
                b"\\E[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m",
            ),
            string(
                b"setaf",
                b"\\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
            ),
            string(b"setrgbb", b"\\E[48:2:%p1%d:%p2%d:%p3%dm"),
            string(b"setrgbf", b"\\E[38:2:%p1%d:%p2%d:%p3%dm"),
            string(
                b"sgr",
                b"%?%p9%t\\E(0%e\\E(B%;\\E[0%?%p6%t;1%;%?%p5%t;2%;%?%p2%t;4%;%?%p1%p3%|%t;7%;%?%p4%t;5%;%?%p7%t;8%;m",
            ),
            string(b"sgr0", b"\\E(B\\E[m"),
            string(b"sitm", b"\\E[3m"),
            string(b"smacs", b"\\E(0"),
            string(b"smam", b"\\E[?7h"),
            string(b"smcup", b"\\E[?1049h"),
            string(b"smir", b"\\E[4h"),
            string(b"smkx", b"\\E[?1h\\E="),
            string(b"smso", b"\\E[7m"),
            string(b"smul", b"\\E[4m"),
            string(b"smxx", b"\\E[9m"),
            string(b"tbc", b"\\E[3g"),
            string(b"tsl", b"\\E]2;"),
            string(b"u6", b"\\E[%i%d;%dR"),
            string(b"u7", b"\\E[6n"),
            string(b"u8", b"\\E[?%[;0123456789]c"),
            string(b"u9", b"\\E[c"),
            string(b"vpa", b"\\E[%i%p1%dd"),
            //-----------------------------------------------------------
            // Completely unvalidated entries that are blindly copied from
            // other terminals (Kitty, Wezterm, Alacritty) and may or may not
            // actually work with rustty. todo is to validate these!
            string(b"kDC", b"\\E[3;2~"),
            string(b"kDC3", b"\\E[3;3~"),
            string(b"kDC4", b"\\E[3;4~"),
            string(b"kDC5", b"\\E[3;5~"),
            string(b"kDC6", b"\\E[3;6~"),
            string(b"kDC7", b"\\E[3;7~"),
            string(b"kDN", b"\\E[1;2B"),
            string(b"kDN3", b"\\E[1;3B"),
            string(b"kDN4", b"\\E[1;4B"),
            string(b"kDN5", b"\\E[1;5B"),
            string(b"kDN6", b"\\E[1;6B"),
            string(b"kDN7", b"\\E[1;7B"),
            string(b"kEND", b"\\E[1;2F"),
            string(b"kEND3", b"\\E[1;3F"),
            string(b"kEND4", b"\\E[1;4F"),
            string(b"kEND5", b"\\E[1;5F"),
            string(b"kEND6", b"\\E[1;6F"),
            string(b"kEND7", b"\\E[1;7F"),
            string(b"kHOM", b"\\E[1;2H"),
            string(b"kHOM3", b"\\E[1;3H"),
            string(b"kHOM4", b"\\E[1;4H"),
            string(b"kHOM5", b"\\E[1;5H"),
            string(b"kHOM6", b"\\E[1;6H"),
            string(b"kHOM7", b"\\E[1;7H"),
            string(b"kIC", b"\\E[2;2~"),
            string(b"kIC3", b"\\E[2;3~"),
            string(b"kIC4", b"\\E[2;4~"),
            string(b"kIC5", b"\\E[2;5~"),
            string(b"kIC6", b"\\E[2;6~"),
            string(b"kIC7", b"\\E[2;7~"),
            string(b"kLFT", b"\\E[1;2D"),
            string(b"kLFT3", b"\\E[1;3D"),
            string(b"kLFT4", b"\\E[1;4D"),
            string(b"kLFT5", b"\\E[1;5D"),
            string(b"kLFT6", b"\\E[1;6D"),
            string(b"kLFT7", b"\\E[1;7D"),
            string(b"kNXT", b"\\E[6;2~"),
            string(b"kNXT3", b"\\E[6;3~"),
            string(b"kNXT4", b"\\E[6;4~"),
            string(b"kNXT5", b"\\E[6;5~"),
            string(b"kNXT6", b"\\E[6;6~"),
            string(b"kNXT7", b"\\E[6;7~"),
            string(b"kPRV", b"\\E[5;2~"),
            string(b"kPRV3", b"\\E[5;3~"),
            string(b"kPRV4", b"\\E[5;4~"),
            string(b"kPRV5", b"\\E[5;5~"),
            string(b"kPRV6", b"\\E[5;6~"),
            string(b"kPRV7", b"\\E[5;7~"),
            string(b"kRIT", b"\\E[1;2C"),
            string(b"kRIT3", b"\\E[1;3C"),
            string(b"kRIT4", b"\\E[1;4C"),
            string(b"kRIT5", b"\\E[1;5C"),
            string(b"kRIT6", b"\\E[1;6C"),
            string(b"kRIT7", b"\\E[1;7C"),
            string(b"kUP", b"\\E[1;2A"),
            string(b"kUP3", b"\\E[1;3A"),
            string(b"kUP4", b"\\E[1;4A"),
            string(b"kUP5", b"\\E[1;5A"),
            string(b"kUP6", b"\\E[1;6A"),
            string(b"kUP7", b"\\E[1;7A"),
            string(b"kbs", b"^?"),
            string(b"kcbt", b"\\E[Z"),
            string(b"kcub1", b"\\EOD"),
            string(b"kcud1", b"\\EOB"),
            string(b"kcuf1", b"\\EOC"),
            string(b"kcuu1", b"\\EOA"),
            string(b"kdch1", b"\\E[3~"),
            string(b"kend", b"\\EOF"),
            string(b"kent", b"\\EOM"),
            string(b"kf1", b"\\EOP"),
            string(b"kf10", b"\\E[21~"),
            string(b"kf11", b"\\E[23~"),
            string(b"kf12", b"\\E[24~"),
            string(b"kf13", b"\\E[1;2P"),
            string(b"kf14", b"\\E[1;2Q"),
            string(b"kf15", b"\\E[1;2R"),
            string(b"kf16", b"\\E[1;2S"),
            string(b"kf17", b"\\E[15;2~"),
            string(b"kf18", b"\\E[17;2~"),
            string(b"kf19", b"\\E[18;2~"),
            string(b"kf2", b"\\EOQ"),
            string(b"kf20", b"\\E[19;2~"),
            string(b"kf21", b"\\E[20;2~"),
            string(b"kf22", b"\\E[21;2~"),
            string(b"kf23", b"\\E[23;2~"),
            string(b"kf24", b"\\E[24;2~"),
            string(b"kf25", b"\\E[1;5P"),
            string(b"kf26", b"\\E[1;5Q"),
            string(b"kf27", b"\\E[1;5R"),
            string(b"kf28", b"\\E[1;5S"),
            string(b"kf29", b"\\E[15;5~"),
            string(b"kf3", b"\\EOR"),
            string(b"kf30", b"\\E[17;5~"),
            string(b"kf31", b"\\E[18;5~"),
            string(b"kf32", b"\\E[19;5~"),
            string(b"kf33", b"\\E[20;5~"),
            string(b"kf34", b"\\E[21;5~"),
            string(b"kf35", b"\\E[23;5~"),
            string(b"kf36", b"\\E[24;5~"),
            string(b"kf37", b"\\E[1;6P"),
            string(b"kf38", b"\\E[1;6Q"),
            string(b"kf39", b"\\E[1;6R"),
            string(b"kf4", b"\\EOS"),
            string(b"kf40", b"\\E[1;6S"),
            string(b"kf41", b"\\E[15;6~"),
            string(b"kf42", b"\\E[17;6~"),
            string(b"kf43", b"\\E[18;6~"),
            string(b"kf44", b"\\E[19;6~"),
            string(b"kf45", b"\\E[20;6~"),
            string(b"kf46", b"\\E[21;6~"),
            string(b"kf47", b"\\E[23;6~"),
            string(b"kf48", b"\\E[24;6~"),
            string(b"kf49", b"\\E[1;3P"),
            string(b"kf5", b"\\E[15~"),
            string(b"kf50", b"\\E[1;3Q"),
            string(b"kf51", b"\\E[1;3R"),
            string(b"kf52", b"\\E[1;3S"),
            string(b"kf53", b"\\E[15;3~"),
            string(b"kf54", b"\\E[17;3~"),
            string(b"kf55", b"\\E[18;3~"),
            string(b"kf56", b"\\E[19;3~"),
            string(b"kf57", b"\\E[20;3~"),
            string(b"kf58", b"\\E[21;3~"),
            string(b"kf59", b"\\E[23;3~"),
            string(b"kf6", b"\\E[17~"),
            string(b"kf60", b"\\E[24;3~"),
            string(b"kf61", b"\\E[1;4P"),
            string(b"kf62", b"\\E[1;4Q"),
            string(b"kf63", b"\\E[1;4R"),
            string(b"kf7", b"\\E[18~"),
            string(b"kf8", b"\\E[19~"),
            string(b"kf9", b"\\E[20~"),
            string(b"khome", b"\\EOH"),
            string(b"kich1", b"\\E[2~"),
            string(b"kind", b"\\E[1;2B"),
            string(b"kmous", b"\\E[<"),
            string(b"knp", b"\\E[6~"),
            string(b"kpp", b"\\E[5~"),
            string(b"kri", b"\\E[1;2A"),
            string(b"rs1", b"\\E]\\E\\\\\\Ec"),
            string(b"sc", b"\\E7"),
        ],
    }
}

/// A content-derived version of the encoded terminfo source.
// pub const version = version: {
//     @setEvalBranchQuota(100_000);
//     var hashing: std.Io.Writer.Hashing(std.hash.Wyhash) =
//         .initHasher(.init(0), &.{});
//     ghostty.encode(&hashing.writer) catch unreachable;
//     break :version std.fmt.comptimePrint("{x}", .{hashing.hasher.final()});
// };
//
// PORT NOTE: the reference computes this at comptime over the comptime `ghostty`
// Source. The port's [`rustty()`] builds the Source at runtime (its names come
// from `rustty-identity`, R2), so `version` is computed once at first use with
// the same value semantics: a deterministic Wyhash(0) of the RUSTTY-encoded
// source bytes, hex-formatted. It is content-derived, never hardcoded — under
// `identity-oracle` it hashes the reference's encoded bytes automatically (the
// identity fork flips the encoded names). The hash algorithm below is a
// faithful port of Zig `std.hash.Wyhash`; strictly, self-consistency would
// suffice (the version is only ever compared to itself: a cache entry against
// the current build), but algorithm parity keeps the value aligned with the
// reference on identical bytes.
pub fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        // ghostty.encode(&hashing.writer) catch unreachable;
        //   (encoding into a Vec cannot fail)
        let mut buf: Vec<u8> = Vec::new();
        rustty().encode(&mut buf).unwrap();
        // std.fmt.comptimePrint("{x}", .{hashing.hasher.final()})
        format!("{:x}", wyhash::hash(0, &buf))
    })
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shim — a faithful port of Zig `std.hash.Wyhash`'s one-shot
// `hash(seed, input)` (lib/std/hash/wyhash.zig), used only by [`version`]. The
// reference streams the encoded bytes through `std.Io.Writer.Hashing`; hashing
// the complete buffer one-shot is equivalent (Zig's own "iterative api" test
// asserts streaming == one-shot). Only the one-shot path is ported. All
// arithmetic mirrors the reference exactly: the `mum` multiply is Zig's `*%`
// (wrapping u128), everything else is bounded (R7).
// ---------------------------------------------------------------------------
mod wyhash {
    // const secret = [_]u64{ ... };
    const SECRET: [u64; 4] = [
        0xa0761d6478bd642f,
        0xe7037ed1a0b428db,
        0x8ebc6af09c88c6e3,
        0x589965cc75374cc3,
    ];

    // a: u64, b: u64, state: [3]u64, total_len: usize,
    //   (the streaming `buf`/`buf_len` fields are not ported — one-shot only)
    struct Wyhash {
        a: u64,
        b: u64,
        state: [u64; 3],
        total_len: usize,
    }

    // inline fn read(comptime bytes: usize, data: []const u8) u64 — the two
    // instantiations used (4- and 8-byte little-endian reads).
    #[inline]
    fn read4(data: &[u8]) -> u64 {
        u64::from(u32::from_le_bytes(data[0..4].try_into().unwrap()))
    }

    #[inline]
    fn read8(data: &[u8]) -> u64 {
        u64::from_le_bytes(data[0..8].try_into().unwrap())
    }

    // inline fn mum(a: *u64, b: *u64) void {
    #[inline]
    fn mum(a: &mut u64, b: &mut u64) {
        // const x = @as(u128, a.*) *% b.*;
        let x = u128::from(*a).wrapping_mul(u128::from(*b));
        // a.* = @as(u64, @truncate(x));
        *a = x as u64;
        // b.* = @as(u64, @truncate(x >> 64));
        *b = (x >> 64) as u64;
    }

    // inline fn mix(a_: u64, b_: u64) u64 {
    #[inline]
    fn mix(a_: u64, b_: u64) -> u64 {
        let mut a = a_;
        let mut b = b_;
        mum(&mut a, &mut b);
        a ^ b
    }

    impl Wyhash {
        // pub fn init(seed: u64) Wyhash {
        fn init(seed: u64) -> Wyhash {
            // self.state[0] = seed ^ mix(seed ^ secret[0], secret[1]);
            // self.state[1] = self.state[0];
            // self.state[2] = self.state[0];
            let s0 = seed ^ mix(seed ^ SECRET[0], SECRET[1]);
            Wyhash {
                a: 0,
                b: 0,
                state: [s0; 3],
                total_len: 0,
            }
        }

        // inline fn smallKey(self: *Wyhash, input: []const u8) void {
        fn small_key(&mut self, input: &[u8]) {
            // std.debug.assert(input.len <= 16);
            debug_assert!(input.len() <= 16);

            if input.len() >= 4 {
                // const end = input.len - 4;
                let end = input.len() - 4;
                // const quarter = (input.len >> 3) << 2;
                let quarter = (input.len() >> 3) << 2;
                // self.a = (read(4, input[0..]) << 32) | read(4, input[quarter..]);
                self.a = (read4(&input[0..]) << 32) | read4(&input[quarter..]);
                // self.b = (read(4, input[end..]) << 32) | read(4, input[end - quarter ..]);
                self.b = (read4(&input[end..]) << 32) | read4(&input[end - quarter..]);
            } else if !input.is_empty() {
                // self.a = (@as(u64, input[0]) << 16) | (@as(u64, input[input.len >> 1]) << 8) | input[input.len - 1];
                self.a = (u64::from(input[0]) << 16)
                    | (u64::from(input[input.len() >> 1]) << 8)
                    | u64::from(input[input.len() - 1]);
                self.b = 0;
            } else {
                self.a = 0;
                self.b = 0;
            }
        }

        // inline fn round(self: *Wyhash, input: *const [48]u8) void {
        fn round(&mut self, input: &[u8; 48]) {
            // inline for (0..3) |i| {
            for i in 0..3 {
                // const a = read(8, input[8 * (2 * i) ..]);
                let a = read8(&input[8 * (2 * i)..]);
                // const b = read(8, input[8 * (2 * i + 1) ..]);
                let b = read8(&input[8 * (2 * i + 1)..]);
                // self.state[i] = mix(a ^ secret[i + 1], b ^ self.state[i]);
                self.state[i] = mix(a ^ SECRET[i + 1], b ^ self.state[i]);
            }
        }

        // inline fn final0(self: *Wyhash) void {
        fn final0(&mut self) {
            self.state[0] ^= self.state[1] ^ self.state[2];
        }

        // input_lb must be at least 16-bytes long (in shorter key cases the
        // smallKey function will be used instead).
        // inline fn final1(self: *Wyhash, input_lb: []const u8, start_pos: usize) void {
        fn final1(&mut self, input_lb: &[u8], start_pos: usize) {
            debug_assert!(input_lb.len() >= 16);
            debug_assert!(input_lb.len() - start_pos <= 48);
            let input = &input_lb[start_pos..];

            // var i: usize = 0;
            // while (i + 16 < input.len) : (i += 16) { ... }
            let mut i: usize = 0;
            while i + 16 < input.len() {
                self.state[0] = mix(
                    read8(&input[i..]) ^ SECRET[1],
                    read8(&input[i + 8..]) ^ self.state[0],
                );
                i += 16;
            }

            // self.a = read(8, input_lb[input_lb.len - 16 ..][0..8]);
            self.a = read8(&input_lb[input_lb.len() - 16..]);
            // self.b = read(8, input_lb[input_lb.len - 8 ..][0..8]);
            self.b = read8(&input_lb[input_lb.len() - 8..]);
        }

        // inline fn final2(self: *Wyhash) u64 {
        fn final2(&mut self) -> u64 {
            self.a ^= SECRET[1];
            self.b ^= self.state[0];
            mum(&mut self.a, &mut self.b);
            // return mix(self.a ^ secret[0] ^ self.total_len, self.b ^ secret[1]);
            mix(
                self.a ^ SECRET[0] ^ (self.total_len as u64),
                self.b ^ SECRET[1],
            )
        }
    }

    // pub fn hash(seed: u64, input: []const u8) u64 {
    pub fn hash(seed: u64, input: &[u8]) -> u64 {
        // var self = Wyhash.init(seed);
        let mut state = Wyhash::init(seed);

        if input.len() <= 16 {
            state.small_key(input);
        } else {
            // var i: usize = 0;
            let mut i: usize = 0;
            if input.len() >= 48 {
                // while (i + 48 < input.len) : (i += 48) { self.round(input[i..][0..48]); }
                while i + 48 < input.len() {
                    state.round(input[i..i + 48].try_into().unwrap());
                    i += 48;
                }
                state.final0();
            }
            state.final1(input, i);
        }

        // self.total_len = input.len;
        state.total_len = input.len();
        // return self.final2();
        state.final2()
    }
}

// --- helpers ---------------------------------------------------------------
//
// The reference uses Zig anonymous-struct literals
// (`.{ .name = "am", .value = .{ .boolean = {} } }`); these small constructors
// keep the table faithful and readable. They add nothing, omit nothing (R1).

fn boolean(name: &[u8]) -> Capability {
    Capability {
        name: name.to_vec(),
        value: Value::Boolean,
    }
}

fn numeric(name: &[u8], v: u32) -> Capability {
    Capability {
        name: name.to_vec(),
        value: Value::Numeric(v),
    }
}

fn string(name: &[u8], v: &[u8]) -> Capability {
    Capability {
        name: name.to_vec(),
        value: Value::String(v.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE (shim verification, not a ghostty test): the official wyhash
    // test vectors from Zig `lib/std/hash/wyhash.zig` `test "test vectors"`,
    // proving the `wyhash` shim above matches `std.hash.Wyhash` byte-for-byte
    // (they cover the smallKey, final1-only, and round/final0 paths).
    #[test]
    fn wyhash_test_vectors() {
        const VECTORS: [(u64, u64, &[u8]); 7] = [
            (0, 0x409638ee2bde459, b""),
            (1, 0xa8412d091b5fe0a9, b"a"),
            (2, 0x32dd92e4b2915153, b"abc"),
            (3, 0x8619124089a3a16b, b"message digest"),
            (4, 0x7a43afb61d7f5f40, b"abcdefghijklmnopqrstuvwxyz"),
            (
                5,
                0xff42329b90e50d58,
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            ),
            (
                6,
                0xc39cab13b115aad3,
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            ),
        ];
        for (seed, expected, input) in VECTORS {
            assert_eq!(expected, wyhash::hash(seed, input));
        }
    }

    // Negative control for `version` (assessment P4-E4): the value is
    // content-derived — hashing the actual encoded bytes must reproduce it,
    // and hashing mutated bytes must not.
    #[test]
    fn version_is_content_derived() {
        let mut buf: Vec<u8> = Vec::new();
        rustty().encode(&mut buf).unwrap();
        assert_eq!(version(), format!("{:x}", wyhash::hash(0, &buf)));

        // Mutate the encoded source: the version must change.
        buf.push(b'X');
        assert_ne!(version(), format!("{:x}", wyhash::hash(0, &buf)));
    }

    #[test]
    fn encode() {
        // Encode
        let mut buf: Vec<u8> = Vec::new();
        rustty().encode(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    /// Task 12: the REAL (non-test-fixture) name-generation site — this
    /// `rustty()` function — must build every name from `rustty-identity`,
    /// never a local literal. Under the default profile this must produce
    /// the rustty product identity: primary name `xterm-rustty` (must equal
    /// `rustty_identity::TERM`, the ported tcell-primary-name-must-equal-TERM
    /// hack), preferred name `rustty`, and formal/title-case name `Rustty`.
    #[cfg(not(feature = "identity-oracle"))]
    #[test]
    fn names_are_rustty_under_default_profile() {
        let src = rustty();
        assert_eq!(
            src.names,
            vec![
                b"xterm-rustty".to_vec(),
                b"rustty".to_vec(),
                b"Rustty".to_vec(),
            ]
        );
        // Cross-check directly against the identity consts (not re-literalized).
        assert_eq!(
            src.names[0],
            rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes()
        );
        assert_eq!(
            src.names[1],
            rustty_identity::TERMINFO_PREFERRED_NAME.as_bytes()
        );
        assert_eq!(src.names[2], rustty_identity::APP_NAME_TITLECASE.as_bytes());

        // The encoded header line carries the rustty product identity.
        let mut buf: Vec<u8> = Vec::new();
        src.encode(&mut buf).unwrap();
        assert!(buf.starts_with(b"xterm-rustty|rustty|Rustty,\n"));
    }

    /// Under the `identity-oracle` parity harness feature, the same
    /// generator must emit the reference's byte-for-byte identity.
    #[cfg(feature = "identity-oracle")]
    #[test]
    fn names_are_ghostty_under_harness_profile() {
        let src = rustty();
        assert_eq!(
            src.names,
            vec![
                b"xterm-ghostty".to_vec(),
                b"ghostty".to_vec(),
                b"Ghostty".to_vec(),
            ]
        );

        let mut buf: Vec<u8> = Vec::new();
        src.encode(&mut buf).unwrap();
        assert!(buf.starts_with(b"xterm-ghostty|ghostty|Ghostty,\n"));
    }
}
