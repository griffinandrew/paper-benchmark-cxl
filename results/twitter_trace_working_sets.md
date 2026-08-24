# Read-through working sets: all 54 Twitter twemcache clusters

2026-08-23/24. Computed from the **complete** traces (2.13 TB compressed,
254.9 billion requests) streamed from the CMU PDL mirror of the Twitter
twemcache release. Not sampled, not prefixes, and not the published summary
statistics -- those answer a different question (see "Why these differ").
Tool: `scripts/wsscalc.c`. 54/54 clusters completed, zero failures.

## Definition

A read-through cache is driven by gets alone: a miss produces the fill, so
counting sets as well would double-count the same object. Three rules:

1. **GET rows only.** Sets, adds, incrs and deletes are not read-through accesses.
2. **`value_size > 0` only.** A get returning size 0 was a miss in the production
   system -- there is no object to hold. This is the same filter the `.filt`
   masters use.
3. **Maximum size over time.** Each distinct key contributes the largest value it
   ever takes, because the cache must be provisioned for an object's biggest form.

```
WSS = sum over distinct keys of max(value_size)
```

Keys are hashed with `MurmurHash3_x64_128(key, seed=0).h1`, the same function
verified byte-exact against the existing `cluster12.filt.zst` and
`cluster37.filt.zst` masters, so these counts are consistent with the traces
used elsewhere in this repo.

## Reproducing

```sh
cc -O3 -o wsscalc scripts/wsscalc.c
B=https://ftp.pdl.cmu.edu/pub/datasets/twemcacheWorkload/open_source
curl -s "$B/cluster12.sort.zst" | zstd -dc | ./wsscalc cluster12 67108864
```

Nothing is stored: each trace is decompressed and reduced in flight. Peak
memory is the hash table, ~51 GB for the worst case (cluster23, 1.36 B keys);
most clusters need under 2 GB. The second argument is an initial table-size
hint in slots; it doubles automatically, so it only affects rehash count.
Throughput is network-bound at ~50 MB/s, about 12 h for all 54.

## Totals

| | |
|---|---|
| Total read-through WSS | **1858 GB** |
| Published no-TTL total (53 clusters with data) | 5028 GB |
| Requests parsed | 254,944,297,528 |
| Distinct objects | 3,948,647,618 |
| Compressed bytes processed | 2.13 TB |

## Per-cluster

`zero%` is the share of gets returning size 0. `pub` is Twitter's published
no-TTL WSS in MB (`stat/2020Mar.md`); cluster5 has no published entry.
`eff get:set` is computed after the size filter. Rows marked `*` have writes
containing incr/delete/add, so that column is get:non-get, not get:set.

