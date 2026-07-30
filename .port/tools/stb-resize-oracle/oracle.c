/* WS11: emit raw resized bytes for a deterministic input, so any toolchain's build of
 * the vendored stb_image_resize can be diffed byte-for-byte. Deliberately links no
 * rustty code: the only variable under test is the C build.
 *
 * Two modes:
 *
 *   oracle <out_w> <out_h>
 *       Resize the built-in 8x16 grayscale ramp. The ramp is byte-identical to
 *       `ramp_8x16()` in crates/rustty-font-shaper/src/stb.rs — KEEP THEM IN SYNC.
 *
 *   oracle <out_w> <out_h> <in_file> <in_w> <in_h> <channels>
 *       Resize an arbitrary raw, densely-packed input read from <in_file>
 *       (in_stride = in_w * channels). This is the mode used to VALIDATE the
 *       in-crate KAT fixtures: the pre-resize FreeType bitmap is dumped from the
 *       pipeline and replayed here under every toolchain in the matrix.
 *
 * Raw output bytes (out_w * out_h * channels) go to stdout.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern int stbir_resize_uint8(const unsigned char *, int, int, int, unsigned char *, int, int, int,
                              int);

int main(int argc, char **argv) {
    if (argc != 3 && argc != 7) {
        fprintf(stderr, "usage: oracle <out_w> <out_h> [<in_file> <in_w> <in_h> <channels>]\n");
        return 2;
    }

    int w = atoi(argv[1]);
    int h = atoi(argv[2]);
    if (w <= 0 || h <= 0) {
        fprintf(stderr, "oracle: out dims must be positive\n");
        return 2;
    }

    unsigned char *src = NULL;
    int in_w, in_h, chans;

    if (argc == 3) {
        /* Built-in deterministic 8x16 grayscale ramp. */
        in_w = 8;
        in_h = 16;
        chans = 1;
        src = malloc((size_t)(in_w * in_h * chans));
        if (!src) return 3;
        for (int y = 0; y < in_h; y++)
            for (int x = 0; x < in_w; x++)
                src[y * in_w + x] = (unsigned char)((x * 31 + y * 17) & 0xff);
    } else {
        in_w = atoi(argv[4]);
        in_h = atoi(argv[5]);
        chans = atoi(argv[6]);
        if (in_w <= 0 || in_h <= 0 || chans <= 0) {
            fprintf(stderr, "oracle: in dims/channels must be positive\n");
            return 2;
        }
        size_t n = (size_t)in_w * (size_t)in_h * (size_t)chans;
        src = malloc(n);
        if (!src) return 3;
        FILE *f = fopen(argv[3], "rb");
        if (!f) {
            fprintf(stderr, "oracle: cannot open %s\n", argv[3]);
            return 3;
        }
        if (fread(src, 1, n, f) != n) {
            fprintf(stderr, "oracle: %s is shorter than %d*%d*%d bytes\n", argv[3], in_w, in_h,
                    chans);
            fclose(f);
            return 3;
        }
        fclose(f);
    }

    size_t out_n = (size_t)w * (size_t)h * (size_t)chans;
    unsigned char *dst = malloc(out_n);
    if (!dst) return 3;
    memset(dst, 0, out_n);

    if (stbir_resize_uint8(src, in_w, in_h, in_w * chans, dst, w, h, w * chans, chans) == 0) {
        fprintf(stderr, "oracle: stbir_resize_uint8 returned 0\n");
        return 4;
    }

    if (fwrite(dst, 1, out_n, stdout) != out_n) return 5;
    free(src);
    free(dst);
    return 0;
}
