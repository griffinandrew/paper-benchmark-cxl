# Read-through working sets: all 54 Twitter twemcache clusters

Measured from the **complete** traces -- not samples, not prefixes, and not the
published summary statistics, which answer a different question. 1.94 TiB
compressed, streamed from the CMU PDL mirror, one pass per cluster, 54/54 with
no failures.

| | |
|---|---|
| **Total read-through WSS** | **1,814 GiB** |
| Trace operations read | 254,944,297,528 |
| Replayed records (get, size>0) | 177,321,146,276 |
| Distinct objects | 3,948,647,618 |
| Compressed bytes processed | 1.94 TiB (2,129,937,413,922 B) |

## Definition

A read-through cache writes an object exactly **once**, on the get that misses.
`Client::handle_read_through` calls `set()` only in the miss arm; a later hit
never rewrites the value. So an object's resident size is the one observed when
it was filled -- the size at its **first non-zero get**.

1. **`get` only.** Not `gets`: the verified `.filt` rule (byte-exact against the
   cluster12/cluster37 masters) keeps `operation == "get"`, so the trace files
   the benchmark replays contain no multi-gets. They are tallied separately.
2. **`value_size > 0` only.** A get returning size 0 was a miss in the source
   system; there is no object to hold.
3. **First non-zero size**, not the maximum over time.

```
WSS = sum over distinct keys of first_nonzero(value_size)
```

An earlier revision of this document used the **maximum** size a key ever took.
That is wrong for read-through -- it would only be correct if a hit resized the
object. The error is not uniform, so the old figures cannot be rescaled: on most
clusters the two rules agree to four significant figures, but cluster41 differs
by **2.21x** (5,684 MiB max vs 2,570 MiB first) and cluster50 by 13%.

All sizes are **base 2**: 1 KiB = 1024 B, 1 MiB = 1024 KiB, 1 GiB = 1024 MiB,
1 TiB = 1024 GiB. Byte counts and percentiles are exact byte values.

Keys are hashed with `MurmurHash3_x64_128(key, seed=0).h1`.

### Two get:set ratios, which are not interchangeable

**Workload get:set** is a property of the source trace: cacheable gets versus
the set family. **Read-through get:set** is what the cache itself performs, and
it is a different quantity -- only gets are replayed, a hit is a GET and a miss
becomes the fill, so it equals `(1 - miss) : miss`.

At an infinite cache every distinct key is filled exactly once, so the
read-through SET count is the distinct-key count and the ratio reaches its
floor at `compulsory = distinct / kept`. A real cache misses more, so its SET
share is always at or above that floor; the exact value depends on cache size,
fast-tier split and policy, and belongs in the sweep results, not here.

The two often invert. cluster37 is write-majority as a workload (0.422 read)
but overwhelmingly read-dominated under read-through (0.952), because the
trace's sets are discarded and the only writes left are its few misses.
Measured check: cluster50's floor is 0.0327 and a real run at a 13.97 GiB
cache (`--cache-max-size 15000000000`, i.e. 15e9 B) missed 0.1275 -- about
4x the floor, which is the capacity-miss contribution.

## What the numbers say

**Our total is 1,769 GiB against 5,028 GiB published, and that is not a rescaling.**
Across the 49 clusters with a published figure the overall ratio is 0.352x, but
per-cluster ratios run from 0.000x (cluster21) to 3.528x (cluster32). Only **4**
clusters -- cluster7, cluster11, cluster14, cluster16 -- agree within 3%. Three definitional
differences pull in opposite directions: excluding zero-size gets shrinks
high-miss clusters, counting only gets shrinks write-dominated ones, and
first-size rather than max-size shrinks clusters with growing objects.

The published figures are quoted as reported. A base-2/base-10 ambiguity in
their units would move every ratio by ~5%, which changes none of the
conclusions below -- the spread is two orders of magnitude wider than that.

