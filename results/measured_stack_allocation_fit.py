import re, collections
pts = collections.defaultdict(list)
for line in open("/home/griff/cv2/sweep/allocsweep.log"):
    m = re.match(r"MEASURED (\S+) (\d+) (\d+)", line)
    if m: pts[m.group(1)].append((int(m.group(2)), int(m.group(3))))

# what get_policy_overhead charges for the eviction stack (total minus the 144)
CHARGED = {"lru":56,"lru-compact":28,"lfu":84,"lfu-compact":32,"fifo":56,"fifo-compact":28,
           "clock":57,"clock-compact":29,"sieve":57,"sieve-compact":29,"mru":56,"mru-compact":28,
           "2q-0.25-0.5":60,"2q-compact-0.25-0.5":36,"s3-fifo-0.1":61,"s3-fifo-compact-0.1":36,"arc":60}

def fit(p):
    n=len(p); sx=sum(x for x,_ in p); sy=sum(y for _,y in p)
    sxx=sum(x*x for x,_ in p); sxy=sum(x*y for x,y in p)
    m=(n*sxy-sx*sy)/(n*sxx-sx*sx); b=(sy-m*sx)/n
    ybar=sy/n; ss_t=sum((y-ybar)**2 for _,y in p)
    ss_r=sum((y-(m*x+b))**2 for x,y in p)
    return m, b, 1-ss_r/ss_t if ss_t else 1.0

print("FLAT eviction stacks: MEASURED allocated bytes/object vs what get_policy_overhead CHARGES\n")
print("%-24s %10s %10s %9s %12s %10s" % ("policy","measured","charged","delta","under-count","R^2"))
print("-"*82)
rows=[]
for pol in ["lru","lru-compact","fifo","fifo-compact","clock","clock-compact","sieve","sieve-compact",
            "mru","mru-compact","lfu","lfu-compact","2q-0.25-0.5","2q-compact-0.25-0.5",
            "s3-fifo-0.1","s3-fifo-compact-0.1","arc"]:
    if pol not in pts: continue
    m,b,r2 = fit(sorted(pts[pol]))
    c = CHARGED[pol]
    rows.append((pol,m,c))
    print("%-24s %10.1f %10d %9.1f %11.0f%% %10.6f" % (pol, m, c, m-c, (m/c-1)*100, r2))

print("\ncompact vs original, MEASURED:")
for a,b_ in (("lru","lru-compact"),("fifo","fifo-compact"),("clock","clock-compact"),
             ("sieve","sieve-compact"),("mru","mru-compact"),("lfu","lfu-compact"),
             ("2q-0.25-0.5","2q-compact-0.25-0.5"),("s3-fifo-0.1","s3-fifo-compact-0.1")):
    d=dict((p,m) for p,m,_ in rows)
    if a in d and b_ in d:
        print("  %-22s %7.1f -> %-7.1f  %+6.1f%%   (charged %d -> %d, %+.0f%%)" % (
            a, d[a], d[b_], (d[b_]/d[a]-1)*100, CHARGED[a], CHARGED[b_], (CHARGED[b_]/CHARGED[a]-1)*100))
