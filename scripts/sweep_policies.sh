#!/bin/bash
# Sweep every hybrid policy through ONE benchmark binary via PAPER_POLICY.
#
# The last two are the three-queue 2Q variant. k_in matches its 2Q siblings
# (0.1); k_out is swept at 0.25 and 0.5 because k_out is a live parameter
# for the first time here -- in PaperPolicy::TwoQ it is written, never read.
#
# Usage:  sweep_policies.sh <trace-file> <out-prefix> [cache_bytes] [fast_gb] [clients]
#
# SWEEP_TIMEOUT caps each policy (seconds, default 2400). Full-length traces
# need more: a 1.1B-record trace takes ~68 min per policy at one client.
#
# The binary must be built from any hybrid feature (they are runtime-equal):
#   cargo +nightly build --release --features umf,hybrid
# with paper-cache's features = ["lru_hybrid_cache"].
#
# Runs fully locally: the trace must be a raw (decompressed) record file on
# this machine, and the memory sampler runs in-process -- no ssh anywhere in
# the measurement path. Peak node0/node1/RSS are sampled every 2s into
# <out-prefix>_<policy>.samp; full benchmark output into ..._<policy>.out.
set -u
TRACE=${1:?trace file}
OUT=${2:?output prefix}
CACHE=${3:-15000000000}
FASTGB=${4:-5.0}
CLIENTS=${5:-1}
BIN=${SWEEP_BIN:-./target/release/paper-benchmark}
TIMEOUT=${SWEEP_TIMEOUT:-2400}
RECORDS=$(( $(stat -c%s "$TRACE") / 25 ))

POLICIES="lru-hybrid lfu-hybrid fifo-hybrid lru-sized-hybrid lru-lfu-hybrid-3 \
2q-hybrid-0.1 2q-ghost-hybrid-0.1 2q-fast-admission-hybrid-0.1 \
2q-fast-admission-reprieve-hybrid-0.1 s3-fifo-hybrid-0.1 s3-fifo-ghost-hybrid-0.1 \
s3-fifo-ghost-lazy-demotion-hybrid-0.1 s3-fifo-ghost-lazy-demotion-fast-admission-hybrid-0.1 \
s3-fifo-ghost-lazy-demotion-fast-admission-midpoint-hybrid-0.1 \
s3-fifo-lazy-demotion-fast-admission-midpoint-reprieve-hybrid-0.1 \
s3-fifo-lazy-demotion-fast-admission-reprieve-hybrid-0.1 \
s3-fifo-lazy-demotion-reprieve-hybrid-0.1 \
s3-fifo-lazy-demotion-fast-admission-split-slow-reprieve-hybrid-0.1 \
2q-full-fast-admission-hybrid-0.1-0.25 2q-full-fast-admission-hybrid-0.1-0.5"

: > "${OUT}_status.txt"
BINNAME=$(basename "$BIN")
for pol in $POLICIES; do
  ( while :; do
      p=$(pgrep -x "$BINNAME" | head -1)
      if [ -n "$p" ]; then
        r=$(awk '/VmRSS/{print $2}' /proc/$p/status 2>/dev/null)
        n=$(numastat -p $p 2>/dev/null | awk '/^Total/{printf "%.2f %.2f", $2/1024, $3/1024}')
        [ -n "$r" ] && echo "rss_kb=$r node=$n"
      fi
      sleep 2
    done ) > "${OUT}_${pol}.samp" 2>/dev/null &
  SAMP=$!
  timeout "$TIMEOUT" env PAPER_POLICY=$pol FAST_TIER_GB=$FASTGB "$BIN" \
    --trace-stdin --trace-records "$RECORDS" --use-cache --cache-max-size "$CACHE" \
    -c "$CLIENTS" --client-type read-through --max-latency-samples 10000000 \
    < "$TRACE" > "${OUT}_${pol}.out" 2> "${OUT}_${pol}.err"
  rc=$?
  kill $SAMP 2>/dev/null; pkill -x "$BINNAME" 2>/dev/null
  echo "$pol rc=$rc complete=$(grep -c 'SET avg latency' "${OUT}_${pol}.out" 2>/dev/null)" >> "${OUT}_status.txt"
  sleep 5
done
echo DONE >> "${OUT}_status.txt"
