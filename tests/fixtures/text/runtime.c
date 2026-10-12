#include "text.h"
#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int live;
void *bhasa_text_test_malloc(size_t size) {
    void *result = malloc(size); if (result != NULL) ++live; return result;
}
void *bhasa_text_test_realloc(void *pointer, size_t size) {
    int empty = pointer == NULL;
    void *result = realloc(pointer, size); if (result != NULL && empty) ++live; return result;
}
void bhasa_text_test_free(void *pointer) { if (pointer != NULL) { assert(live > 0); --live; } free(pointer); }

static bhasa_buffer *bytes(const unsigned char *value, size_t size) {
    bhasa_buffer *result = bhasa_rt_buffer_new();
    for (size_t index = 0; index < size; ++index) bhasa_rt_buffer_push(result, value[index]);
    return result;
}

int main(int argc, char **argv) {
    bhasa_buffer *value = bhasa_rt_buffer_new();
    if (argc > 1) {
        if (strcmp(argv[1], "invalid") == 0) { bhasa_rt_buffer_push(value, 255); (void)bhasa_rt_utf8_slice(value, 0, 0); }
        if (strcmp(argv[1], "range") == 0) (void)bhasa_rt_utf8_slice(value, 1, 0);
        if (strcmp(argv[1], "negative") == 0) (void)bhasa_rt_utf8_slice(value, 0, -1);
        return 99;
    }
    assert(bhasa_rt_utf8_valid(value) == 1 && bhasa_rt_utf8_count(value) == 0);
    assert(bhasa_rt_utf8_get(value, 0) == -1);
    /* Every Unicode scalar round-trips, including NUL and noncharacters. */
    for (int64_t scalar = 0; scalar <= 0x10ffff; ++scalar) {
        value->len = 0;
        int status = (int)bhasa_rt_utf8_append(value, scalar);
        if (scalar >= 0xd800 && scalar <= 0xdfff) { assert(status == EINVAL && value->len == 0); continue; }
        assert(status == 0 && bhasa_rt_utf8_valid(value) == 1);
        assert(bhasa_rt_utf8_count(value) == 1);
        assert(bhasa_rt_utf8_get(value, 0) == scalar);
        assert(bhasa_rt_utf8_get(value, 1) == -1);
    }
    value->len = 0;
    assert(bhasa_rt_utf8_append(value, -1) == EINVAL);
    assert(bhasa_rt_utf8_append(value, 0x110000) == EINVAL && value->len == 0);
    bhasa_rt_buffer_free(value);
    const unsigned char sample[] = {'A', 0xc3, 0xa9, 0xe4, 0xb8, 0xad, 0xf0, 0x9f, 0x99, 0x82, 0, 'Z'};
    value = bytes(sample, sizeof(sample));
    assert(bhasa_rt_utf8_count(value) == 6);
    assert(bhasa_rt_utf8_get(value, 3) == 0x1f642 && bhasa_rt_utf8_get(value, -1) == -1);
    bhasa_buffer *part = bhasa_rt_utf8_slice(value, 1, 3);
    assert(part->len == 9 && memcmp(part->data, sample + 1, 9) == 0);
    bhasa_rt_buffer_set(value, 1, 255);
    assert(bhasa_rt_utf8_valid(part) == 1); /* Independent owned copy. */
    bhasa_rt_buffer_free(part);
    assert(bhasa_rt_utf8_get(value, 0) == -1); /* Bad suffix invalidates whole input. */
    bhasa_rt_buffer_free(value);
    static const unsigned char malformed[][5] = {
        {0x80}, {0xbf}, {0xc0,0x80}, {0xc1,0xbf}, {0xc2}, {0xdf},
        {0xe0,0x80,0x80}, {0xe0,0x9f,0xbf}, {0xe1,0x80}, {0xed,0xa0,0x80},
        {0xed,0xbf,0xbf}, {0xef,0xbf}, {0xf0,0x80,0x80,0x80},
        {0xf0,0x8f,0xbf,0xbf}, {0xf1,0x80,0x80}, {0xf4,0x90,0x80,0x80},
        {0xf5,0x80,0x80,0x80}, {0xff}, {0xc2,'x'}, {0xe1,0x80,'x'},
        {0xf1,0x80,0x80,'x'}
    };
    static const size_t lengths[] = {1,1,2,2,1,1,3,3,2,3,3,2,4,4,3,4,4,1,2,3,4};
    for (size_t index = 0; index < sizeof(lengths)/sizeof(lengths[0]); ++index) {
        value = bytes(malformed[index], lengths[index]);
        assert(bhasa_rt_utf8_valid(value) == 0 && bhasa_rt_utf8_count(value) == -1);
        assert(bhasa_rt_utf8_get(value, 0) == -1);
        bhasa_rt_buffer_free(value);
    }
    bhasa_buffer invalid = {-1,0,NULL};
    assert(bhasa_rt_utf8_count(&invalid) == -1 && bhasa_rt_utf8_valid(NULL) == 0);
    assert(bhasa_rt_utf8_append(&invalid, 65) == EINVAL);
    value = bhasa_rt_buffer_new();
    part = bhasa_rt_utf8_slice(value, 0, 0);
    assert(part->len == 0);
    bhasa_rt_buffer_free(value); bhasa_rt_buffer_free(part);
    assert(live == 0);
    puts("UTF-8 scalar and ownership checks passed");
    return 0;
}
