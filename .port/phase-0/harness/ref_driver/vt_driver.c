// vt_driver.c — Reference VT oracle for the rustty differential harness.
//
// Reads a VT-encoded byte stream from a file (argv positional) or stdin,
// feeds it verbatim into a libghostty-vt terminal via
// ghostty_terminal_vt_write(), then emits a CANONICAL serialization of the
// resulting Screen/PageList using the libghostty-vt formatter
// (ghostty_formatter_terminal_new + ghostty_formatter_format_alloc).
//
// This mirrors example/c-vt-formatter and example/c-vt-stream from the
// READ-ONLY ghostty reference tree, but parameterizes the input (file/stdin)
// and grid/format so the harness can feed *identical* input + options to both
// the reference and the (future) rustty port and diff canonical output.
//
// Output (stdout) is the formatter's canonical bytes EXACTLY (no extra
// trailing newline, no banner) so it can be byte-diffed. Diagnostics go to
// stderr only. Exit code 0 on success; non-zero on any libghostty error.
//
// The reference is linked against zig-out/libghostty-vt — the IDENTITY oracle.
// Per R2, the rustty port side will be built with the `identity-oracle`
// profile so identity tokens (XTVERSION/DA/version strings emitted into the
// stream, TERM, etc.) match byte-for-byte with NO canonicalization here.

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <ghostty/vt.h>

static const char *result_str(GhosttyResult r) {
  switch (r) {
    case GHOSTTY_SUCCESS:      return "GHOSTTY_SUCCESS";
    default:                   return "GHOSTTY_ERROR";
  }
}

// Read an entire stream into a malloc'd buffer. Returns NULL on error.
static uint8_t *read_all(FILE *f, size_t *out_len) {
  size_t cap = 1 << 16, len = 0;
  uint8_t *buf = (uint8_t *)malloc(cap);
  if (!buf) return NULL;
  for (;;) {
    if (len == cap) {
      cap *= 2;
      uint8_t *nb = (uint8_t *)realloc(buf, cap);
      if (!nb) { free(buf); return NULL; }
      buf = nb;
    }
    size_t n = fread(buf + len, 1, cap - len, f);
    len += n;
    if (n == 0) {
      if (feof(f)) break;
      if (ferror(f)) { free(buf); return NULL; }
    }
  }
  *out_len = len;
  return buf;
}

static void usage(const char *prog) {
  fprintf(stderr,
    "usage: %s [--cols N] [--rows N] [--scrollback N]\n"
    "          [--format plain|vt|html] [--trim] [--unwrap] [--no-trim]\n"
    "          [INPUT_FILE]\n"
    "  Reads VT bytes (file or stdin), writes canonical formatted screen to stdout.\n",
    prog);
}

