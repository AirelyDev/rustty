//! Trimmed stream.zig fixture for the G9 generator self-test (N517).
//! csiDispatch deliberately has NO 'm' arm (negative control).

const Stream = struct {
    pub fn Stream(comptime Self: type) type {
    return struct {
        inline fn csiDispatch(self: *Self, input: Parser.Action.CSI) void {
            switch (input.final) {
                'A' => self.handler.vt(.stub, .{}),
                'B' => self.handler.vt(.stub, .{}),
                'C' => self.handler.vt(.stub, .{}),
                'D' => self.handler.vt(.stub, .{}),
                'E' => self.handler.vt(.stub, .{}),
                'F' => self.handler.vt(.stub, .{}),
                'G' => self.handler.vt(.stub, .{}),
                'H' => self.handler.vt(.stub, .{}),
                'I' => self.handler.vt(.stub, .{}),
                'J' => self.handler.vt(.stub, .{}),
                'K' => self.handler.vt(.stub, .{}),
                'L' => self.handler.vt(.stub, .{}),
                'M' => self.handler.vt(.stub, .{}),
                'P' => self.handler.vt(.stub, .{}),
                'S' => self.handler.vt(.stub, .{}),
                'T' => self.handler.vt(.stub, .{}),
                'W' => self.handler.vt(.stub, .{}),
                'X' => self.handler.vt(.stub, .{}),
                'Z' => self.handler.vt(.stub, .{}),
                'a' => self.handler.vt(.stub, .{}),
                'b' => self.handler.vt(.stub, .{}),
                'c' => self.handler.vt(.stub, .{}),
                'd' => self.handler.vt(.stub, .{}),
                'e' => self.handler.vt(.stub, .{}),
                'f' => self.handler.vt(.stub, .{}),
                'g' => self.handler.vt(.stub, .{}),
                'h' => self.handler.vt(.stub, .{}),
                'j' => self.handler.vt(.stub, .{}),
                'k' => self.handler.vt(.stub, .{}),
                'l' => self.handler.vt(.stub, .{}),
                'n' => self.handler.vt(.stub, .{}),
                'p' => self.handler.vt(.stub, .{}),
                'q' => self.handler.vt(.stub, .{}),
                'r' => self.handler.vt(.stub, .{}),
                's' => self.handler.vt(.stub, .{}),
                't' => self.handler.vt(.stub, .{}),
                'u' => self.handler.vt(.stub, .{}),
                '@' => self.handler.vt(.stub, .{}),
                '`' => self.handler.vt(.stub, .{}),
                else => log.warn("unknown CSI '{c}'", .{}),
            }
        }

        inline fn escDispatch(
            self: *Self,
            action: Parser.Action.ESC,
        ) void {
            switch (action.final) {
                'B' => self.handler.vt(.stub, .{}),
                'A' => self.handler.vt(.stub, .{}),
                '0' => self.handler.vt(.stub, .{}),
                '7' => self.handler.vt(.stub, .{}),
                '8' => self.handler.vt(.stub, .{}),
                'D' => self.handler.vt(.stub, .{}),
                'E' => self.handler.vt(.stub, .{}),
                'H' => self.handler.vt(.stub, .{}),
                'M' => self.handler.vt(.stub, .{}),
                'N' => self.handler.vt(.stub, .{}),
                'O' => self.handler.vt(.stub, .{}),
                'V' => self.handler.vt(.stub, .{}),
                'W' => self.handler.vt(.stub, .{}),
                'Z' => self.handler.vt(.stub, .{}),
                'c' => self.handler.vt(.stub, .{}),
                'n' => self.handler.vt(.stub, .{}),
                'o' => self.handler.vt(.stub, .{}),
                '~' => self.handler.vt(.stub, .{}),
                '}' => self.handler.vt(.stub, .{}),
                '|' => self.handler.vt(.stub, .{}),
                '=' => self.handler.vt(.stub, .{}),
                '>' => self.handler.vt(.stub, .{}),
                '\\' => self.handler.vt(.stub, .{}),
                else => {},
            }
        }
    };
    }
};
