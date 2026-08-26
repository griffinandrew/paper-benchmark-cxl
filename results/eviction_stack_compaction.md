# Compacting the eviction stacks

What every hybrid policy stores per object, why they all cost the same, and
what the `LfuCompactHybridStack` technique is worth if applied to the rest.

All per-object figures here are **measured**, not derived, by
`policy_stack::measure_overhead`: the least-squares slope of resident bytes
against object count, one point per process at 2^20..2^23 objects, R^2 >=
0.9999. Projections are labelled as such and are the only unmeasured numbers.

## The measurement

| stack | lists | measured B/object |
|---|---|---|
| `LfuHybridStack` | 2 (`FrequencyChain`) | **147.5** |
| `LruHybridStack` | 1 | 99.3 |
| `FifoHybridStack` | 1 | 100.2 |
| `TwoQHybridStack` and the 2Q family | 2-3 | 99.3-99.9 |
| `S3FifoHybridStack` and the S3-FIFO family | 2 | 99.3-99.9 |
| `LruSizedHybridStack` | 4 | 99.9 |
| `LruLfuHybridStack` | 2 | 99.9 |
| `LfuCompactHybridStack` | 1 slab | **66.0** |

The striking result is the middle band: **seventeen stacks land within 0.6 B of
each other**, and the count of lists makes no difference. One list costs the
same as four. That is not a coincidence and it is the whole basis of this
proposal.

## Why list count does not matter

An object is in exactly **one** list at a time. 2Q's `a1_in`, `a1_out` and `am`
partition the objects; they do not replicate them. `LruSizedHybridStack`'s four
lists are a 2x2 partition by size class and tier. So per object, every stack
pays exactly three things:

```
1. one HashList node       Entry<HashedKey> { data: 8, prev: 8, next: 8 }
                           24 B, but individually malloc'd -> 32 B size class
2. one HashList index row   HashMap<DataRef, NonNull<Entry>>: 8 + 8 + ctrl
3. one `entries` row        HashMap<HashedKey, XEntry>: 8 + 8 + ctrl
```

Items 2 and 3 are **two separate hash maps, both keyed by the same
`HashedKey`, both holding exactly one row per object**. That is the redundancy.
Every policy carries it, and it is roughly a third of the per-object cost.

The `XEntry` payload is 8 bytes in every policy -- `tier`, `dram_resident`,
`size`, plus a `queue` tag in the 2Q family -- so an entire hash map exists to
store eight bytes that would fit in the list node's existing padding.

`LfuHybridStack` costs 147.5 rather than ~99.6 because `FrequencyChain` adds
per-object count-stack machinery on top of the three items above.

## What the compact stack actually changed

`CompactFrequencyChain` reached 66.0 B/object by four changes. Their
contributions are separable, and only the first two are general:

**T1. Fold `entries` into the list node.** The 8-byte payload moves into the
slab record, and one of the two hash maps disappears. Largest single win,
applies to all 19 stacks, and is the least invasive: no algorithm changes, only
a change in where `tier`/`size`/`dram_resident` are read from.

**T2. Slab with u32 links, not malloc'd pointer nodes.** `Entry<T>` is 24 B
individually allocated, which jemalloc rounds to a 32 B size class and charges
allocator metadata and fragmentation on top. A `Vec<CompactEntry>` costs
exactly `size_of::<CompactEntry>()` per object with one allocation for the
whole slab, and `prev`/`next` shrink from 8-byte pointers to 4-byte indices.
Applies to all 19.

This is also where the *performance* came from. Splitting the timings showed
insert unchanged (1.0x) and the gain concentrated in `bump` -- a frequency bump
walks neighbours, and in a slab the neighbours are usually already in cache.

**T3. One slab spanning all of a policy's lists, with a tag field.** A queue or
tier change becomes a relink -- two u32 writes -- instead of remove-from-one-
structure plus insert-into-another. Structural rather than per-object, so it
does not show in the B/object figure; it removes fixed overhead and per-move
work. Applies to the 15 multi-list policies, worth most where there are most
lists (`LruSizedHybridStack`, then the 2Q and S3-FIFO families).

