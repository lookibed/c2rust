/* Audit acceptance: double/float literals, comparisons with zero,
 * float arithmetic, conversions. No loops other than while. Returns 0 on success. */

static int dbl_literal(void) {
    double d = 1e12;
    return (long)d == 1000000000000L;
}

static int dbl_exact(void) {
    double d = 16777217.0; /* not representable as float */
    return (long)d == 16777217L;
}

static int dbl_third(void) {
    double d = 1.0 / 3.0;
    return d > 0.33333333 && d < 0.33333334 && (d * 3.0 == 1.0);
}

static int cmp_zero(void) {
    double d = 0.5;
    int a = (d == 0);
    int b = (d != 0);
    int c = d ? 1 : 0;
    int e = 0;
    if (d) e = 1;
    double n = -0.5;
    int f = 0;
    if (n) f = 1;
    return a * 10000 + b * 1000 + c * 100 + e * 10 + f; /* 1111 */
}

static int flt(void) {
    float f = 1.5f;
    f *= 2.0f;
    float h = 0.1f;
    return f == 3.0f && h > 0.09f && h < 0.11f;
}

static int not_dbl(void) {
    double d = 0.0;
    double e = 2.0;
    return (!d) * 10 + (e && 1); /* 11 */
}

static int conv(void) {
    double d = -2.7;
    int i = (int)d;
    float g = (float)7 / 2;
    return i * 100 + (int)(g * 10); /* -165 */
}

static int while_dbl(void) {
    double x = 100.0;
    int n = 0;
    while (x > 1.0) {
        x /= 2.0;
        n++;
    }
    return n; /* 7 */
}

static int mixed(void) {
    int i = 7;
    double d = i / 2;
    double e = i / 2.0;
    return (d == 3.0) * 10 + (e == 3.5); /* 11 */
}

static int neg_zero_and_unsigned(void) {
    unsigned u = 4000000000u;
    double d = u;
    float f = -0.0f;
    return d > 3.9e9 && f == 0.0f;
}

static double half(double x) { return x / 2; }
static float halff(float x) { return x / 2; }

static int calls(void) {
    return half(5) == 2.5 && halff(5) == 2.5f;
}

int floating_runtime(void) {
    if (!dbl_literal()) return 1;
    if (!dbl_exact()) return 2;
    if (!dbl_third()) return 3;
    if (cmp_zero() != 1111) return 4;
    if (!flt()) return 5;
    if (not_dbl() != 11) return 6;
    if (conv() != -165) return 7;
    if (while_dbl() != 7) return 8;
    if (mixed() != 11) return 9;
    if (!neg_zero_and_unsigned()) return 10;
    if (!calls()) return 11;
    return 0;
}
