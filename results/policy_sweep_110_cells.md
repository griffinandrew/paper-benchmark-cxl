# The 110-cell policy sweep: 55 policies, two traces

Every eviction policy the library implements, run on the full cluster13 and
cluster53 traces at a 12 GB budget (4 GiB fast tier on the 38 tiered designs),
read-through, one client. 110 cells, **zero failures**.

The headline is not a ranking. It is that **one of the two traces cannot rank
policies at all**, and that most of what looks like a latency win in the other
is a hit-rate failure wearing a disguise.

## 1. cluster13 cannot rank policies on hit rate

**42 of the 55 policies sit exactly on the compulsory-miss floor.** Within those
42, the entire spread in miss count is **4,492 requests out of 151,075,072** —
0.0062% of the misses, and below the 4 dp the harness prints.

Over that same 42, **GET mean spans 867 → 1,761 ns, a 2.031x range.**

At 12 GB, cluster13's reuse working set fits. ~70.5M objects are evicted per run
and essentially none is ever re-referenced, so the eviction decision is free.
The trace admits exactly one useful hit-rate statement — *does this policy avoid
discarding short-reuse objects, yes or no* — and 42 of 55 answer yes.

The other 13 answer no, and **all 13 are losses; none is below the floor**:

| policy | extra misses vs floor | relative |
|---|--:|--:|
| arc | +2,712,099 | +3.74% |
| 2q-ghost-hybrid-0.1 | +6,802,044 | +9.39% |
| 2q-hybrid-0.1 | +8,048,155 | +11.10% |
| lfu-hybrid | +9,652,753 | +13.32% |
| lfu | +10,982,806 | +15.15% |
| lfu-compact | +11,973,758 | +16.52% |
| mru | +12,064,610 | +16.65% |
| **mru-compact** | **+12,504,146** | **+17.25%** |

*(8 of 13 shown; the rest are 2Q hybrid variants between +10.7% and +13.1%.)*

**Cluster53 is the only trace here that discriminates on hit rate**: miss spans
0.011603 to 0.141573, a 12.20x range — 2.309x even excluding the two MRU cells.
Every hit-rate claim in the paper should rest on cluster53. Cluster13 should be
presented as what it is: a data-structure-cost microbenchmark.

## 2. On cluster13, "fastest" mostly means "kept less"

Nine of the ten fastest policies by GET mean are drawn from the thirteen that
miss *more* than the floor.

| rank | policy | GET ns | avg served GET | vs floor |
|--:|---|--:|--:|--:|
| 1 | mru-compact | 603 | 4,127 B | **+17.25% misses** |
| 2 | mru | 624 | 4,145 B | +16.65% |
| 3 | lfu-compact | 640 | 4,173 B | +16.52% |
| 4 | lfu | 652 | 4,206 B | +15.15% |
| 5 | lfu-compact-hybrid | 697 | 4,190 B | +15.82% |
| … | | | | |
| **10** | **2q-compact-0.25-0.5** | **867** | **4,656 B** | **at floor** |

Pearson(GET mean, avg served GET size) = **+0.585**; Pearson(GET mean, miss) =
**−0.584**. Those are the same fact. **A policy lowers its GET mean by throwing
away objects that would have been re-referenced, so its surviving hits are
smaller and hotter.** `mru-compact` serves 66.1M hits of 4,127 B where
`lru-compact` serves 78.6M of 4,656 B — 12.5M fewer hits, 11.4% smaller.

Size-normalised (ns per KiB served) MRU is still ahead — 149.6 vs 193.3 — because
its hits are the most-recently-inserted objects and are trivially CPU-cache-hot.
**Neither residual is evidence that MRU's data structure is faster.** Any
sentence of the form "MRU/LFU is the fastest policy on cluster13" is a hit-rate
failure reported as a latency win.

**The fair cluster13 comparison is inside the 42-cell floor group**, which serves
a byte-identical mix (4,656 B GET, 5,606 B SET) with hit counts within 4,492 of
each other. There, **all 12 flat cells occupy GET ranks 1–12, ahead of all 30
hybrids.** Fastest `2q-compact-0.25-0.5` 867 ns; slowest `s3-fifo-hybrid-0.1`
1,761 ns.

On cluster53 the separation is even cleaner and by design rather than by size:
**all 17 flat cells occupy ranks 1–17**; the fastest hybrid (3,453 ns) is slower
than the slowest flat (3,413 ns). Perfect separation.

## 3. Compaction: a consistent GET win, a confounded capacity win

