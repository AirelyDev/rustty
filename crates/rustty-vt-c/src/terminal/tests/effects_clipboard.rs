//! PORTS: ghostty src/terminal/c/terminal.zig — tests: clipboard
//! write/read callbacks, mode 2048, size, and device attributes.

use super::*;

// test "set clipboard_write callback" (oracle 634ef7198; 7ae9b1113 end
// state: reply-based, with name/granted/can_remember)
#[test]
fn set_clipboard_write_callback() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    static mut COUNT: usize = 0;
    static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_SIZE: usize = 0;
    static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
    static mut LAST_CONTENTS_NULL: bool = false;
    static mut LAST_CONTENTS_LEN: usize = 0;
    static mut LAST_MIMES: Vec<Vec<u8>> = Vec::new();
    static mut LAST_DATA: Vec<Vec<u8>> = Vec::new();
    static mut LAST_NAME_LEN: usize = 0;
    static mut LAST_GRANTED: bool = true;
    static mut LAST_CAN_REMEMBER: bool = true;
    static mut NEXT_RESULT: clipboard::WriteStatus = clipboard::WriteStatus::Success;
    static mut REPLY_RESULT: Option<clipboard::WriteResult> = None;

    // const Reply = struct { var last: ?clipboard.Write.Result = null; ... };
    // Captures the trampoline's reply for the direct-invoke checks below.
    fn capture_reply(_: *mut c_void, _: *mut c_void, result: clipboard::WriteResult) {
        unsafe { REPLY_RESULT = Some(result) };
    }

    extern "C" fn clipboard_write(
        terminal_: Terminal,
        ud: *mut c_void,
        request: *const ClipboardWrite,
    ) {
        unsafe {
            COUNT += 1;
            LAST_TERMINAL = terminal_;
            LAST_USERDATA = ud;
            LAST_SIZE = (*request).size;
            LAST_LOCATION = (*request).location;
            LAST_CONTENTS_NULL = (*request).contents.is_null();
            LAST_CONTENTS_LEN = (*request).contents_len;
            LAST_NAME_LEN = (*request).name.len;
            LAST_GRANTED = (*request).granted;
            LAST_CAN_REMEMBER = (*request).can_remember;
            let mut mimes: Vec<Vec<u8>> = Vec::new();
            let mut data: Vec<Vec<u8>> = Vec::new();
            if !(*request).contents.is_null() {
                let contents =
                    std::slice::from_raw_parts((*request).contents, (*request).contents_len);
                for c in contents {
                    mimes.push(std::slice::from_raw_parts(c.mime.ptr, c.mime.len).to_vec());
                    data.push(std::slice::from_raw_parts(c.data.ptr, c.data.len).to_vec());
                }
            }
            LAST_MIMES = mimes;
            LAST_DATA = data;

            // request.reply(request, &.{ .size = ..., .result = next_result, .remember = false });
            ((*request).reply)(
                request,
                &ClipboardWriteReply {
                    size: std::mem::size_of::<ClipboardWriteReply>(),
                    result: NEXT_RESULT,
                    remember: false,
                },
            );
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            COUNT = 0;
            LAST_TERMINAL = std::ptr::null_mut();
            LAST_USERDATA = std::ptr::null_mut();
            NEXT_RESULT = clipboard::WriteStatus::Denied;
            REPLY_RESULT = None;
        }

        let mut sentinel: u8 = 88;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
        });

        // Split OSC 52 write whose decoded payload contains an embedded NUL.
        vtw(t, b"\x1B]52;c;aGVs");
        vtw(t, b"bG8Ad29ybGQ=\x1B\\");
        assert_eq!(1, unsafe { COUNT });
        assert_eq!(t, unsafe { LAST_TERMINAL });
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        assert_eq!(std::mem::size_of::<ClipboardWrite>(), unsafe { LAST_SIZE });
        assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
        assert!(!unsafe { LAST_CONTENTS_NULL });
        assert_eq!(1, unsafe { LAST_CONTENTS_LEN });
        assert_eq!(b"text/plain".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
        });
        assert_eq!(b"hello\x00world".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
        });

        // OSC 52 carries no program identity or password grant state.
        assert_eq!(0, unsafe { LAST_NAME_LEN });
        assert!(!unsafe { LAST_GRANTED });
        assert!(!unsafe { LAST_CAN_REMEMBER });

        // OSC 52 destinations are normalized rather than exposed as wire bytes.
        for (selector, expected) in [
            (b's', clipboard::Location::Selection),
            (b'p', clipboard::Location::Primary),
            (b'q', clipboard::Location::Standard),
        ] {
            let seq = [
                0x1B, b']', b'5', b'2', b';', selector, b';', b'e', b'A', b'=', b'=', 0x1B, b'\\',
            ];
            vtw(t, &seq);
            assert_eq!(expected, unsafe { LAST_LOCATION });
        }
        assert_eq!(4, unsafe { COUNT });

        // An empty content list is a clear and retains a null descriptor pointer.
        vtw(t, b"\x1B]52;s;\x1B\\");
        assert_eq!(5, unsafe { COUNT });
        assert_eq!(clipboard::Location::Selection, unsafe { LAST_LOCATION });
        assert!(unsafe { LAST_CONTENTS_NULL });
        assert_eq!(0, unsafe { LAST_CONTENTS_LEN });

        // Read requests and malformed base64 must never reach the callback.
        vtw(t, b"\x1B]52;c;?\x1B\\");
        vtw(t, b"\x1B]52;c;%%%\x1B\\");
        assert_eq!(5, unsafe { COUNT });

        // iTerm2 Copy reaches the same normalized callback.
        vtw(t, b"\x1B]1337;Copy=:aVRlcm0=\x1B\\");
        assert_eq!(6, unsafe { COUNT });
        assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
        assert_eq!(b"text/plain".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
        });
        assert_eq!(b"iTerm".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
        });

        // Every representation is converted, and callback replies propagate
        // back through the C trampoline for protocols that can acknowledge
        // writes.
        // (Direct invoke of the INSTALLED handler effect — the dynamic
        // trampoline — mirroring the oracle's
        // `handler.effects.clipboard_write.?(handler, ...)`.)
        let internal_contents = [
            clipboard::Content {
                mime: b"text/plain",
                data: b"plain",
            },
            clipboard::Content {
                mime: b"application/octet-stream",
                data: b"a\x00b",
            },
            clipboard::Content {
                mime: b"text/html",
                data: b"<b>plain</b>",
            },
            clipboard::Content {
                mime: b"text/rtf",
                data: b"{\\rtf1 plain}",
            },
            clipboard::Content {
                mime: b"image/png",
                data: b"\x89PNG",
            },
        ];
        unsafe { NEXT_RESULT = clipboard::WriteStatus::Busy };
        // const handler = &t.?.stream.handler;
        // handler.effects.clipboard_write.?(handler, .{ ... });
        //
        // SB: take the handler address ONCE via a raw place projection;
        // the `.reply_ctx = handler` pointer and the `&mut` argument must
        // not be two separate overlapping retags.
        let installed = unsafe { (*t).stream.handler.effects.clipboard_write }.unwrap();
        let handler_ptr: *mut Handler<'static> =
            unsafe { std::ptr::addr_of_mut!((*t).stream.handler) };
        installed(
            unsafe { &mut *handler_ptr },
            clipboard::Write {
                location: clipboard::Location::Primary,
                contents: &internal_contents,
                name: b"app",
                granted: true,
                can_remember: true,
                reply_ctx: handler_ptr as *mut c_void,
                reply_fn: capture_reply,
            },
        );
        // try testing.expect(Reply.last.? == .busy);
        assert_eq!(Some(clipboard::WriteResult::Busy), unsafe { REPLY_RESULT });
        assert_eq!(3, unsafe { LAST_NAME_LEN });
        assert!(unsafe { LAST_GRANTED });
        assert!(unsafe { LAST_CAN_REMEMBER });
        assert_eq!(7, unsafe { COUNT });
        assert_eq!(5, unsafe { LAST_CONTENTS_LEN });
        assert_eq!(b"application/octet-stream".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[1].as_slice()
        });
        assert_eq!(b"a\x00b".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[1].as_slice()
        });
        assert_eq!(b"image/png".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[4].as_slice()
        });
        assert_eq!(b"\x89PNG".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[4].as_slice()
        });

        // Removing the callback takes effect immediately and uninstalls
        // the trampoline.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, std::ptr::null())
        });
        // try testing.expect(t.?.stream.handler.effects.clipboard_write == null);
        assert!(unsafe { (*t).stream.handler.effects.clipboard_write }.is_none());
        vtw(t, b"\x1B]52;c;eA==\x1B\\");
        assert_eq!(7, unsafe { COUNT });

        unsafe {
            LAST_MIMES = Vec::new();
            LAST_DATA = Vec::new();
            free(t);
        }
    });
}

