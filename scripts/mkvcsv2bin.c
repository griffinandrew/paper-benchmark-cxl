/* mkvcsv2bin.c -- Meta/CacheLib kvcache CSV -> 25-byte benchmark trace records.
 *
 * Same 25-byte record and same MurmurHash3_x64_128(key,0).h1 key hashing as
 * twcsv2bin.c (which was verified byte-exact against cluster12/37 masters), so
 * MetaKV traces are directly comparable to the Twitter ones.
 *
 * Schema (202401 / memcache 2024-08):
 *   op_time,key,key_size,op,op_count,size,cache_hits,ttl,usecase,sub_usecase
 * Schema (202210):
 *   op_time,key,key_size,op,op_count,size,cache_hits,ttl
 * Both are handled: fields are located by index from a --cols=N argument.
 *
 * Filter, matching the verified Twitter ".filt" rule:
 *   keep iff op is a GET form AND size > 0.
 *   Over 119,482 sampled 202401 rows, (GET* && size==0) <=> (cache_hits==0)
 *   with zero exceptions, so "size > 0" is exactly "this access hit and we
 *   know the value size" -- the same semantics as dropping Twitter's zero-size
 *   get misses.
 *
 * op_count: a CSV row is NOT one access. It collapses op_count identical
 * consecutive ops (mean 2.3-2.9, max 377). CacheBench itself replays each row
 * op_count times (req->repeats_, "repeatOpCount": true), so by default we
 * expand -- emitting one record per access. -1/--no-expand emits one per row.
 *
 * GET_LEASE/SET_LEASE fold into Get/Set exactly as CacheLib's own
 * KVReplayGenerator does. DELETE is dropped (no encoding in 25 bytes).
 *
 * build: cc -O2 -o mkvcsv2bin mkvcsv2bin.c
 * use:   curl -r 0-N <url> | ./mkvcsv2bin --cols=10 > out.bin
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


#define MAXSZ (64u << 20)   /* drop absurd objects: the benchmark materialises a
                             * real buffer of value_size per access, so a 1.8 GB
                             * CDN object would be pathological. */
enum { F_KV, F_BLOCK, F_CDN };

