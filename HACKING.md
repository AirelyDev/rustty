# Developing Rustty

This document describes the technical details behind Rustty's development.
If you'd like to open any pull requests or would like to implement new features
into Rustty, please make sure to read our ["Contributing to Rustty"](CONTRIBUTING.md)
document first.

Rustty is a full-fidelity Rust port of [Ghostty](https://github.com/ghostty-org/ghostty).
To start development on Rustty, you need to build Rustty from a Git checkout.
Clone the Git repository:

```shell
git clone https://github.com/AirelyDev/rustty
cd rustty
```

> [!NOTE]
>
> Rustty may require [extra dependencies](#extra-dependencies)
> when building from a Git checkout compared to a source tarball.
> Tip versions may also require a different version of the Rust toolchain or
> other toolchains (e.g. the Xcode SDK on macOS) compared to stable versions —
> make sure to follow the steps closely!

When you're developing Rustty, it's very likely that you will want to build a
_debug_ build to diagnose issues more easily. This is already the default for
Cargo builds, so simply run `cargo build` **without any `--release` flag**.

There are many more build steps than just `cargo build`, some of which are listed
here:

| Command                                     | Description                                                                                                          |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `cargo run -p rustty`                       | Runs Rustty                                                                                                          |
| `.port/tools/gates/check_leaks.sh`          | Runs the memory-safety gate (Miri/ASan) to [check for memory leaks](#checking-for-memory-leaks)                      |
| `cargo test`                                | Runs unit tests (accepts `-p <crate>` to scope to a crate, and a trailing `<filter>` to only run matching tests)     |
| `cargo run -p xtask -- update-translations` | Updates Rustty's translation strings (see the [Contributor's Guide on Localizing Rustty](po/README_CONTRIBUTORS.md)) |
| `cargo run -p xtask -- dist`                | Builds a source tarball                                                                                              |
| `cargo run -p xtask -- distcheck`           | Builds and validates a source tarball                                                                                |

In addition, all Rust code should be formatted with `cargo fmt` and linted with
`cargo clippy` before you commit (see [Linting](#linting)).

## Build Methods and Artifacts

**`cargo` is the foundation; everything else wraps it.** There is no `zig` in
Rustty — the `xtask` crate (`cargo run -p xtask -- <cmd>`) replaces Ghostty's
`src/build/` and orchestrates the multi-step builds (C headers, the resource
tree, the macOS xcframework, source tarballs). `CMake`, `Doxygen`, the
`Makefile`, `macos/build.nu`, and the Nix flake are consumers or packagers
layered on top of those two.

| Method        | Command                                                   | Produces                                                            | Lands in                                  | Use it for                                                                           |
| ------------- | --------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------ |
| **cargo**     | `cargo build [--release]`                                 | all workspace crates                                                | `target/{debug,release}/`                 | day-to-day development                                                               |
|               | `cargo run -p rustty`                                     | the `rustty` binary                                                 | `target/<profile>/rustty`                 | running Rustty locally                                                               |
|               | `cargo test [-p <crate>]`                                 | —                                                                   | —                                         | tests (memory-safety gate: [Checking for Memory Leaks](#checking-for-memory-leaks))  |
|               | `cargo build -p rustty-vt-c --release [--features simd]`  | the VT C library                                                    | `target/release/librustty_vt_c.{a,dylib}` | the headless `librustty-vt` C library                                                |
| **xtask**     | `cargo run -p xtask -- header-gen`                        | `include/rustty.h` (+ `rustty/vt/*`) via cbindgen                   | `include/`                                | regenerating the VT C headers                                                        |
|               | `… -- header-gen-apprt`                                   | `include/rustty-apprt.h`                                            | `include/`                                | the embedding-API header                                                             |
|               | `… -- xcframework`                                        | `RusttyKit.xcframework` (SIMD-on)                                   | `dist/RusttyKit.xcframework`              | the macOS Swift-app library                                                          |
|               | `… -- resources`                                          | terminfo, shell-integration, themes, man pages, completions, locale | `zig-out/share/`                          | the macOS Copy-Resources tree                                                        |
|               | `… -- dist`                                               | the source tarball                                                  | `dist/rustty-<version>.tar.gz`            | packaging / release source                                                           |
|               | `… -- distcheck`                                          | (extracts + builds the tarball)                                     | —                                         | validating the tarball is self-contained                                             |
| **CMake**     | `cmake -B build && cmake --build build`                   | `librustty_vt_c` for C/CMake consumers                              | `target/<profile>/`                       | C/CMake projects consuming the VT library (FetchContent / `find_package(rustty-vt)`) |
|               | `cmake --install build --prefix P`                        | library + headers + pkg-config config                               | `P/{lib,include,lib/cmake/rustty-vt}`     | installing the VT library to a prefix                                                |
| **Doxygen**   | `doxygen Doxyfile`                                        | the C-API docs (HTML + man pages)                                   | `doc/librustty/`, `doc/man/`              | the `librustty` C-API reference                                                      |
| **Makefile**  | `make clean`                                              | removes `target`, the xcframework, `build-cmake`, `doc`             | —                                         | quick cleanup                                                                        |
| **macOS app** | `cargo run -p xtask -- xcframework` then `macos/build.nu` | `Rustty.app` (two-phase: library → Swift app via xcodebuild)        | `macos/build/<config>/Rustty.app`         | the macOS GUI app                                                                    |
| **Nix**       | `nix build .#librustty-vt` / `.#rustty`; `nix develop`    | C library / GTK app / dev shell / VMs                               | the Nix store                             | reproducible builds, Linux packaging, dev shell                                      |

### Where the artifacts live

- **CLI/app binary:** `target/{debug,release}/rustty`
- **C libraries:** `target/release/librustty_vt_c.{a,dylib}` (VT engine), `librustty_apprt_c.a` (embedding API)
- **C headers (checked in):** `include/rustty/vt.h` + `include/rustty/vt/*` (curated), `include/rustty.h` + `include/rustty-apprt.h` (cbindgen-generated)
- **macOS xcframework:** `dist/RusttyKit.xcframework`
- **macOS app:** `macos/build/<config>/Rustty.app`
- **Runtime resources:** `zig-out/share/` (terminfo, shell-integration, themes, man, completions, locale)
- **Source tarball:** `dist/rustty-<version>.tar.gz`
- **C-API docs:** `doc/librustty/` (HTML), `doc/man/` (man3)
- **Packaging _source_ assets (checked in):** `dist/{cmake,doxygen,linux,windows,macos}`

`target/`, `zig-out/`, `dist/RusttyKit.xcframework`, `dist/*.tar.gz`, `doc/`, and
`build-cmake/` are all git-ignored (regenerable). The
`dist/{cmake,doxygen,linux,windows,macos}` **source** assets are tracked.

### Releasing

- **Source release:** `cargo run -p xtask -- dist` produces the tarball; sign it
  with the project's minisign key and host it at `rustty.sh` (see
  [PACKAGING.md](PACKAGING.md)).
- **macOS app + auto-update:** `xtask xcframework` → `macos/build.nu` →
  `Rustty.app`; sign it, then `dist/macos/update_appcast_{tag,tip}.py` generates
  the Sparkle `appcast.xml` (release host `rustty.sh`, tip host `tip.rustty.sh`).
- **Linux distro packages:** `dist/linux/` (desktop / metainfo / dbus / systemd)
  feed flatpak and snap builds.

> [!NOTE]
>
> Some methods above are **forward references** while the port is brought up:
> the **Nix** flake and VMs land in Batch 4 (see
> `docs/superpowers/specs/2026-07-15-batch4-nix-flake-design.md`); the automated
> **release / flatpak / snap** GitHub workflows land with it (Phase C); the
> translation `.mo` compilation and runtime loading land with the i18n batch;
> and the **GTK Linux app** (`nix build .#rustty`, flatpak/snap) is gated on the
> GTK apprt going live. Everything under **cargo**, the wired **xtask**
> subcommands, **CMake**, **Doxygen**, the **Makefile**, and the **macOS app**
> works today.

## Extra Dependencies

Building the GTK (Linux) application from a Git checkout requires some
additional dependencies:

- `blueprint-compiler` (version 0.16.0 or newer)

> [!NOTE]
>
> The GTK application is not yet live in the Rust port; the GTK apprt is being
> brought up in a later milestone. Until then these dependencies are only
> required for the components that consume the Blueprint UI definitions.

macOS users don't require any additional dependencies.

## Xcode Version and SDKs

Building the Rustty macOS app requires that Xcode, the macOS SDK,
the iOS SDK, and Metal Toolchain are all installed.

A common issue is that the incorrect version of Xcode is either
installed or selected. Use the `xcode-select` command to
ensure that the correct version of Xcode is selected:

```shell-session
sudo xcode-select --switch /Applications/Xcode.app
```

> [!IMPORTANT]
>
> Main branch development of Rustty requires **Xcode 26 and the macOS 26 SDK**.
>
> You do not need to be running on macOS 26 to build Rustty, you can
> still use Xcode 26 on macOS 15 stable.

## AI and Agents

If you're using AI assistance with Rustty, Rustty provides an
[AGENTS.md file](AGENTS.md) read by most of the popular AI agents to help
produce higher quality results.

We also provide commands in `.agents/commands` that have some vetted
prompts for common tasks that have been shown to produce good results.
We provide these to help reduce the amount of time a contributor has to
spend prompting the AI to get good results, and hopefully to lower the slop
produced.

- `/gh-issue <number/url>` - Produces a prompt for diagnosing a GitHub
  issue, explaining the problem, and suggesting a plan for resolving it.
  Requires `gh` to be installed with read-only access to Rustty.

> [!NOTE]
>
> `AGENTS.md` and the `.agents/commands` prompts land in a later documentation
> batch. Until then, treat this section as a forward reference.

> [!WARNING]
>
> All AI assistance usage must be disclosed and we expect contributors to
> understand the code that is produced and be able to answer questions about
> it. If you don't understand the code produced, feel free to disclose that,
> but if it has problems, we may ask you to fix it and close the issue. It
> isn't a maintainers job to review a PR so broken that it requires significant
> rework to be acceptable.

## Logging

Rustty can write logs to a number of destinations. On all platforms, logging to
`stderr` is available. Depending on the platform and how Rustty was launched,
logs sent to `stderr` may be stored by the system and made available for later
retrieval.

On Linux if Rustty is launched by the default `systemd` user service, you can use
`journald` to see Rustty's logs: `journalctl --user --unit app-io.airely.rustty.service`.

On macOS logging to the macOS unified log is available and enabled by default.
Use the system `log` CLI to view Rustty's logs: `sudo log stream --level debug --predicate 'subsystem=="io.airely.rustty"'`.

Rustty's logging can be configured in two ways. The first is by what
optimization level Rustty is compiled with. If Rustty is compiled as a debug
build (`cargo build` without `--release`) debug logs will be output to `stderr`.
If Rustty is compiled with any other optimization the debug logs will not be
output to `stderr`.

Rustty also checks the `RUSTTY_LOG` environment variable. It can be used
to control which destinations receive logs. Rustty currently defines two
destinations:

- `stderr` - logging to `stderr`.
- `macos` - logging to macOS's unified log (has no effect on non-macOS platforms).

Combine values with a comma to enable multiple destinations. Prefix a
destination with `no-` to disable it. Enabling and disabling destinations
can be done at the same time. Setting `RUSTTY_LOG` to `true` will enable all
destinations. Setting `RUSTTY_LOG` to `false` will disable all destinations.

## Linting

The linters below cover our docs, resources, shell scripts, Nix modules, and
Swift code. Rust code is formatted and linted separately with `cargo fmt` and
`cargo clippy` — run both from the repo root before you commit:

```
cargo fmt
cargo clippy --workspace --all-targets
```

> [!NOTE]
>
> The commands in this section that invoke `nix develop -c ...` are a **forward
> reference**: they become available once the Nix flake lands (Batch 4). Until
> then, use the non-Nix install instructions below. Pin your tool versions to
> whatever the flake's `nix/devShell.nix` declares once it exists.

### Prettier

Rustty's docs and resources (not including Rust code) are linted using
[Prettier](https://prettier.io) with out-of-the-box settings. A Prettier CI
check will fail builds with improper formatting. Therefore, if you are
modifying anything Prettier will lint, you may want to install it locally and
run this from the repo root before you commit:

```
prettier --write .
```

Make sure your Prettier version matches the version of Prettier in
`nix/devShell.nix` (once the Nix flake lands, Batch 4).

Once the Nix flake lands, Nix users can use the following command to format
with Prettier:

```
nix develop -c prettier --write .
```

### Alejandra

Nix modules are formatted with [Alejandra](https://github.com/kamadorueda/alejandra/). An Alejandra CI check will fail builds with improper formatting.

Once the Nix flake lands (Batch 4), Nix users can use the following command to
format with Alejandra:

```
nix develop -c alejandra .
```

Non-Nix users should install Alejandra and use the following command to format with Alejandra:

```
alejandra .
```

Make sure your Alejandra version matches the version of Alejandra in
`nix/devShell.nix` (once the Nix flake lands, Batch 4).

### ShellCheck

Bash scripts are checked with [ShellCheck](https://www.shellcheck.net/) in CI.

Once the Nix flake lands (Batch 4), Nix users can use the following command to
run ShellCheck over all of our scripts:

```
nix develop -c shellcheck \
    --check-sourced \
    --severity=warning \
    $(find . \( -name "*.sh" -o -name "*.bash" \) -type f ! -path "./target/*" ! -path "./macos/build/*" ! -path "./.git/*" | sort)
```

Non-Nix users can [install ShellCheck](https://github.com/koalaman/shellcheck#user-content-installing) and then run:

```
shellcheck \
    --check-sourced \
    --severity=warning \
    $(find . \( -name "*.sh" -o -name "*.bash" \) -type f ! -path "./target/*" ! -path "./macos/build/*" ! -path "./.git/*" | sort)
```

### SwiftLint

Swift code is linted using [SwiftLint](https://github.com/realm/SwiftLint). A
SwiftLint CI check will fail builds with improper formatting. Therefore, if you
are modifying Swift code, you may want to install it locally and run this from
the repo root before you commit:

```
swiftlint lint --fix
```

Make sure your SwiftLint version matches the version in `nix/devShell.nix`
(once the Nix flake lands, Batch 4).

Once the Nix flake lands, Nix users can use the following command to format
with SwiftLint:

```
nix develop -c swiftlint lint --fix
```

To check for violations without auto-fixing:

```
nix develop -c swiftlint lint --strict
```

## Including and Updating Translations

See the [Contributor's Guide](po/README_CONTRIBUTORS.md) for more details.

> [!NOTE]
>
> The translation contributor's guide lands in a later documentation batch;
> treat this as a forward reference until then.

## Checking for Memory Leaks

While Rust does an amazing job of finding and preventing memory leaks in safe
code, Rustty uses many third-party libraries that are written in C, plus its own
`-sys` FFI crates. Improper usage of those libraries or bugs in those libraries
can cause memory leaks and undefined behavior that the Rust compiler cannot
detect by itself.

Rustty's primary memory-safety gate is `.port/tools/gates/check_leaks.sh`, which
routes each crate to the right tool:

```sh
.port/tools/gates/check_leaks.sh
```

- **Pure-Rust crates → Miri** (`cargo +nightly miri test -p <crate>`): catches
  undefined behavior (use-after-free, out-of-bounds, uninitialized reads,
  invalid aliasing) _and_ leaks. This is the primary gate.
- **Rust↔C FFI crates → AddressSanitizer** (`-Zsanitizer=address`): Miri cannot
  execute foreign C, so FFI crates are checked with ASan for undefined behavior
  across the boundary.

Run the gate for a single crate with `check_leaks.sh --crate <name>`, or list
what it would run with `check_leaks.sh --list`.

### Valgrind (Linux, FFI/C paths)

LeakSanitizer and Valgrind are unavailable on arm64 macOS, so FFI _leak_
detection is deferred to Linux. On Linux, Valgrind remains the recommended tool
for checking the FFI/C code paths for leaks that ASan's leak checking or Miri
cannot reach. Build a debug binary and run it under Valgrind with suppressions
for known false positives:

```sh
valgrind --leak-check=full --error-exitcode=1 \
    ./target/debug/rustty
```

## Input Stack Testing

The input stack is the part of the codebase that starts with a
key event and ends with text encoding being sent to the pty (it
does not include _rendering_ the text, which is part of the
font or rendering stack).

If you modify any part of the input stack, you must manually verify
all the following input cases work properly. We unfortunately do
not automate this in any way, but if we can do that one day that'd
save a LOT of grief and time.

Note: this list may not be exhaustive, I'm still working on it.

### Linux IME

IME (Input Method Editors) are a common source of bugs in the input stack,
especially on Linux since there are multiple different IME systems
interacting with different windowing systems and application frameworks
all written by different organizations.

The following matrix should be tested to ensure that all IME input works
properly:

1. Wayland, X11
2. ibus, fcitx, none
3. Dead key input (e.g. Spanish), CJK (e.g. Japanese), Emoji, Unicode Hex
4. ibus versions: 1.5.29, 1.5.30, 1.5.31 (each exhibit slightly different behaviors)

> [!NOTE]
>
> This is a **work in progress**. I'm still working on this list and it
> is not complete. As I find more test cases, I will add them here.

#### Dead Key Input

Set your keyboard layout to "Spanish" (or another layout that uses dead keys).

1. Launch Rustty
2. Press `'`
3. Press `a`
4. Verify that `á` is displayed

Note that the dead key may or may not show a preedit state visually.
For ibus and fcitx it does but for the "none" case it does not. Importantly,
the text should be correct when it is sent to the pty.

We should also test canceling dead key input:

1. Launch Rustty
2. Press `'`
3. Press escape
4. Press `a`
5. Verify that `a` is displayed (no diacritic)

#### CJK Input

Configure fcitx or ibus with a keyboard layout like Japanese or Mozc. The
exact layout doesn't matter.

1. Launch Rustty
2. Press `Ctrl+Shift` to switch to "Hiragana"
3. On a US physical layout, type: `konn`, you should see `こん` in preedit.
4. Press `Enter`
5. Verify that `こん` is displayed in the terminal.

We should also test switching input methods while preedit is active, which
should commit the text:

1. Launch Rustty
2. Press `Ctrl+Shift` to switch to "Hiragana"
3. On a US physical layout, type: `konn`, you should see `こん` in preedit.
4. Press `Ctrl+Shift` to switch to another layout (any)
5. Verify that `こん` is displayed in the terminal as committed text.

## Nix Virtual Machines

> [!NOTE]
>
> The Nix flake and VM definitions land in Batch 4. The commands below become
> available once the flake lands; treat this section as a forward reference
> until then.

Several Nix virtual machine definitions are provided by the project for testing
and developing Rustty against multiple different Linux desktop environments.

Running these requires a working Nix installation, either Nix on your
favorite Linux distribution, NixOS, or macOS with nix-darwin installed. Further
requirements for macOS are detailed below.

VMs should only be run on your local desktop and then powered off when not in
use, which will discard any changes to the VM.

The VM definitions provide minimal software "out of the box" but additional
software can be installed by using standard Nix mechanisms like `nix run nixpkgs#<package>`.

### Linux

1. Check out the Rustty source and change to the directory.
2. Run `nix run .#<vmtype>`. `<vmtype>` can be any of the VMs defined in the
   `nix/vm` directory (without the `.nix` suffix) excluding any file prefixed
   with `common` or `create`.
3. The VM will build and then launch. Depending on the speed of your system, this
   can take a while, but eventually you should get a new VM window.
4. The Rustty source directory should be mounted to `/tmp/shared` in the VM. Depending
   on what UID and GID of the user that you launched the VM as, `/tmp/shared` _may_ be
   writable by the VM user, so be careful!

### macOS

1. To run the VMs on macOS you will need to enable the Linux builder in your `nix-darwin`
   config. This _should_ be as simple as adding `nix.linux-builder.enable=true` to your
   configuration and then rebuilding. See [this](https://nixcademy.com/posts/macos-linux-builder/)
   blog post for more information about the Linux builder and how to tune the performance.
2. Once the Linux builder has been enabled, you should be able to follow the Linux instructions
   above to launch a VM.

### Custom VMs

To easily create a custom VM without modifying the Rustty source, create a new
directory, then create a file called `flake.nix` with the following text in the
new directory.

```
{
  inputs = {
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    rustty.url = "github:AirelyDev/rustty";
  };
  outputs = {
    nixpkgs,
    rustty,
    ...
  }: {
   nixosConfigurations.custom-vm = rustty.create-gnome-vm {
     nixpkgs = nixpkgs;
     system = "x86_64-linux";
     overlay = rustty.overlays.releasefast;
     # module = ./configuration.nix # also works
     module = {pkgs, ...}: {
       environment.systemPackages = [
         pkgs.btop
       ];
     };
    };
  };
}
```

The custom VM can then be run with a command like this:

```
nix run .#nixosConfigurations.custom-vm.config.system.build.vm
```

A file named `rustty.qcow2` will be created that is used to persist any changes
made in the VM. To "reset" the VM to default delete the file and it will be
recreated the next time you run the VM.

### Contributing new VM definitions

#### VM Acceptance Criteria

We welcome the contribution of new VM definitions, as long as they meet the following criteria:

1. They should be different enough from existing VM definitions that they represent a distinct
   user (and developer) experience.
2. There's a significant Rustty user population that uses a similar environment.
3. The VMs can be built using only packages from the current stable NixOS release.

#### VM Definition Criteria

1. VMs should be as minimal as possible so that they build and launch quickly.
   Additional software can be added at runtime with a command like `nix run nixpkgs#<package name>`.
2. VMs should not expose any services to the network, or run any remote access
   software like SSH daemons, VNC or RDP.
3. VMs should auto-login using the "rustty" user.

## Nix VM Integration Tests

> [!NOTE]
>
> Like the VM definitions above, the Nix VM integration tests land in Batch 4
> alongside the flake. Treat this section as a forward reference until then.

Several Nix VM tests are provided by the project for testing Rustty in a "live"
environment rather than just unit tests.

Running these requires a working Nix installation, either Nix on your
favorite Linux distribution, NixOS, or macOS with nix-darwin installed. Further
requirements for macOS are detailed below.

### Linux

1. Check out the Rustty source and change to the directory.
2. Run `nix run .#checks.<system>.<test-name>.driver`. `<system>` should be
   `x86_64-linux` or `aarch64-linux` (even on macOS, this launches a Linux
   VM, not a macOS one). `<test-name>` should be one of the tests defined in
   `nix/tests.nix`. The test will build and then launch. Depending on the speed
   of your system, this can take a while. Eventually though the test should
   complete. Hopefully successfully, but if not error messages should be printed
   out that can be used to diagnose the issue.
3. To run _all_ of the tests, run `nix flake check`.

### macOS

1. To run the VMs on macOS you will need to enable the Linux builder in your `nix-darwin`
   config. This _should_ be as simple as adding `nix.linux-builder.enable=true` to your
   configuration and then rebuilding. See [this](https://nixcademy.com/posts/macos-linux-builder/)
   blog post for more information about the Linux builder and how to tune the performance.
2. Once the Linux builder has been enabled, you should be able to follow the Linux instructions
   above to launch a test.

### Interactively Running Test VMs

To run a test interactively, run `nix run
.#check.<system>.<test-name>.driverInteractive`. This will load a Python console
that can be used to manage the test VMs. In this console run `start_all()` to
start the VM(s). The VMs should boot up and a window should appear showing the
VM's console.

For more information about the Nix test console, see [the NixOS manual](https://nixos.org/manual/nixos/stable/index.html#sec-call-nixos-test-outside-nixos)

### SSH Access to Test VMs

Some test VMs are configured to allow outside SSH access for debugging. To
access the VM, use a command like the following:

```
ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=/dev/null -p 2222 root@192.168.122.1
ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=/dev/null -p 2222 rustty@192.168.122.1
```

The SSH options are important because the SSH host keys will be regenerated
every time the test is started. Without them, your personal SSH known hosts file
will become difficult to manage. The port that is needed to access the VM may
change depending on the test.

None of the users in the VM have passwords so do not expose these VMs to the Internet.
