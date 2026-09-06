/* Audit negative case: a builtin without a daScript lowering must be a
 * strict-mode TranslationError, never a "safe default" constant. */

int unknown_builtin(void) {
    void *p = __builtin_alloca(16);
    return p != 0;
}
