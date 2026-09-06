/* Audit negative case: a call to a function that has no body in the
 * translation unit and no runtime lowering must be a strict-mode
 * TranslationError, never a bare call to a nonexistent daScript function. */

int absolute_value(int x);

int unknown_extern_call(void) {
    return absolute_value(-3);
}