int main(int argc, char **argv) {
  uint16_t cols = 80, rows = 24;
  size_t scrollback = 0;
  GhosttyFormatterFormat emit = GHOSTTY_FORMATTER_FORMAT_PLAIN;
  bool trim = true;     // default: stable canonical form (trailing ws trimmed)
  bool unwrap = false;
  const char *input_path = NULL;

  for (int i = 1; i < argc; i++) {
    const char *a = argv[i];
    if (strcmp(a, "--cols") == 0 && i + 1 < argc) {
      cols = (uint16_t)strtoul(argv[++i], NULL, 10);
    } else if (strcmp(a, "--rows") == 0 && i + 1 < argc) {
      rows = (uint16_t)strtoul(argv[++i], NULL, 10);
    } else if (strcmp(a, "--scrollback") == 0 && i + 1 < argc) {
      scrollback = (size_t)strtoull(argv[++i], NULL, 10);
    } else if (strcmp(a, "--format") == 0 && i + 1 < argc) {
      const char *f = argv[++i];
      if (strcmp(f, "plain") == 0)      emit = GHOSTTY_FORMATTER_FORMAT_PLAIN;
      else if (strcmp(f, "vt") == 0)    emit = GHOSTTY_FORMATTER_FORMAT_VT;
      else if (strcmp(f, "html") == 0)  emit = GHOSTTY_FORMATTER_FORMAT_HTML;
      else { fprintf(stderr, "unknown format: %s\n", f); return 2; }
    } else if (strcmp(a, "--trim") == 0) {
      trim = true;
    } else if (strcmp(a, "--no-trim") == 0) {
      trim = false;
    } else if (strcmp(a, "--unwrap") == 0) {
      unwrap = true;
    } else if (strcmp(a, "-h") == 0 || strcmp(a, "--help") == 0) {
      usage(argv[0]); return 0;
    } else if (a[0] == '-' && a[1] != '\0') {
      fprintf(stderr, "unknown flag: %s\n", a); usage(argv[0]); return 2;
    } else {
      input_path = a;
    }
  }

  if (cols == 0 || rows == 0) {
    fprintf(stderr, "cols/rows must be > 0\n");
    return 2;
  }

  // --- Read the input VT byte stream verbatim ---
  FILE *in = stdin;
  if (input_path) {
    in = fopen(input_path, "rb");
    if (!in) { perror("fopen"); return 3; }
  }
  size_t in_len = 0;
  uint8_t *in_buf = read_all(in, &in_len);
  if (input_path) fclose(in);
  if (!in_buf) { fprintf(stderr, "failed to read input\n"); return 3; }

  // --- Create the terminal ---
  // PORT NOTE: oracle 03d5fa268 removed `GhosttyTerminalOptions` from the ABI;
  // ghostty_terminal_new now takes (allocator, out, cols, rows) and scrollback
  // is configured post-create via ghostty_terminal_set() with
  // GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES (size_t*).
  GhosttyTerminal terminal;
  GhosttyResult result = ghostty_terminal_new(NULL, &terminal, cols, rows);
  if (result != GHOSTTY_SUCCESS) {
    fprintf(stderr, "ghostty_terminal_new failed: %s\n", result_str(result));
    free(in_buf);
    return 4;
  }

  // Apply --scrollback unconditionally: the driver's contract is an EXPLICIT
  // byte limit (default 0 = scrollback disabled, exactly what the old
  // `.max_scrollback = scrollback` options field pinned). Passing the value
  // pointer (never NULL) matters: per the header, a NULL value would REMOVE
  // the byte limit rather than set it to zero.
  result = ghostty_terminal_set(
      terminal, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, &scrollback);
  if (result != GHOSTTY_SUCCESS) {
    fprintf(stderr, "ghostty_terminal_set(SCROLLBACK_MAX_BYTES) failed: %s\n",
            result_str(result));
    ghostty_terminal_free(terminal);
    free(in_buf);
    return 4;
  }

  // --- Feed the input stream into the VT parser ---
  // ghostty_terminal_vt_write returns void; it tolerates partial/invalid
  // sequences exactly as the reference parser does (the oracle behavior).
  ghostty_terminal_vt_write(terminal, in_buf, in_len);
  free(in_buf);

  // --- Build a canonical formatter over the resulting screen ---
  GhosttyFormatterTerminalOptions fmt_opts =
      GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions);
  fmt_opts.emit = emit;
  fmt_opts.trim = trim;
  fmt_opts.unwrap = unwrap;

  GhosttyFormatter formatter;
  result = ghostty_formatter_terminal_new(NULL, &formatter, terminal, fmt_opts);
  if (result != GHOSTTY_SUCCESS) {
    fprintf(stderr, "ghostty_formatter_terminal_new failed: %s\n",
            result_str(result));
    ghostty_terminal_free(terminal);
    return 5;
  }

  // --- Format into an allocated buffer ---
  uint8_t *buf = NULL;
  size_t len = 0;
  result = ghostty_formatter_format_alloc(formatter, NULL, &buf, &len);
  if (result != GHOSTTY_SUCCESS) {
    fprintf(stderr, "ghostty_formatter_format_alloc failed: %s\n",
            result_str(result));
    ghostty_formatter_free(formatter);
    ghostty_terminal_free(terminal);
    return 6;
  }

  // --- Emit canonical bytes verbatim to stdout (no banner, no extra NL) ---
  if (len > 0) {
    size_t w = fwrite(buf, 1, len, stdout);
    if (w != len) {
      fprintf(stderr, "short write to stdout\n");
      ghostty_free(NULL, buf, len);
      ghostty_formatter_free(formatter);
      ghostty_terminal_free(terminal);
      return 7;
    }
  }

  // --- Clean up ---
  ghostty_free(NULL, buf, len);
  ghostty_formatter_free(formatter);
  ghostty_terminal_free(terminal);
  return 0;
}
