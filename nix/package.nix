# PORTS: ghostty nix/package.nix — the GTK Linux application.
#
# NOT a line port: the reference drives `zig build`, which produces the binary
# AND the whole `zig-out/share` resource tree in one step. rustty splits those:
# `cargo build -p rustty` makes the binary, and `xtask resources --target-os
# linux --prefix <p>` stages terminfo, shell integration, themes, man pages,
# completions, locale and the desktop-app files. Both run here.
#
# Vendoring is crane's, same as nix/librustty-vt.nix — see that file for why the
# reference's zon2nix / zigCacheHash complex has no analog.
{
  lib,
  stdenv,
  callPackage,
  craneLib,
  gobject-introspection,
  blueprint-compiler,
  libxml2,
  gettext,
  wrapGAppsHook4,
  git,
  ncurses,
  pkg-config,
  pandoc,
  rustPlatform,
  revision ? "dirty",
  optimize ? "releasefast",
  enableX11 ? true,
  enableWayland ? true,
  wayland-protocols,
  wayland-scanner,
  pkgs,
}: let
  gi_typelib_path = import ./build-support/gi-typelib-path.nix {
    inherit pkgs lib stdenv;
  };
  buildInputs' = import ./build-support/build-inputs.nix {
    inherit pkgs lib stdenv enableX11 enableWayland;
  };

  # Same mapping as nix/librustty-vt.nix. `debug` is cargo's `dev`.
  cargoProfile =
    {
      debug = "dev";
      releasesafe = "release";
      releasefast = "release";
    }
    .${
      optimize
    };
  targetSubdir =
    if cargoProfile == "dev"
    then "debug"
    else "release";
  strip = optimize != "debug" && optimize != "releasesafe";

  # The reference's `-Doptimize=` spelling, which `xtask resources --optimize`
  # takes: it picks the terminfo-source/termcap defaults and the `-debug` app
  # id, exactly as `-Doptimize` does for the reference's resource install.
  xtaskOptimize =
    {
      debug = "Debug";
      releasesafe = "ReleaseSafe";
      releasefast = "ReleaseFast";
    }
    .${
      optimize
    };

  # `-Dgtk-x11` / `-Dgtk-wayland` are cargo features here. `gtk` itself is
  # implied by both, but name it so an all-false build still selects the GTK
  # apprt rather than the headless one.
  #
  # `simd` is `-Dsimd` (default true, N420): a default feature of `rustty`, so
  # it must be named here, where the defaults are dropped.
  features =
    ["gtk" "simd"]
    ++ lib.optional enableX11 "x11"
    ++ lib.optional enableWayland "wayland";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.intersection (lib.fileset.fromSource (lib.sources.cleanSource ../.)) (
      lib.fileset.unions [
        ../Cargo.toml
        ../Cargo.lock
        ../rust-toolchain.toml
        ../crates
        ../crates-sys
        ../dist/linux
        ../images
        ../include
        ../po
        ../xtask
      ]
    );
  };
