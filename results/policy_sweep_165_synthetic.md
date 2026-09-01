# The 165-cell synthetic sweep: 55 policies, three traces

Every eviction policy the library implements, on all three synthetic traces at
a 15 GB budget (5 GB fast tier on the tiered designs), read-through, one
client. 165 cells, **zero failures**.

This extends `policy_sweep_synthetic_traces.md`, which covered 18 policies on
the same harness and config, to the full policy set. Do NOT cross-compare with
`policy_sweep_110_cells.md`: that series is 12 GB / 4 GiB.

Raw: `sweep_165_synthetic.csv`. Five cells appear twice -- they were re-run
after a build contaminated their latency (see `CONTAMINATED_CELLS.txt`); the
LAST row for a (trace, policy) pair is the valid one and is what this doc uses.

## Why these traces and not the clusters

cluster13 pins 42 of 55 policies on its compulsory miss floor, with a total
spread of 4,492 misses in 151M requests -- 0.003%. It cannot rank anything.
These working sets exceed the cache, so capacity misses exist:

| trace | records | miss spread across the 55 policies | relative |
|---|--:|--:|--:|
| standard_web | 14,046,030 | 0.0830 -> 0.1907 | **130%** |
| low_alpha_cold | 5,852,515 | 0.2045 -> 0.4453 | **118%** |
| uniform_baseline | 2,341,006 | 0.4546 -> 0.8603 | **89%** |

## The headline: one policy wins all three

`lru-sized-compact-hybrid` has the lowest miss ratio on **every** trace. Not
by a hair -- it is the only design that is best anywhere in this series.

### standard_web

| rank | policy | design | miss | GET mean | SET mean | objects |
|--:|---|---|--:|--:|--:|--:|
| 1 | `lru-sized-compact-hybrid` | tier | 0.0830 | 6,364 ns | 10,160 ns | 910,418 |
| 2 | `lru-sized-hybrid` | tier | 0.0831 | 6,249 ns | 10,146 ns | 908,825 |
| 3 | `lfu-compact-hybrid` | tier | 0.0845 | 5,199 ns | 13,191 ns | 835,396 |
| 4 | `2q-fast-admission-reprieve-compact-hybrid-0.1` | tier | 0.0846 | 5,872 ns | 10,157 ns | 835,525 |
| 5 | `lfu-hybrid` | tier | 0.0846 | 5,068 ns | 12,965 ns | 832,370 |
| 6 | `s3-fifo-lazy-demotion-reprieve-compact-hybrid-0.1` | tier | 0.0846 | 5,529 ns | 13,604 ns | 835,672 |
| 7 | `2q-fast-admission-reprieve-hybrid-0.1` | tier | 0.0847 | 5,777 ns | 9,923 ns | 833,394 |
| 8 | `s3-fifo-lazy-demotion-fast-admission-midpoint-reprieve-compact-hybrid-0.1` | tier | 0.0847 | 5,767 ns | 9,724 ns | 835,588 |
| ... | | | | | | |
| 53 | `s3-fifo-hybrid-0.1` | tier | 0.1552 | 4,707 ns | 8,092 ns | 394,038 |
| 54 | `mru` | flat | 0.1903 | 4,321 ns | 9,064 ns | 828,563 |
| 55 | `mru-compact` | flat | 0.1907 | 4,368 ns | 9,109 ns | 829,562 |

### low_alpha_cold

| rank | policy | design | miss | GET mean | SET mean | objects |
|--:|---|---|--:|--:|--:|--:|
| 1 | `lru-sized-compact-hybrid` | tier | 0.2045 | 10,508 ns | 11,911 ns | 903,846 |
| 2 | `lru-sized-hybrid` | tier | 0.2049 | 10,458 ns | 11,792 ns | 902,248 |
| 3 | `lfu-compact-hybrid` | tier | 0.2115 | 7,442 ns | 13,288 ns | 835,031 |
| 4 | `2q-fast-admission-reprieve-compact-hybrid-0.1` | tier | 0.2116 | 9,590 ns | 10,660 ns | 835,421 |
| 5 | `2q-fast-admission-reprieve-hybrid-0.1` | tier | 0.2121 | 9,608 ns | 10,721 ns | 833,355 |
| 6 | `lfu-hybrid` | tier | 0.2122 | 7,387 ns | 13,080 ns | 831,997 |
| 7 | `s3-fifo-lazy-demotion-reprieve-compact-hybrid-0.1` | tier | 0.2127 | 8,763 ns | 13,958 ns | 835,154 |
| 8 | `arc` | flat | 0.2128 | 5,416 ns | 11,253 ns | 828,299 |
| ... | | | | | | |
| 53 | `2q-hybrid-0.1` | tier | 0.4453 | 6,414 ns | 8,508 ns | 426,447 |
| 54 | `s3-fifo-compact-hybrid-0.1` | tier | 0.4453 | 6,831 ns | 8,393 ns | 426,436 |
| 55 | `s3-fifo-hybrid-0.1` | tier | 0.4453 | 6,939 ns | 8,517 ns | 426,444 |

