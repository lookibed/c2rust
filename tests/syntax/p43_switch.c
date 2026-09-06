/* Audit acceptance: switch dispatch, fallthrough, default placement,
 * switch inside loops with break/continue. Returns 0 on success. */

static int classify(int x) {
    int r = 0;
    switch (x) {
    case 1:
        r += 10; /* falls through */
    case 2:
        r += 20;
        break;
    case 3:
        r += 30;
        break;
    default:
        r += 1000; /* falls through into case 4 */
    case 4:
        r += 40;
        break;
    }
    return r;
}

static int in_loop(void) {
    int acc = 0;
    for (int i = 0; i < 6; i++) {
        switch (i % 3) {
        case 0:
            continue; /* continues the for loop */
        case 1:
            acc += i;
            break; /* leaves the switch only */
        default:
            acc += 100;
        }
        acc += 1;
    }
    return acc; /* 209 */
}

static int nested_switch(int a, int b) {
    switch (a) {
    case 0:
        switch (b) {
        case 0:
            return 1;
        default:
            return 2;
        }
    case 1:
        if (b) return 3;
        return 4;
    }
    return 5;
}

static int char_switch(char c) {
    switch (c) {
    case 'a':
    case 'e':
    case 'i':
        return 1;
    default:
        return 0;
    }
}

static int no_default_no_match(int x) {
    int r = 7;
    switch (x) {
    case 1:
        r = 1;
        break;
    }
    return r;
}

int switch_runtime(void) {
    if (classify(1) != 30) return 1;
    if (classify(2) != 20) return 2;
    if (classify(3) != 30) return 3;
    if (classify(4) != 40) return 4;
    if (classify(9) != 1040) return 5;
    if (in_loop() != 209) return 6;
    if (nested_switch(0, 0) != 1 || nested_switch(0, 5) != 2) return 7;
    if (nested_switch(1, 1) != 3 || nested_switch(1, 0) != 4 || nested_switch(7, 0) != 5) return 8;
    if (char_switch('e') != 1 || char_switch('z') != 0) return 9;
    if (no_default_no_match(1) != 1 || no_default_no_match(2) != 7) return 10;
    return 0;
}
