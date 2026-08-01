"""Summarize dhat-heap.json — where harbor allocates memory.

Usage:
    python scripts/dhat_analyze.py [path-to-dhat-heap.json]
    (default: dhat-heap.json in the current directory)

Reports global totals plus an allocation-by-owner breakdown, where an
"owner" is the deepest harbor frame in each allocation backtrace.
"""
import json
import sys
from collections import defaultdict

path = sys.argv[1] if len(sys.argv) > 1 else 'dhat-heap.json'
with open(path) as f:
    data = json.load(f)

pps = data['pps']
ftbl = data['ftbl']

def frame(fid):
    if 0 < fid <= len(ftbl):
        s = ftbl[fid - 1]
        # format: 0xADDR: fn (file:line:col)
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
    if 'harbor_' in n or n.startswith('harbor::'):
        return 'harbor'
    if n.startswith('alloc::') or n.startswith('std::') or n.startswith('core::'):
        return 'stdlib'
    return '3rd-party'

def harbor_frames(p):
    return [fid for fid in p['fs'] if classify(fid) == 'harbor']

def owner_of(p):
    hf = harbor_frames(p)
    return fn_name(hf[-1]) if hf else '(none/other)'

def mi(b):
    return b / 1048576.0

# ── global totals ──────────────────────────────────────────────────────────
tot_tb = sum(p['tb'] for p in pps)
tot_mb = sum(p['mb'] for p in pps)
tot_gb = sum(p['gb'] for p in pps)
tot_tbk = sum(p['tbk'] for p in pps)
te = data['te'] - data['tg']
print(f"file: {path}")
print(f"cmd : {data['cmd']}")
print(f"run : {te/1e6:.2f} s   mode={data['mode']} verb={data['verb']}")
print(f"total allocated : {tot_tb:>12,} B ({mi(tot_tb):7.2f} MiB)   calls={tot_tbk:,}")
print(f"max-live sum    : {tot_mb:>12,} B ({mi(tot_mb):7.2f} MiB)   [per-pp upper bound]")
print(f"gmax-live sum   : {tot_gb:>12,} B ({mi(tot_gb):7.2f} MiB)   [true-peak upper bound]")
print()

# ── attribution by deepest harbor frame ────────────────────────────────────
by_owner = defaultdict(lambda: [0, 0, 0])  # owner -> [total, live, calls]
by_class = defaultdict(lambda: [0, 0, 0])  # class -> [total, live, calls]
for p in pps:
    hf = harbor_frames(p)
    owner = fn_name(hf[-1]) if hf else '(none/other)'
    by_owner[owner][0] += p['tb']
    by_owner[owner][1] += p['mb']
    by_owner[owner][2] += p['tbk']
    cls = classify(hf[-1]) if hf else 'none'
    by_class[cls][0] += p['tb']
    by_class[cls][1] += p['mb']
    by_class[cls][2] += p['tbk']

print("=== allocation by owner (deepest harbor frame) ===")
print(f"{'total B':>13} {'live B':>12} {'calls':>8}  share   owner")
for owner, (t, l, c) in sorted(by_owner.items(), key=lambda kv: -kv[1][0])[:15]:
    print(f"{t:>13,} {l:>12,} {c:>8,}  {t/tot_tb*100:5.1f}%   {owner}")
print()
print("=== allocation by class ===")
for cls, (t, l, c) in sorted(by_class.items(), key=lambda kv: -kv[1][0]):
    print(f"  {cls:10s} {t:>12,} B ({mi(t):7.2f} MiB, {t/tot_tb*100:5.1f}%)   live {l:>10,}")
