# Filled read-through traces: how they are built, and what they contain

The traces every previous result was measured on had **dropped** the zero-size
gets. Their record counts are exactly kia's non-zero-size GET counts:

| eval_traces file | records | equals |
|---|--:|---|
| `cluster13.bin` | 151,075,072 | kia's non-zero-size GETs |
| `cluster19.bin` | 888,230,858 | kia's non-zero-size GETs |

A get returning size 0 was a **miss in the source system**. It is a real
lookup that drives the miss ratio and the reuse distances, and dropping it
removes 50.7% of cluster13's references and 40.9% of cluster19's. The filled
traces keep those references and give them a usable size and TTL instead.

They live in `/home/griff/eval_traces_filled` rather than replacing the
originals, so before/after stays comparable.

## How they are generated

`tools/trace_fill --maxsize`, two passes over kia's original 25-byte trace:

**Pass 1** builds a per-key table: the largest value size the key is ever seen
at, and a TTL from any record carrying one — in practice only SETs, because
**100% of GET records carry `ttl == 0`** in every cluster measured.

**Pass 2** rewrites the stream in order:

- **SET records are dropped.** A read-through cache does its own writes, only
  on a miss. `handle_read_through` already discards them (`if access.command
  != Command::Get { return Ok(()); }`), so this only saves I/O.
- **GET records are kept** — same timestamp, same key, same position.
  - `size == 0` → the key's largest observed size
  - `size > 0` → left exactly as recorded
  - TTL → always the key's TTL from the table, since a GET carries none
- A key that appears nowhere with a size or TTL falls back to the trace median
  (`--fallback median --fallback-ttl median`).

Size and TTL resolve under **separate rules**. A TTL of 0 means "this record
carries none", so a zero never overwrites a real TTL in either direction —
without that, a key whose largest size came from a GET would lose the TTL its
SETs had.

**Max vs first size** is the one judgment call, and it is immaterial: the WSS
computed from max sizes exceeds the first-size WSS by +0.000% on cluster26 and
cluster45, +0.002% on cluster13, +0.454% on cluster53.

## Definitions

Per `twitter_trace_working_sets.md`: a read-through cache writes an object
once, on the get that misses, so its resident size is the size at the key's
**first appearance**. The get:set the cache performs is `(1 - miss) : miss`,
and at infinite cache every distinct key is filled exactly once, so the floor
is `compulsory = distinct / records`. A real cache misses more; its SET share
is always at or above the floor.

## Summary

| | cluster26 | cluster45 | cluster53 | cluster13 |
|---|--:|--:|--:|--:|
| replayed records | 110,140,681 | 194,256,353 | 217,410,986 | 306,714,865 |
| distinct objects | 12,230,676 | 63,377,886 | 7,013,251 | 182,773,541 |
| **read-through WSS** | **10.8 GiB** | **7.1 GiB** | **41.3 GiB** | **766.9 GiB** |
| mean object | 945 B | 121 B | 6,329 B | 4,505 B |
| median object | 221 B | 121 B | 6,732 B | 123 B |
| accesses per object | 9.01 | 3.07 | 31.00 | 1.68 |
| one-hit wonders | **2.7%** | 63.1% | 53.1% | **70.5%** |
| **compulsory miss floor** | **0.1110** | **0.3263** | **0.0323** | **0.5959** |
| **read-through get:set** | **0.889:0.111** | 0.674:0.326 | **0.968:0.032** | 0.404:0.596 |
| median TTL | 120 s | **none** | 2,592,000 s | 300 s |
| synthesized size | 0.11% | 32.8% | 20.1% | 0.04% |
| synthesized TTL | 0.11% | n/a | **49.6%** | 0.04% |

## Distributions

### cluster26 — high reuse, small objects, short TTLs

| distribution | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| object size (per object, B) | 26 | 114 | 221 | 488 | 1,605 | 3,259 | 12,403 | 44,738 |
| object size (per access, B) | 26 | 119 | 236 | 580 | 2,325 | 4,716 | 15,390 | 123,768 |
| accesses per object | 1 | 4 | 5 | 8 | 14 | 21 | 52 | 216 |
| TTL (per access, s) | 60 | 60 | 120 | 600 | 660 | 1,140 | 14,400 | 14,400 |

### cluster45 — tiny objects, no TTLs anywhere

| distribution | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| object size (per object, B) | 80 | 121 | 121 | 121 | 126 | 128 | 169 | 292 |
| object size (per access, B) | 78 | 117 | 121 | 123 | 127 | 137 | 194 | 320 |
| accesses per object | 1 | 1 | 1 | 2 | 4 | 6 | 19 | 188 |

### cluster53 — large objects, extreme skew, month-long TTLs