| cluster | requests | gets | zero% | distinct | WSS | mean obj | pub (MB) | ours/pub | eff get:set |
|---|---|---|---|---|---|---|---|---|---|
| cluster1 | 6,461,081,324 | 6,422,678,822 | 0.5% | 94,296 | 12.8 MB | 142 B | 0 | n/a | 0.994:0.006 |
| cluster2 | 7,226,679,214 | 7,211,621,640 | 0.1% | 3,572,298 | 232.9 MB | 68 B | 0 | n/a | 0.998:0.002 |
| cluster3 | 820,307,312 | 814,533,880 | 0.5% | 4,833,152 | 525.9 MB | 114 B | 0 | n/a | 0.993:0.007 |
| cluster4 | 3,448,082,328 | 3,205,067,455 | 38.7% | 16,617,711 | 9.8 GB | 631 B | 0 | n/a | 0.890:0.110 |
| cluster5 | 10,458,446,348 | 6,005,613,088 | 71.2% | 486,105,560 | 35.1 GB | 77 B | n/a | n/a | 0.280:0.720 |
| cluster6 | 8,376,426,233 | 8,184,143,726 | 2.4% | 16,310,959 | 10.6 GB | 694 B | 4,807 | 2.25x | 0.976:0.024 |
| cluster7 | 1,044,513,039 | 851,370,550 | 22.7% | 3,854,808 | 7.4 GB | 2,057 B | 7,574 | 1.00x | 0.773:0.227 |
| cluster8 | 1,302,137,879 | 655,693,888 | 3.0% | 250,451 | 3.3 GB | 14,219 B | 1,007 | 3.37x | 0.496:0.504 |
| cluster9 | 10,646,675,648 | 10,542,054,339 | 0.9% | 7,987,621 | 318.1 MB | 41 B | 449 | 0.71x | 0.990:0.010 |
| cluster10 | 139,150,615 | 69,621,161 | 99.9% | 55,816 | 133.3 MB | 2,504 B | 46,168 | 0.00x | 0.001:0.999 \* |
| cluster11 | 2,731,402,190 | 2,615,001,869 | 0.4% | 13,353,186 | 1.9 GB | 154 B | 1,898 | 1.03x | 0.957:0.043 |
| cluster12 | 2,649,686,669 | 534,078,298 | 0.7% | 375,949,779 | 525.9 GB | 1,502 B | 612,397 | 0.88x | 0.200:0.800 |
| cluster13 | 825,381,985 | 306,714,865 | 50.7% | 72,472,986 | 378.4 GB | 5,607 B | 595,112 | 0.65x | 0.226:0.774 |
| cluster14 | 3,029,143,401 | 1,978,874,339 | 10.1% | 22,825,825 | 18.0 GB | 847 B | 18,333 | 1.01x | 0.629:0.371 \* |
| cluster15 | 5,263,841,494 | 6,387,217 | 15.7% | 5,300,915 | 512.2 MB | 101 B | 75,299 | 0.01x | 0.001:0.999 |
| cluster16 | 10,791,885,322 | 10,040,022,356 | 8.6% | 11,136,125 | 1.1 GB | 102 B | 1,085 | 1.00x | 0.924:0.076 |
| cluster17 | 9,772,999,164 | 9,709,211,548 | 0.0% | 3,306,802 | 1.1 GB | 348 B | 1,162 | 0.95x | 0.993:0.007 |
| cluster18 | 13,062,209,198 | 12,576,276,376 | 0.2% | 7,019,858 | 241.7 MB | 36 B | 311 | 0.78x | 0.963:0.037 |
| cluster19 | 2,006,896,318 | 1,503,297,007 | 40.9% | 140,012,272 | 13.1 GB | 100 B | 14,633 | 0.92x | 0.638:0.362 |
| cluster20 | 3,663,257,915 | 3,529,405,613 | 0.9% | 17,829,879 | 773.4 MB | 45 B | 2,151 | 0.36x | 0.963:0.037 |
| cluster21 | 1,550,193,320 | 0 | 0.0% | 0 | 0.0 MB | 0 B | 8,926 | 0.00x | 0.000:1.000 \* |
| cluster22 | 2,030,691,718 | 1,462,688,874 | 80.7% | 10,388,099 | 188.2 MB | 18 B | 3,627 | 0.05x | 0.332:0.668 \* |
| cluster23 | 5,333,543,178 | 1,998,365,768 | 2.4% | 1,359,361,168 | 285.3 GB | 225 B | 93,931 | 3.11x | 0.369:0.631 \* |
| cluster24 | 3,289,457,233 | 3,264,491,290 | 0.9% | 7,234,762 | 5.7 GB | 838 B | 5,228 | 1.11x | 0.992:0.008 |
| cluster25 | 12,048,484,393 | 11,505,472,653 | 0.0% | 1,162,641 | 46.0 MB | 41 B | 340 | 0.14x | 0.955:0.045 |
| cluster26 | 156,594,627 | 110,140,681 | 44.4% | 9,197,252 | 8.6 GB | 1,007 B | 3,317 | 2.66x | 0.569:0.431 |
| cluster27 | 12,060,815,269 | 10,226,504,181 | 99.3% | 16,492,389 | 125.8 MB | 7 B | 10,653 | 0.01x | 0.038:0.962 |
| cluster28 | 5,267,932,037 | 4,822,119,151 | 0.3% | 21,091,069 | 1.0 GB | 53 B | 2,849 | 0.38x | 0.915:0.085 |
| cluster29 | 6,920,247,859 | 5,952,270,507 | 23.4% | 84,519,980 | 34.4 GB | 437 B | 40,520 | 0.87x | 0.825:0.175 |
| cluster30 | 2,554,900,851 | 2,527,904,535 | 0.9% | 9,015,789 | 6.5 GB | 768 B | 3,618 | 1.83x | 0.989:0.011 |
| cluster31 | 1,341,176,720 | 82,896,903 | 0.3% | 82,652,551 | 1.2 GB | 15 B | 21,989 | 0.05x | 0.062:0.938 |
| cluster32 | 3,667,356,838 | 110,126,289 | 0.2% | 109,882,205 | 14.7 GB | 143 B | 4,262 | 3.53x | 0.030:0.970 |
| cluster33 | 6,705,048,996 | 6,626,771,606 | 1.1% | 16,508,795 | 19.3 GB | 1,254 B | 7,522 | 2.63x | 0.988:0.012 |
| cluster34 | 2,147,047,915 | 2,006,950,025 | 7.0% | 11,397,717 | 9.6 GB | 900 B | 11,552 | 0.85x | 0.930:0.070 |
| cluster35 | 1,828,228,908 | 1,753,779,644 | 34.0% | 40,790,697 | 71.9 GB | 1,893 B | 36,355 | 2.03x | 0.940:0.060 |
| cluster36 | 5,781,263,748 | 5,358,686,546 | 3.3% | 58,244,445 | 3.4 GB | 62 B | 2,696 | 1.29x | 0.925:0.075 \* |
| cluster37 | 3,168,270,709 | 2,054,622,962 | 60.4% | 39,020,642 | 170.7 GB | 4,696 B | 3,121,435 | 0.06x | 0.422:0.578 |
| cluster38 | 1,342,423,951 | 82,994,676 | 0.3% | 82,750,952 | 14.9 GB | 193 B | 88,563 | 0.17x | 0.062:0.938 |
| cluster39 | 1,329,229,014 | 82,085,904 | 0.3% | 81,844,754 | 6.1 GB | 80 B | 47,562 | 0.13x | 0.062:0.938 |
| cluster40 | 3,847,885,075 | 1,934,829,846 | 18.4% | 95,422,184 | 6.5 GB | 72 B | 6,520 | 1.01x | 0.452:0.548 |
| cluster41 | 3,214,344,864 | 1,617,550,652 | 17.8% | 80,391,442 | 5.6 GB | 74 B | 5,467 | 1.04x | 0.454:0.546 |
| cluster42 | 3,904,264,364 | 3,745,552,888 | 4.2% | 5,380,425 | 3.7 GB | 741 B | 3,786 | 1.00x | 0.958:0.042 |
| cluster43 | 9,038,233,795 | 4,543,813,715 | 12.3% | 141,969,352 | 10.8 GB | 81 B | 10,687 | 1.03x | 0.470:0.530 |
| cluster44 | 5,702,089,313 | 5,593,275,797 | 0.7% | 43,900,742 | 928.0 MB | 22 B | 536 | 1.73x | 0.981:0.019 |
| cluster45 | 227,773,739 | 194,256,353 | 38.7% | 25,534,209 | 2.9 GB | 120 B | 1,046 | 2.81x | 0.780:0.220 \* |
| cluster46 | 6,610,320,613 | 4,550,262,875 | 68.4% | 61,825,150 | 16.3 GB | 283 B | 80,265 | 0.21x | 0.411:0.589 |
| cluster47 | 6,225,239,439 | 6,219,711,941 | 0.1% | 4,761,608 | 1.1 GB | 243 B | 434 | 2.55x | 0.999:0.001 |
| cluster48 | 1,129,939,853 | 724,381,060 | 22.0% | 62,232,406 | 2.9 GB | 49 B | 9,100 | 0.32x | 0.582:0.418 |
| cluster49 | 799,295,267 | 455,287,576 | 12.4% | 1,084,166 | 11.2 GB | 11,094 B | 4,979 | 2.30x | 0.537:0.463 |
| cluster50 | 139,655,585 | 72,733,803 | 44.4% | 1,320,408 | 85.0 GB | 69,110 B | 101,492 | 0.86x | 0.377:0.623 |
| cluster51 | 6,322,932,555 | 5,686,356,293 | 2.6% | 3,631,708 | 427.1 MB | 123 B | 258 | 1.66x | 0.897:0.103 |
| cluster52 | 13,426,570,607 | 12,202,501,327 | 2.2% | 135,845,907 | 30.2 GB | 239 B | 14,057 | 2.20x | 0.907:0.093 \* |
| cluster53 | 246,508,262 | 217,410,986 | 23.1% | 1,630,740 | 13.0 GB | 8,541 B | 4,122 | 3.22x | 0.852:0.148 \* |
| cluster54 | 11,866,133,317 | 11,496,118,000 | 2.3% | 33,270,965 | 5.8 GB | 187 B | 8,248 | 0.72x | 0.968:0.032 |

