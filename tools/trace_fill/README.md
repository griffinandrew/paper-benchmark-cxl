# trace_fill

Rewrites a binary access trace so a read-through replay has a usable object
size and TTL on every record.

## The problem it solves

`handle_read_through` in `src/client.rs` already implements the replay model
this is for:

```rust
if access.command != Command::Get { return Ok(()); }
...
// on a miss:
self.client.set(access.key, access.value, access.ttl)?;
```

SET records are dropped, and a miss fills from the GET record's own size and
TTL. But in the Twitter traces those fields are frequently useless:

| | cluster13 | cluster19 |
|---|--:|--:|
| GET records | 306,714,865 | 1,503,297,007 |
| ...with size 0 | 155,639,793 (50.7%) | 615,066,149 (40.9%) |
| ...with ttl 0 | **100%** | **100%** |

A zero-size GET fills a zero-byte object; a zero TTL means the object never
expires. Half of cluster13's fills are affected.

The size and TTL exist elsewhere in the trace, on that key's SET records. This
tool resolves them ahead of time and writes them into the GET records, so the
replay needs no lookup and the harness needs no change.

## Why offline rather than in the benchmark

The obvious alternative is a per-key table inside the benchmark process,
consulted at miss time. Two reasons not to:

- The cache runs **in-process** (`client: Arc<dyn CacheBackend>`), and the
  metadata numbers this project reports come from jemalloc `stats.allocated`
  over that whole process. cluster13 has 124.7M keys with a size; a table for
  them is gigabytes landing directly in the measurement.
- It puts a hash lookup on the miss path, which is the path being timed.

Rewriting the trace once costs two sequential reads and keeps both clean. The
tool is deliberately dependency-free, so preparing a trace never rebuilds
paper-cache and can run while a sweep is timing.

## Usage

```bash
cargo build --release
./target/release/trace_fill <in.bin> <out.bin> --maxsize
```

`--maxsize` is the preset for this benchmark: strip SETs, give every GET a
TTL, give a zero-size GET the largest size its key is ever seen at, and leave
a GET that reported a real size alone. Then replay with:

```bash
paper-benchmark --trace out.bin --client-type read-through
```

`--sari` is the other preset, reproducing the 20-byte `FilteredTwitter_Binary`
traces (see below). `--help` lists the individual flags.

## Record format

A flat array of 25-byte little-endian records, no header:

```
timestamp u64 | command u8 (0=GET, 1=SET) | key u64 | value_size u32 | ttl u32
```

`ttl == 0` means "no TTL". This layout is duplicated from `src/access.rs`,
which is pinned by `access::layout_tests` — if those tests need changing,
`Record::decode`/`encode` and `CHUNK` here must change with them.

## Resolution rules

For a record needing a size or TTL, the donor is chosen by `--resolve`:

- `nearest` (default) — the most recent preceding sized record for that key,
  falling forward to the nearest following one when there is none before. A
  key's size tracks its real writes.
- `global` — one value per key for the whole trace, selected by
  `--global-pick first|last|max`.

`max` costs almost nothing over `first`: the mean object size rises 0.034% on
cluster13 and 0.226% on cluster19, so worst-case provisioning is effectively
free.

## What the previous eval traces were

`/home/griff/eval_traces` holds the traces every result before this was
measured on. Their record counts are not the GET stream:

| file | records | equals |
|---|--:|---|
| `cluster13.bin` | 151,075,072 | kia's non-zero-size GETs, exactly |
| `cluster19.bin` | 888,230,858 | kia's non-zero-size GETs, exactly |

So the zero-size GETs were being **dropped**, not filled: 50.7% of cluster13's
references and 40.9% of cluster19's were absent from the replay. The filled
traces live alongside them in `/home/griff/eval_traces_filled` rather than
replacing them, so the two are comparable.

## Fallback rates, per trace

`--fallback median` and `--fallback-ttl median` synthesize values for keys the
trace never describes. How much that invents varies by more than three orders
of magnitude, so it is a per-trace decision, not a global one:

| trace | records | synthesized size | synthesized TTL | median TTL |
|---|--:|--:|--:|--:|
| cluster13 | 306,714,865 | 0.04% | 0.04% | 300 s |
| cluster26 | 110,140,681 | 0.11% | 0.11% | 60 s |
| cluster19 | 1,503,297,007 | 6.58% | 6.68% | 25,910 s |
| cluster53 | 217,410,986 | 20.1% | **49.6%** | 2,592,000 s |
| cluster45 | 194,256,353 | **32.8%** | n/a | **no TTLs exist** |

cluster13 and cluster26 are essentially all real. cluster53 would carry half
its TTLs invented at 30 days. cluster45 contains no TTL on any record, so
`--fallback-ttl` has nothing to draw on and every object is immortal whatever
is asked for. Use 13 and 26 where TTL behaviour matters.

Note also that TTL scale decides whether expiry is exercised at all: cluster13
expires at ~300 s and will fire during a run, while cluster19 at 23,093-28,800 s
will not unless the replay lasts 6.5+ hours, leaving the TTL worker idling at
its 1000 ms tick.

## Relationship to Sari's traces

`/mnt/disk1-20tb/Sari-Traces/FilteredTwitter_Binary` on the `intel` host holds
20-byte records — `uint32 Timestamp | uint64 KeyHash | uint32 ValueSize |
uint32 Ttl`, per his `reader.c` — with no command field. Reverse-engineered
from the data, his method is: keep the GET stream, assign each key one global
size and TTL from its SETs, drop GETs for keys never SET. `--sari` reproduces
his cluster13 record count to within 0.11% (306,579,723 against 306,236,574).

Two differences worth knowing:

- His `ValueSize` is **key length + value length**; this format carries the
  value alone. The gap is exactly +44 at every percentile on cluster13, and
  per-record on cluster19 (mean +42.34). `--size-add 44` reproduces cluster13
  to within a byte of the mean; exact reproduction needs the original CSVs'
  `key_size` column, which the binary format does not carry.
- Stripping SETs is not neutral on every cluster. cluster13 has **1.69 SETs
  per GET** — more writes than reads, so those writes cannot be miss-fills and
  are the application's own. Dropping them removes most of the workload.
  cluster19, at 0.335, is consistent with miss-fills and converts cleanly.
