// NEW-FINDINGS N509: Mesa's EGL leaves a never-terminated display's driver
// data "definitely lost" at process exit, with no Rustty code in the process.
// The evidence behind the two `mesa-egl-dri2-*-not-terminated` entries of
// `.port/tools/gates/valgrind-llvmpipe.supp`.
//
//   cc -g -O0 -o egl-no-terminate egl-no-terminate.c -lEGL
//   LIBGL_ALWAYS_SOFTWARE=1 valgrind --leak-check=full \
//       --errors-for-leak-kinds=definite ./egl-no-terminate keep
//   LIBGL_ALWAYS_SOFTWARE=1 valgrind --leak-check=full \
//       --errors-for-leak-kinds=definite ./egl-no-terminate terminate
//
// Measured 2026-10-07 on the Fedora 44 aarch64 VM (mesa-libEGL 26.1.4,
// libglvnd-egl 1.7.0, valgrind 3.27.1): `keep` (the reference's and the
// port's policy: never eglTerminate the shared display) reports 2 definite
// leaks, 384 bytes from `dri2_display_create` and 32 from `_eglCreateArray`
// under `_eglLinkConfig`, both under `eglInitialize`; `terminate` reports 0
// errors. With the suppression file, `keep` is 0 errors (suppressed: 2), and
// a 4 KiB malloc leaked after eglInitialize is still reported.
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
  PFNEGLGETPLATFORMDISPLAYEXTPROC gpd =
      (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
  EGLDisplay d = gpd(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
  EGLint maj, min;
  if (!eglInitialize(d, &maj, &min)) { puts("init failed"); return 2; }
  printf("EGL %d.%d\n", maj, min);
  if (argc > 1 && strcmp(argv[1], "terminate") == 0) eglTerminate(d);
  return 0;
}
