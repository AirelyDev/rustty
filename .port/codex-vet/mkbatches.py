#!/usr/bin/env python3
"""Partition confirmed+fixable findings from STATE.json into fix LANES.

A lane is an ordered list of sub-batches. Lanes are mutually file-disjoint and
run in parallel; sub-batches within a lane run sequentially (they share files).
Sub-batches group at most MAXB findings, preferring same primary-dir families.

Emits {wave, lanes:[{lane, batches:[{bid, findings, files}]}]} on stdout.

Usage: mkbatches.py [--wave NAME] [--only gid,...] [--exclude gid,...] [--maxb N]
"""
import json, sys, os, re, collections

ROOT = os.path.dirname(os.path.abspath(__file__))
STATE = os.path.join(ROOT, 'STATE.json')
MAXB = 4

def norm(f):
    return f.replace('/Users/gael/projects/rustty/', '').lstrip('/')

def primary_key(it):
    # family key: directory of the first (most specific) file, else gid prefix
    fs = it['files']
    if fs:
        return os.path.dirname(fs[0])
    return it['gid'].split('-')[0]

def main():
    args = sys.argv[1:]
    wave = 'w1'
    only = exclude = None
    maxb = MAXB
    while args:
        a = args.pop(0)
        if a == '--wave': wave = args.pop(0)
        elif a == '--only': only = set(args.pop(0).split(','))
        elif a == '--exclude': exclude = set(args.pop(0).split(','))
        elif a == '--maxb': maxb = int(args.pop(0))
    s = json.load(open(STATE))
    items = []
    for i in s['findings']:
        v = i.get('vet')
        if not v or v.get('covered_by'): continue
        if v.get('verdict') != 'confirmed' or not v.get('fixable_now'): continue
        fx = i.get('fix') or {}
        if fx.get('status') in ('fixed', 'verified', 'no_fix_needed'): continue
        if only and i['id'] not in only: continue
        if exclude and i['id'] in exclude: continue
        files = sorted({norm(f) for f in (v.get('files') or [])})
        items.append({'gid': i['id'], 'title': i['title'][:140],
                      'fix_category': v.get('fix_category', 'code-fix'),
                      'files': files})

    # hand overrides: kill cross-lane file overlaps (codegen/xtask/headers/
    # Cargo-features serialize in one chain; screen/hash_map stay in the vt
    # chain; the renderer seam family stays together)
    OVERRIDES = {
        'W57-B03': 'rustty-vt', 'W59-B04': 'rustty-vt', 'W24-B01-F1': 'rustty-vt',
        'W36-B06': 'rustty-vt-c',
        'W94-B01-01': 'rustty-config', 'W95-B02-01': 'rustty-config',
        'W18-B07-PLUS-01': 'rustty-config', 'W21-B02-RGB-01': 'rustty-config',
        'F-013': 'rustty-renderer', 'F-014': 'rustty-renderer', 'W8-03': 'rustty-renderer',
        'W12-02': 'codegen-abi', 'W12-03': 'codegen-abi', 'W36-B03': 'codegen-abi',
        'W16-TARGET-01': 'codegen-abi', 'W92-B02-01': 'codegen-abi',
        'W92-B02-02': 'codegen-abi', 'W92-B02-04': 'codegen-abi',
        'W92-B04-02': 'codegen-abi', 'W22-B02-DIAG-01': 'codegen-abi',
        'W22-B01-TEST-01': 'codegen-abi', 'W93-B01-DIAG-01': 'codegen-abi',
        'F-018': 'codegen-abi', 'W2-01': 'codegen-abi', 'W2-03': 'codegen-abi',
        'F-006': 'codegen-abi', 'W14-C01': 'codegen-abi', 'F-007': 'codegen-abi',
    }

    # chain key = majority subsystem of touched files (docs don't vote)
    def chain_key(it):
        if it['gid'] in OVERRIDES:
            return OVERRIDES[it['gid']]
        votes = collections.Counter()
        for f in it['files']:
            if f.endswith('.md'):
                continue
            m = re.match(r'crates/([^/]+)/', f)
            if m:
                k = m.group(1)
                # C-ABI headers belong to their generating crate
            elif f.startswith('include/rustty-apprt'):
                k = 'rustty-apprt-c'
            elif f.startswith('include/'):
                k = 'rustty-vt-c'
            elif f.startswith('xtask/') or f.startswith('dist/') or f == 'PACKAGING.md':
                k = 'xtask'
            elif f.startswith('.port/phase-0/'):
                k = 'harness'
            elif f.startswith('.port/'):
                continue
            else:
                k = f.split('/')[0]
            votes[k] += 1
        if not votes:
            return 'misc-' + it['gid']
        return votes.most_common(1)[0][0]

    clusters = collections.defaultdict(list)
    for it in items:
        clusters[chain_key(it)].append(it)
    lanes = []
    for ln, (root, its) in enumerate(sorted(clusters.items(), key=lambda kv: -len(kv[1]))):
        # stable family ordering inside the lane
        its.sort(key=lambda it: (primary_key(it), it['gid']))
        subs, cur, curkey = [], [], None
        for it in its:
            k = primary_key(it)
            if cur and (len(cur) >= maxb or k != curkey):
                subs.append(cur); cur = []
            cur.append(it); curkey = k
        if cur: subs.append(cur)
        lane = {'lane': f'{wave}-L{ln+1:02d}', 'batches': []}
        for bn, sub in enumerate(subs):
            files = sorted({f for it in sub for f in it['files']})
            lane['batches'].append({
                'bid': f'{wave}-L{ln+1:02d}-b{bn+1:02d}',
                'findings': [{k: it[k] for k in ('gid', 'title', 'fix_category')} for it in sub],
                'files': files})
        lanes.append(lane)
    json.dump({'wave': wave, 'lanes': lanes}, sys.stdout, indent=1)
    n = sum(len(b['findings']) for l in lanes for b in l['batches'])
    print(f"\n# {n} findings -> {len(lanes)} lanes", file=sys.stderr)
    for l in lanes:
        print(f"#  {l['lane']}: {len(l['batches'])} sub-batches", file=sys.stderr)
        for b in l['batches']:
            print(f"#    {b['bid']}: [{', '.join(f['gid'] for f in b['findings'])}]", file=sys.stderr)

if __name__ == '__main__':
    main()