// test "clipboard_write without callback is unsupported and silent"
#[test]
fn clipboard_write_without_callback_is_unsupported_and_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // OSC 52 without a callback should not crash
        vtw(t, b"\x1B]52;c;aGVsbG8=\x1B\\");

        // No trampoline is installed until a callback is set, so the
        // stream skips clipboard work (and never spools a Kitty clipboard
        // transaction it can't deliver).
        // try testing.expect(t.?.stream.handler.effects.clipboard_write == null);
        assert!(unsafe { (*t).stream.handler.effects.clipboard_write }.is_none());

        unsafe { free(t) };
    });
}

// test "kitty clipboard write via C effects" (oracle 6959fd46c/4f49dc2b8/
// 7ae9b1113 end state)
#[test]
fn kitty_clipboard_write_via_c_effects() {
    // const S = struct { var responses: [512]u8 = undefined; ... };
    // PORT NOTE: the fixed `[512]u8` capture buffers become `Vec`s (the
    // crate's test-capture convention); every assertion is carried.
    static mut RESPONSES: Vec<u8> = Vec::new();
    static mut WRITE_COUNT: usize = 0;
    static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
    static mut LAST_CONTENTS_LEN: usize = 0;
    static mut LAST_MIMES: Vec<Vec<u8>> = Vec::new();
    static mut LAST_DATA: Vec<Vec<u8>> = Vec::new();
    static mut LAST_GRANTED: bool = true;
    static mut LAST_CAN_REMEMBER: bool = true;
    static mut NEXT_REMEMBER: bool = false;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
        unsafe {
            (*std::ptr::addr_of_mut!(RESPONSES))
                .extend_from_slice(std::slice::from_raw_parts(ptr, len));
        }
    }

    extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
        unsafe {
            WRITE_COUNT += 1;
            LAST_LOCATION = (*request).location;
            LAST_CONTENTS_LEN = (*request).contents_len;
            let mut mimes: Vec<Vec<u8>> = Vec::new();
            let mut data: Vec<Vec<u8>> = Vec::new();
            if !(*request).contents.is_null() {
                let contents =
                    std::slice::from_raw_parts((*request).contents, (*request).contents_len);
                for c in contents {
                    mimes.push(std::slice::from_raw_parts(c.mime.ptr, c.mime.len).to_vec());
                    data.push(std::slice::from_raw_parts(c.data.ptr, c.data.len).to_vec());
                }
            }
            LAST_MIMES = mimes;
            LAST_DATA = data;
            LAST_GRANTED = (*request).granted;
            LAST_CAN_REMEMBER = (*request).can_remember;
            ((*request).reply)(
                request,
                &ClipboardWriteReply {
                    size: std::mem::size_of::<ClipboardWriteReply>(),
                    result: clipboard::WriteStatus::Success,
                    remember: NEXT_REMEMBER,
                },
            );
        }
    }

    // const R = struct { ... fn clipboardRead(...) ... };
    static mut R_COUNT: usize = 0;
    static mut R_LAST_MIMES_LEN: usize = 0;
    static mut R_LAST_MIME_IS_TEXT: bool = false;
    static mut R_LAST_LIST: bool = true;
    static mut R_LAST_NAME_LEN: usize = 0;
    static mut R_LAST_GRANTED: bool = true;
    static mut R_LAST_CAN_REMEMBER: bool = true;

    extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
        unsafe {
            R_COUNT += 1;
            R_LAST_MIMES_LEN = (*request).mimes_len;
            R_LAST_MIME_IS_TEXT = (*request).mimes_len > 0 && {
                let m = *(*request).mimes;
                std::slice::from_raw_parts(m.ptr, m.len) == b"text/plain"
            };
            R_LAST_LIST = (*request).list;
            R_LAST_NAME_LEN = (*request).name.len;
            R_LAST_GRANTED = (*request).granted;
            R_LAST_CAN_REMEMBER = (*request).can_remember;

            let mime: &[u8] = b"text/plain";
            let data: &[u8] = b"hello";
            let contents = [ClipboardContent {
                mime: LibString::init(mime),
                data: LibString::init(data),
            }];
            ((*request).reply)(
                request,
                &ClipboardReadReply {
                    size: std::mem::size_of::<ClipboardReadReply>(),
                    result: clipboard::ReadStatus::Success,
                    contents: contents.as_ptr(),
                    contents_len: contents.len(),
                    available: std::ptr::null(),
                    available_len: 0,
                    remember: false,
                },
            );
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            RESPONSES = Vec::new();
            WRITE_COUNT = 0;
            LAST_MIMES = Vec::new();
            LAST_DATA = Vec::new();
            LAST_GRANTED = true;
            LAST_CAN_REMEMBER = true;
            NEXT_REMEMBER = false;
            R_COUNT = 0;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
        });

        // A full OSC 5522 write transaction: begin, chunked data for two
        // representations, commit. Only the commit invokes the callback,
        // and its result maps to the DONE response.
        let seqs: [&[u8]; 5] = [
            b"\x1B]5522;type=write:id=c1\x1B\\",
            b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\", // "Ghost"
            b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;dHk=\x1B\\",     // "ty"
            b"\x1B]5522;type=wdata:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\", // "<b>hi</b>"
            b"\x1B]5522;type=wdata\x1B\\",
        ];
        for seq in seqs {
            vtw(t, seq);
        }

        assert_eq!(1, unsafe { WRITE_COUNT });
        assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
        assert_eq!(2, unsafe { LAST_CONTENTS_LEN });
        assert_eq!(b"text/plain".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
        });
        assert_eq!(b"Ghostty".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
        });
        assert_eq!(b"text/html".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_MIMES))[1].as_slice()
        });
        assert_eq!(b"<b>hi</b>".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA))[1].as_slice()
        });
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE:id=c1\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );
        assert!(!unsafe { LAST_GRANTED });
        assert!(!unsafe { LAST_CAN_REMEMBER });

        // Password grants round-trip through the C reply: the first pw'd
        // commit isn't granted and asks to remember, so the next one
        // arrives granted.
        unsafe {
            RESPONSES = Vec::new();
            NEXT_REMEMBER = true;
        }
        let grant_seqs: [&[u8]; 4] = [
            b"\x1B]5522;type=write:id=g1:pw=c2VjcmV0:name=YXBw\x1B\\",
            b"\x1B]5522;type=wdata\x1B\\",
            b"\x1B]5522;type=write:id=g2:pw=c2VjcmV0:name=YXBw\x1B\\",
            b"\x1B]5522;type=wdata\x1B\\",
        ];
        for seq in grant_seqs {
            vtw(t, seq);
        }
        assert_eq!(3, unsafe { WRITE_COUNT });
        assert!(unsafe { LAST_GRANTED });
        assert!(unsafe { LAST_CAN_REMEMBER });
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE:id=g1\x1B\\\x1B]5522;type=write:status=DONE:id=g2\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );
        unsafe { NEXT_REMEMBER = false };

        // Without a read callback reads are denied.
        unsafe { RESPONSES = Vec::new() };
        vtw(t, b"\x1B]5522;type=read:id=r1;dGV4dC9wbGFpbg==\x1B\\");
        assert_eq!(3, unsafe { WRITE_COUNT });
        assert_eq!(
            b"\x1B]5522;type=read:status=EPERM:id=r1\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );

        // With a read callback the request is served through it.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
        });
        unsafe { RESPONSES = Vec::new() };
        // name="app" without a password: forwarded for prompts, not
        // rememberable.
        vtw(
            t,
            b"\x1B]5522;type=read:id=r2:name=YXBw;dGV4dC9wbGFpbg==\x1B\\",
        );
        assert_eq!(1, unsafe { R_COUNT });
        assert_eq!(1, unsafe { R_LAST_MIMES_LEN });
        assert!(unsafe { R_LAST_MIME_IS_TEXT });
        assert!(!unsafe { R_LAST_LIST });
        assert_eq!(3, unsafe { R_LAST_NAME_LEN });
        assert!(!unsafe { R_LAST_GRANTED });
        assert!(!unsafe { R_LAST_CAN_REMEMBER });
        assert_eq!(
            b"\x1B]5522;type=read:status=OK:id=r2\x1B\\\x1B]5522;type=read:status=DATA:id=r2:mime=dGV4dC9wbGFpbg==;aGVsbG8=\x1B\\\x1B]5522;type=read:status=DONE:id=r2\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );

        // Without a clipboard callback the transaction fails up front.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, std::ptr::null())
        });
        unsafe { RESPONSES = Vec::new() };
        vtw(t, b"\x1B]5522;type=write:id=c2\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=write:status=ENOSYS:id=c2\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );

        unsafe {
            RESPONSES = Vec::new();
            LAST_MIMES = Vec::new();
            LAST_DATA = Vec::new();
            free(t);
        }
    });
}

