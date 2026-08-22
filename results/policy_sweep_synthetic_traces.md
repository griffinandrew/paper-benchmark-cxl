# 18-policy sweep: synthetic traces (standard_web, low_alpha_cold, uniform_baseline)

2026-08-22. Same harness (`scripts/sweep_policies.sh`), binary, and config as
the cluster sweeps: paper-cache `jemalloc-only`, 15 GB cache, 5 GB fast tier,
one client, read-through, single run per cell. GET counts hits only; SET is
the read-through fill. 54/54 runs clean.

Unlike the cluster traces, these working sets EXCEED the cache, so capacity
misses exist and miss ratios genuinely separate the policies -- the first
hit-ratio-discriminating data in this results series.

## Trace profiles

|                   | standard_web | low_alpha_cold | uniform_baseline |
|-------------------|--------------|----------------|------------------|
| records           | 14.05M       | 5.85M          | 2.34M            |
| unique keys       | 1.087M       | 1.065M         | 1.012M           |
| avg value         | 16.5 KB      | 16.5 KB        | 16.5 KB          |
| working set       | 17.95 GB     | 17.58 GB       | 16.71 GB         |
| best-case hit     | 0.9226       | 0.8181         | 0.5678           |
| miss floor        | 0.0774       | 0.1819         | 0.4322           |

All 100% GET records; every SET below is a fill.

## standard_web (skewed)

| policy                | GET  | SET   | miss  | objects | peakRSS | demote | promote |
|-----------------------|------|-------|-------|---------|---------|--------|---------|
| LRU                   | 5997 | 6172  | .0816 | 902,589 | 22.69   | 2.09M  | 1.23M   |
| LFU                   | 5098 | 11109 | .0814 | 901,145 | 22.27   | 274K   | 257K    |
| FIFO                  | 7037 | 5597  | .0902 | 902,809 | 22.39   | 980K   | 0       |
| LRU-sized             | 6149 | 6376  | .0803 | 962,796 | 22.83   | 1.84M  | 1.08M   |
| LRU/LFU k=3           | 5730 | 5915  | .0814 | 902,707 | 22.60   | 1.50M  | 639K    |
| 2Q                    | 4512 | 8180  | .1512 | 415,561 | 13.01   | 45K    | 333K    |
| 2Q-ghost              | 5280 | 8768  | .1125 | 814,321 | 20.61   | 976K   | 1.26M   |
| 2Q-fastadm            | 4613 | 5373  | .1512 | 415,561 | 13.33   | 263K   | 141K    |
| 2Q-fastadm-repr       | 5799 | 5938  | .0814 | 902,436 | 22.75   | 2.17M  | 1.31M   |
| S3-FIFO               | 4887 | 8136  | .1512 | 415,550 | 13.01   | 36K    | 325K    |
| S3-ghost              | 6866 | 8391  | .1125 | 814,321 | 20.27   | 436K   | 723K    |
| S3-ghost-lazy         | 4983 | 8482  | .1125 | 814,320 | 20.27   | 436K   | 723K    |
| S3-ghost-lazy-fa      | 5087 | 5457  | .1128 | 811,899 | 20.27   | 524K   | 0       |
| S3-ghost-lazy-fa-mid  | 5091 | 5417  | .1128 | 811,899 | 20.28   | 524K   | 0       |
| S3-lazy-fa-mid-repr   | 5564 | 5528  | .0814 | 902,537 | 23.21   | 1.32M  | 466K    |
| S3-lazy-fa-repr       | 5571 | 5488  | .0814 | 902,566 | 23.05   | 1.27M  | 416K    |
| S3-lazy-repr          | 5400 | 13681 | .0814 | 902,549 | 22.73   | 355K   | 644K    |
| S3-lazy-fa-split-repr | 5575 | 5601  | .0814 | 902,464 | 23.02   | 1.51M  | 654K    |

## low_alpha_cold (weak skew)

| policy                | GET   | SET   | miss  | objects | peakRSS | demote | promote |
|-----------------------|-------|-------|-------|---------|---------|--------|---------|
| LRU                   | 10218 | 7079  | .1989 | 901,841 | 22.69   | 2.51M  | 1.63M   |
| LFU                   | 7225  | 11155 | .1984 | 900,607 | 22.32   | 353K   | 336K    |
| FIFO                  | 9236  | 5583  | .2147 | 902,670 | 22.49   | 972K   | 0       |
| LRU-sized             | 10311 | 7331  | .1936 | 955,387 | 22.93   | 2.16M  | 1.40M   |
| LRU/LFU k=3           | 9075  | 6502  | .1986 | 902,079 | 22.64   | 1.75M  | 878K    |
| 2Q                    | 6357  | 8344  | .4329 | 452,992 | 13.61   | 107K   | 395K    |
| 2Q-ghost              | 8925  | 10056 | .2807 | 902,107 | 22.69   | 1.47M  | 1.75M   |
| 2Q-fastadm            | 5862  | 5068  | .4292 | 455,483 | 14.09   | 369K   | 203K    |
| 2Q-fastadm-repr       | 9912  | 6594  | .1981 | 902,159 | 22.63   | 2.61M  | 1.74M   |
| S3-FIFO               | 6940  | 8301  | .4329 | 452,981 | 13.52   | 72K    | 362K    |
| S3-ghost              | 9350  | 8972  | .2807 | 902,052 | 22.50   | 568K   | 850K    |
| S3-ghost-lazy         | 7588  | 9229  | .2807 | 902,051 | 22.37   | 568K   | 850K    |
| S3-ghost-lazy-fa      | 7474  | 5222  | .2803 | 902,052 | 22.18   | 653K   | 0       |
| S3-ghost-lazy-fa-mid  | 7498  | 5250  | .2804 | 902,052 | 22.21   | 653K   | 0       |
| S3-lazy-fa-mid-repr   | 8997  | 5696  | .1985 | 902,041 | 24.04   | 1.57M  | 694K    |
| S3-lazy-fa-repr       | 8956  | 5741  | .1985 | 902,046 | 24.05   | 1.52M  | 645K    |
| S3-lazy-repr          | 8711  | 14007 | .1984 | 902,096 | 23.88   | 603K   | 889K    |
| S3-lazy-fa-split-repr | 9041  | 6024  | .1986 | 901,852 | 23.11   | 1.82M  | 944K    |