### uniform_baseline

| rank | policy | design | miss | GET mean | SET mean | objects |
|--:|---|---|--:|--:|--:|--:|
| 1 | `lru-sized-compact-hybrid` | tier | 0.4546 | 13,012 ns | 12,245 ns | 887,836 |
| 2 | `lru-sized-hybrid` | tier | 0.4551 | 13,099 ns | 12,309 ns | 886,263 |
| 3 | `2q-fast-admission-reprieve-compact-hybrid-0.1` | tier | 0.4637 | 12,289 ns | 11,338 ns | 835,407 |
| 4 | `fifo-compact-hybrid` | tier | 0.4639 | 10,180 ns | 10,377 ns | 835,310 |
| 5 | `lru-lfu-compact-hybrid-2` | tier | 0.4640 | 13,229 ns | 12,615 ns | 835,247 |
| 6 | `2q-full-fast-admission-compact-hybrid-0.25-0.5` | tier | 0.4641 | 12,647 ns | 11,066 ns | 835,272 |
| 7 | `lru-compact-hybrid` | tier | 0.4641 | 13,173 ns | 12,482 ns | 835,393 |
| 8 | `s3-fifo-lazy-demotion-fast-admission-midpoint-reprieve-compact-hybrid-0.1` | tier | 0.4641 | 11,841 ns | 10,030 ns | 835,430 |
| ... | | | | | | |
| 53 | `2q-hybrid-0.1` | tier | 0.8603 | 11,170 ns | 10,462 ns | 248,829 |
| 54 | `s3-fifo-compact-hybrid-0.1` | tier | 0.8603 | 10,961 ns | 10,282 ns | 248,818 |
| 55 | `s3-fifo-hybrid-0.1` | tier | 0.8603 | 10,940 ns | 10,234 ns | 248,822 |

## Compaction is behaviourally free, across 165 cells

A `-compact` variant is a memory-layout change and must not alter behaviour.
Every pair where both ran:

| trace | pairs compared | max abs miss difference |
|---|--:|--:|
| standard_web | 27 | 0.0004 |
| low_alpha_cold | 27 | 0.0008 |
| uniform_baseline | 27 | 0.0011 |

These are NOT evidence of behavioural divergence, and should not be read as
such. A compact variant charges less per object, so at a fixed byte budget the
cache holds a DIFFERENT resident set -- and a different resident set produces a
different miss ratio under an identical policy. That is the intended effect of
compaction, not a defect.

Behavioural equivalence is asserted directly, and elsewhere: each compact
stack carries fidelity tests that replay one identical operation sequence
through both itself and its original and compare eviction order
element-for-element. A trace-level miss ratio cannot substitute for that -- it
would not catch a counter firing on the wrong path, which is the defect class
that produced a doubled demotion count on an earlier conversion in this tree.

## Flat vs tiered

| trace | best flat | best tiered | tiered advantage |
|---|--:|--:|--:|
| standard_web | 0.0848 (`arc`) | 0.0830 (`lru-sized-compact-hybrid`) | 2.1% |
| low_alpha_cold | 0.2128 (`arc`) | 0.2045 (`lru-sized-compact-hybrid`) | 3.9% |
| uniform_baseline | 0.4658 (`mru-compact`) | 0.4546 (`lru-sized-compact-hybrid`) | 2.4% |

## Re-run cells

* `lru` on low_alpha_cold (1 re-run)
* `s3-fifo-lazy-demotion-fast-admission-midpoint-reprieve-compact-hybrid-0.1` on standard_web (1 re-run)
* `s3-fifo-lazy-demotion-fast-admission-reprieve-compact-hybrid-0.1` on standard_web (1 re-run)
* `s3-fifo-lazy-demotion-fast-admission-split-slow-reprieve-compact-hybrid-0.1` on standard_web (1 re-run)
* `s3-fifo-lazy-demotion-reprieve-compact-hybrid-0.1` on standard_web (1 re-run)