// test "set clipboard write max bytes" (oracle 5350d4a5f/70f006575 end
// state: reject-not-truncate + configurable limit)
#[test]
fn set_clipboard_write_max_bytes() {
    static mut RESPONSES: Vec<u8> = Vec::new();
    static mut WRITE_COUNT: usize = 0;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
        unsafe {
            (*std::ptr::addr_of_mut!(RESPONSES))
                .extend_from_slice(std::slice::from_raw_parts(ptr, len));
        }
    }

    extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
        unsafe {
            WRITE_COUNT += 1;
            ((*request).reply)(
                request,
                &ClipboardWriteReply {
                    size: std::mem::size_of::<ClipboardWriteReply>(),
                    result: clipboard::WriteStatus::Success,
                    remember: false,
                },
            );
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            RESPONSES = Vec::new();
            WRITE_COUNT = 0;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
        });

        // The built-in default reads back.
        let mut max: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ClipboardWriteMaxBytes,
                &mut max as *mut usize as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::kitty::clipboard::max_write_size, max);

        // Set a tiny limit; an oversized text write fails with EFBIG and
        // never reaches the callback.
        let limit: usize = 4;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ClipboardWriteMaxBytes,
                &limit as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ClipboardWriteMaxBytes,
                &mut max as *mut usize as *mut c_void,
            )
        });
        assert_eq!(limit, max);

        let seqs: [&[u8]; 4] = [
            b"\x1B]5522;type=write:id=c1\x1B\\",
            b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbA==\x1B\\", // "Hell"
            b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bw==\x1B\\",     // "o"
            b"\x1B]5522;type=wdata\x1B\\",
        ];
        for seq in seqs {
            vtw(t, seq);
        }
        assert_eq!(0, unsafe { WRITE_COUNT });
        assert_eq!(
            b"\x1B]5522;type=write:status=EFBIG:id=c1\x1B\\".as_slice(),
            unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
        );

        // A NULL value reverts to the built-in default.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWriteMaxBytes, std::ptr::null())
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ClipboardWriteMaxBytes,
                &mut max as *mut usize as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::kitty::clipboard::max_write_size, max);

        unsafe {
            RESPONSES = Vec::new();
            free(t);
        }
    });
}

