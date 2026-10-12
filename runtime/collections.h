#ifndef BHASA_RUNTIME_COLLECTIONS_H
#define BHASA_RUNTIME_COLLECTIONS_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Opaque, single-owner containers. Slots are aligned to at least eight bytes.
 * Compiler-generated callbacks clone into uninitialized storage and drop one
 * initialized value. Callbacks must not mutate or reenter their container. */
typedef void (*bhasa_slot_clone)(void *destination, const void *source);
typedef void (*bhasa_slot_drop)(void *slot);
typedef uint64_t (*bhasa_key_hash)(const void *key);
typedef int64_t (*bhasa_key_equal)(const void *left, const void *right);
typedef struct bhasa_list bhasa_list;
typedef struct bhasa_map bhasa_map;

bhasa_list *bhasa_rt_list_new(int64_t size, bhasa_slot_clone clone, bhasa_slot_drop drop);
bhasa_list *bhasa_rt_list_clone(const bhasa_list *list);
void bhasa_rt_list_free(bhasa_list *list);
int64_t bhasa_rt_list_count(const bhasa_list *list);
/* get clones into caller-owned uninitialized storage, never exposing an
 * interior pointer. Invalid indices and missing map keys stop the program. */
void bhasa_rt_list_get(const bhasa_list *list, int64_t index, void *output);
void bhasa_rt_list_push(bhasa_list *list, const void *value);
void bhasa_rt_list_set(bhasa_list *list, int64_t index, const void *value);
void bhasa_rt_list_clear(bhasa_list *list);

bhasa_map *bhasa_rt_map_new(int64_t key_size, int64_t value_size,
    bhasa_slot_clone key_clone, bhasa_slot_drop key_drop,
    bhasa_slot_clone value_clone, bhasa_slot_drop value_drop,
    bhasa_key_hash hash, bhasa_key_equal equal);
bhasa_map *bhasa_rt_map_clone(const bhasa_map *map);
void bhasa_rt_map_free(bhasa_map *map);
int64_t bhasa_rt_map_count(const bhasa_map *map);
void bhasa_rt_map_get(const bhasa_map *map, const void *key, void *output);
void bhasa_rt_map_put(bhasa_map *map, const void *key, const void *value);
int64_t bhasa_rt_map_contains(const bhasa_map *map, const void *key);
int64_t bhasa_rt_map_remove(bhasa_map *map, const void *key);
void bhasa_rt_map_clear(bhasa_map *map);
/* Keys are pointer-to-slot, not the integer/string value itself. str is a
 * non-null, immutable, NUL-terminated UTF-8 pointer with static lifetime. */
uint64_t bhasa_rt_map_hash_i64(const void *key);
int64_t bhasa_rt_map_equal_i64(const void *left, const void *right);
uint64_t bhasa_rt_map_hash_str(const void *key);
int64_t bhasa_rt_map_equal_str(const void *left, const void *right);
#ifdef __cplusplus
}
#endif
#endif
