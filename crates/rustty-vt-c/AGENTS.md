# librustty-vt C API

- C API must be designed with ABI compatibility in mind.
- Rust tagged enums (the Zig `union(enum)` analogs) must be projected to a
  C-ABI-compatible `extern union` + tag surface. This crate hosts those
  hand-authored projections (the rustty analog of Zig's
  `lib.TaggedUnion(...)`); see `src/sgr.rs` for a worked example.
- The C ABI is emitted by **cbindgen**, not by hand-written `@export` lines.
  To add or change a function, propagate it all the way through:
  1. Author the function in `crates/rustty-vt-c/src/<module>.rs` as a
     `#[no_mangle] pub extern "C" fn rustty_<name>(...)` (the `rustty_` prefix
     is the published symbol name). The `extern "C"` wrapper is the single
     point that crosses the C ABI; it forwards to the ported `rustty-vt`
     logic, which is the source of truth.
  2. Make sure the module is declared with `pub mod <module>;` in
     `src/lib.rs` (the C API hub, analog of Zig's `terminal/c/main.zig`) so
     cbindgen sees the symbol.
  3. Regenerate the header with `cargo run -p xtask -- header-gen` (which
     invokes cbindgen per `crates/rustty-vt-c/cbindgen.toml`, emitting
     `include/rustty.h`). The public per-module header family lives under
     `include/rustty/vt/` (with the umbrella `include/rustty/vt.h`); keep the
     corresponding declaration and its Doxygen documentation in the matching
     header there.
- In the hand-maintained public headers under `include/rustty/vt/`, keep the
  header contents sorted by: (1) macros, (2) forward declarations, (3) types,
  (4) functions.

## ABI Compatibility

- Prefer opaque pointers for long-lived objects, such as
  `RusttyTerminal`.
- Structs:
  - May contain padding bytes if we're confident we'll never grow
    beyond a certain size.
  - May use the "sized struct" pattern: a `#[repr(C)]` struct with
    `size: usize = size_of::<Self>()` as the first field. In the C header,
    callers use `RUSTTY_INIT_SIZED` from `types.h` to zero-initialize and
    set the size.