// test "set clipboard_read callback" (oracle e03475c0c)
#[test]
fn set_clipboard_read_callback() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    static mut COUNT: usize = 0;
    static mut LAST_SIZE: usize = 0;
    static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
    static mut LAST_MIMES_LEN: usize = 0;
    static mut LAST_MIME_IS_TEXT: bool = false;
    static mut LAST_LIST: bool = true;
    static mut LAST_NAME_LEN: usize = 1;
    static mut LAST_GRANTED: bool = true;
    static mut LAST_CAN_REMEMBER: bool = true;
    static mut RESULT: clipboard::ReadStatus = clipboard::ReadStatus::Success;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
        unsafe {
            LAST_DATA = Some(std::slice::from_raw_parts(ptr, len).to_vec());
        }
    }

    extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
        unsafe {
            COUNT += 1;
            LAST_SIZE = (*request).size;
            LAST_LOCATION = (*request).location;
            LAST_MIMES_LEN = (*request).mimes_len;
            LAST_MIME_IS_TEXT = (*request).mimes_len > 0 && {
                let m = *(*request).mimes;
                std::slice::from_raw_parts(m.ptr, m.len) == b"text/plain"
            };
            LAST_LIST = (*request).list;
            LAST_NAME_LEN = (*request).name.len;
            LAST_GRANTED = (*request).granted;
            LAST_CAN_REMEMBER = (*request).can_remember;

            let mime: &[u8] = b"text/plain";
            let data: &[u8] = b"hello";
            let contents = [ClipboardContent {
                mime: LibString::init(mime),
                data: LibString::init(data),
            }];
            ((*request).reply)(
                request,
                &ClipboardReadReply {
                    size: std::mem::size_of::<ClipboardReadReply>(),
                    result: RESULT,
                    contents: contents.as_ptr(),
                    contents_len: contents.len(),
                    available: std::ptr::null(),
                    available_len: 0,
                    remember: false,
                },
            );
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            LAST_DATA = None;
            COUNT = 0;
            RESULT = clipboard::ReadStatus::Success;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // Without a callback the handler effect is unset and reads are silent.
        assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_none());
        vtw(t, b"\x1B]52;c;?\x1B\\");
        assert!(unsafe { (&*std::ptr::addr_of!(LAST_DATA)).is_none() });

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
        });
        assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_some());

        vtw(t, b"\x1B]52;p;?\x07");
        assert_eq!(1, unsafe { COUNT });
        assert_eq!(std::mem::size_of::<ClipboardRead>(), unsafe { LAST_SIZE });
        assert_eq!(clipboard::Location::Primary, unsafe { LAST_LOCATION });
        assert_eq!(1, unsafe { LAST_MIMES_LEN });
        assert!(unsafe { LAST_MIME_IS_TEXT });
        assert!(!unsafe { LAST_LIST });
        assert_eq!(0, unsafe { LAST_NAME_LEN });
        assert!(!unsafe { LAST_GRANTED });
        assert!(!unsafe { LAST_CAN_REMEMBER });
        assert_eq!(b"\x1B]52;p;aGVsbG8=\x07".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA)).as_deref().unwrap()
        });

        // Denied replies with an empty clipboard.
        unsafe { RESULT = clipboard::ReadStatus::Denied };
        vtw(t, b"\x1B]52;c;?\x1B\\");
        assert_eq!(2, unsafe { COUNT });
        assert_eq!(b"\x1B]52;c;\x1B\\".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_DATA)).as_deref().unwrap()
        });

        // Clearing the callback uninstalls the handler effect.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardRead, std::ptr::null())
        });
        assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_none());

        unsafe {
            LAST_DATA = None;
            free(t);
        }
    });
}

