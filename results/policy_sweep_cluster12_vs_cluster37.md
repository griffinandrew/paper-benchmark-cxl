# 18-policy sweep: cluster12 vs cluster37

2026-08-21/22. One binary (`scripts/sweep_policies.sh`, policies selected at
run time via `PAPER_POLICY`), paper-cache branch `jemalloc-only` (NUMA-bound
jemalloc arenas, 8/node, runtime policy dispatch). 15 GB cache, 5 GB fast
tier, one client, read-through, first 20M accesses of each trace. Single run
per cell; c1 run-to-run spread measured elsewhere at ~±0.7%, so treat <2%
differences as unresolved.

GET latency counts **hits only** in this in-process mode (the miss arm never
records the get timer; the fill is recorded under SET). Both traces are 100%
GET records: every SET below is a read-through fill.

## Trace profiles (20M-access prefixes)

|                       | cluster12 | cluster37 |
|-----------------------|-----------|-----------|
| best-case hit ratio   | 0.2938    | 0.9434    |
| unique keys           | 14.12M    | 1.13M     |
| avg value             | 1.44 KB   | 4.9 KB    |
| working set           | ~20 GB (exceeds cache) | 5.59 GB (fits cache, ~exceeds fast tier) |

Both runs sit exactly at their compulsory floors (0.7062 / 0.0566), so the
miss column cannot discriminate policies at this cache size; latency, memory,
and migration counts carry the signal.

## cluster12 (write-dominated: 70.6% of ops are fills)

| policy                | GET  | SET  | objects   | peakRSS | node0 | fast/slow GB | demote | promote |
|-----------------------|------|------|-----------|---------|-------|--------------|--------|---------|
| LRU                   | 1625 | 1233 | 9,460,965 | 18.29   | 7.00  | 3.1/11.1     | 12.1M  | 210     |
| LFU                   | 2098 | 1707 | 8,133,910 | 19.54   | 8.23  | 3.8/10.2     | 3.1M   | 1.5M    |
| FIFO                  | 1597 | 1198 | 9,461,792 | 18.19   | 7.64  | 3.7/10.5     | 11.7M  | 0       |
| LRU-sized             | 1621 | 1208 | 7,400,123 | 21.69   | 10.52 | 4.0/10.4     | 12.2M  | 243     |
| LRU/LFU k=3           | 1611 | 1220 | 8,311,352 | 22.54   | 11.27 | 3.9/10.4     | 11.6M  | 79      |
| 2Q                    | 2056 | 1494 | 4,691,316 | 12.44   | 6.66  | 4.2/5.6      | 1.7M   | 3.5M    |
| 2Q-ghost              | 2106 | 1538 | 4,691,451 | 13.12   | 6.94  | 3.9/5.9      | 1.8M   | 3.5M    |
| 2Q-fastadm            | 1466 | 1094 | 4,691,316 | 12.53   | 6.98  | 4.3/5.5      | 2.3M   | 21      |
| 2Q-fastadm-repr       | 1585 | 1195 | 8,508,159 | 18.97   | 8.12  | 3.8/10.4     | 12.0M  | 195     |
| S3-FIFO               | 2031 | 1476 | 4,691,316 | 12.74   | 6.95  | 4.2/5.6      | 1.7M   | 3.5M    |
| S3-ghost              | 2098 | 1536 | 4,691,452 | 13.21   | 7.06  | 3.9/5.9      | 1.8M   | 3.5M    |
| S3-ghost-lazy         | 2102 | 1541 | 4,691,452 | 13.18   | 7.03  | 3.9/5.9      | 1.9M   | 3.5M    |
| S3-ghost-lazy-fa      | 1478 | 1104 | 4,465,896 | 13.02   | 7.68  | 3.9/5.6      | 2.4M   | 0       |
| S3-ghost-lazy-fa-mid  | 1491 | 1116 | 4,465,896 | 12.97   | 7.64  | 3.9/5.6      | 2.4M   | 0       |
| S3-lazy-fa-mid-repr   | 1583 | 1193 | 8,921,386 | 19.62   | 8.88  | 3.8/10.4     | 12.1M  | 171     |
| S3-lazy-fa-repr       | 1591 | 1197 | 8,921,385 | 19.07   | 8.28  | 3.8/10.4     | 12.1M  | 145     |
| S3-lazy-repr          | 2079 | 1725 | 8,686,707 | 18.82   | 7.32  | 3.8/10.5     | 1.9M   | 3.5M    |
| S3-lazy-fa-split-repr | 1578 | 1200 | 8,921,412 | 20.96   | 10.49 | 3.8/10.4     | 12.1M  | 197     |

- Two families with no GET overlap: promote-on-reaccess (2031-2106) vs
  fast-admission/classic (1466-1625). Promotions are per *key*; here 60% of
  hits carry promotion work (3.5M promos / 5.9M hits), which is the gap.
