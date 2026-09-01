import numpy as np, sys

PATH = "/home/griff/eval_traces/cluster13.bin"
REC = np.dtype([('ts','<u8'),('cmd','u1'),('key','<u8'),('size','<u4'),('ttl','<u4')])
assert REC.itemsize == 25, REC.itemsize

a = np.memmap(PATH, dtype=REC, mode='r')
n = len(a)
print("records            %d" % n)

# --- sanity: the layout is what we think it is -----------------------------
cmds, cc = np.unique(a['cmd'][:5_000_000], return_counts=True)
print("cmd byte (5M head) %s" % dict(zip(cmds.tolist(), cc.tolist())))
sz = np.asarray(a['size'])
print("size min/max       %d / %d   (tracestat: 91 / 690384)" % (sz.min(), sz.max()))

ts = np.asarray(a['ts'])
t0, t1 = int(ts[0]), int(ts[-1])
span = t1 - t0
print("ts first/last      %d / %d" % (t0, t1))
print("span               %d s = %.2f h   -> %.0f req/s" % (span, span/3600.0, n/max(span,1)))
print("ts monotonic       %s" % bool(np.all(np.diff(ts[::1000]) >= 0)))
del ts

keys = np.asarray(a['key'])

# --- popularity -------------------------------------------------------------
print("\nsorting %d keys ..." % n); sys.stdout.flush()
order = np.argsort(keys, kind='stable')
sk = keys[order]

newgrp = np.empty(n, dtype=bool)
newgrp[0] = True
np.not_equal(sk[1:], sk[:-1], out=newgrp[1:])
starts = np.flatnonzero(newgrp)
distinct = len(starts)
counts = np.diff(np.append(starts, n))

print("distinct keys      %d" % distinct)
print("compulsory floor   %.6f   (distinct / records)" % (distinct / n))
print("one-hit-wonders    %d  = %.2f%% of keys, %.2f%% of accesses"
      % ((counts == 1).sum(), 100.0*(counts == 1).sum()/distinct,
         100.0*(counts == 1).sum()/n))

cs = np.sort(counts)[::-1]
cum = np.cumsum(cs, dtype=np.int64)
for frac in (0.0001, 0.001, 0.01, 0.10):
    k = max(1, int(distinct*frac))
    print("top %-7s of keys  %6.2f%% of accesses" % ("%.2f%%" % (frac*100), 100.0*cum[k-1]/n))
print("accesses/key       mean %.2f  p50 %d  p90 %d  p99 %d  max %d"
      % (counts.mean(), np.percentile(counts,50), np.percentile(counts,90),
         np.percentile(counts,99), counts.max()))

# --- reuse distance (gap in accesses between consecutive hits on a key) -----
# stable sort keeps original positions increasing inside each key group, so a
# diff of `order` across a non-boundary is exactly that gap.
gaps = np.diff(order)
reuse = gaps[~newgrp[1:]]
del gaps, order, sk
print("\nreuse pairs        %d  (= records - distinct = %d)" % (len(reuse), n - distinct))
qs = [1,10,25,50,75,90,95,99]
pr = np.percentile(reuse, qs)
print("reuse gap (accesses)  " + "  ".join("p%d=%d" % (q,v) for q,v in zip(qs, pr)))
print("reuse gap mean     %.0f   max %d" % (reuse.mean(), reuse.max()))

held = 1_928_337   # objects the 12 GB cache actually held (measured)
for w in (100_000, 1_000_000, held, 5_000_000, 20_000_000):
    print("  reuse gap <= %-10d  %6.3f%% of re-accesses" % (w, 100.0*(reuse <= w).mean()))

# --- working set ------------------------------------------------------------
first_sz = sz[np.sort(starts)] if False else None
print("\nWSS (sum of first-seen sizes) is in tracestat: 406.3 GB over %d objects" % distinct)
print("12 GB cache / WSS  %.2f%%" % (100.0*12e9/406335652833))
import numpy as np
PATH = "/home/griff/eval_traces/cluster13.bin"
REC = np.dtype([('ts','<u8'),('cmd','u1'),('key','<u8'),('size','<u4'),('ttl','<u4')])
a = np.memmap(PATH, dtype=REC, mode='r')
n = len(a)

ts = np.asarray(a['ts'])
d = np.diff(ts[:20_000_000])
print("ts granularity: all multiples of 1000?  %s" % bool((ts[:20_000_000] % 1000 == 0).all()))
print("ts distinct steps (first 20M): %s" % np.unique(d)[:8].tolist())
print("=> 625,199,000 ticks / 1000 = %.2f days" % (625199000/1000/86400))
del ts, d

sz = np.asarray(a['size'])
vals, cnts = np.unique(sz, return_counts=True)
top = np.argsort(cnts)[::-1][:8]
print("\ntop object sizes by access count:")
for i in top:
    print("   %7d B  %12d accesses  %5.2f%%" % (vals[i], cnts[i], 100.0*cnts[i]/n))
small = sz <= 200
print("\naccesses to objects <= 200 B: %.2f%%  (they are %.3f%% of the bytes)"
      % (100.0*small.mean(), 100.0*sz[small].sum()/sz.sum()))
big = sz >= 10000
print("accesses to objects >= 10 KB: %.2f%%  (they are %.2f%% of the bytes)"
      % (100.0*big.mean(), 100.0*sz[big].sum()/sz.sum()))
print("\nttl nonzero: %d" % int((np.asarray(a['ttl']) != 0).sum()))
