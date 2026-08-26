//! D4 — comptime-phase preservation + golden VT bytes (kills Bun regression
//! Class 3, oven-sh/bun#30693).
//!
//! Detector spec: `.port/phase-2/BUN_PORT_LEARNINGS.md` §4 "D4"; checklist gate
//! `.port/tools/gates/check_comptime_phase.md`.
//!
//! WHY THIS MODULE EXISTS
//! ----------------------
//! Bun ported a Zig `inline fn pretty(comptime fmt, args)` — which rewrote tags
//! on the TEMPLATE *before* substitution — to a runtime function that parsed the
//! fully-rendered bytes. An OSC 8 hyperlink terminator (ST = `ESC \`) then
//! collided with the tag grammar and corrupted output (`ai` -> `air`, #30693).
//! Ghostty is the VERY-HIGH-likelihood host for exactly this class: it
//! comptime-builds CSI/SGR/DCS/OSC responses and the data domain *is* escape
//! sequences.
//!
//! This is a NEW, DISJOINT test crate (Cargo auto-discovers `tests/*.rs`). It
//! adds ZERO edits to shipped source: it drives only the public reply-emitter
//! surface of `rustty-vt` and asserts two things over the whole reply corpus:
//!
//!   1. BYTE-EXACT golden strings for every emitter (the phase-separation is
//!      correct iff the bytes are correct), and
//!   2. ST-INTEGRITY: every OSC / DCS / APC (and SOS / PM) opener is followed by
//!      a correct terminator (ST = `ESC \` or BEL), with no stray `ESC` inside a
//!      string body and no orphan terminator — the precise invariant #30693
//!      violated.
//!
//! Emitters covered (device_status/device_attributes/kitty/OSC/DCS/APC):
//!   * DA1 / DA2 / DA3 — `device_attributes::{Primary,Secondary,Tertiary}`; DA3
//!     is a DCS: `ESC P ! | .... ESC \`
//!   * size reports — `size_report::encode` (CSI ... t; Mode 2048 / 14t/16t/18t)
//!   * kitty graphics — `kitty::graphics_command::Response::encode` (APC `ESC _ G`)
//!   * glyph APC — `apc::glyph::response::Response::format_wire` (`ESC _ 25a1`)
//!   * OSC terminator — `osc::Terminator::string()` (ST vs BEL), plus canonical
//!     OSC-8 hyperlink / OSC-52 clipboard reply goldens fed through the
//!     ST-integrity scanner (the literal #30693 shape).

use rustty_vt::apc::glyph::response::{
    Coverage, Response as GlyphResponse, ResponseClear, ResponseQuery, ResponseRegister,
    ResponseSupport, ResponseSupportFormats, Status,
};
use rustty_vt::device_attributes::{
    ConformanceLevel, DeviceType, Feature, Primary, Secondary, Tertiary,
};
use rustty_vt::kitty::graphics_command::Response as KittyResponse;
use rustty_vt::osc::Terminator;
use rustty_vt::size_report::{self, Size, Style};

// ---------------------------------------------------------------------------
// (b) ST-INTEGRITY assertion helper — the D4 invariant checker.
// ---------------------------------------------------------------------------

/// Human-readable name for a string-sequence opener final byte (7-bit or 8-bit).
fn opener_name(kind: u8) -> &'static str {
    match kind {
        b']' | 0x9d => "OSC",
        b'P' | 0x90 => "DCS",
        b'_' | 0x9f => "APC",
        b'X' | 0x98 => "SOS",
        b'^' | 0x9e => "PM",
        _ => "STRING",
    }
}

