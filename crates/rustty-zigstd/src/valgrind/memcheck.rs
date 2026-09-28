//! PORTS: the part of Zig 0.16 `std.valgrind.memcheck`
//! (`lib/std/valgrind/memcheck.zig`) the port uses: the `ClientRequest` codes,
//! `makeMemDefined` and `makeMemUndefined`.
//!
//! Every request goes through [`super::do_client_request`], so, as in the
//! reference, it is a no-op returning the default wherever client requests are
//! not compiled in (`valgrind_support`) or the program does not run under
//! Valgrind.
//!
//! The one user is the renderer's two llvmpipe golden tests: they declare the
//! pixels they read back from Mesa's software rasterizer defined, because
//! memcheck cannot follow llvmpipe's blending arithmetic (NEW-FINDINGS N412,
//! `.port/tools/valgrind-llvmpipe-probe/probe.c`).

use super::{do_client_request, tool_base};

// pub const ClientRequest = enum(usize) {
//     MakeMemNoAccess = valgrind.ToolBase("MC".*),
//     MakeMemUndefined,
//     MakeMemDefined,
//     Discard,
//     CheckMemIsAddressable,
//     CheckMemIsDefined,
//     DoLeakCheck,
//     CountLeaks,
//     GetVbits,
//     SetVbits,
//     CreateBlock,
//     MakeMemDefinedIfAddressable,
//     CountLeakBlocks,
//     EnableAddrErrorReportingInRange,
//     DisableAddrErrorReportingInRange,
// };
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientRequest {
    MakeMemNoAccess = tool_base(*b"MC") as usize,
    MakeMemUndefined,
    MakeMemDefined,
    Discard,
    CheckMemIsAddressable,
    CheckMemIsDefined,
    DoLeakCheck,
    CountLeaks,
    GetVbits,
    SetVbits,
    CreateBlock,
    MakeMemDefinedIfAddressable,
    CountLeakBlocks,
    EnableAddrErrorReportingInRange,
    DisableAddrErrorReportingInRange,
}

// fn doClientRequestExpr(default: usize, request: ClientRequest, a1: usize,
//     a2: usize, a3: usize, a4: usize, a5: usize) usize {
//     return valgrind.doClientRequest(default, @as(usize, @intCast(@intFromEnum(request))), a1, a2, a3, a4, a5);
// }
fn do_client_request_expr(
    default: usize,
    request: ClientRequest,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> usize {
    do_client_request(default, request as usize, a1, a2, a3, a4, a5)
}

/// Mark memory at qzz.ptr as addressable but undefined for qzz.len bytes.
// pub fn makeMemUndefined(qzz: []const u8) void {
//     _ = doClientRequestExpr(0, // default return
//         .MakeMemUndefined, @intFromPtr(qzz.ptr), qzz.len, 0, 0, 0);
// }
pub fn make_mem_undefined(qzz: &[u8]) {
    let _ = do_client_request_expr(
        0, // default return
        ClientRequest::MakeMemUndefined,
        qzz.as_ptr() as usize,
        qzz.len(),
        0,
        0,
        0,
    );
}

/// Mark memory at qzz.ptr as addressable and defined or qzz.len bytes.
// pub fn makeMemDefined(qzz: []const u8) void {
//     _ = doClientRequestExpr(0, // default return
//         .MakeMemDefined, @intFromPtr(qzz.ptr), qzz.len, 0, 0, 0);
// }
pub fn make_mem_defined(qzz: &[u8]) {
    let _ = do_client_request_expr(
        0, // default return
        ClientRequest::MakeMemDefined,
        qzz.as_ptr() as usize,
        qzz.len(),
        0,
        0,
        0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The request codes are Valgrind's `memcheck.h` values: the tool base
    /// `'M' << 24 | 'C' << 16`, then consecutive codes in the reference's
    /// order.
    #[test]
    fn client_request_codes_are_memcheck_h() {
        assert_eq!(ClientRequest::MakeMemNoAccess as usize, 0x4d43_0000);
        assert_eq!(ClientRequest::MakeMemUndefined as usize, 0x4d43_0001);
        assert_eq!(ClientRequest::MakeMemDefined as usize, 0x4d43_0002);
        assert_eq!(ClientRequest::CheckMemIsDefined as usize, 0x4d43_0005);
        assert_eq!(
            ClientRequest::DisableAddrErrorReportingInRange as usize,
            0x4d43_000e
        );
    }

    /// Natively both requests are no-ops. Under Valgrind (G-VALGRIND runs this
    /// binary under memcheck) the buffer is first marked undefined and then
    /// defined again, and the branch below reads it: if `make_mem_defined` did
    /// not reach memcheck, the branch would be an uninitialised-value error and
    /// the gate would report it.
    #[test]
    #[cfg_attr(miri, ignore = "inline assembly does not run under Miri (R14)")]
    fn make_mem_defined_undoes_make_mem_undefined() {
        let buf = std::hint::black_box(vec![7u8; 64]);
        make_mem_undefined(&buf);
        make_mem_defined(&buf);
        let mut sum = 0u32;
        for &b in &buf {
            if b == 7 {
                sum += 1;
            }
        }
        assert_eq!(sum, 64);
    }
}
