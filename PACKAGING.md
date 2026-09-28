# Packaging Rustty for Distribution

Rustty relies on downstream package maintainers to distribute Rustty to
end-users. This document provides guidance to package maintainers on how to
package Rustty for distribution.

> [!IMPORTANT]
>
> This document is only accurate for the Rustty source alongside it.
> **Do not use this document for older or newer versions of Rustty!** If
> you are reading this document in a different version of Rustty, please
> find the `PACKAGING.md` file alongside that version.

## Source Tarballs

Source tarballs with stable checksums are available for tagged releases
at the Rustty release-artifact host in the following URL format where
`VERSION` is the version number with no prefix such as `1.0.0`:

<!--
  NOTE: the release-artifact host is `rustty.sh`. The pipeline that produces
  and signs these files now EXISTS in the repository
  (.github/workflows/release-tag.yml -> `cargo run -p xtask -- distcheck` +
  minisign, promoted by publish-tag.yml), but it is INERT: every job is gated
  on the `RUSTTY_RELEASE_ENABLED` repository variable, and the bucket behind
  rustty.sh plus the signing/notarization secrets are maintainer-provided
  infrastructure. Until a maintainer arms it, these URLs will not resolve.
-->

```
https://rustty.sh/VERSION/rustty-VERSION.tar.gz
https://rustty.sh/VERSION/rustty-VERSION.tar.gz.minisig
```

