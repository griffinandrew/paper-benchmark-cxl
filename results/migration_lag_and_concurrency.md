# Migration lag: why concurrency breaks tiered measurement

The investigation started as "why is the merged object store slower under
multiple clients". The answer to that turned out to be small. What the
investigation found instead applies to **both** designs and to every tiered
number in this results series taken above one client.

Config throughout: `uniform_baseline` and `standard_web`, 15 GB cache / 5 GB
fast tier, `lru-compact-hybrid`, read-through. Hardware: node 0 = 80 GB DRAM
with 8 CPUs, node 1 = 122 GB Optane, no CPUs, distance 20.

## 1. Five hypotheses that were wrong

Each was measured, not argued, and each failed. Recording them because the
failures are what narrowed the answer.

| hypothesis | test | result |
|---|---|---|
| the policy worker is saturated | per-thread CPU | worker at **34%** of wall. Not close |
| the worker is bottlenecked on relinks | `MERGED_UPDATE_INTERVAL=4e9`, skipping every relink | 40s -> 39s. **No change** |
| shard lock contention, too few shards | 32 -> 128 -> 256 shards | on the saturated trace, **slightly worse** |
| the `tail_seq` scan is cache-hostile | packed the array (4 lines vs 32) | 16,620 -> 16,842 ns. **No change** |
| bursts differ, hence queue depth differs | mean batch size | identical: 3.1/3.1, 2.6/2.6, 2.8/2.8 |

Per-thread CPU over a 39 s `-c 5` run, 23 threads, is the one that reframed
everything:

```
mig-0            26.5 s   68% of wall     <- busiest
mig-1            26.2 s   67%             <- busiest
policy worker    13.4 s   34%
5 x client      ~ 9.5 s   24% each
TOTAL           132.1 s   parallelism 3.39x
```

Nothing is saturated. The two **migration** threads are the hottest things in
the system, and they are the component nobody had been looking at.

## 2. The real ceiling: migration volume

```
demo_tot  1,544,941      2.28M migrations on a 2.34M-record trace
promo_tot   738,296      = 0.975 migrations PER REQUEST
```

Nearly one tier migration per request. The split design measures 0.965 -- so
this is a property of the tier configuration, not of either store design.

Sanity-checked structurally rather than taken on trust:

| | expected | measured |
|---|--:|--:|
| promotions (hits on slow objects; slow is 64% of resident) | ~801k | 738k |
| demotions (1.09M inserts + 738k promotions, each displacing one) | ~1.8M | 1.54M |

At 16,515 B per object that is **37.7 GB copied in 39 s**, DRAM <-> Optane.
This EXCEEDS the 16.71 GB working set, which is not a contradiction: WSS counts
each object once, migration traffic counts each move, and 2.28M migrations over
1.01M distinct objects is **2.25 moves per object**. 2.25 x 16.71 GB = 37.6 GB.

## 3. The phase transition

Queue depth against client count, `uniform_baseline`, migrations constant:

| clients | split | merged | merged/split |
|--:|--:|--:|--:|
| 1 | 8,930 | 2,039 | 0.23x |
| 2 | 481,011 | 173,644 | 0.36x |
| 4 | 1,231,770 | 937,917 | 0.76x |
| 8 | 1,564,192 | **1,578,614** | 1.01x |

At one client the consumers keep up: split's 8,930 is almost exactly one burst
(9,022 measured directly via `BURST_MAX`). **Adding a second client jumps it
54x.** By eight clients the backlog is 1.58M pending moves against 831k
resident objects.

It is a cliff, not a gradient, and where the cliff sits depends on the
workload:

| trace | migrations/request | depth c1 | depth c5 | saturated? |
|---|--:|--:|--:|---|
| standard_web | 0.232 | 8,908 | 9,502 | never -- depth stays at one burst |
| low_alpha_cold | 0.691 | -- | 1,845,926 | yes |
| uniform_baseline | 0.975 | 8,930 | 1,477,047 | by c2 |

`BURST_MAX` separates the two regimes directly:

```
split,  standard_web   depth 9,986      burst 9,006    depth IS one burst
merged, standard_web   depth 2,114      burst   315
split,  uniform        depth 1,473,643  burst 9,022    depth is 163x a burst
merged, uniform        depth 1,302,617  burst   837
```

