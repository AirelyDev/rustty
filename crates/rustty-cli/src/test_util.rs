//! Test-only helpers shared by the `action`/`ghostty` test modules.
//!
//! `std.process.ArgIteratorGeneral(.{})` test helper.
//!
//! PORT NOTE: the reference tests use `std.process.ArgIteratorGeneral(.{}).init`,
//! which tokenizes a single command-line string into args, splitting on
//! whitespace, honoring double-quote groups (the quotes are stripped) and
//! backslash escapes. Reproduced here as a test-only helper yielding the same
//! `?[]const u8` (== `Option<Vec<u8>>`) sequence via [`crate::action::ArgsIter`].
//! With default options single quotes and comments are NOT special. (This mirrors
//! the identical helper in `rustty-args`' own test module.)

use crate::action::ArgsIter;

pub(crate) struct ArgIteratorGeneral {
    args: Vec<Vec<u8>>,
    idx: usize,
}

impl ArgIteratorGeneral {
    pub(crate) fn init(cmd: &[u8]) -> ArgIteratorGeneral {
        let mut args: Vec<Vec<u8>> = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        let mut in_token = false;
        let mut in_quote = false;
        let mut i = 0;
        while i < cmd.len() {
            let c = cmd[i];
            if in_quote {
                match c {
                    b'"' => {
                        in_quote = false;
                        i += 1;
                    }
                    b'\\' => {
                        // Backslash escape inside quotes: next char literal.
                        if i + 1 < cmd.len() {
                            cur.push(cmd[i + 1]);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        cur.push(c);
                        i += 1;
                    }
                }
            } else {
                match c {
                    b' ' | b'\t' | b'\n' | b'\r' => {
                        if in_token {
                            args.push(std::mem::take(&mut cur));
                            in_token = false;
                        }
                        i += 1;
                    }
                    b'"' => {
                        in_token = true;
                        in_quote = true;
                        i += 1;
                    }
                    b'\\' => {
                        in_token = true;
                        if i + 1 < cmd.len() {
                            cur.push(cmd[i + 1]);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        in_token = true;
                        cur.push(c);
                        i += 1;
                    }
                }
            }
        }
        if in_token {
            args.push(cur);
        }
        ArgIteratorGeneral { args, idx: 0 }
    }

    pub(crate) fn deinit(&mut self) {}
}

impl ArgsIter for ArgIteratorGeneral {
    fn next(&mut self) -> Option<Vec<u8>> {
        if self.idx >= self.args.len() {
            return None;
        }
        let v = self.args[self.idx].clone();
        self.idx += 1;
        Some(v)
    }
}
