# Read-through working sets: all 54 Twitter twemcache clusters

Measured from the **complete** traces, not samples, prefixes, or the published
summary statistics -- those answer a different question. 2.13 TB compressed
streamed from the CMU PDL mirror, 54/54 clusters, zero failures.

| | |
|---|---|
| **Total read-through WSS** | **1858 GB** |
| Requests parsed | 254,944,297,528 |
| Distinct objects | 3,948,647,618 |
| Compressed bytes processed | 2.13 TB |
| Published no-TTL total, same clusters | 5028 GB |

## Definition

A read-through cache is driven by gets alone: a miss produces the fill, so
counting sets as well would double-count the same object.

1. **GET rows only.** Sets, adds, incrs and deletes are not read-through accesses.
2. **`value_size > 0` only.** A get returning size 0 was a miss in production --
   there is no object to hold. Same filter the `.filt` masters use.
3. **Maximum size over time.** Each distinct key contributes the largest value it
   ever takes, because the cache must be provisioned for an object's biggest form.

```
WSS = sum over distinct keys of max(value_size)
```

Keys are hashed with `MurmurHash3_x64_128(key, seed=0).h1`, verified byte-exact
against the existing `cluster12.filt.zst` and `cluster37.filt.zst` masters, so
these counts are consistent with the traces used elsewhere in this repo.

## What the numbers say

**Our total is 1,858 GB against 5,028 GB published, but that is not a rescaling.**
Per-cluster ratios run from 0.00x to 3.53x. Three definitional differences pull in
opposite directions: excluding zero-size gets shrinks high-miss clusters, counting
only gets shrinks write-dominated ones, and max-size-over-time inflates clusters
with variable object sizes. Where all three cancel -- clusters 7, 14, 16, 40, 41,
42, 43 -- the two agree to within 1.00-1.04x.

**Some clusters are almost entirely phantom reads.** cluster27 returns nothing on
99.3% of its gets and cluster10 on 99.9%. Their true read-through working sets are
126 MB and 133 MB, against published figures of 10,653 MB and 46,168 MB.

**Excluding empty gets flips the read/write balance.** cluster37 is nominally
`get:0.63` but is effectively 0.422:0.578 -- write-majority. cluster50 flips from
`get:0.52` to 0.377:0.623, cluster27 from `get:0.85` to 0.038:0.962. These are
exactly the clusters whose production caches were missing most often.

**cluster23 holds 1.36 billion distinct objects**, an order of magnitude more than
any other cluster, at only 225 B each. Sizing a hash table for it needs ~51 GB;
the other 53 clusters together need less.

**cluster21 has no read-through working set at all.** It is `add:1.00` across 1.55
billion requests with zero gets. A WSS of 0 here is a category error, not a small
number -- exclude it from read-through studies rather than reporting zero.
cluster10 is effectively the same case: 1,235 usable gets out of 69.6 million.

## Per-cluster

Sorted by working set. `zero%` is the share of gets returning size 0. `pub` is
Twitter's published no-TTL WSS in MB (`stat/2020Mar.md`); cluster5 has no entry.
`eff get:set` is computed after the size filter; rows marked `*` have writes
containing incr/delete/add, so that column is get:non-get, not get:set.

