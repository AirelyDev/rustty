# PORTS: ghostty nix/build-support/build-inputs.nix — the shared native
# buildInputs list, reused by package.nix and by ld-library-path.nix.
#
# DIVERGENCE FROM THE REFERENCE, and the reason it is large: ghostty links a
# dozen system C libraries that rustty VENDORS. Every crates-sys/* crate builds
# its C from crates-sys/<name>/vendor/ via the `cc` crate and never consults
# pkg-config, so the following reference entries have no analog here and are
# deliberately absent:
#
#   bzip2 expat fontconfig freetype harfbuzz libpng libxml2 oniguruma
#   simdutf zlib glslang spirv-cross
#
# Keeping them would make the closure claim dependencies the build does not
# have, and would hide the day one of them stops being vendored. What remains
# is what rustty genuinely resolves from the system: the GL/GTK/desktop stack.
{
  pkgs,
  lib,
  stdenv,
  enableX11 ? true,
  enableWayland ? true,
}:
[
  pkgs.libGL
]
++ lib.optionals stdenv.hostPlatform.isLinux [
  pkgs.libxkbcommon

  pkgs.glib
  pkgs.gobject-introspection
  pkgs.gsettings-desktop-schemas
  pkgs.gst_all_1.gst-plugins-base
  pkgs.gst_all_1.gst-plugins-good
  pkgs.gst_all_1.gstreamer
  pkgs.gtk4
  pkgs.libadwaita
]
++ lib.optionals (stdenv.hostPlatform.isLinux && enableX11) [
  pkgs.libx11
  pkgs.libxcursor
  pkgs.libxi
  pkgs.libxrandr
]
++ lib.optionals (stdenv.hostPlatform.isLinux && enableWayland) [
  pkgs.gtk4-layer-shell
  pkgs.wayland
]
