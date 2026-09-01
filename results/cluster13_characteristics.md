# cluster13: workload characteristics

Measured directly from `/home/griff/eval_traces/cluster13.bin` (numpy over the
raw 25-byte records), not inferred from run output. Reproduce with
`results/cluster13_characteristics.py`.

## Shape

| | |
|---|---|
| records | 151,075,072 (3.78 GB, 25 B each) |
| commands | 100% GET. 0 SET, 0 DEL |
| TTLs | none — `ttl` is zero on every record |
| timestamps | 1000-tick granularity (1 s resolution stored x1000), monotonic |
| span | 625,199,000 ticks = **7.24 days**, ~241 req/s mean |
| distinct keys | 72,472,986 |
| working set | 406.3 GB (sum of first-seen sizes) |

**Caveat that matters for any absolute claim.** `twcsv2bin.c` DROPS zero-size
gets, and on this cluster that is 155.6M records — 50.7% of the original 306.7M.
What is replayed here is the non-zero-size half of the capture. The all-GET
command mix is also an artefact of the converter: the read-through client
synthesises a set on each miss, so the write path is exercised, but the
trace itself carries no explicit writes.

## Size: sharply bimodal, and count disagrees with bytes

```
mean 5,112 B   p1 98   p25 123   p50 123   p75 2,715   p90 15,127   p99 68,407
min 91   max 690,384   distinct sizes 178,122
```

| | share of accesses | share of bytes |
|---|--:|--:|
| objects <= 200 B | **72.72%** | **1.72%** |
| objects >= 10 KB | 13.61% | **84.48%** |

A single size dominates: **63.84% of all accesses are to exactly 123-byte
objects**, another 6.87% to 98-byte objects. So by request count this is a
tiny-object workload and per-object metadata is the dominant cost; by bytes it
is a large-object workload and the value buffers are. Any per-object overhead
claim on this trace has to say which of the two it is weighting.

This is the regime `lru-sized-hybrid` exists for, and it is why cluster13's
metadata overhead reads ~0.4% when computed against mean object size but is a
large multiple of the median object.

## Popularity: essentially flat, NOT Zipfian

```
accesses/key   mean 2.08   p50 2   p90 5   p99 6   max 364
one-hit-wonders   36,207,739 keys = 49.96% of keys, 23.97% of accesses
```

| head of the key distribution | share of accesses |
|---|--:|
| top 0.01% | 0.04% |
| top 0.1% | 0.29% |
| top 1% | **2.88%** |
| top 10% | 27.47% |

For comparison, a Zipfian web trace puts 50-80% of accesses in the top 1% of
keys. Here the top 1% carries 2.88%, and the top 10% carries 27.5% against the
10% a uniform distribution would give. **There is almost no hot set to exploit.**
Half the keys are never seen twice.

## Reuse: extremely tight

Gap, in intervening accesses, between consecutive accesses to the same key
(78,602,086 reuse pairs):

```
p1=1  p10=1  p25=2  p50=4  p75=26  p90=577  p95=1,122  p99=4,342
mean 345   max 50,243,729
```

| reuse gap within | share of re-accesses |
|---|--:|
| 100,000 accesses | 99.989% |
| 1,000,000 | 99.998% |
| 1,928,337 (what the 12 GB cache held) | **99.999%** |

## The consequence: cluster13 cannot rank eviction policies

```
compulsory floor = 72,472,986 / 151,075,072 = 0.479715
measured miss ratio at 12 GB           = 0.4797   (every policy)
12 GB / 406.3 GB working set           = 2.95%
```

A cache holding **2.95% of the working set already achieves the theoretical
minimum miss ratio**. That is not a coincidence of cache size: because reuse is
so tight (p99 = 4,342 accesses) and popularity so flat, essentially everything
that will ever be re-read is re-read almost immediately, and any cache large
enough to span ~100k accesses captures all of it. Nothing is left for a policy
to be clever about.

This confirms the earlier sweep observation from the other direction: 42 of 55
policies landed exactly on the floor, with a total spread of 4,492 misses in
151M requests — 0.003%.

**A larger cache cannot help either.** The floor is compulsory misses; the only
way to move it is to have fewer than 72.5M distinct keys.

## What cluster13 IS good for

Latency and memory. 48.0% of requests are a key's first touch, so the trace
drives 72.5M inserts through the allocator and the object map under a realistic,
strongly bimodal size distribution. Miss ratio being pinned to the floor is an
advantage there: it holds the workload identical across designs, so differences
in allocated bytes and in GET/SET latency are attributable to the design rather
than to a different set of cached objects.

That is exactly how it behaved in the merged-store comparison — identical
0.4797 miss ratio across all four configurations, with clean separation in
allocated bytes (13.87 -> 13.30 GB tiered, -4.1%) and GET latency (1452 -> 1148
ns, -20.9%).

Use cluster53 or the synthetic traces when the question is which policy wins.
