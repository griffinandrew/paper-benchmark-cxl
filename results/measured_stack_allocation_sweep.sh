#!/bin/bash
# Formal allocated-bytes measurement of every FLAT eviction stack.
#
# Method is the tree's own, unchanged: jemalloc stats.allocated (never RSS),
# ONE point per process, sampled at powers of two so every point sits at the
# same phase of every structure's resize cycle. Slope across n is the
# per-object cost; the intercept is fixed setup and is discarded.
set -uo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/griff/work/paper-cache-cxl
F=all_dram

POLICIES="lru lru-compact lfu lfu-compact fifo fifo-compact clock clock-compact sieve sieve-compact mru mru-compact 2q-0.25-0.5 2q-compact-0.25-0.5 s3-fifo-0.1 s3-fifo-compact-0.1 arc"

echo "=== building once ==="
cargo +nightly test --release --features $F --lib measure_one_point --no-run 2>&1 | grep -E "^error|Finished"

echo
echo "=== raw points: MEASURED <policy> <n> <allocated_bytes> ==="
for P in $POLICIES; do
  for N in 1048576 2097152 4194304 8388608; do
    MEASURE_POLICY="$P" MEASURE_N=$N \
      cargo +nightly test --release --features $F --lib measure_one_point -- --ignored --nocapture 2>/dev/null \
      | grep "^MEASURED "
  done
done
echo "SWEEP_DONE"