Signature files are signed with
[minisign](https://jedisct1.github.io/minisign/)
using the following public key:

```
RWRcF0y/6b9ncIoHA72QDYLji3/E3Yp1NiGyJyp2wVC7y0kyjfvpn7mL
```

<!--
  MAINTAINERS: this public key must be the counterpart of the `MINISIGN_KEY`
  secret the release workflows sign with (release-tag.yml "Sign Tarball",
  release-tip.yml "Sign Tarball"). If the release keypair is ever generated or
  rotated, update this block in the same change — packagers verify against it,
  and a stale key here means every published signature fails verification.
-->

**Tip source tarballs** are available on the
[GitHub releases page](https://github.com/AirelyDev/rustty/releases/tag/tip).
Use the `rustty-source.tar.gz` asset and _not the GitHub auto-generated
source tarball_. These tarballs are generated for every commit to
the `main` branch and are not associated with a specific version.

<!--
  The tip pipeline exists (.github/workflows/release-tip.yml: it runs after a
  green `ci` on `main`, builds the tarball with `xtask distcheck`, signs it,
  and force-moves the `tip` tag), but like the tagged pipeline it is gated on
  the `RUSTTY_RELEASE_ENABLED` repository variable — until a maintainer arms
  it, this asset is not published.
-->

> [!WARNING]
>
> Source tarballs are _not the same_ as a Git checkout. Source tarballs
> contain some preprocessed files (a vendored dependency tree and generated
> resources) that allow building Rustty with less dependencies. If you are
> building Rustty from a Git checkout, the steps below are the same but they
> may require additional dependencies not listed here. See the `README.md`
> for more information on building from a Git checkout.
>
> For everyone except Rustty developers, please use the source tarballs.
> We generate tip source tarballs for users following the development
> branch.

## Rust Toolchain

[Rust](https://www.rust-lang.org) is required to build Rustty. Rustty pins
the exact toolchain it builds with so that package maintainers get a
reproducible build without having to track the toolchain manually.

To find the toolchain required to build Rustty, check the `channel` field
in the `rust-toolchain.toml` file at the repository root. You don't need to
know Rust to extract this information. This will always be a released,
stable Rust version (for example, at the time of writing this document,
Rustty requires Rust `1.93.1`).

If you have [`rustup`](https://rustup.rs) installed, it reads
`rust-toolchain.toml` automatically and provisions the pinned toolchain on
the first `cargo` invocation. Distributions that provide their own Rust
compiler should ensure the packaged `rustc`/`cargo` is at least the pinned
version.

## Building Rustty

The following is a standard example of how to build Rustty _for system
packages_. This is not the recommended way to build Rustty for your
own system. For that, see the primary README.

1. First, we make our dependencies available offline. This is the only
   step that requires internet access.

   Release **source tarballs** already ship with a `vendor/` directory and
   a `.cargo/config.toml` that redirects crates.io to it, so this step is a
   no-op — the tarball is already self-contained and the build below runs
   fully offline.

   When building from a **Git checkout**, populate the vendor directory
   yourself:

```sh
cargo vendor --locked vendor > .cargo/config.toml
```

2. Next, we build Rustty. With the vendor directory in place this step
   requires no internet access:

```sh
CARGO_PROFILE_RELEASE_DEBUG=0 \
RUSTFLAGS="-C target-cpu=x86-64" \
cargo build \
  --offline \
  --locked \
  --release \
  -p rustty
```

This produces the release binary at `target/release/rustty`.

3. Finally, install the binary and the runtime resource tree into your
   packaging root. Use the `DESTDIR` and `PREFIX` conventions common to
   system packages so the install step is a plain copy into a staging
   directory:

```sh
DESTDIR=/tmp/rustty
PREFIX=/usr

# The binary.
install -Dm755 target/release/rustty "$DESTDIR$PREFIX/bin/rustty"

# The runtime resources (terminfo, completions, man pages, i18n, themes,
# shell integration). This regenerates the full share/ tree.
#
# `--target-os` selects the target's resource layout (default: the build
# host). On Linux it ALSO stages the desktop-integration assets — the
# .desktop launcher, D-Bus/systemd activation services, AppStream metainfo,
# Nautilus/Dolphin right-click actions, and the hicolor icon set. `--prefix`
# must match PREFIX so the templated Exec= lines point at the installed
# binary. (On FreeBSD, `--target-os freebsd` puts the terminfo tree under
# share/site-terminfo, where the runtime looks it up.) `--optimize` is the
# resource half of `-Doptimize`: a release build installs only the compiled
# terminfo database, and a Debug/ReleaseSafe one also installs the terminfo
# source and termcap and names the desktop files with the `-debug` app id.
cargo run -p xtask -- resources --target-os linux --prefix "$PREFIX" \
  --optimize ReleaseFast
cp -R zig-out/share "$DESTDIR$PREFIX/share"
```

Flags for special Linux packaging arms: `--flatpak` (flatpak D-Bus
activation, icons capped at 512x512, no systemd unit), `--debug` (forces
the `-debug` app id whatever `--optimize` says), and `--system-package` (the systemd user unit is staged
under `zig-out/lib/systemd/user` — copy `zig-out/lib` into
`$DESTDIR$PREFIX/lib` as well in that case, since system packages install
systemd user units under `lib`, not `share`).

This installs Rustty into `/tmp/rustty` with the prefix `/usr` (i.e. the
binary ends up at `/tmp/rustty/usr/bin/rustty`). This style is common for
system packages which separate a build and install step, since the install
step can then be done with a `mv` or `cp` command (from `/tmp/rustty` to
wherever the package manager expects it).

> [!NOTE]
>
> A fully **reproducible** Nix/crane build (a `flake.nix` with a pinned
> toolchain and a hash-locked vendor set) is the recommended path for
> distributions that want bit-for-bit reproducibility. That packaging path
> lands with the release infrastructure (Batch 4) and will be documented
> here once available.

### Build Options

Rustty uses the Cargo build system. You can see all available build options by
running `cargo build --help`. The following are options that are particularly
relevant to package maintainers, with their Zig-build equivalents noted for
maintainers migrating from other terminal emulators:

- **Install prefix / staging** (was `--prefix` + `DESTDIR`): Cargo does not
  install to a prefix itself. Build with `--release`, then copy
  `target/release/rustty` and the `xtask`-generated `share/` tree into
  `$DESTDIR$PREFIX` as shown above. The `DESTDIR` / `PREFIX` shell variables
  are yours to define — they are ordinary paths, not Cargo flags.

- **Release optimization** (was `-Doptimize=ReleaseFast`): Use `--release`,
  which selects the `[profile.release]` profile. This builds with
  optimizations enabled and is the recommended mode for distribution. If you
  want to strip debug info and trade a little safety for speed, set
  `CARGO_PROFILE_RELEASE_DEBUG=0` (and, if desired, `panic = "abort"` /
  `lto = true` via a profile override). Other profiles are available:
  `dev` (unoptimized + debug, the `Debug` analog) and any custom
  `[profile.*]` you define in `Cargo.toml`.

- **Strip** (was `-Dstrip=<bool>`): like upstream's default for a
  `ReleaseFast` build, `--release` strips the `rustty` executable and the
  embedding library; `dev` and `--profile release-safe` (the `ReleaseSafe`
  analog) do not. To keep the symbols of a release build (for a separate
  debug-symbols package, as the Flatpak manifest does), set
  `CARGO_PROFILE_RELEASE_STRIP=false`.

- **SIMD** (was `-Dsimd=<bool>`): on by default. `-Dsimd=false` is
  `--no-default-features` plus the other features you want, e.g.
  `--no-default-features --features gtk,wayland,x11`: no SIMD code path is
  compiled and neither highway nor simdutf is linked.

- **Internationalization** (was `-Di18n=<bool>`): on by default for macOS
  and glibc Linux, off elsewhere. Set `RUSTTY_I18N=false` (or `true`) in the
  environment of both the `cargo build` and the `xtask resources` steps; the
  latter then omits (or installs) the `share/locale` catalogs. `xtask
  resources` also takes `--i18n <bool>` directly.

- **Documentation** (was `-Demit-docs=<bool>`): `xtask resources` renders
  the man pages and the HTML/markdown docs in-process (no pandoc needed), so
  they are installed by default; `--emit-docs false` leaves them out.

- **PIE** (was `-Dpie=<bool>`): Rust links Linux and FreeBSD executables as
  position-independent by default, which is upstream's `-Dpie=true` (its
  default for system packages). There is no switch for upstream's non-PIE
  default.

- **Baseline CPU** (was `-Dcpu=baseline`): Build for the "baseline" CPU of the
  target architecture. This avoids building for newer CPU features that may
  not be available on all target machines. rustc already does this by
  default: without `-C target-cpu` it targets the triple's generic baseline
  (`x86-64` for 64-bit x86, `generic` for aarch64 Linux, `apple-m1` for
  aarch64 macOS). Do not add `RUSTFLAGS="-C target-cpu=native"` to a
  distributed package; it tunes for the build host.

- **Windowing protocols** (was `-Dgtk-wayland=<bool>` / `-Dgtk-x11=<bool>`):
  The default feature set builds the Linux GTK app with **both** native
  Wayland and X11 integration (the `wayland` and `x11` Cargo features are
  default-on, matching the upstream defaults on a distro GTK built with both
  backends). If your GTK is built for a single backend, opt out of the other
  arm with `--no-default-features --features gtk,wayland` (Wayland-only) or
  `--no-default-features --features gtk,x11` (X11-only). Both features are
  no-ops off Linux, so the default build stays correct on every other target.

- **Target triple** (was `-Dtarget=$arch-$os-$abi`): Build for a specific
  target with `--target=<triple>` (for example
  `--target=x86_64-unknown-linux-gnu`). This is often necessary for system
  packages to specify a specific architecture, libc, etc. Run
  `rustc --print target-list` to get a full list of available targets, and
  ensure the corresponding `rust-std` component is installed (`rustup target
add <triple>`).

## WebAssembly (librustty-vt)

librustty-vt can be built for WebAssembly for use in browsers and other
wasm runtimes:

```sh
cargo rustc -p rustty-vt-c --target wasm32-unknown-unknown --release -- \
  -C link-arg=--export-table -C link-arg=-zstack-size=131072
cargo run -p xtask -- wasm-patch-growable-table \
  target/wasm32-unknown-unknown/release/rustty_vt_c.wasm zig-out/bin/rustty-vt.wasm
```

This produces `zig-out/bin/rustty-vt.wasm`: the module exports its indirect
function table, patched growable, so JavaScript can install effect callbacks,
and it has a 128 KiB stack. (In a git checkout, `.port/tools/build-vt-wasm.sh`
runs both steps.) The Rust std for the target must be installed (`rustup
target add wasm32-unknown-unknown`).

Some notes for packaging the wasm module:

- The `simd128` feature is enabled by default, as upstream Ghostty enables
  it (the checkout's `.cargo/config.toml` adds `-C target-feature=+simd128`
  for every wasm32 target). Every browser engine has supported it for years
  (Chrome 91, Firefox 89, Safari 16.4) and it is a large performance win for
  VT parsing. If you target an unusual runtime without SIMD support, opt out
  the way `-Dcpu=generic` does upstream: set `RUSTFLAGS` (even to an empty
  string, or to `-C target-cpu=generic`), which replaces the configured
  flags.

- Optional feature areas can be compiled out to significantly reduce binary
  size. Each `-Dvt-features` feature is a Cargo feature of `rustty-vt-c`, all
  of them on by default; build with `--no-default-features` and list the
  ones you want. For example, a read-only terminal viewer only needs the
  render state API:

  ```sh
  cargo rustc -p rustty-vt-c --target wasm32-unknown-unknown --release \
    --no-default-features --features render-state -- \
    -C link-arg=--export-table -C link-arg=-zstack-size=131072
  ```

  An interactive terminal typically wants
  `--features render-state,input-encode,selection,color`. Disabled features
  drop both their C API exports and any escape sequence handling (the
  sequences are still consumed and safely ignored). See the `vt-features`
  block of `crates/rustty-vt-c/Cargo.toml` and the per-feature consts in
  `crates/rustty-vt/src/options.rs` for the full list of features and what
  each one covers.

- For the smallest module, build with `CARGO_PROFILE_RELEASE_OPT_LEVEL=s` (the
  analog of `ReleaseSmall`) and run the result through
  [Binaryen's](https://github.com/WebAssembly/binaryen) `wasm-opt -O3`. When
  invoking `wasm-opt`, pass the feature flags for what the module uses, e.g.
  `--enable-simd --enable-bulk-memory --enable-sign-ext
--enable-nontrapping-float-to-int --enable-multivalue
--enable-reference-types`, and `--strip-dwarf` to drop any debug info.
