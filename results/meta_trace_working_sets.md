# Read-through working sets: the Meta/CacheLib traces

Measured by the same rules as the Twitter twemcache clusters, so the two are
directly comparable: read-through working set from each key's **first non-zero
get**, complete traces streamed in one pass, no sampling or truncation.

**Status: 8 of 8 datasets complete.** This document is regenerated as each
lands; figures for finished datasets are final.

## Provenance

Public S3 bucket, no credentials required:

```
https://cachelib-workload-sharing.s3.amazonaws.com/pub/
s3://cachelib-workload-sharing/pub/
```

Documentation: <https://cachelib.org/docs/Cache_Library_User_Guides/Cachebench_FB_HW_eval/>

| dataset | path under `pub/` |
|---|---|
| `kvcache_202206` | `kvcache/202206/kvcache_traces_{1..5}.csv` |
| `kvcache_202210` | `kvcache/202210/kvcache_traces_{1..5}.csv` |
| `kvcache_202401` | `kvcache/202401/kvcache_traces_{1..5}.csv` |
| `kvcache_flat_202312` | `kvcache/Flat_202312/flat_kvcache_0{1..4}.csv` |
| `memcache_202408` | `memcache/08_07_2024-Intel/2024-08-0{1..5}.csv` |
| `storage_block_202312` | `storage/202312/block_traces_{1..5}.csv` |
| `cdn_202303` | `cdn/{reag0c01,rnha0c01,rprn0c01}_20230315_20230322_*.csv` |
| `cdn_bigcache_202504` | `cdn/sea1c01_20250414_20250421_1.0000.csv` |

### Three caveats about provenance

**Not everything here is documented.** The CacheBench page describes
`kvcache/202206`, `kvcache/202401`, `cdn/202303`, `cdn/202504` and
`storage/202312`. It does not mention `kvcache/202210`, `kvcache/Flat_202312`
or `memcache/08_07_2024-Intel`, which exist in the bucket but carry no
published description of their collection or scaling.

**There is no published CSV schema.** Column layouts were inferred from the
files, which is why the converter takes an explicit `--cols=N` or `--fmt=`
per dataset rather than reading a spec.

**Row counts are expanded accesses, not CSV rows.** These traces carry a
repeat-count column; one row can stand for many accesses, and the converter
expands them. Twitter's row counts are raw CSV rows. The two columns are
therefore *not* directly comparable -- kvcache_202210's 597,181,141 kept CSV
rows become 1,301,094,881 replayed accesses, a factor of 2.2.

## The cache sizes Meta ran these under

The traces are sampled fractions of production clusters, and the
documentation gives both the per-host production cache and the traffic
factor, so the size a faithful replay should use is the product.

| dataset | DRAM/host | SSD/host | traffic factor | scaled DRAM | scaled SSD |
|---|---|---|---|---|---|
| `kvcache_202206` | 42 GB | 930 GB | 1/100 | **430 MiB** | 9.3 GiB |
| `kvcache_202401` | 42 GB | 930 GB | 1/125 | **344 MiB** | 7.4 GiB |
| `storage_block_202312` | 10 GB | 380 GB | 1/4000 | **3 MiB** | 0.1 GiB |
| `cdn_bigcache_202504` | 105 GB | 3577 GB | 1/7.08 | **15186 MiB** | 505.2 GiB |

`cdn/202303` is given as already-scaled sizes per cluster rather than as a
factor:

| cluster | scaled DRAM | scaled NVM |
|---|---|---|
| eag | 2857 MiB (2.79 GiB) | 129619 MiB (126.6 GiB) |
| nha | 6006 MiB (5.87 GiB) | 272314 MiB (265.9 GiB) |
| prn | 8357 MiB (8.16 GiB) | 375956 MiB (367.1 GiB) |

### What that implies, and it is the most useful thing on this page

