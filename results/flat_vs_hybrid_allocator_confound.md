# Flat versus hybrid: an allocator confound, and what is left after it

Why the all-DRAM baseline measured 18% slower than the tiered designs, why that
result inverts once three artifacts are removed, and what the honest
flat-versus-hybrid comparison looks like.

Every number here is **measured** on Twitter cluster13 (and cluster53 as the
control), 12 GB cache, one client, read-through, 30M-record prefix unless a row
says otherwise, latency percentiles from a uniform Algorithm-R reservoir of
1M samples. Where a figure comes from the in-library step profiler it is
labelled as such; those are inflated by a constant `Instant::now` cost per step,
equally in both builds, so only their contrast is meaningful.

## The result

| comparison | GET mean | verdict |
|---|---|---|
| as originally measured (151M records) | tiered 865 vs flat 1056 ns | tiered wins by 18% |
| after removing three artifacts | **flat 806 vs tiered 1173 ns** | **flat wins by 31%** |

The original figure was not merely inflated. It was **inverted**: the all-DRAM
cache is faster, and after the remedies in this document it wins at every
percentile of both GET and SET.

Three separate artifacts stacked in the same direction:

1. **Allocator stream mixing** (~150-250 ns/GET) -- the flat build's long-lived
   value buffers shared jemalloc size classes, bins and the client thread's
   default tcache with the transient per-GET `to_vec()` destination. The hybrid
   builds escaped this by accident, because `TieredBuffer::Slow` allocates
   through a different allocator entirely.
2. **The per-GET allocation itself** (118 ns flat, 58 ns tiered) -- `get()`
   returns an owned `Vec`, so `malloc` sits inside the timed region.
3. **A benchmark key-string round trip** (~150 ns, flat only) -- the harness
   converted the trace's u64 key to a heap `String` and parsed it back inside
   the timed bracket, and that string's size class collided with the flat
   build's `Arc` control blocks.

None of the three is a property of tiering.

## Why the original statistic was the wrong one

cluster13's per-access value size is **p50 123 B against a mean of 5,606 B** --
a 45x skew (`twitter_trace_working_sets.md`). At the median a GET copies two
cache lines and latency is pure fixed overhead; above p90 the payload is
15-137 KB and latency is memory bandwidth. A mean blends the two regimes, and
the original comparison lived entirely in the first one:

| GET, 151M records | p50 | p75 | p90 | p99 | p999 | mean |
|---|---|---|---|---|---|---|
| flat | 617 | 916 | 1659 | 8356 | 26587 | 1056 |
| tiered | **322** | 665 | 1690 | 8269 | 26774 | 865 |

From p90 outward the two are identical within noise. The entire difference was
at the median, where no bytes move. The result also **does not replicate**: at
30M records the mean flips sign (flat 1221 vs tiered 1226-1294).

Report latency stratified by payload size, or at minimum as percentiles. The
benchmark now records a size reservoir alongside the latency reservoir so the
two can be read together.

## What was ruled out, and by what

Each hypothesis was killed by a measurement chosen to discriminate against
whatever survived the previous one.

| hypothesis | killed by |
|---|---|
| correctness artifact | work conservation exact; payloads full size; hit-size distributions identical at every percentile |
| different code path / map / object layout | same `DashMap`, same `Object` (`data: Arc<V>` is one pointer), one shared adapter; only compiled difference is an enum discriminant match, which penalizes *tiered* |
| memory bandwidth, NUMA placement, TLB reach, LLC pollution | `FAST_TIER_GB` swept 11->1 GB moved node-1 residency 13x and p50 by **+1.9 ns/GiB** -- wrong sign, 14x too small. At FAST=1, 96.5% of data on CXL, p50 still 361 vs 610 |
| transparent huge pages, prefaulting | `AnonHugePages: 0` system-wide; `prefault_fast_tier` disabled |
| sampling bias | textbook Algorithm R, uniform over the run |
| worker eviction pressure | eviction totals near-identical; tiered does 5M *extra* migrations |
| the custom NUMA extent hooks | stock tikv-jemalloc as global allocator: everything moved ~2%, gap preserved exactly. Hooks are extent-granularity -- 63 bin allocations per extent operation |
| arena lock contention | 32 arenas changed nothing; total bin-lock wait 2 ms per run. Arenas partition by *thread*; there is one client thread |
| hit-size composition | measured identical: p50 123 B, p90 ~13.8 KB, p99 ~70.9 KB in all configs |
| promotion re-packing / hot-set compaction | step profiler: per-step medians identical (lookup 80 vs 81 ns) where it predicted a 100-180 ns delta |
| bimodal mode-edge, stall concentration | joint profiler: identical per-op totals *and* identical stall co-occurrence (P(any) 0.160 vs 0.163) |

