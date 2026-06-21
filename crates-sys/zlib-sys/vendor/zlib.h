/* zlib.h -- interface of the 'zlib' general purpose compression library
  version 1.3.1, January 22nd, 2024

  Copyright (C) 1995-2024 Jean-loup Gailly and Mark Adler

  This software is provided 'as-is', without any express or implied
  warranty.  In no event will the authors be held liable for any damages
  arising from the use of this software.

  Permission is granted to anyone to use this software for any purpose,
  including commercial applications, and to alter it and redistribute it
  freely, subject to the following restrictions:

  1. The origin of this software must not be misrepresented; you must not
     claim that you wrote the original software. If you use this software
     in a product, an acknowledgment in the product documentation would be
     appreciated but is not required.
  2. Altered source versions must be plainly marked as such, and must not be
     misrepresented as being the original software.
  3. This notice may not be removed or altered from any source distribution.

  Jean-loup Gailly        Mark Adler
  jloup@gzip.org          madler@alumni.caltech.edu

  R3 vendored subset (zlib 1.3.1): the minimal public surface the Phase-1
  vendor-proof slice (adler32) consumes. Copied from the SAME upstream library
  the reference links; built standalone (no ghostty dependency).
*/

#ifndef ZLIB_H
#define ZLIB_H

#include "zconf.h"

#ifdef __cplusplus
extern "C" {
#endif

#define ZLIB_VERSION "1.3.1"
#define ZLIB_VERNUM 0x1310

/* The null pointer constant used by the public API (upstream zlib.h). */
#ifndef Z_NULL
#  define Z_NULL 0
#endif

/*
     Update a running Adler-32 checksum with the bytes buf[0..len-1] and
   return the updated checksum. An Adler-32 value is in the range of a 32-bit
   unsigned integer. If buf is Z_NULL, this function returns the required
   initial value for the checksum.

     An Adler-32 checksum is almost as reliable as a CRC-32 but can be computed
   much faster.
*/
ZEXTERN uLong ZEXPORT adler32 OF((uLong adler, const Bytef *buf, uInt len));

/*
     Same as adler32(), but with a size_t length.
*/
ZEXTERN uLong ZEXPORT adler32_z OF((uLong adler, const Bytef *buf,
                                    z_size_t len));

/*
     Combine two Adler-32 checksums into one.  For two sequences of bytes, seq1
   and seq2 with lengths len1 and len2, Adler-32 checksums were calculated for
   each, adler1 and adler2.  adler32_combine() returns the Adler-32 checksum of
   seq1 and seq2 concatenated, requiring only adler1, adler2, and len2.
*/
ZEXTERN uLong ZEXPORT adler32_combine OF((uLong adler1, uLong adler2,
                                          long len2));

#ifdef __cplusplus
}
#endif

#endif /* ZLIB_H */
