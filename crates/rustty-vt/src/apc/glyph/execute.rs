//! rustty-vt apc/glyph/execute
//!
//! PORTS: ghostty src/terminal/apc/glyph/execute.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const request = @import("request.zig");
// const response = @import("response.zig");
// const Glossary = @import("Glossary.zig");
// const Request = request.Request;
// const Response = response.Response;
// PORT NOTE: `Allocator` is not threaded through (the glossary and its entries
// own their decoded outlines as `Vec`s, freed on drop).
use crate::apc::glyph::glossary::{self, Entry, Glossary};
use crate::apc::glyph::request::{self, Request};
use crate::apc::glyph::response::{
    Coverage, Response, ResponseClear, ResponseQuery, ResponseRegister, ResponseRegisterReason,
    ResponseSupport, ResponseSupportFormats, Status,
};

// const log = std.log.scoped(.glyph);
// PORT NOTE: the scoped logger is unused beyond `log.debug` in `execute`; the
// log call is preserved as a comment (the project ports `std.log.scoped` as a
// commented-out seam — no behavior depends on it).

/// Payload formats we support. Hardcoded because the support is
/// fixed.
// PORT NOTE: reference `pub const supported_formats: response.Response.Support
// .Formats = .{ .glyf = true }`; the other format bits default to false.
pub const SUPPORTED_FORMATS: ResponseSupportFormats = ResponseSupportFormats {
    glyf: true,
    colrv0: false,
    colrv1: false,
};

/// Execute a Glyph protocol request against the given state.
///
/// This will never fail, but the response may indiciate an error and
/// the terminal state may not be updated to reflect the command. This will
/// never put the terminal in a corrupt or non-recoverable state.
///
/// For example, allocation errors can happen, but they're wrapped up in
/// an out of memory response.
///
/// Query responses only report glossary coverage. Callers that can determine
/// system font coverage must update the returned query response before sending
/// it to the client.
pub fn execute(
    alloc: crate::alloc::Allocator,
    glossary: &mut Glossary,
    req: &Request,
) -> Option<Response> {
    // log.debug("executing glyph protocol request: {t}", .{req.*});
    // return switch (req.*) {
    match req {
        // .support => .{ .support = .{ .fmt = supported_formats } },
        Request::Support => Some(Response::Support(ResponseSupport {
            fmt: SUPPORTED_FORMATS,
        })),
        // .query => |qry| query(glossary, qry),
        Request::Query(qry) => query(glossary, qry),
        // .register => |reg| register(alloc, glossary, reg),
        Request::Register(reg) => register(alloc, glossary, reg),
        // .clear => |clr| clear(alloc, glossary, clr),
        Request::Clear(clr) => clear(glossary, clr),
    }
}

fn query(glossary: &mut Glossary, qry: &request::Query) -> Option<Response> {
    // const cp = qry.get(.cp) orelse return null;
    let cp = qry.get_cp()?;
    // return .{ .query = .{ .cp = cp, .status = .{ .glossary = glossary.contains(cp) } } };
    Some(Response::Query(ResponseQuery {
        cp,
        status: Coverage {
            glossary: glossary.contains(cp),
            ..Coverage::default()
        },
    }))
}

fn register(
    alloc: crate::alloc::Allocator,
    glossary: &mut Glossary,
    reg: &request::Register,
) -> Option<Response> {
    // const reply = reg.get(.reply) orelse .all;
    let reply = reg.get_reply().unwrap_or(request::Reply::All);
    // const cp = registerFallible(alloc, glossary, reg) catch |err| return switch (reply) {
    let cp = match register_fallible(alloc, glossary, reg) {
        Ok(cp) => cp,
        Err(err) => {
            return match reply {
                // .none => null,
                request::Reply::None => None,
                // .all, .failures => .{ .register = .{ ... } },
                request::Reply::All | request::Reply::Failures => {
                    Some(Response::Register(ResponseRegister {
                        // .cp = reg.get(.cp) orelse 0,
                        cp: reg.get_cp().unwrap_or(0),
                        // .status = .err,
                        status: Status::ERR,
                        // .reason = switch (err) { ... },
                        reason: Some(match err {
                            // error.OutOfMemory => .{ .other = "out_of_memory" },
                            RegisterFallibleError::OutOfMemory => {
                                ResponseRegisterReason::Other(b"out_of_memory".to_vec())
                            }
                            // error.OutOfNamespace => .out_of_namespace,
                            RegisterFallibleError::OutOfNamespace => {
                                ResponseRegisterReason::OutOfNamespace
                            }
                            // error.PayloadTooLarge => .payload_too_large,
                            RegisterFallibleError::PayloadTooLarge => {
                                ResponseRegisterReason::PayloadTooLarge
                            }
                            // error.MalformedPayload => .malformed_payload,
                            RegisterFallibleError::MalformedPayload => {
                                ResponseRegisterReason::MalformedPayload
                            }
                            // error.CompositeUnsupported => .composite_unsupported,
                            RegisterFallibleError::CompositeUnsupported => {
                                ResponseRegisterReason::CompositeUnsupported
                            }
                            // error.HintingUnsupported => .hinting_unsupported,
                            RegisterFallibleError::HintingUnsupported => {
                                ResponseRegisterReason::HintingUnsupported
                            }
                            // error.InvalidOptions, error.UnsupportedFormat => .malformed_payload,
                            RegisterFallibleError::InvalidOptions
                            | RegisterFallibleError::UnsupportedFormat => {
                                ResponseRegisterReason::MalformedPayload
                            }
                        }),
                    }))
                }
            };
        }
    };

    // return switch (reply) {
    match reply {
        // .none, .failures => null,
        request::Reply::None | request::Reply::Failures => None,
        // .all => .{ .register = .{ .cp = cp } },
        request::Reply::All => Some(Response::Register(ResponseRegister::new(cp))),
    }
}

