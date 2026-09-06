/* Audit acceptance: C integer semantics — 16-bit types, promotions,
 * narrowing casts, compound assignment conversions, inc/dec, single
 * evaluation of lvalues, _Bool, pointer casts. Returns 0 on success. */

#include <stdint.h>
#include <stdlib.h>

static int short_wrap(void) {
    short s = 32767;
    s++;
    return s == -32768;
}

static int ushort_wrap(void) {
    unsigned short u = 65535;
    u++;
    return u == 0;
}

static int short_assign(void) {
    short s = (short)70000;
    return s == 4464;
}

static int uchar_promotion(void) {
    unsigned char uc = 10;
    int i = -20;
    return (uc + i) < 0; /* unsigned char promotes to int, so 1 */
}

static int narrowing(void) {
    int a = 300;
    return (char)a + (unsigned char)a; /* 44 + 44 = 88 */
}

static int negative_index(void) {
    int *p = (int *)malloc(4 * sizeof(int));
    p[0] = 1; p[1] = 2; p[2] = 3; p[3] = 4;
    int *q = &p[2];
    long i = -1;
    int r = q[i] * 10 + q[-2];
    free(p);
    return r; /* 21 */
}

static int compound(void) {
    int i = 10;
    i *= 3;
    unsigned u = 7;
    i += u;
    long l = 5;
    i -= l;
    char c = 100;
    c += 100; /* wraps to -56 */
    unsigned char uc = 250;
    uc += 10; /* wraps to 4 */
    c &= ~1;
    return i * 1000 + (int)c * 10 + uc; /* 32000 - 560 + 4 = 31444 */
}

static int incdec(void) {
    unsigned char c = 255;
    c++;
    char d = 127;
    ++d;
    int *p = (int *)malloc(3 * sizeof(int));
    p[0] = 5; p[1] = 6; p[2] = 7;
    int *q = p;
    int v = *q++;
    int w = *q;
    q--;
    int x = *q;
    free(p);
    return c * 1000000 + (d == -128) * 100000 + v * 1000 + w * 10 + x; /* 105065 */
}

static int cell;
static int calls;
static int *get_cell(void) { calls++; return &cell; }

static int lvalue_once(void) {
    cell = 1;
    calls = 0;
    *get_cell() += 5;
    (*get_cell())++;
    return cell * 10 + calls; /* 72 */
}

static int bool_conv(void) {
    _Bool b = 5;
    _Bool z = 0;
    int p = 0;
    _Bool q = &p != 0;
    _Bool w = 3.5;
    return b * 8 + z * 4 + q * 2 + w; /* 11 */
}

static int ptr_bytes(void) {
    uint32_t v = 0x11223344u;
    unsigned char *p = (unsigned char *)&v;
    return p[0] == 0x44 && p[3] == 0x11;
}

static int enum_big(void) {
    enum { HUGE = 0x80000000u };
    return HUGE > 0;
}

static int sizeof_mul(unsigned long n) {
    unsigned long total = sizeof(int) * n;
    return total == 12;
}

static int shifts(void) {
    int a = -16;
    unsigned u = 0x80000000u;
    return (a >> 2) == -4 && (u >> 31) == 1 && (1u << 31) == 0x80000000u && (a * 2) == -32;
}

static int divmod(void) {
    return (-7 / 2) == -3 && (-7 % 2) == -1 && (7 / -2) == -3 && (7 % -2) == 1 && (7u / 2) == 3;
}

static int mixed_cmp(void) {
    int i = -1;
    unsigned u = 1;
    return (i < u) == 0; /* -1 converts to UINT_MAX */
}

static int wide(void) {
    int64_t big = 1;
    big <<= 40;
    uint64_t ub = (uint64_t)-1;
    return (big == 1099511627776LL) && (ub >> 63) == 1;
}

int integer_semantics_runtime(void) {
    if (!short_wrap()) return 1;
    if (!ushort_wrap()) return 2;
    if (!short_assign()) return 3;
    if (!uchar_promotion()) return 4;
    if (narrowing() != 88) return 5;
    if (negative_index() != 21) return 6;
    if (compound() != 31444) return 7;
    if (incdec() != 105065) return 8;
    if (lvalue_once() != 72) return 9;
    if (bool_conv() != 11) return 10;
    if (!ptr_bytes()) return 11;
    if (!enum_big()) return 12;
    if (!sizeof_mul(3)) return 13;
    if (!shifts()) return 14;
    if (!divmod()) return 15;
    if (!mixed_cmp()) return 16;
    if (!wide()) return 17;
    return 0;
}
