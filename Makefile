init:
	@echo You probably want to run "cargo build" instead.
.PHONY: init

# Remove build outputs: the Cargo target tree, the generated resource tree, the
# XCFrameworks and their staged headers, the macOS Xcode build products, the
# CMake wrapper build dir, and the Doxygen output.
#
# `zig-out/` is not a leftover of the Zig build system here — it is where
# `cargo run -p xtask -- resources` writes its ~1,200 generated files (man
# pages, the compiled terminfo database, the .mo catalogues, themes, shell
# integration, the Linux desktop/dbus/systemd units), keeping the reference's
# path so the packaging manifests copy from the same place. It was missing from
# this target, which is how a `share/ghostty/` tree and `ghostty.terminfo` from
# before the rename survived here for six weeks alongside the current output.
# The reference's clean removes `zig-out` too.
clean:
	cargo clean
	rm -rf \
		zig-out \
		dist/RusttyKit.xcframework \
		dist/rustty-vt.xcframework \
		dist/xcframework-headers-* \
		macos/build \
		macos/DerivedData \
		build-cmake \
		doc
.PHONY: clean