The ~9,000 burst matches what one global `settle_fast_tier` should emit: 3% of
a 5 GB tier at 16.5 KB objects. The merged store settles 32 shards
independently, so each burst is roughly a thirty-second of that -- which is why
its queue is shallower wherever the consumers can keep up at all, and why that
advantage decays to nothing (1.01x) once both are throughput-bound.

## 4. The consequence for measurement

`fast_bytes_used` and `fast_object_count` are updated by `settle_fast_tier` at
DECISION time. They have to be: the settle loop reads `fast_used` to decide
whether to keep demoting, so completion-time accounting would make it demote
the entire fast tier in one pass, never seeing its own decisions register.

So those gauges report **intent**. Physical placement trails them by the queue
depth:

| clients | objects mis-placed vs belief | as % of resident |
|--:|--:|--:|
| 1 | ~2,000 | 0.2% |
| 8 | ~1,578,000 | **190%** |

**Every tiered measurement in this series above one client describes what the
policy intended, not where the bytes are.** That applies to the split design
identically; it is not a merged-store artifact. Only `standard_web` (depth
2,114 at c5) is close enough to treat intent and placement as the same thing.

## 5. The consequence for the DRAM bound

The migration queue is `unbounded`. There is no backpressure. Pending
demotions are bytes the policy believes are in Optane that are physically still
in DRAM, and nothing limits how many there can be.

That makes `FAST_TIER_GB` a bound on intent rather than on DRAM, which defeats
its purpose -- the reason `with_shared_overhead` reserves metadata out of that
budget is to bound real DRAM.

Estimated gap, using the run's overall 68/32 demotion:promotion mix:

| clients | queue depth | net mis-placed | approx bytes | vs the 5 GB budget |
|--:|--:|--:|--:|--:|
| 1 | 7,340 | ~2,500 | ~41 MB | 0.8% |
| 5 | 1,304,465 | ~468,000 | ~7.7 GB | **154%** |

A NUMA check agrees on the order but not the number: node 0 peaked at
11,129 MB at `-c 1` and 14,637 MB at `-c 5`, a +3.5 GB delta on identical data
and identical migration totals.

**Both figures are weak and should be quoted as "gigabytes", not as a number.**
jemalloc `retained` was ~13 GB in these runs, so node occupancy is dominated by
retained arenas rather than live objects -- only the delta between two
otherwise-identical runs means anything. And the 7.7 GB assumes the pending
queue has the same demotion:promotion mix as the run overall, which cannot be
checked from existing output: by end of run the queue has drained, so
`demo_tot` and the completed `Demotions:` agree and carry no information about
the peak.

The exact instrument is two atomics incremented on push and decremented on
completion, split by tier. Not yet built.

## 6. What this says about merged vs split

Small, and secondary to the above.

More migration threads settles the mechanism. On merged `uniform_baseline`
at `-c 5`:

| threads | queue depth | GET mean |
|--:|--:|--:|
| 2 | 1,337,445 | 16,194 |
| 4 | **302,179** | **30,178** |
| 8 | 266,172 | 26,288 |

The queue drains 4.4x better -- so the consumers were never bandwidth-bound --
but GET latency rises **86%**. Migration threads and clients contend for the
same locks, and giving migrations more workers just lets them win more of that
fight.

That identifies the merged store's cost precisely. The split design has **two
lock domains**: its eviction stack is touched only by the policy worker and so
takes no lock at all, while the object map is shared with clients and migration
threads. Merging them removes a lock domain that the split design got for free
-- every `apply_migration` takes two shard acquisitions (2.28M x 2 = 4.6M,
against the worker's 2.3M), on the same locks the clients need.

Hence the rule that survives every measurement here:

> **Merged wins while `depth ~= burst`. It loses once `depth >> burst`.**

There is **no throughput regression** -- wall clock is identical at every
client count from 2 upward (52/43/36 s vs 53/43/36 s) and merged is faster at
one client (63 s vs 66 s). The difference is per-op latency, which lock
blocking inflates without reducing aggregate throughput: a client waiting on a
shard lock is counted as in-flight but yields the CPU, so the others proceed.

## Reproducing

Scripts on claude-g: `/tmp/bottleneck.sh` (per-thread CPU, relink-skip probe,
client scaling), `/tmp/burstrun.sh` (`BURST_MAX`, migration-thread scaling),
`/tmp/numaprobe.sh` (node occupancy), `/tmp/thsample2.py` (per-thread CPU
sampler -- note it must accumulate per-tid maxima, since a final-snapshot
sampler sees only the threads that outlive the run).
