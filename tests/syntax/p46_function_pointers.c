/* Audit acceptance: calls through function pointers, recursion and mutual
 * recursion with forward declarations. Returns 0 on success. */

typedef int (*binop)(int, int);

static int add(int a, int b) { return a + b; }
static int mul(int a, int b) { return a * b; }

static int apply(binop f, int a, int b) { return f(a, b); }

static binop pick(int which) { return which ? mul : add; }

struct ops {
    binop op;
    int k;
};

static int via_struct_value(void) {
    struct ops o = { mul, 3 };
    return o.op(7, o.k);
}

static int fact(int n) { return n <= 1 ? 1 : n * fact(n - 1); }

static int is_even(int n);
static int is_odd(int n) { return n == 0 ? 0 : is_even(n - 1); }
static int is_even(int n) { return n == 0 ? 1 : is_odd(n - 1); }

int function_pointer_runtime(void) {
    if (apply(add, 2, 3) != 5) return 1;
    if (apply(mul, 4, 5) != 20) return 2;
    binop f = pick(1);
    if (f(3, 3) != 9) return 3;
    f = pick(0);
    if ((*f)(3, 3) != 6) return 4;
    binop nul = 0;
    if (nul != 0) return 5;
    if (f == 0) return 6;
    if (via_struct_value() != 21) return 7;
    if (fact(5) != 120) return 8;
    if (!is_even(10) || is_even(7)) return 9;
    return 0;
}
