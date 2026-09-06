/* Audit acceptance: evaluation order and side effects of the comma
 * operator, short-circuit && / ||, ?: and GNU ?:. Returns 0 on success. */

static int g;

static int inc(void) { g++; return 1; }
static int zero(void) { g += 10; return 0; }

static int comma(void) {
    g = 0;
    int r = (inc(), inc(), 5);
    return g * 10 + r; /* 25 */
}

static int and_short(void) {
    g = 0;
    int x = 0;
    int r = zero() && (x = inc());
    return g * 100 + x * 10 + r; /* 1000 */
}

static int or_short(void) {
    g = 0;
    int x = 0;
    int r = inc() || (x = inc());
    return g * 100 + x * 10 + r; /* 101 */
}

static int and_full(void) {
    g = 0;
    int r = inc() && inc();
    return g * 10 + r; /* 21 */
}

static int ternary(void) {
    int i = 0, j = 0;
    int c = 1;
    int r = c ? i++ : j++;
    return i * 100 + j * 10 + r; /* 100 */
}

static int elvis(void) {
    g = 0;
    int r = inc() ?: zero();
    return g * 10 + r; /* 11 */
}

static int null_guard(int *p) { return p && *p == 3; }

static int chain(void) {
    g = 0;
    int r = inc() && zero() && inc();
    return g * 10 + r; /* 110 */
}

static int cond_assign(int a) {
    int x = 0;
    if (a && (x = 7)) return x;
    return -x;
}

static int while_effect(void) {
    g = 0;
    int n = 0;
    while (inc() && n < 3) n++;
    return g * 10 + n; /* 4 calls, n = 3 -> 43 */
}

int side_effects_runtime(void) {
    int v = 3;
    if (comma() != 25) return 1;
    if (and_short() != 1000) return 2;
    if (or_short() != 101) return 3;
    if (and_full() != 21) return 4;
    if (ternary() != 100) return 5;
    if (elvis() != 11) return 6;
    if (null_guard(0) != 0 || null_guard(&v) != 1) return 7;
    if (chain() != 110) return 8;
    if (cond_assign(0) != 0 || cond_assign(1) != 7) return 9;
    if (while_effect() != 43) return 10;
    return 0;
}