Deltas are compact vs original, same policy, same trace.

- **GET mean improved in 15 of 16 pairs.** Sole exception `sieve` on cluster13
  (+1.209%). Exact two-sided sign test **p = 0.000519**. Median **−1.462%**,
  mean −1.234%.
- **Resident objects rose in 15 of 16**, median **+0.423%**.
- **SET mean improved in 14 of 16** — but see the noise floor in §6; every SET
  comparison here is below it.

Three qualifications the paper must carry:

**The miss-ratio sign test is meaningless as stated.** Five of the eight
cluster13 pairs differ by ≤4.2e−6 relative, and `clock` is an *exact tie* —
identical GET and SET counts (78,601,704 / 72,473,368), so compaction changed
nothing about which objects were retained, only that 9,212 more of them fit.
That cell isolates the slab-layout effect from the policy effect perfectly.
Restricted to the 11 pairs with a resolvable miss difference: 8 improve (all
cluster53) and **3 worsen**. Compaction's hit-rate effect is a cluster53
phenomenon; on cluster13 it is nil or slightly negative.

**The design is order-confounded.** All 16 compact cells ran *after* their
original, by 1.17–2.34 h. On cluster13 there is no drift — pearson(run order,
GET mean) = **+0.038**. On cluster53 there is: **−0.641**, slope −4.05 ns/run,
≈ −1.9% across the block, the same magnitude and sign as the claimed win and not
separable from it. **Interleave the arms before quoting the cluster53 compaction
figure.**

**Two cluster13 cells never filled the cache** — both
`2q-full-fast-admission-*-hybrid-0.25-0.5`, at 75.9% and 76.4% of 12e9 while all
108 others reach 100.00%. Capacity is not binding, so `-compact` cannot buy
objects there; the pair is identical on 24 fields including objects, promotions
and demotions. Exclude or footnote both in any equal-capacity comparison.

## 4. The tiering tax

Ratio of hybrid GET mean to flat GET mean. **Strict** = the 6 pairs where the
hybrid genuinely is the same algorithm tiered (lru, lru-compact, lfu,
lfu-compact, fifo, fifo-compact).

| | cluster13 | cluster53 |
|---|--:|--:|
| best | 1.089x (lfu-compact) | 1.046x (lfu) |
| **median** | **1.606x** | **1.099x** |
| worst | 1.619x (fifo-compact) | 1.382x (fifo-compact) |
| hybrids faster than their flat base (broad, n=38) | 2 / 38 | 0 / 38 |

cluster13 is sharply bimodal — 1.089, 1.167 for the LFU pair against
1.603–1.619 for everything else. The mechanism is promotion starvation (§5).

**There is no cell in this sweep where tiering makes GET faster at equal hit
rate.** The two sub-1.0 entries are both `2q-fast-admission-hybrid-0.1` on
cluster13, and both are the §2 artifact: they beat flat 2Q only by missing
12–13% more and serving 4,276 B hits instead of 4,656 B.

**2Q and S3-FIFO must not be compared flat-to-hybrid at all.** Flat `2q` is
faithful three-queue 2Q while `2q-hybrid` is Simplified 2Q; flat `s3-fifo` has a
0..3 counter and a ghost queue while `s3-fifo-hybrid` has a reference bit and no
ghost. They are siblings, not tiered versions — which is what the faithful
S3-FIFO family (paper-cache `d501ca7`) exists to fix.

## 5. Promotion starvation is the mechanism behind cluster13's tax

**26 of 76 hybrid cells complete fewer than 10,000 promotions — 24 of them on
cluster13.** `fifo-hybrid` and `fifo-compact-hybrid` complete **exactly zero** on
both clusters, which is structural: FIFO has no reference signal to promote on.

The cross-cluster ratios are extreme: `lru-hybrid` completes **732** promotions
on cluster13 against **4,499,999** on cluster53; `2q-fast-admission-hybrid-0.1`
93 against 5,126,226. Twenty-two policies show ≥1000x swings.

A c13 hybrid that completes 732 promotions serves nearly every hit from the slow
tier — hence the 1.6x tax.

**Caveat on the counter itself:** `lru-hybrid/c13` shows `queue_depth_max=42874`
against 70.5M evictions of churn, so promotions are being *enqueued and dropped*.
`Promotions:` is a completed count throttled by queue capacity, **not a measure
of promotion demand.** Do not write "hybrid X promotes N times" without it.

## 6. The noise floor, from an accidental repeat

