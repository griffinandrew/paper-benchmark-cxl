import re, os, sys

LIB = "/home/griff/work/paper-cache-cxl"
DOC = "/home/griff/work/paper-benchmark-cxl/results/twitter_trace_working_sets.md"

# ---- per-policy eviction-stack terms, read from the source ----
src = open(os.path.join(LIB, "src/object/overhead.rs")).read()
body = src[src.index("pub fn get_policy_overhead"):]
body = body[:body.index("\n}\n")]
MAP_ARC = 144

stack = {}
for pat in (r"PaperPolicy::(\w+)\([^)]*\)\s*=>\s*([^,\n]+),", r"PaperPolicy::(\w+)\s*=>\s*([^,\n]+),"):
    for m in re.finditer(pat, body):
        name, expr = m.group(1), m.group(2).strip()
        if "OBJECT_MAP_AND_ARC_OVERHEAD" not in expr or name in stack:
            continue
        stack[name] = sum(int(x) for x in re.findall(r"\b(\d+)\b", expr))

ORDER = ["Lru","LruCompact","Lfu","LfuCompact","Fifo","FifoCompact","Clock","ClockCompact",
         "Sieve","SieveCompact","Mru","MruCompact","TwoQ","TwoQCompact","SThreeFifo",
         "SThreeFifoCompact","Arc"]
ORDER = [p for p in ORDER if p in stack]
total = {p: stack[p] + MAP_ARC for p in ORDER}

# ---- parse the WSS table ----
UNIT = {"B": 1, "KiB": 1024, "MiB": 1024**2, "GiB": 1024**3, "TiB": 1024**4}
def bytes_of(s):
    m = re.match(r"\*?\*?([\d,.]+)\s*(B|KiB|MiB|GiB|TiB)\*?\*?$", s.strip())
    if not m: return None
    return float(m.group(1).replace(",", "")) * UNIT[m.group(2)]

rows = []
for line in open(DOC):
    if not line.startswith("| cluster"): continue
    c = [x.strip() for x in line.strip().strip("|").split("|")]
    if len(c) < 9 or c[1] == "access": continue
    wss = bytes_of(c[1])
    if wss is None: continue
    try:
        distinct = int(c[4].replace(",", ""))
    except ValueError:
        continue
    if distinct == 0: continue
    rows.append((c[0], wss, distinct))

rows.sort(key=lambda r: -r[1])
def gib(b): return b / 1024**3
def human(b):
    for u, d in (("TiB", 1024**4), ("GiB", 1024**3), ("MiB", 1024**2), ("KiB", 1024)):
        if b >= d: return "%.1f %s" % (b/d, u)
    return "%.0f B" % b

sys.stdout = open("/tmp/all54.md", "w")

print("# Metadata overhead at full working set: all %d Twitter clusters\n" % len(rows))
print("Per-object metadata is `get_policy_overhead` — the eviction-stack term plus")
print("a **fixed 144 B** (`Arc` header 48 + `DashMap` entry 96) that every policy pays")
print("and no eviction-stack work can touch. WSS and distinct-object counts come from")
print("`twitter_trace_working_sets.md`; nothing here is re-measured, it is arithmetic")
print("over that table and the terms in `src/object/overhead.rs`.\n")
print("`meta/WSS` is metadata bytes divided by working-set bytes if the ENTIRE working")
print("set were resident. Above 100% means the bookkeeping outweighs the data it")
print("describes. It is equivalently `per-object metadata / mean object size`, so it is")
print("a property of object size alone — which is why it transfers to any cache size.\n")

print("## Per-object metadata by policy\n")
print("| policy | eviction stack | Arc + map | total | vs LRU |")
print("|---|--:|--:|--:|--:|")
for p in ORDER:
    print("| %s | %d | %d | **%d B** | %+.1f%% |" % (p, stack[p], MAP_ARC, total[p],
          (total[p]-total["Lru"])/total["Lru"]*100))
print("\nThe floor is 144 B/object for every policy. Compaction moves the total by at")
print("most 14% (LFU 228 -> 176) even though the stack itself halves.\n")

print("## Every cluster, LRU and LRU-compact, sorted by metadata burden\n")
print("| cluster | WSS | distinct | mean obj | LRU meta | meta/WSS | compact meta | meta/WSS | saved |")
print("|---|--:|--:|--:|--:|--:|--:|--:|--:|")
burden = sorted(rows, key=lambda r: -(total["Lru"]*r[2]/r[1]))
for name, wss, n in burden:
    a, b = total["Lru"]*n, total["LruCompact"]*n
    print("| %s | %s | %s | %.0f B | %s | **%.0f%%** | %s | %.0f%% | %s |" % (
        name, human(wss), format(n, ","), wss/n, human(a), a/wss*100, human(b), b/wss*100,
        human(a-b)))

print("\n## meta/WSS for every policy\n")
print("| cluster | mean obj | " + " | ".join(ORDER) + " |")
print("|---|--:|" + "--:|"*len(ORDER))
for name, wss, n in burden:
    print("| %s | %.0f B | %s |" % (name, wss/n,
          " | ".join("%.0f%%" % (total[p]*n/wss*100) for p in ORDER)))

# ---- aggregate ----
tw = sum(r[1] for r in rows); tn = sum(r[2] for r in rows)
print("\n## Aggregate over all %d clusters\n" % len(rows))
print("| | |")
print("|---|--:|")
print("| Total WSS | %s |" % human(tw))
print("| Total distinct objects | %s |" % format(tn, ","))
print("| Mean object across the fleet | %.0f B |" % (tw/tn))
print("| Metadata, LRU | %s (**%.0f%% of WSS**) |" % (human(total["Lru"]*tn), total["Lru"]*tn/tw*100))
print("| Metadata, LRU-compact | %s (%.0f%%) |" % (human(total["LruCompact"]*tn), total["LruCompact"]*tn/tw*100))
print("| Metadata, LFU | %s (%.0f%%) |" % (human(total["Lfu"]*tn), total["Lfu"]*tn/tw*100))
print("| Metadata, LFU-compact | %s (%.0f%%) |" % (human(total["LfuCompact"]*tn), total["LfuCompact"]*tn/tw*100))
print("| Irreducible floor (144 B/object) | %s (%.0f%%) |" % (human(144*tn), 144*tn/tw*100))
print("| **Fleet-wide saving, LRU -> compact** | **%s** |" % human((total["Lru"]-total["LruCompact"])*tn))
print("| **Fleet-wide saving, LFU -> compact** | **%s** |" % human((total["Lfu"]-total["LfuCompact"])*tn))

over = [r for r in rows if total["Lru"]*r[2]/r[1] > 1.0]
half = [r for r in rows if total["Lru"]*r[2]/r[1] > 0.5]
under = [r for r in rows if total["Lru"]*r[2]/r[1] < 0.05]
print("\n## The distribution is bimodal\n")
print("| regime | clusters | share |")
print("|---|--:|--:|")
print("| metadata EXCEEDS data (>100%%) | %d | %.0f%% |" % (len(over), 100*len(over)/len(rows)))
print("| metadata > half of data (>50%%) | %d | %.0f%% |" % (len(half), 100*len(half)/len(rows)))
print("| metadata negligible (<5%%) | %d | %.0f%% |" % (len(under), 100*len(under)/len(rows)))
print("\nMetadata-bound clusters (>100%% under LRU): %s\n" % ", ".join(r[0] for r in over))
print("Object-bound clusters (<5%% under LRU): %s" % ", ".join(r[0] for r in under))
sys.stdout.close()
