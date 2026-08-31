import re, os
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

os.chdir("/home/griff/cv2")
OUT = "/home/griff/work/paper-benchmark-cxl/results/figures"
os.makedirs(OUT, exist_ok=True)

def strip(s): return re.sub(r"\x1b\[[0-9;]*m", "", s)

def parse(f):
    t = strip(open(f, errors="replace").read())
    d = {}
    for tag in ("GET", "SET"):
        k = tag.lower()
        blk = re.search(r"\*\*\* %s stats \*\*\*(.*?)(?=\*\*\*|\Z)" % tag, t, re.S).group(1)
        d[k+"mean"] = int(re.search(r"%s avg latency:\s*([\d,]+)ns" % tag, t).group(1).replace(",", ""))
        d[k+"pct"]  = [int(x) for x in re.findall(r"(\d+)ns", blk.split("\n|---")[-1])[:9]]
    m = re.search(r"^Objects:\s*([\d,]+)", t, re.M)
    d["obj"] = int(m.group(1).replace(",", ""))
    d["miss"] = float(re.search(r"^Miss ratio:\s*([\d.]+)", t, re.M).group(1))
    return d

LABELS = ["p50", "p75", "p90", "p95", "p99", "p99.9", "mean"]
IDX    = [0, 1, 2, 3, 4, 5]

BG   = "#FBFAF8"
INK  = "#1E232B"
MUTE = "#6B7280"
BASE = "#3F6E8C"   # baseline: cool slate blue
VAR  = "#C8622F"   # variant: warm terracotta
GRID = "#DDD8D1"

def series(d, k):
    return [d[k+"pct"][i] for i in IDX] + [d[k+"mean"]]

def figure(fname, title, subtitle, base_lbl, var_lbl, files):
    """files: {cluster: (baseline_file, variant_file)}"""
    fig, axes = plt.subplots(2, 2, figsize=(13.5, 8.4), facecolor=BG)
    fig.subplots_adjust(hspace=0.42, wspace=0.18, top=0.845, bottom=0.10, left=0.075, right=0.985)

    for col, cl in enumerate(("13", "53")):
        b, v = (parse(files[cl][0]), parse(files[cl][1]))
        for row, op in enumerate(("get", "set")):
            ax = axes[row][col]
            ax.set_facecolor(BG)
            bs, vs = series(b, op), series(v, op)
            x = np.arange(len(LABELS)); w = 0.38

            ax.bar(x - w/2, bs, w, label=base_lbl, color=BASE, zorder=3)
            ax.bar(x + w/2, vs, w, label=var_lbl, color=VAR, zorder=3)

            for i, (bb, vv) in enumerate(zip(bs, vs)):
                pct = (vv - bb) / bb * 100.0
                ax.annotate("%+.0f%%" % pct, (x[i] + w/2, vv), textcoords="offset points",
                            xytext=(0, 4), ha="center", fontsize=8.0,
                            color=VAR if pct > 0 else "#2F7D5B", fontweight="bold", zorder=4)

            ax.set_yscale("log")
            ax.set_xticks(x); ax.set_xticklabels(LABELS, fontsize=9)
            ax.set_ylabel("%s latency (ns, log)" % op.upper(), fontsize=9.5, color=INK)
            ax.set_title("cluster%s — %s" % (cl, op.upper()), fontsize=10.5,
                         color=INK, fontweight="bold", loc="left", pad=8)
            ax.grid(axis="y", color=GRID, lw=0.8, zorder=0)
            ax.set_axisbelow(True)
            ax.tick_params(colors=MUTE, labelsize=8.5)
            for s in ("top", "right"):
                ax.spines[s].set_visible(False)
            for s in ("left", "bottom"):
                ax.spines[s].set_color(GRID)
            top = max(max(bs), max(vs))
            ax.set_ylim(top=top * 2.6)
            if row == 0 and col == 0:
                ax.legend(frameon=False, fontsize=9, loc="upper left")

    fig.suptitle(title, fontsize=15, color=INK, fontweight="bold", x=0.075, ha="left", y=0.965)
    fig.text(0.075, 0.905, subtitle, fontsize=9.5, color=MUTE, ha="left")
    fig.text(0.075, 0.022,
             "Percentages are the variant against the baseline. Log y-axis. "
             "cluster13 SET percentiles are reservoir-sampled (10M of 72-84M); cluster53 SET is full-population.",
             fontsize=7.8, color=MUTE, ha="left")
    fig.savefig(os.path.join(OUT, fname), format="svg", facecolor=BG)
    plt.close(fig)
    print("wrote", fname)

for pol, name in (("lru", "LRU"), ("lfu", "LFU")):
    figure(
        "fig_%s_flat_vs_hybrid.svg" % pol,
        "%s compact: all-DRAM vs tiered" % name,
        "Same policy, same 12 GB budget, eviction stacks in DRAM on both sides. "
        "Positive = the tiered design is slower.",
        "all-DRAM (flat)", "hybrid (tiered)",
        {cl: ("rc_%s-compact_cluster%s.out" % (pol, cl),
              "rc_%s-compact-hybrid_cluster%s.out" % (pol, cl)) for cl in ("13", "53")},
    )
    figure(
        "fig_%s_hybrid_stacks_dram_vs_cxl.svg" % pol,
        "%s compact hybrid: eviction stacks in DRAM vs CXL" % name,
        "Same tiered design; only where the eviction-stack metadata lives changes. "
        "Positive = CXL stacks are slower.",
        "stacks in DRAM", "stacks in CXL",
        {cl: ("rc_%s-compact-hybrid_cluster%s.out" % (pol, cl),
              "rp_%s-compact-hybrid_cluster%s.out" % (pol, cl)) for cl in ("13", "53")},
    )
