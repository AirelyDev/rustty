{
  description = "A full-fidelity Rust port of Ghostty";

  inputs = {
    # Tracking nixpkgs-unstable for a recent GTK 4 / libadwaita and a Rust
    # toolchain new enough for rust-toolchain.toml's pinned channel. Same
    # reasoning as the reference: stay current, but be aware that the glibc our
    # dependencies are built against has to stay compatible with the systems we
    # build for.
    nixpkgs.url = "https://channels.nixos.org/nixpkgs-unstable/nixexprs.tar.zst";

    # Used for default.nix / shell.nix.
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };

    systems = {
      url = "github:nix-systems/default";
      flake = false;
    };

    # Cargo.lock-native vendoring: the real `cargo build` runs in the sandbox,
    # so every build.rs and every crates-sys/* C compile works. Replaces the
    # reference's zig + zon2nix inputs entirely.
    crane.url = "github:ipetkov/crane";

    # Honors rust-toolchain.toml via `fromRustupToolchainFile`, so the Nix
    # toolchain and a plain `cargo` outside Nix are the same pin.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    systems,
    ...
  }: let
    inherit (nixpkgs) lib;

    # nix-systems/default is {aarch64,x86_64}-{darwin,linux}. x86_64-darwin is
    # filtered out because the nixpkgs we track (26.11 / unstable) DROPPED it —
    # evaluating any package for that system is a hard error, which would make
    # `nix flake show` and `nix flake check` fail for everyone. Restore it by
    # deleting this filter if the nixpkgs input ever moves back to a branch
    # that supports it.
    supportedSystems = lib.filter (s: s != "x86_64-darwin") (import systems);

    # rustty's headless librustty-vt is plain cargo + cmake, so it is offered on
    # every supported system. The GTK app is not: see `forLinuxSystems` below.
    forAllSystems = f:
      lib.genAttrs supportedSystems (system:
        f (import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        }));

    mkPkgArgs = optimize: {
      inherit optimize;
      revision = self.shortRev or self.dirtyShortRev or "dirty";
    };

    # The toolchain rust-toolchain.toml pins, plus the components a dev shell
    # needs beyond it (rust-src for rust-analyzer and for Miri's std).
    rustToolchainFor = pkgs:
      (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
        extensions = ["rust-src" "rustfmt" "clippy"];
      };

    craneLibFor = pkgs: (crane.mkLib pkgs).overrideToolchain (rustToolchainFor pkgs);

    librusttyVtFor = pkgs: args:
      pkgs.callPackage ./nix/librustty-vt.nix ({
          craneLib = craneLibFor pkgs;
        }
        // args);

    rusttyFor = pkgs: args:
      pkgs.callPackage ./nix/package.nix ({
          craneLib = craneLibFor pkgs;
        }
        // args);

    # The GTK app is Linux-only (nix/package.nix's meta.platforms), so its
    # attributes are only exposed there. The reference filters Darwin out of
    # its package outputs for the same reason — Nix cannot drive the macOS app.
    linuxSystems = lib.filter (s: lib.hasSuffix "-linux" s) supportedSystems;
    forLinuxSystems = f:
      lib.genAttrs linuxSystems (system:
        f (import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        }));
  in {
    devShells = forAllSystems (pkgs: {
      default = pkgs.callPackage ./nix/devShell.nix {
        rustToolchain = rustToolchainFor pkgs;
        wraptest = pkgs.callPackage ./nix/pkgs/wraptest.nix {};

        python3 = pkgs.python3.override {
          self = pkgs.python3;
          packageOverrides = pyfinal: pyprev: {
            blessed = pyfinal.callPackage ./nix/pkgs/blessed.nix {};
            ucs-detect = pyfinal.callPackage ./nix/pkgs/ucs-detect.nix {};
            wcwidth = pyfinal.callPackage ./nix/pkgs/wcwidth.nix {};
          };
        };
      };
    });

    packages =
      lib.recursiveUpdate
      (forAllSystems (pkgs: rec {
        librustty-vt-debug = librusttyVtFor pkgs (mkPkgArgs "debug");
        librustty-vt-releasesafe = librusttyVtFor pkgs (mkPkgArgs "releasesafe");
        librustty-vt-releasefast = librusttyVtFor pkgs (mkPkgArgs "releasefast");
        librustty-vt-debug-no-simd = librusttyVtFor pkgs ((mkPkgArgs "debug") // {simd = false;});
        librustty-vt-releasesafe-no-simd = librusttyVtFor pkgs ((mkPkgArgs "releasesafe") // {simd = false;});
        librustty-vt-releasefast-no-simd = librusttyVtFor pkgs ((mkPkgArgs "releasefast") // {simd = false;});

        librustty-vt = librustty-vt-releasefast;
        default = librustty-vt;
      }))
      (forLinuxSystems (pkgs: rec {
        rustty-debug = rusttyFor pkgs (mkPkgArgs "debug");
        rustty-releasesafe = rusttyFor pkgs (mkPkgArgs "releasesafe");
        rustty-releasefast = rusttyFor pkgs (mkPkgArgs "releasefast");
        rustty = rustty-releasefast;
      }));

    # `nix flake check` runs these. passthru.tests are the substance — they
    # build a C program against the installed library both ways and run it.
    checks = forAllSystems (pkgs: let
      sys = pkgs.stdenv.hostPlatform.system;
      pkgsFor = self.packages.${sys};
    in
      {
        inherit (pkgsFor) librustty-vt;
      }
      // (lib.mapAttrs' (n: v: lib.nameValuePair "librustty-vt-${n}" v)
        pkgsFor.librustty-vt.passthru.tests)
      # The GTK app, where it exists. Its own installCheckPhase runs
      # `rustty +version`, so building it IS the check.
      // (lib.optionalAttrs (pkgsFor ? rustty) {inherit (pkgsFor) rustty;}));

    formatter = forAllSystems (pkgs: pkgs.alejandra);

    overlays = {
      default = self.overlays.releasefast;
      releasefast = final: prev:
        {
          librustty-vt = librusttyVtFor final (mkPkgArgs "releasefast");
        }
        // lib.optionalAttrs final.stdenv.hostPlatform.isLinux {
          rustty = rusttyFor final (mkPkgArgs "releasefast");
        };
      debug = final: prev:
        {
          librustty-vt = librusttyVtFor final (mkPkgArgs "debug");
        }
        // lib.optionalAttrs final.stdenv.hostPlatform.isLinux {
          rustty = rusttyFor final (mkPkgArgs "debug");
        };
    };
  };

  # NOTE: the reference declares its own cachix substituter here. rustty has no
  # binary cache yet, and inventing a substituter URL + trusted public key
  # would be fabricating credentials, so nixConfig is deliberately absent.
  # Add it here when a cache actually exists:
  #
  #   nixConfig = {
  #     extra-substituters = ["https://<rustty-cache>.cachix.org"];
  #     extra-trusted-public-keys = ["<key>"];
  #   };
}
