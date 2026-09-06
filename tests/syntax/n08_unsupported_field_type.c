/* Audit negative case: a record field whose C type has no daScript
 * representation (vector type) must fail strict translation instead of
 * being approximated as int64/auto. */

typedef int v4 __attribute__((vector_size(16)));

struct holder {
    int tag;
    v4 lanes;
};

int unsupported_field_type(void) {
    struct holder h;
    h.tag = 1;
    return h.tag;
}
