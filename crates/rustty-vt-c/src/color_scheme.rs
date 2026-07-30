//! rustty-vt-c color_scheme
//!
//! PORTS: ghostty src/terminal/c/color_scheme.zig (BL-5 f00e90694)

use rustty_vt::device_status::{self, ColorScheme};

use crate::result::Result;

// pub fn report_encode(scheme, out_, out_len, out_written) callconv(...) Result
/// C: rustty_color_scheme_report_encode
///
/// # Safety
/// `out_`, if non-null, must be writable for `out_len` bytes; `out_written`
/// must be a valid writable pointer.
pub unsafe fn report_encode(
    scheme: ColorScheme,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // PORT NOTE: the reference encodes into a fixed writer and, on
    // error.WriteFailed, re-runs against a discarding writer to compute the
    // required size. rustty encodes into a Vec (encoder is infallible) and
    // compares lengths — byte-identical outputs and identical Result mapping.
    let mut buf = Vec::with_capacity(device_status::MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE);
    device_status::encode_color_scheme_report(&mut buf, scheme);
    if out_.is_null() || out_len < buf.len() {
        // SAFETY: out_written validity is the caller's contract.
        unsafe { *out_written = buf.len() };
        return Result::OutOfSpace;
    }
    // SAFETY: caller contract (out_ writable for out_len >= buf.len()).
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), out_, buf.len());
        *out_written = buf.len();
    }
    Result::Success
}

#[cfg(test)]
mod tests {
    use rustty_vt::device_status::{ColorScheme, MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE};

    use super::*;
    use crate::result::Result;

    // test "encode color scheme report dark"
    #[test]
    fn dark() {
        let mut buf = [0u8; MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE];
        let mut written = 0usize;
        let result =
            unsafe { report_encode(ColorScheme::Dark, buf.as_mut_ptr(), buf.len(), &mut written) };
        assert_eq!(Result::Success, result);
        assert_eq!(b"\x1B[?997;1n", &buf[..written]);
    }

    // test "encode color scheme report light"
    #[test]
    fn light() {
        let mut buf = [0u8; MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(
                ColorScheme::Light,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        assert_eq!(b"\x1B[?997;2n", &buf[..written]);
    }

    // test "encode color scheme report with null buffer"
    #[test]
    fn null_buffer() {
        let mut written = 0usize;
        let result =
            unsafe { report_encode(ColorScheme::Dark, std::ptr::null_mut(), 0, &mut written) };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(9, written);
    }

    // test "encode color scheme report with insufficient buffer"
    #[test]
    fn insufficient_buffer() {
        let mut buf = [0u8; 3];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(
                ColorScheme::Light,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(9, written);
    }

    // test "encode color scheme report with exact buffer"
    #[test]
    fn exact_buffer() {
        let mut buf = [0u8; 9];
        let mut written = 0usize;
        let result =
            unsafe { report_encode(ColorScheme::Dark, buf.as_mut_ptr(), buf.len(), &mut written) };
        assert_eq!(Result::Success, result);
        assert_eq!(9, written);
    }
}
