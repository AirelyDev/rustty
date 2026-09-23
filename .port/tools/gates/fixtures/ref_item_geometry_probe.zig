// RUSTTY REF-ITEM GEOMETRY PROBE (task R13-L2, NEW-FINDINGS N88 / N91).
//
// NOT a Zig source file of its own: `check_ref_item_geometry.py` appends this
// text to a SCRATCH COPY of the pinned reference's `src/terminal/page.zig`
// (never to the reference itself), so every name below is one that file
// already has in scope: `std`, `builtin`, `color`, `hyperlink`, `stylepkg`,
// `size`.
//
// It reports the size and alignment of the types the port's page layout is
// tied to: the style set's and the hyperlink set's `RefCountedSet.Item` (the
// port lays both sets out at these strides, `ref_counted_set::RefItemGeometry`)
// and the declarations they are built from. The values are compile-time
// constants, so the probe reports them through `@compileError` rather than a
// test run: one `zig build test-lib-vt-build -Dtarget=<t>` per target reports
// the values for that target without executing anything, which is what lets a
// macOS host report `aarch64-linux-gnu` and `wasm32-wasi` too. The build is
// EXPECTED to fail with exactly this message; the gate parses the lines
// between the BEGIN and END markers.
//
// Line format: `<arch>-<os> <name> <size> <align>`.
comptime {
    const Probe = struct {
        fn line(comptime name: []const u8, comptime T: type) []const u8 {
            return std.fmt.comptimePrint("{s}-{s} {s} {d} {d}\n", .{
                @tagName(builtin.cpu.arch),
                @tagName(builtin.os.tag),
                name,
                @sizeOf(T),
                @alignOf(T),
            });
        }
    };
    // The markers are split in the source, because the compiler echoes the
    // source line of the `@compileError` and the gate must find each marker
    // only in the message itself.
    @compileError("RUSTTY-REF-ITEM-GEOMETRY-" ++ "PROBE BEGIN\n" ++
        Probe.line("usize", usize) ++
        Probe.line("color.RGB", color.RGB) ++
        Probe.line("style.Style.Color", stylepkg.Style.Color) ++
        Probe.line("style.Style.Flags", @FieldType(stylepkg.Style, "flags")) ++
        Probe.line("style.Style", stylepkg.Style) ++
        Probe.line("style.Set.Item.Metadata", stylepkg.Set.Item.Metadata) ++
        Probe.line("style.Set.Item", stylepkg.Set.Item) ++
        Probe.line("size.Offset(u8).Slice", size.Offset(u8).Slice) ++
        Probe.line("hyperlink.PageEntry.Id", hyperlink.PageEntry.Id) ++
        Probe.line("hyperlink.PageEntry", hyperlink.PageEntry) ++
        Probe.line("hyperlink.Set.Item.Metadata", hyperlink.Set.Item.Metadata) ++
        Probe.line("hyperlink.Set.Item", hyperlink.Set.Item) ++
        "RUSTTY-REF-ITEM-GEOMETRY-" ++ "PROBE END");
}