// Parity S166-F1 (no reference test): the reference writes an OSC 52
// read reply's response from inside the reply (`ClipboardReadState.reply`
// -> `respond` -> `handler.writePty`), so a C embedder sees its
// write_pty callback run before its `reply` call returns. The C reply
// trampoline reaches the handler through the pointer the read trampoline
// stored in `ClipboardReadCtx`.
#[test]
fn clipboard_read_c_reply_writes_the_response_inside_reply() {
    static mut LOG: Vec<u8> = Vec::new();
    static mut LOG_LEN_AFTER_REPLY: Option<usize> = None;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
        unsafe {
            (&mut *std::ptr::addr_of_mut!(LOG))
                .extend_from_slice(std::slice::from_raw_parts(ptr, len));
        }
    }

    extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
        unsafe {
            let contents = [ClipboardContent {
                mime: LibString::init(b"text/plain"),
                data: LibString::init(b"hello"),
            }];
            ((*request).reply)(
                request,
                &ClipboardReadReply {
                    size: std::mem::size_of::<ClipboardReadReply>(),
                    result: clipboard::ReadStatus::Success,
                    contents: contents.as_ptr(),
                    contents_len: contents.len(),
                    available: std::ptr::null(),
                    available_len: 0,
                    remember: false,
                },
            );
            LOG_LEN_AFTER_REPLY = Some((&*std::ptr::addr_of!(LOG)).len());
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            LOG = Vec::new();
            LOG_LEN_AFTER_REPLY = None;
        }
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
        });

        vtw(t, b"\x1B]52;c;?\x1B\\");
        let expected: &[u8] = b"\x1B]52;c;aGVsbG8=\x1B\\";
        assert_eq!(Some(expected.len()), unsafe { LOG_LEN_AFTER_REPLY });
        assert_eq!(expected, unsafe { (&*std::ptr::addr_of!(LOG)).as_slice() });

        unsafe {
            LOG = Vec::new();
            free(t);
        }
    });
}

