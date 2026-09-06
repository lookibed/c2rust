/* Audit acceptance: goto-driven control flow (state machine, cleanup,
 * backward jump, jump into a loop body). Returns 0 on success. */

/* Remainder of the nbits-bit number v modulo 3, reading bits MSB first. */
static int mod3(unsigned v, int nbits) {
    int i = nbits - 1;
    goto s0;
s0:
    if (i < 0) return 0;
    if ((v >> i) & 1u) { i--; goto s1; }
    i--;
    goto s0;
s1:
    if (i < 0) return 1;
    if ((v >> i) & 1u) { i--; goto s0; }
    i--;
    goto s2;
s2:
    if (i < 0) return 2;
    if ((v >> i) & 1u) { i--; goto s2; }
    i--;
    goto s1;
}

static int cleanup(int x) {
    int r = 0;
    if (x < 0) goto fail;
    r = x * 2;
    if (r > 100) goto fail;
    r += 1;
    goto done;
fail:
    r = -1;
done:
    return r;
}

static int backward(int n) {
    int s = 0;
again:
    s += n;
    n--;
    if (n > 0) goto again;
    return s;
}

static int into_loop_body(int n) {
    int s = 0;
    int i = 0;
    if (n > 2) goto mid;
    for (; i < n; i++) {
        s += i;
    mid:
        s += 10;
    }
    return s; /* n=1 -> 10, n=3 -> 33 */
}

int goto_runtime(void) {
    if (mod3(6u, 3) != 0) return 1;
    if (mod3(7u, 3) != 1) return 2;
    if (mod3(5u, 3) != 2) return 3;
    if (cleanup(5) != 11 || cleanup(-1) != -1 || cleanup(60) != -1) return 4;
    if (backward(4) != 10) return 5;
    if (into_loop_body(1) != 10 || into_loop_body(3) != 33) return 6;
    return 0;
}