| cluster | WSS | distinct | requests | zero% | mean obj | pub (MB) | ours/pub | eff get:set |
|---|---|---|---|---|---|---|---|---|
| cluster12 | **525.9 GB** | 375,949,779 | 2,649,686,669 | 0.7% | 1,502 B | 612,397 | 0.88x | 0.200:0.800 |
| cluster13 | **378.4 GB** | 72,472,986 | 825,381,985 | **50.7%** | 5,607 B | 595,112 | 0.65x | 0.226:0.774 |
| cluster23 | **285.3 GB** | 1,359,361,168 | 5,333,543,178 | 2.4% | 225 B | 93,931 | **3.11x** | 0.369:0.631 \* |
| cluster37 | **170.7 GB** | 39,020,642 | 3,168,270,709 | **60.4%** | 4,696 B | 3,121,435 | **0.06x** | 0.422:0.578 |
| cluster50 | **85.0 GB** | 1,320,408 | 139,655,585 | 44.4% | 69,110 B | 101,492 | 0.86x | 0.377:0.623 |
| cluster35 | **71.9 GB** | 40,790,697 | 1,828,228,908 | 34.0% | 1,893 B | 36,355 | **2.03x** | 0.940:0.060 |
| cluster5 | **35.1 GB** | 486,105,560 | 10,458,446,348 | **71.2%** | 77 B | n/a | n/a | 0.280:0.720 |
| cluster29 | **34.4 GB** | 84,519,980 | 6,920,247,859 | 23.4% | 437 B | 40,520 | 0.87x | 0.825:0.175 |
| cluster52 | **30.2 GB** | 135,845,907 | 13,426,570,607 | 2.2% | 239 B | 14,057 | **2.20x** | 0.907:0.093 \* |
| cluster33 | **19.3 GB** | 16,508,795 | 6,705,048,996 | 1.1% | 1,254 B | 7,522 | **2.63x** | 0.988:0.012 |
| cluster14 | **18.0 GB** | 22,825,825 | 3,029,143,401 | 10.1% | 847 B | 18,333 | 1.01x | 0.629:0.371 \* |
| cluster46 | **16.3 GB** | 61,825,150 | 6,610,320,613 | **68.4%** | 283 B | 80,265 | **0.21x** | 0.411:0.589 |
| cluster38 | **14.9 GB** | 82,750,952 | 1,342,423,951 | 0.3% | 193 B | 88,563 | **0.17x** | 0.062:0.938 |
| cluster32 | **14.7 GB** | 109,882,205 | 3,667,356,838 | 0.2% | 143 B | 4,262 | **3.53x** | 0.030:0.970 |
| cluster19 | **13.1 GB** | 140,012,272 | 2,006,896,318 | 40.9% | 100 B | 14,633 | 0.92x | 0.638:0.362 |
| cluster53 | **13.0 GB** | 1,630,740 | 246,508,262 | 23.1% | 8,541 B | 4,122 | **3.22x** | 0.852:0.148 \* |
| cluster49 | **11.2 GB** | 1,084,166 | 799,295,267 | 12.4% | 11,094 B | 4,979 | **2.30x** | 0.537:0.463 |
| cluster43 | **10.8 GB** | 141,969,352 | 9,038,233,795 | 12.3% | 81 B | 10,687 | 1.03x | 0.470:0.530 |
| cluster6 | **10.6 GB** | 16,310,959 | 8,376,426,233 | 2.4% | 694 B | 4,807 | **2.25x** | 0.976:0.024 |
| cluster4 | **9.8 GB** | 16,617,711 | 3,448,082,328 | 38.7% | 631 B | 0 | n/a | 0.890:0.110 |
| cluster34 | **9.6 GB** | 11,397,717 | 2,147,047,915 | 7.0% | 900 B | 11,552 | 0.85x | 0.930:0.070 |
| cluster26 | **8.6 GB** | 9,197,252 | 156,594,627 | 44.4% | 1,007 B | 3,317 | **2.66x** | 0.569:0.431 |
| cluster7 | **7.4 GB** | 3,854,808 | 1,044,513,039 | 22.7% | 2,057 B | 7,574 | 1.00x | 0.773:0.227 |
| cluster40 | **6.5 GB** | 95,422,184 | 3,847,885,075 | 18.4% | 72 B | 6,520 | 1.01x | 0.452:0.548 |
| cluster30 | **6.5 GB** | 9,015,789 | 2,554,900,851 | 0.9% | 768 B | 3,618 | **1.83x** | 0.989:0.011 |
| cluster39 | **6.1 GB** | 81,844,754 | 1,329,229,014 | 0.3% | 80 B | 47,562 | **0.13x** | 0.062:0.938 |
| cluster54 | **5.8 GB** | 33,270,965 | 11,866,133,317 | 2.3% | 187 B | 8,248 | 0.72x | 0.968:0.032 |
| cluster24 | **5.7 GB** | 7,234,762 | 3,289,457,233 | 0.9% | 838 B | 5,228 | 1.11x | 0.992:0.008 |
| cluster41 | **5.6 GB** | 80,391,442 | 3,214,344,864 | 17.8% | 74 B | 5,467 | 1.04x | 0.454:0.546 |
| cluster42 | **3.7 GB** | 5,380,425 | 3,904,264,364 | 4.2% | 741 B | 3,786 | 1.00x | 0.958:0.042 |
| cluster36 | **3.4 GB** | 58,244,445 | 5,781,263,748 | 3.3% | 62 B | 2,696 | 1.29x | 0.925:0.075 \* |
| cluster8 | **3.3 GB** | 250,451 | 1,302,137,879 | 3.0% | 14,219 B | 1,007 | **3.37x** | 0.496:0.504 |
| cluster48 | **2.9 GB** | 62,232,406 | 1,129,939,853 | 22.0% | 49 B | 9,100 | **0.32x** | 0.582:0.418 |
| cluster45 | **2.9 GB** | 25,534,209 | 227,773,739 | 38.7% | 120 B | 1,046 | **2.81x** | 0.780:0.220 \* |
| cluster11 | **1.9 GB** | 13,353,186 | 2,731,402,190 | 0.4% | 154 B | 1,898 | 1.03x | 0.957:0.043 |
| cluster31 | **1.2 GB** | 82,652,551 | 1,341,176,720 | 0.3% | 15 B | 21,989 | **0.05x** | 0.062:0.938 |
| cluster47 | **1.1 GB** | 4,761,608 | 6,225,239,439 | 0.1% | 243 B | 434 | **2.55x** | 0.999:0.001 |
| cluster17 | **1.1 GB** | 3,306,802 | 9,772,999,164 | 0.0% | 348 B | 1,162 | 0.95x | 0.993:0.007 |
| cluster16 | **1.1 GB** | 11,136,125 | 10,791,885,322 | 8.6% | 102 B | 1,085 | 1.00x | 0.924:0.076 |
| cluster28 | **1.0 GB** | 21,091,069 | 5,267,932,037 | 0.3% | 53 B | 2,849 | **0.38x** | 0.915:0.085 |
| cluster44 | 928.0 MB | 43,900,742 | 5,702,089,313 | 0.7% | 22 B | 536 | **1.73x** | 0.981:0.019 |
| cluster20 | 773.4 MB | 17,829,879 | 3,663,257,915 | 0.9% | 45 B | 2,151 | **0.36x** | 0.963:0.037 |
| cluster3 | 525.9 MB | 4,833,152 | 820,307,312 | 0.5% | 114 B | 0 | n/a | 0.993:0.007 |
| cluster15 | 512.2 MB | 5,300,915 | 5,263,841,494 | 15.7% | 101 B | 75,299 | **0.01x** | 0.001:0.999 |
| cluster51 | 427.1 MB | 3,631,708 | 6,322,932,555 | 2.6% | 123 B | 258 | **1.66x** | 0.897:0.103 |
| cluster9 | 318.1 MB | 7,987,621 | 10,646,675,648 | 0.9% | 41 B | 449 | 0.71x | 0.990:0.010 |
| cluster18 | 241.7 MB | 7,019,858 | 13,062,209,198 | 0.2% | 36 B | 311 | 0.78x | 0.963:0.037 |
| cluster2 | 232.9 MB | 3,572,298 | 7,226,679,214 | 0.1% | 68 B | 0 | n/a | 0.998:0.002 |
| cluster22 | 188.2 MB | 10,388,099 | 2,030,691,718 | **80.7%** | 18 B | 3,627 | **0.05x** | 0.332:0.668 \* |
| cluster10 | 133.3 MB | 55,816 | 139,150,615 | **99.9%** | 2,504 B | 46,168 | **0.00x** | 0.001:0.999 \* |
| cluster27 | 125.8 MB | 16,492,389 | 12,060,815,269 | **99.3%** | 7 B | 10,653 | **0.01x** | 0.038:0.962 |
| cluster25 | 46.0 MB | 1,162,641 | 12,048,484,393 | 0.0% | 41 B | 340 | **0.14x** | 0.955:0.045 |
| cluster1 | 12.8 MB | 94,296 | 6,461,081,324 | 0.5% | 142 B | 0 | n/a | 0.994:0.006 |
| cluster21 | 0.0 MB | 0 | 1,550,193,320 | 0.0% | 0 B | 8,926 | **0.00x** | 0.000:1.000 \* |