/// Scan `bytes` and return `Err(reason)` on any ST-integrity violation.
///
/// String-type control sequences (OSC `ESC ]`, DCS `ESC P`, APC `ESC _`, and the
/// rarely-emitted SOS `ESC X` / PM `ESC ^`, plus their 8-bit C1 forms) MUST be
/// closed by a String Terminator — ST (`ESC \` / 0x9c) or BEL (0x07) — before the
/// stream ends or another opener begins. Inside a string body the ONLY legal
/// `ESC` is the one that begins ST; any other `ESC` is the #30693 grammar
/// collision (a data byte aliasing the template terminator) and is rejected. A
/// terminator with no matching opener (orphan ST) is likewise rejected.
///
/// Non-string escapes (CSI `ESC [`, designators, single-byte controls) cannot
/// carry an ST collision, so they are skipped without deep parsing.
fn st_integrity(bytes: &[u8]) -> Result<(), String> {
    let n = bytes.len();
    let mut i = 0usize;
    // Some((opener_final_byte, start_offset)) while inside a string body.
    let mut open: Option<(u8, usize)> = None;

    while i < n {
        let b = bytes[i];

        if b == 0x1b {
            // ESC — a two-byte lookahead decides its role.
            if i + 1 >= n {
                return Err(format!("dangling ESC at end of stream (offset {i})"));
            }
            let nb = bytes[i + 1];

            if let Some((kind, start)) = open {
                // Inside a string: the only permitted ESC is ST (`ESC \`).
                if nb == b'\\' {
                    open = None;
                    i += 2;
                    continue;
                }
                return Err(format!(
                    "stray ESC 0x{nb:02x} inside {} string opened at offset {start} \
                     (violation at offset {i}); only ST (ESC \\) may appear in a \
                     string body — template/data grammar collision (D4 / #30693)",
                    opener_name(kind)
                ));
            }

            match nb {
                // 7-bit string openers.
                b']' | b'P' | b'_' | b'X' | b'^' => {
                    open = Some((nb, i));
                    i += 2;
                }
                // Orphan ST outside any string.
                b'\\' => {
                    return Err(format!(
                        "stray ST (ESC \\) with no matching string opener (offset {i})"
                    ));
                }
                // Any other escape (CSI, designators, single control): not a
                // string sequence, cannot host an ST collision.
                _ => i += 2,
            }
            continue;
        }

        if b == 0x07 {
            // BEL terminates an open string; a bare BEL is a legal bell control.
            open = None;
            i += 1;
            continue;
        }

        // 8-bit C1 forms (defensive: 7-bit replies rarely use them). OSC=0x9d,
        // DCS=0x90, APC=0x9f, SOS=0x98, PM=0x9e, ST=0x9c. Inside a string, data
        // bytes are opaque except an 8-bit ST which closes it.
        if open.is_none() {
            if matches!(b, 0x9d | 0x90 | 0x9f | 0x98 | 0x9e) {
                open = Some((b, i));
            }
        } else if b == 0x9c {
            open = None;
        }
        i += 1;
    }

    if let Some((kind, start)) = open {
        return Err(format!(
            "unterminated {} string opened at offset {start}: no ST/BEL before EOF",
            opener_name(kind)
        ));
    }
    Ok(())
}

/// Assert ST-integrity, panicking with the emitter label + a hex dump on failure.
fn assert_st_integrity(label: &str, bytes: &[u8]) {
    if let Err(why) = st_integrity(bytes) {
        panic!(
            "ST-integrity violation in `{label}`: {why}\n  bytes = {}\n  hex   = {}",
            escape_debug(bytes),
            hexdump(bytes),
        );
    }
}

fn escape_debug(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            0x1b => s.push_str("<ESC>"),
            0x07 => s.push_str("<BEL>"),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("<{b:02x}>")),
        }
    }
    s
}

