/* Audit acceptance: every C loop form must survive CFG reconstruction.
 * Returns 0 on success, otherwise the number of the first failing check. */

static int for_sum(int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        if (i == 3) continue;
        if (i == 7) break;
        s += i;
    }
    return s; /* 0+1+2+4+5+6 = 18 */
}

static int while_sum(int n) {
    int s = 0, i = 0;
    while (i < n) {
        s += i;
        i++;
    }
    return s; /* 45 for n = 10 */
}

static int do_while_continue(int n) {
    int s = 0;
    do {
        s += n;
        n--;
        if (n == 2) continue; /* continue must re-test the condition */
    } while (n > 0);
    return s; /* 4+3+2+1 = 10 for n = 4 */
}

static int nested(void) {
    int c = 0;
    for (int i = 0; i < 5; i++) {
        for (int j = 0; j < 5; j++) {
            if (j > i) break;
            if ((i + j) % 2) continue;
            c++;
        }
    }
    return c; /* 9 */
}

static int early_return(int n) {
    for (int i = 0;; i++) {
        if (i * i > n) return i;
    }
}

static int empty_body(int n) {
    int i = 0;
    while (i < n) i++;
    return i;
}

static int forever_break(void) {
    int k = 0;
    for (;;) {
        k += 3;
        if (k > 10) break;
    }
    return k; /* 12 */
}

static int comma_for(void) {
    int i, j, s = 0;
    for (i = 0, j = 10; i < j; i++, j--) s += j - i;
    return s; /* 10+8+6+4+2 = 30 */
}

int loops_runtime(void) {
    if (for_sum(10) != 18) return 1;
    if (while_sum(10) != 45) return 2;
    if (do_while_continue(4) != 10) return 3;
    if (nested() != 9) return 4;
    if (early_return(10) != 4) return 5;
    if (empty_body(7) != 7) return 6;
    if (forever_break() != 12) return 7;
    if (comma_for() != 30) return 8;
    return 0;
}