## Reproducing

```sh
cc -O3 -o wsscalc scripts/wsscalc.c
B=https://ftp.pdl.cmu.edu/pub/datasets/twemcacheWorkload/open_source
curl -s "$B/cluster12.sort.zst" | zstd -dc | ./wsscalc cluster12 67108864
```

Nothing is stored: each trace is decompressed and reduced in flight. Peak memory
is the hash table -- ~51 GB for the worst case (cluster23, 1.36B keys), under
2 GB for most clusters. The second argument is an initial table-size hint in
slots; it doubles automatically, so it only affects rehash count. Throughput is
network-bound at ~50 MB/s, about 12 h for all 54.

## Caveats

- The `eff get:set` column is exact for 46 clusters. For the 8 marked `*`
  (10, 14, 21, 22, 23, 36, 45, 53) the write side mixes `set` with
  `incr`/`delete`/`add`/`prepend`, so the column is get:non-get. A true get:set
  for those needs a second pass with a full operation histogram, i.e. re-reading
  the 2.13 TB.
- **TTL is not modelled.** These are the working sets a cache must hold if
  nothing expires. Twitter's published table also has a TTL-aware column, which
  is dramatically smaller for some clusters (cluster37: 3.12 TB -> 551 MB, a
  5,665x gap) and identical for others (clusters 24, 36, 44, 47, 53 are 1x --
  nothing ever expires). A TTL-aware version of this measurement is a separate
  calculation, and for most clusters it is the number that should drive
  provisioning.
- **Mean object size hides bimodality.** The `mean obj` column above is a mean,
  and for skewed clusters it is badly unrepresentative: cluster12's mean is
  1,693 B against a median of **6 B**, a 282x gap -- half its objects are 6 bytes
  and three-quarters are under 42 B, with essentially all the bytes in a thin
  tail above p90. cluster50 is the opposite, mean 68,429 B against a median of
  71,000 B, a genuinely uniform large-object workload. Percentiles for all 54
  clusters need another full pass and are not yet computed.
- Object sizes come from the trace's `value_size` column and exclude key bytes
  and per-object cache metadata. Measured separately on this cache, that
  metadata costs **19.4-24.4 B per object** regardless of object size -- under 2%
  for kilobyte objects but over 10% for 170 B ones, so small-object clusters need
  roughly 10% more capacity than the WSS column alone suggests. For cluster12,
  whose median object is 6 B, the metadata exceeds the median object itself.
