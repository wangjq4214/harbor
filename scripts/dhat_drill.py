"""Drill into specific allocation owners in dhat-heap.json.

Usage:
    python scripts/dhat_drill.py [path-to-dhat-heap.json] [owner...]

Without owners, prints the top four harbor owners by total bytes.
With owner substrings, prints the top allocation sites for each matching
owner (deepest harbor frame contains the substring).
"""
import json
import sys
from collections import defaultdict

path = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith('harbor') else 'dhat-heap.json'
filters = [a for a in sys.argv[1:] if a != path]

with open(path) as f:
    data = json.load(f)

pps = data['pps']
ftbl = data['ftbl']

def frame(fid):
    if 0 < fid <= len(ftbl):
        s = ftbl[fid - 1]
        if ': ' in s:
            addr, _, rest = s.partition(': ')
            fn, _, loc = rest.rpartition(' (')
            return fn.strip(), loc
        return s, ''
    return '?', '?'

def fn_name(fid):
    return frame(fid)[0]

def classify(fid):
    n = fn_name(fid)
    return 'harbor' if ('harbor_' in n or n.startswith('harbor::')) else 'other'

def harbor_frames(p):
    return [fid for fid in p['fs'] if classify(fid) == 'harbor']

def owner_of(p):
    hf = harbor_frames(p)
    return fn_name(hf[-1]) if hf else '(none/other)'

by_owner = defaultdict(list)
for p in pps:
    by_owner[owner_of(p)].append(p)

def drill(owner):
    plist = by_owner[owner]
    tot = sum(p['tb'] for p in plist)
    live = sum(p['mb'] for p in plist)
    print(f"===== {owner}  (pps={len(plist)}, total={tot:,} B, live={live:,} B) =====")
    # group by the deepest-3 harbor frames joined with allocation location
    sites = defaultdict(lambda: [0, 0, 0])
    for p in plist:
        hf = harbor_frames(p)
        sig = ' / '.join(
            f"{fn_name(fid)} {frame(fid)[1]}" for fid in hf[-3:]
        )
        sites[sig][0] += p['tb']
        sites[sig][1] += p['mb']
        sites[sig][2] += p['tbk']
    for sig, (t, l, c) in sorted(sites.items(), key=lambda kv: -kv[1][0])[:8]:
        print(f"  {t:>10,} B total, live {l:>8,}, calls {c:>6,}  |  {sig[:150]}")
    print()

if filters:
    for owner in sorted(by_owner):
        if any(f in owner for f in filters):
            drill(owner)
else:
    for owner, plist in sorted(by_owner.items(), key=lambda kv: -sum(p['tb'] for p in kv[1]))[:4]:
        drill(owner)
