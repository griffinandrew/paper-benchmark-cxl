# Metadata overhead across all 53 Twitter clusters

**On 28 of 53 clusters — a majority — the cache's own bookkeeping is larger than
the data it describes.** Fleet-wide the ratio is 41%: 735.5 GiB of metadata to
hold 1.8 TiB of values, under LRU.

This is arithmetic, not measurement: per-object metadata from
`get_policy_overhead` in `src/object/overhead.rs`, working-set sizes and
distinct-object counts from `twitter_trace_working_sets.md`. Nothing here was
re-run.

## The one number that matters

`meta/WSS` is metadata bytes over working-set bytes if the whole working set
were resident. It reduces exactly to

```
meta/WSS  =  per-object metadata / mean object size
```

so it is a property of **object size alone**. It does not depend on cache size,
on hit rate, or on which cache you run — which is why it transfers, and why a
single column can characterise a whole fleet.

Mean object size across these 53 clusters spans **8 B to 60,095 B**, four
orders of magnitude. That range, not the policy, is what decides whether
metadata matters.

## Findings

**1. The distribution is bimodal, not centred.** 28 clusters (53%) are
metadata-bound at over 100%; 6 (11%) are object-bound under 5%. Very little
sits in between. A cache design is effectively serving two different workloads.

**2. There is a hard floor no eviction-policy work can reach past.** Every
policy pays a fixed **144 B/object** — the `Arc` header (48) plus the `DashMap`
entry (96). Across the fleet that floor alone is **529.6 GiB, 29% of WSS**. The
entire spread between the cheapest and most expensive policy is 200 B to 228 B,
so *policy choice moves total metadata by at most 14%*. Halving the eviction
stack, which the compact designs do, moves the total by 12–14%. **Further
progress has to come from the object map, not the policy.**

