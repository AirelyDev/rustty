# PORTS: ghostty `src/build/docker/debian/Dockerfile`, the image behind its
# `test-debian-13` CI job.
#
# WHY IT LIVES HERE. `xtask` IS rustty's `build.zig` (R4: it replaces
# `src/build/`), so `src/build/docker/` maps to `xtask/docker/`.
#
# WHAT IT IS FOR. Two things, and the second is why it is worth carrying even
# though `G-DIST`'s debian arm can pull a prebuilt `rust:trixie`:
#
#   1. It builds the FULL APP on a clean Debian 13, catching anything that only
#      links because a dev box happens to have it. (The prebuilt-image arm in
#      `check_dist.sh` builds the lib-vt slice only.)
#   2. It is the canonical, executable answer to "what does a Debian build of
#      rustty need". A packager can read exactly one file. That answer had no
#      home in the tree before this.
#
# Usage:
#   podman build -f xtask/docker/debian/Dockerfile -t rustty-debian .
#   podman build --build-arg DISTRO_VERSION=13 -f xtask/docker/debian/Dockerfile .
ARG DISTRO_VERSION="13"
FROM docker.io/library/debian:${DISTRO_VERSION}

# Install Dependencies
RUN DEBIAN_FRONTEND="noninteractive" apt-get -qq update && \
    apt-get -qq -y --no-install-recommends install \
    # Build Tools
    blueprint-compiler \
    build-essential \
    ca-certificates \
    curl \
    libbz2-dev \
    libonig-dev \
    libxml2-utils \
    lintian \
    lsb-release \
    pandoc \
    # bindgen needs libclang to generate the -sys bindings; the reference has no
    # analogue because Zig's @cImport is part of the compiler.
    libclang-dev \
    pkg-config \
    # Rustty Dependencies
    libadwaita-1-dev \
    libgtk-4-dev \
    libgtk4-layer-shell-dev && \
    # Clean up for better caching
    rm -rf /var/lib/apt/lists/*

WORKDIR /src

# The toolchain pin, copied first so the rustup layer caches independently of
# the sources. The reference reads `minimum_zig_version` out of build.zig.zon
# for the same reason; rustty's pin lives in rust-toolchain.toml, which rustup
# honours automatically.
COPY ./rust-toolchain.toml /src/

# Install Rust
# https://rustup.rs
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --no-modify-path && \
    /root/.cargo/bin/rustup show
ENV PATH="/root/.cargo/bin:${PATH}"

COPY . /src

# `--offline` is deliberately NOT passed: this image builds from a git checkout,
# not from the vendored `xtask dist` tarball. The offline path is what
# `check_dist.sh`'s debian arm exercises.
RUN cargo build --locked -p rustty

RUN ./target/debug/rustty +version