/// Error set returned by [`register_fallible`].
// PORT NOTE: reference `(Glossary.Entry.InitError || Glossary.RegisterError)`.
// The union of `EntryInitError` and `RegisterError` is flattened here; the
// shared `OutOfMemory`/`OutOfNamespace` members appear once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegisterFallibleError {
    OutOfMemory,
    PayloadTooLarge,
    MalformedPayload,
    CompositeUnsupported,
    HintingUnsupported,
    InvalidOptions,
    UnsupportedFormat,
    OutOfNamespace,
}

impl From<glossary::EntryInitError> for RegisterFallibleError {
    fn from(err: glossary::EntryInitError) -> RegisterFallibleError {
        match err {
            glossary::EntryInitError::OutOfMemory => RegisterFallibleError::OutOfMemory,
            glossary::EntryInitError::PayloadTooLarge => RegisterFallibleError::PayloadTooLarge,
            glossary::EntryInitError::MalformedPayload => RegisterFallibleError::MalformedPayload,
            glossary::EntryInitError::CompositeUnsupported => {
                RegisterFallibleError::CompositeUnsupported
            }
            glossary::EntryInitError::HintingUnsupported => {
                RegisterFallibleError::HintingUnsupported
            }
            glossary::EntryInitError::InvalidOptions => RegisterFallibleError::InvalidOptions,
            glossary::EntryInitError::UnsupportedFormat => RegisterFallibleError::UnsupportedFormat,
        }
    }
}

impl From<glossary::RegisterError> for RegisterFallibleError {
    fn from(err: glossary::RegisterError) -> RegisterFallibleError {
        match err {
            glossary::RegisterError::OutOfMemory => RegisterFallibleError::OutOfMemory,
            glossary::RegisterError::OutOfNamespace => RegisterFallibleError::OutOfNamespace,
        }
    }
}

fn register_fallible(
    alloc: crate::alloc::Allocator,
    glossary: &mut Glossary,
    reg: &request::Register,
) -> Result<u32, RegisterFallibleError> {
    // const cp = reg.get(.cp) orelse return error.MalformedPayload;
    let cp = reg
        .get_cp()
        .ok_or(RegisterFallibleError::MalformedPayload)?;

    // var entry = try Glossary.Entry.init(alloc, reg);
    // errdefer entry.deinit(alloc);
    // PORT NOTE: the reference `errdefer entry.deinit(alloc)` frees the decoded
    // entry if `glossary.register` fails; here `entry` is dropped on the error
    // path (or moved into the glossary on success), reproducing that cleanup.
    let entry: Entry = Entry::init(alloc, reg)?;

    // try glossary.register(alloc, cp, entry);
    glossary.allocator().debug_assert_same(alloc);
    glossary.register(cp, entry)?;
    // return cp;
    Ok(cp)
}