`index.csv` contains exactly one duplicate key: `s3-fifo-compact-0.1 / cluster53`
ran twice, 13.6 h apart, identical configuration — the first contaminated by a
concurrent build, the second a clean re-run. That accident is the sweep's only
direct repeatability estimate:

| statistic | run 1 | run 2 | Δ |
|---|--:|--:|--:|
| objects | 917,089 | 917,086 | **−0.0003%** |
| GET mean | 3,336 ns | 3,341 ns | **+0.150%** |
| **SET mean** | **7,472 ns** | **9,390 ns** | **+25.7%** |

**GET mean and object counts are repeatable to well under 0.2%**, which puts the
~1.5% compaction effect and the 8–62% tiering tax comfortably above noise.

**SET mean is not repeatable at the 25% level.** That invalidates every SET-mean
comparison in this sweep smaller than ~26%, including all 16 compaction SET
deltas. It is n=1 — a point estimate, not a variance — but it is consistent with
the ±2 pp warning in `eviction_stacks_in_cxl.md` §5.6.

Consumers of `index.csv` must keep the **last** row per key; the runner is
append-only and its resume path writes no row, so nothing de-duplicates.

## 7. Corrections to already-published documents

**7.1 `eviction_stacks_in_cxl.md` §6.4 is wrong.** It states `SET latency
samples:` is "absent from all eight cluster53 files". Presence is governed by SET
count against the 10M reservoir cap, not by cluster. Across this sweep:
**cluster13 55/55 present; cluster53 2/55** — `mru` and `mru-compact`, whose SET
counts (23.7M, 23.5M) exceed the cap. The claim was true of that document's own
8-file matrix and generalises wrongly. Those two cells' SET percentiles are
reservoir-sampled and need a footnote.

**7.2 `eviction_stacks_in_cxl.md` §6.1's intent-vs-completion bound is off by
831x.** That document reports 0.023% overstatement, measured on
`lfu-compact-hybrid/c13`. This sweep's same cell reproduces it (450 excess of
1,987,651, 0.023%) — but **the bound holds only for LFU.** MIGSTATS intents
exceed TIER completions in 10 of 76 hybrid cells, worst at **+19.12%**
(`lru-lfu-hybrid-2/c13`, 72,002,175 intents against 60,445,968 completions). All
large discrepancies are demotions; promotion intents equal completions in every
big case. Every figure in this document uses TIER-block completions.

**7.3 The four `lru-sized-*` cells print the wrong fast-tier size — but the runs
are valid.** They report `Fast tier size: 2.00 GiB` where the other 72 report
4 GiB. The TIER block's *realized* fast-tier DRAM contradicts the printed line:
those four land at 95.9–96.6% of 4 GiB, squarely inside the other 72 (median
97.2%), and no cell in the sweep exceeds 4 GiB. `FAST_TIER_GB=4.0` reached every
binary; the `-sized` variant prints the wrong constant.

That matters because the cells are otherwise the sweep's most interesting hybrid
result: at the same ~4 GiB, `lru-sized-hybrid/c13` fits **1,187,214** fast-tier
objects averaging 3,133 B where `lru-hybrid/c13` fits 545,522 averaging 6,789 B —
**2.18x more objects of half the size** — and on cluster53
`lru-sized-compact-hybrid` and `lru-sized-hybrid` rank **#1 and #2 of all 55
policies by miss ratio** (0.011603, 0.011673). Footnote the print as a reporting
bug; keep the cells.

## 8. Provenance

`sweep/index.csv` summarises the run; `results/sweep_110_cells.csv` is the
reconciled dataset used here. Every field was extracted twice by independent
paths and diffed: **1,744 field comparisons, 17 disagreements**, all 17 resolved
against the source files in favour of the raw parse (15 were wall-clock
truncated to 6 significant digits, max error 5 ms; 2 were miss ratios off by
1 ULP). A third independent parse found **0 mismatches** against the reconciled
result. Both extractions independently avoided all four pitfalls recorded in
`eviction_stacks_in_cxl.md` §6.

Throughput, where quoted, is `records / wall_s` — never the harness's
`GETs/sec`, which is `1e9 / mean latency`. It spans 140,937 ops/s
(`s3-fifo-compact-hybrid-0.1/c53`) to 330,833 ops/s (`mru-compact/c13`).

Reproduce with `/home/griff/cv2/sweep/sweep_full.sh` (resumable; skips completed
cells). `arc` is the only policy with neither a compact nor a hybrid
counterpart, so it appears in no paired comparison.
