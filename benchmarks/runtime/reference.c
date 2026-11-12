/* Same operations and wrapping Int32 arithmetic; all inputs arrive at runtime.
 * Bounded stack/pool storage models ordinary C on an MCU without malloc.
 * Pipelines materialize their containers; there is no algebraic sum shortcut.
 */
#include <stdint.h>
#include <stddef.h>

#define MAX_N 4096

#if defined(CASE_scalar) || defined(CASE_state)
int32_t reference(int32_t n, int32_t seed) {
    uint32_t value = (uint32_t)seed;
    for (int32_t i = 0; i < n; ++i)
        value = value * UINT32_C(1664525) + UINT32_C(1013904223);
    return (int32_t)value;
}
#elif defined(CASE_fibonacci)
static int32_t fib(int32_t n) {
    return n < 2 ? n : fib(n - 1) + fib(n - 2);
}
int32_t reference(int32_t n, int32_t seed) {
    return (int32_t)((uint32_t)fib(n) + (uint32_t)seed);
}
#elif defined(CASE_list_pipeline)
typedef struct Node { uint32_t value; struct Node *next; } Node;
int32_t reference(int32_t n, int32_t seed) {
    Node input[MAX_N], reversed[MAX_N], output[MAX_N];
    Node *xs = NULL, *ys = NULL, *zs = NULL;
    for (int32_t i = n; i > 0; --i) {
        input[i-1] = (Node){(uint32_t)(i-1), xs};
        xs = &input[i-1];
    }
    size_t used = 0;
    for (Node *p = xs; p != NULL; p = p->next) {
        reversed[used] = (Node){p->value * 3u + (uint32_t)seed, ys};
        ys = &reversed[used++];
    }
    used = 0;
    for (Node *p = ys; p != NULL; p = p->next) {
        output[used] = (Node){p->value, zs};
        zs = &output[used++];
    }
    uint32_t sum = 0;
    for (Node *p = zs; p != NULL; p = p->next) sum += p->value;
    return (int32_t)sum;
}
#elif defined(CASE_array_pipeline)
int32_t reference(int32_t n, int32_t seed) {
    uint32_t a[MAX_N], b[MAX_N];
    for (int32_t i = 0; i < n; ++i) a[i] = (uint32_t)i;
    for (int32_t i = 0; i < n; ++i) b[i] = a[i] * 3u + (uint32_t)seed;
    uint32_t sum = 0;
    for (int32_t i = 0; i < n; ++i) sum += b[i];
    return (int32_t)sum;
}
#elif defined(CASE_array_read)
int32_t reference(int32_t n, int32_t seed) {
    uint32_t a[MAX_N];
    for (int32_t i = 0; i < n; ++i) a[i] = (uint32_t)i;
    uint32_t sum = (uint32_t)seed;
    for (int32_t pass = 0; pass < 32; ++pass)
        for (int32_t i = 0; i < n; ++i) sum = sum * 33u + a[i];
    return (int32_t)sum;
}
#elif defined(CASE_array_update)
int32_t reference(int32_t n, int32_t seed) {
    uint32_t a[64];
    for (int32_t i = 0; i < 64; ++i) a[i] = (uint32_t)seed;
    for (int32_t left = n; left > 0; --left) a[left % 64] = (uint32_t)left;
    uint32_t sum = 0;
    for (int32_t i = 0; i < 64; ++i) sum += a[i];
    return (int32_t)sum;
}
#elif defined(CASE_closure)
typedef struct Closure { int32_t (*call)(int32_t, int32_t); int32_t x; } Closure;
static int32_t add(int32_t x, int32_t v) { return (int32_t)((uint32_t)x + (uint32_t)v); }
static Closure adder(int32_t x) { return (Closure){add, x}; }
int32_t reference(int32_t n, int32_t seed) {
    int32_t acc = seed;
    for (int32_t left = n; left > 0; --left) {
        Closure f = adder(left);
        acc = f.call(f.x, acc);
    }
    return acc;
}
#else
#error Select a benchmark CASE
#endif
