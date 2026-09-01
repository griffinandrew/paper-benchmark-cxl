# Dropping the weak refcount: 16 B/object, and what it unlocks on cluster19

`Arc<T>` allocates an inner block carrying BOTH a strong and a weak count.
Nothing in this crate ever creates a `Weak`, so on this crate's `TieredBuffer`
the weak count is pure cost:

```
strong           8
weak             8   <- never used
TieredBuffer    24   (Box ptr 8, len 8, enum tag 8)
                --
                40   -> jemalloc rounds to its 48-byte size class
```

`shared::Shared` keeps only the strong count: 8 + 24 = 32, landing exactly on
the class below. It retains the two properties the cache depends on -- cheap
clone (so a reader can release the shard lock before touching the bytes, which
is what lets `apply_migration` build a destination buffer unlocked) and
`ptr_eq`, the identity check that makes a migration safe to apply.

Branch `slim-refcount`. `ARC_VALUE_HEADER_OVERHEAD` follows, 48 -> 32, so the
accounting stops charging for bytes that are no longer allocated.

## Synthetic traces: consistent, and free

Split design, 15 GiB cache / 5 GiB fast tier, one client.

| trace | delta allocated | delta miss | delta GET | delta SET |
|---|--:|--:|--:|--:|
| uniform_baseline | -16.4 B/object | 0 | +0.3% | -0.3% |
| low_alpha_cold | -15.6 B/object | 0 | +0.3% | +0.6% |
| standard_web | -15.9 B/object | 0 | +0.7% | -0.4% |

Miss ratio is identical on all three; latency moves by less than 1% in both
directions, i.e. noise. The saving is real and costs nothing.

On these traces it is also nearly pointless: 16 B is 0.09% of a 17.9 KB object.

## cluster19: the threshold

Merged store, 150M-record prefix, 12 GB cache / 4 GiB fast tier, one client.
~100 B objects, 31.9M of them, so metadata dominates.

| | `Arc` (48) | `Shared` (32) |
|---|--:|--:|
| objects | 31,902,702 | 31,902,702 |
| miss ratio | 0.2127 | 0.2127 |
| **fast tier objects** | **0** | **2,122,910** |
| allocated | 8,947,206,176 | **8,085,810,112** |
| GET mean | 1,317 ns | 1,271 ns (-3.5%) |
| SET mean | 1,117 ns | 1,055 ns (-5.6%) |

**The fast tier goes from completely empty to holding 2.1 million objects.**

Metadata is DRAM-resident in BOTH tiers, so it is reserved out of the fast
budget before any value can be placed there. At 143 B/object that reservation
EXCEEDS the whole fast tier and the effective capacity is zero -- every object
is demoted and the tiering is inoperative. At 127 it fits:

```
fast budget                       4 GiB = 4,294,967,296 B
metadata  127 B x 31,902,702            = 4,051,643,154 B
                                          -------------
usable fast capacity                        243,324,142 B  ~ 232 MB

measured  2,122,910 objects x ~100 B                       ~ 212 MB
```

The prediction and the measurement agree to within 9%.

## Two things not to overclaim

**The 27 B/object total saving is larger than the 16 B the refcount explains.**
Total allocation fell 861 MB across 31.9M objects. 16 B of that is the
refcount; the remaining ~11 B is unattributed, and most plausibly comes from
allocator size-class behaviour changing as 2.1M objects move from the slow-tier
arena into the fast one. Not established.

**The fast-tier object gain is only meaningful where metadata binds.** On the
synthetic traces it went +4,306 on uniform_baseline and -550 on low_alpha_cold
-- composition noise, not capacity. Only on cluster19, where the reservation
actually exceeded the budget, is the change decisive.

## Why this matters beyond 16 bytes

Metadata does not scale with object size. It is negligible where objects are
large and decisive where they are small, which is exactly the bimodal split
`metadata_overhead_by_trace.md` found across the 53-cluster fleet: 28 clusters
are metadata-bound above 100%, six are object-bound under 5%, and very little
sits between.

cluster19 is the trace this line of work was aimed at, and this is the first
change that moves it.

Remaining, measured and named: 25.6 B/object of empty slab from `Vec` doubling
(capacity measures 1.40x the object count), which a chunked slab would recover;
and the other 16 B of the buffer handle -- its `len` duplicates the slot's
`size`, its enum tag duplicates the slot's `tier` -- which needs a u32 length
and the tier in a pointer bit.