**Some clusters are almost entirely phantom reads.**
cluster10 returns nothing on 99.9% of its gets: a true read-through working
set of 133.3 MiB against a published 45.1 GiB.
cluster27 returns nothing on 99.3% of its gets: a true read-through working
set of 125.8 MiB against a published 10.4 GiB.
cluster22 returns nothing on 80.7% of its gets: a true read-through working
set of 188.2 MiB against a published 3.5 GiB.
These are exactly the clusters whose production caches were missing most often,
and they dominate the gap in the total.

**cluster21 has no read-through working set at all.** It is `add:1.00` across
1,550,193,320 requests with zero gets. A WSS of 0 here is a category error, not a
small number -- exclude it from read-through studies rather than reporting zero.

**4 clusters have no reuse whatsoever** -- cluster31, cluster32, cluster38, cluster39 -- meaning
`distinct == kept`: every get is a first-time access. A read-through cache
misses everything on these at *any* capacity, so they cannot be used to
discriminate between policies.

**cluster23 holds 1,359,361,168 distinct objects**, an order of magnitude more than any
other cluster, at only 225 B each. Sizing a hash table for it needs ~51 GB;
it was the one cluster that had to be measured alone.

**cluster12 is the largest working set at 525.8 GiB**, driven by object size rather
than count: 375,949,779 distinct objects averaging 1,501 B.

## Per-cluster

**replayed records** is the count the benchmark actually sees: gets with a
non-zero size, after both filters. It is exactly the record count of the `.bin`
trace file (file size / 25). **trace ops** is every operation in the source
CSV, before filtering. The two can differ enormously -- cluster10 has
139,150,615 trace ops but only 57,829 replayed records, because 99.9% of its
gets return size 0 and its remaining traffic is sets.

`zero%` is the share of gets returning size 0. **pub** is Twitter's published
no-TTL WSS from `stat/2020Mar.md`; `ratio` is ours over theirs. Five clusters
have no published entry and are left blank.

**workload get:set** counts cacheable source operations after the size filter:
the set family is `set`/`add`/`cas`/`replace`/`append`/`prepend`, while
`incr`/`decr`/`delete` are excluded from both sides and counted in `other ops`.
Where `other ops` is large the ratio describes only part of the traffic --
cluster14, cluster22 and cluster29 are the notable cases.

**read-through get:set (inf)** and **compulsory miss** describe the cache, not
the workload, and are the best case: `distinct / kept`, achieved only at
infinite capacity. Any real cache sits above that miss floor. A floor of
1.0000 means distinct == kept -- every get is a first-time access, so there is
no reuse to exploit and read-through misses everything at *any* size. A
cluster with no replayed gets has no read-through workload at all and is
shown as `--` rather than as a perfect hit rate.

