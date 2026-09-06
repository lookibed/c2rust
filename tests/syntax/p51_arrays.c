/* Audit acceptance: C arrays must have storage and C layout — local
 * uninitialised and initialised arrays, nested arrays, arrays of structs,
 * arrays inside structs (copyable), large globals, sizeof, decay.
 * Uses while loops only. Returns 0 on success. */

static int local_uninit(void) {
    int a[4];
    a[0] = 1;
    a[1] = 2;
    a[2] = a[0] + a[1];
    a[3] = a[2] * 2;
    return a[3]; /* 6 */
}

static int local_init(void) {
    int a[5] = { 1, 2, 3 }; /* trailing elements zero */
    int s = 0;
    int i = 0;
    while (i < 5) {
        s += a[i];
        i++;
    }
    return s; /* 6 */
}

static int nested_arr(void) {
    int m[2][3] = { { 1, 2, 3 }, { 4, 5, 6 } };
    return m[1][2] * 10 + m[0][1]; /* 62 */
}

struct pt {
    int x;
    int y;
};

static int arr_of_struct(void) {
    struct pt ps[3] = { { 1, 2 }, { 3, 4 }, { 5, 6 } };
    ps[1].x = 30;
    return ps[1].x + ps[2].y; /* 36 */
}

struct box {
    int n;
    int v[4];
};

static int arr_in_struct(void) {
    struct box b = { 2, { 7, 8, 9, 10 } };
    struct box c = b; /* deep copy */
    c.v[0] = 100;
    return b.v[0] * 1000 + c.v[0] + b.v[3]; /* 7110 */
}

static int g_arr[20000];

static int big_global(void) {
    g_arr[19999] = 5;
    return g_arr[19999] + g_arr[0]; /* 5 */
}

static int sizeof_arr(void) {
    int a[7];
    return sizeof(a) / sizeof(a[0]); /* 7 */
}

static int decay(int *p, int n) {
    int s = 0;
    while (n--) s += *p++;
    return s;
}

static int decay_call(void) {
    int a[3] = { 4, 5, 6 };
    return decay(a, 3); /* 15 */
}

static int ptr_write(void) {
    int a[3] = { 0, 0, 0 };
    int *p = a + 1;
    *p = 9;
    p[1] = 8;
    return a[1] * 10 + a[2]; /* 98 */
}

static char cs[8];

static int char_arr(void) {
    cs[0] = 'h';
    cs[1] = 0;
    return cs[0] == 'h' && cs[1] == 0;
}

static int str_init(void) {
    char s[] = "abc";
    return s[0] == 'a' && s[3] == 0 && sizeof(s) == 4;
}

static int struct_ptr_array_field(void) {
    struct box b = { 1, { 1, 2, 3, 4 } };
    struct box *q = &b;
    q->v[2] = 33;
    q->n = 9;
    return b.v[2] + b.n + q->v[3]; /* 46 */
}

int arrays_runtime(void) {
    if (local_uninit() != 6) return 1;
    if (local_init() != 6) return 2;
    if (nested_arr() != 62) return 3;
    if (arr_of_struct() != 36) return 4;
    if (arr_in_struct() != 7110) return 5;
    if (big_global() != 5) return 6;
    if (sizeof_arr() != 7) return 7;
    if (decay_call() != 15) return 8;
    if (ptr_write() != 98) return 9;
    if (!char_arr()) return 10;
    if (!str_init()) return 11;
    if (struct_ptr_array_field() != 46) return 12;
    return 0;
}