## The mechanism

`set()` stores a buffer of exactly `value.len()` bytes. `get()` returns a fresh
buffer of exactly `value.len()` bytes. **The two streams are the same size by
construction**, so they land in the same size class, the same bins, and the
same per-thread default tcache. One is long-lived (freed by the policy worker
at eviction -- a cross-thread free); the other is churned once per GET.

Mixing them fragments the bins the GET path draws from. Measured from
jemalloc's own statistics at 10M records:

| 32 B size class | flat | tiered |
|---|---|---|
| current slabs | **342** | 6 |
| live regions | 5 | 5 |
| slab reuse (`nreslabs`) | 3,161,412 | 687,815 |
| lock spin-acquires | 4,012 | 713 |
| total lock wait | **2.0 ms** | **0** |

Flat holds 342 slabs to store five live objects. Every tcache refill therefore
hands the GET destination a region on a cold, scattered page.

The hybrid builds never had the problem: `LfuHybrid`'s admission latch routes
new keys to `TieredBuffer::new_slow` -> the node-1 allocator with
`MALLOCX_TCACHE_NONE`, so the client's default tcache stays a hot closed loop.
The effect even ranks the policies by how separated their value stream is:

| build | value allocation flow | GET p50 |
|---|---|---|
| flat | node-0, default tcache, freed at eviction | 612-698 |
| LRU hybrid | admits node-0 default tcache, demotes ~67% away early | 508 |
| LFU hybrid | latches to node-1 `TCACHE_NONE` for every new key | **353** |

**This is why cross-policy comparisons are confounded too.** LFU-hybrid beats
LRU-hybrid here largely because its admission latch separates the value stream,
not because LFU tiering is a better design.

Three independent knobs confirm the mechanism. Disabling the tcache collapsed
the gap and *inverted* the LRU pair to flat-wins (462 vs 478). Arena count and
allocator implementation changed nothing. And segregating the value pool
flipped the tcache from harmful to helpful:

| flat, LFU | tcache on | tcache off | effect of the cache |
|---|---|---|---|
| unsegregated | 614 | **467** | hurting by 147 ns |
| segregated | **523** | 573 | helping by 50 ns |
| tiered (reference) | 353 | 366 | helping by 13 ns |

## The read paths are identical -- including from CXL

A sampled in-library profiler (`GETINTO_PROFILE=1`, 1 GET in 64, timestamps
around hash / DashMap lookup + Arc clone / copy / event send) settled whether
any design difference survives once allocation is out of the path. Small
(<=256 B) hits, profiler figures:

| | hash | lookup+Arc | copy | send | per-op total p50 | P(any stall) |
|---|---|---|---|---|---|---|
| flat (DRAM hits) | 45 | 80 | 67 | 79 | **276** | 0.160 |
| tiered (93% CXL hits) | 43 | 81 | 72 | 70 | **279** | 0.163 |

Two things worth carrying into the paper.

**The designs are equal on the read path** -- not similar, statistically
indistinguishable, at every percentile and in joint stall structure.

**Hot small objects are placement-free.** Tiered served 93% of these hits from
CXL at the same median cost flat needed from DRAM. For cache-hot small objects
the two cache lines are in SRAM regardless of which memory backs them; CXL's
cost appears only where bytes move (copy p90 1214 vs 818 ns, p99 10.0 vs
5.1 us). The slow tier is far cheaper for a hot-skewed small-object population
than raw device latency suggests -- a positive result for tiering, and the one
this investigation actually supports.

## The key-string artifact

With allocation removed and the library proven symmetric, ~200 ns still
separated the *benchmark-reported* medians. A wrapper bisection
(`WRAPPER_PROFILE=1`) split the bracketed time three ways:

| span | flat | tiered |
|---|---|---|
| `key.parse::<u64>()` | **236** | **90** |
| library call | 598 | 590 |
| remainder | 346 | 347 |

The whole residual was the parse -- not its arithmetic, but the memory read of
a per-record key `String` the harness created from a u64 the trace already
contained. A ~20-byte key string lands in the 32 B size class, the same class
as the flat build's `Arc<Box<[u8]>>` control blocks (32 B); those long-lived
blocks capture the freed key strings, draining the recycling pool so every
timed parse reads a cold page. The tiered build's `Arc<TieredBuffer>` is 40 B,
lands in the 48 B class, and never collides.

