// NEW-FINDINGS N510: memcheck reports the read-back pixels of llvmpipe's
// scissored clears as undefined, with no Rustty code in the process. The C
// twin of `opengl::target::tests::target_read_pixels_alloc_returns_tight_rgba_rows`
// (a 3x2 sRGB RGBA8 texture FBO cleared red, its bottom-left texel cleared
// green through a scissor, read back with glReadPixels and compared).
//
//   cc -g -O0 -o readback-clear readback-clear.c -lEGL -lGL
//   LIBGL_ALWAYS_SOFTWARE=1 valgrind --track-origins=yes ./readback-clear
//
// Measured 2026-10-07 on the Fedora 44 aarch64 VM (Mesa 26.1.4 llvmpipe,
// valgrind 3.27.1): prints `GL err=0 same=1` (every byte has its expected
// value), and memcheck reports "Conditional jump or move depends on
// uninitialised value(s)" at the memcmp of the pixels, origin "created by a
// stack allocation" at an address with no symbol (code llvmpipe
// JIT-compiled), the origin N412 found for blended pixels. The display is
// terminated, so N509's leak records do not appear.
#define GL_GLEXT_PROTOTYPES 1
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GL/gl.h>
#include <GL/glext.h>
#include <stdio.h>
#include <string.h>

int main(void) {
  PFNEGLGETPLATFORMDISPLAYEXTPROC gpd =
      (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
  EGLDisplay d = gpd(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
  EGLint maj, min;
  eglInitialize(d, &maj, &min);
  eglBindAPI(EGL_OPENGL_API);
  EGLint ca[] = {EGL_SURFACE_TYPE, 0, EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT,
                 EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8,
                 EGL_ALPHA_SIZE, 8, EGL_NONE};
  EGLConfig cfg;
  EGLint n = 0;
  if (!eglChooseConfig(d, ca, &cfg, 1, &n) || n < 1) { puts("no config"); return 3; }
  EGLint xa[] = {EGL_CONTEXT_MAJOR_VERSION, 4, EGL_CONTEXT_MINOR_VERSION, 3,
                 EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
                 EGL_NONE};
  EGLContext c = eglCreateContext(d, cfg, EGL_NO_CONTEXT, xa);
  eglMakeCurrent(d, EGL_NO_SURFACE, EGL_NO_SURFACE, c);

  GLuint tex, fb;
  glGenTextures(1, &tex);
  glBindTexture(GL_TEXTURE_2D, tex);
  glTexImage2D(GL_TEXTURE_2D, 0, GL_SRGB8_ALPHA8, 3, 2, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
  glGenFramebuffers(1, &fb);
  glBindFramebuffer(GL_FRAMEBUFFER, fb);
  glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
  glEnable(GL_FRAMEBUFFER_SRGB);
  glClearColor(1, 0, 0, 1);
  glClear(GL_COLOR_BUFFER_BIT);
  glEnable(GL_SCISSOR_TEST);
  glScissor(0, 0, 1, 1);
  glClearColor(0, 1, 0, 1);
  glClear(GL_COLOR_BUFFER_BIT);
  glDisable(GL_SCISSOR_TEST);

  unsigned char px[24] = {0};
  glBindFramebuffer(GL_READ_FRAMEBUFFER, fb);
  glReadPixels(0, 0, 3, 2, GL_RGBA, GL_UNSIGNED_BYTE, px);
  unsigned char R[4] = {255, 0, 0, 255}, G[4] = {0, 255, 0, 255}, e[24];
  memcpy(e, G, 4);
  for (int i = 1; i < 6; i++) memcpy(e + 4 * i, R, 4);
  int same = memcmp(px, e, 24) == 0;
  printf("GL err=%x same=%d\n", glGetError(), same);

  eglMakeCurrent(d, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  eglDestroyContext(d, c);
  eglTerminate(d);
  return same ? 0 : 1;
}