fn hexdump(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Corpus driver: (label, produced-bytes) with a byte-exact golden assertion.
// ---------------------------------------------------------------------------

/// One golden check: `produced` must equal `golden`, then it must pass the
/// ST-integrity scanner. Returns the produced bytes so callers can accumulate a
/// full-corpus ST sweep.
fn golden(label: &str, produced: &[u8], golden: &[u8]) {
    assert_eq!(
        produced,
        golden,
        "byte-exact golden mismatch for `{label}`:\n  produced = {}\n  expected = {}",
        escape_debug(produced),
        escape_debug(golden),
    );
    assert_st_integrity(label, produced);
}

fn da_str<F: FnOnce(&mut String) -> core::fmt::Result>(f: F) -> Vec<u8> {
    let mut buf = String::new();
    f(&mut buf).unwrap();
    buf.into_bytes()
}

// ---------------------------------------------------------------------------
// device_attributes — DA1 / DA2 / DA3.
// ---------------------------------------------------------------------------

#[test]
fn golden_device_attributes_da1() {
    golden(
        "DA1 default",
        &da_str(|w| Primary::default().encode(w)),
        b"\x1b[?62;22c",
    );

    golden(
        "DA1 with clipboard",
        &da_str(|w| {
            Primary {
                features: &[Feature::AnsiColor, Feature::Clipboard],
                ..Primary::default()
            }
            .encode(w)
        }),
        b"\x1b[?62;22;52c",
    );

    golden(
        "DA1 multiple features",
        &da_str(|w| {
            Primary {
                conformance_level: ConformanceLevel::VT420,
                features: &[
                    Feature::Columns132,
                    Feature::SelectiveErase,
                    Feature::AnsiColor,
                ],
            }
            .encode(w)
        }),
        b"\x1b[?64;1;6;22c",
    );

    golden(
        "DA1 no features",
        &da_str(|w| {
            Primary {
                conformance_level: ConformanceLevel::VT100,
                features: &[],
            }
            .encode(w)
        }),
        b"\x1b[?1c",
    );
}

#[test]
fn golden_device_attributes_da2() {
    golden(
        "DA2 default",
        &da_str(|w| Secondary::default().encode(w)),
        b"\x1b[>1;0;0c",
    );

    golden(
        "DA2 custom",
        &da_str(|w| {
            Secondary {
                device_type: DeviceType::VT510,
                firmware_version: 279,
                rom_cartridge: 0,
            }
            .encode(w)
        }),
        b"\x1b[>61;279;0c",
    );
}

#[test]
fn golden_device_attributes_da3_is_dcs() {
    // DA3 (DECRPTUI) is a DCS string: `ESC P ! | <8 hex> ESC \`. The ST-integrity
    // scanner MUST see the `ESC P` opener paired with the trailing `ESC \`.
    golden(
        "DA3 default",
        &da_str(|w| Tertiary::default().encode(w)),
        b"\x1bP!|00000000\x1b\\",
    );

    golden(
        "DA3 custom unit id",
        &da_str(|w| {
            Tertiary {
                unit_id: 0xAABBCCDD,
            }
            .encode(w)
        }),
        b"\x1bP!|AABBCCDD\x1b\\",
    );
}

// ---------------------------------------------------------------------------
// size_report — CSI ... t family.
// ---------------------------------------------------------------------------

#[test]
fn golden_size_reports() {
    let size = Size {
        rows: 24,
        columns: 80,
        cell_width: 9,
        cell_height: 18,
    };

    golden(
        "size Mode2048",
        &da_str(|w| size_report::encode(w, Style::Mode2048, size)),
        b"\x1b[48;24;80;432;720t",
    );
    golden(
        "size Csi14T",
        &da_str(|w| size_report::encode(w, Style::Csi14T, size)),
        b"\x1b[4;432;720t",
    );
    golden(
        "size Csi16T",
        &da_str(|w| size_report::encode(w, Style::Csi16T, size)),
        b"\x1b[6;18;9t",
    );
    golden(
        "size Csi18T",
        &da_str(|w| size_report::encode(w, Style::Csi18T, size)),
        b"\x1b[8;24;80t",
    );
}

// ---------------------------------------------------------------------------
// kitty graphics — APC `ESC _ G ... ESC \`.
// ---------------------------------------------------------------------------

fn kitty_encode(r: &KittyResponse) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    r.encode(&mut buf).unwrap();
    buf
}

#[test]
fn golden_kitty_graphics_response() {
    // id only.
    golden(
        "kitty id",
        &kitty_encode(&KittyResponse {
            id: 1,
            ..KittyResponse::default()
        }),
        b"\x1b_Gi=1;OK\x1b\\",
    );

    // id + image number + placement.
    golden(
        "kitty id+I+p",
        &kitty_encode(&KittyResponse {
            id: 31,
            image_number: 2,
            placement_id: 7,
            message: b"OK".to_vec(),
            // (tier-2b `4b915e3bc`: the animation-frame echo, absent here.)
            ..KittyResponse::default()
        }),
        b"\x1b_Gi=31,I=2,p=7;OK\x1b\\",
    );

    // Error message payload (still a well-framed APC string).
    golden(
        "kitty error",
        &kitty_encode(&KittyResponse {
            id: 5,
            message: b"ENOENT:bad path".to_vec(),
            ..KittyResponse::default()
        }),
        b"\x1b_Gi=5;ENOENT:bad path\x1b\\",
    );

    // No id and no image number => empty (nothing emitted). Empty passes ST.
    let empty = kitty_encode(&KittyResponse::default());
    assert!(empty.is_empty(), "kitty default must emit nothing");
    assert_st_integrity("kitty empty", &empty);
}