| cluster | WSS | pub | ratio | distinct | replayed records | trace ops | zero% | mean obj | workload get:set | read-through get:set (inf) | compulsory miss | other ops |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cluster12 | **525.8 GiB** | **598.0 GiB** | 0.879x | 375,949,779 | 530,235,253 | 2,649,686,669 | 0.7% | 1,501 B | 0.200:0.800 | 0.291:0.709 | 0.7090 | 0 |
| cluster13 | **378.4 GiB** | **581.2 GiB** | 0.651x | 72,472,986 | 151,075,072 | 825,381,985 | 50.7% | 5,606 B | 0.226:0.774 | 0.520:0.480 | 0.4797 | 0 |
| cluster23 | **285.1 GiB** | **91.7 GiB** | 3.108x | 1,359,361,168 | 1,950,265,755 | 5,333,543,178 | 2.4% | 225 B | 0.526:0.474 | 0.303:0.697 | 0.6970 | 1,575,128,916 |
| cluster37 | **165.4 GiB** | **3048.3 GiB** | 0.054x | 39,020,642 | 812,935,002 | 3,168,270,709 | 60.4% | 4,550 B | 0.422:0.578 | 0.952:0.048 | 0.0480 | 0 |
| cluster50 | **73.9 GiB** | **99.1 GiB** | 0.746x | 1,320,408 | 40,417,289 | 139,655,585 | 44.4% | 60,102 B | 0.377:0.623 | 0.967:0.033 | 0.0327 | 0 |
| cluster35 | **71.9 GiB** | **35.5 GiB** | 2.026x | 40,790,697 | 1,156,801,614 | 1,828,228,908 | 34.0% | 1,893 B | 0.940:0.060 | 0.965:0.035 | 0.0353 | 0 |
| cluster5 | **34.8 GiB** | -- | -- | 486,105,560 | 1,729,686,333 | 10,458,446,348 | 71.2% | 76 B | 0.280:0.720 | 0.719:0.281 | 0.2810 | 795,241 |
| cluster29 | **32.8 GiB** | **39.6 GiB** | 0.829x | 84,519,980 | 4,557,571,744 | 6,920,247,859 | 23.4% | 416 B | 0.830:0.170 | 0.981:0.019 | 0.0185 | 34,606,714 |
| cluster52 | **29.7 GiB** | **13.7 GiB** | 2.161x | 135,845,907 | 11,938,611,526 | 13,426,570,607 | 2.2% | 234 B | 0.936:0.064 | 0.989:0.011 | 0.0114 | 83,985,740 |
| cluster33 | **19.2 GiB** | **7.3 GiB** | 2.617x | 16,508,795 | 6,556,287,649 | 6,705,048,996 | 1.1% | 1,250 B | 0.988:0.012 | 0.997:0.003 | 0.0025 | 0 |
| cluster14 | **18.0 GiB** | **17.9 GiB** | 1.004x | 22,825,825 | 1,778,492,432 | 3,029,143,401 | 10.1% | 845 B | 0.823:0.177 | 0.987:0.013 | 0.0128 | 666,841,423 |
| cluster38 | **14.9 GiB** | **86.5 GiB** | 0.173x | 82,750,952 | 82,750,952 | 1,342,423,951 | 0.3% | 193 B | 0.062:0.938 | 0.000:1.000 | 1.0000 | 0 |
| cluster32 | **14.7 GiB** | **4.2 GiB** | 3.528x | 109,882,205 | 109,882,205 | 3,667,356,838 | 0.2% | 143 B | 0.030:0.970 | 0.000:1.000 | 1.0000 | 0 |
| cluster46 | **14.3 GiB** | **78.4 GiB** | 0.182x | 61,825,150 | 1,438,340,495 | 6,610,320,613 | 68.4% | 247 B | 0.411:0.589 | 0.957:0.043 | 0.0430 | 0 |
| cluster19 | **13.0 GiB** | **14.3 GiB** | 0.911x | 140,012,272 | 888,230,858 | 2,006,896,318 | 40.9% | 99 B | 0.638:0.362 | 0.842:0.158 | 0.1576 | 0 |
| cluster53 | **12.7 GiB** | **4.0 GiB** | 3.166x | 1,630,740 | 167,242,141 | 246,508,262 | 23.1% | 8,392 B | 0.852:0.148 | 0.990:0.010 | 0.0098 | 9,186 |
| cluster6 | **10.4 GiB** | **4.7 GiB** | 2.217x | 16,310,959 | 7,989,109,493 | 8,376,426,233 | 2.4% | 685 B | 0.976:0.024 | 0.998:0.002 | 0.0020 | 0 |
| cluster34 | **9.4 GiB** | **11.3 GiB** | 0.830x | 11,397,717 | 1,866,963,124 | 2,147,047,915 | 7.0% | 882 B | 0.930:0.070 | 0.994:0.006 | 0.0061 | 0 |
| cluster4 | **9.3 GiB** | -- | -- | 16,617,711 | 1,966,071,956 | 3,448,082,328 | 38.7% | 597 B | 0.890:0.110 | 0.992:0.008 | 0.0085 | 0 |
| cluster26 | **8.6 GiB** | **3.2 GiB** | 2.663x | 9,197,252 | 61,282,896 | 156,594,627 | 44.4% | 1,007 B | 0.569:0.431 | 0.850:0.150 | 0.1501 | 0 |
| cluster7 | **7.4 GiB** | **7.4 GiB** | 0.995x | 3,854,808 | 658,146,160 | 1,044,513,039 | 22.7% | 2,049 B | 0.773:0.227 | 0.994:0.006 | 0.0059 | 0 |
| cluster30 | **6.5 GiB** | **3.5 GiB** | 1.826x | 9,015,789 | 2,505,976,895 | 2,554,900,851 | 0.9% | 768 B | 0.989:0.011 | 0.996:0.004 | 0.0036 | 0 |
| cluster39 | **6.1 GiB** | **46.4 GiB** | 0.132x | 81,844,754 | 81,844,754 | 1,329,229,014 | 0.3% | 80 B | 0.062:0.938 | 0.000:1.000 | 1.0000 | 0 |
| cluster54 | **5.6 GiB** | **8.1 GiB** | 0.696x | 33,270,965 | 11,230,782,982 | 11,866,133,317 | 2.3% | 180 B | 0.968:0.032 | 0.997:0.003 | 0.0030 | 0 |
| cluster24 | **5.6 GiB** | **5.1 GiB** | 1.096x | 7,234,762 | 3,235,047,823 | 3,289,457,233 | 0.9% | 830 B | 0.992:0.008 | 0.998:0.002 | 0.0022 | 71 |
| cluster43 | **5.2 GiB** | **10.4 GiB** | 0.501x | 141,969,352 | 3,983,819,373 | 9,038,233,795 | 12.3% | 39 B | 0.470:0.530 | 0.964:0.036 | 0.0356 | 0 |
| cluster49 | **5.2 GiB** | **4.9 GiB** | 1.068x | 1,084,166 | 398,742,932 | 799,295,267 | 12.4% | 5,143 B | 0.537:0.463 | 0.997:0.003 | 0.0027 | 0 |
| cluster42 | **3.3 GiB** | **3.7 GiB** | 0.887x | 5,380,425 | 3,588,668,502 | 3,904,264,364 | 4.2% | 654 B | 0.958:0.042 | 0.999:0.001 | 0.0015 | 0 |
| cluster36 | **3.2 GiB** | **2.6 GiB** | 1.219x | 58,244,445 | 5,183,335,185 | 5,781,263,748 | 3.3% | 59 B | 0.940:0.060 | 0.989:0.011 | 0.0112 | 0 |
| cluster40 | **2.9 GiB** | **6.4 GiB** | 0.459x | 95,422,184 | 1,579,774,374 | 3,847,885,075 | 18.4% | 32 B | 0.452:0.548 | 0.940:0.060 | 0.0604 | 0 |
| cluster45 | **2.9 GiB** | **1.0 GiB** | 2.811x | 25,534,209 | 119,112,869 | 227,773,739 | 38.7% | 120 B | 0.780:0.220 | 0.786:0.214 | 0.2144 | 0 |
| cluster48 | **2.9 GiB** | **8.9 GiB** | 0.321x | 62,232,406 | 565,082,959 | 1,129,939,853 | 22.0% | 49 B | 0.582:0.418 | 0.890:0.110 | 0.1101 | 0 |
| cluster41 | **2.5 GiB** | **5.3 GiB** | 0.470x | 80,391,442 | 1,329,168,009 | 3,214,344,864 | 17.8% | 33 B | 0.454:0.546 | 0.940:0.060 | 0.0605 | 0 |
| cluster11 | **1.9 GiB** | **1.9 GiB** | 1.012x | 13,353,186 | 2,604,983,380 | 2,731,402,190 | 0.4% | 150 B | 0.969:0.031 | 0.995:0.005 | 0.0051 | 0 |
| cluster31 | **1.2 GiB** | **21.5 GiB** | 0.054x | 82,652,551 | 82,652,551 | 1,341,176,720 | 0.3% | 14 B | 0.062:0.938 | 0.000:1.000 | 1.0000 | 0 |
| cluster47 | **1.1 GiB** | 434.0 MiB | 2.553x | 4,761,608 | 6,214,132,754 | 6,225,239,439 | 0.1% | 243 B | 0.999:0.001 | 0.999:0.001 | 0.0008 | 0 |
| cluster17 | **1.1 GiB** | **1.1 GiB** | 0.942x | 3,306,802 | 9,705,671,840 | 9,772,999,164 | 0.0% | 347 B | 0.995:0.005 | 1.000:0.000 | 0.0003 | 0 |
| cluster8 | **1.1 GiB** | 1007.0 MiB | 1.081x | 250,451 | 636,139,234 | 1,302,137,879 | 3.0% | 4,555 B | 0.496:0.504 | 1.000:0.000 | 0.0004 | 0 |
| cluster28 | **1.0 GiB** | **2.8 GiB** | 0.374x | 21,091,069 | 4,808,149,321 | 5,267,932,037 | 0.3% | 53 B | 0.926:0.074 | 0.996:0.004 | 0.0044 | 10,529,394 |
| cluster16 | **1.0 GiB** | **1.1 GiB** | 0.976x | 11,136,125 | 9,181,549,918 | 10,791,885,322 | 8.6% | 99 B | 0.932:0.068 | 0.999:0.001 | 0.0012 | 0 |
| cluster44 | 915.9 MiB | 536.0 MiB | 1.709x | 43,900,742 | 5,552,331,685 | 5,702,089,313 | 0.7% | 21 B | 0.985:0.015 | 0.992:0.008 | 0.0079 | 0 |
| cluster20 | 769.7 MiB | **2.1 GiB** | 0.358x | 17,829,879 | 3,497,689,684 | 3,663,257,915 | 0.9% | 45 B | 0.967:0.033 | 0.995:0.005 | 0.0051 | 0 |
| cluster3 | 525.7 MiB | -- | -- | 4,833,152 | 810,564,898 | 820,307,312 | 0.5% | 114 B | 0.993:0.007 | 0.994:0.006 | 0.0060 | 0 |
| cluster15 | 512.2 MiB | **73.5 GiB** | 0.007x | 5,300,915 | 5,382,244 | 5,263,841,494 | 15.7% | 101 B | 0.001:0.999 | 0.015:0.985 | 0.9849 | 0 |
| cluster51 | 322.5 MiB | 258.0 MiB | 1.250x | 3,631,708 | 5,536,310,793 | 6,322,932,555 | 2.6% | 93 B | 0.897:0.103 | 0.999:0.001 | 0.0007 | 0 |
| cluster9 | 297.4 MiB | 449.0 MiB | 0.662x | 7,987,621 | 10,451,678,798 | 10,646,675,648 | 0.9% | 39 B | 0.991:0.009 | 0.999:0.001 | 0.0008 | 310,184 |
| cluster18 | 240.0 MiB | 311.0 MiB | 0.772x | 7,019,858 | 12,549,781,541 | 13,062,209,198 | 0.2% | 35 B | 0.975:0.025 | 0.999:0.001 | 0.0006 | 338,033 |
| cluster2 | 232.3 MiB | -- | -- | 3,572,298 | 7,200,998,901 | 7,226,679,214 | 0.1% | 68 B | 0.999:0.001 | 1.000:0.000 | 0.0005 | 0 |
| cluster22 | 188.2 MiB | **3.5 GiB** | 0.052x | 10,388,099 | 281,853,093 | 2,030,691,718 | 80.7% | 19 B | 0.584:0.416 | 0.963:0.037 | 0.0369 | 367,318,333 |
| cluster10 | 133.3 MiB | **45.1 GiB** | 0.003x | 55,816 | 57,829 | 139,150,615 | 99.9% | 2,504 B | 0.001:0.999 | 0.035:0.965 | 0.9652 | 0 |
| cluster27 | 125.8 MiB | **10.4 GiB** | 0.012x | 16,492,389 | 72,949,391 | 12,060,815,269 | 99.3% | 8 B | 0.038:0.962 | 0.774:0.226 | 0.2261 | 0 |
| cluster25 | 45.6 MiB | 340.0 MiB | 0.134x | 1,162,641 | 11,502,958,922 | 12,048,484,393 | 0.0% | 41 B | 0.969:0.031 | 1.000:0.000 | 0.0001 | 40,827 |
| cluster1 | 11.7 MiB | -- | -- | 94,296 | 6,393,552,893 | 6,461,081,324 | 0.5% | 129 B | 0.995:0.005 | 1.000:0.000 | 0.0000 | 0 |
| cluster21 | 0.0 MiB | **8.7 GiB** | 0.000x | 0 | 0 | 1,550,193,320 | 0.0% | 0 B | 0.000:1.000 | -- | -- | 0 |

