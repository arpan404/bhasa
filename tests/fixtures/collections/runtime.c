#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int64_t allocations, owned_clones, owned_drops;
static int reject_allocation;
static void *counted_malloc(size_t size) {
    if (reject_allocation) return NULL;
    void *pointer = malloc(size);
    if (pointer != NULL) ++allocations;
    return pointer;
}
static void counted_free(void *pointer) { if (pointer != NULL) --allocations; free(pointer); }
/* This white-box fixture can pass actual interior slots back to mutation APIs.
 * Generated Bhasa code only uses copy-out get; native alias handling is stricter. */
#define malloc counted_malloc
#define free counted_free
#include "../../../runtime/collections.c"
#undef malloc
#undef free

static void scalar_clone(void *out, const void *in) { memcpy(out, in, sizeof(int64_t)); }
static void scalar_drop(void *slot) { (void)slot; }
static uint64_t collisions(const void *key) { (void)key; return 7; }
typedef struct owned { int64_t number; char *text; bhasa_list *nested; } owned;
static void owned_clone(void *out, const void *in) {
    const owned *source = in; owned *target = out;
    target->number = source->number;
    size_t size = strlen(source->text) + 1;
    target->text = counted_malloc(size); assert(target->text != NULL); memcpy(target->text, source->text, size);
    target->nested = bhasa_rt_list_clone(source->nested); ++owned_clones;
}
static void owned_drop(void *slot) { owned *value = slot; counted_free(value->text); bhasa_rt_list_free(value->nested); ++owned_drops; }
static owned original(int64_t number) {
    bhasa_list *nested = bhasa_rt_list_new(8, scalar_clone, scalar_drop);
    bhasa_rt_list_push(nested, &number);
    return (owned){number, "owned bytes", nested};
}
static void original_free(owned *value) { bhasa_rt_list_free(value->nested); }
static void expect_owned(const owned *value, int64_t expected) {
    assert(value->number == expected); assert(strcmp(value->text, "owned bytes") == 0);
    int64_t nested; bhasa_rt_list_get(value->nested, 0, &nested); assert(nested == expected);
}
static bhasa_list *guard_list;
static bhasa_map *guard_map;
static void cleanup_guard(void) {
    reject_allocation = 0;
    if (guard_list != NULL) { assert(bhasa_rt_list_count(guard_list) == 1); int64_t value; bhasa_rt_list_get(guard_list, 0, &value); assert(value == 42); bhasa_rt_list_free(guard_list); }
    if (guard_map != NULL) { assert(bhasa_rt_map_count(guard_map) == 1); int64_t key = 1, value; bhasa_rt_map_get(guard_map, &key, &value); assert(value == 42); bhasa_rt_map_free(guard_map); }
    assert(allocations == 0);
}
static int guards(const char *mode) {
    assert(atexit(cleanup_guard) == 0);
    if (strcmp(mode, "null-list") == 0) { (void)bhasa_rt_list_count(NULL); return 1; }
    if (strcmp(mode, "bad-size") == 0) { (void)bhasa_rt_list_new(0, scalar_clone, scalar_drop); return 1; }
    if (strcmp(mode, "overflow-size") == 0) { (void)bhasa_rt_list_new(INT64_MAX, scalar_clone, scalar_drop); return 1; }
    if (strcmp(mode, "bad-callback") == 0) { (void)bhasa_rt_map_new(8, 8, scalar_clone, scalar_drop, scalar_clone, scalar_drop, NULL, bhasa_rt_map_equal_i64); return 1; }
    int64_t value = 42;
    if (strncmp(mode, "list-", 5) == 0) {
        guard_list = bhasa_rt_list_new(8, scalar_clone, scalar_drop); bhasa_rt_list_push(guard_list, &value);
        if (strcmp(mode, "list-get") == 0) bhasa_rt_list_get(guard_list, -1, &value);
        if (strcmp(mode, "list-set") == 0) bhasa_rt_list_set(guard_list, 1, &value);
        if (strcmp(mode, "list-output") == 0) bhasa_rt_list_get(guard_list, 0, guard_list->data);
        if (strcmp(mode, "list-allocation") == 0) { reject_allocation = 1; bhasa_rt_list_push(guard_list, &value); }
        return 1;
    }
    int64_t key = 1;
    guard_map = bhasa_rt_map_new(8, 8, scalar_clone, scalar_drop, scalar_clone, scalar_drop, bhasa_rt_map_hash_i64, bhasa_rt_map_equal_i64);
    bhasa_rt_map_put(guard_map, &key, &value); key = 2;
    if (strcmp(mode, "map-missing") == 0) bhasa_rt_map_get(guard_map, &key, &value);
    if (strcmp(mode, "map-allocation") == 0) { reject_allocation = 1; bhasa_rt_map_put(guard_map, &key, &value); }
    return 1;
}
static void list_checks(void) {
    bhasa_list *list = bhasa_rt_list_new((int64_t)sizeof(owned), owned_clone, owned_drop);
    for (int64_t i = 0; i < 16; ++i) { owned value = original(i); bhasa_rt_list_push(list, &value); original_free(&value); }
    /* An interior value survives growth, and self set acquires before drop. */
    bhasa_rt_list_push(list, list_slot(list, 0)); assert(bhasa_rt_list_count(list) == 17);
    bhasa_rt_list_set(list, 16, list_slot(list, 16));
    bhasa_rt_list_set(list, 0, list_slot(list, 10));
    owned output; bhasa_rt_list_get(list, 0, &output); expect_owned(&output, 10);
    bhasa_list *copy = bhasa_rt_list_clone(list);
    bhasa_rt_list_clear(list); bhasa_rt_list_clear(list); bhasa_rt_list_free(list);
    expect_owned(&output, 10); owned_drop(&output);
    bhasa_rt_list_get(copy, 16, &output); expect_owned(&output, 0); owned_drop(&output);
    bhasa_rt_list_free(copy);
    /* One-byte values exercise slot padding and alignment under UBSan. */
}
static void byte_clone(void *out, const void *in) { *(unsigned char *)out = *(const unsigned char *)in; }
static void map_checks(void) {
    bhasa_map *map = bhasa_rt_map_new(8, (int64_t)sizeof(owned), scalar_clone, scalar_drop, owned_clone, owned_drop, collisions, bhasa_rt_map_equal_i64);
    for (int64_t i = 0; i < 12; ++i) { owned value = original(i); bhasa_rt_map_put(map, &i, &value); original_free(&value); }
    int64_t zero = 0, extra = 12; int found; int64_t at = locate(map, &zero, collisions(&zero), &found); assert(found);
    bhasa_rt_map_put(map, key_slot(map, at), value_slot(map, at));
    at = locate(map, &zero, collisions(&zero), &found);
    bhasa_rt_map_put(map, &extra, value_slot(map, at)); /* Growth with aliased input. */
    for (int64_t i = 13; i < 1024; ++i) { owned value = original(i); bhasa_rt_map_put(map, &i, &value); original_free(&value); }
    assert(bhasa_rt_map_count(map) == 1024);
    for (int64_t i = 1; i < 1024; i += 2) { assert(bhasa_rt_map_remove(map, &i) == 1); assert(bhasa_rt_map_remove(map, &i) == 0); }
    for (int64_t i = 0; i < 1024; ++i) { assert(bhasa_rt_map_contains(map, &i) == (i % 2 == 0)); if (i % 2 == 0) { owned out; bhasa_rt_map_get(map, &i, &out); expect_owned(&out, i == 12 ? 0 : i); owned_drop(&out); } }
    for (int64_t i = 1; i < 1024; i += 2) { owned value = original(i); bhasa_rt_map_put(map, &i, &value); original_free(&value); }
    bhasa_map *copy = bhasa_rt_map_clone(map); assert(bhasa_rt_map_count(copy) == 1024);
    owned survivor; bhasa_rt_map_get(copy, &zero, &survivor);
    bhasa_rt_map_clear(map); bhasa_rt_map_clear(map); bhasa_rt_map_free(map);
    bhasa_rt_map_free(copy); expect_owned(&survivor, 0); owned_drop(&survivor);
    /* String equality is by contents, not address. Keys have static lifetimes. */
    static const char first[] = "same"; static const char second[] = "same";
    const char *a = first, *b = second, *unicode = "भाषा";
    assert(a != b); assert(bhasa_rt_map_hash_str(&a) == bhasa_rt_map_hash_str(&b));
    map = bhasa_rt_map_new((int64_t)sizeof(char *), 8, scalar_clone, scalar_drop, scalar_clone, scalar_drop, bhasa_rt_map_hash_str, bhasa_rt_map_equal_str);
    int64_t value = 42; bhasa_rt_map_put(map, &a, &value); ++value; bhasa_rt_map_put(map, &b, &value);
    assert(bhasa_rt_map_count(map) == 1); int64_t result; bhasa_rt_map_get(map, &a, &result); assert(result == 43);
    bhasa_rt_map_put(map, &unicode, &value); assert(bhasa_rt_map_contains(map, &unicode)); bhasa_rt_map_free(map);
}
int main(int argc, char **argv) {
    if (argc == 2) return guards(argv[1]);
    for (int i = 0; i < 4; ++i) { list_checks(); map_checks(); assert(owned_clones == owned_drops); assert(allocations == 0); }
    bhasa_list *bytes = bhasa_rt_list_new(1, byte_clone, scalar_drop);
    for (int64_t i = 0; i < 257; ++i) { unsigned char value = (unsigned char)i; bhasa_rt_list_push(bytes, &value); }
    for (int64_t i = 0; i < 257; ++i) { unsigned char value; bhasa_rt_list_get(bytes, i, &value); assert(value == (unsigned char)i); }
    bhasa_rt_list_free(bytes); bhasa_rt_list_free(NULL); bhasa_rt_map_free(NULL);
    assert(allocations == 0); assert(owned_clones == owned_drops);
    puts("generic collections runtime passed"); return 0;
}
