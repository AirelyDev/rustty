"""Paired, interleaved, order-rotated wall-clock runs of two rustty-bench binaries."""
import os, subprocess, sys, time, statistics, json, resource
S = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(S, '..', '..', '..'))
C = os.path.join(REPO, '.port/phase-0/baseline/corpora')
bins = sys.argv[1].split(',')
N = int(sys.argv[2])
only = sys.argv[3].split(',') if len(sys.argv) > 3 else None
CONFIGS = [
    ('terminal-stream/ascii', ['+terminal-stream', '--data=%s/ascii.bin' % C]),
    ('terminal-parser/utf8', ['+terminal-parser', '--data=%s/utf8.bin' % C]),
    ('screen-clone.noop/small', ['+screen-clone', '--mode=noop', '--data=%s/ascii-small.bin' % C]),
    ('screen-clone.clone/small', ['+screen-clone', '--mode=clone', '--data=%s/ascii-small.bin' % C]),
    ('screen-clone.render/small L1', ['+screen-clone', '--mode=render', '--loops=1', '--data=%s/ascii-small.bin' % C]),
    ('screen-clone.render/small L5', ['+screen-clone', '--mode=render', '--loops=5', '--data=%s/ascii-small.bin' % C]),
]
def run(b, args):
    env = dict(os.environ)
    exe = b
    if b.endswith('+C'):
        exe = b[:-2]
        env['RUSTTY_S7A_CALLOC'] = '1'
    else:
        env.pop('RUSTTY_S7A_CALLOC', None)
    ru0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t = time.perf_counter()
    r = subprocess.run([os.path.join(S, exe)] + args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    dt = (time.perf_counter() - t) * 1000
    ru1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    if os.environ.get('S7A_CPU'):
        dt = ((ru1.ru_utime - ru0.ru_utime) + (ru1.ru_stime - ru0.ru_stime)) * 1000
    assert r.returncode == 0, (b, args, r.returncode)
    return dt
out = {}
for cid, args in CONFIGS:
    if only and not any(o in cid for o in only):
        continue
    res = {b: [] for b in bins}
    for i in range(N):
        order = bins[i % len(bins):] + bins[:i % len(bins)]
        for b in order:
            res[b].append(run(b, args))
    base = bins[0]
    line = [cid]
    for b in bins:
        mn, md = min(res[b]), statistics.median(res[b])
        line.append('%s min %.2f med %.2f' % (b, mn, md))
    for b in bins[1:]:
        dmin = (min(res[b]) / min(res[base]) - 1) * 100
        dmed = (statistics.median(res[b]) / statistics.median(res[base]) - 1) * 100
        # paired per-iteration ratio median
        pr = statistics.median([y / x for x, y in zip(res[base], res[b])])
        line.append('%s vs %s: min %+.1f%% med %+.1f%% paired-med %+.1f%%' % (b, base, dmin, dmed, (pr - 1) * 100))
    print(' | '.join(line), flush=True)
    out[cid] = res
print('load', os.getloadavg())
json.dump(out, open(os.path.join(S, 'paired-%d.json' % int(time.time())), 'w'))