| distribution | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| object size (per object, B) | 8 | 880 | 6,732 | 6,732 | 6,732 | 18,744 | 33,297 | 38,610 |
| object size (per access, B) | 8 | 272 | 6,732 | 13,662 | 27,225 | 32,076 | 37,554 | 39,435 |
| accesses per object | 1 | 1 | 1 | 4 | 19 | 47 | 275 | 3,376 |
| TTL (per access, s) | 1,814,400 | 2,592,000 | 2,592,000 | 2,592,000 | 2,592,000 | 2,592,000 | 2,592,000 | 2,592,000 |

### cluster13 — bimodal sizes, almost no reuse

| distribution | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| object size (per object, B) | 98 | 123 | 123 | 123 | 12,995 | 28,047 | 64,169 | 131,139 |
| object size (per access, B) | 98 | 123 | 123 | 123 | 13,166 | 28,498 | 65,292 | 133,408 |
| accesses per object | 1 | 1 | 1 | 2 | 3 | 5 | 7 | 7 |
| TTL (per access, s) | 300 | 300 | 300 | 300 | 300 | 300 | 300 | 300 |

## Which trace answers which question

**cluster26 is the workhorse.** Only 2.7% one-hit wonders against 63-71% on
the others, 9 accesses per object, and a 0.111 miss floor — there is real
reuse for a policy to exploit, so designs can actually differ. A 221 B median
object makes metadata a first-order term, and 60-600 s TTLs mean the TTL
worker fires during a run rather than idling. Its size distribution is also
the widest, p50 221 B to p99.9 44 KiB, so size-aware policies have something
to discriminate on.

**cluster53 discriminates on capacity.** A 0.032 miss floor and 31 accesses
per object, but heavily skewed: 53% of objects are seen once while p99.9 is
3,376 accesses. Large objects (6.7 KiB median) make metadata negligible, which
is the useful contrast against cluster26. Caveat: 20.1% of its sizes and
**49.6% of its TTLs are synthesized**, and the median TTL is 30 days, so
nothing expires in any run of realistic length.

**cluster45 is the metadata-pressure case.** 63.4M objects at a 121 B median —
metadata is roughly 46% of each object's footprint at 105 B/object, so a
conventional 25% fast tier holds nothing at all (the reservation exceeds the
budget and effective fast capacity saturates to zero). Useful precisely for
demonstrating that. It carries **no TTL on any record**, so it cannot be used
for expiry work at all.

**cluster13 is a thrash workload, not a policy benchmark.** 70.5% one-hit
wonders, 1.68 accesses per object, and a 0.596 compulsory miss floor — the
cache cannot exceed a 40% hit rate at *any* capacity, and under read-through
SETs outnumber GETs. Its 766.9 GiB WSS is 3.8x the whole machine and 9.7x node
0's DRAM, so at a 5 GiB cache it holds 2.0% of its working set. Good for
write-path and accounting stress; useless for discriminating between eviction
policies, because most traffic never hits.

## What moved versus the dropped-zeros traces

| cluster | WSS old to new | distinct old to new | get:set old to new |
|---|---|---|---|
| cluster13 | 378.4 to 766.9 GiB | 72.5M to 182.8M | 0.520:0.480 to 0.404:0.596 |
| cluster26 | 8.6 to 10.8 GiB | 9.2M to 12.2M | 0.850:0.150 to 0.889:0.111 |
| cluster45 | 2.9 to 7.1 GiB | 25.5M to 63.4M | 0.786:0.214 to 0.674:0.326 |
| cluster53 | 12.7 to 41.3 GiB | 1.6M to 7.0M | 0.990:0.010 to 0.968:0.032 |

The direction is not uniform. cluster13 gains references more slowly than it
gains distinct keys, so its miss floor **rises** from 0.4797 to 0.5959.
cluster26 gains references faster than keys, so its floor **falls** from 0.1501
to 0.1110. Old and new numbers are not rescalings of each other and should not
be compared without saying which trace produced them.

## Sizing

At the `synthetic_sweep.sh` rationale — cache below WSS so eviction is real,
fast tier a quarter of that so migration is real:

| cluster | WSS | 25% of WSS | 48% of WSS | fits node 0 DRAM (78.7 GiB)? |
|---|--:|--:|--:|---|
| cluster26 | 10.8 GiB | 2.7 GiB | 5.2 GiB | yes |
| cluster45 | 7.1 GiB | 1.8 GiB | 3.4 GiB | yes |
| cluster53 | 41.3 GiB | 10.3 GiB | 19.8 GiB | yes |
| cluster13 | 766.9 GiB | 191.7 GiB | 368.1 GiB | no, exceeds the machine |

The trap is the other direction: at the 15 GiB cache used for earlier sweeps,
cluster26 (10.8 GiB) and cluster45 (7.1 GiB) have their **entire working set
resident**, so nothing is ever evicted and every policy reports an identical,
meaningless result.