**Meta's DRAM tier is tiny relative to the working set, and the flash tier
does nearly all the work.** Scaled to the trace, `kvcache_202206` runs a
**430 MiB** DRAM cache against a **24.90 GiB** working set -- it holds
**1.69%** of it. `kvcache_202401` is 1.78%. The DRAM:SSD ratio is about
**1:22** in both.

Any tiered evaluation that splits capacity near 1:2 is therefore describing a
far more DRAM-rich machine than the one these traces came from. That is a
legitimate design point, but it should be stated rather than assumed, and a
1:22 configuration is worth running alongside it.

## Measured results

| dataset | replayed accesses | distinct | WSS | mean obj | p50 obj |
|---|---|---|---|---|---|
| `kvcache_202206` | 1,439,157,229 | 17,990,451 | **24.90 GiB** | 1,485 B | 132 B |
| `kvcache_202210` | 1,301,094,881 | 16,428,431 | **25.78 GiB** | 1,684 B | 29 B |
| `kvcache_202401` | 995,580,106 | 14,757,964 | **18.92 GiB** | 1,376 B | 25 B |
| `kvcache_flat_202312` | 7,967,466,833 | 74,145,149 | **103.37 GiB** | 1,496 B | 13 B |
| `memcache_202408` | 451,728,352 | 6,198,138 | **9.42 GiB** | 1,631 B | 24 B |
| `storage_block_202312` **[not comparable]** | 44,728,210 | 16,597,456 | **61015.38 GiB** | 3,947,277 B | 2,121,728 B |
| `cdn_202303` **[not comparable]** | 248,775,964 | 86,190,527 | **25490.63 GiB** | 317,556 B | 30,350 B |
| `cdn_bigcache_202504` **[not comparable]** | 196,878,105 | 22,271,394 | **2344.68 GiB** | 113,040 B | 37,705 B |


### `storage_block_202312` is reported but not comparable

`block_id` is not a cache key. 36.5% of rows carry a non-zero `io_offset`
and 44,177 of 99,148 sampled block_ids recur at different offsets and sizes,
so one block is read as many distinct regions. Keying on `block_id`
conflates those regions into one object and then records a single I/O's
`io_size` as that object's size. The result -- 16.6 M objects averaging 3.9
MB for a 61,015 GiB working set, against a documented 95 MiB scaled SSD tier
-- is an artifact of the mismatch, not a measurement. Re-running with
`(block_id, io_offset)` as the key would give a real answer.

Excluded from all totals.

### `cdn_202303` is reported but not comparable

The converter reads `responseSize` as the object size, but that is the size
of one response: 18.8% of requests are range requests, 12% of rows return
less than the object's own size, and 43.8% of repeat accesses to the same
cacheKey report a *different* responseSize. So "the first non-zero size" is
not the object's size. The file carries the right column -- `objectSize`,
populated on 99.7% of rows, mean 10.4 MB against responseSize's 531 KB -- so
re-running with `I_SIZE=3` would give a real answer, roughly 20x larger.

Excluded from all totals.

### `cdn_bigcache_202504` is reported but not comparable

Same defect as `cdn_202303`: sized on `responseSize` rather than
`objectSize`, so range requests and partial responses make the recorded
first size unrelated to what the cache would hold.

Excluded from all totals.

## Operation mix

**Workload get:set** is the source trace before filtering. **Read-through
get:set** is what the cache performs -- only gets are replayed, a hit is a GET
and a miss becomes the fill -- so it equals `(1 - miss) : miss`, and its floor
is `distinct / accesses`, reached only at infinite capacity.