in
  stdenv.mkDerivation (finalAttrs: {
    pname = "rustty";
    version = "0.1.0-dev+${revision}-nix";

    inherit src;

    cargoVendorDir = craneLib.vendorCargoDeps {inherit src;};

    nativeBuildInputs =
      [
        git
        ncurses
        pandoc
        pkg-config
        gobject-introspection
        wrapGAppsHook4
        blueprint-compiler
        libxml2 # for xmllint
        gettext # xtask resources shells out to msgfmt for the .mo catalogs
        craneLib.configureCargoVendoredDepsHook
        craneLib.cargo
        # crates-sys/libxml2-sys (and the other bindgen-driven -sys crates the
        # GTK app pulls in) call bindgen at build time, which dlopens libclang:
        #   "Unable to find libclang ... set the LIBCLANG_PATH environment
        #    variable"
        # bindgenHook is nixpkgs' canonical fix — it exports LIBCLANG_PATH and
        # the BINDGEN_EXTRA_CLANG_ARGS the sandbox needs to find libc headers.
        # librustty-vt.nix needs none of this: rustty-vt-c's closure has no
        # bindgen consumer.
        rustPlatform.bindgenHook
      ]
      ++ lib.optionals enableWayland [
        wayland-scanner
        wayland-protocols
      ];

    buildInputs = buildInputs';

    dontStrip = !strip;
    # The reference passes `-Dstrip=${lib.boolToString strip}`; the Cargo
    # option is the profile's strip (N422). Both `releasesafe` and
    # `releasefast` build the `release` profile, which strips by default, so
    # this is what keeps a releasesafe build's symbols.
    CARGO_PROFILE_RELEASE_STRIP = lib.boolToString strip;

    GI_TYPELIB_PATH = gi_typelib_path;

    # See nix/librustty-vt.nix: crane's vendored-deps hook hard-fails without
    # CARGO_HOME, and it must exist before preConfigure runs.
    postUnpack = ''
      export CARGO_HOME="$NIX_BUILD_TOP/cargo-home"
      mkdir -p "$CARGO_HOME"
    '';
    CARGO_NET_OFFLINE = "true";

    dontConfigure = false;
    configurePhase = ''
      runHook preConfigure
      runHook postConfigure
    '';

    buildPhase = ''
      runHook preBuild

      cargo build --offline --frozen \
        --profile ${cargoProfile} \
        -p rustty \
        --no-default-features \
        --features "${lib.concatStringsSep " " features}"

      # The resource tree. --prefix is the FINAL install prefix baked into the
      # .desktop Exec= lines and the D-Bus service, so it must be $out and not
      # the reference's /usr default.
      cargo run --offline --frozen -p xtask -- resources \
        --target-os linux \
        --prefix "$out" \
        --optimize ${xtaskOptimize}

      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall

      install -Dm755 "target/${targetSubdir}/rustty" "$out/bin/rustty"
      mkdir -p "$out/share"
      # The tree exactly as `xtask resources` staged it. Deliberately NOT
      # relocating the systemd user unit out of share/: `--system-package` is
      # the switch that decides share/ vs lib/, and second-guessing it here
      # produced the unit in BOTH places.
      cp -r zig-out/share/. "$out/share/"

      runHook postInstall
    '';

    outputs = [
      "out"
      "terminfo"
      "shell_integration"
      "vim"
    ];

    # Split the same four outputs the reference does, for the same reason: a
    # user who wants rustty's terminfo on a REMOTE host should be able to
    # install that alone.
    postInstall = ''
      terminfo_src="$out/share/terminfo"

      mkdir -p "$out/nix-support"

      mkdir -p "$terminfo/share"
      mv "$terminfo_src" "$terminfo/share/terminfo"
      ln -sf "$terminfo/share/terminfo" "$terminfo_src"
      echo "$terminfo" >> "$out/nix-support/propagated-user-env-packages"

      mkdir -p "$shell_integration"
      mv "$out/share/rustty/shell-integration" "$shell_integration/shell-integration"
      ln -sf "$shell_integration/shell-integration" "$out/share/rustty/shell-integration"
      echo "$shell_integration" >> "$out/nix-support/propagated-user-env-packages"

      mv "$out/share/vim/vimfiles" "$vim"
      ln -sf "$vim" "$out/share/vim/vimfiles"
      echo "$vim" >> "$out/nix-support/propagated-user-env-packages"

      echo "gst_all_1.gstreamer" >> "$out/nix-support/propagated-user-env-packages"
      echo "gst_all_1.gst-plugins-base" >> "$out/nix-support/propagated-user-env-packages"
      echo "gst_all_1.gst-plugins-good" >> "$out/nix-support/propagated-user-env-packages"
    '';

    # `rustty +version` exercises the binary and its resource lookup without
    # needing a display, which is the most a sandbox can check for a GUI app.
    doInstallCheck = true;
    installCheckPhase = ''
      runHook preInstallCheck
      "$out/bin/rustty" +version | grep -q "Rustty"
      runHook postInstallCheck
    '';

    meta = {
      homepage = "https://rustty.sh";
      license = lib.licenses.mit;
      platforms = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      mainProgram = "rustty";
    };
  })