## Object-size distribution

Two weightings, because they answer different questions. **per-ACCESS** weights
every request: what bandwidth and latency see. **per-OBJECT** weights each
distinct key once at its fill size: the population the cache holds, and the one
whose sum is the WSS column above.

Mean object size is a poor summary here and the percentiles show why.
cluster21 has no rows: it has no gets to weight.

| cluster | | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |
|---|---|---|---|---|---|---|---|---|---|
| cluster12 | access | 6 | 6 | 6 | 42 | 6,371 | 9,532 | 24,832 | 46,066 |
|  | object | 6 | 6 | 6 | 42 | 5,629 | 8,629 | 22,757 | 45,148 |
| cluster13 | access | 98 | 123 | 123 | 2,715 | 15,127 | 31,060 | 68,407 | 137,249 |
|  | object | 98 | 123 | 123 | 4,140 | 17,366 | 33,109 | 69,652 | 138,730 |
| cluster23 | access | 6 | 97 | 128 | 180 | 379 | 590 | 998 | 1,954 |
|  | object | 6 | 127 | 129 | 220 | 519 | 631 | 1,082 | 2,364 |
| cluster37 | access | 287 | 586 | 1,254 | 3,265 | 6,596 | 13,409 | 66,454 | 84,531 |
|  | object | 248 | 940 | 1,440 | 4,230 | 7,484 | 14,290 | 70,934 | 120,197 |
| cluster50 | access | 752 | 43,520 | 71,000 | 106,839 | 110,641 | 111,549 | 113,821 | 121,749 |
|  | object | 370 | 32,972 | 66,299 | 81,618 | 109,686 | 110,554 | 112,591 | 118,748 |
| cluster35 | access | 43 | 364 | 1,068 | 3,844 | 5,090 | 5,210 | 5,392 | 5,602 |
|  | object | 42 | 318 | 985 | 3,586 | 5,078 | 5,203 | 5,388 | 5,603 |
| cluster5 | access | 1 | 4 | 77 | 77 | 129 | 317 | 748 | 2,915 |
|  | object | 1 | 77 | 77 | 77 | 77 | 77 | 77 | 733 |
| cluster29 | access | 45 | 147 | 226 | 562 | 1,716 | 2,454 | 3,554 | 8,018 |
|  | object | 47 | 200 | 283 | 429 | 686 | 1,113 | 2,731 | 7,192 |
| cluster52 | access | 27 | 27 | 27 | 402 | 748 | 1,009 | 1,410 | 2,118 |
|  | object | 17 | 27 | 215 | 334 | 527 | 664 | 1,016 | 1,610 |
| cluster33 | access | 1 | 537 | 819 | 1,156 | 1,597 | 1,897 | 3,157 | 7,297 |
|  | object | 1 | 462 | 735 | 1,491 | 3,116 | 4,248 | 7,650 | 8,780 |
| cluster14 | access | 21 | 911 | 1,257 | 1,363 | 1,458 | 1,534 | 1,834 | 3,070 |
|  | object | 21 | 66 | 1,118 | 1,328 | 1,432 | 1,532 | 1,759 | 3,021 |
| cluster38 | access | 30 | 87 | 123 | 204 | 381 | 552 | 1,186 | 2,493 |
|  | object | 30 | 87 | 123 | 204 | 381 | 552 | 1,186 | 2,493 |
| cluster32 | access | 112 | 121 | 146 | 159 | 175 | 182 | 194 | 204 |
|  | object | 112 | 121 | 146 | 159 | 175 | 182 | 194 | 204 |
| cluster46 | access | 19 | 48 | 684 | 889 | 961 | 8,104 | 8,104 | 8,104 |
|  | object | 19 | 19 | 19 | 95 | 243 | 370 | 8,104 | 8,104 |
| cluster19 | access | 49 | 94 | 109 | 123 | 132 | 135 | 140 | 144 |
|  | object | 48 | 90 | 102 | 115 | 126 | 131 | 137 | 142 |
| cluster53 | access | 8 | 136 | 6,864 | 16,764 | 28,842 | 33,066 | 38,016 | 39,468 |
|  | object | 8 | 8 | 400 | 13,398 | 29,634 | 32,934 | 37,290 | 39,369 |
| cluster6 | access | 1 | 632 | 1,338 | 1,372 | 3,883 | 3,966 | 5,976 | 8,555 |
|  | object | 1 | 13 | 13 | 631 | 2,078 | 3,779 | 7,207 | 13,263 |
| cluster34 | access | 1 | 1 | 102 | 109 | 196 | 209 | 243 | 6,924 |
|  | object | 1 | 1 | 1 | 78 | 109 | 1,172 | 16,091 | 159,527 |
| cluster4 | access | 10 | 59 | 745 | 1,795 | 3,226 | 5,001 | 6,595 | 7,228 |
|  | object | 10 | 60 | 568 | 826 | 1,183 | 1,565 | 3,830 | 6,941 |
| cluster26 | access | 15 | 114 | 247 | 602 | 2,529 | 5,685 | 18,719 | 133,040 |
|  | object | 26 | 108 | 226 | 540 | 1,830 | 4,094 | 12,546 | 42,862 |
| cluster7 | access | 1 | 2,103 | 2,264 | 2,571 | 3,797 | 4,316 | 5,238 | 6,085 |
|  | object | 1 | 1,936 | 2,091 | 2,192 | 2,332 | 2,428 | 2,652 | 3,646 |
| cluster30 | access | 306 | 314 | 354 | 378 | 427 | 450 | 514 | 2,496 |
|  | object | 306 | 306 | 322 | 1,048 | 1,414 | 2,577 | 4,148 | 7,640 |
| cluster39 | access | 49 | 49 | 72 | 95 | 127 | 136 | 172 | 314 |
|  | object | 49 | 49 | 72 | 95 | 127 | 136 | 172 | 314 |
| cluster54 | access | 1 | 339 | 339 | 376 | 425 | 436 | 468 | 588 |
|  | object | 1 | 1 | 1 | 339 | 339 | 339 | 431 | 10,338 |
| cluster24 | access | 1 | 503 | 602 | 717 | 914 | 1,446 | 1,664 | 2,252 |
|  | object | 1 | 510 | 669 | 1,386 | 1,574 | 1,662 | 1,942 | 3,637 |
| cluster43 | access | 1 | 160 | 799 | 3,219 | 7,189 | 9,461 | 12,373 | 14,271 |
|  | object | 1 | 1 | 28 | 48 | 48 | 72 | 203 | 2,687 |
| cluster49 | access | 108 | 6,054 | 18,329 | 43,655 | 54,887 | 57,737 | 59,545 | 60,725 |
|  | object | 24 | 75 | 413 | 3,912 | 16,545 | 29,955 | 55,954 | 59,804 |
| cluster42 | access | 30 | 1,328 | 4,268 | 4,268 | 4,268 | 4,268 | 13,526 | 20,810 |
|  | object | 30 | 30 | 30 | 152 | 554 | 1,982 | 16,042 | 21,610 |
| cluster36 | access | 30 | 35 | 35 | 35 | 145 | 214 | 354 | 576 |
|  | object | 30 | 30 | 30 | 30 | 137 | 214 | 434 | 686 |
| cluster40 | access | 1 | 92 | 336 | 1,240 | 4,871 | 24,759 | 27,339 | 28,359 |
|  | object | 1 | 1 | 28 | 48 | 48 | 72 | 136 | 719 |
| cluster45 | access | 77 | 98 | 121 | 125 | 128 | 146 | 216 | 417 |
|  | object | 79 | 116 | 122 | 126 | 128 | 145 | 206 | 469 |
| cluster48 | access | 4 | 4 | 4 | 12 | 76 | 180 | 620 | 1,940 |
|  | object | 4 | 4 | 4 | 36 | 116 | 228 | 660 | 1,860 |
| cluster41 | access | 1 | 92 | 355 | 1,283 | 4,909 | 24,339 | 27,309 | 28,359 |
|  | object | 1 | 1 | 28 | 48 | 48 | 72 | 136 | 843 |
| cluster11 | access | 17 | 17 | 121 | 651 | 1,223 | 1,548 | 3,173 | 7,062 |
|  | object | 17 | 17 | 39 | 129 | 292 | 752 | 1,763 | 3,910 |
| cluster31 | access | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
|  | object | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
| cluster47 | access | 76 | 212 | 329 | 515 | 680 | 801 | 1,010 | 1,341 |
|  | object | 66 | 124 | 193 | 313 | 476 | 584 | 803 | 1,116 |
| cluster17 | access | 26 | 499 | 604 | 717 | 810 | 901 | 1,024 | 1,098 |
|  | object | 17 | 29 | 449 | 509 | 589 | 643 | 758 | 923 |
| cluster8 | access | 436 | 4,368 | 9,412 | 20,849 | 44,767 | 67,280 | 101,036 | 181,194 |
|  | object | 184 | 433 | 519 | 4,503 | 11,108 | 19,178 | 57,468 | 106,864 |
| cluster28 | access | 29 | 186 | 196 | 201 | 211 | 211 | 211 | 683 |
|  | object | 29 | 29 | 29 | 30 | 190 | 202 | 242 | 368 |
| cluster16 | access | 38 | 106 | 108 | 108 | 120 | 141 | 251 | 458 |
|  | object | 30 | 46 | 87 | 108 | 210 | 353 | 437 | 509 |
| cluster44 | access | 18 | 18 | 22 | 22 | 27 | 27 | 27 | 27 |
|  | object | 18 | 22 | 22 | 22 | 22 | 22 | 27 | 27 |
| cluster20 | access | 17 | 17 | 42 | 70 | 75 | 78 | 83 | 96 |
|  | object | 17 | 17 | 41 | 67 | 74 | 77 | 84 | 98 |
| cluster3 | access | 55 | 65 | 82 | 113 | 191 | 281 | 824 | 3,680 |
|  | object | 17 | 65 | 73 | 107 | 171 | 255 | 789 | 3,569 |
| cluster15 | access | 30 | 106 | 108 | 109 | 109 | 109 | 109 | 109 |
|  | object | 30 | 106 | 108 | 109 | 109 | 109 | 109 | 109 |
| cluster51 | access | 58 | 265 | 369 | 475 | 511 | 511 | 649 | 685 |
|  | object | 56 | 58 | 58 | 58 | 162 | 338 | 598 | 667 |
| cluster9 | access | 24 | 24 | 30 | 60 | 130 | 190 | 364 | 522 |
|  | object | 17 | 30 | 30 | 35 | 62 | 89 | 148 | 190 |
| cluster18 | access | 17 | 28 | 31 | 37 | 97 | 178 | 423 | 1,428 |
|  | object | 17 | 26 | 28 | 34 | 39 | 107 | 177 | 243 |
| cluster2 | access | 30 | 30 | 78 | 91 | 101 | 113 | 121 | 11,067 |
|  | object | 30 | 30 | 75 | 88 | 97 | 103 | 116 | 130 |
| cluster22 | access | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
|  | object | 19 | 19 | 19 | 19 | 19 | 19 | 19 | 19 |
| cluster10 | access | 30 | 3,170 | 3,224 | 3,295 | 3,390 | 3,468 | 3,536 | 3,590 |
|  | object | 30 | 3,170 | 3,224 | 3,295 | 3,389 | 3,468 | 3,536 | 3,590 |
| cluster27 | access | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 |
|  | object | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 |
| cluster25 | access | 1 | 1 | 1 | 1 | 60 | 258 | 971 | 1,425 |
|  | object | 1 | 17 | 17 | 17 | 94 | 120 | 389 | 1,069 |
| cluster1 | access | 30 | 311 | 335 | 431 | 503 | 527 | 671 | 767 |
|  | object | 30 | 30 | 30 | 311 | 335 | 359 | 455 | 551 |
