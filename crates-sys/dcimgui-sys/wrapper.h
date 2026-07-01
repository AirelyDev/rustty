/* dcimgui-sys bindgen surface.
 *
 * Mirrors the reference's binding import (ghostty/pkg/dcimgui/main.zig) exactly:
 * the single public Dear Bindings C header the reference's `main.zig` @cIncludes
 * (`dcimgui.h`). The two layout-affecting macros the reference @cDefines before
 * the include — `IMGUI_USE_WCHAR32=1` (widens `ImWchar` to 32 bits) and
 * `IMGUI_HAS_DOCK=1` (docking-branch API) — are supplied by the build script's
 * bindgen clang args (together with `IMGUI_DISABLE_OBSOLETE_FUNCTIONS=1`, which
 * the vendored library is compiled with, so the generated symbol set matches the
 * compiled archive). bindgen parses this against the vendored Dear ImGui 1.92.5
 * `imgui/` tree (for `imconfig.h`, which `dcimgui.h` includes) plus the Dear
 * Bindings `dcimgui/` dir. */
#include <dcimgui.h>
