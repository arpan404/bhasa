#include "collections.h"
#include <limits.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct bhasa_list {
    int64_t length, capacity;
    size_t stride;
    bhasa_slot_clone clone;
    bhasa_slot_drop drop;
    unsigned char *data;
};
typedef struct bucket { uint64_t hash; unsigned char state; } bucket;
struct bhasa_map {
    int64_t length, used, capacity;
    size_t key_stride, value_stride;
    bhasa_slot_clone key_clone, value_clone;
    bhasa_slot_drop key_drop, value_drop;
    bhasa_key_hash hash;
    bhasa_key_equal equal;
    bucket *buckets;
    unsigned char *keys, *values;
};
static _Noreturn void fail(const char *message) {
    fprintf(stderr, "Bhasa runtime error: %s\n", message);
    exit(1);
}
static void require_slot(const void *slot) {
    if (slot == NULL) fail("null collection slot");
}
static size_t stride_for(int64_t size) {
    if (size <= 0 || (uint64_t)size > SIZE_MAX - 7 || size > INT64_MAX - 7)
        fail("invalid collection slot size");
    return ((size_t)size + 7) & ~(size_t)7;
}
static size_t bytes_for(int64_t capacity, size_t stride) {
    if (capacity < 0 || stride == 0 || (uint64_t)capacity > SIZE_MAX / stride)
        fail("collection capacity overflow");
    return (size_t)capacity * stride;
}
static void *allocate(size_t bytes) {
    void *result = malloc(bytes == 0 ? 1 : bytes);
    if (result == NULL) fail("allocation failed");
    return result;
}
static void output_valid(const void *output, const void *data, size_t bytes) {
    require_slot(output);
    if (data != NULL && (uintptr_t)output >= (uintptr_t)data &&
        (uintptr_t)output - (uintptr_t)data < bytes)
        fail("collection output must use independent uninitialized storage");
}
static void list_valid(const bhasa_list *list) {
    if (list == NULL || list->length < 0 || list->capacity < list->length ||
        list->stride == 0 || list->clone == NULL || list->drop == NULL ||
        (list->capacity != 0 && list->data == NULL)) fail("invalid list");
}
static void *list_slot(const bhasa_list *list, int64_t index) {
    if (index < 0 || index >= list->length) fail("list index out of bounds");
    return list->data + bytes_for(index, list->stride);
}
static void reserve_list(bhasa_list *list, int64_t needed) {
    if (needed <= list->capacity) return;
    (void)bytes_for(needed, list->stride);
    int64_t next = list->capacity < 16 ? 16 : list->capacity;
    while (next < needed) {
        if (next > INT64_MAX / 2 || (uint64_t)next > SIZE_MAX / list->stride / 2) {
            next = needed;
            break;
        }
        next *= 2;
    }
    if ((uint64_t)next > SIZE_MAX / list->stride) next = needed;
    unsigned char *data = allocate(bytes_for(next, list->stride));
    if (list->length != 0) memcpy(data, list->data, bytes_for(list->length, list->stride));
    free(list->data); list->data = data; list->capacity = next;
}
bhasa_list *bhasa_rt_list_new(int64_t size, bhasa_slot_clone clone, bhasa_slot_drop drop) {
    size_t stride = stride_for(size);
    if (clone == NULL || drop == NULL) fail("invalid list callbacks");
    bhasa_list *list = allocate(sizeof(*list));
    *list = (bhasa_list){0, 0, stride, clone, drop, NULL};
    return list;
}
int64_t bhasa_rt_list_count(const bhasa_list *list) { list_valid(list); return list->length; }
void bhasa_rt_list_get(const bhasa_list *list, int64_t index, void *output) {
    list_valid(list); const void *slot = list_slot(list, index);
    output_valid(output, list->data, bytes_for(list->capacity, list->stride));
    list->clone(output, slot);
}
void bhasa_rt_list_push(bhasa_list *list, const void *value) {
    list_valid(list); require_slot(value);
    if (list->length == INT64_MAX) fail("collection capacity overflow");
    void *snapshot = allocate(list->stride); list->clone(snapshot, value);
    reserve_list(list, list->length + 1);
    memcpy(list->data + bytes_for(list->length, list->stride), snapshot, list->stride);
    ++list->length; free(snapshot); /* Move the cloned slot, never drop it. */
}
void bhasa_rt_list_set(bhasa_list *list, int64_t index, const void *value) {
    list_valid(list); void *slot = list_slot(list, index); require_slot(value);
    void *snapshot = allocate(list->stride); list->clone(snapshot, value);
    list->drop(slot); memcpy(slot, snapshot, list->stride); free(snapshot);
}
void bhasa_rt_list_clear(bhasa_list *list) {
    list_valid(list);
    for (int64_t i = 0; i < list->length; ++i) list->drop(list->data + bytes_for(i, list->stride));
    list->length = 0;
}
void bhasa_rt_list_free(bhasa_list *list) {
    if (list == NULL) return;
    bhasa_rt_list_clear(list); free(list->data); free(list);
}
bhasa_list *bhasa_rt_list_clone(const bhasa_list *list) {
    list_valid(list);
    bhasa_list *copy = bhasa_rt_list_new((int64_t)list->stride, list->clone, list->drop);
    reserve_list(copy, list->length);
    for (int64_t i = 0; i < list->length; ++i) {
        copy->clone(copy->data + bytes_for(i, copy->stride), list->data + bytes_for(i, list->stride));
        ++copy->length;
    }
    return copy;
}
static void map_valid(const bhasa_map *map) {
    if (map == NULL || map->length < 0 || map->used < map->length || map->capacity < map->used ||
        map->key_stride == 0 || map->value_stride == 0 || map->key_clone == NULL ||
        map->key_drop == NULL || map->value_clone == NULL || map->value_drop == NULL ||
        map->hash == NULL || map->equal == NULL ||
        (map->capacity != 0 && (map->buckets == NULL || map->keys == NULL || map->values == NULL ||
            (map->capacity & (map->capacity - 1)) != 0))) fail("invalid map");
}
static void *key_slot(const bhasa_map *map, int64_t index) { return map->keys + bytes_for(index, map->key_stride); }
static void *value_slot(const bhasa_map *map, int64_t index) { return map->values + bytes_for(index, map->value_stride); }
/* Returns an existing key or the first available insertion bucket. */
static int64_t locate(const bhasa_map *map, const void *key, uint64_t hash, int *found) {
    *found = 0;
    if (map->capacity == 0) return -1;
    uint64_t mask = (uint64_t)map->capacity - 1;
    uint64_t start = hash & mask; int64_t deleted = -1;
    for (uint64_t n = 0; n < (uint64_t)map->capacity; ++n) {
        int64_t i = (int64_t)((start + n) & mask);
        if (map->buckets[i].state == 0) return deleted >= 0 ? deleted : i;
        if (map->buckets[i].state == 2) { if (deleted < 0) deleted = i; }
        else if (map->buckets[i].hash == hash && map->equal(key_slot(map, i), key)) { *found = 1; return i; }
    }
    return deleted;
}
static void rehash(bhasa_map *map, int64_t capacity) {
    size_t bucket_bytes = bytes_for(capacity, sizeof(bucket));
    size_t key_bytes = bytes_for(capacity, map->key_stride);
    size_t value_bytes = bytes_for(capacity, map->value_stride);
    bucket *buckets = allocate(bucket_bytes); memset(buckets, 0, bucket_bytes);
    unsigned char *keys = allocate(key_bytes), *values = allocate(value_bytes);
    uint64_t mask = (uint64_t)capacity - 1;
    for (int64_t i = 0; i < map->capacity; ++i) {
        if (map->buckets[i].state != 1) continue;
        uint64_t at = map->buckets[i].hash & mask;
        while (buckets[at].state != 0) at = (at + 1) & mask;
        buckets[at] = map->buckets[i];
        memcpy(keys + bytes_for((int64_t)at, map->key_stride), key_slot(map, i), map->key_stride);
        memcpy(values + bytes_for((int64_t)at, map->value_stride), value_slot(map, i), map->value_stride);
    }
    free(map->buckets); free(map->keys); free(map->values);
    map->buckets = buckets; map->keys = keys; map->values = values;
    map->capacity = capacity; map->used = map->length;
}
bhasa_map *bhasa_rt_map_new(int64_t key_size, int64_t value_size,
    bhasa_slot_clone key_clone, bhasa_slot_drop key_drop,
    bhasa_slot_clone value_clone, bhasa_slot_drop value_drop,
    bhasa_key_hash hash, bhasa_key_equal equal) {
    size_t key_stride = stride_for(key_size), value_stride = stride_for(value_size);
    if (key_clone == NULL || key_drop == NULL || value_clone == NULL || value_drop == NULL || hash == NULL || equal == NULL)
        fail("invalid map callbacks");
    bhasa_map *map = allocate(sizeof(*map));
    *map = (bhasa_map){0, 0, 0, key_stride, value_stride, key_clone, value_clone, key_drop, value_drop, hash, equal, NULL, NULL, NULL};
    return map;
}
int64_t bhasa_rt_map_count(const bhasa_map *map) { map_valid(map); return map->length; }
static int storage_overlaps(const void *left, size_t left_size, const void *right, size_t right_size) {
    if (left_size == 0 || right_size == 0) return 0;
    uintptr_t a = (uintptr_t)left, b = (uintptr_t)right;
    /* Subtraction avoids overflowing an end address. */
    return a >= b ? a - b < right_size : b - a < left_size;
}
static void iteration_storage_valid(const bhasa_map *map, const void *storage, size_t size) {
    require_slot(storage);
    if ((uintptr_t)storage % 8 != 0 || size - 1 > UINTPTR_MAX - (uintptr_t)storage)
        fail("invalid map iteration storage");
    if (storage_overlaps(storage, size, map, sizeof(*map)) ||
        storage_overlaps(storage, size, map->buckets, bytes_for(map->capacity, sizeof(bucket))) ||
        storage_overlaps(storage, size, map->keys, bytes_for(map->capacity, map->key_stride)) ||
        storage_overlaps(storage, size, map->values, bytes_for(map->capacity, map->value_stride)))
        fail("map iteration storage must be independent");
}
int64_t bhasa_rt_map_next_key(const bhasa_map *snapshot, int64_t *cursor, void *output) {
    map_valid(snapshot);
    /* Validate all aliases before reading or updating the cursor. The output
     * callback and cursor write must not corrupt the container or each other. */
    iteration_storage_valid(snapshot, cursor, sizeof(*cursor));
    iteration_storage_valid(snapshot, output, snapshot->key_stride);
    if (storage_overlaps(cursor, sizeof(*cursor), output, snapshot->key_stride))
        fail("map iteration storage must be independent");
    int64_t index = *cursor;
    if (index < 0 || index > snapshot->capacity) fail("invalid map iteration cursor");
    while (index < snapshot->capacity) {
        if (snapshot->buckets[index].state == 1) {
            snapshot->key_clone(output, key_slot(snapshot, index));
            *cursor = index + 1;
            return 1;
        }
        ++index;
    }
    *cursor = snapshot->capacity;
    return 0;
}
int64_t bhasa_rt_map_contains(const bhasa_map *map, const void *key) {
    map_valid(map); require_slot(key); int found; (void)locate(map, key, map->hash(key), &found); return found;
}
void bhasa_rt_map_get(const bhasa_map *map, const void *key, void *output) {
    map_valid(map); require_slot(key); int found; int64_t i = locate(map, key, map->hash(key), &found);
    if (!found) fail("map key not found");
    output_valid(output, map->keys, bytes_for(map->capacity, map->key_stride));
    output_valid(output, map->values, bytes_for(map->capacity, map->value_stride));
    map->value_clone(output, value_slot(map, i));
}
void bhasa_rt_map_put(bhasa_map *map, const void *key, const void *value) {
    map_valid(map); require_slot(key); require_slot(value);
    void *key_copy = allocate(map->key_stride), *value_copy = allocate(map->value_stride);
    map->key_clone(key_copy, key); map->value_clone(value_copy, value);
    uint64_t hash = map->hash(key_copy); int found; int64_t i = locate(map, key_copy, hash, &found);
    if (found) {
        map->key_drop(key_slot(map, i)); map->value_drop(value_slot(map, i));
        memcpy(key_slot(map, i), key_copy, map->key_stride); memcpy(value_slot(map, i), value_copy, map->value_stride);
    } else {
        if (map->length == INT64_MAX) fail("collection capacity overflow");
        if (map->capacity == 0) rehash(map, 16);
        else if (map->used + 1 > map->capacity - map->capacity / 4) {
            int64_t next = map->capacity;
            if (map->length + 1 > map->capacity / 2) {
                if (next > INT64_MAX / 2) fail("collection capacity overflow");
                next *= 2;
            }
            rehash(map, next);
        }
        i = locate(map, key_copy, hash, &found);
        if (i < 0 || found) fail("invalid map insertion");
        if (map->buckets[i].state == 0) ++map->used;
        memcpy(key_slot(map, i), key_copy, map->key_stride); memcpy(value_slot(map, i), value_copy, map->value_stride);
        map->buckets[i] = (bucket){hash, 1}; ++map->length;
    }
    free(key_copy); free(value_copy);
}
int64_t bhasa_rt_map_remove(bhasa_map *map, const void *key) {
    map_valid(map); require_slot(key); int found; int64_t i = locate(map, key, map->hash(key), &found);
    if (!found) return 0;
    map->key_drop(key_slot(map, i)); map->value_drop(value_slot(map, i));
    map->buckets[i].state = 2; --map->length; return 1;
}
void bhasa_rt_map_clear(bhasa_map *map) {
    map_valid(map);
    for (int64_t i = 0; i < map->capacity; ++i) {
        if (map->buckets[i].state == 1) { map->key_drop(key_slot(map, i)); map->value_drop(value_slot(map, i)); }
        map->buckets[i].state = 0;
    }
    map->length = 0; map->used = 0;
}
void bhasa_rt_map_free(bhasa_map *map) {
    if (map == NULL) return;
    bhasa_rt_map_clear(map); free(map->buckets); free(map->keys); free(map->values); free(map);
}
bhasa_map *bhasa_rt_map_clone(const bhasa_map *map) {
    map_valid(map);
    bhasa_map *copy = bhasa_rt_map_new((int64_t)map->key_stride, (int64_t)map->value_stride,
        map->key_clone, map->key_drop, map->value_clone, map->value_drop, map->hash, map->equal);
    if (map->length != 0) rehash(copy, map->capacity);
    for (int64_t i = 0; i < map->capacity; ++i) {
        if (map->buckets[i].state == 1) bhasa_rt_map_put(copy, key_slot(map, i), value_slot(map, i));
    }
    return copy;
}
uint64_t bhasa_rt_map_hash_i64(const void *key) {
    require_slot(key); uint64_t value; memcpy(&value, key, sizeof(value));
    value ^= value >> 30; value *= UINT64_C(0xbf58476d1ce4e5b9);
    value ^= value >> 27; value *= UINT64_C(0x94d049bb133111eb);
    return value ^ (value >> 31);
}
int64_t bhasa_rt_map_equal_i64(const void *left, const void *right) {
    require_slot(left); require_slot(right); int64_t a, b; memcpy(&a, left, sizeof(a)); memcpy(&b, right, sizeof(b)); return a == b;
}
static const char *string_key(const void *slot) {
    require_slot(slot); const char *text; memcpy(&text, slot, sizeof(text)); require_slot(text); return text;
}
uint64_t bhasa_rt_map_hash_str(const void *key) {
    const unsigned char *text = (const unsigned char *)string_key(key);
    uint64_t hash = UINT64_C(14695981039346656037);
    while (*text != 0) { hash ^= *text++; hash *= UINT64_C(1099511628211); }
    return hash;
}
int64_t bhasa_rt_map_equal_str(const void *left, const void *right) { return strcmp(string_key(left), string_key(right)) == 0; }
