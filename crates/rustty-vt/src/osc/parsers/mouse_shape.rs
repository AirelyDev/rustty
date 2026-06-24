//! PORTS: ghostty src/terminal/osc/parsers/mouse_shape.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

// Parse OSC 22
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"22");
    debug_assert!(parser.state == crate::osc::State::S22);
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    if !cap.write_byte(0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing();
    parser.command = Command::MouseShape {
        // data[0 .. data.len() - 1 :0]
        // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes (trailing NUL
        // excluded), NOT a lossy String decode.
        value: data[0..data.len() - 1].to_vec(),
    };
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_22_pointer_cursor() {
        let mut p: Parser = Parser::init(None);

        let input = "22;pointer";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::MouseShape { .. }));
        match cmd {
            crate::osc::Command::MouseShape { value } => assert_eq!(b"pointer", value.as_slice()),
            _ => unreachable!(),
        }
    }
}
