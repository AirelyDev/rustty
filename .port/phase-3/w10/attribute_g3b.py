import os, sys, re, json, collections
GATES = "/Users/gael/projects/rustty/.claude/worktrees/agent-aab0cba2b9681b52e/.port/tools/gates"
sys.path.insert(0, GATES)
import _gatelib as G
GHOSTTY_SRC = "/Users/gael/projects/ghostty/src"
cm = G.crate_map()

# parse MISSING sentences per crate from a report
def parse(p):
    d = collections.defaultdict(list); cur = None
    for ln in open(p):
        m = re.match(r'^G3b \[([^\]]+)\]:', ln)
        if m: cur = m.group(1); continue
        m = re.match(r'^\s+MISSING ref comment: (.*)$', ln)
        if m and cur: d[cur].append(m.group(1).rstrip())
    return d

rep = parse(sys.argv[1])
crates = sys.argv[2:] if len(sys.argv) > 2 else sorted(rep)
for crate in crates:
    sents = rep.get(crate, [])
    if not sents: continue
    files = [os.path.join(GHOSTTY_SRC, f) for f, _ in cm[crate]["files"]]
    blobs = {}
    for f in files:
        try:
            blobs[f] = re.sub(r'[^a-z0-9_ ]', ' ', open(f, encoding='utf-8', errors='replace').read().lower())
        except OSError:
            pass
    counts = collections.Counter()
    unattributed = []
    for s in sents:
        key = re.sub(r'[^a-z0-9_ ]', ' ', s.lower())
        key = re.sub(r'\s+', ' ', key).strip()
        hit = None
        for f, b in blobs.items():
            if key and key in re.sub(r'\s+', ' ', b):
                hit = f; break
        if hit: counts[os.path.relpath(hit, GHOSTTY_SRC)] += 1
        else: unattributed.append(s)
    print("== %s (%d printed MISSING)" % (crate, len(sents)))
    for f, c in counts.most_common():
        print("   %4d  %s" % (c, f))
    if unattributed:
        print("   %4d  <unattributed>" % len(unattributed))
        for s in unattributed[:5]: print("          ~ " + s[:90])