// test "mode 2048 enable and disable use C callbacks" (oracle 07af4612b)
#[test]
fn mode_2048_enable_and_disable_use_c_callbacks() {
    static mut DATA: [u8; 128] = [0; 128];
    static mut LEN: usize = 0;
    static mut CALLS: usize = 0;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, length: usize) {
        unsafe {
            let dst = std::ptr::addr_of_mut!(DATA) as *mut u8;
            std::ptr::copy_nonoverlapping(ptr, dst, length);
            LEN = length;
            CALLS += 1;
        }
    }

    extern "C" fn size_cb(_: Terminal, _: *mut c_void, out_size: *mut size_report::Size) -> bool {
        unsafe {
            *out_size = size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 8,
                cell_height: 16,
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            LEN = 0;
            CALLS = 0;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::SizeCb, size_cb as *const c_void)
        });

        vtw(t, b"\x1B[?2048h");
        vtw(t, b"\x1B[?2048h");

        assert_eq!(2, unsafe { CALLS });
        assert_eq!(b"\x1B[48;24;80;384;640t".as_slice(), unsafe {
            &(&*std::ptr::addr_of!(DATA))[..LEN]
        });
        vtw(t, b"\x1B[?2048l");

        assert_eq!(2, unsafe { CALLS });
        // try testing.expect(!t.?.terminal.modes.get(.in_band_size_reports));
        assert!(!unsafe {
            (*(*t).terminal)
                .modes
                .get(rustty_vt::modes::Mode::InBandSizeReports)
        });

        unsafe { free(t) };
    });
}

