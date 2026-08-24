/* wsscalc.c -- read-through working-set size, computed our way.
 *
 * Rule (matches the verified ".filt" convention plus max-size-over-time):
 *   - GET rows only (a set is not a read-through access; read-through fills
 *     come from get misses, so sets would double-count).
 *   - value_size > 0 only (size 0 is a miss in the source system: no object).
 *   - For each distinct key keep the MAXIMUM value_size ever seen, because an
 *     object must be provisioned for the largest form it takes.
 *   WSS = sum over distinct keys of max(value_size).
 *
 * Twitter CSV: timestamp,anon_key,key_size,value_size,client_id,operation,ttl
 * Key is hashed with MurmurHash3_x64_128(key,0).h1 -- the same hash verified
 * byte-exact against the cluster12/37 masters, so keys agree with our traces.
 *
 * Open-addressing table, u64 key -> u32 max size, doubling at 0.60 load.
 * build: cc -O3 -march=native -o wsscalc wsscalc.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <inttypes.h>


static inline uint64_t rotl64(uint64_t x, int8_t r){ return (x << r) | (x >> (64 - r)); }
static inline uint64_t fmix64(uint64_t k){
    k ^= k >> 33; k *= 0xff51afd7ed558ccdULL;
    k ^= k >> 33; k *= 0xc4ceb9fe1a85ec53ULL;
    k ^= k >> 33; return k;
}
/* MurmurHash3_x64_128, seed 0; returns h1 (the first/low 64 bits) */
static uint64_t mmh3_64(const uint8_t *data, size_t len)
{
    const size_t nblocks = len / 16;
    uint64_t h1 = 0, h2 = 0;
    const uint64_t c1 = 0x87c37b91114253d5ULL, c2 = 0x4cf5ad432745937fULL;
    for (size_t i = 0; i < nblocks; i++) {
        uint64_t k1, k2;
        memcpy(&k1, data + i*16,     8);
        memcpy(&k2, data + i*16 + 8, 8);
        k1 *= c1; k1 = rotl64(k1,31); k1 *= c2; h1 ^= k1;
        h1 = rotl64(h1,27); h1 += h2; h1 = h1*5 + 0x52dce729;
        k2 *= c2; k2 = rotl64(k2,33); k2 *= c1; h2 ^= k2;
        h2 = rotl64(h2,31); h2 += h1; h2 = h2*5 + 0x38495ab5;
    }
    const uint8_t *tail = data + nblocks*16;
    uint64_t k1 = 0, k2 = 0;
    switch (len & 15) {
    case 15: k2 ^= (uint64_t)tail[14] << 48; /* fallthrough */
    case 14: k2 ^= (uint64_t)tail[13] << 40; /* fallthrough */
    case 13: k2 ^= (uint64_t)tail[12] << 32; /* fallthrough */
    case 12: k2 ^= (uint64_t)tail[11] << 24; /* fallthrough */
    case 11: k2 ^= (uint64_t)tail[10] << 16; /* fallthrough */
    case 10: k2 ^= (uint64_t)tail[ 9] << 8;  /* fallthrough */
    case  9: k2 ^= (uint64_t)tail[ 8] << 0;
             k2 *= c2; k2 = rotl64(k2,33); k2 *= c1; h2 ^= k2; /* fallthrough */
    case  8: k1 ^= (uint64_t)tail[ 7] << 56; /* fallthrough */
    case  7: k1 ^= (uint64_t)tail[ 6] << 48; /* fallthrough */
    case  6: k1 ^= (uint64_t)tail[ 5] << 40; /* fallthrough */
    case  5: k1 ^= (uint64_t)tail[ 4] << 32; /* fallthrough */
    case  4: k1 ^= (uint64_t)tail[ 3] << 24; /* fallthrough */
    case  3: k1 ^= (uint64_t)tail[ 2] << 16; /* fallthrough */
    case  2: k1 ^= (uint64_t)tail[ 1] << 8;  /* fallthrough */
    case  1: k1 ^= (uint64_t)tail[ 0] << 0;
             k1 *= c1; k1 = rotl64(k1,31); k1 *= c2; h1 ^= k1;
    }
    h1 ^= len; h2 ^= len;
    h1 += h2; h2 += h1;
    h1 = fmix64(h1); h2 = fmix64(h2);
    h1 += h2;
    return h1;
}

static inline void put64(uint8_t *p, uint64_t v){ for (int i=0;i<8;i++) p[i]=(uint8_t)(v>>(8*i)); }
static inline void put32(uint8_t *p, uint32_t v){ for (int i=0;i<4;i++) p[i]=(uint8_t)(v>>(8*i)); }


