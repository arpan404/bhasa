#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    assert!(
        !timed_out,
        "map iteration fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

#[test]
fn map_snapshot_keys_preserve_ownership_and_reject_destructive_aliases() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-iteration-runtime");
    let source = workspace.write("runtime.c", HARNESS);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("map-iteration");
        succeeds(&bounded(
            Command::new(&clang)
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-pedantic",
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                    optimization,
                ])
                .arg("-I")
                .arg(fixture("runtime"))
                .arg(&source)
                .arg("-o")
                .arg(&executable),
        ));
        let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        assert_eq!(stdout(&output), "map iteration runtime passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
        for (mode, diagnostic) in [
            ("null-map", "invalid map"),
            ("null-cursor", "null collection slot"),
            ("null-output", "null collection slot"),
            ("negative", "invalid map iteration cursor"),
            ("past-end", "invalid map iteration cursor"),
            ("maximum", "invalid map iteration cursor"),
            ("misaligned-cursor", "invalid map iteration storage"),
            ("misaligned-output", "invalid map iteration storage"),
            ("wrapped-output", "invalid map iteration storage"),
            ("cursor-header", "map iteration storage must be independent"),
            (
                "cursor-buckets",
                "map iteration storage must be independent",
            ),
            ("cursor-keys", "map iteration storage must be independent"),
            ("cursor-values", "map iteration storage must be independent"),
            ("output-header", "map iteration storage must be independent"),
            (
                "output-buckets",
                "map iteration storage must be independent",
            ),
            ("output-keys", "map iteration storage must be independent"),
            ("output-values", "map iteration storage must be independent"),
            ("same-storage", "map iteration storage must be independent"),
            (
                "partial-cursor",
                "map iteration storage must be independent",
            ),
            (
                "partial-header",
                "map iteration storage must be independent",
            ),
        ] {
            let output = bounded(
                Command::new(&executable)
                    .arg(mode)
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert_eq!(
                output.status.code(),
                Some(1),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                stderr(&output).contains(&format!("Bhasa runtime error: {diagnostic}")),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                !stderr(&output).contains("AddressSanitizer")
                    && !stderr(&output).contains("UndefinedBehaviorSanitizer"),
                "{mode}: {}",
                describe(&output)
            );
        }
    }
}

// Include the actual runtime to test otherwise-inaccessible storage aliases.
// Generated code only passes independent cursor/output stack slots.
const HARNESS: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int64_t allocations, key_clones, key_drops, value_clones, value_drops;
static void *counted_malloc(size_t size) {
    void *pointer = malloc(size);
    assert(pointer != NULL);
    ++allocations;
    return pointer;
}
static void counted_free(void *pointer) {
    if (pointer != NULL) --allocations;
    assert(allocations >= 0);
    free(pointer);
}
#define malloc counted_malloc
#define free counted_free
#include "collections.c"
#undef malloc
#undef free