- Slow-admission designs pay the fill's PMEM value write inline on the client
  thread: SET 1476-1725 vs 1094-1233.
- The one-access designs self-cap at 4.69M objects: 70% of keys are
  single-touch and age out of the 1.5 GB one-access region.
- Promotions = 3,515,4xx across six unrelated designs: the trace's
  re-accessed-key count showing through each of them.

## cluster37 (read-dominated: 94.3% of ops are hits)

| policy                | GET  | SET  | miss  | objects   | peakRSS | fast/slow GB | demote | promote |
|-----------------------|------|------|-------|-----------|---------|--------------|--------|---------|
| S3-FIFO               | 1662 | 3739 | .0568 | 857,464   | 5.93    | 2.1/1.5      | 0      | 616K    |
| 2Q-ghost              | 1668 | 3754 | .0568 | 859,039   | 5.97    | 2.1/1.5      | 0      | 618K    |
| S3-ghost-lazy         | 1669 | 3737 | .0568 | 859,039   | 5.98    | 2.1/1.5      | 0      | 618K    |
| S3-ghost              | 1671 | 3716 | .0568 | 859,039   | 5.97    | 2.1/1.5      | 0      | 618K    |
| 2Q                    | 1681 | 3760 | .0568 | 857,464   | 5.94    | 2.1/1.5      | 0      | 616K    |
| S3-lazy-repr          | 1691 | 4449 | .0566 | 1,131,409 | 7.97    | 2.1/3.5      | 0      | 613K    |
| S3-lazy-fa-repr       | 1699 | 2144 | .0566 | 1,131,409 | 8.07    | 3.5/2.1      | 284K   | 0       |
| S3-ghost-lazy-fa      | 1703 | 2096 | .0568 | 853,617   | 5.96    | 3.6/0        | 0      | 0       |
| 2Q-fastadm-repr       | 1709 | 2123 | .0566 | 1,131,409 | 8.06    | 3.6/2.1      | 284K   | 5K      |
| S3-ghost-lazy-fa-mid  | 1709 | 2097 | .0568 | 853,617   | 5.97    | 3.6/0        | 0      | 0       |
| S3-lazy-fa-mid-repr   | 1717 | 2148 | .0566 | 1,131,409 | 8.06    | 3.5/2.1      | 284K   | 0       |
| LRU                   | 1722 | 2144 | .0566 | 1,131,409 | 8.33    | 4.6/0        | 173K   | 498     |
| S3-lazy-fa-split-repr | 1731 | 2150 | .0566 | 1,131,409 | 8.06    | 3.6/2.1      | 284K   | 1.5K    |
| 2Q-fastadm            | 1738 | 2108 | .0568 | 857,464   | 5.97    | 3.6/0        | 0      | 0       |
| LRU/LFU k=3           | 1741 | 2149 | .0566 | 1,131,409 | 8.37    | 4.7/0        | 150K   | 213     |
| LRU-sized             | 1745 | 2158 | .0566 | 1,131,409 | 8.32    | 4.7/0        | 148K   | 335     |
| LFU                   | 1766 | 2564 | .0566 | 1,131,409 | 8.30    | 4.6/0        | 69K    | 85K     |
| FIFO                  | 1778 | 2155 | .0566 | 1,131,409 | 8.38    | 4.7/0        | 156K   | 0       |

- The ranking inverts: promote-on-reaccess designs post the best GETs *and*
  the lowest memory. Promotion work is per-key, and here it amortizes over
  30x more hits (616K promos / 18.9M hits = 3% of hits vs 60% on cluster12).
- The classics end with slow tier EMPTY: the resident working set (~4.6 GB of
  values) fits the fast tier after the ~75 B/object metadata reservation, so
  they converge to pure-DRAM caches with only transient warm-up demotions.
- The one-access designs still shed 24% of keys (857K of 1.13M) -- but the
  price is miss 0.0568 vs the 0.0566 floor, for 29% less peak RSS than LRU
  (5.93 vs 8.33 GB) and the fastest GETs. The admission filter drops the cold
  tail nearly for free here.
- LFU's cluster12 GET anomaly is gone (1766, within 44 ns of LRU),
  confirming it was fill-path cost; its SET stays worst-of-classics (the
  admission latch sends fills to PMEM inline).

## Cross-trace verdict

No single design wins both shapes. Write-dominated: fast-admission
(admit-to-DRAM, demote in background) -- 2Q-fastadm and S3-ghost-lazy-fa by
~10% GET / ~10% SET over the classics. Read-dominated: promote-on-reaccess
2Q/S3 -- best GET at 29% less memory, giving up 0.0002 miss. The reprieve
family is the all-rounder: within ~4% of the local winner on both traces at
full capacity.

Known limits: single runs; both traces at their compulsory-miss floors (a
cache below ~5.6 GB on cluster37 would separate hit ratios); c1 only, and
the c8 story may differ (allocation contention).