| dataset | workload get:set | zero-size gets | read-through get:set (inf) | compulsory miss |
|---|---|---|---|---|
| `kvcache_202206` | 0.830:0.170 | 23.0% of gets | 0.987:0.013 | 0.0125 |
| `kvcache_202210` | 0.831:0.169 | 20.6% of gets | 0.987:0.013 | 0.0126 |
| `kvcache_202401` | 0.789:0.211 | 29.3% of gets | 0.985:0.015 | 0.0148 |
| `kvcache_flat_202312` | 0.865:0.135 | 15.8% of gets | 0.991:0.009 | 0.0093 |
| `memcache_202408` | 0.826:0.174 | 24.1% of gets | 0.986:0.014 | 0.0137 |
| `storage_block_202312` | 0.637:0.363 | 0.0% of gets | 0.629:0.371 | 0.3711 |
| `cdn_202303` | 1.000:0.000 | 0.1% of gets | 0.654:0.346 | 0.3465 |
| `cdn_bigcache_202504` | 1.000:0.000 | 0.1% of gets | 0.887:0.113 | 0.1131 |

### Meta's reuse is an order of magnitude better than Twitter's

Compulsory-miss floors here are around **1%**, meaning each distinct object is
accessed roughly 70-110 times. The best Twitter cluster was cluster50 at
3.27%, and four Twitter clusters had a floor of **1.0000** -- no reuse at all,
so a read-through cache misses everything at any size.

That inverts what the two corpora test. The Twitter clusters mostly stress
**capacity**: the working set does not fit and the question is what to evict.
Meta kvcache stresses **policy**: the hot set is small and heavily reused, so
which objects are kept is what matters. Combined with mean objects near 1.5 KB
against Twitter's typical ~200 B, they sit at close to opposite corners of the
design space, which is the argument for reporting both.

## Object-size distribution

**per-ACCESS** weights every request; **per-OBJECT** weights each distinct key
once at its fill size, and its sum is the WSS column above.

| dataset | | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|---|---|---|---|---|---|---|---|---|
| `kvcache_202206` | access | 63 | 103 | 124 | 189 | 504 | 1,487 | 8,122 | 46,233 |
|  | object | 74 | 104 | 132 | 169 | 394 | 759 | 10,606 | 480,563 |
| `kvcache_202210` | access | 8 | 9 | 40 | 126 | 540 | 1,300 | 8,354 | 37,085 |
|  | object | 8 | 8 | 29 | 69 | 328 | 1,031 | 12,143 | 523,288 |
| `kvcache_202401` | access | 8 | 12 | 47 | 249 | 1,563 | 4,301 | 21,013 | 90,638 |
|  | object | 8 | 9 | 25 | 67 | 585 | 2,582 | 12,305 | 420,582 |
| `kvcache_flat_202312` | access | 8 | 10 | 50 | 147 | 489 | 1,913 | 11,425 | 42,867 |
|  | object | 8 | 9 | 13 | 68 | 757 | 3,224 | 11,000 | 523,288 |
| `memcache_202408` | access | 9 | 13 | 84 | 369 | 1,291 | 5,334 | 42,545 | 161,134 |
|  | object | 9 | 10 | 24 | 94 | 472 | 1,592 | 17,646 | 419,429 |
| `storage_block_202312` | access | 53 | 839,680 | 2,093,056 | 2,187,264 | 8,388,608 | 8,388,608 | 8,388,608 | 8,388,608 |
|  | object | 159 | 929,570 | 2,121,728 | 8,388,608 | 8,388,608 | 8,388,608 | 8,388,608 | 8,388,608 |
| `cdn_202303` | access | 217 | 6,439 | 35,508 | 158,543 | 1,001,076 | 1,865,922 | 7,697,304 | 67,038,755 |
|  | object | 557 | 10,195 | 30,350 | 106,914 | 415,848 | 1,001,617 | 5,189,576 | 67,038,755 |
| `cdn_bigcache_202504` | access | 1,470 | 15,375 | 48,183 | 140,703 | 461,031 | 789,823 | 1,889,958 | 4,629,264 |
|  | object | 1,770 | 16,152 | 37,705 | 90,821 | 232,004 | 400,890 | 1,217,698 | 4,708,512 |

All sizes are **base 2**: 1 KiB = 1024 B, 1 MiB = 1024 KiB, 1 GiB = 1024 MiB.