// ---------------------------------------------------------------------------
// glyph APC — `ESC _ 25a1 ; ... ESC \`.
// ---------------------------------------------------------------------------

fn glyph_wire(r: &GlyphResponse) -> Vec<u8> {
    let mut s = String::new();
    r.format_wire(&mut s).unwrap();
    s.into_bytes()
}

#[test]
fn golden_glyph_apc_responses() {
    golden(
        "glyph support glyf,colrv0",
        &glyph_wire(&GlyphResponse::Support(ResponseSupport {
            fmt: ResponseSupportFormats {
                glyf: true,
                colrv0: true,
                colrv1: false,
            },
        })),
        b"\x1b_25a1;s;fmt=glyf,colrv0\x1b\\",
    );

    golden(
        "glyph support none",
        &glyph_wire(&GlyphResponse::Support(ResponseSupport {
            fmt: ResponseSupportFormats {
                glyf: false,
                colrv0: false,
                colrv1: false,
            },
        })),
        b"\x1b_25a1;s;fmt=\x1b\\",
    );

    golden(
        "glyph query system",
        &glyph_wire(&GlyphResponse::Query(ResponseQuery {
            cp: 0x41,
            status: Coverage {
                system: true,
                glossary: false,
            },
        })),
        b"\x1b_25a1;q;cp=41;status=system\x1b\\",
    );

    golden(
        "glyph register ok",
        &glyph_wire(&GlyphResponse::Register(ResponseRegister::new(0x1f600))),
        b"\x1b_25a1;r;cp=1f600;status=0\x1b\\",
    );

    golden(
        "glyph clear ok",
        &glyph_wire(&GlyphResponse::Clear(ResponseClear {
            status: Status::OK,
            reason: None,
        })),
        b"\x1b_25a1;c;status=0\x1b\\",
    );
}

// ---------------------------------------------------------------------------
// OSC — terminator surface + canonical reply goldens (the literal #30693 shape).
// ---------------------------------------------------------------------------

#[test]
fn golden_osc_terminator_surface() {
    // The public terminator constructor + byte form.
    assert_eq!(Terminator::init(None).string(), b"\x1b\\");
    assert_eq!(Terminator::init(Some(0x1b)).string(), b"\x1b\\");
    assert_eq!(Terminator::init(Some(0x07)).string(), b"\x07");
    // A lone terminator is not itself a string; it must NOT read as an orphan
    // opener/closer imbalance when it legitimately closes an opener (see below).
}

#[test]
fn golden_osc_replies_st_integrity() {
    // Canonical OSC replies assembled with each terminator flavour. These are
    // the exact shapes that broke in #30693 (OSC 8 hyperlink + ST). Both the
    // ST-terminated and BEL-terminated forms must scan clean.
    let mut term_st = Vec::new();
    term_st.extend_from_slice(b"\x1b]8;;https://example.com/a");
    term_st.extend_from_slice(Terminator::St.string()); // ESC \
    golden(
        "OSC 8 hyperlink (ST)",
        &term_st,
        b"\x1b]8;;https://example.com/a\x1b\\",
    );

    let mut term_bel = Vec::new();
    term_bel.extend_from_slice(b"\x1b]8;;https://example.com/a");
    term_bel.extend_from_slice(Terminator::Bel.string()); // BEL
    golden(
        "OSC 8 hyperlink (BEL)",
        &term_bel,
        b"\x1b]8;;https://example.com/a\x07",
    );

    // OSC 8 close (empty params) — the `ai`->`air` corruption site.
    golden("OSC 8 close", b"\x1b]8;;\x1b\\", b"\x1b]8;;\x1b\\");

    // OSC 52 clipboard report, ST-terminated.
    golden(
        "OSC 52 clipboard",
        b"\x1b]52;c;aGVsbG8=\x1b\\",
        b"\x1b]52;c;aGVsbG8=\x1b\\",
    );

    // OSC 4 color report, BEL-terminated.
    golden(
        "OSC 4 color (BEL)",
        b"\x1b]4;1;rgb:ffff/0000/0000\x07",
        b"\x1b]4;1;rgb:ffff/0000/0000\x07",
    );
}