/* ---- open addressing: key 0 is held in a side slot so 0 can mean empty ---- */
static uint64_t *K = NULL;
static uint32_t *V = NULL;
static uint64_t  CAP = 0, CNT = 0;
static int       have_zero = 0;
static uint32_t  zero_val = 0;

static void tbl_alloc(uint64_t cap) {
    K = calloc(cap, sizeof(uint64_t));
    V = malloc(cap * sizeof(uint32_t));
    if (!K || !V) { fprintf(stderr, "OOM allocating %" PRIu64 " slots\n", cap); exit(2); }
    CAP = cap;
}

static void tbl_grow(void);

static inline void tbl_put(uint64_t k, uint32_t v) {
    if (k == 0) {                       /* sentinel collision: keep separately */
        if (!have_zero) { have_zero = 1; zero_val = v; }
        else if (v > zero_val) zero_val = v;
        return;
    }
    uint64_t i = (k * 0x9E3779B97F4A7C15ULL) & (CAP - 1);
    for (;;) {
        uint64_t cur = K[i];
        if (cur == k) { if (v > V[i]) V[i] = v; return; }
        if (cur == 0) {
            K[i] = k; V[i] = v; CNT++;
            if (CNT * 10 > CAP * 6) tbl_grow();
            return;
        }
        i = (i + 1) & (CAP - 1);
    }
}

static void tbl_grow(void) {
    uint64_t old_cap = CAP;
    uint64_t *oldK = K; uint32_t *oldV = V;
    uint64_t newcap = CAP * 2;
    fprintf(stderr, "  [grow %" PRIu64 " -> %" PRIu64 " slots, %" PRIu64 " keys]\n",
            old_cap, newcap, CNT);
    K = calloc(newcap, sizeof(uint64_t));
    V = malloc(newcap * sizeof(uint32_t));
    if (!K || !V) { fprintf(stderr, "OOM growing to %" PRIu64 "\n", newcap); exit(2); }
    CAP = newcap; CNT = 0;
    for (uint64_t j = 0; j < old_cap; j++)
        if (oldK[j]) {
            uint64_t k = oldK[j], i = (k * 0x9E3779B97F4A7C15ULL) & (CAP - 1);
            while (K[i]) i = (i + 1) & (CAP - 1);
            K[i] = k; V[i] = oldV[j]; CNT++;
        }
    free(oldK); free(oldV);
}

int main(int argc, char **argv) {
    const char *label = (argc > 1) ? argv[1] : "?";
    uint64_t cap = (argc > 2) ? strtoull(argv[2], NULL, 10) : (1ULL << 22);
    tbl_alloc(cap);

    char *line = NULL; size_t bufcap = 0; ssize_t n;
    char *f[8];
    uint64_t rows = 0, gets = 0, kept = 0, zero_get = 0, bad = 0;

    while ((n = getline(&line, &bufcap, stdin)) > 0) {
        while (n > 0 && (line[n-1] == '\n' || line[n-1] == '\r')) line[--n] = 0;
        if (n == 0) continue;
        int nf = 0; f[nf++] = line;
        for (char *p = line; *p && nf < 8; p++) if (*p == ',') { *p = 0; f[nf++] = p + 1; }
        if (nf != 7) { bad++; continue; }          /* truncated tail line */
        rows++;
        if (f[5][0] != 'g' || strcmp(f[5], "get")) continue;   /* GET rows only */
        gets++;
        uint64_t vs = strtoull(f[3], NULL, 10);
        if (vs == 0) { zero_get++; continue; }     /* miss in the source system */
        if (vs > 0xFFFFFFFFULL) vs = 0xFFFFFFFFULL;
        kept++;
        tbl_put(mmh3_64((const uint8_t *)f[1], strlen(f[1])), (uint32_t)vs);
    }

    uint64_t distinct = CNT + (have_zero ? 1 : 0);
    long double wss = have_zero ? zero_val : 0;
    for (uint64_t i = 0; i < CAP; i++) if (K[i]) wss += V[i];

    printf("%s rows=%" PRIu64 " gets=%" PRIu64 " zero_gets=%" PRIu64 " kept=%" PRIu64
           " distinct=%" PRIu64 " wss_bytes=%.0Lf wss_mb=%.1Lf bad=%" PRIu64 "\n",
           label, rows, gets, zero_get, kept, distinct, wss, wss / 1048576.0L, bad);
    return 0;
}