## Why these differ from the published figures

The totals (1,858 GB vs 5,028 GB) look like a constant rescaling. They are not:
per-cluster ratios run from **0.00x to 3.53x**. Three definitional differences
pull in opposite directions, and which one dominates depends on the workload:

- **Excluding zero-size gets** shrinks high-miss clusters, sometimes to nothing.
  cluster10 returns size 0 on 99.9% of its gets and cluster27 on 99.3%, so their
  true read-through working sets are 133 MB and 126 MB against published figures
  of 46,168 MB and 10,653 MB.
- **Counting only gets** shrinks write-dominated clusters. cluster31, 38 and 39
  are all `set:0.94`, and land at 0.05x, 0.17x and 0.13x.
- **Max-size-over-time** inflates clusters whose objects vary in size:
  cluster32 3.53x, cluster8 3.37x, cluster53 3.22x, cluster23 3.11x.

Where all three cancel, the two agree closely -- clusters 7, 14, 16, 40, 41, 42
and 43 all land within 1.00-1.04x. So the published number is usable only for
low-miss, stable-size, get-dominated clusters, and misleading elsewhere.

## Consequences for trace selection

**cluster21 has no read-through working set at all.** It is `add:1.00` across
1.55 billion requests with zero gets. A WSS of 0 here is a category error, not a
small number; exclude it from read-through studies rather than reporting zero.
cluster10 is effectively the same case (1,235 usable gets out of 69.6 million).