// ---------------------------------------------------------------------------
// NEGATIVE tests — prove the ST-integrity scanner actually catches #30693.
// A helper that passes everything would be worthless; these lock it down.
// ---------------------------------------------------------------------------

#[test]
fn st_scanner_rejects_unterminated_opener() {
    // OSC opener with no ST/BEL before EOF.
    assert!(st_integrity(b"\x1b]8;;https://example.com").is_err());
    // DCS opener never closed.
    assert!(st_integrity(b"\x1bP!|00000000").is_err());
    // APC opener never closed.
    assert!(st_integrity(b"\x1b_25a1;s;fmt=glyf").is_err());
}

#[test]
fn st_scanner_rejects_stray_esc_inside_string() {
    // The #30693 grammar collision: a data byte sequence that begins ESC but is
    // NOT the ST — a rendered-bytes parser would mis-split the tag here.
    let err = st_integrity(b"\x1b]8;;pre\x1b[0mpost\x1b\\").unwrap_err();
    assert!(err.contains("grammar collision"), "got: {err}");
    // Same for a DCS body carrying a stray ESC.
    assert!(st_integrity(b"\x1bPdata\x1bXmore\x1b\\").is_err());
}

#[test]
fn st_scanner_rejects_orphan_terminator() {
    // A bare ST with no opener (a template terminator that leaked into data).
    assert!(st_integrity(b"plain text\x1b\\").is_err());
}

#[test]
fn st_scanner_accepts_wellformed_and_concatenated() {
    // Empty and pure-CSI streams have no string sequences => trivially clean.
    assert!(st_integrity(b"").is_ok());
    assert!(st_integrity(b"\x1b[?62;22c").is_ok());
    // Multiple back-to-back well-framed replies (ST and BEL) must all pass.
    let mut stream = Vec::new();
    stream.extend_from_slice(b"\x1b[?62;22c"); // DA1 (CSI)
    stream.extend_from_slice(b"\x1bP!|00000000\x1b\\"); // DA3 (DCS + ST)
    stream.extend_from_slice(b"\x1b]8;;https://x\x07"); // OSC (BEL)
    stream.extend_from_slice(b"\x1b_Gi=1;OK\x1b\\"); // kitty APC (ST)
    assert!(st_integrity(&stream).is_ok());
}

// ---------------------------------------------------------------------------
// Full-corpus ST sweep: drive every emitter once more and scan the concatenated
// reply stream — a single failure anywhere in the corpus fails here too.
// ---------------------------------------------------------------------------

#[test]
fn full_corpus_st_sweep() {
    let size = Size {
        rows: 24,
        columns: 80,
        cell_width: 9,
        cell_height: 18,
    };
    let mut stream: Vec<u8> = Vec::new();

    stream.extend_from_slice(&da_str(|w| Primary::default().encode(w)));
    stream.extend_from_slice(&da_str(|w| Secondary::default().encode(w)));
    stream.extend_from_slice(&da_str(|w| Tertiary::default().encode(w)));
    for style in [Style::Mode2048, Style::Csi14T, Style::Csi16T, Style::Csi18T] {
        stream.extend_from_slice(&da_str(|w| size_report::encode(w, style, size)));
    }
    stream.extend_from_slice(&kitty_encode(&KittyResponse {
        id: 1,
        ..KittyResponse::default()
    }));
    stream.extend_from_slice(&glyph_wire(&GlyphResponse::Register(
        ResponseRegister::new(0x1f600),
    )));

    assert_st_integrity("full reply corpus", &stream);
}