**3. Compaction is worth 103.0 GiB fleet-wide on LRU and 191.2 GiB on LFU** —
LFU roughly double, because its original is the worst offender (84 B of stack
across three structures, against LRU's 56).

**4. The two traces this project benchmarks are the two where none of this
shows up.** cluster13 (4%) and cluster53 (2%) both sit in the object-bound
bottom six. They were chosen for working-set size and reuse structure, and they
are precisely the regime where compact stacks cannot demonstrate a capacity
benefit — which matches the sweep, where compaction wins 1–3% on latency and
essentially nothing on object count.

**cluster19 (201%) and cluster38 (103%) are already on disk and have never been
run.** They are where this work would actually pay, and running them is the
single highest-value measurement outstanding.

**5. At the extreme the design breaks down entirely.** cluster27's mean object
is **8 bytes**. Its metadata is **2,501% of its data** — 25 GiB of bookkeeping
per GiB cached. No eviction policy is relevant at that size; the object model
itself is wrong for the workload.


Per-object metadata is `get_policy_overhead` — the eviction-stack term plus
a **fixed 144 B** (`Arc` header 48 + `DashMap` entry 96) that every policy pays
and no eviction-stack work can touch. WSS and distinct-object counts come from
`twitter_trace_working_sets.md`; nothing here is re-measured, it is arithmetic
over that table and the terms in `src/object/overhead.rs`.

`meta/WSS` is metadata bytes divided by working-set bytes if the ENTIRE working
set were resident. Above 100% means the bookkeeping outweighs the data it
describes. It is equivalently `per-object metadata / mean object size`, so it is
a property of object size alone — which is why it transfers to any cache size.

## Per-object metadata by policy

| policy | eviction stack | Arc + map | total | vs LRU |
|---|--:|--:|--:|--:|
| Lru | 56 | 144 | **200 B** | +0.0% |
| LruCompact | 28 | 144 | **172 B** | -14.0% |
| Lfu | 84 | 144 | **228 B** | +14.0% |
| LfuCompact | 32 | 144 | **176 B** | -12.0% |
| Fifo | 56 | 144 | **200 B** | +0.0% |
| FifoCompact | 28 | 144 | **172 B** | -14.0% |
| Clock | 57 | 144 | **201 B** | +0.5% |
| ClockCompact | 29 | 144 | **173 B** | -13.5% |
| Sieve | 57 | 144 | **201 B** | +0.5% |
| SieveCompact | 29 | 144 | **173 B** | -13.5% |
| Mru | 56 | 144 | **200 B** | +0.0% |
| MruCompact | 28 | 144 | **172 B** | -14.0% |
| TwoQ | 60 | 144 | **204 B** | +2.0% |
| TwoQCompact | 36 | 144 | **180 B** | -10.0% |
| SThreeFifo | 61 | 144 | **205 B** | +2.5% |
| SThreeFifoCompact | 36 | 144 | **180 B** | -10.0% |
| Arc | 60 | 144 | **204 B** | +2.0% |

The floor is 144 B/object for every policy. Compaction moves the total by at
most 14% (LFU 228 -> 176) even though the stack itself halves.

## Every cluster, LRU and LRU-compact, sorted by metadata burden

| cluster | WSS | distinct | mean obj | LRU meta | meta/WSS | compact meta | meta/WSS | saved |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| cluster27 | 125.8 MiB | 16,492,389 | 8 B | 3.1 GiB | **2501%** | 2.6 GiB | 2150% | 440.4 MiB |
| cluster31 | 1.2 GiB | 82,652,551 | 16 B | 15.4 GiB | **1283%** | 13.2 GiB | 1103% | 2.2 GiB |
| cluster22 | 188.2 MiB | 10,388,099 | 19 B | 1.9 GiB | **1053%** | 1.7 GiB | 905% | 277.4 MiB |
| cluster44 | 915.9 MiB | 43,900,742 | 22 B | 8.2 GiB | **914%** | 7.0 GiB | 786% | 1.1 GiB |
| cluster40 | 2.9 GiB | 95,422,184 | 33 B | 17.8 GiB | **613%** | 15.3 GiB | 527% | 2.5 GiB |
| cluster41 | 2.5 GiB | 80,391,442 | 33 B | 15.0 GiB | **599%** | 12.9 GiB | 515% | 2.1 GiB |
| cluster18 | 240.0 MiB | 7,019,858 | 36 B | 1.3 GiB | **558%** | 1.1 GiB | 480% | 187.5 MiB |
| cluster9 | 297.4 MiB | 7,987,621 | 39 B | 1.5 GiB | **512%** | 1.3 GiB | 441% | 213.3 MiB |
| cluster43 | 5.2 GiB | 141,969,352 | 39 B | 26.4 GiB | **509%** | 22.7 GiB | 437% | 3.7 GiB |
| cluster25 | 45.6 MiB | 1,162,641 | 41 B | 221.8 MiB | **486%** | 190.7 MiB | 418% | 31.0 MiB |
| cluster20 | 769.7 MiB | 17,829,879 | 45 B | 3.3 GiB | **442%** | 2.9 GiB | 380% | 476.1 MiB |
| cluster48 | 2.9 GiB | 62,232,406 | 50 B | 11.6 GiB | **400%** | 10.0 GiB | 344% | 1.6 GiB |
| cluster28 | 1.0 GiB | 21,091,069 | 51 B | 3.9 GiB | **393%** | 3.4 GiB | 338% | 563.2 MiB |
| cluster36 | 3.2 GiB | 58,244,445 | 59 B | 10.8 GiB | **339%** | 9.3 GiB | 292% | 1.5 GiB |
| cluster2 | 232.3 MiB | 3,572,298 | 68 B | 681.4 MiB | **293%** | 586.0 MiB | 252% | 95.4 MiB |
| cluster5 | 34.8 GiB | 486,105,560 | 77 B | 90.5 GiB | **260%** | 77.9 GiB | 224% | 12.7 GiB |
| cluster39 | 6.1 GiB | 81,844,754 | 80 B | 15.2 GiB | **250%** | 13.1 GiB | 215% | 2.1 GiB |
| cluster51 | 322.5 MiB | 3,631,708 | 93 B | 692.7 MiB | **215%** | 595.7 MiB | 185% | 97.0 MiB |
| cluster16 | 1.0 GiB | 11,136,125 | 96 B | 2.1 GiB | **207%** | 1.8 GiB | 178% | 297.4 MiB |
| cluster19 | 13.0 GiB | 140,012,272 | 100 B | 26.1 GiB | **201%** | 22.4 GiB | 173% | 3.7 GiB |
| cluster15 | 512.2 MiB | 5,300,915 | 101 B | 1011.1 MiB | **197%** | 869.5 MiB | 170% | 141.5 MiB |
| cluster3 | 525.7 MiB | 4,833,152 | 114 B | 921.9 MiB | **175%** | 792.8 MiB | 151% | 129.1 MiB |
| cluster45 | 2.9 GiB | 25,534,209 | 122 B | 4.8 GiB | **164%** | 4.1 GiB | 141% | 681.8 MiB |
| cluster1 | 11.7 MiB | 94,296 | 130 B | 18.0 MiB | **154%** | 15.5 MiB | 132% | 2.5 MiB |
| cluster32 | 14.7 GiB | 109,882,205 | 144 B | 20.5 GiB | **139%** | 17.6 GiB | 120% | 2.9 GiB |
| cluster11 | 1.9 GiB | 13,353,186 | 153 B | 2.5 GiB | **131%** | 2.1 GiB | 113% | 356.6 MiB |
| cluster54 | 5.6 GiB | 33,270,965 | 181 B | 6.2 GiB | **111%** | 5.3 GiB | 95% | 888.4 MiB |
| cluster38 | 14.9 GiB | 82,750,952 | 193 B | 15.4 GiB | **103%** | 13.3 GiB | 89% | 2.2 GiB |
| cluster23 | 285.1 GiB | 1,359,361,168 | 225 B | 253.2 GiB | **89%** | 217.8 GiB | 76% | 35.4 GiB |
| cluster52 | 29.7 GiB | 135,845,907 | 235 B | 25.3 GiB | **85%** | 21.8 GiB | 73% | 3.5 GiB |
| cluster47 | 1.1 GiB | 4,761,608 | 248 B | 908.2 MiB | **81%** | 781.1 MiB | 69% | 127.1 MiB |
| cluster46 | 14.3 GiB | 61,825,150 | 248 B | 11.5 GiB | **81%** | 9.9 GiB | 69% | 1.6 GiB |
| cluster17 | 1.1 GiB | 3,306,802 | 357 B | 630.7 MiB | **56%** | 542.4 MiB | 48% | 88.3 MiB |
| cluster29 | 32.8 GiB | 84,519,980 | 417 B | 15.7 GiB | **48%** | 13.5 GiB | 41% | 2.2 GiB |
| cluster4 | 9.3 GiB | 16,617,711 | 601 B | 3.1 GiB | **33%** | 2.7 GiB | 29% | 443.7 MiB |
| cluster42 | 3.3 GiB | 5,380,425 | 659 B | 1.0 GiB | **30%** | 882.6 MiB | 26% | 143.7 MiB |
| cluster6 | 10.4 GiB | 16,310,959 | 685 B | 3.0 GiB | **29%** | 2.6 GiB | 25% | 435.5 MiB |
| cluster30 | 6.5 GiB | 9,015,789 | 774 B | 1.7 GiB | **26%** | 1.4 GiB | 22% | 240.7 MiB |
| cluster24 | 5.6 GiB | 7,234,762 | 831 B | 1.3 GiB | **24%** | 1.2 GiB | 21% | 193.2 MiB |
| cluster14 | 18.0 GiB | 22,825,825 | 847 B | 4.3 GiB | **24%** | 3.7 GiB | 20% | 609.5 MiB |
| cluster34 | 9.4 GiB | 11,397,717 | 886 B | 2.1 GiB | **23%** | 1.8 GiB | 19% | 304.4 MiB |
| cluster26 | 8.6 GiB | 9,197,252 | 1004 B | 1.7 GiB | **20%** | 1.5 GiB | 17% | 245.6 MiB |
| cluster33 | 19.2 GiB | 16,508,795 | 1249 B | 3.1 GiB | **16%** | 2.6 GiB | 14% | 440.8 MiB |
| cluster12 | 525.8 GiB | 375,949,779 | 1502 B | 70.0 GiB | **13%** | 60.2 GiB | 11% | 9.8 GiB |
| cluster35 | 71.9 GiB | 40,790,697 | 1893 B | 7.6 GiB | **11%** | 6.5 GiB | 9% | 1.1 GiB |
| cluster7 | 7.4 GiB | 3,854,808 | 2061 B | 735.2 MiB | **10%** | 632.3 MiB | 8% | 102.9 MiB |
| cluster10 | 133.3 MiB | 55,816 | 2504 B | 10.6 MiB | **8%** | 9.2 MiB | 7% | 1.5 MiB |
| cluster37 | 165.4 GiB | 39,020,642 | 4551 B | 7.3 GiB | **4%** | 6.3 GiB | 4% | 1.0 GiB |
| cluster8 | 1.1 GiB | 250,451 | 4716 B | 47.8 MiB | **4%** | 41.1 MiB | 4% | 6.7 MiB |
| cluster49 | 5.2 GiB | 1,084,166 | 5150 B | 206.8 MiB | **4%** | 177.8 MiB | 3% | 29.0 MiB |
| cluster13 | 378.4 GiB | 72,472,986 | 5606 B | 13.5 GiB | **4%** | 11.6 GiB | 3% | 1.9 GiB |
| cluster53 | 12.7 GiB | 1,630,740 | 8362 B | 311.0 MiB | **2%** | 267.5 MiB | 2% | 43.5 MiB |
| cluster50 | 73.9 GiB | 1,320,408 | 60095 B | 251.8 MiB | **0%** | 216.6 MiB | 0% | 35.3 MiB |

## meta/WSS for every policy

| cluster | mean obj | Lru | LruCompact | Lfu | LfuCompact | Fifo | FifoCompact | Clock | ClockCompact | Sieve | SieveCompact | Mru | MruCompact | TwoQ | TwoQCompact | SThreeFifo | SThreeFifoCompact | Arc |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| cluster27 | 8 B | 2501% | 2150% | 2851% | 2200% | 2501% | 2150% | 2513% | 2163% | 2513% | 2163% | 2501% | 2150% | 2551% | 2250% | 2563% | 2250% | 2551% |
| cluster31 | 16 B | 1283% | 1103% | 1463% | 1129% | 1283% | 1103% | 1289% | 1110% | 1289% | 1110% | 1283% | 1103% | 1309% | 1155% | 1315% | 1155% | 1309% |
| cluster22 | 19 B | 1053% | 905% | 1200% | 926% | 1053% | 905% | 1058% | 911% | 1058% | 911% | 1053% | 905% | 1074% | 948% | 1079% | 948% | 1074% |
| cluster44 | 22 B | 914% | 786% | 1042% | 805% | 914% | 786% | 919% | 791% | 919% | 791% | 914% | 786% | 933% | 823% | 937% | 823% | 933% |
| cluster40 | 33 B | 613% | 527% | 699% | 539% | 613% | 527% | 616% | 530% | 616% | 530% | 613% | 527% | 625% | 552% | 628% | 552% | 625% |
| cluster41 | 33 B | 599% | 515% | 683% | 527% | 599% | 515% | 602% | 518% | 602% | 518% | 599% | 515% | 611% | 539% | 614% | 539% | 611% |
| cluster18 | 36 B | 558% | 480% | 636% | 491% | 558% | 480% | 561% | 483% | 561% | 483% | 558% | 480% | 569% | 502% | 572% | 502% | 569% |
| cluster9 | 39 B | 512% | 441% | 584% | 451% | 512% | 441% | 515% | 443% | 515% | 443% | 512% | 441% | 523% | 461% | 525% | 461% | 523% |
| cluster43 | 39 B | 509% | 437% | 580% | 448% | 509% | 437% | 511% | 440% | 511% | 440% | 509% | 437% | 519% | 458% | 521% | 458% | 519% |
| cluster25 | 41 B | 486% | 418% | 554% | 428% | 486% | 418% | 489% | 421% | 489% | 421% | 486% | 418% | 496% | 438% | 498% | 438% | 496% |
| cluster20 | 45 B | 442% | 380% | 504% | 389% | 442% | 380% | 444% | 382% | 444% | 382% | 442% | 380% | 451% | 398% | 453% | 398% | 451% |
| cluster48 | 50 B | 400% | 344% | 456% | 352% | 400% | 344% | 402% | 346% | 402% | 346% | 400% | 344% | 408% | 360% | 410% | 360% | 408% |
| cluster28 | 51 B | 393% | 338% | 448% | 346% | 393% | 338% | 395% | 340% | 395% | 340% | 393% | 338% | 401% | 354% | 403% | 354% | 401% |
| cluster36 | 59 B | 339% | 292% | 386% | 298% | 339% | 292% | 341% | 293% | 341% | 293% | 339% | 292% | 346% | 305% | 348% | 305% | 346% |
| cluster2 | 68 B | 293% | 252% | 334% | 258% | 293% | 252% | 295% | 254% | 295% | 254% | 293% | 252% | 299% | 264% | 301% | 264% | 299% |
| cluster5 | 77 B | 260% | 224% | 297% | 229% | 260% | 224% | 261% | 225% | 261% | 225% | 260% | 224% | 265% | 234% | 267% | 234% | 265% |
| cluster39 | 80 B | 250% | 215% | 285% | 220% | 250% | 215% | 251% | 216% | 251% | 216% | 250% | 215% | 255% | 225% | 256% | 225% | 255% |
| cluster51 | 93 B | 215% | 185% | 245% | 189% | 215% | 185% | 216% | 186% | 216% | 186% | 215% | 185% | 219% | 193% | 220% | 193% | 219% |
| cluster16 | 96 B | 207% | 178% | 236% | 183% | 207% | 178% | 208% | 179% | 208% | 179% | 207% | 178% | 212% | 187% | 213% | 187% | 212% |
| cluster19 | 100 B | 201% | 173% | 229% | 177% | 201% | 173% | 202% | 174% | 202% | 174% | 201% | 173% | 205% | 181% | 206% | 181% | 205% |
| cluster15 | 101 B | 197% | 170% | 225% | 174% | 197% | 170% | 198% | 171% | 198% | 171% | 197% | 170% | 201% | 178% | 202% | 178% | 201% |
| cluster3 | 114 B | 175% | 151% | 200% | 154% | 175% | 151% | 176% | 152% | 176% | 152% | 175% | 151% | 179% | 158% | 180% | 158% | 179% |
| cluster45 | 122 B | 164% | 141% | 187% | 144% | 164% | 141% | 165% | 142% | 165% | 142% | 164% | 141% | 167% | 148% | 168% | 148% | 167% |
| cluster1 | 130 B | 154% | 132% | 175% | 135% | 154% | 132% | 154% | 133% | 154% | 133% | 154% | 132% | 157% | 138% | 158% | 138% | 157% |
| cluster32 | 144 B | 139% | 120% | 159% | 123% | 139% | 120% | 140% | 120% | 140% | 120% | 139% | 120% | 142% | 125% | 143% | 125% | 142% |
| cluster11 | 153 B | 131% | 113% | 149% | 115% | 131% | 113% | 132% | 113% | 132% | 113% | 131% | 113% | 134% | 118% | 134% | 118% | 134% |
| cluster54 | 181 B | 111% | 95% | 126% | 97% | 111% | 95% | 111% | 96% | 111% | 96% | 111% | 95% | 113% | 100% | 113% | 100% | 113% |
| cluster38 | 193 B | 103% | 89% | 118% | 91% | 103% | 89% | 104% | 89% | 104% | 89% | 103% | 89% | 106% | 93% | 106% | 93% | 106% |
| cluster23 | 225 B | 89% | 76% | 101% | 78% | 89% | 76% | 89% | 77% | 89% | 77% | 89% | 76% | 91% | 80% | 91% | 80% | 91% |
| cluster52 | 235 B | 85% | 73% | 97% | 75% | 85% | 73% | 86% | 74% | 86% | 74% | 85% | 73% | 87% | 77% | 87% | 77% | 87% |
| cluster47 | 248 B | 81% | 69% | 92% | 71% | 81% | 69% | 81% | 70% | 81% | 70% | 81% | 69% | 82% | 73% | 83% | 73% | 82% |
| cluster46 | 248 B | 81% | 69% | 92% | 71% | 81% | 69% | 81% | 70% | 81% | 70% | 81% | 69% | 82% | 72% | 83% | 72% | 82% |
| cluster17 | 357 B | 56% | 48% | 64% | 49% | 56% | 48% | 56% | 48% | 56% | 48% | 56% | 48% | 57% | 50% | 57% | 50% | 57% |
| cluster29 | 417 B | 48% | 41% | 55% | 42% | 48% | 41% | 48% | 42% | 48% | 42% | 48% | 41% | 49% | 43% | 49% | 43% | 49% |
| cluster4 | 601 B | 33% | 29% | 38% | 29% | 33% | 29% | 33% | 29% | 33% | 29% | 33% | 29% | 34% | 30% | 34% | 30% | 34% |
| cluster42 | 659 B | 30% | 26% | 35% | 27% | 30% | 26% | 31% | 26% | 31% | 26% | 30% | 26% | 31% | 27% | 31% | 27% | 31% |
| cluster6 | 685 B | 29% | 25% | 33% | 26% | 29% | 25% | 29% | 25% | 29% | 25% | 29% | 25% | 30% | 26% | 30% | 26% | 30% |
| cluster30 | 774 B | 26% | 22% | 29% | 23% | 26% | 22% | 26% | 22% | 26% | 22% | 26% | 22% | 26% | 23% | 26% | 23% | 26% |
| cluster24 | 831 B | 24% | 21% | 27% | 21% | 24% | 21% | 24% | 21% | 24% | 21% | 24% | 21% | 25% | 22% | 25% | 22% | 25% |
| cluster14 | 847 B | 24% | 20% | 27% | 21% | 24% | 20% | 24% | 20% | 24% | 20% | 24% | 20% | 24% | 21% | 24% | 21% | 24% |
| cluster34 | 886 B | 23% | 19% | 26% | 20% | 23% | 19% | 23% | 20% | 23% | 20% | 23% | 19% | 23% | 20% | 23% | 20% | 23% |
| cluster26 | 1004 B | 20% | 17% | 23% | 18% | 20% | 17% | 20% | 17% | 20% | 17% | 20% | 17% | 20% | 18% | 20% | 18% | 20% |
| cluster33 | 1249 B | 16% | 14% | 18% | 14% | 16% | 14% | 16% | 14% | 16% | 14% | 16% | 14% | 16% | 14% | 16% | 14% | 16% |
| cluster12 | 1502 B | 13% | 11% | 15% | 12% | 13% | 11% | 13% | 12% | 13% | 12% | 13% | 11% | 14% | 12% | 14% | 12% | 14% |
| cluster35 | 1893 B | 11% | 9% | 12% | 9% | 11% | 9% | 11% | 9% | 11% | 9% | 11% | 9% | 11% | 10% | 11% | 10% | 11% |
| cluster7 | 2061 B | 10% | 8% | 11% | 9% | 10% | 8% | 10% | 8% | 10% | 8% | 10% | 8% | 10% | 9% | 10% | 9% | 10% |
| cluster10 | 2504 B | 8% | 7% | 9% | 7% | 8% | 7% | 8% | 7% | 8% | 7% | 8% | 7% | 8% | 7% | 8% | 7% | 8% |
| cluster37 | 4551 B | 4% | 4% | 5% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 5% | 4% | 4% |
| cluster8 | 4716 B | 4% | 4% | 5% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% | 4% |
| cluster49 | 5150 B | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% |
| cluster13 | 5606 B | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% | 3% | 4% |
| cluster53 | 8362 B | 2% | 2% | 3% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% | 2% |
| cluster50 | 60095 B | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 0% |

## Aggregate over all 53 clusters

| | |
|---|--:|
| Total WSS | 1.8 TiB |
| Total distinct objects | 3,948,647,618 |
| Mean object across the fleet | 493 B |
| Metadata, LRU | 735.5 GiB (**41% of WSS**) |
| Metadata, LRU-compact | 632.5 GiB (35%) |
| Metadata, LFU | 838.5 GiB (46%) |
| Metadata, LFU-compact | 647.2 GiB (36%) |
| Irreducible floor (144 B/object) | 529.6 GiB (29%) |
| **Fleet-wide saving, LRU -> compact** | **103.0 GiB** |
| **Fleet-wide saving, LFU -> compact** | **191.2 GiB** |

## The distribution is bimodal

| regime | clusters | share |
|---|--:|--:|
| metadata EXCEEDS data (>100%) | 28 | 53% |
| metadata > half of data (>50%) | 33 | 62% |
| metadata negligible (<5%) | 6 | 11% |

Metadata-bound clusters (>100% under LRU): cluster5, cluster38, cluster32, cluster19, cluster39, cluster54, cluster43, cluster36, cluster40, cluster45, cluster48, cluster41, cluster11, cluster31, cluster28, cluster16, cluster44, cluster20, cluster3, cluster15, cluster51, cluster9, cluster18, cluster2, cluster22, cluster27, cluster25, cluster1

Object-bound clusters (<5% under LRU): cluster13, cluster37, cluster50, cluster53, cluster49, cluster8

---

## What this does and does not say

**Caveats, in order of how much they could move the numbers.**

*Full residency is counterfactual for the metadata-bound clusters.* At any real
budget you hold a fraction of the working set, so the absolute GiB figures are
an upper bound. The **ratio** is not — it is per-object metadata over mean
object size, and it holds at any residency. Read the percentage column; treat
the GiB columns as "cost per unit of data cached, scaled to the whole set".

*The key is not counted here.* `get_policy_overhead` excludes the key, which is
charged separately in `base_size` as `size_of::<HashedKey>() = 8 B`. Real
Twitter keys are ~40–50 B, and the binary traces do not carry `key_size` at all
(the record stores a u64 hash — see the trace-format note in
`eviction_stacks_in_cxl.md`). So the true per-object cost is higher than every
figure here, and the *understatement is worst exactly where it hurts*: on an
8 B-object cluster a 40 B key is five times the payload.

*Mean object size hides distribution.* These use `WSS / distinct`, but several
clusters are heavily skewed — cluster13's median object is 123 B against a
5,606 B mean. A cluster can be object-bound on average and metadata-bound
across most of its keys.

*`OBJECT_MAP_ENTRY_OVERHEAD` is measured, not counted.* 96 B/object comes from
fitting jemalloc `stats.allocated` for the `DashMap` shape (R² = 1.000000), not
from summing struct fields. It is the largest single term here, so the whole
table inherits that measurement.

## What this implies for the paper

The compact-stack contribution should not be argued on the two evaluation
traces, where it is worth 1–3% of latency and nothing in capacity. It should be
argued on **object size**, with the fleet distribution above as the evidence:
the saving is 12–14% of per-object metadata, which is 2–8% of total footprint
on a small-object cluster and negligible on a large-object one.

And the honest limit belongs in the same paragraph: **29% of WSS fleet-wide is
the `Arc` + hashtable floor**, untouched by any of this work. That is the
larger target, and it is not an eviction-policy problem.

## Reproduction

```
python3 /home/griff/cv2/sweep/all54.py    # writes /tmp/all54.md
```

Reads `src/object/overhead.rs` for the per-policy terms and
`results/twitter_trace_working_sets.md` for WSS and distinct counts. cluster21
is absent from every table: it is `add:1.00` across the whole trace and has no
read-through working set to divide by.
