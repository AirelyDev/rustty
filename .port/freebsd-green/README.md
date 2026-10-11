# FreeBSD native run (V23), 2026-10-11

The first time Rustty was built, tested and run on FreeBSD. Findings: N553-N564 and UD381
(`.port/parity-closure/NEW-FINDINGS.md`, `UPSTREAM-DEFECTS.md`); the row is V23 in
`.port/phase-2/VERIFICATION_DEBT.md`. Logs: `evidence/freebsd-15.1-arm64-2026-10-11.tgz`.

## The host

FreeBSD 15.1-RELEASE-p4, arm64, a UTM (QEMU backend) guest with 8 CPUs and 10 GB, reached as
`ssh rbsd` (user `gael`, passwordless sudo through `/usr/local/etc/sudoers.d/claude`).

Packages (`pkg install`): `rust` (1.98.1; rustup has no `aarch64-unknown-freebsd` host, so the
pinned 1.93.1 is not available), `git cmake ninja pkgconf gmake bash python311 node gettext-tools
blueprint-compiler gtk4 libadwaita gtk4-layer-shell llvm19 fontconfig freetype2 harfbuzz oniguruma
libxml2`, `zig` (0.16.0, for the oracle), and for the desktops `xorg-minimal xfce xfce4-terminal
tigervnc-server dbus dejavu xauth xrandr mesa-dri xf86-video-scfb xf86-input-libinput lightdm
lightdm-gtk-greeter virtio-gpu-qemu-kmod sway wayvnc foot swaybg grim wtype wmenu xdpyinfo scrot
xdotool wqy-fonts noto-emoji`.

Environment the build needs:

- `~/.cargo/config.toml` sets `CLANG_PATH=/usr/local/llvm19/bin/clang` and
  `LIBCLANG_PATH=/usr/local/llvm19/lib`. Without it bindgen takes libclang from `llvm19` and the
  builtin headers from the base clang, and `wuffs-sys` fails in `arm_neon.h`.
- `sentry-sys` is excluded, as in the Windows lane: its build script refuses every target but
  Darwin, like the reference's `pkg/sentry`.
- Run the workspace tests with a display and a private session bus:
  `DISPLAY=:1 dbus-run-session -- cargo test --workspace --exclude sentry-sys -- --skip
  declare_ax_prompt_command_is_installed_and_reports_status_correctly`, then that one test inside
  a pty: `script -q /dev/null cargo test -p rustty-extra --test bash_shell_integration_gate`.
  bash 5.3 on FreeBSD stops itself (SIGTTIN) when run as `bash -i` without a terminal; running
  everything under `script` instead makes `pager_non_tty` wait on a real pager.
- The live GTK KATs run one at a time under the headless sway: `~/run-live.sh < ~/live-list.txt`
  on the VM (`XDG_RUNTIME_DIR=/tmp/xdg-runtime-1001`, `WAYLAND_DISPLAY=wayland-1`, `SWAYSOCK`
  from that directory; recipes per KAT as in `gtk_render_surface.rs`'s header).

## Desktops (all start at boot)

- Console (the UTM window): LightDM auto-logs `gael` into sway (Wayland) running on Xorg, through
  `/usr/local/bin/sway-on-x11-session`. arm64 FreeBSD has no DRM driver for virtio-gpu, so a
  compositor cannot drive the console directly; `virtio_gpu_qemu.ko` (loader.conf) gives Xorg a
  mappable framebuffer. Xfce stays installed (`autologin-session=xfce` to switch back).
- `Xvnc :1` with Xfce (`/usr/local/etc/rc.d/xvnc_session`), `127.0.0.1:5901`, password in
  `~/.vnc/passwd`; reach it with `ssh -L 5901:127.0.0.1:5901 rbsd` and `vnc://localhost:5901`.
- Headless sway with wayvnc (`/usr/local/etc/rc.d/sway_headless`), `127.0.0.1:5902`.

## The oracle on FreeBSD

`~/oracle/ghostty` holds the pin's tracked tree plus `zig-pkg/`; `zig build` there (Zig 0.16.0)
builds Ghostty at `246f70287`. It was the behaviour reference for the Wayland virtual-keyboard
comparison and for N557.

## Not covered

- The three DMA-BUF live KATs: the VM has no DRM driver, so Mesa exports no DMA-BUF.
- Sanitizers (Rust supports ASan/TSan on FreeBSD on x86_64 only) and amd64.

## Disk

The guest's freed blocks reach the Mac only through TRIM: `zpool set autotrim=on zroot` is set,
and `sudo zpool trim zroot` after deleting `target/` returns the space at once.