## uniform_baseline (no skew)

| policy                | GET   | SET   | miss  | objects | peakRSS | demote | promote |
|-----------------------|-------|-------|-------|---------|---------|--------|---------|
| LRU                   | 13652 | 7117  | .4457 | 902,085 | 22.69   | 1.53M  | 768K    |
| LFU                   | 9758  | 12011 | .4460 | 900,752 | 22.51   | 309K   | 295K    |
| FIFO                  | 10305 | 5921  | .4456 | 902,273 | 22.47   | 759K   | 0       |
| LRU-sized             | 13455 | 7468  | .4413 | 937,985 | 22.69   | 1.30M  | 641K    |
| LRU/LFU k=3           | 11941 | 6450  | .4457 | 902,119 | 22.81   | 1.10M  | 343K    |
| 2Q                    | 11004 | 10059 | .8496 | 268,515 | 9.86    | 0      | 178K    |
| 2Q-ghost              | 12226 | 11199 | .6535 | 786,056 | 18.92   | 639K   | 922K    |
| 2Q-fastadm            | 4755  | 4089  | .8436 | 273,101 | 10.08   | 0      | 0       |
| 2Q-fastadm-repr       | 13282 | 7028  | .4455 | 902,299 | 22.84   | 1.57M  | 816K    |
| S3-FIFO               | 10954 | 10005 | .8496 | 268,516 | 9.85    | 0      | 178K    |
| S3-ghost              | 10723 | 10410 | .6535 | 786,056 | 18.93   | 409K   | 696K    |
| S3-ghost-lazy         | 10661 | 10492 | .6535 | 786,054 | 18.89   | 409K   | 696K    |
| S3-ghost-lazy-fa      | 8953  | 5156  | .6526 | 783,548 | 18.60   | 498K   | 0       |
| S3-ghost-lazy-fa-mid  | 8955  | 5098  | .6526 | 783,560 | 18.66   | 498K   | 0       |
| S3-lazy-fa-mid-repr   | 12094 | 6251  | .4457 | 902,045 | 23.73   | 1.16M  | 406K    |
| S3-lazy-fa-repr       | 11868 | 6167  | .4457 | 902,046 | 24.00   | 1.13M  | 379K    |
| S3-lazy-repr          | 11532 | 14668 | .4457 | 902,046 | 24.15   | 275K   | 556K    |
| S3-lazy-fa-split-repr | 12251 | 6330  | .4457 | 902,046 | 23.56   | 1.26M  | 502K    |

## Findings

1. **The one-access designs collapse on cold-tail workloads.** The same
   filter that cost +0.0002 miss for -29% RSS on cluster37 doubles the miss
   ratio on standard_web (.1512 vs .0814), runs 2.2x on low_alpha (.4329),
   and holds 269K of 1M keys at .85 miss on uniform (2Q-fastadm degenerates
   to a DRAM-only cache: slow tier empty). Their fast GETs are an artifact of
   serving a small hot subset. Ghost queues recover only part of the gap.

2. **LFU posts the best full-capacity GET on all three synthetics** -- the
   inverse of cluster12: 15% / 29% / 28% faster than LRU. With 16.5 KB
   values, GET is dominated by PMEM reads; LFU both places better and
   migrates 5-8x less (uniform: 309K demotions vs LRU's 1.53M -- recency
   churn with zero predictive value). Its SET stays worst everywhere
   (~11-12 us: latched fills copy 16.5 KB to PMEM inline on the client
   thread), but fills are the minority on these traces.

3. **LRU-sized takes the best miss ratio on both skewed traces**
   (.0803 / .1936) -- the size-split design's first measured win.

4. **Under uniform, every full-capacity policy converges to the same miss
   ratio (~.4457)** as theory demands; only overhead separates them, and the
   migration counters show exactly who pays it.

5. **The reprieve family stays the all-rounder** across all five traces so
   far: classic-level miss ratio, balanced GET/SET, no failure mode yet.

Same caveats as the cluster file: single runs, c1 only, GET = hits only.
Note the old `synthetic_sweep.sh` ran these traces at 8 GB / 2 GB, so those
historical numbers are not comparable to these.