Both falsification tests passed: on cluster53 (miss ratio ~1%, so almost no
sets) flat's parse fell to **99 ns**; segregating value buffers while leaving
the `Arc` blocks alone left it at **219 ns**.

Fixed by passing the trace's u64 keys through end to end. Parse span is now
37 ns (flat) and 32 ns (tiered), and ~60M allocator operations per run
disappear with it.

## Final numbers

cluster13, 30M records, LFU, u64 keys throughout.

| variant | GET p50 | p75 | p90 | p99 | GET mean | SET p50 | SET mean |
|---|---|---|---|---|---|---|---|
| flat, pure read (`get_into`) | 218 | 532 | 1278 | **5738** | **622** | 855 | 3006 |
| tiered, pure read (`get_into`) | 224 | 591 | 1752 | 11215 | 902 | 1056 | 4150 |
| flat, `get()` | 395 | 765 | 1722 | 9848 | 987 | 853 | **2848** |
| tiered, `get()` | 269 | 812 | 2378 | 14592 | 1173 | 1054 | 4071 |
| **flat + segregated pool, `get()`** | **238** | **695** | **1776** | **8058** | **806** | **795** | 3501 |

The pure-read medians **converge** (218 vs 224 ns), which is the expected result
given the profiler showed the read paths identical. What remains is the honest
cost of tiering: flat wins the mean by 31% and p99 by ~2x, entirely on
large-object reads where CXL bandwidth is the binding constraint.

In the user-facing comparison the remedied all-DRAM build wins every percentile
of both operations.

## How to run a fair comparison

**Separate the two questions.** *"Does tiering help?"* -> equal-hygiene cells
with `USE_GET_INTO=1`, so no allocator sits in the measured region. *"What does
a user experience?"* -> end-to-end `get()` with `segregated_value_arena`
enabled. Report both, and label the difference between them as allocator
behaviour, because that is what it is.

**Enable `segregated_value_arena` for every baseline.** It is not benchmark
hygiene only -- on the flat build it is worth -18% GET mean, -21% p99, and a
better SET p50 (SET mean regresses; the trade is documented in `Cargo.toml`).

**Equalize before comparing policies, not just tiers.** The admission-latch
effect above means any policy-versus-policy table measures admission plumbing
unless both sides share an allocation flow.

**Do not put a network transport inside the measured path.** A TCP round trip
is 10-50 us; the effects here are 100-300 ns. For a TPP baseline the cache does
need to be its own process -- TPP tiers whole processes, and an in-process
harness would let the kernel tier the benchmark's own buffers -- but pair that
isolation with the cache timing itself internally, not with a socket in the
hot path.

**Treat the all-DRAM baseline as a first-class target.** The hybrids received
generations of optimization while the baseline stayed frozen, and that
staleness alone manufactured an 18% win for the proposed system.

## Reproduction

Feature sets were recovered from cargo fingerprints rather than reconstructed
from memory: flat is `all_dram`; tiered is `hybrid_cache_common` +
`key_value_pmem` + 21 hybrid designs; both built with
`RUSTFLAGS=-C link-arg=-l:libjemalloc.so.2` on nightly.

Traces are truncated to exactly the record count passed to `--trace-records`
(25 B per record); passing a smaller `--trace-records` than the stream length
leaves kwik's progress bar in a stopped state and panics *before* statistics
print.

Instruments, all off by default:

| switch | what it does |
|---|---|
| `segregated_value_arena` (feature) | the remedy: value buffers get their own arenas and tcache |
| `stock_jemalloc` (feature) | swaps the NUMA arena allocator for plain tikv-jemalloc |
| `USE_GET_INTO=1` | drives the client through `get_into`, removing per-GET allocation |
| `GETINTO_PROFILE=1` | in-library per-step timing, percentiles, tier split, stall co-occurrence |
| `WRAPPER_PROFILE=1` | splits the benchmark's bracketed time into parse / library / remainder |
| `_RJEM_MALLOC_CONF=...` | jemalloc's own knobs (`tcache:false`, `stats_print:true`, decay) |
| `NUMA_ARENAS_PER_NODE`, `PAPER_NUMA_SLOW_TCACHE` | existing allocator knobs used to eliminate contention and TCACHE_NONE hypotheses |

Remedies and instruments live on branch `allocator-fairness-remedies` in both
repositories, split into separate commits so the keepers can be taken without
the diagnostics.
