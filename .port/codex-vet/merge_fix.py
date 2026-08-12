#!/usr/bin/env python3
"""Merge fix/verify outcomes from a wf-codexvet-fix journal into STATE.json.

Usage: merge_fix.py <journal.jsonl> [<journal.jsonl> ...]

Per finding, sets .fix = {status, wave, detail, files_touched, tests_added,
verified, verify_problems}. status is 'verified' when the fix landed AND its
independent verifier passed it, 'fixed' when it landed but verification is
pending, 'rejected' when the verifier failed it (needs re-dispatch), or the
agent-reported blocked/partial/no_change_needed.
"""
import json, sys, os, collections

ROOT = os.path.dirname(os.path.abspath(__file__))
STATE = os.path.join(ROOT, 'STATE.json')


def load(journals):
    fixes, verifies = {}, {}
    for j in journals:
        if not os.path.exists(j):
            continue
        for line in open(j):
            try:
                r = json.loads(line)
            except Exception:
                continue
            if r.get('type') != 'result':
                continue
            res = r.get('result')
            if not isinstance(res, dict):
                continue
            bid = res.get('bid')
            if not bid:
                continue
            if 'outcomes' in res:
                fixes[bid] = res
            elif 'verdicts' in res:
                verifies[bid] = res
    return fixes, verifies


def main():
    journals = sys.argv[1:]
    fixes, verifies = load(journals)
    s = json.load(open(STATE))
    idx = {i['id']: i for i in s['findings']}
    tally = collections.Counter()
    for bid, f in fixes.items():
        v = verifies.get(bid) or {}
        vmap = {d['gid']: d for d in v.get('verdicts', [])}
        for o in f.get('outcomes', []):
            gid = o.get('gid')
            it = idx.get(gid)
            if not it:
                continue
            st = o.get('status')
            ver = vmap.get(gid)
            if st == 'fixed' and ver is not None:
                st = 'verified' if ver.get('verified') else 'rejected'
            prev = it.get('fix') or {}
            it['fix'] = {
                'status': st,
                'wave': prev.get('wave', 'w1'),
                'bid': bid,
                'detail': (o.get('detail') or '')[:1200],
                'files_touched': o.get('files_touched', []),
                'tests_added': o.get('tests_added', []),
                'tests_run': (f.get('tests_run') or '')[:600],
                'verified': (ver or {}).get('verified'),
                'verify_problems': ((ver or {}).get('problems') or '')[:1200],
            }
            tally[st] += 1
    # Standalone verify results (verify-only / vmverify batches carry no
    # outcomes): apply their verdicts onto whatever fix record already exists.
    fixed_bids = set(fixes)
    for bid, v in verifies.items():
        if bid in fixed_bids:
            continue
        for d in v.get('verdicts', []):
            it = idx.get(d.get('gid'))
            if not it:
                continue
            fx = it.setdefault('fix', {})
            fx['verified'] = d.get('verified')
            fx['verify_problems'] = (d.get('problems') or '')[:1200]
            fx['verify_bid'] = bid
            if d.get('verified'):
                fx['status'] = 'verified'
                tally['verified(standalone)'] += 1
            else:
                fx['status'] = 'rejected'
                tally['rejected(standalone)'] += 1
    # Orchestrator overrides (hand-adjudicated statuses that must survive
    # re-merges, e.g. agent-reported 'partial' the verifier called clean).
    ovp = os.path.join(ROOT, 'overrides.json')
    if os.path.exists(ovp):
        for gid, o in json.load(open(ovp)).items():
            it = idx.get(gid)
            if not it:
                continue
            fx = it.setdefault('fix', {})
            fx['status'] = o['status']
            fx['orchestrator_note'] = o.get('note', '')
            tally['override:' + o['status']] += 1
    json.dump(s, open(STATE, 'w'), indent=1)
    print('merged batches:', len(fixes), 'verified batches:', len(verifies))
    for k, n in tally.most_common():
        print(f'  {k}: {n}')
    pend = [i['id'] for i in s['findings']
            if (i.get('fix') or {}).get('status') == 'pending']
    rej = [i['id'] for i in s['findings']
           if (i.get('fix') or {}).get('status') in ('rejected', 'blocked', 'partial')]
    print('still pending:', len(pend))
    if rej:
        print('needs attention:', rej)


if __name__ == '__main__':
    main()
