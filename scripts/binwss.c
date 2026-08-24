/* binwss.c -- working set + value-size distribution of a 25-byte record stream.
 *
 * Reads 25-byte records on stdin (pipe mkvcsv2bin/twcsv2bin into it) and reports
 * WSS = sum over distinct keys of max(value_size) -- the SAME rule wsscalc.c
 * applies to the Twitter clusters, so the two are directly comparable.
 *
 * Reports percentiles two ways, because they answer different questions:
 *   per-ACCESS  weights each request -- what the bandwidth/latency sees.
 *   per-OBJECT  weights each distinct key once, at its max size over time --
 *               the population the cache actually holds, and the one whose
 *               sum is the working set.
 *
 * Exact counts via a direct-index histogram for sizes < 16 MiB (the tail above
 * that is kept in an overflow bucket and reported separately, never silently
 * folded in).  Record: <Q ts><B cmd><Q key><I vsize><I ttl>, little endian.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <inttypes.h>

#define HMAX (16u<<20)

static uint64_t *K=NULL; static uint32_t *V=NULL; static uint64_t CAP=0, CNT=0;
static void grow(void);
static inline void put(uint64_t k, uint32_t v){
    if(!k) k=0x9E3779B97F4A7C15ULL;
    uint64_t i=(k*0x9E3779B97F4A7C15ULL)&(CAP-1);
    for(;;){ uint64_t c=K[i];
        if(c==k){ if(v>V[i]) V[i]=v; return; }
        if(!c){ K[i]=k; V[i]=v; CNT++; if(CNT*10>CAP*6) grow(); return; }
        i=(i+1)&(CAP-1); } }
static void grow(void){
    uint64_t oc=CAP,*oK=K; uint32_t*oV=V; uint64_t nc=CAP*2;
    K=calloc(nc,8); V=malloc(nc*4);
    if(!K||!V){ fprintf(stderr,"OOM\n"); exit(2); }
    CAP=nc; CNT=0;
    for(uint64_t j=0;j<oc;j++) if(oK[j]){
        uint64_t k=oK[j],i=(k*0x9E3779B97F4A7C15ULL)&(CAP-1);
        while(K[i]) i=(i+1)&(CAP-1);
        K[i]=k; V[i]=oV[j]; CNT++; }
    free(oK); free(oV); }

static void report(const char*tag, uint64_t*h, uint64_t n, uint64_t over, uint64_t overmax){
    if(!n){ printf("  %-12s (empty)\n",tag); return; }
    const double ps[]={0.01,0.25,0.50,0.75,0.90,0.95,0.99,0.999};
    const char *nm[]={"p1","p25","p50","p75","p90","p95","p99","p99.9"};
    printf("  %-11s n=%-14" PRIu64, tag, n);
    /* Threshold is ceil(p*n), floored at 1: a truncated threshold of 0 would
     * match at acc==0 and report the smallest bucket for every percentile. */
    uint64_t thr[8];
    for(int i=0;i<8;i++){
        long double t = ps[i]*(long double)n;
        thr[i] = (uint64_t)t; if((long double)thr[i] < t) thr[i]++;
        if(thr[i]==0) thr[i]=1;
    }
    uint64_t acc=0; int pi=0; double out[8]; for(int i=0;i<8;i++) out[i]=-1;
    for(uint64_t s=0; s<HMAX && pi<8; s++){
        acc+=h[s];
        while(pi<8 && acc >= thr[pi]){ out[pi]=(double)s; pi++; }
    }
    while(pi<8){ out[pi]=(double)overmax; pi++; }
    for(int i=0;i<8;i++) printf(" %s=%.0f",nm[i],out[i]);
    if(over) printf("  [>16MiB: %" PRIu64 ", max %" PRIu64 "]",over,overmax);
    printf("\n");
}

int main(int argc,char**argv){
    const char*lab=argc>1?argv[1]:"?";
    FILE*f=stdin;
    uint64_t *ha=calloc(HMAX,8), *ho=calloc(HMAX,8);
    CAP=1ULL<<22; K=calloc(CAP,8); V=malloc(CAP*4);
    if(!ha||!ho||!K||!V){ fprintf(stderr,"OOM\n"); return 2; }
    uint64_t n=0, aover=0, aovermax=0; long double sum=0;
    unsigned char buf[25*40000];
    size_t got;
    while((got=fread(buf,1,sizeof buf,f))>0){
        for(size_t o=0;o+25<=got;o+=25){
            uint64_t key; uint32_t vs;
            memcpy(&key,buf+o+9,8); memcpy(&vs,buf+o+17,4);
            n++; sum+=vs;
            if(vs<HMAX) ha[vs]++; else { aover++; if(vs>aovermax) aovermax=vs; }
            put(key,vs);
        }
    }
    uint64_t oover=0, oovermax=0, on=0;
    for(uint64_t i=0;i<CAP;i++) if(K[i]){ on++;
        if(V[i]<HMAX) ho[V[i]]++; else { oover++; if(V[i]>oovermax) oovermax=V[i]; } }
    long double wss=0;
    for(uint64_t i=0;i<CAP;i++) if(K[i]) wss += V[i];
    printf("%s  records=%" PRIu64 "  distinct=%" PRIu64 "  mean/access=%.0Lf B"
           "  WSS_BYTES=%.0Lf  WSS_GB=%.2Lf\n",
           lab, n, on, n? sum/n : 0, wss, wss/1073741824.0L);
    report("per-ACCESS", ha, n,  aover, aovermax);
    report("per-OBJECT", ho, on, oover, oovermax);
    return 0;
}