int main(int argc, char **argv)
{
    int ncols = 10, fmt = F_KV, expand = 1, keep_sets = 0;
    for (int i = 1; i < argc; i++) {
        if (!strncmp(argv[i], "--cols=", 7))                              ncols = atoi(argv[i] + 7);
        else if (!strcmp(argv[i], "--fmt=block"))                         fmt = F_BLOCK, ncols = 11;
        else if (!strcmp(argv[i], "--fmt=cdn"))                           fmt = F_CDN,   ncols = 15;
        else if (!strcmp(argv[i], "--fmt=kv"))                            fmt = F_KV;
        else if (!strcmp(argv[i],"-1") || !strcmp(argv[i],"--no-expand")) expand = 0;
        else if (!strcmp(argv[i],"-a") || !strcmp(argv[i],"--all"))       keep_sets = 1;
    }
    /* Field positions per family.
     *  kv 10: op_time,key,key_size,op,op_count,size,cache_hits,ttl,usecase,sub_usecase
     *  kv  8: op_time,key,key_size,op,op_count,size,cache_hits,ttl
     *  kv  6: key,op,size,op_count,key_size,ttl                    (no timestamp)
     *  kv  5: key,op,size,op_count,key_size                        (no timestamp, no ttl)
     *  block: op_time,block_id,block_id_size,io_size,io_offset,user_name,
     *         user_namespace,op_name,op_count,host_name,rs_shard_id
     *  cdn:   timestamp,cacheKey,OpType,objectSize,responseSize,responseHeaderSize,
     *         rangeStart,rangeEnd,TTL,SamplingRate,cache_hit,item_value,
     *         RequestHandler,cdn_content_type_id,vip_type
     * Eras without a timestamp emit ts=0; the benchmark discards timestamps
     * unless --native-time, which a streamed trace cannot use, so it is inert. */
    int I_TIME, I_KEY, I_OP, I_COUNT, I_SIZE, I_TTL;
    int ts_is_ms = 0;
    if (fmt == F_BLOCK)      { I_TIME=0; I_KEY=1; I_SIZE=3; I_OP=7; I_COUNT=8;  I_TTL=-1; }
    else if (fmt == F_CDN)   { I_TIME=0; I_KEY=1; I_OP=2;  I_SIZE=4; I_COUNT=-1; I_TTL=8; ts_is_ms=1; }
    else if (ncols >= 8)     { I_TIME=0; I_KEY=1; I_OP=3;  I_COUNT=4; I_SIZE=5; I_TTL=7; }
    else                     { I_TIME=-1;I_KEY=0; I_OP=1;  I_COUNT=3; I_SIZE=2;
                               I_TTL = (ncols >= 6) ? 5 : -1; }

    char *line = NULL; size_t cap = 0; ssize_t n;
    char *fld[20];
    unsigned long long wrote=0, rows=0, drop_op=0, drop_zero=0, drop_big=0, badcols=0;
    uint8_t rec[25];
    int header_checked = 0;

    while ((n = getline(&line, &cap, stdin)) > 0) {
        while (n > 0 && (line[n-1]=='\n' || line[n-1]=='\r')) line[--n] = 0;
        if (n == 0) continue;

        int f = 0; fld[f++] = line;
        for (char *p = line; *p && f < 20; p++) if (*p == ',') { *p = 0; fld[f++] = p + 1; }
        if (f != ncols) { badcols++; continue; }   /* truncated tail line lands here */

        if (!header_checked) {                     /* skip a header row if present */
            header_checked = 1;
            if (!strcmp(fld[I_KEY],"key") || !strcmp(fld[I_KEY],"cacheKey")
                || !strcmp(fld[I_KEY],"block_id") || !strcmp(fld[I_OP],"op")) continue;
        }

        const char *op = fld[I_OP];
        int cmd;
        if (fmt == F_BLOCK) {
            if      (!strncmp(op, "getChunkData", 12)) cmd = 0;
            else if (!strncmp(op, "putChunk",      8)) cmd = 1;
            else { drop_op++; continue; }
        } else if (fmt == F_CDN) {
            if (!strcmp(op, "1")) cmd = 0;         /* sole OpType observed: a fetch */
            else { drop_op++; continue; }
        } else {
            if      (!strcmp(op,"GET") || !strcmp(op,"GET_LEASE")) cmd = 0;
            else if (!strcmp(op,"SET") || !strcmp(op,"SET_LEASE")) cmd = 1;
            else { drop_op++; continue; }
        }
        if (cmd == 1 && !keep_sets) { drop_op++; continue; }

        unsigned long long vsize = strtoull(fld[I_SIZE], NULL, 10);
        if (fld[I_SIZE][0] == '-') vsize = 0;
        if (cmd == 0 && vsize == 0) { drop_zero++; continue; }
        if (vsize > MAXSZ)          { drop_big++;  continue; }

        unsigned long long tsraw = (I_TIME >= 0) ? strtoull(fld[I_TIME], NULL, 10) : 0ULL;
        unsigned long long ts    = ts_is_ms ? tsraw : tsraw * 1000ULL;
        unsigned long long ttl   = (I_TTL   >= 0) ? strtoull(fld[I_TTL], NULL, 10) : 0ULL;
        unsigned long long reps  = (expand && I_COUNT >= 0)
                                     ? strtoull(fld[I_COUNT], NULL, 10) : 1ULL;
        if (reps == 0) reps = 1;

        put64(rec, ts);
        rec[8] = (uint8_t)cmd;
        put64(rec + 9, mmh3_64((const uint8_t *)fld[I_KEY], strlen(fld[I_KEY])));
        put32(rec + 17, (uint32_t)vsize);
        put32(rec + 21, (uint32_t)ttl);

        for (unsigned long long r = 0; r < reps; r++) {
            if (fwrite(rec, 1, 25, stdout) != 25) { perror("write"); return 1; }
            wrote++;
        }
        rows++;
    }
    fflush(stdout);
    fprintf(stderr,
        "kept %llu rows -> %llu records (%llu B); dropped %llu op, %llu zero-size, "
        "%llu oversize(>64MB), %llu malformed\n",
        rows, wrote, wrote*25ULL, drop_op, drop_zero, drop_big, badcols);
    return 0;
}