static void int_clone(void *out, const void *in) {
    memcpy(out, in, sizeof(int64_t)); ++key_clones;
}
static void pair_clone(void *out, const void *in) {
    memcpy(out, in, sizeof(int64_t) * 2); ++key_clones;
}
static void str_clone(void *out, const void *in) {
    memcpy(out, in, sizeof(char *)); ++key_clones;
}
static void key_drop(void *slot) { (void)slot; ++key_drops; }
typedef struct owned { int64_t number; char *text; } owned;
static void owned_clone(void *out, const void *in) {
    const owned *source = in; owned *target = out;
    target->number = source->number;
    size_t size = strlen(source->text) + 1;
    target->text = counted_malloc(size);
    memcpy(target->text, source->text, size); ++value_clones;
}
static void owned_drop(void *slot) {
    owned *value = slot; counted_free(value->text); ++value_drops;
}
static bhasa_map *int_map(bhasa_key_hash hash) {
    return bhasa_rt_map_new(8, (int64_t)sizeof(owned), int_clone, key_drop,
        owned_clone, owned_drop, hash, bhasa_rt_map_equal_i64);
}
static void put_int(bhasa_map *map, int64_t key, int64_t number) {
    owned value = {number, "independent owned value"};
    bhasa_rt_map_put(map, &key, &value);
}
static void balanced(void) {
    assert(allocations == 0);
    assert(key_clones == key_drops);
    assert(value_clones == value_drops);
}
static uint64_t collisions(const void *key) { (void)key; return 7; }
static void exhausted(const bhasa_map *map, int64_t *cursor) {
    for (int repeat = 0; repeat < 3; ++repeat) {
        int64_t output = INT64_MIN, before_keys = key_clones, before_values = value_clones;
        assert(bhasa_rt_map_next_key(map, cursor, &output) == 0);
        assert(output == INT64_MIN);
        assert(key_clones == before_keys && value_clones == before_values);
        assert(*cursor == map->capacity);
    }
}
static void empty_checks(void) {
    bhasa_map *map = int_map(collisions);
    int64_t cursor = 0;
    exhausted(map, &cursor);
    put_int(map, 1, 10);
    int64_t key = 1;
    assert(bhasa_rt_map_remove(map, &key) == 1);
    assert(map->capacity != 0 && map->used != 0);
    cursor = 0; exhausted(map, &cursor); /* Only tombstones remain. */
    bhasa_rt_map_clear(map);
    cursor = 0; exhausted(map, &cursor); /* Allocated but empty buckets. */
    bhasa_rt_map_free(map); balanced();
}
static void int_checks(bhasa_key_hash hash) {
    bhasa_map *map = int_map(hash);
    for (int64_t key = 0; key < 257; ++key) put_int(map, key, key);
    for (int64_t key = 1; key < 257; key += 2) assert(bhasa_rt_map_remove(map, &key) == 1);
    put_int(map, 2, 9002); /* Replacement is visited once. */
    put_int(map, INT64_MIN, 42); put_int(map, INT64_MAX, 43);
    bhasa_map *snapshot = bhasa_rt_map_clone(map);
    assert(bhasa_rt_map_count(snapshot) == 131);
    bhasa_rt_map_clear(map); put_int(map, 999, -1); bhasa_rt_map_free(map);
    int seen[257] = {0}, low = 0, high = 0, visits = 0;
    int64_t cursor = 0, key;
    int64_t clones_before = key_clones;
    while (bhasa_rt_map_next_key(snapshot, &cursor, &key)) {
        ++visits;
        if (key == INT64_MIN) ++low;
        else if (key == INT64_MAX) ++high;
        else { assert(key >= 0 && key < 257 && key % 2 == 0); ++seen[key]; }
        owned value; bhasa_rt_map_get(snapshot, &key, &value);
        assert(value.number == (key == INT64_MIN ? 42 : key == INT64_MAX ? 43 : key == 2 ? 9002 : key));
        assert(strcmp(value.text, "independent owned value") == 0);
        owned_drop(&value); key_drop(&key);
        key = -99; /* Mutable output never changes the stored key. */
    }
    assert(visits == 131 && key_clones - clones_before == visits && low == 1 && high == 1);
    for (int key_index = 0; key_index < 257; ++key_index) assert(seen[key_index] == (key_index % 2 == 0));
    exhausted(snapshot, &cursor);
    /* A second cursor is independent; early termination needs no runtime iterator allocation. */
    cursor = 0; assert(bhasa_rt_map_next_key(snapshot, &cursor, &key) == 1); key_drop(&key);
    bhasa_rt_map_free(snapshot); balanced();

    map = int_map(hash);
    for (int64_t index = 0; index < 10; ++index) put_int(map, index, index);
    for (int64_t index = 0; index < 10; ++index) if (index != 4) assert(bhasa_rt_map_remove(map, &index) == 1);
    cursor = 0; assert(bhasa_rt_map_next_key(map, &cursor, &key) == 1); assert(key == 4); key_drop(&key);
    exhausted(map, &cursor); bhasa_rt_map_free(map); balanced();
}
static void string_checks(void) {
    static const char duplicate[] = "alpha";
    const char *keys[] = {"", "alpha", "removed", "भाषा", "🌍"};
    bhasa_map *map = bhasa_rt_map_new((int64_t)sizeof(char *), (int64_t)sizeof(owned),
        str_clone, key_drop, owned_clone, owned_drop, bhasa_rt_map_hash_str, bhasa_rt_map_equal_str);
    for (int64_t index = 0; index < 5; ++index) {
        owned value = {index, "string value"}; bhasa_rt_map_put(map, &keys[index], &value);
    }
    const char *same = duplicate; owned replacement = {10, "replacement"};
    bhasa_rt_map_put(map, &same, &replacement);
    assert(bhasa_rt_map_remove(map, &keys[2]) == 1);
    bhasa_map *snapshot = bhasa_rt_map_clone(map);
    bhasa_rt_map_clear(map); bhasa_rt_map_free(map);
    int seen[5] = {0}; int64_t cursor = 0, visits = 0;
    const char *key;
    while (bhasa_rt_map_next_key(snapshot, &cursor, &key)) {
        int index = 0;
        while (index < 5 && strcmp(key, keys[index]) != 0) ++index;
        assert(index < 5 && index != 2); ++seen[index]; ++visits;
        owned value; bhasa_rt_map_get(snapshot, &key, &value);
        assert(value.number == (index == 1 ? 10 : index));
        assert(strcmp(value.text, index == 1 ? "replacement" : "string value") == 0);
        owned_drop(&value); key_drop(&key); key = NULL;
    }
    assert(visits == 4);
    for (int index = 0; index < 5; ++index) assert(seen[index] == (index != 2));
    key = "unchanged"; const char *sentinel = key;
    assert(bhasa_rt_map_next_key(snapshot, &cursor, &key) == 0 && key == sentinel);
    bhasa_rt_map_free(snapshot); balanced();
}

