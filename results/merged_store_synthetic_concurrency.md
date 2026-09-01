# Merged store vs split design: synthetic traces, 1 and 5 clients

15 GB cache / 5 GB fast tier, `lru-compact-hybrid`, all four binaries built from
one tree (commit `ba3b40d`). `-c 5` PARTITIONS the trace across 5 concurrent
client threads — verified: same total ops, same miss ratio, wall time 49 s → 31 s.

Raw: `sweep/synmg_index.csv`, cells in `sweep/synmg/`.

## The headline is negative: sharding did not buy a concurrency win

| trace | miss | GET mean, `-c 1` | GET mean, `-c 5` |
|---|--:|--:|--:|
| standard_web | 0.085 | 6070 → **5584 (−8.0%)** | 9270 → 9318 (+0.5%) |
| low_alpha_cold | 0.214 | 10078 → **9416 (−6.6%)** | 13891 → **15804 (+13.8%)** |
| uniform_baseline | 0.465 | 13146 → 13572 (+3.2%) | 14056 → **16620 (+18.2%)** |

Going from one client to five makes the merged store **worse relative to the
split design on every trace**, not better. Tail latency follows: standard_web
GET p99 is 34,437 → 23,452 ns (−31.9%) at `-c 1`, and 48,033 → 48,186 (a wash)
at `-c 5`.

**Miss ratio predicts the outcome exactly**, which identifies the mechanism.
Merged wins where reads dominate and loses where inserts do:

* insert on merged — one shard WRITE lock for slot-alloc, `link_front`,
  `bucket_link` (plus a possible rehash), `settle_fast_tier`, `publish_tail`
* insert on split — one DashMap bucket write, plus a stack insert into a
  **separate structure with its own lock**

The split design gets two independent lock domains for free *because* it has two
structures. Merging them merges their write contention, and sharding cannot buy
back a lock domain that no longer exists. It recovered the catastrophic
single-global-lock revision (+3.6% GET / +25.4% SET); it does not make the
merged store scale better than the split one.

## `MERGED_UPDATE_INTERVAL` does nothing

| trace, `-c 5` | interval 0 | interval 64 |
|---|--:|--:|
| uniform_baseline | 40 s / 16,620 ns | 40 s / 16,751 ns |
| low_alpha_cold | 63 s / 15,804 ns | 63 s / 15,787 ns |
| standard_web | 87 s / 9,318 ns | 89 s / 9,270 ns |

Noise everywhere. memcached's `ITEM_UPDATE_INTERVAL` targets contention on the
LRU lock from many client threads; here `touch` runs on the **single** policy
worker thread, so skipping relinks relieves pressure from a party that is not
the bottleneck. The losses track insert rate, not hit rate.

## Memory: consistent, small, and object count is the wrong metric

These traces hold ~17.9 KB objects, so metadata is ~1% of an object and the
ceiling is low. Merged holds FEWER objects while allocating LESS, which looks
contradictory until the value bytes are computed from `used_size`:

| trace | `-c` | tier alloc | merged alloc | Δ | data cached Δ |
|---|--:|--:|--:|--:|--:|
| standard_web | 1 | 16,071 MB | 15,900 MB | −1.06% | +61.5 MB |
| standard_web | 5 | 16,172 | 16,003 | −1.04% | +61.5 MB |
| low_alpha_cold | 1 | 15,618 | 15,451 | −1.07% | +61.5 MB |
| low_alpha_cold | 5 | 15,700 | 15,529 | −1.09% | +61.5 MB |
| uniform_baseline | 1 | 15,276 | 15,184 | −0.60% | +61.5 MB |
| uniform_baseline | 5 | 15,341 | 15,246 | −0.62% | +61.5 MB |

Merged caches **+61.5 MB more value bytes** (+0.41%) in the same 15 GB budget,
identically in all six pairs, while allocating ~1% less. It holds ~0.4% fewer
objects only because the resident size mix shifted 0.9% larger (mean resident
value 17,734 → 17,885 B). **Object count is the wrong metric here; cached value
bytes is the right one.** Miss ratio differs by 0.1–0.2%.

## Divergence: concurrency makes the split design worse

Maximum map-vs-stack drift observed over each run:

| trace | split `-c 1` | split `-c 5` | merged, either |
|---|--:|--:|--:|
| uniform_baseline | 4 | **174** | 0 |
| low_alpha_cold | 47 | 12 | 0 |
| standard_web | 44 | 10 | 0 |

Five concurrent clients take uniform_baseline's split drift from 4 to 174 — 43×.
The merged store is exactly 0 in all 15 cells, as it is in every cluster cell.

## Conclusion

The merged store's case is **memory and correctness, not throughput**. It saves
17–27% of allocation where objects are small (cluster19), cannot desynchronise
by construction, and wins latency on read-heavy workloads single-client. Under
concurrency on insert-heavy workloads it is slower than the split design.
