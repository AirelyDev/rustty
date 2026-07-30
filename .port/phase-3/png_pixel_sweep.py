import subprocess, io
from PIL import Image, ImageChops

G = "/Users/gael/projects/ghostty"
files = subprocess.run(
    ["git", "-C", G, "diff", "--name-only", "4749c4e93..origin/main", "--", "*.png"],
    capture_output=True, text=True, check=True).stdout.split()
print(f"{len(files)} changed PNG(s)")

def blob(rev, path):
    r = subprocess.run(["git", "-C", G, "show", f"{rev}:{path}"], capture_output=True)
    return r.stdout if r.returncode == 0 else None

# SELF-CHECK: getbbox() on an RGBA difference only looks at the ALPHA channel,
# so a first attempt at this sweep reported 0/37 pixel differences on images
# whose alpha is uniformly opaque. Prove the comparator is live before trusting
# a "no differences" result.
p0 = "src/font/sprite/testdata/U+2500...U+25FF-11x21+2.png"
i0 = Image.open(io.BytesIO(blob("4749c4e93", p0))).convert("L")
j0 = Image.open(io.BytesIO(blob("origin/main", p0))).convert("L")
assert i0.tobytes() != j0.tobytes(), "SELF-CHECK FAILED: comparator is not live"
print("self-check: comparator is live (known-different pair detected)\n")

pixel_diff, byte_only = [], []
for f in sorted(files):
    a, b = blob("4749c4e93", f), blob("origin/main", f)
    ia, ib = Image.open(io.BytesIO(a)), Image.open(io.BytesIO(b))
    if ia.size != ib.size or ia.mode != ib.mode:
        pixel_diff.append((f, f"{ia.mode}{ia.size} -> {ib.mode}{ib.size}", "geometry change"))
        continue
    la, lb = ia.convert("L"), ib.convert("L")
    if la.tobytes() == lb.tobytes():
        byte_only.append(f)
    else:
        d = ImageChops.difference(la, lb)
        n = sum(1 for px in d.getdata() if px)
        pixel_diff.append((f, f"{ia.mode}{ia.size}", f"{n} px differ, max delta {d.getextrema()[1]}, bbox {d.getbbox()}"))

print(f"BYTE-ONLY churn (identical pixels): {len(byte_only)}")
for f in byte_only: print("   ", f)
print(f"\nPIXEL DIFFERENCES: {len(pixel_diff)}")
for f, s, n in pixel_diff: print("   ", f, "|", s, "|", n)