**T4. Buckets keyed by distinct frequency.** LFU-specific. `fast_buckets` and
`slow_buckets` hold one entry per *distinct frequency* rather than per object,
which is what removed LFU's count-stack surcharge. Only `LfuHybridStack` and
`LruLfuHybridStack` (whose slow side is a `FrequencyChain`) have anything to
gain here.

## Projection

For the seventeen ~99.6 B stacks, T1 + T2 give a slab record plus a single
index, which is structurally identical to what `CompactFrequencyChain`
measured at 66.0:

| | now (measured) | with T1+T2 (projected) |
|---|---|---|
| eviction stack | 99.6 B | ~66 B |
| + Arc header and map entry, (48 + 63) x 1.12 | 124 B | 124 B |
| **total reservation** | **224 B/object** | **190 B/object** |

About **34% off the eviction stack**, **15% off the total per-object
reservation**. The corresponding measured result for LFU was fast-tier metadata
falling from 0.181 GB to 0.127 GB (-29.6%) at the standing config, which freed
room for 0.8-3.9% more objects resident in fast.

These are projections. The one measured instance of the technique
(`LfuCompactHybridStack`) landed at 66.0, but it started from a different
baseline, so the seventeen should be measured individually as they are
converted rather than assumed to hit the same number.

## What it does not buy

Performance. The corrected LFU comparison is **parity, within ~1%** on every
throughput and latency measure across standard_web, low_alpha_cold and
uniform_baseline. An earlier revision of this measurement showed +12% to +56%
throughput; that was an admission-tier bug placing every new object in DRAM,
not the data structure. The case for compaction is DRAM, and the performance
claim is only that it is not paid for.

## Order of work

1. **`LruHybridStack`, `FifoHybridStack`** -- one list, no queue tag, no
   frequency buckets. Validates T1+T2 in isolation, and both have an existing
   integration suite to port.
2. **2Q family** (4 stacks) -- adds T3 and a 2-bit queue tag. `TwoQEntry`
   already carries `queue`, so the tag is not new state.
3. **S3-FIFO family** (9 stacks) -- same shape as 2Q. Ghost queues are already
   compacted to 8 B/entry and are charged separately against `ghost.len()`, so
   they are out of scope here.
4. **`LruSizedHybridStack`** -- 4 lists, largest T3 win, but its four capacity
   and count gauges make it the most intricate; do it last.
5. **`LruLfuHybridStack`** -- T4 applies to its `FrequencyChain` slow side;
   `CompactFrequencyChain` is reusable close to as-is.

## Verification protocol

Each conversion is a port of an existing algorithm, and the two defects that
got through on the LFU one were both silent *omissions* -- an absent builder
call and an inverted default -- that produced plausible numbers rather than
failures. So per stack:

1. **Fidelity test** against the original: identical migration sequence, order
   and final tiers, across several capacities *and several reservations*. Equal
   reservation isolates logic; unequal reservation shows what the smaller
   reservation itself does.
2. **Port the integration suite.** Not a subset. Having no
   `lfu_compact_hybrid_cache_integration.rs` is precisely why the admission bug
   survived -- unit tests drive the stack directly and never place any bytes.
   Verify the ported suite has teeth by reverting the fix and watching it fail.
3. **Check `hybrid_policy::admission_tier`.** Now exhaustive, so a new policy
   is a compile error; a *converted* policy keeps its existing arm, but the arm
   must still match the new stack's latch behaviour.
4. **Re-measure** with `measure_overhead` rather than deriving the constant.

## The pattern worth naming

Four defects in this work were the same shape: **per-policy dispatch where a
missing entry produces a plausible default instead of an error.** Overhead
constants behind per-policy `cfg` (silent 0), `hybrid_lru_sized` reporting, the
ghost import, and `admission_tier`'s `_ => Tier::Fast`. Each conversion below
adds a policy to several such dispatch sites. Making them exhaustive first is
cheaper than finding the next one in a sweep.