static bhasa_map *guard_map;
static bhasa_map saved_map;
static int64_t guard_cursor, saved_cursor, guard_output[2] = {123, 456};
static int64_t saved_keys, saved_values;
static _Alignas(8) unsigned char guard_bytes[32];
static struct { int64_t prefix; bhasa_map map; } guard_shadow;
static void guard_cleanup(void) {
    assert(memcmp(guard_map, &saved_map, sizeof(saved_map)) == 0);
    assert(memcmp(&guard_shadow.map, &saved_map, sizeof(saved_map)) == 0);
    assert(guard_shadow.prefix == 77 && guard_cursor == saved_cursor);
    assert(guard_output[0] == 123 && guard_output[1] == 456);
    for (size_t index = 0; index < sizeof(guard_bytes); ++index) assert(guard_bytes[index] == 42);
    assert(key_clones == saved_keys && value_clones == saved_values);
    int64_t key[2] = {1, 99}; owned value;
    bhasa_rt_map_get(guard_map, key, &value);
    assert(value.number == 42 && strcmp(value.text, "unchanged") == 0);
    owned_drop(&value); bhasa_rt_map_free(guard_map); balanced();
}
static int rejected(const char *mode) {
    guard_map = bhasa_rt_map_new(16, (int64_t)sizeof(owned), pair_clone, key_drop,
        owned_clone, owned_drop, collisions, bhasa_rt_map_equal_i64);
    int64_t key[2] = {1, 99}; owned value = {42, "unchanged"};
    bhasa_rt_map_put(guard_map, key, &value);
    saved_map = *guard_map; guard_shadow.map = *guard_map; guard_shadow.prefix = 77;
    memset(guard_bytes, 42, sizeof(guard_bytes));
    int64_t *cursor = &guard_cursor; void *output = guard_output;
    const bhasa_map *map = guard_map;
    if (strcmp(mode, "null-map") == 0) map = NULL;
    else if (strcmp(mode, "null-cursor") == 0) cursor = NULL;
    else if (strcmp(mode, "null-output") == 0) output = NULL;
    else if (strcmp(mode, "negative") == 0) guard_cursor = -1;
    else if (strcmp(mode, "past-end") == 0) guard_cursor = map->capacity + 1;
    else if (strcmp(mode, "maximum") == 0) guard_cursor = INT64_MAX;
    else if (strcmp(mode, "misaligned-cursor") == 0) cursor = (int64_t *)(guard_bytes + 1);
    else if (strcmp(mode, "misaligned-output") == 0) output = guard_bytes + 1;
    else if (strcmp(mode, "wrapped-output") == 0) output = (void *)(UINTPTR_MAX - 7);
    else if (strcmp(mode, "cursor-header") == 0) cursor = (int64_t *)guard_map;
    else if (strcmp(mode, "cursor-buckets") == 0) cursor = (int64_t *)guard_map->buckets;
    else if (strcmp(mode, "cursor-keys") == 0) cursor = (int64_t *)guard_map->keys;
    else if (strcmp(mode, "cursor-values") == 0) cursor = (int64_t *)guard_map->values;
    else if (strcmp(mode, "output-header") == 0) output = guard_map;
    else if (strcmp(mode, "output-buckets") == 0) output = guard_map->buckets;
    else if (strcmp(mode, "output-keys") == 0) output = guard_map->keys;
    else if (strcmp(mode, "output-values") == 0) output = guard_map->values;
    else if (strcmp(mode, "same-storage") == 0) cursor = guard_output;
    else if (strcmp(mode, "partial-cursor") == 0) cursor = guard_output + 1;
    else if (strcmp(mode, "partial-header") == 0) { map = &guard_shadow.map; output = &guard_shadow.prefix; }
    else return 98;
    saved_cursor = guard_cursor; saved_keys = key_clones; saved_values = value_clones;
    assert(atexit(guard_cleanup) == 0);
    (void)bhasa_rt_map_next_key(map, cursor, output);
    return 99;
}
int main(int argc, char **argv) {
    if (argc == 2) return rejected(argv[1]);
    for (int repeat = 0; repeat < 4; ++repeat) {
        empty_checks(); int_checks(collisions); int_checks(bhasa_rt_map_hash_i64); string_checks();
    }
    balanced(); puts("map iteration runtime passed"); return 0;
}
"#;
