# PORTS: ghostty nix/devShell.nix.
#
# The Rust toolchain replaces zig + zon2nix; everything else that is genuinely
# used by rustty's workflow carries over. The single most load-bearing job this
# file does is PIN THE LINT TOOL VERSIONS — HACKING.md tells contributors to
# match "the version in nix/devShell.nix", and until this file existed that
# sentence pointed at nothing. The G-LINT gate runs exactly these six.
#
# DROPPED from the reference, with reasons:
#   zig, zon2nix          no Zig toolchain; crane vendors from Cargo.lock
#   twiggy, wizer, poop   zig/wasm-size tooling with no rustty consumer yet
#   snapcraft             commented out in the reference too
#   glxinfo, vulkan-loader commented out in the reference ("unused")
#   bzip2 expat fontconfig freetype harfbuzz libpng libxml2 oniguruma simdutf
#   zlib glslang spirv-cross
#                         rustty VENDORS these C libraries (crates-sys/*/vendor,
#                         built by the cc crate); see
#                         build-support/build-inputs.nix for the same list
#   the macOS AFL++ `unset NIX_CC ... LDFLAGS` block
#                         the reference's fuzzer is AFL++ (afl-cc); rustty's is
#                         cargo-fuzz/libFuzzer (crates/rustty-vt/fuzz), which
#                         does not go through afl-cc
{
  mkShell,
  lib,
  stdenv,
  pkgs,
  rustToolchain,
  bashInteractive,
  doxygen,
  nushell,
  appstream,
  flatpak-builder,
  gdb,
  cmake,
  ncurses,
  nodejs,
  prettier,
  parallel,
  pkg-config,
  python3,
  qemu,
  scdoc,
  valgrind,
  vttest,
  wabt,
  wasm-tools,
  wasmtime,
  binaryen,
  wraptest,
  zip,
  llvmPackages_latest,
  rustPlatform,
  rust-cbindgen,
  glib,
  gtk4,
  gtk4-layer-shell,
  gobject-introspection,
  gst_all_1,
  libadwaita,
  blueprint-compiler,
  gettext,
  adwaita-icon-theme,
  hicolor-icon-theme,
  libxkbcommon,
  libX11,
  libXcursor,
  libXext,
  libXi,
  libXinerama,
  libXrandr,
  alejandra,
  jq,
  kaitai-struct-compiler,
  minisign,
  pandoc,
  pinact,
  hyperfine,
  typos,
  shellcheck,
  swiftlint,
  uv,
  wayland,
  wayland-scanner,
  wayland-protocols,
  # needed by GTK for loading SVG icons while running from within the
  # developer shell
  glycin-loaders,
  librsvg,
}: let
  # See librustty-vt.nix / package.nix. Keep in sync.
  ld_library_path = import ./build-support/ld-library-path.nix {
    inherit pkgs lib stdenv;
  };
  gi_typelib_path = import ./build-support/gi-typelib-path.nix {
    inherit pkgs lib stdenv;
  };
  python = python3.withPackages (python-pkgs: [
    python-pkgs.jsonschema
    python-pkgs.kaitaistruct
    python-pkgs.ucs-detect
    # crates/rustty-vt-c/types-schema-verify.py (G-ABISCHEMA) imports it.
    python-pkgs.wasmtime
  ]);
