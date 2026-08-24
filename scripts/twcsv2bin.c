/* twcsv2bin.c -- Twitter twemcache CSV  ->  25-byte benchmark trace records.
 *
 * Reproduces, byte for byte, the pipeline that produced
 * /home/griffin/staged/cluster12.filt.zst and cluster37.filt.zst.
 *
 * CSV columns: timestamp,anon_key,key_size,value_size,client_id,operation,ttl
 * Record (25 B, little endian): <Q ts_ms> <B cmd> <Q key64> <I value_size> <I ttl>
 *   ts_ms   = csv_timestamp * 1000
 *   cmd     = 0 (Get)              [filt mode emits Get rows only]
 *   key64   = MurmurHash3_x64_128(anon_key, seed=0).h1   (low 64 bits)
 *   value_size, ttl  = passed through from the CSV
 *
 * Default (filt) mode keeps a row iff  operation == "get"  AND  value_size > 0.
 * -a/--all also emits set rows as cmd=1 (raw .bin equivalent); non get/set ops
 * are always dropped because the 25-byte format has no encoding for them.
 *
 * build:  cc -O2 -o twcsv2bin twcsv2bin.c
 * use:    curl ... | zstd -dc | ./twcsv2bin > out.bin
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

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

int main(int argc, char **argv)
{
    int keep_sets = 0;
    for (int i = 1; i < argc; i++)
        if (!strcmp(argv[i],"-a") || !strcmp(argv[i],"--all")) keep_sets = 1;

    char   *line = NULL;
    size_t  cap  = 0;
    ssize_t n;
    uint8_t obuf[25*4096];
    size_t  ofill = 0;
    unsigned long long in = 0, out = 0, badcols = 0, dropped_zero = 0, dropped_op = 0;

    while ((n = getline(&line, &cap, stdin)) > 0) {
        if (line[n-1] == '\n') line[--n] = 0;
        if (n == 0) continue;
        in++;
        /* locate the 5 trailing commas so a key containing ',' cannot desync us */
        char *c[6];              /* c[0]=first comma, c[1..5]=last five commas */
        char *p = memchr(line, ',', (size_t)n);
        if (!p) { badcols++; continue; }
        c[0] = p;
        int f = 0; char *q = line + n;
        while (f < 5 && q > c[0]) {
            q--;
            if (*q == ',') c[5 - f++] = q;
        }
        if (f != 5) { badcols++; continue; }   /* truncated final line lands here */

        uint64_t ts    = strtoull(line, NULL, 10);
        uint64_t vsize = strtoull(c[2] + 1, NULL, 10);   /* value_size */
        uint64_t ttl   = strtoull(c[5] + 1, NULL, 10);   /* ttl        */
        const char *op = c[4] + 1;
        size_t oplen   = (size_t)(c[5] - c[4] - 1);

        uint8_t cmd;
        if (oplen == 3 && !memcmp(op, "get", 3))      cmd = 0;
        else if (keep_sets && oplen == 3 && !memcmp(op, "set", 3)) cmd = 1;
        else { dropped_op++; continue; }

        if (cmd == 0 && vsize == 0) { dropped_zero++; continue; }  /* get miss */

        uint64_t key = mmh3_64((const uint8_t *)(c[0] + 1), (size_t)(c[1] - c[0] - 1));

        uint8_t *r = obuf + ofill;
        put64(r,      ts * 1000ULL);
        r[8] = cmd;
        put64(r + 9,  key);
        put32(r + 17, (uint32_t)vsize);
        put32(r + 21, (uint32_t)ttl);
        ofill += 25; out++;
        if (ofill == sizeof obuf) { fwrite(obuf, 1, ofill, stdout); ofill = 0; }
    }
    if (ofill) fwrite(obuf, 1, ofill, stdout);
    fflush(stdout);
    fprintf(stderr,
        "twcsv2bin: read %llu lines -> wrote %llu records (%llu bytes); "
        "dropped %llu non-get/set, %llu zero-size gets, %llu malformed\n",
        in, out, out*25ULL, dropped_op, dropped_zero, badcols);
    return 0;
}
