/* zconf.h -- configuration of the zlib compression library
 * Copyright (C) 1995-2024 Jean-loup Gailly, Mark Adler
 * For conditions of distribution and use, see copyright notice in zlib.h
 *
 * R3 vendored subset (zlib 1.3.1). This is the minimal configuration surface
 * the Phase-1 vendor-proof slice (adler32) consumes. Copied from the SAME
 * upstream library the reference links; built standalone (no ghostty dep).
 */

#ifndef ZCONF_H
#define ZCONF_H

/* Function prototype macro (zlib's portability shim). */
#ifndef OF /* function prototypes */
#  define OF(args)  args
#endif

/* Visibility / linkage macros. */
#ifndef ZEXTERN
#  define ZEXTERN extern
#endif
#ifndef ZEXPORT
#  define ZEXPORT
#endif
#ifndef ZEXPORTVA
#  define ZEXPORTVA
#endif

#ifndef FAR
#  define FAR
#endif

/* Basic types used by the public API. */
typedef unsigned char  Byte;
typedef unsigned int   uInt;
typedef unsigned long  uLong;

typedef Byte FAR  Bytef;
typedef uInt FAR  uIntf;
typedef uLong FAR uLongf;

typedef void const *voidpc;
typedef void FAR   *voidpf;
typedef void       *voidp;

/* z_size_t: an unsigned integer the width of a pointer/size_t. */
#include <stddef.h>
typedef size_t z_size_t;

/* 32-bit unsigned integer type (used by crc/adler combine math). */
#if !defined(Z_U4) && defined(UINT_MAX)
#endif
#ifndef Z_U4
#  define Z_U4 unsigned
#endif

/* internal-linkage marker (upstream zutil.h). */
#ifndef local
#  define local static
#endif

/* 64-bit offset type for the *_combine math (upstream zconf.h). */
#include <stdint.h>
typedef int64_t z_off64_t;

#endif /* ZCONF_H */