in
  mkShell {
    name = "rustty";
    packages =
      [
        # The pinned Rust toolchain, read from rust-toolchain.toml by
        # flake.nix so this shell and `cargo` outside Nix agree exactly.
        rustToolchain

        # For builds
        rust-cbindgen
        cmake
        # bindgen (crates-sys/{oniguruma,libxml2,wuffs,...}-sys) needs BOTH
        # libclang to dlopen AND the libc include paths to parse vendored
        # headers. Setting LIBCLANG_PATH by hand covers only the first: with it
        # alone, `cargo test -p rustty-oniguruma` worked but
        # `cargo test -p rustty-wuffs` still died with
        #   fatal error: 'stdlib.h' file not found
        # bindgenHook is nixpkgs' canonical answer and sets both (it is what
        # nix/package.nix uses). mkShell runs setup hooks, so it belongs here
        # rather than as hand-maintained environment variables.
        rustPlatform.bindgenHook
        doxygen
        jq
        llvmPackages_latest.llvm
        minisign
        ncurses
        pandoc
        pkg-config
        scdoc
        zip

        # For web and wasm stuff
        nodejs

        # Linting. These are the six the G-LINT gate
        # (.port/tools/gates/check_lints.sh) runs; this is their version pin.
        prettier
        alejandra
        pinact
        typos
        shellcheck

        # Testing
        parallel
        python
        vttest
        hyperfine
        # G-KAITAI (.port/tools/gates/check_kaitai_snapshot.sh) compiles the
        # snapshot .ksy with it.
        kaitai-struct-compiler

        # wasm
        binaryen
        wabt
        wasm-tools
        wasmtime

        # Localization. `xtask update-translations` needs xgettext/msgmerge,
        # and `xtask resources` needs msgfmt.
        gettext

        # CI
        uv

        # Scripting
        nushell

        # Needed on every platform so `xtask dist` can stage the GTK
        # resources into the source tarball.
        blueprint-compiler
        libadwaita
        gtk4
      ]
      ++ lib.optionals stdenv.hostPlatform.isLinux [
        # A non-interactive bash is the default in some Nix setups.
        bashInteractive

        # Used for testing SIMD codegen. Linux only because the macOS build
        # only ships the qemu-system files.
        qemu

        appstream
        flatpak-builder
        gdb
        valgrind
        wraptest

        libxkbcommon
        libX11
        libXcursor
        libXext
        libXi
        libXinerama
        libXrandr

        # Only needed for GTK builds
        gtk4-layer-shell
        glib
        gobject-introspection
        wayland
        wayland-scanner
        wayland-protocols
        gst_all_1.gstreamer
        gst_all_1.gst-plugins-base
        gst_all_1.gst-plugins-good

        # needed by GTK for loading SVG icons while running from within the
        # developer shell
        glycin-loaders
        librsvg
      ]
      ++ lib.optionals stdenv.hostPlatform.isDarwin [
        swiftlint
      ];

    # This should be set onto the rpath of the rustty binary if you want it to
    # be "portable" across the system: `cargo run -p xtask -- patchelf
    # target/debug/rustty` does that, and sets the interpreter to the ld.so on
    # PATH (the reference's patch-interp/patch-rpath defaults, S63-F4).
    LD_LIBRARY_PATH = ld_library_path;
    GI_TYPELIB_PATH = gi_typelib_path;

    shellHook =
      (lib.optionalString stdenv.hostPlatform.isLinux ''
        # On Linux we need to set up the environment so that all GTK data is
        # available (namely icons).

        # Minimal subset of env set by wrapGAppsHook4 for icons and global settings
        export XDG_DATA_DIRS=$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${adwaita-icon-theme}/share
        export XDG_DATA_DIRS=$XDG_DATA_DIRS:$GSETTINGS_SCHEMAS_PATH # from glib setup hook
      '')
      + (lib.optionalString stdenv.hostPlatform.isDarwin ''
        # On macOS we unset the Nix macOS SDK env vars because the Rustty app
        # build relies on a system Xcode installation; Nix only provides an SDK.
        unset SDKROOT
        unset DEVELOPER_DIR

        # Remove "xcrun" injected by a Nix dependency so the system Xcode tools
        # win — crates/rustty-renderer's build.rs shells out to `xcrun metal`.
        export PATH=$(echo "$PATH" | awk -v RS=: -v ORS=: '$0 !~ /xcrun/ || $0 == "/usr/bin" {print}' | sed 's/:$//')
        export PATH="${python}/bin:/opt/homebrew/opt/llvm/bin:/opt/homebrew/bin:/usr/local/opt/llvm/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
      '');
  }
