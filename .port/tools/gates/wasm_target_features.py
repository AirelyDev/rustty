#!/usr/bin/env python3
"""Assert a wasm module was built with a CPU feature (G-XTARGET's simd128 arm, N422).

The reference resolves every wasm target with the `simd128` CPU feature unless
an explicit CPU was requested (`src/build/Config.zig:113-130`), and its CI
proves it on the artifact (`test.yml` "Verify WASM simd128": the module must
FAIL `wasm-tools validate --features=-simd`). rustty's counterpart of the
default is `.cargo/config.toml`'s wasm32 rustflags. This reads the module's
`target_features` custom section, which wasm-ld writes from the features its
inputs were compiled with, so it needs no wasm-tools on the CI image.

Usage:
  wasm_target_features.py <module.wasm> <feature>   exit 0 if `+<feature>` is listed, 1 if not
  wasm_target_features.py --self-test               exit 0 if the checker itself works
"""
import sys


def _leb(b, i):
    r = s = 0
    while True:
        x = b[i]
        i += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, i


def features(blob):
    """The `target_features` entries of a wasm module, e.g. ['+simd128', ...],
    or None when the module has no such section."""
    if blob[:4] != b"\0asm":
        raise ValueError("not a wasm module")
    i = 8
    out = None
    while i < len(blob):
        sid = blob[i]
        size, i = _leb(blob, i + 1)
        end = i + size
        if sid == 0:
            nl, j = _leb(blob, i)
            if blob[j:j + nl] == b"target_features":
                j += nl
                out = []
                n, j = _leb(blob, j)
                for _ in range(n):
                    prefix = chr(blob[j])
                    fl, j = _leb(blob, j + 1)
                    out.append(prefix + blob[j:j + fl].decode())
                    j += fl
        i = end
    return out


def _uleb(n):
    out = bytearray()
    while True:
        byte = n & 0x7F
        n >>= 7
        if n:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def _module(feats):
    """A minimal wasm module: a type section, then (optionally) the section."""
    body = b"\0asm\x01\0\0\0" + b"\x01\x04\x01\x60\x00\x00"
    if feats is not None:
        name = b"target_features"
        payload = _uleb(len(name)) + name + _uleb(len(feats))
        for f in feats:
            payload += f[0].encode() + _uleb(len(f) - 1) + f[1:].encode()
        body += b"\x00" + _uleb(len(payload)) + payload
    return body


def self_test():
    ok = True

    def expect(label, got, want):
        nonlocal ok
        good = got == want
        ok &= good
        print(f"  [{'PASS' if good else 'FAIL'}] {label}")

    expect("section with +simd128 is read", features(_module(["+bulk-memory", "+simd128"])),
           ["+bulk-memory", "+simd128"])
    expect("section without simd128 has no +simd128",
           "+simd128" in features(_module(["+bulk-memory", "+sign-ext"])), False)
    expect("a -simd128 entry is not +simd128",
           "+simd128" in features(_module(["-simd128"])), False)
    expect("no section reads as None", features(_module(None)), None)
    try:
        features(b"\x7fELF....")
        expect("non-wasm input is refused", False, True)
    except ValueError:
        expect("non-wasm input is refused", True, True)
    print(f"SELF-TEST: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    path, feature = argv[1], argv[2]
    with open(path, "rb") as f:
        feats = features(f.read())
    if feats is None:
        print(f"{path}: no target_features section")
        return 1
    if "+" + feature not in feats:
        print(f"{path}: +{feature} not among {feats}")
        return 1
    print(f"{path}: +{feature}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