// test "mode 2048 enable tolerates missing C callbacks" (oracle 07af4612b)
#[test]
fn mode_2048_enable_tolerates_missing_c_callbacks() {
    static mut WRITES: usize = 0;
    static mut SIZES: usize = 0;

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {
        unsafe { WRITES += 1 };
    }

    extern "C" fn size_cb(_: Terminal, _: *mut c_void, out_size: *mut size_report::Size) -> bool {
        unsafe {
            SIZES += 1;
            *out_size = size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 8,
                cell_height: 16,
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            WRITES = 0;
            SIZES = 0;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        vtw(t, b"\x1B[?2048h");

        assert_eq!(0, unsafe { WRITES });
        assert!(unsafe {
            (*(*t).terminal)
                .modes
                .get(rustty_vt::modes::Mode::InBandSizeReports)
        });

        // var no_write: Terminal = null;
        let (no_write_result, no_write) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, no_write_result);
        assert_eq!(Result::Success, unsafe {
            set(no_write, Option_::SizeCb, size_cb as *const c_void)
        });
        vtw(no_write, b"\x1B[?2048h");
        assert_eq!(1, unsafe { SIZES });
        assert!(unsafe {
            (*(*no_write).terminal)
                .modes
                .get(rustty_vt::modes::Mode::InBandSizeReports)
        });

        unsafe {
            free(no_write);
            free(t);
        }
    });
}

