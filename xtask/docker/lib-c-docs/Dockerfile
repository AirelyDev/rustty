# PORTS: ghostty `src/build/docker/lib-c-docs/Dockerfile`.
#
# Builds the librustty C-API reference with Doxygen and serves it as static
# HTML. rustty carried the `Doxyfile`, `DoxygenLayout.xml` and the whole of
# `dist/doxygen/` from the top-level-files port, but NOTHING in the tree ever
# ran `doxygen` — this is the missing consumer.
#
# The paths differ from the reference only where rustty's layout does: the
# JSON schema is `crates/rustty-vt-c/types.schema.json` (the reference keeps it
# at `src/terminal/c/types.schema.json`) and the output goes under
# `doc/librustty`, matching the `Doxyfile`'s `OUTPUT_DIRECTORY`/`HTML_OUTPUT`
# pair (the reference writes `zig-out/share/ghostty/doc/libghostty`; the
# Doxyfile records why rustty's is `doc/`).
#
# Usage:
#   podman build -f xtask/docker/lib-c-docs/Dockerfile -t rustty-c-docs .
#   podman run --rm -p 8080:80 rustty-c-docs
#   # noindex variant, for a staging deploy:
#   podman build --build-arg ADD_NOINDEX_HEADER=true \
#     -f xtask/docker/lib-c-docs/Dockerfile -t rustty-c-docs .
#
# `G-DOXYGEN` (.port/tools/gates/check_doxygen.sh) runs the same doxygen
# invocation directly, without the container, so a warning-clean build is
# gated on every CI run rather than only when someone publishes docs.

#--------------------------------------------------------------------
# Generate documentation with Doxygen
#--------------------------------------------------------------------
FROM --platform=linux/amd64 archlinux:latest AS builder

# Build argument for noindex header
ARG ADD_NOINDEX_HEADER=false
RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm \
    doxygen \
    graphviz && \
    pacman -Scc --noconfirm
WORKDIR /rustty
COPY include/ ./include/
COPY images/ ./images/
COPY dist/doxygen/ ./dist/doxygen/
COPY crates/rustty-vt-c/types.schema.json ./crates/rustty-vt-c/types.schema.json
COPY example/ ./example/
COPY Doxyfile ./
COPY DoxygenLayout.xml ./
RUN mkdir -p doc/librustty
RUN doxygen

#--------------------------------------------------------------------
# Host the static HTML
#--------------------------------------------------------------------
FROM nginx:alpine AS runtime

# Pass build arg to runtime stage
ARG ADD_NOINDEX_HEADER=false
ENV ADD_NOINDEX_HEADER=$ADD_NOINDEX_HEADER

# Copy documentation and entrypoint script
COPY --from=builder /rustty/doc/librustty /usr/share/nginx/html
COPY xtask/docker/lib-c-docs/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 80
CMD ["/entrypoint.sh"]