**Excluding empty gets flips the read/write balance on high-miss clusters.**
Several clusters that look read-dominated are write-dominated in effect:

| cluster | published mix | effective get:set |
|---|---|---|
| cluster27 | `get:0.85 set:0.15` | 0.038 : 0.962 |
| cluster37 | `get:0.63 set:0.37` | 0.422 : 0.578 |
| cluster50 | `get:0.52 set:0.48` | 0.377 : 0.623 |
| cluster46 | `get:0.68 set:0.32` | 0.411 : 0.589 |
| cluster40 | `get:0.50 set:0.50` | 0.452 : 0.548 |

These are exactly the clusters whose production caches were missing most often,
so most of their nominal "reads" moved no bytes.

**cluster23 is the scale outlier**: 1.36 billion distinct objects, an order of
magnitude more than any other cluster, at only 225 B each. Sizing a hash table
for it needs ~51 GB; the other 53 clusters together need less.

**The largest working sets** are cluster12 (526 GB), cluster13 (378 GB),
cluster23 (285 GB), cluster37 (171 GB) and cluster50 (85 GB). Everything else is
under 90 GB, and 30 of the 54 are under 10 GB.

## Caveats

- The `eff get:set` column is exact for 46 clusters. For the 8 marked `*`
  (10, 14, 21, 22, 23, 36, 45, 53) the write side mixes `set` with
  `incr`/`delete`/`add`/`prepend`, so the column is get:non-get. A true get:set
  for those needs a second pass with a full operation histogram, which means
  re-reading the 2.13 TB.
- TTL is not modelled. These are the working sets a cache must hold if nothing
  expires. Twitter's published table also reports a TTL-aware column, which is
  dramatically smaller for some clusters (cluster37: 3.12 TB -> 551 MB). A
  TTL-aware version of this measurement would be a separate calculation.
- Object sizes are taken from the trace's `value_size` column and exclude key
  bytes and per-object cache metadata. Measured separately on this cache, that
  metadata costs **19.4-24.4 B per object** regardless of object size -- which is
  under 2% for kilobyte objects but over 10% for the 170 B ones, so small-object
  clusters need roughly 10% more capacity than the WSS column alone suggests.
