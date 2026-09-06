/* Audit acceptance: function-scope static storage keeps state across calls
 * and is distinct per function even when names collide. Returns 0 on success. */

static int bump(void) {
    static int count = 0;
    count++;
    return count;
}

static int bump_step(int step) {
    static int total; /* zero-initialised */
    total += step;
    return total;
}

static int other(void) {
    static int count = 100; /* same name as in bump(), different object */
    count++;
    return count;
}

int static_local_runtime(void) {
    bump();
    bump();
    if (bump() != 3) return 1;
    bump_step(5);
    if (bump_step(7) != 12) return 2;
    if (other() != 101 || other() != 102) return 3;
    if (bump() != 4) return 4;
    return 0;
}
