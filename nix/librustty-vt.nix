# PORTS: ghostty nix/libghostty-vt.nix — the headless C library derivation and
# its passthru.tests.
#
# NOT a line port. The reference drives `zig build -Demit-lib-vt`; rustty has
# no build.zig. Two structural decisions:
#
# 1. VENDORING VIA CRANE (the Batch-4 spec's locked choice). crane's
#    `vendorCargoDeps` turns Cargo.lock into an offline registry, so the real
#    `cargo build` runs in the sandbox and every build.rs / -sys C compile
#    works. The whole zon2nix / build.zig.zon.nix / zigCacheHash.nix /
#    check-zig-cache.sh complex the reference needs has no analog and is
#    dropped: there is no hash file to keep in sync.
#
# 2. INSTALLING VIA CMAKE, not via crane's buildPackage. rustty's CMakeLists.txt
#    already owns the definition of "what an installed librustty-vt is" — the
#    shared library, the static archive, the public headers, the CMake package
#    config, and (as of this branch) the two pkg-config files. Driving it here
#    means the Nix derivation and the G-CCONSUMER gate exercise the SAME install
#    rules, so they cannot drift apart. crane is used for what it is good at
#    (offline deps) and CMake for what it already does.
{
  lib,
  stdenv,
  cmake,
  pkg-config,
  craneLib,
  rustPlatform,
  runCommand,
  testers,
  gettext,
  libiconv,
  darwin ? null,
  revision ? "dirty",
  # "debug" | "releasesafe" | "releasefast", mapped to a cargo profile below.
  optimize ? "releasefast",
  simd ? true,
}: let
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.intersection (lib.fileset.fromSource (lib.sources.cleanSource ../.)) (
      lib.fileset.unions [
        ../CMakeLists.txt
        ../Cargo.toml
        ../Cargo.lock
        ../rust-toolchain.toml
        ../crates
        ../crates-sys
        ../dist/cmake
        ../include
        ../xtask
      ]
    );
  };

  cargoVendorDir = craneLib.vendorCargoDeps {inherit src;};

  # PORTS: the reference's `-Doptimize=` values. Cargo has two built-in
  # profiles, so `releasesafe` maps to `release` as well: the difference the
  # reference draws (safety checks on) is expressed in rustty by
  # `debug-assertions`, and adding a third [profile.*] to the workspace
  # Cargo.toml for one Nix attribute is not worth the churn. Named separately
  # so the attribute set matches the reference's and can diverge later.
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
  cmakeBuildType =
    if cargoProfile == "dev"
    then "Debug"
    else "Release";