fn clear(glossary: &mut Glossary, clr: &request::Clear) -> Option<Response> {
    // if (clr.get(.cp)) |cp| {
    if let Some(cp) = clr.get_cp() {
        // glossary.delete(alloc, cp) catch |err| return .{ .clear = .{ ... } };
        if let Err(err) = glossary.delete(cp) {
            return Some(Response::Clear(ResponseClear {
                // .status = .err,
                status: Status::ERR,
                // .reason = switch (err) { error.OutOfNamespace => "out_of_namespace" },
                reason: Some(match err {
                    glossary::ClearError::OutOfNamespace => b"out_of_namespace".to_vec(),
                }),
            }));
        }
    // } else if (clr.has(.cp)) {
    } else if clr.has_cp() {
        // return .{ .clear = .{ .status = .err, .reason = "malformed_payload" } };
        return Some(Response::Clear(ResponseClear {
            status: Status::ERR,
            reason: Some(b"malformed_payload".to_vec()),
        }));
    // } else {
    } else {
        // glossary.clearAndFree(alloc);
        glossary.clear_and_free();
    }

    // return .{ .clear = .{} };
    Some(Response::Clear(ResponseClear::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apc::glyph::request::CommandParser;

    // PORT NOTE: reference `testParse(alloc, data)` feeds bytes through a
    // `CommandParser` and returns the completed `Request`. `alloc` is dropped.
    fn test_parse(data: &[u8]) -> Request {
        // var parser = request.CommandParser.init(alloc, 1024 * 1024);
        let mut parser = CommandParser::init(crate::alloc::Global, 1024 * 1024);
        // defer parser.deinit();
        // for (data) |byte| try parser.feed(byte);
        for &byte in data {
            parser.feed(byte).unwrap();
        }
        // return try parser.complete(alloc);
        parser.complete(crate::alloc::Global).unwrap()
    }

    // PORT NOTE: reference `testExecute(alloc, glossary, req)` is a thin wrapper
    // over `execute`; the `alloc` argument is dropped.
    fn test_execute(glossary: &mut Glossary, req: &Request) -> Option<Response> {
        execute(crate::alloc::Global, glossary, req)
    }

    #[test]
    fn execute_register_stores_glyph_and_returns_success() {
        // var glossary: Glossary = .empty;
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "r;cp=e0a0;AAAAAAAAAAAAAA==");
        let req = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==");

        // try testing.expectEqual(Response{ .register = .{ .cp = 0xE0A0 } },
        //     testExecute(alloc, &glossary, &req).?);
        assert_eq!(
            Response::Register(ResponseRegister::new(0xE0A0)),
            test_execute(&mut glossary, &req).unwrap()
        );
        // try testing.expect(glossary.contains(0xE0A0));
        assert!(glossary.contains(0xE0A0));
    }

    #[test]
    fn execute_register_reply_failures_suppresses_success() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "r;cp=e0a0;reply=2;AAAAAAAAAAAAAA==");
        let req = test_parse(b"r;cp=e0a0;reply=2;AAAAAAAAAAAAAA==");

        // try testing.expect(testExecute(alloc, &glossary, &req) == null);
        assert!(test_execute(&mut glossary, &req).is_none());
        // try testing.expect(glossary.contains(0xE0A0));
        assert!(glossary.contains(0xE0A0));
    }

    #[test]
    fn execute_register_reply_none_suppresses_failure() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "r;cp=41;reply=0;%%%not-base64%%%");
        let req = test_parse(b"r;cp=41;reply=0;%%%not-base64%%%");

        // try testing.expect(testExecute(alloc, &glossary, &req) == null);
        assert!(test_execute(&mut glossary, &req).is_none());
        // try testing.expect(!glossary.contains('A'));
        assert!(!glossary.contains('A' as u32));
    }

    #[test]
    fn execute_register_rejects_non_pua() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "r;cp=41;AAAAAAAAAAAAAA==");
        let req = test_parse(b"r;cp=41;AAAAAAAAAAAAAA==");

        // try testing.expectEqual(Response{ .register = .{ .cp = 'A', .status = .err,
        //     .reason = .out_of_namespace } }, testExecute(...).?);
        assert_eq!(
            Response::Register(ResponseRegister {
                cp: 'A' as u32,
                status: Status::ERR,
                reason: Some(ResponseRegisterReason::OutOfNamespace),
            }),
            test_execute(&mut glossary, &req).unwrap()
        );
        // try testing.expect(!glossary.contains('A'));
        assert!(!glossary.contains('A' as u32));
    }

    #[test]
    fn execute_register_reports_malformed_payload() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "r;cp=e0a0;%%%not-base64%%%");
        let req = test_parse(b"r;cp=e0a0;%%%not-base64%%%");

        assert_eq!(
            Response::Register(ResponseRegister {
                cp: 0xE0A0,
                status: Status::ERR,
                reason: Some(ResponseRegisterReason::MalformedPayload),
            }),
            test_execute(&mut glossary, &req).unwrap()
        );
        // try testing.expect(!glossary.contains(0xE0A0));
        assert!(!glossary.contains(0xE0A0));
    }

    #[test]
    fn execute_clear_removes_all_glyphs() {
        let mut glossary = Glossary::empty();

        // var reg1 = try testParse(alloc, "r;cp=e0a0;AAAAAAAAAAAAAA==");
        let reg1 = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==");
        // _ = testExecute(alloc, &glossary, &reg1);
        let _ = test_execute(&mut glossary, &reg1);

        // var reg2 = try testParse(alloc, "r;cp=e0a1;AAAAAAAAAAAAAA==");
        let reg2 = test_parse(b"r;cp=e0a1;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg2);

        // var req = try testParse(alloc, "c");
        let req = test_parse(b"c");

        // try testing.expectEqual(Response{ .clear = .{} }, testExecute(...).?);
        assert_eq!(
            Response::Clear(ResponseClear::default()),
            test_execute(&mut glossary, &req).unwrap()
        );
        // try testing.expect(!glossary.contains(0xE0A0));
        assert!(!glossary.contains(0xE0A0));
        // try testing.expect(!glossary.contains(0xE0A1));
        assert!(!glossary.contains(0xE0A1));
    }

    #[test]
    fn execute_clear_removes_one_glyph() {
        let mut glossary = Glossary::empty();

        let reg1 = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg1);

        let reg2 = test_parse(b"r;cp=e0a1;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg2);

        // var req = try testParse(alloc, "c;cp=e0a0");
        let req = test_parse(b"c;cp=e0a0");

        assert_eq!(
            Response::Clear(ResponseClear::default()),
            test_execute(&mut glossary, &req).unwrap()
        );
        // try testing.expect(!glossary.contains(0xE0A0));
        assert!(!glossary.contains(0xE0A0));
        // try testing.expect(glossary.contains(0xE0A1));
        assert!(glossary.contains(0xE0A1));
    }

    #[test]
    fn execute_clear_rejects_non_pua() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "c;cp=41");
        let req = test_parse(b"c;cp=41");

        // try testing.expectEqual(Response{ .clear = .{ .status = .err,
        //     .reason = "out_of_namespace" } }, testExecute(...).?);
        assert_eq!(
            Response::Clear(ResponseClear {
                status: Status::ERR,
                reason: Some(b"out_of_namespace".to_vec()),
            }),
            test_execute(&mut glossary, &req).unwrap()
        );
    }

    #[test]
    fn execute_clear_rejects_malformed_cp_without_clearing_glossary() {
        let mut glossary = Glossary::empty();

        let reg1 = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg1);

        let reg2 = test_parse(b"r;cp=e0a1;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg2);

        // for ([_][]const u8{ "c;cp=zz", "c;cp=", "c;cp=200000" }) |data| {
        for data in [
            b"c;cp=zz".as_slice(),
            b"c;cp=".as_slice(),
            b"c;cp=200000".as_slice(),
        ] {
            let req = test_parse(data);

            // try testing.expectEqual(Response{ .clear = .{ .status = .err,
            //     .reason = "malformed_payload" } }, testExecute(...).?);
            assert_eq!(
                Response::Clear(ResponseClear {
                    status: Status::ERR,
                    reason: Some(b"malformed_payload".to_vec()),
                }),
                test_execute(&mut glossary, &req).unwrap()
            );
            // try testing.expect(glossary.contains(0xE0A0));
            assert!(glossary.contains(0xE0A0));
            // try testing.expect(glossary.contains(0xE0A1));
            assert!(glossary.contains(0xE0A1));
        }
    }

    #[test]
    fn execute_query_reports_no_coverage() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "q;cp=e0a0");
        let req = test_parse(b"q;cp=e0a0");

        // try testing.expectEqual(Response{ .query = .{ .cp = 0xE0A0, .status = .{} } },
        //     testExecute(...).?);
        assert_eq!(
            Response::Query(ResponseQuery {
                cp: 0xE0A0,
                status: Coverage::default(),
            }),
            test_execute(&mut glossary, &req).unwrap()
        );
    }

    #[test]
    fn execute_query_reports_glossary_coverage() {
        let mut glossary = Glossary::empty();

        // var reg = try testParse(alloc, "r;cp=e0a0;AAAAAAAAAAAAAA==");
        let reg = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==");
        let _ = test_execute(&mut glossary, &reg);

        let req = test_parse(b"q;cp=e0a0");

        // try testing.expectEqual(Response{ .query = .{ .cp = 0xE0A0,
        //     .status = .{ .glossary = true } } }, testExecute(...).?);
        assert_eq!(
            Response::Query(ResponseQuery {
                cp: 0xE0A0,
                status: Coverage {
                    glossary: true,
                    ..Coverage::default()
                },
            }),
            test_execute(&mut glossary, &req).unwrap()
        );
    }

    #[test]
    fn execute_query_without_cp_returns_no_response() {
        let mut glossary = Glossary::empty();

        // var req = try testParse(alloc, "q;foo=bar");
        let req = test_parse(b"q;foo=bar");

        // try testing.expect(testExecute(alloc, &glossary, &req) == null);
        assert!(test_execute(&mut glossary, &req).is_none());
    }
}
