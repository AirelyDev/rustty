init:
	@echo You probably want to run "cargo build" instead.
.PHONY: init

# Remove build outputs: the Cargo target tree, the macOS xcframework, the
# CMake wrapper build dir, and the Doxygen output.
clean:
	cargo clean
	rm -rf \
		dist/RusttyKit.xcframework \
		build-cmake \
		doc
.PHONY: clean