in
  stdenv.mkDerivation (finalAttrs: {
    pname = "librustty-vt";
    version = "0.1.0-dev+${revision}-nix";

    inherit src cargoVendorDir;

    nativeBuildInputs =
      [
        cmake
        pkg-config
        craneLib.configureCargoVendoredDepsHook
        craneLib.cargo
        # PORTS: RUSTTY-ONLY (N487). The reference (nix/libghostty-vt.nix) has no
        # bindgen consumer because its C dependencies are built by zig. Here
        # crates-sys/wuffs-sys (reached through rustty-vt-c) runs bindgen in
        # its build script, which dlopens libclang; bindgenHook exports
        # LIBCLANG_PATH and BINDGEN_EXTRA_CLANG_ARGS, as nix/package.nix does.
        rustPlatform.bindgenHook
      ]
      ++ lib.optionals stdenv.hostPlatform.isDarwin [
        darwin.cctools
      ];

    # gettext supplies the libintl that crates/rustty-os links on macOS; on
    # Linux glibc provides the gettext entry points. libiconv likewise.
    buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [
      gettext
      libiconv
    ];

    # configureCargoVendoredDepsHook writes source-replacement config into
    # $CARGO_HOME/config.toml and hard-fails ("CARGO_HOME: not defined") if the
    # variable is unset. crane's own mkCargoDerivation sets it; this derivation
    # is a plain mkDerivation using only crane's vendoring, so it sets it here.
    # It must exist before preConfigure runs, hence postUnpack.
    postUnpack = ''
      export CARGO_HOME="$NIX_BUILD_TOP/cargo-home"
      mkdir -p "$CARGO_HOME"
    '';

    # No network in the sandbox anyway; this makes a would-be fetch a loud
    # error rather than a confusing timeout.
    CARGO_NET_OFFLINE = "true";

    cmakeFlags = [
      (lib.cmakeFeature "CMAKE_BUILD_TYPE" cmakeBuildType)
      (lib.cmakeBool "RUSTTY_VT_SIMD" simd)
      # The reference passes `-Dlib-version-string=${finalAttrs.version}`
      # (nix/libghostty-vt.nix:79); the CMake option is its port (N424).
      (lib.cmakeFeature "RUSTTY_LIB_VERSION_STRING" finalAttrs.version)
    ];

    # CMakeLists.txt shells out to `cargo build` with the workspace root as its
    # working directory, so the source must be writable (cargo writes target/).
    # cmake's out-of-source build dir does not cover that.
    dontUseCmakeBuildDir = false;

    outputs = [
      "out"
      "dev"
    ];

    # nixpkgs' multiple-outputs hook already routes include/ to $dev. Move the
    # rest of the development-time surface there too — the static archive, both
    # .pc files, and the CMake package config — leaving $out holding only the
    # shared library, which is what a runtime closure should carry.
    postInstall = ''
      mkdir -p "$dev/lib" "$dev/share/pkgconfig"
      mv "$out/lib/librustty_vt_c.a" "$dev/lib/"
      mv "$out/share/pkgconfig"/*.pc "$dev/share/pkgconfig/"
      mv "$out/lib/cmake" "$dev/lib/cmake"
      rmdir "$out/share/pkgconfig" "$out/share" 2>/dev/null || true
    '';

    # Re-point the .pc files at absolute store paths.
    #
    # dist/cmake/librustty-vt.pc.in derives its prefix from ''${pcfiledir}/../..
    # so that `cmake --install --prefix P` is relocatable. That is exactly right
    # for a single-prefix install and exactly wrong for a nixpkgs split-output
    # one: the files now sit in $dev while the shared library lives in $out, so
    # no single relative prefix can be correct for both. Substituting real paths
    # here is what the reference does too (its postFixup patches the prefix its
    # build.zig baked in), for the same reason.
    # Rewrite whole LINES with sed rather than matching literal text: nixpkgs'
    # own fixupPhase has already run by this point and rewrites `includedir=`
    # itself ("Patching ... includedir to output ..."), so a
    # `--replace-fail 'libdir=''${prefix}/lib'` matching the template's exact
    # spelling is not guaranteed to still find anything. Line-anchored
    # substitution is insensitive to whatever the earlier hook did.
    postFixup = ''
      for pc in "$dev"/share/pkgconfig/*.pc; do
        sed -i -e "s|^prefix=.*|prefix=$dev|" "$pc"
      done
      # The shared library is the one thing NOT under $dev.
      sed -i -e "s|^libdir=.*|libdir=$out/lib|" "$dev/share/pkgconfig/librustty-vt.pc"
      sed -i -e "s|^libdir=.*|libdir=$dev/lib|" "$dev/share/pkgconfig/librustty-vt-static.pc"
    '';

    doCheck = false;

    passthru.tests = let
      linkCheck = bin: pat:
        if stdenv.hostPlatform.isDarwin
        then ''otool -L "${bin}" | grep -q ${pat}''
        else ''ldd "${bin}" 2>/dev/null | grep -q ${pat}'';
    in {
      # PORTS: the reference's `sanity-check`. Asserts a real exported symbol
      # exists in BOTH artifacts, and — when simd is on — that the vendored
      # SIMD C++ actually landed in the archive. Without the second half a
      # `simd = true` build that silently lost highway/simdutf would pass.
      sanity-check = runCommand "sanity-check" {} (builtins.concatStringsSep "\n" [
        ''
          set +o pipefail
          ${lib.getExe' stdenv.cc "nm"} "${finalAttrs.finalPackage}/lib/librustty_vt_c${stdenv.hostPlatform.extensions.sharedLibrary}" | grep -qE 'T _?rustty_terminal_new'
          ${lib.getExe' stdenv.cc "nm"} "${finalAttrs.finalPackage.dev}/lib/librustty_vt_c.a" | grep -qE 'T _?rustty_terminal_new'
        ''
        (lib.optionalString simd ''
          ${lib.getExe' stdenv.cc "nm"} "${finalAttrs.finalPackage.dev}/lib/librustty_vt_c.a" | grep -q 'T .*simdutf'
          ${lib.getExe' stdenv.cc "nm"} "${finalAttrs.finalPackage.dev}/lib/librustty_vt_c.a" | grep -q 'T .*3hwy'
        '')
        ''
          touch "$out"
        ''
      ]);

      pkg-config = testers.hasPkgConfigModules {
        package = finalAttrs.finalPackage.dev;
      };

      # PORTS: the reference's `pkg-config-libs`. Checks the .pc files advertise
      # what they should — the shared one a -l flag, the static one the archive
      # by path.
      pkg-config-libs =
        runCommand "pkg-config-libs" {
          nativeBuildInputs = [pkg-config];
        } ''
          export PKG_CONFIG_PATH="${finalAttrs.finalPackage}/share/pkgconfig:${finalAttrs.finalPackage.dev}/share/pkgconfig"

          pkg-config --libs --static librustty-vt | grep -q -- '-lrustty_vt_c'
          pkg-config --libs --static librustty-vt-static | grep -q -- 'librustty_vt_c.a'

          touch "$out"
        '';

      # PORTS: the reference's `build-with-shared` / `build-with-static`. The
      # same C program compiled both ways, run, and checked for the right
      # linkage — the Nix-side twin of the G-CCONSUMER gate.
      build-with-shared = stdenv.mkDerivation {
        name = "build-with-shared";
        src = ./test-src;
        doInstallCheck = true;
        nativeBuildInputs = [pkg-config];
        buildInputs = [finalAttrs.finalPackage];
        buildPhase = ''
          runHook preBuildHooks
          cc -o test test_librustty_vt.c \
            ''$(pkg-config --cflags --libs librustty-vt)
          runHook postBuildHooks
        '';
        installPhase = ''
          runHook preInstallHooks
          mkdir -p "$out/bin"
          cp -a test "$out/bin/test"
          runHook postInstallHooks
        '';
        installCheckPhase = ''
          runHook preInstallCheckHooks
          "$out/bin/test" | grep -q "SIMD: ${
            if simd
            then "yes"
            else "no"
          }"
          ${linkCheck "$out/bin/test" "librustty_vt_c"}
          runHook postInstallCheckHooks
        '';
        meta.mainProgram = "test";
      };

      build-with-static = stdenv.mkDerivation {
        name = "build-with-static";
        src = ./test-src;
        doInstallCheck = true;
        nativeBuildInputs = [pkg-config];
        buildInputs = [finalAttrs.finalPackage.dev];
        buildPhase = ''
          runHook preBuildHooks
          cc -o test test_librustty_vt.c \
            ''$(pkg-config --cflags --libs --static librustty-vt-static)
          runHook postBuildHooks
        '';
        installPhase = ''
          runHook preInstallHooks
          mkdir -p "$out/bin"
          cp -a test "$out/bin/test"
          runHook postInstallHooks
        '';
        installCheckPhase = ''
          runHook preInstallCheckHooks
          "$out/bin/test" | grep -q "SIMD: ${
            if simd
            then "yes"
            else "no"
          }"
          ! ${linkCheck "$out/bin/test" "librustty_vt_c"}
          runHook postInstallCheckHooks
        '';
        meta.mainProgram = "test";
      };
    };

    meta = {
      homepage = "https://rustty.sh";
      license = lib.licenses.mit;
      platforms = lib.platforms.unix;
      pkgConfigModules = [
        "librustty-vt"
        "librustty-vt-static"
      ];
    };
  })
