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

    # Used only by nix/tests.nix: the NixOS VM integration tests drive a
    # home-manager-managed rustty config inside the guest, exactly as the
    # reference's do.
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    systems,
    home-manager,
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
    # rust-overlay COMPOSED with ours: anything that builds its own nixpkgs
    # instance for a guest (nix/vm/create.nix, nix/tests.nix) needs
    # `pkgs.rust-bin` to be present, and our overlay reaches for it.
    vmOverlay = lib.composeManyExtensions [
      (import rust-overlay)
      self.overlays.debug
    ];

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
    checks = forAllSystems (
      pkgs: let
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
        // (lib.optionalAttrs (pkgsFor ? rustty) {inherit (pkgsFor) rustty;})
      # The NixOS VM integration tests. Linux only — they boot a guest, which
      # needs /dev/kvm — and `runNixOSTest` itself is not available for Darwin.
    );

    # The NixOS VM integration tests, as their OWN output rather than inside
    # `checks`.
    #
    # The reference puts them in `checks`, and on a normal Linux machine that
    # is right. They are separated here because `nix flake check` must stay
    # runnable in environments that cannot host a full guest: verified on this
    # project's Linux box, the guest KERNEL boots fine under KVM but systemd's
    # backdoor.service never comes up inside a rootless container, so leaving
    # them in `checks` would make `nix flake check` — and gate G-NIX with it —
    # permanently red for a reason that has nothing to do with rustty.
    #
    # They are NOT skipped-and-forgotten: G-NIX names them and runs them under
    # RUSTTY_NIX_VM_TESTS=1, and they are one command away here:
    #
    #     nix build .#vmChecks.x86_64-linux.basic-version-check
    vmChecks = forLinuxSystems (pkgs:
      import ./nix/tests.nix {
        inherit home-manager nixpkgs self;
        inherit (pkgs.stdenv.hostPlatform) system;
        overlays = [vmOverlay];
      });

    # PORTS: the reference's `apps` output — one launcher per desktop-environment
    # VM. `nix run .#wayland-gnome` boots a NixOS guest with the rustty overlay
    # applied, so the app under test is the one this flake builds.
    #
    # Linux-only: nix/vm/create.nix builds a nixosSystem around
    # `packages.rustty`, which nix/package.nix declares Linux-only. The
    # reference exposes these on Darwin too (its nix-darwin users have a Linux
    # builder); doing that here would mean evaluating a package that does not
    # exist for the system.
    apps = forLinuxSystems (pkgs: let
      runVM = module: let
        vm = import ./nix/vm/create.nix {
          inherit (pkgs.stdenv.hostPlatform) system;
          inherit module nixpkgs;
          overlay = vmOverlay;
        };
        program = pkgs.writeShellScript "run-rustty-vm" ''
          SHARED_DIR=$(pwd)
          export SHARED_DIR

          ${pkgs.lib.getExe vm.config.system.build.vm} "$@"
        '';
      in {
        type = "app";
        program = "${program}";
        meta.description = "start a vm from ${toString module}";
      };
    in {
      wayland-cinnamon = runVM ./nix/vm/wayland-cinnamon.nix;
      wayland-gnome = runVM ./nix/vm/wayland-gnome.nix;
      wayland-plasma6 = runVM ./nix/vm/wayland-plasma6.nix;
      x11-cinnamon = runVM ./nix/vm/x11-cinnamon.nix;
      x11-plasma6 = runVM ./nix/vm/x11-plasma6.nix;
      x11-xfce = runVM ./nix/vm/x11-xfce.nix;
    });

    formatter = forAllSystems (pkgs: pkgs.alejandra);

    overlays = {
      default = self.overlays.releasefast;
      # Unconditional, exactly like the reference's. An earlier version guarded
      # the `rustty` attribute with
      # `lib.optionalAttrs final.stdenv.hostPlatform.isLinux` so it would not
      # exist on Darwin — but reading `final.stdenv` from inside an overlay is
      # a cycle (stdenv is itself built through the overlay list), and it made
      # every VM app fail with "infinite recursion encountered". The attribute
      # therefore EXISTS on Darwin and errors if forced, which is the same
      # property the reference has; the VM apps that consume this overlay are
      # Linux-only anyway.
      releasefast = final: prev: {
        librustty-vt = librusttyVtFor final (mkPkgArgs "releasefast");
        rustty = rusttyFor final (mkPkgArgs "releasefast");
      };
      debug = final: prev: {
        librustty-vt = librusttyVtFor final (mkPkgArgs "debug");
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