// test "pwd_changed without callback is silent"
#[test]
fn pwd_changed_without_callback_is_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // OSC 7 without a callback should not crash, but should still set the pwd
        vtw(t, b"\x1B]7;file:///tmp\x1B\\");
        assert_eq!(b"file:///tmp".as_slice(), unsafe {
            (*zig_terminal(t)).get_pwd().unwrap()
        });
        unsafe { free(t) };
    });
}

// test "set size callback"
#[test]
fn set_size_callback() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    extern "C" fn size_cb(
        _t: Terminal,
        _ud: *mut c_void,
        out_size: *mut size_report::Size,
    ) -> bool {
        unsafe {
            *out_size = size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 8,
                cell_height: 16,
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::SizeCb, size_cb as *const c_void)
        });

        // CSI 18 t — report text area size in characters
        vtw(t, b"\x1B[18t");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1b[8;24;80t".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "size without callback is silent"
#[test]
fn size_without_callback_is_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // CSI 18 t without a size callback should not crash
        vtw(t, b"\x1B[18t");
        unsafe { free(t) };
    });
}

// test "set device_attributes callback primary"
#[test]
fn set_device_attributes_callback_primary() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
        let mut features = [0u16; 64];
        features[0] = 22;
        features[1] = 52;
        unsafe {
            *out = CDeviceAttributes {
                primary: CDeviceAttributesPrimary {
                    conformance_level: 64,
                    features,
                    num_features: 2,
                },
                secondary: CDeviceAttributesSecondary {
                    device_type: 1,
                    firmware_version: 10,
                    rom_cartridge: 0,
                },
                tertiary: CDeviceAttributesTertiary { unit_id: 0 },
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::DeviceAttributes, da as *const c_void)
        });

        // CSI c — primary DA
        vtw(t, b"\x1B[c");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1b[?64;22;52c".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "set device_attributes callback secondary"
#[test]
fn set_device_attributes_callback_secondary() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
        let mut features = [0u16; 64];
        features[0] = 22;
        unsafe {
            *out = CDeviceAttributes {
                primary: CDeviceAttributesPrimary {
                    conformance_level: 62,
                    features,
                    num_features: 1,
                },
                secondary: CDeviceAttributesSecondary {
                    device_type: 1,
                    firmware_version: 10,
                    rom_cartridge: 0,
                },
                tertiary: CDeviceAttributesTertiary { unit_id: 0 },
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::DeviceAttributes, da as *const c_void)
        });

        // CSI > c — secondary DA
        vtw(t, b"\x1B[>c");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1b[>1;10;0c".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "set device_attributes callback tertiary"
#[test]
fn set_device_attributes_callback_tertiary() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
        unsafe {
            *out = CDeviceAttributes {
                primary: CDeviceAttributesPrimary {
                    conformance_level: 62,
                    features: [0u16; 64],
                    num_features: 0,
                },
                secondary: CDeviceAttributesSecondary {
                    device_type: 1,
                    firmware_version: 0,
                    rom_cartridge: 0,
                },
                tertiary: CDeviceAttributesTertiary {
                    unit_id: 0xAABBCCDD,
                },
            };
        }
        true
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::DeviceAttributes, da as *const c_void)
        });

        // CSI = c — tertiary DA
        vtw(t, b"\x1B[=c");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1bP!|AABBCCDD\x1b\\".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "device_attributes without callback uses default"
#[test]
fn device_attributes_without_callback_uses_default() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // Without setting a device_attributes callback, DA1 should return the default
        vtw(t, b"\x1B[c");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1b[?62;22c".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "device_attributes callback returns false uses default"
#[test]
fn device_attributes_callback_returns_false_uses_default() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    extern "C" fn da(_t: Terminal, _ud: *mut c_void, _out: *mut CDeviceAttributes) -> bool {
        false
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::DeviceAttributes, da as *const c_void)
        });

        // Callback returns false, should use default response
        vtw(t, b"\x1B[c");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1b[?62;22c".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}
