/* Audit acceptance: printer precedence, string/char literals, builtins.
 * Uses while loops only. Returns 0 on success. */

static int unary_prec(int a, int b) {
    return -(a + b) + ~(a | b) + !(a != b); /* a=1,b=2: -3 + -4 + 0 = -7 */
}

static int shift_mix(unsigned x) {
    return ((x >> 4) & 0xF) + (((~(x & 0x30)) >> 4) & 0xF); /* x=0x35: 3 + 12 = 15 */
}

static int cast_chain(long v) {
    return (int)(unsigned char)(v + 1); /* 254 -> 255 */
}

static int dbl_paren(double a, double b) {
    return (int)(-(a * b) / 2.0); /* 3,4 -> -6 */
}

static int str_escape(void) {
    const char *s = "a\"b\\c\n\t";
    return s[1] == '"' && s[3] == '\\' && s[5] == '\n' && s[6] == '\t' && s[7] == 0;
}

static int str_bytes(void) {
    const char *s = "\xff\x01";
    return (unsigned char)s[0] == 0xff && s[1] == 1;
}

static int char_lit(void) {
    char c = '\xff';
    return c == -1 && 'A' == 65 && '\n' == 10 && '\0' == 0;
}

static int str_len(const char *s) {
    int n = 0;
    while (*s) {
        s++;
        n++;
    }
    return n;
}

static int builtins(void) {
    return __builtin_popcount(0xFFu) == 8 && __builtin_clz(1u) == 31 && __builtin_ctz(8u) == 3 &&
           __builtin_bswap32(0x11223344u) == 0x44332211u && __builtin_ffs(8) == 4 &&
           __builtin_bswap16(0x1234) == 0x3412;
}

static int overflow(void) {
    int r;
    int o = __builtin_add_overflow(2147483647, 1, &r);
    int r2;
    int o2 = __builtin_mul_overflow(1000, 1000, &r2);
    return o == 1 && o2 == 0 && r2 == 1000000;
}

static int expect(int x) {
    if (__builtin_expect(x == 3, 1)) return 1;
    return 0;
}

static int neg_lit(void) {
    int a = -5;
    unsigned b = 4294967295u;
    long c = -9223372036854775807L - 1;
    return a == -5 && b == 0xFFFFFFFFu && c < 0;
}

static int paren_assign(int a, int b) {
    int r;
    r = (a = b) + 1; /* r = b + 1, a = b */
    return r * 10 + a; /* b=4: 54 */
}

int printer_runtime(void) {
    if (unary_prec(1, 2) != -7) return 1;
    if (shift_mix(0x35u) != 15) return 2;
    if (cast_chain(254) != 255) return 3;
    if (dbl_paren(3.0, 4.0) != -6) return 4;
    if (!str_escape()) return 5;
    if (!str_bytes()) return 6;
    if (!char_lit()) return 7;
    if (str_len("hello, world") != 12 || str_len("") != 0) return 8;
    if (!builtins()) return 9;
    if (!overflow()) return 10;
    if (expect(3) != 1 || expect(4) != 0) return 11;
    if (!neg_lit()) return 12;
    if (paren_assign(1, 4) != 54) return 13;
    return 0;
}
