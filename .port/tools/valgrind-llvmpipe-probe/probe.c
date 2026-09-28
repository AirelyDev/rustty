// NEW-FINDINGS N412: does memcheck report llvmpipe's own output as undefined,
// with no Rustty code in the process? The evidence behind
// `.port/tools/gates/valgrind-llvmpipe.supp`.
//
// Draws 64 instanced quads (a triangle strip per instance, per-instance
// attributes from fully written buffers, a flat colour varying, a fully
// written R8 texture read with texelFetch) with premultiplied-alpha blending
// (GL_ONE, GL_ONE_MINUS_SRC_ALPHA, the renderer's blend) into a cleared RGBA8
// FBO over EGL surfaceless, reads the pixels back and branches on them: the
// same shape as the renderer's cell drawing and its golden tests.
//
//   cc -g -O0 -o probe probe.c -lEGL -lGL
//   LIBGL_ALWAYS_SOFTWARE=1 valgrind --track-origins=yes ./probe
//
// Measured 2026-09-28 on the Fedora 44 aarch64 VM (Mesa 26.1.4 llvmpipe,
// LLVM 22.1.8, valgrind 3.27.1), two runs each:
//   - prints `red pixels: 4096 of 4096`: every pixel has the expected value;
//   - memcheck reports a conditional jump inside LLVM's AsmPrinter under
//     `gallivm_jit_function` (origin: a stack slot of Mesa's
//     `lp_build_create_jit_compiler_for_module`);
//   - and "Conditional jump or move depends on uninitialised value(s)" at the
//     `px[i * 4] > 128` comparison below, origin "created by a stack
//     allocation" at an address with no symbol (code llvmpipe JIT-compiled).
// With NOBLEND=1 (blending off) the pixel report goes away and only the LLVM
// one remains. Blending reads the destination and scales it by
// (1 - source alpha) = 0 here; memcheck does not know that a product with a
// defined zero is defined, so undefined bytes of llvmpipe's own tile memory
// taint pixels whose value does not depend on them. Whatever the application
// uploads, llvmpipe-blended pixels can carry undefined bits in memcheck's
// shadow memory.
#define GL_GLEXT_PROTOTYPES 1
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GL/gl.h>
#include <GL/glext.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *vs =
    "#version 430 core\n"
    "layout(location=0) in uvec2 gp; layout(location=1) in uvec4 ic;\n"
    "out vec2 tc; flat out vec4 col;\n"
    "void main(){ vec2 corner=vec2(float(gl_VertexID&1), float((gl_VertexID>>1)&1));\n"
    "  vec2 cell=(vec2(gp)+corner)*8.0; tc=corner*8.0; col=vec4(ic)/255.0;\n"
    "  gl_Position=vec4(cell/32.0-1.0,0,1); }\n";
static const char *fs =
    "#version 430 core\n"
    "in vec2 tc; flat in vec4 col; uniform sampler2D t; out vec4 c;\n"
    "void main(){ float a=texelFetch(t, ivec2(tc), 0).r; c=col*a; }\n";

int main(void) {
  PFNEGLGETPLATFORMDISPLAYEXTPROC gpd =
      (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
  EGLDisplay d = gpd(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
  EGLint maj, min;
  if (!eglInitialize(d, &maj, &min)) { puts("eglInitialize failed"); return 2; }
  eglBindAPI(EGL_OPENGL_API);
  EGLint ca[] = {EGL_CONTEXT_MAJOR_VERSION, 4, EGL_CONTEXT_MINOR_VERSION, 3,
                 EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT, EGL_NONE};
  EGLContext ctx = eglCreateContext(d, EGL_NO_CONFIG_KHR, EGL_NO_CONTEXT, ca);
  if (!ctx || !eglMakeCurrent(d, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) { puts("no context"); return 2; }
  printf("%s\n", glGetString(GL_RENDERER));

  GLuint fb, tex;
  glGenFramebuffers(1, &fb);
  glBindFramebuffer(GL_FRAMEBUFFER, fb);
  glGenTextures(1, &tex);
  glBindTexture(GL_TEXTURE_2D, tex);
  glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, 64, 64, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
  glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);

  GLuint p = glCreateProgram();
  GLuint s1 = glCreateShader(GL_VERTEX_SHADER), s2 = glCreateShader(GL_FRAGMENT_SHADER);
  glShaderSource(s1, 1, &vs, NULL);
  glCompileShader(s1);
  glShaderSource(s2, 1, &fs, NULL);
  glCompileShader(s2);
  glAttachShader(p, s1);
  glAttachShader(p, s2);
  glLinkProgram(p);
  GLint ok;
  glGetProgramiv(p, GL_LINK_STATUS, &ok);
  if (!ok) { puts("link failed"); return 2; }
  glUseProgram(p);

  GLuint vao;
  glGenVertexArrays(1, &vao);
  glBindVertexArray(vao);
  glViewport(0, 0, 64, 64);
  glClearColor(0, 0, 0, 0);
  glClear(GL_COLOR_BUFFER_BIT);

  GLuint at;
  glActiveTexture(GL_TEXTURE0);
  glGenTextures(1, &at);
  glBindTexture(GL_TEXTURE_2D, at);
  unsigned char *ad = malloc(64 * 64);
  memset(ad, 255, 64 * 64);
  glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
  glTexImage2D(GL_TEXTURE_2D, 0, GL_R8, 64, 64, 0, GL_RED, GL_UNSIGNED_BYTE, ad);
  free(ad);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
  glUniform1i(glGetUniformLocation(p, "t"), 0);
  if (!getenv("NOBLEND")) {
    glEnable(GL_BLEND);
    glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
  }

  GLuint vb[2];
  glGenBuffers(2, vb);
  unsigned int gp[64 * 2];
  unsigned char ic[64 * 4];
  for (int i = 0; i < 64; i++) {
    gp[i * 2] = i % 8;
    gp[i * 2 + 1] = i / 8;
    ic[i * 4] = 255; ic[i * 4 + 1] = 0; ic[i * 4 + 2] = 0; ic[i * 4 + 3] = 255;
  }
  glBindBuffer(GL_ARRAY_BUFFER, vb[0]);
  glBufferData(GL_ARRAY_BUFFER, sizeof gp, gp, GL_STATIC_DRAW);
  glVertexAttribIPointer(0, 2, GL_UNSIGNED_INT, 0, 0);
  glEnableVertexAttribArray(0);
  glVertexAttribDivisor(0, 1);
  glBindBuffer(GL_ARRAY_BUFFER, vb[1]);
  glBufferData(GL_ARRAY_BUFFER, sizeof ic, ic, GL_STATIC_DRAW);
  glVertexAttribIPointer(1, 4, GL_UNSIGNED_BYTE, 0, 0);
  glEnableVertexAttribArray(1);
  glVertexAttribDivisor(1, 1);
  glDrawArraysInstanced(GL_TRIANGLE_STRIP, 0, 4, 64);
  glFinish();

  unsigned char *px = calloc(64 * 64 * 4, 1);
  glReadPixels(0, 0, 64, 64, GL_RGBA, GL_UNSIGNED_BYTE, px);
  int red = 0;
  for (int i = 0; i < 64 * 64; i++) {
    if (px[i * 4] > 128) red++;
  }
  printf("red pixels: %d of %d\n", red, 64 * 64);
  free(px);
  eglMakeCurrent(d, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  eglDestroyContext(d, ctx);
  eglTerminate(d);
  return red == 64 * 64 ? 0 : 1;
}
